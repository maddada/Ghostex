//! Agent-initiated view switch requests turned into clickable toasts.
//!
//! CDXC:SwitchRequests 2026-11-23 DECISION:
//! User (Q4705-Q4711): when an agent session (`Principal::Session`) asks Ghostex to change what the
//! user is looking at (`focusSession`, `focusGroup`, `switchProject`, `openBrowser`, `openPaths`,
//! `openSettings`, or `renameCommand`), never switch the view immediately. Broadcast a
//! `switchRequest` event so desktop, `gpui-web`, and mobile show a clickable toast instead. Voice
//! (`Principal::Owner` loopback) and the user's own terminal commands (`Principal::LocalAgent`)
//! keep switching immediately.

use std::path::Path;

use ghostex_gx_protocol::{SwitchRequest, SWITCH_REQUEST_EVENT_TYPE};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::constants::GXSERVER_PROTOCOL_VERSION;
use crate::domain::DomainRepository;
use crate::security::Principal;
use crate::server::AppState;
use crate::storage::open_gxserver_database;

/// Renderer-command actions that become switch-request toasts when invoked by an agent session.
pub(crate) fn is_agent_switch_request_action(action: &str) -> bool {
    matches!(
        action,
        "focusSession"
            | "focusGroup"
            | "switchProject"
            | "openBrowser"
            | "openBrowserPane"
            | "openPaths"
            | "openSettings"
            | "renameCommand"
    )
}

/// CDXC:SwitchRequests 2026-11-26 DECISION:
/// User (Q4999, #762): an agent may open a page in the background without the switch toast,
/// because a background open changes nothing the user is looking at: the page loads in a tab of
/// the agent's own project that is not shown, focused or announced (`open_browser_url_in_background`
/// in apps/desktop/src/app/workspace_events.rs). Every other open keeps the toast. To keep that
/// power narrow the open is pinned to the caller's own project, whatever project, group or folder
/// its payload named, and it goes into the security log (quietly: no bell, no phone) once the app
/// answered (`record_agent_open` in browser_agent_limits.rs).
///
/// Returns true when `payload` is an agent session's background open, pinned in place.
pub(crate) fn pin_agent_background_browser_open(
    state: &AppState,
    caller: &Principal,
    action: &str,
    payload: &mut Map<String, Value>,
) -> bool {
    if !matches!(action, "openBrowser" | "openBrowserPane")
        || payload.get("background") != Some(&Value::Bool(true))
    {
        return false;
    }
    let Some((project_id, _)) = caller_agent_session(state, caller) else {
        return false;
    };
    for key in ["groupId", "projectName", "projectPath"] {
        payload.remove(key);
    }
    payload.insert(
        "projectId".to_string(),
        Value::String(project_id.to_string()),
    );
    true
}

/// CDXC:SwitchRequests 2026-11-26 DECISION:
/// User (Q5021-Q5022, #762): with "Let agents open and switch pages without asking" on (Settings >
/// Browser, or the toast's Always), an agent's page open needs no toast: it opens and comes on
/// screen as if the user had clicked Open. Only page opens: switching chats, projects, settings
/// and paths still asks. The setting is the user's alone (`agent_denied_key` in the settings
/// catalog keeps agents from turning it on). Such an open is still logged, quietly
/// (`record_agent_open` in browser_agent_limits.rs, #762 follow-up).
pub(crate) fn agent_page_opens_allowed(state: &AppState, action: &str) -> bool {
    matches!(action, "openBrowser" | "openBrowserPane")
        && crate::shared_settings_file::read_shared_settings(&state.paths).and_then(|settings| {
            settings
                .get(ghostex_settings_catalog::help::AGENT_PAGES_OPEN_WITHOUT_ASKING_SETTING)
                .and_then(Value::as_bool)
        }) == Some(true)
}

