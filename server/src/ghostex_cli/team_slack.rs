//! The Slack side of `ghostex team` (`flow`, `slack-manifest`, `slack-connect`,
//! `linear-connect`, `own-linear-key`) and `ghostex slack post`, which agents started from Slack use to post to their
//! ticket's working thread.

use std::io::{IsTerminal, Read};

use serde_json::{json, Map, Value};

use super::args::{parse_args, Flags};
use super::output::print_json;
use super::rpc::{self, CliError, CliResult};
use super::selector::resolve_cli_session_selector;
use super::team::{call_and_print, resolve_workspace_id, workspace_params};
use crate::paths::get_gxserver_paths;
use crate::team_sync::set_deployment_env;

/// `""` and `none` clear a single value.
fn nullable(value: &str) -> Value {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
        Value::Null
    } else {
        json!(trimmed)
    }
}

/// `ghostex team flow [show] | set … | map <channel> … | unmap <channel>`.
pub(super) fn flow_command(rest: &[String], flags: &Flags) -> CliResult<()> {
    let action = rest.first().map(String::as_str).unwrap_or("show");
    let mut params = workspace_params(flags)?;
    match action {
        "show" => call_and_print("/api/readSlackFlowSettings", params, flags),
        "set" => {
            for (flag, key) in [
                ("workingChannel", "workingChannelId"),
                ("linearTeam", "linearTeamKey"),
                ("qcOwner", "qcOwnerSlackUserId"),
            ] {
                if let Some(value) = flags.string_value(flag) {
                    params.insert(key.to_string(), nullable(value));
                }
            }
            if let Some(value) = flags.string_value("watchOnly") {
                let channels: Vec<&str> = value
                    .split(',')
                    .map(str::trim)
                    .filter(|channel| !channel.is_empty() && !channel.eq_ignore_ascii_case("none"))
                    .collect();
                params.insert("watchOnlyChannelIds".to_string(), json!(channels));
            }
            if let Some(value) = flags.string_value("defaultRun") {
                let place = match value.trim() {
                    "cloud" => "cloud",
                    "local" => "local",
                    other => {
                        return Err(CliError::Other(format!(
                            "--default-run is cloud or local, not \"{other}\"."
                        )))
                    }
                };
                params.insert("defaultRunPlace".to_string(), json!(place));
            }
            if let Some(path) = flags.string_value("instructionsFile") {
                let text = std::fs::read_to_string(path)
                    .map_err(|error| CliError::Other(format!("Could not read {path}: {error}")))?;
                params.insert("instructions".to_string(), nullable(&text));
            } else if let Some(text) = flags.string_value("instructions") {
                params.insert("instructions".to_string(), nullable(text));
            }
            if params.len() == 1 {
                return Err(CliError::Other(
                    "Pass --working-channel, --watch-only, --default-run, --linear-team, --qc-owner or --instructions-file.".to_string(),
                ));
            }
            call_and_print("/api/setSlackFlowSettings", params, flags)
        }
        "map" => {
            let channel = rest.get(1).ok_or_else(|| {
                CliError::Other(
                    "Pass the channel: ghostex team flow map <channel ID> --repo owner/name [--project name] [--linear-team KEY] [--linear-project name].".to_string(),
                )
            })?;
            let mut mapping = Map::new();
            mapping.insert("channelId".to_string(), json!(channel));
            for (flag, key) in [
                ("repo", "repo"),
                ("project", "project"),
                ("linearTeam", "linearTeamKey"),
                ("linearProject", "linearProject"),
            ] {
                if let Some(value) = flags.string_value(flag) {
                    mapping.insert(key.to_string(), nullable(value));
                }
            }
            params.insert("mapChannel".to_string(), Value::Object(mapping));
            call_and_print("/api/setSlackFlowSettings", params, flags)
        }
        "unmap" => {
            let channel = rest.get(1).ok_or_else(|| {
                CliError::Other(
                    "Pass the channel: ghostex team flow unmap <channel ID>.".to_string(),
                )
            })?;
            params.insert("unmapChannel".to_string(), json!(channel));
            call_and_print("/api/setSlackFlowSettings", params, flags)
        }
        other => Err(CliError::Other(format!(
            "Unknown flow command \"{other}\". Use show, set, map or unmap."
        ))),
    }
}

/// Prints the Slack app manifest (JSON, ready to paste into "Create New App → From a manifest").
pub(super) fn slack_manifest_command(flags: &Flags) -> CliResult<()> {
    let mut params = workspace_params(flags)?;
    if let Some(name) = flags.string_value("name") {
        params.insert("name".to_string(), json!(name));
    }
    let result = rpc::call_gxserver_rpc("/api/readSlackManifest", &Value::Object(params), flags)?;
    if flags.truthy("json") {
        print_json(&result);
        return Ok(());
    }
    print_json(result.get("manifest").unwrap_or(&Value::Null));
    if let Some(next) = result.get("next").and_then(Value::as_str) {
        eprintln!("\n{next}");
    }
    Ok(())
}

