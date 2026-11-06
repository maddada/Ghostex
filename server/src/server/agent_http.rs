use super::*;
use crate::session_chat_interactive::emit_session_chat_prompt_state_frame;

/*
CDXC:RepoStructure 2026-06-16-10:00:
Phase 6 agent endpoints share the durable domain repository and authenticated RPC envelope with earlier Rust milestones. Keep zmx fork startup explicit through the selected listener port and schedule presentation deltas only after the session row has been updated.
*/
pub(crate) async fn handle_agent_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    /*
    CDXC:Zmx 2026-09-01:
    `/api/forkSession` and `/api/switchDraftAgent` reach
    `dispatch_zmx_lifecycle_endpoint`, and the rename path below reaches
    `dispatch_zmx_session_interaction_endpoint`, so this handler spawns zmx and
    launchd processes on whatever thread calls it. It is synchronous end to
    end, so the whole body goes to the blocking pool rather than parking an
    executor worker. The `schedule_*` helpers still `tokio::spawn` from there:
    blocking-pool threads run inside the runtime context.
    */
    let blocking_state = state.clone();
    let blocking_endpoint_path = endpoint_path.clone();
    let blocking_request_id = request_id.clone();
    match tokio::task::spawn_blocking(move || {
        dispatch_agent_http_blocking(
            &blocking_state,
            blocking_endpoint_path,
            blocking_request_id,
            params,
        )
    })
    .await
    {
        Ok(response) => response,
        Err(error) => domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "internalError",
                message: format!("Agent endpoint task failed: {error}"),
            },
        ),
    }
}

