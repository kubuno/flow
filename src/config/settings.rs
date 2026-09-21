use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub server:    ServerSettings,
    pub core:      CoreSettings,
    pub database:  DatabaseSettings,
    pub runtime:   RuntimeSettings,
    pub queue:     QueueSettings,
    pub code_node: CodeNodeSettings,
    pub logging:   LoggingSettings,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerSettings {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CoreSettings {
    pub url:             String,
    pub internal_secret: String,
    #[serde(default = "default_files_url")]
    pub files_url:       String,
}

fn default_files_url() -> String { "http://127.0.0.1:3101".to_string() }

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeSettings {
    pub worker_count:           u32,
    pub execution_timeout_secs: u64,
    pub node_timeout_secs:      u64,
    pub max_retries:            i32,
    pub retry_backoff_ms:       u64,
    pub max_execution_history:  i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueueSettings {
    pub poll_interval_ms: u64,
    pub batch_size:       i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CodeNodeSettings {
    pub timeout_secs:    u64,
    pub memory_limit_mb: u32,
}

/// The `[database]` section is owned by kubuno-db: which of its fields matter
/// depends on the engine the administrator chooses at run time, and the pool is
/// opened by `kubuno_db::connect`.
pub use kubuno_db::DbSettings as DatabaseSettings;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    Pretty,
    Json,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingSettings {
    pub level:  String,
    pub format: LogFormat,
}

impl Settings {
    pub fn load() -> Result<Self, ConfigError> {
        let mut builder = Config::builder()
            .set_default("server.host", "127.0.0.1")?
            .set_default("server.port", 3118i64)?
            .set_default("core.url", "http://127.0.0.1:8080")?
            .set_default("core.internal_secret", "")?
            .set_default("core.files_url", "http://127.0.0.1:3101")?
            .set_default("database.max_connections", 20i64)?
            .set_default("database.min_connections", 2i64)?
            .set_default("database.connect_timeout", 10i64)?
            .set_default("database.run_migrations", true)?
            .set_default("database.engine", "postgres")?
            // SQLite only: where `<schema>.sqlite` lives.
            .set_default("database.path", "./data/db")?
            .set_default("runtime.worker_count", 4i64)?
            .set_default("runtime.execution_timeout_secs", 3600i64)?
            .set_default("runtime.node_timeout_secs", 60i64)?
            .set_default("runtime.max_retries", 3i64)?
            .set_default("runtime.retry_backoff_ms", 1000i64)?
            .set_default("runtime.max_execution_history", 500i64)?
            .set_default("queue.poll_interval_ms", 500i64)?
            .set_default("queue.batch_size", 10i64)?
            .set_default("code_node.timeout_secs", 30i64)?
            .set_default("code_node.memory_limit_mb", 64i64)?
            .set_default("logging.level", "info")?
            .set_default("logging.format", "pretty")?
            .add_source(File::with_name("config").required(false))
            .add_source(File::with_name("/etc/kubuno/modules/flow/config").required(false))
            .add_source(
                Environment::with_prefix("KF")
                    .separator("__")
                    .try_parsing(true),
            );

        // Variables injectées par le superviseur core — priorité maximale
        builder = builder
            .set_override_option("core.url",             std::env::var("KUBUNO_CORE_URL").ok())?
            .set_override_option("core.internal_secret", std::env::var("KUBUNO_INTERNAL_SECRET").ok())?
            .set_override_option("database.host",     std::env::var("KUBUNO_DB_HOST").ok())?
            .set_override_option("database.port",     std::env::var("KUBUNO_DB_PORT").ok()
                                                        .and_then(|v| v.parse::<u64>().ok().map(|n| n.to_string())))?
            .set_override_option("database.user",     std::env::var("KUBUNO_DB_USER").ok())?
            .set_override_option("database.password", std::env::var("KUBUNO_DB_PASSWORD").ok())?
            .set_override_option("database.database", std::env::var("KUBUNO_DB_NAME").ok())?
            .set_override_option("database.path",     std::env::var("KUBUNO_DB_PATH").ok())?
            .set_override_option("database.engine",   std::env::var("KUBUNO_DB_ENGINE").ok())?;

        builder.build()?.try_deserialize()
    }
}
