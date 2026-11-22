//! The queued prompt's play button: an Escape that must not outlive the message it was for.

use super::*;

/// The most transcript a stale-click check reads past the click.
const TRANSCRIPT_SCAN_LIMIT_BYTES: u64 = 512 * 1024;
/// The faint text Claude Code puts in its empty input box while messages wait in its queue.
const CLAUDE_QUEUE_HINT: &str = "press up to edit queued messages";
/// The footer hint Antigravity CLI draws below its input box while messages wait in its queue.
const ANTIGRAVITY_QUEUE_FOOTER_HINT: &str = "press up to edit queued";
/// The suffix Antigravity CLI draws on a queued preview line above its input box.
const ANTIGRAVITY_QUEUE_PREVIEW_HINT: &str = "(alt+m to ";
/// The footer hint Antigravity CLI draws while a turn is running or still cancelling.
const ANTIGRAVITY_CANCEL_FOOTER_HINT: &str = "esc to cancel";
/// How many contiguous non-empty rows right above Antigravity's top rule to inspect for queued rows.
const ANTIGRAVITY_QUEUE_PREVIEW_SCAN_LINES: usize = 8;
/// How long `submit_antigravity_popped_queue` waits for Escape to cancel the turn and pop the
/// queued message into Antigravity's input box.
const ANTIGRAVITY_POP_WAIT_MS: u64 = 3_000;
const ANTIGRAVITY_POP_POLL_MS: u64 = 100;
/// How long `submit_antigravity_popped_queue` watches the input box after Return to verify it left.
const ANTIGRAVITY_SUBMIT_SETTLE_MS: u64 = 1_000;
const ANTIGRAVITY_SUBMIT_POLL_MS: u64 = 150;

/// Where the session's transcript ended when the play button was clicked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueuedPromptTranscriptMark {
    agent: crate::session_chat::SessionChatTranscriptAgent,
    path: PathBuf,
    offset: u64,
}

impl QueuedPromptTranscriptMark {
    /// BLOCKING (transcript path resolution). `None` when the session's agent keeps no transcript
    /// Ghostex can read or has not written one yet.
    pub(super) fn sample(session: &Value) -> Option<Self> {
        let agent = crate::session_chat::resolve_session_chat_transcript_agent(
            session_chat_agent_for_session(session).as_deref(),
        )?;
        let path = crate::session_chat::resolve_session_chat_transcript_path(
            agent,
            read_runtime_text(session, "agentSessionId").as_deref(),
            read_runtime_text(session, "agentSessionPath").as_deref(),
        )?;
        let offset = fs::metadata(&path).ok()?.len();
        Some(Self {
            agent,
            path,
            offset,
        })
    }

    /// BLOCKING (file read). Whether the agent recorded a prompt it took after the click.
    fn prompt_recorded_since(&self) -> bool {
        if self.agent == crate::session_chat::SessionChatTranscriptAgent::Antigravity {
            crate::session_chat_antigravity_mirror::sync_antigravity_transcript_mirror_for_path(
                &self.path,
            );
        }
        let Ok(mut file) = fs::File::open(&self.path) else {
            return false;
        };
        let mut appended = Vec::new();
        if std::io::Seek::seek(&mut file, std::io::SeekFrom::Start(self.offset)).is_err()
            || std::io::Read::read_to_end(
                &mut std::io::Read::take(file, TRANSCRIPT_SCAN_LIMIT_BYTES),
                &mut appended,
            )
            .is_err()
        {
            return false;
        }
        String::from_utf8_lossy(&appended).lines().any(|line| {
            crate::session_chat_watchdog::transcript_line_records_prompt(self.agent, line)
        })
    }
}

/// Pre-Escape state captured for an Antigravity queued-prompt play-button click.
pub(super) struct AntigravityQueuedEscapeGuard {
    /// Positive confirmation that Antigravity's screen showed queued messages before Escape.
    pub(super) confirmed_queue: bool,
    /// Any non-empty draft already sitting in `>` before Escape, so auto-Return never submits it.
    pub(super) baseline_draft: Option<String>,
    /// Transcript mark sampled at click time, to abort auto-Return if a prompt already landed.
    pub(super) transcript: Option<QueuedPromptTranscriptMark>,
}

