//! Claude's live Ink panels, including nested menus and editable fields.

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::domain::DomainStateError;
use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};
use crate::session_chat_send::{
    capture_session_terminal_text, execute_session_chat_send, SessionChatSendStep,
    SessionChatSendTarget, SESSION_CHAT_INTERRUPT,
};
use crate::session_chat_terminal_dialog::{TerminalDialog, TerminalDialogRow};

/// CDXC:AgentScreenDetection 2026-09-05 DECISION:
/// User: drive Claude's commands through zmx and make their interactions usable in chat, as for Codex.
/// Claude's panel boundary survives nested menus and clipped footers; a later composer means the panel is historical.
/// SEE-ALSO: packages/core-ui/chat/session-chat-terminal-dialog.tsx.
pub fn detect_claude_dialog(text: &str) -> Option<TerminalDialog> {
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
        line.chars().count() >= 20 && line.chars().all(|c| c == '▔')
    })? + 1;
    let content = &lines[start..];
    // The composer has no left indent. Selection markers inside Ink panels do.
    if content
        .iter()
        .any(|line| line.starts_with('❯') || line.starts_with("╭─"))
    {
        return None;
    }
    let heading = content
        .iter()
        .position(|line| line.chars().any(|c| c.is_alphanumeric()))?;
    let title = content[heading].trim().to_string();
    if title.len() > 240 || title.starts_with("Question ") {
        return None;
    }
    let remainder = &content[heading + 1..];
    let is_hint = |line: &str| {
        let lower = line.to_ascii_lowercase();
        lower.contains("esc to")
            || lower.contains("esc cancel")
            || lower.contains("enter to")
            || lower.contains(" to search")
            || lower.contains(" to filter")
            || lower.contains(" to sort")
            || lower.contains("←/→")
            || lower.contains("↑/↓")
            || lower.contains("d to day")
    };
    let footer = remainder
        .iter()
        .filter(|line| is_hint(line))
        .map(|line| line.trim())
        .collect::<Vec<_>>()
        .join("\n");
    let lower = footer.to_ascii_lowercase();
    let boxed = remainder.iter().find_map(|line| {
        let line = line.trim();
        line.strip_prefix('│')
            .map(|value| value.trim_end_matches('│').trim())
    });
    let search = boxed.is_some_and(|line| line.starts_with('⌕'));
    let feedback_field = remainder
        .iter()
        .position(|line| line.trim() == "Describe the issue below:")
        .map(|i| {
            remainder[i + 1..]
                .iter()
                .take_while(|line| !is_hint(line))
                .map(|line| line.trim())
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string()
        });
    let plan_feedback = (title == "Ready to code?"
        && remainder
            .iter()
            .any(|line| line.contains("shift+tab to approve with this feedback")))
    .then(|| {
        remainder.iter().rev().find_map(|line| {
            let row = line.trim().strip_prefix("❯ ")?;
            let (_, value) = row.split_once(". ")?;
            (!value.starts_with("Yes,")).then(|| {
                if value == "Tell Claude what to change" {
                    ""
                } else {
                    value
                }
            })
        })
    })
    .flatten();
    let text_field = remainder
        .iter()
        .find_map(|line| line.trim().strip_prefix("> "));
    let input = if search {
        Some("search")
    } else if boxed.is_some()
        || text_field.is_some()
        || feedback_field.is_some()
        || plan_feedback.is_some()
    {
        Some("text")
    } else {
        None
    };
    let field = plan_feedback
        .or(feedback_field.as_deref())
        .or(text_field)
        .or(boxed)
        .unwrap_or_default()
        .trim_start_matches('⌕')
        .trim();
    let input_value = if field.ends_with('…') {
        String::new()
    } else {
        field.to_string()
    };
    let mut rows = Vec::new();
    for line in remainder {
        let line = line.trim();
        let selected = line.starts_with('❯');
        let row = line.trim_start_matches(['❯', '↓', '↑']).trim();
        if let Some((number, label)) = row.split_once(". ") {
            if let Ok(number) = number.parse::<u32>() {
                let (label, description) = label
                    .trim()
                    .split_once("  ")
                    .map(|(label, description)| (label, Some(description.trim().to_string())))
                    .unwrap_or((label.trim(), None));
                rows.push(TerminalDialogRow {
                    number,
                    label: label.to_string(),
                    description,
                    selected,
                });
            }
        }
    }
    // Tabbed/searchable lists keep their full panel: a row click cannot express
    // focus changes, filtering, checkboxes, or partially visible numbered lists.
    let numbered = input.is_none()
        && rows.iter().filter(|r| r.selected).count() == 1
        && !title.contains("   ")
        && !remainder
            .iter()
            .any(|line| line.trim().starts_with(['↓', '↑']))
        && !lower.contains("space");
    if !numbered {
        rows.clear();
    }
    let body = remainder
        .iter()
        .filter(|line| {
            !is_hint(line)
                && (!numbered
                    || !rows.iter().any(|r| {
                        line.trim()
                            .trim_start_matches('❯')
                            .trim()
                            .starts_with(&format!("{}. ", r.number))
                    }))
        })
        .cloned()
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    let mut actions = Vec::new();
    if input != Some("text")
        && (remainder
            .iter()
            .any(|line| line.trim().starts_with(['❯', '↓', '↑']))
            || lower.contains("↑/↓")
            || lower.contains("↓ to")
            || title.starts_with("Help ")
            || title.starts_with("Settings "))
    {
        actions.extend(["up", "down"]);
    }
    if lower.contains("←/→")
        || title.starts_with("Help ")
        || title.starts_with("Settings ")
        || title.starts_with("Plugins ")
    {
        actions.extend(["left", "right"]);
    }
    if lower.contains("tab") {
        actions.push("tab");
    }
    if lower.contains("space") {
        actions.push("toggle");
    }
    if lower.contains("session only") {
        actions.push("sessionOnly");
    }
    if lower.contains("ctrl+a") {
        actions.push("projects");
    }
    if lower.contains("ctrl+b") {
        actions.push("branch");
    }
    if lower.contains("t to sort") {
        actions.push("sort");
    }
    if lower.contains("r reset") {
        actions.push("reset");
    }
    if lower.contains("d to day") {
        actions.extend(["day", "week"]);
    }
    if lower.contains("enter") || !rows.is_empty() {
        actions.push("confirm");
    }
    actions.push("cancel");
    Some(TerminalDialog {
        id: format!("{:x}", Sha256::digest(content.join("\n").as_bytes())),
        title,
        body,
        footer,
        rows,
        input: input.map(str::to_string),
        input_value,
        actions: actions.into_iter().map(str::to_string).collect(),
    })
}

