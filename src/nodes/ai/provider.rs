//! AI Agent runtime: builds the LLM request from the agent's sub-nodes (model,
//! memory, tools, parser), runs the tool-calling loop, parses the output and
//! persists conversation memory. Supports Anthropic + OpenAI-compatible function
//! calling; Gemini is text-only.

use std::collections::HashMap;

use reqwest::Method;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::models::workflow::WorkflowDefinition;
use crate::nodes::trait_::{ExecutionContext, NodeContext, NodeError, NodeOutput};
use crate::runtime::executor::run_workflow_inline;

struct ModelCfg {
    provider:    String,
    model:       String,
    api_key:     String,
    base_url:    Option<String>,
    temperature: f64,
    max_tokens:  u64,
}

#[derive(Clone)]
struct ToolCall { id: String, name: String, args: Value }

enum Turn {
    User(String),
    Assistant { text: String, calls: Vec<ToolCall> },
    ToolResult { id: String, content: String },
}

enum Reply { Text(String), Calls(Vec<ToolCall>) }

fn sub_first<'a>(config: &'a Value, port: &str) -> Option<&'a Value> {
    config.get("__sub").and_then(|s| s.get(port)).and_then(|v| v.as_array()).and_then(|a| a.first())
}
fn sub_all<'a>(config: &'a Value, port: &str) -> Vec<&'a Value> {
    config.get("__sub").and_then(|s| s.get(port)).and_then(|v| v.as_array()).map(|a| a.iter().collect()).unwrap_or_default()
}

fn parse_model(item: &Value) -> Option<ModelCfg> {
    let type_id = item.get("type").and_then(|v| v.as_str())?;
    let provider = type_id.strip_prefix("ai.model.").unwrap_or("anthropic").to_string();
    let c = item.get("config").cloned().unwrap_or(json!({}));
    let api_key = c.get("credential").and_then(|cr| cr.get("apiKey")).and_then(|v| v.as_str())
        .or_else(|| c.get("api_key").and_then(|v| v.as_str()))
        .unwrap_or("").to_string();
    Some(ModelCfg {
        provider,
        model: c.get("model").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        api_key,
        base_url: c.get("base_url").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(str::to_string),
        temperature: c.get("temperature").and_then(|v| v.as_f64()).unwrap_or(0.7),
        max_tokens: c.get("max_tokens").and_then(|v| v.as_u64()).unwrap_or(1024),
    })
}

