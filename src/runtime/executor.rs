//! Orchestrateur d'exécution d'un workflow (ordre topologique + gating des branches).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::Settings;
use crate::models::workflow::{WorkflowDefinition, WorkflowEdge, WorkflowNode};
use crate::nodes::trait_::{ExecutionContext, NodeContext, NodeError};
use crate::nodes::NodeRegistry;
use crate::runtime::core_proxy::CoreProxy;
use crate::runtime::resolver;

pub struct Executor {
    pub db:       PgPool,
    pub registry: Arc<NodeRegistry>,
    pub proxy:    Arc<CoreProxy>,
    pub settings: Arc<Settings>,
}

#[derive(Debug)]
pub struct ExecOutcome {
    pub status:         &'static str, // "success" | "error" | "stopped"
    pub nodes_executed: i32,
    pub nodes_total:    i32,
    pub error_message:  Option<String>,
    pub retryable:      bool,
}

impl Executor {
    /// Exécute un workflow déjà matérialisé par une ligne `flow.executions`.
    pub async fn run(
        &self,
        execution_id: Uuid,
        owner_id:     Uuid,
        workflow_id:  Uuid,
        definition:   &WorkflowDefinition,
        trigger_data: Value,
        attempt:      i32,
    ) -> ExecOutcome {
        let start = Instant::now();
        let nodes_total = definition.nodes.len() as i32;

        // Index des nœuds et tri topologique.
        let by_id: HashMap<&str, &WorkflowNode> =
            definition.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

        let order = match topo_sort(&definition.nodes, &definition.edges) {
            Ok(o) => o,
            Err(_) => {
                let msg = "Cycle détecté dans le workflow".to_string();
                self.finalize(execution_id, "error", 0, nodes_total, Some(&msg), start).await;
                return ExecOutcome { status: "error", nodes_executed: 0, nodes_total, error_message: Some(msg), retryable: false };
            }
        };

        let node_ctx = NodeContext {
            proxy:    &self.proxy,
            user_id:  owner_id,
            db:       &self.db,
            settings: &self.settings,
        };

        let mut outputs: HashMap<String, Value> = HashMap::new();
        let mut live_edges: HashSet<String> = HashSet::new();
        let mut executed = 0i32;
        let node_timeout = Duration::from_secs(self.settings.runtime.node_timeout_secs.max(1));

        for node_id in &order {
            let Some(node) = by_id.get(node_id.as_str()) else { continue };
            let meta = self.registry.meta(&node.node_type);
            let is_trigger = meta.as_ref().map(|m| m.is_trigger()).unwrap_or(false);

            // Activation : trigger inconditionnel, sinon au moins une arête entrante vive.
            let incoming: Vec<&WorkflowEdge> = definition.edges.iter()
                .filter(|e| &e.target == node_id).collect();
            let active_incoming: Vec<&WorkflowEdge> = incoming.iter()
                .copied().filter(|e| live_edges.contains(&e.id)).collect();

            if !is_trigger && active_incoming.is_empty() && !incoming.is_empty() {
                continue; // nœud non atteint (branche morte)
            }

            // Donnée d'entrée : sortie des prédécesseurs vivants.
            let input = build_input(&active_incoming, &outputs, &trigger_data);

            // Contexte de résolution d'expressions.
            let full = json!({
                "trigger": trigger_data,
                "nodes":   &outputs,
                "input":   input,
                "json":    input,
            });

            let exec_ctx = ExecutionContext {
                execution_id,
                workflow_id,
                owner_id,
                current_node_id: node.id.clone(),
                attempt,
                input: input.clone(),
                full,
            };

            // Nœud inconnu → erreur.
            let Some(executor) = self.registry.get(&node.node_type) else {
                let msg = format!("Type de nœud inconnu : {}", node.node_type);
                self.log_node(execution_id, node, "error", Some(&input), None, Some(&msg), None, attempt, start.elapsed()).await;
                self.finalize(execution_id, "error", executed, nodes_total, Some(&msg), start).await;
                return ExecOutcome { status: "error", nodes_executed: executed, nodes_total, error_message: Some(msg), retryable: false };
            };

            // Résolution des expressions de la config.
            let resolved = resolver::resolve_value(&node.config, &exec_ctx.full);

            let node_start = Instant::now();
            let result = tokio::time::timeout(node_timeout, executor.execute(resolved, &exec_ctx, &node_ctx)).await;
            let node_dur = node_start.elapsed();

            match result {
                Ok(Ok(output)) => {
                    executed += 1;
                    self.log_node(execution_id, node, "success", Some(&input), Some(&output.data), None, None, attempt, node_dur).await;
                    outputs.insert(node.id.clone(), output.data.clone());
                    // Marquer les arêtes sortantes vives selon les branches choisies.
                    for e in definition.edges.iter().filter(|e| e.source == node.id) {
                        if edge_live(&output.branches, e) {
                            live_edges.insert(e.id.clone());
                        }
                    }
                }
                Ok(Err(NodeError::Stopped(msg))) => {
                    // Stop explicite (mode erreur) → exécution en erreur, non retryable.
                    self.log_node(execution_id, node, "error", Some(&input), None, Some(&msg), None, attempt, node_dur).await;
                    self.finalize(execution_id, "error", executed, nodes_total, Some(&msg), start).await;
                    return ExecOutcome { status: "error", nodes_executed: executed, nodes_total, error_message: Some(msg), retryable: false };
                }
                Ok(Err(e)) => {
                    let retryable = e.is_retryable();
                    let msg = e.to_string();
                    self.log_node(execution_id, node, "error", Some(&input), None, Some(&msg), None, attempt, node_dur).await;
                    self.finalize(execution_id, "error", executed, nodes_total, Some(&msg), start).await;
                    return ExecOutcome { status: "error", nodes_executed: executed, nodes_total, error_message: Some(msg), retryable };
                }
                Err(_) => {
                    let msg = format!("Nœud « {} » : délai dépassé", node.name.clone().unwrap_or(node.node_type.clone()));
                    self.log_node(execution_id, node, "error", Some(&input), None, Some(&msg), None, attempt, node_dur).await;
                    self.finalize(execution_id, "error", executed, nodes_total, Some(&msg), start).await;
                    return ExecOutcome { status: "error", nodes_executed: executed, nodes_total, error_message: Some(msg), retryable: true };
                }
            }
        }

        self.finalize(execution_id, "success", executed, nodes_total, None, start).await;
        ExecOutcome { status: "success", nodes_executed: executed, nodes_total, error_message: None, retryable: false }
    }

