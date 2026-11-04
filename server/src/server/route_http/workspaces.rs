//! Workspace routes (crate::workspaces): read, create, update and delete workspaces, and move a
//! project into one.

use serde_json::{json, Map, Value};

use crate::{
    constants::GXSERVER_PROTOCOL_VERSION,
    domain::{DomainRepository, DomainStateError},
    presentation::increment_presentation_revision,
    sidebar_project_collections::{
        read_sidebar_project_collections, update_sidebar_project_collections,
    },
    sidebar_spaces::rehome_sidebar_spaces_for_workspaces,
    work_mode::{store_linear_api_key, LinearKeyScope},
    workspaces::{
        assign_machine_workspace_in, create_workspace_in, delete_workspace_in, find_workspace_id,
        move_project_to_workspace, read_sidebar_workspaces, reapply_workspace_defaults,
        resolve_project_reference, update_workspace_in, write_sidebar_workspaces,
        DEFAULT_WORKSPACE_ID,
    },
};

use super::super::work_mode_sync::{publish_project_work_mode_change, spawn_work_mode_refresh};
use super::*;

pub(super) async fn route_workspaces_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    let RouteHttpRequest {
        state,
        endpoint,
        request_id,
        body_json,
        token_extension_id,
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
        "/api/createWorkspace" => handle_domain_http(
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
        ),
        "/api/updateWorkspace" => {
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
                    Ok(json!({ "workspaceId": workspace_id, "sidebarWorkspaces": workspaces }))
                },
            );
            spawn_work_mode_refresh(&state);
            response
        }
        "/api/moveProjectToWorkspace" => {
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
        "/api/moveMachineToWorkspace" => handle_domain_http(
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
        ),
        _ => {
            return Err(RouteHttpRequest {
                state,
                endpoint,
                request_id,
                body_json,
                token_extension_id,
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
