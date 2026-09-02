use std::collections::HashMap;
use std::path::Path;

use serde_json::{json, Map, Value};

use crate::ghostex_cli::args::{parse_args, parse_json_value, FlagValue, Flags};
use crate::ghostex_cli::output::{is_failed_cli_result, print_json};
use crate::ghostex_cli::rpc::{
    call_gxserver_rpc, gxserver_root, project_id_from_global_ref, CliError, CliResult,
    GXSERVER_PRODUCT,
};
use crate::ghostex_cli::{actions, selector, set_exit_code};

/*
CDXC:GhostexRustCli 2026-07-13:
Faithful port of the Node CLI's gxserver session inventory surface:
sessionsCommand (--json / --mobile-summary / grouped human list),
fetchGxserverSessionList with the persisted-state SQLite fallback, toCliSession,
the mobile summary compactors consumed by React Native Android, run-action, and the
attach-metadata helpers. JSON field names and console strings match the Node
CLI byte-for-byte; serde_json's alphabetical key order is the one accepted
difference.
*/

const QUICK_TERMINALS_PROJECT_NAME: &str = "Quick Terminals";

// ---------------------------------------------------------------------------
// JS value-semantics helpers (String()/??/||/truthiness on serde_json::Value)
// ---------------------------------------------------------------------------

/// JS `String(value)` for primitives.
pub(crate) fn js_display(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// JS `String(value ?? "")`: missing/undefined and null both become "".
pub(crate) fn js_string(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(other) => js_display(other),
    }
}

/// JS template-literal coercion: `${undefined}` -> "undefined", `${null}` -> "null".
pub(crate) fn js_template(value: Option<&Value>) -> String {
    match value {
        None => "undefined".to_string(),
        Some(value) => js_display(value),
    }
}

/// JS truthiness where `None` models `undefined`.
pub(crate) fn js_truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number
            .as_f64()
            .map(|number| number != 0.0 && !number.is_nan())
            .unwrap_or(false),
        Some(Value::String(text)) => !text.is_empty(),
        Some(_) => true,
    }
}

/// JS `a ?? b ?? c`: first non-nullish operand; otherwise the last operand's
/// own value (Some(Null) if it was null, None if it was undefined).
pub(crate) fn js_coalesce<'a>(chain: &[Option<&'a Value>]) -> Option<&'a Value> {
    for item in chain {
        if let Some(value) = *item {
            if !value.is_null() {
                return Some(value);
            }
        }
    }
    match chain.last() {
        Some(Some(value)) => Some(*value),
        _ => None,
    }
}

/// `key: a ?? b ?? c` object-literal semantics under JSON.stringify: the key is
/// dropped when the chain resolves to undefined, kept as null when it resolves
/// to an explicit null.
fn insert_js(map: &mut Map<String, Value>, key: &str, chain: &[Option<&Value>]) {
    if let Some(value) = js_coalesce(chain) {
        map.insert(key.to_string(), value.clone());
    }
}

/// `key: obj.field` semantics: keep the value (even null) when the key exists,
/// drop it when the key is missing (undefined).
fn insert_present(map: &mut Map<String, Value>, key: &str, value: Option<&Value>) {
    if let Some(value) = value {
        map.insert(key.to_string(), value.clone());
    }
}

/// `key: obj.field ?? undefined` semantics: drop both missing and null.
fn insert_non_null(map: &mut Map<String, Value>, key: &str, value: Option<&Value>) {
    if let Some(value) = value {
        if !value.is_null() {
            map.insert(key.to_string(), value.clone());
        }
    }
}

/// Spread-override semantics (`{ ...session, key: a ?? b }`): a resolved
/// undefined removes the key (JSON.stringify drops it), anything else sets it.
fn set_js(map: &mut Map<String, Value>, key: &str, chain: &[Option<&Value>]) {
    match js_coalesce(chain) {
        Some(value) => {
            map.insert(key.to_string(), value.clone());
        }
        None => {
            map.remove(key);
        }
    }
}

/// Stable map key for JS `Map` keys built from raw JSON values; None models an
/// undefined key so missing ids stay distinct from null/"null".
fn value_key(value: Option<&Value>) -> Option<String> {
    value.map(|value| value.to_string())
}

/// JS strict `flags.x === true` (only a bare boolean flag counts).
fn strict_true(flags: &Flags, key: &str) -> bool {
    matches!(flags.0.get(key), Some(FlagValue::Bool(true)))
}

// ---------------------------------------------------------------------------
// sessions command
// ---------------------------------------------------------------------------

pub fn sessions_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    /*
     * CDXC:MobileSidebarHud 2026-07-12-00:00:
     * The mobile summary is the one poll React Native Android makes, so it also carries
     * the gxserver-owned agent launcher rows and per-project quick actions.
     * HUD fetch failures must never break the session list; mobile simply
     * hides the launcher rows until the next poll.
     */
    if flags.truthy("json") && flags.truthy("mobileSummary") {
        let result = fetch_session_list_result(flags, true)?;
        let hud = fetch_mobile_sidebar_hud(flags).ok();
        let mut merged = match to_mobile_session_list(&result) {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        if let Some(Value::Object(hud_map)) = hud {
            for (key, value) in hud_map {
                merged.insert(key, value);
            }
        }
        print_json(&Value::Object(merged));
        return Ok(());
    }
    let result = fetch_session_list_result(flags, true)?;
    if flags.truthy("json") {
        print_json(&result);
        return Ok(());
    }
    let empty = Vec::new();
    let sessions = result
        .get("sessions")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let grouped = !strict_true(flags, "ungrouped") && !strict_true(flags, "u");
    print_session_list(sessions, grouped);
    Ok(())
}

fn fetch_mobile_sidebar_hud(flags: &Flags) -> CliResult<Value> {
    let hud = call_gxserver_rpc(
        "/api/readSidebarHud",
        &json!({ "includeAllProjectCommands": true }),
        flags,
    )?;
    let mut agents: Vec<Value> = Vec::new();
    if let Some(list) = hud.get("agents").and_then(Value::as_array) {
        for agent in list {
            let has_agent_id =
                matches!(agent.get("agentId"), Some(Value::String(text)) if !text.is_empty());
            if !has_agent_id {
                continue;
            }
            let mut map = Map::new();
            insert_present(&mut map, "agentId", agent.get("agentId"));
            insert_present(&mut map, "icon", agent.get("icon"));
            insert_present(&mut map, "name", agent.get("name"));
            agents.push(Value::Object(map));
        }
    }
    let mut quick_actions_by_project = Map::new();
    if let Some(by_project) = hud.get("commandsByProject").and_then(Value::as_object) {
        for (project_id, commands) in by_project {
            let mut actions_list: Vec<Value> = Vec::new();
            if let Some(commands) = commands.as_array() {
                for command in commands {
                    if !is_configured_mobile_quick_action(command) {
                        continue;
                    }
                    let mut map = Map::new();
                    insert_present(&mut map, "actionType", command.get("actionType"));
                    insert_present(&mut map, "commandId", command.get("commandId"));
                    insert_present(&mut map, "icon", command.get("icon"));
                    insert_present(&mut map, "name", command.get("name"));
                    if command.get("actionType").and_then(Value::as_str) == Some("browser") {
                        insert_present(&mut map, "url", command.get("url"));
                    }
                    actions_list.push(Value::Object(map));
                }
            }
            if !actions_list.is_empty() {
                quick_actions_by_project.insert(project_id.clone(), Value::Array(actions_list));
            }
        }
    }
    Ok(json!({
        "agents": agents,
        "quickActionsByProject": quick_actions_by_project,
    }))
}

fn is_configured_mobile_quick_action(command: &Value) -> bool {
    let has_command_id =
        matches!(command.get("commandId"), Some(Value::String(text)) if !text.is_empty());
    if !has_command_id {
        return false;
    }
    if command.get("actionType").and_then(Value::as_str) == Some("browser") {
        return matches!(command.get("url"), Some(Value::String(text)) if !text.trim().is_empty());
    }
    matches!(command.get("command"), Some(Value::String(text)) if !text.trim().is_empty())
}

pub fn run_quick_action_command(args: &[String]) -> CliResult<()> {
    /*
     * CDXC:MobileQuickActions 2026-07-12-00:00:
     * Mobile quick actions cannot reuse `run-command`: that routes through
     * gxserver renderer commands into the desktop app's command pane, which
     * the phone cannot see. `run-action` resolves the trusted HUD command on
     * the Mac (mobile sends only ids) and materializes a normal gxserver
     * terminal session running it, so the phone can attach like any other
     * session. Browser actions return the URL for the phone to open locally.
     */
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    let command_id = flags
        .text("commandId")
        .or_else(|| parsed.rest.first().cloned())
        .unwrap_or_default()
        .trim()
        .to_string();
    let project_id = normalize_required_project_id(flags.text("projectId"), "run-action")?;
    if command_id.is_empty() {
        return Err(CliError::Other(
            "run-action requires a command id.".to_string(),
        ));
    }
    let hud = call_gxserver_rpc(
        "/api/readSidebarHud",
        &json!({ "activeProjectId": project_id }),
        flags,
    )?;
    let action = hud
        .get("commands")
        .and_then(Value::as_array)
        .and_then(|commands| {
            commands.iter().find(|command| {
                command.get("commandId").and_then(Value::as_str) == Some(command_id.as_str())
            })
        });
    let action = match action {
        Some(action) if is_configured_mobile_quick_action(action) => action,
        _ => {
            print_json(&json!({
                "commandId": command_id,
                "error": "Quick action is not configured for this project.",
                "ok": false,
            }));
            set_exit_code(1);
            return Ok(());
        }
    };
    if action.get("actionType").and_then(Value::as_str) == Some("browser") {
        let mut map = Map::new();
        map.insert("actionType".to_string(), json!("browser"));
        map.insert("commandId".to_string(), json!(command_id));
        insert_present(&mut map, "name", action.get("name"));
        map.insert("ok".to_string(), json!(true));
        insert_present(&mut map, "url", action.get("url"));
        print_json(&Value::Object(map));
        return Ok(());
    }
    let payload = {
        let mut map = Map::new();
        insert_present(&mut map, "command", action.get("command"));
        map.insert("projectId".to_string(), json!(project_id));
        map.insert("start".to_string(), json!(true));
        map.insert(
            "title".to_string(),
            match action.get("name") {
                Some(name) if js_truthy(Some(name)) => name.clone(),
                _ => json!("Action"),
            },
        );
        Value::Object(map)
    };
    let created = match actions::send_gxserver_cli_action("createSession", &payload, flags) {
        Ok(created) => created,
        Err(error) => {
            print_json(&json!({
                "actionType": "terminal",
                "commandId": command_id,
                "error": error.to_string(),
                "ok": false,
            }));
            set_exit_code(1);
            return Ok(());
        }
    };
    let session_id = created
        .get("session")
        .and_then(|session| session.get("sessionId"));
    if is_failed_cli_result(&created) || !js_truthy(session_id) {
        print_json(&json!({
            "actionType": "terminal",
            "commandId": command_id,
            "error": "Could not start the quick action session.",
            "ok": false,
        }));
        set_exit_code(1);
        return Ok(());
    }
    let mut merged = created.as_object().cloned().unwrap_or_default();
    merged.insert("actionType".to_string(), json!("terminal"));
    merged.insert("commandId".to_string(), json!(command_id));
    merged.insert("ok".to_string(), json!(true));
    print_json(&Value::Object(merged));
    Ok(())
}

