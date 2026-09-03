// C1 wave-1 deferred split: apps/desktop/src/app/helpers/board_gxserver.rs
// (~4.3k lines) further divided into responsibility-scoped submodules (pure
// move, no logic changes). This file holds gxserver domain/recent project listing helpers, small JSON
// field-extraction utilities, and the previous-sessions listing/search-result
// conversion helpers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::time::Duration;

use crate::app::helpers::*;
use crate::*;

#[allow(dead_code)] // no caller: gxserver project settings are consumed through the sidebar runtime bridge instead
pub(crate) fn gpui_project_settings_projects_from_gxserver() -> Vec<serde_json::Value> {
    /*
    CDXC:Projects 2026-06-24-11:59:
    Settings project rows in GPUI must come from real local gxserver project domain data, falling back only to the presentation snapshot when `/api/listProjects` is unavailable or lacks usable rows. Do not synthesize paths, names, worktree metadata, or Beads settings from UI labels, session titles, terminal cwd, or local filesystem guesses.
    */
    let domain_projects = gpui_gxserver_domain_projects(Duration::from_secs(2));
    gpui_project_settings_projects_from_domain_projects_or_presentation(&domain_projects)
}

pub(crate) fn gpui_gxserver_domain_projects(timeout: Duration) -> Vec<serde_json::Value> {
    gpui_gxserver_domain_projects_result(timeout).unwrap_or_default()
}

pub(crate) fn gpui_gxserver_domain_projects_result(
    timeout: Duration,
) -> Result<Vec<serde_json::Value>, String> {
    let result = gpui_gxserver_rpc_result("/api/listProjects", &serde_json::json!({}), timeout)?;
    result
        .get("projects")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .ok_or_else(|| GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string())
}

pub(crate) fn gpui_gxserver_recent_projects(timeout: Duration) -> Vec<serde_json::Value> {
    gpui_gxserver_rpc_result("/api/listRecentProjects", &serde_json::json!({}), timeout)
        .ok()
        .and_then(|result| {
            result
                .get("recentProjects")
                .and_then(serde_json::Value::as_array)
                .map(|projects| {
                    projects
                        .iter()
                        .filter_map(gpui_recent_project_from_gxserver)
                        .collect::<Vec<_>>()
                })
        })
        .unwrap_or_default()
}

pub(crate) fn gpui_recent_project_from_gxserver(
    project: &serde_json::Value,
) -> Option<serde_json::Value> {
    let project = project.as_object()?;
    let project_id = gpui_trimmed_json_string_field(project, "projectId")?;
    let title = gpui_trimmed_json_string_field(project, "title")?;
    let path = gpui_trimmed_json_string_field(project, "path")?;
    let session_count = project
        .get("sessionCount")
        .and_then(json_u64_value)
        .unwrap_or(0);

    let mut item = serde_json::Map::new();
    item.insert(
        "path".to_string(),
        serde_json::Value::String(path.to_string()),
    );
    item.insert(
        "projectId".to_string(),
        serde_json::Value::String(project_id.to_string()),
    );
    item.insert(
        "sessionCount".to_string(),
        serde_json::Value::Number(serde_json::Number::from(session_count)),
    );
    item.insert(
        "title".to_string(),
        serde_json::Value::String(title.to_string()),
    );
    gpui_insert_optional_nonempty_string(
        &mut item,
        "recentClosedAt",
        gpui_trimmed_json_string_field(project, "recentClosedAt"),
    );
    gpui_insert_optional_nonempty_string(
        &mut item,
        "iconDataUrl",
        gpui_trimmed_json_string_field(project, "iconDataUrl"),
    );
    gpui_insert_optional_nonempty_string(
        &mut item,
        "theme",
        gpui_trimmed_json_string_field(project, "theme"),
    );
    gpui_insert_optional_nonempty_string(
        &mut item,
        "themeColor",
        gpui_trimmed_json_string_field(project, "themeColor"),
    );
    if let Some(icon) = project.get("icon").filter(|value| value.is_object()) {
        item.insert("icon".to_string(), icon.clone());
    }
    Some(serde_json::Value::Object(item))
}

#[derive(Clone)]
pub(crate) struct GpuiRecentProjectsRequest {
    pub(crate) machine_id: Option<String>,
    pub(crate) machine_name: Option<String>,
    pub(crate) remote_target: Option<GpuiRemoteGxserverRequestTarget>,
}

