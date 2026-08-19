use anyhow::{Context, Result};
use clap::Parser;
use kubuno_flow::{
    config::Settings,
    nodes::build_registry,
    router,
    runtime::{core_proxy::CoreProxy, scheduler, worker},
    state::AppState,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use std::sync::{Arc, RwLock};
use std::time::Duration;

// ── Lecture de module.toml ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct Manifest {
    module:        ManifestModule,
    #[serde(default)]
    sidebar_items: Vec<SidebarItemRaw>,
    events:        Option<ManifestEvents>,
    /// Declarative instance settings (execution limits), rendered by the core's
    /// generic admin form.
    #[serde(default)]
    settings:      Vec<SettingDefRaw>,
    /// Pages the admin panel is split into (`[[setting_groups]]`).
    #[serde(default)]
    setting_groups: Vec<SettingGroupRaw>,
}
#[derive(Deserialize)]
struct ManifestModule {
    display_name:  String,
    description:   Option<String>,
    settings_path: Option<String>,
}
#[derive(Deserialize)]
struct SidebarItemRaw {
    id:       String,
    label:    String,
    icon:     String,
    path:     String,
    position: i32,
    #[serde(default)]
    protected_folder: Option<String>,
}
#[derive(Deserialize)]
struct ManifestEvents {
    #[serde(default)]
    subscribed: Vec<String>,
}

/// One `[[setting_groups]]` entry of module.toml, forwarded verbatim. `id` is a
/// STABLE, UNTRANSLATED slug: it travels in the URL of the admin page.
#[derive(Deserialize, Serialize)]
struct SettingGroupRaw {
    id:          String,
    label:       String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    icon:        Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    position:    Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

/// One `[[settings]]` entry from module.toml, forwarded verbatim.
#[derive(Deserialize, Serialize)]
struct SettingDefRaw {
    key:         String,
    scope:       String,
    #[serde(rename = "type")]
    value_type:  String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    values:      Option<serde_json::Value>,
    default:     serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label:       Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    category:    Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    group:       Option<String>,
    #[serde(default)]
    public:      bool,
    // Presentation metadata the console understands. Forwarded verbatim so a
    // bound declared in the manifest is also enforced by the core's write path.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    advanced:    bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    risk:        Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    min:         Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max:         Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    unit:        Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    placeholder: Option<String>,
    /// The `string` value is a LIST, one entry per line → textarea.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    multiline:   bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    depends_on:  Option<String>,
}

fn load_manifest() -> Option<Manifest> {
    let path = if let Ok(dir) = std::env::var("KUBUNO_MODULE_DIR") {
        std::path::PathBuf::from(dir).join("module.toml")
    } else {
        std::env::current_exe().ok()?.parent()?.join("module.toml")
    };
    let content = std::fs::read_to_string(&path)
        .map_err(|e| tracing::warn!(path = %path.display(), error = %e, "module.toml introuvable"))
        .ok()?;
    toml::from_str::<Manifest>(&content)
        .map_err(|e| tracing::error!(path = %path.display(), error = %e, "module.toml invalide"))
        .ok()
}

#[derive(Parser, Debug)]
#[command(name = "kubuno-flow", version, about = "Module flow Kubuno")]
struct Cli {
    #[arg(short, long, env = "KF_CONFIG_FILE")]
    config: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    let _cli = Cli::parse();

    let settings = Settings::load().context("Chargement de la configuration")?;

