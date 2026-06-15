//! Stockage du CONTENU des workflows dans le module `files` (plus en base).
//!
//! Format Kubuno propre à Flow — MIME `application/vnd.kubuno.flow+json`,
//! extension `.kbflw`, JSON gzippé. La base ne garde que la référence `file_id`
//! + la métadonnée (nom, statut, compteurs…).
//!
//! Les workflows vivent dans le dossier **protégé** `Flow/` (non supprimable).

use bytes::Bytes;
use serde_json::{json, Value};
use std::io::{Read as _, Write as _};
use uuid::Uuid;

use crate::{errors::FlowError, state::AppState};

pub const FLOW_MIME: &str = "application/vnd.kubuno.flow+json";

// ── Compression (gzip) ──────────────────────────────────────────────────────

fn gzip(raw: &[u8]) -> Result<Vec<u8>, FlowError> {
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(raw).map_err(|e| FlowError::Internal(anyhow::anyhow!(e)))?;
    enc.finish().map_err(|e| FlowError::Internal(anyhow::anyhow!(e)))
}

fn gunzip(raw: &[u8]) -> Result<Vec<u8>, FlowError> {
    if raw.len() >= 2 && raw[0] == 0x1f && raw[1] == 0x8b {
        let mut dec = flate2::read::GzDecoder::new(raw);
        let mut out = Vec::new();
        dec.read_to_end(&mut out).map_err(|e| FlowError::Internal(anyhow::anyhow!(e)))?;
        Ok(out)
    } else {
        Ok(raw.to_vec())
    }
}

// ── Définition du workflow {nodes, edges} ───────────────────────────────────

pub fn empty_definition() -> Value {
    json!({ "nodes": [], "edges": [] })
}

pub fn definition_content_from(definition: Value) -> Value {
    json!({ "version": 1, "definition": definition })
}

pub fn extract_definition(content: &Value) -> Value {
    content.get("definition").cloned().unwrap_or_else(empty_definition)
}

fn kb_file_name(title: &str) -> String {
    let base = std::path::Path::new(title).file_stem().and_then(|s| s.to_str()).unwrap_or(title);
    let base = if base.trim().is_empty() { "Sans titre" } else { base.trim() };
    format!("{base}.kbflw")
}

/// Crée le fichier de contenu d'un workflow dans le dossier protégé `Flow/`.
pub async fn create_workflow_file(
    state: &AppState, user_id: Uuid, title: &str, definition: Value,
) -> Result<Uuid, FlowError> {
    // protect = true → le dossier Flow/ ne peut pas être supprimé par l'utilisateur.
    // icône Lucide "Workflow" (dossier appartenant au module flow).
    let folder = state.files_client.ensure_folder_path(user_id, "Flow", true, Some("Workflow")).await
        .map_err(FlowError::Internal)?;
    let content = definition_content_from(definition);
    let raw = serde_json::to_vec(&content).map_err(|e| FlowError::Internal(anyhow::anyhow!(e)))?;
    let gz  = gzip(&raw)?;
    let file = state.files_client.create_file_with_content(
        user_id, Some(folder.id), &kb_file_name(title), FLOW_MIME, Bytes::from(gz),
        Some(json!({ "module": "flow", "subtype": "workflow" })), false,
    ).await.map_err(FlowError::Internal)?;
    Ok(file.id)
}

pub async fn read_definition(state: &AppState, user_id: Uuid, file_id: Uuid) -> Result<Value, FlowError> {
    let (_info, raw) = state.files_client.get_file_content(user_id, file_id).await
        .map_err(FlowError::Internal)?;
    let json = gunzip(&raw)?;
    let content = serde_json::from_slice::<Value>(&json)
        .map_err(|e| FlowError::Internal(anyhow::anyhow!("définition illisible: {e}")))?;
    Ok(extract_definition(&content))
}

pub async fn write_definition(state: &AppState, user_id: Uuid, file_id: Uuid, definition: Value) -> Result<(), FlowError> {
    let content = definition_content_from(definition);
    let raw = serde_json::to_vec(&content).map_err(|e| FlowError::Internal(anyhow::anyhow!(e)))?;
    let gz  = gzip(&raw)?;
    state.files_client.update_file_content(user_id, file_id, Bytes::from(gz)).await
        .map_err(FlowError::Internal).map(|_| ())
}


// ── Noms de fichiers : DÉLÉGUÉS à la face client du module `files` ────────────
pub fn strip_ext(name: &str) -> String { crate::files_client::strip_ext(name) }
/// Nom complet du fichier .kb*** (best-effort).
pub async fn file_name(state: &crate::state::AppState, owner_id: uuid::Uuid, file_id: uuid::Uuid) -> Option<String> {
    state.files_client.get_file_meta(owner_id, file_id).await.ok().map(|i| i.name)
}
/// Renomme le fichier .kb*** pour qu'il porte `<title>.<ext>` (titre = nom). Best-effort.
pub async fn rename_content_file(state: &crate::state::AppState, owner_id: uuid::Uuid, file_id: uuid::Uuid, title: &str, ext: &str) {
    crate::files_client::set_title(&state.files_client, owner_id, file_id, title, ext).await
}
