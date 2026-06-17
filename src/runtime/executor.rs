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
    pub db:           PgPool,
    pub registry:     Arc<NodeRegistry>,
    pub proxy:        Arc<CoreProxy>,
    pub settings:     Arc<Settings>,
    pub files_client: Arc<crate::files_client::FilesClient>,
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
            proxy:        &self.proxy,
            user_id:      owner_id,
            db:           &self.db,
            settings:     &self.settings,
            registry:     &self.registry,
            files_client: &self.files_client,
            depth:        0,
        };

        let mut outputs: HashMap<String, Value> = HashMap::new();
        let mut live_edges: HashSet<String> = HashSet::new();
        let mut executed = 0i32;
        let node_timeout = Duration::from_secs(self.settings.runtime.node_timeout_secs.max(1));

        // Un nœud est-il un sous-nœud fournisseur IA (modèle/mémoire/outil/parser) ?
        let is_provider = |t: &str| self.registry.ai_output(t).is_some();

        for node_id in &order {
            let Some(node) = by_id.get(node_id.as_str()) else { continue };

            // Sous-nœud fournisseur : jamais exécuté dans le flux principal ; sa config
            // est consommée par l'agent via son port de sous-entrée.
            if is_provider(&node.node_type) { continue; }

            let meta = self.registry.meta(&node.node_type);
            let is_trigger = meta.as_ref().map(|m| m.is_trigger()).unwrap_or(false);

            // Activation : trigger inconditionnel, sinon au moins une arête entrante vive.
            // Les arêtes venant d'un sous-nœud IA ne comptent PAS comme flux de données.
            let incoming: Vec<&WorkflowEdge> = definition.edges.iter()
                .filter(|e| &e.target == node_id
                    && by_id.get(e.source.as_str()).map(|s| !is_provider(&s.node_type)).unwrap_or(true))
                .collect();
            let active_incoming: Vec<&WorkflowEdge> = incoming.iter()
                .copied().filter(|e| live_edges.contains(&e.id)).collect();

            if !is_trigger && active_incoming.is_empty() && !incoming.is_empty() {
                continue; // nœud non atteint (branche morte)
            }

            // Donnée d'entrée : sortie des prédécesseurs vivants.
            let input = build_input(&active_incoming, &outputs, &trigger_data);

            // Nœud désactivé : ignoré, l'entrée passe telle quelle vers toutes les sorties.
            if node.settings.disabled {
                outputs.insert(node.id.clone(), input.clone());
                for e in definition.edges.iter().filter(|e| e.source == node.id) {
                    live_edges.insert(e.id.clone());
                }
                continue;
            }

            // Contexte de résolution d'expressions (variables n8n-like : $json, $now…).
            let mut ctx_map = serde_json::Map::new();
            ctx_map.insert("trigger".into(), trigger_data.clone());
            ctx_map.insert("nodes".into(), json!(&outputs));
            ctx_map.insert("input".into(), input.clone());
            ctx_map.insert("json".into(), input.clone());
            ctx_map.insert("$json".into(), input.clone());
            ctx_map.insert("$input".into(), input.clone());
            ctx_map.insert("$workflow".into(), json!({ "id": workflow_id.to_string() }));
            ctx_map.insert("$execution".into(), json!({ "id": execution_id.to_string(), "mode": "trigger" }));
            crate::runtime::expr::with_now(&mut ctx_map);
            let full = Value::Object(ctx_map);

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

            // Résolution des expressions de la config, puis injection des credentials
            // (les champs Credential = id → remplacés par leur payload déchiffré).
            let mut resolved = resolver::resolve_value(&node.config, &exec_ctx.full);
            crate::services::credentials::inject_into_config(
                &self.registry, &self.db, &self.settings.core.internal_secret,
                owner_id, &node.node_type, &mut resolved,
            ).await;

            // Agent IA : rassembler les sous-nœuds branchés (modèle/mémoire/outils/parser)
            // par port, résoudre leur config (+ credentials), injecter sous `__sub`.
            let subs = self.registry.sub_inputs(&node.node_type);
            if !subs.is_empty() {
                let mut sub_map = serde_json::Map::new();
                for si in &subs {
                    let mut items = Vec::new();
                    for e in definition.edges.iter().filter(|e| &e.target == node_id && e.target_port.as_deref() == Some(si.id.as_str())) {
                        if let Some(src) = by_id.get(e.source.as_str()) {
                            let mut sc = resolver::resolve_value(&src.config, &exec_ctx.full);
                            crate::services::credentials::inject_into_config(
                                &self.registry, &self.db, &self.settings.core.internal_secret,
                                owner_id, &src.node_type, &mut sc,
                            ).await;
                            items.push(json!({ "type": src.node_type, "name": src.name, "config": sc }));
                        }
                    }
                    sub_map.insert(si.id.clone(), Value::Array(items));
                }
                if let Some(obj) = resolved.as_object_mut() {
                    obj.insert("__sub".into(), Value::Object(sub_map));
                }
            }

            // Tentatives par nœud (retry-on-fail). 0 retry → 1 tentative.
            let max_tries = node.settings.retry_max.unwrap_or(0).min(5) + 1;
            let retry_delay = Duration::from_millis(node.settings.retry_delay_ms.unwrap_or(1000).min(60_000));

            let node_start = Instant::now();
            let mut local_attempt = 0u32;
            // (output, OK) | Err((message, retryable_au_niveau_job, stop_explicite))
            let outcome: Result<crate::nodes::trait_::NodeOutput, (String, bool, bool)> = loop {
                local_attempt += 1;
                let r = tokio::time::timeout(node_timeout, executor.execute(resolved.clone(), &exec_ctx, &node_ctx)).await;
                match r {
                    Ok(Ok(output)) => break Ok(output),
                    Ok(Err(NodeError::Stopped(msg))) => break Err((msg, false, true)),
                    Ok(Err(e)) => {
                        if local_attempt < max_tries { tokio::time::sleep(retry_delay).await; continue; }
                        break Err((e.to_string(), e.is_retryable(), false));
                    }
                    Err(_) => {
                        if local_attempt < max_tries { tokio::time::sleep(retry_delay).await; continue; }
                        let msg = format!("Nœud « {} » : délai dépassé", node.name.clone().unwrap_or(node.node_type.clone()));
                        break Err((msg, true, false));
                    }
                }
            };
            let node_dur = node_start.elapsed();

            match outcome {
                Ok(output) => {
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
                // Continuer-sur-erreur (sauf Stop explicite) : journalise l'erreur mais poursuit
                // le flux avec `{ error }` en sortie, toutes les arêtes sortantes actives.
                Err((msg, _retryable, is_stop)) if !is_stop && node.settings.continues_on_error() => {
                    executed += 1;
                    self.log_node(execution_id, node, "error", Some(&input), None, Some(&msg), None, attempt, node_dur).await;
                    let errout = json!({ "error": msg });
                    outputs.insert(node.id.clone(), errout);
                    for e in definition.edges.iter().filter(|e| e.source == node.id) {
                        live_edges.insert(e.id.clone());
                    }
                }
                Err((msg, retryable, _is_stop)) => {
                    self.log_node(execution_id, node, "error", Some(&input), None, Some(&msg), None, attempt, node_dur).await;
                    self.finalize(execution_id, "error", executed, nodes_total, Some(&msg), start).await;
                    return ExecOutcome { status: "error", nodes_executed: executed, nodes_total, error_message: Some(msg), retryable };
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

/// Exécute un workflow « en ligne » (sans persistance ni logs) et retourne la
/// sortie du dernier nœud. Utilisé par le nœud Sous-workflow ; `parent.depth`
/// borne l'imbrication pour empêcher toute récursion infinie entre workflows.
pub async fn run_workflow_inline(
    parent:       &NodeContext<'_>,
    owner_id:     Uuid,
    workflow_id:  Uuid,
    definition:   &WorkflowDefinition,
    trigger_data: Value,
) -> Result<Value, String> {
    if parent.depth > 5 {
        return Err("Imbrication de sous-workflows trop profonde (max 5)".into());
    }
    let child = NodeContext {
        proxy:        parent.proxy,
        user_id:      owner_id,
        db:           parent.db,
        settings:     parent.settings,
        registry:     parent.registry,
        files_client: parent.files_client,
        depth:        parent.depth + 1,
    };

    let by_id: HashMap<&str, &WorkflowNode> =
        definition.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let order = topo_sort(&definition.nodes, &definition.edges)
        .map_err(|_| "Cycle détecté dans le sous-workflow".to_string())?;

    let mut outputs: HashMap<String, Value> = HashMap::new();
    let mut live_edges: HashSet<String> = HashSet::new();
    let mut last_output = trigger_data.clone();
    let node_timeout = Duration::from_secs(parent.settings.runtime.node_timeout_secs.max(1));

    for node_id in &order {
        let Some(node) = by_id.get(node_id.as_str()) else { continue };
        let meta = parent.registry.meta(&node.node_type);
        let is_trigger = meta.as_ref().map(|m| m.is_trigger()).unwrap_or(false);

        let incoming: Vec<&WorkflowEdge> = definition.edges.iter().filter(|e| &e.target == node_id).collect();
        let active_incoming: Vec<&WorkflowEdge> = incoming.iter().copied().filter(|e| live_edges.contains(&e.id)).collect();
        if !is_trigger && active_incoming.is_empty() && !incoming.is_empty() {
            continue;
        }

        let input = build_input(&active_incoming, &outputs, &trigger_data);

        if node.settings.disabled {
            outputs.insert(node.id.clone(), input.clone());
            last_output = input.clone();
            for e in definition.edges.iter().filter(|e| e.source == node.id) {
                live_edges.insert(e.id.clone());
            }
            continue;
        }

        let mut ctx_map = serde_json::Map::new();
        ctx_map.insert("trigger".into(), trigger_data.clone());
        ctx_map.insert("nodes".into(), json!(&outputs));
        ctx_map.insert("input".into(), input.clone());
        ctx_map.insert("json".into(), input.clone());
        ctx_map.insert("$json".into(), input.clone());
        ctx_map.insert("$input".into(), input.clone());
        ctx_map.insert("$workflow".into(), json!({ "id": workflow_id.to_string() }));
        crate::runtime::expr::with_now(&mut ctx_map);
        let full = Value::Object(ctx_map);

        let exec_ctx = ExecutionContext {
            execution_id: Uuid::nil(),
            workflow_id,
            owner_id,
            current_node_id: node.id.clone(),
            attempt: 1,
            input: input.clone(),
            full: full.clone(),
        };

        let Some(executor) = parent.registry.get(&node.node_type) else {
            return Err(format!("Type de nœud inconnu : {}", node.node_type));
        };
        let mut resolved = resolver::resolve_value(&node.config, &full);
        crate::services::credentials::inject_into_config(
            parent.registry, parent.db, &parent.settings.core.internal_secret,
            owner_id, &node.node_type, &mut resolved,
        ).await;
        let result = tokio::time::timeout(node_timeout, executor.execute(resolved, &exec_ctx, &child)).await;
        match result {
            Ok(Ok(output)) => {
                last_output = output.data.clone();
                outputs.insert(node.id.clone(), output.data.clone());
                for e in definition.edges.iter().filter(|e| e.source == node.id) {
                    if edge_live(&output.branches, e) {
                        live_edges.insert(e.id.clone());
                    }
                }
            }
            Ok(Err(NodeError::Stopped(msg))) => return Err(msg),
            Ok(Err(e)) => return Err(e.to_string()),
            Err(_) => return Err("Sous-workflow : délai dépassé".into()),
        }
    }
    Ok(last_output)
}
