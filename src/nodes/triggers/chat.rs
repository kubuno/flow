use async_trait::async_trait;
use serde_json::Value;

use super::trigger_node;
use crate::nodes::trait_::{
    ExecutionContext, FieldDef, FieldType, NodeCategory, NodeContext, NodeError, NodeMeta, NodeOutput,
};

pub struct ChatTrigger;

impl ChatTrigger {
    pub fn meta_impl() -> NodeMeta {
        NodeMeta {
            node_type:   "trigger.chat".into(),
            name:        "Message de chat".into(),
            description: "Déclenche le workflow à la réception d'un message (idéal pour les nœuds IA)".into(),
            category:    NodeCategory::Trigger,
            icon:        "MessageSquare".into(),
            color:       "#1a73e8".into(),
            inputs:      0,
            outputs:     vec![],
            fields:      vec![
                FieldDef::new("conversation_id", "Conversation (optionnel)", FieldType::Text)
                    .help("Vide = toutes les conversations. Sinon, l'identifiant de la conversation à écouter."),
            ],
        }
    }
}

trigger_node!(ChatTrigger);
