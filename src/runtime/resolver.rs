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

/// Résout une chaîne. Si la chaîne entière est une unique expression `{{ x }}`,
/// la valeur typée correspondante est retournée (objet, nombre, booléen…).
/// Sinon, interpolation textuelle.
pub fn resolve_string(s: &str, ctx: &Value) -> Value {
    let trimmed = s.trim();
    if trimmed.starts_with("{{") && trimmed.ends_with("}}") {
        let inner = &trimmed[2..trimmed.len() - 2];
        // Une seule expression et pas d'autre `{{` à l'intérieur → valeur typée.
        if !inner.contains("{{") {
            return lookup(inner.trim(), ctx);
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
                let v = lookup(expr.trim(), ctx);
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

/// Navigue dans le contexte selon une expression dot-notation.
/// Retourne `Value::Null` si le chemin n'existe pas.
fn lookup(expr: &str, ctx: &Value) -> Value {
    let mut current = ctx;
    for raw_part in expr.split('.') {
        let part = raw_part.trim();
        if part.is_empty() {
            continue;
        }
        // Gère foo[0][1]
        let mut name = part;
        let mut indices: Vec<usize> = Vec::new();
        if let Some(bracket) = part.find('[') {
            name = &part[..bracket];
            let mut rest = &part[bracket..];
            while let Some(open) = rest.find('[') {
                if let Some(close) = rest[open..].find(']') {
                    if let Ok(n) = rest[open + 1..open + close].parse::<usize>() {
                        indices.push(n);
                    }
                    rest = &rest[open + close + 1..];
                } else {
                    break;
                }
            }
        }

        if !name.is_empty() {
            match current.get(name) {
                Some(v) => current = v,
                None => return Value::Null,
            }
        }
        for idx in indices {
            match current.get(idx) {
                Some(v) => current = v,
                None => return Value::Null,
            }
        }
    }
    current.clone()
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
}
