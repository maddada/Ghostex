use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::Duration,
};

use tokio::sync::{mpsc, oneshot};

use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::logging::{GxserverLogInput, GxserverLogger, LogLevel};
use crate::protocol::rpc_success;
use crate::server::{
    domain_error_response, read_runtime_text, routed_json, AppState, RoutedResponse,
};
use crate::session_chat::{SessionChatQuestion, SessionChatQuestionSelection};
use crate::session_chat_follower::session_chat_agent_for_session;
use crate::session_chat_options::schedule_session_chat_option_redetect;
use crate::session_chat_queue_runtime::send_session_chat_message_internal;
use crate::storage::open_gxserver_database;
use axum::http::StatusCode;
use serde_json::{json, Map, Value};

/*
CDXC:SessionChatSend 2026-07-31:
Session Chat send path (upstream chat spec §7/§8 port). The agent is a TUI, so sending is
writing bytes to its pty via `zmx send` stdin. The spec's measured discipline is
preserved verbatim: a Ctrl+U/Ctrl+K clear burst sized by the 2N-1 law, a
bracketed-paste body with ESC sanitized and newlines normalized to CR, and a
SEPARATE Enter write (a trailing \r inside the paste burst is read as newline
text and the message stays staged). What is NOT preserved is the blind 500ms
settle that used to precede that Enter: the Enter now waits until a screen
capture proves the composer actually took the body, and is never written when
the body is provably absent (CDXC:SessionChatVerifyPaste below). A per-session
queue serializes sequences: each send owns the input line from its clear
until its Enter fires. HTTP handlers enqueue and return immediately.
*/

// Master constant table (upstream chat spec §7.1).
pub const SESSION_CHAT_SUBMIT_DELAY_MS: u64 = 500;
pub const SESSION_CHAT_QUESTION_STEP_MS: u64 = 1_000;
pub const SESSION_CHAT_IMAGE_ATTACHMENT_SETTLE_MS: u64 = 300;
/*
The clear burst must reach the TUI in its OWN stdin read chunk. Written
back-to-back with a paste frame, the two coalesce into one chunk, and Claude
Code's chunk-level paste handling inserts the burst bytes as literal text at
the head of the message instead of interpreting them as kill keys (observed
2026-08-23: a chat-sent prompt was recorded with 39×Ctrl-U + 39×Ctrl-K glued
to its front, which also left the optimistic echo unconsumed — the duplicated
user bubble). Same pacing discipline as the image settle and the separate
delayed Enter.
*/
pub const SESSION_CHAT_CLEAR_INPUT_SETTLE_MS: u64 = 150;
pub const SESSION_CHAT_SUBMIT: &str = "\r";
/*
CDXC:SessionChatVerifyPaste 2026-08-24:
Why the Enter is closed-loop. The old sequence wrote the paste body, slept
SESSION_CHAT_SUBMIT_DELAY_MS, then wrote a bare "\r" with nothing checking
that the composer had taken the body. Claude Code ingests a multi-KB paste
asynchronously — it paints "Pasting text…" and only later collapses the body
into a "[Pasted text #N +M lines]" placeholder — and under machine load that
ingestion takes LONGER than 500ms. Reproduced deterministically 2026-08-23
with a 69-line / 4.6KB message: the bare Enter submitted an EMPTY composer,
the body arrived afterwards, and the user's message was silently lost because
it remained stranded in the terminal composer. zmx transport was byte-perfect
at that size, so the race is purely TUI ingestion time versus a fixed delay.

The fix is to watch the screen instead of the clock: settle briefly (so a
paste that already landed costs no extra latency), then poll captures until
the body is provably on screen. A send whose body cannot be proven present is
ABORTED without an Enter — losing the send with an error the user sees is
strictly better than submitting an empty turn and dropping their text.
*/
pub const SESSION_CHAT_VERIFY_SETTLE_MS: u64 = SESSION_CHAT_SUBMIT_DELAY_MS;
pub const SESSION_CHAT_VERIFY_POLL_MS: u64 = 150;
pub const SESSION_CHAT_VERIFY_MIN_TIMEOUT_MS: u64 = 2_000;
pub const SESSION_CHAT_VERIFY_MAX_TIMEOUT_MS: u64 = 8_000;

/*
CDXC:SessionChatComposerReady 2026-08-26:
The window `SessionChatSendStep::WaitForComposer` gives a CLI to paint its input
box. Short settle because the overwhelming majority of sends go to a CLI that
has been idle for minutes and answers on the first capture; six seconds of
ceiling because the slow case is a cold agent process still loading its config,
skills and MCP servers, which was measured past four on this machine — the very
reason the old blind four-second sleeps kept losing first prompts.
*/
pub const SESSION_CHAT_COMPOSER_WAIT_SETTLE_MS: u64 = 0;
pub const SESSION_CHAT_COMPOSER_WAIT_TIMEOUT_MS: u64 = 6_000;
/// Bytes of payload per millisecond of extra patience: a paste twice the size
/// takes about twice as long to ingest.
pub const SESSION_CHAT_VERIFY_BYTES_PER_MS: u64 = 2;
/// "Pasting text…" is the TUI telling us ingestion is still running, so the
/// deadline is worth extending — once, so a wedged indicator cannot stall the
/// queue indefinitely.
pub const SESSION_CHAT_VERIFY_PASTING_EXTENSION_MS: u64 = 4_000;
/// The TUIs' collapsed large-paste placeholders. A body the composer collapsed
/// never shows its own text, so the placeholder is the only proof of landing.
/// Measured live 2026-08-24 with a 1KB / 28-line paste into every supported
/// agent: Claude Code `[Pasted text #1 +69 lines]`, Codex
/// `[Pasted Content 1037 chars]`, pi `[paste #1 +28 lines]`, Grok Build
/// `[Pasted: 28 lines]`, omp `[Paste #1, +28 lines]`. Five agents, five
/// spellings, one shared prefix — so the match is that prefix,
/// case-insensitively, against the whitespace-stripped screen. (Matching only
/// the Claude form aborted Codex sends that had in fact been delivered.)
const SESSION_CHAT_PASTED_PLACEHOLDER_NEEDLE: &str = "[paste";
/// Claude Code's still-ingesting indicator, `Pasting text…`, normalized.
const SESSION_CHAT_PASTING_INDICATOR_NEEDLE: &str = "Pastingtext";
/// Shown to the user when the composer never took the body. Deliberately
/// describes the terminal, not the network: nothing was submitted.
pub const SESSION_CHAT_PASTE_NOT_ACCEPTED: &str = "The terminal did not accept the pasted message.";
/// Fallback for a composer wait that timed out without a per-agent reason.
pub const SESSION_CHAT_COMPOSER_NOT_READY: &str =
    "The agent's input box is not on screen, so nothing was sent.";
/*
Esc in the kitty CSI-u encoding (CSI 27 u). Ghostex agent sessions always run
under zmx, whose VT layer answers the kitty keyboard-protocol query, so Claude
Code runs with the protocol enabled and a lone 0x1b byte is never delivered as
an Esc keypress (it reads as the ambiguous start of a sequence and is dropped).
Verified live 2026-08-01 against Claude Code v2.1.220 on a zmx pty: "\x1b" did
not interrupt a running turn; "\x1b[27u" interrupted immediately. Crossterm-
based TUIs (codex) parse CSI-u Esc as well, so one encoding covers both.
*/
pub const SESSION_CHAT_INTERRUPT: &str = "\u{1b}[27u";
/*
Shift+Tab in the kitty CSI-u encoding (CSI 9 ; 2 u — Tab with the Shift
modifier). Claude Code cycles its permission mode on it and has no
slash-command equivalent, so the chat surface injects the raw bytes.
Verified live 2026-08-01 against Claude Code v2.1.220 on a zmx pty: the legacy
back-tab "\x1b[Z" did nothing, while "\x1b[9;2u" cycled the footer through
auto → manual → accept edits → plan → bypass on every write. Same kitty-active
reasoning as SESSION_CHAT_INTERRUPT above.
*/
pub const SESSION_CHAT_SHIFT_TAB: &str = "\u{1b}[9;2u";
pub const SESSION_CHAT_SHIFT_UP: &str = "\u{1b}[1;2A";
pub const SESSION_CHAT_SHIFT_DOWN: &str = "\u{1b}[1;2B";
/*
The two kill keys, and deliberately no Ctrl+Y beside them. A yank returns only
the LAST kill, which after a 2N-1 burst is a fragment of a multi-line draft or
nothing at all, so no writer of this input line can restore what it cleared.
Writers discard (the chat-send policy); terminal→chat view switching stays the
loss-safe transfer.
*/
pub const AGENT_TUI_CLEAR_INPUT_LINE: &str = "\u{15}"; // Ctrl+U — clear toward start
pub const AGENT_TUI_CLEAR_INPUT_FORWARD: &str = "\u{b}"; // Ctrl+K — clear toward end
pub const AGENT_TUI_CLEAR_LINE_SLACK: usize = 8;
pub const AGENT_TUI_CLEAR_MAX_LINES: usize = 40;
const SESSION_CHAT_DRAFT_PRESERVE_TIMEOUT: Duration = Duration::from_secs(16);
const SESSION_CHAT_PROMPT_EDITOR_INPUT: &str = "\u{7}";
const SESSION_CHAT_GROK_PROMPT_EDITOR_INPUT: &str = "\u{10}editor\r";
const PROMPT_STASH_REQUEST_FRESHNESS: Duration = Duration::from_secs(15);
const BRACKETED_PASTE_START: &str = "\u{1b}[200~";
const BRACKETED_PASTE_END: &str = "\u{1b}[201~";
static SESSION_CHAT_DRAFT_PRESERVE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

// Ask-answer keystrokes (upstream chat spec §8.4/§8.5).
const ASK_ENTER: &str = "\r";
const ASK_NEXT_TAB: &str = "\u{1b}[C"; // Right arrow → next question / Submit tab
const ASK_PREVIOUS_ROW: &str = "\u{1b}[A"; // Up
const ASK_NEXT_ROW: &str = "\u{1b}[B"; // Down
const ASK_NOTES: &str = "\t"; // Tab → open notes (Codex)
const ASK_DELETE: &str = "\u{7f}"; // DEL — clear/skip a Codex row
const ASK_TAB: &str = "\t"; // Tab → next question tab (omp's ask dialog)
const ASK_SPACE: &str = " "; // Space → toggle a multi-select row (omp)

