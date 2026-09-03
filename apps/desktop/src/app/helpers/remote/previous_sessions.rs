// C1 wave-1 deferred split: apps/desktop/src/app/helpers/remote.rs (~8.2k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds recent-project listing and previous
// (closed) remote session sourcing, listing, restore, and delete. See
// docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::time::Duration;

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_recent_projects_from_remote_gxserver(
    target: &GpuiRemoteGxserverRequestTarget,
    machine_id: &str,
    machine_name: Option<&str>,
) -> Vec<serde_json::Value> {
    gpui_remote_gxserver_rpc_result(
        target,
        "/api/listRecentProjects",
        &serde_json::json!({}),
        Duration::from_secs(10),
    )
    .ok()
    .and_then(|result| result.get("recentProjects").cloned())
    .and_then(|projects| projects.as_array().cloned())
    .map(|projects| {
        projects
            .iter()
            .filter_map(|project| {
                gpui_recent_project_from_remote_gxserver(project, machine_id, machine_name)
            })
            .collect()
    })
    .unwrap_or_default()
}

pub(crate) fn gpui_recent_project_from_remote_gxserver(
    project: &serde_json::Value,
    machine_id: &str,
    machine_name: Option<&str>,
) -> Option<serde_json::Value> {
    let mut project = gpui_recent_project_from_gxserver(project)?
        .as_object()?
        .clone();
    let project_id = project.get("projectId")?.as_str()?;
    if !gpui_remote_sidebar_project_id_allowed(project_id) {
        return None;
    }
    project.insert(
        "projectId".to_string(),
        serde_json::json!(format!("remote:{machine_id}:project:{project_id}")),
    );
    project.insert("remoteMachineId".to_string(), serde_json::json!(machine_id));
    if let Some(machine_name) = machine_name {
        project.insert(
            "remoteMachineName".to_string(),
            serde_json::json!(machine_name),
        );
    }
    Some(serde_json::Value::Object(project))
}