/// Returns `Some((project_id, session_id))` when the caller is an agent session whose switch
/// requests must become clickable toasts instead of stealing focus.
///
/// Plain shell terminal sessions inside Ghostex (`role == Agent` with no agent provider attached)
/// stay instant just like `Principal::LocalAgent` and Voice (`Principal::Owner`), per Q4705.
pub(crate) fn caller_agent_session<'a>(
    state: &AppState,
    caller: &'a Principal,
) -> Option<(&'a str, &'a str)> {
    match caller {
        Principal::Session {
            project_id,
            session_id,
            role,
        } => {
            if !matches!(role, crate::security::SessionRole::Agent) {
                return Some((project_id.as_str(), session_id.as_str()));
            }
            let session = lookup_session(state, project_id, session_id);
            let is_plain_terminal = session.as_ref().is_some_and(|row| {
                str_field(row, "agentId").is_none()
                    && str_field(row, "agentName").is_none()
                    && str_field(row, "provider").is_none()
            });
            (!is_plain_terminal).then_some((project_id.as_str(), session_id.as_str()))
        }
        _ => None,
    }
}

/// Broadcasts a `switchRequest` event to every connected client (desktop, gpui-web, and mobile)
/// for an agent-initiated `/api/focusSession` call.
pub(crate) fn emit_focus_session_toast(
    state: &AppState,
    caller_project_id: &str,
    caller_session_id: &str,
    target_session: &Value,
    payload: Map<String, Value>,
) -> SwitchRequest {
    let (requester_title, requester_icon) =
        lookup_requester(state, caller_project_id, caller_session_id);
    let target_project_id = str_field(target_session, "projectId");
    let target_session_id = str_field(target_session, "sessionId");
    let target_title = session_display_title(target_session)
        .or_else(|| target_session_id.clone())
        .unwrap_or_else(|| "a session".to_string());
    let target_project_name = target_project_id
        .as_deref()
        .and_then(|project_id| lookup_project_name(state, project_id))
        .filter(|name| !name.eq_ignore_ascii_case(&target_title));

    let title = match requester_title.as_deref() {
        Some(requester) => format!("{requester} wants to show you {target_title}"),
        None => format!("Switch to {target_title}"),
    };
    let request = SwitchRequest {
        id: Uuid::new_v4().to_string(),
        action: "focusSession".to_string(),
        payload: Value::Object(payload),
        title,
        subtitle: target_project_name,
        primary_label: "Switch".to_string(),
        icon: requester_icon.or_else(|| session_icon_slug(target_session)),
        requester_title,
        project_id: target_project_id,
        session_id: target_session_id,
        external_url: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    broadcast_switch_request(state, &request);
    request
}

/// Broadcasts a `switchRequest` event for an agent-initiated `/api/dispatchRendererCommand` action.
pub(crate) fn emit_renderer_command_toast(
    state: &AppState,
    caller_project_id: &str,
    caller_session_id: &str,
    action: &str,
    payload: Map<String, Value>,
) -> SwitchRequest {
    let (requester_title, requester_icon) =
        lookup_requester(state, caller_project_id, caller_session_id);
    let requester_prefix = requester_title.as_deref();

    let (title, subtitle, primary_label, icon, project_id, session_id, external_url) = match action
    {
        "focusSession" => {
            let (project_id, session_id) = extract_session_target(&payload);
            let target_session = match (project_id.as_deref(), session_id.as_deref()) {
                (Some(pid), Some(sid)) => lookup_session(state, pid, sid),
                _ => None,
            };
            let target_title = target_session
                .as_ref()
                .and_then(session_display_title)
                .or_else(|| session_id.clone())
                .unwrap_or_else(|| "a session".to_string());
            let subtitle = project_id
                .as_deref()
                .and_then(|pid| lookup_project_name(state, pid))
                .filter(|name| !name.eq_ignore_ascii_case(&target_title));
            let title = match requester_prefix {
                Some(requester) => format!("{requester} wants to show you {target_title}"),
                None => format!("Switch to {target_title}"),
            };
            let icon = requester_icon
                .clone()
                .or_else(|| target_session.as_ref().and_then(session_icon_slug));
            (
                title,
                subtitle,
                "Switch".to_string(),
                icon,
                project_id,
                session_id,
                None,
            )
        }
        "focusGroup" | "switchProject" => {
            let project_id = str_in_map(&payload, "projectId")
                .or_else(|| str_in_map(&payload, "groupId"))
                .or_else(|| str_in_map(&payload, "folder"));
            let project_name = project_id
                .as_deref()
                .and_then(|pid| lookup_project_name(state, pid))
                .or_else(|| {
                    str_in_map(&payload, "folder").and_then(|folder| {
                        Path::new(&folder)
                            .file_name()
                            .map(|name| name.to_string_lossy().to_string())
                    })
                })
                .or_else(|| project_id.clone())
                .unwrap_or_else(|| "a project".to_string());
            let title = match requester_prefix {
                Some(requester) => format!("{requester} wants to switch to {project_name}"),
                None => format!("Switch to {project_name}"),
            };
            let subtitle = str_in_map(&payload, "folder").filter(|folder| folder != &project_name);
            (
                title,
                subtitle,
                "Switch".to_string(),
                requester_icon.clone(),
                project_id,
                None,
                None,
            )
        }
        "openBrowser" | "openBrowserPane" => {
            let url = str_in_map(&payload, "url").unwrap_or_default();
            let host = extract_url_host(&url);
            let title = match (requester_prefix, host.as_deref()) {
                (Some(requester), Some(host)) => format!("{requester} wants to open {host}"),
                (Some(requester), None) => format!("{requester} wants to open a link"),
                (None, Some(host)) => format!("Open {host}"),
                (None, None) => "Open link in Browser".to_string(),
            };
            let subtitle = (!url.is_empty()).then_some(url.clone());
            let project_id = str_in_map(&payload, "projectId");
            let external_url = (!url.is_empty()).then_some(url);
            (
                title,
                subtitle,
                "Open".to_string(),
                Some("browser".to_string()),
                project_id,
                None,
                external_url,
            )
        }
        "openPaths" => {
            let paths = payload
                .get("paths")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::trim)
                        .filter(|path| !path.is_empty())
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let (target_label, subtitle) = match paths.as_slice() {
                [] => ("files".to_string(), None),
                [only] => {
                    let file_name = Path::new(only)
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .filter(|name| !name.is_empty())
                        .unwrap_or_else(|| only.clone());
                    let subtitle = (file_name != *only).then(|| only.clone());
                    (file_name, subtitle)
                }
                many => (format!("{} files", many.len()), many.first().cloned()),
            };
            let title = match requester_prefix {
                Some(requester) => format!("{requester} wants to open {target_label}"),
                None => format!("Open {target_label}"),
            };
            (
                title,
                subtitle,
                "Open".to_string(),
                requester_icon.clone(),
                str_in_map(&payload, "projectId"),
                None,
                None,
            )
        }
        "openSettings" => {
            let tab = str_in_map(&payload, "tab");
            let search = str_in_map(&payload, "searchQuery");
            let target = match tab.as_deref() {
                Some(tab) => format!("Settings > {}", title_case_tab(tab)),
                None => "Settings".to_string(),
            };
            let title = match requester_prefix {
                Some(requester) => format!("{requester} wants to open {target}"),
                None => format!("Open {target}"),
            };
            (
                title,
                search,
                "Open".to_string(),
                requester_icon.clone(),
                None,
                None,
                None,
            )
        }
        "renameCommand" => {
            let (project_id, session_id) = extract_session_target(&payload);
            let target_session = match (project_id.as_deref(), session_id.as_deref()) {
                (Some(pid), Some(sid)) => lookup_session(state, pid, sid),
                _ => None,
            };
            let target_title = target_session
                .as_ref()
                .and_then(session_display_title)
                .or_else(|| session_id.clone())
                .unwrap_or_else(|| "a session".to_string());
            let new_title = str_in_map(&payload, "title");
            let title = match requester_prefix {
                Some(requester) => format!("{requester} wants to rename {target_title}"),
                None => format!("Rename {target_title}"),
            };
            let subtitle = new_title.map(|next| format!("New title: {next}"));
            let icon = requester_icon
                .clone()
                .or_else(|| target_session.as_ref().and_then(session_icon_slug));
            (
                title,
                subtitle,
                "Switch".to_string(),
                icon,
                project_id,
                session_id,
                None,
            )
        }
        _ => {
            let title = match requester_prefix {
                Some(requester) => format!("{requester} wants to switch views"),
                None => "Switch view".to_string(),
            };
            (
                title,
                None,
                "Switch".to_string(),
                requester_icon.clone(),
                None,
                None,
                None,
            )
        }
    };

    let request = SwitchRequest {
        id: Uuid::new_v4().to_string(),
        action: action.to_string(),
        payload: Value::Object(payload),
        title,
        subtitle,
        primary_label,
        icon,
        requester_title,
        project_id,
        session_id,
        external_url,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    broadcast_switch_request(state, &request);
    request
}

fn broadcast_switch_request(state: &AppState, request: &SwitchRequest) {
    state.event_hub.broadcast(json!({
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "serverId": state.metadata.server_id.clone(),
        "type": SWITCH_REQUEST_EVENT_TYPE,
        "request": request,
    }));
}

fn lookup_requester(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> (Option<String>, Option<String>) {
    let Some(session) = lookup_session(state, project_id, session_id) else {
        return (None, None);
    };
    (session_display_title(&session), session_icon_slug(&session))
}

fn lookup_session(state: &AppState, project_id: &str, session_id: &str) -> Option<Value> {
    let db = open_gxserver_database(&state.paths).ok()?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    repository
        .get_session(project_id, session_id)
        .ok()
        .flatten()
}

fn lookup_project_name(state: &AppState, project_id: &str) -> Option<String> {
    let db = open_gxserver_database(&state.paths).ok()?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let project = repository
        .get_project_without_session_history(project_id)
        .ok()
        .flatten()?;
    str_field(&project, "name").or_else(|| {
        str_field(&project, "folder").and_then(|folder| {
            Path::new(&folder)
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
    })
}

fn session_display_title(session: &Value) -> Option<String> {
    str_field(session, "title")
        .or_else(|| str_field(session, "displayTitle"))
        .or_else(|| str_field(session, "name"))
}

fn session_icon_slug(session: &Value) -> Option<String> {
    let raw = str_field(session, "agentIcon")
        .or_else(|| str_field(session, "agentId"))
        .or_else(|| str_field(session, "agentName"))?;
    let lower = raw.to_ascii_lowercase();
    if lower.contains("openclaude") {
        Some("openclaude".to_string())
    } else if lower.contains("claude") {
        Some("claude".to_string())
    } else if lower.contains("codex") {
        Some("codex".to_string())
    } else if lower.contains("opencode") {
        Some("opencode".to_string())
    } else if lower.contains("antigravity") {
        Some("antigravity-cli".to_string())
    } else if lower.contains("cursor") {
        Some("cursor-cli".to_string())
    } else if lower.contains("copilot") {
        Some("copilot".to_string())
    } else if lower.contains("pi") {
        Some("pi".to_string())
    } else {
        Some(lower)
    }
}

fn extract_session_target(payload: &Map<String, Value>) -> (Option<String>, Option<String>) {
    let target = payload
        .get("sessionTarget")
        .and_then(Value::as_object)
        .unwrap_or(payload);
    let project_id = str_in_map(target, "projectId").or_else(|| str_in_map(payload, "projectId"));
    let session_id = str_in_map(target, "sessionId").or_else(|| str_in_map(payload, "sessionId"));
    (project_id, session_id)
}

fn str_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn str_in_map(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn extract_url_host(url: &str) -> Option<String> {
    let trimmed = url.trim();
    let after_scheme = trimmed
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(trimmed);
    let host_port = after_scheme.split(['/', '?', '#']).next()?;
    let host = host_port.rsplit('@').next().unwrap_or(host_port).trim();
    (!host.is_empty()).then(|| host.to_string())
}

fn title_case_tab(tab: &str) -> String {
    let mut chars = tab.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => String::new(),
    }
}
