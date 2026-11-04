//! Reading an agent-owned dialog well enough to answer it from the chat.
//!
//! Port of `packages/shared/session-chat-presentation/terminal-prompts.ts`. The labels are
//! matched off the dialog's own title and footer, because the agent CLIs describe their keys in
//! prose and there is no structured field to read.

use serde_json::{json, Map, Value};

use crate::questions::model::{TerminalDialog, TerminalNotice, TerminalNoticeAction};
use crate::questions::terminal_dialog_copy::terminal_dialog_copy;
use crate::transcript::line_breaks::{agent_line_breaks, AgentLineBreaks};

/// The button copy for one of the dialog's named actions.
fn action_label(action: &str) -> Option<&'static str> {
    Some(match action {
        "up" => "↑ Previous",
        "down" => "↓ Next",
        "left" => "← Left",
        "right" => "Right →",
        "pageUp" => "Page up",
        "pageDown" => "Page down",
        "home" => "First",
        "end" => "Last",
        "tab" => "Next field",
        "toggle" => "Toggle selected",
        "confirm" => "Confirm",
        "cancel" => "Back / Cancel",
        "sessionOnly" => "Use for this session",
        "sort" => "Change sort",
        "reset" => "Reset to auto",
        "day" => "Day view",
        "week" => "Week view",
        "projects" => "Toggle all projects",
        "branch" => "Toggle current branch",
        _ => return None,
    })
}

/// How the chat draws a live terminal dialog: what the submit and cancel buttons say, whether the
/// text field is one line or many, and which actions become buttons.
pub fn terminal_dialog_presentation(dialog: &TerminalDialog) -> Value {
    let submit_label = if dialog.title == "Ready to code?" {
        "Request changes"
    } else if dialog.title.starts_with("Tell us more (") {
        "Send feedback"
    } else if dialog.title == "Custom review instructions" {
        "Start review"
    } else if dialog.title == "Add marketplace" {
        "Add marketplace"
    } else if dialog.title == crate::questions::terminal_dialog_copy::CLAUDE_SIGN_IN_TITLE {
        "Sign in"
    } else if dialog.footer.contains("Enter to continue") {
        "Continue"
    } else if dialog.footer.contains("Enter to add") {
        "Add directory"
    } else if dialog.footer.contains("submit") {
        "Submit"
    } else {
        "Save"
    };
    let multiline_input = dialog.input.as_deref() == Some("text")
        && (dialog.title.starts_with("Tell us more (")
            || dialog.title == "Custom review instructions"
            || dialog.title == "Submit feedback / bug report");
    /*
    CDXC:SessionChat 2026-09-08 DECISION:
    User: always show the exit action at the bottom beside the other buttons for /usage and similar
    agent dialogs, so leaving them never requires switching to the terminal.
    */
    let side_question = dialog
        .side_question
        .as_ref()
        .map(side_question_presentation);
    let blocks = dialog.blocks.as_deref().unwrap_or_default();
    let tab_strip = blocks.iter().find(|block| block["type"] == "tabs");
    let copy = terminal_dialog_copy(dialog);
    let device_code = copy.as_ref().is_some_and(|copy| {
        copy["title"] == crate::questions::terminal_dialog_copy::CODEX_DEVICE_CODE_TITLE
    });
    // Rows the card numbers (dialog_rows.rs) are its buttons: Confirm and Cancel under them only
    // repeated a row (CDXC:SessionChat 2026-10-27 on `highlighted_row`).
    let numbered_rows = dialog.picker.is_none()
        && (!dialog.rows.is_empty()
            || (dialog.actions.iter().any(|action| action == "yes")
                && dialog.actions.iter().any(|action| action == "no")));
    let visible_actions = dialog.actions.iter().filter(|action| {
        let action = action.as_str();
        if numbered_rows && side_question.is_none() && matches!(action, "confirm" | "cancel") {
            return false;
        }
        if device_code && matches!(action, "up" | "down") {
            return false;
        }
        if dialog.input.as_deref() == Some("text") && action == "confirm" {
            return false;
        }
        // The side question card offers exactly what its panel's hint line does: fork and close.
        if let Some(card) = &side_question {
            return action == "cancel" || (action == "fork" && card["canFork"] == true);
        }
        // Tabs are clicked directly, and rows are numbered choices (dialog_rows.rs). Otherwise the
        // server offers up/down only when a `❯` row or an arrow hint is on screen, so a panel with
        // blocks keeps them.
        !(tab_strip.is_some() && matches!(action, "left" | "right"))
            // A list scrolled past its visible rows ("↓ more below") keeps them to reach the rest.
            && !(!dialog.rows.is_empty()
                && !dialog.body.contains("more below")
                && !dialog.body.contains("more above")
                && matches!(action, "up" | "down"))
            // A y/n prompt's keys are its Yes and No rows (dialog_rows.rs).
            && !matches!(action, "yes" | "no")
    });
    let highlighted = highlighted_row(dialog);
    let cancel_label = if side_question.is_some() || !blocks.is_empty() {
        "Close"
    } else if dialog.footer.to_lowercase().contains("esc to clear") {
        "Clear / Back"
    } else if dialog.footer.contains("esc not now") {
        "Not now"
    } else if dialog.footer.contains("go back") {
        "Back"
    } else if dialog.footer.contains("close") || dialog.footer.contains("q to quit") {
        "Close"
    } else {
        "Cancel"
    };
    let actions: Vec<Value> = visible_actions
        .map(|action| {
            let label = if action == "cancel" {
                cancel_label.to_string()
            } else if action == "fork" {
                "Fork".to_string()
            } else if matches!(action.as_str(), "left" | "right")
                && dialog.footer.contains("change size")
            {
                // Claude's autocompact offer: ←/→ step the size its second row offers.
                if action == "left" {
                    "← Smaller"
                } else {
                    "Larger →"
                }
                .to_string()
            } else if action == "confirm" && dialog.footer.contains("set as default") {
                "Set as default".to_string()
            } else if action == "confirm" && dialog.footer.contains("Enter to retry") {
                "Retry".to_string()
            } else if action == "confirm" && dialog.footer.contains("Enter to continue") {
                "Continue".to_string()
            } else if let Some(row) = highlighted.as_deref().filter(|_| action == "confirm") {
                row.to_string()
            } else {
                action_label(action)
                    .map(str::to_string)
                    .unwrap_or_else(|| action.clone())
            };
            json!({ "action": action, "label": label })
        })
        .collect();
    json!({
        "submitLabel": submit_label,
        "multilineInput": multiline_input,
        "cancelLabel": cancel_label,
        "actions": actions,
        "copy": copy,
        "sideQuestion": side_question,
        "blocks": (!blocks.is_empty()).then_some(blocks),
        "title": tab_strip.and_then(|strip| strip["title"].as_str()),
        "picker": crate::questions::terminal_picker::picker_presentation(dialog),
    })
}

