use serde_json::{Map, Value};

use crate::ghostex_cli::args::{parse_args, ParsedArgs};
use crate::ghostex_cli::output::{is_failed_cli_result, print_json};
use crate::ghostex_cli::rpc::{call_gxserver_rpc, CliError, CliResult};
use crate::ghostex_cli::selector;
use crate::notification_feed::NOTIFICATION_FEED_CREATE_ENDPOINT;

/// `ghostex notify` is the CLI face of `/api/createNotification`: hooks and scripts post a row to the titlebar bell for the session they run in, and gxserver owns the row, its read state, and the change announcement.
pub fn notify_command(args: &[String]) -> CliResult<()> {
    if matches!(
        args.first().map(String::as_str),
        Some("help") | Some("-h") | Some("--help")
    ) {
        println!("{}", crate::ghostex_cli::usage::notify_usage());
        return Ok(());
    }
    let parsed = parse_args(args);
    let title = parsed
        .flags
        .text("title")
        .or_else(|| parsed.rest.first().cloned())
        .unwrap_or_default()
        .trim()
        .to_string();
    if title.is_empty() {
        return Err(CliError::Other(format!(
            "notify requires --title.\n\n{}",
            crate::ghostex_cli::usage::notify_usage()
        )));
    }
    let (project_id, session_id) = resolve_notify_session(&parsed)?;
    let mut payload = Map::new();
    payload.insert("projectId".to_string(), Value::String(project_id));
    payload.insert("sessionId".to_string(), Value::String(session_id));
    payload.insert("title".to_string(), Value::String(title));
    if let Some(body) = parsed
        .flags
        .text("body")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        payload.insert("body".to_string(), Value::String(body));
    }
    let result = call_gxserver_rpc(
        NOTIFICATION_FEED_CREATE_ENDPOINT,
        &Value::Object(payload),
        &parsed.flags,
    )?;
    print_json(&result);
    if is_failed_cli_result(&result) {
        crate::ghostex_cli::set_exit_code(1);
    }
    Ok(())
}

/// The session to notify for: an explicit selector, else the calling session.
fn resolve_notify_session(parsed: &ParsedArgs) -> CliResult<(String, String)> {
    let selector_text = parsed
        .flags
        .text("sessionId")
        .or_else(|| parsed.flags.text("session"))
        .or_else(|| parsed.flags.text("selector"))
        .unwrap_or_default()
        .trim()
        .to_string();
    if selector_text.is_empty() {
        return session_reference_from_environment().ok_or_else(|| {
            CliError::Other(
                "notify must run inside a Ghostex session. Pass --session-id <alias|id|title> to notify for another session."
                    .to_string(),
            )
        });
    }
    let session = selector::resolve_cli_session_selector(&selector_text, &parsed.flags)?;
    session_reference(&session).ok_or_else(|| {
        CliError::Other(format!(
            "Session \"{selector_text}\" does not report a project id."
        ))
    })
}

/// `(projectId, sessionId)` of a listed session row.
fn session_reference(session: &Value) -> Option<(String, String)> {
    let session_id = trimmed_text(session.get("sessionId"))?;
    let project_id = trimmed_text(session.get("projectId")).or_else(|| {
        let global_ref = trimmed_text(session.get("globalRef"))?;
        let (project_id, _) = crate::agent_hooks::probing::parse_global_session_ref(&global_ref);
        project_id
    })?;
    Some((project_id, session_id))
}

/// Every Ghostex pane exports `GHOSTEX_GLOBAL_SESSION_REF` and `GHOSTEX_NATIVE_SESSION_ID`, so a hook can notify with no arguments.
fn session_reference_from_environment() -> Option<(String, String)> {
    if let Some(global_ref) = environment_text("GHOSTEX_GLOBAL_SESSION_REF") {
        if let (Some(project_id), Some(session_id)) =
            crate::agent_hooks::probing::parse_global_session_ref(&global_ref)
        {
            return Some((project_id, session_id));
        }
    }
    let native_session_id = environment_text("GHOSTEX_NATIVE_SESSION_ID")?;
    let (project_id, session_id) = native_session_id.split_once(':')?;
    let project_id = project_id.trim();
    let session_id = session_id.trim();
    if project_id.is_empty() || session_id.is_empty() {
        return None;
    }
    Some((project_id.to_string(), session_id.to_string()))
}

fn environment_text(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn trimmed_text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}
