use async_trait::async_trait;
use serde_json::Value;

use super::trigger_node;
use crate::nodes::trait_::{
    ExecutionContext, FieldDef, FieldType, NodeCategory, NodeContext, NodeError, NodeMeta, NodeOutput,
};

pub struct SseTrigger;

impl SseTrigger {
    pub fn meta_impl() -> NodeMeta {
        NodeMeta {
            node_type:   "trigger.sse".into(),
            name:        "Server-Sent Events (SSE)".into(),
            description: "Déclenche le workflow à chaque événement d'un flux SSE distant".into(),
            category:    NodeCategory::Trigger,
            icon:        "Radio".into(),
            color:       "#1a73e8".into(),
            inputs:      0,
            outputs:     vec![],
            fields:      vec![
                FieldDef::new("url", "URL du flux SSE", FieldType::Text).required().placeholder("https://exemple.com/events"),
                FieldDef::new("headers", "En-têtes (JSON)", FieldType::Json).help(r#"{"Authorization":"Bearer …"}"#),
            ],
        }
    }
}

trigger_node!(SseTrigger);