pub(crate) fn gpui_find_gxserver_project_by_id(
    project_id: &str,
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let result = gpui_gxserver_rpc_result(
        "/api/listProjects",
        &serde_json::json!({}),
        Duration::from_secs(10),
    )?;
    let projects = result
        .get("projects")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "gxserver returned invalid project metadata.".to_string())?;
    gpui_normal_gxserver_project_row_by_id(projects, project_id)
        .cloned()
        .ok_or_else(|| "gxserver project metadata was not found.".to_string())
}

pub(crate) fn gpui_normal_gxserver_project_row_by_id<'a>(
    projects: &'a [serde_json::Value],
    project_id: &str,
) -> Option<&'a serde_json::Map<String, serde_json::Value>> {
    /*
    CDXC:Projects 2026-06-25-21:36:
    Project Settings metadata writes must reject stale or direct ids that resolve only to explicit parked `/api/listProjects` rows. Skip only boolean `isRecentProject: true`; false, missing, and non-boolean flags remain normal Settings metadata targets, while Recent Project actions keep using `/api/listRecentProjects`.
    */
    let project_id = gpui_trimmed_nonempty_str(Some(project_id))?;
    projects
        .iter()
        .filter_map(serde_json::Value::as_object)
        .find(|project| {
            !gpui_gxserver_project_row_is_explicit_recent_project(project)
                && gpui_trimmed_json_string_field(project, "projectId") == Some(project_id)
        })
}

pub(crate) fn gpui_clone_json_object_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> serde_json::Map<String, serde_json::Value> {
    object
        .get(key)
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default()
}

pub(crate) fn gpui_settings_metadata_string_or_null(value: &str) -> serde_json::Value {
    match gpui_trimmed_nonempty_str(Some(value)) {
        Some(value) => serde_json::Value::String(value.to_string()),
        None => serde_json::Value::Null,
    }
}

pub(crate) fn gpui_settings_beads_display_key_or_null(value: &str) -> serde_json::Value {
    let display_key = value
        .trim()
        .chars()
        .flat_map(|ch| ch.to_uppercase())
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(3)
        .collect::<String>();
    if display_key.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::String(display_key)
    }
}

pub(crate) fn gpui_insert_optional_nonempty_string(
    item: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<&str>,
) {
    if let Some(value) = gpui_trimmed_nonempty_str(value) {
        item.insert(
            key.to_string(),
            serde_json::Value::String(value.to_string()),
        );
    }
}

pub(crate) fn gpui_trimmed_json_string_field<'a>(
    object: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<&'a str> {
    gpui_trimmed_nonempty_str(json_string_field(object, key))
}

pub(crate) fn gpui_trimmed_nonempty_str(value: Option<&str>) -> Option<&str> {
    let value = value?.trim();
    (!value.is_empty()).then_some(value)
}

#[derive(Clone, Debug)]
pub(crate) struct GpuiPreviousSessionsRequest {
    pub(crate) cursor: Option<String>,
    pub(crate) limit: usize,
    pub(crate) query: Option<String>,
    pub(crate) request_id: String,
    pub(crate) session_tags: Option<Vec<String>>,
}

#[derive(Default)]
pub(crate) struct GpuiPreviousSessionsPage {
    pub(crate) cursor: Option<String>,
    pub(crate) items: Vec<serde_json::Value>,
}

pub(crate) fn gpui_list_previous_sessions_from_gxserver(
    request: &GpuiPreviousSessionsRequest,
) -> Result<GpuiPreviousSessionsPage, String> {
    let result = gpui_gxserver_rpc_result(
        "/api/listPreviousSessions",
        &gpui_previous_sessions_list_params(request),
        Duration::from_secs(10),
    )?;
    let results = result
        .get("results")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "gxserver returned invalid previous-session results.".to_string())?;
    Ok(GpuiPreviousSessionsPage {
        cursor: result
            .get("cursor")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        items: results
            .iter()
            .filter_map(gpui_gxserver_search_result_to_previous_session_item)
            .collect(),
    })
}

pub(crate) fn gpui_gxserver_search_result_to_previous_session_item(
    result: &serde_json::Value,
) -> Option<serde_json::Value> {
    gpui_gxserver_search_result_to_previous_session_item_with_options(result, "gxserver", None)
}

