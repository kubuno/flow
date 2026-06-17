use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{
    handlers::{credentials, execute, executions, health, import_export, mcp, nodes, webhooks, workflows},
    middleware::require_auth,
    state::AppState,
};

pub fn build(state: AppState) -> Router {
    let authed = Router::new()
        // Workflows
        .route("/workflows", get(workflows::list).post(workflows::create))
        .route("/workflows/open-by-file", post(workflows::open_by_file))
        .route("/workflows/:id", get(workflows::get).put(workflows::update).delete(workflows::delete))
        .route("/workflows/:id/activate",   post(workflows::activate))
        .route("/workflows/:id/deactivate", post(workflows::deactivate))
        .route("/workflows/:id/duplicate",  post(workflows::duplicate))
        // Exécution
        .route("/workflows/:id/execute", post(execute::execute))
        .route("/executions/:id/stream", get(execute::stream))
        .route("/workflows/:id/nodes/:node_id/test", post(execute::test_node))
        // Historique
        .route("/workflows/:id/executions", get(executions::list_for_workflow))
        .route("/executions/:id", get(executions::detail).delete(executions::delete))
        // Webhook (gestion)
        .route("/workflows/:id/webhook", post(webhooks::register))
        // Catalogue de nœuds
        .route("/nodes", get(nodes::catalog))
        .route("/expression-help", get(nodes::expression_help))
        .route("/nodes/:type", get(nodes::get))
        // Credentials
        .route("/credential-types", get(credentials::types))
        .route("/credentials/test", post(credentials::test))
        .route("/credentials", get(credentials::list).post(credentials::create))
        .route("/credentials/:id", axum::routing::put(credentials::update).delete(credentials::delete))
        // Import / Export
        .route("/workflows/:id/export", get(import_export::export))
        .route("/import", post(import_export::import))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth))
        .with_state(state.clone());

    // Routes publiques (sans JWT) : webhook entrant + health + MCP interne.
    // `/mcp/run` valide lui-même `X-Internal-Secret` (appel proxifié par le core).
    let public = Router::new()
        .route("/webhook/:token", get(webhooks::receive).post(webhooks::receive))
        .route("/mcp/run", post(mcp::run))
        .route("/health", get(health::health))
        .with_state(state);

    Router::new()
        .merge(public)
        .merge(authed)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
