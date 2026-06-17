//! Nœuds logiques et transformateurs — traitement 100 % interne, aucun appel sortant.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::nodes::trait_::{
    ExecutionContext, FieldDef, FieldType, NodeCategory, NodeContext, NodeError, NodeMeta,
    NodeOutput, PortDef,
};

// ── Helpers ─────────────────────────────────────────────────────────────────────

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

/// Compare deux valeurs selon un opérateur textuel.
fn compare(left: &Value, op: &str, right: &Value) -> bool {
    match op {
        "eq"        => left == right || left.to_string().trim_matches('"') == right.to_string().trim_matches('"'),
        "ne"        => left != right,
        "gt"        => matches!((as_f64(left), as_f64(right)), (Some(a), Some(b)) if a > b),
        "lt"        => matches!((as_f64(left), as_f64(right)), (Some(a), Some(b)) if a < b),
        "gte"       => matches!((as_f64(left), as_f64(right)), (Some(a), Some(b)) if a >= b),
        "lte"       => matches!((as_f64(left), as_f64(right)), (Some(a), Some(b)) if a <= b),
        "contains"  => left.to_string().contains(&right.to_string().trim_matches('"').to_string()),
        "truthy"    => truthy(left),
        "empty"     => !truthy(left),
        _           => false,
    }
}

// ── If / Else ───────────────────────────────────────────────────────────────────

pub struct IfNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for IfNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "logic.if".into(), name: "If / Else".into(),
            description: "Bifurcation conditionnelle (sorties vrai / faux)".into(),
            category: NodeCategory::Logic, icon: "GitBranch".into(), color: "#f9ab00".into(),
            inputs: 1,
            outputs: vec![
                PortDef { id: "true".into(),  label: "Vrai".into() },
                PortDef { id: "false".into(), label: "Faux".into() },
            ],
            fields: vec![
                FieldDef::new("value", "Valeur", FieldType::Expression).required().placeholder("{{ trigger.montant }}"),
                FieldDef::new("operator", "Opérateur", FieldType::Select).required().options(&[
                    ("eq","="),("ne","≠"),("gt",">"),("lt","<"),("gte","≥"),("lte","≤"),
                    ("contains","contient"),("truthy","est vrai"),("empty","est vide"),
                ]).default(json!("eq")),
                FieldDef::new("compare", "Comparer à", FieldType::Expression),
            ],
        }
    }
    async fn execute(&self, config: Value, ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let value = config.get("value").cloned().unwrap_or(Value::Null);
        let op = config.get("operator").and_then(|v| v.as_str()).unwrap_or("eq");
        let compare_to = config.get("compare").cloned().unwrap_or(Value::Null);
        let result = compare(&value, op, &compare_to);
        let port = if result { "true" } else { "false" };
        Ok(NodeOutput::branch(ctx.input.clone(), vec![port.to_string()]))
    }
}

// ── Switch ──────────────────────────────────────────────────────────────────────

pub struct SwitchNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for SwitchNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "logic.switch".into(), name: "Switch".into(),
            description: "Branchement multiple selon une valeur".into(),
            category: NodeCategory::Logic, icon: "Split".into(), color: "#f9ab00".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::new("value", "Valeur", FieldType::Expression).required(),
                FieldDef::new("cases", "Correspondances (JSON)", FieldType::Json)
                    .help(r#"[{"equals":"a","port":"0"}, …] — sinon port "default""#),
            ],
        }
    }
    async fn execute(&self, config: Value, ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let value = config.get("value").cloned().unwrap_or(Value::Null);
        let cases = config.get("cases").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let mut port = "default".to_string();
        for c in &cases {
            if let Some(eq) = c.get("equals") {
                if compare(&value, "eq", eq) {
                    port = c.get("port").and_then(|p| p.as_str()).unwrap_or("default").to_string();
                    break;
                }
            }
        }
        Ok(NodeOutput::branch(ctx.input.clone(), vec![port]))
    }
}

// ── Filter ──────────────────────────────────────────────────────────────────────

