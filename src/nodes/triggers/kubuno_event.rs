use async_trait::async_trait;
use serde_json::Value;

use super::trigger_node;
use crate::nodes::trait_::{
    ExecutionContext, FieldDef, FieldType, NodeCategory, NodeContext, NodeError, NodeMeta, NodeOutput,
};

pub struct KubunoEventTrigger;

impl KubunoEventTrigger {
    pub fn meta_impl() -> NodeMeta {
        NodeMeta {
            node_type:   "trigger.kubuno_event".into(),
            name:        "Événement Kubuno".into(),
            description: "Déclenche sur un événement du bus Kubuno (formulaire, message, fichier…)".into(),
            category:    NodeCategory::Trigger,
            icon:        "Bell".into(),
            color:       "#1a73e8".into(),
            inputs:      0,
            outputs:     vec![],
            fields:      vec![
                FieldDef::new("event_type", "Type d'événement", FieldType::Select)
                    .required()
                    .options(&[
                        ("FormSubmitted", "Formulaire soumis"),
                        ("MessageSent", "Message envoyé"),
                        ("ContactUpdated", "Contact modifié"),
                        ("FileUploaded", "Fichier ajouté"),
                        ("EventCreated", "Événement agenda créé"),
                    ]),
            ],
        }
    }
}

trigger_node!(KubunoEventTrigger);
