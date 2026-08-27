// C1 wave-1 deferred split: apps/desktop/src/app/helpers/remote.rs (~8.2k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the remote sidebar RPC request/response
// param and payload builders. See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::time::Duration;

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_remote_sidebar_request_timeout(
    command: &serde_json::Map<String, serde_json::Value>,
) -> Duration {
    let timeout_ms = command
        .get("timeoutMs")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(15_000)
        .clamp(
            GPUI_REMOTE_GXSERVER_SIDEBAR_REQUEST_TIMEOUT_MIN_MS,
            GPUI_REMOTE_GXSERVER_SIDEBAR_REQUEST_TIMEOUT_MAX_MS,
        );
    Duration::from_millis(timeout_ms)
}

pub(crate) fn gpui_remote_sidebar_request_path_allowed(path: &str) -> bool {
    matches!(
        path,
        "/api/createSession"
            | "/api/createAgentSession"
            | "/api/readAgentHookStatus"
            | "/api/installAgentHooks"
            | "/api/forkSession"
            | "/api/scheduleDelayedSend"
            | "/api/cancelDelayedSend"
            | "/api/sleepSession"
            | "/api/wakeSession"
            | "/api/killSession"
            /*
            CDXC:GPUIRemoteAgentLaunch 2026-08-18:
            Creating a remote agent session is a two-step daemon operation:
            `/api/createAgentSession` writes the row and queues the agent's
            launch startup text, then `/api/startSessionProvider` spawns the
            zmx provider that actually runs the agent, and `/api/sendSessionMessage`
            delivers the workflow prompt. Leaving the second and third steps off
            this allowlist made every remote agent launch report "Remote agent
            failed" at the Rust boundary and silently dropped Git/worktree
            workflow prompts. Params below are reshaped to the two ids (plus a
            bounded message body) so CEF still cannot tunnel startup text,
            commands, or daemon flags to a remote machine.
            */
            | "/api/startSessionProvider"
            | "/api/sendSessionMessage"
            | "/api/updateSession"
            | "/api/requestSessionRename"
            /*
            CDXC:ExportTranscript 2026-08-20:
            A remote session's transcript only exists on the machine that runs
            the agent, so Export Transcript is an id-scoped read-and-write on
            that machine's own daemon, exactly like sleep/wake. Params are
            reshaped to the two ids below and the answer is reduced to the
            written path plus its size.
            */
            | "/api/exportSessionTranscript"
            /*
            CDXC:SidebarV2Lifecycle 2026-07-29:
            Sidebar V2's settle/snooze commands are id-scoped session mutations
            on a remote machine's own daemon, exactly like sleep/wake/kill.
            Their params are reshaped below so CEF can only ever send the two
            ids plus a bounded ISO wake time.
            */
            | "/api/settleSession"
            | "/api/unsettleSession"
            | "/api/snoozeSession"
            | "/api/unsnoozeSession"
            | "/api/listPreviousSessions"
            | "/api/removeSession"
            | "/api/updateProject"
            | "/api/listRecentProjects"
            | "/api/closeProjectToRecent"
            | "/api/restoreRecentProject"
            | "/api/removeRecentProject"
            | "/api/removeProject"
            | "/api/listProjectWorktrees"
            | "/api/createProjectWorktree"
            | "/api/openProjectWorktree"
            /*
            CDXC:SidebarV2LogicalProjects 2026-07-29:
            Sidebar V2's worktree flow, allow-listed for remote machines: only
            the daemon that holds the repository can cut or delete a checkout in
            it, so these are project-scoped mutations on that machine's own
            gxserver, exactly like the settle/snooze pair above. Params are
            reshaped below so CEF can only ever send a project id, bounded
            agent/branch/prompt strings, and one nested existing-worktree path;
            responses are reduced to the created session's ids plus the removal
            verdict.
            */
            | "/api/createWorktreeSession"
            | "/api/removeSessionWorktree"
            | "/api/mergeWorktreeIntoMain"
            | "/api/checkoutProjectNewBranch"
            | "/api/readPresentationSnapshot"
            | "/api/updateSidebarProjectCollections"
            | "/api/runGitAction"
            | "/api/runGitHubAction"
            | "/api/runBeadsAction"
            | "/api/generateCommitMessage"
            | "/api/createPullRequest"
            | "/api/deleteWorktreeProject"
    )
}

