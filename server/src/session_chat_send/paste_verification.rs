use super::*;

/// One `zmx send` stdin burst, logged through the shared temporary input log.
/// Shared with the Claude rewind driver (session_chat_rewind.rs), whose whole
/// dialog runs inside one job of this worker and therefore writes through the
/// same attributed burst every other step does.
pub(crate) async fn write_session_chat_payload(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    payload: &str,
) -> Result<(), String> {
    #[cfg(test)]
    if let Some(written) = test_terminal::write(zmx_name, payload) {
        return written;
    }
    crate::zmx::log_temporary_zmx_input_write(
        project_id,
        session_id,
        zmx_name,
        "sessionChatQueueWrite",
        source,
        payload,
    );
    let zmx_name = zmx_name.to_string();
    let written = payload;
    let payload = payload.to_string();
    let write = tokio::task::spawn_blocking(move || {
        crate::zmx::session_chat_zmx_write(&zmx_name, &payload)
    })
    .await;
    match write {
        Ok(Ok(0)) => {
            note_session_chat_submit(project_id, session_id, written);
            Ok(())
        }
        Ok(Ok(code)) => Err(format!(
            "The session terminal refused chat input (exit {code})."
        )),
        Ok(Err(error)) => Err(error),
        Err(error) => Err(format!("The session terminal input task failed: {error}")),
    }
}

/// Current terminal text for the session, or `None` when it could not be read
/// whole — a capture whose tail was dropped cannot prove what is on screen.
/// Shared with the send-delivery watchdog (session_chat_watchdog/), which
/// takes exactly one of these per timeout event.
/// Whether an agent CLI (claude, codex, …) is still running inside the session's daemon, read from the process snapshot the identity poller uses. An unreadable snapshot counts as running: a restart must never type into a CLI it could not prove gone.
pub(crate) async fn session_agent_process_running(zmx_name: &str, home_dir: &Path) -> bool {
    let name = zmx_name.to_string();
    let home = home_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        crate::zmx::read_zmx_session_process_identities(std::slice::from_ref(&name), &home)
            .map(|identities| identities.contains_key(&name))
            .unwrap_or(true)
    })
    .await
    .unwrap_or(true)
}

pub(crate) async fn capture_session_terminal_text(zmx_name: &str) -> Option<String> {
    #[cfg(test)]
    if let Some(screen) = test_terminal::capture(zmx_name) {
        return Some(screen);
    }
    let zmx_name = zmx_name.to_string();
    let capture =
        tokio::task::spawn_blocking(move || crate::zmx::read_zmx_session_screen_capture(&zmx_name))
            .await
            .ok()?
            .ok()?;
    (!capture.truncated).then_some(capture.text)
}

pub(crate) async fn capture_session_terminal_text_vt(zmx_name: &str) -> Option<String> {
    let name = zmx_name.to_string();
    let capture =
        tokio::task::spawn_blocking(move || crate::zmx::read_zmx_session_screen_capture_vt(&name))
            .await
            .ok()?
            .ok()?;
    (!capture.truncated).then_some(capture.text)
}

