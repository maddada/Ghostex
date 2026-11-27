//! `ghostex sessions search <text>`: every chat's messages, searched by gxserver's chat search
//! index (server/src/chat_search/), for scripts and clients that reach the computer over SSH.

use serde_json::{Map, Value};

use crate::ghostex_cli::args::parse_args;
use crate::ghostex_cli::output::{is_failed_cli_result, print_json};
use crate::ghostex_cli::rpc::{call_gxserver_rpc, CliError, CliResult};
use crate::ghostex_cli::set_exit_code;

fn usage() -> &'static str {
    "Usage: ghostex sessions search <text> [--project-id <id>] [--limit <n>] [--json]"
}

/// `args` are the words after `search`.
pub fn sessions_search_command(args: &[String]) -> CliResult<()> {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("{}", usage());
        return Ok(());
    }
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    let text = parsed.rest.join(" ");
    if text.trim().is_empty() {
        return Err(CliError::Other(usage().to_string()));
    }
    let mut params = Map::new();
    params.insert("query".to_string(), Value::String(text));
    if let Some(project_id) = flags.string_value("projectId").map(str::trim) {
        if !project_id.is_empty() {
            params.insert(
                "projectId".to_string(),
                Value::String(project_id.to_string()),
            );
        }
    }
    if let Some(limit) = flags.number("limit").filter(|limit| *limit >= 1.0) {
        params.insert("limit".to_string(), Value::from(limit as u64));
    }
    let result = call_gxserver_rpc(
        crate::chat_search::SEARCH_ENDPOINT,
        &Value::Object(params),
        flags,
    )?;
    if flags.truthy("json") || is_failed_cli_result(&result) {
        print_json(&result);
        if is_failed_cli_result(&result) {
            set_exit_code(1);
        }
        return Ok(());
    }
    let empty = Vec::new();
    let results = result
        .get("results")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    for row in results {
        let text = |key: &str| row.get(key).and_then(Value::as_str).unwrap_or_default();
        let who = if text("source") == "user" {
            "You"
        } else {
            "Agent"
        };
        println!(
            "{}  [{}:{}]  {}",
            text("title"),
            text("projectId"),
            text("sessionId"),
            text("lifecycleState")
        );
        println!("    {who}: {}", text("snippet"));
    }
    let took = result.get("tookMs").and_then(Value::as_f64).unwrap_or(0.0);
    let ready = result
        .pointer("/index/ready")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    println!(
        "{} chat{} in {took:.1} ms{}",
        results.len(),
        if results.len() == 1 { "" } else { "s" },
        if ready {
            ""
        } else {
            " (still reading older chats)"
        }
    );
    Ok(())
}
