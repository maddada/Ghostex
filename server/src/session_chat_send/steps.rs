use super::*;

// ---------------------------------------------------------------------------
// Step builders (upstream chat spec §7.5)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionChatSendStep {
    RetryCodexConversation,
    DriveCodexAsyncQuestion(crate::session_chat_codex_async_answer::AsyncAnswer),
    /// Stop this interrupt job if Escape would open Codex's message-editing pager.
    GuardCodexInterrupt,
    /// Stop this interrupt job if its Escape would be Claude's double-tap rewind.
    GuardClaudeInterrupt,
    /// Stop the play button's Escape when the session took a submit or recorded a prompt after
    /// the click (session_chat_send/queued_prompt_escape.rs).
    GuardQueuedPromptEscape {
        requested_at: Instant,
        agent: Option<String>,
        transcript: Option<QueuedPromptTranscriptMark>,
    },
    /// Wait for Cursor to show "ctrl+c to stop", and stop this interrupt job if it never does.
    GuardCursorInterrupt,
    /// Answer the "Tell the agent what to do instead" box Cursor opens after an approval's Skip
    /// with `CURSOR_SKIP_REASON`, once it shows (`cursor_decision_skips_outright`).
    SkipCursorInsteadBox,
    /// Stop this interrupt job unless Hermes shows a running turn and an empty input line
    /// (session_chat_hermes_interrupt.rs).
    GuardHermesInterrupt,
    /// Stop this interrupt job unless Copilot shows a running turn (session_chat_copilot_interrupt.rs).
    GuardCopilotInterrupt,
    /// Stop this interrupt job unless Grok shows a running turn (session_chat_grok_interrupt.rs).
    GuardGrokInterrupt,
    /// Recheck the transcript pager and cross-client visibility at the front of the queue.
    CloseUnwatchedCodexTranscriptPager,
    /// Scroll Claude's `/btw` panel end to end and keep its whole answer (session_chat_claude_panel.rs).
    ReadClaudeSideAnswer,
    StopLocalCommandOutput,
    /// Baseline the screen so the command's own output can be read back off it.
    /// `durable_id` is set for a slash command the user sent from chat, which is
    /// archived in `session_chat_local_command.rs`.
    BeginLocalCommandOutput {
        agent: Option<String>,
        command: String,
        durable_id: Option<String>,
    },
    FinishLocalCommandOutput,
    /// Revalidate after reaching the front of the send queue, before any dialog input.
    VerifyTerminalDialog {
        agent: String,
        id: String,
    },
    /// Revalidate that the agent still shows the list or screen the chat is sending keys to
    /// (session_chat_resume_picker.rs). Unlike a dialog, any repaint is fine: every key repaints it.
    VerifyTerminalPicker {
        agent: String,
    },
    /// Move any existing agent-TUI composer draft into Saved Prompts before
    /// chat takes ownership of the input line. The prompt-editor handshake
    /// answers only after the CLI has durably stashed and cleared that draft.
    PreserveTerminalDraft {
        state_dir: PathBuf,
        prompt_editor_input: String,
        /// A view transfer may replace its own copy, but must keep newer terminal typing.
        replacement: Option<String>,
    },
    /// One `zmx send` stdin burst.
    Write(String),
    /// One `zmx send` stdin burst logged under its OWN diagnostic-input source
    /// instead of the job's. Out-of-band writers (the title jobs) fold a whole
    /// sequence — draft kill → command text → settle → Enter → draft restore —
    /// into a SINGLE job so no other job can land between its writes, and this
    /// keeps every one of those writes attributed the way its own dispatch used
    /// to attribute it (`auto-title-command`, `manual-title-submit`, …).
    WriteFrom {
        source: String,
        payload: String,
    },
    SleepMs(u64),
    /// See CDXC:SessionChat in session_chat_codex_question_align.rs.
    AlignCodexQuestion {
        question: usize,
        row: Option<usize>,
    },
    /// See CDXC:SessionChat in session_chat_question_row_align.rs.
    AlignQuestionRow(crate::session_chat_question_row_align::QuestionRowTarget),
    /// See CDXC:SessionChat in session_chat_claude_question_prep.rs.
    PrepareClaudeQuestion(crate::session_chat_claude_question_prep::ClaudeQuestionPrep),
    /// Close these questions' Claude selector with Escape, or stop the job when it stays open
    /// (`cancel_claude_question` in session_chat_claude_question_answer.rs).
    CancelClaudeQuestion(Vec<crate::session_chat::SessionChatQuestion>),
    /// Make Empryo's window show this session's own tab, or stop the job (session_chat_empryo_tabs.rs).
    SelectEmpryoTab {
        wait_ms: u64,
    },
    /// Read the text in the agent's input box (Empryo, ZCode), without touching it, into the job's
    /// draft sink.
    ReadInputBoxDraft {
        agent: String,
    },
    /// Bring Claude Code's queued messages back into its empty input box with Up, read them off
    /// the transcript into the job's draft sink, and empty the box again (claude_queue_pull.rs).
    PullClaudeQueue {
        transcript: PathBuf,
    },
    /// Delete the queued message `text` from Grok Build's queue panel, answering it in the job's
    /// draft sink when it was there (grok_queue_pull.rs).
    PullGrokQueue {
        text: String,
    },
    /// Stop the job when Empryo's input box holds text other than `replacement`.
    GuardEmpryoDraft {
        replacement: String,
    },
    /// Read the agent's input box after the submit key; see session_chat_send_submit.rs.
    VerifySubmitted {
        agent: String,
        text: String,
        /// The submit key the send wrote, pressed again when the box still holds the message.
        submit: String,
    },
    /// Close a positively identified Claude Code panel whose Escape is safe (Settings, or
    /// an offer listed in session_chat_claude_popups.rs), or Codex's side conversation with
    /// Ctrl+C, then require its real composer to appear before any later input-line write can run.
    DismissClaudePanel {
        agent: Option<String>,
        timeout_ms: u64,
    },
    /// Close Claude Code's self-drafted feedback card with Escape, one press per capture that
    /// still shows it (session_chat_claude_feedback_card.rs).
    DismissClaudeFeedbackCard,
    /*
    CDXC:SessionChat 2026-08-26:
    Hold the sequence until the agent CLI's input box is on screen. This runs
    BEFORE the clear burst, which is the whole point: the burst is Ctrl+U/Ctrl+K
    keystrokes, and a CLI showing a trust dialog or an auth menu answers those
    with whatever those keys mean to IT. Nothing is written until the composer
    is proved, and a proof that never arrives aborts the sequence, so no Enter
    can follow.

    Fail-open on `Unknown` is deliberate and matches VerifyPasteLanded: an
    unreadable screen, or an agent with no measured signature, must never be
    what stops a message.
    */
    WaitForComposer {
        agent: Option<String>,
        settle_ms: u64,
        timeout_ms: u64,
    },
    StopClaudeBackground {
        background: crate::accounts::claude_background::BackgroundSession,
        home_dir: PathBuf,
    },
    InterruptAgentForAccountSwitch {
        home_dir: PathBuf,
        timeout_ms: u64,
    },
    /// Type a start command at the shell prompt, submit it, and make sure it ran
    /// (session_chat_send/launch_command.rs).
    RunShellCommand {
        command: String,
        home_dir: PathBuf,
        timeout_ms: u64,
    },
    /// Keep the restart sequence exclusive until the selected login is running and ready.
    WaitForAccountReady {
        agent: String,
        expected_identity: String,
        home_dir: PathBuf,
        previous_process_id: Option<i64>,
        timeout_ms: u64,
        /// The session's folders are remembered as trusted, so the folder trust prompt the new
        /// login asks is accepted while waiting (accounts/restart_verification.rs).
        accept_remembered_trust: bool,
    },
    ClearComposer {
        agent: String,
    },
    /// CDXC:SessionFork 2026-09-24 WHY:
    /// A startup rename can wait behind a prompt or manual rename in the input queue. Recheck persisted ownership before pasting and submitting. A failed send can leave its exact rename staged, so that retry completes only the owned command's Enter without clearing or repasting; different input remains untouched.
    GuardForkRename {
        agent: String,
        title: String,
        state_db_file: PathBuf,
        server_id: String,
        first_user_message: Option<String>,
        rename_requested_at: Option<String>,
        expected_composer: Option<String>,
    },
    /// Hold the sequence until the session's screen proves `text` reached the
    /// agent's composer (CDXC:Clipboard). Settles `settle_ms`,
    /// then polls captures until the deadline `timeout_ms` sets. Failing this
    /// step aborts the sequence, so the Enter that follows it can never
    /// submit a composer the body never reached.
    VerifyPasteLanded {
        text: String,
        settle_ms: u64,
        timeout_ms: u64,
    },
    /// After the clear-and-retype attempt's paste check: clear the input box and type the
    /// message once when the first, late paste landed as well (session_chat_send/single_paste.rs).
    KeepSinglePaste {
        agent: String,
        text: String,
    },
    /// Lets this job's verified clear, paste check and submit check keep waiting while the agent
    /// is in the middle of a turn (session_chat_send/busy_input_wait.rs). Writes nothing.
    WaitOutBusyAgent(AgentMidTurnProbe),
    /*
    CDXC:SessionChat 2026-09-02:
    Hand the input line to the agent rewind driver for its whole terminal
    dialog. The driver is adaptive (it reads the screen and decides the next
    keystroke from what it sees), so it cannot be expressed as a fixed step
    list, but it MUST still own the pty the way a fixed list does: a queued
    prompt landing between two Up presses would be typed into the dialog. So it
    runs as one step of one job here, and reports through the job registry in
    session_chat_rewind.rs rather than through this enum, which stays plain
    data.
    */
    DriveSessionChatRewind {
        job_id: u64,
    },
    /// Hand the input line to the Codex model picker driver
    /// (session_chat_codex_picker.rs) for the whole `/model` flow, for the same
    /// reasons DriveSessionChatRewind lists: adaptive, and it must own the pty.
    DriveCodexModelPicker {
        job_id: u64,
    },
    DriveClaudeModelPicker {
        job_id: u64,
    },
    DriveProviderModelPicker {
        job_id: u64,
    },
}

