-- ── Workflow : définition (nodes/edges) déplacée vers le module files (.kbflw) ─
-- La définition ne vit plus en base ; seule la référence file_id subsiste.

ALTER TABLE flow.workflows ADD COLUMN IF NOT EXISTS file_id UUID;
ALTER TABLE flow.workflows DROP COLUMN IF EXISTS definition;
