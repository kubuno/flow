//! Backoff exponentiel pour les retries de jobs.

use std::time::Duration;

/// Délai avant la prochaine tentative (backoff exponentiel plafonné à 5 min).
pub fn backoff_delay(attempt: i32, base_ms: u64) -> Duration {
    let exp = attempt.clamp(0, 10) as u32;
    let ms = base_ms.saturating_mul(2u64.saturating_pow(exp));
    Duration::from_millis(ms.min(300_000))
}
