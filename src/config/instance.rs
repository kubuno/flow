//! Instance-wide settings of the flow module, as the administrator left them in
//! the console.
//!
//! Declared by `module.toml`'s `[[settings]]`, stored in `core.settings`, and read
//! back here through `/internal/modules/flow/settings`. Until this existed the
//! runtime only ever read its STATIC deploy config (`state.settings.runtime.*` /
//! `state.settings.code_node.*`), so admin edits to the execution limits were
//! impossible; every value an administrator can now change flows through here.
//!
//! Only knobs that are ACTUALLY applied at runtime are exposed: a setting that
//! changes nothing is worse than an absent one. Boot-only values (`worker_count`,
//! queue polling/batch) and the stale-job reclaim window (`execution_timeout_secs`)
//! are deliberately left out.
//!
//! The two host lists of the outbound-network policy are NOT here: they are
//! strings, this struct is `Copy`, and their consumer is the proxy rather than a
//! node. They live in `runtime::egress::EgressPolicy`, fed by the same refresher.

use serde_json::Value;

#[derive(Debug, Clone, Copy)]
pub struct InstanceConfig {
    /// Per-node execution timeout, in seconds. A node running longer is aborted.
    pub node_timeout_secs: u64,
    /// How many execution rows are kept per workflow before the pruner deletes
    /// the oldest. Must stay at least 1.
    pub max_execution_history: i64,
    /// Timeout, in seconds, of the Code (QuickJS) node's inline JavaScript.
    pub code_node_timeout_secs: u64,
    /// Memory ceiling, in MiB, of the Code node's QuickJS runtime. Handed to the
    /// engine allocator, so an allocation past it fails inside the script.
    pub code_node_memory_limit_mb: u32,
    /// Maximum number of job retries after a retryable failure. `0` = no retry.
    pub max_retries: i32,
    /// Base delay, in milliseconds, of the exponential retry backoff.
    pub retry_backoff_ms: u64,
    /// Ceiling on how many (non-trashed) workflows a single user may own.
    /// `0` = unlimited. Enforced at creation, duplication and import.
    pub max_workflows_per_user: i32,
    /// Floor, in seconds, between two automatic runs of the SAME workflow.
    /// `0` = no floor. Applied to the scheduled (cron) trigger.
    pub min_schedule_interval_secs: u64,
    /// Whether the unauthenticated `/webhook/:token` endpoint accepts calls.
    /// `false` keeps the tokens minted but refuses every inbound hit.
    pub allow_public_webhooks: bool,
    /// Maximum accepted body size, in kibibytes, of an inbound webhook call.
    pub webhook_max_body_kb: i64,
}

impl Default for InstanceConfig {
    fn default() -> Self {
        Self {
            node_timeout_secs:         60,
            max_execution_history:     500,
            code_node_timeout_secs:    30,
            code_node_memory_limit_mb: 64,
            max_retries:               3,
            retry_backoff_ms:          1000,
            max_workflows_per_user:    0,
            min_schedule_interval_secs: 0,
            allow_public_webhooks:     true,
            webhook_max_body_kb:       1024,
        }
    }
}

impl InstanceConfig {
    /// Maps the core's `{key: value}` object onto the struct. Every read falls
    /// back to the compiled default rather than to a permissive value; an
    /// out-of-range number is treated as a mistake and ignored the same way.
    /// `0` is MEANINGFUL for `max_retries` (never retry) and `retry_backoff_ms`
    /// (retry immediately), so it is accepted there rather than floored away.
    pub fn from_settings(settings: &Value) -> Self {
        let d = Self::default();
        let int_in = |key: &str, min: i64, max: i64, fallback: i64| -> i64 {
            settings
                .get(key)
                .and_then(Value::as_i64)
                .filter(|n| (min..=max).contains(n))
                .unwrap_or(fallback)
        };
        let bool_of = |key: &str, fallback: bool| {
            settings.get(key).and_then(Value::as_bool).unwrap_or(fallback)
        };
        Self {
            node_timeout_secs:      int_in("node_timeout_secs", 1, 3600, d.node_timeout_secs as i64) as u64,
            max_execution_history:  int_in("max_execution_history", 1, 1_000_000, d.max_execution_history),
            code_node_timeout_secs: int_in("code_node_timeout_secs", 1, 300, d.code_node_timeout_secs as i64) as u64,
            code_node_memory_limit_mb: int_in(
                "code_node_memory_limit_mb", 8, 4096, d.code_node_memory_limit_mb as i64,
            ) as u32,
            max_retries:            int_in("max_retries", 0, 10, d.max_retries as i64) as i32,
            retry_backoff_ms:       int_in("retry_backoff_ms", 0, 600_000, d.retry_backoff_ms as i64) as u64,
            max_workflows_per_user: int_in("max_workflows_per_user", 0, 100_000, d.max_workflows_per_user as i64) as i32,
            min_schedule_interval_secs: int_in(
                "min_schedule_interval_secs", 0, 86_400, d.min_schedule_interval_secs as i64,
            ) as u64,
            allow_public_webhooks:  bool_of("allow_public_webhooks", d.allow_public_webhooks),
            webhook_max_body_kb:    int_in("webhook_max_body_kb", 1, 65_536, d.webhook_max_body_kb),
        }
    }
}