/*
CDXC:SessionChat 2026-10-15 DECISION:
User picked: show ↑ Previous / ↓ Next on every Claude panel the server offers them for (blocks or not; a card whose rows are numbered choices drops them, per the option D decision in dialog_rows.rs). Labelling Confirm with the highlighted row's text is superseded below for cards with rows.
CDXC:SessionChat 2026-10-27 DECISION:
User, on the Codex approval card ("bruv please let's not show the hotkey multiple places"), answered Q680 "numbers only", Q681 "remove both" and Q682 "lines above the prompt". This replaces the 2026-10-15 rule "label Confirm with the highlighted row's text" wherever the card numbers the rows: the rows are the buttons, so the Confirm and Cancel buttons under them go, the number badge is each row's only hint (the agent's own "(y)", "(p)", "(esc)" and the ⌘Enter and Esc keycaps go), and ⌘Enter and Esc keep working unlabelled. A panel whose choice is not a row (a `❯` line read as blocks) keeps its Confirm named after that line, since nothing else answers it. The open terminal output shows only the lines above the prompt (notice_tail.rs). User (Q687, "Yes, phone only, only those dialogs"): the phone, which has no Esc key, keeps a Cancel button under a numbered dialog that has no "No" row (apps/mobile/app/src/chat/native/cards/dialogCancel.ts); every other card stays numbers only.
SEE-ALSO: packages/gx-chat-core/src/questions/notice_choices.rs (`without_key_hint`), apps/desktop/src/app/native_chat/notice.rs, apps/desktop/src/app/native_chat/keyboard.rs, apps/mobile/app/src/chat/native/cards/NoticeCard.tsx.
*/
/// What Enter would pick: the selected row, or the `❯` line of a panel read as blocks, without
/// its mark, number, description or stats, cut to a button's length.
fn highlighted_row(dialog: &TerminalDialog) -> Option<String> {
    let raw = match dialog.rows.iter().find(|row| row.selected) {
        Some(row) => row.label.clone(),
        None if dialog.input.as_deref() != Some("text") => {
            let line = dialog.body.lines().rev().find_map(|line| {
                line.trim_start()
                    .strip_prefix('❯')
                    .map(|rest| rest.trim().to_string())
            })?;
            let line = line
                .split_once(". ")
                .filter(|(number, _)| number.parse::<u32>().is_ok())
                .map_or(line.as_str(), |(_, rest)| rest)
                .to_string();
            line.split("  ").next().unwrap_or_default().to_string()
        }
        None => return None,
    };
    let label = raw
        .trim()
        .trim_start_matches(['✔', '✓', '◯', '◉', '●', '○', '☐', '☒'])
        .trim();
    let label = label.split(" · ").next().unwrap_or_default().trim();
    if label.is_empty() {
        return None;
    }
    const MAX: usize = 48;
    if label.chars().count() <= MAX {
        return Some(label.to_string());
    }
    // A closing "(project scope)" is what tells the rows apart, so it survives the cut.
    let tail = label
        .ends_with(')')
        .then(|| label.rfind(" (").map(|at| &label[at + 1..]))
        .flatten()
        .filter(|tail| tail.chars().count() < MAX / 2)
        .unwrap_or_default();
    let head: String = label.chars().take(MAX - 1 - tail.chars().count()).collect();
    Some(format!(
        "{}…{}{tail}",
        head.trim_end(),
        if tail.is_empty() { "" } else { " " }
    ))
}