pub(crate) fn gpui_remote_sidebar_request_params(
    path: &str,
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    /*
    CDXC:GPUIRemoteProjects 2026-06-24-18:22:
    The remote sidebar bridge allowlists project mutation endpoints only for id-scoped operations. Shape params at the Rust boundary so CEF cannot tunnel arbitrary updateProject fields, paths, names, commands, URLs, branch refs, tokens, stdout/stderr, or daemon response authority to a remote gxserver.
    */
    match path {
        "/api/updateProject" => gpui_remote_sidebar_update_project_params(params),
        "/api/readAgentHookStatus" | "/api/installAgentHooks" => {
            gpui_remote_sidebar_agent_hook_params(params)
        }
        "/api/updateSidebarProjectCollections" => {
            gpui_remote_sidebar_project_collections_params(params)
        }
        "/api/closeProjectToRecent"
        | "/api/restoreRecentProject"
        | "/api/removeRecentProject"
        | "/api/removeProject"
        | "/api/listProjectWorktrees"
        | "/api/mergeWorktreeIntoMain" => gpui_remote_sidebar_project_id_params(params),
        "/api/scheduleDelayedSend" => gpui_remote_sidebar_delayed_send_params(params, false),
        "/api/cancelDelayedSend" => gpui_remote_sidebar_delayed_send_params(params, true),
        "/api/startSessionProvider" => gpui_remote_sidebar_session_lifecycle_params(params, None),
        "/api/sendSessionMessage" => gpui_remote_sidebar_send_session_message_params(params),
        "/api/settleSession"
        | "/api/unsettleSession"
        | "/api/unsnoozeSession"
        | "/api/exportSessionTranscript" => {
            gpui_remote_sidebar_session_lifecycle_params(params, None)
        }
        "/api/snoozeSession" => {
            gpui_remote_sidebar_session_lifecycle_params(params, Some("snoozedUntil"))
        }
        "/api/requestSessionRename" => gpui_remote_sidebar_request_session_rename_params(params),
        "/api/createProjectWorktree" => gpui_remote_sidebar_create_project_worktree_params(params),
        "/api/openProjectWorktree" => gpui_remote_sidebar_open_project_worktree_params(params),
        "/api/createWorktreeSession" => gpui_remote_sidebar_create_worktree_session_params(params),
        "/api/removeSessionWorktree" => gpui_remote_sidebar_remove_session_worktree_params(params),
        "/api/checkoutProjectNewBranch" => {
            gpui_remote_sidebar_checkout_project_new_branch_params(params)
        }
        _ => Some(params),
    }
}

pub(crate) fn gpui_remote_sidebar_agent_hook_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let agent_ids = params
        .as_object()?
        .get("agentIds")?
        .as_array()?
        .iter()
        .map(serde_json::Value::as_str)
        .collect::<Option<Vec<_>>>()?;
    if agent_ids.is_empty()
        || agent_ids.len() > 16
        || agent_ids
            .iter()
            .any(|agent_id| !gpui_remote_sidebar_agent_id_allowed(agent_id))
    {
        return None;
    }
    Some(serde_json::json!({ "agentIds": agent_ids }))
}

pub(crate) fn gpui_remote_sidebar_delayed_send_params(
    params: serde_json::Value,
    cancel: bool,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let session_id = object
        .get("sessionId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_session_id_allowed(value))?;
    let mut shaped = serde_json::Map::new();
    shaped.insert("projectId".to_string(), serde_json::json!(project_id));
    shaped.insert("sessionId".to_string(), serde_json::json!(session_id));
    if cancel {
        return Some(serde_json::Value::Object(shaped));
    }
    let delay_ms = object.get("delayMs").and_then(serde_json::Value::as_u64);
    let send_when_agent_stops = object
        .get("sendWhenAgentStops")
        .and_then(serde_json::Value::as_bool)
        == Some(true);
    let send_when_all_project_sessions_stop = object
        .get("sendWhenAllProjectSessionsStop")
        .and_then(serde_json::Value::as_bool)
        == Some(true);
    if usize::from(delay_ms.is_some())
        + usize::from(send_when_agent_stops)
        + usize::from(send_when_all_project_sessions_stop)
        != 1
    {
        return None;
    }
    if let Some(delay_ms) = delay_ms {
        gpui_command_delayed_send_duration_from_millis(delay_ms)?;
        shaped.insert("delayMs".to_string(), serde_json::json!(delay_ms));
    } else if send_when_agent_stops {
        shaped.insert(
            "sendWhenAgentStops".to_string(),
            serde_json::Value::Bool(true),
        );
    } else {
        shaped.insert(
            "sendWhenAllProjectSessionsStop".to_string(),
            serde_json::Value::Bool(true),
        );
    }
    Some(serde_json::Value::Object(shaped))
}

pub(crate) fn gpui_remote_sidebar_project_id_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    Some(serde_json::json!({ "projectId": project_id }))
}

