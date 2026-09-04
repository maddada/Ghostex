/*
CDXC:AgentScreenDetection 2026-08-18 (picker rewrite 2026-08-21):
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
its printed number, and which row the TUI highlights right now.
`session_chat_notice.rs` turns that into the input-blocking `resumePrompt`
terminal notice the chat surfaces render as an answer picker, and
`answerSessionChatPrompt`'s `terminalChoice` lane types the chosen row's NUMBER.

Detection lives here (server side) so every chat client — gpui, ghostex-web
and the RN mobile app — inherits it from one implementation.

Answering by number, measured 2026-08-22 against Claude Code on a zmx pty:
writing the legacy Down arrow (`ESC [ B`) into this picker does NOTHING — the
highlight does not move — so an arrow-walk-then-Enter answer confirmed the
HIGHLIGHTED row every time, which is row 1, whatever the user picked. Writing
the digit `2` both selected and committed row 2 with no Enter at all. This is
the same finding as Claude's AskUserQuestion selector (see
`build_claude_ask_answer_keys`, bug STA-1860, "delivered every non-first pick as
the first option"), and the same fix: drive these selectors by their stable
1-based number, never by navigation.

The number comes off the ROW, not from its position in the run, because the
row order and the "Don't ask me again" row are Claude's to change.

Matching is deliberately narrow: the picker only counts when a run of
consecutive numbered rows carries BOTH canonical labels, one of those rows is
the highlighted one, and the "Enter to confirm" footer follows the run inside
the scanned tail window.
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

/*
CDXC:SessionChat 2026-08-29:
Claude Code's model/effort switch confirmation (one component paints both,
verified in the 2.1.251 bundle):

    Switch model?
    Your next response will be slower and use more tokens

    This conversation is cached for the current model. Switching to Fable 5
    means the full history gets re-read on your next message.

    ❯ 1. Yes, switch to Fable 5
      2. No, go back

It appears after picking a row in /model (or /effort) and owns the input line
exactly like the resume picker, so a `/model` dispatched from the chat composer
strands the CLI on a dialog chat never showed. Unlike the resume picker the
component passes `hideInputGuide`, so there is NO "Enter to confirm" footer to
anchor liveness on; the run must instead sit at the very bottom of the capture.
Claude's other "No, go back" confirmations (MCP trust, gateway trust) pass
`hideIndexes`, so they never parse as numbered rows and cannot match here.
*/
pub const SESSION_CHAT_SWITCH_CONFIRM_PROMPT_KIND: &str = "switchConfirmPrompt";

const SWITCH_CONFIRM_YES_PREFIX: &str = "Yes, switch to ";
const SWITCH_CONFIRM_NO_LABEL: &str = "No, go back";
const SWITCH_MODEL_HEADING: &str = "Switch model?";
const SWITCH_EFFORT_HEADING: &str = "Change effort level?";

/// With the input guide hidden, only the (optional) statusline and shortcut
/// hint may follow a live switch-confirm run; anything more is scrollback from
/// a dialog that was already answered.
const SESSION_CHAT_SWITCH_CONFIRM_TAIL_LINES: usize = 5;

/*
CDXC:AgentScreenDetection 2026-08-29:
Claude Code's safeguards pause (labels verified in the 2.1.251 bundle):

    Session paused

    Fable 5's safeguards flagged this message. Our intentionally broad
    safeguards allow us to deliver more capabilities faster, but can sometimes
    flag legitimate coding, cybersecurity, and biology tasks. …

    Details: `[bio]`

    ❯ 1. Switch to Opus 5
      2. Edit prompt and retry with Fable 5

    ✻ Waiting for API response · will retry in 0s · check your network

The API refused the message and the CLI pauses on this chooser (switch to the
fallback model vs edit the prompt) until it is answered. Row labels carry the
model names, with fallback wordings "Switch to the fallback model" and "Edit
prompt and retry" when a display name is unavailable — hence prefix matching.
Like the switch confirmation it paints no "Enter to confirm" footer, so
liveness is the run sitting near the bottom of the capture; the retry spinner
line (and any statusline) below it is why the pause tail window is one line
deeper than the switch-confirm one.

This is a sibling of the transcript-detected `apiRefusal` watchdog notice, not
a replacement: that one reports a refusal the CLI already recorded and moved
past, while this one is a live dialog that blocks all input.
*/
pub const SESSION_CHAT_SESSION_PAUSED_PROMPT_KIND: &str = "sessionPausedPrompt";

