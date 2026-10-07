use async_trait::async_trait;
use serde_json::Value;

use super::trigger_node;
use crate::nodes::trait_::{
    ExecutionContext, NodeCategory, NodeContext, NodeError, NodeMeta, NodeOutput,
};

pub struct ErrorTrigger;

impl ErrorTrigger {
    pub fn meta_impl() -> NodeMeta {
        NodeMeta {
            node_type:   "trigger.error".into(),
            name:        "Déclencheur d'erreur".into(),
            description: "Démarre ce workflow quand un AUTRE de vos workflows échoue".into(),
            category:    NodeCategory::Trigger,
            icon:        "TriangleAlert".into(),
            color:       "#d93025".into(),
            inputs:      0,
            outputs:     vec![],
            // Données reçues : { workflow: {id, name}, error: {message}, execution: {id} }.
            fields:      vec![],
        }
    }
}

trigger_node!(ErrorTrigger);
