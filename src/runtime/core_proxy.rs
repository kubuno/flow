//! CoreProxy — canal de communication sortante de Flow.
//!
//! Adaptation pragmatique : le core n'expose pas (encore) `/internal/proxy`.
//! Les appels vers les modules Kubuno passent donc par le proxy existant du core
//! (`{core}/api/v1/{module}{path}`) en s'authentifiant avec le secret interne et
//! l'identité de l'utilisateur (`X-Kubuno-User-Id`). Les appels externes (internet)
//! sont effectués directement ici — aucun nœud ne crée son propre client HTTP.

use reqwest::Method;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone)]
pub struct CoreProxy {
    client:          reqwest::Client,
    core_url:        String,
    internal_secret: String,
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
}

impl ProxyError {
    pub fn is_retryable(&self) -> bool {
        matches!(self, ProxyError::Http(_) | ProxyError::RateLimited)
    }
}

impl CoreProxy {
    pub fn new(core_url: String, internal_secret: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client, core_url, internal_secret }
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
