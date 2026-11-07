use super::*;

/*
CDXC:SessionChat 2026-07-31:
Send-side endpoints. Every write goes through the per-session async send
queue in session_chat_send/queue.rs (upstream chat spec §7 pacing: clear burst → bracketed-paste
body → separate delayed Enter; answer keystroke groups 1000ms apart), so the
HTTP handlers only validate, build steps, enqueue, and return — they never
hold the connection across the pacing delays.
*/
pub(crate) struct SessionChatSendTarget {
    pub(crate) project_id: String,
    pub(crate) session_id: String,
    pub(crate) zmx_name: String,
    pub(crate) session: Value,
}

pub(crate) fn resolve_session_chat_send_target(
    state: &AppState,
    params: &Map<String, Value>,
    operation: &str,
) -> std::result::Result<SessionChatSendTarget, DomainStateError> {
    let project_id = params
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let session_id = params
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    if project_id.is_empty() || session_id.is_empty() {
        return Err(DomainStateError {
            code: "invalidParams",
            message: format!("{operation} requires projectId and sessionId."),
        });
    }
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let session = repository
        .get_session(&project_id, &session_id)?
        .ok_or_else(|| DomainStateError {
            code: "notFound",
            message: "The session no longer exists.".to_string(),
        })?;
    let zmx_name = crate::zmx::provider_zmx_session_name(&session)?;
    Ok(SessionChatSendTarget {
        project_id,
        session_id,
        zmx_name,
        session,
    })
}

/// Every chat send, including gxserver's own (coordinator reports), passes the send ledger, so a
/// repeated `sendRequestId` answers with the first attempt's result instead of typing again.
pub(crate) async fn handle_send_session_chat_message_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    crate::session_chat_send_requests::send_once(
        state,
        endpoint_path,
        request_id,
        body,
        |state, endpoint_path, request_id, body| async move {
            send_session_chat_message_attempt(&state, endpoint_path, request_id, &body).await
        },
    )
    .await
}