pub struct FilterNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for FilterNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "logic.filter".into(), name: "Filtrer".into(),
            description: "Garde les éléments d'un tableau qui remplissent une condition, ou bloque le flux".into(),
            category: NodeCategory::Logic, icon: "Filter".into(), color: "#f9ab00".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::new("field", "Champ de l'élément", FieldType::Text).placeholder("montant"),
                FieldDef::new("operator", "Opérateur", FieldType::Select).options(&[
                    ("eq","="),("ne","≠"),("gt",">"),("lt","<"),("gte","≥"),("lte","≤"),
                    ("contains","contient"),("truthy","est vrai"),("empty","est vide"),
                ]).default(json!("truthy")),
                FieldDef::new("compare", "Comparer à", FieldType::Expression),
            ],
        }
    }
    async fn execute(&self, config: Value, ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let field = config.get("field").and_then(|v| v.as_str()).unwrap_or("");
        let op = config.get("operator").and_then(|v| v.as_str()).unwrap_or("truthy");
        let compare_to = config.get("compare").cloned().unwrap_or(Value::Null);

        let pick = |item: &Value| -> Value {
            if field.is_empty() { item.clone() } else { item.get(field).cloned().unwrap_or(Value::Null) }
        };

        match &ctx.input {
            Value::Array(arr) => {
                let kept: Vec<Value> = arr.iter()
                    .filter(|it| { let v = pick(it); compare(&v, op, &compare_to) })
                    .cloned().collect();
                if kept.is_empty() {
                    Ok(NodeOutput::branch(json!([]), vec![]))
                } else {
                    Ok(NodeOutput::data(Value::Array(kept)))
                }
            }
            other => {
                let v = if field.is_empty() { other.clone() } else { other.get(field).cloned().unwrap_or(Value::Null) };
                if compare(&v, op, &compare_to) {
                    Ok(NodeOutput::data(other.clone()))
                } else {
                    Ok(NodeOutput::branch(other.clone(), vec![]))
                }
            }
        }
    }
}

// ── Transform ───────────────────────────────────────────────────────────────────

pub struct TransformNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for TransformNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "logic.transform".into(), name: "Transformer".into(),
            description: "Construit un nouvel objet à partir d'expressions".into(),
            category: NodeCategory::Logic, icon: "Shuffle".into(), color: "#f9ab00".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::new("mapping", "Mapping (JSON)", FieldType::Json)
                    .help(r#"{"nomComplet":"{{trigger.prenom}} {{trigger.nom}}"} — déjà résolu"#),
            ],
        }
    }
    async fn execute(&self, config: Value, _ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        // Le mapping a déjà ses expressions résolues par l'executor.
        let mapping = config.get("mapping").cloned().unwrap_or(json!({}));
        Ok(NodeOutput::data(mapping))
    }
}

// ── Set variable ─────────────────────────────────────────────────────────────────

pub struct SetVariableNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for SetVariableNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "logic.set_variable".into(), name: "Définir variable".into(),
            description: "Ajoute/modifie un champ dans les données qui circulent".into(),
            category: NodeCategory::Logic, icon: "Variable".into(), color: "#f9ab00".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::new("name", "Nom", FieldType::Text).required(),
                FieldDef::new("value", "Valeur", FieldType::Expression).required(),
            ],
        }
    }
    async fn execute(&self, config: Value, ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let name = config.get("name").and_then(|v| v.as_str()).ok_or(NodeError::MissingField("name"))?;
        let value = config.get("value").cloned().unwrap_or(Value::Null);
        let mut out = match &ctx.input { Value::Object(o) => o.clone(), _ => serde_json::Map::new() };
        out.insert(name.to_string(), value);
        Ok(NodeOutput::data(Value::Object(out)))
    }
}

// ── Template ────────────────────────────────────────────────────────────────────

pub struct TemplateNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for TemplateNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "logic.template".into(), name: "Template".into(),
            description: "Génère un texte avec des variables {{ }}".into(),
            category: NodeCategory::Logic, icon: "FileText".into(), color: "#f9ab00".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::new("template", "Modèle", FieldType::Textarea).required(),
            ],
        }
    }
    async fn execute(&self, config: Value, _ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let text = config.get("template").cloned().unwrap_or(Value::String(String::new()));
        Ok(NodeOutput::data(json!({ "text": text })))
    }
}

// ── Merge ───────────────────────────────────────────────────────────────────────

pub struct MergeNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for MergeNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "logic.merge".into(), name: "Fusionner".into(),
            description: "Fusionne plusieurs branches entrantes en une".into(),
            category: NodeCategory::Logic, icon: "Merge".into(), color: "#f9ab00".into(),
            inputs: 1, outputs: vec![], fields: vec![],
        }
    }
    async fn execute(&self, _config: Value, ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        Ok(NodeOutput::data(ctx.input.clone()))
    }
}