pub(crate) fn dispatch_agent_http_blocking(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    params: Map<String, Value>,
) -> RoutedResponse {
    let db = match open_gxserver_database(&state.paths) {
        Ok(db) => db,
        Err(error) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "internalError",
                    message: format!("SQLite gxserver state error: {error}"),
                },
            );
        }
    };
    // A Claude hook superseded by the hook-free observer is only compared (hookless_activity/drive.rs).
    if crate::hookless_activity::claude_hook_superseded(state, &endpoint_path, &params) {
        crate::hookless_activity::observe_hook_event(state, &endpoint_path, &params, &Value::Null);
        return routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(
                request_id,
                json!({
                    "changed": false,
                    "enteredAttention": false,
                    "reason": "claude-hook-superseded",
                }),
            ),
        );
    }
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let context = ZmxServerContext {
        auth_token_file: state.paths.auth_token_file.to_string_lossy().to_string(),
        base_url: format!(
            "http://{}:{}",
            state.config.listeners.local.host, state.config.listeners.local.port
        ),
    };
    match dispatch_agent_endpoint(
        &repository,
        &db,
        &state.paths.home_dir,
        &endpoint_path,
        &params,
        Some(&context),
    ) {
        Ok(output) => {
            let presentation_session = output.presentation_session.clone();
            let mut result = output.result;
            let fork_initial_rename = fork_initial_rename_target(&endpoint_path, &result);
            log_agent_hook_passive_identity_conflict(state, &endpoint_path, &params, &result);
            log_agent_activity_transition(state, &endpoint_path, &params, &result);
            crate::hookless_activity::observe_hook_event(state, &endpoint_path, &params, &result);
            crate::accounts::api_errors::observe_agent_hook(state, &endpoint_path, &params);
            crate::notification_feed::observe_agent_activity_result(
                state,
                &endpoint_path,
                &params,
                &result,
            );
            crate::notification_feed::observe_agent_turn(state, &endpoint_path, &params, &result);
            crate::push::observe_agent_start(state, &endpoint_path, &result);
            crate::session_chat_question_resend::observe_turn_end(state, &params, &result);
            crate::session_chat_claude_feedback_card::observe_turn_end(state, &params, &result);
            crate::session_chat_open_questions::observe_activity(state, &params, &result);
            crate::session_chat_plan_mode_guard::observe_hook(
                state,
                &endpoint_path,
                &params,
                &result,
            );
            let session_chat_state_changed = (endpoint_path == "/api/ingestAgentHookEvent"
                && (result
                    .get("sessionChatPromptChanged")
                    .and_then(Value::as_bool)
                    == Some(true)
                    || result
                        .get("sessionChatActivityChanged")
                        .and_then(Value::as_bool)
                        == Some(true)))
                // An Escape that retired an approval card (`escape_denies_stored_approval`).
                || (endpoint_path == "/api/updateAgentActivity"
                    && result
                        .get("sessionChatPromptChanged")
                        .and_then(Value::as_bool)
                        == Some(true));
            /*
            CDXC:AgentScreenDetection 2026-10-04 WHY:
            A new chat's follower can only watch Claude's statusline once it knows the Claude session id, and that id arrives with the agent's first hook. The follower otherwise adopts it on its next resolve poll, up to a second later, which held the model pill back when the first statusline beat the hook. A new id wakes the follower now; the fresh generation it starts adopts the id at once.
            */
            let agent_session_id_changed = endpoint_path == "/api/ingestAgentHookEvent"
                && result.get("agentSessionIdChanged").and_then(Value::as_bool) == Some(true);
            strip_agent_hook_internal_result_fields(&endpoint_path, &mut result);
            if agent_session_id_changed {
                if let Some(session) = result.get("session") {
                    if let (Some(project_id), Some(session_id)) = (
                        read_session_text(session, "projectId"),
                        read_session_text(session, "sessionId"),
                    ) {
                        crate::session_chat_follower::request_session_chat_resnapshot(
                            state,
                            &project_id,
                            &session_id,
                        );
                    }
                }
            }
            // An Empryo launch model waits for Empryo to be up, which its first hook reports.
            if endpoint_path == "/api/ingestAgentHookEvent" {
                crate::session_chat_empryo_launch_selection::queue_empryo_launch_selection_from_hook(
                    state, &db, &result,
                );
            }
            // CDXC:SessionChat 2026-10-07 WHY: a fork's Empryo window joins the engine of the session it was forked from, the case where Empryo 3.9.1 opens the engine's tab without the window's own (seen live 2026-10-07: the fork's first send was held until a manual sleep and wake). The fork's terminal starts inside the fork call, which cannot restart it, so the fork gets the same one rejoin restart a create does.
            if endpoint_path == "/api/forkSession" || endpoint_path == "/api/startSideChat" {
                if let Some(session) = result.pointer("/fork/session") {
                    crate::session_chat_empryo_tabs::select_empryo_own_tab_after_start(
                        session,
                        Some(state.clone()),
                    );
                }
            }
            if endpoint_path == "/api/startSideChat" {
                after_side_chat_start(state, &params, &result);
            }
            if endpoint_path == "/api/ingestAgentHookEvent" {
                schedule_codex_title_fallback(state, &params, &result);
            }
            let should_queue_agent_title_metadata_check =
                should_schedule_agent_title_metadata_check(&endpoint_path, &result);
            let should_schedule_first_prompt_auto_title =
                result.get("reason").and_then(Value::as_str)
                    == Some("first-prompt-auto-title-claimed");
            // CDXC:SessionTitles 2026-09-10 WHY:
            // The claim is already persisted. Start its worker before publishing presentation, whose error return used to strand a running claim without any task to finish it. This ordering covers every provider.
            if should_schedule_first_prompt_auto_title {
                if let Some(session) = result.get("session") {
                    if let (Some(project_id), Some(session_id), Some(attempt_id)) = (
                        read_session_text(session, "projectId"),
                        read_session_text(session, "sessionId"),
                        read_runtime_text(session, FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY),
                    ) {
                        schedule_first_prompt_auto_title_job(
                            state.clone(),
                            project_id,
                            session_id,
                            attempt_id,
                        );
                    }
                }
            }
            /*
            CDXC:Drafts 2026-08-28 (live-pane switching):
            Everything the daemon has detected off this session's screen belongs
            to the agent that just got interrupted — its model/effort pills, its
            statusline, its terminal notice, and its composer-readiness verdict.
            None of it survives the switch, and on the live-pane path nothing
            else drops it: the provider never restarts, so the follower is never
            stopped and `stop_session_chat_follower`'s forget never runs. Drop
            the whole detection entry here, so the next read re-detects against
            the NEW agent instead of showing the previous one's model for as
            long as the 5s cache holds.

            `switch_draft_agent` cannot do this itself: the cache lives on
            `AppState` and agent endpoints are dispatched without it. Keyed off
            `presentation_session`, which the endpoint sets only when the row
            actually changed agent (a switch to the agent already in use is a
            no-op and must not blank a valid detection).
            */
            /*
            CDXC:SessionChat 2026-09-04 DECISION:
            User: an Escape typed in the terminal pane must get the same
            returned-prompt handling as the chat box's Stop. The pane reports
            that Escape as this activity event, so the detector hangs off it
            here (see session_chat_returned_prompt.rs); a session that is not
            followable, or has no chat send to answer, makes it a no-op.
            */
            if endpoint_path == "/api/updateAgentActivity"
                && params.get("event").and_then(Value::as_str) == Some("escape")
            {
                if let Ok(target) = crate::session_chat_send::resolve_session_chat_send_target(
                    state,
                    &params,
                    "updateAgentActivity",
                ) {
                    crate::session_chat_returned_prompt::schedule_session_chat_returned_prompt_detection(
                        state,
                        &target,
                        &request_id,
                    );
                }
            }
            if endpoint_path == "/api/switchDraftAgent"
                || endpoint_path == "/api/draftRunLocation"
                || endpoint_path == "/api/switchSessionAgent"
            {
                if let Some((project_id, session_id)) = presentation_session.as_ref() {
                    crate::session_chat_options::forget_session_chat_options(
                        state, project_id, session_id,
                    );
                }
            }
            if endpoint_path == "/api/updateAgentActivity" {
                if let Some((project_id, session_id)) = presentation_session.as_ref() {
                    let _ = reconcile_agent_metadata_title_for_session(
                        &repository,
                        project_id,
                        session_id,
                        &state.paths.home_dir,
                        "pending",
                    );
                }
            }
            // The terminal-typed half of "Unpark after sending a message"; the
            // chat half is in session_chat_queue_runtime/post_send.rs. Runs before the
            // delta below so the same publish carries the cleared flag.
            if endpoint_path == "/api/ingestAgentHookEvent"
                && crate::session_parking::is_user_prompt_submit_hook_event(&params)
            {
                if let Some((project_id, session_id)) = presentation_session.as_ref() {
                    if let Err(error) = crate::session_parking::unpark_session_after_user_message(
                        &repository,
                        &state.paths.app_config_dir,
                        project_id,
                        session_id,
                        &now_iso(),
                    ) {
                        return domain_error_response(endpoint_path, request_id, error);
                    }
                }
            }
            if let Some((project_id, session_id)) = presentation_session.as_ref() {
                if let Err(error) = schedule_presentation_session_delta(
                    state,
                    &db,
                    &repository,
                    project_id,
                    session_id,
                ) {
                    return domain_error_response(endpoint_path, request_id, error);
                }
                if let Ok(Some(session)) = repository.get_session(project_id, session_id) {
                    schedule_stale_activity_presentation_refresh(
                        state,
                        &session,
                        stale_activity_refresh_reason(&endpoint_path),
                    );
                }
            }
            if presentation_session.is_none()
                && endpoint_path == "/api/ingestTerminalTitleEvent"
                && result_activity(&result) == Some("working")
            {
                if let Some(session) = result.get("session") {
                    schedule_stale_activity_presentation_refresh(
                        state,
                        session,
                        "terminal-title-stale-activity",
                    );
                }
            }
            if should_queue_agent_title_metadata_check {
                if let Some((project_id, session_id)) = presentation_session {
                    if endpoint_path == "/api/requestSessionRename" {
                        schedule_session_rename_confirmation(state.clone(), project_id, session_id);
                    } else {
                        schedule_agent_title_metadata_check(state.clone(), project_id, session_id);
                    }
                }
            }
            if let Some(target) = fork_initial_rename {
                schedule_fork_initial_rename(state.clone(), target);
            }
            if let Some((project_id, session_id, command)) =
                requested_agent_title_command_submission(&endpoint_path, &params, &result)
            {
                /*
                CDXC:SessionTitles 2026-09-15 WHY:
                Both local chat view and remote sessions can lack a mounted terminal in the requesting app.
                Submit from the owning gxserver through the session input queue so renaming does not depend on the visible view.
                This extends the existing remote-only submission path to local rename requests.
                */
                let mut send_params = Map::new();
                send_params.insert("projectId".to_string(), json!(project_id));
                send_params.insert("sessionId".to_string(), json!(session_id));
                send_params.insert(
                    "diagnosticInputSource".to_string(),
                    json!("session-rename-command"),
                );
                send_params.insert("submit".to_string(), Value::Bool(true));
                send_params.insert("text".to_string(), Value::String(command.clone()));
                /*
                CDXC:Drafts 2026-08-28:
                `request_session_rename` already armed the draft's suppression
                window when it accepted this rename, but the bytes go out HERE,
                a round trip later. Re-arm from the moment they are actually
                typed so the window covers the churn they cause rather than
                having been partly spent getting to this line. No-op unless the
                target is a draft.
                */
                if let Ok(Some(session)) = repository.get_session(&project_id, &session_id) {
                    let _ =
                        crate::agents::arm_draft_launch_activity_suppression(&repository, &session);
                }
                let lifecycle = crate::zmx::LifecycleParams {
                    project_id: project_id.clone(),
                    session_id: session_id.clone(),
                };
                let daemon_missing =
                    crate::zmx::probe_and_cache_session_provider(&repository, &lifecycle)
                        .is_ok_and(|(probe, ..)| probe.lifecycle_state == "missing");
                // OpenCode's conversation lives in its service whether or not a terminal runs it,
                // so it is renamed there (`PATCH /api/session/{id}`) instead of a typed `/rename`.
                let renamed_in_service = params
                    .get("title")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|title| !title.is_empty())
                    .zip(
                        repository
                            .get_session(&project_id, &session_id)
                            .ok()
                            .flatten(),
                    )
                    .filter(|(_, session)| {
                        crate::session_chat_follower::session_chat_agent_for_session(session)
                            .as_deref()
                            == Some("opencode")
                    })
                    .is_some_and(|(title, session)| {
                        crate::session_chat_opencode::session_id(&session)
                            .and_then(|id| crate::session_chat_opencode::rename(&id, title))
                            .inspect_err(|error| {
                                crate::pi_chat_channel::log_screen_fallback(
                                    &project_id,
                                    &session_id,
                                    "opencodeRename",
                                    &error.message,
                                )
                            })
                            .is_ok()
                    });
                // Hermes 0.22 is renamed through its gateway (`session.mutate rename`) instead of a
                // typed `/title` (CDXC:HermesGateway 2026-11-05 in hermes_gateway.rs).
                let renamed_in_service = renamed_in_service
                    || params
                        .get("title")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|title| !title.is_empty())
                        .zip(
                            repository
                                .get_session(&project_id, &session_id)
                                .ok()
                                .flatten(),
                        )
                        .filter(|(_, session)| {
                            crate::session_chat_follower::session_chat_agent_for_session(session)
                                .as_deref()
                                == Some("hermes")
                        })
                        .is_some_and(|(title, session)| {
                            crate::hermes_gateway::rename(&session, title)
                                .inspect_err(|reason| {
                                    crate::hermes_gateway::log_screen_fallback(
                                        &project_id,
                                        &session_id,
                                        "rename",
                                        &reason.to_string(),
                                    )
                                })
                                .is_ok()
                        });
                if renamed_in_service {
                    crate::session_chat_app_command::record_session_chat_user_rename_command(
                        &project_id,
                        &session_id,
                        &command,
                    );
                    if let Some(result) = result.as_object_mut() {
                        result.insert("agentRenameCommand".to_string(), json!("sent"));
                    }
                } else if daemon_missing {
                    if let Err(error) =
                        crate::session_chat_send_wake::queue_startup_command_and_wake(
                            state,
                            &project_id,
                            &session_id,
                            &command,
                        )
                    {
                        return domain_error_response(endpoint_path, request_id, error);
                    }
                    if let Some(result) = result.as_object_mut() {
                        result.insert(
                            "agentRenameCommand".to_string(),
                            json!("queuedUntilAgentStarts"),
                        );
                    }
                } else {
                    // Pi and OMP take the name through their chat channel (pi_chat_channel.rs).
                    let channel_named = params
                        .get("title")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|title| !title.is_empty())
                        .zip(
                            repository
                                .get_session(&project_id, &session_id)
                                .ok()
                                .flatten(),
                        )
                        .is_some_and(|(title, session)| {
                            crate::pi_chat_actions::is_pi_family(&session)
                                && crate::pi_chat_channel::set_session_name_blocking(
                                    &project_id,
                                    &session_id,
                                    title,
                                )
                        });
                    if !channel_named {
                        if let Err(error) = dispatch_zmx_session_interaction_endpoint(
                            &repository,
                            "/api/sendSessionMessage",
                            &send_params,
                        ) {
                            return zmx_error_response(endpoint_path, request_id, error);
                        }
                    }
                    crate::session_chat_app_command::record_session_chat_user_rename_command(
                        &project_id,
                        &session_id,
                        &command,
                    );
                    if let Some(result) = result.as_object_mut() {
                        result.insert("agentRenameCommand".to_string(), json!("sent"));
                    }
                }
            }
            if session_chat_state_changed {
                if let Some(session) = result.get("session") {
                    emit_session_chat_prompt_state_frame(state, session);
                }
            }
            routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, result),
            )
        }
        Err(error) => agent_error_response(endpoint_path, request_id, error),
    }
}

