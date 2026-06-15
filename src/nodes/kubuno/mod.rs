//! Nœuds d'action vers les modules Kubuno. Tout passe par `CoreProxy.call_module`,
//! qui appelle le proxy du core (`/api/v1/{module}{path}`) avec l'identité de
//! l'utilisateur. Aucun client HTTP n'est créé dans ces nœuds.
//!
//! NB : les chemins/corps ci-dessous ciblent les routes réelles des modules au
//! moment de l'écriture ; ils restent ajustables si une API module évolue.

use async_trait::async_trait;
use reqwest::Method;
use serde_json::{json, Value};

use crate::nodes::trait_::{
    ExecutionContext, FieldDef, FieldType, NodeCategory, NodeContext, NodeError, NodeMeta, NodeOutput,
};
use crate::runtime::core_proxy::ProxyResponse;

fn field_str<'a>(config: &'a Value, key: &'static str) -> Result<&'a str, NodeError> {
    config.get(key).and_then(|v| v.as_str()).ok_or(NodeError::MissingField(key))
}

fn proxy_err(e: impl std::fmt::Display) -> NodeError {
    NodeError::ProxyError(e.to_string())
}

fn ok_output(resp: ProxyResponse) -> NodeOutput {
    NodeOutput::data(json!({
        "status": resp.status,
        "data":   resp.body,
    }))
}

// ── Mail : envoyer un email ──────────────────────────────────────────────────────

pub struct SendMailNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for SendMailNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "kubuno.mail.send".into(), name: "Mail — Envoyer".into(),
            description: "Envoie un email via le module Mail".into(),
            category: NodeCategory::Kubuno, icon: "Mail".into(), color: "#1a73e8".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::new("to", "Destinataire", FieldType::Expression).required().placeholder("{{ trigger.email }}"),
                FieldDef::new("subject", "Objet", FieldType::Expression).required(),
                FieldDef::new("body", "Corps (HTML)", FieldType::Textarea).required(),
            ],
        }
    }
    async fn execute(&self, config: Value, ctx: &ExecutionContext, n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let body = json!({
            "to":      field_str(&config, "to")?,
            "subject": field_str(&config, "subject")?,
            "body":    field_str(&config, "body")?,
        });
        let resp = n.proxy.call_module("mail", "/send", Method::POST, Some(body), n.user_id, Some(&ctx.idempotency_key()))
            .await.map_err(proxy_err)?;
        Ok(ok_output(resp))
    }
}

// ── Contacts : créer un contact ──────────────────────────────────────────────────

pub struct CreateContactNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for CreateContactNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "kubuno.contacts.create".into(), name: "Contacts — Créer".into(),
            description: "Crée un contact dans le carnet d'adresses".into(),
            category: NodeCategory::Kubuno, icon: "UserPlus".into(), color: "#1a73e8".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::new("first_name", "Prénom", FieldType::Expression).required(),
                FieldDef::new("last_name", "Nom", FieldType::Expression),
                FieldDef::new("email", "Email", FieldType::Expression),
                FieldDef::new("phone", "Téléphone", FieldType::Expression),
            ],
        }
    }
    async fn execute(&self, config: Value, ctx: &ExecutionContext, n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let body = json!({
            "first_name": field_str(&config, "first_name")?,
            "last_name":  config.get("last_name").cloned().unwrap_or(Value::Null),
            "email":      config.get("email").cloned().unwrap_or(Value::Null),
            "phone":      config.get("phone").cloned().unwrap_or(Value::Null),
        });
        let resp = n.proxy.call_module("contacts", "/contacts", Method::POST, Some(body), n.user_id, Some(&ctx.idempotency_key()))
            .await.map_err(proxy_err)?;
        Ok(ok_output(resp))
    }
}

// ── Chat : envoyer un message ────────────────────────────────────────────────────

pub struct SendChatNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for SendChatNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "kubuno.chat.send".into(), name: "Chat — Envoyer".into(),
            description: "Envoie un message dans une conversation".into(),
            category: NodeCategory::Kubuno, icon: "MessageSquare".into(), color: "#1a73e8".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::new("conversation_id", "Conversation", FieldType::Expression).required(),
                FieldDef::new("content", "Message", FieldType::Textarea).required(),
            ],
        }
    }
    async fn execute(&self, config: Value, ctx: &ExecutionContext, n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let conv = field_str(&config, "conversation_id")?;
        let body = json!({ "content": field_str(&config, "content")? });
        let path = format!("/conversations/{conv}/messages");
        let resp = n.proxy.call_module("chat", &path, Method::POST, Some(body), n.user_id, Some(&ctx.idempotency_key()))
            .await.map_err(proxy_err)?;
        Ok(ok_output(resp))
    }
}

