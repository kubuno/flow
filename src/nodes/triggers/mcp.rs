use async_trait::async_trait;
use serde_json::Value;

use super::trigger_node;
use crate::nodes::trait_::{
    ExecutionContext, FieldDef, FieldType, NodeCategory, NodeContext, NodeError, NodeMeta, NodeOutput,
};

pub struct McpTrigger;

impl McpTrigger {
    pub fn meta_impl() -> NodeMeta {
        NodeMeta {
            node_type:   "trigger.mcp".into(),
            name:        "Serveur MCP".into(),
            description: "Expose ce workflow comme outil appelable via le serveur MCP de Kubuno".into(),
            category:    NodeCategory::Trigger,
            icon:        "Plug".into(),
            color:       "#7e57c2".into(),
            inputs:      0,
            outputs:     vec![],
            fields:      vec![
                FieldDef::new("description", "Description (pour l'IA)", FieldType::Textarea)
                    .help("Décrit ce que fait ce workflow, pour que l'agent IA sache quand l'appeler."),
                FieldDef::new("input_hint", "Entrée attendue (JSON, indicatif)", FieldType::Json)
                    .help("Exemple de structure d'entrée transmise par l'appelant."),
            ],
        }
    }
}

trigger_node!(McpTrigger);
