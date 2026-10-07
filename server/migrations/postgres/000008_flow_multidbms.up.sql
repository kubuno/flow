-- PostgreSQL alignment for the runtime-dispatch (kubuno-db) port.
--
-- Two engine-portability changes the MySQL/SQLite sides declare from their
-- CREATE, applied here to the schema PostgreSQL already reached:
--
--   * tags: TEXT[] -> JSONB. Arrays exist in no other engine, so the column
--     becomes a JSON array of strings (read back via #[sqlx(json)], bound as a
--     Vec<String>). Existing values convert with to_jsonb().
--   * The updated_at trigger + its plpgsql function go away: neither MySQL nor
--     SQLite has plpgsql, so updated_at is now stamped in Rust at every write.

-- ── tags TEXT[] -> JSONB ──────────────────────────────────────────────────────
ALTER TABLE flow.workflows ALTER COLUMN tags DROP DEFAULT;
ALTER TABLE flow.workflows ALTER COLUMN tags TYPE JSONB USING to_jsonb(tags);
ALTER TABLE flow.workflows ALTER COLUMN tags SET DEFAULT '[]'::jsonb;

-- ── retire the updated_at trigger layer (updated_at is set in Rust now) ────────
DROP TRIGGER IF EXISTS workflows_updated_at ON flow.workflows;
DROP FUNCTION IF EXISTS flow.set_updated_at();
