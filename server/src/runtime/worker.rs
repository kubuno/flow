//! Workers Tokio : consomment la file de jobs et exécutent les workflows.

use std::time::Duration;

use kubuno_db::{new_id, params};
use serde_json::json;
use uuid::Uuid;

use crate::models::workflow::WorkflowDefinition;
use crate::runtime::executor::Executor;
use crate::runtime::{queue, retry};
use crate::state::AppState;

/// Démarre `worker_count` boucles de consommation + un balayeur de jobs orphelins.
pub fn spawn_workers(state: AppState) {
    let count = state.settings.runtime.worker_count.max(1);
    for i in 0..count {
        let st = state.clone();
        let worker_id = format!("worker-{i}");
        tokio::spawn(async move { worker_loop(st, worker_id).await });
    }

    // Balayeur : re-met en file les jobs `running` orphelins toutes les 60s.
    let st = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let stale = (st.settings.runtime.execution_timeout_secs as i64) + 60;
            if let Ok(n) = queue::requeue_stale(&st.db, stale).await {
                if n > 0 {
                    tracing::warn!(count = n, "Jobs orphelins remis en file");
                }
            }
        }
    });
}

async fn worker_loop(state: AppState, worker_id: String) {
    let poll = Duration::from_millis(state.settings.queue.poll_interval_ms.max(50));
    let batch = state.settings.queue.batch_size.max(1);

    loop {
        let jobs = match queue::claim_batch(&state.db, &worker_id, batch).await {
            Ok(j) => j,
            Err(e) => {
                tracing::error!(error = %e, "claim_batch échoué");
                tokio::time::sleep(poll).await;
                continue;
            }
        };

        if jobs.is_empty() {
            tokio::time::sleep(poll).await;
            continue;
        }

        for job in jobs {
            process_job(&state, job).await;
        }
    }
}

