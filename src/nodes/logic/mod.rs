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
