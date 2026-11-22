//! CDXC:AgentScreenDetection 2026-10-06 DECISION:
//! "Questions and approvals. These are read from the screen, since Empryo has no waiting hook. They're answered with digit keys 1–9, arrows and Enter, and Esc cancels." Empryo 3.9.0-beta asks both through one panel: a rounded box holding the question, numbered rows (`1 › Red`, `2   Blue`), an `Other…` row unless the asker hid it, and the key hint `[1-9] Pick · [↑↓] Select · [Enter] Confirm`. A digit picks and submits its row at once; the `Other…` row opens a one-line answer box that Enter submits. An approval is that panel with Allow / Deny rows, so it becomes a permission card answered through the picker lane, and every other panel becomes a question card.
//! The approval card is raised by session_chat_empryo_blocking.rs; answers go through session_chat_send/answer_http.rs.

use crate::session_chat::{
    SessionChatInteractivePrompt, SessionChatQuestion, SessionChatQuestionOption,
    SessionChatQuestionSelection,
};
use crate::session_chat_notice::{
    SessionChatTerminalNotice, SessionChatTerminalNoticeAction, SessionChatTerminalNoticeChoice,
    SessionChatTerminalNoticeSeverity, SessionChatTerminalNoticeSource,
    SESSION_CHAT_NOTICE_PERMISSION_PROMPT,
};
use crate::session_chat_send::AskAnswerKeyGroup;

const EMPRYO_ASK_TOOL: &str = "ask_user";
const ENTER: &str = "\r";
const DOWN: &str = "\u{1b}[B";
const UP: &str = "\u{1b}[A";
/// Rows Empryo paints below the panel: its input box squeezed to the top border, and the statusline.
const PANEL_FOOTER_MAX_LINES: usize = 4;

/// One Empryo choice panel read off the screen.
pub(crate) struct EmpryoChoicePanel {
    /// The panel's prose above its rows, paragraphs joined by a blank line.
    text: String,
    /// Row labels in screen order, without the `Other…` row.
    options: Vec<String>,
    /// The highlighted row, which arrows move from.
    selected: usize,
    /// The panel ends with an `Other…` row that takes a typed answer.
    other: bool,
}

impl EmpryoChoicePanel {
    /// Empryo's tool approvals offer Allow first and Deny among the rest.
    fn is_approval(&self) -> bool {
        self.options.first().map(String::as_str) == Some("Allow")
            && self.options.iter().any(|option| option == "Deny")
    }
}

/// `N › label` (highlighted) or `N   label`: the row number and its label.
fn option_row(inner: &str) -> Option<(usize, bool, String)> {
    let inner = inner.trim_start();
    let digits = inner.chars().take_while(char::is_ascii_digit).count();
    let number = inner[..digits].parse::<usize>().ok()?;
    let rest = inner[digits..].strip_prefix(' ')?;
    let (selected, label) = match rest.strip_prefix("› ") {
        Some(label) => (true, label),
        None => (false, rest.strip_prefix("  ")?),
    };
    let label = label.trim();
    (!label.is_empty()).then(|| (number, selected, label.to_string()))
}

/// The text between a panel row's borders.
fn panel_row(line: &str) -> Option<&str> {
    line.strip_prefix('│')?.strip_suffix('│')
}

