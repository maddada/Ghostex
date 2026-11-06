//! Claude Code's and Codex's `/resume` conversation pickers, read row by row so the chat can
//! mirror them, and the keys that drive them; plus the keys the chat passes through to the
//! blocking screens of every other agent.

/*
CDXC:SessionChat 2026-10-11 DECISION:
User: make `/resume` usable from the chat view. For Claude Code and Codex the chat MIRRORS the agent's own picker: its rows, the highlighted row, the search text and the page, with the chat taking the keyboard (Up/Down move, Enter picks, Esc cancels, typing filters) and every key sent to the real picker, so the card follows the agent's screen and never drifts from it; on the phone a tapped row is picked. For every other agent, and for Codex's own screens its dialog reader cannot read, the chat PASSES THROUGH: it shows that part of the agent's screen live and, in the user's words, "it should take over stuff like letter/number presses and up/down/left/right and escape presses in the chat itself so i need to get rid of it before i can use those keys in the chat composer": letters, digits, the four arrows, Enter and Esc go to the agent even when the composer holds text, until the agent closes the screen or the user gives the keyboard back from the card. This supersedes the Up/Down/Enter/Esc-only pass-through of the same day.
SEE-ALSO: packages/gx-chat-core/src/questions/terminal_picker.rs, apps/desktop/src/app/native_chat/terminal_picker.rs, apps/mobile/app/src/chat/native/cards/TerminalPickerCard.tsx.
*/

use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::session_chat_send::{
    capture_session_terminal_text, execute_session_chat_send, SessionChatSendStep,
    SessionChatSendTarget, SESSION_CHAT_INTERRUPT,
};
use crate::session_chat_terminal_dialog::TerminalDialog;

const CLAUDE_RESUME_TITLE: &str = "Resume session";
const CODEX_RESUME_TITLES: [&str; 2] = ["Resume a previous session", "Fork a previous session"];

/// One conversation in the picker.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumePickerRow {
    /// The conversation's name or first prompt.
    pub title: String,
    /// What the agent prints beside it: when it was last used, and Claude's branch and size.
    pub detail: Option<String>,
    /// The project heading Claude groups rows under.
    pub group: Option<String>,
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumePicker {
    /// `claude` or `codex`: which key grammar drives it.
    pub agent: &'static str,
    pub rows: Vec<ResumePickerRow>,
    /// The search text the agent shows, empty when it shows its placeholder.
    pub query: String,
    /// Claude's search box has the keyboard and no row is highlighted; Down moves into the list.
    pub search_focused: bool,
    /// Codex's "2 of 4": where the highlight is in the whole list.
    pub position: Option<String>,
    /// What the agent says in place of rows ("No sessions found").
    pub message: Option<String>,
}

impl ResumePicker {
    fn selected(&self) -> Option<usize> {
        self.rows.iter().position(|row| row.selected)
    }
}

fn indent(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ').count()
}

/// Claude's second row line: "4 seconds ago · master · 231.2KB".
fn is_claude_detail(line: &str) -> bool {
    let first = line.split(" · ").next().unwrap_or_default().trim();
    let size = line.rsplit(" · ").next().unwrap_or_default().trim();
    first.ends_with(" ago")
        || matches!(first, "just now" | "yesterday")
        || (line.contains(" · ")
            && ["B", "KB", "MB", "GB"].iter().any(|unit| {
                size.strip_suffix(unit)
                    .is_some_and(|number| number.trim().parse::<f64>().is_ok())
            }))
}