// ── Split ───────────────────────────────────────────────────────────────────────

pub struct SplitNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for SplitNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "logic.split".into(), name: "Diviser".into(),
            description: "Extrait un tableau d'un champ pour traitement aval".into(),
            category: NodeCategory::Logic, icon: "Rows".into(), color: "#f9ab00".into(),
            inputs: 1, outputs: vec![],
            fields: vec![ FieldDef::new("field", "Champ tableau", FieldType::Text).placeholder("items") ],
        }
    }
    async fn execute(&self, config: Value, ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let field = config.get("field").and_then(|v| v.as_str()).unwrap_or("");
        let arr = if field.is_empty() { ctx.input.clone() } else { ctx.input.get(field).cloned().unwrap_or(json!([])) };
        Ok(NodeOutput::data(arr))
    }
}

// ── Wait ────────────────────────────────────────────────────────────────────────

pub struct WaitNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for WaitNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "logic.wait".into(), name: "Attendre".into(),
            description: "Pause d'une durée donnée (max 5 min en ligne)".into(),
            category: NodeCategory::Logic, icon: "Timer".into(), color: "#f9ab00".into(),
            inputs: 1, outputs: vec![],
            fields: vec![ FieldDef::new("seconds", "Secondes", FieldType::Number).default(json!(5)) ],
        }
    }
    async fn execute(&self, config: Value, ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let secs = config.get("seconds").and_then(as_f64).unwrap_or(5.0).clamp(0.0, 300.0);
        tokio::time::sleep(std::time::Duration::from_secs_f64(secs)).await;
        Ok(NodeOutput::data(ctx.input.clone()))
    }
}

// ── Calculate ───────────────────────────────────────────────────────────────────

pub struct CalculateNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for CalculateNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "logic.calculate".into(), name: "Calcul".into(),
            description: "Opération mathématique sur deux nombres".into(),
            category: NodeCategory::Logic, icon: "Calculator".into(), color: "#f9ab00".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::new("a", "A", FieldType::Expression).required(),
                FieldDef::new("operation", "Opération", FieldType::Select).options(&[
                    ("add","+"),("sub","−"),("mul","×"),("div","÷"),("mod","mod"),
                ]).default(json!("add")),
                FieldDef::new("b", "B", FieldType::Expression).required(),
            ],
        }
    }
    async fn execute(&self, config: Value, _ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let a = config.get("a").and_then(as_f64).unwrap_or(0.0);
        let b = config.get("b").and_then(as_f64).unwrap_or(0.0);
        let op = config.get("operation").and_then(|v| v.as_str()).unwrap_or("add");
        let r = match op {
            "add" => a + b,
            "sub" => a - b,
            "mul" => a * b,
            "div" if b != 0.0 => a / b,
            "mod" if b != 0.0 => a % b,
            _ => 0.0,
        };
        Ok(NodeOutput::data(json!({ "result": r })))
    }
}

// ── JSON parse / stringify ───────────────────────────────────────────────────────

pub struct JsonNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for JsonNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "logic.json".into(), name: "JSON".into(),
            description: "Parse ou sérialise du JSON".into(),
            category: NodeCategory::Logic, icon: "Braces".into(), color: "#f9ab00".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::new("mode", "Mode", FieldType::Select).options(&[("parse","Parser"),("stringify","Sérialiser")]).default(json!("parse")),
                FieldDef::new("value", "Valeur", FieldType::Expression),
            ],
        }
    }
    async fn execute(&self, config: Value, ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let mode = config.get("mode").and_then(|v| v.as_str()).unwrap_or("parse");
        let value = config.get("value").cloned().unwrap_or_else(|| ctx.input.clone());
        let out = match mode {
            "parse" => match &value {
                Value::String(s) => serde_json::from_str(s).unwrap_or(Value::Null),
                other => other.clone(),
            },
            _ => Value::String(serde_json::to_string(&value).unwrap_or_default()),
        };
        Ok(NodeOutput::data(json!({ "result": out })))
    }
}

// ── Aggregate ───────────────────────────────────────────────────────────────────

