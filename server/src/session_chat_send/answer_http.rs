use super::*;

/*
CDXC:SessionChat 2026-08-01:
Second source for the question card, used when agent hooks never reported one:
re-read the session's transcript tail and look for an AskUserQuestion tool call
that has no tool result yet. Bounded to a short window and only reached on an
explicit answer action, so the directory scan cost is paid once per answer.
*/
pub(crate) const SESSION_CHAT_PROMPT_SCAN_LIMIT: usize = 60;

pub(crate) fn transcript_pending_question_prompt(
    session: &Value,
) -> Option<crate::session_chat::SessionChatInteractivePrompt> {
    let transcript_agent = crate::session_chat::resolve_session_chat_transcript_agent(
        session_chat_agent_for_session(session).as_deref(),
    )?;
    let path = crate::session_chat::resolve_session_chat_transcript_path(
        transcript_agent,
        read_runtime_text(session, "agentSessionId").as_deref(),
        read_runtime_text(session, "agentSessionPath").as_deref(),
    )?;
    // Read for every running session on each recovery pass (accounts/recovery.rs) and on sends;
    // kept while the transcript is unchanged (`file_memo`).
    static MEMO: std::sync::LazyLock<
        crate::file_memo::FileMemo<Option<crate::session_chat::SessionChatInteractivePrompt>>,
    > = std::sync::LazyLock::new(crate::file_memo::FileMemo::new);
    MEMO.get_or_read(&path, || {
        let crate::session_chat::SessionChatTailPage::Page { messages, .. } =
            crate::session_chat::read_session_chat_tail_page(
                transcript_agent,
                &path,
                SESSION_CHAT_PROMPT_SCAN_LIMIT,
                None,
            )
            .ok()?
        else {
            return None;
        };
        crate::session_chat::scan_transcript_prompt_state(&messages)
            .pending()
            .cloned()
    })
}

/*
CDXC:SessionChat 2026-11-05 DECISION:
User (security audit, Q16 "check those four, and fix the ones that are real"): an answer is applied only to the prompt its card showed. A picker row (`terminalChoice`) and a notice's key action used to go to whatever the screen showed when gxserver got them, as long as that screen had a row with the same number, and the hook approval card's Allow typed its key into whatever dialog was up. So an answer picked on prompt A, or retried after a reconnect, could confirm prompt B, including B's "don't ask again" row. Clients now say which card they answered: a notice by its `detectedAt` (the instance id, kept while the screen keeps saying the same thing and new for a different prompt), the hook approval card by its tool and summary. gxserver compares that with what it would show now (a fresh capture, merged with the watchdog card as the chats see it) and refuses a mismatch, so the card comes back with the current prompt. Answers from an older client that names no card keep working as before. Deny and Escape stay unbound: they only stop whatever is asking.
SEE-ALSO: packages/gx-chat-core/src/questions/terminal_prompts.rs and actions.rs (what the chat core sends).
*/
async fn verify_answer_targets_shown_prompt(
    state: &AppState,
    target: &SessionChatSendTarget,
    params: &serde_json::Map<String, Value>,
) -> Result<(), DomainStateError> {
    let changed = || DomainStateError {
        code: "invalidState",
        message: "The prompt changed before your answer arrived. Answer the one showing now."
            .to_string(),
    };
    if let Some(shown) = params
        .get("noticeDetectedAt")
        .and_then(Value::as_str)
        .filter(|shown| !shown.is_empty())
    {
        let agent = session_chat_agent_for_session(&target.session);
        let detected = crate::session_chat_options::SessionChatOptionDetector::new(state)
            .detect(
                &target.project_id,
                &target.session_id,
                agent.as_deref(),
                true,
            )
            .await;
        let current = crate::session_chat_notice::resolve_session_chat_terminal_notice(
            &target.project_id,
            &target.session_id,
            detected.notice,
        );
        return match current {
            Some(notice) if notice.detected_at == shown => Ok(()),
            _ => Err(changed()),
        };
    }
    if let Some(shown_tool) = params.get("promptTool").and_then(Value::as_str) {
        let shown_summary = params.get("promptSummary").and_then(Value::as_str);
        let shown_tool_use_id = params.get("toolUseId").and_then(Value::as_str);
        let stored = crate::agents::session_chat_prompt_setting(&target.session)
            .as_deref()
            .and_then(crate::session_chat::parse_stored_session_chat_prompt);
        let same = match stored {
            Some(crate::session_chat::SessionChatInteractivePrompt::Approval {
                tool,
                summary,
                tool_use_id,
            }) => {
                tool == shown_tool
                    && summary.as_deref() == shown_summary
                    && match (shown_tool_use_id, tool_use_id.as_deref()) {
                        (Some(shown), Some(stored)) => shown == stored,
                        _ => true,
                    }
            }
            _ => false,
        };
        if !same {
            return Err(changed());
        }
    }
    Ok(())
}

