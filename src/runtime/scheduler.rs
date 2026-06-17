//! Déclencheurs temporels et événementiels.
//!
//! - **Cron** : balaye chaque minute les workflows actifs ayant un nœud
//!   `trigger.cron` dont l'expression correspond à l'instant courant.
//! - **Événements** : écoute le canal PostgreSQL `kubuno_events` (LISTEN/NOTIFY,
//!   le même que le core) et déclenche les workflows abonnés via `trigger.kubuno_event`.

use chrono::{Datelike, Timelike, Utc};
use serde_json::Value;
use sqlx::postgres::PgListener;
use uuid::Uuid;

use crate::models::workflow::WorkflowDefinition;
use crate::runtime::queue;
use crate::state::AppState;

pub fn spawn_schedulers(state: AppState) {
    crate::runtime::email_trigger::spawn(state.clone());
    crate::runtime::sse_trigger::spawn(state.clone());

    let cron_state = state.clone();
    tokio::spawn(async move { cron_loop(cron_state).await });

    let event_state = state.clone();
    tokio::spawn(async move {
        loop {
            if let Err(e) = event_loop(event_state.clone()).await {
                tracing::warn!(error = %e, "Écouteur d'événements interrompu, reconnexion dans 5s…");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    });
}

// ── CRON ─────────────────────────────────────────────────────────────────────────

async fn cron_loop(state: AppState) {
    loop {
        // Attendre le prochain top de minute (00 seconde).
        let now = Utc::now();
        let wait = 60 - now.second() as u64;
        tokio::time::sleep(std::time::Duration::from_secs(wait.max(1))).await;

        let now = Utc::now();
        let active = sqlx::query_as::<_, (Uuid, Uuid, Option<Uuid>)>(
            "SELECT id, owner_id, file_id FROM flow.workflows WHERE status = 'active' AND is_trashed = FALSE",
        )
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        for (wf_id, owner_id, file_id) in active {
            let def_val = match file_id {
                Some(fid) => crate::services::content_files::read_definition(&state, owner_id, fid).await
                    .unwrap_or_else(|_| crate::services::content_files::empty_definition()),
                None => crate::services::content_files::empty_definition(),
            };
            let def = WorkflowDefinition::from_value(&def_val);
            for node in def.nodes.iter().filter(|n| n.node_type == "trigger.cron") {
                if let Some(expr) = node.config.get("cron").and_then(|v| v.as_str()) {
                    if cron_matches(expr, &now) {
                        let trigger_data = serde_json::json!({ "cron": expr, "fired_at": now.to_rfc3339() });
                        let _ = queue::enqueue(&state.db, wf_id, owner_id, "cron", trigger_data, state.settings.runtime.max_retries).await;
                    }
                }
            }
        }
    }
}

/// Matcher cron 5 champs : min hour day-of-month month day-of-week.
/// Supporte `*`, listes `a,b`, plages `a-b`, pas `*/n` et `a-b/n`.
fn cron_matches(expr: &str, dt: &chrono::DateTime<Utc>) -> bool {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() != 5 {
        return false;
    }
    let min   = dt.minute();
    let hour  = dt.hour();
    let dom   = dt.day();
    let month = dt.month();
    let dow   = dt.weekday().num_days_from_sunday(); // 0 = dimanche

    field_matches(parts[0], min, 0, 59)
        && field_matches(parts[1], hour, 0, 23)
        && field_matches(parts[2], dom, 1, 31)
        && field_matches(parts[3], month, 1, 12)
        && field_matches(parts[4], dow, 0, 6)
}

fn field_matches(field: &str, value: u32, min: u32, max: u32) -> bool {
    for part in field.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((r, s)) => (r, s.parse::<u32>().unwrap_or(1).max(1)),
            None => (part, 1),
        };
        let (lo, hi) = if range == "*" {
            (min, max)
        } else if let Some((a, b)) = range.split_once('-') {
            match (a.parse::<u32>(), b.parse::<u32>()) {
                (Ok(a), Ok(b)) => (a, b),
                _ => continue,
            }
        } else if let Ok(n) = range.parse::<u32>() {
            (n, n)
        } else {
            continue;
        };
        if value >= lo && value <= hi && step != 0 && (value - lo).is_multiple_of(step) {
            return true;
        }
    }
    false
}

