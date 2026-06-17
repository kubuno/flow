//! MongoDB node — document operations against an EXTERNAL MongoDB (credential or
//! connection string). One-shot client per execution.

use async_trait::async_trait;
use futures::TryStreamExt;
use mongodb::bson::{to_document, Bson, Document};
use mongodb::Client;
use serde_json::{json, Value};

use crate::nodes::trait_::{
    ExecutionContext, FieldDef, FieldType, NodeCategory, NodeContext, NodeError, NodeMeta, NodeOutput,
};

fn uri(config: &Value) -> Result<String, NodeError> {
    if let Some(cs) = config.get("credential").and_then(|c| c.get("connectionString")).and_then(|v| v.as_str()) {
        if !cs.is_empty() { return Ok(cs.to_string()); }
    }
    config.get("connection").and_then(|v| v.as_str()).filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or(NodeError::MissingField("connection"))
}

/// serde_json object → BSON document (empty object → empty filter).
fn to_doc(v: &Value) -> Result<Document, NodeError> {
    if v.is_null() { return Ok(Document::new()); }
    to_document(v).map_err(|e| NodeError::InvalidConfig(format!("Document invalide : {e}")))
}

fn doc_to_json(d: Document) -> Value {
    Bson::Document(d).into()
}

pub struct MongoNode;

#[async_trait]
impl crate::nodes::trait_::NodeExecutor for MongoNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "db.mongodb".into(), name: "MongoDB".into(),
            description: "Lit/écrit des documents sur un MongoDB externe".into(),
            category: NodeCategory::External, icon: "Database".into(), color: "#13aa52".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::credential("credential", "Credential MongoDB", "mongoDb"),
                FieldDef::new("connection", "Connexion (URI, si pas de credential)", FieldType::Expression)
                    .placeholder("mongodb://user:mdp@hote:27017"),
                FieldDef::new("database", "Base de données", FieldType::Expression).required(),
                FieldDef::new("collection", "Collection", FieldType::Expression).required(),
                FieldDef::new("operation", "Opération", FieldType::Select).required().options(&[
                    ("find","Rechercher (find)"),("findOne","Un seul (findOne)"),
                    ("insertOne","Insérer un"),("insertMany","Insérer plusieurs"),
                    ("updateMany","Mettre à jour"),("deleteMany","Supprimer"),
                ]).default(json!("find")),
                FieldDef::new("filter", "Filtre (JSON)", FieldType::Json).help(r#"{"statut":"actif"} — vide = tous"#),
                FieldDef::new("data", "Données (JSON)", FieldType::Json).help("Document(s) à insérer, ou champs à $set."),
                FieldDef::new("limit", "Limite (find)", FieldType::Number).default(json!(100)),
            ],
        }
    }

    async fn execute(&self, config: Value, ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let uri = uri(&config)?;
        let db_name = config.get("database").and_then(|v| v.as_str()).ok_or(NodeError::MissingField("database"))?;
        let coll_name = config.get("collection").and_then(|v| v.as_str()).ok_or(NodeError::MissingField("collection"))?;
        let op = config.get("operation").and_then(|v| v.as_str()).unwrap_or("find");

        let client = Client::with_uri_str(&uri).await
            .map_err(|e| NodeError::ServiceError(format!("Connexion MongoDB : {e}")))?;
        let coll = client.database(db_name).collection::<Document>(coll_name);
        let svc = |e: mongodb::error::Error| NodeError::ServiceError(format!("MongoDB : {e}"));

        let filter = to_doc(&config.get("filter").cloned().unwrap_or(Value::Null))?;
        let data_val = config.get("data").filter(|v| !v.is_null()).cloned().unwrap_or_else(|| ctx.input.clone());

        let result = match op {
            "find" => {
                let limit = config.get("limit").and_then(|v| v.as_i64()).unwrap_or(100).max(0);
                let mut cursor = coll.find(filter).limit(limit).await.map_err(svc)?;
                let mut rows = Vec::new();
                while let Some(doc) = cursor.try_next().await.map_err(svc)? { rows.push(doc_to_json(doc)); }
                json!({ "rows": rows, "count": rows.len() })
            }
            "findOne" => {
                let found = coll.find_one(filter).await.map_err(svc)?;
                json!({ "row": found.map(doc_to_json) })
            }
            "insertOne" => {
                let doc = to_doc(&data_val)?;
                let r = coll.insert_one(doc).await.map_err(svc)?;
                json!({ "insertedId": r.inserted_id.into_relaxed_extjson() })
            }
            "insertMany" => {
                let docs: Vec<Document> = match &data_val {
                    Value::Array(a) => a.iter().map(to_doc).collect::<Result<_, _>>()?,
                    other => vec![to_doc(other)?],
                };
                let r = coll.insert_many(docs).await.map_err(svc)?;
                json!({ "insertedCount": r.inserted_ids.len() })
            }
            "updateMany" => {
                let update = mongodb::bson::doc! { "$set": to_doc(&data_val)? };
                let r = coll.update_many(filter, update).await.map_err(svc)?;
                json!({ "matched": r.matched_count, "modified": r.modified_count })
            }
            "deleteMany" => {
                let r = coll.delete_many(filter).await.map_err(svc)?;
                json!({ "deleted": r.deleted_count })
            }
            _ => return Err(NodeError::InvalidConfig("Opération inconnue".into())),
        };
        Ok(NodeOutput::data(result))
    }
}
