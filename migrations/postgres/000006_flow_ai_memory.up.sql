-- Conversation memory for AI Agent nodes (window buffer). One row per
-- (workflow, session) holding the recent message list as JSON.
CREATE TABLE IF NOT EXISTS flow.ai_memory (
    workflow_id UUID NOT NULL,
    session_key VARCHAR(255) NOT NULL,
    messages    JSONB NOT NULL DEFAULT '[]',
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (workflow_id, session_key)
);