/// Outcome of `GuardQueuedPromptEscape`.
pub(super) enum QueuedPromptEscapeVerdict {
    /// Drop the click: do not write Escape or auto-Return.
    DropStale,
    /// Proceed with Escape, carrying any pre-Escape Antigravity baseline for auto-Return.
    Proceed(Option<AntigravityQueuedEscapeGuard>),
}

/// Single-parse snapshot of Antigravity CLI's screen for queued-prompt Escape handling.
struct AntigravityScreenState {
    shows_queued: bool,
    esc_to_cancel: bool,
    /// Non-empty, non-hint input text when the composer is Ready with no blocking/answerable
    /// notice, permission picker, or numbered choice row.
    ready_input_text: Option<String>,
    /// Non-empty, non-hint input text (used for pre-Escape baseline draft protection).
    input_text: Option<String>,
}

fn normalize_antigravity_line(line: &str) -> String {
    line.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Whether the text after `>` looks like a numbered picker option (`1. Yes`, `2) No`), never a
/// composer prompt to auto-submit.
fn looks_like_numbered_picker_row(text: &str) -> bool {
    let trimmed = text.trim_start();
    let mut chars = trimmed.chars().peekable();
    let mut digits = 0usize;
    while chars.peek().is_some_and(char::is_ascii_digit) {
        chars.next();
        digits += 1;
    }
    digits > 0
        && digits <= 2
        && matches!(chars.next(), Some('.' | ')'))
        && chars.next().is_some_and(char::is_whitespace)
}

/// Reads the plain (space-preserving) text inside Antigravity's `>` input region.
fn antigravity_plain_region_text(plain: &[String], region: &std::ops::Range<usize>) -> String {
    let first = plain
        .get(region.start)
        .map(|line| {
            let trimmed = line.trim_start();
            trimmed
                .strip_prefix('>')
                .unwrap_or(trimmed)
                .trim_end()
                .to_string()
        })
        .unwrap_or_default();
    std::iter::once(first)
        .chain(
            plain
                .get(region.start + 1..region.end)
                .unwrap_or(&[])
                .iter()
                .map(|line| line.trim_end().to_string()),
        )
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Checks whether the contiguous non-empty block immediately above Antigravity's top rule (allowing
/// at most one blank padding line directly above `head`) shows a queued preview (`▸ ... (alt+m to ...)`),
/// including when a queued row wraps across contiguous lines. Stops at the first blank line so
/// assistant prose higher up is never scanned.
fn antigravity_preview_block_shows_queued(above_head: &[String]) -> bool {
    let mut end = above_head.len();
    if end > 0 && above_head[end - 1].trim().is_empty() {
        end -= 1;
    }
    let mut start = end;
    while start > 0
        && end - start < ANTIGRAVITY_QUEUE_PREVIEW_SCAN_LINES
        && !above_head[start - 1].trim().is_empty()
    {
        start -= 1;
    }
    let block = &above_head[start..end];
    for (index, line) in block.iter().enumerate() {
        if !line.trim_start().starts_with('▸') {
            continue;
        }
        let combined = block[index..]
            .iter()
            .map(|row| row.trim())
            .collect::<Vec<_>>()
            .join(" ");
        if normalize_antigravity_line(&combined).contains(ANTIGRAVITY_QUEUE_PREVIEW_HINT) {
            return true;
        }
    }
    false
}

/// Parses an Antigravity VT capture once into queue state, `esc to cancel` state, and ready input.
/// Returns `None` when Antigravity's input box is not on screen (overlay, dialog, or unreadable).
fn inspect_antigravity_screen(screen: &str) -> Option<AntigravityScreenState> {
    let (plain, head, region, footer_start) =
        crate::session_chat_composer::antigravity_composer_lines(screen)?;
    let footer = plain.get(footer_start..).unwrap_or(&[]);
    let mut footer_has_queue = false;
    let mut esc_to_cancel = false;
    for line in footer {
        let normalized = normalize_antigravity_line(line);
        if normalized.contains(ANTIGRAVITY_QUEUE_FOOTER_HINT) {
            footer_has_queue = true;
        }
        if normalized.contains(ANTIGRAVITY_CANCEL_FOOTER_HINT) {
            esc_to_cancel = true;
        }
    }
    let shows_queued = footer_has_queue || antigravity_preview_block_shows_queued(&plain[..head]);
    // Use the VT-aware composer input check to reject coloured mode hints, then read the
    // space-preserving text from `plain`.
    let has_typed_input =
        crate::session_chat_composer::session_chat_composer_input("antigravity", screen)
            .is_some_and(|input| !input.is_empty());
    let input_text = if has_typed_input {
        let text = antigravity_plain_region_text(&plain, &region);
        (!text.is_empty()).then_some(text)
    } else {
        None
    };
    let plain_text = plain.join("\n");
    let notice = crate::session_chat_notice::classify_session_chat_terminal_notice(
        Some("antigravity"),
        &plain_text,
    );
    let readiness = crate::session_chat_composer::detect_session_chat_composer_readiness(
        Some("antigravity"),
        &plain_text,
        notice.as_ref(),
    );
    let composer_ready = readiness.state
        == crate::session_chat_composer::SessionChatComposerState::Ready
        && !readiness.blocks_message_for(Some("antigravity"))
        && !readiness.waits_on_question()
        && !notice
            .as_ref()
            .is_some_and(|n| n.blocks_input() || n.is_answerable())
        && crate::session_chat_antigravity_permission::detect_antigravity_permission_picker(
            &plain_text,
        )
        .is_none();
    let ready_input_text = composer_ready
        .then(|| input_text.clone())
        .flatten()
        .filter(|text| !looks_like_numbered_picker_row(text));
    Some(AntigravityScreenState {
        shows_queued,
        esc_to_cancel,
        ready_input_text,
        input_text,
    })
}

fn matches_baseline_draft(candidate: &str, baseline: Option<&str>) -> bool {
    baseline.is_some_and(|draft| {
        normalize_session_chat_screen_text(candidate) == normalize_session_chat_screen_text(draft)
    })
}

fn popped_text_if_ready(
    state: &AntigravityScreenState,
    baseline_draft: Option<&str>,
) -> Option<String> {
    if state.shows_queued || state.esc_to_cancel {
        return None;
    }
    let text = state.ready_input_text.as_deref()?;
    if matches_baseline_draft(text, baseline_draft) {
        return None;
    }
    Some(text.to_string())
}

fn verify_antigravity_return_screen(
    project_id: &str,
    session_id: &str,
    screen: Option<&str>,
    expected_text: &str,
    baseline_draft: Option<&str>,
    since: Instant,
) -> Result<(), String> {
    let Some(screen) = screen else {
        return Err("Antigravity screen could not be read before Return.".to_string());
    };
    if prompt_card_arrived_since(project_id, session_id, since) {
        return Err("A prompt card arrived before Return.".to_string());
    }
    let Some(state) = inspect_antigravity_screen(screen) else {
        return Err("Antigravity input box is no longer on screen.".to_string());
    };
    let Some(ready_text) = popped_text_if_ready(&state, baseline_draft) else {
        return Err("Antigravity is not idle with a newly popped prompt.".to_string());
    };
    if normalize_session_chat_screen_text(&ready_text)
        != normalize_session_chat_screen_text(expected_text)
    {
        return Err("Antigravity input box changed before Return.".to_string());
    }
    Ok(())
}

/// Atomically verifies Antigravity's screen on the same daemon connection and writes Return (`\r`).
/// Answers `Ok(true)` if Return was written, `Ok(false)` if the check refused (screen moved on or a
/// prompt appeared), or `Err` on transport failure.
#[allow(clippy::too_many_arguments)]
async fn write_antigravity_popped_return(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    expected_text: &str,
    baseline_draft: Option<&str>,
    since: Instant,
) -> Result<bool, SessionChatSendError> {
    #[cfg(test)]
    if let Some(screen) = test_terminal::capture(zmx_name) {
        if verify_antigravity_return_screen(
            project_id,
            session_id,
            screen.as_deref(),
            expected_text,
            baseline_draft,
            since,
        )
        .is_err()
        {
            return Ok(false);
        }
        write_session_chat_payload(
            project_id,
            session_id,
            zmx_name,
            source,
            SESSION_CHAT_SUBMIT,
        )
        .await
        .map_err(|message| SessionChatSendError::new(SessionChatSendFailure::Write, message))?;
        return Ok(true);
    }
    let (name, project, session, expected, baseline) = (
        zmx_name.to_string(),
        project_id.to_string(),
        session_id.to_string(),
        expected_text.to_string(),
        baseline_draft.map(str::to_string),
    );
    let sent = tokio::task::spawn_blocking(move || {
        crate::zmx::send_after_screen_check(&name, true, SESSION_CHAT_SUBMIT, |screen| {
            verify_antigravity_return_screen(
                &project,
                &session,
                screen,
                &expected,
                baseline.as_deref(),
                since,
            )
        })
    })
    .await;
    match sent {
        Ok(Ok(crate::zmx::ZmxCheckedSend::Sent)) => {
            crate::zmx::log_temporary_zmx_input_write(
                project_id,
                session_id,
                zmx_name,
                "sessionChatQueueWrite",
                source,
                SESSION_CHAT_SUBMIT,
            );
            note_session_chat_submit(project_id, session_id, SESSION_CHAT_SUBMIT);
            Ok(true)
        }
        Ok(Ok(crate::zmx::ZmxCheckedSend::Refused(_))) => Ok(false),
        Ok(Ok(crate::zmx::ZmxCheckedSend::Unreachable(error)))
        | Ok(Ok(crate::zmx::ZmxCheckedSend::NotTaken(error)))
        | Ok(Err(error)) => Err(SessionChatSendError::new(
            SessionChatSendFailure::Write,
            error,
        )),
        Err(error) => Err(SessionChatSendError::new(
            SessionChatSendFailure::Write,
            error.to_string(),
        )),
    }
}

async fn transcript_recorded_prompt(mark: Option<&QueuedPromptTranscriptMark>) -> bool {
    let Some(mark) = mark.cloned() else {
        return false;
    };
    tokio::task::spawn_blocking(move || mark.prompt_recorded_since())
        .await
        .unwrap_or(false)
}

/// CDXC:SessionChat 2026-11-22 WHY:
/// Clicking the queued prompt's play button ("Queued, click to interrupt and send") writes Escape
/// to cancel the running turn. Unlike Claude Code, Antigravity CLI (verified live on 1.5.14)
/// answers Escape by cancelling the running turn and moving its queued message(s) out of the queue
/// into the `>` input box WITHOUT submitting them, while its `alt+m` shortcut only injects after
/// the current tool call finishes. Leaving the prompt sitting in `>` meant the chat view stayed
/// stuck showing the queued bubble and play button, and repeated clicks sent more Escapes to an
/// already-idle CLI. After Escape lands, watch until Antigravity clears its queue indicator, clears
/// `esc to cancel` (teardown settled), and holds the newly popped text (never a pre-Escape manual
/// draft) in a ready, unblocked input box, then submit Return atomically via
/// `send_after_screen_check`. Never submit on timeout or when pre-Escape queue state was unconfirmed.
#[allow(clippy::too_many_arguments)]
pub(super) async fn submit_antigravity_popped_queue(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    guard: AntigravityQueuedEscapeGuard,
    since: Instant,
    cancelled: &(impl Fn() -> bool + Sync + ?Sized),
) -> Result<(), SessionChatSendError> {
    if !guard.confirmed_queue {
        return Ok(());
    }
    let deadline = Instant::now() + Duration::from_millis(ANTIGRAVITY_POP_WAIT_MS);
    let ready_text = loop {
        if cancelled() {
            return Err(SessionChatSendError::not_attempted(
                SESSION_CHAT_SEND_CANCELLED.to_string(),
            ));
        }
        if prompt_card_arrived_since(project_id, session_id, since) {
            return Ok(());
        }
        if let Some(screen) = capture_session_terminal_text_vt(zmx_name).await {
            if let Some(state) = inspect_antigravity_screen(&screen) {
                if let Some(text) = popped_text_if_ready(&state, guard.baseline_draft.as_deref()) {
                    break Some(text);
                }
            }
        }
        if Instant::now() >= deadline {
            break None;
        }
        tokio::time::sleep(Duration::from_millis(ANTIGRAVITY_POP_POLL_MS)).await;
    };
    let Some(popped_text) = ready_text else {
        return Ok(());
    };
    if cancelled() {
        return Err(SessionChatSendError::not_attempted(
            SESSION_CHAT_SEND_CANCELLED.to_string(),
        ));
    }
    if transcript_recorded_prompt(guard.transcript.as_ref()).await {
        return Ok(());
    }
    if !write_antigravity_popped_return(
        project_id,
        session_id,
        zmx_name,
        source,
        &popped_text,
        guard.baseline_draft.as_deref(),
        since,
    )
    .await?
    {
        return Ok(());
    }
    let settle_deadline = Instant::now() + Duration::from_millis(ANTIGRAVITY_SUBMIT_SETTLE_MS);
    loop {
        tokio::time::sleep(Duration::from_millis(ANTIGRAVITY_SUBMIT_POLL_MS)).await;
        if cancelled() || prompt_card_arrived_since(project_id, session_id, since) {
            return Ok(());
        }
        let Some(screen) = capture_session_terminal_text_vt(zmx_name).await else {
            return Ok(());
        };
        let still_held = inspect_antigravity_screen(&screen)
            .and_then(|state| popped_text_if_ready(&state, guard.baseline_draft.as_deref()))
            .is_some_and(|text| {
                normalize_session_chat_screen_text(&text)
                    == normalize_session_chat_screen_text(&popped_text)
            });
        if !still_held {
            return Ok(());
        }
        if Instant::now() >= settle_deadline {
            if transcript_recorded_prompt(guard.transcript.as_ref()).await {
                return Ok(());
            }
            let _ = write_antigravity_popped_return(
                project_id,
                session_id,
                zmx_name,
                source,
                &popped_text,
                guard.baseline_draft.as_deref(),
                since,
            )
            .await?;
            return Ok(());
        }
    }
}

/// Whether the agent's own screen shows that nothing waits in its queue. `false` whenever the
/// screen cannot settle it (unreadable, another agent, Claude's input box holding text), so an
/// unclear screen never drops a click.
async fn agent_queue_shows_empty(zmx_name: &str, agent: Option<&str>) -> bool {
    match crate::agents::identity::normalize_agent_id(agent).as_deref() {
        Some(agent @ ("claude" | "openclaude")) => capture_session_terminal_text_vt(zmx_name)
            .await
            .and_then(|screen| {
                crate::session_chat_composer::session_chat_composer_input(agent, &screen)
            })
            .is_some_and(|input| {
                input.text_is_empty()
                    && !input.shell_mode
                    && !input
                        .text
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .to_lowercase()
                        .contains(CLAUDE_QUEUE_HINT)
            }),
        Some("codex") => capture_session_terminal_text(zmx_name)
            .await
            .is_some_and(|screen| {
                crate::session_chat_notice::codex_queued_input_previews(&screen).is_empty()
            }),
        _ => false,
    }
}

/// When each session last had a submit key (Return, or Empryo's Alt+Q) written to it.
fn last_submits() -> &'static Mutex<HashMap<String, Instant>> {
    static SUBMITS: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    SUBMITS.get_or_init(Mutex::default)
}

/// Notes a submit key written to the session's terminal; `write_session_chat_payload` calls it.
pub(super) fn note_session_chat_submit(project_id: &str, session_id: &str, payload: &str) {
    if payload != SESSION_CHAT_SUBMIT && payload != SESSION_CHAT_EMPRYO_SUBMIT {
        return;
    }
    if let Ok(mut submits) = last_submits().lock() {
        submits.insert(
            crate::server::session_observer_key(project_id, session_id),
            Instant::now(),
        );
    }
}

/// CDXC:SessionChat 2026-10-22 WHY:
/// The play button's Escape ("Queued, click to interrupt and send") waits in the session's send queue behind any send still in flight, so a click made while the message was still being typed for a busy agent ran after that message's own Return. Claude Code hands a prompt back to its input box, without recording it, when an Escape lands between taking its Return and writing the prompt to the transcript; on 2026-10-22 (Main Orchestrator, mastro_ed) five clicks made while the send waited out the agent's turn were typed 2 to 8 seconds after its Return, the first one cancelled the message Claude had just taken, and the user saw "Your message might not have reached the agent" until they sent it again. A later click has the same hazard once the agent starts the queued prompt itself (its turn ended, or a first click already interrupted it): the next Escape cancels or interrupts that prompt. A click is not always made before that start, so "a prompt recorded after the click" alone missed it: live on 2026-10-22 a second click five seconds after the first interrupted the queued prompt Claude had already started. The Escape therefore also needs the agent's own screen to show a message waiting: Claude Code's "Press up to edit queued messages" in its empty input box, Codex's queued-message preview. The screen, not the transcript: a prompt can wait in Claude's queue behind thousands of transcript rows (1% of the queued prompts in this machine's transcripts waited behind more than 6,700), some enqueue rows are never released by a queue row, and Codex writes nothing while a message waits. Live on 2026-10-22, Codex 0.163 had already taken a message sent mid-turn into the running turn, a click then interrupted that turn and the message went unanswered, and a second click armed "esc again to edit previous message" on an idle Codex.
/// CDXC:SessionChat 2026-10-22 DECISION:
/// User, asked what a play-button click made while the message is still being typed should do: "Nothing, the message waits" (the agent queues it behind its turn and the button stays to be clicked again). Asked whether to also drop the click once the agent starts the prompt itself: "Yes, close it". So the Escape is written only while none of these has happened since the click: no submit key reached the session, the transcript records no prompt the agent took, and the agent's screen does not show its queue empty.
pub(super) async fn queued_prompt_escape_check(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    agent: Option<&str>,
    requested_at: Instant,
    transcript: Option<&QueuedPromptTranscriptMark>,
) -> QueuedPromptEscapeVerdict {
    let submitted_since_click = last_submits()
        .lock()
        .ok()
        .and_then(|submits| {
            submits
                .get(&crate::server::session_observer_key(project_id, session_id))
                .copied()
        })
        .is_some_and(|submitted_at| submitted_at > requested_at);
    if submitted_since_click {
        return QueuedPromptEscapeVerdict::DropStale;
    }
    if crate::agents::identity::normalize_agent_id(agent).as_deref() == Some("antigravity") {
        let antigravity_guard = match capture_session_terminal_text_vt(zmx_name)
            .await
            .as_deref()
            .and_then(inspect_antigravity_screen)
        {
            Some(state) if !state.shows_queued => return QueuedPromptEscapeVerdict::DropStale,
            Some(state) => Some(AntigravityQueuedEscapeGuard {
                confirmed_queue: true,
                baseline_draft: state.input_text,
                transcript: transcript.cloned(),
            }),
            // Unreadable screen, overlay, or picker: do not arm auto-Return.
            None => None,
        };
        if transcript_recorded_prompt(transcript).await {
            return QueuedPromptEscapeVerdict::DropStale;
        }
        return QueuedPromptEscapeVerdict::Proceed(antigravity_guard);
    }
    if agent_queue_shows_empty(zmx_name, agent).await
        || transcript_recorded_prompt(transcript).await
    {
        return QueuedPromptEscapeVerdict::DropStale;
    }
    QueuedPromptEscapeVerdict::Proceed(None)
}