pub(crate) fn handle_agent_hook_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => {
            return routed_json(
                Some(endpoint_path),
                StatusCode::INTERNAL_SERVER_ERROR,
                rpc_error("internalError", error.message, Some(request_id)),
            )
        }
    };
    /*
    CDXC:AgentHooks 2026-06-19-14:15:
    Hook uninstall is routed with read/install through the local authenticated RPC envelope. The handler returns the TypeScript-compatible status payload plus removedPaths without writing provider paths, hook commands, or hook file contents to persistent logs.

    CDXC:AgentHooks 2026-06-22-08:23:
    Area 27 status-code parity keeps malformed hook RPC params on TypeScript gxserver's generic internalError path. File/config hook operation failures still use the shared domain-error mapping after params have been accepted.
    */
    let result = match endpoint_path.as_str() {
        "/api/installAgentHooks" => install_agent_hooks(&state.paths, &params),
        "/api/uninstallAgentHooks" => uninstall_agent_hooks(&state.paths, &params),
        _ => read_agent_hook_status(&state.paths, &params),
    };
    match result {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

pub(crate) fn agent_error_response(
    endpoint_path: String,
    request_id: String,
    error: AgentEndpointError,
) -> RoutedResponse {
    match error {
        AgentEndpointError::Domain(error) => {
            domain_error_response(endpoint_path, request_id, error)
        }
        AgentEndpointError::DependencyUnavailable(message) => routed_json(
            Some(endpoint_path),
            StatusCode::SERVICE_UNAVAILABLE,
            rpc_error("dependencyUnavailable", message, Some(request_id)),
        ),
    }
}

pub(crate) fn log_agent_hook_passive_identity_conflict(
    state: &AppState,
    endpoint_path: &str,
    params: &Map<String, Value>,
    result: &Value,
) {
    if endpoint_path != "/api/ingestAgentHookEvent"
        || result.get("reason").and_then(Value::as_str) != Some("passive-session-identity-conflict")
    {
        return;
    }
    let Some(conflict) = result.get("identityConflict").and_then(Value::as_object) else {
        return;
    };
    if conflict.get("source").and_then(Value::as_str) != Some("passive") {
        return;
    }
    let agent_id = conflict.get("agentId").and_then(Value::as_str);
    let current_agent_session_id = conflict
        .get("currentAgentSessionId")
        .and_then(Value::as_str);
    let incoming_agent_session_id = conflict
        .get("incomingAgentSessionId")
        .and_then(Value::as_str);
    let reason = conflict.get("reason").and_then(Value::as_str);
    let source = conflict.get("source").and_then(Value::as_str);
    /*
    CDXC:AgentHooks 2026-06-22-08:31:
    Passive hook identity conflicts need the same support-bundle evidence as TypeScript without exposing thread ids, paths, titles, prompts, or hook payloads. Hash private agent-session ids at the writer boundary and log only enum fields, stable gxserver ids, and payload-shape booleans.
    */
    let _ = state.logger.log_routine(
        DiagnosticLogScenario::AgentDetection,
        GxserverLogInput {
            level: LogLevel::Debug,
            event: "sessionIdentity.updateBlocked".to_string(),
            server_id: Some(state.metadata.server_id.clone()),
            request_id: None,
            client: None,
            duration_ms: None,
            error: None,
            details: Some(json!({
                "agentId": agent_id,
                "currentAgentSessionIdHash": hash_log_identity(current_agent_session_id),
                "currentAgentSessionIdPresent": current_agent_session_id.is_some(),
                "incomingAgentSessionIdHash": hash_log_identity(incoming_agent_session_id),
                "incomingAgentSessionIdPresent": incoming_agent_session_id.is_some(),
                "ownerProjectId": conflict.get("ownerProjectId").cloned(),
                "ownerSessionId": conflict.get("ownerSessionId").cloned(),
                "projectId": params.get("projectId").and_then(Value::as_str),
                "reason": reason,
                "sessionId": params.get("sessionId").and_then(Value::as_str),
                "source": source,
            })),
        },
    );
    let hook_activity = normalize_agent_hook_event_activity(params);
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "sessionIdentity.passiveEventRejected".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(json!({
            "activity": hook_activity,
            "agentId": agent_id,
            "currentAgentSessionIdHash": hash_log_identity(current_agent_session_id),
            "currentAgentSessionIdPresent": current_agent_session_id.is_some(),
            "hasAgentSessionId": params.get("agentSessionId").is_some(),
            "hasAgentSessionPath": params.get("agentSessionPath").is_some(),
            "hasExplicitStatus": params.get("status").is_some(),
            "hasFirstUserMessage": params.get("firstUserMessage").is_some(),
            "hasHookEventName": params.get("eventName").is_some() || params.get("rawEventName").is_some(),
            "hasTitle": params.get("title").is_some(),
            "identitySource": source,
            "incomingAgentSessionIdHash": hash_log_identity(incoming_agent_session_id),
            "incomingAgentSessionIdPresent": incoming_agent_session_id.is_some(),
            "ownerProjectId": conflict.get("ownerProjectId").cloned(),
            "ownerSessionId": conflict.get("ownerSessionId").cloned(),
            "projectId": params.get("projectId").and_then(Value::as_str),
            "reason": reason,
            "sessionId": params.get("sessionId").and_then(Value::as_str),
            "source": "agent-hook-event",
        })),
    });
}

