use axum::{
    extract::{Path, State},
    Json,
};

use crate::{
    errors::{FlowError, Result},
    nodes::trait_::NodeMeta,
    state::AppState,
};

/// GET /nodes — catalogue de tous les nœuds disponibles + métadonnées.
pub async fn catalog(State(state): State<AppState>) -> Json<Vec<NodeMeta>> {
    Json(state.registry.catalog())
}

/// GET /nodes/:type — métadonnées d'un nœud.
pub async fn get(
    State(state): State<AppState>,
    Path(node_type): Path<String>,
) -> Result<Json<NodeMeta>> {
    state
        .registry
        .meta(&node_type)
        .map(Json)
        .ok_or_else(|| FlowError::NotFound(format!("Nœud inconnu : {node_type}")))
}
