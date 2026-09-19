//! Résolution d'expressions `{{ chemin.vers.champ }}`.
//!
//! Sécurisé par conception : aucune évaluation de code arbitraire. Seule la
//! dot-notation (avec indexation de tableau `[n]`) est supportée, naviguant dans
//! un contexte JSON (`trigger`, `nodes.<id>`, variables, etc.).

use serde_json::Value;

/// Résout récursivement toutes les expressions d'une valeur JSON de configuration.
pub fn resolve_value(value: &Value, ctx: &Value) -> Value {
    match value {
        Value::String(s) => resolve_string(s, ctx),
        Value::Array(arr) => Value::Array(arr.iter().map(|v| resolve_value(v, ctx)).collect()),
        Value::Object(obj) => {
            let mut out = serde_json::Map::with_capacity(obj.len());
            for (k, v) in obj {
                out.insert(k.clone(), resolve_value(v, ctx));
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

/// Puts back, into an already-resolved config, the fields the node declared as
/// `literal` — their stored text is used exactly as authored.
///
/// `resolve_value` walks the whole config and cannot tell an SQL statement from
/// a label, so the statement is restored here rather than skipped there. See
/// `FieldDef::literal` for why an SQL statement must never carry expressions.
pub fn restore_literal_fields(
    registry: &crate::nodes::NodeRegistry,
    node_type: &str,
    raw: &Value,
    resolved: &mut Value,
) {
    let Some(meta) = registry.meta(node_type) else { return };
    for f in &meta.fields {
        if !f.literal { continue; }
        if let Some(original) = raw.get(&f.name) {
            resolved[&f.name] = original.clone();
        }
    }
}

/// Résout une chaîne. Si la chaîne entière est une unique expression `{{ x }}`,
/// la valeur typée correspondante est retournée (objet, nombre, booléen…).
/// Sinon, interpolation textuelle.
pub fn resolve_string(s: &str, ctx: &Value) -> Value {
    let trimmed = s.trim();
    if trimmed.starts_with("{{") && trimmed.ends_with("}}") {
        let inner = &trimmed[2..trimmed.len() - 2];
        // Une seule expression et pas d'autre `{{` à l'intérieur → valeur typée.
        if !inner.contains("{{") {
            return crate::runtime::expr::evaluate(inner.trim(), ctx);
        }
    }

    // Interpolation : remplacer chaque {{ ... }} par sa représentation texte.
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'{' {
            if let Some(end) = s[i + 2..].find("}}") {
                let expr = &s[i + 2..i + 2 + end];
                let v = crate::runtime::expr::evaluate(expr.trim(), ctx);
                out.push_str(&value_to_text(&v));
                i = i + 2 + end + 2;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    Value::String(out)
}

fn value_to_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn single_expr_returns_typed() {
        let ctx = json!({ "trigger": { "amount": 42 } });
        assert_eq!(resolve_string("{{ trigger.amount }}", &ctx), json!(42));
    }

    #[test]
    fn interpolation() {
        let ctx = json!({ "trigger": { "name": "Bob" } });
        assert_eq!(
            resolve_string("Bonjour {{trigger.name}} !", &ctx),
            json!("Bonjour Bob !")
        );
    }

    #[test]
    fn array_index() {
        let ctx = json!({ "items": [{ "id": 1 }, { "id": 2 }] });
        assert_eq!(resolve_string("{{ items[1].id }}", &ctx), json!(2));
    }

    #[test]
    fn missing_is_null() {
        let ctx = json!({});
        assert_eq!(resolve_string("{{ nope.nope }}", &ctx), Value::Null);
    }

    /// A webhook-triggered workflow receives its body from whoever calls the
    /// public URL. If that body could reach the text of an SQL statement, an
    /// anonymous caller would be writing SQL against the owner's database, so
    /// the statement must come out exactly as it was authored.
    #[test]
    fn sql_statement_is_never_built_from_trigger_data() {
        let registry = crate::nodes::build_registry();
        let ctx = json!({ "trigger": { "body": { "email": "x' OR '1'='1" } } });
        let raw = json!({
            "query":  "SELECT * FROM clients WHERE email = '{{ trigger.body.email }}'",
            "params": [],
            "label":  "Lookup for {{ trigger.body.email }}",
        });

        let mut resolved = resolve_value(&raw, &ctx);
        restore_literal_fields(&registry, "db.postgres", &raw, &mut resolved);

        assert_eq!(resolved["query"], raw["query"], "the statement must stay verbatim");
        // Fields that are not marked literal keep being resolved as before.
        assert_eq!(resolved["label"], json!("Lookup for x' OR '1'='1"));
    }

    #[test]
    fn mysql_statement_is_also_verbatim() {
        let registry = crate::nodes::build_registry();
        let ctx = json!({ "trigger": { "body": { "id": "1; DROP TABLE clients" } } });
        let raw = json!({ "query": "SELECT * FROM t WHERE id = {{ trigger.body.id }}" });

        let mut resolved = resolve_value(&raw, &ctx);
        restore_literal_fields(&registry, "db.mysql", &raw, &mut resolved);

        assert_eq!(resolved["query"], raw["query"]);
    }
}