pub(crate) async fn handle_answer_session_chat_prompt_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let target = match resolve_session_chat_send_target(state, &params, "answerSessionChatPrompt") {
        Ok(target) => target,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let kind = params
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default();
    // "For later" and removing a question form or page without answering it are the chat's own
    // bookkeeping: nothing reaches the agent (session_chat_open_questions.rs, `set_later` and
    // `set_removed`).
    if kind == "questionForLater" || kind == "questionRemove" {
        let Some(question_id) = params
            .get("questionId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= 4096)
            .map(str::to_string)
        else {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "invalidParams",
                    message: "A question ID is required.".to_string(),
                },
            );
        };
        let remove = kind == "questionRemove";
        let later = params.get("later").and_then(Value::as_bool).unwrap_or(true);
        let worker_state = state.clone();
        let (project_id, session_id) = (target.project_id.clone(), target.session_id.clone());
        let result = tokio::task::spawn_blocking(move || {
            if remove {
                crate::session_chat_open_questions::set_removed(
                    &worker_state,
                    &project_id,
                    &session_id,
                    &question_id,
                )
            } else {
                crate::session_chat_open_questions::set_later(
                    &worker_state,
                    &project_id,
                    &session_id,
                    &question_id,
                    later,
                )
            }
        })
        .await;
        return match result {
            Ok(Ok(())) => routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, json!({ "later": later, "removed": remove })),
            ),
            Ok(Err(error)) => domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "internalError",
                    message: error.to_string(),
                },
            ),
            Err(_) => domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "internalError",
                    message: "Could not save the question's state.".to_string(),
                },
            ),
        };
    }
    // A Connectors approval card is answered in gxserver: the agent waits inside a tool call, so
    // nothing is typed (crate::connectors::agent). Only a question answer can be meant for one.
    if let Some(answered) =
        crate::connectors::answer_connector_card_from_chat(state, &target.session, &params)
    {
        return match answered {
            Ok(result) => routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, result),
            ),
            Err(error) => domain_error_response(endpoint_path, request_id, error),
        };
    }
    // A notice action (Install OpenCode 2, Restart) is answered by gxserver, not by the OpenCode
    // conversation, which may not exist yet (session_chat_agent_restart.rs).
    if session_chat_agent_for_session(&target.session).as_deref() == Some("opencode")
        && kind != "restartAgent"
    {
        let id = match crate::session_chat_opencode::session_id(&target.session) {
            Ok(id) => id,
            Err(error) => return domain_error_response(endpoint_path, request_id, error),
        };
        let answer_params = params.clone();
        let result = tokio::task::spawn_blocking(move || {
            crate::session_chat_opencode::answer(&id, &answer_params)
        })
        .await;
        match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => return domain_error_response(endpoint_path, request_id, error),
            Err(_) => {
                return domain_error_response(
                    endpoint_path,
                    request_id,
                    crate::session_chat_opencode::error("OpenCode answer task failed."),
                )
            }
        }
        schedule_session_chat_option_redetect(
            state,
            &target.project_id,
            &target.session_id,
            Some("opencode"),
        );
        return routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, json!({"answered":true})),
        );
    }
    if matches!(kind, "asyncQuestion" | "dismissAsyncQuestion") {
        let Some(question_id) = params
            .get("questionId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= 4096)
        else {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "invalidParams",
                    message: "A question ID is required.".to_string(),
                },
            );
        };
        if session_chat_agent_for_session(&target.session).as_deref() != Some("codex") {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "invalidParams",
                    message: "Only Codex supports asynchronous questions.".into(),
                },
            );
        }
        let answer_text = if kind == "asyncQuestion" {
            let Some(text) = params
                .get("text")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| {
                    !text.is_empty()
                        && text.len() <= 32_768
                        && !text
                            .chars()
                            .any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
                })
            else {
                return domain_error_response(
                    endpoint_path,
                    request_id,
                    DomainStateError {
                        code: "invalidParams",
                        message: "A non-empty question answer is required (up to 32 KB).".into(),
                    },
                );
            };
            Some(text.to_string())
        } else {
            None
        };
        let session = target.session.clone();
        let id = question_id.to_string();
        let answer = tokio::task::spawn_blocking(move || {
            crate::session_chat_codex_async_answer::resolve(&session, &id, answer_text)
        })
        .await;
        let answer = match answer {
            Ok(Ok(answer)) => answer,
            result => {
                return domain_error_response(
                    endpoint_path,
                    request_id,
                    DomainStateError {
                        code: "invalidParams",
                        message: match result {
                            Ok(Err(message)) => message,
                            _ => "Could not read Codex's question.".into(),
                        },
                    },
                )
            }
        };
        if let Some(answer) = answer {
            if let Err(error) = execute_session_chat_send(
                &target.project_id,
                &target.session_id,
                &target.zmx_name,
                "asyncQuestion",
                vec![SessionChatSendStep::DriveCodexAsyncQuestion(answer)],
            )
            .await
            {
                return domain_error_response(
                    endpoint_path,
                    request_id,
                    DomainStateError {
                        code: "agentBusy",
                        message: error.message,
                    },
                );
            }
        }
        return match crate::session_chat_async_questions::dismiss(
            state,
            &target.project_id,
            &target.session_id,
            question_id,
        ) {
            Ok(()) => routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, json!({ "queued": false })),
            ),
            Err(error) => domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "internalError",
                    message: error.to_string(),
                },
            ),
        };
    }
    if kind == "restartAgent" {
        return match crate::session_chat_agent_restart::restart(state, &params).await {
            Ok(()) => routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, json!({ "queued": false })),
            ),
            Err(error) => domain_error_response(endpoint_path, request_id, error),
        };
    }
    if kind == "recoverCodexConversation" {
        let result = crate::session_chat_codex_lock::recover(state, &params).await;
        schedule_session_chat_option_redetect(
            state,
            &target.project_id,
            &target.session_id,
            Some("codex"),
        );
        return match result {
            Ok(()) => routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, json!({ "queued": false })),
            ),
            Err(error) => domain_error_response(endpoint_path, request_id, error),
        };
    }
    if kind == "terminalKey" {
        let Some(agent) = crate::agents::identity::normalize_agent_id(
            session_chat_agent_for_session(&target.session).as_deref(),
        ) else {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "invalidParams",
                    message: "This session has no agent to send keys to.".to_string(),
                },
            );
        };
        let result =
            crate::session_chat_resume_picker::answer_terminal_key(&target, &agent, &params).await;
        // The card draws the screen the key changed; republish it now, not on the next probe.
        crate::session_chat_options::SessionChatOptionDetector::new(state)
            .detect(&target.project_id, &target.session_id, Some(&agent), true)
            .await;
        crate::session_chat_options::session_chat_terminal_notice_publisher(
            state,
            &target.project_id,
            &target.session_id,
        )();
        schedule_session_chat_option_redetect(
            state,
            &target.project_id,
            &target.session_id,
            Some(&agent),
        );
        return match result {
            Ok(result) => routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, result),
            ),
            Err(error) => domain_error_response(endpoint_path, request_id, error),
        };
    }
    if kind == "terminalDialog" {
        let agent = crate::session_chat_options::session_chat_option_agent(
            session_chat_agent_for_session(&target.session).as_deref(),
        );
        let picker = crate::session_chat_resume_picker::is_picker_answer(&params);
        let (agent_name, result) = match agent {
            Some(crate::session_chat_options::SessionChatOptionAgent::Codex) if picker => (
                "codex",
                crate::session_chat_resume_picker::answer_resume_picker(&target, "codex", &params)
                    .await,
            ),
            Some(crate::session_chat_options::SessionChatOptionAgent::Claude) if picker => (
                "claude",
                crate::session_chat_resume_picker::answer_resume_picker(&target, "claude", &params)
                    .await,
            ),
            Some(crate::session_chat_options::SessionChatOptionAgent::Codex) => (
                "codex",
                crate::session_chat_codex_dialog::answer_codex_dialog(state, &target, &params)
                    .await,
            ),
            Some(crate::session_chat_options::SessionChatOptionAgent::Claude) => (
                "claude",
                crate::session_chat_claude_dialog::answer_claude_dialog(&target, &params).await,
            ),
            _ => {
                return domain_error_response(
                    endpoint_path,
                    request_id,
                    DomainStateError {
                        code: "invalidParams",
                        message: "This agent does not offer terminal dialogs.".to_string(),
                    },
                )
            }
        };
        crate::session_chat_options::SessionChatOptionDetector::new(state)
            .detect(
                &target.project_id,
                &target.session_id,
                Some(agent_name),
                true,
            )
            .await;
        crate::session_chat_options::session_chat_terminal_notice_publisher(
            state,
            &target.project_id,
            &target.session_id,
        )();
        schedule_session_chat_option_redetect(
            state,
            &target.project_id,
            &target.session_id,
            Some(agent_name),
        );
        return match result {
            Ok(result) => routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, result),
            ),
            Err(error) => domain_error_response(endpoint_path, request_id, error),
        };
    }
    if kind == "trustAndRemember" {
        let result =
            crate::session_chat_trust_memory::answer_trust_and_remember(state, &target).await;
        let agent = session_chat_agent_for_session(&target.session);
        schedule_session_chat_option_redetect(
            state,
            &target.project_id,
            &target.session_id,
            agent.as_deref(),
        );
        return match result {
            Ok(result) => routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, result),
            ),
            Err(error) => domain_error_response(endpoint_path, request_id, error),
        };
    }
    // Set when the terminal row answered is the No of a tool permission prompt.
    let mut denied_permission_row = false;
    let steps = match kind {
        "approval" => {
            // Allow → the option's raw send byte ("1"); Deny/empty → ESC.
            // Raw, no bracketed paste, no delayed Enter (upstream chat spec §8.3).
            let approval_send = params
                .get("approvalSend")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !approval_send.is_empty() {
                if let Err(error) =
                    verify_answer_targets_shown_prompt(state, &target, &params).await
                {
                    return domain_error_response(endpoint_path, request_id, error);
                }
            }
            let payload = if approval_send.is_empty() {
                crate::session_chat_send::SESSION_CHAT_INTERRUPT.to_string()
            } else {
                approval_send.to_string()
            };
            vec![crate::session_chat_send::SessionChatSendStep::Write(
                payload,
            )]
        }
        "question" => {
            let agent = session_chat_agent_for_session(&target.session);
            let screen_prompt =
                if crate::session_chat_options::session_chat_questions_only_on_screen(
                    agent.as_deref(),
                ) {
                    crate::session_chat_options::SessionChatOptionDetector::new(state)
                        .detect(
                            &target.project_id,
                            &target.session_id,
                            agent.as_deref(),
                            true,
                        )
                        .await
                        .prompt
                } else {
                    None
                };
            let stored_prompt = crate::agents::session_chat_prompt_setting(&target.session)
                .as_deref()
                .and_then(crate::session_chat::parse_stored_session_chat_prompt)
                // A card the transcript produced (hooks that never forwarded
                // toolInput) must be answerable too, or the user gets a card
                // that rejects every answer.
                .or_else(|| transcript_pending_question_prompt(&target.session))
                .or(screen_prompt);
            let Some(crate::session_chat::SessionChatInteractivePrompt::Question {
                questions, ..
            }) = stored_prompt
            else {
                return domain_error_response(
                    endpoint_path,
                    request_id,
                    DomainStateError {
                        code: "invalidParams",
                        message: "The session has no pending question prompt.".to_string(),
                    },
                );
            };
            let selections: Vec<crate::session_chat::SessionChatQuestionSelection> =
                match params.get("selections").cloned() {
                    None => Vec::new(),
                    Some(value) => match serde_json::from_value(value) {
                        Ok(selections) => selections,
                        Err(error) => {
                            return domain_error_response(
                                endpoint_path,
                                request_id,
                                DomainStateError {
                                    code: "invalidParams",
                                    message: format!(
                                        "answerSessionChatPrompt selections are malformed: {error}"
                                    ),
                                },
                            );
                        }
                    },
                };
            // One agent can host different asking tools with different
            // terminal UIs (omp ships its own `ask` dialog next to the pi
            // cursor bridge), so the key plan follows the tool that asked.
            let question_tool = questions
                .first()
                .and_then(|question| question.tool_name.as_deref())
                .map(crate::session_chat::normalize_session_chat_tool_name);
            match agent.as_deref() {
                Some("claude" | "openclaude") => {
                    // See CDXC:AgentScreenDetection in session_chat_question_liveness.rs.
                    use crate::session_chat_claude_question_answer::ClaudeQuestionScreen;
                    let screen =
                        crate::session_chat_claude_question_answer::read_claude_question_screen(
                            &target.zmx_name,
                            &questions,
                        )
                        .await;
                    if screen == ClaudeQuestionScreen::Unreadable {
                        return domain_error_response(
                            endpoint_path,
                            request_id,
                            DomainStateError {
                                code: "invalidState",
                                message: "The session's terminal could not be read, so the answer was not sent."
                                    .to_string(),
                            },
                        );
                    }
                    let preview_question_without_option = (screen != ClaudeQuestionScreen::Gone)
                        .then(|| claude_preview_question_without_option(&questions, &selections))
                        .flatten();
                    if let Some(question) = preview_question_without_option {
                        return domain_error_response(
                            endpoint_path,
                            request_id,
                            DomainStateError {
                                code: "invalidParams",
                                message: format!(
                                    "Pick an option for \"{}\": Claude only takes typed text as a note on the option you pick for this question.",
                                    question.header.as_deref().unwrap_or(&question.question)
                                ),
                            },
                        );
                    }
                    let answer_message = || {
                        crate::session_chat_send::build_session_chat_message_steps(
                            agent.as_deref(),
                            &crate::session_chat_question_liveness::format_ask_answer_message(
                                &questions,
                                &selections,
                            ),
                            &[],
                            false,
                        )
                    };
                    let has_answer = crate::session_chat_send::has_ask_answer(&selections);
                    match screen {
                        ClaudeQuestionScreen::Placed(position) => {
                            // See CDXC:SessionChat in session_chat_question_liveness.rs.
                            let mut groups =
                                crate::session_chat_question_liveness::claude_question_selector_reset_keys(
                                    position,
                                );
                            groups.extend(crate::session_chat_send::build_claude_ask_answer_keys(
                                &questions,
                                &selections,
                            ));
                            crate::session_chat_send::build_ask_answer_steps(&groups)
                        }
                        ClaudeQuestionScreen::Gone if !has_answer => Vec::new(),
                        ClaudeQuestionScreen::Gone => answer_message(),
                        // See CDXC:SessionChat on `claude_question_dialog_at_bottom`.
                        ClaudeQuestionScreen::Unplaced { cancellable: true } if has_answer => {
                            let mut steps = vec![
                                crate::session_chat_send::SessionChatSendStep::CancelClaudeQuestion(
                                    questions.clone(),
                                ),
                            ];
                            steps.extend(answer_message());
                            steps
                        }
                        ClaudeQuestionScreen::Unplaced { .. }
                        | ClaudeQuestionScreen::Unreadable => {
                            return domain_error_response(
                                endpoint_path,
                                request_id,
                                DomainStateError {
                                    code: crate::session_chat_send::ANSWER_IN_TERMINAL,
                                    message: "Claude's question is on screen, but Ghostex could not read where its highlight sits, so the answer was not sent. Open the terminal to answer it there."
                                        .to_string(),
                                },
                            );
                        }
                    }
                }
                Some("codex") => crate::session_chat_send::build_ask_answer_steps(
                    &crate::session_chat_send::build_codex_ask_answer_keys(&questions, &selections),
                ),
                Some("antigravity") => {
                    let on_screen =
                        crate::session_chat_send::capture_session_terminal_text(&target.zmx_name)
                            .await
                            .is_some_and(|screen_text| {
                                crate::session_chat_send::antigravity_question_panel_at_start(
                                    &questions,
                                    &screen_text,
                                )
                            });
                    if !on_screen {
                        return domain_error_response(
                            endpoint_path,
                            request_id,
                            DomainStateError {
                                code: crate::session_chat_send::ANSWER_IN_TERMINAL,
                                message: "Antigravity's question is not on screen at its first question, so the answer was not sent. Answer it in the terminal."
                                    .to_string(),
                            },
                        );
                    }
                    crate::session_chat_send::build_ask_answer_steps(
                        &crate::session_chat_send::build_antigravity_ask_answer_keys(
                            &questions,
                            &selections,
                        ),
                    )
                }
                Some("copilot") => {
                    let keys =
                        crate::session_chat_send::capture_session_terminal_text(&target.zmx_name)
                            .await
                            .and_then(|screen_text| {
                                crate::session_chat_copilot_question::detect_copilot_question_form(
                                    &screen_text,
                                )
                            })
                            .and_then(|form| {
                                crate::session_chat_copilot_question::build_copilot_ask_answer_keys(
                                    &form,
                                    &questions,
                                    &selections,
                                )
                            });
                    let Some(keys) = keys else {
                        return domain_error_response(
                            endpoint_path,
                            request_id,
                            DomainStateError {
                                code: crate::session_chat_send::ANSWER_IN_TERMINAL,
                                message: "Copilot's question is not on screen the way the card shows it, so the answer was not sent. Answer it in the terminal."
                                    .to_string(),
                            },
                        );
                    };
                    crate::session_chat_send::build_ask_answer_steps(&keys)
                }
                Some("freebuff") => {
                    let form = crate::session_chat_send::capture_session_terminal_text(
                        &target.zmx_name,
                    )
                    .await
                    .and_then(|screen_text| {
                        crate::session_chat_freebuff_question::detect_freebuff_question_form(
                            &screen_text,
                        )
                    })
                    .filter(|form| {
                        questions
                            .iter()
                            .any(|question| question.question == form.question.question)
                    });
                    let Some(form) = form else {
                        return domain_error_response(
                            endpoint_path,
                            request_id,
                            DomainStateError {
                                code: crate::session_chat_send::ANSWER_IN_TERMINAL,
                                message: "Freebuff's question is not on screen, so the answer was not sent. Answer it in the terminal."
                                    .to_string(),
                            },
                        );
                    };
                    crate::session_chat_send::build_ask_answer_steps(
                        &crate::session_chat_freebuff_question::build_freebuff_ask_answer_keys(
                            &form,
                            &questions,
                            &selections,
                        ),
                    )
                }
                Some("empryo") if !crate::session_chat_send::has_ask_answer(&selections) => {
                    Vec::new()
                }
                Some("empryo") => {
                    let keys =
                        crate::session_chat_send::capture_session_terminal_text(&target.zmx_name)
                            .await
                            .and_then(|screen_text| {
                                crate::session_chat_empryo_question::build_empryo_ask_answer_keys(
                                    &screen_text,
                                    &questions,
                                    &selections,
                                )
                            });
                    let Some(keys) = keys else {
                        return domain_error_response(
                            endpoint_path,
                            request_id,
                            DomainStateError {
                                code: crate::session_chat_send::ANSWER_IN_TERMINAL,
                                message: "Empryo's question is not on screen, so the answer was not sent. Answer it in the terminal."
                                    .to_string(),
                            },
                        );
                    };
                    crate::session_chat_send::build_ask_answer_steps(&keys)
                }
                Some("cursor") => crate::session_chat_send::build_ask_answer_steps(
                    &crate::session_chat_send::build_cursor_ask_answer_keys(
                        &questions,
                        &selections,
                    ),
                ),
                // omp's built-in `ask` renders its rich dialog; every other
                // question on a pi-family session is the pi-tui select the
                // cursor bridge owns while a cursor_ask_question is pending.
                Some("pi") if question_tool.as_deref() == Some("ask") => {
                    crate::session_chat_send::build_ask_answer_steps(
                        &crate::session_chat_send::build_omp_ask_answer_keys(
                            &questions,
                            &selections,
                        ),
                    )
                }
                Some("pi") => crate::session_chat_send::build_ask_answer_steps(
                    &crate::session_chat_send::build_pi_ask_answer_keys(&questions, &selections),
                ),
                Some("grok") => {
                    let on_screen =
                        crate::session_chat_send::capture_session_terminal_text(&target.zmx_name)
                            .await
                            .is_some_and(|screen_text| {
                                crate::session_chat_grok_question::grok_question_card_on_screen(
                                    &questions,
                                    &screen_text,
                                )
                            });
                    if !on_screen {
                        return domain_error_response(
                            endpoint_path,
                            request_id,
                            DomainStateError {
                                code: crate::session_chat_send::ANSWER_IN_TERMINAL,
                                message: "Grok Build's question is not on screen, so the answer was not sent. Answer it in the terminal."
                                    .to_string(),
                            },
                        );
                    }
                    crate::session_chat_send::build_ask_answer_steps(
                        &crate::session_chat_grok_question::build_grok_ask_answer_keys(
                            &questions,
                            &selections,
                        ),
                    )
                }
                // ZCode's question list replaces its input box while it waits, so the answer is
                // driven through the list (session_chat_zcode_question.rs).
                Some("zcode") => {
                    let on_screen =
                        crate::session_chat_send::capture_session_terminal_text(&target.zmx_name)
                            .await
                            .is_some_and(|screen_text| {
                                crate::session_chat_zcode_question::zcode_question_on_screen(
                                    &questions,
                                    &screen_text,
                                )
                            });
                    if !on_screen {
                        return domain_error_response(
                            endpoint_path,
                            request_id,
                            DomainStateError {
                                code: crate::session_chat_send::ANSWER_IN_TERMINAL,
                                message: "ZCode's question is not on screen at its first question, so the answer was not sent. Answer it in the terminal."
                                    .to_string(),
                            },
                        );
                    }
                    crate::session_chat_send::build_ask_answer_steps(
                        &crate::session_chat_zcode_question::build_zcode_ask_answer_keys(
                            &questions,
                            &selections,
                        ),
                    )
                }
                // Hermes' clarify panel owns the composer while it is open (the
                // Enter binding routes buffer text to the clarify queue), so
                // the composer fallback would corrupt it; drive the panel's
                // digit/freetext keys instead.
                Some("hermes") => crate::session_chat_send::build_ask_answer_steps(
                    &crate::session_chat_send::build_hermes_ask_answer_keys(
                        &questions,
                        &selections,
                    ),
                ),
                _ => {
                    /*
                    Non-stepping agents: the formatted answer text goes
                    through the normal send path (upstream chat spec §8.6).

                    That path's clear burst is kept deliberately. These agents
                    render no selector that owns the input line — the answer is
                    an ordinary message typed into the composer and submitted —
                    so a draft already sitting there would be prepended to the
                    answer and submitted as part of it. The tool is waiting for
                    the answer text and nothing else, so writing verbatim would
                    corrupt the answer AND send the draft; clearing first is the
                    only write that is correct. It also keeps the paste
                    verification, so an answer the composer never took is never
                    followed by an Enter.
                    */
                    if !crate::session_chat_send::has_ask_answer(&selections) {
                        Vec::new()
                    } else {
                        crate::session_chat_send::build_session_chat_message_steps(
                            agent.as_deref(),
                            &crate::session_chat_send::format_ask_answer(&questions, &selections),
                            &[],
                            false,
                        )
                    }
                }
            }
        }
        /*
        CDXC:SessionChat 2026-08-21:
        A row of an on-screen picker (Claude Code's resume-usage chooser), which
        the chat surface renders from the `choices` its terminal notice carries.

        The keystroke is derived from a capture taken RIGHT NOW rather than from
        the detection that painted the card: the notice can be seconds old, and
        the picker may have been answered in the terminal — or repainted with
        different rows — in between. A picker that is no longer on screen is an
        error, never a blind keystroke into whatever replaced it.
        */
        "terminalChoice" => {
            let Some(choice_index) = params
                .get("choiceIndex")
                .and_then(Value::as_u64)
                .map(|index| index as usize)
            else {
                return domain_error_response(
                    endpoint_path,
                    request_id,
                    DomainStateError {
                        code: "invalidParams",
                        message:
                            "answerSessionChatPrompt kind \"terminalChoice\" requires choiceIndex."
                                .to_string(),
                    },
                );
            };
            if let Err(error) = verify_answer_targets_shown_prompt(state, &target, &params).await {
                return domain_error_response(endpoint_path, request_id, error);
            }
            let agent = session_chat_agent_for_session(&target.session);
            let screen =
                crate::session_chat_send::capture_session_terminal_text(&target.zmx_name).await;
            let skips_outright = screen.as_deref().is_some_and(|text| {
                crate::session_chat_options::session_chat_option_agent(agent.as_deref())
                    == Some(crate::session_chat_options::SessionChatOptionAgent::Cursor)
                    && crate::session_chat_cursor_decision::cursor_decision_skips_outright(
                        text,
                        choice_index,
                    )
            });
            let answer_key = screen
                    .as_deref()
                    .and_then(|text| {
                        let option_agent = crate::session_chat_options::session_chat_option_agent(
                            agent.as_deref(),
                        );
                        if agent.as_deref() == Some("empryo") {
                            crate::session_chat_empryo_question::empryo_approval_answer_key(
                                text,
                                choice_index,
                            )
                        } else if option_agent
                            == Some(crate::session_chat_options::SessionChatOptionAgent::Pi)
                        {
                            crate::session_chat_pi_blocking::pi_trust_answer_key(text, choice_index)
                        } else if option_agent
                            == Some(crate::session_chat_options::SessionChatOptionAgent::Antigravity)
                            && crate::session_chat_antigravity_permission::detect_antigravity_permission_picker(text)
                                .is_some()
                        {
                            crate::session_chat_antigravity_permission::detect_antigravity_permission_picker(text)
                                .and_then(|picker| picker.answer_key(choice_index))
                        } else if option_agent
                            == Some(crate::session_chat_options::SessionChatOptionAgent::Copilot)
                            && crate::session_chat_copilot_permission::detect_copilot_permission_picker(text)
                                .is_some()
                        {
                            crate::session_chat_copilot_permission::detect_copilot_permission_picker(text)
                                .and_then(|picker| picker.answer_key(choice_index))
                        } else if option_agent
                            == Some(crate::session_chat_options::SessionChatOptionAgent::Grok)
                            && crate::session_chat_grok_permission::detect_grok_permission_picker(text)
                                .is_some()
                        {
                            crate::session_chat_grok_permission::detect_grok_permission_picker(text)
                                .and_then(|picker| picker.answer_key(choice_index))
                        } else if option_agent
                            == Some(crate::session_chat_options::SessionChatOptionAgent::Cursor)
                            && crate::session_chat_cursor_decision::detect_cursor_decision(text)
                                .is_some()
                        {
                            crate::session_chat_cursor_decision::cursor_decision_answer_key(
                                text,
                                choice_index,
                            )
                        } else {
                            crate::session_chat_workspace_trust::workspace_trust_answer_key(
                        agent.as_deref(),
                        text,
                        choice_index,
                    )
                    .or_else(|| {
                        crate::session_chat_resume_prompt::detect_session_chat_terminal_picker(text)
                            .and_then(|picker| picker.answer_key(choice_index))
                    })
                        }
                    });
            let Some(answer_key) = answer_key else {
                return domain_error_response(
                    endpoint_path,
                    request_id,
                    DomainStateError {
                        code: "invalidState",
                        message: "The picker on screen no longer offers that option.".to_string(),
                    },
                );
            };
            denied_permission_row = screen.as_deref().is_some_and(|text| {
                crate::session_chat_resume_prompt::permission_prompt_row_denies(text, choice_index)
            });
            if skips_outright {
                vec![
                    crate::session_chat_send::SessionChatSendStep::Write("\u{1b}".to_string()),
                    crate::session_chat_send::SessionChatSendStep::SkipCursorInsteadBox,
                ]
            } else {
                crate::session_chat_send::build_terminal_picker_answer_steps(&answer_key)
            }
        }
        _ => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "invalidParams",
                    message:
                        "answerSessionChatPrompt kind must be \"question\", \"approval\" or \"terminalChoice\"."
                            .to_string(),
                },
            );
        }
    };
    let queued = !steps.is_empty();
    if queued {
        crate::session_chat_send::enqueue_session_chat_send(
            &target.project_id,
            &target.session_id,
            &target.zmx_name,
            "session-chat-answer",
            steps,
        );
    }
    /*
    CDXC:SessionChat 2026-10-28 WHY: the chat's approval card answers Claude's permission prompt
    through its numbered rows (`terminalChoice`), and its No row interrupts the tool call just as
    silently as Deny's Escape: no PostToolUse, no Stop. The stored card outlived the answer, so the
    session sat idle under the red "waiting for approval" dot until its next turn (found testing
    plan mode on Linux; gxserver, every platform). The No row retires the card like Deny does.
    */
    let denied_approval = denied_permission_row
        || kind == "approval"
            && params
                .get("approvalSend")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .is_empty();
    if denied_approval {
        if let Some(answered) = crate::agents::session_chat_prompt_setting(&target.session)
            .as_deref()
            .and_then(crate::session_chat::parse_stored_session_chat_prompt)
            .filter(|prompt| {
                matches!(
                    prompt,
                    crate::session_chat::SessionChatInteractivePrompt::Approval { .. }
                )
            })
        {
            crate::session_chat_interactive::retire_denied_session_chat_approval(
                state,
                &target.project_id,
                &target.session_id,
                &answered,
            );
        }
    }
    /*
    CDXC:SessionChat 2026-08-21:
    The card the user just answered is a TERMINAL NOTICE, and notices only
    retire when a fresh capture proves the screen is clean. Left to the
    follower's ~30s probe the answered picker would stay on screen in chat for
    half a minute, so borrow the post-dispatch redetect: it re-reads the screen
    at +2s and +6s and republishes, which is exactly the window the picker
    needs to tear down.
    */
    if matches!(kind, "terminalChoice" | "question" | "approval") {
        let agent = session_chat_agent_for_session(&target.session);
        schedule_session_chat_option_redetect(
            state,
            &target.project_id,
            &target.session_id,
            agent.as_deref(),
        );
    }
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(request_id, json!({ "queued": queued })),
    )
}
