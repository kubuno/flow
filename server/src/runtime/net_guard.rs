//! Host-level anti-SSRF guard — the single definition of "an address a workflow
//! must never reach".
//!
//! Flow dials the outside world in two different shapes:
//!   * an HTTP request built from a URL (HTTP/AI/integration nodes, SSE trigger);
//!   * a RAW SOCKET opened from a host and a port (database nodes, credential
//!     connectivity test, IMAP/POP3 email trigger).
//!
//! The URL shape used to be the only guarded one, which left every raw socket
//! free to reach the instance's own database or any service bound on the host.
//! Both shapes are now judged here, by the same rules, so "internal address"
//! has exactly one definition in the module.
//!
//! `guard_host` returns the addresses it validated. A call site able to dial an
//! IP directly SHOULD use them instead of the hostname: resolving the name a
//! second time just before connecting reopens a DNS-rebinding window, where the
//! second lookup answers with an internal address the guard never saw.

use std::net::IpAddr;

use sqlx::mysql::{MySqlConnectOptions, MySqlSslMode};
use sqlx::postgres::{PgConnectOptions, PgSslMode};

use crate::runtime::core_proxy::{CoreProxy, ProxyError};

/// Splits an outbound URL into the `(host, port)` pair the guard works on,
/// after refusing every scheme that is not plain http/https.
pub fn host_port_from_url(url_str: &str) -> Result<(String, u16), ProxyError> {
    let parsed = reqwest::Url::parse(url_str)
        .map_err(|_| ProxyError::Blocked(format!("URL invalide : {url_str}")))?;

    match parsed.scheme() {
        "http" | "https" => {}
        other => return Err(ProxyError::Blocked(format!("schéma non autorisé : {other}"))),
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| ProxyError::Blocked("hôte manquant dans l'URL".into()))?;
    let port = parsed.port_or_known_default().unwrap_or(80);
    Ok((host.to_string(), port))
}

/// Strips the brackets of an IPv6 literal (`[::1]` → `::1`). `Url::host_str`
/// keeps them, raw configuration fields usually don't.
fn unbracket(host: &str) -> &str {
    host.strip_prefix('[')
        .and_then(|r| r.strip_suffix(']'))
        .unwrap_or(host)
}

/// Unconditional address check for one `(host, port)`. Returns every address
/// the host resolves to — all of them already validated — so the caller can
/// connect to a validated IP rather than to the name.
///
/// This is the whole SSRF policy: no scheme, no URL, nothing HTTP-specific, so
/// a database driver or a mail client is held to the same rules as an HTTP node.
pub async fn guard_host(host: &str, port: u16) -> Result<Vec<IpAddr>, ProxyError> {
    let host = unbracket(host.trim());
    if host.is_empty() {
        return Err(ProxyError::Blocked("hôte manquant".into()));
    }
    // A filesystem path, not a host: a unix socket (`/var/run/postgresql`) never
    // touches the network stack and would walk straight past the address policy.
    if host.starts_with('/') || host.contains('/') || host.starts_with('.') {
        return Err(ProxyError::Blocked(format!(
            "socket local interdit : {host}"
        )));
    }

    // Raw IP literal: judged directly, no DNS involved.
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_blocked_ip(ip) {
            return Err(ProxyError::Blocked(format!(
                "adresse interne interdite : {ip}"
            )));
        }
        return Ok(vec![ip]);
    }

    // Hostname: resolve and refuse if ANY resolved address is internal — a name
    // that answers with one public and one private address is a classic bypass.
    let addrs = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| ProxyError::Blocked(format!("résolution DNS impossible : {host}")))?;

    let mut ips = Vec::new();
    for addr in addrs {
        if is_blocked_ip(addr.ip()) {
            return Err(ProxyError::Blocked(format!(
                "l'hôte {host} pointe vers une adresse interne"
            )));
        }
        ips.push(addr.ip());
    }
    if ips.is_empty() {
        return Err(ProxyError::Blocked(format!(
            "aucune adresse résolue pour {host}"
        )));
    }
    Ok(ips)
}