    let log_level = settings.logging.level.clone();
    let subscriber = tracing_subscriber::fmt().with_env_filter(
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&log_level)),
    );
    match settings.logging.format {
        kubuno_flow::config::LogFormat::Json   => subscriber.json().init(),
        kubuno_flow::config::LogFormat::Pretty => subscriber.init(),
    }

    tracing::info!("Kubuno Flow v{} démarrage…", env!("CARGO_PKG_VERSION"));

    // Sécurité : interdire toute exécution de processus sur l’hôte (voir kubuno-seccomp).
    kubuno_seccomp::lock_down_process_execution("flow");

    // Pool PostgreSQL
    let opts = settings.database.connect_options()?;
    let pool = PgPoolOptions::new()
        .max_connections(settings.database.max_connections)
        .min_connections(settings.database.min_connections)
        .acquire_timeout(settings.database.connect_timeout)
        .connect_with(opts)
        .await
        .context("Connexion PostgreSQL")?;

    // Migrations (search_path flow,public)
    if settings.database.run_migrations {
        sqlx::query("CREATE SCHEMA IF NOT EXISTS flow")
            .execute(&pool)
            .await
            .context("Création du schéma flow")?;

        let migration_opts = settings.database.connect_options()?
            .options([("search_path", "flow,public")]);
        let migration_pool = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(settings.database.connect_timeout)
            .connect_with(migration_opts)
            .await
            .context("Pool de migration")?;

        sqlx::migrate!("./migrations")
            .run(&migration_pool)
            .await
            .context("Migrations")?;
    }

    // The administrator's outbound host lists. Built empty, filled by the first
    // settings fetch below: an unreachable core must never lock the instance out
    // of the network, only leave it as permissive as the SSRF guard alone.
    let egress_policy: kubuno_flow::runtime::egress::SharedEgressPolicy =
        Arc::new(RwLock::new(Default::default()));

    let proxy = Arc::new(CoreProxy::new(
        settings.core.url.clone(),
        settings.core.internal_secret.clone(),
        egress_policy,
    ));
    let registry = Arc::new(build_registry());
    let files_client = Arc::new(kubuno_flow::files_client::FilesClient::new(
        settings.core.files_url.clone(),
        settings.core.internal_secret.clone(),
    ));

    let http = Client::new();

    // Initial fetch of the admin-editable instance settings; fall back to the
    // compiled defaults if the core is not yet reachable (the refresher below
    // will pick them up once it comes back).
    let boot_settings = kubuno_flow::config::fetch_instance_raw(
        &settings.core.url, &settings.core.internal_secret, &http,
    )
    .await;
    let instance = boot_settings
        .as_ref()
        .map(kubuno_flow::config::InstanceConfig::from_settings)
        .unwrap_or_default();
    if let Some(raw) = boot_settings.as_ref() {
        proxy.set_egress_policy(
            kubuno_flow::runtime::egress::EgressPolicy::from_settings(raw),
        );
    }
    let instance = Arc::new(RwLock::new(instance));

    let state = AppState {
        db:       pool,
        settings: Arc::new(settings.clone()),
        proxy,
        registry,
        files_client,
        instance: instance.clone(),
    };

    // Refresh the instance settings from the core every 60s so admin edits take
    // effect without a restart. A failed fetch keeps the last known values.
    {
        let http_r     = http.clone();
        let core_url   = settings.core.url.clone();
        let secret     = settings.core.internal_secret.clone();
        let instance_r = instance.clone();
        let proxy_r    = state.proxy.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                if let Some(raw) =
                    kubuno_flow::config::fetch_instance_raw(&core_url, &secret, &http_r).await
                {
                    if let Ok(mut guard) = instance_r.write() {
                        *guard = kubuno_flow::config::InstanceConfig::from_settings(&raw);
                    }
                    // Same payload, second consumer: the outbound host lists.
                    proxy_r.set_egress_policy(
                        kubuno_flow::runtime::egress::EgressPolicy::from_settings(&raw),
                    );
                }
            }
        });
    }

    // Workers + schedulers
    worker::spawn_workers(state.clone());
    scheduler::spawn_schedulers(state.clone());

    // Enregistrement auprès du core + heartbeat
    register_with_core(&http, &settings).await;
    {
        let http2 = http.clone();
        let settings2 = settings.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                let url = format!("{}/internal/modules/flow/heartbeat", settings2.core.url);
                let secret = &settings2.core.internal_secret;
                match http2.post(&url).header("X-Internal-Secret", secret.as_str()).send().await {
                    Ok(r) if r.status().is_success() => {}
                    Ok(r) if r.status() == reqwest::StatusCode::NOT_FOUND => {
                        tracing::info!("Heartbeat 404 — ré-enregistrement…");
                        register_with_core(&http2, &settings2).await;
                    }
                    Ok(r) if r.status() == reqwest::StatusCode::FORBIDDEN => {
                        tracing::info!("Heartbeat 403 — module désactivé, attente…");
                    }
                    Ok(r)  => tracing::warn!(status = %r.status(), "Heartbeat réponse inattendue"),
                    Err(e) => tracing::warn!(error = %e, "Heartbeat erreur réseau"),
                }
            }
        });
    }

    // Serveur HTTP
    let addr = format!("{}:{}", settings.server.host, settings.server.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Bind sur {addr}"))?;
    tracing::info!("Kubuno Flow démarré sur http://{addr}");

    let app = router::build(state);
    axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
        .await
        .context("Erreur du serveur HTTP")?;

    Ok(())
}

