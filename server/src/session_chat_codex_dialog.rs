//! Codex-owned dialogs projected into the shared chat notice card.

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::domain::DomainStateError;
use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};
use crate::session_chat_send::{
    capture_session_terminal_text, execute_session_chat_send, SessionChatSendStep,
    SessionChatSendTarget, SESSION_CHAT_INTERRUPT,
};

use crate::session_chat_terminal_dialog::{TerminalDialog, TerminalDialogRow};

fn clean(line: &str) -> &str {
    line.trim().trim_matches(['│', '┃', '▌']).trim()
}

fn left_column(line: &str) -> &str {
    line.split("          ").next().unwrap_or_default().trim()
}

fn row(line: &str) -> Option<TerminalDialogRow> {
    let line = clean(line);
    let selected = line.starts_with('›');
    let line = line.strip_prefix('›').unwrap_or(line).trim_start();
    let (number, label) = line.split_once(". ")?;
    let number = number.parse::<u32>().ok()?;
    if number == 0 || label.trim().is_empty() {
        return None;
    }
    let (label, description) = label
        .split_once("  ")
        .map(|(label, detail)| {
            (
                label,
                Some(detail.split_whitespace().collect::<Vec<_>>().join(" ")),
            )
        })
        .unwrap_or((label, None));
    Some(TerminalDialogRow {
        number,
        label: label.trim().to_string(),
        description,
        selected,
    })
}

