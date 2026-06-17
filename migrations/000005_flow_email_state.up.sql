-- Per-trigger state for email (IMAP/POP3) triggers, so we only fire on NEW mail
-- received after activation (no mailbox mutation, no backlog flood).
--   IMAP: state = { "last_uid": <n> }
--   POP3: state = { "seen": ["<uidl>", …] }
CREATE TABLE IF NOT EXISTS flow.email_trigger_state (
    workflow_id UUID NOT NULL,
    node_id     VARCHAR(100) NOT NULL,
    state       JSONB NOT NULL DEFAULT '{}',
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (workflow_id, node_id)
);
