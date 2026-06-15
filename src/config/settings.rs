use anyhow::Context;
use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use std::time::Duration;

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

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseSettings {
    pub url:             Option<String>,
    pub host:            Option<String>,
    pub port:            Option<u16>,
    pub user:            Option<String>,
    pub password:        Option<String>,
    pub database:        Option<String>,
    pub max_connections: u32,
    pub min_connections: u32,
    #[serde(with = "duration_secs")]
    pub connect_timeout: Duration,
    pub run_migrations:  bool,
}

impl DatabaseSettings {
    pub fn connect_options(&self) -> anyhow::Result<sqlx::postgres::PgConnectOptions> {
        use std::str::FromStr;
        if self.host.is_some() || self.user.is_some() {
            let user     = self.user.as_deref().context("database.user requis")?;
            let password = self.password.as_deref().context("database.password requis")?;
            let database = self.database.as_deref().context("database.database requis")?;
            return Ok(sqlx::postgres::PgConnectOptions::new()
                .host(self.host.as_deref().unwrap_or("localhost"))
                .port(self.port.unwrap_or(5432))
                .username(user)
                .password(password)
                .database(database));
        }
        if let Some(url) = &self.url {
            return sqlx::postgres::PgConnectOptions::from_str(url)
                .context("database.url invalide");
        }
        Err(anyhow::anyhow!("database : fournissez les champs host/user/password/database"))
    }
}

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
        if let Ok(v) = std::env::var("KUBUNO_CORE_URL")        { builder = builder.set_override("core.url",             v)?; }
        if let Ok(v) = std::env::var("KUBUNO_INTERNAL_SECRET") { builder = builder.set_override("core.internal_secret", v)?; }
        if let Ok(v) = std::env::var("KUBUNO_DB_HOST")         { builder = builder.set_override("database.host",     v)?; }
        if let Ok(v) = std::env::var("KUBUNO_DB_PORT")         { builder = builder.set_override("database.port",     v.parse::<i64>().unwrap_or(5432))?; }
        if let Ok(v) = std::env::var("KUBUNO_DB_USER")         { builder = builder.set_override("database.user",     v)?; }
        if let Ok(v) = std::env::var("KUBUNO_DB_PASSWORD")     { builder = builder.set_override("database.password", v)?; }
        if let Ok(v) = std::env::var("KUBUNO_DB_NAME")         { builder = builder.set_override("database.database", v)?; }

        builder.build()?.try_deserialize()
    }
}

mod duration_secs {
    use serde::{Deserialize, Deserializer};
    use std::time::Duration;
    pub fn deserialize<'de, D>(d: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(d)?;
        Ok(Duration::from_secs(secs))
    }
}