/*
CDXC:Diagnostics 2026-08-28:
Every agent-activity flip (working/idle/attention) with its trigger, the
event that carried it, its working source (explicit hook vs terminal title),
and whether the initial passive-signal suppression window was still armed.
This is the server half of the "Loading conversation…" flash repro: the chat
view blanks when a working flip reaches an empty transcript, so the question
the log answers is where that working came from. Transitions only — title
ticks that keep the same activity never write — and only enum fields, stable
ids, and booleans, never the title text itself.
*/
pub(crate) fn log_agent_activity_transition(
    state: &AppState,
    endpoint_path: &str,
    params: &Map<String, Value>,
    result: &Value,
) {
    let trigger = match endpoint_path {
        "/api/ingestAgentHookEvent" => "agentHook",
        "/api/ingestTerminalTitleEvent" => "terminalTitle",
        "/api/updateAgentActivity" => "activityRpc",
        _ => return,
    };
    let Some(previous) = result.get("previousActivity").and_then(Value::as_str) else {
        return;
    };
    let Some(activity) = result.get("activity").and_then(Value::as_object) else {
        return;
    };
    let next = activity
        .get("activity")
        .and_then(Value::as_str)
        .unwrap_or("idle");
    if previous == next {
        return;
    }
    let suppression_active = activity
        .get("suppressedUntil")
        .and_then(Value::as_str)
        .and_then(crate::session_status::parse_iso_ms)
        .is_some_and(|until| until > chrono::Utc::now().timestamp_millis());
    let _ = state.logger.log_routine(
        DiagnosticLogScenario::AgentActivity,
        GxserverLogInput {
            level: LogLevel::Debug,
            event: "agentActivity.transition".to_string(),
            server_id: Some(state.metadata.server_id.clone()),
            request_id: None,
            client: None,
            duration_ms: None,
            error: None,
            details: Some(json!({
                "activityEvent": params.get("event").and_then(Value::as_str),
                "agent": activity.get("agentName").and_then(Value::as_str),
                "enteredAttention": result.get("enteredAttention").and_then(Value::as_bool),
                "hasHookEventName": params.get("eventName").is_some() || params.get("rawEventName").is_some(),
                "next": next,
                "previous": previous,
                "projectId": params.get("projectId").and_then(Value::as_str),
                "reason": result.get("reason").and_then(Value::as_str),
                "sessionId": params.get("sessionId").and_then(Value::as_str),
                "suppressionActive": suppression_active,
                "trigger": trigger,
                "workingSource": activity.get("workingSource").and_then(Value::as_str),
            })),
        },
    );
}