fn read_stdin(prompt: &str) -> CliResult<String> {
    let mut input = String::new();
    if std::io::stdin().is_terminal() {
        eprintln!("{prompt}");
    }
    std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| CliError::Other(format!("Could not read stdin: {error}")))?;
    Ok(input)
}

/// Reads the Slack bot token and signing secret from stdin and stores them as the team's Convex
/// environment variables (`SLACK_BOT_TOKEN`, `SLACK_SIGNING_SECRET`).
pub(super) fn slack_connect_command(flags: &Flags) -> CliResult<()> {
    let input = read_stdin(
        "Paste the Bot User OAuth Token (xoxb-…) and the Signing Secret, one per line, then press Ctrl+D (Ctrl+Z, Enter on Windows):",
    )?;
    let words: Vec<&str> = input.split_whitespace().collect();
    let token = words.iter().find(|word| word.starts_with("xoxb-")).copied();
    let secret = words
        .iter()
        .find(|word| {
            word.len() == 32 && word.chars().all(|character| character.is_ascii_hexdigit())
        })
        .copied();
    let (Some(token), Some(secret)) = (token, secret) else {
        return Err(CliError::Other(
            "Pass both the Bot User OAuth Token (starts with xoxb-) and the Signing Secret (32 hex characters, Basic Information → App Credentials).".to_string(),
        ));
    };
    let workspace_id = resolve_workspace_id(flags)?;
    set_deployment_env(
        &get_gxserver_paths(None),
        &workspace_id,
        flags.truthy("dev"),
        &[("SLACK_BOT_TOKEN", token), ("SLACK_SIGNING_SECRET", secret)],
    )
    .map_err(CliError::Other)?;
    print_json(
        &json!({ "ok": true, "workspaceId": workspace_id, "set": ["SLACK_BOT_TOKEN", "SLACK_SIGNING_SECRET"] }),
    );
    Ok(())
}

/// Reads a Linear API key from stdin and stores it as the team's Linear key, which the Slack
/// command flow uses to find and create tickets while the requester's computer is off. Owners
/// only (the team refuses anyone else); `--remove` removes it.
pub(super) fn linear_connect_command(flags: &Flags) -> CliResult<()> {
    let mut params = workspace_params(flags)?;
    if !flags.truthy("remove") {
        let input = read_stdin(
            "Paste a Linear API key (lin_api_…), then press Ctrl+D (Ctrl+Z, Enter on Windows):",
        )?;
        let key = input
            .split_whitespace()
            .find(|word| word.starts_with("lin_api_"))
            .ok_or_else(|| {
                CliError::Other("Pass a Linear API key (starts with lin_api_).".to_string())
            })?;
        params.insert("apiKey".to_string(), json!(key));
    }
    call_and_print("/api/setTeamLinearKey", params, flags)
}

/// `ghostex team own-linear-key on|off`: whether the Slack flow creates the tickets you request
/// with this workspace's Linear key (stored in the team's Convex project for you) instead of the
/// team's key.
pub(super) fn own_linear_key_command(rest: &[String], flags: &Flags) -> CliResult<()> {
    let enabled = match rest.first().map(String::as_str) {
        Some("on") => true,
        Some("off") => false,
        _ => {
            return Err(CliError::Other(
                "Use: ghostex team own-linear-key on|off [--workspace name]".to_string(),
            ))
        }
    };
    let mut params = workspace_params(flags)?;
    params.insert("enabled".to_string(), json!(enabled));
    call_and_print("/api/setOwnLinearKey", params, flags)
}

/// `ghostex slack post [--session <ref>] "<text>" [--final]`. Without `--session`, the session
/// whose terminal runs the command.
pub(super) fn slack_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    match parsed.rest.first().map(String::as_str) {
        Some("post") => {}
        _ => {
            return Err(CliError::Other(
                "Use: ghostex slack post --session <session> \"<text>\" [--final]".to_string(),
            ))
        }
    }
    let mut words: Vec<String> = parsed.rest[1..].to_vec();
    // `--final "text"` reads the text as the flag's value; it is still the text.
    if let Some(text) = flags.string_value("final") {
        words.push(text.to_string());
    }
    let text = words.join(" ");
    if text.trim().is_empty() {
        return Err(CliError::Other("Pass the text to post.".to_string()));
    }
    let selector = flags
        .text("session")
        .or_else(|| std::env::var("GHOSTEX_SESSION_ID").ok())
        .filter(|selector| !selector.trim().is_empty())
        .ok_or_else(|| CliError::Other("Pass --session <session>.".to_string()))?;
    let session = resolve_cli_session_selector(&selector, flags)?;
    let result = rpc::call_gxserver_rpc(
        "/api/postSlackWorkingThread",
        &json!({
            "projectId": session.get("projectId").cloned().unwrap_or(Value::Null),
            "sessionId": session.get("sessionId").cloned().unwrap_or(Value::Null),
            "text": text,
            "final": flags.contains("final"),
        }),
        flags,
    )?;
    print_json(&result);
    Ok(())
}
