//! Flow-control nodes that operate on other Flow workflows (sub-workflows).

use async_trait::async_trait;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::models::workflow::WorkflowDefinition;
use crate::nodes::trait_::{
    ExecutionContext, FieldDef, FieldType, NodeCategory, NodeContext, NodeError, NodeMeta, NodeOutput,
};
use crate::runtime::executor::run_workflow_inline;

/// Execute another workflow inline and return its final node's output.
pub struct SubWorkflowNode;

#[async_trait]
impl crate::nodes::trait_::NodeExecutor for SubWorkflowNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "flow.subworkflow".into(),
            name:      "Sous-workflow".into(),
            description: "Exécute un autre workflow et récupère son résultat".into(),
            category:  NodeCategory::Logic,
            icon:      "Workflow".into(),
            color:     "#7e57c2".into(),
            inputs:    1,
            outputs:   vec![],
            fields:    vec![
                FieldDef::new("workflow_id", "Workflow", FieldType::Expression)
                    .required()
                    .placeholder("UUID du workflow à exécuter")
                    .help("Identifiant du workflow appelé (visible dans son URL)."),
                FieldDef::new("input", "Données d'entrée (JSON)", FieldType::Json)
                    .help("Données transmises au déclencheur du sous-workflow. Vide = données entrantes."),
            ],
        }
    }

    async fn execute(&self, config: Value, ctx: &ExecutionContext, n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let wf_str = config.get("workflow_id").and_then(|v| v.as_str())
            .ok_or(NodeError::MissingField("workflow_id"))?;
        let wf_id = Uuid::parse_str(wf_str.trim())
            .map_err(|_| NodeError::InvalidConfig("Identifiant de workflow invalide".into()))?;

        // The sub-workflow must belong to the same owner (no cross-user execution).
        let file_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT file_id FROM flow.workflows WHERE id = $1 AND owner_id = $2 AND is_trashed = FALSE",
        )
        .bind(wf_id)
        .bind(n.user_id)
        .fetch_optional(n.db)
        .await
        .map_err(|e| { tracing::error!(error=%e, "Sous-workflow : lecture DB"); NodeError::ServiceError(e.to_string()) })?
        .ok_or_else(|| NodeError::InvalidConfig("Workflow introuvable".into()))?;

        let def_value = match file_id {
            Some(fid) => {
                let (_info, raw) = n.files_client.get_file_content(n.user_id, fid).await
                    .map_err(|e| NodeError::ServiceError(e.to_string()))?;
                crate::services::content_files::parse_definition_bytes(&raw)
                    .map_err(|e| NodeError::ServiceError(e.to_string()))?
            }
            None => json!({ "nodes": [], "edges": [] }),
        };
        let definition = WorkflowDefinition::from_value(&def_value);

        // Trigger data for the sub-workflow: explicit `input` field or this node's input.
        let trigger = config.get("input").filter(|v| !v.is_null()).cloned()
            .unwrap_or_else(|| ctx.input.clone());

        let output = run_workflow_inline(n, n.user_id, wf_id, &definition, trigger)
            .await
            .map_err(NodeError::Other)?;
        Ok(NodeOutput::data(output))
    }
}

/// Run a sub-workflow once per item of an array input, collecting the results.
/// This is Kubuno Flow's batching/loop primitive (the topological executor itself
/// has no graph cycles, so iteration is expressed by fan-out over a sub-workflow).
pub struct LoopItemsNode;

#[async_trait]
impl crate::nodes::trait_::NodeExecutor for LoopItemsNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "flow.loop_items".into(),
            name:      "Boucle sur les éléments".into(),
            description: "Exécute un sous-workflow pour chaque élément d'un tableau".into(),
            category:  NodeCategory::Logic,
            icon:      "Repeat".into(),
            color:     "#7e57c2".into(),
            inputs:    1,
            outputs:   vec![],
            fields:    vec![
                FieldDef::new("workflow_id", "Sous-workflow", FieldType::Expression)
                    .required().placeholder("UUID du workflow à exécuter par élément"),
                FieldDef::new("items", "Champ tableau", FieldType::Expression)
                    .placeholder("{{ $json.items }} — vide = l'entrée elle-même"),
            ],
        }
    }

    async fn execute(&self, config: Value, ctx: &ExecutionContext, n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        const MAX_ITEMS: usize = 500;
        let wf_str = config.get("workflow_id").and_then(|v| v.as_str())
            .ok_or(NodeError::MissingField("workflow_id"))?;
        let wf_id = Uuid::parse_str(wf_str.trim())
            .map_err(|_| NodeError::InvalidConfig("Identifiant de workflow invalide".into()))?;

        // Array source: explicit `items` (already resolved) or the node input.
        let source = config.get("items").filter(|v| !v.is_null()).cloned().unwrap_or_else(|| ctx.input.clone());
        let items: Vec<Value> = match source {
            Value::Array(a) => a,
            Value::Null => vec![],
            other => vec![other],
        };
        if items.len() > MAX_ITEMS {
            return Err(NodeError::InvalidConfig(format!("Trop d'éléments ({}, max {MAX_ITEMS})", items.len())));
        }

        let file_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT file_id FROM flow.workflows WHERE id = $1 AND owner_id = $2 AND is_trashed = FALSE",
        )
        .bind(wf_id)
        .bind(n.user_id)
        .fetch_optional(n.db)
        .await
        .map_err(|e| { tracing::error!(error=%e, "Boucle : lecture DB"); NodeError::ServiceError(e.to_string()) })?
        .ok_or_else(|| NodeError::InvalidConfig("Workflow introuvable".into()))?;

        let def_value = match file_id {
            Some(fid) => {
                let (_info, raw) = n.files_client.get_file_content(n.user_id, fid).await
                    .map_err(|e| NodeError::ServiceError(e.to_string()))?;
                crate::services::content_files::parse_definition_bytes(&raw)
                    .map_err(|e| NodeError::ServiceError(e.to_string()))?
            }
            None => json!({ "nodes": [], "edges": [] }),
        };
        let definition = WorkflowDefinition::from_value(&def_value);

        let mut results: Vec<Value> = Vec::with_capacity(items.len());
        for item in items {
            let out = run_workflow_inline(n, n.user_id, wf_id, &definition, item)
                .await
                .map_err(NodeError::Other)?;
            results.push(out);
        }
        Ok(NodeOutput::data(json!({ "items": results, "count": results.len() })))
    }
}
