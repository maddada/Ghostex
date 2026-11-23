//! Workspace routes (crate::workspaces): read, create, update and delete workspaces, and move a
//! project into one.

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use crate::{
    audit,
    constants::GXSERVER_PROTOCOL_VERSION,
    domain::{read_domain_rpc_params, DomainRepository, DomainStateError},
    presentation::increment_presentation_revision,
    protocol::rpc_error,
    security::Principal,
    sidebar_project_collections::{
        read_sidebar_project_collections, update_sidebar_project_collections,
    },
    sidebar_spaces::rehome_sidebar_spaces_for_workspaces,
    work_mode::{store_linear_api_key, LinearKeyScope},
    workspaces::{
        assign_machine_workspace_in, assign_remote_project_workspace_in, create_workspace_in,
        delete_workspace_in, delete_workspace_instructions_file, find_workspace_id,
        move_project_to_workspace, read_sidebar_workspaces, read_workspace_instructions_file,
        reapply_workspace_defaults, resolve_project_reference, update_workspace_in,
        workspace_name, write_sidebar_workspaces, write_workspace_instructions_file,
        DEFAULT_WORKSPACE_ID, MAX_WORKSPACE_INSTRUCTIONS_BYTES,
    },
};

use super::super::work_mode_sync::{publish_project_work_mode_change, spawn_work_mode_refresh};
use super::*;

const WORKSPACE_WRITE_REFUSED: &str = "Only the user can change workspaces: in Settings > Workspaces, on your phone, or from a terminal outside Ghostex (agent sessions and Ghostex terminal panes can't modify workspaces).";

