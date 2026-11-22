//! `ghostex needs-you`: an agent puts a step only the user can do (a sudo command, a tap on the
//! phone, a sign-in, a check by eye) above its chat box as a to-do with a Done button
//! (server/src/session_needs_you/).

use serde_json::{json, Map, Value};

use crate::ghostex_cli::args::{parse_args, ParsedArgs};
use crate::ghostex_cli::output::print_json;
use crate::ghostex_cli::rpc::{call_gxserver_rpc, CliError, CliResult};
use crate::ghostex_cli::{notify, selector, usage};

pub fn needs_you_command(args: &[String]) -> CliResult<()> {
    let subcommand = args.first().map(String::as_str).unwrap_or("help");
    let rest = args.get(1..).unwrap_or_default();
    if matches!(subcommand, "help" | "-h" | "--help")
        || rest
            .iter()
            .any(|arg| matches!(arg.as_str(), "-h" | "--help"))
    {
        println!("{}", usage::needs_you_usage());
        return Ok(());
    }
    let parsed = parse_args(rest);
    let (project_id, session_id) = session(&parsed)?;
    let mut params = Map::new();
    params.insert("projectId".into(), json!(project_id));
    params.insert("sessionId".into(), json!(session_id));
    match subcommand {
        "add" => {
            let title = parsed
                .rest
                .first()
                .cloned()
                .or_else(|| flag(&parsed, "title"))
                .ok_or_else(|| {
                    usage_error("add needs what the user should do, as a short title.")
                })?;
            params.insert("op".into(), json!("add"));
            params.insert("title".into(), json!(title));
            for key in ["detail", "command"] {
                if let Some(value) = flag(&parsed, key) {
                    params.insert(key.into(), json!(value));
                }
            }
            // `--answer` repeated keeps each answer whole, commas and all.
            let answers = repeated(rest, "answer");
            if !answers.is_empty() {
                params.insert("answers".into(), json!(answers));
            }
        }
        "list" => {
            params.insert("op".into(), json!("list"));
            params.insert("all".into(), json!(parsed.flags.truthy("all")));
        }
        "done" | "drop" => {
            let tag =
                parsed.rest.first().cloned().ok_or_else(|| {
                    usage_error(&format!("{subcommand} needs the to-do's tag: Q61."))
                })?;
            params.insert("op".into(), json!(subcommand));
            params.insert("tag".into(), json!(tag));
        }
        other => {
            return Err(CliError::Other(format!(
                "Unknown needs-you command: {other}\n\n{}",
                usage::needs_you_usage()
            )))
        }
    }
    let answer = call_gxserver_rpc(
        crate::session_needs_you::SESSION_NEEDS_YOU_ENDPOINT,
        &Value::Object(params),
        &parsed.flags,
    )?;
    if parsed.flags.truthy("json") {
        print_json(&answer);
        return Ok(());
    }
    print_answer(subcommand, &answer);
    Ok(())
}

fn print_answer(subcommand: &str, answer: &Value) {
    let text = |key: &str| answer.get(key).and_then(Value::as_str).unwrap_or_default();
    match subcommand {
        "add" if answer["coordinator"] == true => println!(
            "{} added to your coordinator's Needs you ({}). The user's answer arrives as a message: \"{}: Done\".",
            text("tag"),
            text("item"),
            text("tag"),
        ),
        "add" => println!(
            "{} added above the chat box. The user's answer arrives as a message: \"{}: Done\".",
            text("tag"),
            text("tag"),
        ),
        "list" => {
            let items = answer["items"].as_array().cloned().unwrap_or_default();
            if items.is_empty() {
                println!("No open to-dos for the user.");
            }
            for item in items {
                let field = |key: &str| item.get(key).and_then(Value::as_str).unwrap_or_default();
                let mut line = format!("{} {}", field("tag"), field("title"));
                if !field("command").is_empty() {
                    line.push_str(&format!("  [{}]", field("command")));
                }
                println!("{line}");
            }
            for settled in answer["settled"].as_array().into_iter().flatten() {
                let field =
                    |key: &str| settled.get(key).and_then(Value::as_str).unwrap_or_default();
                let how = if field("answer").is_empty() {
                    field("outcome").to_string()
                } else {
                    format!("{}: {}", field("outcome"), field("answer"))
                };
                println!("{} {} ({how})", field("tag"), field("title"));
            }
        }
        _ => println!("{} cleared.", text("tag")),
    }
}

/// Every value of a flag given more than once (`--answer A --answer B`), each kept whole.
fn repeated(args: &[String], name: &str) -> Vec<String> {
    let flag = format!("--{name}");
    let inline = format!("--{name}=");
    let mut values = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if let Some(value) = args[index].strip_prefix(&inline) {
            values.push(value.to_string());
        } else if args[index] == flag {
            if let Some(value) = args.get(index + 1) {
                values.push(value.clone());
                index += 1;
            }
        }
        index += 1;
    }
    values
}

fn flag(parsed: &ParsedArgs, key: &str) -> Option<String> {
    parsed
        .flags
        .string_value(key)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn usage_error(message: &str) -> CliError {
    CliError::Other(format!("{message}\n\n{}", usage::needs_you_usage()))
}

/// The session the to-do is for: `--session`, else the session this command runs in.
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
            "needs-you must run inside a Ghostex session (the to-do shows above its chat box), or name one with --session."
                .to_string(),
        )
    })
}