pub(crate) fn gpui_remote_sidebar_project_collections_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let state = gpui_remote_sidebar_project_collections_state(params.get("state")?)?;
    Some(serde_json::json!({ "state": state }))
}

pub(crate) fn gpui_remote_sidebar_project_collections_state(
    value: &serde_json::Value,
) -> Option<serde_json::Value> {
    const MAX_COLLECTIONS: usize = 256;
    const MAX_PROJECT_IDS_PER_COLLECTION: usize = 512;
    const MAX_ID_CHARS: usize = 256;
    const MAX_TITLE_CHARS: usize = 256;
    const MAX_NEXT_COLLECTION_NUMBER: u64 = 1_000_000;

    let source = value.as_object()?;
    let source_collections = source.get("collections")?.as_object()?;
    let source_order = source.get("order")?.as_array()?;
    let next_collection_number = source.get("nextCollectionNumber")?.as_u64()?;
    if source_collections.len() > MAX_COLLECTIONS
        || source_order.len() != source_collections.len()
        || !(1..=MAX_NEXT_COLLECTION_NUMBER).contains(&next_collection_number)
    {
        return None;
    }

    fn bounded_text(candidate: &str, max_chars: usize) -> Option<&str> {
        let trimmed = candidate.trim();
        (!trimmed.is_empty()
            && trimmed.chars().count() <= max_chars
            && !trimmed.contains('\0')
            && !trimmed.chars().any(char::is_control))
        .then_some(trimmed)
    }
    let valid_color = |candidate: &str| {
        candidate == "transparent"
            || (candidate.len() == 7
                && candidate.starts_with('#')
                && candidate[1..].bytes().all(|byte| byte.is_ascii_hexdigit()))
    };

    let mut collections = serde_json::Map::new();
    for (collection_id, candidate) in source_collections {
        let normalized_collection_id = bounded_text(collection_id, MAX_ID_CHARS)?;
        let candidate = candidate.as_object()?;
        let embedded_collection_id = candidate.get("collectionId")?.as_str()?;
        if embedded_collection_id != normalized_collection_id {
            return None;
        }
        let title = bounded_text(candidate.get("title")?.as_str()?, MAX_TITLE_CHARS)?;
        let color = candidate.get("color")?.as_str()?;
        let collapsed = candidate.get("collapsed")?.as_bool()?;
        let source_project_ids = candidate.get("projectIds")?.as_array()?;
        if !valid_color(color)
            || source_project_ids.is_empty()
            || source_project_ids.len() > MAX_PROJECT_IDS_PER_COLLECTION
        {
            return None;
        }
        let mut project_ids = Vec::with_capacity(source_project_ids.len());
        for project_id in source_project_ids {
            let project_id = project_id.as_str()?.trim();
            if !gpui_remote_sidebar_project_id_allowed(project_id) {
                return None;
            }
            project_ids.push(serde_json::Value::String(project_id.to_string()));
        }
        collections.insert(
            normalized_collection_id.to_string(),
            serde_json::json!({
                "collapsed": collapsed,
                "collectionId": normalized_collection_id,
                "color": color,
                "projectIds": project_ids,
                "title": title,
            }),
        );
    }

    let mut order = Vec::with_capacity(source_order.len());
    let mut seen_order_ids = std::collections::HashSet::new();
    for collection_id in source_order {
        let collection_id = bounded_text(collection_id.as_str()?, MAX_ID_CHARS)?;
        if !collections.contains_key(collection_id)
            || !seen_order_ids.insert(collection_id.to_string())
        {
            return None;
        }
        order.push(serde_json::Value::String(collection_id.to_string()));
    }

    Some(serde_json::json!({
        "collections": collections,
        "nextCollectionNumber": next_collection_number,
        "order": order,
    }))
}

