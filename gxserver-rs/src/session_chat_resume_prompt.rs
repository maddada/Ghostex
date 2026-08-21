/*
CDXC:SessionChatResumePrompt 2026-08-18 (picker rewrite 2026-08-21):
Claude Code's resume-usage chooser. When a session is resumed after it has
grown old/large, the CLI paints a blocking picker before it will accept any
input:

    This session is 2h 44m old and 258.1k tokens.

    Resuming the full session will consume a substantial portion of your usage
    limits. We recommend resuming from a summary.

    ❯ 1. Resume from summary (recommended)
      2. Resume full session as-is
      3. Don't ask me again

    Enter to confirm · Esc to cancel

Session Chat sends are pty writes, so a chat message typed while that picker
owns the input line is read as picker keystrokes: the body is swallowed and the
trailing Enter confirms whatever row happens to be highlighted — the summary
row, which silently compacts the conversation the user was about to continue.

Which row the user wants is NOT ours to decide (summary vs full session is a
usage-limit trade-off only they can make), so this module no longer answers the
picker. It DESCRIBES it: the prose above the rows, every row in screen order,
and which row the TUI highlights right now. `session_chat_notice.rs` turns that
into the input-blocking `resumePrompt` terminal notice the chat surfaces render
as an answer picker, and `answerSessionChatPrompt`'s `terminalChoice` lane
walks the highlight onto the row the user picked.

Detection lives here (server side) so every chat client — gpui, ghostex-web
and the RN mobile app — inherits it from one implementation.

Matching is deliberately narrow: the picker only counts when a run of
consecutive numbered rows carries BOTH canonical labels, one of those rows is
the highlighted one, and the "Enter to confirm" footer follows the run inside
the scanned tail window. The number of arrow presses is derived from where the
highlight actually sits, never assumed, because the row order and the
"Don't ask me again" row are Claude's to change.
*/

use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};

/// Tail window scanned for the picker. The picker plus its usage paragraph is
/// ~10 lines; 25 leaves headroom for wrapped prose above the rows without
/// reaching back into scrollback that the session already answered.
const SESSION_CHAT_RESUME_PROMPT_SCAN_LINES: usize = 25;

/// The confirm footer has to sit at the very bottom of the capture: Claude
/// paints its statusline and shortcut hint below a live picker, nothing else.
/// Scrollback still holds the rendering of a picker the session already
/// answered, and that one is followed by the resumed conversation.
const SESSION_CHAT_RESUME_PROMPT_FOOTER_TAIL_LINES: usize = 3;

/// Prose lines above the rows that describe the choice ("This session is 2h 44m
/// old…", "Resuming the full session will consume…"). Claude wraps them, so
/// this counts SCREEN lines, not sentences.
const SESSION_CHAT_RESUME_PROMPT_PROSE_LINES: usize = 6;

/// Cap on the joined prose, which becomes the notice's `detail`.
const SESSION_CHAT_RESUME_PROMPT_PROSE_MAX_CHARS: usize = 400;

/// Row-marker glyphs Claude/Codex-style pickers use for the highlighted row.
const SELECTION_MARKERS: &[char] = &['\u{276f}', '\u{203a}', '\u{25b6}', '\u{2192}', '>'];

const RESUME_FULL_SESSION_LABEL: &str = "Resume full session";
const RESUME_FROM_SUMMARY_LABEL: &str = "Resume from summary";
const CONFIRM_FOOTER: &str = "Enter to confirm";

/// Notice kind the picker publishes itself as.
pub const SESSION_CHAT_RESUME_PROMPT_KIND: &str = "resumePrompt";

/// One row of an on-screen picker, in the order it is painted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionChatTerminalPickerRow {
    pub label: String,
    /// True for the row the TUI highlights right now (the one a bare Enter
    /// would confirm).
    pub selected: bool,
}

/// A live, answerable picker: what it asks, what it offers, and where the
/// highlight sits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionChatTerminalPicker {
    /// Prose painted above the rows, joined and capped. `None` when the picker
    /// stands alone.
    pub detail: Option<String>,
    pub rows: Vec<SessionChatTerminalPickerRow>,
    /// Index into `rows` of the highlighted row.
    pub selected_index: usize,
}

impl SessionChatTerminalPicker {
    /// Rows to travel from the highlighted row to `target`: positive means Down
    /// presses, negative means Up, 0 means Enter alone. `None` when `target` is
    /// not a row this picker painted, so an answer can never be guessed at.
    pub fn row_moves_to(&self, target: usize) -> Option<i32> {
        (target < self.rows.len()).then(|| target as i32 - self.selected_index as i32)
    }
}

#[derive(Clone, Debug)]
struct PickerRow {
    /// Index of the row inside the scanned window.
    line: usize,
    label: String,
    selected: bool,
}

/// A picker painted inside a bordered box arrives as `│ ❯ 2. … │`; the border
/// is chrome, not content.
fn strip_box_border(line: &str) -> &str {
    const BORDERS: &[char] = &['\u{2502}', '\u{2503}', '\u{2506}', '\u{250a}', '|'];
    line.trim()
        .trim_start_matches(BORDERS)
        .trim_end_matches(BORDERS)
        .trim()
}

