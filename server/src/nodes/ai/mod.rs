//! AI Agent node and its sub-nodes (n8n-style). The agent has typed sub-input
//! ports — Model (required), Memory, Tools, Output Parser — fed by sub-nodes that
//! connect from below. Sub-nodes are pure providers (never run in the main flow);
//! the executor collects their config under `__sub` and the agent consumes it.

mod provider;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::nodes::trait_::{
    ExecutionContext, FieldDef, FieldType, NodeCategory, NodeContext, NodeError, NodeMeta, NodeOutput, SubInput,
};

// ── Helpers communs aux sous-nœuds (fournisseurs non exécutables) ────────────────

macro_rules! provider_node {
    ($name:ident, $out:expr) => {
        #[async_trait]
        impl crate::nodes::trait_::NodeExecutor for $name {
            fn meta(&self) -> NodeMeta { $name::meta_impl() }
            fn ai_output(&self) -> Option<String> { Some($out.to_string()) }
            async fn execute(&self, _c: Value, _e: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
                Err(NodeError::InvalidConfig("Ce sous-nœud se branche sur un agent IA (il ne s'exécute pas seul)".into()))
            }
        }
    };
}

fn model_fields(default_model: &str, cred_types: &str) -> Vec<FieldDef> {
    vec![
        FieldDef::credential("credential", "Credential (clé API)", cred_types),
        FieldDef::new("model", "Modèle", FieldType::Text).required().default(json!(default_model)),
        FieldDef::new("api_key", "Clé API (si pas de credential)", FieldType::Expression),
        FieldDef::new("temperature", "Température", FieldType::Number).default(json!(0.7)),
        FieldDef::new("max_tokens", "Jetons max", FieldType::Number).default(json!(1024)),
    ]
}

// ── Chat Models ──────────────────────────────────────────────────────────────────

pub struct AnthropicModel;
impl AnthropicModel { pub fn meta_impl() -> NodeMeta { NodeMeta {
    node_type: "ai.model.anthropic".into(), name: "Modèle Anthropic (Claude)".into(),
    description: "Modèle de chat Claude pour un agent IA".into(),
    category: NodeCategory::Ai, icon: "Sparkles".into(), color: "#d4a373".into(),
    inputs: 0, outputs: vec![], fields: model_fields("claude-haiku-4-5-20251001", "anthropicApi"),
}}}
provider_node!(AnthropicModel, "ai_languageModel");

pub struct OpenAiModel;
impl OpenAiModel { pub fn meta_impl() -> NodeMeta { NodeMeta {
    node_type: "ai.model.openai".into(), name: "Modèle OpenAI".into(),
    description: "Modèle de chat OpenAI pour un agent IA".into(),
    category: NodeCategory::Ai, icon: "Sparkles".into(), color: "#10a37f".into(),
    inputs: 0, outputs: vec![], fields: model_fields("gpt-4o-mini", "openAiApi"),
}}}
provider_node!(OpenAiModel, "ai_languageModel");

pub struct GeminiModel;
impl GeminiModel { pub fn meta_impl() -> NodeMeta { NodeMeta {
    node_type: "ai.model.gemini".into(), name: "Modèle Google Gemini".into(),
    description: "Modèle de chat Gemini pour un agent IA (sans appel d'outils)".into(),
    category: NodeCategory::Ai, icon: "Sparkles".into(), color: "#4285f4".into(),
    inputs: 0, outputs: vec![], fields: model_fields("gemini-1.5-flash", "googleGeminiApi"),
}}}
provider_node!(GeminiModel, "ai_languageModel");

pub struct MistralModel;
impl MistralModel { pub fn meta_impl() -> NodeMeta { NodeMeta {
    node_type: "ai.model.mistral".into(), name: "Modèle Mistral".into(),
    description: "Modèle de chat Mistral pour un agent IA".into(),
    category: NodeCategory::Ai, icon: "Sparkles".into(), color: "#ff7000".into(),
    inputs: 0, outputs: vec![], fields: model_fields("mistral-small-latest", "mistralApi"),
}}}
provider_node!(MistralModel, "ai_languageModel");

pub struct OpenAiCompatModel;
impl OpenAiCompatModel { pub fn meta_impl() -> NodeMeta {
    let mut f = model_fields("", "");
    f.insert(0, FieldDef::new("base_url", "URL de base", FieldType::Text).required().placeholder("https://…/v1/chat/completions"));
    NodeMeta {
        node_type: "ai.model.openai_compat".into(), name: "Modèle compatible OpenAI".into(),
        description: "N'importe quel endpoint compatible OpenAI (Groq, Ollama, auto-hébergé…)".into(),
        category: NodeCategory::Ai, icon: "Sparkles".into(), color: "#5f6368".into(),
        inputs: 0, outputs: vec![], fields: f,
    }
}}
provider_node!(OpenAiCompatModel, "ai_languageModel");