fn workspace_write_gate(
    endpoint_path: &str,
    request_id: &str,
    caller: &Principal,
    body_json: &Value,
) -> Option<RoutedResponse> {
    let allowed = caller.is_user_device()
        && !(matches!(caller, Principal::LocalAgent)
            && crate::audit::token_file_names_session());
    if allowed {
        return None;
    }
    let (kind, summary) = match endpoint_path {
        "/api/createWorkspace" => ("workspace.create", "Refused creating a workspace"),
        "/api/updateWorkspace" => ("workspace.update", "Refused updating a workspace"),
        "/api/deleteWorkspace" => ("workspace.delete", "Refused deleting a workspace"),
        "/api/writeWorkspaceInstructions" => (
            "workspace.instructions.write",
            "Refused changing workspace instructions",
        ),
        "/api/moveProjectToWorkspace" => (
            "workspace.moveProject",
            "Refused moving a project between workspaces",
        ),
        "/api/moveMachineToWorkspace" => (
            "workspace.moveMachine",
            "Refused moving a computer between workspaces",
        ),
        "/api/moveRemoteProjectToWorkspace" => (
            "workspace.moveRemoteProject",
            "Refused moving a remote project between workspaces",
        ),
        _ => ("workspace.write", "Refused changing a workspace"),
    };
    let target = read_domain_rpc_params(body_json)
        .ok()
        .and_then(|params| {
            params
                .get("workspaceId")
                .or_else(|| params.get("name"))
                .or_else(|| params.get("projectId"))
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    let mut action = audit::Action::new(kind, summary);
    if let Some(target) = target {
        action = action.target(target);
    }
    audit::record(
        action,
        audit::Actor::current(),
        audit::Outcome::Refused(WORKSPACE_WRITE_REFUSED.to_string()),
        None,
    );
    Some(routed_json(
        Some(endpoint_path.to_string()),
        StatusCode::FORBIDDEN,
        rpc_error(
            "forbidden",
            WORKSPACE_WRITE_REFUSED,
            Some(request_id.to_string()),
        ),
    ))
}

fn workspace_instructions_read_gate(
    state: &AppState,
    endpoint_path: &str,
    request_id: &str,
    caller: &Principal,
) -> Option<RoutedResponse> {
    let refused_reason = match caller {
        caller if caller.is_user_device() => None,
        Principal::Session {
            project_id,
            session_id,
            ..
        } => {
            let is_api_chat = crate::storage::open_gxserver_database(&state.paths)
                .ok()
                .and_then(|db| {
                    DomainRepository::new(&db, &state.metadata.server_id)
                        .get_session(project_id, session_id)
                        .ok()
                        .flatten()
                })
                .is_some_and(|session| {
                    crate::agents::is_gbot_api_chat(
                        session.get("runtimeSettings").and_then(Value::as_object),
                    )
                });
            is_api_chat.then_some("GBot API chats cannot read workspace instructions.")
        }
        _ => Some("Only your own devices or sessions can read workspace instructions."),
    }?;
    audit::record(
        audit::Action::new(
            "workspace.instructions.read",
            "Refused reading workspace instructions",
        ),
        audit::Actor::from(caller),
        audit::Outcome::Refused(refused_reason.to_string()),
        None,
    );
    Some(routed_json(
        Some(endpoint_path.to_string()),
        StatusCode::FORBIDDEN,
        rpc_error("forbidden", refused_reason, Some(request_id.to_string())),
    ))
}

pub(super) async fn route_workspaces_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    let RouteHttpRequest {
        state,
        endpoint,
        request_id,
        body_json,
        token_extension_id,
        caller,
    } = request;
    Ok(match endpoint.path.as_str() {
        "/api/readWorkspaces" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |_, db, _, _| {
                read_sidebar_workspaces(db)
                    .map(|workspaces| json!({ "sidebarWorkspaces": workspaces }))
            },
        ),
        "/api/createWorkspace" => {
            if let Some(refused) =
                workspace_write_gate(&endpoint.path, &request_id, &caller, &body_json)
            {
                return Ok(refused);
            }
            handle_domain_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
                |_, db, params, _| {
                    let _event_sequence = lock_presentation_event_sequence(&state)?;
                    let current = read_sidebar_workspaces(db)?;
                    refuse_other_trackers(params)?;
                    let (workspace_id, next) = create_workspace_in(&current, params)?;
                    let workspaces = write_sidebar_workspaces(db, &next)?;
                    broadcast_workspaces(&state, db, &workspaces)?;
                    Ok(json!({ "workspaceId": workspace_id, "sidebarWorkspaces": workspaces }))
                },
            )
        }
        "/api/updateWorkspace" => {
            if let Some(refused) =
                workspace_write_gate(&endpoint.path, &request_id, &caller, &body_json)
            {
                return Ok(refused);
            }
            let response = handle_domain_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
                |_, db, params, _| {
                    let _event_sequence = lock_presentation_event_sequence(&state)?;
                    let current = read_sidebar_workspaces(db)?;
                    let workspace_id = required_workspace(&current, params)?;
                    refuse_other_trackers(params)?;
                    let next = update_workspace_in(&current, &workspace_id, params)?;
                    let workspaces = write_sidebar_workspaces(db, &next)?;
                    if workspaces != current {
                        broadcast_workspaces(&state, db, &workspaces)?;
                    }
                    Ok(json!({ "workspaceId": workspace_id, "sidebarWorkspaces": workspaces }))
                },
            );
            spawn_work_mode_refresh(&state);
            response
        }
        "/api/deleteWorkspace" => {
            if let Some(refused) =
                workspace_write_gate(&endpoint.path, &request_id, &caller, &body_json)
            {
                return Ok(refused);
            }
            let response = handle_domain_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
                |repository, db, params, _| {
                    let (workspaces, workspace_id) = {
                        let _event_sequence = lock_presentation_event_sequence(&state)?;
                        let current = read_sidebar_workspaces(db)?;
                        let workspace_id = required_workspace(&current, params)?;
                        // First, so a workspace that can't be deleted (the default one) keeps its
                        // ticket command.
                        let next = delete_workspace_in(&current, &workspace_id)?;
                        // Its ticket command and that command's credentials go with it, like its
                        // Linear key, whichever client deletes it (the decision is on
                        // `delete_workspace_work_source`). The command record and the workspaces
                        // document change in one transaction, and the credentials file is put back
                        // on any failure, so an error leaves the workspace, its command and its
                        // credentials as they were, with nothing broadcast.
                        let transaction = db.unchecked_transaction().map_err(sql_error)?;
                        let references =
                            || crate::work_mode::referenced_source_ids(repository, &current);
                        let deletion = crate::work_mode::delete_workspace_work_source(
                            db,
                            &state.paths,
                            &workspace_id,
                            &references,
                        )
                        .map_err(work_source_error)?;
                        let written = write_sidebar_workspaces(db, &next).and_then(|workspaces| {
                            transaction.commit().map_err(sql_error)?;
                            Ok(workspaces)
                        });
                        let workspaces = match written {
                            Ok(workspaces) => {
                                deletion.commit();
                                workspaces
                            }
                            Err(error) => return Err(undone(deletion, &state.paths, error)),
                        };
                        broadcast_workspaces(&state, db, &workspaces)?;
                        if let Some(spaces) =
                            rehome_sidebar_spaces_for_workspaces(db, Some(&workspace_id), &[])?
                        {
                            broadcast_spaces(&state, db, spaces)?;
                        }
                        (workspaces, workspace_id)
                    };
                    // Its projects fall back to the default workspace and take its default.
                    let changed =
                        reapply_workspace_defaults(repository, &workspaces, DEFAULT_WORKSPACE_ID)?;
                    crate::work_mode::replace_project_workspaces(&state.paths, repository, db)?;
                    publish_projects(&state, db, repository, &changed)?;
                    let _ = store_linear_api_key(
                        &state.paths,
                        LinearKeyScope::Workspace(&workspace_id),
                        None,
                    );
                    delete_workspace_instructions_file(&state.paths, &workspace_id);
                    Ok(json!({ "workspaceId": workspace_id, "sidebarWorkspaces": workspaces }))
                },
            );
            spawn_work_mode_refresh(&state);
            response
        }
        "/api/readWorkspaceInstructions" => {
            if let Some(refused) =
                workspace_instructions_read_gate(&state, &endpoint.path, &request_id, &caller)
            {
                return Ok(refused);
            }
            handle_domain_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
                |_, db, params, _| {
                    let workspaces = read_sidebar_workspaces(db)?;
                    if params.contains_key("workspaceId") {
                        let workspace_id = required_workspace(&workspaces, params)?;
                        let (content, path, exists, truncated) =
                            read_workspace_instructions_file(&state.paths, &workspace_id)?;
                        return Ok(json!({
                            "workspaceId": workspace_id,
                            "name": workspace_name(&workspaces, &workspace_id),
                            "content": content,
                            "path": path.to_string_lossy(),
                            "exists": exists,
                            "truncated": truncated,
                            "maxBytes": MAX_WORKSPACE_INSTRUCTIONS_BYTES,
                        }));
                    }
                    let mut entries = Map::new();
                    for workspace_id in workspaces
                        .get("order")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                    {
                        let (content, path, exists, truncated) =
                            read_workspace_instructions_file(&state.paths, workspace_id)?;
                        entries.insert(
                            workspace_id.to_string(),
                            json!({
                                "workspaceId": workspace_id,
                                "name": workspace_name(&workspaces, workspace_id),
                                "content": content,
                                "path": path.to_string_lossy(),
                                "exists": exists,
                                "truncated": truncated,
                                "maxBytes": MAX_WORKSPACE_INSTRUCTIONS_BYTES,
                            }),
                        );
                    }
                    Ok(json!({
                        "workspaces": entries,
                        "maxBytes": MAX_WORKSPACE_INSTRUCTIONS_BYTES,
                    }))
                },
            )
        }
        "/api/writeWorkspaceInstructions" => {
            if let Some(refused) =
                workspace_write_gate(&endpoint.path, &request_id, &caller, &body_json)
            {
                return Ok(refused);
            }
            handle_domain_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
                |_, db, params, _| {
                    let workspaces = read_sidebar_workspaces(db)?;
                    let workspace_id = required_workspace(&workspaces, params)?;
                    let content = params
                        .get("content")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let (saved, path, exists) = write_workspace_instructions_file(
                        &state.paths,
                        &workspace_id,
                        content,
                    )?;
                    Ok(json!({
                        "workspaceId": workspace_id,
                        "name": workspace_name(&workspaces, &workspace_id),
                        "content": saved,
                        "path": path.to_string_lossy(),
                        "exists": exists,
                        "truncated": false,
                        "maxBytes": MAX_WORKSPACE_INSTRUCTIONS_BYTES,
                    }))
                },
            )
        }
        "/api/moveProjectToWorkspace" => {
            if let Some(refused) =
                workspace_write_gate(&endpoint.path, &request_id, &caller, &body_json)
            {
                return Ok(refused);
            }
            let response = handle_domain_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
                |repository, db, params, _| {
                    let project_id = match required_text(params, "projectId") {
                        Ok(project_id) => project_id,
                        // `ghostex workspace move-project` names the project by id, name or folder.
                        Err(error) => match params.get("project").and_then(Value::as_str) {
                            Some(reference) => resolve_project_reference(repository, reference)?,
                            None => return Err(error),
                        },
                    };
                    let workspaces = read_sidebar_workspaces(db)?;
                    let workspace_id = required_workspace(&workspaces, params)?;
                    // Placed before they publish, so their rows show the new workspace's ticket
                    // command; and under the ticket command store's writer lock, which a
                    // background read holds from its placement check until its command starts.
                    let changed = crate::work_mode::place_projects_after(
                        &state.paths,
                        repository,
                        db,
                        || {
                            move_project_to_workspace(
                                repository,
                                &workspaces,
                                &project_id,
                                &workspace_id,
                            )
                        },
                    )?;
                    if !changed.is_empty() {
                        leave_old_workspace_groups(&state, db, &changed)?;
                    }
                    publish_projects(&state, db, repository, &changed)?;
                    Ok(json!({
                        "projectId": project_id,
                        "workspaceId": workspace_id,
                        "movedProjectIds": changed,
                    }))
                },
            );
            spawn_work_mode_refresh(&state);
            response
        }
        "/api/moveMachineToWorkspace" => {
            if let Some(refused) =
                workspace_write_gate(&endpoint.path, &request_id, &caller, &body_json)
            {
                return Ok(refused);
            }
            handle_domain_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
                |_, db, params, _| {
                    let machine_id = required_text(params, "machineId")?;
                    let _event_sequence = lock_presentation_event_sequence(&state)?;
                    let current = read_sidebar_workspaces(db)?;
                    let workspace_id = required_workspace(&current, params)?;
                    let next = assign_machine_workspace_in(&current, &machine_id, &workspace_id)?;
                    let workspaces = write_sidebar_workspaces(db, &next)?;
                    if workspaces != current {
                        broadcast_workspaces(&state, db, &workspaces)?;
                    }
                    Ok(json!({
                        "machineId": machine_id,
                        "workspaceId": workspace_id,
                        "sidebarWorkspaces": workspaces,
                    }))
                },
            )
        }
        "/api/moveRemoteProjectToWorkspace" => {
            if let Some(refused) =
                workspace_write_gate(&endpoint.path, &request_id, &caller, &body_json)
            {
                return Ok(refused);
            }
            handle_domain_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
                |_, db, params, _| {
                    let machine_id = required_text(params, "machineId")?;
                    let project_id = required_text(params, "projectId")?;
                    let _event_sequence = lock_presentation_event_sequence(&state)?;
                    let current = read_sidebar_workspaces(db)?;
                    let workspace_id = required_workspace(&current, params)?;
                    let next = assign_remote_project_workspace_in(
                        &current,
                        &machine_id,
                        &project_id,
                        &workspace_id,
                    )?;
                    let workspaces = write_sidebar_workspaces(db, &next)?;
                    if workspaces != current {
                        broadcast_workspaces(&state, db, &workspaces)?;
                    }
                    Ok(json!({
                        "machineId": machine_id,
                        "projectId": project_id,
                        "workspaceId": workspace_id,
                        "sidebarWorkspaces": workspaces,
                    }))
                },
            )
        }
        _ => {
            return Err(RouteHttpRequest {
                state,
                endpoint,
                request_id,
                body_json,
                token_extension_id,
                caller,
            })
        }
    })
}