    #[allow(clippy::too_many_arguments)]
    async fn log_node(
        &self,
        execution_id: Uuid,
        node:         &WorkflowNode,
        status:       &str,
        input:        Option<&Value>,
        output:       Option<&Value>,
        error:        Option<&str>,
        stack:        Option<&str>,
        attempt:      i32,
        dur:          Duration,
    ) {
        let res = sqlx::query(
            r#"INSERT INTO flow.node_logs
                (execution_id, node_id, node_type, node_name, status, input_data, output_data,
                 error_message, error_stack, duration_ms, attempt)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)"#,
        )
        .bind(execution_id)
        .bind(&node.id)
        .bind(&node.node_type)
        .bind(node.name.as_deref())
        .bind(status)
        .bind(input.cloned())
        .bind(output.cloned())
        .bind(error)
        .bind(stack)
        .bind(dur.as_millis() as i32)
        .bind(attempt)
        .execute(&self.db)
        .await;
        if let Err(e) = res {
            tracing::error!(error = %e, "Insertion node_log échouée");
        }
    }

    async fn finalize(
        &self,
        execution_id: Uuid,
        status:       &str,
        executed:     i32,
        total:        i32,
        error:        Option<&str>,
        start:        Instant,
    ) {
        let dur = start.elapsed().as_millis() as i32;
        let res = sqlx::query(
            r#"UPDATE flow.executions SET
                status = $2, nodes_executed = $3, nodes_total = $4,
                error_message = $5, duration_ms = $6, finished_at = NOW()
               WHERE id = $1"#,
        )
        .bind(execution_id)
        .bind(status)
        .bind(executed)
        .bind(total)
        .bind(error)
        .bind(dur)
        .execute(&self.db)
        .await;
        if let Err(e) = res {
            tracing::error!(error = %e, "Finalisation execution échouée");
        }
    }
}

/// Données d'entrée d'un nœud : sortie unique, ou tableau si plusieurs branches.
fn build_input(active: &[&WorkflowEdge], outputs: &HashMap<String, Value>, trigger: &Value) -> Value {
    let mut collected: Vec<Value> = Vec::new();
    for e in active {
        if let Some(v) = outputs.get(&e.source) {
            collected.push(v.clone());
        }
    }
    match collected.len() {
        0 => trigger.clone(),
        1 => collected.into_iter().next().unwrap(),
        _ => Value::Array(collected),
    }
}

fn edge_live(branches: &Option<Vec<String>>, edge: &WorkflowEdge) -> bool {
    match branches {
        None => true,
        Some(ports) => {
            let p = edge.source_port.clone().unwrap_or_else(|| "default".to_string());
            ports.iter().any(|x| x == &p)
        }
    }
}

/// Tri topologique (Kahn). Erreur si cycle.
fn topo_sort(nodes: &[WorkflowNode], edges: &[WorkflowEdge]) -> Result<Vec<String>, ()> {
    let ids: HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    let mut indeg: HashMap<String, usize> = nodes.iter().map(|n| (n.id.clone(), 0)).collect();
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();

    for e in edges {
        if !ids.contains(e.source.as_str()) || !ids.contains(e.target.as_str()) {
            continue;
        }
        adj.entry(e.source.clone()).or_default().push(e.target.clone());
        *indeg.entry(e.target.clone()).or_insert(0) += 1;
    }

    let mut queue: Vec<String> = indeg.iter().filter(|(_, &d)| d == 0).map(|(k, _)| k.clone()).collect();
    queue.sort();
    let mut order = Vec::with_capacity(nodes.len());

    while let Some(n) = queue.pop() {
        order.push(n.clone());
        if let Some(neis) = adj.get(&n) {
            let mut newly_ready = Vec::new();
            for m in neis {
                if let Some(d) = indeg.get_mut(m) {
                    *d -= 1;
                    if *d == 0 {
                        newly_ready.push(m.clone());
                    }
                }
            }
            newly_ready.sort();
            queue.extend(newly_ready);
        }
    }

    if order.len() == nodes.len() {
        Ok(order)
    } else {
        Err(())
    }
}
