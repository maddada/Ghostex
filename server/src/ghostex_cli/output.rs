use serde_json::Value;

pub fn print_json(value: &Value) {
    let mut normalized = value.clone();
    normalize_js_numbers(&mut normalized);
    println!(
        "{}",
        serde_json::to_string_pretty(&normalized).unwrap_or_else(|_| "null".to_string())
    );
}

/*
The Node CLI parses every RPC response with JSON.parse, so all numbers become
JS doubles and integral values print without a fractional part (0.0 on the
wire prints as 0). serde_json preserves the wire text instead; normalize
integral floats to integers so printed JSON matches the Node CLI.
*/
pub fn normalize_js_numbers(value: &mut Value) {
    match value {
        Value::Number(number) => {
            if let Some(float) = number.as_f64() {
                if number.as_i64().is_none()
                    && number.as_u64().is_none()
                    && float.fract() == 0.0
                    && float.abs() <= 9_007_199_254_740_992.0
                {
                    *value = Value::from(float as i64);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                normalize_js_numbers(item);
            }
        }
        Value::Object(map) => {
            for (_, item) in map.iter_mut() {
                normalize_js_numbers(item);
            }
        }
        _ => {}
    }
}

pub fn is_failed_cli_result(result: &Value) -> bool {
    result.get("ok") == Some(&Value::Bool(false))
        || result.get("bridgeOk") == Some(&Value::Bool(false))
}

pub fn cli_args_want_json(args: &[String]) -> bool {
    args.iter()
        .any(|arg| arg == "--json" || arg == "-json" || arg.starts_with("--json="))
}

/// JS `new Date().toISOString().replace(/[:.]/g, "-")`.
pub fn timestamp_slug() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%dT%H-%M-%S-%3fZ")
        .to_string()
}
