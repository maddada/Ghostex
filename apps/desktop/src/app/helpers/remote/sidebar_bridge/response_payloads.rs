use super::*;
use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_remote_sidebar_response_payload(
    path: &str,
    result: serde_json::Value,
) -> serde_json::Value {
    /*
    CDXC:RemoteMachines 2026-06-24-17:19:
    Response-capable remote sidebar RPCs may return only the sanitized payload shapes explicitly matched here: created-session ids, previous-session metadata, recent-project rows, presentation snapshots, project Git preference metadata, command-stripped typed Git/GitHub/Beads results, generated commit text, PR state confirmation, and delete-warning kinds. Keep path-bearing project list/add and remote native launch data out of this bridge so renderer payloads do not become side-effect authority.
    */
    match path {
        "/api/createSession" | "/api/createAgentSession" => {
            gpui_remote_sidebar_created_session_response_payload(result)
        }
        "/api/listPreviousSessions" => {
            gpui_remote_sidebar_previous_sessions_response_payload(result)
        }
        "/api/listRecentProjects" => gpui_remote_sidebar_recent_projects_response_payload(result),
        "/api/readPresentationSnapshot" => result,
        "/api/readSidebarHud" => gpui_remote_sidebar_hud_response_payload(result),
        "/api/readAgentHookStatus" | "/api/installAgentHooks" => {
            gpui_remote_sidebar_agent_hook_status_response_payload(result)
        }
        "/api/scheduleDelayedSend" | "/api/postponeDelayedSend" => serde_json::json!({}),
        "/api/queueSessionChatPrompt" => result
            .pointer("/prompt/id")
            .and_then(serde_json::Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= 128 && !id.chars().any(char::is_control))
            .map(|id| serde_json::json!({ "prompt": { "id": id } }))
            .unwrap_or(serde_json::Value::Null),
        "/api/cancelDelayedSend" => serde_json::json!({
            "changed": result
                .get("changed")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
        }),
        "/api/updateSidebarProjectCollections" => result
            .get("sidebarProjectCollections")
            .and_then(gpui_remote_sidebar_project_collections_state)
            .map(|sidebar_project_collections| {
                serde_json::json!({
                    "sidebarProjectCollections": sidebar_project_collections,
                })
            })
            .unwrap_or(serde_json::Value::Null),
        // Both documents are adopted by the caller, which refuses an answer that is not the
        // document it asked about, so each is rebuilt through the same shaping its params went
        // through rather than passed on as the machine sent it.
        "/api/updateSidebarSpaces" => result
            .get("sidebarSpaces")
            .and_then(gpui_remote_sidebar_spaces_state)
            .map(|sidebar_spaces| serde_json::json!({ "sidebarSpaces": sidebar_spaces }))
            .unwrap_or(serde_json::Value::Null),
        "/api/updateWorkspaceSessionGroups" => result
            .get("groups")
            .and_then(gpui_remote_sidebar_workspace_groups_state)
            .map(|groups| serde_json::json!({ "groups": groups }))
            .unwrap_or(serde_json::Value::Null),
        "/api/updateProject"
        | "/api/closeProjectToRecent"
        | "/api/restoreRecentProject"
        | "/api/removeRecentProject"
        | "/api/removeProject" => gpui_remote_sidebar_project_response_payload(result),
        "/api/createProjectWorktree" | "/api/openProjectWorktree" => {
            gpui_remote_sidebar_worktree_project_response_payload(result)
        }
        "/api/listProjectWorktrees" => {
            gpui_remote_sidebar_project_worktrees_response_payload(result)
        }
        "/api/createWorktreeSession" => {
            gpui_remote_sidebar_create_worktree_session_response_payload(result)
        }
        "/api/removeSessionWorktree" => {
            gpui_remote_sidebar_remove_session_worktree_response_payload(result)
        }
        "/api/mergeWorktreeIntoMain" => gpui_remote_sidebar_merge_worktree_response_payload(result),
        "/api/checkoutProjectNewBranch" => {
            gpui_remote_sidebar_checkout_new_branch_response_payload(result)
        }
        "/api/runGitAction" | "/api/runGitHubAction" | "/api/runBeadsAction" => {
            gpui_remote_sidebar_typed_operation_response_payload(result)
        }
        "/api/generateCommitMessage" => {
            gpui_remote_sidebar_generate_commit_message_response_payload(result)
        }
        "/api/createPullRequest" => {
            gpui_remote_sidebar_create_pull_request_response_payload(result)
        }
        "/api/deleteWorktreeProject" => {
            gpui_remote_sidebar_delete_worktree_response_payload(result)
        }
        "/api/exportSessionTranscript" => {
            gpui_remote_sidebar_export_session_transcript_response_payload(result)
        }
        "/api/agentAccounts" => gpui_remote_sidebar_agent_accounts_response_payload(result),
        _ => serde_json::Value::Null,
    }
}