// ── ÉVÉNEMENTS ─────────────────────────────────────────────────────────────────

async fn event_loop(state: AppState) -> Result<(), sqlx::Error> {
    let mut listener = PgListener::connect_with(&state.db).await?;
    listener.listen("kubuno_events").await?;
    tracing::info!("Flow : écoute du canal kubuno_events");

    loop {
        let notification = listener.recv().await?;
        let payload = notification.payload();
        let event: Value = match serde_json::from_str(payload) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let event_type = match event.get("type").and_then(|v| v.as_str()) {
            Some(t) => t.to_string(),
            None => continue,
        };
        let event_payload = event.get("payload").cloned().unwrap_or(Value::Null);

        let active = sqlx::query_as::<_, (Uuid, Uuid, Option<Uuid>)>(
            "SELECT id, owner_id, file_id FROM flow.workflows WHERE status = 'active' AND is_trashed = FALSE",
        )
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        for (wf_id, owner_id, file_id) in active {
            let def_val = match file_id {
                Some(fid) => crate::services::content_files::read_definition(&state, owner_id, fid).await
                    .unwrap_or_else(|_| crate::services::content_files::empty_definition()),
                None => crate::services::content_files::empty_definition(),
            };
            let def = WorkflowDefinition::from_value(&def_val);
            // Filtre optionnel d'un champ de config contre une clé de la charge utile.
            let payload_match = |n: &crate::models::workflow::WorkflowNode, cfg_key: &str, payload_key: &str| {
                match n.config.get(cfg_key).and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                    None => true, // pas de filtre → tout passe
                    Some(want) => event_payload.get(payload_key).and_then(|v| v.as_str()) == Some(want),
                }
            };
            let subscribes = def.nodes.iter().any(|n| match n.node_type.as_str() {
                "trigger.kubuno_event" => n.config.get("event_type").and_then(|v| v.as_str()) == Some(event_type.as_str()),
                "trigger.form" => event_type == "FormSubmitted" && payload_match(n, "form_id", "form_id"),
                "trigger.chat" => event_type == "MessageSent" && payload_match(n, "conversation_id", "conversation_id"),
                _ => false,
            });
            if subscribes {
                let trigger_data = serde_json::json!({ "event_type": event_type, "payload": event_payload });
                let _ = queue::enqueue(&state.db, wf_id, owner_id, "event", trigger_data, state.settings.runtime.max_retries).await;
            }
        }
    }
}

// ── DÉCLENCHEUR D'ERREUR ────────────────────────────────────────────────────────

/// Quand un workflow échoue, démarre les workflows (du même propriétaire) qui ont
/// un nœud `trigger.error`, en leur passant les détails de l'échec. Le garde-fou
/// `source != "error"` (côté appelant) évite les boucles infinies.
pub async fn dispatch_error_workflows(
    state: &AppState,
    failed_id: Uuid,
    failed_name: &str,
    owner_id: Uuid,
    execution_id: Uuid,
    error_message: &str,
) {
    let active = sqlx::query_as::<_, (Uuid, Option<Uuid>)>(
        "SELECT id, file_id FROM flow.workflows WHERE owner_id = $1 AND status = 'active' AND is_trashed = FALSE",
    )
    .bind(owner_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let trigger_data = serde_json::json!({
        "workflow":  { "id": failed_id.to_string(), "name": failed_name },
        "error":     { "message": error_message },
        "execution": { "id": execution_id.to_string() },
    });

    for (wf_id, file_id) in active {
        let def_val = match file_id {
            Some(fid) => crate::services::content_files::read_definition(state, owner_id, fid).await
                .unwrap_or_else(|_| crate::services::content_files::empty_definition()),
            None => crate::services::content_files::empty_definition(),
        };
        let def = WorkflowDefinition::from_value(&def_val);
        if def.nodes.iter().any(|n| n.node_type == "trigger.error") {
            let _ = queue::enqueue(&state.db, wf_id, owner_id, "error", trigger_data.clone(), state.settings.runtime.max_retries).await;
        }
    }
}
