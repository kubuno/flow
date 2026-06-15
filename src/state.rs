use std::sync::Arc;

use sqlx::PgPool;

use crate::config::Settings;
use crate::files_client::FilesClient;
use crate::nodes::NodeRegistry;
use crate::runtime::core_proxy::CoreProxy;

#[derive(Clone)]
pub struct AppState {
    pub db:           PgPool,
    pub settings:     Arc<Settings>,
    pub proxy:        Arc<CoreProxy>,
    pub registry:     Arc<NodeRegistry>,
    pub files_client: Arc<FilesClient>,
}
