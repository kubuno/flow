use async_trait::async_trait;
use serde_json::Value;

use super::trigger_node;
use crate::nodes::trait_::{
    ExecutionContext, NodeCategory, NodeContext, NodeError, NodeMeta, NodeOutput,
};

pub struct ExecuteWorkflowTrigger;

impl ExecuteWorkflowTrigger {
    pub fn meta_impl() -> NodeMeta {
        NodeMeta {
            node_type:   "trigger.execute_workflow".into(),
            name:        "Appelé par un autre workflow".into(),
            description: "Point d'entrée d'un sous-workflow appelé via « Sous-workflow » / « Boucle »".into(),
            category:    NodeCategory::Trigger,
            icon:        "LogIn".into(),
            color:       "#7e57c2".into(),
            inputs:      0,
            outputs:     vec![],
            // Émet les données passées par l'appelant (champ `input` du nœud Sous-workflow).
            fields:      vec![],
        }
    }
}

trigger_node!(ExecuteWorkflowTrigger);