pub struct AggregateNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for AggregateNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "logic.aggregate".into(), name: "Agrégat".into(),
            description: "Compter / sommer / moyenner un tableau".into(),
            category: NodeCategory::Logic, icon: "Sigma".into(), color: "#f9ab00".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::new("operation", "Opération", FieldType::Select).options(&[("count","Compter"),("sum","Somme"),("avg","Moyenne"),("min","Min"),("max","Max")]).default(json!("count")),
                FieldDef::new("field", "Champ numérique", FieldType::Text),
            ],
        }
    }
    async fn execute(&self, config: Value, ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let op = config.get("operation").and_then(|v| v.as_str()).unwrap_or("count");
        let field = config.get("field").and_then(|v| v.as_str()).unwrap_or("");
        let arr = ctx.input.as_array().cloned().unwrap_or_default();
        let nums: Vec<f64> = arr.iter().filter_map(|it| {
            let v = if field.is_empty() { it } else { it.get(field).unwrap_or(&Value::Null) };
            as_f64(v)
        }).collect();
        let result = match op {
            "count" => arr.len() as f64,
            "sum"   => nums.iter().sum(),
            "avg"   => if nums.is_empty() { 0.0 } else { nums.iter().sum::<f64>() / nums.len() as f64 },
            "min"   => nums.iter().cloned().fold(f64::INFINITY, f64::min),
            "max"   => nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
            _ => 0.0,
        };
        Ok(NodeOutput::data(json!({ "result": result })))
    }
}

// ── Error handler ───────────────────────────────────────────────────────────────

pub struct ErrorHandlerNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for ErrorHandlerNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "logic.error_handler".into(), name: "Gérer erreur".into(),
            description: "Point d'entrée du chemin d'erreur (passe les données)".into(),
            category: NodeCategory::Logic, icon: "TriangleAlert".into(), color: "#d93025".into(),
            inputs: 1, outputs: vec![], fields: vec![],
        }
    }
    async fn execute(&self, _config: Value, ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        Ok(NodeOutput::data(ctx.input.clone()))
    }
}

// ── Stop ────────────────────────────────────────────────────────────────────────

pub struct StopNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for StopNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "logic.stop".into(), name: "Stop".into(),
            description: "Termine le workflow (succès ou erreur)".into(),
            category: NodeCategory::Logic, icon: "Square".into(), color: "#d93025".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::new("mode", "Mode", FieldType::Select).options(&[("success","Succès"),("error","Erreur")]).default(json!("success")),
                FieldDef::new("message", "Message", FieldType::Text),
            ],
        }
    }
    async fn execute(&self, config: Value, ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let mode = config.get("mode").and_then(|v| v.as_str()).unwrap_or("success");
        if mode == "error" {
            let msg = config.get("message").and_then(|v| v.as_str()).unwrap_or("Arrêt sur erreur");
            return Err(NodeError::Stopped(msg.to_string()));
        }
        // Succès : on stoppe la propagation (aucune sortie active).
        Ok(NodeOutput::branch(ctx.input.clone(), vec![]))
    }
}

// ── Date / Time ──────────────────────────────────────────────────────────────────

pub struct DateTimeNode;

fn parse_dt(v: &Value) -> Option<chrono::DateTime<chrono::Utc>> {
    use chrono::Utc;
    match v {
        Value::String(s) => {
            if s.trim().is_empty() { return Some(Utc::now()); }
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) { return Some(dt.with_timezone(&Utc)); }
            if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
                return d.and_hms_opt(0, 0, 0).map(|nd| chrono::DateTime::<Utc>::from_naive_utc_and_offset(nd, Utc));
            }
            None
        }
        Value::Number(n) => n.as_i64().and_then(|ts| chrono::DateTime::<Utc>::from_timestamp(ts, 0)),
        Value::Null => Some(Utc::now()),
        _ => None,
    }
}

