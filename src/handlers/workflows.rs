use axum::{
    extract::{Path, State},
    Json,
};
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

/// GET /workflows — liste des workflows de l'utilisateur (hors corbeille).
/// Vue liste : la définition complète n'est pas chargée (un placeholder vide est
/// renvoyé) — l'éditeur récupère le graphe réel via GET /workflows/:id.
pub async fn list(
    State(state): State<AppState>,
    user: FlowUserExt,
) -> Result<Json<Vec<Workflow>>> {
    let mut workflows = sqlx::query_as::<_, Workflow>(
        r#"SELECT * FROM flow.workflows
           WHERE owner_id = $1 AND is_trashed = FALSE
           ORDER BY updated_at DESC"#,
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;
    for wf in &mut workflows {
        wf.definition = cf::empty_definition();
    }
    Ok(Json(workflows))
}

/// POST /workflows — création.
pub async fn create(
    State(state): State<AppState>,
    user: FlowUserExt,
    Json(dto): Json<CreateWorkflowDto>,
) -> Result<Json<Workflow>> {
    dto.validate().map_err(|e| FlowError::Validation(e.to_string()))?;

    let definition = dto.definition.unwrap_or_else(cf::empty_definition);
    let tags = dto.tags.unwrap_or_default();

    // Définition → fichier .kbflw (dossier protégé Flow/).
    let file_id = cf::create_workflow_file(&state, user.id, &dto.name, definition.clone()).await?;

    let mut wf = sqlx::query_as::<_, Workflow>(
        r#"INSERT INTO flow.workflows (owner_id, name, description, file_id, tags)
           VALUES ($1, $2, $3, $4, $5) RETURNING *"#,
    )
    .bind(user.id)
    .bind(&dto.name)
    .bind(dto.description.as_deref())
    .bind(file_id)
    .bind(&tags)
    .fetch_one(&state.db)
    .await?;
    wf.definition = definition;
    Ok(Json(wf))
}

/// Charge un workflow possédé, sans peupler la définition (métadonnée seule).
async fn fetch_owned(state: &AppState, id: Uuid, owner: Uuid) -> Result<Workflow> {
    sqlx::query_as::<_, Workflow>("SELECT * FROM flow.workflows WHERE id = $1 AND owner_id = $2")
        .bind(id)
        .bind(owner)
        .fetch_optional(&state.db)
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
                sqlx::query("UPDATE flow.workflows SET name = $2 WHERE id = $1")
                    .bind(id).bind(&stem).execute(&state.db).await?;
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
    let id = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM flow.workflows WHERE file_id = $1 AND owner_id = $2",
    )
    .bind(dto.file_id).bind(user.id)
    .fetch_optional(&state.db).await?
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

    let mut wf = sqlx::query_as::<_, Workflow>(
        r#"UPDATE flow.workflows SET
            name = $2, description = $3, file_id = $4, tags = $5, status = $6, is_starred = $7
           WHERE id = $1 RETURNING *"#,
    )
    .bind(id)
    .bind(&name)
    .bind(description.as_deref())
    .bind(file_id)
    .bind(&tags)
    .bind(&status)
    .bind(is_starred)
    .fetch_one(&state.db)
    .await?;
    wf.definition = definition;

    // Nom modifié → renommer le fichier .kbflw (nom = nom du fichier). Best-effort.
    if name_changed && !name.trim().is_empty() {
        cf::rename_content_file(&state, user.id, file_id, &name, "kbflw").await;
    }

    Ok(Json(wf))
}

/// DELETE /workflows/:id — corbeille.
pub async fn delete(
    State(state): State<AppState>,
    user: FlowUserExt,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    fetch_owned(&state, id, user.id).await?;
    sqlx::query("UPDATE flow.workflows SET is_trashed = TRUE, status = 'inactive' WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "deleted": true })))
}

/// POST /workflows/:id/activate
pub async fn activate(
    State(state): State<AppState>,
    user: FlowUserExt,
    Path(id): Path<Uuid>,
) -> Result<Json<Workflow>> {
    fetch_owned(&state, id, user.id).await?;
    let wf = sqlx::query_as::<_, Workflow>(
        "UPDATE flow.workflows SET status = 'active' WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(wf))
}

/// POST /workflows/:id/deactivate
pub async fn deactivate(
    State(state): State<AppState>,
    user: FlowUserExt,
    Path(id): Path<Uuid>,
) -> Result<Json<Workflow>> {
    fetch_owned(&state, id, user.id).await?;
    let wf = sqlx::query_as::<_, Workflow>(
        "UPDATE flow.workflows SET status = 'inactive' WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(wf))
}

/// POST /workflows/:id/duplicate
pub async fn duplicate(
    State(state): State<AppState>,
    user: FlowUserExt,
    Path(id): Path<Uuid>,
) -> Result<Json<Workflow>> {
    let src = fetch_owned_full(&state, id, user.id).await?;
    let new_name = format!("{} (copie)", src.name);
    let new_file_id = cf::create_workflow_file(&state, user.id, &new_name, src.definition.clone()).await?;

    let mut wf = sqlx::query_as::<_, Workflow>(
        r#"INSERT INTO flow.workflows (owner_id, name, description, file_id, tags)
           VALUES ($1, $2, $3, $4, $5) RETURNING *"#,
    )
    .bind(user.id)
    .bind(&new_name)
    .bind(src.description.as_deref())
    .bind(new_file_id)
    .bind(&src.tags)
    .fetch_one(&state.db)
    .await?;
    wf.definition = src.definition;
    Ok(Json(wf))
}