// ── Memory ─────────────────────────────────────────────────────────────────────

pub struct WindowMemory;
impl WindowMemory { pub fn meta_impl() -> NodeMeta { NodeMeta {
    node_type: "ai.memory.window".into(), name: "Mémoire (fenêtre)".into(),
    description: "Conserve les N derniers échanges de la conversation".into(),
    category: NodeCategory::Ai, icon: "Database".into(), color: "#9334e6".into(),
    inputs: 0, outputs: vec![],
    fields: vec![
        FieldDef::new("session_key", "Clé de session", FieldType::Expression).placeholder("{{ $json.userId }}").help("Vide = « default »."),
        FieldDef::new("window", "Échanges conservés", FieldType::Number).default(json!(10)),
    ],
}}}
provider_node!(WindowMemory, "ai_memory");

// ── Tools ──────────────────────────────────────────────────────────────────────

pub struct HttpTool;
impl HttpTool { pub fn meta_impl() -> NodeMeta { NodeMeta {
    node_type: "ai.tool.http".into(), name: "Outil — Requête HTTP".into(),
    description: "Donne à l'agent un outil qui appelle une API HTTP".into(),
    category: NodeCategory::Ai, icon: "Globe".into(), color: "#f9ab00".into(),
    inputs: 0, outputs: vec![],
    fields: vec![
        FieldDef::new("name", "Nom de l'outil", FieldType::Text).required().placeholder("get_weather"),
        FieldDef::new("description", "Description (pour l'IA)", FieldType::Textarea).required(),
        FieldDef::new("url", "URL", FieldType::Text).required().help("Peut contenir {{input}} (remplacé par l'argument de l'IA)."),
        FieldDef::new("method", "Méthode", FieldType::Select).options(&[("GET","GET"),("POST","POST"),("PUT","PUT"),("DELETE","DELETE")]).default(json!("GET")),
    ],
}}}
provider_node!(HttpTool, "ai_tool");

pub struct WorkflowTool;
impl WorkflowTool { pub fn meta_impl() -> NodeMeta { NodeMeta {
    node_type: "ai.tool.workflow".into(), name: "Outil — Workflow".into(),
    description: "Donne à l'agent un outil qui exécute un autre workflow".into(),
    category: NodeCategory::Ai, icon: "Workflow".into(), color: "#7e57c2".into(),
    inputs: 0, outputs: vec![],
    fields: vec![
        FieldDef::new("name", "Nom de l'outil", FieldType::Text).required(),
        FieldDef::new("description", "Description (pour l'IA)", FieldType::Textarea).required(),
        FieldDef::new("workflow_id", "Workflow à exécuter", FieldType::Expression).required(),
    ],
}}}
provider_node!(WorkflowTool, "ai_tool");

// ── Output parser ────────────────────────────────────────────────────────────────

pub struct StructuredParser;
impl StructuredParser { pub fn meta_impl() -> NodeMeta { NodeMeta {
    node_type: "ai.parser.structured".into(), name: "Sortie structurée (JSON)".into(),
    description: "Force l'agent à répondre en JSON selon un schéma".into(),
    category: NodeCategory::Ai, icon: "Braces".into(), color: "#1e8e3e".into(),
    inputs: 0, outputs: vec![],
    fields: vec![
        FieldDef::new("schema", "Schéma JSON (exemple)", FieldType::Json).required()
            .help(r#"Ex. {"nom":"texte","age":0} — décrit la forme attendue."#),
    ],
}}}
provider_node!(StructuredParser, "ai_outputParser");

// ── Agent IA ─────────────────────────────────────────────────────────────────────

pub struct AiAgent;

#[async_trait]
impl crate::nodes::trait_::NodeExecutor for AiAgent {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "ai.agent".into(), name: "Agent IA".into(),
            description: "Agent conversationnel : modèle + mémoire + outils + analyseur de sortie".into(),
            category: NodeCategory::Ai, icon: "Bot".into(), color: "#6750a4".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::new("prompt", "Message (entrée)", FieldType::Expression).default(json!("{{ $json.text }}")),
                FieldDef::new("system", "Instruction système", FieldType::Textarea)
                    .default(json!("Tu es un assistant utile.")),
                FieldDef::new("max_iterations", "Itérations max (boucle d'outils)", FieldType::Number).default(json!(5)),
            ],
        }
    }

    fn sub_inputs(&self) -> Vec<SubInput> {
        vec![
            SubInput::new("ai_languageModel", "Modèle", "ai_languageModel").required(),
            SubInput::new("ai_memory", "Mémoire", "ai_memory"),
            SubInput::new("ai_tool", "Outils", "ai_tool").multiple(),
            SubInput::new("ai_outputParser", "Analyseur de sortie", "ai_outputParser"),
        ]
    }

    async fn execute(&self, config: Value, ctx: &ExecutionContext, n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        provider::run_agent(config, ctx, n).await
    }
}
