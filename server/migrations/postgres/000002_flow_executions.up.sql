CREATE TABLE IF NOT EXISTS flow.executions (
    id             UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    job_id         UUID REFERENCES flow.jobs(id) ON DELETE SET NULL,
    workflow_id    UUID NOT NULL REFERENCES flow.workflows(id) ON DELETE CASCADE,
    owner_id       UUID NOT NULL,
    status         VARCHAR(10) NOT NULL DEFAULT 'running',
    trigger_source VARCHAR(10) NOT NULL DEFAULT 'manual',
    trigger_data   JSONB NOT NULL DEFAULT '{}',
    duration_ms    INTEGER,
    nodes_executed INTEGER NOT NULL DEFAULT 0,
    nodes_total    INTEGER NOT NULL DEFAULT 0,
    error_message  TEXT,
    started_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    finished_at    TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_flow_exec_wf     ON flow.executions(workflow_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_flow_exec_owner  ON flow.executions(owner_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_flow_exec_status ON flow.executions(status, started_at DESC);

CREATE TABLE IF NOT EXISTS flow.node_logs (
    id                UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    execution_id      UUID NOT NULL REFERENCES flow.executions(id) ON DELETE CASCADE,
    node_id           VARCHAR(100) NOT NULL,
    node_type         VARCHAR(100) NOT NULL,
    node_name         VARCHAR(255),
    status            VARCHAR(10) NOT NULL,
    input_data        JSONB,
    output_data       JSONB,
    error_message     TEXT,
    error_stack       TEXT,
    duration_ms       INTEGER,
    attempt           INTEGER NOT NULL DEFAULT 1,
    proxy_duration_ms INTEGER,
    proxy_status_code SMALLINT,
    executed_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_flow_nodelog_exec ON flow.node_logs(execution_id, executed_at);