/// The side question card: the question, the answer as Markdown for the renderer and as the raw
/// text Copy puts on the clipboard, and whether Claude is still answering.
fn side_question_presentation(side_question: &Value) -> Value {
    let answer = side_question["answer"].as_str().unwrap_or_default();
    let complete = side_question["complete"] == true;
    let markdown = crate::transcript::native_markdown::native_markdown(
        &agent_line_breaks(answer, AgentLineBreaks::Every),
        false,
    );
    json!({
        "question": side_question["question"],
        "questionTruncated": side_question["questionTruncated"] == true,
        "answer": answer,
        "answerReferences": crate::transcript::markdown_links::markdown_references(&markdown),
        "answerMarkdown": markdown,
        "answering": side_question["answering"] == true,
        "complete": complete,
        "canFork": complete && side_question["canFork"] == true,
    })
}

/// The answer that picking row `choice_index` sends: a dialog row when the notice carries a live
/// dialog, a plain screen choice otherwise.
pub fn terminal_notice_choice_answer(notice: Option<&TerminalNotice>, choice_index: i64) -> Value {
    match notice.and_then(|notice| notice.dialog.as_ref()) {
        Some(dialog) => json!({
            "choiceIndex": choice_index,
            "kind": "terminalDialog",
            "dialogId": dialog.id,
        }),
        None => json!({ "choiceIndex": choice_index, "kind": "terminalChoice" }),
    }
}

/// The answer a notice action sends, or `None` for an action the chat cannot answer itself (the
/// switch-to-terminal button, or a `sendKeys` action with nothing to send).
pub fn terminal_notice_action_answer(
    notice: &TerminalNotice,
    action: &TerminalNoticeAction,
) -> Option<Value> {
    if action.kind == "recoverCodexConversation" {
        if let Some(lock) = notice.conversation_lock.as_ref() {
            let mut answer = Map::new();
            answer.insert("kind".to_string(), json!("recoverCodexConversation"));
            answer.insert("conversationLock".to_string(), lock.clone());
            return Some(Value::Object(answer));
        }
    }
    if action.kind == "sendKeys" {
        if let Some(send) = action.send.as_ref() {
            return Some(json!({ "kind": "approval", "approvalSend": send }));
        }
    }
    if action.kind == "trustAndRemember" {
        return Some(json!({ "kind": "trustAndRemember" }));
    }
    // gxserver tells the restarts apart by the action id: Sign in on the card of an agent whose
    // account has no saved login rides this answer (server/src/session_chat_agent_restart.rs).
    if action.kind == "restartAgent" {
        return Some(json!({ "kind": "restartAgent", "actionId": action.id }));
    }
    None
}

/// `terminalNoticeActionShortcutEligible`: whether the primary shortcut may trigger the action.
/// Trust and Remember, Sign in and Reconnect are deliberately click-only: a shortcut meant to
/// accept one prompt must not also change what happens on every later one, or start a sign-in.
pub fn terminal_notice_action_shortcut_eligible(action: &TerminalNoticeAction) -> bool {
    action.kind != "trustAndRemember"
        && !matches!(action.id.as_str(), "signInAccount" | "reconnectAccount")
}
