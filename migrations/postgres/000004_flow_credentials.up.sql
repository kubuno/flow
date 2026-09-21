-- Reusable credentials, AES-GCM encrypted at rest. The plaintext payload (a JSON
-- object of field → value) is never stored; only the ciphertext + per-row nonce.
CREATE TABLE IF NOT EXISTS flow.credentials (
    id         UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner_id   UUID NOT NULL,
    name       VARCHAR(255) NOT NULL,
    type       VARCHAR(100) NOT NULL,
    data       BYTEA NOT NULL,  -- AES-256-GCM ciphertext of the JSON payload
    nonce      BYTEA NOT NULL,  -- 12-byte GCM nonce
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_flow_credentials_owner ON flow.credentials(owner_id, type);
