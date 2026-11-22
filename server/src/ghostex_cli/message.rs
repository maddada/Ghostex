//! `ghostex message`: an agent reads one of the user's numbered messages (`show M12`, or
//! `show G9jt7.M12` for another chat's) or lists them (`list`), so a reference such as "do it like
//! M12" can be looked up in full (server/src/chat_message_numbers.rs).

use serde_json::{Map, Value, json};

use crate::ghostex_cli::args::{ParsedArgs, parse_args};
use crate::ghostex_cli::output::print_json;
use crate::ghostex_cli::rpc::{CliError, CliResult, call_gxserver_rpc};
use crate::ghostex_cli::{notify, selector, usage};

/// Every flag that takes a value in `message show|list` or in send-message (the alias
/// `message <selector> <text>`), in both spellings, so a flag's value is never taken for the
/// subcommand; every other flag is a switch (`--json`).
const VALUE_FLAGS: &[&str] = &[
    "--session",
    "--session-id",
    "--sessionId",
    "--project",
    "--project-id",
    "--projectId",
    "--selector",
    "--session-title",
    "--sessionTitle",
    "--target",
    "--agent",
    "--text",
    "--last",
    "--grep",
    "--computer",
];

/// Where the subcommand stands in `args`: the first word that is neither a flag nor a value
/// flag's value. Flags may come before it (`message --session X show M12`, `message --json list`).
fn subcommand_at(args: &[String]) -> Option<usize> {
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if VALUE_FLAGS.contains(&arg) {
            index += 2;
        } else if arg.starts_with('-') {
            index += 1;
        } else {
            return Some(index);
        }
    }
    None
}

/// Whether `ghostex message …` reads numbered messages (`show` or `list`, wherever the flags
/// are) rather than sending one: any other `message <selector> <text>` stays send-message's alias.
pub fn is_message_read(args: &[String]) -> bool {
    subcommand_at(args).is_some_and(|at| matches!(args[at].as_str(), "show" | "list"))
}

/// `args` without the subcommand, written so the parser reads every switch as a switch and every
/// word as a word: value flags keep their value, switches become `--name=true`, and the words
/// follow `--`.
fn normalized(args: &[String], skip: Option<usize>) -> Vec<String> {
    let mut flags = Vec::new();
    let mut words = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if Some(index) == skip {
            index += 1;
        } else if VALUE_FLAGS.contains(&arg.as_str()) {
            flags.push(arg.clone());
            if let Some(value) = args.get(index + 1) {
                flags.push(value.clone());
            }
            index += 2;
        } else if arg.starts_with("--") && !arg.contains('=') {
            flags.push(format!("{arg}=true"));
            index += 1;
        } else if arg.starts_with('-') {
            flags.push(arg.clone());
            index += 1;
        } else {
            words.push(arg.clone());
            index += 1;
        }
    }
    flags.push("--".to_string());
    flags.extend(words);
    flags
}

pub fn message_command(args: &[String]) -> CliResult<()> {
    let at = subcommand_at(args);
    let subcommand = at.map_or("help", |at| args[at].as_str());
    if matches!(subcommand, "help")
        || args
            .iter()
            .any(|arg| matches!(arg.as_str(), "-h" | "--help"))
    {
        println!("{}", usage::message_usage());
        return Ok(());
    }
    let parsed = parse_args(&normalized(args, at));
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
            )));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split(' ').map(str::to_string).collect()
    }

    #[test]
    fn show_and_list_are_found_after_flags_and_sends_stay_sends() {
        assert!(is_message_read(&words("show M12")));
        assert!(is_message_read(&words("--session X show M12")));
        assert!(is_message_read(&words("--json list")));
        assert!(is_message_read(&words("--last 5 list --grep accent")));
        assert!(!is_message_read(&words("X show me the logs")));
        assert!(!is_message_read(&words("--session X hello")));
        assert!(is_message_read(&words("--sessionId G9 show M12")));
        assert!(is_message_read(&words("--project P show M12")));
        assert!(is_message_read(&words("--text hi --agent codex list")));
        let parsed = parse_args(&normalized(&words("--json show M12"), Some(1)));
        assert!(parsed.flags.truthy("json"));
        assert_eq!(parsed.rest, vec!["M12".to_string()]);
        let parsed = parse_args(&normalized(&words("--session G1 --json list"), Some(3)));
        assert_eq!(parsed.flags.string_value("session"), Some("G1"));
        assert!(parsed.rest.is_empty());
    }
}
