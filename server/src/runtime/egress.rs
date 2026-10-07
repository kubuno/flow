//! Administrator host policy for outbound requests.
//!
//! This layer sits STRICTLY ON TOP of `net_guard::guard_host`, the
//! unconditional anti-SSRF guard applied to every outbound URL and to every raw
//! socket: the guard always runs first and can never be turned off from the
//! console. What an administrator gets here is the ability
//! to narrow the remaining surface — an allowlist of the only hosts a workflow
//! may reach, and a denylist of hosts it may never reach. Neither list can widen
//! anything: a host the guard refuses stays refused whatever the lists say.
//!
//! Both lists are plain text, one entry per line, blank lines and `#` comments
//! ignored. An entry is a HOST, not a URL, but a pasted URL is tolerated — the
//! scheme, the port, the path and any credentials are stripped. `*.example.com`
//! matches sub-domains only; list `example.com` too if the apex must be reached.
//! Matching is case-insensitive and the trailing dot of an absolute name is
//! ignored, so `Example.COM.` and `example.com` are the same entry.
//!
//! The denylist wins over the allowlist: an entry present in both is refused.

use serde_json::Value;
use std::sync::{Arc, RwLock};

/// One parsed entry of a list.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Pattern {
    /// `example.com` — that exact host.
    Exact(String),
    /// `*.example.com` — any sub-domain of it, but NOT the apex.
    Subdomains(String),
}

impl Pattern {
    fn matches(&self, host: &str) -> bool {
        match self {
            Pattern::Exact(h) => host == h,
            Pattern::Subdomains(suffix) => {
                host.len() > suffix.len() + 1
                    && host.ends_with(suffix)
                    && host.as_bytes()[host.len() - suffix.len() - 1] == b'.'
            }
        }
    }
}

/// The two lists as the administrator left them. An empty allowlist means "no
/// allowlist" — every host the SSRF guard already accepted stays reachable.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EgressPolicy {
    allow: Vec<Pattern>,
    deny:  Vec<Pattern>,
}

/// Shared handle: built at boot, replaced in place by the settings refresher so
/// an administrator's edit takes effect without a restart.
pub type SharedEgressPolicy = Arc<RwLock<EgressPolicy>>;

/// Normalises one line into a matchable host pattern. Returns `None` for a line
/// that carries nothing (blank, comment, or an entry that boils down to nothing).
fn parse_entry(raw: &str) -> Option<Pattern> {
    let mut s = raw.trim();
    if s.is_empty() || s.starts_with('#') {
        return None;
    }
    // Tolerate a pasted URL: drop the scheme, then anything from the first '/'.
    if let Some(pos) = s.find("://") {
        s = &s[pos + 3..];
    }
    if let Some(pos) = s.find('/') {
        s = &s[..pos];
    }
    // Drop userinfo (`user:pass@host`) — everything before the last '@'.
    if let Some(pos) = s.rfind('@') {
        s = &s[pos + 1..];
    }
    // Drop the port. An IPv6 literal is bracketed, so only strip a ':' that is
    // not part of `[…]`.
    let s = if let Some(rest) = s.strip_prefix('[') {
        match rest.find(']') {
            Some(end) => &rest[..end],
            None => rest,
        }
    } else if let Some(pos) = s.find(':') {
        &s[..pos]
    } else {
        s
    };

    let host = s.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    match host.strip_prefix("*.") {
        Some(suffix) if !suffix.is_empty() => Some(Pattern::Subdomains(suffix.to_string())),
        _ if host == "*" => None, // "everything" is what an empty list already means
        _ => Some(Pattern::Exact(host)),
    }
}

fn parse_list(settings: &Value, key: &str) -> Vec<Pattern> {
    settings
        .get(key)
        .and_then(Value::as_str)
        .map(|text| text.lines().filter_map(parse_entry).collect())
        .unwrap_or_default()
}

impl EgressPolicy {
    /// Reads both lists out of the core's `{key: value}` settings object. A
    /// missing or non-string value yields an empty list, i.e. no extra
    /// restriction — never a lock-out caused by a malformed setting.
    pub fn from_settings(settings: &Value) -> Self {
        Self {
            allow: parse_list(settings, "egress_allowed_hosts"),
            deny:  parse_list(settings, "egress_denied_hosts"),
        }
    }

    /// True when the administrator narrowed anything at all.
    pub fn is_empty(&self) -> bool {
        self.allow.is_empty() && self.deny.is_empty()
    }

    /// Verdict for one host. `Err` carries the reason, already in French, ready
    /// to be surfaced in an execution log. The host is normalised the same way
    /// the entries are, so `EXAMPLE.com.` is judged like `example.com`.
    pub fn check(&self, host: &str) -> Result<(), String> {
        let h = host.trim().trim_end_matches('.').to_ascii_lowercase();
        let h = h.strip_prefix('[').map(|r| r.trim_end_matches(']')).unwrap_or(&h);

        if self.deny.iter().any(|p| p.matches(h)) {
            return Err(format!("hôte refusé par la liste noire de l'instance : {h}"));
        }
        if !self.allow.is_empty() && !self.allow.iter().any(|p| p.matches(h)) {
            return Err(format!(
                "hôte absent de la liste blanche de l'instance : {h}"
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_empty_policy_allows_everything() {
        let p = EgressPolicy::default();
        assert!(p.is_empty());
        assert!(p.check("example.com").is_ok());
    }

    #[test]
    fn entries_tolerate_pasted_urls_ports_and_case() {
        let p = EgressPolicy::from_settings(&json!({
            "egress_allowed_hosts": "https://API.Example.com:8443/v1/things\n\n# commentaire\nhooks.slack.com\n",
        }));
        assert!(p.check("api.example.com").is_ok());
        assert!(p.check("hooks.slack.com").is_ok());
        assert!(p.check("evil.com").is_err());
    }

    #[test]
    fn a_wildcard_matches_subdomains_but_not_the_apex() {
        let p = EgressPolicy::from_settings(&json!({
            "egress_allowed_hosts": "*.example.com",
        }));
        assert!(p.check("a.example.com").is_ok());
        assert!(p.check("a.b.example.com").is_ok());
        assert!(p.check("example.com").is_err());
        assert!(p.check("notexample.com").is_err());
    }

    #[test]
    fn the_denylist_wins_over_the_allowlist() {
        let p = EgressPolicy::from_settings(&json!({
            "egress_allowed_hosts": "*.example.com",
            "egress_denied_hosts":  "secret.example.com",
        }));
        assert!(p.check("public.example.com").is_ok());
        assert!(p.check("secret.example.com").is_err());
    }

    #[test]
    fn a_denylist_alone_leaves_the_rest_reachable() {
        let p = EgressPolicy::from_settings(&json!({
            "egress_denied_hosts": "bad.example.com\n*.tracker.net",
        }));
        assert!(p.check("anything.org").is_ok());
        assert!(p.check("bad.example.com").is_err());
        assert!(p.check("a.tracker.net").is_err());
    }

    #[test]
    fn a_lone_star_is_not_an_allowlist_of_one() {
        // "*" would otherwise parse as an exact host nothing can match, locking
        // the instance out of every external call.
        let p = EgressPolicy::from_settings(&json!({ "egress_allowed_hosts": "*" }));
        assert!(p.is_empty());
        assert!(p.check("example.com").is_ok());
    }
}
