use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

/// Ligne de la table `flow.executions`.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Execution {
    pub id:             Uuid,
    pub job_id:         Option<Uuid>,
    pub workflow_id:    Uuid,
    pub owner_id:       Uuid,
    pub status:         String,
    pub trigger_source: String,
    pub trigger_data:   Value,
    pub duration_ms:    Option<i32>,
    pub nodes_executed: i32,
    pub nodes_total:    i32,
    pub error_message:  Option<String>,
    pub started_at:     DateTime<Utc>,
    pub finished_at:    Option<DateTime<Utc>>,
}

/// Ligne de la table `flow.node_logs`.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct NodeLog {
    pub id:                Uuid,
    pub execution_id:      Uuid,
    pub node_id:           String,
    pub node_type:         String,
    pub node_name:         Option<String>,
    pub status:            String,
    pub input_data:        Option<Value>,
    pub output_data:       Option<Value>,
    pub error_message:     Option<String>,
    pub error_stack:       Option<String>,
    pub duration_ms:       Option<i32>,
    pub attempt:           i32,
    pub proxy_duration_ms: Option<i32>,
    pub proxy_status_code: Option<i16>,
    pub executed_at:       DateTime<Utc>,
}

/// Statut d'exécution (workflow et nœud).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStatus {
    Running,
    Success,
    Error,
    Stopped,
}

impl ExecutionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionStatus::Running => "running",
            ExecutionStatus::Success => "success",
            ExecutionStatus::Error   => "error",
            ExecutionStatus::Stopped => "stopped",
        }
    }
}