const SESSION_PAUSED_HEADING: &str = "Session paused";
const SESSION_PAUSED_SWITCH_PREFIX: &str = "Switch to ";
const SESSION_PAUSED_EDIT_PREFIX: &str = "Edit prompt and retry";
const SESSION_CHAT_SESSION_PAUSED_TAIL_LINES: usize = 6;

/*
CDXC:AgentScreenDetection 2026-09-04 WHY:
Claude Code's tool permission prompt, read off the screen (measured against
2.1.260 on a zmx pty):

    Bash command

      cd /Users/madda/dev/_scratch/permission-repro && cd sub && cat file.txt
      Print sub/file.txt from repo root

    Multiple directory changes in one command require approval for clarity

    Do you want to proceed?
    ❯ 1. Yes
      2. Yes, and switch to auto mode · auto mode handles these prompts for you
      3. No

    Esc to cancel · Tab to amend

The hook-derived approval card (`PermissionRequest` → `SessionChatInteractivePrompt::Approval`)
already covers this dialog when the hook fires and its card survives, but the
hook payload carries no `tool_use_id`, so a sibling tool call finishing in the
same parallel batch retired the card while the dialog was still on screen
(observed 2026-09-04: a "Compound command contains cd with a relative file read
while a Read() deny rule exists" prompt with no card anywhere in chat). The
screen is the only source that cannot be out of date about a dialog that owns
the input line, so this kind describes the dialog from the capture like the
resume picker does: the heading directly above the rows, a Yes row, a No row,
and the run at the bottom of the capture with no composer chrome after it (the
TUI erases the dialog once it is answered, and repaints its composer box).

The row set is Claude's to change ("Yes, and don't ask again for …", "Yes,
allow all edits during this session", "Yes, and switch to auto mode"), which is
why only the Yes/No prefixes are required and every row is carried verbatim.
The thinking-mode toggle paints the same heading over Enabled/Disabled rows and
must not match; the Yes/No requirement is what keeps it out.

CDXC:AgentScreenDetection 2026-09-04 DECISION:
User: answer this dialog with "Enter for Yes and down arrow then Enter for No",
so the answer is an arrow walk from the highlighted row to the chosen one plus
Enter, not the row digit the other pickers type. Measured the same day: `ESC [ B`
DOES move this dialog's highlight (unlike the resume picker measurement above),
and the walk is computed from a capture taken at answer time, so the starting
row is never stale.
*/
pub const SESSION_CHAT_PERMISSION_PROMPT_KIND: &str = "permissionPrompt";

const PERMISSION_PROMPT_HEADING: &str = "Do you want to proceed?";
const PERMISSION_PROMPT_YES_PREFIX: &str = "Yes";
const PERMISSION_PROMPT_NO_PREFIX: &str = "No";

/// Only the "Esc to cancel" footer and the user's statusline (three lines on
/// the measured setup, user-configurable) may follow a live permission prompt.
const SESSION_CHAT_PERMISSION_PROMPT_TAIL_LINES: usize = 8;

/// A horizontal rule this long after the rows is the composer box Claude
/// repaints once a dialog is answered, which proves the rows are scrollback.
const SESSION_CHAT_PERMISSION_PROMPT_MIN_RULE_CHARS: usize = 20;

const KEY_DOWN: &str = "\u{1b}[B";
const KEY_UP: &str = "\u{1b}[A";
const KEY_ENTER: &str = "\r";

/// Which on-screen chooser a detection describes; the notice kind and title
/// come from this.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionChatTerminalPickerKind {
    /// The resume-usage chooser (summary vs full session).
    Resume,
    /// The "Switch model?" confirmation.
    SwitchModel,
    /// The "Change effort level?" confirmation, painted by the same component.
    SwitchEffort,
    /// The safeguards "Session paused" chooser.
    SessionPaused,
    /// A tool permission prompt ("Do you want to proceed?" over Yes/No rows).
    PermissionPrompt,
}

/// One row of an on-screen picker, in the order it is painted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionChatTerminalPickerRow {
    /// The number the TUI printed on this row, which is also the key that
    /// selects it. Read off the screen, never inferred from the position.
    pub number: u32,
    pub label: String,
    /// True for the row the TUI highlights right now (the one a bare Enter
    /// would confirm).
    pub selected: bool,
}

/// A live, answerable picker: what it asks, what it offers, and where the
/// highlight sits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionChatTerminalPicker {
    pub kind: SessionChatTerminalPickerKind,
    /// Prose painted above the rows, joined and capped. `None` when the picker
    /// stands alone.
    pub detail: Option<String>,
    pub rows: Vec<SessionChatTerminalPickerRow>,
    /// Index into `rows` of the highlighted row.
    pub selected_index: usize,
}

