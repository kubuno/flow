use axum::{
    extract::{Path, State},
    Json,
};
use kubuno_db::{new_id, params};
use serde_json::{json, Value};
use uuid::Uuid;
use validator::Validate;

use crate::{
    errors::{FlowError, Result},
    middleware::FlowUserExt,
    models::workflow::{CreateWorkflowDto, UpdateWorkflowDto, Workflow},
    services::content_files as cf,
    state::AppState,
};

#[derive(Debug, serde::Deserialize)]
pub struct ListQuery {
    /// `true` lists the trash instead of the live workflows. Without it, tools
    /// auditing which files still belong to a workflow cannot see the trashed
    /// ones and would mistake their files for orphans.
    pub trashed: Option<bool>,
}

/// GET /workflows[?trashed=true] — liste des workflows de l'utilisateur.
/// Vue liste : la définition complète n'est pas chargée (un placeholder vide est
/// renvoyé) — l'éditeur récupère le graphe réel via GET /workflows/:id.
pub async fn list(
    State(state): State<AppState>,
    user: FlowUserExt,
    axum::extract::Query(q): axum::extract::Query<ListQuery>,
) -> Result<Json<Vec<Workflow>>> {
    let mut workflows = state.db.fetch_all_as::<Workflow>(
        "SELECT * FROM flow.workflows \
           WHERE owner_id = $1 AND is_trashed = $2 \
           ORDER BY updated_at DESC",
        params![user.id, q.trashed.unwrap_or(false)],
    )
    .await?;
    for wf in &mut workflows {
        wf.definition = cf::empty_definition();
    }
    Ok(Json(workflows))
}

/// Refuses a new workflow when the owner already sits at the instance ceiling.
///
/// Checked before the `.kbflw` file is created, so a refused creation leaves
/// nothing behind in Drive. `0` means unlimited and skips the count entirely.
pub(crate) async fn enforce_workflow_quota(state: &AppState, owner: Uuid) -> Result<()> {
    let max = state.instance().max_workflows_per_user;
    if max <= 0 {
        return Ok(());
    }
    let owned = state.db.fetch_scalar::<i64>(
        "SELECT COUNT(*) FROM flow.workflows WHERE owner_id = $1 AND is_trashed = FALSE",
        params![owner],
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, owner = %owner, "Comptage des workflows pour le quota");
        FlowError::Database(e)
    })?;

    if owned >= max as i64 {
        return Err(FlowError::Validation(format!(
            "Quota atteint : {max} workflows au maximum par utilisateur sur cette instance."
        )));
    }
    Ok(())
}

/// POST /workflows — création.
pub async fn create(
    State(state): State<AppState>,
    user: FlowUserExt,
    Json(dto): Json<CreateWorkflowDto>,
) -> Result<Json<Workflow>> {
    dto.validate().map_err(|e| FlowError::Validation(e.to_string()))?;
    enforce_workflow_quota(&state, user.id).await?;

    let definition = dto.definition.unwrap_or_else(cf::empty_definition);
    let tags = dto.tags.unwrap_or_default();

    // Définition → fichier .kbflw (dossier protégé Flow/).
    let file_id = cf::create_workflow_file(&state, user.id, &dto.name, definition.clone()).await?;

    // id minté en Rust + relecture (MySQL n'a pas de RETURNING). tags est lié
    // explicitement (NOT NULL, sans défaut sur MySQL/SQLite).
    let id = new_id();
    let now = chrono::Utc::now();
    state.db.execute(
        "INSERT INTO flow.workflows (id, owner_id, name, description, file_id, tags, created_at, updated_at) \
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        params![id, user.id, &dto.name, dto.description.as_deref(), file_id, &tags, now, now],
    )
    .await?;
    let mut wf = fetch_owned(&state, id, user.id).await?;
    wf.definition = definition;
    Ok(Json(wf))
}

/// Charge un workflow possédé, sans peupler la définition (métadonnée seule).
async fn fetch_owned(state: &AppState, id: Uuid, owner: Uuid) -> Result<Workflow> {
    state.db.fetch_optional_as::<Workflow>(
        "SELECT * FROM flow.workflows WHERE id = $1 AND owner_id = $2",
        params![id, owner],
    )
    .await?
    .ok_or_else(|| FlowError::NotFound("Workflow introuvable".into()))
}