#[async_trait]
impl crate::nodes::trait_::NodeExecutor for DateTimeNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "logic.datetime".into(), name: "Date / Heure".into(),
            description: "Calcule, formate ou compare des dates".into(),
            category: NodeCategory::Logic, icon: "Clock".into(), color: "#f9ab00".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::new("operation", "Opération", FieldType::Select).options(&[
                    ("now","Maintenant"),("format","Formater"),("add","Ajouter une durée"),
                    ("diff","Différence"),("timestamp","Horodatage Unix"),
                ]).default(json!("now")),
                FieldDef::new("date", "Date", FieldType::Expression).placeholder("{{ trigger.date }} — vide = maintenant"),
                FieldDef::new("date2", "Deuxième date (différence)", FieldType::Expression),
                FieldDef::new("format", "Format (formater)", FieldType::Text).placeholder("%d/%m/%Y %H:%M").default(json!("%Y-%m-%d %H:%M:%S")),
                FieldDef::new("amount", "Quantité (ajouter)", FieldType::Number).default(json!(1)),
                FieldDef::new("unit", "Unité", FieldType::Select).options(&[
                    ("seconds","Secondes"),("minutes","Minutes"),("hours","Heures"),("days","Jours"),
                ]).default(json!("days")),
            ],
        }
    }
    async fn execute(&self, config: Value, _ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        use chrono::Duration;
        let op = config.get("operation").and_then(|v| v.as_str()).unwrap_or("now");
        let date = config.get("date").cloned().unwrap_or(Value::Null);
        let dt = parse_dt(&date).ok_or_else(|| NodeError::InvalidConfig("Date invalide".into()))?;

        let result = match op {
            "now"       => json!({ "result": chrono::Utc::now().to_rfc3339() }),
            "timestamp" => json!({ "result": dt.timestamp() }),
            "format"    => {
                let fmt = config.get("format").and_then(|v| v.as_str()).unwrap_or("%Y-%m-%d %H:%M:%S");
                json!({ "result": dt.format(fmt).to_string() })
            }
            "add" => {
                let n = config.get("amount").and_then(as_f64).unwrap_or(0.0) as i64;
                let unit = config.get("unit").and_then(|v| v.as_str()).unwrap_or("days");
                let delta = match unit {
                    "seconds" => Duration::seconds(n),
                    "minutes" => Duration::minutes(n),
                    "hours"   => Duration::hours(n),
                    _         => Duration::days(n),
                };
                json!({ "result": (dt + delta).to_rfc3339() })
            }
            "diff" => {
                let d2 = parse_dt(&config.get("date2").cloned().unwrap_or(Value::Null))
                    .ok_or_else(|| NodeError::InvalidConfig("Deuxième date invalide".into()))?;
                let unit = config.get("unit").and_then(|v| v.as_str()).unwrap_or("days");
                let secs = (dt - d2).num_seconds() as f64;
                let v = match unit {
                    "seconds" => secs,
                    "minutes" => secs / 60.0,
                    "hours"   => secs / 3600.0,
                    _         => secs / 86400.0,
                };
                json!({ "result": v })
            }
            _ => json!({ "result": dt.to_rfc3339() }),
        };
        Ok(NodeOutput::data(result))
    }
}

// ── Helpers tableaux ─────────────────────────────────────────────────────────────

fn as_array(v: &Value) -> Vec<Value> {
    match v {
        Value::Array(a) => a.clone(),
        Value::Null => vec![],
        other => vec![other.clone()],
    }
}

/// Compare deux valeurs pour le tri : numérique si possible, sinon texte.
fn cmp_vals(a: &Value, b: &Value) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (as_f64(a), as_f64(b)) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
        _ => a.to_string().cmp(&b.to_string()),
    }
}

fn pick_field<'a>(item: &'a Value, field: &str) -> &'a Value {
    if field.is_empty() { item } else { item.get(field).unwrap_or(&Value::Null) }
}

// ── Sort ─────────────────────────────────────────────────────────────────────────

pub struct SortNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for SortNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "logic.sort".into(), name: "Trier".into(),
            description: "Trie un tableau selon un champ".into(),
            category: NodeCategory::Logic, icon: "ArrowUpDown".into(), color: "#f9ab00".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::new("field", "Champ", FieldType::Text).placeholder("montant (vide = la valeur elle-même)"),
                FieldDef::new("order", "Ordre", FieldType::Select).options(&[("asc","Croissant"),("desc","Décroissant")]).default(json!("asc")),
            ],
        }
    }
    async fn execute(&self, config: Value, ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let field = config.get("field").and_then(|v| v.as_str()).unwrap_or("");
        let desc = config.get("order").and_then(|v| v.as_str()).unwrap_or("asc") == "desc";
        let mut arr = as_array(&ctx.input);
        arr.sort_by(|a, b| {
            let o = cmp_vals(pick_field(a, field), pick_field(b, field));
            if desc { o.reverse() } else { o }
        });
        Ok(NodeOutput::data(Value::Array(arr)))
    }
}

