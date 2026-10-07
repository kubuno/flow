-- MySQL / MariaDB — the `flow` database is created by kubuno-db's schema setup
-- before the migrator runs, so there is no CREATE DATABASE here. This single
-- file declares the FINAL shape the PostgreSQL side reached across its
-- 000001..000008 migrations (definition moved to `.kbflw` files, credentials,
-- email/ai state, starred flag, tags as JSON).
--
-- Differences from PostgreSQL, and why:
--   * UUID -> BINARY(16): what sqlx encodes a `uuid::Uuid` as on MySQL.
--   * No DEFAULT on `id`: MySQL has no gen_random_uuid() and no RETURNING, so
--     the process supplies every primary key.
--   * TIMESTAMPTZ -> DATETIME(6); every value written is UTC (the pool pins
--     `time_zone = '+00:00'`). updated_at is stamped in Rust (no trigger).
--   * TEXT[] / JSONB -> JSON (tags, trigger_data, messages, node log payloads).
--   * BYTEA -> LONGBLOB / VARBINARY (credential ciphertext + nonce).
--   * Partial indexes (WHERE ...) become plain indexes (MySQL has none).
--   * utf8mb4_bin so tokens/keys stay case-sensitive.

CREATE TABLE workflows (
    id               BINARY(16)   NOT NULL PRIMARY KEY,
    owner_id         BINARY(16)   NOT NULL,
    name             VARCHAR(255) NOT NULL,
    description      TEXT         NULL,
    file_id          BINARY(16)   NULL,
    status           VARCHAR(10)  NOT NULL DEFAULT 'inactive',
    execution_count  INT          NOT NULL DEFAULT 0,
    error_count      INT          NOT NULL DEFAULT 0,
    last_executed_at DATETIME(6)  NULL,
    last_error       TEXT         NULL,
    tags             JSON         NOT NULL,
    is_trashed       BOOLEAN      NOT NULL DEFAULT FALSE,
    is_starred       BOOLEAN      NOT NULL DEFAULT FALSE,
    created_at       DATETIME(6)  NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at       DATETIME(6)  NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
) DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;
CREATE INDEX idx_flow_wf_owner  ON workflows(owner_id, updated_at);
CREATE INDEX idx_flow_wf_status ON workflows(status);

CREATE TABLE webhooks (
    token       VARCHAR(64)  NOT NULL PRIMARY KEY,
    workflow_id BINARY(16)   NOT NULL,
    node_id     VARCHAR(100) NOT NULL,
    owner_id    BINARY(16)   NOT NULL,
    created_at  DATETIME(6)  NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    FOREIGN KEY (workflow_id) REFERENCES workflows(id) ON DELETE CASCADE
) DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;
CREATE INDEX idx_flow_webhooks_wf ON webhooks(workflow_id);

CREATE TABLE jobs (
    id             BINARY(16)   NOT NULL PRIMARY KEY,
    workflow_id    BINARY(16)   NOT NULL,
    owner_id       BINARY(16)   NOT NULL,
    status         VARCHAR(10)  NOT NULL DEFAULT 'pending',
    trigger_data   JSON         NOT NULL,
    trigger_source VARCHAR(10)  NOT NULL DEFAULT 'manual',
    priority       INT          NOT NULL DEFAULT 5,
    attempt        INT          NOT NULL DEFAULT 0,
    max_attempts   INT          NOT NULL DEFAULT 3,
    scheduled_at   DATETIME(6)  NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    started_at     DATETIME(6)  NULL,
    finished_at    DATETIME(6)  NULL,
    last_error     TEXT         NULL,
    worker_id      VARCHAR(100) NULL,
    created_at     DATETIME(6)  NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    FOREIGN KEY (workflow_id) REFERENCES workflows(id) ON DELETE CASCADE
) DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;
CREATE INDEX idx_flow_jobs_queue ON jobs(status, priority, scheduled_at);
CREATE INDEX idx_flow_jobs_wf    ON jobs(workflow_id, created_at);

CREATE TABLE executions (
    id             BINARY(16)  NOT NULL PRIMARY KEY,
    job_id         BINARY(16)  NULL,
    workflow_id    BINARY(16)  NOT NULL,
    owner_id       BINARY(16)  NOT NULL,
    status         VARCHAR(10) NOT NULL DEFAULT 'running',
    trigger_source VARCHAR(10) NOT NULL DEFAULT 'manual',
    trigger_data   JSON        NOT NULL,
    duration_ms    INT         NULL,
    nodes_executed INT         NOT NULL DEFAULT 0,
    nodes_total    INT         NOT NULL DEFAULT 0,
    error_message  TEXT        NULL,
    started_at     DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    finished_at    DATETIME(6) NULL,
    FOREIGN KEY (job_id)      REFERENCES jobs(id)      ON DELETE SET NULL,
    FOREIGN KEY (workflow_id) REFERENCES workflows(id) ON DELETE CASCADE
) DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;
CREATE INDEX idx_flow_exec_wf     ON executions(workflow_id, started_at);
CREATE INDEX idx_flow_exec_owner  ON executions(owner_id, started_at);
CREATE INDEX idx_flow_exec_status ON executions(status, started_at);

CREATE TABLE node_logs (
    id                BINARY(16)   NOT NULL PRIMARY KEY,
    execution_id      BINARY(16)   NOT NULL,
    node_id           VARCHAR(100) NOT NULL,
    node_type         VARCHAR(100) NOT NULL,
    node_name         VARCHAR(255) NULL,
    status            VARCHAR(10)  NOT NULL,
    input_data        JSON         NULL,
    output_data       JSON         NULL,
    error_message     TEXT         NULL,
    error_stack       TEXT         NULL,
    duration_ms       INT          NULL,
    attempt           INT          NOT NULL DEFAULT 1,
    proxy_duration_ms INT          NULL,
    proxy_status_code SMALLINT     NULL,
    executed_at       DATETIME(6)  NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    FOREIGN KEY (execution_id) REFERENCES executions(id) ON DELETE CASCADE
) DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;
CREATE INDEX idx_flow_nodelog_exec ON node_logs(execution_id, executed_at);

CREATE TABLE credentials (
    id         BINARY(16)    NOT NULL PRIMARY KEY,
    owner_id   BINARY(16)    NOT NULL,
    name       VARCHAR(255)  NOT NULL,
    `type`     VARCHAR(100)  NOT NULL,
    data       LONGBLOB      NOT NULL,
    nonce      VARBINARY(16) NOT NULL,
    created_at DATETIME(6)   NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at DATETIME(6)   NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
) DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;
CREATE INDEX idx_flow_credentials_owner ON credentials(owner_id, `type`);

CREATE TABLE email_trigger_state (
    workflow_id BINARY(16)   NOT NULL,
    node_id     VARCHAR(100) NOT NULL,
    state       JSON         NOT NULL,
    updated_at  DATETIME(6)  NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (workflow_id, node_id)
) DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;

CREATE TABLE ai_memory (
    workflow_id BINARY(16)   NOT NULL,
    session_key VARCHAR(255) NOT NULL,
    messages    JSON         NOT NULL,
    updated_at  DATETIME(6)  NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (workflow_id, session_key)
) DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;
