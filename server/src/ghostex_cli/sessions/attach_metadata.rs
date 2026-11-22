use serde_json::{json, Map, Value};

use crate::ghostex_cli::args::Flags;
use crate::ghostex_cli::rpc::{call_gxserver_rpc, project_id_from_global_ref, CliError, CliResult};

use super::*;

// ---------------------------------------------------------------------------
// attach metadata
// ---------------------------------------------------------------------------

fn fetch_attach_metadata_for_session(session: &Value, flags: &Flags) -> CliResult<Option<Value>> {
    let prompt_editor = prompt_editor_attach_mode_from_flags(flags);
    let global_ref_project = session
        .get("globalRef")
        .and_then(Value::as_str)
        .and_then(project_id_from_global_ref)
        .map(Value::String);
    let mut params = Map::new();
    insert_js(
        &mut params,
        "projectId",
        &[session.get("projectId"), global_ref_project.as_ref()],
    );
    if let Some(editor) = prompt_editor {
        params.insert("promptEditor".to_string(), json!(editor));
    }
    insert_js(&mut params, "sessionId", &[session.get("sessionId")]);
    let result = call_gxserver_rpc("/api/attachSessionMetadata", &Value::Object(params), flags)?;
    Ok(result.get("attach").cloned())
}

fn start_missing_provider_for_cli_attach(
    session: &Value,
    attach: Option<Value>,
    flags: &Flags,
) -> CliResult<Option<Value>> {
    if !should_start_missing_provider_for_cli_attach(attach.as_ref()) {
        return Ok(attach);
    }
    /*
     * CDXC:Cli 2026-06-09-09:53:
     * CLI, TUI, Android, and SSH attach commands can create a zmx provider
     * before macOS ever sees the row. Start missing providers through gxserver
     * before launching the blocking interactive attach so gxserver persists
     * providerState=exists and publishes the sidebar presentation delta.
     */
    let attach_value = attach.as_ref().expect("checked above");
    let attach_session = attach_value.get("session");
    let global_ref_project = session
        .get("globalRef")
        .and_then(Value::as_str)
        .and_then(project_id_from_global_ref)
        .map(Value::String);
    let mut params = Map::new();
    insert_js(
        &mut params,
        "projectId",
        &[
            attach_session.and_then(|value| value.get("projectId")),
            session.get("projectId"),
            global_ref_project.as_ref(),
        ],
    );
    if let Some(editor) = prompt_editor_attach_mode_from_flags(flags) {
        params.insert("promptEditor".to_string(), json!(editor));
    }
    insert_js(
        &mut params,
        "sessionId",
        &[
            attach_session.and_then(|value| value.get("sessionId")),
            session.get("sessionId"),
        ],
    );
    insert_js(
        &mut params,
        "startupText",
        &[attach_value.get("startupText")],
    );
    call_gxserver_rpc("/api/startSessionProvider", &Value::Object(params), flags)?;
    fetch_attach_metadata_for_session(session, flags)
}

fn prompt_editor_attach_mode_from_flags(flags: &Flags) -> Option<&'static str> {
    /*
     * CDXC:PromptEditor 2026-06-11-18:24:
     * `ghostex attach --prompt-editor monaco` advertises the local desktop
     * daemon, while `code-server` advertises an editor that owns files on the
     * attached machine. Every other attach omits editor capability.
     */
    let value = flags
        .text("promptEditor")
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    match value.as_str() {
        "monaco" => Some("monaco"),
        "code-server" => Some("code-server"),
        _ => None,
    }
}

pub(super) fn should_start_missing_provider_for_cli_attach(attach: Option<&Value>) -> bool {
    let Some(attach) = attach else {
        return false;
    };
    js_truthy(Some(attach))
        && !js_truthy(attach.get("restoreBlocked"))
        && attach.get("provider").and_then(Value::as_str) == Some("zmx")
        && attach
            .get("providerState")
            .and_then(|value| value.get("lifecycleState"))
            .and_then(Value::as_str)
            == Some("missing")
}