/// Reads the instance settings from the core and returns the raw `{key: value}`
/// object. Any failure yields `None`, so the caller keeps the values it already
/// had rather than reverting to defaults because the core was briefly
/// unreachable.
///
/// Raw rather than typed because two consumers read the same payload: this
/// module's `Copy` struct, and the outbound host policy, whose two values are
/// multi-line strings that have no place in a `Copy` snapshot.
pub async fn fetch_raw(core_url: &str, secret: &str, http: &reqwest::Client) -> Option<Value> {
    let url = format!("{core_url}/internal/modules/flow/settings");
    let resp = http
        .get(&url)
        .header("X-Internal-Secret", secret)
        .send()
        .await
        .map_err(|e| tracing::warn!(error = %e, "Lecture des réglages d'instance flow"))
        .ok()?;

    if !resp.status().is_success() {
        tracing::warn!(status = %resp.status(), "Réglages d'instance flow refusés par le core");
        return None;
    }

    let body: Value = resp
        .json()
        .await
        .map_err(|e| tracing::warn!(error = %e, "Réglages d'instance flow : réponse illisible"))
        .ok()?;

    body.get("settings").cloned()
}

/// Typed view of the settings, for callers that only need the scalars.
pub async fn fetch(core_url: &str, secret: &str, http: &reqwest::Client) -> Option<InstanceConfig> {
    fetch_raw(core_url, secret, http)
        .await
        .map(|s| InstanceConfig::from_settings(&s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_keys_keep_the_compiled_defaults() {
        let c = InstanceConfig::from_settings(&json!({}));
        assert_eq!(c.node_timeout_secs, 60);
        assert_eq!(c.max_execution_history, 500);
        assert_eq!(c.code_node_timeout_secs, 30);
        assert_eq!(c.max_retries, 3);
        assert_eq!(c.retry_backoff_ms, 1000);
        assert_eq!(c.code_node_memory_limit_mb, 64);
        assert_eq!(c.max_workflows_per_user, 0);
        assert_eq!(c.min_schedule_interval_secs, 0);
        assert!(c.allow_public_webhooks);
        assert_eq!(c.webhook_max_body_kb, 1024);
    }

    #[test]
    fn the_new_guards_are_read_and_clamped() {
        let c = InstanceConfig::from_settings(&json!({
            "code_node_memory_limit_mb": 256,
            "max_workflows_per_user":    25,
            "min_schedule_interval_secs": 300,
            "allow_public_webhooks":     false,
            "webhook_max_body_kb":       64,
        }));
        assert_eq!(c.code_node_memory_limit_mb, 256);
        assert_eq!(c.max_workflows_per_user, 25);
        assert_eq!(c.min_schedule_interval_secs, 300);
        assert!(!c.allow_public_webhooks);
        assert_eq!(c.webhook_max_body_kb, 64);

        // A memory ceiling below the floor is a mistake, not a request to run
        // the engine out of memory: the compiled default is kept.
        let c = InstanceConfig::from_settings(&json!({ "code_node_memory_limit_mb": 1 }));
        assert_eq!(c.code_node_memory_limit_mb, 64);
    }

    #[test]
    fn zero_is_meaningful_for_retry_settings() {
        let c = InstanceConfig::from_settings(&json!({
            "max_retries": 0, "retry_backoff_ms": 0,
        }));
        assert_eq!(c.max_retries, 0);     // never retry
        assert_eq!(c.retry_backoff_ms, 0); // retry immediately
    }

    #[test]
    fn values_are_read_and_clamped() {
        let c = InstanceConfig::from_settings(&json!({
            "node_timeout_secs": 120, "max_execution_history": 50, "code_node_timeout_secs": 10,
        }));
        assert_eq!(c.node_timeout_secs, 120);
        assert_eq!(c.max_execution_history, 50);
        assert_eq!(c.code_node_timeout_secs, 10);
    }

    #[test]
    fn out_of_range_falls_back() {
        // history 0 is invalid (must keep at least one), timeout absurdly large.
        let c = InstanceConfig::from_settings(&json!({
            "max_execution_history": 0, "node_timeout_secs": 999_999,
        }));
        assert_eq!(c.max_execution_history, 500);
        assert_eq!(c.node_timeout_secs, 60);
    }
}