// ---------------------------------------------------------------------------
// Clear burst (upstream chat spec §7.2) — measured, not derived
// ---------------------------------------------------------------------------

/// Logical line count: `text.split(/\r\n|\r|\n/).length`. Wrapping is
/// irrelevant; one Ctrl+U clears exactly one logical line.
pub fn count_agent_tui_input_lines(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut lines = 1usize;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' => {
                lines += 1;
                if bytes.get(index + 1) == Some(&b'\n') {
                    index += 1;
                }
            }
            b'\n' => lines += 1,
            _ => {}
        }
        index += 1;
    }
    lines
}

/// The 2N-1 law: repetitions = 2*lines - 1 of Ctrl+U, then the same count of
/// Ctrl+K. The known line count is a LOWER bound (the user can also type into
/// the TUI directly), so bias upward — overshoot measured perfectly clean on
/// both Claude and Codex; undershoot leaves residue glued onto the next
/// message.
pub fn build_agent_tui_clear_input(line_count: usize) -> String {
    let lines = line_count.clamp(1, AGENT_TUI_CLEAR_MAX_LINES);
    let repetitions = 2 * lines - 1;
    format!(
        "{}{}",
        AGENT_TUI_CLEAR_INPUT_LINE.repeat(repetitions),
        AGENT_TUI_CLEAR_INPUT_FORWARD.repeat(repetitions)
    )
}

pub fn build_agent_tui_clear_input_for_text(text: &str) -> String {
    build_agent_tui_clear_input(count_agent_tui_input_lines(text) + AGENT_TUI_CLEAR_LINE_SLACK)
}

// ---------------------------------------------------------------------------
// Bracketed paste & sanitization (upstream chat spec §7.3/§7.4)
// ---------------------------------------------------------------------------

/// An embedded ESC (e.g. a pasted `\x1b[201~` from scrollback) would close
/// the paste frame early and run the tail as KEYSTROKES; replace with ␛.
pub fn sanitize_bracketed_paste_text(text: &str) -> String {
    text.replace('\u{1b}', "\u{241b}")
}

/// xterm's native paste converts every clipboard newline to CR; direct frames
/// must match, or ConPTY TUIs treat raw LF as submit.
pub fn normalize_terminal_paste_line_endings(text: &str) -> String {
    text.replace("\r\n", "\r").replace('\n', "\r")
}

/*
Agent composers reserve a final backslash followed by Return for inserting a
newline. Ghostex sends the body and Return as separate pty writes, so a prompt
whose final byte is `\\` otherwise triggers that shortcut instead of submitting.

Stage one terminal-only trailing space to disambiguate the Return. Supported
agent composers trim trailing whitespace when they submit, so the logical
prompt still ends in the user's backslash. Apply this at the shared terminal
encoding boundary so direct chat sends, queued sends, interactive free-text
answers, and generic submitted session messages all obey the same rule.
*/
pub fn disambiguate_agent_tui_submit_text(text: &str) -> String {
    let mut staged = text.to_string();
    if staged.ends_with('\\') {
        staged.push(' ');
    }
    staged
}

pub fn wrap_terminal_bracketed_paste_text(text: &str) -> String {
    format!(
        "{BRACKETED_PASTE_START}{}{BRACKETED_PASTE_END}",
        sanitize_bracketed_paste_text(&normalize_terminal_paste_line_endings(text))
    )
}

/// Trailing newline alone counts as multiline.
pub fn is_multiline_draft(text: &str) -> bool {
    text.contains(['\r', '\n'])
}

/// Multiline → framed (NO submit); single-line → sanitized unframed text.
pub fn build_session_chat_paste_bytes(text: &str) -> String {
    let staged = disambiguate_agent_tui_submit_text(text);
    if is_multiline_draft(text) {
        wrap_terminal_bracketed_paste_text(&staged)
    } else {
        sanitize_bracketed_paste_text(&staged)
    }
}

/// Image paths must LOOK like a real terminal image paste; a plain typed
/// path/@mention is read as text/file-read.
pub fn build_session_chat_image_paste_bytes(path: &str) -> String {
    wrap_terminal_bracketed_paste_text(path)
}

// ---------------------------------------------------------------------------
// Ask-answer keystroke builders (upstream chat spec §8.4/§8.5/§8.6)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AskAnswerKeyGroup {
    /// Written verbatim (arrows, digits, Enter, Tab, DEL).
    Raw(String),
    /// Free text; goes through the paste sanitizer when written.
    Text(String),
}

fn selection_other(selection: Option<&SessionChatQuestionSelection>) -> &str {
    selection
        .and_then(|selection| selection.other.as_deref())
        .unwrap_or_default()
        .trim()
}

