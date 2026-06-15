//! Nœud Code — JavaScript inline via QuickJS (rquickjs). Traitement interne
//! uniquement : pas d'accès réseau (fetch/XHR retirés). Pour les appels HTTP,
//! utiliser un nœud HTTP Request séparé.

use async_trait::async_trait;
use serde_json::{json, Value};
use std::time::Duration;

use crate::nodes::trait_::{
    ExecutionContext, FieldDef, FieldType, NodeCategory, NodeContext, NodeError, NodeMeta, NodeOutput,
};

pub struct CodeNode;

#[async_trait]
impl crate::nodes::trait_::NodeExecutor for CodeNode {
    fn meta(&self) -> NodeMeta {
        NodeMeta {
            node_type:   "code.js".into(),
            name:        "Code (JavaScript)".into(),
            description: "Transforme les données avec du JavaScript ($input, $json)".into(),
            category:    NodeCategory::Code,
            icon:        "Code".into(),
            color:       "#5f6368".into(),
            inputs:      1,
            outputs:     vec![],
            fields:      vec![
                FieldDef::new("code", "Code JavaScript", FieldType::Code)
                    .default(json!("const items = $input.all()\nreturn items"))
                    .help("Retourne la valeur de sortie. $input.all() / $input.first() / $json disponibles."),
            ],
        }
    }

    async fn execute(&self, config: Value, ctx: &ExecutionContext, node_ctx: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let code = config.get("code").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let input = ctx.input.clone();
        let timeout = Duration::from_secs(node_ctx.settings.code_node.timeout_secs.max(1));

        let result = tokio::time::timeout(
            timeout + Duration::from_millis(500),
            tokio::task::spawn_blocking(move || run_js(&code, &input, timeout)),
        )
        .await;

        match result {
            Ok(Ok(Ok(value))) => Ok(NodeOutput::data(value)),
            Ok(Ok(Err(e)))    => Err(NodeError::Other(format!("Code JS : {e}"))),
            Ok(Err(e))        => Err(NodeError::Other(format!("Thread Code paniqué : {e}"))),
            Err(_)            => Err(NodeError::Other("Le nœud Code a dépassé son délai".into())),
        }
    }
}

fn run_js(code: &str, input: &Value, _timeout: Duration) -> Result<Value, String> {
    use rquickjs::{Context, Runtime};

    let rt = Runtime::new().map_err(|e| format!("runtime QuickJS : {e}"))?;
    let context = Context::full(&rt).map_err(|e| format!("contexte QuickJS : {e}"))?;

    let input_json = serde_json::to_string(input).unwrap_or_else(|_| "null".into());

    context.with(|ctx| {
        // Durcissement : retirer les capacités réseau / système.
        ctx.eval::<(), _>(
            "var fetch=undefined; var XMLHttpRequest=undefined; var require=undefined; var process=undefined;",
        ).map_err(|e| e.to_string())?;

        let wrapped = format!(
            r#"globalThis.__RESULT__ = (function() {{
                var __INPUT__ = {input_json};
                var items = Array.isArray(__INPUT__) ? __INPUT__ : [__INPUT__];
                var $input = {{
                    all:   function() {{ return items.map(function(x) {{ return {{ json: x }}; }}); }},
                    first: function() {{ return {{ json: items[0] }}; }}
                }};
                var $json = items[0];
                {code}
            }})();"#
        );

        ctx.eval::<(), _>(wrapped).map_err(|e| e.to_string())?;

        let s: rquickjs::String = ctx
            .eval("JSON.stringify(globalThis.__RESULT__ === undefined ? null : globalThis.__RESULT__)")
            .map_err(|e| e.to_string())?;
        let json_str = s.to_string().map_err(|e| e.to_string())?;
        let value: Value = serde_json::from_str(&json_str).unwrap_or(Value::Null);
        Ok(value)
    })
}
