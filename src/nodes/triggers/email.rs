use async_trait::async_trait;
use serde_json::{json, Value};

use super::trigger_node;
use crate::nodes::trait_::{
    ExecutionContext, FieldDef, FieldType, NodeCategory, NodeContext, NodeError, NodeMeta, NodeOutput,
};

pub struct EmailTrigger;

impl EmailTrigger {
    pub fn meta_impl() -> NodeMeta {
        NodeMeta {
            node_type:   "trigger.email".into(),
            name:        "E-mail (IMAP / POP3)".into(),
            description: "Déclenche le workflow à la réception d'un nouvel e-mail".into(),
            category:    NodeCategory::Trigger,
            icon:        "Mail".into(),
            color:       "#1a73e8".into(),
            inputs:      0,
            outputs:     vec![],
            fields:      vec![
                FieldDef::new("protocol", "Protocole", FieldType::Select)
                    .options(&[("imap", "IMAP"), ("pop3", "POP3")]).default(json!("imap")),
                FieldDef::credential("credential", "Credential (IMAP/POP3)", "imap"),
                FieldDef::new("host", "Hôte (si pas de credential)", FieldType::Text).placeholder("imap.exemple.com"),
                FieldDef::new("port", "Port", FieldType::Number).help("IMAP 993 / POP3 995 (TLS)."),
                FieldDef::new("username", "Utilisateur (si pas de credential)", FieldType::Text),
                FieldDef::new("password", "Mot de passe (si pas de credential)", FieldType::Text),
                FieldDef::new("secure", "TLS/SSL", FieldType::Boolean).default(json!(true)),
                FieldDef::new("folder", "Dossier (IMAP)", FieldType::Text).default(json!("INBOX")),
            ],
        }
    }
}

trigger_node!(EmailTrigger);
