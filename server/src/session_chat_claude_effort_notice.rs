//! Claude's unnumbered default-effort recommendation.

use sha2::{Digest, Sha256};

use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};
use crate::session_chat_terminal_dialog::{TerminalDialog, TerminalDialogRow};

/// CDXC:AgentScreenDetection 2026-09-15 DECISION:
/// User: show Claude's Fable xhigh pricing notice in chat so it can be answered without switching to the terminal.
/// WHY: Claude 2.1.268 draws this under a solid rule with unnumbered choices, unlike its other panels. The terminal's wording and row order supply both choices; the shared dialog driver answers by arrows and Enter.
pub(crate) fn detect_effort_notice(text: &str) -> Option<TerminalDialog> {
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            normalize_spaces(&strip_ansi_sgr(line))
                .trim_end()
                .to_string()
        })
        .collect();
    let start = lines.iter().rposition(|line| {
        let line = line.trim();
        line.chars().count() >= 20 && line.chars().all(|c| c == '─')
    })? + 1;
    let content = &lines[start..];
    let heading = content.iter().position(|line| !line.trim().is_empty())?;
    let title = content[heading].trim();
    let (model, effort) = title
        .strip_prefix("Use ")?
        .strip_suffix(" effort by default?")?
        .rsplit_once(" at ")?;
    let switch_label = format!("Switch {model} to {effort} effort");
    let remainder = &content[heading + 1..];
    let first_row = remainder.iter().position(|line| {
        let label = line.trim().trim_start_matches('❯').trim();
        label.starts_with("Keep ") || label == switch_label
    })?;
    let mut rows = Vec::new();
    for line in remainder[first_row..]
        .iter()
        .filter(|line| !line.trim().is_empty())
    {
        let line = line.trim();
        let label = line.trim_start_matches('❯').trim();
        if label != switch_label
            && !label
                .strip_prefix("Keep ")
                .is_some_and(|effort| matches!(effort, "high" | "xhigh" | "max"))
        {
            return None;
        }
        rows.push(TerminalDialogRow {
            number: rows.len() as u32 + 1,
            label: label.to_string(),
            description: None,
            selected: line.starts_with('❯'),
        });
    }
    if rows.len() != 2
        || rows.iter().filter(|row| row.selected).count() != 1
        || rows.iter().filter(|row| row.label == switch_label).count() != 1
    {
        return None;
    }
    Some(TerminalDialog {
        id: hex::encode(Sha256::digest(content.join("\n").as_bytes())),
        title: title.to_string(),
        body: remainder[..first_row].join("\n").trim().to_string(),
        footer: String::new(),
        rows,
        input: None,
        input_value: String::new(),
        actions: vec!["confirm".to_string(), "cancel".to_string()],
        side_question: None,
        blocks: None,
        picker: None,
    })
}
