//! Runs the flow module's own migrations and storage paths against a real
//! server of **each** engine, from a single compiled binary — the proof that
//! the engine is a run-time choice, not a build-time one, and that the ported
//! pieces (JSON `tags`, the BYTEA credential blobs, the portable job-queue claim
//! and the dialect upserts) behave identically on all of them.
//!
//! A workflow's definition is drive-backed (the `.kbflw` files live in the
//! `drive` module, reached over HTTP), so the full workflow CRUD path cannot run
//! in a unit test. This binary exercises the DATABASE layer directly, issuing
//! the very SQL the services issue (ids minted in Rust, tags as a JSON array,
//! the rows_affected job claim), which is what the port changed.
//!
//! * SQLite always runs (a temp file, no server).
//! * PostgreSQL runs when `KUBUNO_PG_TEST_URL` points at a throwaway database.
//! * MySQL/MariaDB runs when `KUBUNO_MYSQL_TEST_URL` does.
//!
//! ```sh
//! KUBUNO_PG_TEST_URL=postgres://u:p@127.0.0.1:5432/kubuno_test \
//! KUBUNO_MYSQL_TEST_URL=mysql://u:p@127.0.0.1:3306/flow \
//!   cargo test --test db_portability
//! ```

use kubuno_db::dialect::Assign;
use kubuno_db::{new_id, params, DbPool, DbSettings};
use kubuno_flow::models::credential::Credential;
use kubuno_flow::models::workflow::Workflow;
use kubuno_flow::runtime::queue;
use kubuno_flow::SCHEMA;
use uuid::Uuid;

fn base_settings(engine: &str) -> DbSettings {
    DbSettings {
        engine: engine.to_string(),
        url: None,
        host: None,
        port: None,
        user: None,
        password: None,
        database: None,
        path: None,
        max_connections: 4,
        min_connections: 0,
        connect_timeout: std::time::Duration::from_secs(10),
        run_migrations: true,
    }
}

/// Migrations run one at a time: the PostgreSQL and MySQL suites may share a server.
static EXCLUSIVE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn migrated_pool(settings: DbSettings) -> (DbPool, impl Sized) {
    let guard = EXCLUSIVE.lock().await;
    let pool = kubuno_db::connect(&settings, SCHEMA).await.expect("connect");
    kubuno_db::migrations!(
        "./migrations/postgres",
        "./migrations/mysql",
        "./migrations/sqlite",
    )
    .run(&pool, SCHEMA)
    .await
    .expect("migrations");
    (pool, guard)
}

// ── Direct DB writes mirroring the services (no drive dependency) ────────────

async fn insert_workflow(pool: &DbPool, owner: Uuid, name: &str, tags: &[&str]) -> Uuid {
    let id = new_id();
    let now = chrono::Utc::now();
    let tags: Vec<String> = tags.iter().map(|s| s.to_string()).collect();
    pool.execute(
        "INSERT INTO flow.workflows (id, owner_id, name, description, file_id, tags, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        params![id, owner, name, Some("desc"), Some(new_id()), &tags, now, now],
    )
    .await
    .expect("insert workflow");
    id
}

async fn get_workflow(pool: &DbPool, id: Uuid, owner: Uuid) -> Option<Workflow> {
    pool.fetch_optional_as::<Workflow>(
        "SELECT * FROM flow.workflows WHERE id = $1 AND owner_id = $2",
        params![id, owner],
    )
    .await
    .expect("get workflow")
}

// ── The suite, run once per engine ───────────────────────────────────────────

