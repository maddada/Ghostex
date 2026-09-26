//! Reading an agent-owned dialog well enough to answer it from the chat.
//!
//! Port of `packages/shared/session-chat-presentation/terminal-prompts.ts`. The labels are
//! matched off the dialog's own title and footer, because the agent CLIs describe their keys in
//! prose and there is no structured field to read.

use serde_json::{json, Map, Value};

use crate::questions::model::{TerminalDialog, TerminalNotice, TerminalNoticeAction};
use crate::questions::terminal_dialog_copy::terminal_dialog_copy;

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
    let visible_actions = dialog
        .actions
        .iter()
        .filter(|action| dialog.input.as_deref() != Some("text") || action.as_str() != "confirm");
    let cancel_label = if dialog.footer.to_lowercase().contains("esc to clear") {
        "Clear / Back"
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
            } else if action == "confirm" && dialog.footer.contains("set as default") {
                "Set as default".to_string()
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
        "copy": terminal_dialog_copy(dialog),
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
    None
}

/// `terminalNoticeActionShortcutEligible`: whether the primary shortcut may trigger the action.
/// Trust and Remember is deliberately click-only: a shortcut meant to accept one prompt must not
/// also change what happens on every later one.
pub fn terminal_notice_action_shortcut_eligible(action: &TerminalNoticeAction) -> bool {
    action.kind != "trustAndRemember"
}
