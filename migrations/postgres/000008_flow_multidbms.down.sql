-- Reverse of 000008: JSONB tags back to TEXT[], updated_at trigger restored.

CREATE OR REPLACE FUNCTION flow.set_updated_at()
RETURNS TRIGGER AS $$
BEGIN NEW.updated_at = NOW(); RETURN NEW; END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS workflows_updated_at ON flow.workflows;
CREATE TRIGGER workflows_updated_at
    BEFORE UPDATE ON flow.workflows
    FOR EACH ROW EXECUTE FUNCTION flow.set_updated_at();

ALTER TABLE flow.workflows ALTER COLUMN tags DROP DEFAULT;
ALTER TABLE flow.workflows ALTER COLUMN tags TYPE TEXT[] USING
    ARRAY(SELECT jsonb_array_elements_text(tags));
ALTER TABLE flow.workflows ALTER COLUMN tags SET DEFAULT '{}';