/// Charge un workflow possédé en peuplant la définition depuis le fichier .kbflw.
async fn fetch_owned_full(state: &AppState, id: Uuid, owner: Uuid) -> Result<Workflow> {
    let mut wf = fetch_owned(state, id, owner).await?;
    wf.definition = match wf.file_id {
        Some(fid) => cf::read_definition(state, owner, fid).await.unwrap_or_else(|_| cf::empty_definition()),
        None => cf::empty_definition(),
    };
    Ok(wf)
}

/// GET /workflows/:id
pub async fn get(
    State(state): State<AppState>,
    user: FlowUserExt,
    Path(id): Path<Uuid>,
) -> Result<Json<Workflow>> {
    let mut wf = fetch_owned_full(&state, id, user.id).await?;
    // Nom = nom du fichier .kbflw (sans extension) ; self-heal si renommé ailleurs.
    if let Some(fid) = wf.file_id {
        if let Some(fname) = cf::file_name(&state, user.id, fid).await {
            let stem = cf::strip_ext(&fname);
            if !stem.is_empty() && stem != wf.name {
                state.db.execute(
                    "UPDATE flow.workflows SET name = $1, updated_at = $2 WHERE id = $3",
                    params![&stem, chrono::Utc::now(), id],
                ).await?;
                wf.name = stem;
            }
        }
    }
    Ok(Json(wf))
}

#[derive(serde::Deserialize)]
pub struct OpenByFileDto {
    pub file_id: Uuid,
}

/// POST /workflows/open-by-file — résout un workflow depuis l'id de fichier .kbflw.
pub async fn open_by_file(
    State(state): State<AppState>,
    user: FlowUserExt,
    Json(dto): Json<OpenByFileDto>,
) -> Result<Json<Workflow>> {
    let id = state.db.fetch_optional_scalar::<Uuid>(
        "SELECT id FROM flow.workflows WHERE file_id = $1 AND owner_id = $2",
        params![dto.file_id, user.id],
    )
    .await?
    .ok_or_else(|| FlowError::NotFound("Aucun workflow lié à ce fichier".into()))?;

    Ok(Json(fetch_owned_full(&state, id, user.id).await?))
}

/// PUT /workflows/:id — sauvegarde (nom/description/définition/tags/statut).
pub async fn update(
    State(state): State<AppState>,
    user: FlowUserExt,
    Path(id): Path<Uuid>,
    Json(dto): Json<UpdateWorkflowDto>,
) -> Result<Json<Workflow>> {
    dto.validate().map_err(|e| FlowError::Validation(e.to_string()))?;
    let existing = fetch_owned(&state, id, user.id).await?;

    // Ne renommer que si le nom CHANGE réellement (le frontend renvoie le titre à
    // chaque autosave) — évite un rename .kbflw inutile à chaque sauvegarde.
    let name_changed = dto.name.as_deref().map(str::trim).is_some_and(|n| n != existing.name);
    let name = dto.name.unwrap_or(existing.name);
    let description = dto.description.or(existing.description);
    let tags = dto.tags.unwrap_or(existing.tags);
    let status = match dto.status {
        Some(s) if s == "active" || s == "inactive" => s,
        Some(_) => return Err(FlowError::Validation("status invalide".into())),
        None => existing.status,
    };
    // Partial update: keep the current value unless the DTO carries a new one.
    let is_starred = dto.is_starred.unwrap_or(existing.is_starred);

    // Définition modifiée → écrite dans le fichier (créé si absent).
    let (file_id, definition) = match dto.definition {
        Some(def) => {
            let fid = match existing.file_id {
                Some(fid) => { cf::write_definition(&state, user.id, fid, def.clone()).await?; fid }
                None => cf::create_workflow_file(&state, user.id, &name, def.clone()).await?,
            };
            (fid, def)
        }
        None => match existing.file_id {
            Some(fid) => (fid, cf::read_definition(&state, user.id, fid).await.unwrap_or_else(|_| cf::empty_definition())),
            None => {
                let def = cf::empty_definition();
                (cf::create_workflow_file(&state, user.id, &name, def.clone()).await?, def)
            }
        },
    };

    state.db.execute(
        "UPDATE flow.workflows SET \
            name = $1, description = $2, file_id = $3, tags = $4, status = $5, is_starred = $6, updated_at = $7 \
           WHERE id = $8 AND owner_id = $9",
        params![
            &name, description.as_deref(), file_id, &tags, &status, is_starred,
            chrono::Utc::now(), id, user.id
        ],
    )
    .await?;
    let mut wf = fetch_owned(&state, id, user.id).await?;
    wf.definition = definition;

    // Nom modifié → renommer le fichier .kbflw (nom = nom du fichier). Best-effort.
    if name_changed && !name.trim().is_empty() {
        cf::rename_content_file(&state, user.id, file_id, &name, "kbflw").await;
    }

    Ok(Json(wf))
}

