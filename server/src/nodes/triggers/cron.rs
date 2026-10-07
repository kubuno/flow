use async_trait::async_trait;
use serde_json::Value;

use super::trigger_node;
use crate::nodes::trait_::{
    ExecutionContext, FieldDef, FieldType, NodeCategory, NodeContext, NodeError, NodeMeta, NodeOutput,
};

pub struct CronTrigger;

impl CronTrigger {
    pub fn meta_impl() -> NodeMeta {
        NodeMeta {
            node_type:   "trigger.cron".into(),
            name:        "Planification".into(),
            description: "Déclenche le workflow selon une expression cron (5 champs)".into(),
            category:    NodeCategory::Trigger,
            icon:        "Clock".into(),
            color:       "#1a73e8".into(),
            inputs:      0,
            outputs:     vec![],
            fields:      vec![
                FieldDef::new("cron", "Expression cron", FieldType::Text)
                    .required()
                    .placeholder("0 * * * *")
                    .help("min heure jour mois jour-semaine — ex: 0 9 * * 1 (lundi 9h)"),
            ],
        }
    }
}

trigger_node!(CronTrigger);