/// The screen-watch window for a payload: a floor that covers small pastes,
/// plus time proportional to the byte count, capped so a wedged TUI cannot
/// hold the per-session queue.
pub fn session_chat_verify_timeout_ms(text_bytes: usize) -> u64 {
    // ConPTY delivers console input records incrementally: a measured 3.3KB
    // Codex paste stayed blank for 3.8s before its chip appeared. Keep polling
    // for positive evidence, with a Windows ingestion budget of 2ms per byte.
    let ingestion_ms = if cfg!(windows) {
        (text_bytes as u64).saturating_mul(2)
    } else {
        text_bytes as u64 / SESSION_CHAT_VERIFY_BYTES_PER_MS
    };
    let scaled = SESSION_CHAT_VERIFY_SETTLE_MS.saturating_add(ingestion_ms);
    scaled
        .max(SESSION_CHAT_VERIFY_MIN_TIMEOUT_MS)
        .min(SESSION_CHAT_VERIFY_MAX_TIMEOUT_MS)
}

/*
The clear discipline EVERY server-side writer of the agent composer shares:
the measured burst (the 2N-1 law, sized for the text about to be written) as
its OWN write, followed by the settle that keeps it out of the next write's
stdin chunk. Chunk separation is not cosmetic — a burst that coalesces with
the following frame is inserted as literal text (see
SESSION_CHAT_CLEAR_INPUT_SETTLE_MS above) — so the pair is built here once
rather than restated by each writer. `source` attributes the burst to an
out-of-band writer's own diagnostic-input source (the title jobs); `None`
leaves it on the job's.
*/
pub fn build_agent_tui_clear_input_steps(
    source: Option<&str>,
    text: &str,
) -> Vec<SessionChatSendStep> {
    let payload = build_agent_tui_clear_input_for_text(text);
    vec![
        match source {
            Some(source) => SessionChatSendStep::WriteFrom {
                source: source.to_string(),
                payload,
            },
            None => SessionChatSendStep::Write(payload),
        },
        SessionChatSendStep::SleepMs(SESSION_CHAT_CLEAR_INPUT_SETTLE_MS),
    ]
}

