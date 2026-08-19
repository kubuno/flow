//! CoreProxy — canal de communication sortante de Flow.
//!
//! Adaptation pragmatique : le core n'expose pas (encore) `/internal/proxy`.
//! Les appels vers les modules Kubuno passent donc par le proxy existant du core
//! (`{core}/api/v1/{module}{path}`) en s'authentifiant avec le secret interne et
//! l'identité de l'utilisateur (`X-Kubuno-User-Id`). Les appels externes (internet)
//! sont effectués directement ici — aucun nœud ne crée son propre client HTTP.

use reqwest::Method;
use std::collections::HashMap;
use std::net::IpAddr;
use uuid::Uuid;

use crate::runtime::egress::{EgressPolicy, SharedEgressPolicy};
use crate::runtime::net_guard::{guard_host, host_port_from_url};

#[derive(Clone)]
pub struct CoreProxy {
    client:          reqwest::Client,
    core_url:        String,
    internal_secret: String,
    /// Administrator allow/deny host lists, refreshed live. They can only
    /// NARROW what `net_guard::guard_host` already allows — see `runtime::egress`.
    egress_policy:   SharedEgressPolicy,
}

#[derive(Debug, Clone)]
pub struct ProxyResponse {
    pub status:      u16,
    pub headers:     HashMap<String, String>,
    pub body:        serde_json::Value,
    pub duration_ms: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    #[error("Erreur HTTP : {0}")]
    Http(#[from] reqwest::Error),
    #[error("Non autorisé")]
    Unauthorized,
    #[error("Rate limit dépassé")]
    RateLimited,
    #[error("Erreur core {status} : {body}")]
    CoreError { status: u16, body: String },
    /// Outbound request or socket refused by the egress guard (SSRF protection).
    #[error("Connexion sortante bloquée : {0}")]
    Blocked(String),
}

impl ProxyError {
    pub fn is_retryable(&self) -> bool {
        // A blocked request is a policy decision — never retry it.
        matches!(self, ProxyError::Http(_) | ProxyError::RateLimited)
    }
}

impl CoreProxy {
    pub fn new(
        core_url:        String,
        internal_secret: String,
        egress_policy:   SharedEgressPolicy,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client, core_url, internal_secret, egress_policy }
    }

    /// Full verdict on an outbound URL: the unconditional SSRF guard first, then
    /// the administrator's host lists. Exposed so the few call sites that do NOT
    /// go through `call_external` — the SSE trigger keeps a long-lived streaming
    /// connection of its own — can be held to the same rules.
    pub async fn check_egress(&self, url: &str) -> Result<(), ProxyError> {
        let (host, port) = host_port_from_url(url)?;
        self.check_egress_host(&host, port).await.map(|_| ())
    }

    /// Same verdict for a call site that opens a RAW SOCKET and therefore has no
    /// URL to offer: database nodes, credential connectivity test, IMAP/POP3
    /// email trigger. Returns the validated addresses so the caller can dial one
    /// of them directly instead of resolving the name again — that second lookup
    /// is the DNS-rebinding window.
    pub async fn check_egress_host(
        &self,
        host: &str,
        port: u16,
    ) -> Result<Vec<IpAddr>, ProxyError> {
        let ips = guard_host(host, port).await?;
        self.apply_host_policy(host)?;
        Ok(ips)
    }

    /// The administrator layer alone. Never called without the address guard.
    fn apply_host_policy(&self, host: &str) -> Result<(), ProxyError> {
        let policy = match self.egress_policy.read() {
            Ok(p) => p.clone(),
            // A poisoned lock must not silently drop a restriction: refuse.
            Err(e) => {
                tracing::error!(error = %e, "Politique d'hôtes sortants illisible");
                return Err(ProxyError::Blocked(
                    "politique d'hôtes sortants indisponible".into(),
                ));
            }
        };
        if policy.is_empty() {
            return Ok(());
        }
        policy.check(host).map_err(ProxyError::Blocked)
    }

    /// Replaces the live policy. Called by the settings refresher.
    pub fn set_egress_policy(&self, fresh: EgressPolicy) {
        match self.egress_policy.write() {
            Ok(mut guard) => *guard = fresh,
            Err(e) => tracing::error!(error = %e, "Politique d'hôtes sortants non mise à jour"),
        }
    }

    /// Appel vers un module Kubuno interne via le proxy du core.
    /// `module` = "mail" | "chat" | …  `path` = "/send" (sans /api/v1/{module}).
    pub async fn call_module(
        &self,
        module:          &str,
        path:            &str,
        method:          Method,
        body:            Option<serde_json::Value>,
        user_id:         Uuid,
        idempotency_key: Option<&str>,
    ) -> Result<ProxyResponse, ProxyError> {
        let path = if path.starts_with('/') { path.to_string() } else { format!("/{path}") };
        let url = format!(
            "{}/api/v1/{}{}",
            self.core_url.trim_end_matches('/'),
            module,
            path
        );

        let mut req = self
            .client
            .request(method, &url)
            .header("X-Internal-Secret", &self.internal_secret)
            .header("X-Kubuno-User-Id", user_id.to_string());

        if let Some(key) = idempotency_key {
            req = req.header("X-Idempotency-Key", key);
        }
        if let Some(b) = body {
            req = req.json(&b);
        }

        self.send(req).await
    }