/*
CDXC:SidebarV2Lifecycle 2026-07-29:
Settle/snooze params reduced to their id scope at the Rust boundary. The only
extra field any of them may carry is `snoozedUntil`, and it is accepted only as
a bounded RFC3339-shaped ASCII timestamp — the remote daemon validates that it
is strictly in the future, but CEF must not be able to tunnel arbitrary text
through this endpoint on the way there.
*/
pub(crate) fn gpui_remote_sidebar_session_lifecycle_params(
    params: serde_json::Value,
    extra_timestamp_key: Option<&str>,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let session_id = object
        .get("sessionId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_session_id_allowed(value))?;
    let mut shaped = serde_json::Map::new();
    shaped.insert(
        "projectId".to_string(),
        serde_json::Value::String(project_id.to_string()),
    );
    shaped.insert(
        "sessionId".to_string(),
        serde_json::Value::String(session_id.to_string()),
    );
    if let Some(key) = extra_timestamp_key {
        let timestamp = object
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| gpui_remote_sidebar_iso_timestamp_allowed(value))?;
        shaped.insert(
            key.to_string(),
            serde_json::Value::String(timestamp.to_string()),
        );
    }
    Some(serde_json::Value::Object(shaped))
}

/*
CDXC:GPUIRemoteAgentLaunch 2026-08-18:
A remote agent prompt is user-authored message text, not a command: gxserver
types it into the session and submits it. Shape it to the two ids plus the
bounded body and pin `submit` here so this route can never become a way for CEF
to write unsubmitted terminal input or pass daemon-only send flags.
*/
pub(crate) fn gpui_remote_sidebar_send_session_message_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    const MAX_MESSAGE_BYTES: usize = 32 * 1024;
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let session_id = object
        .get("sessionId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_session_id_allowed(value))?;
    let text = object
        .get("text")
        .and_then(serde_json::Value::as_str)
        .filter(|value| {
            !value.is_empty() && value.len() <= MAX_MESSAGE_BYTES && !value.contains('\0')
        })?;
    Some(serde_json::json!({
        "projectId": project_id,
        "sessionId": session_id,
        "submit": true,
        "text": text,
    }))
}

pub(crate) fn gpui_remote_sidebar_iso_timestamp_allowed(value: &str) -> bool {
    (20..=40).contains(&value.len())
        && value.starts_with(|ch: char| ch.is_ascii_digit())
        && value
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, '-' | ':' | '.' | 'T' | 'Z' | '+'))
}

pub(crate) fn gpui_remote_sidebar_request_session_rename_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    /*
    CDXC:GPUIRemoteSessionRename 2026-08-12:
    A remote rename may carry only the target ids, bounded normalized title,
    and optional agent id into the selected machine's gxserver. The native
    bridge fixes the request reason/source and opts into daemon-owned command
    submission so CEF cannot turn this route into arbitrary remote terminal
    input.
    */
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let session_id = object
        .get("sessionId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_session_id_allowed(value))?;
    let title = gxserver_workspace_terminal_rename_title_field(object, "title").ok()?;
    let mut shaped = serde_json::Map::new();
    shaped.insert("projectId".to_string(), serde_json::json!(project_id));
    shaped.insert("reason".to_string(), serde_json::json!("gpui-sidebar"));
    shaped.insert("sessionId".to_string(), serde_json::json!(session_id));
    shaped.insert(
        "submitAgentRenameCommand".to_string(),
        serde_json::Value::Bool(true),
    );
    shaped.insert("title".to_string(), serde_json::Value::String(title));
    shaped.insert("titleSource".to_string(), serde_json::json!("user"));
    if let Some(agent_name) = object.get("agentName") {
        let agent_name = agent_name
            .as_str()
            .map(str::trim)
            .filter(|value| gpui_remote_sidebar_agent_id_allowed(value))?;
        shaped.insert(
            "agentName".to_string(),
            serde_json::Value::String(agent_name.to_string()),
        );
    }
    Some(serde_json::Value::Object(shaped))
}

