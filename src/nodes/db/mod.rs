//! Database (SGBD) nodes — operate on an EXTERNAL database whose connection
//! string is supplied by the user in the node config. These nodes never touch
//! Kubuno's own/shared database: each execution opens a one-shot connection to
//! the user-provided endpoint and closes it when done.
//!
//! Engines: PostgreSQL/CockroachDB (this file), MySQL/MariaDB, Redis, MongoDB,
//! and Supabase (REST). Oracle / SQL Server require native drivers and are not
//! bundled.

pub mod mongo;
pub mod mysql;
pub mod redis_node;
pub mod supabase;

pub use mongo::MongoNode;
pub use mysql::MySqlQueryNode;
pub use redis_node::RedisNode;
pub use supabase::SupabaseNode;

use async_trait::async_trait;
use serde_json::{json, Value};
use sqlx::postgres::{PgArguments, PgConnectOptions, PgSslMode};
use sqlx::{Connection, PgConnection, Postgres, Row};

use crate::nodes::trait_::{
    ExecutionContext, FieldDef, FieldType, NodeCategory, NodeContext, NodeError, NodeMeta, NodeOutput,
};

type PgQuery<'q> = sqlx::query::Query<'q, Postgres, PgArguments>;

/// Bind a list of JSON params onto a query (values cloned → no lifetime tie to `params`).
fn bind_params<'q>(mut q: PgQuery<'q>, params: &[Value]) -> PgQuery<'q> {
    for p in params {
        q = match p {
            Value::Null => q.bind(Option::<String>::None),
            Value::Bool(b) => q.bind(*b),
            Value::String(s) => q.bind(s.clone()),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() { q.bind(i) }
                else { q.bind(n.as_f64().unwrap_or(0.0)) }
            }
            other => q.bind(other.clone()), // array/object → jsonb
        };
    }
    q
}

/// Open a connection — from a `postgres` credential (host/user/…) if present,
/// otherwise from the `connection` DSN string.
async fn open(config: &Value) -> Result<PgConnection, NodeError> {
    if let Some(cred) = config.get("credential").filter(|v| v.get("host").is_some()) {
        let s = |k: &str| cred.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let port = cred.get("port").and_then(|v| v.as_i64())
            .or_else(|| cred.get("port").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()))
            .unwrap_or(5432) as u16;
        let ssl = cred.get("ssl").and_then(|v| v.as_bool()).unwrap_or(false);
        let mut opts = PgConnectOptions::new()
            .host(&s("host")).port(port).username(&s("user"))
            .ssl_mode(if ssl { PgSslMode::Require } else { PgSslMode::Prefer });
        let db = s("database"); if !db.is_empty() { opts = opts.database(&db); }
        let pass = s("password"); if !pass.is_empty() { opts = opts.password(&pass); }
        return PgConnection::connect_with(&opts).await
            .map_err(|e| NodeError::ServiceError(format!("Connexion DB : {e}")));
    }
    let dsn = config.get("connection").and_then(|v| v.as_str()).filter(|s| !s.is_empty())
        .ok_or(NodeError::MissingField("connection"))?;
    PgConnection::connect(dsn).await
        .map_err(|e| NodeError::ServiceError(format!("Connexion DB : {e}")))
}

/// Quote a SQL identifier (table/column), handling dotted schema.table.
fn quote_ident(name: &str) -> String {
    name.split('.')
        .map(|part| format!("\"{}\"", part.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(".")
}

// ── PostgreSQL : requête SQL ──────────────────────────────────────────────────────

pub struct PostgresQueryNode;

#[async_trait]
impl crate::nodes::trait_::NodeExecutor for PostgresQueryNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "db.postgres".into(), name: "PostgreSQL — Requête".into(),
            description: "Exécute une requête SQL sur une base PostgreSQL externe".into(),
            category: NodeCategory::External, icon: "Database".into(), color: "#336791".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::credential("credential", "Credential PostgreSQL", "postgres"),
                FieldDef::new("connection", "Connexion (DSN, si pas de credential)", FieldType::Expression)
                    .placeholder("postgres://user:mdp@hote:5432/base")
                    .help("Chaîne de connexion d'une base EXTERNE. Astuce : {{ $vars.dbUrl }}."),
                FieldDef::new("query", "Requête SQL", FieldType::Code).required()
                    .placeholder("SELECT * FROM clients WHERE actif = $1")
                    .help("Paramètres positionnels $1, $2… liés depuis « Paramètres »."),
                FieldDef::new("params", "Paramètres (JSON tableau)", FieldType::Json)
                    .help(r#"["valeur1", 42, true] — liés à $1, $2, $3"#),
            ],
        }
    }

    async fn execute(&self, config: Value, _ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let query = config.get("query").and_then(|v| v.as_str()).ok_or(NodeError::MissingField("query"))?;
        let params: Vec<Value> = config.get("params").and_then(|v| v.as_array()).cloned().unwrap_or_default();

        let mut conn = open(&config).await?;

        // Première tentative : envelopper dans une CTE pour récupérer les lignes en JSON.
        // Fonctionne pour SELECT/WITH et pour INSERT/UPDATE/DELETE … RETURNING.
        let wrapped = format!(
            "WITH __q AS ({query}) SELECT coalesce(json_agg(__q), '[]'::json)::text AS data FROM __q"
        );
        let q = bind_params(sqlx::query(&wrapped), &params);
        match q.fetch_one(&mut conn).await {
            Ok(row) => {
                let data: String = row.try_get("data").unwrap_or_else(|_| "[]".into());
                let rows: Value = serde_json::from_str(&data).unwrap_or(json!([]));
                let count = rows.as_array().map(|a| a.len()).unwrap_or(0);
                Ok(NodeOutput::data(json!({ "rows": rows, "count": count })))
            }
            // Pas de RETURNING (mutation simple) → exécuter et renvoyer le nb de lignes affectées.
            Err(e) if e.to_string().contains("does not have a RETURNING") => {
                let q2 = bind_params(sqlx::query(query), &params);
                let res = q2.execute(&mut conn).await
                    .map_err(|e| NodeError::ServiceError(format!("SQL : {e}")))?;
                Ok(NodeOutput::data(json!({ "affected": res.rows_affected(), "rows": [] })))
            }
            Err(e) => Err(NodeError::ServiceError(format!("SQL : {e}"))),
        }
    }
}