/// CDXC:AgentScreenDetection 2026-09-05 DECISION:
/// User: expose the "Implement this plan?" picker in chat so switching to the terminal is unnecessary.
/// User: audit every Codex command and make its messages and interactions usable in chat, using the existing UX and improving it where needed.
/// Numbered selectors, searchable menus, checkbox settings, and text forms have different key semantics, so only numbered rows use digit shortcuts; the other views retain their own navigation, toggle, save, and cancel actions.
/// SEE-ALSO: packages/core-ui/chat/session-chat-terminal-dialog.tsx and Codex tui/src/bottom_pane/list_selection_view.rs.
pub fn detect_codex_dialog(text: &str) -> Option<TerminalDialog> {
    let lines: Vec<String> = text
        .lines()
        .rev()
        .take(160)
        .map(|line| {
            normalize_spaces(&strip_ansi_sgr(line))
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let end = lines.iter().rposition(|line| !line.trim().is_empty())?;
    let footer_index = (end.saturating_sub(3)..=end).rev().find(|&i| {
        let line = clean(&lines[i]).to_ascii_lowercase();
        (line.contains("esc")
            && (line.contains("close")
                || line.contains("cancel")
                || line.contains("go back")
                || line.contains("quit")
                || line.contains("exit")
                || line.contains("back")))
            || line == "q to quit"
            || line == "press enter to continue"
            || line.starts_with("press space to select or enter to save")
    })?;
    if lines[footer_index + 1..]
        .iter()
        .any(|line| clean(line).starts_with('›'))
    {
        return None;
    }
    // A dialog is separated from scrollback by an empty band. Preserve blank
    // lines inside it, but stop at the last double-blank boundary before it.
    let mut start = (1..footer_index)
        .rev()
        .find(|&i| lines[i].trim().is_empty() && lines[i - 1].trim().is_empty())
        .map(|i| i + 1)
        .unwrap_or(0);
    // Onboarding and text-entry views use a single empty line above their
    // heading, unlike the standard list selection view's two-line band.
    if let Some(named) = (start..footer_index).rev().find(|&i| {
        clean(&lines[i]).starts_with("Tell us more (")
            || matches!(
                clean(&lines[i]),
                "Name thread"
                    | "Rename thread"
                    | "Edit goal"
                    | "Save conversation"
                    | "Add marketplace"
                    | "Remap Shortcut"
                    | "Choose an import source"
                    | "Choose what to import"
                    | "Custom review instructions"
                    | "Export filename"
                    | "Save transcript"
                    | "Resume a previous session"
                    | "Fork a previous session"
            )
    }) {
        start = named;
    }
    if clean(&lines[footer_index]) == "q to quit" {
        start = 0;
    } else if let Some(fullscreen) = lines[..footer_index].iter().rposition(|line| {
        matches!(
            clean(line),
            "Resume a previous session" | "Fork a previous session"
        )
    }) {
        start = fullscreen;
    }
    let content: Vec<&str> = lines[start..footer_index]
        .iter()
        .map(|line| line.as_str())
        .collect();
    let heading = content.iter().position(|line| !clean(line).is_empty())?;
    let title = if clean(&lines[footer_index]) == "q to quit" {
        let heading = content[heading]
            .split('/')
            .find(|part| !part.trim().is_empty())
            .unwrap_or_default()
            .trim();
        match heading.replace(' ', "").as_str() {
            "DIFF" => "Git diff".to_string(),
            "TRANSCRIPT" => "Conversation transcript".to_string(),
            _ => heading.to_string(),
        }
    } else {
        clean(left_column(content[heading])).to_string()
    };
    if title.starts_with("Question ")
        || content
            .iter()
            .any(|line| clean(line).starts_with("Question ") && line.contains('/'))
    {
        return None;
    }
    if title.len() > 200 {
        return None;
    }
    let mut rows: Vec<TerminalDialogRow> = Vec::new();
    let mut body = Vec::new();
    let mut in_rows = false;
    for line in &content[heading + 1..] {
        if let Some(parsed) = row(line) {
            in_rows = true;
            rows.push(parsed);
        } else if in_rows && !line.trim().is_empty() && line.starts_with("     ") {
            if let Some(last) = rows.last_mut() {
                let detail = last.description.get_or_insert_with(String::new);
                if !detail.is_empty() {
                    detail.push(' ');
                }
                detail.push_str(clean(line));
            }
        } else if !in_rows || !line.trim().is_empty() {
            body.push(*line);
        }
    }
    let numbered = !rows.is_empty() && rows.iter().filter(|r| r.selected).count() == 1;
    let body = if clean(&lines[footer_index]) == "q to quit" {
        content[heading + 1..]
            .iter()
            .filter(|line| line.trim() != "~" && !line.contains("pgup/pgdn to page"))
            .copied()
            .collect::<Vec<_>>()
            .join("\n")
    } else if numbered {
        body.join("\n")
    } else {
        content[heading + 1..].join("\n")
    };
    if !numbered {
        rows.clear();
    }
    let footer = lines[footer_index
        .saturating_sub(usize::from(clean(&lines[footer_index]) == "q to quit"))
        ..=end]
        .iter()
        .map(|line| clean(line))
        .collect::<Vec<_>>()
        .join("\n");
    let lower = format!("{title}\n{body}\n{footer}").to_ascii_lowercase();
    let footer_lower = footer.to_ascii_lowercase();
    let search_placeholder = content.iter().position(|line| {
        let line = left_column(line).to_ascii_lowercase();
        line.starts_with("type to search") || line.starts_with("type to filter")
    });
    let searchable_title = matches!(
        title.as_str(),
        "Select Syntax Theme"
            | "Select Pet"
            | "Keymap"
            | "Select a base branch"
            | "Select a commit to review"
            | "Auto-review Denials"
            | "Apps"
            | "Plugins"
            | "Resume a previous session"
            | "Fork a previous session"
    );
    let search_index = search_placeholder.or_else(|| {
        if !searchable_title {
            return None;
        }
        let gap = (heading + 1..content.len()).find(|&i| left_column(content[i]).is_empty())?;
        let mut index = (gap + 1..content.len()).find(|&i| !left_column(content[i]).is_empty())?;
        if title == "Keymap" && clean(content[index]).starts_with('[') {
            index = (index + 1..content.len()).find(|&i| !left_column(content[i]).is_empty())?;
        }
        Some(index)
    });
    let input = if title == "Remap Shortcut" {
        Some("key".to_string())
    } else if search_index.is_some() {
        Some("search".to_string())
    } else if content.iter().any(|line| line.trim().starts_with('▌'))
        || lower.contains("type a name")
    {
        Some("text".to_string())
    } else {
        None
    };
    let input_value = if let Some(index) = search_index {
        if search_placeholder.is_some() {
            content
                .get(index + 1)
                .and_then(|line| clean(line).strip_prefix('>'))
                .unwrap_or_default()
                .trim_start()
                .to_string()
        } else {
            left_column(content[index]).trim().to_string()
        }
    } else if input.as_deref() == Some("text") {
        let input_start = content
            .iter()
            .rposition(|line| line.trim() == "▌")
            .map(|i| i + 1)
            .unwrap_or(heading + 1);
        content[input_start..]
            .iter()
            .filter(|line| line.trim().starts_with('▌'))
            .map(|line| clean(line))
            .filter(|line| {
                !line.starts_with("Type ")
                    && !line.starts_with("(optional)")
                    && !line.starts_with("owner/repo, git URL")
                    && !line.is_empty()
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        String::new()
    };
    let mut actions = Vec::new();
    if rows.is_empty() && !matches!(input.as_deref(), Some("text" | "key")) {
        actions.extend(["up", "down"].map(str::to_string));
        if footer_lower.contains("left/right") || footer_lower.contains("←/→") {
            actions.extend(["left", "right"].map(str::to_string));
        }
        if footer_lower.contains("tab") {
            actions.push("tab".to_string());
        }
        if footer_lower.contains("space") {
            actions.push("toggle".to_string());
        }
    }
    if footer_lower.contains("page") || footer_lower.contains("browse") {
        actions.extend(["pageUp", "pageDown", "home", "end"].map(str::to_string));
    }
    if footer.to_ascii_lowercase().contains("enter") {
        actions.push("confirm".to_string());
    }
    actions.push("cancel".to_string());
    let identity = content.join("\n") + &footer;
    let id = format!("{:x}", Sha256::digest(identity.as_bytes()));
    Some(TerminalDialog {
        id,
        title,
        body: body.trim().to_string(),
        footer,
        rows,
        input,
        input_value,
        actions,
    })
}

impl TerminalDialog {
    fn payload(&self, params: &Map<String, Value>) -> Result<String, DomainStateError> {
        let invalid = || DomainStateError {
            code: "invalidParams",
            message: "That action is not offered by this Codex dialog.".to_string(),
        };
        if let Some(index) = params.get("choiceIndex").and_then(Value::as_u64) {
            let row = self.rows.get(index as usize).ok_or_else(invalid)?;
            if row.number <= 9 {
                return Ok(row.number.to_string());
            }
            let selected = self
                .rows
                .iter()
                .position(|r| r.selected)
                .ok_or_else(invalid)?;
            let target = index as usize;
            let arrow = if target >= selected {
                "\x1b[B"
            } else {
                "\x1b[A"
            };
            return Ok(arrow.repeat(target.abs_diff(selected)) + "\r");
        }
        let action = params
            .get("dialogAction")
            .and_then(Value::as_str)
            .ok_or_else(invalid)?;
        if (action == "text" && self.input.as_deref() == Some("search"))
            || (action == "submit" && self.input.as_deref() == Some("text"))
        {
            let text = params
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(invalid)?;
            let search = self.input.as_deref() == Some("search");
            if text.len() > if search { 512 } else { 8192 }
                || text
                    .chars()
                    .any(|c| c.is_control() && (search || !matches!(c, '\n' | '\t')))
            {
                return Err(invalid());
            }
            return Ok(if self.input.as_deref() == Some("search") {
                "\x7f".repeat(self.input_value.chars().count()) + text
            } else {
                let clear = crate::session_chat_send::build_agent_tui_clear_input_for_text(
                    &self.input_value,
                )
                .replace('\u{15}', "\x1b[117;5u")
                .replace('\u{b}', "\x1b[107;5u");
                format!("{clear}\x1b[200~{text}\x1b[201~\r")
            });
        }
        if action == "key" && self.input.as_deref() == Some("key") {
            let key = params
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(invalid)?;
            let modifiers = params
                .get("keyModifiers")
                .and_then(Value::as_u64)
                .filter(|m| *m < 16)
                .ok_or_else(invalid)?
                + 1;
            let code = match key {
                "Enter" => 13,
                "Tab" => 9,
                "Backspace" => 127,
                "Escape" => 27,
                "ArrowUp" | "ArrowDown" | "ArrowRight" | "ArrowLeft" | "Home" | "End" => {
                    let suffix = match key {
                        "ArrowUp" => 'A',
                        "ArrowDown" => 'B',
                        "ArrowRight" => 'C',
                        "ArrowLeft" => 'D',
                        "Home" => 'H',
                        _ => 'F',
                    };
                    return Ok(format!("\x1b[1;{modifiers}{suffix}"));
                }
                "Insert" | "Delete" | "PageUp" | "PageDown" => {
                    let code = match key {
                        "Insert" => 2,
                        "Delete" => 3,
                        "PageUp" => 5,
                        _ => 6,
                    };
                    return Ok(format!("\x1b[{code};{modifiers}~"));
                }
                "F1" | "F2" | "F3" | "F4" => {
                    let suffix = match key {
                        "F1" => 'P',
                        "F2" => 'Q',
                        "F3" => 'R',
                        _ => 'S',
                    };
                    return Ok(format!("\x1b[1;{modifiers}{suffix}"));
                }
                "F5" | "F6" | "F7" | "F8" | "F9" | "F10" | "F11" | "F12" => {
                    let code = match key {
                        "F5" => 15,
                        "F6" => 17,
                        "F7" => 18,
                        "F8" => 19,
                        "F9" => 20,
                        "F10" => 21,
                        "F11" => 23,
                        _ => 24,
                    };
                    return Ok(format!("\x1b[{code};{modifiers}~"));
                }
                _ if key.chars().count() == 1 => key.chars().next().ok_or_else(invalid)? as u32,
                _ => return Err(invalid()),
            };
            return Ok(format!("\x1b[{code};{modifiers}u"));
        }
        if !self.actions.iter().any(|a| a == action) {
            return Err(invalid());
        }
        Ok(match action {
            "up" => "\x1b[A",
            "down" => "\x1b[B",
            "left" => "\x1b[D",
            "right" => "\x1b[C",
            "tab" => "\t",
            "toggle" => " ",
            "confirm" => "\r",
            "pageUp" => "\x1b[5~",
            "pageDown" => "\x1b[6~",
            "home" => "\x1b[H",
            "end" => "\x1b[F",
            "cancel" if self.footer.contains("q to quit") => "q",
            "cancel" => SESSION_CHAT_INTERRUPT,
            _ => return Err(invalid()),
        }
        .to_string())
    }
}

pub(crate) async fn answer_codex_dialog(
    target: &SessionChatSendTarget,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let stale = || DomainStateError {
        code: "invalidState",
        message: "Codex's dialog changed. Review the current choices and try again.".to_string(),
    };
    let screen = capture_session_terminal_text(&target.zmx_name)
        .await
        .ok_or_else(stale)?;
    let dialog = detect_codex_dialog(&screen).ok_or_else(stale)?;
    if params.get("dialogId").and_then(Value::as_str) != Some(dialog.id.as_str()) {
        return Err(stale());
    }
    let payload = dialog.payload(params)?;
    execute_session_chat_send(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        "session-chat-dialog",
        vec![
            SessionChatSendStep::BeginCodexCommandOutput {
                command: dialog.title,
            },
            SessionChatSendStep::VerifyTerminalDialog {
                agent: "codex".to_string(),
                id: dialog.id.clone(),
            },
            SessionChatSendStep::Write(payload),
            SessionChatSendStep::SleepMs(150),
            SessionChatSendStep::FinishCodexCommandOutput,
        ],
    )
    .await
    .map_err(|error| DomainStateError {
        code: "invalidState",
        message: error.message,
    })?;
    let completes = params.contains_key("choiceIndex")
        || matches!(
            params.get("dialogAction").and_then(Value::as_str),
            Some("submit" | "confirm" | "cancel")
        );
    if completes
        && capture_session_terminal_text(&target.zmx_name)
            .await
            .and_then(|screen| detect_codex_dialog(&screen))
            .is_some_and(|current| current.id == dialog.id)
    {
        return Err(DomainStateError {
            code: "invalidState",
            message: "Codex kept this dialog open. Review its message and try again.".to_string(),
        });
    }
    Ok(json!({"queued": true}))
}

/// Local commands print results outside Codex's conversation JSONL.
pub(crate) fn command_has_local_output(text: &str) -> bool {
    matches!(
        text.split_whitespace().next().unwrap_or_default(),
        "/status"
            | "/pwd"
            | "/cwd"
            | "/cd"
            | "/usage"
            | "/ps"
            | "/stop"
            | "/clean"
            | "/mcp"
            | "/ide"
            | "/approve"
            | "/diff"
            | "/debug-config"
            | "/copy"
            | "/export"
            | "/raw"
            | "/vim"
            | "/personality"
            | "/goal"
            | "/plan"
            | "/model"
            | "/skills"
            | "/hooks"
            | "/apps"
            | "/plugins"
            | "/memories"
            | "/keymap"
            | "/theme"
            | "/title"
            | "/statusline"
            | "/pets"
            | "/pet"
            | "/permissions"
            | "/experimental"
            | "/import"
            | "/rename"
            | "/setup-default-sandbox"
            | "/sandbox-add-read-dir"
            | "/rollout"
            | "/agents"
            | "/subagents"
            | "/resume"
            | "/review"
            | "/feedback"
            | "/new"
            | "/clear"
            | "/fork"
            | "/init"
            | "/compact"
            | "/recap"
            | "/side"
            | "/btw"
            | "/app"
            | "/archive"
            | "/delete"
            | "/fast"
            | "/effort"
    )
}

fn history_without_composer(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text
        .lines()
        .map(|line| {
            normalize_spaces(&strip_ansi_sgr(line))
                .trim_end()
                .to_string()
        })
        .collect();
    if let Some(dialog) = detect_codex_dialog(text) {
        if let Some(heading) = lines
            .iter()
            .rposition(|line| clean(left_column(line)) == dialog.title)
        {
            lines.truncate(heading);
        }
    } else if let Some(composer) = lines.iter().rposition(|line| {
        let line = clean(line);
        line.starts_with('›') && row(line).is_none()
    }) {
        lines.truncate(composer);
    }
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    lines
}

/// Only newly printed command output, excluding the previous conversation and
/// the returned composer. A live dialog has its own card instead.
pub fn codex_command_output(before: &str, after: &str) -> Option<String> {
    if detect_codex_dialog(after).is_some() {
        return None;
    }
    let before = history_without_composer(before);
    let after = history_without_composer(after);
    // Codex repaints old /status cells with refreshed rate-limit timestamps.
    // Locate the transcript's ending text, rather than requiring its entire
    // historical prefix to remain byte-identical after a local command.
    let boundary = (before.len().saturating_sub(16)..before.len()).find_map(|start| {
        let anchor = &before[start..];
        if !anchor
            .iter()
            .any(|line| line.chars().any(char::is_alphanumeric))
            || after.len() < anchor.len()
        {
            return None;
        }
        after
            .windows(anchor.len())
            .enumerate()
            .filter(|(_, window)| *window == anchor)
            .min_by_key(|(index, _)| index.abs_diff(start))
            .map(|(index, _)| index + anchor.len())
    })?;
    let output = after[boundary..].join("\n").trim().to_string();
    // A submitted terminal prompt belongs to the JSONL transcript, not this local command.
    let output = output
        .lines()
        .take_while(|line| !clean(line).starts_with('›'))
        .collect::<Vec<_>>()
        .join("\n");
    if output.trim().is_empty() {
        None
    } else {
        Some(output.trim().chars().take(24_000).collect())
    }
}