pub(crate) fn strip_agent_hook_internal_result_fields(endpoint_path: &str, result: &mut Value) {
    if endpoint_path == "/api/updateAgentActivity" {
        if let Some(object) = result.as_object_mut() {
            object.remove("sessionChatPromptChanged");
        }
        return;
    }
    if endpoint_path != "/api/ingestAgentHookEvent" {
        return;
    }
    if let Some(object) = result.as_object_mut() {
        object.remove("identityConflict");
        object.remove("sessionChatPromptChanged");
        object.remove("sessionChatActivityChanged");
        object.remove("agentSessionIdChanged");
    }
}

pub(crate) fn hash_log_identity(value: Option<&str>) -> Option<String> {
    let value = value?;
    let digest = Sha256::digest(value.as_bytes());
    Some(
        digest
            .iter()
            .take(8)
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
    )
}

pub(crate) fn should_schedule_agent_title_metadata_check(
    endpoint_path: &str,
    result: &Value,
) -> bool {
    match endpoint_path {
        "/api/requestSessionRename" => {
            result.get("pendingAgentMetadata").and_then(Value::as_bool) == Some(true)
        }
        "/api/ingestTerminalTitleEvent" => {
            result.get("changed").and_then(Value::as_bool) == Some(true)
        }
        "/api/ingestAgentHookEvent" => !matches!(
            result.get("reason").and_then(Value::as_str),
            Some("agent-hook-agent-mismatch" | "passive-session-identity-conflict")
        ),
        "/api/updateAgentActivity" => true,
        _ => false,
    }
}

pub(crate) fn stale_activity_refresh_reason(endpoint_path: &str) -> &'static str {
    match endpoint_path {
        "/api/ingestAgentHookEvent" => "agent-hook-stale-activity",
        "/api/ingestTerminalTitleEvent" => "terminal-title-stale-activity",
        _ => "agent-activity-stale-activity",
    }
}
