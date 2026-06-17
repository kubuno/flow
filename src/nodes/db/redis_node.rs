//! Redis node — key/value operations against an EXTERNAL Redis (credential or
//! URL). One-shot async connection per execution.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::nodes::trait_::{
    ExecutionContext, FieldDef, FieldType, NodeCategory, NodeContext, NodeError, NodeMeta, NodeOutput,
};

fn redis_url(config: &Value) -> Result<String, NodeError> {
    if let Some(cred) = config.get("credential").filter(|v| v.get("host").is_some()) {
        let s = |k: &str| cred.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let host = s("host");
        if host.is_empty() { return Err(NodeError::MissingField("host")); }
        let port = cred.get("port").and_then(|v| v.as_i64())
            .or_else(|| cred.get("port").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()))
            .unwrap_or(6379);
        let db = cred.get("database").and_then(|v| v.as_i64()).unwrap_or(0);
        let pass = s("password");
        let auth = if pass.is_empty() { String::new() } else { format!(":{pass}@") };
        return Ok(format!("redis://{auth}{host}:{port}/{db}"));
    }
    config.get("connection").and_then(|v| v.as_str()).filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or(NodeError::MissingField("connection"))
}

pub struct RedisNode;

#[async_trait]
impl crate::nodes::trait_::NodeExecutor for RedisNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "db.redis".into(), name: "Redis".into(),
            description: "Lit/écrit des clés sur un serveur Redis externe".into(),
            category: NodeCategory::External, icon: "Database".into(), color: "#d82c20".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::credential("credential", "Credential Redis", "redis"),
                FieldDef::new("connection", "Connexion (URL, si pas de credential)", FieldType::Expression)
                    .placeholder("redis://:mdp@hote:6379/0"),
                FieldDef::new("operation", "Opération", FieldType::Select).required().options(&[
                    ("get","GET"),("set","SET"),("del","DEL"),("incr","INCR"),("decr","DECR"),
                    ("exists","EXISTS"),("expire","EXPIRE"),("ttl","TTL"),("keys","KEYS"),
                    ("lpush","LPUSH"),("rpush","RPUSH"),("publish","PUBLISH"),
                ]).default(json!("get")),
                FieldDef::new("key", "Clé / motif / canal", FieldType::Expression).required(),
                FieldDef::new("value", "Valeur", FieldType::Expression),
                FieldDef::new("ttl", "TTL (s, pour SET/EXPIRE)", FieldType::Number),
            ],
        }
    }

    async fn execute(&self, config: Value, _ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let url = redis_url(&config)?;
        let op = config.get("operation").and_then(|v| v.as_str()).unwrap_or("get");
        let key = config.get("key").and_then(|v| v.as_str()).ok_or(NodeError::MissingField("key"))?.to_string();
        let val = config.get("value").map(|v| match v { Value::String(s) => s.clone(), Value::Null => String::new(), other => other.to_string() }).unwrap_or_default();
        let ttl = config.get("ttl").and_then(|v| v.as_i64());

        let client = redis::Client::open(url).map_err(|e| NodeError::ServiceError(format!("Redis : {e}")))?;
        let mut con = client.get_multiplexed_async_connection().await
            .map_err(|e| NodeError::ServiceError(format!("Connexion Redis : {e}")))?;
        let svc = |e: redis::RedisError| NodeError::ServiceError(format!("Redis : {e}"));

        let result: Value = match op {
            "get" => { let r: Option<String> = redis::cmd("GET").arg(&key).query_async(&mut con).await.map_err(svc)?; json!(r) }
            "set" => {
                let mut cmd = redis::cmd("SET"); cmd.arg(&key).arg(&val);
                if let Some(t) = ttl { cmd.arg("EX").arg(t); }
                let r: String = cmd.query_async(&mut con).await.map_err(svc)?; json!(r)
            }
            "del" => { let r: i64 = redis::cmd("DEL").arg(&key).query_async(&mut con).await.map_err(svc)?; json!(r) }
            "incr" => { let r: i64 = redis::cmd("INCR").arg(&key).query_async(&mut con).await.map_err(svc)?; json!(r) }
            "decr" => { let r: i64 = redis::cmd("DECR").arg(&key).query_async(&mut con).await.map_err(svc)?; json!(r) }
            "exists" => { let r: i64 = redis::cmd("EXISTS").arg(&key).query_async(&mut con).await.map_err(svc)?; json!(r == 1) }
            "expire" => { let r: i64 = redis::cmd("EXPIRE").arg(&key).arg(ttl.unwrap_or(60)).query_async(&mut con).await.map_err(svc)?; json!(r == 1) }
            "ttl" => { let r: i64 = redis::cmd("TTL").arg(&key).query_async(&mut con).await.map_err(svc)?; json!(r) }
            "keys" => { let r: Vec<String> = redis::cmd("KEYS").arg(&key).query_async(&mut con).await.map_err(svc)?; json!(r) }
            "lpush" => { let r: i64 = redis::cmd("LPUSH").arg(&key).arg(&val).query_async(&mut con).await.map_err(svc)?; json!(r) }
            "rpush" => { let r: i64 = redis::cmd("RPUSH").arg(&key).arg(&val).query_async(&mut con).await.map_err(svc)?; json!(r) }
            "publish" => { let r: i64 = redis::cmd("PUBLISH").arg(&key).arg(&val).query_async(&mut con).await.map_err(svc)?; json!(r) }
            _ => Value::Null,
        };
        Ok(NodeOutput::data(json!({ "result": result })))
    }
}