/// The open choice panel, when the bottom of the screen shows one. Read bottom-up: the key hint
/// sits within a few rows of the bottom, so an ordinary screen is left after those rows.
pub(crate) fn detect_empryo_choice_panel(screen_text: &str) -> Option<EmpryoChoicePanel> {
    // The panel's rows, bottom row first, from the line under its foot up to its head.
    let mut lines: Vec<String> = Vec::new();
    let mut hint = None;
    let mut rows_below_hint = 0;
    for line in screen_text.lines().rev() {
        let line = crate::session_chat_options::strip_ansi_sgr(line)
            .trim()
            .to_string();
        if hint.is_none() {
            if panel_row(&line).is_some_and(|row| row.contains("[Enter]") && row.contains("[1-9]"))
            {
                hint = Some(lines.len());
            } else if !line.is_empty() {
                rows_below_hint += 1;
                // The panel's foot, the squeezed input box and the statusline.
                if rows_below_hint > PANEL_FOOTER_MAX_LINES + 1 {
                    return None;
                }
            }
        }
        let head = hint.is_some() && line.starts_with('╭');
        lines.push(line);
        if head {
            break;
        }
    }
    let hint = hint?;
    if !lines.get(hint.checked_sub(1)?)?.starts_with('╰') || !lines.last()?.starts_with('╭') {
        return None;
    }
    let body: Vec<&str> = lines[hint + 1..lines.len() - 1]
        .iter()
        .rev()
        .map(|line| panel_row(line).map(str::trim_end))
        .collect::<Option<_>>()?;
    let first_row = body.iter().position(|line| option_row(line).is_some())?;
    let mut options = Vec::new();
    let mut selected = 0;
    for line in &body[first_row..] {
        let (number, highlighted, label) = option_row(line)?;
        if number != options.len() + 1 {
            return None;
        }
        if highlighted {
            selected = options.len();
        }
        options.push(label);
    }
    // `Other…` in every locale Empryo ships ends with an ellipsis and is one word.
    let other = options.len() > 1
        && options
            .last()
            .is_some_and(|label| label.ends_with('…') && !label.contains(' '));
    if other {
        options.pop();
    }
    let text = body[..first_row]
        .split(|line| line.trim().is_empty())
        .filter(|paragraph| !paragraph.is_empty())
        .map(|paragraph| {
            paragraph
                .iter()
                .map(|line| line.trim())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    (!text.is_empty() && !options.is_empty()).then_some(EmpryoChoicePanel {
        text,
        options,
        selected,
        other,
    })
}

/// The question card for an open panel that is not an approval.
pub fn detect_empryo_question_prompt(
    agent: Option<&str>,
    screen_text: &str,
) -> Option<SessionChatInteractivePrompt> {
    if agent.map(str::trim) != Some("empryo") {
        return None;
    }
    let panel = detect_empryo_choice_panel(screen_text).filter(|panel| !panel.is_approval())?;
    Some(SessionChatInteractivePrompt::Question {
        questions: vec![SessionChatQuestion {
            question: panel.text,
            header: None,
            multi_select: false,
            allow_custom: Some(panel.other),
            tool_name: Some(EMPRYO_ASK_TOOL.to_string()),
            recommended: None,
            preview_layout: false,
            options: panel
                .options
                .into_iter()
                .map(|label| SessionChatQuestionOption {
                    label,
                    description: None,
                    preview: None,
                })
                .collect(),
        }],
        tool_use_id: None,
    })
}

/// The permission card for an open approval panel, its rows answerable from chat.
pub fn detect_empryo_approval_notice(screen_text: &str) -> Option<SessionChatTerminalNotice> {
    let panel = detect_empryo_choice_panel(screen_text).filter(EmpryoChoicePanel::is_approval)?;
    Some(
        SessionChatTerminalNotice::new(
            SESSION_CHAT_NOTICE_PERMISSION_PROMPT,
            SessionChatTerminalNoticeSeverity::Warning,
            SessionChatTerminalNoticeSource::Screen,
            "Empryo is asking for permission",
        )
        .with_detail(panel.text)
        .with_screen_tail(
            crate::session_chat_notice::session_chat_terminal_screen_tail(screen_text),
        )
        .with_choices(
            panel
                .options
                .into_iter()
                .enumerate()
                .map(|(index, label)| SessionChatTerminalNoticeChoice {
                    index,
                    label,
                    selected: index == panel.selected,
                })
                .collect(),
        )
        .with_actions(vec![SessionChatTerminalNoticeAction::switch_to_terminal(
            "Open terminal",
        )]),
    )
}

/// The digit key for row `index`; Empryo numbers rows 1–9.
fn row_digit(index: usize) -> Option<String> {
    (index < 9).then(|| (index + 1).to_string())
}

/// Keys that pick row `index`: its digit while there is one, otherwise arrows from the highlight
/// and Enter.
fn pick_keys(panel: &EmpryoChoicePanel, index: usize) -> Vec<AskAnswerKeyGroup> {
    if let Some(digit) = row_digit(index) {
        return vec![AskAnswerKeyGroup::Raw(digit)];
    }
    let arrow = if index >= panel.selected { DOWN } else { UP };
    let mut keys = vec![AskAnswerKeyGroup::Raw(arrow.to_string()); index.abs_diff(panel.selected)];
    keys.push(AskAnswerKeyGroup::Raw(ENTER.to_string()));
    keys
}

/// The digit that answers the approval panel on screen with row `index`, or `None` when the
/// panel is gone or has no such row.
pub fn empryo_approval_answer_key(screen_text: &str, index: usize) -> Option<String> {
    let panel = detect_empryo_choice_panel(screen_text).filter(EmpryoChoicePanel::is_approval)?;
    (index < panel.options.len()).then_some(())?;
    row_digit(index)
}

/// Keys that answer the question card from the panel on screen, or `None` when that panel no
/// longer asks it. A picked row wins; free text goes through the `Other…` row.
pub fn build_empryo_ask_answer_keys(
    screen_text: &str,
    questions: &[SessionChatQuestion],
    selections: &[SessionChatQuestionSelection],
) -> Option<Vec<AskAnswerKeyGroup>> {
    let panel = detect_empryo_choice_panel(screen_text).filter(|panel| !panel.is_approval())?;
    let question = questions.first()?;
    if question.question != panel.text {
        return None;
    }
    let selection = selections.first();
    if let Some(index) = selection
        .and_then(|selection| selection.indices.first().copied())
        .filter(|index| *index < panel.options.len())
    {
        return Some(pick_keys(&panel, index));
    }
    let other = selection
        .and_then(|selection| selection.other.as_deref())
        .map(str::trim)
        .filter(|text| !text.is_empty() && panel.other)?;
    let mut keys = pick_keys(&panel, panel.options.len());
    // Typed, never pasted: Empryo attaches the clipboard's image to any text paste (see
    // `build_empryo_input_bytes`). The answer box is one line, so line breaks become spaces.
    let other = other
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    keys.push(AskAnswerKeyGroup::Raw(
        crate::session_chat_send::build_empryo_input_bytes(&other),
    ));
    keys.push(AskAnswerKeyGroup::Raw(ENTER.to_string()));
    Some(keys)
}