pub async fn run_agent(config: Value, ctx: &ExecutionContext, n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
    // 1) Modèle (requis).
    let model = sub_first(&config, "ai_languageModel").and_then(parse_model)
        .ok_or_else(|| NodeError::InvalidConfig("Un nœud « Modèle » doit être branché sur l'agent".into()))?;
    if model.api_key.is_empty() && model.provider != "openai_compat" {
        return Err(NodeError::MissingField("api_key (credential du modèle)"));
    }

    // 2) Outils.
    let tool_items = sub_all(&config, "ai_tool");
    let mut tools_by_name: HashMap<String, Value> = HashMap::new();
    let mut tool_specs: Vec<Value> = Vec::new();
    for (i, t) in tool_items.iter().enumerate() {
        let tc = t.get("config").cloned().unwrap_or(json!({}));
        let name = tc.get("name").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(str::to_string).unwrap_or_else(|| format!("tool_{i}"));
        let desc = tc.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let schema = tc.get("schema").cloned().unwrap_or_else(|| json!({
            "type": "object",
            "properties": { "input": { "type": "string", "description": "Argument d'entrée de l'outil" } },
            "required": ["input"]
        }));
        tool_specs.push(json!({ "name": name, "description": desc, "schema": schema }));
        tools_by_name.insert(name, json!({ "type": t.get("type"), "config": tc }));
    }

    // 3) Parser de sortie (optionnel).
    let parser_schema = sub_first(&config, "ai_outputParser")
        .and_then(|p| p.get("config")).and_then(|c| c.get("schema")).cloned();

    // 4) Système + invite.
    let mut system = config.get("system").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if let Some(sc) = &parser_schema {
        system.push_str(&format!("\n\nRéponds UNIQUEMENT avec un JSON valide correspondant à ce schéma (sans texte autour) : {sc}"));
    }
    let prompt = match config.get("prompt") {
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        _ => match &ctx.input {
            Value::String(s) => s.clone(),
            other => other.get("text").and_then(|v| v.as_str()).map(str::to_string).unwrap_or_else(|| other.to_string()),
        },
    };
    let max_iter = config.get("max_iterations").and_then(|v| v.as_u64()).unwrap_or(5).clamp(1, 12);

    // 5) Mémoire (optionnelle) : charger l'historique.
    let mem = sub_first(&config, "ai_memory").map(|m| m.get("config").cloned().unwrap_or(json!({})));
    let session = mem.as_ref().and_then(|c| c.get("session_key")).and_then(|v| v.as_str()).filter(|s| !s.is_empty()).unwrap_or("default").to_string();
    let window = mem.as_ref().and_then(|c| c.get("window")).and_then(|v| v.as_u64()).unwrap_or(10);

    let mut turns: Vec<Turn> = Vec::new();
    if mem.is_some() {
        for (role, content) in load_memory(n, ctx.workflow_id, &session).await {
            match role.as_str() {
                "assistant" => turns.push(Turn::Assistant { text: content, calls: vec![] }),
                _ => turns.push(Turn::User(content)),
            }
        }
    }
    turns.push(Turn::User(prompt.clone()));

    // 6) Boucle agent (modèle → outils → modèle …).
    let mut final_text = String::new();
    for _ in 0..max_iter {
        match call_llm(n, &model, &system, &turns, &tool_specs).await? {
            Reply::Text(t) => { final_text = t; break; }
            Reply::Calls(calls) => {
                turns.push(Turn::Assistant { text: String::new(), calls: calls.clone() });
                for c in calls {
                    let result = match tools_by_name.get(&c.name) {
                        Some(t) => run_tool(n, ctx, t, &c.args).await.unwrap_or_else(|e| format!("Erreur outil : {e}")),
                        None => format!("Outil inconnu : {}", c.name),
                    };
                    turns.push(Turn::ToolResult { id: c.id, content: result });
                }
            }
        }
    }
    if final_text.is_empty() {
        final_text = "(agent : nombre maximal d'itérations atteint)".into();
    }

    // 7) Mémoire : sauvegarde.
    if mem.is_some() {
        save_memory(n, ctx.workflow_id, &session, &prompt, &final_text, window).await;
    }

    // 8) Analyse de sortie.
    let output = match &parser_schema {
        Some(_) => extract_json(&final_text).unwrap_or_else(|| json!({ "text": final_text, "_parse_error": true })),
        None => json!(final_text),
    };
    Ok(NodeOutput::data(json!({ "output": output, "text": final_text })))
}

// ── Appel LLM (dispatch fournisseur) ─────────────────────────────────────────────

async fn call_llm(n: &NodeContext<'_>, m: &ModelCfg, system: &str, turns: &[Turn], tools: &[Value]) -> Result<Reply, NodeError> {
    match m.provider.as_str() {
        "anthropic" => call_anthropic(n, m, system, turns, tools).await,
        "gemini"    => call_gemini(n, m, system, turns).await,
        _           => call_openai(n, m, system, turns, tools).await, // openai / mistral / openai_compat
    }
}

fn err(e: impl std::fmt::Display) -> NodeError { NodeError::ProxyError(e.to_string()) }

async fn call_anthropic(n: &NodeContext<'_>, m: &ModelCfg, system: &str, turns: &[Turn], tools: &[Value]) -> Result<Reply, NodeError> {
    let mut messages = Vec::new();
    for t in turns {
        match t {
            Turn::User(s) => messages.push(json!({ "role": "user", "content": s })),
            Turn::Assistant { text, calls } => {
                let mut content = Vec::new();
                if !text.is_empty() { content.push(json!({ "type": "text", "text": text })); }
                for c in calls { content.push(json!({ "type": "tool_use", "id": c.id, "name": c.name, "input": c.args })); }
                messages.push(json!({ "role": "assistant", "content": content }));
            }
            Turn::ToolResult { id, content, .. } =>
                messages.push(json!({ "role": "user", "content": [{ "type": "tool_result", "tool_use_id": id, "content": content }] })),
        }
    }
    let mut body = json!({ "model": m.model, "max_tokens": m.max_tokens, "temperature": m.temperature, "messages": messages });
    if !system.is_empty() { body["system"] = json!(system); }
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools.iter().map(|t| json!({
            "name": t["name"], "description": t["description"], "input_schema": t["schema"]
        })).collect());
    }
    let headers = HashMap::from([
        ("x-api-key".to_string(), m.api_key.clone()),
        ("anthropic-version".to_string(), "2023-06-01".to_string()),
    ]);
    let resp = n.proxy.call_external("https://api.anthropic.com/v1/messages", Method::POST, headers, Some(body), 120, n.user_id).await.map_err(err)?;

    let blocks = resp.body.get("content").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let calls: Vec<ToolCall> = blocks.iter().filter(|b| b.get("type").and_then(|v| v.as_str()) == Some("tool_use"))
        .map(|b| ToolCall {
            id: b.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            name: b.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            args: b.get("input").cloned().unwrap_or(json!({})),
        }).collect();
    if !calls.is_empty() { return Ok(Reply::Calls(calls)); }
    let text = blocks.iter().filter_map(|b| b.get("text").and_then(|v| v.as_str())).collect::<Vec<_>>().join("");
    Ok(Reply::Text(text))
}

