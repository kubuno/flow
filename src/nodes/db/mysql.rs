//! MySQL / MariaDB query node. Connects to an EXTERNAL database (credential or
//! DSN) — never Kubuno's own database. One-shot connection per execution.

use async_trait::async_trait;
use serde_json::{json, Value};
use sqlx::mysql::{MySqlArguments, MySqlConnectOptions, MySqlConnection};
use sqlx::{Column, Connection, MySql, Row};

use crate::nodes::trait_::{
    ExecutionContext, FieldDef, FieldType, NodeCategory, NodeContext, NodeError, NodeMeta, NodeOutput,
};

type MyQuery<'q> = sqlx::query::Query<'q, MySql, MySqlArguments>;

fn bind_params<'q>(mut q: MyQuery<'q>, params: &[Value]) -> MyQuery<'q> {
    for p in params {
        q = match p {
            Value::Null => q.bind(Option::<String>::None),
            Value::Bool(b) => q.bind(*b),
            Value::String(s) => q.bind(s.clone()),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() { q.bind(i) } else { q.bind(n.as_f64().unwrap_or(0.0)) }
            }
            other => q.bind(other.to_string()),
        };
    }
    q
}

/// Decode one MySQL cell to JSON by trying common types in order.
fn cell(row: &sqlx::mysql::MySqlRow, i: usize) -> Value {
    if let Ok(Some(v)) = row.try_get::<Option<i64>, _>(i) { return json!(v); }
    if let Ok(Some(v)) = row.try_get::<Option<f64>, _>(i) { return json!(v); }
    if let Ok(Some(v)) = row.try_get::<Option<bool>, _>(i) { return json!(v); }
    if let Ok(Some(v)) = row.try_get::<Option<Value>, _>(i) { return v; }
    if let Ok(Some(v)) = row.try_get::<Option<String>, _>(i) { return json!(v); }
    if let Ok(Some(v)) = row.try_get::<Option<chrono::NaiveDateTime>, _>(i) { return json!(v.to_string()); }
    if let Ok(Some(v)) = row.try_get::<Option<chrono::NaiveDate>, _>(i) { return json!(v.to_string()); }
    if let Ok(Some(v)) = row.try_get::<Option<sqlx::types::BigDecimal>, _>(i) { return json!(v.to_string()); }
    Value::Null
}

fn row_to_json(row: &sqlx::mysql::MySqlRow) -> Value {
    let mut obj = serde_json::Map::new();
    for (i, col) in row.columns().iter().enumerate() {
        obj.insert(col.name().to_string(), cell(row, i));
    }
    Value::Object(obj)
}

async fn open(config: &Value) -> Result<MySqlConnection, NodeError> {
    if let Some(cred) = config.get("credential").filter(|v| v.get("host").is_some()) {
        let s = |k: &str| cred.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let port = cred.get("port").and_then(|v| v.as_i64())
            .or_else(|| cred.get("port").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()))
            .unwrap_or(3306) as u16;
        let mut opts = MySqlConnectOptions::new().host(&s("host")).port(port).username(&s("user"));
        let db = s("database"); if !db.is_empty() { opts = opts.database(&db); }
        let pass = s("password"); if !pass.is_empty() { opts = opts.password(&pass); }
        return MySqlConnection::connect_with(&opts).await
            .map_err(|e| NodeError::ServiceError(format!("Connexion MySQL : {e}")));
    }
    let dsn = config.get("connection").and_then(|v| v.as_str()).filter(|s| !s.is_empty())
        .ok_or(NodeError::MissingField("connection"))?;
    MySqlConnection::connect(dsn).await
        .map_err(|e| NodeError::ServiceError(format!("Connexion MySQL : {e}")))
}

fn returns_rows(sql: &str) -> bool {
    let head = sql.split_whitespace().next().unwrap_or("").to_uppercase();
    matches!(head.as_str(), "SELECT" | "SHOW" | "WITH" | "DESC" | "DESCRIBE" | "EXPLAIN" | "CALL" | "VALUES" | "TABLE")
}

pub struct MySqlQueryNode;

#[async_trait]
impl crate::nodes::trait_::NodeExecutor for MySqlQueryNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "db.mysql".into(), name: "MySQL / MariaDB — Requête".into(),
            description: "Exécute une requête SQL sur une base MySQL ou MariaDB externe".into(),
            category: NodeCategory::External, icon: "Database".into(), color: "#00758f".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::credential("credential", "Credential MySQL", "mysql,mariaDb"),
                FieldDef::new("connection", "Connexion (DSN, si pas de credential)", FieldType::Expression)
                    .placeholder("mysql://user:mdp@hote:3306/base"),
                FieldDef::new("query", "Requête SQL", FieldType::Code).required()
                    .placeholder("SELECT * FROM clients WHERE actif = ?")
                    .help("Paramètres positionnels « ? » liés depuis « Paramètres »."),
                FieldDef::new("params", "Paramètres (JSON tableau)", FieldType::Json)
                    .help(r#"["valeur1", 42, true] — liés aux « ? » dans l'ordre"#),
            ],
        }
    }

    async fn execute(&self, config: Value, _ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let query = config.get("query").and_then(|v| v.as_str()).ok_or(NodeError::MissingField("query"))?;
        let params: Vec<Value> = config.get("params").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let mut conn = open(&config).await?;

        if returns_rows(query) {
            let rows = bind_params(sqlx::query(query), &params).fetch_all(&mut conn).await
                .map_err(|e| NodeError::ServiceError(format!("SQL : {e}")))?;
            let out: Vec<Value> = rows.iter().map(row_to_json).collect();
            let count = out.len();
            Ok(NodeOutput::data(json!({ "rows": out, "count": count })))
        } else {
            let res = bind_params(sqlx::query(query), &params).execute(&mut conn).await
                .map_err(|e| NodeError::ServiceError(format!("SQL : {e}")))?;
            Ok(NodeOutput::data(json!({ "affected": res.rows_affected(), "rows": [] })))
        }
    }
}
