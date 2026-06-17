//! SSE triggers. A manager reconciles active `trigger.sse` nodes every 30s and
//! spawns one connection task per (workflow, node) that streams Server-Sent
//! Events and enqueues a job per event. Each connection has a bounded lifetime
//! (5 min) then exits; the manager re-spawns it if the workflow is still active,
//! which also stops connections for deactivated workflows.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::StreamExt;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::models::workflow::WorkflowDefinition;
use crate::runtime::queue;
use crate::state::AppState;

const RECONCILE_SECS: u64 = 30;
const CONNECTION_TTL_SECS: u64 = 300;

pub fn spawn(state: AppState) {
    tokio::spawn(async move { manager(state).await });
}

async fn manager(state: AppState) {
    tracing::info!("Flow : worker de déclencheurs SSE démarré");
    let active: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
    loop {
        let workflows = sqlx::query_as::<_, (Uuid, Uuid, Option<Uuid>)>(
            "SELECT id, owner_id, file_id FROM flow.workflows WHERE status = 'active' AND is_trashed = FALSE",
        )
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        for (wf_id, owner_id, file_id) in workflows {
            let Some(fid) = file_id else { continue };
            let def_val = crate::services::content_files::read_definition(&state, owner_id, fid).await
                .unwrap_or_else(|_| crate::services::content_files::empty_definition());
            let def = WorkflowDefinition::from_value(&def_val);
            for node in def.nodes.iter().filter(|n| n.node_type == "trigger.sse") {
                let url = node.config.get("url").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
                let Some(url) = url else { continue };
                let key = format!("{wf_id}:{}", node.id);
                {
                    let mut set = active.lock().unwrap();
                    if set.contains(&key) { continue; }
                    set.insert(key.clone());
                }
                let headers = node.config.get("headers").cloned().unwrap_or(Value::Null);
                let st = state.clone();
                let act = active.clone();
                let url = url.to_string();
                tokio::spawn(async move {
                    let _ = tokio::time::timeout(
                        Duration::from_secs(CONNECTION_TTL_SECS),
                        stream_sse(&st, wf_id, owner_id, &url, &headers),
                    ).await;
                    act.lock().unwrap().remove(&key);
                });
            }
        }
        tokio::time::sleep(Duration::from_secs(RECONCILE_SECS)).await;
    }
}

async fn stream_sse(state: &AppState, wf_id: Uuid, owner: Uuid, url: &str, headers: &Value) {
    let client = match reqwest::Client::builder().build() {
        Ok(c) => c,
        Err(_) => return,
    };
    let mut req = client.get(url).header("Accept", "text/event-stream");
    if let Some(obj) = headers.as_object() {
        for (k, v) in obj {
            if let Some(val) = v.as_str() { req = req.header(k, val); }
        }
    }
    let resp = match req.send().await {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => { tracing::warn!(workflow = %wf_id, status = %r.status(), "SSE : réponse non-2xx"); return; }
        Err(e) => { tracing::warn!(workflow = %wf_id, error = %e, "SSE : connexion échouée"); return; }
    };

    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    let mut data_lines: Vec<String> = Vec::new();
    let mut event_name: Option<String> = None;

    while let Some(chunk) = stream.next().await {
        let Ok(bytes) = chunk else { break };
        buf.push_str(&String::from_utf8_lossy(&bytes));
        while let Some(pos) = buf.find('\n') {
            let line: String = buf.drain(..=pos).collect();
            let line = line.trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                // Fin d'un événement → on déclenche.
                if !data_lines.is_empty() {
                    let raw = data_lines.join("\n");
                    let parsed: Value = serde_json::from_str(&raw).unwrap_or(Value::String(raw));
                    let trigger_data = json!({ "event": event_name.clone(), "data": parsed });
                    let _ = queue::enqueue(&state.db, wf_id, owner, "sse", trigger_data, state.settings.runtime.max_retries).await;
                    data_lines.clear();
                    event_name = None;
                }
            } else if let Some(d) = line.strip_prefix("data:") {
                data_lines.push(d.strip_prefix(' ').unwrap_or(d).to_string());
            } else if let Some(e) = line.strip_prefix("event:") {
                event_name = Some(e.strip_prefix(' ').unwrap_or(e).to_string());
            }
            // id:, retry:, lignes de commentaire « : … » ignorées.
        }
    }
}