pub(crate) fn gpui_remote_sidebar_update_project_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let git_config = object
        .get("gitConfig")
        .and_then(serde_json::Value::as_object)
        .and_then(gpui_remote_sidebar_git_preferences_update_payload)?;
    Some(serde_json::json!({
        "gitConfig": git_config,
        "projectId": project_id,
    }))
}

pub(crate) fn gpui_remote_sidebar_create_project_worktree_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let base_ref = object
        .get("baseRef")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_git_ref_allowed(value))?;
    let name_hint = object
        .get("nameHint")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_slug_label_allowed(value))?;
    Some(serde_json::json!({
        "baseRef": base_ref,
        "nameHint": name_hint,
        "projectId": project_id,
    }))
}

pub(crate) fn gpui_remote_sidebar_open_project_worktree_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let worktree_key = object
        .get("worktreeKey")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_worktree_key_allowed(value))?;
    Some(serde_json::json!({
        "projectId": project_id,
        "worktreeKey": worktree_key,
    }))
}

/*
CDXC:SidebarV2LogicalProjects 2026-07-29:
Sidebar V2 worktree-create params, reduced to the P4 wire contract at the Rust
boundary. Every optional field is dropped unless it passes its own shape check,
so a malformed value can never be forwarded verbatim: the remote daemon then
sees a well-formed request missing that field rather than renderer-supplied text
it has to defend against. Only `projectId` is mandatory — everything else has a
server-side default (last agent, project default branch, no prompt).

`existingWorktree.path` is the one path this bridge accepts from the renderer,
and it is accepted only because the flow demands it: the client learned the path
from THAT machine's own presentation (`session.cwd`). It is still bounded and
absolute-only here, and the daemon re-applies its own path-safety normalization
before touching the filesystem.
*/
pub(crate) fn gpui_remote_sidebar_create_worktree_session_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let mut shaped = serde_json::Map::new();
    shaped.insert(
        "projectId".to_string(),
        serde_json::Value::String(project_id.to_string()),
    );
    if let Some(agent_id) = object
        .get("agentId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_agent_id_allowed(value))
    {
        shaped.insert(
            "agentId".to_string(),
            serde_json::Value::String(agent_id.to_string()),
        );
    }
    if let Some(base_branch) = object
        .get("baseBranch")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_git_ref_allowed(value))
    {
        shaped.insert(
            "baseBranch".to_string(),
            serde_json::Value::String(base_branch.to_string()),
        );
    }
    if let Some(first_prompt) = object
        .get("firstPrompt")
        .and_then(serde_json::Value::as_str)
        .filter(|value| gpui_remote_sidebar_first_prompt_allowed(value))
    {
        shaped.insert(
            "firstPrompt".to_string(),
            serde_json::Value::String(first_prompt.to_string()),
        );
    }
    if object
        .get("startFromOrigin")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        shaped.insert("startFromOrigin".to_string(), serde_json::Value::Bool(true));
    }
    if let Some(path) = object
        .get("existingWorktree")
        .and_then(serde_json::Value::as_object)
        .and_then(|worktree| worktree.get("path"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_worktree_path_allowed(value))
    {
        shaped.insert(
            "existingWorktree".to_string(),
            serde_json::json!({ "path": path }),
        );
    }
    Some(serde_json::Value::Object(shaped))
}

pub(crate) fn gpui_remote_sidebar_remove_session_worktree_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let worktree_path = object
        .get("worktreePath")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_worktree_path_allowed(value))?;
    let mut shaped = serde_json::Map::new();
    shaped.insert(
        "projectId".to_string(),
        serde_json::Value::String(project_id.to_string()),
    );
    shaped.insert(
        "worktreePath".to_string(),
        serde_json::Value::String(worktree_path.to_string()),
    );
    /*
    `force` is a DESTRUCTIVE escalation (delete a dirty checkout), so it is
    forwarded only as an explicit `true`. Any other value simply omits the key
    and the daemon re-applies its dirty refusal.
    */
    if object.get("force").and_then(serde_json::Value::as_bool) == Some(true) {
        shaped.insert("force".to_string(), serde_json::Value::Bool(true));
    }
    Some(serde_json::Value::Object(shaped))
}

