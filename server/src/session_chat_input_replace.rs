use std::time::{Duration, Instant};

use super::{
    build_agent_tui_clear_input, capture_session_terminal_text_vt, write_session_chat_payload,
    SessionChatSendError, SessionChatSendFailure, AGENT_TUI_CLEAR_INPUT_LINE,
    AGENT_TUI_CLEAR_LINE_SLACK, AGENT_TUI_CLEAR_MAX_LINES, SESSION_CHAT_CLEAR_INPUT_SETTLE_MS,
    SESSION_CHAT_COMPOSER_WAIT_TIMEOUT_MS, SESSION_CHAT_INTERRUPT, SESSION_CHAT_SEND_CANCELLED,
};
use crate::session_chat_composer::{
    detect_session_chat_composer_readiness, session_chat_composer_input, SessionChatComposerState,
};

#[derive(Clone, Copy)]
enum ComposerClearMethod {
    InterruptOnce,
    KillLines,
    /// Ctrl+U alone, once before the box is read and again while text is left.
    KillLinesBackward,
}

fn composer_clear_method(agent: &str) -> Option<ComposerClearMethod> {
    match agent {
        /*
        CDXC:Drafts 2026-10-18 WHY:
        ZCode had no verified clear, so moving its terminal draft into the chat read the draft and then failed with "The terminal draft could not be cleared and verified". ZCode's editor is Pi's and so is its Ctrl+C (`@zcode/tui` 3.15.3, checked live): with text it empties the box, on an empty box it ends a running turn or quits ZCode, so it takes Pi's one Ctrl+C after the box shows text.
        */
        "claude" | "codex" | "cursor" | "grok" | "hermes-agent" | "pi" | "omp" | "zcode" => {
            Some(ComposerClearMethod::InterruptOnce)
        }
        "antigravity" | "openclaude" => Some(ComposerClearMethod::KillLines),
        /*
        CDXC:SessionChat 2026-10-06 WHY:
        Empryo 3.9.0-beta quits on Ctrl+C in an empty input and opens its command palette on Ctrl+K, so the shared Ctrl+U/Ctrl+K burst typed every chat message into the palette. Ctrl+U clears its input line, a folded paste included, and does nothing to an empty one, so the clear is Ctrl+U alone: one burst before the box is read (a capture without a cursor cannot tell its tip from typed text), then more while text is left. Attached images take Ctrl+C (EMPRYO_ATTACHMENT_SEED).
        */
        "empryo" => Some(ComposerClearMethod::KillLinesBackward),
        _ => None,
    }
}

/// CDXC:SessionChat 2026-10-06 WHY:
/// Empryo's attached images survive Ctrl+U and Backspace; only its Ctrl+C clear drops them, and only while the box holds text (Ctrl+C on an empty box quits Empryo). So a box holding just images first gets this one character, and Ctrl+C follows once a capture shows it typed.
const EMPRYO_ATTACHMENT_SEED: &str = "x";

/// CDXC:SessionChat 2026-10-09 WHY:
/// The 2026-10-09 coordinator screen showed Claude's input box as `❯�` under "Press Ctrl-C again to exit": the box was empty to Claude but not to the reader, so the clear's Ctrl+C had armed the exit and the send's own recovery (session_chat_queue_runtime/send_heal.rs) would have typed the second one. While the hint is up the clear sends no Ctrl+C and waits for it to go away.
fn exit_is_armed(screen: &str) -> bool {
    screen
        .lines()
        .rev()
        .filter(|line| !line.trim().is_empty())
        .take(4)
        .any(|line| {
            let line = line.to_ascii_lowercase();
            ["ctrl-c again", "ctrl+c again", "ctrl + c again"]
                .iter()
                .any(|hint| line.contains(hint))
                && (line.contains("exit") || line.contains("quit"))
        })
}

fn kill_lines_backward(rows: usize) -> String {
    AGENT_TUI_CLEAR_INPUT_LINE
        .repeat((rows + AGENT_TUI_CLEAR_LINE_SLACK).min(AGENT_TUI_CLEAR_MAX_LINES))
}

pub(super) fn supports_verified_composer_clear(agent: &str) -> bool {
    composer_clear_method(agent).is_some()
}