pub(super) fn apply_attach_metadata_to_cli_session(
    session: &Value,
    attach: Option<&Value>,
) -> CliResult<Value> {
    let Some(attach) = attach.filter(|attach| js_truthy(Some(attach))) else {
        return Ok(session.clone());
    };
    if js_truthy(attach.get("restoreBlocked")) {
        let restore_blocked = attach.get("restoreBlocked");
        let cwd = if js_truthy(restore_blocked.and_then(|value| value.get("cwd"))) {
            format!(
                " ({})",
                js_template(restore_blocked.and_then(|value| value.get("cwd")))
            )
        } else {
            String::new()
        };
        let label = js_template(js_coalesce(&[
            session.get("title"),
            session.get("sessionId"),
        ]));
        return Err(CliError::Other(format!(
            "Session {label} cannot be restored because its cwd is missing{cwd}."
        )));
    }
    let provider_lifecycle = attach
        .get("providerState")
        .and_then(|value| value.get("lifecycleState"));
    let resume_command = normalize_startup_text_for_shell(attach.get("startupText"));
    let mut map = session.as_object().cloned().unwrap_or_default();
    set_js(&mut map, "attachCommand", &[attach.get("attachCommand")]);
    set_js(
        &mut map,
        "projectPath",
        &[attach.get("cwd"), session.get("projectPath")],
    );
    set_js(
        &mut map,
        "provider",
        &[attach.get("provider"), session.get("provider")],
    );
    set_js(
        &mut map,
        "providerSessionName",
        &[attach.get("zmxName"), session.get("providerSessionName")],
    );
    match &resume_command {
        Some(text) => {
            map.insert("resumeCommand".to_string(), json!(text));
        }
        None => {
            map.remove("resumeCommand");
        }
    }
    let status: Option<Value> = if provider_lifecycle.and_then(Value::as_str) == Some("exists") {
        Some(json!("running"))
    } else if resume_command.is_some() {
        Some(json!("sleep"))
    } else {
        session.get("status").cloned()
    };
    match status {
        Some(value) => {
            map.insert("status".to_string(), value);
        }
        None => {
            map.remove("status");
        }
    }
    Ok(Value::Object(map))
}