fn normalize_required_project_id(value: Option<String>, command_name: &str) -> CliResult<String> {
    let project_id = value.unwrap_or_default().trim().to_string();
    if project_id.is_empty() {
        return Err(CliError::Other(format!(
            "{command_name} requires --project-id until gxserver active-project routing lands."
        )));
    }
    Ok(project_id)
}

// ---------------------------------------------------------------------------
// gxserver state + session inventory
// ---------------------------------------------------------------------------

pub fn fetch_gxserver_state(flags: &Flags) -> CliResult<Value> {
    let projects_result = call_gxserver_rpc("/api/listProjects", &json!({}), flags)?;
    let sessions_result = fetch_gxserver_session_list(flags)?;
    let projects = match projects_result.get("projects") {
        Some(value) if !value.is_null() => value.clone(),
        _ => json!([]),
    };
    let sessions = match sessions_result.get("sessions") {
        Some(value) if !value.is_null() => value.clone(),
        _ => json!([]),
    };
    Ok(json!({
        "ok": true,
        "product": GXSERVER_PRODUCT,
        "projects": projects,
        "sessions": sessions,
    }))
}

pub fn fetch_gxserver_session_list(flags: &Flags) -> CliResult<Value> {
    match fetch_live_gxserver_session_list(flags) {
        Ok(result) => Ok(result),
        Err(error) => {
            if let Some(fallback) = read_persisted_gxserver_session_list(&error, flags) {
                return Ok(fallback);
            }
            Err(error)
        }
    }
}

fn fetch_live_gxserver_session_list(flags: &Flags) -> CliResult<Value> {
    let projects_response = call_gxserver_rpc("/api/listProjects", &json!({}), flags)?;
    let recent_projects_response = call_gxserver_rpc("/api/listRecentProjects", &json!({}), flags)?;
    /*
     * CDXC:GxserverActiveOnlySessionList 2026-09-01:
     * The default inventory drops stopped rows a few lines below, and mobile
     * re-runs `ghostex sessions --json --mobile-summary` over SSH every few
     * seconds per machine, so asking for them at all meant shipping the whole
     * stopped agent history across the hop just to throw it away. Ask the
     * daemon for the same set this list is going to keep. `--all` /
     * `--include-stopped` (and every session-action resolver, which sets both)
     * still request the full history.
     */
    let include_stopped = should_include_stopped_gxserver_sessions(flags);
    let sessions_response = call_gxserver_rpc(
        "/api/listSessions",
        &json!({ "includeStopped": include_stopped }),
        flags,
    )?;
    let presentation_response =
        call_gxserver_rpc("/api/readPresentationSnapshot", &json!({}), flags)?;
    let empty = Vec::new();
    let projects = projects_response
        .get("projects")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let sessions = sessions_response
        .get("sessions")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let snapshot = presentation_response.get("snapshot");
    let snapshot_sessions = snapshot.and_then(|snapshot| snapshot.get("sessions"));
    let presentation_by_session_key = presentation_session_map(snapshot_sessions);
    let presentation_order_by_session_key = presentation_session_order_map(snapshot_sessions);
    let active_projects: Vec<&Value> = projects
        .iter()
        .filter(|project| is_active_gxserver_inventory_project(project))
        .collect();
    let mut project_by_id: HashMap<Option<String>, &Value> = HashMap::new();
    for project in &active_projects {
        project_by_id.insert(value_key(project.get("projectId")), *project);
    }
    /*
     * CDXC:GxserverSessionInventory 2026-05-31-08:45 / 2026-06-04-03:33:
     * Default lists include running and sleeping sessions and hide stopped
     * rows; diagnostic callers may opt into stopped rows with
     * --all/--include-stopped. Presentation activity is overlaid onto the CLI
     * inventory so React Native Android, TUI, and gx share the same status contract.
     */
    let project_sessions: Vec<&Value> = sessions
        .iter()
        .filter(|session| project_by_id.contains_key(&value_key(session.get("projectId"))))
        .collect();
    let listed_sessions: Vec<&Value> = if should_include_stopped_gxserver_sessions(flags) {
        project_sessions
    } else {
        project_sessions
            .into_iter()
            .filter(|session| !is_stopped_gxserver_session(session))
            .collect()
    };
    let cli_sessions: Vec<Value> = listed_sessions
        .iter()
        .enumerate()
        .map(|(index, session)| {
            let key = cli_session_key(session.get("projectId"), session.get("sessionId"));
            to_cli_session(
                session,
                project_by_id
                    .get(&value_key(session.get("projectId")))
                    .copied(),
                index,
                presentation_by_session_key.get(&key).copied(),
                presentation_order_by_session_key.get(&key).copied(),
            )
        })
        .collect();
    let mut result = Map::new();
    result.insert("ok".to_string(), json!(true));
    result.insert("product".to_string(), json!(GXSERVER_PRODUCT));
    /*
     * CDXC:MobileProjectIcons 2026-08-21:
     * A project's DISCOVERED icon (the favicon its own repository ships) lives
     * only in the daemon's project_icon cache, so the CLI cannot read it: it is
     * a separate process with an empty cache. The presentation snapshot already
     * publishes it per project, so fold that key back onto the inventory rows
     * here and every consumer — mobile, TUI, `--json` — sees the same icon
     * chain the gpui sidebar ranks.
     */
    let discovered_project_icons =
        presentation_project_icon_map(snapshot.and_then(|snapshot| snapshot.get("projects")));
    result.insert(
        "projects".to_string(),
        Value::Array(
            active_projects
                .iter()
                .map(|project| {
                    let mut merged = (*project).clone();
                    let discovered = value_key(project.get("projectId"))
                        .and_then(|project_id| discovered_project_icons.get(&project_id))
                        .map(|value| (*value).clone());
                    if let (Some(map), Some(discovered)) = (merged.as_object_mut(), discovered) {
                        map.insert("discoveredIconDataUrl".to_string(), discovered);
                    }
                    merged
                })
                .collect(),
        ),
    );
    result.insert(
        "recentProjects".to_string(),
        recent_projects_response
            .get("recentProjects")
            .cloned()
            .unwrap_or_else(|| json!([])),
    );
    insert_present(&mut result, "revision", sessions_response.get("requestId"));
    result.insert("sessions".to_string(), Value::Array(cli_sessions));
    /*
     * CDXC:WorkspaceSessionGroups 2026-07-12-00:00:
     * gxserver's presentation snapshot carries the GPUI-authored named session
     * groups and sidebar project order; pass them through for mobile.
     */
    insert_present(
        &mut result,
        "workspaceGroups",
        snapshot.and_then(|snapshot| snapshot.get("workspaceGroups")),
    );
    /*
     * CDXC:SidebarProjectCollections 2026-07-18-00:00:
     * The presentation snapshot also carries the colored project-collection
     * overlay ("Group N" wrappers) so phones can render and edit the same
     * grouped project list as the desktop sidebar.
     */
    insert_present(
        &mut result,
        "sidebarProjectCollections",
        snapshot.and_then(|snapshot| snapshot.get("sidebarProjectCollections")),
    );
    /*
     * CDXC:SidebarSpaces 2026-08-27:
     * The snapshot also carries the daemon-owned saved sidebar filters so
     * phones render and edit the same Space row as the desktop sidebar.
     */
    insert_present(
        &mut result,
        "sidebarSpaces",
        snapshot.and_then(|snapshot| snapshot.get("sidebarSpaces")),
    );
    /*
     * CDXC:SidebarV2Lifecycle 2026-07-29-00:00:
     * Machine-scoped capability flags travel with the inventory so a client
     * talking to an older daemon hides settle/snooze affordances instead of
     * issuing RPCs that endpoint does not have.
     */
    insert_present(
        &mut result,
        "capabilities",
        snapshot.and_then(|snapshot| snapshot.get("capabilities")),
    );
    Ok(Value::Object(result))
}

/// `discoveredIconDataUrl` per projectId from the daemon's presentation
/// snapshot. Projects the icon pass has not reached publish no key at all, so
/// they are simply absent from the map.
fn presentation_project_icon_map(projects: Option<&Value>) -> HashMap<String, &Value> {
    let mut map = HashMap::new();
    if let Some(list) = projects.and_then(Value::as_array) {
        for project in list {
            let Some(project_id) = value_key(project.get("projectId")) else {
                continue;
            };
            if let Some(icon) = project.get("discoveredIconDataUrl") {
                map.insert(project_id, icon);
            }
        }
    }
    map
}

fn presentation_session_map(sessions: Option<&Value>) -> HashMap<String, &Value> {
    let mut map = HashMap::new();
    if let Some(list) = sessions.and_then(Value::as_array) {
        for session in list {
            let key = cli_session_key(session.get("projectId"), session.get("sessionId"));
            if !key.is_empty() {
                map.insert(key, session);
            }
        }
    }
    map
}

fn presentation_session_order_map(sessions: Option<&Value>) -> HashMap<String, usize> {
    let mut map = HashMap::new();
    if let Some(list) = sessions.and_then(Value::as_array) {
        for (index, session) in list.iter().enumerate() {
            let key = cli_session_key(session.get("projectId"), session.get("sessionId"));
            if !key.is_empty() {
                map.entry(key).or_insert(index);
            }
        }
    }
    map
}

fn cli_session_key(project_id: Option<&Value>, session_id: Option<&Value>) -> String {
    let normalized_project_id = js_string(project_id).trim().to_string();
    let normalized_session_id = js_string(session_id).trim().to_string();
    if !normalized_project_id.is_empty() && !normalized_session_id.is_empty() {
        format!("{normalized_project_id}:{normalized_session_id}")
    } else {
        String::new()
    }
}

fn is_active_gxserver_inventory_project(project: &Value) -> bool {
    /*
     * CDXC:ProjectVisibility 2026-06-30-21:23:
     * Filter shared inventory from gxserver domain visibility fields so parked
     * Recent Projects and hidden Remote Attach carrier projects do not reach
     * mobile summaries or full session lists.
     */
    project.get("isRecentProject") != Some(&Value::Bool(true))
        && project.get("visibility").and_then(Value::as_str) != Some("hidden")
        && project.get("systemKind").and_then(Value::as_str) != Some("remoteAttachCarrier")
}

fn is_mobile_chats_collection_project(project: &Value) -> bool {
    let explicit = |key: &str| project.get(key) == Some(&Value::Bool(true));
    let launch_setting = |key: &str| {
        project
            .get("launchSettings")
            .and_then(|settings| settings.get(key))
            == Some(&Value::Bool(true))
    };
    explicit("isChat")
        || explicit("isQuick")
        || launch_setting("isChat")
        || launch_setting("isQuick")
        || project
            .get("path")
            .and_then(Value::as_str)
            .map(is_mobile_chats_storage_path)
            .unwrap_or(false)
}

