//! Project routes: create, update, park and restore projects, project worktrees, bot projects, the add-project folder browser, repository clones and source-control lookups, project docs and project views.

use axum::http::StatusCode;
use serde_json::json;

use crate::{
    domain::{read_project_id, DomainStateError},
    protocol::rpc_success,
};

use super::*;

/// CDXC:AgentLauncher 2026-10-23 DECISION: User: "we already agreed long time ago 'no more chats'". There are no projectless Chats: nothing creates `~/ghostex/chats` workspaces any more (Quick Terminal, `ghostex create-chat` and `/api/createQuickProject` were removed), and projects left in that folder are ordinary projects. A session that needs a folder of its own runs in the Ghostex config folder's project, as Help and Send Bug / Feedback do.
pub(super) async fn route_projects_http(
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
        "/api/createProject" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let project = repository.create_project(params)?;
                let project =
                    crate::workspaces::place_added_project(repository, db, project, params)?;
                let project_id = value_text(&project, "projectId")?;
                schedule_presentation_project_delta(
                    &state,
                    db,
                    repository,
                    &project_id,
                    "projectAdded",
                )?;
                Ok(json!({ "project": project }))
            },
        ),
        "/api/updateProject" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                // A project's workspace is its `launchSettings.workspaceId`, or a worktree's
                // parent's (`worktree.parentProjectId`): an update that sets either is placed like a
                // move, under the ticket command store's writer lock (`place_projects_after`).
                let project = if params.contains_key("launchSettings")
                    || params.contains_key("worktree")
                {
                    crate::work_mode::place_projects_after(&state.paths, repository, db, || {
                        repository.update_project(params)
                    })?
                } else {
                    repository.update_project(params)?
                };
                let project_id = value_text(&project, "projectId")?;
                schedule_presentation_project_delta(
                    &state,
                    db,
                    repository,
                    &project_id,
                    "projectUpdated",
                )?;
                Ok(json!({ "project": project }))
            },
        ),
        "/api/relocateProject" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let project = repository.relocate_project(params)?;
                let project_id = value_text(&project, "projectId")?;
                schedule_presentation_project_delta(
                    &state,
                    db,
                    repository,
                    &project_id,
                    "projectUpdated",
                )?;
                Ok(json!({ "project": project }))
            },
        ),
        "/api/listProjects" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _, _, _| {
                repository
                    .list_projects()
                    .map(|projects| json!({ "projects": projects }))
            },
        ),
        "/api/listRecentProjects" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _, _, _| {
                repository
                    .list_recent_projects()
                    .map(|recent_projects| json!({ "recentProjects": recent_projects }))
            },
        ),
        "/api/closeProjectToRecent" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                /*
                CDXC:Projects 2026-06-24-12:38:
                GPUI's reused SidebarApp sends the same close-vs-remove command split as macOS. Close parks the canonical gxserver project with a server timestamp and broadcasts a presentation removal for active groups; remove remains the hard-delete endpoint.
                */
                let project_id = read_project_id(params)?;
                let project = repository.close_project_to_recent(&project_id)?;
                schedule_presentation_project_delta(
                    &state,
                    db,
                    repository,
                    &project_id,
                    "projectUpdated",
                )?;
                let recent_projects = repository.list_recent_projects()?;
                Ok(json!({ "project": project, "recentProjects": recent_projects }))
            },
        ),
        "/api/restoreRecentProject" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                /*
                CDXC:Projects 2026-06-24-12:27:
                GPUI restores Recent Projects through gxserver project ids.
                The daemon clears explicit parked state and publishes the
                project presentation update; clients must not reconstruct rows
                from labels, stopped sessions, trusted client paths, or command
                output.
                */
                let project_id = read_project_id(params)?;
                let project = repository.restore_recent_project(&project_id)?;
                schedule_presentation_project_delta(
                    &state,
                    db,
                    repository,
                    &project_id,
                    "projectUpdated",
                )?;
                let recent_projects = repository.list_recent_projects()?;
                Ok(json!({ "project": project, "recentProjects": recent_projects }))
            },
        ),
        "/api/removeRecentProject" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let project_id = read_project_id(params)?;
                let project = repository.remove_recent_project(&project_id)?;
                schedule_presentation_project_delta(
                    &state,
                    db,
                    repository,
                    &project_id,
                    "projectRemoved",
                )?;
                let recent_projects = repository.list_recent_projects()?;
                Ok(json!({ "project": project, "recentProjects": recent_projects }))
            },
        ),
        "/api/addProjectPath" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let project = repository.add_project_path(params)?;
                let project =
                    crate::workspaces::place_added_project(repository, db, project, params)?;
                let project_id = value_text(&project, "projectId")?;
                schedule_presentation_project_delta(
                    &state,
                    db,
                    repository,
                    &project_id,
                    "projectAdded",
                )?;
                Ok(json!({ "project": project }))
            },
        ),
        // Off the async runtime like the startup pass: the sync reads the profiles folder and
        // writes the database.
        "/api/syncBotProjects" => {
            let worker_state = state.clone();
            let worker_endpoint = endpoint.path.clone();
            let worker_request_id = request_id.clone();
            match tokio::task::spawn_blocking(move || {
                handle_domain_http(
                    &worker_state,
                    worker_endpoint,
                    worker_request_id,
                    &body_json,
                    |repository, db, _, _| {
                        bot_sync::sync_and_publish_bot_projects(&worker_state, db, repository)
                    },
                )
            })
            .await
            {
                Ok(response) => response,
                Err(error) => domain_error_response(
                    endpoint.path,
                    request_id,
                    DomainStateError::corrupt_state(format!("Bot project sync failed: {error}")),
                ),
            }
        }
        "/api/listBotFeed" => {
            let feed_state = state.clone();
            match tokio::task::spawn_blocking(move || {
                crate::bot_feed::read_bot_feed(
                    &feed_state.paths,
                    &crate::session_chat_hermes::hermes_home(),
                )
            })
            .await
            {
                Ok(feed) => routed_json(
                    Some(endpoint.path),
                    StatusCode::OK,
                    rpc_success(request_id, feed),
                ),
                Err(error) => domain_error_response(
                    endpoint.path,
                    request_id,
                    DomainStateError::corrupt_state(format!("Bot feed read failed: {error}")),
                ),
            }
        }
        "/api/listProjectWorktrees"
        | "/api/createProjectWorktree"
        | "/api/openProjectWorktree"
        | "/api/mergeWorktreeIntoMain"
        | "/api/checkoutProjectNewBranch" => {
            handle_project_worktree_operation_http(&state, endpoint.path, request_id, &body_json)
                .await
        }
        "/api/removeProject" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                // The CLI's `remove-project` passes --project-id, --path or --name, like `group-project`.
                let project_id = resolve_sidebar_collection_project_id(repository, params)?;
                // Its worktrees follow it to another workspace, so it goes like a move
                // (`place_projects_after`).
                let project =
                    crate::work_mode::place_projects_after(&state.paths, repository, db, || {
                        repository.remove_project(&project_id)
                    })?;
                crate::session_chat_queue::forget_session_chat_queue(db, &project_id, None)?;
                // Approvals letting its agents reach other projects, or others reach it, go too.
                state.security.forget_project_reach_of(&project_id);
                crate::session_chat_queue_attachments::sweep(&state.paths, db);
                schedule_presentation_project_delta(
                    &state,
                    db,
                    repository,
                    &project_id,
                    "projectUpdated",
                )?;
                Ok(json!({ "project": project }))
            },
        ),
        "/api/deleteWorktreeProject" => {
            handle_delete_worktree_project_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/renameWorktreeProject" => {
            handle_rename_worktree_project_http(&state, endpoint.path, request_id, &body_json).await
        }
        /*
        CDXC:Worktrees 2026-07-29-00:00:
        Sidebar V2's worktree flow. Unlike `createProjectWorktree`, these
        endpoints never register the worktree as a project: the checkout is an
        attribute of one session (its cwd), and the branch shown on the card
        comes from the per-session git probe reading that cwd.
        */
        "/api/createWorktreeSession" | "/api/removeSessionWorktree" => {
            handle_worktree_session_http(&state, endpoint.path, request_id, &body_json).await
        }
        // A new chat's worktree tray (worktree_ops/draft_worktree.rs).
        "/api/draftWorktree" => {
            handle_draft_worktree_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/runProjectDocsAction" => {
            project_docs_http::handle(state.clone(), endpoint.path, request_id, body_json).await
        }
        "/api/searchProjectFiles" => {
            crate::project_file_search::handle_search_project_files_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
            )
            .await
        }
        "/api/previewRepositoryClone"
        | "/api/startRepositoryClone"
        | "/api/readRepositoryCloneJob"
        | "/api/cancelRepositoryCloneJob" => {
            handle_repository_clone_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/browseProjectDirectories" => {
            handle_browse_project_directories_http(&state, endpoint.path, request_id, &body_json)
        }
        "/api/createProjectDirectory" => {
            handle_create_project_directory_http(&state, endpoint.path, request_id, &body_json)
        }
        "/api/discoverSourceControl" | "/api/lookupRepository" => {
            handle_source_control_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/projectView" => {
            crate::project_views::handle(&state, endpoint.path, request_id, &body_json).await
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
