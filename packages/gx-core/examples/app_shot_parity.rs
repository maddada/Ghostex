//! The Rust half of the App Shot prompt check: reads capture cases (JSON array of the native
//! payload's fields) from the file named first and prints, per case, the bounded capture and the
//! prompt with and without metadata, in the shape the TypeScript `normalizeGpuiNativeAppShotCapture`
//! and `formatGpuiNativeAppShotPrompt` produce.
//!
//!   cargo run --example app_shot_parity -- cases.json

use ghostex_gx_core::app_shot::{
    format_app_shot_prompt, normalize_app_shot_capture, AppShotCaptureInput,
};
use serde_json::{json, Map, Value};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: app_shot_parity <cases.json>");
    let cases: Vec<Value> =
        serde_json::from_str(&std::fs::read_to_string(path).expect("read")).expect("json");
    let out: Vec<Value> = cases
        .iter()
        .map(|case| {
            let text = |key: &str| case.get(key).and_then(Value::as_str);
            let input = AppShotCaptureInput {
                app_name: text("appName").unwrap_or(""),
                image_path: text("imagePath").unwrap_or(""),
                bundle_identifier: text("bundleIdentifier"),
                window_title: text("windowTitle"),
                window_width: case.get("windowWidth").and_then(Value::as_i64),
                window_height: case.get("windowHeight").and_then(Value::as_i64),
                trigger: text("trigger"),
            };
            let Some(capture) = normalize_app_shot_capture(&input) else {
                return Value::Null;
            };
            let mut n = Map::new();
            n.insert("appName".into(), json!(capture.app_name));
            n.insert("imagePath".into(), json!(capture.image_path));
            for (key, value) in [
                (
                    "bundleIdentifier",
                    capture.bundle_identifier.clone().map(Value::from),
                ),
                ("windowTitle", capture.window_title.clone().map(Value::from)),
                ("windowWidth", capture.window_width.map(Value::from)),
                ("windowHeight", capture.window_height.map(Value::from)),
                ("trigger", capture.trigger.clone().map(Value::from)),
            ] {
                if let Some(value) = value {
                    n.insert(key.into(), value);
                }
            }
            json!({
                "n": n,
                "plain": format_app_shot_prompt(&capture, false),
                "meta": format_app_shot_prompt(&capture, true),
            })
        })
        .collect();
    println!("{}", Value::Array(out));
}
