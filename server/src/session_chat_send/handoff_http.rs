use super::*;

/*
CDXC:Drafts 2026-08-18:
Terminal → chat draft transfer for every host. A user who typed into the agent
CLI and then lands on the chat surface, by tapping the toggle or because the app
auto-switched a terminal-started agent into Chat, must find that text in the
chat composer instead of stranded behind the parked terminal. The capture uses
the agent's prompt-editor handshake, which parks the draft in Saved Prompts;
this reads that row back and (when this capture created it) removes it again, so
a transfer leaves no residue in the user's Saved Prompts list.

Grok Build binds Ctrl+G to its Tasks pane, so its capture opens the editor through
the verified command-palette handshake in session_chat_grok_draft.rs.
*/

/*
CDXC:Drafts 2026-08-30:
The prompt-editor handshake only works when the agent CLI inherits the session
shell's environment, where the launch script points $EDITOR/$VISUAL at
`ghostex prompt-editor`. An agent command that hops to another user or host
(`ssh -tt qawwi@localhost … hermes`, seen live 2026-08-30) starts the CLI
outside that environment: the CLI resolves its own editor instead (on current
macOS the prompt_toolkit fallback chain lands in pico via /usr/bin/nano), the
response file is never written, and the 16s wait ends with the TUI wedged
inside that editor — which then makes every send fail composer detection.
Nothing about such a session can answer the handshake, so the capture is
skipped up front and the draft stays in the parked terminal, the documented
loss-safe failure mode of this endpoint.
*/
/// Whether the effective agent command transfers control to another user or host.
fn session_chat_agent_command_is_user_hop(session: &Value) -> bool {
    let runtime_settings = session.get("runtimeSettings").and_then(Value::as_object);
    let launch_settings = session.get("launchSettings").and_then(Value::as_object);
    let command = runtime_settings
        .and_then(|settings| settings.get("agentCommand"))
        .and_then(Value::as_str)
        .filter(|command| !command.trim().is_empty())
        .or_else(|| {
            launch_settings
                .and_then(|settings| settings.get("agentLaunchPlan"))
                .and_then(Value::as_object)
                .and_then(|plan| plan.get("command"))
                .and_then(Value::as_str)
                .filter(|command| !command.trim().is_empty())
        })
        .or_else(|| {
            launch_settings
                .and_then(|settings| settings.get("agentCommand"))
                .and_then(Value::as_str)
                .filter(|command| !command.trim().is_empty())
        })
        .unwrap_or_default();
    let Some(first_word) = command.split_whitespace().next() else {
        return false;
    };
    let program = first_word.rsplit('/').next().unwrap_or(first_word);
    matches!(program, "ssh" | "autossh" | "mosh" | "et")
}

/// CDXC:Drafts 2026-10-16 WHY:
/// With the agent gone and its terminal back at a shell prompt (a start command that failed, an agent that quit), nothing answers the prompt-editor handshake: the capture typed its editor key into the user's shell, waited out its 16 seconds and answered HTTP 500 on every switch to Chat (a customer's Windows log, 2026-10-16). A shell holds no agent draft, so the switch transfers nothing.
/// CDXC:Drafts 2026-10-24 WHY:
/// A bare prompt was not enough: a Codex whose start command sat unrun at the prompt, and one that never drew anything, got the editor key typed into the shell (`^G^G` after `codex`) and failed after 16 seconds the same way (GitHub #290). An agent whose input box Ghostex can recognise must be showing it, or there is no draft to move.
async fn terminal_holds_no_agent_draft(zmx_name: &str, agent: Option<&str>) -> bool {
    let Some(screen) = capture_session_terminal_text(zmx_name).await else {
        return false;
    };
    crate::session_chat_notice::session_chat_screen_ends_with_shell_prompt(&screen)
        || (crate::session_chat_composer::has_session_chat_composer_signature(agent)
            && crate::session_chat_composer::detect_session_chat_composer_readiness(
                agent, &screen, None,
            )
            .input_box_missing())
}

fn claim_pending_first_user_input_draft_for_chat(
    state: &AppState,
    target: &SessionChatSendTarget,
) -> std::result::Result<Option<String>, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    crate::server::claim_first_user_input_draft_for_chat(
        &repository,
        &target.project_id,
        &target.session_id,
    )
}

