ALTER TABLE flow.workflows ADD COLUMN IF NOT EXISTS definition JSONB NOT NULL DEFAULT '{"nodes":[],"edges":[]}';
ALTER TABLE flow.workflows DROP COLUMN IF EXISTS file_id;
