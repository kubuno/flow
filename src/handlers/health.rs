use axum::{extract::State, Json};
use kubuno_db::params;
use serde_json::{json, Value};

use crate::state::AppState;

pub async fn health(State(state): State<AppState>) -> Json<Value> {
    let db_ok = state.db.execute("SELECT 1", params![]).await.is_ok();
    Json(json!({
        "status":  if db_ok { "ok" } else { "degraded" },
        "module":  "flow",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}
