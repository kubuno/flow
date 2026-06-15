//! Nœuds externes (internet). Appels effectués via `CoreProxy.call_external`.
//! Le système de credentials externes chiffrés (GitHub/Slack/…) est différé ;
//! pour l'instant le nœud HTTP générique couvre la majorité des cas.

use async_trait::async_trait;
use reqwest::Method;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::nodes::trait_::{
    ExecutionContext, FieldDef, FieldType, NodeCategory, NodeContext, NodeError, NodeMeta, NodeOutput,
};

pub struct HttpRequestNode;

#[async_trait]
impl crate::nodes::trait_::NodeExecutor for HttpRequestNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "external.http_request".into(), name: "Requête HTTP".into(),
            description: "Appelle n'importe quelle API REST".into(),
            category: NodeCategory::External, icon: "Globe".into(), color: "#24292f".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::new("url", "URL", FieldType::Expression).required().placeholder("https://api.exemple.com/v1/…"),
                FieldDef::new("method", "Méthode", FieldType::Select).required().options(&[
                    ("GET","GET"),("POST","POST"),("PUT","PUT"),("PATCH","PATCH"),("DELETE","DELETE"),
                ]).default(json!("GET")),
                FieldDef::new("headers", "En-têtes (JSON)", FieldType::Json),
                FieldDef::new("body", "Corps (JSON)", FieldType::Json),
                FieldDef::new("timeout_secs", "Timeout (s)", FieldType::Number).default(json!(30)),
            ],
        }
    }

    async fn execute(&self, config: Value, _ctx: &ExecutionContext, n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let url = config.get("url").and_then(|v| v.as_str()).ok_or(NodeError::MissingField("url"))?;
        let method_str = config.get("method").and_then(|v| v.as_str()).unwrap_or("GET");
        let method = method_str.parse::<Method>().map_err(|_| NodeError::InvalidConfig("Méthode HTTP invalide".into()))?;
        let timeout = config.get("timeout_secs").and_then(|v| v.as_u64()).unwrap_or(30) as u32;

        let headers: HashMap<String, String> = config.get("headers")
            .and_then(|h| serde_json::from_value(h.clone()).ok())
            .unwrap_or_default();

        let body = config.get("body").filter(|v| !v.is_null()).cloned();

        let resp = n.proxy.call_external(url, method, headers, body, timeout, n.user_id)
            .await.map_err(|e| NodeError::ProxyError(e.to_string()))?;

        Ok(NodeOutput::data(json!({
            "status":  resp.status,
            "headers": resp.headers,
            "body":    resp.body,
        })))
    }
}