fn is_mobile_chats_storage_path(value: &str) -> bool {
    let normalized = value.replace('\\', "/");
    let segments: Vec<&str> = normalized
        .trim_end_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != "~")
        .collect();
    segments.windows(2).any(|pair| {
        pair[1] == "chats"
            && (pair[0] == "ghostex"
                || pair[0] == ".active"
                || pair[0] == ".ghostex"
                || pair[0].starts_with(".ghostex-"))
    })
}

fn should_use_local_gxserver_state_fallback(flags: &Flags) -> bool {
    let server = flags
        .text("server")
        .or_else(|| std::env::var("GHOSTEX_GXSERVER_SERVER").ok())
        .unwrap_or_else(|| "local".to_string());
    let server = server.trim();
    let server = if server.is_empty() { "local" } else { server };
    server == "local"
}

fn read_persisted_gxserver_server_id() -> Option<String> {
    let text = std::fs::read_to_string(gxserver_root().join("identity.json")).unwrap_or_default();
    let identity = parse_json_value(&text)?;
    identity
        .get("serverId")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn should_include_stopped_gxserver_sessions(flags: &Flags) -> bool {
    strict_true(flags, "all")
        || strict_true(flags, "includeStopped")
        || strict_true(flags, "stopped")
}

fn is_stopped_gxserver_session(session: &Value) -> bool {
    js_string(session.get("lifecycleState")) == "stopped"
}

// ---------------------------------------------------------------------------
// persisted-state fallback (read-only SQLite)
// ---------------------------------------------------------------------------

fn read_persisted_gxserver_session_list(cause: &CliError, flags: &Flags) -> Option<Value> {
    if !should_use_local_gxserver_state_fallback(flags) {
        return None;
    }
    let db_path = gxserver_root().join("state.db");
    if !db_path.exists() {
        return None;
    }
    let rows = read_persisted_gxserver_inventory_rows(&db_path, true)
        .or_else(|| read_persisted_gxserver_inventory_rows(&db_path, false))?;
    let server_id = read_persisted_gxserver_server_id();
    Some(build_persisted_gxserver_session_list(
        &rows,
        server_id.as_deref(),
        &cause.to_string(),
        flags,
    ))
}

fn build_persisted_gxserver_session_list(
    rows: &[Value],
    server_id: Option<&str>,
    error_message: &str,
    flags: &Flags,
) -> Value {
    let projects: Vec<Value> = rows
        .iter()
        .filter(|row| row.get("rowType").and_then(Value::as_str) == Some("project"))
        .map(|row| {
            let mut map = Map::new();
            let is_recent = row
                .get("isRecentProject")
                .map(|value| value.as_i64() == Some(1) || value == &Value::Bool(true))
                .unwrap_or(false);
            map.insert("isRecentProject".to_string(), json!(is_recent));
            insert_present(&mut map, "name", row.get("name"));
            insert_present(&mut map, "path", row.get("path"));
            insert_present(&mut map, "projectId", row.get("projectId"));
            insert_non_null(&mut map, "systemKind", row.get("systemKind"));
            map.insert(
                "visibility".to_string(),
                match row.get("visibility") {
                    Some(value) if !value.is_null() => value.clone(),
                    _ => json!("visible"),
                },
            );
            Value::Object(map)
        })
        .filter(is_active_gxserver_inventory_project)
        .collect();
    let mut project_by_id: HashMap<Option<String>, &Value> = HashMap::new();
    for project in &projects {
        project_by_id.insert(value_key(project.get("projectId")), project);
    }
    let sessions: Vec<Value> = rows
        .iter()
        .filter(|row| row.get("rowType").and_then(Value::as_str) == Some("session"))
        .filter(|row| project_by_id.contains_key(&value_key(row.get("projectId"))))
        .map(|row| {
            let mut map = Map::new();
            insert_non_null(&mut map, "agentId", row.get("agentId"));
            insert_non_null(&mut map, "cwd", row.get("cwd"));
            let server_truthy = server_id.map(|id| !id.is_empty()).unwrap_or(false);
            if server_truthy && js_truthy(row.get("projectId")) && js_truthy(row.get("sessionId")) {
                map.insert(
                    "globalRef".to_string(),
                    json!(format!(
                        "{}:{}:{}",
                        server_id.unwrap_or_default(),
                        js_template(row.get("projectId")),
                        js_template(row.get("sessionId"))
                    )),
                );
            }
            insert_present(&mut map, "kind", row.get("kind"));
            insert_non_null(&mut map, "lastActiveAt", row.get("lastActiveAt"));
            insert_present(&mut map, "lifecycleState", row.get("lifecycleState"));
            insert_present(&mut map, "projectId", row.get("projectId"));
            map.insert(
                "providerState".to_string(),
                parse_persisted_provider_state(row.get("providerStateJson")),
            );
            insert_present(&mut map, "sessionId", row.get("sessionId"));
            insert_present(&mut map, "title", row.get("title"));
            insert_present(&mut map, "updatedAt", row.get("updatedAt"));
            insert_present(&mut map, "zmxName", row.get("zmxName"));
            Value::Object(map)
        })
        .collect();
    let listed_sessions: Vec<&Value> = if should_include_stopped_gxserver_sessions(flags) {
        sessions.iter().collect()
    } else {
        sessions
            .iter()
            .filter(|session| !is_stopped_gxserver_session(session))
            .collect()
    };
    /*
     * CDXC:CliSessions 2026-06-03-20:28:
     * Bridge-down session inventory degrades from gxserver's own durable state.
     * Keep the fallback read-only and visibly marked so humans and Android can
     * distinguish stale persisted rows from live daemon data.
     */
    let cli_sessions: Vec<Value> = listed_sessions
        .iter()
        .enumerate()
        .map(|(index, session)| {
            to_cli_session(
                session,
                project_by_id
                    .get(&value_key(session.get("projectId")))
                    .copied(),
                index,
                None,
                None,
            )
        })
        .collect();
    json!({
        "error": error_message,
        "fallback": "persisted-gxserver-state",
        "ok": true,
        "product": GXSERVER_PRODUCT,
        "projects": projects,
        "sessions": cli_sessions,
    })
}

fn parse_persisted_provider_state(value: Option<&Value>) -> Value {
    match value {
        Some(Value::String(text)) => parse_json_value(text)
            .filter(|parsed| !parsed.is_null())
            .unwrap_or_else(|| json!({})),
        Some(Value::Number(number)) => Value::Number(number.clone()),
        Some(Value::Bool(flag)) => Value::Bool(*flag),
        _ => json!({}),
    }
}

fn read_persisted_gxserver_inventory_rows(
    db_path: &Path,
    include_project_visibility_columns: bool,
) -> Option<Vec<Value>> {
    let connection =
        rusqlite::Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .ok()?;
    read_persisted_gxserver_inventory_rows_from(&connection, include_project_visibility_columns)
}

fn read_persisted_gxserver_inventory_rows_from(
    connection: &rusqlite::Connection,
    include_project_visibility_columns: bool,
) -> Option<Vec<Value>> {
    let sql = create_persisted_gxserver_inventory_sql(include_project_visibility_columns);
    let mut statement = connection.prepare(&sql).ok()?;
    let column_names: Vec<String> = statement
        .column_names()
        .iter()
        .map(|name| name.to_string())
        .collect();
    let mut rows = statement.query([]).ok()?;
    let mut result: Vec<Value> = Vec::new();
    loop {
        match rows.next() {
            Ok(Some(row)) => {
                let mut map = Map::new();
                for (index, name) in column_names.iter().enumerate() {
                    let value = match row.get_ref(index) {
                        Ok(value) => value,
                        Err(_) => return None,
                    };
                    map.insert(name.clone(), sqlite_value_to_json(value));
                }
                result.push(Value::Object(map));
            }
            Ok(None) => break,
            Err(_) => return None,
        }
    }
    Some(result)
}

fn sqlite_value_to_json(value: rusqlite::types::ValueRef<'_>) -> Value {
    use rusqlite::types::ValueRef;
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(number) => json!(number),
        ValueRef::Real(number) => serde_json::Number::from_f64(number)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        ValueRef::Text(text) => Value::String(String::from_utf8_lossy(text).into_owned()),
        ValueRef::Blob(blob) => Value::String(String::from_utf8_lossy(blob).into_owned()),
    }
}

fn create_persisted_gxserver_inventory_sql(include_project_visibility_columns: bool) -> String {
    let project_visibility_columns = if include_project_visibility_columns {
        "isRecentProject, visibility, systemKind"
    } else {
        "isRecentProject, 'visible' AS visibility, NULL AS systemKind"
    };
    [
        format!("SELECT 'project' AS rowType, projectId, name, path, {project_visibility_columns}, NULL AS sessionId, NULL AS kind, NULL AS title, NULL AS lifecycleState, NULL AS providerStateJson, NULL AS zmxName, NULL AS cwd, NULL AS agentId, NULL AS updatedAt, NULL AS lastActiveAt FROM projects"),
        "UNION ALL".to_string(),
        "SELECT 'session' AS rowType, projectId, NULL AS name, NULL AS path, NULL AS isRecentProject, NULL AS visibility, NULL AS systemKind, sessionId, kind, title, lifecycleState, providerStateJson, zmxName, cwd, agentId, updatedAt, lastActiveAt FROM sessions".to_string(),
        "ORDER BY rowType ASC, updatedAt DESC, projectId ASC, sessionId ASC".to_string(),
    ]
    .join(" ")
}

// ---------------------------------------------------------------------------
// toCliSession
// ---------------------------------------------------------------------------