// ── Limit ────────────────────────────────────────────────────────────────────────

pub struct LimitNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for LimitNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "logic.limit".into(), name: "Limiter".into(),
            description: "Garde les N premiers (ou derniers) éléments".into(),
            category: NodeCategory::Logic, icon: "ListEnd".into(), color: "#f9ab00".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::new("count", "Nombre", FieldType::Number).default(json!(10)),
                FieldDef::new("from", "Depuis", FieldType::Select).options(&[("start","Le début"),("end","La fin")]).default(json!("start")),
            ],
        }
    }
    async fn execute(&self, config: Value, ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let count = config.get("count").and_then(as_f64).unwrap_or(10.0).max(0.0) as usize;
        let from_end = config.get("from").and_then(|v| v.as_str()).unwrap_or("start") == "end";
        let arr = as_array(&ctx.input);
        let out: Vec<Value> = if from_end {
            arr.iter().rev().take(count).cloned().collect::<Vec<_>>().into_iter().rev().collect()
        } else {
            arr.into_iter().take(count).collect()
        };
        Ok(NodeOutput::data(Value::Array(out)))
    }
}

// ── Remove duplicates ────────────────────────────────────────────────────────────

pub struct RemoveDuplicatesNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for RemoveDuplicatesNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "logic.unique".into(), name: "Dédoublonner".into(),
            description: "Retire les éléments en double d'un tableau".into(),
            category: NodeCategory::Logic, icon: "CopyMinus".into(), color: "#f9ab00".into(),
            inputs: 1, outputs: vec![],
            fields: vec![ FieldDef::new("field", "Champ de comparaison", FieldType::Text).placeholder("vide = l'élément entier") ],
        }
    }
    async fn execute(&self, config: Value, ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let field = config.get("field").and_then(|v| v.as_str()).unwrap_or("");
        let arr = as_array(&ctx.input);
        let mut seen: Vec<String> = Vec::new();
        let mut out: Vec<Value> = Vec::new();
        for it in arr {
            let key = pick_field(&it, field).to_string();
            if !seen.contains(&key) { seen.push(key); out.push(it); }
        }
        Ok(NodeOutput::data(Value::Array(out)))
    }
}

// ── Rename keys ──────────────────────────────────────────────────────────────────

pub struct RenameKeysNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for RenameKeysNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "logic.rename_keys".into(), name: "Renommer clés".into(),
            description: "Renomme des champs d'un objet (ou de chaque élément)".into(),
            category: NodeCategory::Logic, icon: "Tags".into(), color: "#f9ab00".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::new("mapping", "Correspondances (JSON)", FieldType::Json)
                    .help(r#"{"ancien":"nouveau", "email":"courriel"}"#),
            ],
        }
    }
    async fn execute(&self, config: Value, ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let mapping = config.get("mapping").and_then(|v| v.as_object()).cloned().unwrap_or_default();
        let rename = |obj: &Value| -> Value {
            let Some(map) = obj.as_object() else { return obj.clone() };
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                let nk = mapping.get(k).and_then(|x| x.as_str()).unwrap_or(k).to_string();
                out.insert(nk, v.clone());
            }
            Value::Object(out)
        };
        let result = match &ctx.input {
            Value::Array(a) => Value::Array(a.iter().map(rename).collect()),
            other => rename(other),
        };
        Ok(NodeOutput::data(result))
    }
}

// ── Edit fields (multi-set) ──────────────────────────────────────────────────────