/// The generic workspace writes, which remote clients may call, set only a built-in tracker. A
/// workspace's ticket command is set and chosen on its own computer (the decision on the
/// work-source endpoints in `protocol.rs`), so a command id here is refused instead of dropped.
/// The check is these routes' own, not `update_workspace_in`'s, which the local-only
/// `/api/setWorkTracker` also writes through.
fn refuse_other_trackers(params: &Map<String, Value>) -> Result<(), DomainStateError> {
    match params.get("tracker") {
        None | Some(Value::Null) => Ok(()),
        Some(tracker) if matches!(tracker.as_str().map(str::trim), Some("linear" | "github")) => {
            Ok(())
        }
        Some(_) => Err(DomainStateError::bad_request(
            "tracker must be \"linear\" or \"github\"; a ticket command is set and picked on its own computer.",
        )),
    }
}

/// A moved project leaves its project group and the Spaces that list it, which belong to the
/// workspace it left, so it shows in the new workspace's Other view.
///
/// CDXC:Workspaces 2026-10-09 DECISION:
/// User (mockup 07): moving a project takes it out of this workspace's Spaces and puts it in the
/// other workspace's "Other" group, where you can add it to a Space; its sessions, worktrees and
/// links move with it.
fn leave_old_workspace_groups(
    state: &AppState,
    db: &rusqlite::Connection,
    moved_project_ids: &[String],
) -> Result<(), DomainStateError> {
    let _event_sequence = lock_presentation_event_sequence(state)?;
    let previous_collections = read_sidebar_project_collections(db)?;
    let mut next = previous_collections.clone();
    if let Some(collections) = next.get_mut("collections").and_then(Value::as_object_mut) {
        for collection in collections.values_mut() {
            if let Some(project_ids) = collection
                .get_mut("projectIds")
                .and_then(Value::as_array_mut)
            {
                project_ids.retain(|id| {
                    !id.as_str()
                        .is_some_and(|id| moved_project_ids.iter().any(|moved| moved == id))
                });
            }
        }
    }
    if next != previous_collections {
        let mut params = Map::new();
        params.insert("state".to_string(), next);
        let collections = update_sidebar_project_collections(db, &params)?;
        let revision = increment_presentation_revision(db)?;
        state.event_hub.broadcast(json!({
            "protocolVersion": GXSERVER_PROTOCOL_VERSION,
            "revision": revision,
            "serverId": state.metadata.server_id.clone(),
            "sidebarProjectCollections": collections.clone(),
            "type": "sidebarProjectCollectionsChanged",
        }));
        broadcast_pruned_sidebar_spaces(state, db, &previous_collections, &collections)?;
    }
    if let Some(spaces) = rehome_sidebar_spaces_for_workspaces(db, None, moved_project_ids)? {
        broadcast_spaces(state, db, spaces)?;
    }
    Ok(())
}