/*
CDXC:Clipboard 2026-08-24:
The screen watch that stands between a paste body and its Enter. It settles
once (so a paste the TUI already took costs nothing extra), then polls captures
until one of three things is true: the body — or the TUI's collapsed paste
placeholder (`[Pasted text …]`, `[Pasted Content …]`, `[paste …]`, … — every
supported agent collapses large pastes, each with its own spelling) — is on
screen, the deadline passed, or the send was superseded. "Pasting text…" is the TUI saying ingestion is still running,
which buys one deadline extension rather than an abort.

Claude, Codex and Grok verify only the live composer after its old draft was cleared.
They require evidence before Enter, including when capture fails. Other agents
retain their existing whole-screen watch and unreadable-capture behavior.
*/
pub(crate) async fn verify_session_chat_paste_landed(
    zmx_name: &str,
    agent: Option<&str>,
    text: &str,
    settle_ms: u64,
    timeout_ms: u64,
    generation: &AtomicU64,
    job_generation: u64,
) -> SessionChatPasteVerification {
    let needles = session_chat_paste_needles(text);
    let mut deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);
    let mut readable = false;
    let mut extended = false;
    tokio::time::sleep(Duration::from_millis(settle_ms)).await;
    loop {
        if job_generation != generation.load(Ordering::SeqCst) {
            return SessionChatPasteVerification::Cancelled;
        }
        let capture = if let Some(agent) = agent {
            capture_session_terminal_text_vt(zmx_name)
                .await
                .map(|screen| {
                    let pasting = normalize_session_chat_screen_text(
                        &crate::session_chat_options::strip_ansi_sgr(&screen),
                    )
                    .contains(SESSION_CHAT_PASTING_INDICATOR_NEEDLE);
                    // Text a cursorless capture cannot tell from a tip still counts here: it is only
                    // matched against the message just typed, which no tip spells (seen live
                    // 2026-10-07: every Empryo slash command from the chat failed on wmx).
                    let body =
                        crate::session_chat_composer::session_chat_composer_input(agent, &screen)
                            .filter(|input| !input.is_empty() || input.text_unreadable())
                            .map(|input| input.text)
                            .unwrap_or_default();
                    (body, pasting)
                })
        } else {
            capture_session_terminal_text(zmx_name).await.map(|screen| {
                let pasting = normalize_session_chat_screen_text(&screen)
                    .contains(SESSION_CHAT_PASTING_INDICATOR_NEEDLE);
                (screen, pasting)
            })
        };
        if let Some((screen, pasting)) = capture {
            readable = true;
            let normalized = normalize_session_chat_screen_text(&screen);
            if normalized
                .to_lowercase()
                .contains(SESSION_CHAT_PASTED_PLACEHOLDER_NEEDLE)
                || needles.iter().any(|needle| normalized.contains(needle))
                || omp_paste_chip_landed(agent, &screen)
            {
                return SessionChatPasteVerification::Landed;
            }
            if !extended && pasting {
                extended = true;
                deadline += Duration::from_millis(SESSION_CHAT_VERIFY_PASTING_EXTENSION_MS);
            }
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(SESSION_CHAT_VERIFY_POLL_MS)).await;
    }
    if readable {
        SessionChatPasteVerification::Absent
    } else if agent.is_some() {
        // A checked replacement cannot submit without evidence of its new body.
        SessionChatPasteVerification::Absent
    } else {
        SessionChatPasteVerification::Unreadable
    }
}

/// Whether OMP's input box shows the chip it draws for a long paste (`omp_paste_chip_shown`).
/// OMP's sends take the whole-screen watch (`agent` is `None`), so the input box is found on the
/// whole screen; only OMP draws that box, so no other agent's screen can match.
fn omp_paste_chip_landed(agent: Option<&str>, screen: &str) -> bool {
    match agent {
        Some("omp") => crate::session_chat_composer::omp_paste_chip_shown(screen),
        Some(_) => false,
        None => crate::session_chat_composer::session_chat_composer_input("omp", screen)
            .is_some_and(|input| crate::session_chat_composer::omp_paste_chip_shown(&input.text)),
    }
}

static SESSION_CHAT_SEND_LOGGER: OnceLock<GxserverLogger> = OnceLock::new();

/// A send that could not be verified is a message the user may never see
/// again, so this is persisted unconditionally (warn/error), unlike the
/// scenario-gated per-write input log. Metadata only — never the message text.
#[allow(clippy::too_many_arguments)]
pub(super) fn log_session_chat_paste_verification(
    level: LogLevel,
    event: &str,
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    text_bytes: usize,
    timeout_ms: u64,
    error: &str,
) {
    let logger = SESSION_CHAT_SEND_LOGGER
        .get_or_init(|| GxserverLogger::new(crate::paths::get_gxserver_paths(None)));
    let _ = logger.log(GxserverLogInput {
        level,
        event: event.to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(error.to_string()),
        details: Some(json!({
            "projectId": project_id,
            "providerSessionId": zmx_name,
            "sessionId": session_id,
            "source": source,
            "textBytes": text_bytes,
            "timeoutMs": timeout_ms,
        })),
    });
}
