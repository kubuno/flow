//! File de jobs PostgreSQL avec `FOR UPDATE SKIP LOCKED`.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
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
    db:             &PgPool,
    workflow_id:    Uuid,
    owner_id:       Uuid,
    trigger_source: &str,
    trigger_data:   Value,
    max_attempts:   i32,
) -> Result<Uuid, sqlx::Error> {
    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO flow.jobs (workflow_id, owner_id, trigger_source, trigger_data, max_attempts)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id
        "#,
    )
    .bind(workflow_id)
    .bind(owner_id)
    .bind(trigger_source)
    .bind(trigger_data)
    .bind(max_attempts)
    .fetch_one(db)
    .await?;
    Ok(id)
}

/// Réclame un lot de jobs prêts, en les marquant `running` de façon atomique.
/// Utilise SKIP LOCKED pour permettre plusieurs workers concurrents sans doublons.
pub async fn claim_batch(
    db:        &PgPool,
    worker_id: &str,
    batch:     i64,
) -> Result<Vec<Job>, sqlx::Error> {
    let jobs = sqlx::query_as::<_, Job>(
        r#"
        UPDATE flow.jobs SET
            status     = 'running',
            started_at = NOW(),
            worker_id  = $1,
            attempt    = attempt + 1
        WHERE id IN (
            SELECT id FROM flow.jobs
            WHERE status = 'pending' AND scheduled_at <= NOW()
            ORDER BY priority ASC, scheduled_at ASC
            FOR UPDATE SKIP LOCKED
            LIMIT $2
        )
        RETURNING *
        "#,
    )
    .bind(worker_id)
    .bind(batch)
    .fetch_all(db)
    .await?;
    Ok(jobs)
}

pub async fn mark_done(db: &PgPool, job_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE flow.jobs SET status = 'done', finished_at = NOW() WHERE id = $1")
        .bind(job_id)
        .execute(db)
        .await?;
    Ok(())
}

/// Marque le job en échec définitif (plus de tentatives).
pub async fn mark_failed(db: &PgPool, job_id: Uuid, error: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE flow.jobs SET status = 'failed', finished_at = NOW(), last_error = $2 WHERE id = $1",
    )
    .bind(job_id)
    .bind(error)
    .execute(db)
    .await?;
    Ok(())
}

/// Replanifie le job pour une nouvelle tentative (retry).
pub async fn reschedule(
    db:           &PgPool,
    job_id:       Uuid,
    delay_secs:   i64,
    error:        &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE flow.jobs SET
            status       = 'pending',
            scheduled_at = NOW() + ($2 || ' seconds')::interval,
            last_error   = $3,
            worker_id    = NULL
        WHERE id = $1
        "#,
    )
    .bind(job_id)
    .bind(delay_secs.to_string())
    .bind(error)
    .execute(db)
    .await?;
    Ok(())
}

/// Re-met en `pending` les jobs `running` orphelins (worker crashé) plus vieux que `stale_secs`.
pub async fn requeue_stale(db: &PgPool, stale_secs: i64) -> Result<u64, sqlx::Error> {
    let res = sqlx::query(
        r#"
        UPDATE flow.jobs SET status = 'pending', worker_id = NULL
        WHERE status = 'running'
          AND started_at < NOW() - ($1 || ' seconds')::interval
        "#,
    )
    .bind(stale_secs.to_string())
    .execute(db)
    .await?;
    Ok(res.rows_affected())
}