pub(crate) fn gpui_remote_sidebar_agent_hook_status_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    let agents = result
        .get("agents")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let agent_id = row
                .get("agentId")
                .and_then(serde_json::Value::as_str)
                .filter(|value| gpui_remote_sidebar_agent_id_allowed(value))?;
            let status = row
                .get("status")
                .and_then(serde_json::Value::as_str)
                .filter(|value| {
                    matches!(
                        *value,
                        "cliMissing" | "installed" | "missing" | "updateRequired"
                    )
                })?;
            Some(serde_json::json!({
                "agentId": agent_id,
                "status": status,
            }))
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "agents": agents,
        "type": "agentHookStatus",
    })
}

/*
CDXC:TranscriptExport 2026-08-20:
The export answer the sidebar actually consumes: where the markdown landed on
the remote machine (so the dialog can show and copy it, and the seeded prompt
can reference it) and how big it is. The remote daemon also reports its source
transcript path and parse counters; those are diagnostics, so they stop here.
*/
pub(crate) fn gpui_remote_sidebar_export_session_transcript_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    let Some(path) = result
        .get("path")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_worktree_path_allowed(value))
    else {
        return serde_json::Value::Null;
    };
    let mut response = serde_json::Map::new();
    response.insert("path".to_string(), serde_json::json!(path));
    if let Some(bytes) = result.get("bytes").and_then(serde_json::Value::as_u64) {
        response.insert("bytes".to_string(), serde_json::json!(bytes));
    }
    // The agent name only picks the dialog's preselected agent, so it stays a
    // short lowercase token (`claude`, `codex`, `grok`, `pi`).
    if let Some(agent) = result
        .get("agent")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 40
                && value
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
        })
    {
        response.insert("agent".to_string(), serde_json::json!(agent));
    }
    serde_json::Value::Object(response)
}

pub(crate) fn gpui_remote_sidebar_created_session_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    let Some(session) = result.get("session").and_then(serde_json::Value::as_object) else {
        return serde_json::json!({});
    };
    let mut sanitized_session = serde_json::Map::new();
    if let Some(project_id) = session
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))
    {
        sanitized_session.insert("projectId".to_string(), serde_json::json!(project_id));
    }
    if let Some(session_id) = session
        .get("sessionId")
        .and_then(serde_json::Value::as_str)
        .filter(|value| gpui_remote_sidebar_session_id_allowed(value))
    {
        sanitized_session.insert("sessionId".to_string(), serde_json::json!(session_id));
    }
    if sanitized_session.is_empty() {
        serde_json::json!({})
    } else {
        serde_json::json!({ "session": sanitized_session })
    }
}

/*
CDXC:StateSync 2026-07-29:
The worktree-create answer the sidebar actually consumes: the created session's
id (so the host can focus it), the checkout it landed in (so the cleanup prompt
can name it later), and the branch (so the toast/label can state it). Anything
else the daemon returns is dropped, and each field is validated with the same
shape rule the request side uses — a daemon reply is not a reason to relax the
boundary the request had to pass.
*/
pub(crate) fn gpui_remote_sidebar_create_worktree_session_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    let mut response = serde_json::Map::new();
    if let Some(session_id) = result
        .get("sessionId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_session_id_allowed(value))
    {
        response.insert("sessionId".to_string(), serde_json::json!(session_id));
    }
    if let Some(worktree_path) = result
        .get("worktreePath")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_worktree_path_allowed(value))
    {
        response.insert("worktreePath".to_string(), serde_json::json!(worktree_path));
    }
    if let Some(branch) = result
        .get("branch")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_git_ref_allowed(value))
    {
        response.insert("branch".to_string(), serde_json::json!(branch));
    }
    serde_json::Value::Object(response)
}