// ── Agenda : créer un événement ──────────────────────────────────────────────────

pub struct CreateEventNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for CreateEventNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "kubuno.calendar.create".into(), name: "Agenda — Créer événement".into(),
            description: "Crée un événement dans l'agenda".into(),
            category: NodeCategory::Kubuno, icon: "CalendarPlus".into(), color: "#1a73e8".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::new("calendar_id", "Calendrier", FieldType::Expression),
                FieldDef::new("title", "Titre", FieldType::Expression).required(),
                FieldDef::new("start_at", "Début (ISO 8601)", FieldType::Expression).required(),
                FieldDef::new("end_at", "Fin (ISO 8601)", FieldType::Expression).required(),
                FieldDef::new("description", "Description", FieldType::Textarea),
            ],
        }
    }
    async fn execute(&self, config: Value, ctx: &ExecutionContext, n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let mut body = json!({
            "title":    field_str(&config, "title")?,
            "start_at": field_str(&config, "start_at")?,
            "end_at":   field_str(&config, "end_at")?,
            "description": config.get("description").cloned().unwrap_or(Value::Null),
        });
        if let Some(cal) = config.get("calendar_id").filter(|v| !v.is_null()) {
            body["calendar_id"] = cal.clone();
        }
        let resp = n.proxy.call_module("calendar", "/events", Method::POST, Some(body), n.user_id, Some(&ctx.idempotency_key()))
            .await.map_err(proxy_err)?;
        Ok(ok_output(resp))
    }
}

// ── Forms : récupérer les réponses ───────────────────────────────────────────────

pub struct FormResponsesNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for FormResponsesNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "kubuno.forms.responses".into(), name: "Forms — Réponses".into(),
            description: "Récupère les réponses d'un formulaire".into(),
            category: NodeCategory::Kubuno, icon: "ClipboardList".into(), color: "#1a73e8".into(),
            inputs: 1, outputs: vec![],
            fields: vec![ FieldDef::new("form_id", "Formulaire", FieldType::Expression).required() ],
        }
    }
    async fn execute(&self, config: Value, _ctx: &ExecutionContext, n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let form_id = field_str(&config, "form_id")?;
        let path = format!("/forms/{form_id}/responses");
        let resp = n.proxy.call_module("forms", &path, Method::GET, None, n.user_id, None)
            .await.map_err(proxy_err)?;
        Ok(ok_output(resp))
    }
}

// ── Files : lister un dossier ────────────────────────────────────────────────────

pub struct ListFilesNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for ListFilesNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "kubuno.drive.list".into(), name: "Drive — Lister".into(),
            description: "Liste les fichiers (racine ou dossier)".into(),
            category: NodeCategory::Kubuno, icon: "Folder".into(), color: "#1a73e8".into(),
            inputs: 1, outputs: vec![],
            fields: vec![ FieldDef::new("folder_id", "Dossier (optionnel)", FieldType::Expression) ],
        }
    }
    async fn execute(&self, config: Value, _ctx: &ExecutionContext, n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let path = match config.get("folder_id").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            Some(id) => format!("/folders/{id}"),
            None => "/".to_string(),
        };
        let resp = n.proxy.call_module("drive", &path, Method::GET, None, n.user_id, None)
            .await.map_err(proxy_err)?;
        Ok(ok_output(resp))
    }
}

// ── Notification : centre de notifications Kubuno (via événement) ─────────────────

pub struct NotificationNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for NotificationNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "kubuno.notification".into(), name: "Notification".into(),
            description: "Envoie une notification à l'utilisateur".into(),
            category: NodeCategory::Kubuno, icon: "Bell".into(), color: "#1a73e8".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::new("title", "Titre", FieldType::Expression).required(),
                FieldDef::new("message", "Message", FieldType::Textarea),
            ],
        }
    }
    async fn execute(&self, config: Value, _ctx: &ExecutionContext, n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let event = json!({
            "type": "Custom",
            "payload": {
                "event_type": "Notification",
                "module_id":  "flow",
                "payload": {
                    "user_id": n.user_id,
                    "title":   field_str(&config, "title")?,
                    "message": config.get("message").cloned().unwrap_or(Value::Null),
                }
            }
        });
        n.proxy.publish_event(&event).await.map_err(proxy_err)?;
        Ok(NodeOutput::data(json!({ "sent": true })))
    }
}
