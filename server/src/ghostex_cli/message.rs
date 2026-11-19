//! `ghostex message`: an agent reads one of the user's numbered messages (`show M12`, or
//! `show G9jt7.M12` for another chat's) or lists them (`list`), so a reference such as "do it like
//! M12" can be looked up in full (server/src/chat_message_numbers.rs).

use serde_json::{json, Map, Value};

use crate::ghostex_cli::args::{parse_args, ParsedArgs};
use crate::ghostex_cli::output::print_json;
use crate::ghostex_cli::rpc::{call_gxserver_rpc, CliError, CliResult};
use crate::ghostex_cli::{notify, selector, usage};

pub fn message_command(args: &[String]) -> CliResult<()> {
    let subcommand = args.first().map(String::as_str).unwrap_or("help");
    let rest = args.get(1..).unwrap_or_default();
    if matches!(subcommand, "help" | "-h" | "--help")
        || rest
            .iter()
            .any(|arg| matches!(arg.as_str(), "-h" | "--help"))
    {
        println!("{}", usage::message_usage());
        return Ok(());
    }
    let parsed = parse_args(rest);
    let (project_id, session_id) = session(&parsed)?;
    let mut params = Map::new();
    params.insert("projectId".into(), json!(project_id));
    params.insert("sessionId".into(), json!(session_id));
    match subcommand {
        "show" => {
            let reference = parsed.rest.first().cloned().ok_or_else(|| {
                CliError::Other(format!(
                    "show needs a message ID: M12, or G9jt7.M12 for another chat's.\n\n{}",
                    usage::message_usage()
                ))
            })?;
            params.insert("op".into(), json!("show"));
            params.insert("ref".into(), json!(reference));
        }
        "list" => {
            params.insert("op".into(), json!("list"));
            if let Some(last) = flag(&parsed, "last").and_then(|last| last.parse::<u64>().ok()) {
                params.insert("last".into(), json!(last));
            }
            if let Some(grep) = flag(&parsed, "grep") {
                params.insert("grep".into(), json!(grep));
            }
        }
        other => {
            return Err(CliError::Other(format!(
                "Unknown message command: {other}\n\n{}",
                usage::message_usage()
            )))
        }
    }
    let answer = call_gxserver_rpc(
        crate::chat_message_numbers::CHAT_MESSAGES_ENDPOINT,
        &Value::Object(params),
        &parsed.flags,
    )?;
    if parsed.flags.truthy("json") {
        print_json(&answer);
        return Ok(());
    }
    if subcommand == "show" {
        let message = &answer["message"];
        let sent = message["sent"].as_str().unwrap_or_default();
        println!(
            "{}{}",
            message["ref"].as_str().unwrap_or_default(),
            if sent.is_empty() {
                String::new()
            } else {
                format!(" (sent {sent})")
            }
        );
        println!();
        println!("{}", message["text"].as_str().unwrap_or_default());
        return Ok(());
    }
    let messages = answer["messages"].as_array().cloned().unwrap_or_default();
    if messages.is_empty() {
        println!("No numbered messages in this chat yet.");
        return Ok(());
    }
    for message in messages {
        println!(
            "{:>5}  {}  {}",
            message["id"].as_str().unwrap_or_default(),
            message["sent"].as_str().unwrap_or_default(),
            message["text"].as_str().unwrap_or_default()
        );
    }
    Ok(())
}

fn flag(parsed: &ParsedArgs, key: &str) -> Option<String> {
    parsed
        .flags
        .string_value(key)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn session(parsed: &ParsedArgs) -> CliResult<(String, String)> {
    if let Some(selector_text) = flag(parsed, "sessionId").or_else(|| flag(parsed, "session")) {
        let session = selector::resolve_cli_session_selector(&selector_text, &parsed.flags)?;
        let text = |key: &str| session.get(key).and_then(Value::as_str).map(str::to_string);
        return match (text("projectId"), text("sessionId")) {
            (Some(project_id), Some(session_id)) => Ok((project_id, session_id)),
            _ => Err(CliError::Other(format!(
                "Session \"{selector_text}\" does not report a project id."
            ))),
        };
    }
    notify::session_reference_from_environment().ok_or_else(|| {
        CliError::Other(
            "message must run inside a Ghostex session (it reads that chat's messages), or name one with --session."
                .to_string(),
        )
    })
}