pub(crate) fn gpui_gxserver_search_result_to_previous_session_item_with_options(
    result: &serde_json::Value,
    history_id_prefix: &str,
    project_name_prefix: Option<&str>,
) -> Option<serde_json::Value> {
    /*
    CDXC:Sessions 2026-06-24-11:53:
    Previous-session rows returned to React must stay contract-shaped and metadata-only. Mirror the existing GPUI TypeScript projection from gxserver search results, including project/session restore identity and title/provider fields, but do not forward raw command text, stdout/stderr, workspace paths, URLs, tokens, gxserver responses, or archived session records.
    */
    let result = result.as_object()?;
    let title = json_string_field(result, "displayTitle")
        .or_else(|| json_string_field(result, "primaryTitle"))
        .or_else(|| json_string_field(result, "title"))
        .unwrap_or("Previous Session");
    let closed_at = json_string_field(result, "closedAt")
        .or_else(|| json_string_field(result, "updatedAt"))
        .or_else(|| json_string_field(result, "createdAt"))?;
    let project_id = json_string_field(result, "projectId")?;
    let session_id = json_string_field(result, "sessionId")?;
    let project_title = json_string_field(result, "projectTitle")?;
    let agent_name =
        json_string_field(result, "agentName").or_else(|| json_string_field(result, "agentId"));
    let agent_icon = json_string_field(result, "agentIcon").or(agent_name);
    let session_persistence_provider =
        json_string_field(result, "sessionPersistenceProvider").unwrap_or("zmx");
    let session_persistence_name = json_string_field(result, "sessionPersistenceName")
        .or_else(|| json_string_field(result, "zmxName"));

    let mut item = serde_json::Map::new();
    item.insert(
        "activity".to_string(),
        serde_json::Value::String("idle".to_string()),
    );
    if let Some(agent_icon) = gpui_sidebar_agent_icon(agent_icon) {
        item.insert(
            "agentIcon".to_string(),
            serde_json::Value::String(agent_icon.to_string()),
        );
    }
    gpui_insert_optional_string(&mut item, "agentId", json_string_field(result, "agentId"));
    gpui_insert_optional_string(
        &mut item,
        "agentSessionId",
        json_string_field(result, "agentSessionId"),
    );
    item.insert(
        "alias".to_string(),
        serde_json::Value::String(title.to_string()),
    );
    item.insert(
        "closedAt".to_string(),
        serde_json::Value::String(closed_at.to_string()),
    );
    item.insert("column".to_string(), serde_json::Value::Number(0.into()));
    /*
    CDXC:SessionFork 2026-08-28:
    Fork shape is derived by the daemon that owns the registry, so the bridge
    only forwards it. `forkBranchCount` drives the branch badge and, together
    with the family ids, keeps the shared title-based dedupe from merging two
    rows that are genuinely separate branches of one conversation.
    */
    gpui_insert_optional_string(
        &mut item,
        "forkedFromSessionId",
        json_string_field(result, "forkedFromSessionId"),
    );
    if let Some(fork_branch_count) = result
        .get("forkBranchCount")
        .and_then(|value| value.as_u64())
    {
        item.insert(
            "forkBranchCount".to_string(),
            serde_json::Value::Number(fork_branch_count.into()),
        );
    }
    if let Some(fork_family_session_ids) = result
        .get("forkFamilySessionIds")
        .and_then(|value| value.as_array())
    {
        item.insert(
            "forkFamilySessionIds".to_string(),
            serde_json::Value::Array(
                fork_family_session_ids
                    .iter()
                    .filter_map(|value| value.as_str())
                    .map(|value| serde_json::Value::String(value.to_string()))
                    .collect(),
            ),
        );
    }
    gpui_insert_optional_string(
        &mut item,
        "displayTitle",
        json_string_field(result, "displayTitle"),
    );
    gpui_insert_optional_string(
        &mut item,
        "displayTitleTooltip",
        json_string_field(result, "displayTitleTooltip"),
    );
    item.insert(
        "historyId".to_string(),
        serde_json::Value::String(format!("{history_id_prefix}:{project_id}:{session_id}")),
    );
    item.insert(
        "isFavorite".to_string(),
        serde_json::Value::Bool(json_bool_field(result, "isFavorite").unwrap_or(false)),
    );
    item.insert("isFocused".to_string(), serde_json::Value::Bool(false));
    item.insert(
        "isGeneratedName".to_string(),
        serde_json::Value::Bool(false),
    );
    item.insert(
        "isPinned".to_string(),
        serde_json::Value::Bool(json_bool_field(result, "isPinned").unwrap_or(false)),
    );
    item.insert(
        "isPrimaryTitleTerminalTitle".to_string(),
        serde_json::Value::Bool(
            json_bool_field(result, "isPrimaryTitleTerminalTitle").unwrap_or(false),
        ),
    );
    item.insert("isRestorable".to_string(), serde_json::Value::Bool(true));
    item.insert("isRunning".to_string(), serde_json::Value::Bool(false));
    item.insert("isVisible".to_string(), serde_json::Value::Bool(false));
    gpui_insert_optional_string(
        &mut item,
        "lastInteractionAt",
        json_string_field(result, "lastActiveAt"),
    );
    item.insert(
        "lifecycleState".to_string(),
        serde_json::Value::String("done".to_string()),
    );
    item.insert(
        "primaryTitle".to_string(),
        serde_json::Value::String(
            json_string_field(result, "primaryTitle")
                .unwrap_or(title)
                .to_string(),
        ),
    );
    item.insert(
        "projectId".to_string(),
        serde_json::Value::String(project_id.to_string()),
    );
    item.insert(
        "projectName".to_string(),
        serde_json::Value::String(
            project_name_prefix
                .map(str::trim)
                .filter(|prefix| !prefix.is_empty())
                .map(|prefix| format!("{prefix} / {project_title}"))
                .unwrap_or_else(|| project_title.to_string()),
        ),
    );
    item.insert("row".to_string(), serde_json::Value::Number(0.into()));
    item.insert(
        "sessionId".to_string(),
        serde_json::Value::String(session_id.to_string()),
    );
    item.insert(
        "sessionKind".to_string(),
        serde_json::Value::String("terminal".to_string()),
    );
    gpui_insert_optional_string(
        &mut item,
        "sessionPersistenceName",
        session_persistence_name,
    );
    item.insert(
        "sessionPersistenceProvider".to_string(),
        serde_json::Value::String(session_persistence_provider.to_string()),
    );
    gpui_insert_optional_string(
        &mut item,
        "sessionTag",
        json_string_field(result, "sessionTag"),
    );
    if let Some(sidebar_order) = result
        .get("sidebarOrder")
        .and_then(serde_json::Value::as_number)
        .cloned()
    {
        item.insert(
            "sidebarOrder".to_string(),
            serde_json::Value::Number(sidebar_order),
        );
    }
    item.insert(
        "shortcutLabel".to_string(),
        serde_json::Value::String(String::new()),
    );
    gpui_insert_optional_string(
        &mut item,
        "terminalTitle",
        json_string_field(result, "terminalTitle"),
    );
    Some(serde_json::Value::Object(item))
}