pub fn build_session_chat_clear_input_steps(
    agent: Option<&str>,
    text: &str,
) -> Vec<SessionChatSendStep> {
    if let Some(agent) = crate::agents::identity::normalize_agent_id(agent)
        .filter(|agent| input_replace::supports_verified_composer_clear(agent))
    {
        vec![SessionChatSendStep::ClearComposer { agent }]
    } else {
        build_agent_tui_clear_input_steps(None, text)
    }
}

/// composer wait → verified agent-specific clear → image pastes back-to-back →
/// (300ms settle when text follows images) → paste body → screen-verified wait
/// → SEPARATE Enter.
pub fn build_session_chat_message_steps(
    agent: Option<&str>,
    text: &str,
    image_paths: &[String],
    dismiss_claude_panel: bool,
) -> Vec<SessionChatSendStep> {
    let empryo = crate::agents::identity::normalize_agent_id(agent).as_deref() == Some("empryo");
    let mut steps = Vec::new();
    if dismiss_claude_panel {
        steps.push(SessionChatSendStep::DismissClaudePanel {
            agent: agent.map(str::to_string),
            timeout_ms: SESSION_CHAT_COMPOSER_WAIT_TIMEOUT_MS,
        });
    }
    steps.push(SessionChatSendStep::WaitForComposer {
        agent: agent.map(str::to_string),
        settle_ms: SESSION_CHAT_COMPOSER_WAIT_SETTLE_MS,
        timeout_ms: SESSION_CHAT_COMPOSER_WAIT_TIMEOUT_MS,
    });
    if empryo {
        steps.push(SessionChatSendStep::SelectEmpryoTab {
            wait_ms: crate::session_chat_empryo_tabs::EMPRYO_SEND_TAB_WAIT_MS,
        });
    }
    steps.extend(build_session_chat_clear_input_steps(agent, text));
    for path in image_paths {
        steps.push(SessionChatSendStep::Write(
            build_session_chat_image_paste_bytes(path),
        ));
    }
    if !text.trim().is_empty() {
        if !image_paths.is_empty() {
            steps.push(SessionChatSendStep::SleepMs(
                SESSION_CHAT_IMAGE_ATTACHMENT_SETTLE_MS,
            ));
        }
        steps.push(SessionChatSendStep::Write(if empryo {
            build_empryo_input_bytes(text)
        } else {
            build_session_chat_paste_bytes(text)
        }));
    }
    let mut verify = session_chat_verify_step(text);
    if crate::session_chat_options::is_session_chat_option_command_text(agent, text) {
        // Short option commands can be checked immediately; Enter still requires visible paste evidence.
        if let Some(SessionChatSendStep::VerifyPasteLanded { settle_ms, .. }) = &mut verify {
            *settle_ms = 0;
        }
    }
    steps.push(
        verify
            // Nothing on screen could confirm this payload, so the Enter keeps
            // the original blind settle. That is only an images-only send,
            // whose payload is one short path — the size that never lost a
            // message.
            .unwrap_or(SessionChatSendStep::SleepMs(SESSION_CHAT_SUBMIT_DELAY_MS)),
    );
    /*
    CDXC:SessionChat 2026-10-07 WHY:
    Every Empryo message is submitted with Alt+Q and every slash command with Enter (the Empryo build coordinator's call; Sven delegated it to the cleanest Empryo experience). This supersedes 2026-10-06's "Enter when idle, Alt+Q when busy": Empryo 3.9.1-beta no longer marks a running turn on its prompt glyph, and its own submit sends an Alt+Q message at once while idle, queues it as its own turn while one runs (Enter would steer that turn), and skips the "Enter again" gate of a repo map still building; slash commands run directly either way, and Enter keeps them clear of the queue. Multi-line text is typed with Shift+Enter between lines rather than pasted (CDXC:SessionChat 2026-10-06 in input_bytes.rs).
    */
    // Empryo's own slash check: a trimmed `/` start runs as a command with either key.
    let submit = if empryo && !text.trim().starts_with('/') {
        SESSION_CHAT_EMPRYO_SUBMIT
    } else {
        SESSION_CHAT_SUBMIT
    };
    steps.push(SessionChatSendStep::Write(submit.to_string()));
    if let Some(agent) = crate::agents::identity::normalize_agent_id(agent)
        .filter(|agent| crate::session_chat_send_submit::verifies_submission(agent))
        .filter(|_| !text.trim().is_empty())
    {
        steps.push(SessionChatSendStep::VerifySubmitted {
            agent,
            text: text.to_string(),
            submit: submit.to_string(),
        });
    }
    steps
}

