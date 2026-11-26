use super::*;

/// The message typed once more after the input box showed it twice.
pub(crate) const SESSION_CHAT_PASTE_DOUBLED: &str =
    "The message showed up twice in the agent's input box, so it was not sent.";

/// The agents whose input box the paste check reads and whose verified clear sends nothing to an
/// empty box (Empryo's clear always sends its first burst, so a late paste is cleared in order).
pub(crate) fn keeps_single_paste(agent: &str) -> bool {
    matches!(agent, "claude" | "openclaude" | "codex" | "grok")
}

/// Whether the input box holds more copies of the message than the message itself spells: some
/// fragment of it is on screen more often than it occurs in the text, or the agent drew more
/// collapsed-paste placeholders than the text contains.
pub(crate) fn session_chat_paste_repeated(input: &str, text: &str) -> bool {
    let placeholders = |value: &str| {
        normalize_session_chat_screen_text(value)
            .to_lowercase()
            .matches(SESSION_CHAT_PASTED_PLACEHOLDER_NEEDLE)
            .count()
    };
    // Needles are counted in their own form (`session_chat_needle_text`, #711).
    let (shown, message) = (
        session_chat_needle_text(input),
        session_chat_needle_text(text),
    );
    placeholders(input) > placeholders(text).max(1)
        || session_chat_paste_needles(text).iter().any(|needle| {
            shown.matches(needle.as_str()).count() > message.matches(needle.as_str()).count()
        })
}

async fn paste_repeated_on_screen(zmx_name: &str, agent: &str, text: &str) -> bool {
    capture_session_terminal_text_vt(zmx_name)
        .await
        .and_then(|screen| {
            crate::session_chat_composer::session_chat_composer_input(agent, &screen)
        })
        .is_some_and(|input| session_chat_paste_repeated(&input.text, text))
}

/// CDXC:SessionChat 2026-10-08 WHY:
/// The clear-and-retype attempt runs because two paste checks never saw the first paste, but under load that paste is late, not lost: the 07:41 send had both pastes land, and Enter submitted the message twice as one prompt. The verified clear sends no keys while the box reads empty, so the late paste arrives behind it. The pty delivers input in order, so once the retyped copy is on screen the first one has arrived too: this is where a box holding the message twice is cleared and typed once, before Enter.
pub(crate) async fn keep_single_session_chat_paste(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    agent: &str,
    text: &str,
    generation: &AtomicU64,
    job_generation: u64,
) -> Result<(), SessionChatSendError> {
    if !paste_repeated_on_screen(zmx_name, agent, text).await {
        return Ok(());
    }
    crate::session_chat_send_diagnostics::record_send_recovery_from_worker(
        "sessionChatSendPasteDoubled",
        project_id,
        session_id,
        "The first paste arrived late, so the input box held the message twice; cleared it and typed it once.",
        &[],
    );
    let cancelled = || job_generation != generation.load(Ordering::SeqCst);
    clear_session_chat_composer(project_id, session_id, zmx_name, source, agent, &cancelled)
        .await?;
    write_session_chat_payload(
        project_id,
        session_id,
        zmx_name,
        source,
        &build_session_chat_paste_bytes(text),
    )
    .await
    .map_err(|error| SessionChatSendError::new(SessionChatSendFailure::Write, error))?;
    match verify_session_chat_paste_landed(
        zmx_name,
        Some(agent),
        text,
        SESSION_CHAT_VERIFY_SETTLE_MS,
        session_chat_verify_timeout_ms(text.len()),
        generation,
        job_generation,
    )
    .await
    {
        SessionChatPasteVerification::Landed => {}
        SessionChatPasteVerification::Cancelled => {
            return Err(SessionChatSendError::not_attempted(
                SESSION_CHAT_SEND_CANCELLED.to_string(),
            ));
        }
        SessionChatPasteVerification::Absent | SessionChatPasteVerification::Unreadable => {
            return Err(SessionChatSendError::new(
                SessionChatSendFailure::Write,
                SESSION_CHAT_PASTE_NOT_ACCEPTED.to_string(),
            ));
        }
    }
    if paste_repeated_on_screen(zmx_name, agent, text).await {
        let _ = clear_session_chat_composer(
            project_id, session_id, zmx_name, source, agent, &cancelled,
        )
        .await;
        return Err(SessionChatSendError::new(
            SessionChatSendFailure::Write,
            SESSION_CHAT_PASTE_DOUBLED.to_string(),
        ));
    }
    Ok(())
}
