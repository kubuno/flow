-- SQLite — `flow` is an ATTACHed database file, attached on every pooled
-- connection by kubuno-db, so the qualified names below resolve as they do on
-- the other two engines. This single file declares the FINAL shape the
-- PostgreSQL side reached across its 000001..000008 migrations.
--
-- Differences from PostgreSQL, and why:
--   * UUID -> BLOB, TIMESTAMPTZ -> TEXT (`%F %T%.f`, UTC), as sqlx encodes them.
--   * No DEFAULT on `id`: SQLite has no UUID generator; the process supplies it.
--   * TEXT[] / JSONB -> TEXT holding JSON (tags, trigger_data, messages, logs).
--   * BYTEA -> BLOB (credential ciphertext + nonce).
--   * BOOLEAN -> INTEGER (0/1). updated_at is stamped in Rust (no trigger).
--   * Foreign-key REFERENCES are unqualified (SQLite assumes the same database);
--     kubuno-db enables `PRAGMA foreign_keys`, so CASCADE deletes fire.

CREATE TABLE flow.workflows (
    id               BLOB    NOT NULL PRIMARY KEY,
    owner_id         BLOB    NOT NULL,
    name             TEXT    NOT NULL,
    description      TEXT,
    file_id          BLOB,
    status           TEXT    NOT NULL DEFAULT 'inactive',
    execution_count  INTEGER NOT NULL DEFAULT 0,
    error_count      INTEGER NOT NULL DEFAULT 0,
    last_executed_at TEXT,
    last_error       TEXT,
    tags             TEXT    NOT NULL,
    is_trashed       INTEGER NOT NULL DEFAULT 0,
    is_starred       INTEGER NOT NULL DEFAULT 0,
    created_at       TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now')),
    updated_at       TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now'))
);
CREATE INDEX flow.idx_flow_wf_owner  ON workflows(owner_id, updated_at);
CREATE INDEX flow.idx_flow_wf_status ON workflows(status);

CREATE TABLE flow.webhooks (
    token       TEXT NOT NULL PRIMARY KEY,
    workflow_id BLOB NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
    node_id     TEXT NOT NULL,
    owner_id    BLOB NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now'))
);
CREATE INDEX flow.idx_flow_webhooks_wf ON webhooks(workflow_id);

CREATE TABLE flow.jobs (
    id             BLOB    NOT NULL PRIMARY KEY,
    workflow_id    BLOB    NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
    owner_id       BLOB    NOT NULL,
    status         TEXT    NOT NULL DEFAULT 'pending',
    trigger_data   TEXT    NOT NULL,
    trigger_source TEXT    NOT NULL DEFAULT 'manual',
    priority       INTEGER NOT NULL DEFAULT 5,
    attempt        INTEGER NOT NULL DEFAULT 0,
    max_attempts   INTEGER NOT NULL DEFAULT 3,
    scheduled_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now')),
    started_at     TEXT,
    finished_at    TEXT,
    last_error     TEXT,
    worker_id      TEXT,
    created_at     TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now'))
);
CREATE INDEX flow.idx_flow_jobs_queue ON jobs(status, priority, scheduled_at);
CREATE INDEX flow.idx_flow_jobs_wf    ON jobs(workflow_id, created_at);

CREATE TABLE flow.executions (
    id             BLOB    NOT NULL PRIMARY KEY,
    job_id         BLOB    REFERENCES jobs(id) ON DELETE SET NULL,
    workflow_id    BLOB    NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
    owner_id       BLOB    NOT NULL,
    status         TEXT    NOT NULL DEFAULT 'running',
    trigger_source TEXT    NOT NULL DEFAULT 'manual',
    trigger_data   TEXT    NOT NULL,
    duration_ms    INTEGER,
    nodes_executed INTEGER NOT NULL DEFAULT 0,
    nodes_total    INTEGER NOT NULL DEFAULT 0,
    error_message  TEXT,
    started_at     TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now')),
    finished_at    TEXT
);
CREATE INDEX flow.idx_flow_exec_wf     ON executions(workflow_id, started_at);
CREATE INDEX flow.idx_flow_exec_owner  ON executions(owner_id, started_at);
CREATE INDEX flow.idx_flow_exec_status ON executions(status, started_at);

CREATE TABLE flow.node_logs (
    id                BLOB    NOT NULL PRIMARY KEY,
    execution_id      BLOB    NOT NULL REFERENCES executions(id) ON DELETE CASCADE,
    node_id           TEXT    NOT NULL,
    node_type         TEXT    NOT NULL,
    node_name         TEXT,
    status            TEXT    NOT NULL,
    input_data        TEXT,
    output_data       TEXT,
    error_message     TEXT,
    error_stack       TEXT,
    duration_ms       INTEGER,
    attempt           INTEGER NOT NULL DEFAULT 1,
    proxy_duration_ms INTEGER,
    proxy_status_code INTEGER,
    executed_at       TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now'))
);
CREATE INDEX flow.idx_flow_nodelog_exec ON node_logs(execution_id, executed_at);

CREATE TABLE flow.credentials (
    id         BLOB    NOT NULL PRIMARY KEY,
    owner_id   BLOB    NOT NULL,
    name       TEXT    NOT NULL,
    type       TEXT    NOT NULL,
    data       BLOB    NOT NULL,
    nonce      BLOB    NOT NULL,
    created_at TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now')),
    updated_at TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now'))
);
CREATE INDEX flow.idx_flow_credentials_owner ON credentials(owner_id, type);

CREATE TABLE flow.email_trigger_state (
    workflow_id BLOB NOT NULL,
    node_id     TEXT NOT NULL,
    state       TEXT NOT NULL,
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now')),
    PRIMARY KEY (workflow_id, node_id)
);

CREATE TABLE flow.ai_memory (
    workflow_id BLOB NOT NULL,
    session_key TEXT NOT NULL,
    messages    TEXT NOT NULL,
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now')),
    PRIMARY KEY (workflow_id, session_key)
);
