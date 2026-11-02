//! Workspace trust selectors whose unnumbered rows are answered by navigation.

use crate::session_chat_notice::{
    SessionChatTerminalNotice, SessionChatTerminalNoticeAction, SessionChatTerminalNoticeChoice,
    SessionChatTerminalNoticeSeverity, SessionChatTerminalNoticeSource,
    SESSION_CHAT_NOTICE_TRUST_PROMPT,
};
use crate::session_chat_options::{
    normalize_spaces, session_chat_option_agent, strip_ansi_sgr, SessionChatOptionAgent,
};

/// CDXC:AgentScreenDetection 2026-09-07 DECISION:
/// User: check every chat-supported agent and make missing workspace trust notices answerable from chat.
/// Claude 2.1.260 can offer continuing without project permissions instead of exiting; Antigravity defaults to trusting the folder.
/// Keep the displayed choices and derive navigation from a fresh capture, since either selection can move independently in the terminal.
pub fn detect_workspace_trust_prompt(
    agent: SessionChatOptionAgent,
    text: &str,
) -> Option<SessionChatTerminalNotice> {
    let (agent_id, evidence) = match agent {
        SessionChatOptionAgent::Claude => ("claude", "Quick safety check:"),
        SessionChatOptionAgent::Antigravity => (
            "antigravity",
            "Antigravity CLI requires permission to read, edit, and execute files here.",
        ),
        _ => return None,
    };
    let mut lines: Vec<String> = text
        .lines()
        .rev()
        .map(|line| normalize_spaces(&strip_ansi_sgr(line)))
        .filter(|line| !line.trim().is_empty())
        .take(80)
        .collect();
    lines.reverse();
    let heading = lines
        .iter()
        .rposition(|line| line.trim() == "Accessing workspace:")?;
    let footer = lines.iter().rposition(|line| {
        let lower = line.to_ascii_lowercase();
        lower.contains("enter") && lower.contains("confirm")
    })?;
    if heading >= footer || lines.len() - footer > 3 {
        return None;
    }
    if crate::session_chat_composer::detect_session_chat_composer_ready(
        Some(agent_id),
        &lines[footer + 1..].join("\n"),
    )
    .state
        == crate::session_chat_composer::SessionChatComposerState::Ready
    {
        return None;
    }
    let context = lines[heading..footer].join(" ");
    let context = context.split_whitespace().collect::<Vec<_>>().join(" ");
    if !context.contains(evidence) {
        return None;
    }
    let mut first_row = None;
    let mut choices = Vec::new();
    for (index, line) in lines.iter().enumerate().take(footer).skip(heading + 1) {
        let trimmed = line.trim();
        let selected = trimmed.starts_with(['❯', '>']);
        let label = trimmed.trim_start_matches(['❯', '>']).trim();
        if matches!(
            label,
            "Yes, I trust this folder" | "No, exit" | "No, continue without these permissions"
        ) {
            first_row.get_or_insert(index);
            choices.push(SessionChatTerminalNoticeChoice {
                index: choices.len(),
                label: label.to_string(),
                selected,
            });
        } else if first_row.is_some() {
            return None;
        }
    }
    if choices.len() != 2
        || choices.iter().filter(|row| row.selected).count() != 1
        || choices
            .iter()
            .filter(|row| row.label == "Yes, I trust this folder")
            .count()
            != 1
    {
        return None;
    }
    Some(
        SessionChatTerminalNotice::new(
            SESSION_CHAT_NOTICE_TRUST_PROMPT,
            SessionChatTerminalNoticeSeverity::Warning,
            SessionChatTerminalNoticeSource::Screen,
            "Trust this folder?",
        )
        .with_detail(lines[heading + 1..first_row?].join("\n"))
        .with_screen_tail(crate::session_chat_notice::session_chat_terminal_screen_tail(text))
        .with_choices(choices)
        .with_actions(vec![SessionChatTerminalNoticeAction::switch_to_terminal(
            "Terminal view",
        )]),
    )
}

pub fn workspace_trust_answer_key(
    agent: Option<&str>,
    text: &str,
    target: usize,
) -> Option<String> {
    let notice = detect_workspace_trust_prompt(session_chat_option_agent(agent)?, text)?;
    notice.choices.get(target)?;
    let selected = notice.choices.iter().position(|row| row.selected)?;
    let arrow = if target >= selected {
        "\x1b[1;1B"
    } else {
        "\x1b[1;1A"
    };
    Some(format!("{}\r", arrow.repeat(target.abs_diff(selected))))
}
