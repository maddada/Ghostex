//! `ghostex link add|remove|restore|pin|unpin|refresh|list` (crate::session_links): the PRs, GitHub issues and Linear tickets
//! a session is linked to. An agent reports the PR or issue it is working on with `ghostex link
//! add`; without `--session` it names the session the command runs in.
//!
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User picked "What the agent reports with `ghostex link add`" as one of the ways a link joins a session by itself; a link added from inside the session's own terminal counts as the agent's ("Agent added"), one added for another session (the phone, `--session`) as the person's ("You added").

use serde_json::{json, Map, Value};

use super::actions::with_resolved_gxserver_session_params;
use super::args::{parse_args, Flags};
use super::output::print_json;
use super::rpc::{self, CliError, CliResult};

pub(super) fn link_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    let subcommand = parsed.rest.first().map(String::as_str).unwrap_or("list");
    let target = parsed.rest.get(1).cloned();
    let (mut params, own_session) = session_params(flags)?;
    match subcommand {
        "add" => {
            let link = target.ok_or_else(|| {
                CliError::Other(
                    "Pass the link: a GitHub PR or issue URL, #123, owner/repo#123, or a Linear ID like ABC-123.".to_string(),
                )
            })?;
            params.insert("link".to_string(), json!(link));
            if let Some(kind) = flags.string_value("kind") {
                params.insert("kind".to_string(), json!(kind));
            } else if flags.truthy("issue") {
                params.insert("kind".to_string(), json!("issue"));
            }
            params.insert(
                "source".to_string(),
                json!(if own_session { "agent" } else { "user" }),
            );
            let result =
                rpc::call_gxserver_rpc("/api/addSessionLink", &Value::Object(params), flags)?;
            print_json(&result);
            Ok(())
        }
        "remove" => {
            let key = target.ok_or_else(|| {
                CliError::Other("Pass the link to remove (its key, URL, #123 or ID).".to_string())
            })?;
            params.insert("key".to_string(), json!(key));
            let result =
                rpc::call_gxserver_rpc("/api/removeSessionLink", &Value::Object(params), flags)?;
            print_json(&result);
            Ok(())
        }
        "restore" => {
            let key = target.ok_or_else(|| {
                CliError::Other(
                    "Pass the removed link to restore (its key, URL, #123 or ID).".to_string(),
                )
            })?;
            params.insert("key".to_string(), json!(key));
            let result =
                rpc::call_gxserver_rpc("/api/restoreSessionLink", &Value::Object(params), flags)?;
            print_json(&result);
            Ok(())
        }
        // Reads the session's links now (what opening its links list does), within the pass's
        // budget; `ghostex link list` afterwards shows what was read.
        "refresh" => {
            let result =
                rpc::call_gxserver_rpc("/api/refreshSessionLinks", &Value::Object(params), flags)?;
            print_json(&result);
            Ok(())
        }
        "pin" | "unpin" => {
            let key = target.ok_or_else(|| {
                CliError::Other(format!(
                    "Pass the link to {subcommand} (its key, URL, #123 or ID)."
                ))
            })?;
            params.insert("key".to_string(), json!(key));
            params.insert("pinned".to_string(), json!(subcommand == "pin"));
            let result =
                rpc::call_gxserver_rpc("/api/pinSessionLink", &Value::Object(params), flags)?;
            print_json(&result);
            Ok(())
        }
        "list" => {
            let result =
                rpc::call_gxserver_rpc("/api/readSessionLinks", &Value::Object(params), flags)?;
            if flags.truthy("json") {
                print_json(&result);
            } else {
                print_links(&result);
            }
            Ok(())
        }
        other => Err(CliError::Other(format!(
            "Unknown link command \"{other}\". Use add, remove, restore, pin, unpin, refresh or list."
        ))),
    }
}

/// The session the flags name, else the one this runs in. The flag says which.
fn session_params(flags: &Flags) -> CliResult<(Map<String, Value>, bool)> {
    let named = flags
        .string_value("sessionId")
        .or_else(|| flags.string_value("session"))
        .map(str::to_string);
    let own_session = named.is_none();
    let selector = named.or_else(|| {
        ["GHOSTEX_GLOBAL_SESSION_REF", "GHOSTEX_SESSION_ID"]
            .iter()
            .find_map(|key| {
                std::env::var(key)
                    .ok()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            })
    });
    let Some(selector) = selector else {
        return Err(CliError::Other(
            "Pass --session <session>, or run this inside a Ghostex session.".to_string(),
        ));
    };
    let mut payload = Map::new();
    payload.insert("sessionId".to_string(), json!(selector));
    let params = with_resolved_gxserver_session_params(&Value::Object(payload), flags)?
        .as_object()
        .cloned()
        .unwrap_or_default();
    Ok((params, own_session))
}

fn print_links(result: &Value) {
    let text = |value: &Value, key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let links = result.get("links").unwrap_or(&Value::Null);
    let items = links
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        println!("No links.");
    }
    for item in &items {
        let pin = if item.get("pinned").and_then(Value::as_bool) == Some(true) {
            "📌 "
        } else {
            ""
        };
        let state = [
            text(item, "stateName"),
            text(item, "state"),
            text(item, "checks"),
            text(item, "review"),
        ]
        .into_iter()
        .filter(|part| !part.is_empty())
        .take(3)
        .collect::<Vec<_>>()
        .join(", ");
        println!(
            "{pin}{:<18} {:<28} {}  ({})  {}",
            text(item, "label"),
            state,
            text(item, "title"),
            text(item, "sourceLabel"),
            text(item, "url"),
        );
    }
    let suggested = links
        .get("suggested")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !suggested.is_empty() {
        println!("\nSuggested (mentioned in the chat):");
        for item in &suggested {
            println!(
                "{:<18} {}  {}",
                text(item, "label"),
                text(item, "title"),
                text(item, "url")
            );
        }
    }
    let removed = links
        .get("removed")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !removed.is_empty() {
        println!("\nRemoved (ghostex link restore <key> brings one back):");
        for item in &removed {
            println!(
                "{:<18} {:<40} {}",
                text(item, "label"),
                text(item, "key"),
                text(item, "title")
            );
        }
    }
}