// ---------------------------------------------------------------------------
// Paste verification (CDXC:Clipboard)
// ---------------------------------------------------------------------------

/// Longest needle taken from one line of the message. Long enough that a hit
/// is evidence, short enough to survive a composer that re-wraps and truncates.
const SESSION_CHAT_VERIFY_NEEDLE_CHARS: usize = 40;

/// Everything a composer is free to change about text it was handed: the frame
/// it draws around the input line, its continuation indent, and the row breaks
/// it inserts — which land mid-word for long tokens. Dropping whitespace and
/// box drawing from BOTH sides is what makes a needle survive re-wrapping.
///
/// The comparison is deliberately lossy in the safe direction: a false match
/// only degrades this step to the old blind-Enter behaviour, while a false
/// miss would abort a message the agent did receive.
pub(crate) fn normalize_session_chat_screen_text(text: &str) -> String {
    text.chars()
        .filter(|character| {
            !character.is_whitespace()
                && !matches!(character, '\u{2500}'..='\u{259f}')
                && !character.is_control()
        })
        .collect()
}

/// Normalized fragments of the message to look for on screen. Both ends are
/// sampled because a long composer shows only part of what it holds — the head
/// while it is still being filled, the tail once it scrolled.
pub(crate) fn session_chat_paste_needles(text: &str) -> Vec<String> {
    let mut needles: Vec<String> = Vec::new();
    let lines: Vec<&str> = text
        .split(['\r', '\n'])
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    for line in [lines.first(), lines.last()].into_iter().flatten() {
        let normalized: String = normalize_session_chat_screen_text(line)
            .chars()
            .take(SESSION_CHAT_VERIFY_NEEDLE_CHARS)
            .collect();
        if !normalized.is_empty() && !needles.contains(&normalized) {
            needles.push(normalized);
        }
    }
    needles
}

