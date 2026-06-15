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

    /// Métadonnées de tous les nœuds (catalogue exposé au frontend).
    pub fn catalog(&self) -> Vec<NodeMeta> {
        let mut metas: Vec<NodeMeta> = self.nodes.values().map(|n| n.meta()).collect();
        metas.sort_by(|a, b| a.node_type.cmp(&b.node_type));
        metas
    }

    pub fn meta(&self, node_type: &str) -> Option<NodeMeta> {
        self.nodes.get(node_type).map(|n| n.meta())
    }
}

/// Construit le registre par défaut avec tous les nœuds intégrés.
pub fn build_registry() -> NodeRegistry {
    use super::code::js_node::CodeNode;
    use super::external::HttpRequestNode;
    use super::kubuno::*;
    use super::logic::*;
    use super::triggers::{cron::CronTrigger, kubuno_event::KubunoEventTrigger, manual::ManualTrigger, webhook::WebhookTrigger};

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

    // Kubuno
    add!(SendMailNode);
    add!(CreateContactNode);
    add!(SendChatNode);
    add!(CreateEventNode);
    add!(FormResponsesNode);
    add!(ListFilesNode);
    add!(NotificationNode);

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

    // External
    add!(HttpRequestNode);

    // Code
    add!(CodeNode);

    NodeRegistry { nodes }
}