async fn call_openai(n: &NodeContext<'_>, m: &ModelCfg, system: &str, turns: &[Turn], tools: &[Value]) -> Result<Reply, NodeError> {
    let url = match m.provider.as_str() {
        "mistral" => "https://api.mistral.ai/v1/chat/completions".to_string(),
        "openai_compat" => m.base_url.clone().ok_or(NodeError::MissingField("base_url"))?,
        _ => "https://api.openai.com/v1/chat/completions".to_string(),
    };
    let mut messages = Vec::new();
    if !system.is_empty() { messages.push(json!({ "role": "system", "content": system })); }
    for t in turns {
        match t {
            Turn::User(s) => messages.push(json!({ "role": "user", "content": s })),
            Turn::Assistant { text, calls } => {
                let mut msg = json!({ "role": "assistant", "content": if text.is_empty() { Value::Null } else { json!(text) } });
                if !calls.is_empty() {
                    msg["tool_calls"] = Value::Array(calls.iter().map(|c| json!({
                        "id": c.id, "type": "function",
                        "function": { "name": c.name, "arguments": c.args.to_string() }
                    })).collect());
                }
                messages.push(msg);
            }
            Turn::ToolResult { id, content, .. } =>
                messages.push(json!({ "role": "tool", "tool_call_id": id, "content": content })),
        }
    }
    let mut body = json!({ "model": m.model, "messages": messages, "temperature": m.temperature, "max_tokens": m.max_tokens });
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools.iter().map(|t| json!({
            "type": "function",
            "function": { "name": t["name"], "description": t["description"], "parameters": t["schema"] }
        })).collect());
    }
    let mut headers = HashMap::new();
    if !m.api_key.is_empty() { headers.insert("Authorization".to_string(), format!("Bearer {}", m.api_key)); }
    let resp = n.proxy.call_external(&url, Method::POST, headers, Some(body), 120, n.user_id).await.map_err(err)?;

    let msg = resp.body.pointer("/choices/0/message").cloned().unwrap_or(json!({}));
    if let Some(tcs) = msg.get("tool_calls").and_then(|v| v.as_array()) {
        let calls = tcs.iter().map(|tc| ToolCall {
            id: tc.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            name: tc.pointer("/function/name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            args: tc.pointer("/function/arguments").and_then(|v| v.as_str()).and_then(|s| serde_json::from_str(s).ok()).unwrap_or(json!({})),
        }).collect::<Vec<_>>();
        if !calls.is_empty() { return Ok(Reply::Calls(calls)); }
    }
    Ok(Reply::Text(msg.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string()))
}

async fn call_gemini(n: &NodeContext<'_>, m: &ModelCfg, system: &str, turns: &[Turn]) -> Result<Reply, NodeError> {
    // Texte uniquement (pas d'appel d'outils en v1).
    let mut contents = Vec::new();
    if !system.is_empty() { contents.push(json!({ "role": "user", "parts": [{ "text": format!("[Système] {system}") }] })); }
    for t in turns {
        match t {
            Turn::User(s) => contents.push(json!({ "role": "user", "parts": [{ "text": s }] })),
            Turn::Assistant { text, .. } => contents.push(json!({ "role": "model", "parts": [{ "text": text }] })),
            Turn::ToolResult { content, .. } => contents.push(json!({ "role": "user", "parts": [{ "text": content }] })),
        }
    }
    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}", m.model, m.api_key);
    let body = json!({ "contents": contents });
    let resp = n.proxy.call_external(&url, Method::POST, HashMap::new(), Some(body), 120, n.user_id).await.map_err(err)?;
    let text = resp.body.pointer("/candidates/0/content/parts/0/text").and_then(|v| v.as_str()).unwrap_or("").to_string();
    Ok(Reply::Text(text))
}

