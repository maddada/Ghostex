//! `ghostex side-chat start|list|bring-back|delete` (crate::side_chats): side chats from the CLI and
//! the phone. Without `--session` it names the session the command runs in.

use serde_json::{json, Map, Value};

use super::actions::with_resolved_gxserver_session_params;
use super::args::{parse_args, Flags};
use super::output::print_json;
use super::rpc::{self, CliError, CliResult};

const USAGE: &str = "Usage: ghostex side-chat start <question> [--session <session>] | list [--session <session>] | bring-back --session <side chat> | delete --session <side chat>";

pub(super) fn side_chat_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    let subcommand = parsed.rest.first().map(String::as_str).unwrap_or("list");
    let mut params = session_params(flags)?;
    let pathname = match subcommand {
        "start" => {
            let question = flags
                .string_value("question")
                .map(str::to_string)
                .unwrap_or_else(|| parsed.rest[1..].join(" "));
            if question.trim().is_empty() {
                return Err(CliError::Other(format!(
                    "A side chat starts with a question. {USAGE}"
                )));
            }
            params.insert("question".to_string(), json!(question.trim()));
            "/api/startSideChat"
        }
        "list" => "/api/listSideChats",
        "bring-back" => "/api/bringBackSideChat",
        "delete" => "/api/deleteSideChat",
        _ => return Err(CliError::Other(USAGE.to_string())),
    };
    let result = rpc::call_gxserver_rpc(pathname, &Value::Object(params), flags)?;
    print_json(&result);
    Ok(())
}

fn session_params(flags: &Flags) -> CliResult<Map<String, Value>> {
    let selector = flags
        .string_value("sessionId")
        .or_else(|| flags.string_value("session"))
        .map(str::to_string)
        .or_else(|| {
            ["GHOSTEX_GLOBAL_SESSION_REF", "GHOSTEX_SESSION_ID"]
                .iter()
                .find_map(|key| {
                    std::env::var(key)
                        .ok()
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty())
                })
        })
        .ok_or_else(|| {
            CliError::Other(
                "Pass --session <session>, or run this inside a Ghostex session.".to_string(),
            )
        })?;
    let mut payload = Map::new();
    payload.insert("sessionId".to_string(), json!(selector));
    Ok(
        with_resolved_gxserver_session_params(&Value::Object(payload), flags)?
            .as_object()
            .cloned()
            .unwrap_or_default(),
    )
}