/// True for addresses a workflow must never reach: loopback, unspecified,
/// multicast, RFC1918 private, CGNAT, link-local (incl. the 169.254.169.254
/// cloud metadata endpoint), broadcast/documentation, IPv6 ULA and IPv4-mapped
/// equivalents.
pub fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_multicast()
                || o[0] == 0
                // CGNAT shared address space 100.64.0.0/10
                || (o[0] == 100 && (o[1] & 0xC0) == 0x40)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_blocked_ip(IpAddr::V4(v4));
            }
            let seg = v6.segments();
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                // Unique local addresses fc00::/7
                || (seg[0] & 0xfe00) == 0xfc00
                // Link-local unicast fe80::/10
                || (seg[0] & 0xffc0) == 0xfe80
        }
    }
}

// ── SQL drivers ──────────────────────────────────────────────────────────────────
//
// sqlx takes a host and dials it itself, so the guard is applied to the parsed
// connect options and, when the TLS mode allows it, the host is REPLACED by the
// validated address. Both engines are handled the same way; only the name of
// the hostname-verifying TLS mode differs.

/// Refuses a DSN that smuggles a local unix socket past the host check. sqlx
/// leaves its `host` field untouched when a `?host=/path` (PostgreSQL) or
/// `?socket=/path` (MySQL) parameter is present, so the guard would happily
/// judge a public hostname while the driver dials the filesystem instead.
fn refuse_socket_dsn(dsn: &str) -> Result<(), ProxyError> {
    fn refused() -> Result<(), ProxyError> {
        Err(ProxyError::Blocked(
            "socket local interdit dans la chaîne de connexion".into(),
        ))
    }
    // An unparseable DSN is left to the driver, which reports it precisely.
    let Ok(url) = reqwest::Url::parse(dsn) else { return Ok(()) };
    // A path in the host position (`postgres://%2Fvar%2Frun/db`) is a socket too,
    // and sqlx keeps its own `host` field — whatever the environment left there —
    // for the guard to judge.
    if let Some(h) = url.host_str() {
        if h.starts_with('/') || h.to_ascii_lowercase().starts_with("%2f") {
            return refused();
        }
    }
    for (key, value) in url.query_pairs() {
        let key = key.to_ascii_lowercase();
        if matches!(key.as_str(), "host" | "socket" | "unix_socket") && value.starts_with('/') {
            return refused();
        }
    }
    Ok(())
}

/// Parses a PostgreSQL DSN into guarded connect options.
pub async fn guarded_pg_dsn(
    proxy: &CoreProxy,
    dsn: &str,
) -> Result<PgConnectOptions, ProxyError> {
    refuse_socket_dsn(dsn)?;
    let opts: PgConnectOptions = dsn
        .parse()
        .map_err(|_| ProxyError::Blocked("chaîne de connexion PostgreSQL invalide".into()))?;
    guard_pg_options(proxy, opts).await
}

/// Full egress verdict on a PostgreSQL connect-options set, then a switch to the
/// validated address so the driver does not resolve the name a second time.
/// `verify-full` is the one mode that checks the certificate against the host
/// name, so the name is kept there — at the cost of that second lookup.
pub async fn guard_pg_options(
    proxy: &CoreProxy,
    opts: PgConnectOptions,
) -> Result<PgConnectOptions, ProxyError> {
    let host = opts.get_host().to_string();
    let ips = proxy.check_egress_host(&host, opts.get_port()).await?;
    if !matches!(opts.get_ssl_mode(), PgSslMode::VerifyFull) {
        if let Some(ip) = ips.first() {
            return Ok(opts.host(&ip.to_string()));
        }
    }
    Ok(opts)
}

