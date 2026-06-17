//! Resolve a stored credential to its decrypted JSON payload (for node execution).

use serde_json::Value;
use std::time::Duration;
use uuid::Uuid;

use crate::models::credential::Credential;
use crate::nodes::trait_::FieldType;
use crate::nodes::NodeRegistry;
use crate::runtime::core_proxy::CoreProxy;
use crate::services::crypto;

/// Load + decrypt a credential owned by `owner`. Returns the JSON object of
/// field → value, augmented with a `_type` key. Errors if missing/undecryptable.
pub async fn resolve(
    db: &sqlx::PgPool,
    internal_secret: &str,
    owner: Uuid,
    id: Uuid,
) -> Result<Value, String> {
    let row = sqlx::query_as::<_, Credential>(
        "SELECT * FROM flow.credentials WHERE id = $1 AND owner_id = $2",
    )
    .bind(id)
    .bind(owner)
    .fetch_optional(db)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "Credential introuvable".to_string())?;

    let key = crypto::derive_key(internal_secret);
    let plaintext = crypto::decrypt(&key, &row.data, &row.nonce)?;
    let mut data: Value = serde_json::from_slice(&plaintext).map_err(|e| e.to_string())?;
    if let Some(obj) = data.as_object_mut() {
        obj.insert("_type".into(), Value::String(row.type_id.clone()));
    }
    Ok(data)
}

/// Replace every Credential-typed config field (holding a credential id) by its
/// decrypted payload, so nodes receive the secret values directly. Best-effort:
/// an unresolved/invalid id is left as-is (the node will report a clear error).
pub async fn inject_into_config(
    registry: &NodeRegistry,
    db: &sqlx::PgPool,
    internal_secret: &str,
    owner: Uuid,
    node_type: &str,
    config: &mut Value,
) {
    let Some(meta) = registry.meta(node_type) else { return };
    for f in &meta.fields {
        if f.field_type != FieldType::Credential { continue; }
        let Some(id_str) = config.get(&f.name).and_then(|v| v.as_str()).map(str::to_string) else { continue };
        let Ok(id) = Uuid::parse_str(id_str.trim()) else { continue };
        match resolve(db, internal_secret, owner, id).await {
            Ok(data) => { config[&f.name] = data; }
            Err(e) => { tracing::warn!(error = %e, credential = %id, "résolution credential échouée"); }
        }
    }
}

// ── Test d'un credential (connexion / appel authentifié réel) ────────────────────

/// Résultat d'un test : `ok = None` → type non testable.
pub struct TestResult {
    pub ok:      Option<bool>,
    pub message: String,
}

impl TestResult {
    fn ok(msg: &str) -> Self { Self { ok: Some(true), message: msg.into() } }
    fn fail(msg: String) -> Self { Self { ok: Some(false), message: msg } }
    fn na() -> Self { Self { ok: None, message: "Test non disponible pour ce type".into() } }
}

fn s(data: &Value, k: &str) -> String {
    data.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string()
}

/// Best-effort connectivity / auth test for a candidate credential payload.
pub async fn test(proxy: &CoreProxy, type_id: &str, data: &Value) -> TestResult {
    match type_id {
        // PostgreSQL-protocol databases → real connection + SELECT 1.
        "postgres" | "cockroachDb" => test_postgres(data).await,

        // AI providers exposing a models listing endpoint (auth check).
        "anthropicApi" => test_http_auth(proxy, "https://api.anthropic.com/v1/models",
            &[("x-api-key", &s(data, "apiKey")), ("anthropic-version", "2023-06-01")]).await,
        "openAiApi" => test_bearer(proxy, "https://api.openai.com/v1/models", &s(data, "apiKey")).await,
        "mistralApi" => test_bearer(proxy, "https://api.mistral.ai/v1/models", &s(data, "apiKey")).await,
        "groqApi" => test_bearer(proxy, "https://api.groq.com/openai/v1/models", &s(data, "apiKey")).await,
        "googleGeminiApi" => test_http_auth(proxy,
            &format!("https://generativelanguage.googleapis.com/v1beta/models?key={}", s(data, "apiKey")), &[]).await,

        // Host-based services → TCP reachability of host:port.
        "mysql" | "mariaDb" => test_tcp(&s(data, "host"), port(data, 3306)).await,
        "microsoftSql" => test_tcp(&s(data, "host"), port(data, 1433)).await,
        "redis" => test_tcp(&s(data, "host"), port(data, 6379)).await,
        "smtp" => test_tcp(&s(data, "host"), port(data, 587)).await,
        "imap" => test_tcp(&s(data, "host"), port(data, 993)).await,

        _ => TestResult::na(),
    }
}