fn to_cli_session(
    session: &Value,
    project: Option<&Value>,
    index: usize,
    presentation_session: Option<&Value>,
    presentation_order: Option<usize>,
) -> Value {
    let p = |key: &str| presentation_session.and_then(|value| value.get(key));
    let s = |key: &str| session.get(key);
    let lifecycle_state = js_string(s("lifecycleState"));
    let provider_state =
        js_string(s("providerState").and_then(|value| value.get("lifecycleState")));
    let activity = normalize_cli_session_activity(p("activity"));
    let status: Value = if lifecycle_state == "sleeping" {
        json!("sleep")
    } else if lifecycle_state == "stopped" {
        json!("stopped")
    } else if lifecycle_state == "running" {
        json!("running")
    } else if !provider_state.is_empty() {
        json!(provider_state)
    } else if !lifecycle_state.is_empty() {
        json!(lifecycle_state)
    } else {
        json!("unknown")
    };
    let provider_session_name = js_coalesce(&[
        s("zmxName"),
        s("providerState").and_then(|value| value.get("zmxName")),
    ]);
    let title = js_coalesce(&[p("title"), s("title")]);
    /*
     * CDXC:GxserverSessionInventory 2026-06-22-00:47:
     * Prefer presentation identity for the agent provider session id because
     * it is already the UI contract; fall back to listSessions runtime
     * metadata when a snapshot row is unavailable.
     */
    let agent_session_id = string_flag(js_coalesce(&[
        p("agentSessionId"),
        s("runtimeSettings").and_then(|value| value.get("agentSessionId")),
    ]));
    let agent_session_path = string_flag(js_coalesce(&[
        p("agentSessionPath"),
        s("runtimeSettings").and_then(|value| value.get("agentSessionPath")),
    ]));
    /*
     * CDXC:GxserverSessionTitles 2026-06-07-09:33:
     * Expose gxserver's rendered display title separately from the raw durable
     * title so clients can show unsynced/placeholder chrome without leaking
     * display glyphs into rename or restore payloads.
     */
    let display_title = js_coalesce(&[p("displayTitle"), title]);
    let is_live = lifecycle_state == "running" || provider_state == "exists";
    let mut map = Map::new();
    insert_js(&mut map, "actions", &[p("actions")]);
    insert_js(&mut map, "agent", &[s("agentId")]);
    insert_js(&mut map, "agentId", &[s("agentId")]);
    insert_js(&mut map, "agentIcon", &[p("agentIcon"), s("agentId")]);
    insert_js(&mut map, "agentName", &[p("agentName")]);
    map.insert("agentSessionId".to_string(), agent_session_id);
    map.insert("agentSessionPath".to_string(), agent_session_path);
    map.insert("alias".to_string(), json!(index as i64 + 1));
    insert_js(&mut map, "attention", &[p("attention")]);
    insert_js(&mut map, "createdAt", &[p("createdAt"), s("createdAt")]);
    insert_js(&mut map, "globalRef", &[s("globalRef")]);
    insert_js(&mut map, "groupId", &[p("groupId")]);
    insert_js(&mut map, "displayTitle", &[display_title]);
    insert_js(
        &mut map,
        "displayTitleTooltip",
        &[p("displayTitleTooltip"), display_title],
    );
    map.insert("isFocused".to_string(), json!(false));
    insert_js(&mut map, "isFavorite", &[p("isFavorite"), s("isFavorite")]);
    map.insert("isLocalOnly".to_string(), json!(false));
    insert_js(&mut map, "isParked", &[p("isParked"), s("isParked")]);
    insert_js(&mut map, "isPinned", &[p("isPinned"), s("isPinned")]);
    insert_js(&mut map, "sessionTag", &[p("sessionTag"), s("sessionTag")]);
    /*
     * CDXC:SidebarV2Lifecycle 2026-07-29-00:00:
     * Settle/snooze is server-owned inbox state, so the CLI inventory carries
     * it alongside pins and tags. Absent keys mean "never settled / never
     * snoozed", which is also what an older daemon and a pre-migration
     * state.db produce.
     */
    insert_js(&mut map, "settledAt", &[p("settledAt"), s("settledAt")]);
    insert_js(
        &mut map,
        "settledOverride",
        &[p("settledOverride"), s("settledOverride")],
    );
    insert_js(&mut map, "snoozedAt", &[p("snoozedAt"), s("snoozedAt")]);
    insert_js(
        &mut map,
        "snoozedUntil",
        &[p("snoozedUntil"), s("snoozedUntil")],
    );
    /*
     * CDXC:SidebarV2GitStatus 2026-07-29-00:00:
     * Branch / +n −n / PR badge is resolved once per session cwd by the daemon,
     * so the CLI inventory forwards it verbatim rather than shelling out to git
     * per row. Presentation is the only source: a session row in state.db has no
     * git state of its own, and an older daemon simply omits the key.
     */
    insert_js(&mut map, "gitStatus", &[p("gitStatus")]);
    // Host-timer chrome for the mobile session menus; absent when the
    // presentation snapshot does not carry resolved timer projections.
    insert_js(&mut map, "closeAfterDone", &[p("closeAfterDone")]);
    insert_js(
        &mut map,
        "delayedSendRemainingLabel",
        &[p("delayedSendRemainingLabel")],
    );
    /*
     * CDXC:SessionChatPromptQueue 2026-08-21-b:
     * The phone's session-list badge reads these two off the mobile summary, so
     * the inventory has to forward them from the presentation snapshot the same
     * way it forwards the Delayed Send countdown above. Without this the badge
     * is wired end to end on the mobile side and can never light up. Absent
     * means "no queue" / "nothing failed", which is also what a daemon that
     * predates the queue publishes.
     */
    insert_js(&mut map, "queuedPromptCount", &[p("queuedPromptCount")]);
    insert_js(
        &mut map,
        "queuedPromptFailedCount",
        &[p("queuedPromptFailedCount")],
    );
    /*
     * CDXC:SessionAgentNotes 2026-08-24:
     * The phone's session row renders the note dot and the note text from this
     * field, so the inventory forwards the presentation value verbatim.
     * Presentation is the only source: a session row in state.db has no note of
     * its own (notes are keyed by agent session id), and an older daemon simply
     * omits the key.
     */
    insert_js(&mut map, "sessionNote", &[p("sessionNote")]);
    insert_js(
        &mut map,
        "sendWhenAllProjectSessionsStopActive",
        &[p("sendWhenAllProjectSessionsStopActive")],
    );
    insert_js(
        &mut map,
        "sendWhenAgentStopsActive",
        &[p("sendWhenAgentStopsActive")],
    );
    map.insert("isLive".to_string(), json!(is_live));
    insert_js(
        &mut map,
        "isPrimaryTitleTerminalTitle",
        &[p("isPrimaryTitleTerminalTitle")],
    );
    map.insert(
        "isSleeping".to_string(),
        json!(lifecycle_state == "sleeping"),
    );
    insert_js(&mut map, "isTemporaryTitle", &[p("isTemporaryTitle")]);
    insert_js(&mut map, "kind", &[p("kind"), s("kind")]);
    insert_js(
        &mut map,
        "lastActiveAt",
        &[p("lastActiveAt"), s("lastActiveAt")],
    );
    insert_js(
        &mut map,
        "lastInteractionAt",
        &[p("lastActiveAt"), s("lastActiveAt"), s("updatedAt")],
    );
    map.insert("lifecycleState".to_string(), json!(lifecycle_state));
    map.insert("ownership".to_string(), json!("gxserver"));
    insert_js(&mut map, "primaryTitle", &[p("primaryTitle")]);
    insert_js(&mut map, "projectId", &[s("projectId")]);
    insert_js(
        &mut map,
        "projectName",
        &[project.and_then(|value| value.get("name")), s("projectId")],
    );
    match js_coalesce(&[
        p("cwd"),
        s("cwd"),
        project.and_then(|value| value.get("path")),
    ]) {
        Some(value) if !value.is_null() => {
            map.insert("projectPath".to_string(), value.clone());
        }
        _ => {
            map.insert("projectPath".to_string(), json!(""));
        }
    }
    map.insert("provider".to_string(), json!("zmx"));
    insert_js(&mut map, "providerSessionName", &[provider_session_name]);
    if !provider_state.is_empty() {
        map.insert("providerSessionState".to_string(), json!(provider_state));
    }
    insert_js(&mut map, "sessionId", &[s("sessionId")]);
    insert_js(&mut map, "sessionPersistenceName", &[provider_session_name]);
    map.insert("sessionPersistenceProvider".to_string(), json!("zmx"));
    insert_js(&mut map, "sidebarOrder", &[p("sidebarOrder")]);
    insert_js(&mut map, "sortKey", &[p("sortKey")]);
    if let Some(order) = presentation_order {
        map.insert("sortOrder".to_string(), json!(order as i64));
    }
    map.insert("status".to_string(), status);
    map.insert("activity".to_string(), json!(activity));
    insert_js(&mut map, "surface", &[p("surface")]);
    insert_js(&mut map, "terminalTitle", &[p("terminalTitle")]);
    insert_js(&mut map, "title", &[title]);
    insert_js(&mut map, "titleSource", &[p("titleSource")]);
    insert_js(&mut map, "trustedResumeTitle", &[p("trustedResumeTitle")]);
    insert_js(&mut map, "updatedAt", &[p("updatedAt"), s("updatedAt")]);
    insert_js(
        &mut map,
        "visibleInSidebarByDefault",
        &[p("visibleInSidebarByDefault")],
    );
    insert_js(&mut map, "zmxName", &[p("zmxName"), s("zmxName")]);
    Value::Object(map)
}

/// JS stringFlag(): non-strings coerce (null/undefined -> null); strings trim
/// and empty trims become null.
fn string_flag(value: Option<&Value>) -> Value {
    match value {
        None | Some(Value::Null) => Value::Null,
        Some(Value::String(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Value::Null
            } else {
                Value::String(trimmed.to_string())
            }
        }
        Some(other) => Value::String(js_display(other)),
    }
}

fn normalize_cli_session_activity(value: Option<&Value>) -> String {
    let normalized = js_string(value)
        .trim()
        .to_lowercase()
        .replace('_', "-")
        .replace(' ', "-");
    if normalized == "attention"
        || normalized == "needs-attention"
        || normalized == "attention-required"
    {
        return "attention".to_string();
    }
    if normalized == "working"
        || normalized == "active"
        || normalized == "busy"
        || normalized == "processing"
    {
        return "working".to_string();
    }
    "idle".to_string()
}

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
     * CDXC:GxserverCliAttach 2026-06-09-09:53:
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