fn answer_labels(
    question: &SessionChatQuestion,
    selection: Option<&SessionChatQuestionSelection>,
) -> Vec<String> {
    let mut labels: Vec<String> = selection
        .map(|selection| {
            selection
                .indices
                .iter()
                .map(|index| {
                    question
                        .options
                        .get(*index)
                        .map(|option| option.label.clone())
                        .unwrap_or_default()
                })
                .filter(|label| !label.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let other = selection_other(selection);
    if !other.is_empty() {
        labels.push(other.to_string());
    }
    labels
}

/*
Claude's AskUserQuestion is an arrow-navigate selector: a bare Enter commits
the HIGHLIGHTED default and pasted label text does NOT move the highlight
(bug STA-1860 delivered every non-first pick as the first option). Drive it
by each option's stable 1-based number, which matches the card's badge.
Groups are paced NATIVE_CHAT_QUESTION_STEP_MS apart by the queue because a
navigation keystroke batched with Enter commits before the selector applied
it.
*/
pub fn build_claude_ask_answer_keys(
    questions: &[SessionChatQuestion],
    selections: &[SessionChatQuestionSelection],
) -> Vec<AskAnswerKeyGroup> {
    let mut groups: Vec<AskAnswerKeyGroup> = Vec::new();
    let multi_question = questions.len() > 1;
    for (question_index, question) in questions.iter().enumerate() {
        let selection = selections.get(question_index);
        let other = selection_other(selection);
        let type_something = (question.options.len() + 1).to_string(); // the "Type something" row
        let indices: &[usize] = selection
            .map(|selection| selection.indices.as_slice())
            .unwrap_or_default();
        if question.multi_select {
            for index in indices {
                // Each digit TOGGLES a checkbox.
                groups.push(AskAnswerKeyGroup::Raw((index + 1).to_string()));
            }
            if !other.is_empty() {
                groups.push(AskAnswerKeyGroup::Raw(type_something));
                groups.push(AskAnswerKeyGroup::Text(other.to_string()));
                groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
            }
            // Multi-select never auto-advances; step to next/Submit tab.
            groups.push(AskAnswerKeyGroup::Raw(ASK_NEXT_TAB.to_string()));
        } else if !other.is_empty() {
            // Single-select carries one value, so route ANY answer containing
            // free text through "Type something" as one joined string.
            groups.push(AskAnswerKeyGroup::Raw(type_something));
            groups.push(AskAnswerKeyGroup::Text(
                answer_labels(question, selection).join(", "),
            ));
            groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
        } else if let Some(first) = indices.first() {
            // Selects AND commits; auto-advances in multi-question.
            groups.push(AskAnswerKeyGroup::Raw((first + 1).to_string()));
        } else if multi_question {
            // Unanswered question: step past it.
            groups.push(AskAnswerKeyGroup::Raw(ASK_NEXT_TAB.to_string()));
        }
    }
    let ends_on_submit_tab = multi_question || (questions.len() == 1 && questions[0].multi_select);
    if ends_on_submit_tab && !groups.is_empty() {
        // Final Submit confirmation.
        groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
    }
    groups
}

/*
Codex's request_user_input overlay submits on the final option digit,
attaches free text as NOTES to the highlighted row, and starts on the first
row. Notes navigation moves WITHOUT committing via the shortest arrow path.
*/
pub fn build_codex_ask_answer_keys(
    questions: &[SessionChatQuestion],
    selections: &[SessionChatQuestionSelection],
) -> Vec<AskAnswerKeyGroup> {
    let mut groups: Vec<AskAnswerKeyGroup> = Vec::new();
    let mut has_unanswered = false;
    let last_index = questions.len().saturating_sub(1);
    for (question_index, question) in questions.iter().enumerate() {
        let selection = selections.get(question_index);
        let selected_index = selection.and_then(|selection| selection.indices.first().copied());
        let note = selection_other(selection);
        if !note.is_empty() {
            // Default target: the notes row (one past the last option).
            let target_index = selected_index.unwrap_or(question.options.len());
            let row_count = question.options.len() + 1;
            let next_steps = target_index;
            let previous_steps = row_count - target_index;
            let use_previous = previous_steps < next_steps; // pick the shorter path
            let (key, steps) = if use_previous {
                (ASK_PREVIOUS_ROW, previous_steps)
            } else {
                (ASK_NEXT_ROW, next_steps)
            };
            for _ in 0..steps {
                groups.push(AskAnswerKeyGroup::Raw(key.to_string()));
            }
            groups.push(AskAnswerKeyGroup::Raw(ASK_NOTES.to_string()));
            groups.push(AskAnswerKeyGroup::Text(note.to_string()));
            groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
            continue;
        }
        if let Some(selected_index) = selected_index {
            // Digit commits.
            groups.push(AskAnswerKeyGroup::Raw((selected_index + 1).to_string()));
            continue;
        }
        has_unanswered = true;
        groups.push(AskAnswerKeyGroup::Raw(ASK_DELETE.to_string()));
        groups.push(AskAnswerKeyGroup::Raw(if question_index < last_index {
            ASK_NEXT_TAB.to_string()
        } else {
            ASK_ENTER.to_string()
        }));
    }
    if has_unanswered {
        // Codex opens a confirmation; Proceed is highlighted by default.
        groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
    }
    groups
}

/*
Pi's cursor_ask_question renders pi-tui selects (ctx.ui.select): ↑/↓ move the
highlight (wrapping), Enter commits, Esc/Ctrl+C cancels — no digit shortcuts
and no skip. Questions show ONE AT A TIME, so the groups for question N+1 only
land after question N's Enter; the queue's per-group pacing covers the repaint.
A question that allows a custom answer appends one "Type a custom answer" row
after its options, and committing that row opens a one-line input
(ctx.ui.input) that submits on Enter. An optionless question shows that bare
input directly. Cancelling any question ends pi's whole question loop, so an
unanswered question emits the cancel and nothing after it.
*/
pub fn build_pi_ask_answer_keys(
    questions: &[SessionChatQuestion],
    selections: &[SessionChatQuestionSelection],
) -> Vec<AskAnswerKeyGroup> {
    let mut groups: Vec<AskAnswerKeyGroup> = Vec::new();
    for (question_index, question) in questions.iter().enumerate() {
        let selection = selections.get(question_index);
        let other = selection_other(selection);
        let selected_index = selection.and_then(|selection| selection.indices.first().copied());
        let allows_custom = question.allow_custom != Some(false);
        if question.options.is_empty() {
            let answer = answer_labels(question, selection).join(", ");
            if answer.is_empty() {
                groups.push(AskAnswerKeyGroup::Raw(SESSION_CHAT_INTERRUPT.to_string()));
                break;
            }
            groups.push(AskAnswerKeyGroup::Text(answer));
            groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
            continue;
        }
        if !other.is_empty() && allows_custom {
            // Single-value answer: any picked labels join the free text as one
            // string through the custom-answer input (the Claude single-select
            // rule). The custom row sits one past the last option.
            groups.push(AskAnswerKeyGroup::Raw(
                ASK_NEXT_ROW.repeat(question.options.len()),
            ));
            groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
            groups.push(AskAnswerKeyGroup::Text(
                answer_labels(question, selection).join(", "),
            ));
            groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
            continue;
        }
        if let Some(index) = selected_index {
            if index > 0 {
                groups.push(AskAnswerKeyGroup::Raw(ASK_NEXT_ROW.repeat(index)));
            }
            groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
            continue;
        }
        groups.push(AskAnswerKeyGroup::Raw(SESSION_CHAT_INTERRUPT.to_string()));
        break;
    }
    groups
}

/*
Hermes' clarify panel numbers every row: in single-select mode a digit (1-9,
then 0 for the 10th row) SUBMITS that choice directly — and in batch
(multi-question) mode it locks the active question's answer and auto-advances
to the next unanswered one, so the same digit sequence drives both layouts.
The row one past the last choice is "Other (type your answer)": its digit
switches the composer into freetext mode, where typed text + Enter submits
(or locks, in batch mode). Multi-select rows are checkboxes — digits TOGGLE
and Enter confirms; checking Other routes through freetext the same way.
There is no skip key and Esc is not bound to the panel, so an unanswered
question simply stops the key plan and leaves the panel to the terminal.
*/
pub fn build_hermes_ask_answer_keys(
    questions: &[SessionChatQuestion],
    selections: &[SessionChatQuestionSelection],
) -> Vec<AskAnswerKeyGroup> {
    // Panel row digit for 0-based row `index`: 1-9, then 0 for the 10th row.
    // Clarify caps choices at 4, so Other is at worst row 5 — the guards only
    // matter if that cap ever moves.
    fn row_digit(index: usize) -> Option<String> {
        match index {
            0..=8 => Some((index + 1).to_string()),
            9 => Some("0".to_string()),
            _ => None,
        }
    }
    let mut groups: Vec<AskAnswerKeyGroup> = Vec::new();
    for (question_index, question) in questions.iter().enumerate() {
        let selection = selections.get(question_index);
        let other = selection_other(selection);
        let indices: &[usize] = selection
            .map(|selection| selection.indices.as_slice())
            .unwrap_or_default();
        if question.options.is_empty() {
            // Open-ended question: the panel is already in freetext mode.
            let answer = answer_labels(question, selection).join(", ");
            if answer.is_empty() {
                break;
            }
            groups.push(AskAnswerKeyGroup::Text(answer));
            groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
            continue;
        }
        let other_row = question.options.len();
        if question.multi_select {
            if indices.is_empty() && other.is_empty() {
                break;
            }
            for index in indices {
                let Some(digit) = row_digit(*index) else {
                    continue;
                };
                // Each digit TOGGLES a checkbox.
                groups.push(AskAnswerKeyGroup::Raw(digit));
            }
            if !other.is_empty() {
                let Some(digit) = row_digit(other_row) else {
                    break;
                };
                groups.push(AskAnswerKeyGroup::Raw(digit)); // check Other
                groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string())); // → freetext
                groups.push(AskAnswerKeyGroup::Text(other.to_string()));
            }
            groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
            continue;
        }
        if !other.is_empty() {
            // Single-value answer: any picked labels join the free text as one
            // string through Other's freetext (the Claude single-select rule).
            let Some(digit) = row_digit(other_row) else {
                break;
            };
            groups.push(AskAnswerKeyGroup::Raw(digit));
            groups.push(AskAnswerKeyGroup::Text(
                answer_labels(question, selection).join(", "),
            ));
            groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
            continue;
        }
        if let Some(digit) = indices.first().copied().and_then(row_digit) {
            // Submits (single) / locks and advances (batch).
            groups.push(AskAnswerKeyGroup::Raw(digit));
            continue;
        }
        break;
    }
    groups
}

/*
omp's built-in `ask` tool opens its rich dialog (one tab per question plus a
Submit tab whenever there is more than one question or any multi question).
The cursor starts on the `recommended` row (default 0, clamped) and ↑/↓ move
it WITHOUT wrapping; rows are the options in order plus a trailing
"Other (type your own)" row. Single-select Enter picks the row and
auto-advances (submitting a single-question dialog directly); Enter on Other
opens a custom-answer prompt that submits on Enter. Multi-select Space
toggles, and the plan advances with Tab instead of Enter because Enter on the
Other row would re-open the prompt. The final Enter confirms the Submit tab.
Esc cancels the whole dialog, so it is only sent for an unanswered
single-question dialog, which has no tab to step past.
*/
pub fn build_omp_ask_answer_keys(
    questions: &[SessionChatQuestion],
    selections: &[SessionChatQuestionSelection],
) -> Vec<AskAnswerKeyGroup> {
    fn move_cursor(groups: &mut Vec<AskAnswerKeyGroup>, from: usize, to: usize) {
        if to > from {
            groups.push(AskAnswerKeyGroup::Raw(ASK_NEXT_ROW.repeat(to - from)));
        } else if from > to {
            groups.push(AskAnswerKeyGroup::Raw(ASK_PREVIOUS_ROW.repeat(from - to)));
        }
    }
    let has_submit_tab =
        questions.len() > 1 || questions.iter().any(|question| question.multi_select);
    let mut cancelled = false;
    let mut groups: Vec<AskAnswerKeyGroup> = Vec::new();
    for (question_index, question) in questions.iter().enumerate() {
        let selection = selections.get(question_index);
        let other = selection_other(selection);
        let indices: &[usize] = selection
            .map(|selection| selection.indices.as_slice())
            .unwrap_or_default();
        if indices.is_empty() && other.is_empty() {
            if has_submit_tab {
                // Skip: step to the next question tab, leaving no answer.
                groups.push(AskAnswerKeyGroup::Raw(ASK_TAB.to_string()));
                continue;
            }
            groups.push(AskAnswerKeyGroup::Raw(SESSION_CHAT_INTERRUPT.to_string()));
            cancelled = true;
            break;
        }
        let other_row = question.options.len();
        // The dialog clamps the initial cursor to [0, other_row].
        let start = question.recommended.unwrap_or(0).min(other_row);
        if question.multi_select {
            let mut cursor = start;
            for index in indices {
                if *index >= question.options.len() {
                    continue;
                }
                move_cursor(&mut groups, cursor, *index);
                groups.push(AskAnswerKeyGroup::Raw(ASK_SPACE.to_string()));
                cursor = *index;
            }
            if !other.is_empty() {
                move_cursor(&mut groups, cursor, other_row);
                groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string())); // open the prompt
                groups.push(AskAnswerKeyGroup::Text(other.to_string()));
                groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string())); // prompt submits
            }
            groups.push(AskAnswerKeyGroup::Raw(ASK_TAB.to_string()));
            continue;
        }
        if !other.is_empty() {
            // Single-value answer: picked labels join the free text as one
            // string through the custom-answer prompt (the Claude rule).
            move_cursor(&mut groups, start, other_row);
            groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string())); // open the prompt
            groups.push(AskAnswerKeyGroup::Text(
                answer_labels(question, selection).join(", "),
            ));
            groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string())); // submit + advance
            continue;
        }
        let index = (*indices.first().expect("indices checked non-empty")).min(other_row);
        move_cursor(&mut groups, start, index);
        groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string())); // pick + advance/submit
    }
    if has_submit_tab && !cancelled && !groups.is_empty() {
        // Final Submit confirmation.
        groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
    }
    groups
}

