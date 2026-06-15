use async_trait::async_trait;
use serde_json::Value;

use super::trigger_node;
use crate::nodes::trait_::{
    ExecutionContext, FieldDef, FieldType, NodeCategory, NodeContext, NodeError, NodeMeta, NodeOutput,
};

pub struct WebhookTrigger;

impl WebhookTrigger {
    pub fn meta_impl() -> NodeMeta {
        NodeMeta {
            node_type:   "trigger.webhook".into(),
            name:        "Webhook".into(),
            description: "Reçoit une requête HTTP sur une URL unique".into(),
            category:    NodeCategory::Trigger,
            icon:        "Webhook".into(),
            color:       "#1a73e8".into(),
            inputs:      0,
            outputs:     vec![],
            fields:      vec![
                FieldDef::new("method", "Méthode acceptée", FieldType::Select)
                    .options(&[("POST", "POST"), ("GET", "GET"), ("*", "Toutes")])
                    .default(serde_json::json!("POST")),
            ],
        }
    }
}

trigger_node!(WebhookTrigger);