pub(crate) fn gpui_remote_sidebar_agent_id_allowed(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

/*
The optional first prompt is real user prose, so newlines and tabs are legal
where every other bridged string forbids control characters. Everything else in
the control range is still rejected, and the length is bounded so this endpoint
cannot become a bulk channel into the remote daemon.
*/
pub(crate) fn gpui_remote_sidebar_first_prompt_allowed(value: &str) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= 4_000
        && !value
            .chars()
            .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
}

pub(crate) fn gpui_remote_sidebar_worktree_path_allowed(value: &str) -> bool {
    value.starts_with('/')
        && value.len() <= 1_024
        && !value.chars().any(char::is_control)
        && !value.contains("..")
}

pub(crate) fn gpui_remote_sidebar_checkout_project_new_branch_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let branch_label = object
        .get("branchLabel")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_bounded_text_label_allowed(value))?;
    Some(serde_json::json!({
        "branchLabel": branch_label,
        "projectId": project_id,
    }))
}

pub(crate) fn gpui_remote_sidebar_project_id_allowed(value: &str) -> bool {
    let bytes = value.as_bytes();
    (2..=32).contains(&bytes.len())
        && bytes[0] == b'P'
        && bytes[1].is_ascii_digit()
        && bytes[2..]
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

/*
CDXC:GPUIRemoteWorkspaceProjectKey 2026-07-30:
The Agents workspace, presentation focus state, and parked shell-state models
key projects by either a raw local gxserver project id or a machine-scoped
remote project id. Both shapes are opaque workspace keys; everything that
persists or validates a workspace project key must accept both, or remote
workspaces are silently dropped and focus snapshots swap to bogus projects.
*/
pub(crate) fn gpui_remote_sidebar_worktree_key_allowed(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

pub(crate) fn gpui_remote_sidebar_git_ref_allowed(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphanumeric())
        && value.len() <= 200
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '/' | '-'))
        && !value.contains("..")
        && !value.contains("//")
        && !value.ends_with('/')
}

pub(crate) fn gpui_remote_sidebar_slug_label_allowed(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value
            .bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
}

pub(crate) fn gpui_remote_sidebar_bounded_text_label_allowed(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= 160
        && !value.contains('\0')
        && !value.chars().any(char::is_control)
}

pub(crate) fn gpui_remote_sidebar_git_preferences_update_payload(
    source: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    let confirm_commit = json_bool_field(source, "confirmCommit")?;
    let generate_commit_body = json_bool_field(source, "generateCommitBody")?;
    let primary_action = json_string_field(source, "primaryAction")
        .filter(|value| gpui_remote_sidebar_git_action_allowed(*value))?;
    Some(serde_json::json!({
        "confirmCommit": confirm_commit,
        "generateCommitBody": generate_commit_body,
        "primaryAction": primary_action,
    }))
}

pub(crate) fn gpui_remote_sidebar_request_refreshes_presentation(path: &str) -> bool {
    !matches!(
        path,
        "/api/listPreviousSessions"
            | "/api/readAgentHookStatus"
            | "/api/installAgentHooks"
            | "/api/listRecentProjects"
            | "/api/listProjectWorktrees"
            | "/api/readPresentationSnapshot"
            | "/api/checkoutProjectNewBranch"
            | "/api/runGitAction"
            | "/api/runGitHubAction"
            | "/api/runBeadsAction"
            | "/api/generateCommitMessage"
            | "/api/createPullRequest"
    )
}