pub(super) fn normalize_startup_text_for_shell(value: Option<&Value>) -> Option<String> {
    let text = js_string(value);
    let text = text.trim_end_matches('\r').trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

/// fetchAttachMetadataForSession + startMissingProviderForCliAttach +
/// applyAttachMetadataToCliSession combined helper used by the attach flow
/// (mirrors the attachResolvedSession call shape).
pub fn apply_attach_metadata(session: &mut Value, flags: &Flags) -> CliResult<()> {
    let attach = fetch_attach_metadata_for_session(session, flags)?;
    let attach = start_missing_provider_for_cli_attach(session, attach, flags)?;
    let updated = apply_attach_metadata_to_cli_session(session, attach.as_ref())?;
    *session = updated;
    Ok(())
}

/// resolveGxserverInventorySession: look up one session by raw session id or
/// globalRef. Matches the Node CLI: exactly one match is returned; zero or
/// multiple matches are errors.
pub fn resolve_gxserver_inventory_session(
    session_id: &str,
    flags: &Flags,
) -> CliResult<Option<Value>> {
    let selector = session_id.trim();
    if selector.is_empty() {
        return Err(CliError::Other(
            "Session action requires --session-id.".to_string(),
        ));
    }
    let mut list_flags = flags.clone();
    list_flags.insert_bool("all", true);
    list_flags.insert_bool("includeStopped", true);
    if let Some(session) = resolve_from_id_candidates(selector, &list_flags, flags) {
        return Ok(Some(session));
    }
    let result = fetch_gxserver_session_list(&list_flags)?;
    let empty = Vec::new();
    let sessions = result
        .get("sessions")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let matches: Vec<&Value> = sessions
        .iter()
        .filter(|session| {
            session.get("sessionId").and_then(Value::as_str) == Some(selector)
                || session.get("globalRef").and_then(Value::as_str) == Some(selector)
        })
        .collect();
    if matches.len() == 1 {
        return Ok(Some(matches[0].clone()));
    }
    if matches.len() > 1 {
        return Err(CliError::Other(format!(
            "Multiple gxserver sessions matched \"{selector}\". Use the full globalRef from ghostex sessions --json."
        )));
    }
    /*
     * CDXC:Cli 2026-09-24 WHY:
     * Session-chat verbs resolved only a raw session id or a global ref, so the Session ID,
     * Routing ID, Agent Session ID, zmx name and title a user pastes from Copy Details all failed
     * here while attach/focus accepted them. Anything the exact match misses goes through the
     * same selector rules those verbs use, over the inventory that includes sleeping sessions.
     */
    // A closed session keeps its title and agent session id, so an open one wins a tie with it;
    // a selector only a closed session matches still reads that one.
    let open: Vec<Value> = sessions
        .iter()
        .filter(|session| !is_stopped_inventory_session(session))
        .cloned()
        .collect();
    for pool in [open.as_slice(), sessions.as_slice()] {
        let matches = super::selector::resolve_listed_sessions(selector, pool, flags)?;
        match matches.len() {
            0 => continue,
            1 => return Ok(Some(matches[0].clone())),
            _ => {
                let candidates = matches
                    .iter()
                    .map(|session| {
                        let text = |key: &str| {
                            session
                                .get(key)
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string()
                        };
                        format!(
                            "  {}  {} · {} · {}",
                            text("globalRef"),
                            text("projectName"),
                            text("displayTitle"),
                            text("status")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                return Err(CliError::Other(format!(
                    "Multiple sessions matched \"{selector}\"; pass one of these global refs:\n{candidates}"
                )));
            }
        }
    }
    Err(CliError::Other(format!(
        "No gxserver session matched \"{selector}\". Pass a title, a session id, a global ref (S…:P…:G…), a zmx name, an agent session id, or the Session ID or Routing ID from Copy Details; ghostex sessions --json lists them."
    )))
}

/// The session `resolve_gxserver_inventory_session` resolves `selector` to, decided from the few
/// rows gxserver finds for it by an id (`/api/findSessions`), when they decide it exactly as the
/// whole list would: one exact session id or global ref, else one open session by the id rules.
/// `None` sends the caller to the whole list (several matches, a title, an alias, no match).
///
/// CDXC:Cli 2026-11-10 WHY: every session action sent with a bare `--session-id` listed all
/// 10,034 sessions here to resolve it (domain/repository/session_find.rs has the numbers). The
/// found rows hold every row an id rule matches, so the exact match and the open pool's id rules
/// pick what they pick over the whole list; the title rules, which come last, need the whole list.
fn resolve_from_id_candidates(selector: &str, list_flags: &Flags, flags: &Flags) -> Option<Value> {
    let candidates = super::selector::id_selector_candidates(selector, list_flags)?;
    let exact: Vec<&Value> = candidates
        .iter()
        .filter(|session| {
            session.get("sessionId").and_then(Value::as_str) == Some(selector)
                || session.get("globalRef").and_then(Value::as_str) == Some(selector)
        })
        .collect();
    match exact.len() {
        1 => return Some(exact[0].clone()),
        0 => {}
        _ => return None,
    }
    let open: Vec<Value> = candidates
        .iter()
        .filter(|session| !is_stopped_inventory_session(session))
        .cloned()
        .collect();
    let matches = super::selector::id_rule_matches_in(selector, &open, flags)?;
    (matches.len() == 1).then(|| matches[0].clone())
}

fn is_stopped_inventory_session(session: &Value) -> bool {
    ["status", "lifecycleState"]
        .iter()
        .any(|key| session.get(*key).and_then(Value::as_str) == Some("stopped"))
}