/// Transfer a terminal draft into Chat when the session can answer the editor handshake.
pub(crate) async fn handle_handoff_session_chat_draft_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let target = match resolve_session_chat_send_target(state, &params, "handoffSessionChatDraft") {
        Ok(target) => target,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let pending = open_gxserver_database(&state.paths)
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: error.to_string(),
        })
        .and_then(|db| {
            crate::session_chat_draft_handoffs::pending_chat(
                &db,
                &target.project_id,
                &target.session_id,
            )
        });
    match pending {
        Ok(Some(result)) => {
            return routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, result),
            )
        }
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
        Ok(None) => {}
    }
    /*
    CDXC:Drafts 2026-09-02:
    A staged first-input draft that has not reached the terminal yet is handed
    straight to Chat here, before any terminal capture: the auto-switch into
    Chat of a freshly created handoff session lands while the terminal typing
    is still waiting for the CLI composer, so the handshake below would find
    an empty composer and the mention would later be typed behind Chat's back.
    */
    match claim_pending_first_user_input_draft_for_chat(state, &target) {
        Ok(Some(content)) => {
            return routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(
                    request_id,
                    json!({ "content": content, "transferred": true }),
                ),
            );
        }
        Ok(None) => {}
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    }
    // Pi and OMP hand their input box over through the chat channel's editor commands
    // (pi_chat_actions.rs), without the screen check and Ctrl+G handshake below.
    if crate::pi_chat_actions::is_pi_family(&target.session)
        && !session_chat_agent_command_is_user_hop(&target.session)
    {
        if let Some(result) = crate::pi_chat_actions::editor_draft_to_chat(state, &target).await {
            return match result {
                Ok(result) => routed_json(
                    Some(endpoint_path),
                    StatusCode::OK,
                    rpc_success(request_id, result),
                ),
                Err(error) => domain_error_response(endpoint_path, request_id, error),
            };
        }
    }
    let composer_agent =
        crate::session_chat_composer::session_chat_composer_agent_id(&target.session);
    // An OpenCode whose Ghostex TUI plugin has its chat channel open is running and opens its own
    // prompt editor (draft_capture.rs), so its screen need not be read for an input box first.
    let opencode_plugin_open = composer_agent.as_deref() == Some("opencode")
        && crate::pi_chat_channel::live(&target.project_id, &target.session_id)
            .is_some_and(|channel| channel.supports("promptEditor"));
    if session_chat_agent_command_is_user_hop(&target.session)
        || (!opencode_plugin_open
            && terminal_holds_no_agent_draft(&target.zmx_name, composer_agent.as_deref()).await)
    {
        return routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, json!({ "content": "", "transferred": false })),
        );
    }
    let agent = session_chat_agent_for_session(&target.session);
    if let Some(agent @ ("empryo" | "zcode")) = agent.as_deref() {
        let recovered = input_box_draft_to_chat(state, &target, agent).await;
        return match recovered {
            Ok(result) => routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, result),
            ),
            Err(error) => domain_error_response(endpoint_path, request_id, error),
        };
    }
    let captured = crate::session_chat_send::capture_session_chat_terminal_draft(
        &state.paths.app_state_dir,
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        agent.as_deref(),
    )
    .await;
    // Back in Chat, Codex leaves its side conversation (session_chat_codex_side.rs). It runs
    // after the capture so a draft typed there moves to Chat instead of being cleared.
    if agent.as_deref() == Some("codex") {
        queue_codex_side_conversation_close(
            &target.project_id,
            &target.session_id,
            &target.zmx_name,
        );
    }
    let captured = match captured {
        Ok(captured) => captured,
        Err(message) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "internalError",
                    message,
                },
            );
        }
    };
    let Some(prompt_id) = captured.prompt_id else {
        return routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, json!({ "content": "", "transferred": false })),
        );
    };
    let recovered = match read_and_release_stashed_prompt(
        state,
        &target.project_id,
        &target.session_id,
        &prompt_id,
        captured.created,
        captured.draft_version,
    ) {
        Ok(content) => content,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(request_id, recovered),
    )
}