pub(crate) struct GpuiRemotePreviousSessionSource {
    pub(crate) machine_name: Option<String>,
    pub(crate) remote_machine_id: String,
    pub(crate) target: GpuiRemoteGxserverRequestTarget,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GpuiRemotePreviousSessionReference {
    pub(crate) remote_machine_id: String,
    pub(crate) project_id: String,
    pub(crate) session_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum GpuiPreviousSessionRestoreResult {
    Local {
        project_id: String,
        session_id: String,
    },
    Remote {
        project_id: String,
        remote_machine_id: String,
        session_id: String,
    },
}

pub(crate) fn gpui_previous_sessions_request_from_command(
    command: &serde_json::Map<String, serde_json::Value>,
) -> GpuiPreviousSessionsRequest {
    let cursor = command
        .get("cursor")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let limit = command
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .map(|limit| limit.min(200).max(1) as usize)
        .unwrap_or(80);
    let query = command
        .get("query")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let request_id = command
        .get("requestId")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    let session_tags = command
        .get("sessionTags")
        .and_then(serde_json::Value::as_array)
        .map(|tags| {
            tags.iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        });
    GpuiPreviousSessionsRequest {
        cursor,
        limit,
        query,
        request_id,
        session_tags,
    }
}

pub(crate) fn gpui_previous_sessions_result_message(
    request: GpuiPreviousSessionsRequest,
    remote_sources: Vec<GpuiRemotePreviousSessionSource>,
) -> serde_json::Value {
    /*
    CDXC:Sessions 2026-06-24-11:53:
    GPUI Previous Sessions loads real local gxserver history through `/api/listPreviousSessions` with the same bounded previous-only params as the TypeScript sidebar runtime. The response is a transient `previousSessionsResult` sidebarState payload so the shared modal clears loading without replacing the stored hydrate snapshot, and transport/token/network/parser failures return an empty contract-shaped result without logging private daemon data.
    */
    let local_page = gpui_list_previous_sessions_from_gxserver(&request).unwrap_or_default();
    let mut next_cursor = local_page.cursor;
    let mut previous_sessions = local_page.items;
    for remote_source in &remote_sources {
        let remote_page = gpui_list_previous_sessions_from_remote_gxserver(&request, remote_source)
            .unwrap_or_default();
        if next_cursor.is_none() {
            next_cursor = remote_page.cursor;
        }
        previous_sessions.extend(remote_page.items);
    }
    gpui_sort_previous_session_items_by_closed_time(&mut previous_sessions);
    gpui_previous_sessions_result_payload(
        &request.request_id,
        request.query.as_deref(),
        next_cursor.as_deref(),
        previous_sessions,
    )
}

#[derive(Clone, Debug)]
pub(crate) struct GpuiSessionTranscriptSizeTarget {
    pub(crate) key: String,
    pub(crate) project_id: String,
    pub(crate) remote_machine_id: Option<String>,
    pub(crate) session_id: String,
}

#[derive(Clone, Debug)]
pub(crate) struct GpuiSessionTranscriptSizesRequest {
    pub(crate) request_id: String,
    pub(crate) targets: Vec<GpuiSessionTranscriptSizeTarget>,
}

pub(crate) fn gpui_session_transcript_sizes_request_from_command(
    command: &serde_json::Map<String, serde_json::Value>,
) -> GpuiSessionTranscriptSizesRequest {
    let request_id = command
        .get("requestId")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    let targets = command
        .get("sessions")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .take(32)
        .filter_map(gpui_session_transcript_size_target_from_value)
        .collect();
    GpuiSessionTranscriptSizesRequest {
        request_id,
        targets,
    }
}

fn gpui_session_transcript_size_target_from_value(
    value: &serde_json::Value,
) -> Option<GpuiSessionTranscriptSizeTarget> {
    let row = value.as_object()?;
    let key = row.get("key")?.as_str()?.trim();
    if key.is_empty()
        || key.chars().count() > GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        || key.chars().any(char::is_control)
    {
        return None;
    }

    if let Some(history_id) = row.get("historyId").and_then(serde_json::Value::as_str) {
        if let Some(reference) = gpui_remote_previous_session_reference_from_history_id(history_id)
        {
            return Some(GpuiSessionTranscriptSizeTarget {
                key: key.to_string(),
                project_id: reference.project_id,
                remote_machine_id: Some(reference.remote_machine_id),
                session_id: reference.session_id,
            });
        }
        if let Some((project_id, session_id)) =
            gpui_previous_session_reference_from_history_id(history_id)
        {
            return Some(GpuiSessionTranscriptSizeTarget {
                key: key.to_string(),
                project_id: project_id.to_string(),
                remote_machine_id: None,
                session_id: session_id.to_string(),
            });
        }
        return None;
    }

    let routing_id = row.get("routingId")?.as_str()?;
    let parts = routing_id.split(':').collect::<Vec<_>>();
    match parts.as_slice() {
        [project_id, session_id]
            if gpui_remote_sidebar_project_id_allowed(project_id)
                && gpui_remote_sidebar_session_id_allowed(session_id) =>
        {
            Some(GpuiSessionTranscriptSizeTarget {
                key: key.to_string(),
                project_id: (*project_id).to_string(),
                remote_machine_id: None,
                session_id: (*session_id).to_string(),
            })
        }
        [remote_machine_id, project_id, session_id]
            if gpui_remote_sidebar_project_id_allowed(project_id)
                && gpui_remote_sidebar_session_id_allowed(session_id) =>
        {
            let remote_machine_id = gpui_normalize_remote_machine_id(remote_machine_id)?;
            Some(GpuiSessionTranscriptSizeTarget {
                key: key.to_string(),
                project_id: (*project_id).to_string(),
                remote_machine_id: Some(remote_machine_id),
                session_id: (*session_id).to_string(),
            })
        }
        _ => None,
    }
}

/*
CDXC:Sessions 2026-08-27:
The modal requests only rows that enter its scroll viewport. Keep that lazy
batch machine-scoped here: local ids go to the local daemon, remote ids go to
their owning daemon, and the modal receives only its opaque row key plus byte
count. Transcript paths never cross the native bridge.
*/
pub(crate) fn gpui_session_transcript_sizes_result_message(
    request: GpuiSessionTranscriptSizesRequest,
    remote_sources: Vec<GpuiRemotePreviousSessionSource>,
) -> serde_json::Value {
    let mut sizes_by_source_and_session = std::collections::HashMap::new();
    let mut source_ids = request
        .targets
        .iter()
        .map(|target| target.remote_machine_id.clone())
        .collect::<Vec<_>>();
    source_ids.sort();
    source_ids.dedup();

    for source_id in source_ids {
        let source_targets = request
            .targets
            .iter()
            .filter(|target| target.remote_machine_id == source_id)
            .collect::<Vec<_>>();
        let params = serde_json::json!({
            "sessions": source_targets
                .iter()
                .map(|target| serde_json::json!({
                    "projectId": target.project_id,
                    "sessionId": target.session_id,
                }))
                .collect::<Vec<_>>(),
        });
        let result = match source_id.as_deref() {
            None => gpui_gxserver_rpc_result(
                "/api/readSessionTranscriptSizes",
                &params,
                Duration::from_secs(10),
            ),
            Some(remote_machine_id) => remote_sources
                .iter()
                .find(|source| source.remote_machine_id == remote_machine_id)
                .ok_or_else(|| "Remote gxserver is not connected.".to_string())
                .and_then(|source| {
                    gpui_remote_gxserver_rpc_result(
                        &source.target,
                        "/api/readSessionTranscriptSizes",
                        &params,
                        Duration::from_secs(10),
                    )
                }),
        };
        let Some(rows) = result.ok().and_then(|result| {
            result
                .get("sessions")
                .and_then(serde_json::Value::as_array)
                .cloned()
        }) else {
            continue;
        };
        for row in rows {
            let Some(project_id) = row.get("projectId").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let Some(session_id) = row.get("sessionId").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let size_bytes = row.get("sizeBytes").and_then(serde_json::Value::as_u64);
            sizes_by_source_and_session.insert(
                (
                    source_id.clone(),
                    project_id.to_string(),
                    session_id.to_string(),
                ),
                size_bytes,
            );
        }
    }

    let sizes = request
        .targets
        .into_iter()
        .map(|target| {
            let size_bytes = sizes_by_source_and_session
                .get(&(
                    target.remote_machine_id,
                    target.project_id,
                    target.session_id,
                ))
                .copied()
                .flatten();
            serde_json::json!({
                "key": target.key,
                "sizeBytes": size_bytes,
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "requestId": request.request_id,
        "sizes": sizes,
        "type": "sessionTranscriptSizesResult",
    })
}

pub(crate) fn gpui_list_previous_sessions_from_remote_gxserver(
    request: &GpuiPreviousSessionsRequest,
    remote_source: &GpuiRemotePreviousSessionSource,
) -> Result<GpuiPreviousSessionsPage, String> {
    let result = gpui_remote_gxserver_rpc_result(
        &remote_source.target,
        "/api/listPreviousSessions",
        &gpui_previous_sessions_list_params(request),
        Duration::from_secs(10),
    )?;
    let results = result
        .get("results")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "Remote gxserver returned invalid previous-session results.".to_string())?;
    let history_id_prefix = format!("remote-gxserver:{}", remote_source.remote_machine_id);
    Ok(GpuiPreviousSessionsPage {
        cursor: result
            .get("cursor")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        items: results
            .iter()
            .filter_map(|result| {
                gpui_gxserver_search_result_to_previous_session_item_with_options(
                    result,
                    history_id_prefix.as_str(),
                    remote_source.machine_name.as_deref(),
                )
            })
            .collect(),
    })
}

pub(crate) fn gpui_previous_sessions_list_params(
    request: &GpuiPreviousSessionsRequest,
) -> serde_json::Value {
    let mut params = serde_json::Map::new();
    params.insert("includeActive".to_string(), serde_json::Value::Bool(false));
    params.insert("includePrevious".to_string(), serde_json::Value::Bool(true));
    params.insert(
        "limit".to_string(),
        serde_json::Value::Number(serde_json::Number::from(request.limit as u64)),
    );
    if let Some(cursor) = request.cursor.as_ref() {
        params.insert(
            "cursor".to_string(),
            serde_json::Value::String(cursor.clone()),
        );
    }
    if let Some(query) = request.query.as_ref() {
        params.insert(
            "query".to_string(),
            serde_json::Value::String(query.clone()),
        );
    }
    if let Some(session_tags) = request.session_tags.as_ref() {
        params.insert(
            "sessionTags".to_string(),
            serde_json::Value::Array(
                session_tags
                    .iter()
                    .map(|tag| serde_json::Value::String(tag.clone()))
                    .collect(),
            ),
        );
    }
    serde_json::Value::Object(params)
}

pub(crate) fn gpui_sort_previous_session_items_by_closed_time(
    previous_sessions: &mut [serde_json::Value],
) {
    previous_sessions.sort_by(|left, right| {
        gpui_previous_session_item_closed_time(right)
            .cmp(gpui_previous_session_item_closed_time(left))
    });
}

pub(crate) fn gpui_previous_session_item_closed_time(item: &serde_json::Value) -> &str {
    item.get("closedAt")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
}

pub(crate) fn gpui_previous_sessions_result_payload(
    request_id: &str,
    query: Option<&str>,
    cursor: Option<&str>,
    previous_sessions: Vec<serde_json::Value>,
) -> serde_json::Value {
    let mut payload = serde_json::Map::new();
    payload.insert(
        "previousSessions".to_string(),
        serde_json::Value::Array(previous_sessions),
    );
    if let Some(query) = query {
        payload.insert(
            "query".to_string(),
            serde_json::Value::String(query.to_string()),
        );
    }
    if let Some(cursor) = cursor {
        payload.insert(
            "cursor".to_string(),
            serde_json::Value::String(cursor.to_string()),
        );
    }
    payload.insert(
        "requestId".to_string(),
        serde_json::Value::String(request_id.to_string()),
    );
    payload.insert(
        "type".to_string(),
        serde_json::Value::String("previousSessionsResult".to_string()),
    );
    serde_json::Value::Object(payload)
}

pub(crate) fn gpui_stashed_prompts_result_message(
    request_id: &str,
    project_id: Option<&str>,
) -> serde_json::Value {
    /*
    CDXC:SavedPrompts 2026-07-29:
    The Prompts modal loads stashed prompt-editor saves through the local
    gxserver `/api/listStashedPrompts` endpoint; a projectId param scopes the
    answer to that project plus its worktree family server-side. The rows are
    forwarded to the modal verbatim as a transient `stashedPromptsResult`
    payload — the prompt bodies are the product here, so Rust must not log,
    store, or reshape them, and transport failures return an empty list.
    */
    let mut params = serde_json::Map::new();
    if let Some(project_id) = project_id {
        params.insert(
            "projectId".to_string(),
            serde_json::Value::String(project_id.to_string()),
        );
    }
    let result = gpui_gxserver_rpc_result(
        "/api/listStashedPrompts",
        &serde_json::Value::Object(params),
        Duration::from_secs(10),
    )
    .ok();
    let prompts = result
        .as_ref()
        .and_then(|result| result.get("prompts").cloned())
        .filter(serde_json::Value::is_array)
        .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
    /*
    CDXC:SavedPrompts 2026-08-23:
    The tag catalogue rides on the same answer as the prompts so the modal's
    pill rail and its row chips paint together.
    */
    let tags = result
        .as_ref()
        .and_then(|result| result.get("tags").cloned())
        .filter(serde_json::Value::is_array)
        .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
    serde_json::json!({
        "prompts": prompts,
        "requestId": request_id,
        "tags": tags,
        "type": "stashedPromptsResult",
    })
}

/*
CDXC:SavedPrompts 2026-08-23:
Tag create/rename and delete both answer with the whole refreshed catalogue,
because a create can resolve onto a tag that already exists and a delete
invalidates assignments the modal is still holding. Tag names are user-authored
text on the same footing as prompt bodies: forward them verbatim, never log.
*/
pub(crate) fn gpui_save_stashed_prompt_tag_result_message(
    request_id: &str,
    name: &str,
    color: Option<&str>,
    tag_id: Option<&str>,
) -> serde_json::Value {
    let mut params = serde_json::Map::new();
    params.insert(
        "name".to_string(),
        serde_json::Value::String(name.to_string()),
    );
    if let Some(color) = color {
        params.insert(
            "color".to_string(),
            serde_json::Value::String(color.to_string()),
        );
    }
    if let Some(tag_id) = tag_id {
        params.insert(
            "tagId".to_string(),
            serde_json::Value::String(tag_id.to_string()),
        );
    }
    gpui_stashed_prompt_tags_result_message(
        request_id,
        "/api/saveStashedPromptTag",
        serde_json::Value::Object(params),
        None,
        "Could not save this tag.",
    )
}

pub(crate) fn gpui_delete_stashed_prompt_tag_result_message(
    request_id: &str,
    tag_id: &str,
) -> serde_json::Value {
    gpui_stashed_prompt_tags_result_message(
        request_id,
        "/api/deleteStashedPromptTag",
        serde_json::json!({ "tagId": tag_id }),
        Some(tag_id),
        "Could not delete this tag.",
    )
}

fn gpui_stashed_prompt_tags_result_message(
    request_id: &str,
    endpoint: &str,
    params: serde_json::Value,
    deleted_tag_id: Option<&str>,
    failure_message: &str,
) -> serde_json::Value {
    let tags = gpui_gxserver_rpc_result(endpoint, &params, Duration::from_secs(10))
        .ok()
        .and_then(|result| result.get("tags").cloned())
        .filter(serde_json::Value::is_array);
    match tags {
        Some(tags) => {
            let mut payload = serde_json::Map::new();
            payload.insert("ok".to_string(), serde_json::Value::Bool(true));
            payload.insert(
                "requestId".to_string(),
                serde_json::Value::String(request_id.to_string()),
            );
            payload.insert("tags".to_string(), tags);
            if let Some(deleted_tag_id) = deleted_tag_id {
                payload.insert(
                    "deletedTagId".to_string(),
                    serde_json::Value::String(deleted_tag_id.to_string()),
                );
            }
            payload.insert(
                "type".to_string(),
                serde_json::Value::String("stashedPromptTagsResult".to_string()),
            );
            serde_json::Value::Object(payload)
        }
        None => serde_json::json!({
            "error": failure_message,
            "ok": false,
            "requestId": request_id,
            "tags": [],
            "type": "stashedPromptTagsResult",
        }),
    }
}

pub(crate) fn gpui_set_stashed_prompt_tags_result_message(
    request_id: &str,
    prompt_id: &str,
    tag_ids: &[String],
) -> serde_json::Value {
    let prompt = gpui_gxserver_rpc_result(
        "/api/setStashedPromptTags",
        &serde_json::json!({ "promptId": prompt_id, "tagIds": tag_ids }),
        Duration::from_secs(10),
    )
    .ok()
    .and_then(|result| result.get("prompt").cloned())
    .filter(serde_json::Value::is_object);
    match prompt {
        Some(prompt) => serde_json::json!({
            "ok": true,
            "prompt": prompt,
            "requestId": request_id,
            "type": "setStashedPromptTagsResult",
        }),
        None => serde_json::json!({
            "error": "Could not update this prompt's tags.",
            "ok": false,
            "requestId": request_id,
            "type": "setStashedPromptTagsResult",
        }),
    }
}

pub(crate) fn gpui_save_stashed_prompt_result_message(
    request_id: &str,
    content: &str,
    prompt_id: Option<&str>,
    project_id: Option<&str>,
    session_id: Option<&str>,
    tag_ids: Option<&[String]>,
) -> serde_json::Value {
    let mut params = serde_json::Map::new();
    params.insert(
        "content".to_string(),
        serde_json::Value::String(content.to_string()),
    );
    if let Some(prompt_id) = prompt_id {
        params.insert(
            "promptId".to_string(),
            serde_json::Value::String(prompt_id.to_string()),
        );
    }
    if let Some(project_id) = project_id {
        params.insert(
            "projectId".to_string(),
            serde_json::Value::String(project_id.to_string()),
        );
    }
    if let Some(session_id) = session_id {
        params.insert(
            "sessionId".to_string(),
            serde_json::Value::String(session_id.to_string()),
        );
    }
    if let Some(tag_ids) = tag_ids {
        params.insert("tagIds".to_string(), serde_json::json!(tag_ids));
    }
    let prompt = gpui_gxserver_rpc_result(
        "/api/saveStashedPrompt",
        &serde_json::Value::Object(params),
        Duration::from_secs(10),
    )
    .ok()
    .and_then(|result| result.get("prompt").cloned())
    .filter(serde_json::Value::is_object);
    match prompt {
        Some(prompt) => serde_json::json!({
            "ok": true,
            "prompt": prompt,
            "requestId": request_id,
            "type": "saveStashedPromptResult",
        }),
        None => serde_json::json!({
            "error": "Could not save this prompt.",
            "ok": false,
            "requestId": request_id,
            "type": "saveStashedPromptResult",
        }),
    }
}

pub(crate) fn gpui_remote_previous_session_reference_from_history_id(
    history_id: &str,
) -> Option<GpuiRemotePreviousSessionReference> {
    let payload = history_id.strip_prefix("remote-gxserver:")?;
    let mut parts = payload.split(':');
    let remote_machine_id = parts.next().and_then(gpui_normalize_remote_machine_id)?;
    let project_id = parts.next()?;
    let session_id = parts.next()?;
    if parts.next().is_some()
        || !gpui_remote_sidebar_project_id_allowed(project_id)
        || !gpui_remote_sidebar_session_id_allowed(session_id)
    {
        return None;
    }
    Some(GpuiRemotePreviousSessionReference {
        remote_machine_id,
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
    })
}

pub(crate) fn gpui_remote_previous_session_source_for_reference<'a>(
    remote_sources: &'a [GpuiRemotePreviousSessionSource],
    reference: &GpuiRemotePreviousSessionReference,
) -> Option<&'a GpuiRemotePreviousSessionSource> {
    remote_sources
        .iter()
        .find(|source| source.remote_machine_id == reference.remote_machine_id)
}

pub(crate) const GPUI_PREVIOUS_SESSION_RESTORE_DEFAULT_TITLE: &str = "Terminal Session";

#[derive(Clone, Debug)]
pub(crate) struct GpuiPreviousSessionRestoreMetadata {
    pub(crate) agent_id: Option<String>,
    pub(crate) agent_session_id: Option<String>,
    pub(crate) title: String,
    pub(crate) session_tag: Option<String>,
    pub(crate) sidebar_order: Option<serde_json::Number>,
    pub(crate) session_persistence_name: Option<String>,
    pub(crate) session_persistence_provider: Option<String>,
}

impl GpuiPreviousSessionRestoreMetadata {
    pub(crate) fn default_title() -> Self {
        Self {
            agent_id: None,
            agent_session_id: None,
            title: GPUI_PREVIOUS_SESSION_RESTORE_DEFAULT_TITLE.to_string(),
            session_tag: None,
            sidebar_order: None,
            session_persistence_name: None,
            session_persistence_provider: None,
        }
    }
}

pub(crate) fn gpui_previous_session_restore_metadata_from_row(
    row: &serde_json::Map<String, serde_json::Value>,
) -> GpuiPreviousSessionRestoreMetadata {
    let title = gpui_trimmed_json_string_field(row, "displayTitle")
        .or_else(|| gpui_trimmed_json_string_field(row, "primaryTitle"))
        .or_else(|| gpui_trimmed_json_string_field(row, "title"))
        .unwrap_or(GPUI_PREVIOUS_SESSION_RESTORE_DEFAULT_TITLE)
        .to_string();
    let agent_id = gpui_trimmed_json_string_field(row, "agentId").map(str::to_string);
    let agent_session_id =
        gpui_trimmed_json_string_field(row, "agentSessionId").map(str::to_string);
    let session_tag = gpui_trimmed_json_string_field(row, "sessionTag").map(str::to_string);
    let sidebar_order = row
        .get("sidebarOrder")
        .and_then(serde_json::Value::as_number)
        .cloned();
    let session_persistence_name =
        gpui_trimmed_json_string_field(row, "sessionPersistenceName").map(str::to_string);
    let session_persistence_provider =
        gpui_trimmed_json_string_field(row, "sessionPersistenceProvider").map(str::to_string);
    GpuiPreviousSessionRestoreMetadata {
        agent_id,
        agent_session_id,
        title,
        session_tag,
        sidebar_order,
        session_persistence_name,
        session_persistence_provider,
    }
}

pub(crate) fn gpui_previous_session_restore_row_matches(
    row: &serde_json::Map<String, serde_json::Value>,
    project_id: &str,
    session_id: &str,
) -> bool {
    gpui_trimmed_json_string_field(row, "projectId") == Some(project_id)
        && gpui_trimmed_json_string_field(row, "sessionId") == Some(session_id)
}

pub(crate) fn gpui_previous_session_restore_row_is_running(
    row: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    json_bool_field(row, "isRunning") == Some(true)
        || json_string_field(row, "lifecycleState") == Some("running")
        || json_string_field(row, "providerSessionState") == Some("running")
}

pub(crate) fn gpui_previous_session_restore_metadata_params(
    project_id: &str,
    session_id: &str,
) -> serde_json::Value {
    serde_json::json!({
        "includeActive": false,
        "includePrevious": true,
        "limit": 20,
        "projectId": project_id,
        "query": session_id,
    })
}

pub(crate) fn gpui_previous_session_restore_metadata_from_result(
    result: &serde_json::Value,
    project_id: &str,
    session_id: &str,
) -> Option<GpuiPreviousSessionRestoreMetadata> {
    let results = result
        .get("results")
        .and_then(serde_json::Value::as_array)?;
    let mut running_match = None;
    for row in results.iter().filter_map(serde_json::Value::as_object) {
        if !gpui_previous_session_restore_row_matches(row, project_id, session_id) {
            continue;
        }
        let metadata = gpui_previous_session_restore_metadata_from_row(row);
        if !gpui_previous_session_restore_row_is_running(row) {
            return Some(metadata);
        }
        if running_match.is_none() {
            running_match = Some(metadata);
        }
    }
    running_match
}

pub(crate) fn gpui_previous_session_restore_metadata(
    project_id: &str,
    session_id: &str,
) -> Option<GpuiPreviousSessionRestoreMetadata> {
    let result = gpui_gxserver_rpc_result(
        "/api/listPreviousSessions",
        &gpui_previous_session_restore_metadata_params(project_id, session_id),
        Duration::from_secs(10),
    )
    .ok()?;
    gpui_previous_session_restore_metadata_from_result(&result, project_id, session_id)
}

pub(crate) fn gpui_remote_previous_session_restore_metadata(
    target: &GpuiRemoteGxserverRequestTarget,
    project_id: &str,
    session_id: &str,
) -> Option<GpuiPreviousSessionRestoreMetadata> {
    let result = gpui_remote_gxserver_rpc_result(
        target,
        "/api/listPreviousSessions",
        &gpui_previous_session_restore_metadata_params(project_id, session_id),
        Duration::from_secs(10),
    )
    .ok()?;
    gpui_previous_session_restore_metadata_from_result(&result, project_id, session_id)
}

pub(crate) fn gpui_previous_session_restore_create_params(
    project_id: &str,
    session_id: &str,
    metadata: GpuiPreviousSessionRestoreMetadata,
) -> serde_json::Value {
    let mut create_params = serde_json::Map::new();
    create_params.insert(
        "kind".to_string(),
        serde_json::Value::String("terminal".to_string()),
    );
    if let Some(agent_id) = metadata.agent_id {
        create_params.insert("agentId".to_string(), serde_json::Value::String(agent_id));
    }
    create_params.insert(
        "lifecycleState".to_string(),
        serde_json::Value::String("running".to_string()),
    );
    create_params.insert(
        "projectId".to_string(),
        serde_json::Value::String(project_id.to_string()),
    );
    create_params.insert(
        "restoredFromSessionId".to_string(),
        serde_json::Value::String(session_id.to_string()),
    );
    create_params.insert(
        "surface".to_string(),
        serde_json::Value::String("workspace".to_string()),
    );
    create_params.insert(
        "title".to_string(),
        serde_json::Value::String(metadata.title),
    );
    let mut runtime_settings = serde_json::Map::new();
    if let Some(agent_session_id) = metadata.agent_session_id {
        runtime_settings.insert(
            "agentSessionId".to_string(),
            serde_json::Value::String(agent_session_id),
        );
    }
    if let Some(session_persistence_name) = metadata.session_persistence_name {
        runtime_settings.insert(
            "sessionPersistenceName".to_string(),
            serde_json::Value::String(session_persistence_name),
        );
    }
    if let Some(session_persistence_provider) = metadata.session_persistence_provider {
        runtime_settings.insert(
            "sessionPersistenceProvider".to_string(),
            serde_json::Value::String(session_persistence_provider),
        );
    }
    if !runtime_settings.is_empty() {
        create_params.insert(
            "runtimeSettings".to_string(),
            serde_json::Value::Object(runtime_settings),
        );
    }
    if let Some(session_tag) = metadata.session_tag {
        create_params.insert(
            "sessionTag".to_string(),
            serde_json::Value::String(session_tag),
        );
    }
    if let Some(sidebar_order) = metadata.sidebar_order {
        create_params.insert(
            "sidebarOrder".to_string(),
            serde_json::Value::Number(sidebar_order),
        );
    }
    serde_json::Value::Object(create_params)
}

pub(crate) fn gpui_delete_previous_session_from_history_id(
    history_id: &str,
    remote_sources: &[GpuiRemotePreviousSessionSource],
) {
    if let Some(reference) = gpui_remote_previous_session_reference_from_history_id(history_id) {
        let Some(remote_source) =
            gpui_remote_previous_session_source_for_reference(remote_sources, &reference)
        else {
            return;
        };
        let _ = gpui_remote_gxserver_rpc_result(
            &remote_source.target,
            "/api/removeSession",
            &serde_json::json!({
                "projectId": reference.project_id.as_str(),
                "reason": "deletePreviousSession",
                "sessionId": reference.session_id.as_str(),
            }),
            Duration::from_secs(10),
        );
        return;
    }

    let Some((project_id, session_id)) =
        gpui_previous_session_reference_from_history_id(history_id)
    else {
        return;
    };
    let _ = gpui_gxserver_rpc_result(
        "/api/removeSession",
        &serde_json::json!({
            "projectId": project_id,
            "reason": "deletePreviousSession",
            "sessionId": session_id,
        }),
        Duration::from_secs(10),
    );
}

pub(crate) fn gpui_restore_previous_session_from_history_id(
    history_id: &str,
    remote_sources: &[GpuiRemotePreviousSessionSource],
) -> Option<GpuiPreviousSessionRestoreResult> {
    /*
    CDXC:Sessions 2026-06-24-11:53:
    Restore/delete commands from the shared Previous Sessions modal are local gxserver mutations only when the modal row carries the canonical `gxserver:<projectId>:<sessionId>` identity created by this projection. Restore creates a replacement workspace terminal with `restoredFromSessionId`, then removes the stopped history row only after create succeeds; unavailable gxserver or malformed history ids remain silent no-ops rather than fake success.
    */
    if let Some(reference) = gpui_remote_previous_session_reference_from_history_id(history_id) {
        let remote_source =
            gpui_remote_previous_session_source_for_reference(remote_sources, &reference)?;
        return gpui_restore_remote_previous_session(&reference, remote_source);
    }

    let (project_id, session_id) = gpui_previous_session_reference_from_history_id(history_id)?;
    let metadata = gpui_previous_session_restore_metadata(project_id, session_id)
        .unwrap_or_else(GpuiPreviousSessionRestoreMetadata::default_title);
    let response = gpui_gxserver_rpc_result(
        "/api/createSession",
        &gpui_previous_session_restore_create_params(project_id, session_id, metadata),
        Duration::from_secs(30),
    )
    .ok()?;
    let _ = gpui_gxserver_rpc_result(
        "/api/removeSession",
        &serde_json::json!({
            "projectId": project_id,
            "reason": "restorePreviousSession",
            "sessionId": session_id,
        }),
        Duration::from_secs(10),
    );
    // macOS opens the restored terminal as the active tab of the focused pane
    // (`createFocusedTabGroupPlacement`); GPUI follows up by focusing the
    // created session through the reviewed sidebar focusSession routing.
    let created = response.get("session")?;
    let created_project_id = created
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(&project_id)
        .to_string();
    let created_session_id = created
        .get("sessionId")
        .and_then(serde_json::Value::as_str)?
        .to_string();
    Some(GpuiPreviousSessionRestoreResult::Local {
        project_id: created_project_id,
        session_id: created_session_id,
    })
}

pub(crate) fn gpui_restore_remote_previous_session(
    reference: &GpuiRemotePreviousSessionReference,
    remote_source: &GpuiRemotePreviousSessionSource,
) -> Option<GpuiPreviousSessionRestoreResult> {
    /*
    CDXC:RemoteMachines 2026-07-04-14:15:
    App-modal remote previous-session restore follows the SidebarApp runtime:
    recreate the workspace session on the owning remote gxserver, copy only
    metadata fields from that remote gxserver's previous-session row, then
    remove the old remote history row. No local gxserver session or renderer
    supplied remote connection details are involved.
    */
    let metadata = gpui_remote_previous_session_restore_metadata(
        &remote_source.target,
        reference.project_id.as_str(),
        reference.session_id.as_str(),
    )
    .unwrap_or_else(GpuiPreviousSessionRestoreMetadata::default_title);
    let response = gpui_remote_gxserver_rpc_result(
        &remote_source.target,
        "/api/createSession",
        &gpui_previous_session_restore_create_params(
            reference.project_id.as_str(),
            reference.session_id.as_str(),
            metadata,
        ),
        Duration::from_secs(30),
    )
    .ok()?;
    let _ = gpui_remote_gxserver_rpc_result(
        &remote_source.target,
        "/api/removeSession",
        &serde_json::json!({
            "projectId": reference.project_id.as_str(),
            "reason": "restorePreviousSession",
            "sessionId": reference.session_id.as_str(),
        }),
        Duration::from_secs(10),
    );
    let created = response.get("session")?;
    let created_project_id = created
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(reference.project_id.as_str())
        .to_string();
    let created_session_id = created
        .get("sessionId")
        .and_then(serde_json::Value::as_str)?
        .to_string();
    if !gpui_remote_sidebar_project_id_allowed(created_project_id.as_str())
        || !gpui_remote_sidebar_session_id_allowed(created_session_id.as_str())
    {
        return None;
    }
    Some(GpuiPreviousSessionRestoreResult::Remote {
        project_id: created_project_id,
        remote_machine_id: reference.remote_machine_id.clone(),
        session_id: created_session_id,
    })
}

pub(crate) fn gpui_combined_presentation_session_focus_id(
    project_id: &str,
    session_id: &str,
) -> Option<String> {
    // The shared projection URI-encodes both id parts of the combined
    // `combined-session:` sidebar id. Build the id only from characters that
    // URI-encode to themselves so Rust never re-implements the encoder; other
    // ids skip the focus follow-up instead of guessing an encoding.
    let encodes_to_itself = |value: &str| {
        !value.is_empty()
            && value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~'))
    };
    (encodes_to_itself(project_id) && encodes_to_itself(session_id))
        .then(|| format!("combined-session:{project_id}:{session_id}"))
}

pub(crate) fn gpui_close_daemon_session_and_refresh_state(
    project_id: Option<String>,
    session_id: Option<String>,
    active_project_id: Option<&str>,
    focused_session_id: Option<&str>,
) -> serde_json::Value {
    let error_message = match (project_id, session_id) {
        (Some(project_id), Some(session_id)) => {
            /*
            CDXC:Sessions 2026-06-24-12:00:
            Running Sessions can close only gxserver-owned rows whose modal payload carries both project/workspace id and session id. Use `/api/transitionSession` with `close` instead of `/api/removeSession` so gxserver owns provider shutdown and lifecycle history, while malformed ids or transport failures refresh the list with an honest error and no fake success.
            */
            if gpui_gxserver_rpc_result(
                "/api/transitionSession",
                &serde_json::json!({
                    "action": "close",
                    "projectId": project_id,
                    "reason": "gpuiRunningSessionsModal",
                    "sessionId": session_id,
                }),
                Duration::from_secs(30),
            )
            .is_ok()
            {
                None
            } else {
                Some("GPUI could not close that gxserver session. The Running Sessions list was refreshed without reporting fake success.".to_string())
            }
        }
        _ => Some(
            "GPUI could not identify that gxserver session. The Running Sessions list was refreshed without changing daemon state."
                .to_string(),
        ),
    };
    gpui_daemon_sessions_state_message(error_message, active_project_id, focused_session_id)
}

pub(crate) fn gpui_daemon_sessions_state_message(
    error_message: Option<String>,
    active_project_id: Option<&str>,
    _focused_session_id: Option<&str>,
) -> serde_json::Value {
    /*
    CDXC:Sessions 2026-06-24-12:00:
    GPUI Running Sessions state is built from real local gxserver health and
    `/api/readPresentationSnapshot` only. If gxserver, auth, health, or
    presentation is unavailable, return the shared daemonSessionsState shape
    with empty rows and an explicit error message; do not invent daemon state,
    terminal text, commands, URLs, tokens, raw responses, or fallback sessions.
    */
    let health = gpui_gxserver_server_health(Duration::from_secs(2)).ok();
    let daemon = health
        .as_ref()
        .and_then(|health| gpui_daemon_info_from_gxserver_health(health).ok());
    let snapshot = match gpui_read_gxserver_presentation_snapshot() {
        Ok(snapshot) => snapshot,
        Err(_) => {
            let unavailable_message = error_message.unwrap_or_else(|| {
                "Local gxserver is unavailable, so Running Sessions cannot load shared daemon sessions."
                    .to_string()
            });
            return gpui_daemon_sessions_state_payload(
                daemon,
                Vec::new(),
                Some(unavailable_message),
            );
        }
    };
    let sessions =
        gpui_daemon_session_items_from_presentation_snapshot(&snapshot, active_project_id);
    gpui_daemon_sessions_state_payload(daemon, sessions, error_message)
}

pub(crate) fn gpui_daemon_sessions_state_payload(
    daemon: Option<serde_json::Value>,
    sessions: Vec<serde_json::Value>,
    error_message: Option<String>,
) -> serde_json::Value {
    let mut payload = serde_json::Map::new();
    if let Some(daemon) = daemon {
        payload.insert("daemon".to_string(), daemon);
    }
    if let Some(error_message) = error_message {
        payload.insert(
            "errorMessage".to_string(),
            serde_json::Value::String(error_message),
        );
    }
    payload.insert("sessions".to_string(), serde_json::Value::Array(sessions));
    payload.insert(
        "type".to_string(),
        serde_json::Value::String("daemonSessionsState".to_string()),
    );
    serde_json::Value::Object(payload)
}
