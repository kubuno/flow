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

// ── AI / LLM ─────────────────────────────────────────────────────────────────────

/// Generic LLM node. Supports the Anthropic Messages API and any
/// OpenAI-compatible chat-completions endpoint (incl. self-hosted models).
/// Calls go through `CoreProxy.call_external` like any other outbound request.
pub struct AiNode;

#[async_trait]
impl crate::nodes::trait_::NodeExecutor for AiNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "external.ai".into(), name: "IA — Génération de texte".into(),
            description: "Interroge un modèle de langage (Anthropic ou API compatible OpenAI)".into(),
            category: NodeCategory::External, icon: "Sparkles".into(), color: "#8e44ad".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::new("provider", "Fournisseur", FieldType::Select).required().options(&[
                    ("anthropic","Anthropic (Claude)"),("openai","Compatible OpenAI"),
                ]).default(json!("anthropic")),
                FieldDef::new("base_url", "URL de base (compatible OpenAI)", FieldType::Text)
                    .placeholder("https://api.openai.com/v1/chat/completions")
                    .help("Utilisé seulement pour le mode compatible OpenAI (ex. modèle auto-hébergé)."),
                FieldDef::new("model", "Modèle", FieldType::Text).required().default(json!("claude-haiku-4-5-20251001")),
                FieldDef::credential("credential", "Credential (clé API)", "anthropicApi,openAiApi,azureOpenAiApi,mistralApi,googleGeminiApi,groqApi,perplexityApi"),
                FieldDef::new("api_key", "Clé API (ou via credential)", FieldType::Expression).placeholder("{{ $vars.apiKey }}"),
                FieldDef::new("system", "Instruction système", FieldType::Textarea),
                FieldDef::new("prompt", "Message", FieldType::Textarea).required().placeholder("Résume : {{ $json.text }}"),
                FieldDef::new("max_tokens", "Jetons max", FieldType::Number).default(json!(1024)),
                FieldDef::new("temperature", "Température", FieldType::Number).default(json!(0.7)),
            ],
        }
    }

    async fn execute(&self, config: Value, _ctx: &ExecutionContext, n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let provider = config.get("provider").and_then(|v| v.as_str()).unwrap_or("anthropic");
        let model = config.get("model").and_then(|v| v.as_str()).ok_or(NodeError::MissingField("model"))?;
        // Clé API : depuis le credential déchiffré si présent, sinon le champ libre.
        let cred_key = config.get("credential").and_then(|c| c.get("apiKey")).and_then(|v| v.as_str());
        let api_key = cred_key
            .or_else(|| config.get("api_key").and_then(|v| v.as_str()))
            .filter(|s| !s.is_empty())
            .ok_or(NodeError::MissingField("api_key"))?;
        let prompt = config.get("prompt").and_then(|v| v.as_str()).ok_or(NodeError::MissingField("prompt"))?;
        let system = config.get("system").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
        let max_tokens = config.get("max_tokens").and_then(|v| v.as_u64()).unwrap_or(1024);
        let temperature = config.get("temperature").and_then(|v| v.as_f64()).unwrap_or(0.7);

        let mut headers: HashMap<String, String> = HashMap::new();
        let (url, body) = match provider {
            "openai" => {
                let url = config.get("base_url").and_then(|v| v.as_str()).filter(|s| !s.is_empty())
                    .unwrap_or("https://api.openai.com/v1/chat/completions").to_string();
                headers.insert("Authorization".into(), format!("Bearer {api_key}"));
                let mut messages = Vec::new();
                if let Some(s) = system { messages.push(json!({ "role": "system", "content": s })); }
                messages.push(json!({ "role": "user", "content": prompt }));
                (url, json!({ "model": model, "messages": messages, "max_tokens": max_tokens, "temperature": temperature }))
            }
            _ /* anthropic */ => {
                headers.insert("x-api-key".into(), api_key.to_string());
                headers.insert("anthropic-version".into(), "2023-06-01".to_string());
                let mut b = json!({
                    "model": model, "max_tokens": max_tokens, "temperature": temperature,
                    "messages": [{ "role": "user", "content": prompt }],
                });
                if let Some(s) = system { b["system"] = json!(s); }
                ("https://api.anthropic.com/v1/messages".to_string(), b)
            }
        };

        let resp = n.proxy.call_external(&url, Method::POST, headers, Some(body), 120, n.user_id)
            .await.map_err(|e| NodeError::ProxyError(e.to_string()))?;

        // Extract the assistant text from either response shape.
        let text = if provider == "openai" {
            resp.body.pointer("/choices/0/message/content").and_then(|v| v.as_str()).unwrap_or("").to_string()
        } else {
            resp.body.pointer("/content/0/text").and_then(|v| v.as_str()).unwrap_or("").to_string()
        };

        Ok(NodeOutput::data(json!({ "text": text, "model": model, "raw": resp.body })))
    }
}

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
                FieldDef::credential("credential", "Authentification", "httpBasicAuth,httpHeaderAuth,httpQueryAuth,httpBearerAuth,httpDigestAuth,apiKeyAuth"),
                FieldDef::new("headers", "En-têtes (JSON)", FieldType::Json),
                FieldDef::new("body", "Corps (JSON)", FieldType::Json),
                FieldDef::new("timeout_secs", "Timeout (s)", FieldType::Number).default(json!(30)),
            ],
        }
    }

    async fn execute(&self, config: Value, _ctx: &ExecutionContext, n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        use base64::Engine;
        let mut url = config.get("url").and_then(|v| v.as_str()).ok_or(NodeError::MissingField("url"))?.to_string();
        let method_str = config.get("method").and_then(|v| v.as_str()).unwrap_or("GET");
        let method = method_str.parse::<Method>().map_err(|_| NodeError::InvalidConfig("Méthode HTTP invalide".into()))?;
        let timeout = config.get("timeout_secs").and_then(|v| v.as_u64()).unwrap_or(30) as u32;

        let mut headers: HashMap<String, String> = config.get("headers")
            .and_then(|h| serde_json::from_value(h.clone()).ok())
            .unwrap_or_default();

        // Authentification via credential réutilisable (déjà déchiffré par l'executor).
        if let Some(cred) = config.get("credential").filter(|v| v.is_object()) {
            let s = |k: &str| cred.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
            match cred.get("_type").and_then(|v| v.as_str()).unwrap_or("") {
                "httpBasicAuth" | "httpDigestAuth" => {
                    let b64 = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", s("user"), s("password")));
                    headers.insert("Authorization".into(), format!("Basic {b64}"));
                }
                "httpHeaderAuth" => { headers.insert(s("name"), s("value")); }
                "httpBearerAuth" => { headers.insert("Authorization".into(), format!("Bearer {}", s("token"))); }
                "apiKeyAuth" => { headers.insert("Authorization".into(), format!("Bearer {}", s("apiKey"))); }
                "httpQueryAuth" => {
                    let sep = if url.contains('?') { '&' } else { '?' };
                    url = format!("{url}{sep}{}={}", s("name"), s("value"));
                }
                _ => {}
            }
        }

        let body = config.get("body").filter(|v| !v.is_null()).cloned();

        let resp = n.proxy.call_external(&url, method, headers, body, timeout, n.user_id)
            .await.map_err(|e| NodeError::ProxyError(e.to_string()))?;

        Ok(NodeOutput::data(json!({
            "status":  resp.status,
            "headers": resp.headers,
            "body":    resp.body,
        })))
    }
}