/// `❯ 2. Resume full session as-is` → (selected, label). Rows that are not a
/// numbered picker row return `None`.
fn parse_picker_row(line: &str) -> Option<(bool, String)> {
    let trimmed = strip_box_border(line);
    let mut rest = trimmed;
    let mut selected = false;
    if let Some(first) = rest.chars().next() {
        if SELECTION_MARKERS.contains(&first) {
            selected = true;
            rest = rest[first.len_utf8()..].trim_start();
        }
    }
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    let rest = rest[digits.len()..].strip_prefix('.')?;
    let label = rest.trim();
    if label.is_empty() {
        return None;
    }
    Some((selected, label.to_string()))
}

/// The last `SESSION_CHAT_RESUME_PROMPT_SCAN_LINES` non-blank lines, oldest
/// first, with colours stripped and exotic whitespace folded to spaces (the
/// TUI paints with non-breaking spaces).
fn scan_window(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in text.lines().rev() {
        let line = normalize_spaces(&strip_ansi_sgr(raw))
            .trim_end()
            .to_string();
        if line.trim().is_empty() {
            continue;
        }
        lines.push(line);
        if lines.len() >= SESSION_CHAT_RESUME_PROMPT_SCAN_LINES {
            break;
        }
    }
    lines.reverse();
    lines
}

/// Consecutive numbered rows, grouped into the runs they were painted in.
fn picker_runs(window: &[String]) -> Vec<Vec<PickerRow>> {
    let mut runs: Vec<Vec<PickerRow>> = Vec::new();
    let mut current: Vec<PickerRow> = Vec::new();
    for (line, text) in window.iter().enumerate() {
        match parse_picker_row(text) {
            Some((selected, label)) => {
                if current
                    .last()
                    .is_some_and(|previous: &PickerRow| previous.line + 1 != line)
                {
                    runs.push(std::mem::take(&mut current));
                }
                current.push(PickerRow {
                    line,
                    label,
                    selected,
                });
            }
            None => {
                if !current.is_empty() {
                    runs.push(std::mem::take(&mut current));
                }
            }
        }
    }
    if !current.is_empty() {
        runs.push(current);
    }
    runs
}

/// True for a line that is pure box/rule chrome, which carries no prose.
fn is_chrome_line(line: &str) -> bool {
    let content = strip_box_border(line);
    content.is_empty() || !content.chars().any(char::is_alphanumeric)
}

/// The prose Claude painted directly above the rows, oldest first, joined into
/// one paragraph. Stops at chrome or at another picker row so a previous
/// picker's rows can never be read as this one's description.
fn prose_above(window: &[String], first_row_line: usize) -> Option<String> {
    let mut collected: Vec<&str> = Vec::new();
    for line in window[..first_row_line].iter().rev() {
        if collected.len() >= SESSION_CHAT_RESUME_PROMPT_PROSE_LINES {
            break;
        }
        if is_chrome_line(line) || parse_picker_row(line).is_some() {
            break;
        }
        collected.push(strip_box_border(line));
    }
    collected.reverse();
    let prose = collected.join(" ").trim().to_string();
    if prose.is_empty() {
        return None;
    }
    let capped = if prose.chars().count() > SESSION_CHAT_RESUME_PROMPT_PROSE_MAX_CHARS {
        let head: String = prose
            .chars()
            .take(SESSION_CHAT_RESUME_PROMPT_PROSE_MAX_CHARS)
            .collect();
        format!("{}…", head.trim_end())
    } else {
        prose
    };
    Some(capped)
}

/// `Some` only when the resume-usage picker is live on screen and its highlight
/// is proven, so every row is reachable with a derived key count.
pub fn detect_session_chat_terminal_picker(text: &str) -> Option<SessionChatTerminalPicker> {
    let window = scan_window(text);
    for run in picker_runs(&window).into_iter().rev() {
        if !run
            .iter()
            .any(|row| row.label.starts_with(RESUME_FULL_SESSION_LABEL))
        {
            continue;
        }
        if !run
            .iter()
            .any(|row| row.label.starts_with(RESUME_FROM_SUMMARY_LABEL))
        {
            continue;
        }
        let Some(selected_index) = run.iter().position(|row| row.selected) else {
            continue; // no highlight proven ⇒ no derivable key count
        };
        let first_row_line = run.first().map(|row| row.line).unwrap_or_default();
        let last_row_line = run.last().map(|row| row.line).unwrap_or_default();
        let confirmed = window
            .iter()
            .enumerate()
            .skip(last_row_line + 1)
            .find(|(_, line)| line.contains(CONFIRM_FOOTER))
            .is_some_and(|(line, _)| {
                window.len() - line <= SESSION_CHAT_RESUME_PROMPT_FOOTER_TAIL_LINES
            });
        if !confirmed {
            continue; // an answered picker left in scrollback, not a live one
        }
        return Some(SessionChatTerminalPicker {
            detail: prose_above(&window, first_row_line),
            rows: run
                .into_iter()
                .map(|row| SessionChatTerminalPickerRow {
                    label: row.label,
                    selected: row.selected,
                })
                .collect(),
            selected_index,
        });
    }
    None
}