/// Claude Code 2.1's `/resume` panel, below its "Resume session" heading:
///
/// ```text
///    ╭──────────────╮
///    │ ⌕ Search…    │
///    ╰──────────────╯
///      gx-resume-test
///
///    ❯ Echo reply test
///      4 seconds ago · master · 231.2KB
///
///      Delta resume picker test
///      24 seconds ago · master · 229.9KB
///
///      Ctrl+A to show all projects · … · Type to search · Esc to cancel
/// ```
///
/// Measured 2026-10-11 against 2.1.296: once a letter is typed the search box has the keyboard,
/// no row carries `❯`, the project heading goes away, and Down moves into the list; Up from the
/// first row goes back to the box. Backspace edits the search only while the box has the keyboard.
pub(crate) fn claude_resume_picker(title: &str, remainder: &[String]) -> Option<ResumePicker> {
    if title.trim() != CLAUDE_RESUME_TITLE {
        return None;
    }
    let search = remainder.iter().position(|line| line.contains('⌕'))?;
    let query = remainder[search]
        .trim()
        .trim_matches('│')
        .trim()
        .trim_start_matches('⌕')
        .trim();
    let query = if query == "Search…" { "" } else { query };
    let footer = remainder
        .iter()
        .position(|line| line.to_ascii_lowercase().contains("esc to "))
        .unwrap_or(remainder.len());
    let list: Vec<&str> = remainder[search + 1..footer.max(search + 1)]
        .iter()
        .map(String::as_str)
        .filter(|line| !line.trim().starts_with('╰') && !line.trim().starts_with('╭'))
        .collect();
    let highlighted = list
        .iter()
        .position(|line| line.trim_start().starts_with('❯'));
    let label_column = match highlighted {
        Some(index) => indent(list[index]) + 2,
        None => list
            .iter()
            .filter(|line| !line.trim().is_empty())
            .map(|line| indent(line))
            .min()
            .unwrap_or(0),
    };
    let lines: Vec<(usize, &str)> = list
        .iter()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| (index, *line))
        .collect();
    let mut rows = Vec::new();
    let mut group = None;
    let mut message = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        let (index, line) = lines[at];
        let text = line.trim().trim_start_matches('❯').trim();
        // Scroll markers are the agent's own paging; the mirror shows the rows it can see.
        if text.starts_with(['↑', '↓']) {
            at += 1;
            continue;
        }
        let detail = lines
            .get(at + 1)
            .map(|(_, next)| next.trim())
            .filter(|next| is_claude_detail(next));
        let selected = Some(index) == highlighted;
        if let Some(detail) = detail {
            rows.push(ResumePickerRow {
                title: text.to_string(),
                detail: Some(detail.to_string()),
                group: group.clone(),
                selected,
            });
            at += 2;
            continue;
        }
        if selected {
            rows.push(ResumePickerRow {
                title: text.to_string(),
                detail: None,
                group: group.clone(),
                selected,
            });
        } else if indent(line) > label_column && !rows.is_empty() {
            // A title wrapped past its column.
            if let Some(row) = rows.last_mut() {
                row.title.push(' ');
                row.title.push_str(text);
            }
        } else if lines
            .get(at + 1)
            .is_some_and(|(_, next)| !next.trim().is_empty())
        {
            group = Some(text.to_string());
        } else {
            message.push(text.to_string());
        }
        at += 1;
    }
    Some(ResumePicker {
        agent: "claude",
        search_focused: highlighted.is_none(),
        rows,
        query: query.to_string(),
        position: None,
        message: (!message.is_empty()).then(|| message.join(" ")),
    })
}

/// Codex's `/resume` list below its heading (measured 2026-10-11 on codex 0.16x):
///
/// ```text
///  Filter:  Cwd  All    Status:  Active  Archived    Sort:  Updated  Created
///
///  Type to search                 (or "Search: ind")
///
///   › 32s ago     Use India resume picker test
///     56s ago     Test resume picker
/// ───────────────────────────────────────── 1 / 4 · 100% ─
/// ```
///
/// Typing edits the search at any time, Backspace deletes, and Esc clears a search before it
/// closes the list.
pub(crate) fn codex_resume_picker(dialog: &TerminalDialog) -> Option<ResumePicker> {
    if !CODEX_RESUME_TITLES.contains(&dialog.title.as_str()) {
        return None;
    }
    let lines: Vec<&str> = dialog.body.lines().collect();
    let search = lines.iter().position(|line| {
        let line = line.trim();
        line == "Type to search" || line.starts_with("Search:")
    })?;
    let query = lines[search]
        .trim()
        .strip_prefix("Search:")
        .map(str::trim)
        .unwrap_or_default();
    let mut rows = Vec::new();
    let mut message = Vec::new();
    let mut position = None;
    for line in &lines[search + 1..] {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('─') {
            let page = trimmed.trim_matches(['─', ' ']);
            position = page
                .split(" · ")
                .next()
                .and_then(|count| count.split_once(" / "))
                .filter(|(at, of)| {
                    at.trim().parse::<u32>().is_ok() && of.trim().parse::<u32>().is_ok()
                })
                .map(|(at, of)| format!("{} of {}", at.trim(), of.trim()));
            break;
        }
        let selected = trimmed.starts_with('›');
        let text = trimmed.trim_start_matches('›').trim_start();
        let columns: Vec<&str> = text
            .split("  ")
            .map(str::trim)
            .filter(|column| !column.is_empty())
            .collect();
        match columns.as_slice() {
            [] => {}
            [only] => {
                if selected {
                    rows.push(ResumePickerRow {
                        title: only.to_string(),
                        detail: None,
                        group: None,
                        selected,
                    });
                } else {
                    message.push(only.to_string());
                }
            }
            [details @ .., title] => rows.push(ResumePickerRow {
                title: title.to_string(),
                detail: Some(details.join(" · ")),
                group: None,
                selected,
            }),
        }
    }
    Some(ResumePicker {
        agent: "codex",
        rows,
        query: query.to_string(),
        search_focused: false,
        position,
        message: (!message.is_empty()).then(|| message.join(" ")),
    })
}

