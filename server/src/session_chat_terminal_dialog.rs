//! Shared presentation contract for agent-owned terminal dialogs.
use crate::session_chat_notice::{
    SessionChatTerminalNotice, SessionChatTerminalNoticeSeverity, SessionChatTerminalNoticeSource,
};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalDialogRow {
    pub number: u32,
    pub label: String,
    pub description: Option<String>,
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalDialog {
    pub id: String,
    pub title: String,
    pub body: String,
    pub footer: String,
    pub rows: Vec<TerminalDialogRow>,
    pub input: Option<String>,
    pub input_value: String,
    pub actions: Vec<String>,
}

/// CDXC:AgentScreenDetection 2026-09-26 WHY: a line that only tells the terminal user which key does what ("shift+tab to approve with this feedback", "ctrl+g to edit in Prompt-editor · <plan file>") has no meaning in the chat card, whose rows and input are the controls.
fn is_terminal_key_hint(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    [
        "shift+tab to ",
        "ctrl+g to ",
        "ctrl+o to ",
        "tab to ",
        "esc to ",
    ]
    .iter()
    .any(|lead| lower.starts_with(lead))
}

impl TerminalDialog {
    pub fn into_notice(mut self, kind: &'static str) -> SessionChatTerminalNotice {
        // Terminal box rules wrap into several empty rows at chat widths. The
        // chat card and its input already provide those layout boundaries.
        let lines: Vec<&str> = self
            .body
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.starts_with("│ ⌕")
                    || is_terminal_key_hint(trimmed)
                    || (!trimmed.is_empty()
                        && trimmed
                            .chars()
                            .all(|c| c.is_whitespace() || matches!(c, '\u{2500}'..='\u{259f}')))
                {
                    return None;
                }
                Some(
                    line.trim_end()
                        .trim_start_matches('│')
                        .trim_end_matches('│')
                        .trim_end(),
                )
            })
            .collect();
        // A fixed-height pane (the plan Claude asks to approve) pads its text
        // with empty rows; one blank line keeps the paragraph break.
        let mut body: Vec<&str> = Vec::with_capacity(lines.len());
        for line in lines {
            if line.trim().is_empty() && body.last().is_some_and(|last| last.trim().is_empty()) {
                continue;
            }
            body.push(line);
        }
        self.body = body.join("\n").trim().to_string();
        let mut notice = SessionChatTerminalNotice::new(
            kind,
            SessionChatTerminalNoticeSeverity::Info,
            SessionChatTerminalNoticeSource::Screen,
            self.title.clone(),
        );
        notice.detail = (!self.body.is_empty()).then(|| self.body.clone());
        notice.choices = self
            .rows
            .iter()
            .enumerate()
            .map(
                |(index, row)| crate::session_chat_notice::SessionChatTerminalNoticeChoice {
                    index,
                    label: row
                        .description
                        .as_ref()
                        .map(|detail| format!("{} {detail}", row.label))
                        .unwrap_or_else(|| row.label.clone()),
                    selected: row.selected,
                },
            )
            .collect();
        notice.screen_tail = Some(format!(
            "{}\n{}\n{}\n{}",
            self.title,
            self.body,
            self.rows
                .iter()
                .map(|row| format!(
                    "{}{}. {}{}",
                    if row.selected { "› " } else { "  " },
                    row.number,
                    row.label,
                    row.description
                        .as_ref()
                        .map(|detail| format!("  {detail}"))
                        .unwrap_or_default()
                ))
                .collect::<Vec<_>>()
                .join("\n"),
            self.footer
        ));
        notice.dialog = Some(self);
        notice
    }
}