pub(crate) fn gpui_insert_optional_string(
    item: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<&str>,
) {
    if let Some(value) = value {
        item.insert(
            key.to_string(),
            serde_json::Value::String(value.to_string()),
        );
    }
}

pub(crate) fn gpui_daemon_info_from_gxserver_health(
    health: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    if health.get("ok").and_then(serde_json::Value::as_bool) != Some(true)
        || health.get("product").and_then(serde_json::Value::as_str) != Some(GPUI_GXSERVER_PRODUCT)
    {
        return Err("gxserver health was invalid.".to_string());
    }
    let pid = health
        .get("pid")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "gxserver health omitted pid.".to_string())?;
    let port = health
        .get("port")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "gxserver health omitted port.".to_string())?;
    let protocol_version = health
        .get("protocolVersion")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "gxserver health omitted protocol version.".to_string())?;
    let started_at = health
        .get("startedAt")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "gxserver health omitted start time.".to_string())?;
    Ok(serde_json::json!({
        "pid": pid,
        "port": port,
        "protocolVersion": protocol_version,
        "startedAt": started_at,
    }))
}

pub(crate) fn gpui_read_gxserver_presentation_snapshot() -> Result<serde_json::Value, String> {
    let result = gpui_gxserver_rpc_result(
        "/api/readPresentationSnapshot",
        &serde_json::json!({}),
        Duration::from_secs(10),
    )?;
    result
        .get("snapshot")
        .and_then(serde_json::Value::as_object)
        .cloned()
        .map(serde_json::Value::Object)
        .ok_or_else(|| "gxserver returned an invalid presentation snapshot.".to_string())
}

/*
CDXC:Resources 2026-07-26:
Project the daemon presentation snapshot into the shared titlebar
`TitlebarResourceGroup` contract so Resources sees every project that owns live
sessions, not only the mounted panes of the active project. Session membership
mirrors the shared sidebar projection (group membership, no hidden or
command-surface sessions, agent sessions are terminal resources) and only ids,
titles, project paths, and lifecycle enums cross the bridge.
*/
pub(crate) fn gpui_daemon_session_status_from_gxserver(
    lifecycle_state: &str,
    provider_state: &str,
) -> &'static str {
    match lifecycle_state {
        "running" if provider_state == "exists" || provider_state == "unknown" => "running",
        "running" => "disconnected",
        "stopped" => "exited",
        "sleeping" | "missing" | "unknown" => "disconnected",
        _ => "disconnected",
    }
}