/// The resume picker the agent shows right now, read off its screen.
fn detect_resume_picker(agent: &str, screen: &str) -> Option<ResumePicker> {
    match agent {
        "claude" => crate::session_chat_claude_dialog::detect_claude_dialog(screen)?.picker,
        "codex" => crate::session_chat_codex_dialog::detect_codex_dialog(screen)?.picker,
        _ => None,
    }
}

/// What a `VerifyTerminalPicker` step checks at the front of the send queue: the agent still
/// shows the screen its keys are meant for.
pub(crate) fn screen_takes_picker_keys(agent: &str, screen: &str) -> bool {
    detect_resume_picker(agent, screen).is_some()
        || crate::session_chat_notice::classify_session_chat_terminal_notice(Some(agent), screen)
            .is_some_and(|notice| notice.passes_keys_through())
}

fn closed() -> DomainStateError {
    DomainStateError {
        code: "invalidState",
        message: "The conversation list is no longer open.".to_string(),
    }
}

fn invalid() -> DomainStateError {
    DomainStateError {
        code: "invalidParams",
        message: "That key is not offered by this list.".to_string(),
    }
}

const UP: &str = "\x1b[1;1A";
const DOWN: &str = "\x1b[1;1B";
const BACKSPACE: &str = "\x7f";
/// Ink and Codex read each key as its own input event only when it arrives in its own write.
const KEY_GAP_MS: u64 = 40;

fn key_bytes(key: &str) -> Option<&'static str> {
    Some(match key {
        "up" => UP,
        "down" => DOWN,
        "enter" => "\r",
        "escape" => SESSION_CHAT_INTERRUPT,
        "pageUp" => "\x1b[5~",
        "pageDown" => "\x1b[6~",
        _ => return None,
    })
}

fn push_keys(steps: &mut Vec<SessionChatSendStep>, key: &str, count: usize) {
    for _ in 0..count {
        steps.push(SessionChatSendStep::Write(key.to_string()));
        steps.push(SessionChatSendStep::SleepMs(KEY_GAP_MS));
    }
}

/// The keys that walk the highlight onto `index` and pick it. From Claude's search box the first
/// Down lands on the first row.
fn pick_row_steps(picker: &ResumePicker, index: usize) -> Vec<SessionChatSendStep> {
    let mut steps = Vec::new();
    match picker.selected() {
        Some(selected) if index >= selected => push_keys(&mut steps, DOWN, index - selected),
        Some(selected) => push_keys(&mut steps, UP, selected - index),
        None => push_keys(&mut steps, DOWN, index + 1),
    }
    steps.push(SessionChatSendStep::Write("\r".to_string()));
    steps
}

/// Typing that turns the agent's search text into `wanted`: Backspace back to the shared prefix,
/// then the rest one character per write.
fn query_steps(current: &str, wanted: &str) -> Vec<SessionChatSendStep> {
    let shared = current
        .chars()
        .zip(wanted.chars())
        .take_while(|(a, b)| a == b)
        .count();
    let mut steps = Vec::new();
    push_keys(&mut steps, BACKSPACE, current.chars().count() - shared);
    for c in wanted.chars().skip(shared) {
        steps.push(SessionChatSendStep::Write(c.to_string()));
        steps.push(SessionChatSendStep::SleepMs(KEY_GAP_MS));
    }
    steps
}

async fn run(
    target: &SessionChatSendTarget,
    agent: &str,
    steps: Vec<SessionChatSendStep>,
) -> Result<(), DomainStateError> {
    let mut all = vec![SessionChatSendStep::VerifyTerminalPicker {
        agent: agent.to_string(),
    }];
    all.extend(steps);
    execute_session_chat_send(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        "session-chat-picker",
        all,
    )
    .await
    .map_err(|error| DomainStateError {
        code: "invalidState",
        message: error.message,
    })
}

async fn current_picker(
    target: &SessionChatSendTarget,
    agent: &str,
) -> Result<ResumePicker, DomainStateError> {
    capture_session_terminal_text(&target.zmx_name)
        .await
        .and_then(|screen| detect_resume_picker(agent, &screen))
        .ok_or_else(closed)
}

