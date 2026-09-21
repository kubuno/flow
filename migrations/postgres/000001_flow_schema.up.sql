CREATE SCHEMA IF NOT EXISTS flow;

CREATE OR REPLACE FUNCTION flow.set_updated_at()
RETURNS TRIGGER AS $$
BEGIN NEW.updated_at = NOW(); RETURN NEW; END;
$$ LANGUAGE plpgsql;

-- =====================
-- WORKFLOWS
-- =====================
CREATE TABLE IF NOT EXISTS flow.workflows (
    id               UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner_id         UUID NOT NULL,
    name             VARCHAR(255) NOT NULL,
    description      TEXT,
    definition       JSONB NOT NULL DEFAULT '{"nodes":[],"edges":[]}',
    status           VARCHAR(10) NOT NULL DEFAULT 'inactive',
    execution_count  INTEGER NOT NULL DEFAULT 0,
    error_count      INTEGER NOT NULL DEFAULT 0,
    last_executed_at TIMESTAMPTZ,
    last_error       TEXT,
    tags             TEXT[] NOT NULL DEFAULT '{}',
    is_trashed       BOOLEAN NOT NULL DEFAULT FALSE,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_flow_wf_owner  ON flow.workflows(owner_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_flow_wf_status ON flow.workflows(status) WHERE status = 'active';

DROP TRIGGER IF EXISTS workflows_updated_at ON flow.workflows;
CREATE TRIGGER workflows_updated_at
    BEFORE UPDATE ON flow.workflows
    FOR EACH ROW EXECUTE FUNCTION flow.set_updated_at();

-- =====================
-- WEBHOOKS (tokens d'entrée)
-- =====================
CREATE TABLE IF NOT EXISTS flow.webhooks (
    token       VARCHAR(64) PRIMARY KEY,
    workflow_id UUID NOT NULL REFERENCES flow.workflows(id) ON DELETE CASCADE,
    node_id     VARCHAR(100) NOT NULL,
    owner_id    UUID NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_flow_webhooks_wf ON flow.webhooks(workflow_id);

-- =====================
-- FILE DE JOBS
-- =====================
CREATE TABLE IF NOT EXISTS flow.jobs (
    id             UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    workflow_id    UUID NOT NULL REFERENCES flow.workflows(id) ON DELETE CASCADE,
    owner_id       UUID NOT NULL,
    status         VARCHAR(10) NOT NULL DEFAULT 'pending',
    trigger_data   JSONB NOT NULL DEFAULT '{}',
    trigger_source VARCHAR(10) NOT NULL DEFAULT 'manual',
    priority       INTEGER NOT NULL DEFAULT 5,
    attempt        INTEGER NOT NULL DEFAULT 0,
    max_attempts   INTEGER NOT NULL DEFAULT 3,
    scheduled_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at     TIMESTAMPTZ,
    finished_at    TIMESTAMPTZ,
    last_error     TEXT,
    worker_id      VARCHAR(100),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_flow_jobs_queue ON flow.jobs(status, priority, scheduled_at)
    WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_flow_jobs_wf ON flow.jobs(workflow_id, created_at DESC);