async fn process_job(state: &AppState, job: queue::Job) {
    // Charger la référence du fichier de définition (.kbflw).
    let row = state.db.fetch_optional_as::<(Option<Uuid>,)>(
        "SELECT file_id FROM flow.workflows WHERE id = $1",
        params![job.workflow_id],
    )
    .await;

    let file_id = match row {
        Ok(Some((fid,))) => fid,
        Ok(None) => {
            let _ = queue::mark_failed(&state.db, job.id, "Workflow introuvable").await;
            return;
        }
        Err(e) => {
            tracing::error!(error = %e, "Lecture workflow échouée");
            let _ = queue::reschedule(&state.db, job.id, 30, "Erreur DB").await;
            return;
        }
    };
    let definition_value = match file_id {
        Some(fid) => match crate::services::content_files::read_definition(state, job.owner_id, fid).await {
            Ok(d) => d,
            Err(e) => {
                tracing::error!(error = %e, "Lecture de la définition (.kbflw) échouée");
                let _ = queue::reschedule(&state.db, job.id, 30, "Erreur lecture définition").await;
                return;
            }
        },
        None => crate::services::content_files::empty_definition(),
    };
    let definition = WorkflowDefinition::from_value(&definition_value);

    // Créer la ligne d'exécution (id minté en Rust, pas de RETURNING).
    let execution_id = new_id();
    if let Err(e) = state.db.execute(
        "INSERT INTO flow.executions \
            (id, job_id, workflow_id, owner_id, status, trigger_source, trigger_data, nodes_total) \
           VALUES ($1,$2,$3,$4,'running',$5,$6,$7)",
        params![
            execution_id, job.id, job.workflow_id, job.owner_id,
            &job.trigger_source, job.trigger_data.clone(), definition.nodes.len() as i32
        ],
    )
    .await
    {
        tracing::error!(error = %e, "Création execution échouée");
        let _ = queue::reschedule(&state.db, job.id, 30, "Erreur DB").await;
        return;
    }

    let executor = Executor {
        db:           state.db.clone(),
        registry:     state.registry.clone(),
        proxy:        state.proxy.clone(),
        settings:     state.settings.clone(),
        files_client: state.files_client.clone(),
        instance:     state.instance(),
    };

    // Protège le fichier workflow tant que l'exécution se poursuit.
    if let Some(fid) = file_id {
        let _ = state.files_client.set_file_protected(job.owner_id, fid, true).await;
    }

    let outcome = executor
        .run(execution_id, job.owner_id, job.workflow_id, &definition, job.trigger_data.clone(), job.attempt)
        .await;

    if let Some(fid) = file_id {
        let _ = state.files_client.set_file_protected(job.owner_id, fid, false).await;
    }

    // Mise à jour de la file + stats workflow + événements.
    match outcome.status {
        "success" => {
            let _ = queue::mark_done(&state.db, job.id).await;
            let now = chrono::Utc::now();
            let _ = state.db.execute(
                "UPDATE flow.workflows SET \
                    execution_count = execution_count + 1, \
                    last_executed_at = $1, last_error = NULL, updated_at = $2 \
                   WHERE id = $3",
                params![now, now, job.workflow_id],
            ).await;
            publish_workflow_event(state, "WorkflowExecuted", job.workflow_id, job.owner_id, None).await;
        }
        _ => {
            let err = outcome.error_message.clone().unwrap_or_else(|| "Erreur inconnue".into());
            if outcome.retryable && job.attempt < job.max_attempts {
                let delay = retry::backoff_delay(job.attempt, state.instance().retry_backoff_ms);
                let _ = queue::reschedule(&state.db, job.id, delay.as_secs() as i64, &err).await;
                tracing::warn!(job = %job.id, attempt = job.attempt, "Job replanifié (retry)");
            } else {
                let _ = queue::mark_failed(&state.db, job.id, &err).await;
                let now = chrono::Utc::now();
                let _ = state.db.execute(
                    "UPDATE flow.workflows SET \
                        execution_count = execution_count + 1, \
                        error_count = error_count + 1, \
                        last_executed_at = $1, last_error = $2, updated_at = $3 \
                       WHERE id = $4",
                    params![now, &err, now, job.workflow_id],
                ).await;
                publish_workflow_event(state, "WorkflowFailed", job.workflow_id, job.owner_id, Some(&err)).await;

                // Déclencheur d'erreur : démarre les workflows « trigger.error » de l'utilisateur
                // (sauf si CET échec provient déjà d'un workflow d'erreur → pas de boucle).
                if job.trigger_source != "error" {
                    let wf_name = state.db.fetch_optional_scalar::<String>(
                        "SELECT name FROM flow.workflows WHERE id = $1",
                        params![job.workflow_id],
                    ).await.ok().flatten().unwrap_or_default();
                    crate::runtime::scheduler::dispatch_error_workflows(
                        state, job.workflow_id, &wf_name, job.owner_id, execution_id, &err,
                    ).await;
                }
            }
        }
    }

    // Élagage de l'historique.
    prune_history(state, job.workflow_id).await;
}

async fn prune_history(state: &AppState, workflow_id: Uuid) {
    let keep = state.instance().max_execution_history;
    // The kept-rows subquery is wrapped in a derived table: MySQL forbids
    // referencing the DELETE's target table directly in a subquery, and the
    // wrapped form is valid on all three engines. Placeholders are bound once
    // each, in order (the rewriter forbids reusing $n).
    let _ = state.db.execute(
        "DELETE FROM flow.executions \
           WHERE workflow_id = $1 AND id NOT IN ( \
               SELECT id FROM ( \
                   SELECT id FROM flow.executions WHERE workflow_id = $2 \
                   ORDER BY started_at DESC LIMIT $3 \
               ) AS keep \
           )",
        params![workflow_id, workflow_id, keep],
    )
    .await;
}

async fn publish_workflow_event(
    state: &AppState,
    event_type: &str,
    workflow_id: Uuid,
    owner_id: Uuid,
    error: Option<&str>,
) {
    let event = json!({
        "type": "Custom",
        "payload": {
            "event_type": event_type,
            "module_id":  "flow",
            "payload": {
                "workflow_id": workflow_id,
                "user_id":     owner_id,
                "error":       error,
            }
        }
    });
    let _ = state.proxy.publish_event(&event).await;
}
