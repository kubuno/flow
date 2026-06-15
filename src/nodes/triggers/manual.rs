use async_trait::async_trait;
use serde_json::Value;

use super::trigger_node;
use crate::nodes::trait_::{
    ExecutionContext, NodeCategory, NodeContext, NodeError, NodeMeta, NodeOutput,
};

pub struct ManualTrigger;

impl ManualTrigger {
    pub fn meta_impl() -> NodeMeta {
        NodeMeta {
            node_type:   "trigger.manual".into(),
            name:        "Déclenchement manuel".into(),
            description: "Démarre le workflow via le bouton « Tester »".into(),
            category:    NodeCategory::Trigger,
            icon:        "Play".into(),
            color:       "#1a73e8".into(),
            inputs:      0,
            outputs:     vec![],
            fields:      vec![],
        }
    }
}

trigger_node!(ManualTrigger);