pub(crate) fn gpui_remote_sidebar_response_payload(
    path: &str,
    result: serde_json::Value,
) -> serde_json::Value {
    /*
    CDXC:GPUIRemoteSessions 2026-06-24-17:19:
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
        "/api/readAgentHookStatus" | "/api/installAgentHooks" => {
            gpui_remote_sidebar_agent_hook_status_response_payload(result)
        }
        "/api/scheduleDelayedSend" => serde_json::json!({}),
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
        "/api/updateProject"
        | "/api/closeProjectToRecent"
        | "/api/restoreRecentProject"
        | "/api/removeRecentProject"
        | "/api/removeProject"
        | "/api/createProjectWorktree"
        | "/api/openProjectWorktree" => gpui_remote_sidebar_project_response_payload(result),
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
CDXC:ExportTranscript 2026-08-20:
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
CDXC:SidebarV2LogicalProjects 2026-07-29:
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
    CDXC:GPUIRemotePreviousSessions 2026-06-24-17:19:
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
    CDXC:RemoteWorktrees 2026-06-24-18:40:
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

pub(crate) fn gpui_remote_sidebar_recent_projects_value(
    result: &serde_json::Value,
) -> Option<serde_json::Value> {
    let rows = result.get("recentProjects")?.as_array()?;
    Some(serde_json::Value::Array(
        rows.iter()
            .filter_map(|row| row.as_object())
            .filter_map(gpui_remote_sidebar_recent_project_payload)
            .collect(),
    ))
}

pub(crate) fn gpui_remote_sidebar_recent_project_payload(
    project: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    let project_id = json_string_field(project, "projectId")?;
    let title = json_string_field(project, "title")?;
    let path = json_string_field(project, "path")?;
    let mut output = serde_json::Map::new();
    output.insert("path".to_string(), serde_json::json!(path));
    output.insert("projectId".to_string(), serde_json::json!(project_id));
    output.insert("title".to_string(), serde_json::json!(title));
    if let Some(icon) = project.get("icon").and_then(serde_json::Value::as_object) {
        output.insert("icon".to_string(), serde_json::Value::Object(icon.clone()));
    }
    if let Some(icon_data_url) = json_string_field(project, "iconDataUrl") {
        output.insert("iconDataUrl".to_string(), serde_json::json!(icon_data_url));
    }
    if let Some(recent_closed_at) = json_string_field(project, "recentClosedAt") {
        output.insert(
            "recentClosedAt".to_string(),
            serde_json::json!(recent_closed_at),
        );
    }
    if let Some(session_count) = json_u64_field(project, "sessionCount") {
        output.insert("sessionCount".to_string(), serde_json::json!(session_count));
    }
    if let Some(theme) = json_string_field(project, "theme") {
        output.insert("theme".to_string(), serde_json::json!(theme));
    }
    if let Some(theme_color) = json_string_field(project, "themeColor") {
        output.insert("themeColor".to_string(), serde_json::json!(theme_color));
    }
    Some(serde_json::Value::Object(output))
}

pub(crate) fn gpui_remote_sidebar_presentation_project_payload(
    project: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    /*
    CDXC:GPUIRemoteProjects 2026-06-24-18:22:
    Remote project mutations may return only presentation-shaped project metadata plus sanitized Git preferences. Strip raw domain-only state such as custom commands, agents, launch settings, notifications, history, and board config before CEF receives the response.
    */
    let project_id = json_string_field(project, "projectId")?;
    let title = json_string_field(project, "title")
        .or_else(|| json_string_field(project, "name"))
        .unwrap_or("Project");
    let created_at = json_string_field(project, "createdAt").unwrap_or("");
    let updated_at = json_string_field(project, "updatedAt").unwrap_or(created_at);
    let sort_key = json_string_field(project, "sortKey").unwrap_or(updated_at);
    let mut output = serde_json::Map::new();
    output.insert("createdAt".to_string(), serde_json::json!(created_at));
    output.insert(
        "groupIds".to_string(),
        serde_json::json!(
            json_array_field(project, "groupIds")
                .cloned()
                .unwrap_or_default()
        ),
    );
    if let Some(git_config) = project
        .get("gitConfig")
        .and_then(serde_json::Value::as_object)
        .and_then(gpui_remote_sidebar_git_config_payload)
    {
        output.insert("gitConfig".to_string(), git_config);
    }
    output.insert(
        "isFavorite".to_string(),
        serde_json::json!(json_bool_field(project, "isFavorite").unwrap_or(false)),
    );
    output.insert(
        "isPinned".to_string(),
        serde_json::json!(json_bool_field(project, "isPinned").unwrap_or(false)),
    );
    if let Some(path) = json_string_field(project, "path") {
        output.insert("path".to_string(), serde_json::json!(path));
    }
    output.insert("projectId".to_string(), serde_json::json!(project_id));
    output.insert("sortKey".to_string(), serde_json::json!(sort_key));
    output.insert("title".to_string(), serde_json::json!(title));
    output.insert("updatedAt".to_string(), serde_json::json!(updated_at));
    if let Some(worktree) = project
        .get("worktree")
        .and_then(serde_json::Value::as_object)
    {
        output.insert(
            "worktree".to_string(),
            serde_json::Value::Object(worktree.clone()),
        );
    }
    Some(serde_json::Value::Object(output))
}