/// DELETE /workflows/:id — permanent deletion, as the UI promises.
///
/// This used to only flag the row `is_trashed`, but the module offers neither a
/// trash view nor a restore action: the workflow vanished from the list while its
/// `.kbflw` file stayed in Drive and still opened, resurrecting something the user
/// believed deleted. Deleting for real keeps Drive and the list in agreement.
/// Executions, webhooks and jobs go with it (ON DELETE CASCADE).
pub async fn delete(
    State(state): State<AppState>,
    user: FlowUserExt,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let wf = fetch_owned(&state, id, user.id).await?;
    state.db.execute(
        "DELETE FROM flow.workflows WHERE id = $1 AND owner_id = $2",
        params![id, user.id],
    )
    .await?;
    if let Some(fid) = wf.file_id {
        if let Err(e) = state.files_client.delete_file(user.id, fid).await {
            tracing::warn!(workflow_id = %id, file_id = %fid, error = %e,
                           "Drive: delete_file failed — file left orphaned");
        }
    }
    Ok(Json(json!({ "deleted": true })))
}

/// POST /workflows/:id/activate
pub async fn activate(
    State(state): State<AppState>,
    user: FlowUserExt,
    Path(id): Path<Uuid>,
) -> Result<Json<Workflow>> {
    fetch_owned(&state, id, user.id).await?;
    state.db.execute(
        "UPDATE flow.workflows SET status = 'active', updated_at = $1 WHERE id = $2 AND owner_id = $3",
        params![chrono::Utc::now(), id, user.id],
    )
    .await?;
    let wf = fetch_owned(&state, id, user.id).await?;
    Ok(Json(wf))
}

/// POST /workflows/:id/deactivate
pub async fn deactivate(
    State(state): State<AppState>,
    user: FlowUserExt,
    Path(id): Path<Uuid>,
) -> Result<Json<Workflow>> {
    fetch_owned(&state, id, user.id).await?;
    state.db.execute(
        "UPDATE flow.workflows SET status = 'inactive', updated_at = $1 WHERE id = $2 AND owner_id = $3",
        params![chrono::Utc::now(), id, user.id],
    )
    .await?;
    let wf = fetch_owned(&state, id, user.id).await?;
    Ok(Json(wf))
}

/// POST /workflows/:id/duplicate
pub async fn duplicate(
    State(state): State<AppState>,
    user: FlowUserExt,
    Path(id): Path<Uuid>,
) -> Result<Json<Workflow>> {
    enforce_workflow_quota(&state, user.id).await?;
    let src = fetch_owned_full(&state, id, user.id).await?;
    let new_name = format!("{} (copie)", src.name);
    let new_file_id = cf::create_workflow_file(&state, user.id, &new_name, src.definition.clone()).await?;

    let id = new_id();
    let now = chrono::Utc::now();
    state.db.execute(
        "INSERT INTO flow.workflows (id, owner_id, name, description, file_id, tags, created_at, updated_at) \
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        params![id, user.id, &new_name, src.description.as_deref(), new_file_id, &src.tags, now, now],
    )
    .await?;
    let mut wf = fetch_owned(&state, id, user.id).await?;
    wf.definition = src.definition;
    Ok(Json(wf))
}