// ── Exécution d'un outil ─────────────────────────────────────────────────────────

async fn run_tool(n: &NodeContext<'_>, ctx: &ExecutionContext, tool: &Value, args: &Value) -> Result<String, String> {
    let type_id = tool.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let cfg = tool.get("config").cloned().unwrap_or(json!({}));
    match type_id {
        "ai.tool.http" => {
            let input_str = args.get("input").and_then(|v| v.as_str()).map(str::to_string).unwrap_or_else(|| args.to_string());
            let url = cfg.get("url").and_then(|v| v.as_str()).unwrap_or("").replace("{{input}}", &input_str);
            let method = cfg.get("method").and_then(|v| v.as_str()).unwrap_or("GET").parse::<Method>().unwrap_or(Method::GET);
            let body = if method == Method::GET { None } else { Some(args.clone()) };
            let resp = n.proxy.call_external(&url, method, HashMap::new(), body, 60, n.user_id).await.map_err(|e| e.to_string())?;
            Ok(match resp.body { Value::String(s) => s, other => other.to_string() })
        }
        "ai.tool.workflow" => {
            let wf = cfg.get("workflow_id").and_then(|v| v.as_str()).ok_or("workflow_id manquant")?;
            let wf_id = Uuid::parse_str(wf.trim()).map_err(|_| "workflow_id invalide".to_string())?;
            let file_id: Option<Uuid> = sqlx::query_scalar(
                "SELECT file_id FROM flow.workflows WHERE id = $1 AND owner_id = $2 AND is_trashed = FALSE",
            ).bind(wf_id).bind(n.user_id).fetch_optional(n.db).await.map_err(|e| e.to_string())?
             .ok_or("workflow introuvable")?;
            let def_val = match file_id {
                Some(fid) => {
                    let (_i, raw) = n.files_client.get_file_content(n.user_id, fid).await.map_err(|e| e.to_string())?;
                    crate::services::content_files::parse_definition_bytes(&raw).map_err(|e| e.to_string())?
                }
                None => json!({ "nodes": [], "edges": [] }),
            };
            let def = WorkflowDefinition::from_value(&def_val);
            let out = run_workflow_inline(n, n.user_id, wf_id, &def, args.clone()).await?;
            let _ = ctx;
            Ok(out.to_string())
        }
        _ => Err(format!("type d'outil non géré : {type_id}")),
    }
}

// ── Mémoire ──────────────────────────────────────────────────────────────────────

async fn load_memory(n: &NodeContext<'_>, wf: Uuid, session: &str) -> Vec<(String, String)> {
    let row: Option<Value> = sqlx::query_scalar("SELECT messages FROM flow.ai_memory WHERE workflow_id = $1 AND session_key = $2")
        .bind(wf).bind(session).fetch_optional(n.db).await.ok().flatten();
    row.and_then(|v| v.as_array().map(|a| a.iter().filter_map(|m| {
        Some((m.get("role")?.as_str()?.to_string(), m.get("content")?.as_str()?.to_string()))
    }).collect())).unwrap_or_default()
}

async fn save_memory(n: &NodeContext<'_>, wf: Uuid, session: &str, user: &str, assistant: &str, window: u64) {
    let mut msgs = load_memory(n, wf, session).await;
    msgs.push(("user".into(), user.to_string()));
    msgs.push(("assistant".into(), assistant.to_string()));
    let max = (window as usize) * 2;
    if msgs.len() > max { msgs = msgs.split_off(msgs.len() - max); }
    let json_msgs = Value::Array(msgs.into_iter().map(|(r, c)| json!({ "role": r, "content": c })).collect());
    let _ = sqlx::query(
        r#"INSERT INTO flow.ai_memory (workflow_id, session_key, messages, updated_at) VALUES ($1,$2,$3,NOW())
           ON CONFLICT (workflow_id, session_key) DO UPDATE SET messages = $3, updated_at = NOW()"#,
    ).bind(wf).bind(session).bind(&json_msgs).execute(n.db).await;
}

/// Extrait le premier objet JSON d'un texte (le modèle peut entourer de prose).
fn extract_json(text: &str) -> Option<Value> {
    if let Ok(v) = serde_json::from_str::<Value>(text.trim()) { return Some(v); }
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end > start { serde_json::from_str(&text[start..=end]).ok() } else { None }
}