fn backoff(attempt: u32) -> u64 {
    if attempt <= 10 { (attempt * 2) as u64 } else { 30 }
}

async fn register_with_core(http: &Client, settings: &Settings) {
    let base_url = format!("http://{}:{}", settings.server.host, settings.server.port);
    let core_url = &settings.core.url;
    let secret = &settings.core.internal_secret;

    let manifest = load_manifest();
    let display_name = manifest.as_ref().map(|m| m.module.display_name.as_str()).unwrap_or("Flow").to_string();
    let description = manifest.as_ref().and_then(|m| m.module.description.clone());
    let settings_path = manifest.as_ref().and_then(|m| m.module.settings_path.clone());
    let sidebar_items: Vec<Value> = manifest.as_ref()
        .map(|m| m.sidebar_items.iter().map(|s| json!({
            "id": s.id, "label": s.label, "icon": s.icon, "path": s.path, "position": s.position,
            "protected_folder": s.protected_folder,
        })).collect())
        .unwrap_or_else(|| vec![
            json!({ "id": "flow", "label": "Flow", "icon": "Workflow", "path": "/flow", "position": 60, "protected_folder": "Flow" }),
        ]);
    let subscribed_events: Vec<String> = manifest.as_ref()
        .and_then(|m| m.events.as_ref())
        .map(|e| e.subscribed.clone())
        .unwrap_or_default();

    // Declarative instance settings + admin pages, forwarded so the core can render
    // the generic form and split the admin panel into sub-menus.
    let settings_schema: Vec<Value> = manifest.as_ref()
        .map(|m| m.settings.iter().map(|s| serde_json::to_value(s).unwrap_or(Value::Null)).collect())
        .unwrap_or_default();
    let setting_groups: Vec<Value> = manifest.as_ref()
        .map(|m| m.setting_groups.iter().map(|g| serde_json::to_value(g).unwrap_or(Value::Null)).collect())
        .unwrap_or_default();

    let payload = json!({
        "module_id":         "flow",
        "display_name":      display_name,
        "description":       description,
        "settings_path":     settings_path,
        "base_url":          base_url,
        "version":           env!("CARGO_PKG_VERSION"),
        "routes":            [{ "method": "*", "path": "/*" }],
        "sidebar_items":     sidebar_items,
        "subscribed_events": subscribed_events,
        "settings_schema":   settings_schema,
        "setting_groups":    setting_groups,
        // Outil exposé via le serveur MCP du core : exécute un workflow Flow muni
        // d'un nœud « Serveur MCP » (trigger.mcp). Le core proxifie vers /mcp/run.
        "mcp_tools": [{
            "name":        "flow_run_workflow",
            "description": "Exécute un workflow Kubuno Flow exposé en MCP et retourne sa sortie.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "workflow_id": { "type": "string", "description": "Identifiant du workflow à exécuter" },
                    "input":       { "type": "object", "description": "Données d'entrée transmises au workflow" }
                },
                "required": ["workflow_id"]
            },
            "route":  "/mcp/run",
            "method": "POST",
        }],
    });

    for attempt in 1u32.. {
        let url = format!("{core_url}/internal/modules/register");
        match http.post(&url).header("X-Internal-Secret", secret.as_str()).json(&payload).send().await {
            Ok(resp) if resp.status().is_success() => {
                tracing::info!("Module flow enregistré auprès du core");
                return;
            }
            Ok(resp) if resp.status() == reqwest::StatusCode::FORBIDDEN => {
                tracing::info!(attempt, "Module désactivé par l'admin, nouvel essai dans 30s…");
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
            Ok(resp) => {
                let wait = backoff(attempt);
                tracing::warn!(attempt, status = %resp.status(), "Enregistrement échoué, retry dans {wait}s…");
                tokio::time::sleep(Duration::from_secs(wait)).await;
            }
            Err(e) => {
                let wait = backoff(attempt);
                tracing::warn!(attempt, error = %e, "Core inaccessible, retry dans {wait}s…");
                tokio::time::sleep(Duration::from_secs(wait)).await;
            }
        }
    }
}
