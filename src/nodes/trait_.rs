//! Trait commun à tous les nœuds et types de métadonnées du catalogue.

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::config::Settings;
use crate::runtime::core_proxy::CoreProxy;

// ── Métadonnées (catalogue exposé au frontend) ──────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NodeCategory {
    Trigger,
    Kubuno,
    Logic,
    External,
    Code,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    Text,
    Textarea,
    Expression,
    Number,
    Boolean,
    Select,
    Json,
    Code,
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldDef {
    pub name:        String,
    pub label:       String,
    #[serde(rename = "type")]
    pub field_type:  FieldType,
    #[serde(default)]
    pub required:    bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help:        Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default:     Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options:     Option<Vec<FieldOption>>,
}

impl FieldDef {
    pub fn new(name: &str, label: &str, field_type: FieldType) -> Self {
        Self {
            name: name.into(),
            label: label.into(),
            field_type,
            required: false,
            placeholder: None,
            help: None,
            default: None,
            options: None,
        }
    }
    pub fn required(mut self) -> Self { self.required = true; self }
    pub fn help(mut self, h: &str) -> Self { self.help = Some(h.into()); self }
    pub fn placeholder(mut self, p: &str) -> Self { self.placeholder = Some(p.into()); self }
    pub fn default(mut self, v: Value) -> Self { self.default = Some(v); self }
    pub fn options(mut self, opts: &[(&str, &str)]) -> Self {
        self.options = Some(opts.iter().map(|(v, l)| FieldOption { value: v.to_string(), label: l.to_string() }).collect());
        self
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PortDef {
    pub id:    String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeMeta {
    #[serde(rename = "type")]
    pub node_type:   String,
    pub name:        String,
    #[serde(default)]
    pub description: String,
    pub category:    NodeCategory,
    pub icon:        String,
    pub color:       String,
    /// Nombre de ports d'entrée (0 pour les triggers).
    pub inputs:      u8,
    /// Ports de sortie nommés. Vide → une sortie par défaut.
    pub outputs:     Vec<PortDef>,
    pub fields:      Vec<FieldDef>,
}

impl NodeMeta {
    pub fn is_trigger(&self) -> bool {
        self.category == NodeCategory::Trigger
    }
}

// ── Exécution ───────────────────────────────────────────────────────────────────

/// Contexte d'infrastructure passé aux nœuds (canal sortant unique = CoreProxy).
pub struct NodeContext<'a> {
    pub proxy:    &'a CoreProxy,
    pub user_id:  Uuid,
    pub db:       &'a sqlx::PgPool,
    pub settings: &'a Settings,
}

/// Contexte de l'exécution courante d'un nœud.
pub struct ExecutionContext {
    pub execution_id:    Uuid,
    pub workflow_id:     Uuid,
    pub owner_id:        Uuid,
    pub current_node_id: String,
    pub attempt:         i32,
    /// Données entrantes du nœud (sortie du/des prédécesseur(s)).
    pub input:          Value,
    /// Contexte global pour la résolution d'expressions ({{ trigger.x }}, {{ nodes.y }}…).
    pub full:           Value,
}

impl ExecutionContext {
    /// Clé d'idempotence stable pour cette exécution + ce nœud + cette tentative.
    pub fn idempotency_key(&self) -> String {
        format!("flow-{}-{}-{}", self.execution_id, self.current_node_id, self.attempt)
    }
}

/// Sortie d'un nœud : donnée + ports actifs (pour le branchement).
pub struct NodeOutput {
    pub data:     Value,
    /// `None` → toutes les sorties sont actives. `Some([...])` → seules ces sorties.
    pub branches: Option<Vec<String>>,
}

impl NodeOutput {
    pub fn data(data: Value) -> Self {
        Self { data, branches: None }
    }
    pub fn branch(data: Value, ports: Vec<String>) -> Self {
        Self { data, branches: Some(ports) }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NodeError {
    #[error("Champ requis manquant : {0}")]
    MissingField(&'static str),
    #[error("Configuration invalide : {0}")]
    InvalidConfig(String),
    #[error("Erreur du proxy core : {0}")]
    ProxyError(String),
    #[error("Erreur du service : {0}")]
    ServiceError(String),
    #[error("Arrêt demandé : {0}")]
    Stopped(String),
    #[error("{0}")]
    Other(String),
}

impl NodeError {
    /// L'erreur justifie-t-elle un retry du job ?
    pub fn is_retryable(&self) -> bool {
        matches!(self, NodeError::ProxyError(_) | NodeError::ServiceError(_))
    }
}

#[async_trait]
pub trait NodeExecutor: Send + Sync {
    fn meta(&self) -> NodeMeta;

    /// `config` est déjà résolu (expressions `{{ }}` substituées).
    async fn execute(
        &self,
        config:   Value,
        exec_ctx: &ExecutionContext,
        node_ctx: &NodeContext<'_>,
    ) -> Result<NodeOutput, NodeError>;
}