/// The verify step for a message body, or `None` when no capture could ever
/// recognise this payload: an images-only send (the composer renders an
/// attachment chip, never the pasted path) or a body with no characters that
/// survive normalization.
pub(super) fn session_chat_verify_step(text: &str) -> Option<SessionChatSendStep> {
    if text.trim().is_empty() || session_chat_paste_needles(text).is_empty() {
        return None;
    }
    Some(SessionChatSendStep::VerifyPasteLanded {
        text: text.to_string(),
        settle_ms: SESSION_CHAT_VERIFY_SETTLE_MS,
        timeout_ms: session_chat_verify_timeout_ms(text.len()),
    })
}

/// What a capture said about the pasted body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SessionChatPasteVerification {
    /// The body (or its collapsed placeholder) is on screen.
    Landed,
    /// The screen was readable for the whole window and never showed it.
    Absent,
    /// Every capture failed or was truncated: the screen proves nothing.
    Unreadable,
    /// The send was superseded or cancelled while waiting.
    Cancelled,
}

/*
Raw key injection: one write, verbatim. No clear burst (there is no input line
to own), no bracketed paste (the bytes ARE keystrokes — framing them would
make the TUI read them as text) and no trailing Enter (the key IS the
submission). Unknown names return None so the handler can reject them instead
of writing something arbitrary.
*/
/// CDXC:SessionChat 2026-09-28 WHY:
/// The Escape that ends a running turn, shared by Stop (`interruptSessionChat`) and the queued prompt's play button (`sendKey` `escape`), with the agent's guard in front: Claude's drops a second Escape inside its rewind window, and Codex's first clears a selection, a search or a scrolled-up view in its fullscreen transcript, each of which takes one Escape before the turn does. The play button wrote a bare Escape, so while the user had scrolled Codex's view it only returned to the bottom and the queued prompt kept waiting.
pub(crate) fn session_chat_interrupt_escape_steps(agent: Option<&str>) -> Vec<SessionChatSendStep> {
    let mut steps = Vec::new();
    match agent {
        Some("codex") => steps.push(SessionChatSendStep::GuardCodexInterrupt),
        Some("claude") => steps.push(SessionChatSendStep::GuardClaudeInterrupt),
        // CDXC:SessionChat 2026-10-07 WHY: Cursor CLI stops a running turn on Ctrl+C ("ctrl+c to stop") and ignores Escape while it works, so chat Stop answered "interrupted" while the turn kept running. Ctrl+C stops the turn only once Cursor shows it is processing (about 5 s after a send on Windows); before that it only arms "Press Ctrl+C again to exit", so the guard waits for that state and sends nothing if it never comes.
        Some("cursor" | "cursor-agent") => {
            steps.push(SessionChatSendStep::GuardCursorInterrupt);
            steps.push(SessionChatSendStep::Write("\u{3}".to_string()));
            return steps;
        }
        // Copilot's Escape only backgrounds a running shell (session_chat_copilot_interrupt.rs).
        Some("copilot") => {
            steps.push(SessionChatSendStep::GuardCopilotInterrupt);
            steps.push(SessionChatSendStep::Write("\u{3}".to_string()));
            return steps;
        }
        // Grok cancels a turn on Ctrl+C and ignores Escape (session_chat_grok_interrupt.rs).
        Some("grok") => {
            steps.push(SessionChatSendStep::GuardGrokInterrupt);
            steps.push(SessionChatSendStep::Write("\u{3}".to_string()));
            return steps;
        }
        // Hermes cancels a turn on Ctrl+C, never on Escape (session_chat_hermes_interrupt.rs).
        Some("hermes") => {
            steps.push(SessionChatSendStep::GuardHermesInterrupt);
            steps.push(SessionChatSendStep::Write("\u{3}".to_string()));
            return steps;
        }
        _ => {}
    }
    steps.push(SessionChatSendStep::Write(
        SESSION_CHAT_INTERRUPT.to_string(),
    ));
    steps
}