pub struct EditFieldsNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for EditFieldsNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "logic.edit_fields".into(), name: "Éditer les champs".into(),
            description: "Définit plusieurs champs d'un coup (objet ou chaque élément)".into(),
            category: NodeCategory::Logic, icon: "PenLine".into(), color: "#f9ab00".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::new("assignments", "Champs (JSON)", FieldType::Json)
                    .help(r#"{"nomComplet":"{{ $json.prenom }} {{ $json.nom }}", "actif":true} — valeurs déjà résolues"#),
                FieldDef::new("keep_only", "Ne garder que ces champs", FieldType::Boolean).default(json!(false)),
            ],
        }
    }
    async fn execute(&self, config: Value, ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let assignments = config.get("assignments").and_then(|v| v.as_object()).cloned().unwrap_or_default();
        let keep_only = config.get("keep_only").and_then(|v| v.as_bool()).unwrap_or(false);
        let apply = |item: &Value| -> Value {
            let mut out = if keep_only {
                serde_json::Map::new()
            } else {
                item.as_object().cloned().unwrap_or_default()
            };
            for (k, v) in &assignments { out.insert(k.clone(), v.clone()); }
            Value::Object(out)
        };
        let result = match &ctx.input {
            Value::Array(a) => Value::Array(a.iter().map(apply).collect()),
            other => apply(other),
        };
        Ok(NodeOutput::data(result))
    }
}

// ── Hash / Encode ────────────────────────────────────────────────────────────────

pub struct CryptoNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for CryptoNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "logic.crypto".into(), name: "Hash / Encodage".into(),
            description: "Hache ou encode une valeur (SHA-256, Base64, hexadécimal)".into(),
            category: NodeCategory::Logic, icon: "Hash".into(), color: "#f9ab00".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::new("operation", "Opération", FieldType::Select).options(&[
                    ("sha256","SHA-256"),("base64_encode","Base64 (encoder)"),("base64_decode","Base64 (décoder)"),
                    ("hex_encode","Hexadécimal"),
                ]).default(json!("sha256")),
                FieldDef::new("value", "Valeur", FieldType::Expression).required(),
            ],
        }
    }
    async fn execute(&self, config: Value, _ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        use base64::Engine;
        use sha2::{Digest, Sha256};
        let op = config.get("operation").and_then(|v| v.as_str()).unwrap_or("sha256");
        let value = config.get("value").map(|v| match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        }).unwrap_or_default();
        let b64 = base64::engine::general_purpose::STANDARD;
        let result = match op {
            "sha256" => {
                let mut h = Sha256::new();
                h.update(value.as_bytes());
                hex::encode(h.finalize())
            }
            "base64_encode" => b64.encode(value.as_bytes()),
            "base64_decode" => {
                let bytes = b64.decode(value.as_bytes())
                    .map_err(|e| NodeError::InvalidConfig(format!("Base64 invalide : {e}")))?;
                String::from_utf8_lossy(&bytes).to_string()
            }
            "hex_encode" => hex::encode(value.as_bytes()),
            _ => value,
        };
        Ok(NodeOutput::data(json!({ "result": result })))
    }
}

// ── Random ───────────────────────────────────────────────────────────────────────

pub struct RandomNode;
#[async_trait]
impl crate::nodes::trait_::NodeExecutor for RandomNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type: "logic.random".into(), name: "Aléatoire".into(),
            description: "Génère un nombre, un UUID ou tire un élément au hasard".into(),
            category: NodeCategory::Logic, icon: "Dices".into(), color: "#f9ab00".into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::new("operation", "Type", FieldType::Select).options(&[
                    ("integer","Entier"),("float","Décimal"),("uuid","UUID"),("pick","Tirer un élément (entrée tableau)"),
                ]).default(json!("integer")),
                FieldDef::new("min", "Min", FieldType::Number).default(json!(0)),
                FieldDef::new("max", "Max", FieldType::Number).default(json!(100)),
            ],
        }
    }
    async fn execute(&self, config: Value, ctx: &ExecutionContext, _n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        use rand::Rng;
        let op = config.get("operation").and_then(|v| v.as_str()).unwrap_or("integer");
        let min = config.get("min").and_then(as_f64).unwrap_or(0.0);
        let max = config.get("max").and_then(as_f64).unwrap_or(100.0);
        let mut rng = rand::thread_rng();
        let result = match op {
            "integer" => {
                let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
                json!(rng.gen_range(lo as i64..=hi as i64))
            }
            "float" => {
                let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
                json!(if (hi - lo).abs() < f64::EPSILON { lo } else { rng.gen_range(lo..hi) })
            }
            "uuid" => json!(uuid::Uuid::new_v4().to_string()),
            "pick" => {
                let arr = ctx.input.as_array().cloned().unwrap_or_default();
                if arr.is_empty() { Value::Null } else { arr[rng.gen_range(0..arr.len())].clone() }
            }
            _ => Value::Null,
        };
        Ok(NodeOutput::data(json!({ "result": result })))
    }
}
