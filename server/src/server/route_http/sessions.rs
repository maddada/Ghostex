//! Session routes: create, list, update, order, settle/snooze and remove sessions, the presentation snapshot, coordinators, close-after-done, keep-awake leases, previous sessions and fork branches, and session title generation.

use anyhow::Result;
use serde_json::{json, Value};

use crate::{
    agents::{apply_created_session_identity, create_agent_session_params_for_project},
    domain::{read_optional_project_id, read_project_id, read_session_id, DomainStateError},
    presentation::{list_previous_sessions, list_session_fork_branches},
    session_lifecycle,
};

use super::*;

pub(super) async fn route_sessions_http(
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
        "/api/createSession" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                // Only gxserver makes a box session (a box create, or reopening one from history).
                let mut params = crate::agentbox::without_client_agentbox_record(params);
                // A session whose folder is missing never starts (the provider refuses it, and it does
                // not expand `~`), so a new session's own `cwd` is stored as the folder it checked on
                // this computer; a restore keeps its source's.
                let restoring = ["restoredFromSessionId", "restoredFromHistoryId"]
                    .iter()
                    .any(|key| params.get(*key).is_some_and(|value| !value.is_null()));
                if let Some(cwd) = params
                    .get("cwd")
                    .filter(|cwd| cwd.as_str().is_some_and(|cwd| !cwd.trim().is_empty()))
                    .filter(|_| !restoring)
                {
                    let folder =
                        crate::domain::normalize_existing_directory_path(Some(cwd), "cwd")?;
                    params.insert("cwd".to_string(), Value::String(folder));
                }
                let params = &params;
                let created_session = repository.create_session(params, false)?;
                let session = apply_created_session_identity(repository, &created_session, params)?;
                let project_id = value_text(&session, "projectId")?;
                let session_id = value_text(&session, "sessionId")?;
                restore_parked_project_for_new_session(&state, db, repository, &project_id)?;
                schedule_presentation_session_delta(
                    &state,
                    db,
                    repository,
                    &project_id,
                    &session_id,
                )?;
                Ok(json!({ "session": session }))
            },
        ),
        "/api/createAgentSession" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                // A Claude cloud create goes to the repo's main checkout project (claude_cloud/create.rs).
                let cloud_params = crate::claude_cloud::cloud_create_params(repository, params)?;
                let params = cloud_params.as_ref().unwrap_or(params);
                // `create-agent --group-id` needs the session in the group's own project, and a
                // cloud session moves to the main checkout's, so that create is refused up front.
                if let (Some(_), Some(group_project), Some(project)) = (
                    &cloud_params,
                    params.get("sessionGroupProjectId").and_then(Value::as_str),
                    params.get("projectId").and_then(Value::as_str),
                ) {
                    if group_project != project {
                        return Err(DomainStateError::bad_request(format!(
                            "A Claude cloud session for project {group_project} is created in project {project}, the repository's main checkout, so it cannot join a session group of {group_project}."
                        )));
                    }
                }
                let project = repository.resolve_create_session_project(params)?;
                let params = &coordinator_runtime::prepare_coordinator_create_params(params)?;
                let create_params = create_agent_session_params_for_project(db, &project, params)?;
                let created_session = repository.create_session(&create_params, false)?;
                let session =
                    apply_created_session_identity(repository, &created_session, &create_params)?;
                let is_coordinator =
                    crate::coordinators::register_created_coordinator(db, params, &session)?;
                let project_id = value_text(&session, "projectId")?;
                let session_id = value_text(&session, "sessionId")?;
                // Queued before the response, so the skill line goes ahead of anything a client
                // queues next. The session and its coordinator record are committed, so a failure
                // from here on is reported, not raised.
                let role_error = is_coordinator
                    .then(|| crate::agents::session_agent_family_id(&project, &session))
                    .flatten()
                    .and_then(|family| {
                        coordinator_runtime::start_orchestrator_role(
                            &state,
                            &project_id,
                            &session_id,
                            &family,
                            params,
                        )
                        .err()
                    })
                    .map(|error| {
                        let _ = state.logger.log(crate::logging::GxserverLogInput {
                            level: crate::logging::LogLevel::Error,
                            event: "coordinatorRoleQueueFailed".into(),
                            server_id: Some(state.metadata.server_id.clone()),
                            request_id: None,
                            client: None,
                            duration_ms: None,
                            error: Some(error.message.clone()),
                            details: Some(
                                json!({ "projectId": project_id, "sessionId": session_id }),
                            ),
                        });
                        error.message
                    });
                crate::session_chat_empryo_tabs::select_empryo_own_tab_after_start(
                    &session,
                    Some((*state).clone()),
                );
                restore_parked_project_for_new_session(&state, db, repository, &project_id)?;
                schedule_presentation_session_delta(
                    &state,
                    db,
                    repository,
                    &project_id,
                    &session_id,
                )?;
                if crate::empty_session_cleanup::requests_empty_session_cleanup(params)
                    && crate::empty_session_cleanup::cleanup_applies(&state.paths, &session)
                {
                    empty_session_cleanup_runtime::schedule_empty_session_cleanup(
                        state.clone(),
                        project_id,
                        session_id,
                    );
                }
                let mut response = json!({ "session": session });
                if let Some(role_error) = role_error {
                    response["coordinatorRoleError"] = json!(role_error);
                }
                Ok(response)
            },
        ),
        "/api/readResourceSessionOwners" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _, params, _| {
                let names = params
                    .get("zmxNames")
                    .and_then(Value::as_array)
                    .filter(|names| names.len() <= 4096)
                    .ok_or_else(|| {
                        DomainStateError::bad_request("zmxNames must be a bounded array.")
                    })?;
                let names = names
                    .iter()
                    .map(|name| {
                        name.as_str()
                            .filter(|name| !name.is_empty() && name.len() <= 256)
                            .map(str::to_string)
                            .ok_or_else(|| {
                                DomainStateError::bad_request("Invalid zmx session name.")
                            })
                    })
                    .collect::<Result<std::collections::HashSet<_>, _>>()?;
                Ok(json!({ "sessions": repository.resource_session_owners(&names)? }))
            },
        ),
        "/api/listSessions" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let project_id = read_optional_project_id(params)?;
                /*
                CDXC:StateSync 2026-09-01:
                A registry accumulates stopped agent history forever — thousands
                of rows on a working machine — and every one of them was
                hydrated, serialized, and shipped on each poll even though the
                CLI, the mobile inventory, and the desktop close check all
                discard stopped rows on arrival. `includeStopped` lets those
                callers say so up front; it defaults to true, so the endpoint's
                published contract is unchanged for anyone who does not send it.

                The filter is the durable `lifecycleState <> 'stopped'`, which
                is exactly the predicate those callers apply client-side, so an
                opted-in caller sees the same rows it would have kept anyway.

                The three sync passes below only ever act on rows whose stored
                `lifecycleState` is `running` (see `session_state_sync.rs`), and
                every such row survives the filter, so they operate on the same
                candidate set either way. The one pass that also touches other
                rows is the working-directory title repair inside
                `sync_session_state_sidecars`; it stays exhaustive whenever a
                caller omits `includeStopped` and is idempotent, so a narrower
                list simply defers it. Since
                2026-09-11 the snapshot poll and the presentation subscribe use
                `list_presentation_sessions`, which also covers every row the
                repair could change while it is still visible.
                */
                let include_stopped = params
                    .get("includeStopped")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                let list_sessions = |project_id: Option<&str>| {
                    if include_stopped {
                        repository.list_sessions(project_id)
                    } else {
                        repository.list_sessions_excluding_stopped(project_id)
                    }
                };
                /*
                CDXC:StateSync 2026-09-01:
                One `list_sessions` feeds all three sync passes and the
                response. The passes can mutate rows, so re-read only when one
                of them reports an actual change.
                */
                let sessions = list_sessions(project_id.as_deref())?;
                let mut sessions_changed = sync_session_state_sidecars(
                    &state,
                    db,
                    repository,
                    &sessions,
                    "list-sessions",
                )?;
                sessions_changed |= sync_zmx_provider_existence(&state, db, repository, &sessions)?;
                sessions_changed |= sync_live_zmx_process_identities(
                    &state,
                    db,
                    repository,
                    &sessions,
                    None,
                    "list-sessions",
                )?;
                let sessions = if sessions_changed {
                    list_sessions(project_id.as_deref())?
                } else {
                    sessions
                };
                Ok(json!({ "sessions": sessions }))
            },
        ),
        "/api/updateSession" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let session = repository.update_session(params)?;
                let project_id = value_text(&session, "projectId")?;
                let session_id = value_text(&session, "sessionId")?;
                schedule_presentation_session_delta(
                    &state,
                    db,
                    repository,
                    &project_id,
                    &session_id,
                )?;
                Ok(json!({ "session": session }))
            },
        ),
        "/api/updateSessionOrder" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let sessions = repository.update_session_order(params)?;
                for session in &sessions {
                    let project_id = value_text(session, "projectId")?;
                    let session_id = value_text(session, "sessionId")?;
                    schedule_presentation_session_delta(
                        &state,
                        db,
                        repository,
                        &project_id,
                        &session_id,
                    )?;
                }
                Ok(json!({ "sessions": sessions }))
            },
        ),
        /*
        CDXC:StateSync 2026-07-29-00:00:
        Sidebar V2's settle/snooze commands. Guards live in
        `session_lifecycle` so a stale or raced client cannot park working or
        blocked-on-you work behind a settle, and every real change emits a
        presentation delta so all clients reclassify live. A no-op (double
        click, bulk settle over an already-settled row) intentionally skips the
        delta instead of churning the presentation revision.
        */
        "/api/settleSession"
        | "/api/unsettleSession"
        | "/api/snoozeSession"
        | "/api/unsnoozeSession" => {
            let lifecycle_path = endpoint.path.clone();
            let lifecycle_state = state.clone();
            handle_domain_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
                move |repository, db, params, _| {
                    let project_id = read_project_id(params)?;
                    let session_id = read_session_id(params)?;
                    let now = now_iso();
                    let outcome = match lifecycle_path.as_str() {
                        "/api/settleSession" => session_lifecycle::settle_session(
                            repository,
                            &project_id,
                            &session_id,
                            &now,
                        )?,
                        "/api/unsettleSession" => session_lifecycle::unsettle_session(
                            repository,
                            &project_id,
                            &session_id,
                            &now,
                        )?,
                        "/api/snoozeSession" => {
                            let snoozed_until = params
                                .get("snoozedUntil")
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .ok_or_else(|| {
                                    DomainStateError::bad_request(
                                        "snoozedUntil must be an ISO timestamp in the future.",
                                    )
                                })?;
                            session_lifecycle::snooze_session(
                                repository,
                                &project_id,
                                &session_id,
                                snoozed_until,
                                &now,
                            )?
                        }
                        _ => session_lifecycle::unsnooze_session(
                            repository,
                            &project_id,
                            &session_id,
                            &now,
                        )?,
                    };
                    if outcome.changed {
                        schedule_presentation_session_delta(
                            &lifecycle_state,
                            db,
                            repository,
                            &project_id,
                            &session_id,
                        )?;
                    }
                    Ok(json!({
                        "changed": outcome.changed,
                        "session": outcome.session,
                    }))
                },
            )
        }
        "/api/removeSession" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let session = remove_session_row(&state, db, repository, params)?;
                Ok(json!({ "session": session }))
            },
        ),
        "/api/readPresentationSnapshot" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, _, server_id| {
                /*
                CDXC:StateSync 2026-09-01:
                One session list feeds all three sync passes and the snapshot
                projection. The passes can mutate rows, so re-read only when
                one of them reports an actual change.

                CDXC:StateSync 2026-09-11 WHY:
                The list is presentation-scoped: the sync passes only act on
                `running` rows, and the projection discards every stopped row
                that is not pinned, parked, favorite, or tagged, so hydrating
                the thousands of other stopped rows on every two-second poll
                was pure cost. Fork families are derived inside the snapshot
                from the narrow fork-row read over the whole registry.
                */
                let sessions = repository.list_presentation_sessions()?;
                /*
                CDXC:StateSync 2026-09-18 WHY:
                Every client read ran the three repair passes, and the desktop
                alone reads this about once a second, so the zmx process scan
                behind sync_live_zmx_process_identities ran continuously (13%
                of gxserver's CPU in a sample). The passes repair durable rows,
                so a read within a few seconds of the last pass sees the same
                rows it would have repaired; run them at most every 5 seconds.
                */
                let mut sessions_changed = false;
                if presentation_snapshot_sync_due() {
                    sessions_changed = sync_session_state_sidecars(
                        &state,
                        db,
                        repository,
                        &sessions,
                        "read-presentation-snapshot",
                    )?;
                    sessions_changed |=
                        sync_zmx_provider_existence(&state, db, repository, &sessions)?;
                    sessions_changed |= sync_live_zmx_process_identities(
                        &state,
                        db,
                        repository,
                        &sessions,
                        None,
                        "read-presentation-snapshot",
                    )?;
                }
                let sessions = if sessions_changed {
                    repository.list_presentation_sessions()?
                } else {
                    sessions
                };
                read_presentation_snapshot_in_sequence(&state, db, server_id, sessions)
                    .map(|snapshot| json!({ "snapshot": snapshot }))
            },
        ),
        "/api/readCoordinator"
        | "/api/readCoordinatorThreads"
        | "/api/listCoordinators"
        | "/api/updateCoordinator"
        | "/api/linkCoordinatorThread"
        | "/api/unlinkCoordinatorThread"
        | "/api/setCoordinatorThreadResolved"
        | "/api/renameCoordinatorThread"
        | "/api/coordinatorThreadTitleReminder"
        | "/api/checkAgentSessionWrite" => {
            let path = endpoint.path.clone();
            handle_domain_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
                |repository, db, params, _| {
                    coordinator_runtime::handle_coordinator_http(
                        &state, &path, db, repository, params,
                    )
                },
            )
        }
        "/api/readCoordinatorPlaybook"
        | "/api/setCoordinatorPlaybook"
        | "/api/readCoordinatorGuide" => {
            let path = endpoint.path.clone();
            handle_domain_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
                |repository, db, params, _| {
                    crate::coordinators::handle_coordinator_playbook_endpoint(
                        &path, db, repository, params,
                    )
                },
            )
        }
        "/api/resolveCoordinatorThread" => {
            coordinator_resolve::handle_resolve_coordinator_thread_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
            )
            .await
        }
        "/api/acknowledgeCoordinatorThreads" => {
            let params = match read_domain_rpc_params(&body_json) {
                Ok(params) => params,
                Err(error) => return Ok(domain_error_response(endpoint.path, request_id, error)),
            };
            let blocking_state = state.clone();
            match tokio::task::spawn_blocking(move || {
                crate::coordinators::acknowledge_coordinator_threads(&blocking_state, &params)
            })
            .await
            {
                Ok(Ok(result)) => routed_json(
                    Some(endpoint.path),
                    StatusCode::OK,
                    rpc_success(request_id, result),
                ),
                Ok(Err(error)) => domain_error_response(endpoint.path, request_id, error),
                Err(error) => domain_error_response(
                    endpoint.path,
                    request_id,
                    DomainStateError {
                        code: "internalError",
                        message: format!("Coordinator threads task failed: {error}"),
                    },
                ),
            }
        }
        "/api/promoteCoordinator" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                coordinator_runtime::promote_coordinator(&state, db, repository, params)
            },
        ),
        "/api/toggleCloseAfterDone" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                close_after_done_runtime::toggle_close_after_done(&state, db, repository, params)
            },
        ),
        /*
        CDXC:KeepAwake 2026-08-19:
        A client that is ATTACHED to a session (Ghostex mobile, over its SSH CLI
        bridge) renews a keep-awake lease here so this machine's Auto Sleep sweep
        cannot retire a terminal somebody is actually looking at. The lease lives
        in memory with a TTL — see `session_keep_awake` — and is honored by
        `/api/sleepSession` only for automatic sweeps.
        */
        "/api/holdSessionsAwake" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _db, params, _| hold_sessions_awake(repository, params),
        ),
        "/api/holdSessionChatGrid" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _db, params, _| {
                crate::session_chat_grid_claim::hold_session_chat_grid(&state, repository, params)
            },
        ),
        "/api/openCoordinatorThread" => {
            coordinator_open_http::handle_open_coordinator_thread_http(
                &state,
                endpoint.path,
                request_id,
                body_json,
            )
            .await
        }
        "/api/openConversation" => {
            open_conversation_http::handle_open_conversation_http(
                &state,
                endpoint.path,
                request_id,
                body_json,
            )
            .await
        }
        "/api/restorePreviousSession" => {
            previous_session_restore_http::handle_restore_previous_session_http(
                &state,
                endpoint.path,
                request_id,
                body_json,
            )
            .await
        }
        "/api/listPreviousSessions" => {
            let worker_state = state.clone();
            let worker_endpoint = endpoint.path.clone();
            let worker_request_id = request_id.clone();
            match tokio::task::spawn_blocking(move || {
                handle_domain_http(
                    &worker_state,
                    worker_endpoint,
                    worker_request_id,
                    &body_json,
                    |_, db, params, server_id| {
                        crate::external_sessions::discover_before_listing(
                            db,
                            server_id,
                            &worker_state.paths,
                            params
                                .get("refreshExternalSessions")
                                .and_then(Value::as_bool)
                                == Some(true),
                        );
                        list_previous_sessions(db, server_id, params)
                    },
                )
            })
            .await
            {
                Ok(response) => response,
                Err(error) => domain_error_response(
                    endpoint.path,
                    request_id,
                    DomainStateError::corrupt_state(format!("Session discovery failed: {error}")),
                ),
            }
        }
        /*
        CDXC:SessionFork 2026-08-28:
        Previous Sessions hides a closed row once something continues from it, so
        the branch a user forked away from can vanish from every list. This is how
        a client gets the whole family back, ancestors included, to offer a switch
        between branches that share earlier history.
        */
        "/api/sessionForkBranches" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |_, db, params, server_id| list_session_fork_branches(db, server_id, params),
        ),
        "/api/generateSessionTitle" => {
            handle_generate_session_title_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/previewSessionTitle" => {
            handle_preview_session_title_http(&state, endpoint.path, request_id, &body_json).await
        }
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