impl SessionChatTerminalPicker {
    /*
    The keystrokes that pick row `target`. For the number-driven pickers that
    is the row's printed number, as one digit. For the permission prompt it is
    the arrow walk from the highlighted row to `target` followed by Enter (see
    the kind's DECISION comment). `None` when `target` is not a row this picker
    painted, or when its number cannot be typed as a single key — an answer is
    never guessed at, and the card's "Open terminal" action is the honest
    fallback for a picker this build cannot drive.
    */
    pub fn answer_key(&self, target: usize) -> Option<String> {
        let row = self.rows.get(target)?;
        let number = row.number;
        /*
        CDXC:SessionChat 2026-09-04 DECISION:
        User: the "Resume full session as-is" row is answered with a single Escape, nothing else.
        The picker's own footer offers "Esc to cancel", and cancelling the chooser resumes the full session, so Escape reaches that outcome in one key without any row navigation.
        The Escape is the kitty CSI-u encoding for the same reason as SESSION_CHAT_INTERRUPT: under zmx a bare 0x1b is dropped.
        */
        if self.kind == SessionChatTerminalPickerKind::Resume
            && row.label.starts_with(RESUME_FULL_SESSION_LABEL)
        {
            return Some(crate::session_chat_send::SESSION_CHAT_INTERRUPT.to_string());
        }
        if self.kind == SessionChatTerminalPickerKind::PermissionPrompt {
            let step = if target >= self.selected_index {
                KEY_DOWN
            } else {
                KEY_UP
            };
            let distance = target.abs_diff(self.selected_index);
            let mut keys = step.repeat(distance);
            keys.push_str(KEY_ENTER);
            return Some(keys);
        }
        (1..=9).contains(&number).then(|| number.to_string())
    }
}

