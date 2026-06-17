//! Registre central de tous les nœuds disponibles.

use std::collections::HashMap;
use std::sync::Arc;

use super::trait_::{NodeExecutor, NodeMeta};

pub struct NodeRegistry {
    nodes: HashMap<String, Arc<dyn NodeExecutor>>,
}

impl NodeRegistry {
    pub fn get(&self, node_type: &str) -> Option<Arc<dyn NodeExecutor>> {
        self.nodes.get(node_type).cloned()
    }

    /// Catalogue exposé au frontend : métadonnées enrichies des champs IA
    /// (`aiOutput` pour les sous-nœuds fournisseurs, `subInputs` pour les agents).
    pub fn catalog(&self) -> Vec<serde_json::Value> {
        let mut out: Vec<serde_json::Value> = self.nodes.values().map(|n| {
            let mut v = serde_json::to_value(n.meta()).unwrap_or_else(|_| serde_json::json!({}));
            if let Some(obj) = v.as_object_mut() {
                if let Some(ao) = n.ai_output() {
                    obj.insert("aiOutput".into(), serde_json::Value::String(ao));
                }
                let subs = n.sub_inputs();
                if !subs.is_empty() {
                    obj.insert("subInputs".into(), serde_json::to_value(subs).unwrap_or(serde_json::Value::Null));
                }
            }
            v
        }).collect();
        out.sort_by(|a, b| a.get("type").and_then(|v| v.as_str()).cmp(&b.get("type").and_then(|v| v.as_str())));
        out
    }

    pub fn meta(&self, node_type: &str) -> Option<NodeMeta> {
        self.nodes.get(node_type).map(|n| n.meta())
    }

    /// Type d'`ai_output` d'un nœud (sous-nœud fournisseur), s'il y en a un.
    pub fn ai_output(&self, node_type: &str) -> Option<String> {
        self.nodes.get(node_type).and_then(|n| n.ai_output())
    }

    /// Ports de sous-entrée d'un nœud (agent IA).
    pub fn sub_inputs(&self, node_type: &str) -> Vec<super::trait_::SubInput> {
        self.nodes.get(node_type).map(|n| n.sub_inputs()).unwrap_or_default()
    }
}

/// Construit le registre par défaut avec tous les nœuds intégrés.
pub fn build_registry() -> NodeRegistry {
    use super::ai::{
        AiAgent, AnthropicModel, GeminiModel, HttpTool, MistralModel, OpenAiCompatModel, OpenAiModel,
        StructuredParser, WindowMemory, WorkflowTool,
    };
    use super::code::js_node::CodeNode;
    use super::db::{MongoNode, MySqlQueryNode, PostgresInsertNode, PostgresQueryNode, RedisNode, SupabaseNode};
    use super::external::{AiNode, HttpRequestNode};
    use super::flow::{LoopItemsNode, SubWorkflowNode};
    use super::kubuno::*;
    use super::logic::*;
    use super::triggers::{
        chat::ChatTrigger, cron::CronTrigger, email::EmailTrigger, error::ErrorTrigger,
        execute_workflow::ExecuteWorkflowTrigger, form::FormTrigger, kubuno_event::KubunoEventTrigger,
        manual::ManualTrigger, mcp::McpTrigger, sse::SseTrigger, webhook::WebhookTrigger,
    };

    let mut nodes: HashMap<String, Arc<dyn NodeExecutor>> = HashMap::new();

    macro_rules! add {
        ($n:expr) => {{
            let node: Arc<dyn NodeExecutor> = Arc::new($n);
            nodes.insert(node.meta().node_type.clone(), node);
        }};
    }

    // Triggers
    add!(ManualTrigger);
    add!(WebhookTrigger);
    add!(CronTrigger);
    add!(KubunoEventTrigger);
    add!(FormTrigger);
    add!(ChatTrigger);
    add!(ErrorTrigger);
    add!(ExecuteWorkflowTrigger);
    add!(EmailTrigger);
    add!(SseTrigger);
    add!(McpTrigger);

    // Kubuno
    add!(SendMailNode);
    add!(CreateContactNode);
    add!(SendChatNode);
    add!(CreateEventNode);
    add!(FormResponsesNode);
    add!(ListFilesNode);
    add!(NotificationNode);
    add!(CreateTaskNode);

    // Logic
    add!(IfNode);
    add!(SwitchNode);
    add!(FilterNode);
    add!(TransformNode);
    add!(SetVariableNode);
    add!(TemplateNode);
    add!(MergeNode);
    add!(SplitNode);
    add!(WaitNode);
    add!(CalculateNode);
    add!(JsonNode);
    add!(AggregateNode);
    add!(ErrorHandlerNode);
    add!(StopNode);
    add!(DateTimeNode);
    add!(SortNode);
    add!(LimitNode);
    add!(RemoveDuplicatesNode);
    add!(RenameKeysNode);
    add!(EditFieldsNode);
    add!(CryptoNode);
    add!(RandomNode);

    // Flow (sub-workflows & boucle)
    add!(SubWorkflowNode);
    add!(LoopItemsNode);

    // External
    add!(HttpRequestNode);
    add!(AiNode);

    // Base de données (SGBD externe)
    add!(PostgresQueryNode);
    add!(PostgresInsertNode);
    add!(MySqlQueryNode);
    add!(RedisNode);
    add!(MongoNode);
    add!(SupabaseNode);

    // Code
    add!(CodeNode);

    // IA — agent + sous-nœuds (modèles, mémoire, outils, parser)
    add!(AiAgent);
    add!(AnthropicModel);
    add!(OpenAiModel);
    add!(GeminiModel);
    add!(MistralModel);
    add!(OpenAiCompatModel);
    add!(WindowMemory);
    add!(HttpTool);
    add!(WorkflowTool);
    add!(StructuredParser);

    NodeRegistry { nodes }
}