/*
The removal verdict. `dirty` is a REFUSAL the sidebar re-asks on, not a failure,
so it has to survive the boundary alongside `removed`. Warnings are already
user-safe prose by contract (never raw git output), but they are still bounded
in count and length here so a misbehaving daemon cannot flood the renderer.
*/
pub(crate) fn gpui_remote_sidebar_remove_session_worktree_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    let mut response = serde_json::Map::new();
    response.insert(
        "removed".to_string(),
        serde_json::Value::Bool(
            result.get("removed").and_then(serde_json::Value::as_bool) == Some(true),
        ),
    );
    if result.get("dirty").and_then(serde_json::Value::as_bool) == Some(true) {
        response.insert("dirty".to_string(), serde_json::Value::Bool(true));
    }
    let warnings = result
        .get("warnings")
        .and_then(serde_json::Value::as_array)
        .map(|warnings| {
            warnings
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|warning| !warning.is_empty() && !warning.chars().any(char::is_control))
                .take(20)
                .map(|warning| {
                    serde_json::Value::String(warning.chars().take(400).collect::<String>())
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !warnings.is_empty() {
        response.insert("warnings".to_string(), serde_json::Value::Array(warnings));
    }
    serde_json::Value::Object(response)
}

pub(crate) fn gpui_remote_sidebar_previous_sessions_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    /*
    CDXC:RemoteMachines 2026-06-24-17:19:
    Remote previous-session search results only need titles, stable project/session ids, timestamps, tags, and provider identity metadata for restore. Strip path-bearing fields at the Rust boundary before CEF sees the response.
    */
    let cursor = result.get("cursor").cloned();
    let results = result
        .get("results")
        .and_then(serde_json::Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row.as_object())
                .map(|row| {
                    let mut sanitized = row.clone();
                    sanitized.remove("agentSessionPath");
                    sanitized.remove("cwd");
                    serde_json::Value::Object(sanitized)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut response = serde_json::Map::new();
    if let Some(cursor) = cursor {
        response.insert("cursor".to_string(), cursor);
    }
    response.insert("results".to_string(), serde_json::Value::Array(results));
    serde_json::Value::Object(response)
}

pub(crate) fn gpui_remote_sidebar_project_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    let mut response = serde_json::Map::new();
    if let Some(project) = result
        .get("project")
        .and_then(serde_json::Value::as_object)
        .and_then(gpui_remote_sidebar_presentation_project_payload)
    {
        response.insert("project".to_string(), project);
    }
    if let Some(recent_projects) = gpui_remote_sidebar_recent_projects_value(&result) {
        response.insert("recentProjects".to_string(), recent_projects);
    }
    serde_json::Value::Object(response)
}

/// A created or opened worktree's project, plus the warnings that came with it (`{ kind, message }`,
/// gxserver's Beads hooks repair), so a remote machine's warning reaches its user too. Only string
/// kinds and messages of a bounded size pass.
pub(crate) fn gpui_remote_sidebar_worktree_project_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    let warnings = result
        .get("warnings")
        .and_then(serde_json::Value::as_array)
        .map(|warnings| {
            warnings
                .iter()
                .filter_map(|warning| {
                    let kind = warning
                        .get("kind")
                        .and_then(serde_json::Value::as_str)
                        .filter(|kind| {
                            !kind.is_empty()
                                && kind.len() <= 64
                                && kind.chars().all(|ch| ch.is_ascii_alphanumeric())
                        })?;
                    let message = warning
                        .get("message")
                        .and_then(serde_json::Value::as_str)
                        .map(gpui_remote_sidebar_bounded_warning_message)
                        .unwrap_or_default();
                    Some(serde_json::json!({ "kind": kind, "message": message }))
                })
                .take(8)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut response = gpui_remote_sidebar_project_response_payload(result);
    if !warnings.is_empty()
        && let Some(object) = response.as_object_mut()
    {
        object.insert("warnings".to_string(), serde_json::Value::Array(warnings));
    }
    response
}

/// A warning message cut to 4 KiB on a character boundary, with a visible marker when it was cut.
fn gpui_remote_sidebar_bounded_warning_message(message: &str) -> String {
    const LIMIT: usize = 4096;
    const MARKER: &str = " … (cut)";
    if message.len() <= LIMIT {
        return message.to_string();
    }
    let mut end = LIMIT - MARKER.len();
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{MARKER}", &message[..end])
}

pub(crate) fn gpui_remote_sidebar_recent_projects_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    let mut response = serde_json::Map::new();
    response.insert(
        "recentProjects".to_string(),
        gpui_remote_sidebar_recent_projects_value(&result)
            .unwrap_or_else(|| serde_json::Value::Array(Vec::new())),
    );
    serde_json::Value::Object(response)
}

pub(crate) fn gpui_remote_sidebar_project_worktrees_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    /*
    CDXC:RemoteMachines 2026-06-24-18:40:
    Remote Add Worktree receives display rows plus opaque worktree keys from the
    owning daemon. The bridge must not accept renderer paths for the subsequent
    open-existing mutation; it forwards only daemon-returned rows and strips all
    unrelated response fields before CEF receives them.
    */
    let mut response = serde_json::Map::new();
    if let Some(parent_project_id) = result
        .get("parentProjectId")
        .and_then(serde_json::Value::as_str)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))
    {
        response.insert(
            "parentProjectId".to_string(),
            serde_json::json!(parent_project_id),
        );
    }
    if let Some(source_project_id) = result
        .get("sourceProjectId")
        .and_then(serde_json::Value::as_str)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))
    {
        response.insert(
            "sourceProjectId".to_string(),
            serde_json::json!(source_project_id),
        );
    }
    let branches = result
        .get("branches")
        .and_then(serde_json::Value::as_array)
        .map(|branches| {
            branches
                .iter()
                .filter_map(|branch| branch.as_object())
                .filter_map(gpui_remote_sidebar_worktree_branch_payload)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    response.insert("branches".to_string(), serde_json::Value::Array(branches));
    let worktrees = result
        .get("worktrees")
        .and_then(serde_json::Value::as_array)
        .map(|worktrees| {
            worktrees
                .iter()
                .filter_map(|worktree| worktree.as_object())
                .filter_map(gpui_remote_sidebar_worktree_option_payload)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    response.insert("worktrees".to_string(), serde_json::Value::Array(worktrees));
    serde_json::Value::Object(response)
}

pub(crate) fn gpui_remote_sidebar_worktree_branch_payload(
    branch: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    let name = json_string_field(branch, "name")?;
    let mut output = serde_json::Map::new();
    output.insert(
        "current".to_string(),
        serde_json::json!(json_bool_field(branch, "current").unwrap_or(false)),
    );
    output.insert("name".to_string(), serde_json::json!(name));
    output.insert(
        "remote".to_string(),
        serde_json::json!(json_bool_field(branch, "remote").unwrap_or(false)),
    );
    Some(serde_json::Value::Object(output))
}

pub(crate) fn gpui_remote_sidebar_worktree_option_payload(
    worktree: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    let name = json_string_field(worktree, "name")?;
    let path = json_string_field(worktree, "path")?;
    let worktree_key = json_string_field(worktree, "worktreeKey")
        .filter(|value| gpui_remote_sidebar_worktree_key_allowed(value))?;
    let mut output = serde_json::Map::new();
    if let Some(branch) = json_string_field(worktree, "branch") {
        output.insert("branch".to_string(), serde_json::json!(branch));
    }
    output.insert(
        "isCurrentProject".to_string(),
        serde_json::json!(json_bool_field(worktree, "isCurrentProject").unwrap_or(false)),
    );
    output.insert(
        "isRegistered".to_string(),
        serde_json::json!(json_bool_field(worktree, "isRegistered").unwrap_or(false)),
    );
    output.insert("name".to_string(), serde_json::json!(name));
    output.insert("path".to_string(), serde_json::json!(path));
    output.insert("worktreeKey".to_string(), serde_json::json!(worktree_key));
    Some(serde_json::Value::Object(output))
}

pub(crate) fn gpui_remote_sidebar_merge_worktree_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    let mut response = serde_json::Map::new();
    if let Some(parent_project_id) = result
        .get("parentProjectId")
        .and_then(serde_json::Value::as_str)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))
    {
        response.insert(
            "parentProjectId".to_string(),
            serde_json::json!(parent_project_id),
        );
    }
    if let Some(status) = result
        .get("status")
        .and_then(serde_json::Value::as_str)
        .filter(|value| matches!(*value, "conflicts" | "merged"))
    {
        response.insert("status".to_string(), serde_json::json!(status));
    }
    serde_json::Value::Object(response)
}

pub(crate) fn gpui_remote_sidebar_checkout_new_branch_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "checkedOut": result
            .get("checkedOut")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    })
}
