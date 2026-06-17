//! Supabase node — table operations through the Supabase REST (PostgREST) API.
//! Uses the outbound HTTP proxy; no DB driver. Credentials/URL are user-provided.

use async_trait::async_trait;
use reqwest::Method;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::nodes::trait_::{
    ExecutionContext, FieldDef, FieldType, NodeCategory, NodeContext, NodeError, NodeMeta, NodeOutput,
};

pub struct SupabaseNode;

#[async_trait]
impl crate::nodes::trait_::NodeExecutor for SupabaseNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "db.supabase".into(), name: "Supabase".into(),
            description: "Lit/écrit une table via l'API REST Supabase (PostgREST)".into(),
            category: NodeCategory::External, icon: "Database".into(), color: "#3ecf8e".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::credential("credential", "Credential Supabase", "supabase"),
                FieldDef::new("url", "URL du projet (si pas de credential)", FieldType::Expression)
                    .placeholder("https://xxxx.supabase.co"),
                FieldDef::new("api_key", "Clé (service role, si pas de credential)", FieldType::Expression),
                FieldDef::new("operation", "Opération", FieldType::Select).required().options(&[
                    ("select","Lire (SELECT)"),("insert","Insérer"),("update","Mettre à jour"),("delete","Supprimer"),
                ]).default(json!("select")),
                FieldDef::new("table", "Table", FieldType::Expression).required(),
                FieldDef::new("columns", "Colonnes (select)", FieldType::Text).placeholder("*").default(json!("*")),
                FieldDef::new("filters", "Filtres PostgREST", FieldType::Expression)
                    .placeholder("id=eq.5&statut=eq.actif")
                    .help("Syntaxe PostgREST, ex. colonne=eq.valeur (requis pour update/delete)."),
                FieldDef::new("data", "Données (JSON, insert/update)", FieldType::Json),
            ],
        }
    }

    async fn execute(&self, config: Value, ctx: &ExecutionContext, n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        // Base URL + key from credential or explicit fields.
        let cred = config.get("credential").filter(|v| v.is_object());
        let base_url = cred.and_then(|c| c.get("host")).and_then(|v| v.as_str())
            .or_else(|| config.get("url").and_then(|v| v.as_str()))
            .map(|s| s.trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty())
            .ok_or(NodeError::MissingField("url"))?;
        let api_key = cred.and_then(|c| c.get("serviceRole")).and_then(|v| v.as_str())
            .or_else(|| config.get("api_key").and_then(|v| v.as_str()))
            .filter(|s| !s.is_empty())
            .ok_or(NodeError::MissingField("api_key"))?;

        let op = config.get("operation").and_then(|v| v.as_str()).unwrap_or("select");
        let table = config.get("table").and_then(|v| v.as_str()).ok_or(NodeError::MissingField("table"))?;
        let filters = config.get("filters").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
        let columns = config.get("columns").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).unwrap_or("*");

        let mut headers: HashMap<String, String> = HashMap::new();
        headers.insert("apikey".into(), api_key.to_string());
        headers.insert("Authorization".into(), format!("Bearer {api_key}"));
        headers.insert("Content-Type".into(), "application/json".into());
        headers.insert("Prefer".into(), "return=representation".into());

        let mut url = format!("{base_url}/rest/v1/{table}");
        let (method, body): (Method, Option<Value>) = match op {
            "select" => {
                url.push_str(&format!("?select={columns}"));
                if let Some(f) = filters { url.push('&'); url.push_str(f); }
                (Method::GET, None)
            }
            "insert" => {
                let data = config.get("data").filter(|v| !v.is_null()).cloned().unwrap_or_else(|| ctx.input.clone());
                (Method::POST, Some(data))
            }
            "update" => {
                if let Some(f) = filters { url.push('?'); url.push_str(f); }
                let data = config.get("data").filter(|v| !v.is_null()).cloned().unwrap_or_else(|| ctx.input.clone());
                (Method::PATCH, Some(data))
            }
            "delete" => {
                if let Some(f) = filters { url.push('?'); url.push_str(f); }
                (Method::DELETE, None)
            }
            _ => return Err(NodeError::InvalidConfig("Opération inconnue".into())),
        };

        let resp = n.proxy.call_external(&url, method, headers, body, 30, n.user_id)
            .await.map_err(|e| NodeError::ProxyError(e.to_string()))?;
        let count = resp.body.as_array().map(|a| a.len()).unwrap_or(0);
        Ok(NodeOutput::data(json!({ "rows": resp.body, "count": count, "status": resp.status })))
    }
}