#[derive(Clone, Debug)]
struct PickerRow {
    /// Index of the row inside the scanned window.
    line: usize,
    number: u32,
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

/// `❯ 2. Resume full session as-is` → (selected, number, label). Rows that are
/// not a numbered picker row return `None`.
fn parse_picker_row(line: &str) -> Option<(bool, u32, String)> {
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
    let number: u32 = digits.parse().ok()?;
    let rest = rest[digits.len()..].strip_prefix('.')?;
    let label = rest.trim();
    if label.is_empty() {
        return None;
    }
    Some((selected, number, label.to_string()))
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
            Some((selected, number, label)) => {
                if current
                    .last()
                    .is_some_and(|previous: &PickerRow| previous.line + 1 != line)
                {
                    runs.push(std::mem::take(&mut current));
                }
                current.push(PickerRow {
                    line,
                    number,
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

/// The resume-usage picker, proven live by its "Enter to confirm" footer
/// sitting at the very bottom of the capture.
fn resume_run_kind(window: &[String], run: &[PickerRow]) -> Option<SessionChatTerminalPickerKind> {
    if !run
        .iter()
        .any(|row| row.label.starts_with(RESUME_FULL_SESSION_LABEL))
        || !run
            .iter()
            .any(|row| row.label.starts_with(RESUME_FROM_SUMMARY_LABEL))
    {
        return None;
    }
    let last_row_line = run.last().map(|row| row.line).unwrap_or_default();
    let confirmed = window
        .iter()
        .enumerate()
        .skip(last_row_line + 1)
        .find(|(_, line)| line.contains(CONFIRM_FOOTER))
        .is_some_and(|(line, _)| {
            window.len() - line <= SESSION_CHAT_RESUME_PROMPT_FOOTER_TAIL_LINES
        });
    // Not confirmed ⇒ an answered picker left in scrollback, not a live one.
    confirmed.then_some(SessionChatTerminalPickerKind::Resume)
}

/// The model/effort switch confirmation. No footer exists to prove liveness
/// (see the kind's comment), so the run itself must sit at the bottom of the
/// capture and its heading must be painted above it.
fn switch_confirm_run_kind(
    window: &[String],
    run: &[PickerRow],
) -> Option<SessionChatTerminalPickerKind> {
    if !run
        .iter()
        .any(|row| row.label.starts_with(SWITCH_CONFIRM_YES_PREFIX))
        || !run
            .iter()
            .any(|row| row.label.starts_with(SWITCH_CONFIRM_NO_LABEL))
    {
        return None;
    }
    let first_row_line = run.first().map(|row| row.line).unwrap_or_default();
    let last_row_line = run.last().map(|row| row.line).unwrap_or_default();
    if window.len() - last_row_line > SESSION_CHAT_SWITCH_CONFIRM_TAIL_LINES {
        return None;
    }
    window[..first_row_line].iter().rev().find_map(|line| {
        let line = strip_box_border(line);
        if line.contains(SWITCH_MODEL_HEADING) {
            Some(SessionChatTerminalPickerKind::SwitchModel)
        } else if line.contains(SWITCH_EFFORT_HEADING) {
            Some(SessionChatTerminalPickerKind::SwitchEffort)
        } else {
            None
        }
    })
}

/// The safeguards "Session paused" chooser. Same liveness reasoning as the
/// switch confirmation, with its heading painted above the rows.
fn session_paused_run_kind(
    window: &[String],
    run: &[PickerRow],
) -> Option<SessionChatTerminalPickerKind> {
    if !run
        .iter()
        .any(|row| row.label.starts_with(SESSION_PAUSED_SWITCH_PREFIX))
        || !run
            .iter()
            .any(|row| row.label.starts_with(SESSION_PAUSED_EDIT_PREFIX))
    {
        return None;
    }
    let first_row_line = run.first().map(|row| row.line).unwrap_or_default();
    let last_row_line = run.last().map(|row| row.line).unwrap_or_default();
    if window.len() - last_row_line > SESSION_CHAT_SESSION_PAUSED_TAIL_LINES {
        return None;
    }
    window[..first_row_line]
        .iter()
        .any(|line| strip_box_border(line).contains(SESSION_PAUSED_HEADING))
        .then_some(SessionChatTerminalPickerKind::SessionPaused)
}

/// True for a horizontal rule long enough to be Claude's composer frame.
fn is_composer_rule(line: &str) -> bool {
    line.trim().chars().filter(|ch| *ch == '\u{2500}').count()
        >= SESSION_CHAT_PERMISSION_PROMPT_MIN_RULE_CHARS
}

/// The tool permission prompt: its heading painted directly above the rows, a
/// Yes row and a No row, the run at the bottom of the capture, and no composer
/// frame after it (see the kind's comment for why that proves liveness).
fn permission_prompt_run_kind(
    window: &[String],
    run: &[PickerRow],
) -> Option<SessionChatTerminalPickerKind> {
    if !run
        .iter()
        .any(|row| row.label.starts_with(PERMISSION_PROMPT_YES_PREFIX))
        || !run
            .iter()
            .any(|row| row.label.starts_with(PERMISSION_PROMPT_NO_PREFIX))
    {
        return None;
    }
    let first_row_line = run.first().map(|row| row.line).unwrap_or_default();
    let last_row_line = run.last().map(|row| row.line).unwrap_or_default();
    if window.len() - last_row_line > SESSION_CHAT_PERMISSION_PROMPT_TAIL_LINES {
        return None;
    }
    if window[last_row_line + 1..]
        .iter()
        .any(|line| is_composer_rule(line))
    {
        return None;
    }
    let heading = first_row_line
        .checked_sub(1)
        .map(|line| strip_box_border(&window[line]))?;
    heading
        .contains(PERMISSION_PROMPT_HEADING)
        .then_some(SessionChatTerminalPickerKind::PermissionPrompt)
}

/// `Some` only when a known Claude picker is live on screen and its highlight
/// is proven, so every row is reachable with a derived key count.
pub fn detect_session_chat_terminal_picker(text: &str) -> Option<SessionChatTerminalPicker> {
    let window = scan_window(text);
    for run in picker_runs(&window).into_iter().rev() {
        let Some(kind) = resume_run_kind(&window, &run)
            .or_else(|| switch_confirm_run_kind(&window, &run))
            .or_else(|| session_paused_run_kind(&window, &run))
            .or_else(|| permission_prompt_run_kind(&window, &run))
        else {
            continue;
        };
        let Some(selected_index) = run.iter().position(|row| row.selected) else {
            continue; // no highlight proven ⇒ no derivable key count
        };
        let first_row_line = run.first().map(|row| row.line).unwrap_or_default();
        let detail = prose_above(&window, first_row_line).and_then(|prose| {
            // The heading is the card title's job; the detail keeps the tool,
            // the command and Claude's reason for asking.
            if kind != SessionChatTerminalPickerKind::PermissionPrompt {
                return Some(prose);
            }
            let stripped = prose
                .strip_suffix(PERMISSION_PROMPT_HEADING)
                .map(str::trim_end)
                .unwrap_or(&prose)
                .to_string();
            (!stripped.is_empty()).then_some(stripped)
        });
        return Some(SessionChatTerminalPicker {
            kind,
            detail,
            rows: run
                .into_iter()
                .map(|row| SessionChatTerminalPickerRow {
                    number: row.number,
                    label: row.label,
                    selected: row.selected,
                })
                .collect(),
            selected_index,
        });
    }
    None
}