async fn send_session_chat_message_attempt(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let target = match resolve_session_chat_send_target(state, &params, "sendSessionChatMessage") {
        Ok(target) => target,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let text = params
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    /*
    Raw-key mode: `key` carries a keystroke that has no text form (Claude
    Code's Shift+Tab permission-mode cycle or Codex's shifted effort arrows).
    It is mutually exclusive with a message body — the key writes one verbatim
    burst with none of the message pacing (no clear, no paste framing, no
    delayed Enter).
    */
    if let Some(key) = params.get("key").and_then(Value::as_str) {
        if !text.trim().is_empty() || params.get("imagePaths").is_some() {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "invalidParams",
                    message:
                        "sendSessionChatMessage key cannot be combined with text or imagePaths."
                            .to_string(),
                },
            );
        }
        let steps = if key == "escape" {
            let requested_at = Instant::now();
            let session = target.session.clone();
            let transcript =
                tokio::task::spawn_blocking(move || QueuedPromptTranscriptMark::sample(&session))
                    .await
                    .ok()
                    .flatten();
            let agent = session_chat_agent_for_session(&target.session);
            let mut steps = vec![SessionChatSendStep::GuardQueuedPromptEscape {
                requested_at,
                agent: agent.clone(),
                transcript,
            }];
            steps.extend(session_chat_interrupt_escape_steps(agent.as_deref()));
            Some(steps)
        } else {
            crate::session_chat_send::build_session_chat_key_steps(key)
        };
        let Some(steps) = steps else {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "invalidParams",
                    message: format!("sendSessionChatMessage does not know the key \"{key}\"."),
                },
            );
        };
        // CDXC:AgentScreenDetection 2026-10-17 WHY: the Mode pill cycles Cursor with Shift+Tab, which
        // on Cursor's command approval is "Run Everything" and would turn approvals off for the
        // whole session, so a mode step waits until the approval is answered.
        if key == "shift-tab"
            && session_chat_agent_for_session(&target.session).as_deref() == Some("cursor")
            && capture_session_terminal_text(&target.zmx_name)
                .await
                .is_some_and(|screen| {
                    crate::session_chat_cursor_decision::cursor_decision_visible(&screen)
                })
        {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "sessionInputFailed",
                    message: "Cursor is waiting for an answer in the terminal; answer it before changing the mode.".to_string(),
                },
            );
        }
        if let Err(error) = crate::session_chat_send::execute_session_chat_send(
            &target.project_id,
            &target.session_id,
            &target.zmx_name,
            "session-chat-key",
            steps,
        )
        .await
        {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "sessionInputFailed",
                    message: error.message,
                },
            );
        }
        /*
        Raw option keys repaint the footer just like `/model` and `/effort`:
        Shift+Tab changes Claude's permission mode, while shifted arrows change
        Codex effort. Re-read after delivery so the pill confirms the value
        promptly instead of waiting for the idle probe.
        */
        let agent =
            crate::session_chat_follower::session_chat_display_agent_for_session(&target.session);
        schedule_session_chat_option_redetect(
            state,
            &target.project_id,
            &target.session_id,
            agent.as_deref(),
        );
        return routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, json!({ "queued": true, "textBytes": 0 })),
        );
    }
    let image_paths: Vec<String> = params
        .get("imagePaths")
        .and_then(Value::as_array)
        .map(|paths| {
            paths
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if text.trim().is_empty() && image_paths.is_empty() {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "invalidParams",
                message: "sendSessionChatMessage requires text or imagePaths.".to_string(),
            },
        );
    }
    if text.len() > crate::zmx::GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "invalidParams",
                // CDXC:Notices 2026-11-06: plain words for the toast (Q1410), not the endpoint name and byte limit.
                message: "This message is too long to send at once. Split it into smaller parts, or attach it as a file.".to_string(),
            },
        );
    }
    let draft_version = match crate::session_chat_draft_versions::parse(&params) {
        Ok(version) => version,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    // Interrupt and send (chat Ctrl+Enter): stop the agent's running turn and send this message
    // now. A draft or a waking session has no turn to stop, so the paths below that start one
    // ignore it.
    let interrupt = params.get("interrupt").and_then(Value::as_bool) == Some(true);
    // CDXC:SessionChat 2026-09-09 DECISION:
    // User: sending in a new chat is immediate, but delivery waits for the agent's input box. A durable queue receipt lets both apps clear the composer while startup continues.
    // A draft whose Run on row picked a box has no agent to wait for: its first message starts the
    // box (agents/draft_run_location.rs), so it takes the direct send below.
    if crate::agents::session_is_draft(&target.session)
        && crate::agentbox::pending_session_agentbox(&target.session).is_none()
    {
        let queued = crate::session_chat_send_wake::queue_startup_send(
            state,
            endpoint_path,
            request_id,
            &params,
            &target,
            &text,
        );
        if queued.response.status().is_success() {
            crate::session_chat_send_wake::start_draft_agent_if_missing(state, &target);
        }
        return queued;
    }
    // A message never reaches the agent ahead of the model change the user picked before it
    // (2026-09-27 decision in session_chat_model_selection_alert.rs): it waits behind the change in
    // the queue, which applies the change first. A sleeping session keeps the wake path, which
    // queues behind it too.
    if crate::presentation::effective_lifecycle_state(&target.session) == "running"
        && crate::session_chat_model_selection::has_pending_selection(
            state,
            &target.project_id,
            &target.session_id,
        )
    {
        return crate::session_chat_send_wake::queue_startup_send(
            state,
            endpoint_path,
            request_id,
            &params,
            &target,
            &text,
        );
    }
    /*
    CDXC:SessionChat 2026-11-07 WHY: GitHub #479 follow-up: a message sent while Claude's
    question was open waited six seconds for an input box the question keeps off screen, then
    was queued anyway. When the screen shows the question picker the message is queued at once;
    the queue holds it as "Waiting for you to answer the agent's question." and sends it once the
    question is answered. Interrupt and send keeps its direct path: stopping the turn is what it
    asks for.
    */
    if !interrupt
        && crate::presentation::effective_lifecycle_state(&target.session) == "running"
        && claude_question_open(state, &target).await
    {
        crate::session_chat_send_diagnostics::record_send_recovery(
            state,
            "sessionChatSendHeldForQuestion",
            &target.project_id,
            &target.session_id,
            CLAUDE_QUESTION_OPEN,
            &[],
        );
        return crate::session_chat_send_wake::queue_startup_send(
            state,
            endpoint_path,
            request_id,
            &params,
            &target,
            &text,
        );
    }
    match crate::session_chat_queue_runtime::send_session_chat_message_with_draft(
        state,
        &target.project_id,
        &target.session_id,
        &text,
        &image_paths,
        SessionChatMessageSource::Composer,
        draft_version.as_ref(),
        interrupt,
    )
    .await
    {
        Ok(text_bytes) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(
                request_id,
                json!({ "queued": true, "textBytes": text_bytes }),
            ),
        ),
        Err(error)
            if error.code == crate::session_chat_send_wake::SESSION_CHAT_SESSION_STARTING =>
        {
            crate::session_chat_send_wake::queue_startup_send(
                state,
                endpoint_path,
                request_id,
                &params,
                &target,
                &text,
            )
        }
        Err(error) if crate::session_chat_send_wake::holds_until_agent_takes_input(&error) => {
            crate::session_chat_send_diagnostics::record_send_recovery(
                state,
                "sessionChatSendHeldForInputBox",
                &target.project_id,
                &target.session_id,
                &error.message,
                &[],
            );
            crate::session_chat_send_wake::queue_startup_send(
                state,
                endpoint_path,
                request_id,
                &params,
                &target,
                &text,
            )
        }
        // CDXC:SessionChat 2026-10-31 WHY: a chat box send names its draft revision, and only the chat
        // box has a thread to keep the message in. Everything else that sends here (`ghostex`, Slack,
        // automations) gets the error to act on. See the DECISION in session_chat_send_failure.rs.
        Err(error)
            if draft_version.is_some()
                && matches!(error.code, "dependencyUnavailable" | "composerNotCleared") =>
        {
            crate::session_chat_send_wake::queue_failed_composer_send(
                state,
                endpoint_path,
                request_id,
                &params,
                &target,
                &text,
                error,
            )
        }
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

/// What a chat interrupt changes besides the keys it types: shared by Stop and by interrupt and
/// send.
pub(crate) fn record_session_chat_interrupt(
    state: &AppState,
    target: &SessionChatSendTarget,
    request_id: &str,
) {
    // The interrupt is an Escape as far as the activity state machine is
    // concerned: it ends the hook-backed working claim the way the
    // terminal-pane Escape key does (see the escape branch in
    // session_status/transition.rs), because no agent hook reports an interrupted turn.
    let _ =
        crate::accounts::recovery::user_action(state, &target.project_id, &target.session_id, true);
    let mut escape_params = Map::new();
    escape_params.insert("projectId".to_string(), json!(target.project_id));
    escape_params.insert("sessionId".to_string(), json!(target.session_id));
    escape_params.insert("event".to_string(), json!("escape"));
    let _ = crate::server::dispatch_agent_http_blocking(
        state,
        "/api/updateAgentActivity".to_string(),
        request_id.to_string(),
        escape_params,
    );
    /*
    CDXC:SessionChat 2026-10-17 WHY:
    Copilot's Stop is a Ctrl+C that aborts the turn (session_chat_copilot_interrupt.rs), and Copilot fires no hook for an aborted turn and keeps no status in its terminal title, so the Escape rule above left the session "working" for good. Its chat Stop settles it idle; an Escape typed in Copilot's terminal pane only backgrounds a running shell and keeps the rule above.
    */
    if session_chat_agent_for_session(&target.session).as_deref() == Some("copilot") {
        let mut idle_params = Map::new();
        idle_params.insert("projectId".to_string(), json!(target.project_id));
        idle_params.insert("sessionId".to_string(), json!(target.session_id));
        idle_params.insert("activity".to_string(), json!("idle"));
        let _ = crate::server::dispatch_agent_http_blocking(
            state,
            "/api/updateAgentActivity".to_string(),
            request_id.to_string(),
            idle_params,
        );
    }
    crate::session_chat_interactive::retire_interrupted_session_chat_prompt(
        state,
        &target.project_id,
        &target.session_id,
    );
}

pub(crate) async fn handle_interrupt_session_chat_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let target = match resolve_session_chat_send_target(state, &params, "interruptSessionChat") {
        Ok(target) => target,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    crate::autopilot::note_user_interrupt(&target.project_id, &target.session_id);
    // An interrupt that precedes a send (`ghostex agents send --interrupt`) names that send; when
    // the send already went out, this is its retry, and a second Escape would stop the turn the
    // message started.
    match crate::session_chat_send_requests::read_send_request_id(&params) {
        Ok(Some(id))
            if crate::session_chat_send_requests::was_attempted(
                &state.paths,
                &crate::session_chat_send_requests::SendRequestKey::new(
                    &target.project_id,
                    &target.session_id,
                    &id,
                ),
            ) =>
        {
            return routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(
                    request_id,
                    json!({ "interrupted": false, "duplicate": true, "sendRequestId": id }),
                ),
            );
        }
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
        _ => {}
    }
    // Cancel first so queued sends (and an in-flight sequence's remaining
    // steps) drop, then deliver ESC through the queue's new generation.
    crate::session_chat_send::cancel_session_chat_sends(&target.project_id, &target.session_id);
    if session_chat_agent_for_session(&target.session).as_deref() == Some("opencode") {
        let id = crate::session_chat_opencode::session_id(&target.session);
        let prompt_id = params
            .get("toolUseId")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let result = tokio::task::spawn_blocking(move || {
            let id = id?;
            crate::session_chat_opencode::interrupt(&id, prompt_id.as_deref())?;
            crate::session_chat_opencode::invalidate(&id);
            Ok::<_, DomainStateError>(())
        })
        .await
        .unwrap_or_else(|_| {
            Err(crate::session_chat_opencode::error(
                "OpenCode interrupt task failed.",
            ))
        });
        schedule_session_chat_option_redetect(
            state,
            &target.project_id,
            &target.session_id,
            Some("opencode"),
        );
        return match result {
            Ok(()) => routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, json!({"interrupted":true})),
            ),
            Err(error) => domain_error_response(endpoint_path, request_id, error),
        };
    }
    // Pi and OMP (OMP shares Pi's transcript family) stop through the chat channel when their
    // Ghostex extension has it open (CDXC:PiChatChannel 2026-11-05 in pi_chat_channel.rs).
    if session_chat_agent_for_session(&target.session).as_deref() == Some("pi") {
        let (project, session) = (target.project_id.clone(), target.session_id.clone());
        let stopped = tokio::task::spawn_blocking(move || {
            crate::pi_chat_channel::request(
                &project,
                &session,
                "abort",
                json!({}),
                std::time::Duration::from_secs(8),
            )
        })
        .await;
        match stopped {
            Ok(Ok(_)) => {
                record_session_chat_interrupt(state, &target, &request_id);
                return routed_json(
                    Some(endpoint_path),
                    StatusCode::OK,
                    rpc_success(request_id, json!({ "interrupted": true })),
                );
            }
            Ok(Err(reason)) => crate::pi_chat_channel::log_screen_fallback(
                &target.project_id,
                &target.session_id,
                "interrupt",
                &reason.to_string(),
            ),
            Err(_) => {}
        }
    }
    // DeepSeek Harness stops through dsh-tui's own Ctrl+C in Ghostex's plugin
    // (CDXC:DshChatChannel 2026-11-05 in dsh_chat_plugin.rs).
    if session_chat_agent_for_session(&target.session).as_deref() == Some("dsh")
        && crate::dsh_chat_channel::abort(&target.project_id, &target.session_id)
            .await
            .is_some()
    {
        record_session_chat_interrupt(state, &target, &request_id);
        return routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, json!({ "interrupted": true })),
        );
    }
    // A GBots conversation stops in its bot's sidecar, not with keys typed into its terminal client.
    if session_chat_agent_for_session(&target.session).as_deref() == Some("gbots") {
        let (project, session) = (target.project_id.clone(), target.session_id.clone());
        let blocking_state = state.clone();
        let stopped = tokio::task::spawn_blocking(move || {
            crate::gbots::interrupt_gbots_session(&blocking_state, &project, &session)
        })
        .await;
        if let Ok(Some(result)) = stopped {
            return match result {
                Ok(()) => {
                    record_session_chat_interrupt(state, &target, &request_id);
                    routed_json(
                        Some(endpoint_path),
                        StatusCode::OK,
                        rpc_success(request_id, json!({ "interrupted": true })),
                    )
                }
                Err(error) => domain_error_response(endpoint_path, request_id, error),
            };
        }
    }
    // Hermes 0.22 stops through its gateway's `session.interrupt`, with no Ctrl+C to time against
    // the force-quit window (CDXC:HermesGateway 2026-11-05 in hermes_gateway.rs).
    if session_chat_agent_for_session(&target.session).as_deref() == Some("hermes") {
        let session = target.session.clone();
        match tokio::task::spawn_blocking(move || crate::hermes_gateway::interrupt(&session)).await
        {
            Ok(Ok(_)) => {
                crate::hermes_gateway::invalidate(&target.session);
                record_session_chat_interrupt(state, &target, &request_id);
                return routed_json(
                    Some(endpoint_path),
                    StatusCode::OK,
                    rpc_success(request_id, json!({ "interrupted": true })),
                );
            }
            Ok(Err(reason)) => crate::hermes_gateway::log_screen_fallback(
                &target.project_id,
                &target.session_id,
                "interrupt",
                &reason.to_string(),
            ),
            Err(_) => {}
        }
    }
    let steps = session_chat_interrupt_escape_steps(
        session_chat_agent_for_session(&target.session).as_deref(),
    );
    crate::session_chat_send::enqueue_session_chat_send(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        "session-chat-interrupt",
        steps,
    );
    record_session_chat_interrupt(state, &target, &request_id);
    // Claude Code may answer this Escape by handing the prompt back to its
    // composer; the detector decides after the write lands (CDXC:SessionChat
    // in session_chat_returned_prompt.rs).
    crate::session_chat_returned_prompt::schedule_session_chat_returned_prompt_detection(
        state,
        &target,
        &request_id,
    );
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(request_id, json!({ "interrupted": true })),
    )
}

const CLAUDE_QUESTION_OPEN: &str = "Claude is waiting for you to answer its question.";

/// Whether a Claude session's screen shows its question picker now. The screen is captured only
/// when the cached screen verdict or the stored question card says a question may be open, so an
/// ordinary send pays for no extra capture.
async fn claude_question_open(state: &AppState, target: &SessionChatSendTarget) -> bool {
    let agent = crate::session_chat_composer::session_chat_composer_agent_id(&target.session);
    if !matches!(agent.as_deref(), Some("claude" | "openclaude")) {
        return false;
    }
    let maybe_open = crate::session_approval::session_awaits_answer(&target.session)
        || crate::session_chat_options::cached_session_chat_composer_readiness(
            state,
            &target.project_id,
            &target.session_id,
        )
        .waits_on_question();
    if !maybe_open {
        return false;
    }
    capture_session_terminal_text(&target.zmx_name)
        .await
        .is_some_and(|screen| {
            crate::session_chat_composer::session_composer_verdict(agent.as_deref(), &screen)
                .waits_on_question()
        })
}