/// Whether an answer drives the resume picker rather than a dialog.
pub(crate) fn is_picker_answer(params: &Map<String, Value>) -> bool {
    ["pickerKey", "pickerRow", "pickerQuery"]
        .iter()
        .any(|key| params.contains_key(*key))
}

/// Answers `pickerKey` (one key), `pickerRow` (walk to a row and pick it) and `pickerQuery` (make
/// the search text this). The picker is re-read here rather than trusted from the card, because
/// the card is always a capture behind while keys are in flight.
pub(crate) async fn answer_resume_picker(
    target: &SessionChatSendTarget,
    agent: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let picker = current_picker(target, agent).await?;
    if let Some(key) = params.get("pickerKey").and_then(Value::as_str) {
        let bytes = key_bytes(key).ok_or_else(invalid)?;
        run(
            target,
            agent,
            vec![SessionChatSendStep::Write(bytes.to_string())],
        )
        .await?;
    } else if let Some(row) = params.get("pickerRow") {
        let index = row
            .get("index")
            .and_then(Value::as_u64)
            .and_then(|index| usize::try_from(index).ok())
            .ok_or_else(invalid)?;
        // The rows move under a filter or a scroll; pick the conversation the user saw.
        let title = row.get("title").and_then(Value::as_str).unwrap_or_default();
        let index = if picker.rows.get(index).is_some_and(|row| row.title == title) {
            index
        } else {
            picker
                .rows
                .iter()
                .position(|row| row.title == title)
                .ok_or_else(closed)?
        };
        run(target, agent, pick_row_steps(&picker, index)).await?;
    } else if let Some(wanted) = params.get("pickerQuery").and_then(Value::as_str) {
        if wanted.len() > 512 || wanted.chars().any(char::is_control) {
            return Err(invalid());
        }
        let mut picker = picker;
        // Claude's list reads Backspace as nothing, Space as "preview", and puts a typed letter at
        // the start of the search (measured 2026-10-11), so the search box has to have the keyboard
        // first. Up from the top row reaches it; a scrolled list needs more than one walk.
        if agent == "claude" && picker.query != wanted {
            for _ in 0..4 {
                let Some(selected) = picker.selected() else {
                    break;
                };
                let mut steps = Vec::new();
                push_keys(&mut steps, UP, selected + 1);
                run(target, agent, steps).await?;
                picker = current_picker(target, agent).await?;
            }
        }
        if picker.query != wanted {
            run(target, agent, query_steps(&picker.query, wanted)).await?;
        }
    } else {
        return Err(invalid());
    }
    Ok(json!({ "queued": true }))
}

/// A key the chat passes through to an agent screen it cannot read (`terminalKey`).
///
/// CDXC:SessionChat 2026-11-03 WHY: every key is checked at the front of the send queue against the screen it is meant for (`VerifyTerminalPicker`), because the screen can go away while a card is still up or a key is on its way: another client or the terminal answered it, the agent timed out or exited. Without the check such a key lands on whatever the terminal shows next, an Enter submitting another prompt's draft or a shell command. A screen that changes because of the card's own keys (OMP's tool approval switching to "Deny and send message" after the first typed letter) is the detector's to keep recognizing (`session_chat_omp_blocking.rs`), not a reason to drop the check.
pub(crate) async fn answer_terminal_key(
    target: &SessionChatSendTarget,
    agent: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    // A letter, digit or other character typed while the card has the keyboard.
    let typed = params.get("text").and_then(Value::as_str).filter(|text| {
        !text.is_empty() && text.chars().count() <= 8 && !text.chars().any(char::is_control)
    });
    let bytes = match params.get("key").and_then(Value::as_str) {
        _ if typed.is_some() => typed.unwrap_or_default(),
        Some("up") => "\x1b[A",
        Some("down") => "\x1b[B",
        Some("left") => "\x1b[D",
        Some("right") => "\x1b[C",
        Some("enter") => "\r",
        Some("escape") => SESSION_CHAT_INTERRUPT,
        Some("tab") => "\t",
        Some("pageUp") => "\x1b[5~",
        Some("pageDown") => "\x1b[6~",
        _ => return Err(invalid()),
    };
    run(
        target,
        agent,
        vec![SessionChatSendStep::Write(bytes.to_string())],
    )
    .await
    .map_err(|_| DomainStateError {
        code: "invalidState",
        message: "The agent's screen changed. Review it and try again.".to_string(),
    })?;
    Ok(json!({ "queued": true }))
}