/// Non-stepping agents (Grok): one line per question, IN ORDER; empty answers
/// stay empty lines so N lines === N questions.
pub fn format_ask_answer(
    questions: &[SessionChatQuestion],
    selections: &[SessionChatQuestionSelection],
) -> String {
    questions
        .iter()
        .enumerate()
        .map(|(index, question)| answer_labels(question, selections.get(index)).join(", "))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn has_ask_answer(selections: &[SessionChatQuestionSelection]) -> bool {
    selections.iter().any(|selection| {
        !selection.indices.is_empty()
            || !selection
                .other
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
    })
}

// ---------------------------------------------------------------------------
// Step builders (upstream chat spec §7.5)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionChatSendStep {
    /// Move any existing agent-TUI composer draft into Saved Prompts before
    /// chat takes ownership of the input line. The prompt-editor handshake
    /// answers only after the CLI has durably stashed and cleared that draft.
    PreserveTerminalDraft {
        state_dir: PathBuf,
        prompt_editor_input: String,
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
    /*
    CDXC:SessionChatComposerReady 2026-08-26:
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
    /// Hold the sequence until the session's screen proves `text` reached the
    /// agent's composer (CDXC:SessionChatVerifyPaste). Settles `settle_ms`,
    /// then polls captures until the deadline `timeout_ms` sets. Failing this
    /// step aborts the sequence, so the Enter that follows it can never
    /// submit a composer the body never reached.
    VerifyPasteLanded {
        text: String,
        settle_ms: u64,
        timeout_ms: u64,
    },
}

/// The screen-watch window for a payload: a floor that covers small pastes,
/// plus time proportional to the byte count, capped so a wedged TUI cannot
/// hold the per-session queue.
pub fn session_chat_verify_timeout_ms(text_bytes: usize) -> u64 {
    let scaled = SESSION_CHAT_VERIFY_SETTLE_MS
        .saturating_add(text_bytes as u64 / SESSION_CHAT_VERIFY_BYTES_PER_MS);
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

/// composer wait → clear burst → 150ms settle → image pastes back-to-back →
/// (300ms settle when text follows images) → paste body → screen-verified wait
/// → SEPARATE Enter.
pub fn build_session_chat_message_steps(
    agent: Option<&str>,
    text: &str,
    image_paths: &[String],
) -> Vec<SessionChatSendStep> {
    let mut steps = vec![SessionChatSendStep::WaitForComposer {
        agent: agent.map(str::to_string),
        settle_ms: SESSION_CHAT_COMPOSER_WAIT_SETTLE_MS,
        timeout_ms: SESSION_CHAT_COMPOSER_WAIT_TIMEOUT_MS,
    }];
    steps.extend(build_agent_tui_clear_input_steps(None, text));
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
        steps.push(SessionChatSendStep::Write(build_session_chat_paste_bytes(
            text,
        )));
    }
    steps.push(
        session_chat_verify_step(text)
            // Nothing on screen could confirm this payload, so the Enter keeps
            // the original blind settle. That is only an images-only send,
            // whose payload is one short path — the size that never lost a
            // message.
            .unwrap_or(SessionChatSendStep::SleepMs(SESSION_CHAT_SUBMIT_DELAY_MS)),
    );
    steps.push(SessionChatSendStep::Write(SESSION_CHAT_SUBMIT.to_string()));
    steps
}

// ---------------------------------------------------------------------------
// Paste verification (CDXC:SessionChatVerifyPaste)
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
fn normalize_session_chat_screen_text(text: &str) -> String {
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
fn session_chat_paste_needles(text: &str) -> Vec<String> {
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
fn session_chat_verify_step(text: &str) -> Option<SessionChatSendStep> {
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
enum SessionChatPasteVerification {
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
pub fn build_session_chat_key_steps(key: &str) -> Option<Vec<SessionChatSendStep>> {
    let payload = match key {
        "shift-tab" => SESSION_CHAT_SHIFT_TAB,
        "shift-up" => SESSION_CHAT_SHIFT_UP,
        "shift-down" => SESSION_CHAT_SHIFT_DOWN,
        _ => return None,
    };
    Some(vec![SessionChatSendStep::Write(payload.to_string())])
}

/*
CDXC:SessionChatTerminalPicker 2026-08-22:
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
*/
pub fn build_terminal_picker_answer_steps(answer_key: &str) -> Vec<SessionChatSendStep> {
    vec![SessionChatSendStep::Write(answer_key.to_string())]
}

/// Keystroke groups written 1000ms apart; raw groups go verbatim, text groups
/// through the paste sanitizer.
pub fn build_ask_answer_steps(groups: &[AskAnswerKeyGroup]) -> Vec<SessionChatSendStep> {
    let mut steps = Vec::new();
    for (index, group) in groups.iter().enumerate() {
        if index > 0 {
            steps.push(SessionChatSendStep::SleepMs(SESSION_CHAT_QUESTION_STEP_MS));
        }
        steps.push(SessionChatSendStep::Write(match group {
            AskAnswerKeyGroup::Raw(raw) => raw.clone(),
            AskAnswerKeyGroup::Text(text) => build_session_chat_paste_bytes(text),
        }));
    }
    steps
}

// ---------------------------------------------------------------------------
// Per-session send queue (upstream chat spec §7.6)
// ---------------------------------------------------------------------------

/*
CDXC:SessionChatTerminalNotices 2026-08-19:
Why a send did not complete. The message a caller shows the user is the same as
before; what is new is that the caller can tell "the terminal refused this
message" (the agent CLI in the pane never answered the Ctrl+G handshake, or zmx
would not take the bytes — the crashed-agent case this feature exists to
explain) apart from "this send was never attempted" (superseded by a newer send,
cancelled, or the queue was gone). Only the former is evidence about the
terminal, so only the former may raise a notice.
*/
const SESSION_CHAT_SEND_CANCELLED: &str = "The session chat send was cancelled.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionChatSendFailure {
    /// Superseded, cancelled, or never dequeued: the terminal was never asked.
    NotAttempted,
    /// The Ctrl+G draft-preservation handshake never completed.
    PreserveTerminalDraft,
    /// A `zmx send` burst was refused.
    Write,
    /*
    CDXC:SessionChatComposerReady 2026-08-26:
    The agent CLI never showed an input box within the wait. Distinct from
    `Write` because nothing was written: there is no half-typed composer to
    explain and nothing for the delivery watchdog to verify, and the caller maps
    it to its own `composerNotReady` code so the UI can say what is on the
    screen instead of "the terminal refused the input".
    */
    ComposerNotReady,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionChatSendError {
    pub failure: SessionChatSendFailure,
    pub message: String,
}

impl SessionChatSendError {
    fn new(failure: SessionChatSendFailure, message: String) -> Self {
        Self { failure, message }
    }

    fn not_attempted(message: String) -> Self {
        Self::new(SessionChatSendFailure::NotAttempted, message)
    }

    /// True when the session's own terminal is what refused the message —
    /// which is what makes a watchdog escalation (one more capture, to explain
    /// the refusal) worth spending. A composer that never appeared already
    /// carries its own explanation, so it is not one of these.
    pub fn terminal_refused(&self) -> bool {
        matches!(
            self.failure,
            SessionChatSendFailure::PreserveTerminalDraft | SessionChatSendFailure::Write
        )
    }

    /// True when the send stopped because the agent CLI had no input box.
    pub fn composer_not_ready(&self) -> bool {
        self.failure == SessionChatSendFailure::ComposerNotReady
    }
}

struct SessionChatSendJob {
    completion: Option<oneshot::Sender<Result<(), SessionChatSendError>>>,
    /// Set only by `capture_session_chat_terminal_draft`: the standalone
    /// draft-handoff endpoint needs the draft its `PreserveTerminalDraft` step
    /// captured, not merely whether the step succeeded.
    captured_draft: Option<oneshot::Sender<CapturedTerminalDraft>>,
    project_id: String,
    session_id: String,
    source: String,
    zmx_name: String,
    generation: u64,
    steps: Vec<SessionChatSendStep>,
}

struct SessionChatSendQueue {
    tx: mpsc::UnboundedSender<SessionChatSendJob>,
    generation: Arc<AtomicU64>,
}

static SESSION_CHAT_SEND_QUEUES: OnceLock<Mutex<HashMap<String, SessionChatSendQueue>>> =
    OnceLock::new();

fn queue_key(project_id: &str, session_id: &str) -> String {
    format!("{project_id}|{session_id}")
}

/*
Invariant preserved from the upstream chat spec: each sequence owns the input line from its
clear until its Enter fires. One worker task per session drains jobs
serially; a cancelled generation skips queued jobs at dequeue AND aborts the
remaining steps of an in-flight job before its next write/sleep. A failed
zmx write aborts the rest of its sequence so a dangling Enter can never
follow a body that was not delivered. Must be called from within the tokio
runtime (HTTP handlers are).

CDXC:SessionChatSerializedWriters 2026-08-24:
"Each sequence owns the input line" only holds if EVERY server-side writer to
that pty goes through here. Until this date several did not — the first-prompt
auto-title job, the manual "generate name" job (which writes a Ctrl+U draft
kill!), delayed sends, the raw `/api/sendSessionText` + `/api/sendSessionEnter`
endpoints, and the draft-handoff BEL — and they interleaved with in-flight send
sequences (measured: an auto-title write landed 70ms after a corrupted send,
because the auto-title job is triggered by the FIRST user prompt, i.e. exactly
while that prompt is still being delivered). They all enqueue now, and a writer
whose bytes must not be split apart (text → settle → Enter) enqueues them as ONE
job, because separate jobs may be separated by somebody else's job.
*/
pub fn enqueue_session_chat_send(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    steps: Vec<SessionChatSendStep>,
) {
    let _ = queue_session_chat_send(project_id, session_id, zmx_name, source, steps, None, None);
}

/// The same fire-and-forget enqueue for a server-side writer that holds a
/// repository session row rather than a resolved zmx name. The name is resolved
/// exactly as the zmx endpoints resolve it, so a session whose provider is not
/// a zmx pty is rejected instead of written to blindly.
pub fn enqueue_session_write_sequence(
    session: &Value,
    project_id: &str,
    session_id: &str,
    source: &str,
    steps: Vec<SessionChatSendStep>,
) -> std::result::Result<(), DomainStateError> {
    let zmx_name = crate::zmx::provider_zmx_session_name(session)?;
    enqueue_session_chat_send(project_id, session_id, &zmx_name, source, steps);
    Ok(())
}

/// Enqueues one sequence on the same per-session worker as fire-and-forget
/// sends, but resolves only after every preservation/write step has completed.
/// Chat message HTTP calls use this so the composer is cleared only after the
/// terminal draft is safe and the new prompt was actually submitted.
pub async fn execute_session_chat_send(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    steps: Vec<SessionChatSendStep>,
) -> Result<(), SessionChatSendError> {
    let (completion_tx, completion_rx) = oneshot::channel();
    queue_session_chat_send(
        project_id,
        session_id,
        zmx_name,
        source,
        steps,
        Some(completion_tx),
        None,
    )
    .map_err(SessionChatSendError::not_attempted)?;
    completion_rx.await.map_err(|_| {
        SessionChatSendError::not_attempted(
            "The session chat send worker stopped before completing the message.".to_string(),
        )
    })?
}

fn queue_session_chat_send(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    steps: Vec<SessionChatSendStep>,
    completion: Option<oneshot::Sender<Result<(), SessionChatSendError>>>,
    captured_draft: Option<oneshot::Sender<CapturedTerminalDraft>>,
) -> Result<(), String> {
    if steps.is_empty() {
        return Ok(());
    }
    let queues = SESSION_CHAT_SEND_QUEUES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = queues
        .lock()
        .map_err(|_| "The session chat send queue is unavailable.".to_string())?;
    let queue = map
        .entry(queue_key(project_id, session_id))
        .or_insert_with(|| {
            let (tx, rx) = mpsc::unbounded_channel();
            let generation = Arc::new(AtomicU64::new(0));
            tokio::spawn(run_session_chat_send_worker(rx, generation.clone()));
            SessionChatSendQueue { tx, generation }
        });
    let job = SessionChatSendJob {
        completion,
        captured_draft,
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
        source: source.to_string(),
        zmx_name: zmx_name.to_string(),
        generation: queue.generation.load(Ordering::SeqCst),
        steps,
    };
    queue
        .tx
        .send(job)
        .map_err(|_| "The session chat send worker is unavailable.".to_string())
}

/// Cancels every queued (and the remaining steps of any in-flight) send for a
/// session by bumping its generation. Later enqueues use the new generation.
pub fn cancel_session_chat_sends(project_id: &str, session_id: &str) {
    let Some(queues) = SESSION_CHAT_SEND_QUEUES.get() else {
        return;
    };
    let Ok(map) = queues.lock() else {
        return;
    };
    if let Some(queue) = map.get(&queue_key(project_id, session_id)) {
        queue.generation.fetch_add(1, Ordering::SeqCst);
    }
}

async fn run_session_chat_send_worker(
    mut rx: mpsc::UnboundedReceiver<SessionChatSendJob>,
    generation: Arc<AtomicU64>,
) {
    while let Some(job) = rx.recv().await {
        let SessionChatSendJob {
            mut completion,
            mut captured_draft,
            project_id,
            session_id,
            source,
            zmx_name,
            generation: job_generation,
            steps,
        } = job;
        if job_generation != generation.load(Ordering::SeqCst) {
            if let Some(completion) = completion.take() {
                let _ = completion.send(Err(SessionChatSendError::not_attempted(
                    SESSION_CHAT_SEND_CANCELLED.to_string(),
                )));
            }
            continue; // cancelled while queued
        }
        let mut outcome = Ok(());
        for step in steps {
            if job_generation != generation.load(Ordering::SeqCst) {
                outcome = Err(SessionChatSendError::not_attempted(
                    SESSION_CHAT_SEND_CANCELLED.to_string(),
                ));
                break; // cancelled mid-sequence
            }
            match step {
                SessionChatSendStep::SleepMs(delay_ms) => {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
                SessionChatSendStep::PreserveTerminalDraft {
                    state_dir,
                    prompt_editor_input,
                } => {
                    match preserve_terminal_draft(
                        &state_dir,
                        &project_id,
                        &session_id,
                        &zmx_name,
                        &prompt_editor_input,
                        &generation,
                        job_generation,
                    )
                    .await
                    {
                        Ok(draft) => {
                            if let Some(sink) = captured_draft.take() {
                                let _ = sink.send(draft);
                            }
                        }
                        Err(error) => {
                            outcome = Err(error);
                            break;
                        }
                    }
                }
                SessionChatSendStep::Write(payload) => {
                    if let Err(error) = write_session_chat_payload(
                        &project_id,
                        &session_id,
                        &zmx_name,
                        &source,
                        &payload,
                    )
                    .await
                    {
                        outcome = Err(SessionChatSendError::new(
                            SessionChatSendFailure::Write,
                            error,
                        ));
                        break;
                    }
                }
                SessionChatSendStep::WriteFrom {
                    source: write_source,
                    payload,
                } => {
                    if let Err(error) = write_session_chat_payload(
                        &project_id,
                        &session_id,
                        &zmx_name,
                        &write_source,
                        &payload,
                    )
                    .await
                    {
                        outcome = Err(SessionChatSendError::new(
                            SessionChatSendFailure::Write,
                            error,
                        ));
                        break;
                    }
                }
                SessionChatSendStep::WaitForComposer {
                    agent,
                    settle_ms,
                    timeout_ms,
                } => {
                    let wait = crate::session_chat_composer::wait_for_session_chat_composer(
                        &zmx_name,
                        agent.as_deref(),
                        crate::session_chat_composer::SessionChatComposerWaitPolicy {
                            settle_ms,
                            timeout_ms,
                            // A screen this worker cannot read, or an agent with
                            // no measured signature, releases the sequence on
                            // the FIRST probe. The send is what matters; the
                            // gate is only allowed to help.
                            unknown_hold_ms: 0,
                        },
                        &|| job_generation != generation.load(Ordering::SeqCst),
                    )
                    .await;
                    match wait {
                        crate::session_chat_composer::SessionChatComposerWait::Ready
                        | crate::session_chat_composer::SessionChatComposerWait::Unknown => {}
                        crate::session_chat_composer::SessionChatComposerWait::Cancelled => {
                            outcome = Err(SessionChatSendError::not_attempted(
                                SESSION_CHAT_SEND_CANCELLED.to_string(),
                            ));
                            break;
                        }
                        crate::session_chat_composer::SessionChatComposerWait::NotReady(
                            readiness,
                        ) => {
                            let reason = readiness
                                .reason
                                .clone()
                                .unwrap_or_else(|| SESSION_CHAT_COMPOSER_NOT_READY.to_string());
                            log_session_chat_paste_verification(
                                LogLevel::Error,
                                "sessionChatComposerNotReady",
                                &project_id,
                                &session_id,
                                &zmx_name,
                                &source,
                                0,
                                timeout_ms,
                                &reason,
                            );
                            outcome = Err(SessionChatSendError::new(
                                SessionChatSendFailure::ComposerNotReady,
                                reason,
                            ));
                            break;
                        }
                    }
                }
                SessionChatSendStep::VerifyPasteLanded {
                    text,
                    settle_ms,
                    timeout_ms,
                } => {
                    let verification = verify_session_chat_paste_landed(
                        &zmx_name,
                        &text,
                        settle_ms,
                        timeout_ms,
                        &generation,
                        job_generation,
                    )
                    .await;
                    match verification {
                        SessionChatPasteVerification::Landed => {}
                        SessionChatPasteVerification::Unreadable => {
                            /*
                            The observation channel itself is down, so neither
                            "landed" nor "absent" can be shown. Submitting is
                            the pre-2026-08-24 behaviour and the only choice
                            that still delivers messages on a host whose screen
                            captures do not work — but it is never silent.
                            */
                            log_session_chat_paste_verification(
                                LogLevel::Warn,
                                "sessionChatPasteVerificationSkipped",
                                &project_id,
                                &session_id,
                                &zmx_name,
                                &source,
                                text.len(),
                                timeout_ms,
                                "The session screen could not be read, so the pasted message could not be verified before Enter.",
                            );
                        }
                        SessionChatPasteVerification::Absent => {
                            log_session_chat_paste_verification(
                                LogLevel::Error,
                                "sessionChatPasteVerificationFailed",
                                &project_id,
                                &session_id,
                                &zmx_name,
                                &source,
                                text.len(),
                                timeout_ms,
                                SESSION_CHAT_PASTE_NOT_ACCEPTED,
                            );
                            outcome = Err(SessionChatSendError::new(
                                SessionChatSendFailure::Write,
                                SESSION_CHAT_PASTE_NOT_ACCEPTED.to_string(),
                            ));
                            break;
                        }
                        SessionChatPasteVerification::Cancelled => {
                            outcome = Err(SessionChatSendError::not_attempted(
                                SESSION_CHAT_SEND_CANCELLED.to_string(),
                            ));
                            break;
                        }
                    }
                }
            }
        }
        if let Some(completion) = completion.take() {
            let _ = completion.send(outcome);
        }
    }
}

/// One `zmx send` stdin burst, logged through the shared temporary input log.
async fn write_session_chat_payload(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    payload: &str,
) -> Result<(), String> {
    crate::zmx::log_temporary_zmx_input_write(
        project_id,
        session_id,
        zmx_name,
        "sessionChatQueueWrite",
        source,
        payload,
    );
    let zmx_name = zmx_name.to_string();
    let payload = payload.to_string();
    let write = tokio::task::spawn_blocking(move || {
        crate::zmx::session_chat_zmx_write(&zmx_name, &payload)
    })
    .await;
    if matches!(write, Ok(Ok(_))) {
        Ok(())
    } else {
        Err("The session terminal did not accept the chat input.".to_string())
    }
}

/// Current terminal text for the session, or `None` when it could not be read
/// whole — a capture whose tail was dropped cannot prove what is on screen.
/// Shared with the send-delivery watchdog (session_chat_watchdog.rs), which
/// takes exactly one of these per timeout event.
pub(crate) async fn capture_session_terminal_text(zmx_name: &str) -> Option<String> {
    let zmx_name = zmx_name.to_string();
    let capture =
        tokio::task::spawn_blocking(move || crate::zmx::read_zmx_session_screen_capture(&zmx_name))
            .await
            .ok()?
            .ok()?;
    (!capture.truncated).then_some(capture.text)
}

/*
CDXC:SessionChatVerifyPaste 2026-08-24:
The screen watch that stands between a paste body and its Enter. It settles
once (so a paste the TUI already took costs nothing extra), then polls captures
until one of three things is true: the body — or the TUI's collapsed paste
placeholder (`[Pasted text …]`, `[Pasted Content …]`, `[paste …]`, … — every
supported agent collapses large pastes, each with its own spelling) — is on
screen, the deadline passed, or the send was superseded. "Pasting text…" is the TUI saying ingestion is still running,
which buys one deadline extension rather than an abort.

Absence is only ever reported from a screen that was actually readable. When
every capture failed or came back truncated the observation channel is what is
broken, not the send, and the caller submits with a warn record instead of
destroying a message on evidence it does not have.
*/
async fn verify_session_chat_paste_landed(
    zmx_name: &str,
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
        if let Some(screen) = capture_session_terminal_text(zmx_name).await {
            readable = true;
            let normalized = normalize_session_chat_screen_text(&screen);
            if normalized
                .to_lowercase()
                .contains(SESSION_CHAT_PASTED_PLACEHOLDER_NEEDLE)
                || needles.iter().any(|needle| normalized.contains(needle))
            {
                return SessionChatPasteVerification::Landed;
            }
            if !extended && normalized.contains(SESSION_CHAT_PASTING_INDICATOR_NEEDLE) {
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
    } else {
        SessionChatPasteVerification::Unreadable
    }
}

static SESSION_CHAT_SEND_LOGGER: OnceLock<GxserverLogger> = OnceLock::new();

/// A send that could not be verified is a message the user may never see
/// again, so this is persisted unconditionally (warn/error), unlike the
/// scenario-gated per-write input log. Metadata only — never the message text.
#[allow(clippy::too_many_arguments)]
fn log_session_chat_paste_verification(
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

fn prompt_stash_request_path(state_dir: &Path, project_id: &str, session_id: &str) -> PathBuf {
    state_dir
        .join("prompt-stash-requests")
        .join(format!("{project_id}-{session_id}"))
}

fn prompt_handoff_response_path(state_dir: &Path, request_id: &str) -> PathBuf {
    state_dir
        .join("prompt-handoffs")
        .join(format!("{request_id}.json"))
}

/// What the CLI's prompt-editor handshake reported about the composer draft it
/// just moved out of the agent TUI. `prompt_id` is `None` when the composer was
/// empty; `created` marks a stash row this capture owns (as opposed to an
/// update of an existing one), so a caller that only wanted the text can delete
/// it again without destroying a prompt the user had stashed themselves.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapturedTerminalDraft {
    pub created: bool,
    pub prompt_id: Option<String>,
}

/*
CDXC:SessionChatDraftHandoff 2026-08-18:
Terminal → chat draft transfer, shared by every host. The bytes a user typed
into the agent TUI live only in that TUI's composer, so the sole way to read
them is the agent's prompt-editor contract: drop a one-shot `handoff:<id>`
marker, ask the agent to open its external editor, and let `ghostex
prompt-editor` stash the composer into Saved Prompts, clear it, and answer
through the response file. Running it here rather than in each client is what
lets remote gpui sessions and the phone use it at all; they have no filesystem
on the agent's machine.
*/
async fn preserve_terminal_draft(
    state_dir: &Path,
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    prompt_editor_input: &str,
    generation: &AtomicU64,
    job_generation: u64,
) -> Result<CapturedTerminalDraft, SessionChatSendError> {
    run_terminal_draft_capture(
        state_dir,
        project_id,
        session_id,
        zmx_name,
        prompt_editor_input,
        Some((generation, job_generation)),
    )
    .await
    .map_err(|message| {
        /*
        CDXC:SessionChatTerminalNotices 2026-08-19:
        The generation, not the message text, is what says whether this send was
        superseded while the handshake ran. Anything else here is the CLI in the
        pane failing to answer its prompt-editor invocation at all, which is
        exactly the evidence the terminal-notice escalation acts on.
        */
        if job_generation != generation.load(Ordering::SeqCst) {
            SessionChatSendError::not_attempted(message)
        } else {
            SessionChatSendError::new(SessionChatSendFailure::PreserveTerminalDraft, message)
        }
    })
}

/*
CDXC:SessionChatSerializedWriters 2026-08-24:
Standalone terminal-draft capture for the `/api/handoffSessionChatDraft`
endpoint, as a QUEUED job.

This used to run the handshake directly, on the reasoning that the stash
marker's `create_new` open already excludes a concurrent chat send's preserve
step and that a view switch never races a send from the same client. Both
halves were wrong. The marker only stops two CAPTURES from overlapping; it says
nothing about the rest of a send sequence, so the handoff's editor invocation,
which makes the CLI stash and CLEAR the composer, could land between another
sequence's paste and its Enter and turn that send into an empty-line submit.
And the desktop fires this on every terminal→chat view switch, which is
emphatically not serialized against the sends that same client just made.

Riding the queue cannot self-deadlock: the only caller is the HTTP handler, and
the in-worker draft capture (`SessionChatSendStep::PreserveTerminalDraft`) calls
`run_terminal_draft_capture` directly rather than coming back through here, so
nothing ever enqueues onto the queue it is currently draining.
*/
pub async fn capture_session_chat_terminal_draft(
    state_dir: &Path,
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    agent: Option<&str>,
) -> Result<CapturedTerminalDraft, String> {
    let (completion_tx, completion_rx) = oneshot::channel();
    let (draft_tx, draft_rx) = oneshot::channel();
    queue_session_chat_send(
        project_id,
        session_id,
        zmx_name,
        "session-chat-draft-handoff",
        vec![SessionChatSendStep::PreserveTerminalDraft {
            state_dir: state_dir.to_path_buf(),
            prompt_editor_input: if agent == Some("grok") {
                SESSION_CHAT_GROK_PROMPT_EDITOR_INPUT
            } else {
                SESSION_CHAT_PROMPT_EDITOR_INPUT
            }
            .to_string(),
        }],
        Some(completion_tx),
        Some(draft_tx),
    )?;
    completion_rx
        .await
        .map_err(|_| {
            "The session chat send worker stopped before preserving the draft.".to_string()
        })?
        .map_err(|error| error.message)?;
    draft_rx
        .await
        .map_err(|_| "The terminal draft capture reported no result.".to_string())
}

async fn run_terminal_draft_capture(
    state_dir: &Path,
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    prompt_editor_input: &str,
    cancellation: Option<(&AtomicU64, u64)>,
) -> Result<CapturedTerminalDraft, String> {
    let request_id = format!(
        "chat-{}-{}",
        std::process::id(),
        SESSION_CHAT_DRAFT_PRESERVE_SEQUENCE.fetch_add(1, Ordering::Relaxed),
    );
    let marker_path = prompt_stash_request_path(state_dir, project_id, session_id);
    let response_path = prompt_handoff_response_path(state_dir, &request_id);
    let Some(marker_parent) = marker_path.parent() else {
        return Err("The terminal draft stash path is unavailable.".to_string());
    };
    fs::create_dir_all(marker_parent)
        .map_err(|_| "The terminal draft stash path could not be created.".to_string())?;
    if let Some(response_parent) = response_path.parent() {
        fs::create_dir_all(response_parent)
            .map_err(|_| "The terminal draft response path could not be created.".to_string())?;
    }
    let _ = fs::remove_file(&response_path);
    let stale_marker = fs::metadata(&marker_path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age > PROMPT_STASH_REQUEST_FRESHNESS);
    if stale_marker {
        let _ = fs::remove_file(&marker_path);
    }
    let mut marker = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&marker_path)
        .map_err(|_| "The terminal draft is already being moved or stashed.".to_string())?;
    if marker
        .write_all(format!("handoff:{request_id}\n").as_bytes())
        .is_err()
    {
        let _ = fs::remove_file(&marker_path);
        return Err("The terminal draft stash request could not be written.".to_string());
    }

    crate::zmx::log_temporary_zmx_input_write(
        project_id,
        session_id,
        zmx_name,
        "sessionChatPreserveTerminalDraft",
        "session-chat-preserve-draft",
        prompt_editor_input,
    );
    let zmx_name_owned = zmx_name.to_string();
    let prompt_editor_input = prompt_editor_input.to_string();
    let delivered = tokio::task::spawn_blocking(move || {
        crate::zmx::session_chat_zmx_write(&zmx_name_owned, &prompt_editor_input)
    })
    .await;
    if !matches!(delivered, Ok(Ok(_))) {
        let _ = fs::remove_file(&marker_path);
        return Err("The terminal could not start preserving its current draft.".to_string());
    }

    let started = std::time::Instant::now();
    loop {
        let cancelled = cancellation.is_some_and(|(generation, job_generation)| {
            job_generation != generation.load(Ordering::SeqCst)
        });
        if cancelled {
            let _ = fs::remove_file(&marker_path);
            let _ = fs::remove_file(&response_path);
            return Err(SESSION_CHAT_SEND_CANCELLED.to_string());
        }
        if let Ok(text) = fs::read_to_string(&response_path) {
            if let Ok(response) = serde_json::from_str::<serde_json::Value>(&text) {
                let _ = fs::remove_file(&response_path);
                if response.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
                    return Err("The terminal draft could not be saved.".to_string());
                }
                // An empty composer is a successful capture of nothing: the
                // CLI answers `empty` without touching Saved Prompts.
                if response.get("empty").and_then(serde_json::Value::as_bool) == Some(true) {
                    return Ok(CapturedTerminalDraft::default());
                }
                return Ok(CapturedTerminalDraft {
                    created: response
                        .get("created")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                    prompt_id: response
                        .get("promptId")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                });
            }
        }
        if started.elapsed() >= SESSION_CHAT_DRAFT_PRESERVE_TIMEOUT {
            let _ = fs::remove_file(&marker_path);
            let _ = fs::remove_file(&response_path);
            return Err("The terminal did not finish preserving its current draft.".to_string());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

// ---------------------------------------------------------------------------
// Inline tests: byte builders and keystroke builders are pure and locked here
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_chat::SessionChatQuestionOption;

    fn question(text: &str, multi_select: bool, options: &[&str]) -> SessionChatQuestion {
        SessionChatQuestion {
            question: text.to_string(),
            header: None,
            multi_select,
            allow_custom: None,
            tool_name: None,
            recommended: None,
            options: options
                .iter()
                .map(|label| SessionChatQuestionOption {
                    label: (*label).to_string(),
                    description: None,
                })
                .collect(),
        }
    }

    fn selection(indices: &[usize], other: Option<&str>) -> SessionChatQuestionSelection {
        SessionChatQuestionSelection {
            indices: indices.to_vec(),
            other: other.map(str::to_string),
        }
    }

    fn raw(value: &str) -> AskAnswerKeyGroup {
        AskAnswerKeyGroup::Raw(value.to_string())
    }

    fn text(value: &str) -> AskAnswerKeyGroup {
        AskAnswerKeyGroup::Text(value.to_string())
    }

    #[test]
    fn clear_burst_follows_the_2n_minus_1_law() {
        // 1 line → 1 repetition.
        assert_eq!(build_agent_tui_clear_input(1), "\u{15}\u{b}");
        // 3 lines → 5 repetitions of each.
        assert_eq!(
            build_agent_tui_clear_input(3),
            format!("{}{}", "\u{15}".repeat(5), "\u{b}".repeat(5))
        );
        // Cap at 40 lines → 79 repetitions; 0 clamps up to 1.
        assert_eq!(
            build_agent_tui_clear_input(1000),
            format!("{}{}", "\u{15}".repeat(79), "\u{b}".repeat(79))
        );
        assert_eq!(build_agent_tui_clear_input(0), "\u{15}\u{b}");
        // For-text adds the 8-line slack: 1 line + 8 → 17 repetitions.
        assert_eq!(
            build_agent_tui_clear_input_for_text("hello"),
            format!("{}{}", "\u{15}".repeat(17), "\u{b}".repeat(17))
        );
        assert_eq!(count_agent_tui_input_lines("a\r\nb\rc\nd"), 4);
        assert_eq!(count_agent_tui_input_lines("plain"), 1);
        assert_eq!(count_agent_tui_input_lines("trailing\n"), 2);
    }

    #[test]
    fn paste_sanitize_and_normalize_match_spec() {
        assert_eq!(
            sanitize_bracketed_paste_text("a\u{1b}[201~b"),
            "a\u{241b}[201~b"
        );
        assert_eq!(
            normalize_terminal_paste_line_endings("a\r\nb\nc"),
            "a\rb\rc"
        );
        // Lone CR is untouched.
        assert_eq!(normalize_terminal_paste_line_endings("a\rb"), "a\rb");
        // Multiline → framed; single line → sanitized unframed.
        assert_eq!(
            build_session_chat_paste_bytes("one\ntwo"),
            "\u{1b}[200~one\rtwo\u{1b}[201~"
        );
        assert_eq!(build_session_chat_paste_bytes("solo"), "solo");
        assert!(is_multiline_draft("text\n"));
        assert!(!is_multiline_draft("text"));
        assert_eq!(
            build_session_chat_image_paste_bytes("/tmp/a.png"),
            "\u{1b}[200~/tmp/a.png\u{1b}[201~"
        );
    }

    #[test]
    fn message_steps_verify_the_paste_before_the_separate_enter() {
        let steps = build_session_chat_message_steps(Some("claude"), "hi", &[]);
        assert_eq!(
            steps,
            vec![
                SessionChatSendStep::WaitForComposer {
                    agent: Some("claude".to_string()),
                    settle_ms: 0,
                    timeout_ms: 6_000,
                },
                SessionChatSendStep::Write(build_agent_tui_clear_input_for_text("hi")),
                SessionChatSendStep::SleepMs(150),
                SessionChatSendStep::Write("hi".to_string()),
                SessionChatSendStep::VerifyPasteLanded {
                    text: "hi".to_string(),
                    settle_ms: 500,
                    timeout_ms: 2_000,
                },
                SessionChatSendStep::Write("\r".to_string()),
            ]
        );
        let with_images = build_session_chat_message_steps(
            Some("claude"),
            "what is this",
            &["/tmp/ghostex-paste-1.png".to_string()],
        );
        assert_eq!(
            with_images,
            vec![
                SessionChatSendStep::WaitForComposer {
                    agent: Some("claude".to_string()),
                    settle_ms: 0,
                    timeout_ms: 6_000,
                },
                SessionChatSendStep::Write(build_agent_tui_clear_input_for_text("what is this")),
                SessionChatSendStep::SleepMs(150),
                SessionChatSendStep::Write(
                    "\u{1b}[200~/tmp/ghostex-paste-1.png\u{1b}[201~".to_string()
                ),
                SessionChatSendStep::SleepMs(300),
                SessionChatSendStep::Write("what is this".to_string()),
                SessionChatSendStep::VerifyPasteLanded {
                    text: "what is this".to_string(),
                    settle_ms: 500,
                    timeout_ms: 2_000,
                },
                SessionChatSendStep::Write("\r".to_string()),
            ]
        );
        // Images without text: no body write, nothing on screen to verify, so
        // the Enter keeps the original blind settle.
        let images_only =
            build_session_chat_message_steps(Some("claude"), "", &["/tmp/a.png".to_string()]);
        assert_eq!(
            images_only,
            vec![
                SessionChatSendStep::WaitForComposer {
                    agent: Some("claude".to_string()),
                    settle_ms: 0,
                    timeout_ms: 6_000,
                },
                SessionChatSendStep::Write(build_agent_tui_clear_input_for_text("")),
                SessionChatSendStep::SleepMs(150),
                SessionChatSendStep::Write("\u{1b}[200~/tmp/a.png\u{1b}[201~".to_string()),
                SessionChatSendStep::SleepMs(500),
                SessionChatSendStep::Write("\r".to_string()),
            ]
        );
        // The window scales with the payload and is capped.
        assert_eq!(session_chat_verify_timeout_ms(0), 2_000);
        assert_eq!(session_chat_verify_timeout_ms(4_600), 2_800);
        assert_eq!(session_chat_verify_timeout_ms(1_000_000), 8_000);
    }

    #[test]
    fn paste_needles_survive_composer_rewrapping() {
        // Both ends are sampled, whitespace and box drawing are dropped.
        let needles = session_chat_paste_needles("first line here\n\nmiddle\nlast line here");
        assert_eq!(needles, vec!["firstlinehere", "lastlinehere"]);
        // One logical line yields one needle, capped at 40 characters.
        let long = "a".repeat(80);
        assert_eq!(session_chat_paste_needles(&long), vec!["a".repeat(40)]);
        // A composer that framed, indented and wrapped the text still matches.
        let screen = "╭──────────────╮\n│ > first line │\n│   here       │\n╰──────────────╯";
        let normalized = normalize_session_chat_screen_text(screen);
        assert!(normalized.contains("firstlinehere"));
    }

    #[test]
    fn claude_single_question_single_select_commits_by_digit() {
        let questions = vec![question("Pick one", false, &["A", "B", "C"])];
        let selections = vec![selection(&[1], None)];
        // Digit selects AND commits; single single-select never ends on the
        // Submit tab, so no trailing Enter.
        assert_eq!(
            build_claude_ask_answer_keys(&questions, &selections),
            vec![raw("2")]
        );
    }

    #[test]
    fn claude_free_text_routes_through_type_something() {
        let questions = vec![question("Pick", false, &["A", "B"])];
        let selections = vec![selection(&[0], Some("also this"))];
        // "Type something" is row options.len()+1 = 3; label + other joined.
        assert_eq!(
            build_claude_ask_answer_keys(&questions, &selections),
            vec![raw("3"), text("A, also this"), raw("\r")]
        );
    }

    #[test]
    fn claude_multi_select_toggles_then_advances_then_submits() {
        let questions = vec![question("Pick many", true, &["A", "B", "C"])];
        let selections = vec![selection(&[0, 2], None)];
        // Toggle 1 and 3, step to Submit tab, then the final confirmation
        // (single multiSelect question ends on the Submit tab).
        assert_eq!(
            build_claude_ask_answer_keys(&questions, &selections),
            vec![raw("1"), raw("3"), raw("\u{1b}[C"), raw("\r")]
        );
    }

    #[test]
    fn claude_multi_question_steps_past_unanswered_and_confirms() {
        let questions = vec![
            question("First", false, &["A", "B"]),
            question("Second", false, &["X", "Y"]),
        ];
        let selections = vec![
            selection(&[0], None),
            SessionChatQuestionSelection::default(),
        ];
        // Q1 digit auto-advances; Q2 unanswered → Right past it; multi-question
        // ends on the Submit tab → final Enter.
        assert_eq!(
            build_claude_ask_answer_keys(&questions, &selections),
            vec![raw("1"), raw("\u{1b}[C"), raw("\r")]
        );
    }

    #[test]
    fn codex_digit_commits_and_notes_use_shortest_arrow_path() {
        let questions = vec![question("Pick", false, &["A", "B", "C"])];
        assert_eq!(
            build_codex_ask_answer_keys(&questions, &[selection(&[2], None)]),
            vec![raw("3")]
        );
        // Note without selection targets the notes row (index 3 of 4 rows):
        // previous_steps = 4-3 = 1 < next_steps = 3 → one Up, Tab, note, Enter.
        assert_eq!(
            build_codex_ask_answer_keys(&questions, &[selection(&[], Some("my note"))]),
            vec![raw("\u{1b}[A"), raw("\t"), text("my note"), raw("\r")]
        );
        // Note attached to row 1 (index 0): zero arrows (already highlighted).
        assert_eq!(
            build_codex_ask_answer_keys(&questions, &[selection(&[0], Some("why"))]),
            vec![raw("\t"), text("why"), raw("\r")]
        );
    }

    #[test]
    fn codex_unanswered_rows_are_skipped_and_confirmed() {
        let questions = vec![
            question("First", false, &["A", "B"]),
            question("Second", false, &["X", "Y"]),
        ];
        let selections = vec![
            SessionChatQuestionSelection::default(),
            selection(&[1], None),
        ];
        // Q1 unanswered → DEL + Right (not last); Q2 digit commits; unanswered
        // remains → trailing Enter for the confirmation dialog.
        assert_eq!(
            build_codex_ask_answer_keys(&questions, &selections),
            vec![raw("\u{7f}"), raw("\u{1b}[C"), raw("2"), raw("\r")]
        );
        // Unanswered LAST question ends its row with Enter instead of Right.
        let tail_unanswered = vec![
            selection(&[0], None),
            SessionChatQuestionSelection::default(),
        ];
        assert_eq!(
            build_codex_ask_answer_keys(&questions, &tail_unanswered),
            vec![raw("1"), raw("\u{7f}"), raw("\r"), raw("\r")]
        );
    }

    #[test]
    fn format_ask_answer_keeps_one_line_per_question() {
        let questions = vec![
            question("First", false, &["A", "B"]),
            question("Second", false, &["X", "Y"]),
            question("Third", false, &["M"]),
        ];
        let selections = vec![
            selection(&[1], None),
            SessionChatQuestionSelection::default(),
            selection(&[0], Some("extra")),
        ];
        assert_eq!(format_ask_answer(&questions, &selections), "B\n\nM, extra");
        assert!(has_ask_answer(&selections));
        assert!(!has_ask_answer(&[SessionChatQuestionSelection::default()]));
    }

    #[test]
    fn key_steps_are_a_single_verbatim_write() {
        assert_eq!(
            build_session_chat_key_steps("shift-tab"),
            Some(vec![SessionChatSendStep::Write("\u{1b}[9;2u".to_string())])
        );
        // No bracketed paste framing, no trailing Enter, no clear burst.
        assert_eq!(build_session_chat_key_steps("shift-tab").unwrap().len(), 1);
        assert_eq!(
            build_session_chat_key_steps("shift-up"),
            Some(vec![SessionChatSendStep::Write("\u{1b}[1;2A".to_string())])
        );
        assert_eq!(
            build_session_chat_key_steps("shift-down"),
            Some(vec![SessionChatSendStep::Write("\u{1b}[1;2B".to_string())])
        );
        assert_eq!(build_session_chat_key_steps("tab"), None);
        assert_eq!(build_session_chat_key_steps(""), None);
    }

    #[test]
    fn ask_answer_steps_space_groups_one_second_apart() {
        let steps = build_ask_answer_steps(&[raw("1"), text("note\nline"), raw("\r")]);
        assert_eq!(
            steps,
            vec![
                SessionChatSendStep::Write("1".to_string()),
                SessionChatSendStep::SleepMs(1_000),
                SessionChatSendStep::Write("\u{1b}[200~note\rline\u{1b}[201~".to_string()),
                SessionChatSendStep::SleepMs(1_000),
                SessionChatSendStep::Write("\r".to_string()),
            ]
        );
        assert!(build_ask_answer_steps(&[]).is_empty());
    }
}

/*
CDXC:SessionChatSend 2026-07-31:
Send-side endpoints. Every write goes through the per-session async send
queue in session_chat_send.rs (upstream chat spec §7 pacing: clear burst → bracketed-paste
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

pub(crate) async fn handle_send_session_chat_message_http(
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
        let Some(steps) = crate::session_chat_send::build_session_chat_key_steps(key) else {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "invalidParams",
                    message: format!("sendSessionChatMessage does not know the key \"{key}\"."),
                },
            );
        };
        crate::session_chat_send::enqueue_session_chat_send(
            &target.project_id,
            &target.session_id,
            &target.zmx_name,
            "session-chat-key",
            steps,
        );
        /*
        Raw option keys repaint the footer just like `/model` and `/effort`:
        Shift+Tab changes Claude's permission mode, while shifted arrows change
        Codex effort. Re-read at the established +2s/+6s cadence so the pill
        confirms the value promptly instead of waiting for the idle probe.
        */
        let agent = session_chat_agent_for_session(&target.session);
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
                message: format!(
                    "sendSessionChatMessage text exceeds the {}-byte zmx send limit.",
                    crate::zmx::GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES
                ),
            },
        );
    }
    match send_session_chat_message_internal(
        state,
        &target.project_id,
        &target.session_id,
        &text,
        &image_paths,
        "chat",
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
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

/*
CDXC:SessionChatCore 2026-08-01:
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
    let steps = match kind {
        "approval" => {
            // Allow → the option's raw send byte ("1"); Deny/empty → ESC.
            // Raw, no bracketed paste, no delayed Enter (upstream chat spec §8.3).
            let approval_send = params
                .get("approvalSend")
                .and_then(Value::as_str)
                .unwrap_or_default();
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
            let stored_prompt = crate::agents::session_chat_prompt_setting(&target.session)
                .as_deref()
                .and_then(crate::session_chat::parse_stored_session_chat_prompt)
                // A card the transcript produced (hooks that never forwarded
                // toolInput) must be answerable too, or the user gets a card
                // that rejects every answer.
                .or_else(|| transcript_pending_question_prompt(&target.session));
            let Some(crate::session_chat::SessionChatInteractivePrompt::Question { questions }) =
                stored_prompt
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
            let agent = session_chat_agent_for_session(&target.session);
            // One agent can host different asking tools with different
            // terminal UIs (omp ships its own `ask` dialog next to the pi
            // cursor bridge), so the key plan follows the tool that asked.
            let question_tool = questions
                .first()
                .and_then(|question| question.tool_name.as_deref())
                .map(crate::session_chat::normalize_session_chat_tool_name);
            match agent.as_deref() {
                Some("claude" | "openclaude") => crate::session_chat_send::build_ask_answer_steps(
                    &crate::session_chat_send::build_claude_ask_answer_keys(
                        &questions,
                        &selections,
                    ),
                ),
                Some("codex") => crate::session_chat_send::build_ask_answer_steps(
                    &crate::session_chat_send::build_codex_ask_answer_keys(&questions, &selections),
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
                    Non-stepping agents (Grok): the formatted answer text goes
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
                        )
                    }
                }
            }
        }
        /*
        CDXC:SessionChatTerminalPicker 2026-08-21:
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
            let picker = crate::session_chat_send::capture_session_terminal_text(&target.zmx_name)
                .await
                .as_deref()
                .and_then(crate::session_chat_resume_prompt::detect_session_chat_terminal_picker);
            let Some(picker) = picker else {
                return domain_error_response(
                    endpoint_path,
                    request_id,
                    DomainStateError {
                        code: "invalidState",
                        message: "That picker is no longer on the session's screen.".to_string(),
                    },
                );
            };
            let Some(answer_key) = picker.answer_key(choice_index) else {
                return domain_error_response(
                    endpoint_path,
                    request_id,
                    DomainStateError {
                        code: "invalidState",
                        message: "The picker on screen no longer offers that option.".to_string(),
                    },
                );
            };
            crate::session_chat_send::build_terminal_picker_answer_steps(&answer_key)
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
    CDXC:SessionChatTerminalPicker 2026-08-21:
    The card the user just answered is a TERMINAL NOTICE, and notices only
    retire when a fresh capture proves the screen is clean. Left to the
    follower's ~30s probe the answered picker would stay on screen in chat for
    half a minute, so borrow the post-dispatch redetect: it re-reads the screen
    at +2s and +6s and republishes, which is exactly the window the picker
    needs to tear down.
    */
    if kind == "terminalChoice" {
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

/*
CDXC:SessionChatDraftHandoff 2026-08-18:
Terminal → chat draft transfer for every host. A user who typed into the agent
CLI and then lands on the chat surface, by tapping the toggle or because the app
auto-switched a terminal-started agent into Chat, must find that text in the
chat composer instead of stranded behind the parked terminal. The capture uses
the agent's prompt-editor handshake, which parks the draft in Saved Prompts;
this reads that row back and (when this capture created it) removes it again, so
a transfer leaves no residue in the user's Saved Prompts list.

Grok Build binds Ctrl+G to its Tasks pane, so its capture opens the editor from
the command palette with Ctrl+P, `editor`, Enter instead.
*/

/*
CDXC:SessionChatDraftHandoff 2026-08-30:
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
fn session_chat_agent_command_is_user_hop(session: &Value) -> bool {
    let launch_settings = session.get("launchSettings").and_then(Value::as_object);
    let command = launch_settings
        .and_then(|settings| settings.get("agentCommand"))
        .and_then(Value::as_str)
        .filter(|command| !command.trim().is_empty())
        .or_else(|| {
            launch_settings
                .and_then(|settings| settings.get("agentLaunchPlan"))
                .and_then(Value::as_object)
                .and_then(|plan| plan.get("command"))
                .and_then(Value::as_str)
        })
        .unwrap_or_default();
    let Some(first_word) = command.split_whitespace().next() else {
        return false;
    };
    let program = first_word.rsplit('/').next().unwrap_or(first_word);
    matches!(program, "ssh" | "autossh" | "mosh" | "et")
}

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
    if session_chat_agent_command_is_user_hop(&target.session) {
        return routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, json!({ "content": "", "transferred": false })),
        );
    }
    let agent = session_chat_agent_for_session(&target.session);
    let captured = match crate::session_chat_send::capture_session_chat_terminal_draft(
        &state.paths.app_state_dir,
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        agent.as_deref(),
    )
    .await
    {
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
    let content = match read_and_release_stashed_prompt(
        state,
        &target.project_id,
        &prompt_id,
        captured.created,
    ) {
        Ok(content) => content,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(
            request_id,
            json!({ "content": content, "transferred": !content.is_empty() }),
        ),
    )
}

/// Reads back the Saved Prompt the draft capture just wrote and, when the
/// capture created that row, deletes it — the text is moving to a composer, not
/// into the user's stash. An update of a pre-existing row is left alone.
pub(crate) fn read_and_release_stashed_prompt(
    state: &AppState,
    project_id: &str,
    prompt_id: &str,
    created: bool,
) -> std::result::Result<String, DomainStateError> {
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
    if created {
        let mut delete_params = Map::new();
        delete_params.insert("promptId".to_string(), json!(prompt_id));
        let _ = repository.delete_stashed_prompt(&delete_params);
    }
    Ok(content)
}

pub(crate) fn handle_interrupt_session_chat_http(
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
    // Cancel first so queued sends (and an in-flight sequence's remaining
    // steps) drop, then deliver ESC through the queue's new generation.
    crate::session_chat_send::cancel_session_chat_sends(&target.project_id, &target.session_id);
    crate::session_chat_send::enqueue_session_chat_send(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        "session-chat-interrupt",
        vec![crate::session_chat_send::SessionChatSendStep::Write(
            crate::session_chat_send::SESSION_CHAT_INTERRUPT.to_string(),
        )],
    );
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(request_id, json!({ "interrupted": true })),
    )
}