/// CDXC:SessionChat 2026-09-08 DECISION:
/// User: sending from Chat must clear existing Claude or Codex terminal text and send the chat input. Rewind's restored prompt belongs in Chat, and switching to Terminal must not append another copy.
/// The clear is checked against the live draft, not sized solely from the replacement text; a longer old draft can require several separately delivered bursts.
/// CDXC:SessionChat 2026-09-11 DECISION:
/// User approved agent-specific clearing followed by verification before sending. Spaces and newlines count as empty.
/// CDXC:SessionChat 2026-09-11 WHY:
/// The ghostex-web audit cleared populated drafts with one Ctrl+C in seven agents, but Codex and Hermes exited on empty input and Antigravity retained its draft. OpenClaude was unavailable.
/// Send Ctrl+C only once after positive draft evidence; Antigravity and OpenClaude retain line deletion. See docs/2026-09-09/ctrl-c-agent-tests/RESULTS.md.
/// Grok must not receive Ctrl+U because it quits to install a pending update.
pub async fn clear_session_chat_composer(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    agent: &str,
    cancelled: &(impl Fn() -> bool + Sync + ?Sized),
) -> Result<(), SessionChatSendError> {
    let deadline = Instant::now() + Duration::from_millis(SESSION_CHAT_COMPOSER_WAIT_TIMEOUT_MS);
    let agent = crate::agents::identity::normalize_agent_id(Some(agent))
        .unwrap_or_else(|| agent.to_string());
    let method = composer_clear_method(&agent);
    let mut interrupt_sent = false;
    let mut shell_escape_sent = false;
    let mut backward_burst_sent = false;
    let mut attachment_seed_sent = false;
    let mut composer_seen = false;
    let mut not_ready_reason = None;
    loop {
        if cancelled() {
            return Err(SessionChatSendError::not_attempted(
                SESSION_CHAT_SEND_CANCELLED.to_string(),
            ));
        }
        if let Some(screen) = capture_session_terminal_text_vt(zmx_name).await {
            let notice = crate::session_chat_notice::classify_session_chat_terminal_notice(
                Some(&agent),
                &screen,
            );
            let ready =
                detect_session_chat_composer_readiness(Some(&agent), &screen, notice.as_ref());
            if ready.state != SessionChatComposerState::Ready {
                // CDXC:SessionChat 2026-10-06 WHY:
                // A send made while the agent boots passes the pre-send gate (an unreadable screen fails open) and waits here. When Claude's folder-trust question then appeared, the wait ran out and reported "The terminal draft could not be cleared", though no input box ever existed. A question the chat shows as a card is named at once, the way the pre-send gate names it; any other missing input box reports what the screen showed.
                if let Some(notice) = notice.as_ref().filter(|notice| notice.is_answerable()) {
                    return Err(SessionChatSendError::new(
                        SessionChatSendFailure::ComposerNotReady,
                        format!("{}. Answer it in chat before sending.", notice.title),
                    ));
                }
                not_ready_reason = ready.reason.clone().or(not_ready_reason);
            } else {
                composer_seen = true;
                if let Some(input) = session_chat_composer_input(&agent, &screen) {
                    let backward_burst_due =
                        matches!(method, Some(ComposerClearMethod::KillLinesBackward))
                            && !backward_burst_sent;
                    if input.is_empty() && !input.shell_mode && !backward_burst_due {
                        return Ok(());
                    }
                    if cancelled() {
                        return Err(SessionChatSendError::not_attempted(
                            SESSION_CHAT_SEND_CANCELLED.to_string(),
                        ));
                    }
                    // CDXC:SessionChat 2026-09-15 WHY:
                    // Claude 2.1.268 keeps shell mode after Ctrl+C clears its command. Escape on the empty shell input restores the normal prompt; another Ctrl+C can exit Claude.
                    let clear = if input.is_empty() && input.shell_mode {
                        if shell_escape_sent {
                            None
                        } else {
                            shell_escape_sent = true;
                            Some(SESSION_CHAT_INTERRUPT.to_string())
                        }
                    } else {
                        match method {
                            Some(ComposerClearMethod::InterruptOnce)
                                if !interrupt_sent && !exit_is_armed(&screen) =>
                            {
                                interrupt_sent = true;
                                Some("\u{3}".to_string())
                            }
                            Some(ComposerClearMethod::KillLines) => {
                                Some(build_agent_tui_clear_input(
                                    input.rows + AGENT_TUI_CLEAR_LINE_SLACK,
                                ))
                            }
                            Some(ComposerClearMethod::KillLinesBackward)
                                if input.attachments() > 0 && input.text_unreadable() =>
                            {
                                // The seed could never be seen typed, so the Ctrl+C that drops the
                                // images is never safe here (it quits Empryo on an empty box).
                                if attachment_seed_sent {
                                    let _ = write_session_chat_payload(
                                        project_id,
                                        session_id,
                                        zmx_name,
                                        source,
                                        &kill_lines_backward(input.rows),
                                    )
                                    .await;
                                }
                                return Err(SessionChatSendError::new(
                                    SessionChatSendFailure::ComposerNotCleared,
                                    "Empryo's input box holds an attached image that Ghostex cannot clear in this terminal. Remove it in the terminal, then send again. Your chat draft has been kept.".to_string(),
                                ));
                            }
                            Some(ComposerClearMethod::KillLinesBackward) => {
                                backward_burst_sent = true;
                                Some(
                                    if input.attachments() > 0
                                        && input.text_is_empty()
                                        && !attachment_seed_sent
                                    {
                                        attachment_seed_sent = true;
                                        EMPRYO_ATTACHMENT_SEED.to_string()
                                    } else if input.attachments() > 0
                                        && !input.text_is_empty()
                                        && !interrupt_sent
                                    {
                                        // Text is on screen in this capture, so Ctrl+C clears the box. One
                                        // press only: a second could reach the box this one emptied.
                                        interrupt_sent = true;
                                        "\u{3}".to_string()
                                    } else {
                                        kill_lines_backward(input.rows)
                                    },
                                )
                            }
                            _ => None,
                        }
                    };
                    if let Some(clear) = clear {
                        write_session_chat_payload(
                            project_id, session_id, zmx_name, source, &clear,
                        )
                        .await
                        .map_err(|message| {
                            SessionChatSendError::new(SessionChatSendFailure::Write, message)
                        })?;
                    }
                }
            }
        }
        if Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(SESSION_CHAT_CLEAR_INPUT_SETTLE_MS)).await;
    }
    if !composer_seen {
        return Err(SessionChatSendError::new(
            SessionChatSendFailure::ComposerNotReady,
            not_ready_reason.unwrap_or_else(|| {
                "The agent's input box did not appear, so nothing was typed. Your chat draft has been kept."
                    .to_string()
            }),
        ));
    }
    Err(SessionChatSendError::new(
        SessionChatSendFailure::ComposerNotCleared,
        "The terminal draft could not be cleared and verified. Your chat draft has been kept."
            .to_string(),
    ))
}