/// Publishes each changed project and its sessions (their cards follow the project's Work mode).
fn publish_projects(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    project_ids: &[String],
) -> Result<(), DomainStateError> {
    for project_id in project_ids {
        publish_project_work_mode_change(state, db, repository, project_id)?;
    }
    Ok(())
}

/// Callers hold the presentation event sequencer. Nothing is sent while the Workspaces built-in
/// extension is off: clients then hold no workspaces document, and one frame would bring the tile
/// back.
fn broadcast_workspaces(
    state: &AppState,
    db: &rusqlite::Connection,
    workspaces: &Value,
) -> Result<(), DomainStateError> {
    if !crate::workspaces::workspaces_feature_enabled() {
        return Ok(());
    }
    let revision = increment_presentation_revision(db)?;
    state.event_hub.broadcast(json!({
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "revision": revision,
        "serverId": state.metadata.server_id.clone(),
        "sidebarWorkspaces": workspaces.clone(),
        "type": "sidebarWorkspacesChanged",
    }));
    Ok(())
}

/// Callers hold the presentation event sequencer.
fn broadcast_spaces(
    state: &AppState,
    db: &rusqlite::Connection,
    spaces: Value,
) -> Result<(), DomainStateError> {
    let revision = increment_presentation_revision(db)?;
    state.event_hub.broadcast(json!({
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "revision": revision,
        "serverId": state.metadata.server_id.clone(),
        "sidebarSpaces": spaces,
        "sidebarSpacesEnabled": crate::sidebar_spaces::read_sidebar_spaces_enabled(&state.paths),
        "type": "sidebarSpacesChanged",
    }));
    Ok(())
}