pub(crate) fn gpui_remote_sidebar_git_config_payload(
    source: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    let mut output = serde_json::Map::new();
    if let Some(confirm_commit) = json_bool_field(source, "confirmCommit") {
        output.insert(
            "confirmCommit".to_string(),
            serde_json::json!(confirm_commit),
        );
    }
    if let Some(generate_commit_body) = json_bool_field(source, "generateCommitBody") {
        output.insert(
            "generateCommitBody".to_string(),
            serde_json::json!(generate_commit_body),
        );
    }
    if let Some(primary_action) = json_string_field(source, "primaryAction")
        .filter(|value| gpui_remote_sidebar_git_action_allowed(*value))
    {
        output.insert(
            "primaryAction".to_string(),
            serde_json::json!(primary_action),
        );
    }
    (!output.is_empty()).then(|| serde_json::Value::Object(output))
}

pub(crate) fn gpui_remote_sidebar_git_action_allowed(value: &str) -> bool {
    matches!(
        value,
        "commit" | "push" | "pr" | "syncRemote" | "syncMain" | "multiRelease" | "release"
    )
}

pub(crate) fn gpui_remote_sidebar_typed_operation_response_payload(
    mut result: serde_json::Value,
) -> serde_json::Value {
    if let Some(object) = result.as_object_mut() {
        object.remove("command");
    }
    result
}

pub(crate) fn gpui_remote_sidebar_generate_commit_message_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    let mut response = serde_json::Map::new();
    if let Some(subject) = result.get("subject").and_then(serde_json::Value::as_str) {
        response.insert("subject".to_string(), serde_json::json!(subject));
    }
    if let Some(body) = result.get("body").and_then(serde_json::Value::as_str) {
        response.insert("body".to_string(), serde_json::json!(body));
    }
    serde_json::Value::Object(response)
}

pub(crate) fn gpui_remote_sidebar_create_pull_request_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    let mut response = serde_json::Map::new();
    if let Some(ok) = result.get("ok").and_then(serde_json::Value::as_bool) {
        response.insert("ok".to_string(), serde_json::json!(ok));
    }
    if let Some(created) = result.get("created").and_then(serde_json::Value::as_bool) {
        response.insert("created".to_string(), serde_json::json!(created));
    }
    if let Some(reason) = result.get("reason").and_then(serde_json::Value::as_str) {
        response.insert("reason".to_string(), serde_json::json!(reason));
    }
    if let Some(pr) = result.get("pr").and_then(serde_json::Value::as_object) {
        let mut sanitized_pr = serde_json::Map::new();
        if let Some(state) = pr.get("state").and_then(serde_json::Value::as_str) {
            sanitized_pr.insert("state".to_string(), serde_json::json!(state));
        }
        if let Some(number) = pr.get("number").and_then(serde_json::Value::as_u64) {
            sanitized_pr.insert("number".to_string(), serde_json::json!(number));
        }
        if !sanitized_pr.is_empty() {
            response.insert("pr".to_string(), serde_json::Value::Object(sanitized_pr));
        }
    }
    serde_json::Value::Object(response)
}

pub(crate) fn gpui_remote_sidebar_delete_worktree_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    let warnings = result
        .get("warnings")
        .and_then(serde_json::Value::as_array)
        .map(|warnings| {
            warnings
                .iter()
                .filter_map(|warning| {
                    let kind = warning.get("kind").and_then(serde_json::Value::as_str)?;
                    Some(serde_json::json!({ "kind": kind }))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut response = serde_json::Map::new();
    response.insert("warnings".to_string(), serde_json::Value::Array(warnings));
    serde_json::Value::Object(response)
}
