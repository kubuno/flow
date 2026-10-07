//! Portable job queue. The claim is a conditional UPDATE whose `rows_affected`
//! is the proof of ownership, not `FOR UPDATE SKIP LOCKED`: neither SQLite nor
//! MariaDB offers `SKIP LOCKED`, and a lock taken outside a transaction
//! guarantees nothing anyway. Each candidate is claimed on its own; exactly one
//! worker can see the UPDATE change one row.

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use kubuno_db::{new_id, params, DbPool};
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct Job {
    pub id:             Uuid,
    pub workflow_id:    Uuid,
    pub owner_id:       Uuid,
    pub status:         String,
    pub trigger_data:   Value,
    pub trigger_source: String,
    pub priority:       i32,
    pub attempt:        i32,
    pub max_attempts:   i32,
    pub scheduled_at:   DateTime<Utc>,
    pub started_at:     Option<DateTime<Utc>>,
    pub finished_at:    Option<DateTime<Utc>>,
    pub last_error:     Option<String>,
    pub worker_id:      Option<String>,
    pub created_at:     DateTime<Utc>,
}

/// Insère un nouveau job dans la file.
pub async fn enqueue(
    db:             &DbPool,
    workflow_id:    Uuid,
    owner_id:       Uuid,
    trigger_source: &str,
    trigger_data:   Value,
    max_attempts:   i32,
) -> Result<Uuid, sqlx::Error> {
    // The id is minted here, not by the database: MySQL/SQLite have no server
    // UUID default and no RETURNING (see kubuno_db::new_id). Other columns take
    // their table defaults (status='pending', priority, attempt, scheduled_at…).
    let id = new_id();
    db.execute(
        "INSERT INTO flow.jobs (id, workflow_id, owner_id, trigger_source, trigger_data, max_attempts) \
         VALUES ($1, $2, $3, $4, $5, $6)",
        params![id, workflow_id, owner_id, trigger_source, trigger_data, max_attempts],
    )
    .await?;
    Ok(id)
}

/// Réclame un lot de jobs prêts, en les marquant `running` un par un. La preuve
/// de possession = l'UPDATE conditionnel qui ne touche qu'un job encore
/// `pending` (portable, sans SKIP LOCKED).
pub async fn claim_batch(
    db:        &DbPool,
    worker_id: &str,
    batch:     i64,
) -> Result<Vec<Job>, sqlx::Error> {
    let now = Utc::now();

    // Candidates: ready, pending, highest priority (lowest number) first.
    let candidates: Vec<(Uuid,)> = db
        .fetch_all_as::<(Uuid,)>(
            "SELECT id FROM flow.jobs \
             WHERE status = 'pending' AND scheduled_at <= $1 \
             ORDER BY priority ASC, scheduled_at ASC \
             LIMIT $2",
            params![now, batch],
        )
        .await?;

    let mut claimed = Vec::new();
    for (id,) in candidates {
        // One statement both claims the job and stamps it running: the row count
        // it changed IS the proof of ownership, so only one worker keeps it.
        let won = db
            .execute(
                "UPDATE flow.jobs SET \
                    status = 'running', started_at = $1, worker_id = $2, attempt = attempt + 1 \
                 WHERE id = $3 AND status = 'pending'",
                params![now, worker_id, id],
            )
            .await?;
        if won == 1 {
            if let Some(job) = db
                .fetch_optional_as::<Job>("SELECT * FROM flow.jobs WHERE id = $1", params![id])
                .await?
            {
                claimed.push(job);
            }
        }
    }
    Ok(claimed)
}

pub async fn mark_done(db: &DbPool, job_id: Uuid) -> Result<(), sqlx::Error> {
    db.execute(
        "UPDATE flow.jobs SET status = 'done', finished_at = $1 WHERE id = $2",
        params![Utc::now(), job_id],
    )
    .await?;
    Ok(())
}

/// Marque le job en échec définitif (plus de tentatives).
pub async fn mark_failed(db: &DbPool, job_id: Uuid, error: &str) -> Result<(), sqlx::Error> {
    db.execute(
        "UPDATE flow.jobs SET status = 'failed', finished_at = $1, last_error = $2 WHERE id = $3",
        params![Utc::now(), error, job_id],
    )
    .await?;
    Ok(())
}

/// Replanifie le job pour une nouvelle tentative (retry). Le délai est appliqué
/// en Rust (pas d'arithmétique d'intervalle SQL, qui diffère par moteur).
pub async fn reschedule(
    db:         &DbPool,
    job_id:     Uuid,
    delay_secs: i64,
    error:      &str,
) -> Result<(), sqlx::Error> {
    let scheduled_at = Utc::now() + ChronoDuration::seconds(delay_secs.max(0));
    db.execute(
        "UPDATE flow.jobs SET status = 'pending', scheduled_at = $1, last_error = $2, worker_id = NULL \
         WHERE id = $3",
        params![scheduled_at, error, job_id],
    )
    .await?;
    Ok(())
}

/// Re-met en `pending` les jobs `running` orphelins (worker crashé) plus vieux
/// que `stale_secs`. Le seuil temporel est calculé en Rust.
pub async fn requeue_stale(db: &DbPool, stale_secs: i64) -> Result<u64, sqlx::Error> {
    let cutoff = Utc::now() - ChronoDuration::seconds(stale_secs.max(0));
    db.execute(
        "UPDATE flow.jobs SET status = 'pending', worker_id = NULL \
         WHERE status = 'running' AND started_at < $1",
        params![cutoff],
    )
    .await
}
