use async_trait::async_trait;
use serde_json::Value;

use super::trigger_node;
use crate::nodes::trait_::{
    ExecutionContext, FieldDef, FieldType, NodeCategory, NodeContext, NodeError, NodeMeta, NodeOutput,
};

pub struct FormTrigger;

impl FormTrigger {
    pub fn meta_impl() -> NodeMeta {
        NodeMeta {
            node_type:   "trigger.form".into(),
            name:        "Soumission de formulaire".into(),
            description: "Déclenche le workflow quand un formulaire Kubuno est soumis".into(),
            category:    NodeCategory::Trigger,
            icon:        "ClipboardList".into(),
            color:       "#1a73e8".into(),
            inputs:      0,
            outputs:     vec![],
            fields:      vec![
                FieldDef::new("form_id", "Formulaire (optionnel)", FieldType::Text)
                    .help("Vide = tous les formulaires. Sinon, l'identifiant du formulaire à écouter."),
            ],
        }
    }
}

trigger_node!(FormTrigger);