fn should_start_missing_provider_for_cli_attach(attach: Option<&Value>) -> bool {
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

fn apply_attach_metadata_to_cli_session(
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

fn normalize_startup_text_for_shell(value: Option<&Value>) -> Option<String> {
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
    Err(CliError::Other(format!(
        "No gxserver session matched \"{selector}\"."
    )))
}

// ---------------------------------------------------------------------------
// fetchSessionList + mobile summary contract
// ---------------------------------------------------------------------------

/// fetchSessionList(flags, options): returns the CLI session objects
/// (toCliSession shape). The bool mirrors the Node CLI's only option,
/// `writeCache` (refresh the session-alias cache in Ghostex state storage).
pub fn fetch_session_list(flags: &Flags, write_cache: bool) -> CliResult<Vec<Value>> {
    let result = fetch_session_list_result(flags, write_cache)?;
    Ok(result
        .get("sessions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

/// Full fetchSessionList result object (`{ ...listSessions result, sessions }`).
fn fetch_session_list_result(flags: &Flags, write_cache: bool) -> CliResult<Value> {
    let result = fetch_gxserver_session_list(flags)?;
    /*
     * CDXC:AndroidRemoteSessions 2026-06-11-23:52:
     * `ghostex sessions --json` is the React Native Android reconnect/status
     * contract. The inventory must come from gxserver list/snapshot APIs and
     * must not read the retired macOS sidebar persistence file when the daemon
     * is unreachable.
     */
    if is_failed_cli_result(&result) {
        let message = match result.get("error") {
            Some(value) if !value.is_null() => js_display(value),
            _ => "Could not list Ghostex sessions.".to_string(),
        };
        return Err(CliError::Other(message));
    }
    let sessions = result
        .get("sessions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if write_cache {
        let mut cache = Map::new();
        cache.insert(
            "createdAt".to_string(),
            json!(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
        );
        insert_present(&mut cache, "revision", result.get("revision"));
        cache.insert("sessions".to_string(), Value::Array(sessions.clone()));
        selector::write_session_alias_cache(&Value::Object(cache))?;
    }
    let mut merged = result.as_object().cloned().unwrap_or_default();
    merged.insert("sessions".to_string(), Value::Array(sessions));
    Ok(Value::Object(merged))
}

fn to_mobile_session_list(result: &Value) -> Value {
    /*
     * CDXC:iOSRemoteSessions 2026-06-30-04:37:
     * The mobile summary JSON keeps only row/action identity fields so the
     * phone does less network transfer, JSON parsing, and SwiftUI diff work
     * than the full diagnostic `sessions --json` contract.
     *
     * CDXC:ProjectVisibility 2026-06-30-21:23:
     * Mobile summaries must preserve gxserver's active-project filter even if
     * a future caller passes unfiltered inventory into this compactor.
     */
    let projects: Option<Vec<&Value>> =
        result
            .get("projects")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter(|project| is_active_gxserver_inventory_project(project))
                    .collect()
            });
    let active_project_ids: Option<std::collections::HashSet<String>> =
        projects.as_ref().map(|list| {
            list.iter()
                .filter_map(|project| project.get("projectId"))
                .filter(|value| js_truthy(Some(value)))
                .map(|value| value.to_string())
                .collect()
        });
    let sessions: Vec<&Value> = result
        .get("sessions")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter(|session| match &active_project_ids {
                    None => true,
                    Some(ids) => session
                        .get("projectId")
                        .map(|value| ids.contains(&value.to_string()))
                        .unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default();
    /*
     * CDXC:WorkspaceSessionGroups 2026-07-12-00:00:
     * Mobile rows must render in the same order as the GPUI sidebar: pre-sort
     * by gxserver's presentation snapshot order (`sortOrder`) and forward the
     * named-group overlay (`workspaceGroups`).
     */
    let mut ordered_sessions = sessions;
    ordered_sessions.sort_by(|left, right| {
        mobile_sort_order(left)
            .partial_cmp(&mobile_sort_order(right))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut map = Map::new();
    insert_present(&mut map, "capabilities", result.get("capabilities"));
    insert_present(&mut map, "fallback", result.get("fallback"));
    map.insert(
        "ok".to_string(),
        json!(result.get("ok") != Some(&Value::Bool(false))),
    );
    insert_present(&mut map, "product", result.get("product"));
    if let Some(projects) = &projects {
        map.insert(
            "projects".to_string(),
            Value::Array(
                projects
                    .iter()
                    .map(|project| {
                        let mut project_map = Map::new();
                        insert_present(&mut project_map, "name", project.get("name"));
                        insert_present(&mut project_map, "path", project.get("path"));
                        insert_present(&mut project_map, "projectId", project.get("projectId"));
                        /*
                         * CDXC:MobileProjectIcons 2026-08-21:
                         * The phone's sessions list ranks project icons exactly
                         * like SidebarV2ProjectIcon does — user image, then the
                         * icon the repository ships, then a typed glyph — so it
                         * needs all three inputs plus the workspace theme color
                         * the branched project rail is drawn from.
                         */
                        let identity_icon = project.get("identityIcon");
                        insert_present(
                            &mut project_map,
                            "icon",
                            identity_icon.and_then(|icon| icon.get("icon")),
                        );
                        insert_present(
                            &mut project_map,
                            "iconDataUrl",
                            identity_icon.and_then(|icon| icon.get("iconDataUrl")),
                        );
                        insert_present(
                            &mut project_map,
                            "themeColor",
                            identity_icon.and_then(|icon| icon.get("themeColor")),
                        );
                        insert_present(
                            &mut project_map,
                            "discoveredIconDataUrl",
                            project.get("discoveredIconDataUrl"),
                        );
                        insert_present(
                            &mut project_map,
                            "worktree",
                            project
                                .get("worktreeJson")
                                .or_else(|| project.get("worktree")),
                        );
                        project_map.insert(
                            "isChat".to_string(),
                            json!(is_mobile_chats_collection_project(project)),
                        );
                        Value::Object(project_map)
                    })
                    .collect(),
            ),
        );
    }
    let recent_projects = result
        .get("recentProjects")
        .and_then(Value::as_array)
        .map(|projects| {
            projects
                .iter()
                .filter_map(|project| {
                    let project_id = project.get("projectId")?.as_str()?.trim();
                    if project_id.is_empty() {
                        return None;
                    }
                    let mut recent = Map::new();
                    insert_present(&mut recent, "path", project.get("path"));
                    recent.insert("projectId".to_string(), json!(project_id));
                    insert_present(&mut recent, "recentClosedAt", project.get("recentClosedAt"));
                    insert_present(&mut recent, "sessionCount", project.get("sessionCount"));
                    insert_present(&mut recent, "title", project.get("title"));
                    Some(Value::Object(recent))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    map.insert("recentProjects".to_string(), Value::Array(recent_projects));
    insert_present(&mut map, "revision", result.get("revision"));
    map.insert(
        "sessions".to_string(),
        Value::Array(
            ordered_sessions
                .iter()
                .map(|session| to_mobile_session_summary(session))
                .collect(),
        ),
    );
    if let Some(collections) =
        to_mobile_sidebar_project_collections(result.get("sidebarProjectCollections"))
    {
        map.insert("sidebarProjectCollections".to_string(), collections);
    }
    if let Some(spaces) = to_mobile_sidebar_spaces(result.get("sidebarSpaces")) {
        map.insert("sidebarSpaces".to_string(), spaces);
    }
    if let Some(groups) = to_mobile_workspace_groups(result.get("workspaceGroups")) {
        map.insert("workspaceGroups".to_string(), groups);
    }
    Value::Object(map)
}

fn mobile_sort_order(session: &Value) -> f64 {
    const MAX_SAFE_INTEGER: f64 = 9007199254740991.0;
    session
        .get("sortOrder")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .unwrap_or(MAX_SAFE_INTEGER)
}

fn to_mobile_workspace_groups(workspace_groups: Option<&Value>) -> Option<Value> {
    let object = workspace_groups?.as_object()?;
    let project_order: Vec<Value> = object
        .get("projectOrder")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter(|value| matches!(value, Value::String(text) if !text.is_empty()))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let mut projects = Map::new();
    if let Some(project_map) = object.get("projects").and_then(Value::as_object) {
        for (project_id, project_groups) in project_map {
            let mut groups: Vec<Value> = Vec::new();
            if let Some(list) = project_groups.get("groups").and_then(Value::as_array) {
                for group in list {
                    let group_id = match group.get("groupId") {
                        Some(Value::String(text)) if !text.is_empty() => text,
                        _ => continue,
                    };
                    let session_ids: Vec<Value> = group
                        .get("sessionIds")
                        .and_then(Value::as_array)
                        .map(|ids| {
                            ids.iter()
                                .filter(|value| {
                                    matches!(value, Value::String(text) if !text.is_empty())
                                })
                                .cloned()
                                .collect()
                        })
                        .unwrap_or_default();
                    let title = match group.get("title") {
                        Some(Value::String(text)) if !text.is_empty() => text.clone(),
                        _ => group_id.clone(),
                    };
                    groups.push(json!({
                        "groupId": group_id,
                        "sessionIds": session_ids,
                        "title": title,
                    }));
                }
            }
            if !groups.is_empty() {
                projects.insert(project_id.clone(), json!({ "groups": groups }));
            }
        }
    }
    if project_order.is_empty() && projects.is_empty() {
        return None;
    }
    Some(json!({ "projectOrder": project_order, "projects": projects }))
}

fn to_mobile_sidebar_project_collections(collections_state: Option<&Value>) -> Option<Value> {
    /*
     * CDXC:SidebarProjectCollections 2026-07-18-00:00:
     * Mobile keeps the server-normalized {order, collections} contract but
     * re-sanitizes rows because fallback caches may carry stale shapes. Empty
     * overlays collapse to an absent key so phones can cheaply skip rendering.
     */
    let object = collections_state?.as_object()?;
    let mut collections = Map::new();
    if let Some(entries) = object.get("collections").and_then(Value::as_object) {
        for (collection_id, collection) in entries {
            if collection_id.is_empty() {
                continue;
            }
            let project_ids: Vec<Value> = collection
                .get("projectIds")
                .and_then(Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter(|value| matches!(value, Value::String(text) if !text.is_empty()))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            if project_ids.is_empty() {
                continue;
            }
            let title = match collection.get("title") {
                Some(Value::String(text)) if !text.is_empty() => text.clone(),
                _ => collection_id.clone(),
            };
            let color = match collection.get("color") {
                Some(Value::String(text)) if !text.is_empty() => text.clone(),
                _ => "transparent".to_string(),
            };
            collections.insert(
                collection_id.clone(),
                json!({
                    "collectionId": collection_id,
                    "color": color,
                    "projectIds": project_ids,
                    "title": title,
                }),
            );
        }
    }
    if collections.is_empty() {
        return None;
    }
    let mut order: Vec<Value> = Vec::new();
    let mut seen_order_ids = std::collections::HashSet::new();
    if let Some(entries) = object.get("order").and_then(Value::as_array) {
        for entry in entries {
            let Some(id) = entry.as_str() else { continue };
            if collections.contains_key(id) && seen_order_ids.insert(id.to_string()) {
                order.push(Value::String(id.to_string()));
            }
        }
    }
    for collection_id in collections.keys() {
        if seen_order_ids.insert(collection_id.clone()) {
            order.push(Value::String(collection_id.clone()));
        }
    }
    let next_collection_number = object
        .get("nextCollectionNumber")
        .and_then(Value::as_i64)
        .filter(|value| *value >= 1)
        .unwrap_or((collections.len() as i64) + 1);
    Some(json!({
        "collections": collections,
        "nextCollectionNumber": next_collection_number,
        "order": order,
    }))
}

fn to_mobile_sidebar_spaces(spaces_state: Option<&Value>) -> Option<Value> {
    /*
     * CDXC:SidebarSpaces 2026-08-27:
     * Mobile keeps the server-normalized {order, spaces} contract but
     * re-sanitizes rows because fallback caches may carry stale shapes. A Space
     * with no members is kept — it is a real, selectable, still-empty filter —
     * so only a document with no Spaces at all collapses to an absent key.
     */
    let object = spaces_state?.as_object()?;
    let mut spaces = Map::new();
    if let Some(entries) = object.get("spaces").and_then(Value::as_object) {
        for (space_id, space) in entries {
            if space_id.is_empty() {
                continue;
            }
            let member_ids = |key: &str| -> Vec<Value> {
                space
                    .get(key)
                    .and_then(Value::as_array)
                    .map(|ids| {
                        ids.iter()
                            .filter(
                                |value| matches!(value, Value::String(text) if !text.is_empty()),
                            )
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default()
            };
            let name = match space.get("name") {
                Some(Value::String(text)) if !text.is_empty() => text.clone(),
                _ => space_id.clone(),
            };
            let icon = match space.get("icon") {
                Some(Value::String(text)) if !text.is_empty() => text.clone(),
                _ => "stack".to_string(),
            };
            let color = match space.get("color") {
                Some(Value::String(text)) if !text.is_empty() => text.clone(),
                _ => "#4f5663".to_string(),
            };
            spaces.insert(
                space_id.clone(),
                json!({
                    "color": color,
                    "icon": icon,
                    "memberCollectionIds": member_ids("memberCollectionIds"),
                    "memberProjectIds": member_ids("memberProjectIds"),
                    "name": name,
                    "spaceId": space_id,
                }),
            );
        }
    }
    if spaces.is_empty() {
        return None;
    }
    let mut order: Vec<Value> = Vec::new();
    let mut seen_order_ids = std::collections::HashSet::new();
    if let Some(entries) = object.get("order").and_then(Value::as_array) {
        for entry in entries {
            let Some(id) = entry.as_str() else { continue };
            if spaces.contains_key(id) && seen_order_ids.insert(id.to_string()) {
                order.push(Value::String(id.to_string()));
            }
        }
    }
    for space_id in spaces.keys() {
        if seen_order_ids.insert(space_id.clone()) {
            order.push(Value::String(space_id.clone()));
        }
    }
    Some(json!({
        "order": order,
        "spaces": spaces,
    }))
}

fn to_mobile_session_summary(session: &Value) -> Value {
    let s = |key: &str| session.get(key);
    let mut map = Map::new();
    insert_js(&mut map, "activity", &[s("activity")]);
    insert_js(&mut map, "agent", &[s("agent"), s("agentId")]);
    insert_js(&mut map, "agentIcon", &[s("agentIcon")]);
    insert_js(&mut map, "agentName", &[s("agentName")]);
    insert_js(&mut map, "alias", &[s("alias")]);
    insert_js(&mut map, "displayTitle", &[s("displayTitle"), s("title")]);
    insert_js(&mut map, "groupId", &[s("groupId")]);
    insert_js(&mut map, "isFavorite", &[s("isFavorite")]);
    insert_js(&mut map, "isFocused", &[s("isFocused")]);
    insert_js(&mut map, "isLive", &[s("isLive")]);
    insert_js(&mut map, "isParked", &[s("isParked")]);
    insert_js(&mut map, "isPinned", &[s("isPinned")]);
    insert_js(&mut map, "isSleeping", &[s("isSleeping")]);
    insert_js(&mut map, "kind", &[s("kind")]);
    insert_js(
        &mut map,
        "lastInteractionAt",
        &[s("lastInteractionAt"), s("lastActiveAt"), s("updatedAt")],
    );
    insert_js(&mut map, "nativePaneState", &[s("nativePaneState")]);
    insert_js(&mut map, "projectId", &[s("projectId")]);
    insert_js(&mut map, "projectName", &[s("projectName")]);
    insert_js(&mut map, "projectPath", &[s("projectPath")]);
    insert_js(&mut map, "provider", &[s("provider")]);
    insert_js(
        &mut map,
        "providerSessionName",
        &[s("providerSessionName"), s("sessionPersistenceName")],
    );
    insert_js(
        &mut map,
        "providerSessionState",
        &[s("providerSessionState")],
    );
    insert_js(&mut map, "sessionId", &[s("sessionId")]);
    insert_js(
        &mut map,
        "shouldSubmitStagedFirstPromptTitleCommand",
        &[s("shouldSubmitStagedFirstPromptTitleCommand")],
    );
    /*
     * CDXC:SidebarV2Lifecycle 2026-07-29-00:00:
     * The settle/snooze lifecycle rides the one poll mobile already makes, so
     * the phone can render the same settled/snoozed shelves as the desktop
     * inbox without a second round trip. Absent keys mean "no lifecycle state".
     */
    insert_js(&mut map, "settledAt", &[s("settledAt")]);
    insert_js(&mut map, "settledOverride", &[s("settledOverride")]);
    insert_js(&mut map, "snoozedAt", &[s("snoozedAt")]);
    insert_js(&mut map, "snoozedUntil", &[s("snoozedUntil")]);
    /*
     * CDXC:SidebarV2GitStatus 2026-07-29-00:00:
     * The card row's git/PR state rides the same poll mobile already makes, so
     * the phone can render branch, +n −n, and the PR badge without a second
     * round trip or a git binary of its own.
     */
    insert_js(&mut map, "gitStatus", &[s("gitStatus")]);
    /*
     * CDXC:SessionChatPromptQueue 2026-08-21-b:
     * The phone's session-row queue badge reads these two. This compactor is a
     * SECOND whitelist after `to_cli_session`'s: forwarding them there only is
     * not enough, because everything not named here is dropped again before the
     * summary reaches the phone. `queuedPromptCount` includes `failed` rows and
     * `queuedPromptFailedCount` turns the badge red; both absent means no badge.
     */
    insert_js(&mut map, "queuedPromptCount", &[s("queuedPromptCount")]);
    insert_js(
        &mut map,
        "queuedPromptFailedCount",
        &[s("queuedPromptFailedCount")],
    );
    /*
     * CDXC:SessionAgentNotes 2026-08-24:
     * Same SECOND-whitelist trap as the queue counts above: forwarding these in
     * `to_cli_session` only is not enough, because everything not named here is
     * dropped again before the summary reaches the phone. `sessionNote` draws
     * the row's note dot and its note text; `agentSessionId` gates the
     * "Session note" long-press item, because a session with no provider
     * conversation has nothing to attach a note to.
     */
    insert_js(&mut map, "sessionNote", &[s("sessionNote")]);
    insert_js(&mut map, "agentSessionId", &[s("agentSessionId")]);
    insert_js(&mut map, "sortOrder", &[s("sortOrder")]);
    insert_js(&mut map, "status", &[s("status")]);
    insert_js(&mut map, "surface", &[s("surface")]);
    insert_js(&mut map, "title", &[s("title")]);
    Value::Object(map)
}

// ---------------------------------------------------------------------------
// human session list printing (private ports of printSessionList helpers)
// ---------------------------------------------------------------------------

struct SessionProjectGroup<'a> {
    project_name: String,
    project_path: String,
    sessions: Vec<&'a Value>,
}

fn print_session_list(sessions: &[Value], grouped: bool) {
    if sessions.is_empty() {
        println!("No running terminal sessions.");
        return;
    }
    /*
     * CDXC:CliSessions 2026-05-20-12:20:
     * Group by project with the project path as the section header, print each
     * session as a short two-line block without field labels, and preserve the
     * sidebar order returned by the native inventory.
     */
    let project_groups = group_sessions_preserving_sidebar_order(sessions);
    if !grouped {
        for project in &project_groups {
            for session in &project.sessions {
                println!(
                    "{}",
                    format_compact_session_line(session, Some(&project.project_name))
                );
            }
        }
        return;
    }
    for (project_index, project) in project_groups.iter().enumerate() {
        if project_index > 0 {
            println!();
        }
        println!("{}", project.project_name);
        if !project.project_path.is_empty() {
            println!("{}", project.project_path);
        }
        for session in &project.sessions {
            println!("{}", format_compact_session_line(session, None));
        }
    }
}

fn group_sessions_preserving_sidebar_order(sessions: &[Value]) -> Vec<SessionProjectGroup<'_>> {
    let mut groups: Vec<SessionProjectGroup> = Vec::new();
    let mut group_index_by_project: HashMap<Option<String>, usize> = HashMap::new();
    for session in sessions {
        let key = value_key(session.get("projectId"));
        let group_index = match group_index_by_project.get(&key) {
            Some(index) => *index,
            None => {
                let is_first_group = groups.is_empty();
                groups.push(SessionProjectGroup {
                    project_name: resolve_session_picker_project_name(session, is_first_group),
                    project_path: match session.get("projectPath") {
                        Some(value) if js_truthy(Some(value)) => js_display(value),
                        _ => String::new(),
                    },
                    sessions: Vec::new(),
                });
                group_index_by_project.insert(key, groups.len() - 1);
                groups.len() - 1
            }
        };
        groups[group_index].sessions.push(session);
    }
    groups
}

fn resolve_session_picker_project_name(session: &Value, is_first_group: bool) -> String {
    if is_first_group && js_string(session.get("projectPath")).trim().is_empty() {
        return QUICK_TERMINALS_PROJECT_NAME.to_string();
    }
    if js_truthy(session.get("projectName")) {
        return js_display(session.get("projectName").expect("truthy value"));
    }
    if js_truthy(session.get("projectPath")) {
        return js_display(session.get("projectPath").expect("truthy value"));
    }
    QUICK_TERMINALS_PROJECT_NAME.to_string()
}

fn format_compact_session_line(session: &Value, project_label: Option<&str>) -> String {
    let marker = if js_truthy(session.get("isFocused")) {
        "›"
    } else {
        " "
    };
    let title = match js_coalesce(&[session.get("displayTitle")])
        .filter(|value| js_truthy(Some(*value)))
        .or_else(|| session.get("title").filter(|value| js_truthy(Some(*value))))
    {
        Some(value) => js_display(value),
        None => "-".to_string(),
    };
    let alias = js_template(session.get("alias"));
    let headline = match project_label {
        Some(label) => format!("{marker} #{alias}  {label} · {title}"),
        None => format!("{marker} #{alias}  {title}"),
    };
    let details: Vec<String> = [
        js_string(session.get("agent")),
        format_compact_provider(session).unwrap_or_default(),
        js_string(session.get("status")),
        format_active_time(session.get("lastInteractionAt")),
    ]
    .iter()
    .map(|value| value.trim().to_string())
    .filter(|value| !value.is_empty() && value != "-")
    .collect();
    if details.is_empty() {
        return headline;
    }
    format!("{headline}\n    {}", details.join(" · "))
}

fn format_compact_provider(session: &Value) -> Option<String> {
    let provider = session
        .get("provider")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if provider.is_empty() {
        return None;
    }
    let provider_session_name = session
        .get("providerSessionName")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if !provider_session_name.is_empty() {
        return Some(format!("{provider}/{provider_session_name}"));
    }
    Some(provider.to_string())
}

fn format_active_time(value: Option<&Value>) -> String {
    let Some(timestamp_ms) = parse_js_date_ms(&js_string(value)) else {
        return "-".to_string();
    };
    let now_ms = chrono::Utc::now().timestamp_millis();
    let seconds = (((now_ms - timestamp_ms) as f64) / 1000.0).round().max(0.0);
    if seconds < 60.0 {
        return format!("{}s ago", seconds as i64);
    }
    let minutes = (seconds / 60.0).round();
    if minutes < 60.0 {
        return format!("{}m ago", minutes as i64);
    }
    let hours = (minutes / 60.0).round();
    if hours < 48.0 {
        return format!("{}h ago", hours as i64);
    }
    let days = (hours / 24.0).round();
    format!("{}d ago", days as i64)
}

/// Date.parse() for the ISO shapes gxserver emits (RFC 3339, date-only, or a
/// naive datetime treated as UTC). Anything else is NaN -> None.
fn parse_js_date_ms(text: &str) -> Option<i64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        return Some(parsed.timestamp_millis());
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        return Some(date.and_hms_opt(0, 0, 0)?.and_utc().timestamp_millis());
    }
    if let Ok(datetime) = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S%.f") {
        return Some(datetime.and_utc().timestamp_millis());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_cli_session_overlays_presentation_and_maps_status() {
        let session = json!({
            "sessionId": "G1",
            "projectId": "P1",
            "lifecycleState": "sleeping",
            "zmxName": "g-1",
            "agentId": "claude",
            "title": "Raw",
            "createdAt": "c1",
            "updatedAt": "2026-01-01T00:00:00Z",
            "runtimeSettings": { "agentSessionId": "  abc  " },
            "providerState": { "lifecycleState": "missing" },
            "isFavorite": true,
        });
        let project = json!({ "projectId": "P1", "name": "Alpha", "path": "/a" });
        let presentation = json!({
            "title": "Pres",
            "displayTitle": "● Pres",
            "activity": "Working",
            "agentIcon": "claude-icon",
            "zmxName": "g-1p",
            "cwd": "/cwd",
        });
        let cli = to_cli_session(&session, Some(&project), 0, Some(&presentation), Some(3));
        assert_eq!(cli["status"], json!("sleep"));
        assert_eq!(cli["isSleeping"], json!(true));
        assert_eq!(cli["isLive"], json!(false));
        assert_eq!(cli["alias"], json!(1));
        assert_eq!(cli["agent"], json!("claude"));
        assert_eq!(cli["agentIcon"], json!("claude-icon"));
        assert_eq!(cli["agentSessionId"], json!("abc"));
        assert_eq!(cli["agentSessionPath"], Value::Null);
        assert_eq!(cli["title"], json!("Pres"));
        assert_eq!(cli["displayTitle"], json!("● Pres"));
        assert_eq!(cli["displayTitleTooltip"], json!("● Pres"));
        assert_eq!(cli["projectPath"], json!("/cwd"));
        assert_eq!(cli["projectName"], json!("Alpha"));
        assert_eq!(cli["provider"], json!("zmx"));
        assert_eq!(cli["providerSessionName"], json!("g-1"));
        assert_eq!(cli["sessionPersistenceName"], json!("g-1"));
        assert_eq!(cli["providerSessionState"], json!("missing"));
        assert_eq!(cli["sortOrder"], json!(3));
        assert_eq!(cli["zmxName"], json!("g-1p"));
        assert_eq!(cli["activity"], json!("working"));
        assert_eq!(cli["ownership"], json!("gxserver"));
        assert_eq!(cli["isFocused"], json!(false));
        assert_eq!(cli["isLocalOnly"], json!(false));
        assert_eq!(cli["isFavorite"], json!(true));
        assert_eq!(cli["lastInteractionAt"], json!("2026-01-01T00:00:00Z"));
        // undefined presentation-only fields are dropped like JSON.stringify does
        assert!(cli.get("actions").is_none());
        assert!(cli.get("attention").is_none());
        assert!(cli.get("groupId").is_none());
    }

    #[test]
    fn to_cli_session_status_fallbacks_and_empty_project_path() {
        let session = json!({ "sessionId": "G2", "projectId": "P1" });
        let cli = to_cli_session(&session, None, 4, None, None);
        assert_eq!(cli["status"], json!("unknown"));
        assert_eq!(cli["alias"], json!(5));
        assert_eq!(cli["projectPath"], json!(""));
        assert_eq!(cli["projectName"], json!("P1"));
        assert_eq!(cli["activity"], json!("idle"));
        assert!(cli.get("sortOrder").is_none());
        assert!(cli.get("providerSessionState").is_none());
        let running = json!({ "sessionId": "G3", "projectId": "P1", "lifecycleState": "running" });
        let cli = to_cli_session(&running, None, 0, None, None);
        assert_eq!(cli["status"], json!("running"));
        assert_eq!(cli["isLive"], json!(true));
        let provider_only = json!({
            "sessionId": "G4",
            "projectId": "P1",
            "providerState": { "lifecycleState": "exists" },
        });
        let cli = to_cli_session(&provider_only, None, 0, None, None);
        assert_eq!(cli["status"], json!("exists"));
        assert_eq!(cli["isLive"], json!(true));
    }

    #[test]
    fn normalize_cli_session_activity_matches_node() {
        assert_eq!(
            normalize_cli_session_activity(Some(&json!("Attention"))),
            "attention"
        );
        assert_eq!(
            normalize_cli_session_activity(Some(&json!("needs_attention"))),
            "attention"
        );
        assert_eq!(
            normalize_cli_session_activity(Some(&json!(" attention required "))),
            "attention"
        );
        assert_eq!(
            normalize_cli_session_activity(Some(&json!("BUSY"))),
            "working"
        );
        assert_eq!(
            normalize_cli_session_activity(Some(&json!("processing"))),
            "working"
        );
        assert_eq!(
            normalize_cli_session_activity(Some(&json!("something"))),
            "idle"
        );
        assert_eq!(normalize_cli_session_activity(None), "idle");
    }

    #[test]
    fn string_flag_matches_node() {
        assert_eq!(string_flag(None), Value::Null);
        assert_eq!(string_flag(Some(&Value::Null)), Value::Null);
        assert_eq!(string_flag(Some(&json!("  x "))), json!("x"));
        assert_eq!(string_flag(Some(&json!("   "))), Value::Null);
        assert_eq!(string_flag(Some(&json!(5))), json!("5"));
        assert_eq!(string_flag(Some(&json!(true))), json!("true"));
    }

    #[test]
    fn configured_mobile_quick_action_rules() {
        assert!(is_configured_mobile_quick_action(&json!({
            "commandId": "c1", "actionType": "terminal", "command": "make test",
        })));
        assert!(is_configured_mobile_quick_action(&json!({
            "commandId": "c1", "command": "ls",
        })));
        assert!(is_configured_mobile_quick_action(&json!({
            "commandId": "c1", "actionType": "browser", "url": "https://x",
        })));
        assert!(!is_configured_mobile_quick_action(&json!({
            "commandId": "", "command": "ls",
        })));
        assert!(!is_configured_mobile_quick_action(&json!({
            "commandId": "c1", "actionType": "browser", "url": "   ",
        })));
        assert!(!is_configured_mobile_quick_action(&json!({
            "commandId": "c1", "actionType": "terminal", "command": "  ",
        })));
        assert!(!is_configured_mobile_quick_action(
            &json!({ "command": "ls" })
        ));
    }

    #[test]
    fn mobile_session_list_shape_and_ordering() {
        let result = json!({
            "ok": true,
            "product": "gxserver",
            "revision": "r1",
            "projects": [
                { "projectId": "P1", "name": "Alpha", "path": "/a" },
                { "projectId": "P2", "name": "Hidden", "path": "/h", "visibility": "hidden" },
            ],
            "sessions": [
                {
                    "sessionId": "G2", "projectId": "P1", "title": "Two", "alias": 2,
                    "sortOrder": 5, "isFocused": false, "isLive": true, "isSleeping": false,
                    "status": "running", "provider": "zmx", "providerSessionName": "g-2",
                    "activity": "idle", "projectName": "Alpha", "projectPath": "/a",
                    "agent": null, "agentId": "claude",
                },
                { "sessionId": "G3", "projectId": "P2", "title": "Ghost" },
                {
                    "sessionId": "G1", "projectId": "P1", "title": "One", "alias": 1,
                    "sortOrder": 1, "status": "sleep", "isLive": false, "isSleeping": true,
                    "isFocused": false, "provider": "zmx", "updatedAt": "u1",
                },
            ],
            "workspaceGroups": {
                "projectOrder": ["P1", ""],
                "projects": {
                    "P1": { "groups": [ { "groupId": "g1", "sessionIds": ["G1", ""], "title": "" } ] },
                    "P2": { "groups": [ { "groupId": "", "sessionIds": [] } ] },
                },
            },
        });
        let mobile = to_mobile_session_list(&result);
        assert_eq!(
            mobile,
            json!({
                "ok": true,
                "product": "gxserver",
                "revision": "r1",
                "projects": [ { "isChat": false, "name": "Alpha", "path": "/a", "projectId": "P1" } ],
                "recentProjects": [],
                "sessions": [
                    {
                        "sessionId": "G1", "projectId": "P1", "title": "One",
                        "displayTitle": "One", "alias": 1, "sortOrder": 1, "status": "sleep",
                        "isLive": false, "isSleeping": true, "isFocused": false,
                        "provider": "zmx", "lastInteractionAt": "u1",
                    },
                    {
                        "sessionId": "G2", "projectId": "P1", "title": "Two",
                        "displayTitle": "Two", "alias": 2, "sortOrder": 5, "status": "running",
                        "isLive": true, "isSleeping": false, "isFocused": false,
                        "provider": "zmx", "providerSessionName": "g-2", "activity": "idle",
                        "projectName": "Alpha", "projectPath": "/a", "agent": "claude",
                    },
                ],
                "workspaceGroups": {
                    "projectOrder": ["P1"],
                    "projects": {
                        "P1": { "groups": [ { "groupId": "g1", "sessionIds": ["G1"], "title": "g1" } ] },
                    },
                },
            })
        );
    }

    #[test]
    fn mobile_workspace_groups_empty_becomes_undefined() {
        assert_eq!(to_mobile_workspace_groups(None), None);
        assert_eq!(to_mobile_workspace_groups(Some(&Value::Null)), None);
        assert_eq!(to_mobile_workspace_groups(Some(&json!("x"))), None);
        assert_eq!(
            to_mobile_workspace_groups(Some(&json!({ "projectOrder": [], "projects": {} }))),
            None
        );
    }

    #[test]
    fn mobile_sidebar_project_collections_shape() {
        let mobile = to_mobile_sidebar_project_collections(Some(&json!({
            "collections": {
                "c1": {
                    "collapsed": true,
                    "collectionId": "c1",
                    "color": "#7c6df2",
                    "projectIds": ["P1", ""],
                    "title": "Group 1",
                },
                "c2": { "projectIds": ["P2"] },
                "c3": { "projectIds": [] },
            },
            "nextCollectionNumber": 7,
            "order": ["c2", "ghost", "c1"],
        })));
        assert_eq!(
            mobile,
            Some(json!({
                "collections": {
                    "c1": {
                        "collectionId": "c1",
                        "color": "#7c6df2",
                        "projectIds": ["P1"],
                        "title": "Group 1",
                    },
                    "c2": {
                        "collectionId": "c2",
                        "color": "transparent",
                        "projectIds": ["P2"],
                        "title": "c2",
                    },
                },
                "nextCollectionNumber": 7,
                "order": ["c2", "c1"],
            }))
        );
    }

    #[test]
    fn mobile_sidebar_project_collections_empty_becomes_undefined() {
        assert_eq!(to_mobile_sidebar_project_collections(None), None);
        assert_eq!(
            to_mobile_sidebar_project_collections(Some(&Value::Null)),
            None
        );
        assert_eq!(
            to_mobile_sidebar_project_collections(Some(&json!("x"))),
            None
        );
        assert_eq!(
            to_mobile_sidebar_project_collections(Some(&json!({
                "collections": {},
                "nextCollectionNumber": 1,
                "order": [],
            }))),
            None
        );
    }

    #[test]
    fn persisted_inventory_sql_row_mapping() {
        let connection = rusqlite::Connection::open_in_memory().expect("open");
        connection
            .execute_batch(
                "CREATE TABLE projects (projectId TEXT, name TEXT, path TEXT, isRecentProject INTEGER, visibility TEXT, systemKind TEXT);
                 CREATE TABLE sessions (projectId TEXT, sessionId TEXT, kind TEXT, title TEXT, lifecycleState TEXT, providerStateJson TEXT, zmxName TEXT, cwd TEXT, agentId TEXT, updatedAt TEXT, lastActiveAt TEXT);
                 INSERT INTO projects VALUES ('P1', 'Alpha', '/a', 0, 'visible', NULL);
                 INSERT INTO projects VALUES ('P2', 'Hidden', '/h', 0, 'hidden', NULL);
                 INSERT INTO projects VALUES ('P3', 'Recent', '/r', 1, 'visible', NULL);
                 INSERT INTO sessions VALUES ('P1', 'G1', 'terminal', 'One', 'sleeping', '{\"lifecycleState\":\"missing\",\"zmxName\":\"g-1\"}', 'g-1', '/a/w', 'claude', '2026-01-02T00:00:00Z', '2026-01-01T00:00:00Z');
                 INSERT INTO sessions VALUES ('P1', 'G2', 'terminal', 'Stopped', 'stopped', NULL, NULL, NULL, NULL, '2026-01-01T00:00:00Z', NULL);
                 INSERT INTO sessions VALUES ('P2', 'G3', 'terminal', 'Ghost', 'running', NULL, NULL, NULL, NULL, '2026-01-01T00:00:00Z', NULL);",
            )
            .expect("seed");
        let rows = read_persisted_gxserver_inventory_rows_from(&connection, true).expect("rows");
        assert_eq!(
            rows.iter()
                .filter(|row| row["rowType"] == json!("project"))
                .count(),
            3
        );
        let list =
            build_persisted_gxserver_session_list(&rows, Some("S1"), "boom", &Flags::default());
        assert_eq!(list["error"], json!("boom"));
        assert_eq!(list["fallback"], json!("persisted-gxserver-state"));
        assert_eq!(list["ok"], json!(true));
        let projects = list["projects"].as_array().expect("projects");
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0]["projectId"], json!("P1"));
        assert_eq!(projects[0]["visibility"], json!("visible"));
        assert_eq!(projects[0]["isRecentProject"], json!(false));
        let sessions = list["sessions"].as_array().expect("sessions");
        assert_eq!(sessions.len(), 1); // stopped + hidden-project rows filtered
        assert_eq!(sessions[0]["sessionId"], json!("G1"));
        assert_eq!(sessions[0]["globalRef"], json!("S1:P1:G1"));
        assert_eq!(sessions[0]["status"], json!("sleep"));
        assert_eq!(sessions[0]["providerSessionState"], json!("missing"));
        assert_eq!(sessions[0]["providerSessionName"], json!("g-1"));
        assert_eq!(sessions[0]["projectPath"], json!("/a/w"));

        // --all keeps the stopped row
        let mut all_flags = Flags::default();
        all_flags.insert_bool("all", true);
        let list = build_persisted_gxserver_session_list(&rows, None, "boom", &all_flags);
        assert_eq!(list["sessions"].as_array().expect("sessions").len(), 2);
        assert!(list["sessions"][0].get("globalRef").is_none());
    }

    #[test]
    fn persisted_inventory_visibility_column_fallback() {
        let connection = rusqlite::Connection::open_in_memory().expect("open");
        connection
            .execute_batch(
                "CREATE TABLE projects (projectId TEXT, name TEXT, path TEXT, isRecentProject INTEGER);
                 CREATE TABLE sessions (projectId TEXT, sessionId TEXT, kind TEXT, title TEXT, lifecycleState TEXT, providerStateJson TEXT, zmxName TEXT, cwd TEXT, agentId TEXT, updatedAt TEXT, lastActiveAt TEXT);
                 INSERT INTO projects VALUES ('P1', 'Alpha', '/a', 0);",
            )
            .expect("seed");
        assert!(read_persisted_gxserver_inventory_rows_from(&connection, true).is_none());
        let rows = read_persisted_gxserver_inventory_rows_from(&connection, false).expect("rows");
        let project = rows
            .iter()
            .find(|row| row["rowType"] == json!("project"))
            .expect("project row");
        assert_eq!(project["visibility"], json!("visible"));
        assert_eq!(project["systemKind"], Value::Null);
    }

    #[test]
    fn attach_metadata_apply_and_guards() {
        let session = json!({
            "sessionId": "G1", "projectId": "P1", "title": "One", "status": "sleep",
            "projectPath": "/old", "provider": "zmx", "providerSessionName": "g-old",
        });
        // no attach metadata -> unchanged
        let unchanged = apply_attach_metadata_to_cli_session(&session, None).expect("ok");
        assert_eq!(unchanged, session);
        // restoreBlocked -> error with cwd suffix
        let blocked = json!({ "restoreBlocked": { "cwd": "/gone" } });
        let error =
            apply_attach_metadata_to_cli_session(&session, Some(&blocked)).expect_err("err");
        assert_eq!(
            error.to_string(),
            "Session One cannot be restored because its cwd is missing (/gone)."
        );
        // exists provider -> running
        let attach = json!({
            "attachCommand": "zmx attach g-new",
            "cwd": "/new",
            "provider": "zmx",
            "zmxName": "g-new",
            "providerState": { "lifecycleState": "exists" },
        });
        let applied = apply_attach_metadata_to_cli_session(&session, Some(&attach)).expect("ok");
        assert_eq!(applied["attachCommand"], json!("zmx attach g-new"));
        assert_eq!(applied["projectPath"], json!("/new"));
        assert_eq!(applied["providerSessionName"], json!("g-new"));
        assert_eq!(applied["status"], json!("running"));
        assert!(applied.get("resumeCommand").is_none());
        // resume command -> sleep
        let attach = json!({ "startupText": "claude --resume abc\r\r" });
        let applied = apply_attach_metadata_to_cli_session(&session, Some(&attach)).expect("ok");
        assert_eq!(applied["resumeCommand"], json!("claude --resume abc"));
        assert_eq!(applied["status"], json!("sleep"));
        assert!(applied.get("attachCommand").is_none());
    }

    #[test]
    fn should_start_missing_provider_rules() {
        assert!(should_start_missing_provider_for_cli_attach(Some(&json!({
            "provider": "zmx", "providerState": { "lifecycleState": "missing" },
        }))));
        assert!(!should_start_missing_provider_for_cli_attach(None));
        assert!(!should_start_missing_provider_for_cli_attach(Some(
            &Value::Null
        )));
        assert!(!should_start_missing_provider_for_cli_attach(Some(
            &json!({
                "provider": "zmx",
                "providerState": { "lifecycleState": "missing" },
                "restoreBlocked": { "cwd": "/gone" },
            })
        )));
        assert!(!should_start_missing_provider_for_cli_attach(Some(
            &json!({
                "provider": "tmux", "providerState": { "lifecycleState": "missing" },
            })
        )));
        assert!(!should_start_missing_provider_for_cli_attach(Some(
            &json!({
                "provider": "zmx", "providerState": { "lifecycleState": "exists" },
            })
        )));
    }

    #[test]
    fn normalize_startup_text_matches_node() {
        assert_eq!(normalize_startup_text_for_shell(None), None);
        assert_eq!(normalize_startup_text_for_shell(Some(&json!("  "))), None);
        assert_eq!(
            normalize_startup_text_for_shell(Some(&json!("run me\r\r"))),
            Some("run me".to_string())
        );
        assert_eq!(
            normalize_startup_text_for_shell(Some(&json!(" x "))),
            Some("x".to_string())
        );
    }

    #[test]
    fn compact_session_line_formatting() {
        let session = json!({
            "alias": 2, "isFocused": true, "displayTitle": "Fix bug", "title": "raw",
            "agent": "claude", "provider": "zmx", "providerSessionName": "g-2",
            "status": "running",
        });
        assert_eq!(
            format_compact_session_line(&session, None),
            "› #2  Fix bug\n    claude · zmx/g-2 · running"
        );
        assert_eq!(
            format_compact_session_line(&session, Some("Alpha")),
            "› #2  Alpha · Fix bug\n    claude · zmx/g-2 · running"
        );
        let bare = json!({ "alias": 1 });
        assert_eq!(format_compact_session_line(&bare, None), "  #1  -");
    }

    #[test]
    fn group_sessions_first_group_quick_terminals() {
        let sessions = vec![
            json!({ "projectId": "P0", "projectPath": "", "title": "scratch" }),
            json!({ "projectId": "P1", "projectName": "Alpha", "projectPath": "/a" }),
            json!({ "projectId": "P0", "projectPath": "" }),
        ];
        let groups = group_sessions_preserving_sidebar_order(&sessions);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].project_name, "Quick Terminals");
        assert_eq!(groups[0].sessions.len(), 2);
        assert_eq!(groups[1].project_name, "Alpha");
        assert_eq!(groups[1].project_path, "/a");
    }

    #[test]
    fn active_project_filter_matches_node() {
        assert!(is_active_gxserver_inventory_project(
            &json!({ "projectId": "P1" })
        ));
        assert!(!is_active_gxserver_inventory_project(
            &json!({ "isRecentProject": true })
        ));
        assert!(!is_active_gxserver_inventory_project(
            &json!({ "visibility": "hidden" })
        ));
        assert!(!is_active_gxserver_inventory_project(
            &json!({ "systemKind": "remoteAttachCarrier" })
        ));
        // JS strict !== true: numeric 1 stays active
        assert!(is_active_gxserver_inventory_project(
            &json!({ "isRecentProject": 1 })
        ));
    }

    #[test]
    fn mobile_chat_project_classification_matches_gpui_contract() {
        assert!(is_mobile_chats_collection_project(&json!({
            "path": "/Users/me/.ghostex-dev/chats/session-a"
        })));
        assert!(is_mobile_chats_collection_project(&json!({
            "launchSettings": { "isQuick": true }
        })));
        assert!(!is_mobile_chats_collection_project(&json!({
            "name": "Chat tools",
            "path": "/Users/me/code/chat-tools"
        })));
    }

    #[test]
    fn mobile_summary_keeps_active_and_recent_project_contracts_separate() {
        let summary = to_mobile_session_list(&json!({
            "ok": true,
            "projects": [
                { "projectId": "P1", "name": "Empty", "path": "/repo/empty" },
                { "projectId": "PC", "name": "Chat", "path": "/Users/me/.ghostex/chats/c1" }
            ],
            "recentProjects": [
                { "projectId": "PR", "title": "Parked", "path": "/repo/parked", "sessionCount": 2 }
            ],
            "sessions": []
        }));
        let projects = summary["projects"].as_array().unwrap();
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0]["projectId"], "P1");
        assert_eq!(projects[0]["isChat"], false);
        assert_eq!(projects[1]["isChat"], true);
        assert_eq!(summary["recentProjects"][0]["projectId"], "PR");
        assert!(summary["sessions"].as_array().unwrap().is_empty());
    }

    #[test]
    fn format_active_time_invalid_is_dash() {
        assert_eq!(format_active_time(None), "-");
        assert_eq!(format_active_time(Some(&json!("not a date"))), "-");
        assert_eq!(format_active_time(Some(&Value::Null)), "-");
    }
}
