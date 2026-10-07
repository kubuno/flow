//! Nœuds déclencheurs. À l'exécution, un trigger émet simplement les données du
//! déclenchement (`ctx.full["trigger"]`) vers les nœuds suivants. Leur rôle réel
//! (recevoir un webhook, matcher un cron, écouter un événement) est assuré par le
//! scheduler / les handlers, pas par cette méthode `execute`.

pub mod chat;
pub mod cron;
pub mod email;
pub mod error;
pub mod execute_workflow;
pub mod form;
pub mod kubuno_event;
pub mod manual;
pub mod mcp;
pub mod sse;
pub mod webhook;

use serde_json::Value;

use crate::nodes::trait_::{ExecutionContext, NodeError, NodeOutput};

async fn emit_trigger(exec_ctx: &ExecutionContext) -> Result<NodeOutput, NodeError> {
    let data = exec_ctx
        .full
        .get("trigger")
        .cloned()
        .unwrap_or(Value::Null);
    Ok(NodeOutput::data(data))
}

/// Marqueur réutilisable : tous les triggers partagent le même `execute`.
macro_rules! trigger_node {
    ($name:ident) => {
        #[async_trait]
        impl crate::nodes::trait_::NodeExecutor for $name {
            fn meta(&self) -> crate::nodes::trait_::NodeMeta {
                $name::meta_impl()
            }
            async fn execute(
                &self,
                _config: Value,
                exec_ctx: &ExecutionContext,
                _node_ctx: &NodeContext<'_>,
            ) -> Result<NodeOutput, NodeError> {
                super::emit_trigger(exec_ctx).await
            }
        }
    };
}
pub(crate) use trigger_node;