async fn place_session_chat_draft(
    target: &super::SessionChatSendTarget,
    content: &str,
    state_dir: &std::path::Path,
) -> Result<serde_json::Value, crate::domain::DomainStateError> {
    if content.len() > crate::zmx::GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES {
        return Err(crate::domain::DomainStateError {
            code: "invalidParams",
            message: "The draft exceeds the terminal input size limit.".to_string(),
        });
    }
    let agent = crate::session_chat_composer::session_chat_composer_agent_id(&target.session)
        .or_else(|| crate::session_chat_follower::session_chat_agent_for_session(&target.session));
    let mut steps = if agent.as_deref() == Some("empryo") {
        // Empryo has no prompt-editor handshake (Ctrl+G opens its Git menu; CDXC:Drafts
        // 2026-10-06 in handoff_http.rs): keep any other text in its input box, clear only the
        // same text, and type the draft the way a chat send does.
        vec![
            super::SessionChatSendStep::WaitForComposer {
                agent: agent.clone(),
                settle_ms: super::SESSION_CHAT_COMPOSER_WAIT_SETTLE_MS,
                timeout_ms: super::SESSION_CHAT_COMPOSER_WAIT_TIMEOUT_MS,
            },
            super::SessionChatSendStep::SelectEmpryoTab {
                wait_ms: crate::session_chat_empryo_tabs::EMPRYO_SEND_TAB_WAIT_MS,
            },
            super::SessionChatSendStep::GuardEmpryoDraft {
                replacement: content.to_string(),
            },
            super::SessionChatSendStep::ClearComposer {
                agent: "empryo".to_string(),
            },
            super::SessionChatSendStep::Write(super::build_empryo_input_bytes(content)),
        ]
    } else if agent.as_deref() == Some("opencode")
        && crate::pi_chat_channel::live(&target.project_id, &target.session_id)
            .is_some_and(|channel| channel.supports("promptEditor"))
    {
        // OpenCode's Ghostex TUI plugin opens the prompt editor itself, and its open channel says
        // the TUI is up, so no input box is read off the screen; OpenCode 2 has no route that
        // sets the prompt, so the text is still pasted.
        vec![
            super::SessionChatSendStep::PreserveTerminalDraft {
                replacement: Some(content.to_string()),
                state_dir: state_dir.to_path_buf(),
                prompt_editor_input: super::session_chat_prompt_editor_input(agent.as_deref())
                    .to_string(),
            },
            super::SessionChatSendStep::Write(super::build_session_chat_paste_bytes(content)),
        ]
    } else {
        // Capture already saves and clears the old input through the agent's editor.
        // A second clear burst here would erase keystrokes typed after that handshake.
        vec![
            super::SessionChatSendStep::WaitForComposer {
                agent: agent.clone(),
                settle_ms: super::SESSION_CHAT_COMPOSER_WAIT_SETTLE_MS,
                timeout_ms: super::SESSION_CHAT_COMPOSER_WAIT_TIMEOUT_MS,
            },
            super::SessionChatSendStep::PreserveTerminalDraft {
                replacement: Some(content.to_string()),
                state_dir: state_dir.to_path_buf(),
                prompt_editor_input: super::session_chat_prompt_editor_input(agent.as_deref())
                    .to_string(),
            },
            super::SessionChatSendStep::WaitForComposer {
                agent: agent.clone(),
                settle_ms: super::SESSION_CHAT_COMPOSER_WAIT_SETTLE_MS,
                timeout_ms: super::SESSION_CHAT_COMPOSER_WAIT_TIMEOUT_MS,
            },
            super::SessionChatSendStep::Write(super::build_session_chat_paste_bytes(content)),
        ]
    };
    if let Some(verify) = super::session_chat_verify_step(content) {
        steps.push(verify);
    }
    super::execute_session_chat_send(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        "session-chat-draft-to-terminal",
        steps,
    )
    .await
    .map_err(|error| crate::domain::DomainStateError {
        code: "sessionInputFailed",
        message: error.message,
    })?;
    Ok(serde_json::json!({ "replaced": true }))
}

