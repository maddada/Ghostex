use std::collections::HashMap;

use serde_json::{json, Map, Value};

use crate::ghostex_cli::args::{parse_json_value, Flags};
use crate::ghostex_cli::rpc::{call_gxserver_rpc, gxserver_root, CliResult, GXSERVER_PRODUCT};

use super::*;

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

/// The session list as the running gxserver reports it, with no fallback to its persisted state.
pub fn fetch_live_gxserver_session_list(flags: &Flags) -> CliResult<Value> {
    let projects_response = call_gxserver_rpc("/api/listProjects", &json!({}), flags)?;
    let recent_projects_response = call_gxserver_rpc("/api/listRecentProjects", &json!({}), flags)?;
    /*
     * CDXC:StateSync 2026-09-01:
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
     * CDXC:StateSync 2026-05-31-08:45 / 2026-06-04-03:33:
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
     * CDXC:Icons 2026-08-21:
     * A project's DISCOVERED icon (the favicon its own repository ships) lives
     * only in the daemon's project_icon cache, so the CLI cannot read it: it is
     * a separate process with an empty cache. The presentation snapshot already
     * publishes it per project, so fold that key back onto the inventory rows
     * here and every consumer — mobile, TUI, `--json` — sees the same icon
     * chain the gpui sidebar ranks.
     */
    let discovered_project_icons = presentation_project_field_map(
        snapshot.and_then(|snapshot| snapshot.get("projects")),
        "discoveredIconDataUrl",
    );
    // CDXC:Sidebar 2026-10-30 WHY: a project's git origin is probed by gxserver's background pass (project_git_remote.rs) and kept in its memory, which this separate process does not have, so it is folded in from the snapshot like the discovered icon; the phone's Unified list names projects by repository from it.
    let project_git_remotes = presentation_project_field_map(
        snapshot.and_then(|snapshot| snapshot.get("projects")),
        "gitRemoteOriginUrl",
    );
    // CDXC:WorkMode 2026-10-24 WHY: whether Issues Workflow is on for a project, its Linear and GitHub parts and its primary tracker are worked out from what gxserver's background passes keep in memory (its Linear key, `gh auth status`), which this separate process does not have, so they are folded in from the snapshot; the phone picks its Link to rows from them.
    let project_work_fields: Vec<(&str, HashMap<String, &Value>)> =
        ["workMode", "workLinear", "workGithubOff", "workTracker"]
            .into_iter()
            .map(|field| {
                (
                    field,
                    presentation_project_field_map(
                        snapshot.and_then(|snapshot| snapshot.get("projects")),
                        field,
                    ),
                )
            })
            .collect();
    result.insert(
        "projects".to_string(),
        Value::Array(
            active_projects
                .iter()
                .map(|project| {
                    let mut merged = (*project).clone();
                    let project_id = value_key(project.get("projectId"));
                    let discovered = project_id
                        .as_ref()
                        .and_then(|project_id| discovered_project_icons.get(project_id))
                        .map(|value| (*value).clone());
                    if let Some(map) = merged.as_object_mut() {
                        if let Some(discovered) = discovered {
                            map.insert("discoveredIconDataUrl".to_string(), discovered);
                        }
                        if let Some(origin) = project_id
                            .as_ref()
                            .and_then(|project_id| project_git_remotes.get(project_id))
                        {
                            map.insert("gitRemoteOriginUrl".to_string(), (*origin).clone());
                        }
                        for (field, values) in &project_work_fields {
                            if let Some(value) = project_id
                                .as_ref()
                                .and_then(|project_id| values.get(project_id))
                            {
                                map.insert((*field).to_string(), (*value).clone());
                            }
                        }
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
     * CDXC:Sessions 2026-07-12-00:00:
     * gxserver's presentation snapshot carries the GPUI-authored named session
     * groups and sidebar project order; pass them through for mobile.
     */
    insert_present(
        &mut result,
        "workspaceGroups",
        snapshot.and_then(|snapshot| snapshot.get("workspaceGroups")),
    );
    /*
     * CDXC:Projects 2026-07-18-00:00:
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
     * CDXC:Spaces 2026-08-27:
     * The snapshot also carries the daemon-owned saved sidebar filters so
     * phones render and edit the same Space row as the desktop sidebar.
     *
     * CDXC:Spaces 2026-10-06 WHY:
     * Spaces follow the machine's own switch (`sidebarSpacesEnabled`, which
     * the snapshot publishes beside them), and the phone draws a Space row
     * whenever this list carries Spaces, so a machine with Spaces off sends
     * none. A daemon too old to publish the switch keeps sending them.
     */
    let spaces_enabled = snapshot
        .and_then(|snapshot| snapshot.get("sidebarSpacesEnabled"))
        .and_then(Value::as_bool)
        != Some(false);
    insert_present(
        &mut result,
        "sidebarSpaces",
        snapshot
            .filter(|_| spaces_enabled)
            .and_then(|snapshot| snapshot.get("sidebarSpaces")),
    );
    /*
     * CDXC:Sessions 2026-09-11 WHY:
     * Session rows above carry custom tag ids, and the phone reaches gxserver
     * only through this CLI, so the catalog that names and colors those ids
     * rides the same result instead of needing a second SSH exec.
     */
    insert_present(
        &mut result,
        "customSessionTags",
        snapshot.and_then(|snapshot| snapshot.get("customSessionTags")),
    );
    // CDXC:Workspaces 2026-10-09 WHY: the phone's workspace switcher lists this computer's workspaces and filters projects and Spaces by them, and it reaches gxserver only through this CLI, so the workspaces document rides the same result. Absent on a daemon without workspaces, and while the Workspaces extension is off, which is what hides the phone's switcher and filter.
    insert_present(
        &mut result,
        "sidebarWorkspaces",
        snapshot
            .filter(|_| crate::workspaces::workspaces_feature_enabled())
            .and_then(|snapshot| snapshot.get("sidebarWorkspaces")),
    );
    /*
     * CDXC:StateSync 2026-07-29-00:00:
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

/// One key (`discoveredIconDataUrl`, `workTracker`) per projectId from the
/// daemon's presentation snapshot. Projects that publish no such key (the icon
/// pass has not reached them, work mode is off) are simply absent from the map.
fn presentation_project_field_map<'a>(
    projects: Option<&'a Value>,
    key: &str,
) -> HashMap<String, &'a Value> {
    let mut map = HashMap::new();
    if let Some(list) = projects.and_then(Value::as_array) {
        for project in list {
            let Some(project_id) = value_key(project.get("projectId")) else {
                continue;
            };
            if let Some(value) = project.get(key) {
                map.insert(project_id, value);
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

pub(super) fn is_active_gxserver_inventory_project(project: &Value) -> bool {
    /*
     * CDXC:Projects 2026-06-30-21:23:
     * Filter shared inventory from gxserver domain visibility fields so parked
     * Recent Projects and hidden Remote Attach carrier projects do not reach
     * mobile summaries or full session lists.
     */
    project.get("isRecentProject") != Some(&Value::Bool(true))
        && project.get("visibility").and_then(Value::as_str) != Some("hidden")
        && project.get("systemKind").and_then(Value::as_str) != Some("remoteAttachCarrier")
}

pub(super) fn should_use_local_gxserver_state_fallback(flags: &Flags) -> bool {
    let server = flags
        .text("server")
        .or_else(|| std::env::var("GHOSTEX_GXSERVER_SERVER").ok())
        .unwrap_or_else(|| "local".to_string());
    let server = server.trim();
    let server = if server.is_empty() { "local" } else { server };
    server == "local"
}

pub(super) fn read_persisted_gxserver_server_id() -> Option<String> {
    let text = std::fs::read_to_string(gxserver_root().join("identity.json")).unwrap_or_default();
    let identity = parse_json_value(&text)?;
    identity
        .get("serverId")
        .and_then(Value::as_str)
        .map(str::to_string)
}

pub(super) fn should_include_stopped_gxserver_sessions(flags: &Flags) -> bool {
    strict_true(flags, "all")
        || strict_true(flags, "includeStopped")
        || strict_true(flags, "stopped")
}

pub(super) fn is_stopped_gxserver_session(session: &Value) -> bool {
    js_string(session.get("lifecycleState")) == "stopped"
}