/// Parses a MySQL/MariaDB DSN into guarded connect options.
pub async fn guarded_mysql_dsn(
    proxy: &CoreProxy,
    dsn: &str,
) -> Result<MySqlConnectOptions, ProxyError> {
    refuse_socket_dsn(dsn)?;
    let opts: MySqlConnectOptions = dsn
        .parse()
        .map_err(|_| ProxyError::Blocked("chaîne de connexion MySQL invalide".into()))?;
    guard_mysql_options(proxy, opts).await
}

/// MySQL/MariaDB counterpart of `guard_pg_options`. `verify-identity` is the
/// mode that checks the host name against the certificate.
pub async fn guard_mysql_options(
    proxy: &CoreProxy,
    opts: MySqlConnectOptions,
) -> Result<MySqlConnectOptions, ProxyError> {
    if opts.get_socket().is_some() {
        return Err(ProxyError::Blocked("socket local interdit".into()));
    }
    let host = opts.get_host().to_string();
    let ips = proxy.check_egress_host(&host, opts.get_port()).await?;
    if !matches!(opts.get_ssl_mode(), MySqlSslMode::VerifyIdentity) {
        if let Some(ip) = ips.first() {
            return Ok(opts.host(&ip.to_string()));
        }
    }
    Ok(opts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::egress::EgressPolicy;
    use std::sync::{Arc, RwLock};

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    /// A proxy with no administrator restriction: only the address guard speaks.
    fn open_proxy() -> CoreProxy {
        CoreProxy::new(
            "http://core.invalid".into(),
            "secret".into(),
            Arc::new(RwLock::new(EgressPolicy::default())),
        )
    }

    fn proxy_with(settings: serde_json::Value) -> CoreProxy {
        CoreProxy::new(
            "http://core.invalid".into(),
            "secret".into(),
            Arc::new(RwLock::new(EgressPolicy::from_settings(&settings))),
        )
    }

    #[test]
    fn internal_addresses_are_blocked() {
        for s in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.5.4",
            "192.168.1.1",
            "169.254.169.254", // cloud metadata
            "100.64.0.1",      // CGNAT
            "0.0.0.0",
            "::1",
            "fe80::1",
            "fc00::1",
            "::ffff:127.0.0.1", // IPv4-mapped loopback
        ] {
            assert!(is_blocked_ip(ip(s)), "{s} should be blocked");
        }
    }

    #[test]
    fn public_addresses_are_allowed() {
        for s in ["8.8.8.8", "1.1.1.1", "93.184.216.34", "2606:2800:220:1::"] {
            assert!(!is_blocked_ip(ip(s)), "{s} should be allowed");
        }
    }

    #[test]
    fn only_http_urls_yield_a_host_and_port() {
        assert_eq!(
            host_port_from_url("https://example.com/x").ok(),
            Some(("example.com".into(), 443))
        );
        assert_eq!(
            host_port_from_url("http://example.com:8080/x").ok(),
            Some(("example.com".into(), 8080))
        );
        // Schemes that are not plain HTTP, and non-URLs.
        assert!(host_port_from_url("ftp://example.com/x").is_err());
        assert!(host_port_from_url("file:///etc/passwd").is_err());
        assert!(host_port_from_url("postgres://user@host/db").is_err());
        assert!(host_port_from_url("definitely not a url").is_err());
    }

    #[tokio::test]
    async fn ip_literals_are_judged_without_dns() {
        // Every flavour of internal address, given as a literal.
        for h in ["127.0.0.1", "169.254.169.254", "10.1.2.3", "::1", "[::1]", "fc00::1"] {
            assert!(guard_host(h, 5432).await.is_err(), "{h} should be blocked");
        }
        // A public literal is accepted and handed back for a direct dial.
        assert_eq!(
            guard_host("93.184.216.34", 993).await.ok(),
            Some(vec![ip("93.184.216.34")])
        );
    }

    #[tokio::test]
    async fn local_sockets_and_empty_hosts_are_refused() {
        assert!(guard_host("/var/run/postgresql", 5432).await.is_err());
        assert!(guard_host("./relative", 5432).await.is_err());
        assert!(guard_host("", 5432).await.is_err());
        assert!(guard_host("   ", 5432).await.is_err());
    }

    #[tokio::test]
    async fn a_name_resolving_to_loopback_is_refused() {
        // `localhost` needs no network to resolve, so this stays deterministic.
        assert!(guard_host("localhost", 6379).await.is_err());
    }

    #[tokio::test]
    async fn a_database_on_an_internal_host_is_refused() {
        let proxy = open_proxy();
        // The instance's own database, reached through a workflow node.
        let opts = PgConnectOptions::new().host("127.0.0.1").port(5432);
        assert!(guard_pg_options(&proxy, opts).await.is_err());
        assert!(guarded_pg_dsn(&proxy, "postgres://u:p@localhost:5432/kubuno").await.is_err());
        assert!(guarded_pg_dsn(&proxy, "postgres://u:p@10.0.0.7:5432/db").await.is_err());
        assert!(guarded_mysql_dsn(&proxy, "mysql://u:p@192.168.1.9:3306/db").await.is_err());
    }

    /// A public endpoint stays reachable and its address/port survive the guard.
    /// The test uses IP literals so it needs no DNS; the name → validated-address
    /// substitution itself is covered by `guard_host`, which hands the addresses
    /// back.
    #[tokio::test]
    async fn a_public_database_host_survives_the_guard() {
        let proxy = open_proxy();
        let opts = guarded_pg_dsn(&proxy, "postgres://u:p@93.184.216.34:6543/db")
            .await
            .expect("public host should be reachable");
        assert_eq!(opts.get_host(), "93.184.216.34");
        assert_eq!(opts.get_port(), 6543);

        let opts = guarded_mysql_dsn(&proxy, "mysql://u:p@93.184.216.34:3307/db")
            .await
            .expect("public host should be reachable");
        assert_eq!(opts.get_host(), "93.184.216.34");
        assert_eq!(opts.get_port(), 3307);
    }

    #[tokio::test]
    async fn a_dsn_cannot_smuggle_a_local_socket() {
        let proxy = open_proxy();
        // Public host name in the URL, unix socket in the parameters: sqlx would
        // dial the socket and ignore the host the guard just validated.
        assert!(
            guarded_pg_dsn(&proxy, "postgres://u:p@93.184.216.34/db?host=/var/run/postgresql")
                .await
                .is_err()
        );
        assert!(
            guarded_mysql_dsn(&proxy, "mysql://u:p@93.184.216.34/db?socket=/var/run/mysqld.sock")
                .await
                .is_err()
        );
        // Socket in the host position, percent-encoded.
        assert!(guarded_pg_dsn(&proxy, "postgres://%2Fvar%2Frun%2Fpostgresql/db").await.is_err());
    }

    #[tokio::test]
    async fn the_admin_lists_apply_to_databases_too() {
        // Denied: a host the address guard alone would have accepted.
        let proxy = proxy_with(serde_json::json!({ "egress_denied_hosts": "93.184.216.34" }));
        assert!(guarded_pg_dsn(&proxy, "postgres://u:p@93.184.216.34/db").await.is_err());

        // Allowlist: everything outside it is refused, including for raw sockets.
        let proxy = proxy_with(serde_json::json!({ "egress_allowed_hosts": "93.184.216.34" }));
        assert!(guarded_pg_dsn(&proxy, "postgres://u:p@93.184.216.34/db").await.is_ok());
        assert!(guarded_pg_dsn(&proxy, "postgres://u:p@8.8.8.8/db").await.is_err());
    }

    #[tokio::test]
    async fn a_refusal_is_never_retryable() {
        let proxy = open_proxy();
        let e = guarded_pg_dsn(&proxy, "postgres://u:p@127.0.0.1/db")
            .await
            .expect_err("loopback must be refused");
        assert!(!e.is_retryable());
        assert!(matches!(e, ProxyError::Blocked(_)));
    }
}
