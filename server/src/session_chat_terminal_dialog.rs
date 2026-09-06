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

impl TerminalDialog {
    pub fn into_notice(mut self, kind: &'static str) -> SessionChatTerminalNotice {
        // Terminal box rules wrap into several empty rows at chat widths. The
        // chat card and its input already provide those layout boundaries.
        self.body = self
            .body
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.starts_with("│ ⌕")
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
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
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