pub fn build_session_chat_key_steps(key: &str) -> Option<Vec<SessionChatSendStep>> {
    if key == "enter" {
        return Some(vec![
            SessionChatSendStep::SleepMs(250),
            SessionChatSendStep::Write("\r".to_string()),
        ]);
    }
    let payload = match key {
        // The chat's play button on a prompt the agent queued: one Escape ends the running turn so the agent takes it now.
        "escape" => SESSION_CHAT_INTERRUPT,
        "shift-tab" => SESSION_CHAT_SHIFT_TAB,
        "shift-up" => SESSION_CHAT_SHIFT_UP,
        "shift-down" => SESSION_CHAT_SHIFT_DOWN,
        _ => return None,
    };
    Some(vec![SessionChatSendStep::Write(payload.to_string())])
}

/*
CDXC:SessionChat 2026-08-22:
Answering an on-screen picker (Claude Code's resume-usage chooser today): type
the chosen row's NUMBER, and nothing else.

This used to walk the highlight with arrow keys and confirm with Enter, which
always answered row 1. Measured on a zmx pty: `ESC [ B` written into that picker
does not move the highlight at all, so every walk was a no-op and the trailing
Enter committed whatever was already highlighted. The digit both selects and
commits — no Enter, no settle, nothing to pace. Same behaviour, same fix, and
same reason as Claude's AskUserQuestion selector above.

One verbatim write: no clear burst and no bracketed paste, because this is a
keystroke for a dialog that owns the input line, not text for a composer.

CDXC:AgentScreenDetection 2026-09-04 DECISION:
The permission prompt is the one picker whose keys are an arrow walk plus
Enter instead of a digit (user: "Enter for Yes and down arrow then Enter for
No"); `SessionChatTerminalPicker::answer_key` derives the walk from the
highlight in the answer-time capture, and this still writes it verbatim.
*/
pub fn build_terminal_picker_answer_steps(answer_key: &str) -> Vec<SessionChatSendStep> {
    vec![SessionChatSendStep::Write(answer_key.to_string())]
}

/// Keystroke groups written 1000ms apart; raw groups go verbatim, text groups
/// through the paste sanitizer.
pub fn build_ask_answer_steps(groups: &[AskAnswerKeyGroup]) -> Vec<SessionChatSendStep> {
    let mut steps = Vec::new();
    for (index, group) in groups.iter().enumerate() {
        // An alignment step returns only once the screen shows the row it
        // wanted, so the key after it needs no settle time of its own.
        let after_alignment = index > 0
            && matches!(
                groups[index - 1],
                AskAnswerKeyGroup::AlignCodexQuestion { .. }
                    | AskAnswerKeyGroup::AlignQuestionRow(_)
                    | AskAnswerKeyGroup::PrepareClaudeQuestion(_)
            );
        if index > 0 && !after_alignment {
            steps.push(SessionChatSendStep::SleepMs(SESSION_CHAT_QUESTION_STEP_MS));
        }
        steps.push(match group {
            AskAnswerKeyGroup::Raw(raw) => SessionChatSendStep::Write(raw.clone()),
            AskAnswerKeyGroup::Text(text) => {
                SessionChatSendStep::Write(build_session_chat_paste_bytes(text))
            }
            AskAnswerKeyGroup::AlignCodexQuestion { question, row } => {
                SessionChatSendStep::AlignCodexQuestion {
                    question: *question,
                    row: *row,
                }
            }
            AskAnswerKeyGroup::AlignQuestionRow(target) => {
                SessionChatSendStep::AlignQuestionRow(target.clone())
            }
            AskAnswerKeyGroup::PrepareClaudeQuestion(prep) => {
                SessionChatSendStep::PrepareClaudeQuestion(prep.clone())
            }
        });
    }
    steps
}
