use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

/// Position d'un nœud sur le canvas.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NodePosition {
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
}

/// Un nœud du workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowNode {
    pub id:   String,
    /// Type de nœud, ex: "kubuno.mail.send", "logic.if", "trigger.webhook"
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub position: NodePosition,
    #[serde(default)]
    pub config: Value,
}

/// Une arête reliant la sortie d'un nœud à l'entrée d'un autre.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEdge {
    pub id:     String,
    pub source: String,
    pub target: String,
    /// Port de sortie source (ex: "true"/"false" pour If, "0".."n" pour Switch).
    #[serde(default)]
    pub source_port: Option<String>,
    #[serde(default)]
    pub target_port: Option<String>,
}

/// Définition complète d'un workflow (colonne JSONB `definition`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkflowDefinition {
    #[serde(default)]
    pub nodes: Vec<WorkflowNode>,
    #[serde(default)]
    pub edges: Vec<WorkflowEdge>,
}

impl WorkflowDefinition {
    pub fn from_value(v: &Value) -> Self {
        serde_json::from_value(v.clone()).unwrap_or_default()
    }
}

/// Ligne de la table `flow.workflows`.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Workflow {
    pub id:               Uuid,
    pub owner_id:         Uuid,
    pub name:             String,
    pub description:      Option<String>,
    // La définition (nodes/edges) vit dans un fichier .kbflw ; peuplée après le SELECT.
    #[sqlx(default)]
    pub definition:       Value,
    pub file_id:          Option<Uuid>,
    pub status:           String,
    pub execution_count:  i32,
    pub error_count:      i32,
    pub last_executed_at: Option<DateTime<Utc>>,
    pub last_error:       Option<String>,
    pub tags:             Vec<String>,
    pub is_trashed:       bool,
    pub created_at:       DateTime<Utc>,
    pub updated_at:       DateTime<Utc>,
}

#[derive(Debug, Deserialize, validator::Validate)]
pub struct CreateWorkflowDto {
    #[validate(length(min = 1, max = 255))]
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub definition: Option<Value>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, validator::Validate)]
pub struct UpdateWorkflowDto {
    #[serde(default)]
    #[validate(length(min = 1, max = 255))]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub definition: Option<Value>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub status: Option<String>,
}