// ── PostgreSQL : insérer ──────────────────────────────────────────────────────────

pub struct PostgresInsertNode;

#[async_trait]
impl crate::nodes::trait_::NodeExecutor for PostgresInsertNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "db.postgres.insert".into(), name: "PostgreSQL — Insérer".into(),
            description: "Insère une ou plusieurs lignes dans une table PostgreSQL".into(),
            category: NodeCategory::External, icon: "DatabaseZap".into(), color: "#336791".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::credential("credential", "Credential PostgreSQL", "postgres"),
                FieldDef::new("connection", "Connexion (DSN, si pas de credential)", FieldType::Expression)
                    .placeholder("postgres://user:mdp@hote:5432/base"),
                FieldDef::new("table", "Table", FieldType::Text).required().placeholder("public.clients"),
                FieldDef::new("data", "Données (JSON)", FieldType::Json).required()
                    .help(r#"Objet {"nom":"Ada"} ou tableau d'objets. Vide = données entrantes."#),
            ],
        }
    }

    async fn execute(&self, config: Value, ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let table = config.get("table").and_then(|v| v.as_str()).filter(|s| !s.is_empty())
            .ok_or(NodeError::MissingField("table"))?;

        // Lignes à insérer : champ `data` (objet ou tableau) ou, à défaut, l'entrée du nœud.
        let data = config.get("data").filter(|v| !v.is_null()).cloned().unwrap_or_else(|| ctx.input.clone());
        let rows: Vec<Value> = match data {
            Value::Array(a) => a,
            obj @ Value::Object(_) => vec![obj],
            _ => return Err(NodeError::InvalidConfig("« Données » doit être un objet ou un tableau d'objets".into())),
        };
        let first = rows.first().and_then(|r| r.as_object())
            .ok_or_else(|| NodeError::InvalidConfig("Aucune ligne à insérer".into()))?;
        if first.is_empty() {
            return Err(NodeError::InvalidConfig("Ligne vide".into()));
        }
        // Colonnes = clés de la première ligne (ordre stable).
        let cols: Vec<String> = first.keys().cloned().collect();

        // Construit VALUES ($1,$2),($3,$4)… et la liste ordonnée des valeurs.
        let mut placeholders: Vec<String> = Vec::with_capacity(rows.len());
        let mut values: Vec<Value> = Vec::with_capacity(rows.len() * cols.len());
        let mut idx = 1;
        for r in &rows {
            let obj = r.as_object().ok_or_else(|| NodeError::InvalidConfig("Chaque ligne doit être un objet".into()))?;
            let mut ph = Vec::with_capacity(cols.len());
            for c in &cols {
                ph.push(format!("${idx}"));
                idx += 1;
                values.push(obj.get(c).cloned().unwrap_or(Value::Null));
            }
            placeholders.push(format!("({})", ph.join(",")));
        }
        let col_sql = cols.iter().map(|c| quote_ident(c)).collect::<Vec<_>>().join(",");
        let inner = format!(
            "INSERT INTO {} ({}) VALUES {} RETURNING *",
            quote_ident(table), col_sql, placeholders.join(","),
        );
        let wrapped = format!("WITH __q AS ({inner}) SELECT coalesce(json_agg(__q), '[]'::json)::text AS data FROM __q");

        let mut conn = open(&config).await?;
        let q = bind_params(sqlx::query(&wrapped), &values);
        let row = q.fetch_one(&mut conn).await
            .map_err(|e| NodeError::ServiceError(format!("SQL : {e}")))?;
        let data: String = row.try_get("data").unwrap_or_else(|_| "[]".into());
        let inserted: Value = serde_json::from_str(&data).unwrap_or(json!([]));
        let count = inserted.as_array().map(|a| a.len()).unwrap_or(0);
        Ok(NodeOutput::data(json!({ "rows": inserted, "count": count })))
    }
}