/// CDXC:Drafts 2026-10-06 WHY:
/// Every other agent hands its terminal draft over through its prompt editor (Ctrl+G), and Ctrl+G opens Empryo's Git menu, so each switch to Chat left that menu open and failed after 16 seconds (seen live 2026-10-06). Empryo's draft is read off its input box instead, saved to Saved Prompts like the editor route saves it, and only then cleared with Empryo's own Ctrl+U clear.
/// CDXC:Drafts 2026-10-18 WHY:
/// ZCode never answers the Ctrl+G handshake either, so every switch of a ZCode session between Chat and the terminal waited 16 seconds and failed with "The terminal did not finish preserving its current draft" (GitHub #254). Its draft is read off its input box the same way and cleared with the send's own clear.
async fn input_box_draft_to_chat(
    state: &AppState,
    target: &SessionChatSendTarget,
    agent: &str,
) -> std::result::Result<Value, DomainStateError> {
    let internal = |message: String| DomainStateError {
        code: "internalError",
        message,
    };
    let Some(content) = crate::session_chat_send::read_input_box_terminal_draft(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        agent,
    )
    .await
    .map_err(internal)?
    else {
        return Ok(json!({ "content": "", "transferred": false }));
    };
    let saved = {
        let db =
            open_gxserver_database(&state.paths).map_err(|error| internal(error.to_string()))?;
        let mut params = Map::new();
        params.insert("content".to_string(), json!(content));
        params.insert("draftHandoff".to_string(), json!(true));
        params.insert("projectId".to_string(), json!(target.project_id));
        params.insert("sessionId".to_string(), json!(target.session_id));
        DomainRepository::new(&db, state.metadata.server_id.as_str())
            .save_stashed_prompt(&params)?
    };
    let prompt_id = saved
        .pointer("/prompt/promptId")
        .and_then(Value::as_str)
        .ok_or_else(|| internal("The terminal draft could not be saved.".to_string()))?
        .to_string();
    let mut steps = Vec::new();
    if agent == "empryo" {
        steps.push(SessionChatSendStep::SelectEmpryoTab {
            wait_ms: crate::session_chat_empryo_tabs::EMPRYO_SEND_TAB_WAIT_MS,
        });
    }
    steps.push(SessionChatSendStep::ClearComposer {
        agent: agent.to_string(),
    });
    crate::session_chat_send::execute_session_chat_send(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        "session-chat-draft-handoff",
        steps,
    )
    .await
    .map_err(|error| internal(error.message))?;
    read_and_release_stashed_prompt(
        state,
        &target.project_id,
        &target.session_id,
        &prompt_id,
        saved
            .get("created")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        saved
            .get("draftVersion")
            .filter(|version| !version.is_null())
            .cloned()
            .and_then(|version| serde_json::from_value(version).ok()),
    )
}

/// CDXC:Drafts 2026-09-10 WHY:
/// The captured text must enter durable recovery and the transfer ledger before releasing its temporary stash. Previously a lost HTTP reply or a second toggle could destroy the only copy.
pub(crate) fn read_and_release_stashed_prompt(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    prompt_id: &str,
    created: bool,
    captured_version: Option<crate::session_chat_draft_versions::DraftVersion>,
) -> std::result::Result<Value, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let mut list_params = Map::new();
    list_params.insert("projectId".to_string(), json!(project_id));
    let listed = repository.list_stashed_prompts(&list_params)?;
    let content = listed
        .get("prompts")
        .and_then(Value::as_array)
        .and_then(|prompts| {
            prompts
                .iter()
                .find(|prompt| prompt.get("promptId").and_then(Value::as_str) == Some(prompt_id))
        })
        .and_then(|prompt| prompt.get("content"))
        .and_then(Value::as_str)
        .ok_or_else(|| DomainStateError {
            code: "notFound",
            message: "The transferred draft could not be recalled.".to_string(),
        })?
        .to_string();
    let version = match captured_version {
        Some(version) => version,
        None => crate::session_chat_draft_handoffs::returned_version(
            &db, project_id, session_id, &content,
        )?,
    };
    let id = uuid::Uuid::new_v4().to_string();
    crate::session_chat_draft_handoffs::stage(
        &db, project_id, session_id, &id, &content, &version, "chat",
    )?;
    crate::session_chat_draft_versions::save(
        &db,
        project_id,
        session_id,
        "gxserver-draft-handoff",
        &content,
        &version,
    )?;
    if created {
        let mut delete_params = Map::new();
        delete_params.insert("promptId".to_string(), json!(prompt_id));
        let _ = repository.delete_stashed_prompt(&delete_params);
    }
    Ok(
        json!({"content":content,"draftVersion":version,"handoffId":id,"transferred":!content.is_empty()}),
    )
}