/// `workspaceId` as a workspace id or name (the CLI passes names), resolved against `workspaces`.
fn required_workspace(
    workspaces: &Value,
    params: &Map<String, Value>,
) -> Result<String, DomainStateError> {
    let reference = required_text(params, "workspaceId")?;
    find_workspace_id(workspaces, &reference)
        .ok_or_else(|| DomainStateError::bad_request(crate::workspaces::NO_SUCH_WORKSPACE))
}

fn required_text(params: &Map<String, Value>, key: &str) -> Result<String, DomainStateError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| DomainStateError::bad_request(format!("Pass {key}.")))
}

fn sql_error(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite workspaces error: {error}"),
    }
}

fn work_source_error(error: crate::work_mode::WorkSourceError) -> DomainStateError {
    match error {
        crate::work_mode::WorkSourceError::Storage(error) => error,
        crate::work_mode::WorkSourceError::Refused(text) => DomainStateError::bad_request(text),
        crate::work_mode::WorkSourceError::Failed(_) => {
            DomainStateError::bad_request("Could not remove its ticket command.")
        }
    }
}

/// `error`, after putting back the credentials a failed deletion removed (the transaction it was
/// in rolled the command record back already).
fn undone(
    deletion: crate::work_mode::WorkSourceDeletion,
    paths: &crate::paths::GxserverPaths,
    error: DomainStateError,
) -> DomainStateError {
    match deletion.undo(paths) {
        Ok(()) => error,
        Err(undo_error) => DomainStateError {
            code: "internalError",
            message: format!(
                "{} (and its ticket command's credentials could not be put back: {undo_error})",
                error.message
            ),
        },
    }
}