fn port(data: &Value, default: u16) -> u16 {
    data.get("port").and_then(|v| v.as_i64())
        .or_else(|| data.get("port").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()))
        .map(|p| p as u16)
        .unwrap_or(default)
}

async fn test_postgres(data: &Value) -> TestResult {
    use sqlx::postgres::{PgConnectOptions, PgSslMode};
    use sqlx::{Connection, PgConnection, Row};
    let host = s(data, "host");
    if host.is_empty() { return TestResult::fail("Hôte requis".into()); }
    let ssl = data.get("ssl").and_then(|v| v.as_bool()).unwrap_or(false);
    let mut opts = PgConnectOptions::new()
        .host(&host).port(port(data, 5432)).username(&s(data, "user"))
        .ssl_mode(if ssl { PgSslMode::Require } else { PgSslMode::Prefer });
    let db = s(data, "database"); if !db.is_empty() { opts = opts.database(&db); }
    let pass = s(data, "password"); if !pass.is_empty() { opts = opts.password(&pass); }

    let attempt = async {
        let mut conn = PgConnection::connect_with(&opts).await.map_err(|e| e.to_string())?;
        let row = sqlx::query("SELECT 1 AS ok").fetch_one(&mut conn).await.map_err(|e| e.to_string())?;
        let _: i32 = row.try_get("ok").map_err(|e| e.to_string())?;
        Ok::<(), String>(())
    };
    match tokio::time::timeout(Duration::from_secs(10), attempt).await {
        Ok(Ok(())) => TestResult::ok("Connexion réussie"),
        Ok(Err(e)) => TestResult::fail(format!("Connexion échouée : {e}")),
        Err(_) => TestResult::fail("Connexion : délai dépassé".into()),
    }
}

async fn test_bearer(proxy: &CoreProxy, url: &str, token: &str) -> TestResult {
    test_http_auth(proxy, url, &[("Authorization", &format!("Bearer {token}"))]).await
}

async fn test_http_auth(proxy: &CoreProxy, url: &str, headers: &[(&str, &str)]) -> TestResult {
    let map: std::collections::HashMap<String, String> =
        headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    match proxy.call_external(url, reqwest::Method::GET, map, None, 15, Uuid::nil()).await {
        Ok(_) => TestResult::ok("Authentification réussie"),
        Err(e) => {
            let msg = e.to_string();
            let low = msg.to_lowercase();
            if msg.contains("401") || msg.contains("403") || low.contains("unauthorized") || low.contains("autoris") || low.contains("forbidden") {
                TestResult::fail("Clé/identifiants refusés".into())
            } else {
                TestResult::fail(format!("Échec : {msg}"))
            }
        }
    }
}

async fn test_tcp(host: &str, port: u16) -> TestResult {
    if host.is_empty() { return TestResult::fail("Hôte requis".into()); }
    let addr = format!("{host}:{port}");
    match tokio::time::timeout(Duration::from_secs(8), tokio::net::TcpStream::connect(&addr)).await {
        Ok(Ok(_)) => TestResult::ok(&format!("{addr} joignable")),
        Ok(Err(e)) => TestResult::fail(format!("Inaccessible : {e}")),
        Err(_) => TestResult::fail(format!("{addr} : délai dépassé")),
    }
}