    /// Appel vers une URL externe (internet). Effectué directement par le proxy Flow.
    #[allow(clippy::too_many_arguments)]
    pub async fn call_external(
        &self,
        url:          &str,
        method:       Method,
        headers:      HashMap<String, String>,
        body:         Option<serde_json::Value>,
        timeout_secs: u32,
        _user_id:     Uuid,
    ) -> Result<ProxyResponse, ProxyError> {
        // SSRF guard: a workflow node can put an ARBITRARY user-supplied URL here.
        // Refuse non-http(s) schemes and any host that resolves to an internal
        // address (loopback, private, link-local incl. the 169.254.169.254 cloud
        // metadata endpoint, ULA…) so a flow can't reach the core's /internal/*
        // routes or other services on the host. Applied unconditionally, then
        // narrowed further by the administrator's host lists.
        self.check_egress(url).await?;

        let mut req = self
            .client
            .request(method, url)
            .timeout(std::time::Duration::from_secs(timeout_secs.max(1) as u64))
            .header("User-Agent", "Kubuno-Flow/0.1");

        for (k, v) in headers {
            req = req.header(k, v);
        }
        if let Some(b) = body {
            req = req.json(&b);
        }

        self.send(req).await
    }

    /// Publie un événement sur le bus Kubuno via le core (`/internal/events/publish`).
    pub async fn publish_event(&self, event: &serde_json::Value) -> Result<(), ProxyError> {
        let url = format!("{}/internal/events/publish", self.core_url.trim_end_matches('/'));
        let req = self
            .client
            .post(&url)
            .header("X-Internal-Secret", &self.internal_secret)
            .json(event);
        self.send(req).await.map(|_| ())
    }

    async fn send(&self, req: reqwest::RequestBuilder) -> Result<ProxyResponse, ProxyError> {
        let start = std::time::Instant::now();
        let resp = req.send().await.map_err(ProxyError::Http)?;
        let duration_ms = start.elapsed().as_millis() as u64;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ProxyError::Unauthorized);
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ProxyError::RateLimited);
        }

        let headers: HashMap<String, String> = resp
            .headers()
            .iter()
            .filter_map(|(k, v)| v.to_str().ok().map(|s| (k.to_string(), s.to_string())))
            .collect();

        let text = resp.text().await.unwrap_or_default();
        // Le corps peut ne pas être du JSON (texte, HTML…) — on l'enveloppe alors.
        let body: serde_json::Value =
            serde_json::from_str(&text).unwrap_or_else(|_| serde_json::Value::String(text.clone()));

        let code = status.as_u16();
        if !status.is_success() {
            return Err(ProxyError::CoreError { status: code, body: text });
        }

        Ok(ProxyResponse { status: code, headers, body, duration_ms })
    }
}

#[cfg(test)]
mod egress_tests {
    use super::*;
    use crate::runtime::egress::EgressPolicy;
    use std::sync::{Arc, RwLock};

    fn proxy_with(settings: serde_json::Value) -> CoreProxy {
        CoreProxy::new(
            "http://core.invalid".into(),
            "secret".into(),
            Arc::new(RwLock::new(EgressPolicy::from_settings(&settings))),
        )
    }

    #[tokio::test]
    async fn guard_rejects_bad_scheme_and_internal_ip_literals() {
        let proxy = proxy_with(serde_json::json!({}));
        // Non-http(s) scheme.
        assert!(proxy.check_egress("ftp://example.com/x").await.is_err());
        assert!(proxy.check_egress("file:///etc/passwd").await.is_err());
        // Not a URL at all.
        assert!(proxy.check_egress("definitely not a url").await.is_err());
        // Internal IP literals (no DNS involved).
        assert!(proxy.check_egress("http://127.0.0.1:8080/internal/x").await.is_err());
        assert!(proxy.check_egress("http://169.254.169.254/latest/meta-data").await.is_err());
        assert!(proxy.check_egress("http://[::1]/").await.is_err());
        // Public IP literal is allowed (no DNS involved).
        assert!(proxy.check_egress("https://93.184.216.34/").await.is_ok());
    }

    /// The administrator layer can only NARROW: a host the address guard already
    /// refused stays refused even when it is on the allowlist.
    #[tokio::test]
    async fn the_admin_policy_narrows_but_never_widens() {
        let proxy = proxy_with(serde_json::json!({
            "egress_allowed_hosts": "93.184.216.34\nlocalhost\n127.0.0.1",
        }));

        // On the allowlist AND public → reachable.
        assert!(proxy.check_egress_host("93.184.216.34", 5432).await.is_ok());
        // On the allowlist but internal → still refused by the address guard.
        assert!(proxy.check_egress_host("127.0.0.1", 5432).await.is_err());
        assert!(proxy.check_egress_host("localhost", 5432).await.is_err());
        // Public but absent from the allowlist → refused by the administrator.
        assert!(proxy.check_egress_host("8.8.8.8", 5432).await.is_err());
    }

    /// A denylist entry closes a host the address guard would have accepted.
    #[tokio::test]
    async fn a_denied_host_is_refused_on_raw_sockets_too() {
        let proxy = proxy_with(serde_json::json!({ "egress_denied_hosts": "8.8.8.8" }));
        assert!(proxy.check_egress_host("8.8.8.8", 6379).await.is_err());
        assert!(proxy.check_egress_host("1.1.1.1", 6379).await.is_ok());
    }

    /// A blocked request is a verdict, not a hiccup: retrying it would only
    /// replay the same refusal.
    #[test]
    fn a_blocked_error_is_never_retryable() {
        assert!(!ProxyError::Blocked("x".into()).is_retryable());
    }
}
