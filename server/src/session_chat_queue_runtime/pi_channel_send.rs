use super::*;

/// A chat send to a Pi or OMP session whose Ghostex extension has its chat channel open
/// (pi_chat_channel.rs): Pi's own `sendUserMessage` takes the message inside the Pi process, so
/// nothing is typed into the terminal or read back from it, and the TUI shows it like any message.
/// `None` when the screen path has to send it instead: no channel (an older extension, Pi not
/// started yet or asleep), or a message the extension cannot send (a built-in TUI command such as
/// `/model` or `/compact`, which only the TUI's editor runs). Each fallback is logged.
pub(super) async fn send_pi_channel_message(
    state: &AppState,
    target: &crate::session_chat_send::SessionChatSendTarget,
    text: &str,
    image_paths: &[String],
    interrupt: bool,
    source: SessionChatMessageSource,
    drafted_text: &str,
    draft_before_send: Option<&crate::session_chat_queue::SessionChatDraft>,
    draft_version: Option<&crate::session_chat_draft_versions::DraftVersion>,
) -> Option<std::result::Result<usize, DomainStateError>> {
    let (project_id, session_id) = (target.project_id.clone(), target.session_id.clone());
    let Some(channel) = crate::pi_chat_channel::live(&project_id, &session_id) else {
        crate::pi_chat_channel::log_screen_fallback(
            &project_id,
            &session_id,
            "send",
            "no chat channel (older extension or agent not running)",
        );
        return None;
    };
    if !channel.supports("send") {
        crate::pi_chat_channel::log_screen_fallback(
            &project_id,
            &session_id,
            "send",
            "extension cannot send",
        );
        return None;
    }
    let params = json!({
        "text": text,
        "images": image_paths,
        "interrupt": interrupt,
        "deliverAs": pi_busy_delivery(state),
    });
    let sent = tokio::task::spawn_blocking(move || {
        crate::pi_chat_channel::request(
            &project_id,
            &session_id,
            "send",
            params,
            Duration::from_secs(20),
        )
    })
    .await;
    let label = if channel.agent == "omp" { "OMP" } else { "Pi" };
    let ran_command = match sent {
        Ok(Ok(result)) => result.get("delivered").and_then(Value::as_str) == Some("command"),
        Ok(Err(
            reason @ (crate::pi_chat_channel::ChannelError::Unavailable(_)
            | crate::pi_chat_channel::ChannelError::Unsupported(_)),
        )) => {
            crate::pi_chat_channel::log_screen_fallback(
                &target.project_id,
                &target.session_id,
                "send",
                &reason.to_string(),
            );
            return None;
        }
        Ok(Err(crate::pi_chat_channel::ChannelError::Failed(message))) => {
            return Some(Err(DomainStateError {
                code: "invalidState",
                message: format!("{label} refused the message: {message}"),
            }));
        }
        // It may have gone through, so typing it as well could send it twice.
        Ok(Err(crate::pi_chat_channel::ChannelError::NoAnswer)) => {
            return Some(Err(DomainStateError {
                code: "deliveryUnconfirmed",
                message: format!(
                    "{label} took the message but did not confirm it. Check the chat before sending it again."
                ),
            }));
        }
        Err(error) => {
            return Some(Err(DomainStateError {
                code: "internalError",
                message: format!("{label} chat channel task failed: {error}"),
            }));
        }
    };
    let (project_id, session_id) = (target.project_id.as_str(), target.session_id.as_str());
    crate::telemetry::prompt_sent(
        &target.session,
        match source {
            SessionChatMessageSource::Composer => "chat",
            _ => "queue",
        },
    );
    // A command the extension ran (`/model`, `/compact`, `/name`) is not the draft's first prompt;
    // what it changed is read back, as after the same command typed on the screen path.
    if ran_command {
        suppress_draft_activity_after_app_command(state, project_id, session_id);
        crate::session_chat_options::schedule_session_chat_option_redetect(
            state,
            project_id,
            session_id,
            crate::session_chat_composer::session_chat_composer_agent_id(&target.session)
                .as_deref(),
        );
    } else {
        promote_draft_session_after_send(state, project_id, session_id);
        unpark_session_after_send(state, project_id, session_id);
    }
    crate::session_chat_returned_prompt::record_session_chat_send_submitted(project_id, session_id);
    if let Some(version) = draft_version {
        if let Ok(db) = open_gxserver_database(&state.paths) {
            let _ =
                crate::session_chat_draft_versions::consume(&db, project_id, session_id, version);
        }
        broadcast_session_chat_queue_state(state, project_id, session_id);
    } else if source == SessionChatMessageSource::Composer {
        retire_sent_session_chat_draft(
            state,
            project_id,
            session_id,
            drafted_text,
            draft_before_send,
        );
    }
    Some(Ok(text.len()))
}

/// Settings > Agents > Messages sent while it works.
const PI_MESSAGES_WHILE_WORKING_SETTING: &str = "piMessagesWhileWorking";

/// CDXC:AgentProviders 2026-11-20 DECISION:
/// User (triage Q4302, "steer by default"): a message sent to an OMP or Pi that is working steers it, the way Enter does in their own terminals, instead of waiting until it yields. "Messages sent while it works" (Settings > Agents, in the row of OMP, Pi and every custom agent that works like them, one setting for all of them) keeps After yield for anyone who wants it. Ghostex's extension already passes `deliverAs: "steer"` to Pi's `sendUserMessage` (server/src/agent_hooks/chat_channel_source.rs); anything else is a follow-up, so an extension that predates this still queues the message as before. An idle agent starts the message straight away either way.
/// SEE-ALSO: apps/desktop/src/app/window/settings_modal/tabs/agents/pi_messages.rs (the row).
fn pi_busy_delivery(state: &AppState) -> &'static str {
    let saved =
        crate::session_lifecycle::read_sidebar_settings(&state.paths).and_then(|settings| {
            settings
                .get(PI_MESSAGES_WHILE_WORKING_SETTING)
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    match saved.as_deref() {
        Some("followUp") => "followUp",
        _ => "steer",
    }
}