async fn run_all(pool: &DbPool) {
    let owner = new_id();

    // 1) Workflow CRUD + JSON tags round-trip + int-width COUNT.
    let id = insert_workflow(pool, owner, "Mon workflow", &["alpha", "beta"]).await;
    let wf = get_workflow(pool, id, owner).await.expect("workflow present");
    assert_eq!(wf.name, "Mon workflow");
    assert_eq!(wf.tags, vec!["alpha".to_string(), "beta".to_string()]);
    assert_eq!(wf.status, "inactive");
    assert!(!wf.is_starred);
    assert_eq!(wf.execution_count, 0);
    assert!(wf.file_id.is_some());

    // Update: status + tags + updated_at, then reselect.
    let now = chrono::Utc::now();
    let new_tags = vec!["solo".to_string()];
    pool.execute(
        "UPDATE flow.workflows SET status = $1, tags = $2, is_starred = $3, updated_at = $4 \
         WHERE id = $5 AND owner_id = $6",
        params!["active", &new_tags, true, now, id, owner],
    )
    .await
    .expect("update workflow");
    let wf = get_workflow(pool, id, owner).await.expect("workflow present");
    assert_eq!(wf.status, "active");
    assert_eq!(wf.tags, new_tags);
    assert!(wf.is_starred);

    // COUNT(*) as i64 — the PG-strict int-width path (a literal/COUNT read as i64
    // is where a naive port breaks on PostgreSQL).
    let count: i64 = pool
        .fetch_scalar::<i64>(
            "SELECT COUNT(*) FROM flow.workflows WHERE owner_id = $1 AND is_trashed = FALSE",
            params![owner],
        )
        .await
        .expect("count");
    assert_eq!(count, 1);

    // 2) Job queue — enqueue then the rows_affected claim, once and only once.
    let j1 = queue::enqueue(pool, id, owner, "manual", serde_json::json!({"n": 1}), 3)
        .await
        .expect("enqueue 1");
    let _j2 = queue::enqueue(pool, id, owner, "webhook", serde_json::json!({"n": 2}), 3)
        .await
        .expect("enqueue 2");

    let claimed = queue::claim_batch(pool, "worker-test", 10).await.expect("claim");
    assert_eq!(claimed.len(), 2, "both pending jobs claimed");
    assert!(claimed.iter().all(|j| j.status == "running"));
    assert!(claimed.iter().all(|j| j.attempt == 1));
    // trigger_data survived the JSON round-trip.
    assert!(claimed.iter().any(|j| j.trigger_data.get("n").and_then(|v| v.as_i64()) == Some(1)));

    // A second claim finds nothing: the guard (status='pending') made the first
    // claim exclusive — no SKIP LOCKED anywhere.
    let again = queue::claim_batch(pool, "worker-test", 10).await.expect("claim again");
    assert!(again.is_empty(), "already-claimed jobs are not re-claimed");

    queue::mark_done(pool, j1).await.expect("mark done");
    let done: Option<String> = pool
        .fetch_optional_scalar::<String>("SELECT status FROM flow.jobs WHERE id = $1", params![j1])
        .await
        .expect("job status");
    assert_eq!(done.as_deref(), Some("done"));

    // 3) Credentials — the encrypted BYTEA/BLOB columns round-trip as bytes.
    let cid = new_id();
    let data = vec![0xDEu8, 0xAD, 0xBE, 0xEF, 0x00, 0x11];
    let nonce = vec![1u8; 12];
    let now = chrono::Utc::now();
    pool.execute(
        "INSERT INTO flow.credentials (id, owner_id, name, type, data, nonce, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        params![cid, owner, "Ma clé", "anthropicApi", data.clone(), nonce.clone(), now, now],
    )
    .await
    .expect("insert credential");
    let cred = pool
        .fetch_optional_as::<Credential>(
            "SELECT * FROM flow.credentials WHERE id = $1 AND owner_id = $2",
            params![cid, owner],
        )
        .await
        .expect("get credential")
        .expect("credential present");
    assert_eq!(cred.type_id, "anthropicApi");
    assert_eq!(cred.data, data);
    assert_eq!(cred.nonce, nonce);

    // 4) Dialect upsert — the email-trigger state path (insert, then conflict).
    let node_id = "trigger-1";
    let upsert = pool.backend().upsert(
        "flow.email_trigger_state",
        &["workflow_id", "node_id"],
        &[Assign::Incoming("state"), Assign::Incoming("updated_at")],
    );
    let sql = format!(
        "INSERT INTO flow.email_trigger_state (workflow_id, node_id, state, updated_at) \
         VALUES ($1, $2, $3, $4){upsert}"
    );
    pool.execute(&sql, params![id, node_id, serde_json::json!({"last_uid": 1}), chrono::Utc::now()])
        .await
        .expect("insert email state");
    pool.execute(&sql, params![id, node_id, serde_json::json!({"last_uid": 42}), chrono::Utc::now()])
        .await
        .expect("upsert email state");
    let state: serde_json::Value = pool
        .fetch_scalar::<serde_json::Value>(
            "SELECT state FROM flow.email_trigger_state WHERE workflow_id = $1 AND node_id = $2",
            params![id, node_id],
        )
        .await
        .expect("read email state");
    assert_eq!(state.get("last_uid").and_then(|v| v.as_i64()), Some(42));
    let rows: i64 = pool
        .fetch_scalar::<i64>(
            "SELECT COUNT(*) FROM flow.email_trigger_state WHERE workflow_id = $1",
            params![id],
        )
        .await
        .expect("count state");
    assert_eq!(rows, 1, "upsert updated in place, did not duplicate");

    // 5) Cascade: deleting the workflow removes its jobs and state (FK CASCADE).
    pool.execute("DELETE FROM flow.workflows WHERE id = $1", params![id])
        .await
        .expect("delete workflow");
    assert!(get_workflow(pool, id, owner).await.is_none());
    let job_rows: i64 = pool
        .fetch_scalar::<i64>("SELECT COUNT(*) FROM flow.jobs WHERE workflow_id = $1", params![id])
        .await
        .expect("count jobs");
    assert_eq!(job_rows, 0, "jobs cascade-deleted with the workflow");
}

// ── Per-engine entry points ──────────────────────────────────────────────────

#[tokio::test]
async fn sqlite() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut settings = base_settings("sqlite");
    settings.path = Some(dir.path().to_string_lossy().to_string());
    let (pool, _guard) = migrated_pool(settings).await;
    run_all(&pool).await;
}

#[tokio::test]
async fn postgres() {
    let Ok(url) = std::env::var("KUBUNO_PG_TEST_URL") else {
        eprintln!("KUBUNO_PG_TEST_URL non défini — test PostgreSQL ignoré");
        return;
    };
    let mut settings = base_settings("postgres");
    settings.url = Some(url);
    let (pool, _guard) = migrated_pool(settings).await;
    run_all(&pool).await;
}

#[tokio::test]
async fn mysql() {
    let Ok(url) = std::env::var("KUBUNO_MYSQL_TEST_URL") else {
        eprintln!("KUBUNO_MYSQL_TEST_URL non défini — test MySQL/MariaDB ignoré");
        return;
    };
    let mut settings = base_settings("mysql");
    settings.url = Some(url);
    let (pool, _guard) = migrated_pool(settings).await;
    run_all(&pool).await;
}
