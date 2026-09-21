use std::sync::{Arc, RwLock};

use kubuno_db::DbPool;

use crate::config::{InstanceConfig, Settings};
use crate::files_client::FilesClient;
use crate::nodes::NodeRegistry;
use crate::runtime::core_proxy::CoreProxy;

#[derive(Clone)]
pub struct AppState {
    pub db:           DbPool,
    pub settings:     Arc<Settings>,
    pub proxy:        Arc<CoreProxy>,
    pub registry:     Arc<NodeRegistry>,
    pub files_client: Arc<FilesClient>,
    /// Admin-editable instance settings, refreshed in the background from the core.
    pub instance:     Arc<RwLock<InstanceConfig>>,
}

impl AppState {
    /// Snapshot of the current instance settings. Takes the read lock briefly and
    /// returns a cheap `Copy`, so callers never hold the lock across `.await`.
    pub fn instance(&self) -> InstanceConfig {
        *self.instance.read().unwrap()
    }
}