fn invalid() -> DomainStateError {
    DomainStateError {
        code: "invalidParams",
        message: "That action is not offered by this Claude dialog.".to_string(),
    }
}

/// CDXC:AgentScreenDetection 2026-09-05 WHY:
/// Claude 2.1.260 ignored legacy arrows in live zmx menus; explicit unmodified CSI keys moved the highlight correctly.
/// Text editing and submission use separate stdin writes so Ink does not interpret a combined control/paste burst as literal input.
pub(crate) fn claude_dialog_steps(
    dialog: &TerminalDialog,
    params: &Map<String, Value>,
) -> Result<Vec<SessionChatSendStep>, DomainStateError> {
    let mut steps = vec![SessionChatSendStep::VerifyTerminalDialog {
        agent: "claude".to_string(),
        id: dialog.id.clone(),
    }];
    if let Some(index) = params.get("choiceIndex").and_then(Value::as_u64) {
        let index = usize::try_from(index).map_err(|_| invalid())?;
        dialog.rows.get(index).ok_or_else(invalid)?;
        let selected = dialog
            .rows
            .iter()
            .position(|row| row.selected)
            .ok_or_else(invalid)?;
        for _ in 0..index.abs_diff(selected) {
            steps.push(SessionChatSendStep::Write(
                if index > selected {
                    "\x1b[1;1B"
                } else {
                    "\x1b[1;1A"
                }
                .to_string(),
            ));
            steps.push(SessionChatSendStep::SleepMs(80));
        }
        if dialog.rows[index].label != "Tell Claude what to change" {
            steps.push(SessionChatSendStep::Write("\r".to_string()));
        }
    } else {
        let action = params
            .get("dialogAction")
            .and_then(Value::as_str)
            .ok_or_else(invalid)?;
        if (action == "text" && dialog.input.as_deref() == Some("search"))
            || (action == "submit" && dialog.input.as_deref() == Some("text"))
        {
            let text = params
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(invalid)?;
            if text.len() > 8192
                || text.chars().any(|c| {
                    c.is_control() && !(dialog.title == "Submit feedback / bug report" && c == '\n')
                })
            {
                return Err(invalid());
            }
            if dialog.footer.contains("/ to search") {
                steps.push(SessionChatSendStep::Write("/".to_string()));
                steps.push(SessionChatSendStep::SleepMs(100));
            }
            let clear = if dialog.input.as_deref() == Some("search") {
                "\x1b[101;5u\x1b[117;5u".to_string()
            } else {
                crate::session_chat_send::build_agent_tui_clear_input_for_text(&dialog.input_value)
                    .replace('\u{15}', "\x1b[117;5u")
                    .replace('\u{b}', "\x1b[107;5u")
            };
            steps.push(SessionChatSendStep::Write(clear));
            steps.push(SessionChatSendStep::SleepMs(100));
            if !text.is_empty() {
                steps.push(SessionChatSendStep::Write(format!(
                    "\x1b[200~{text}\x1b[201~"
                )));
                steps.push(SessionChatSendStep::SleepMs(150));
            }
            if action == "submit" {
                steps.push(SessionChatSendStep::Write("\r".to_string()));
            }
        } else {
            if !dialog.actions.iter().any(|a| a == action) {
                return Err(invalid());
            }
            let payload = match action {
                "up" => "\x1b[1;1A",
                "down" => "\x1b[1;1B",
                "left" => "\x1b[1;1D",
                "right" => "\x1b[1;1C",
                "tab" => "\t",
                "toggle" => " ",
                "confirm" => "\r",
                "sessionOnly" => "s",
                "projects" => "\x1b[97;5u",
                "branch" => "\x1b[98;5u",
                "sort" => "t",
                "reset" => "r",
                "day" => "d",
                "week" => "w",
                "cancel" => SESSION_CHAT_INTERRUPT,
                _ => return Err(invalid()),
            };
            steps.push(SessionChatSendStep::Write(payload.to_string()));
        }
    }
    steps.push(SessionChatSendStep::SleepMs(250));
    Ok(steps)
}

pub(crate) async fn answer_claude_dialog(
    target: &SessionChatSendTarget,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let stale = || DomainStateError {
        code: "invalidState",
        message: "Claude's dialog changed. Review the current choices and try again.".to_string(),
    };
    let dialog = capture_session_terminal_text(&target.zmx_name)
        .await
        .and_then(|screen| detect_claude_dialog(&screen))
        .ok_or_else(stale)?;
    if params.get("dialogId").and_then(Value::as_str) != Some(dialog.id.as_str()) {
        return Err(stale());
    }
    execute_session_chat_send(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        "session-chat-dialog",
        claude_dialog_steps(&dialog, params)?,
    )
    .await
    .map_err(|error| DomainStateError {
        code: "invalidState",
        message: error.message,
    })?;
    if params.contains_key("choiceIndex")
        && capture_session_terminal_text(&target.zmx_name)
            .await
            .and_then(|screen| detect_claude_dialog(&screen))
            .is_some_and(|current| current.id == dialog.id)
    {
        return Err(DomainStateError {
            code: "invalidState",
            message: "Claude kept this dialog open. Review its message and try again.".to_string(),
        });
    }
    Ok(json!({"queued": true}))
}