pub(crate) async fn handle_replace_session_chat_draft_http(
    state: &crate::server::AppState,
    endpoint_path: String,
    request_id: String,
    body: &serde_json::Value,
) -> crate::server::RoutedResponse {
    use crate::{
        domain::{read_domain_rpc_params, DomainStateError},
        server::{domain_error_response, routed_json},
    };
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let target =
        match super::resolve_session_chat_send_target(state, &params, "replaceSessionChatDraft") {
            Ok(target) => target,
            Err(error) => return domain_error_response(endpoint_path, request_id, error),
        };
    let Some(content) = params
        .get("content")
        .and_then(serde_json::Value::as_str)
        .filter(|text| !text.trim().is_empty())
    else {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "invalidParams",
                message: "A terminal draft replacement requires content.".to_string(),
            },
        );
    };
    let db = match crate::storage::open_gxserver_database(&state.paths) {
        Ok(db) => db,
        Err(error) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "internalError",
                    message: error.to_string(),
                },
            )
        }
    };
    let id = params
        .get("handoffId")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let staged = (|| {
        if let Some(previous) = crate::session_chat_draft_handoffs::result(
            &db,
            &target.project_id,
            &target.session_id,
            &id,
        )? {
            if previous["content"].as_str() != Some(content) {
                return Err(DomainStateError::bad_request(
                    "A draft transfer ID cannot be reused for different text.",
                ));
            }
            if previous["state"] == "placed" || previous["state"] == "received" {
                return Ok(Some(previous));
            }
            return Err(DomainStateError::bad_request(
                "This transfer is already pending. Its text remains in Recovered.",
            ));
        }
        if let Some(version) = crate::session_chat_draft_versions::parse(&params)? {
            crate::session_chat_draft_versions::require_saved(
                &db,
                &target.project_id,
                &target.session_id,
                content,
                &version,
            )?;
        }
        let version = crate::session_chat_draft_versions::parse(&params)?.unwrap_or(
            crate::session_chat_draft_versions::DraftVersion {
                draft_id: uuid::Uuid::new_v4().to_string(),
                revision: 1,
            },
        );
        crate::session_chat_draft_handoffs::stage(
            &db,
            &target.project_id,
            &target.session_id,
            &id,
            content,
            &version,
            "terminal",
        )?;
        Ok(None)
    })();
    match staged {
        Ok(Some(result)) => {
            return routed_json(
                Some(endpoint_path),
                axum::http::StatusCode::OK,
                crate::protocol::rpc_success(request_id, result),
            )
        }
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
        Ok(None) => {}
    }
    // Pi and OMP take the draft into their input box through the chat channel (pi_chat_actions.rs).
    let placed = match crate::pi_chat_actions::is_pi_family(&target.session) {
        true => crate::pi_chat_actions::place_editor_draft(state, &target, content).await,
        false => None,
    };
    let placed = match placed {
        Some(placed) => placed,
        None => place_session_chat_draft(&target, content, &state.paths.app_state_dir).await,
    };
    match placed.and_then(|result| {
            crate::session_chat_draft_handoffs::placed(&db, &id)?;
            Ok(result)
        }) {
        Ok(result) => routed_json(
            Some(endpoint_path),
            axum::http::StatusCode::OK,
            crate::protocol::rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}
