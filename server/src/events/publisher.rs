//! Publication d'événements Flow sur le bus Kubuno (via le CoreProxy).

use serde_json::json;
use uuid::Uuid;

use crate::state::AppState;

pub async fn publish_workflow_executed(state: &AppState, workflow_id: Uuid, owner_id: Uuid) {
    let event = json!({
        "type": "Custom",
        "payload": {
            "event_type": "WorkflowExecuted",
            "module_id":  "flow",
            "payload": { "workflow_id": workflow_id, "user_id": owner_id }
        }
    });
    let _ = state.proxy.publish_event(&event).await;
}
