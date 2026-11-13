//! The card under a message that has not reached the agent yet: what became of it, in one
//! sentence, with the one or two things to do about it as buttons and every other action in its ⋯
//! menu. The document's `deliveryCards` holds one per waiting or failed message, by the
//! `promptId` of its `startupDelivery`; a renderer draws it under the bubble.
//!
//! CDXC:SessionChat 2026-11-13 DECISION:
//! User, of the failed send's loose grey line under the bubble with bare Retry and Remove links: "these look so out of place here, do inline html for a better way to show errors like this in all cases". From the chat errors page they picked Q2941 B, the Dictate card's shape right under the bubble, adding "I don't see 'Remove', which was an option, please don't lose options. And you can do a 3 dots menu if you need it when there are too many options": every action a message had stays reachable, the main one or two as buttons and the rest in the card's ⋯ menu. Q2942: a failed send shows in one place, its message; the separate "Not sent" notice card is not drawn while its message carries this card, and its Retry, Fix it and Start fresh are this card's. Q2943: Edit and Remove are in the ⋯ menu. Q2944: the level shows only in the icon's colour. Q2945: a message retried that fails again says "Still not sent" and leads with Fix it. One wording everywhere: "Not sent".
//! SEE-ALSO: apps/desktop/src/app/native_chat/startup_delivery.rs and apps/mobile/app/src/chat/native/transcript/SystemRows.tsx (they draw it), questions/document.rs (the notice it takes over), composer/actions.rs (Retry is remembered, Remove also closes the notice).

use std::collections::BTreeMap;

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole, StartupDelivery};
use serde_json::{json, Value};

use crate::questions::model::TerminalNotice;
use crate::questions::terminal_prompts::terminal_notice_action_answer;
use crate::state::ChatState;

/// How many retried messages the core remembers, newest last.
pub const RETRIED_LIMIT: usize = 32;

/// Remembers that the user asked `prompt_id` to go again, so a second failure says so.
pub fn note_retry(state: &mut ChatState, prompt_id: &str) {
    if prompt_id.is_empty() {
        return;
    }
    let retried = &mut state.composer.retried_prompt_ids;
    retried.retain(|id| id != prompt_id);
    retried.push(prompt_id.to_string());
    if retried.len() > RETRIED_LIMIT {
        let excess = retried.len() - RETRIED_LIMIT;
        retried.drain(..excess);
    }
}

/// The failed message the session's notice card stands for, when gxserver sent one.
pub fn send_failure_prompt_id(notice: &TerminalNotice) -> Option<&str> {
    notice
        .send_failure
        .as_ref()
        .and_then(|failure| failure.get("promptId"))
        .and_then(Value::as_str)
}

/// One button or menu row.
#[derive(Clone)]
struct Item {
    id: &'static str,
    label: &'static str,
    icon: Option<&'static str>,
    /// What the renderer sends to the core.
    command: Option<Value>,
}

impl Item {
    fn to_value(&self, primary: bool) -> Value {
        let mut value = json!({ "id": self.id, "label": self.label });
        if let Some(icon) = self.icon {
            value["icon"] = json!(icon);
        }
        if let Some(command) = &self.command {
            value["command"] = command.clone();
        }
        if primary {
            value["primary"] = json!(true);
        }
        value
    }
}

fn terminal_view() -> Item {
    Item {
        id: "terminalView",
        label: "Terminal view",
        icon: Some("terminal-2"),
        // The core asks the host to show the session's terminal (menus/actions.rs).
        command: Some(json!({ "type": "switchToTerminal" })),
    }
}

fn remove(prompt_id: &str) -> Item {
    Item {
        id: "remove",
        label: "Remove",
        icon: Some("trash"),
        command: Some(json!({ "type": "removeQueue", "promptId": prompt_id })),
    }
}

/// Edit puts the words back in the chat box: a row the chat itself could not send has its own
/// action, and a row in gxserver's queue moves back the queue's way (`edit: true`).
fn edit(prompt_id: &str) -> Item {
    Item {
        id: "edit",
        label: "Edit",
        icon: Some("pencil"),
        command: Some(if crate::composer::delivery::is_unsent(prompt_id) {
            json!({ "type": "editUnsent", "promptId": prompt_id })
        } else {
            json!({ "type": "removeQueue", "promptId": prompt_id, "edit": true })
        }),
    }
}

/// The text a message shows, for telling a plain message from an answers envelope or a message
/// from another agent, which draw their own cards.
fn plain_user_message(message: &ChatMessage) -> bool {
    if message.role != ChatRole::User {
        return false;
    }
    let text = message
        .blocks
        .iter()
        .filter_map(|block| match block {
            ChatBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    crate::transcript::page_answers::answers_page_id(&text).is_none()
        && crate::transcript::agent_message::parse_inter_agent_message(&text).is_none()
}

/// The reason, without the instructions gxserver or the chat once wrote into it: the buttons say
/// what to do, the sentence says what happened.
pub fn plain_reason(reason: &str) -> String {
    let mut text = reason.trim();
    for lead in ["Not sent.", "Not sent:", "Not delivered:"] {
        if let Some(rest) = text.strip_prefix(lead) {
            text = rest.trim_start();
        }
    }
    for tail in [
        " Retry, or open Terminal view to check it.",
        " Open Terminal view to check it.",
        " Send it again.",
        " Retry, or Fix it to restart the agent and send it.",
        " Fix it restarts the agent and sends it.",
        " Start fresh starts the agent over and sends it.",
    ] {
        if let Some(rest) = text.strip_suffix(tail) {
            text = rest.trim_end();
        }
    }
    text.to_string()
}

/// What the session's notice offers for a failed message it stands for: Retry, Fix it and Start
/// fresh, as the answers gxserver takes.
struct NoticeActions {
    retry: Option<Item>,
    fix: Option<Item>,
    start_fresh: Option<Item>,
}

fn notice_actions(notice: &TerminalNotice) -> NoticeActions {
    let mut found = NoticeActions {
        retry: None,
        fix: None,
        start_fresh: None,
    };
    for action in notice.actions.iter().flatten() {
        let Some(answer) = terminal_notice_action_answer(notice, action) else {
            continue;
        };
        let command = Some(json!({ "type": "answer", "answer": answer }));
        match action.id.as_str() {
            "retrySend" => {
                found.retry = Some(Item {
                    id: "retry",
                    label: "Retry",
                    icon: Some("refresh"),
                    command,
                })
            }
            "fixSend" => {
                found.fix = Some(Item {
                    id: "fix",
                    label: "Fix it",
                    icon: None,
                    command,
                })
            }
            "startFreshSend" => {
                found.start_fresh = Some(Item {
                    id: "startFresh",
                    label: "Start fresh",
                    icon: None,
                    command,
                })
            }
            _ => {}
        }
    }
    found
}

fn card(
    prompt_id: &str,
    level: &str,
    title: &str,
    sentence: &str,
    buttons: &[Item],
    menu: &[Item],
) -> Value {
    let icon = match level {
        "failed" => "alert-circle",
        "needs" => "clock",
        _ => "loader",
    };
    json!({
        "promptId": prompt_id,
        "level": level,
        "icon": icon,
        "title": title,
        "sentence": sentence,
        "buttons": buttons
            .iter()
            .enumerate()
            .map(|(at, item)| item.to_value(at == 0 && level != "working"))
            .collect::<Vec<_>>(),
        "menu": menu.iter().map(|item| item.to_value(false)).collect::<Vec<_>>(),
    })
}

/// The card for one message's delivery, or `None` while it only needs the spinner beside it.
fn delivery_card(
    delivery: &StartupDelivery,
    notice: Option<&TerminalNotice>,
    retried: bool,
    can_remove: bool,
) -> Option<Value> {
    let prompt_id = delivery.prompt_id.as_str();
    let reason = delivery.wait_reason.as_deref().unwrap_or_default().trim();
    match delivery.state.as_str() {
        "failed" => {
            let sentence = match plain_reason(delivery.error_message.as_deref().unwrap_or("")) {
                text if text.is_empty() => "Ghostex couldn't deliver it.".to_string(),
                text => text,
            };
            let from_notice = notice.map(notice_actions);
            let own_retry = Item {
                id: "retry",
                label: "Retry",
                icon: Some("refresh"),
                command: Some(json!({ "type": "retryQueue", "promptId": prompt_id })),
            };
            let (retry, fix, start_fresh) = match from_notice {
                Some(found) => (found.retry, found.fix, found.start_fresh),
                None => (Some(own_retry), None, None),
            };
            let restart = fix.clone().or(start_fresh.clone());
            // Q2945: after a Retry that failed again, the restart leads.
            let lead = if retried {
                restart.clone().or(retry.clone())
            } else {
                retry.clone().or(restart.clone())
            };
            let second = if retried && restart.is_some() {
                retry.clone()
            } else {
                None
            }
            .unwrap_or_else(terminal_view);
            let mut buttons: Vec<Item> = lead.into_iter().collect();
            buttons.push(second);
            let shown = |item: &Item| buttons.iter().any(|button| button.id == item.id);
            let mut menu: Vec<Item> = [retry, fix, start_fresh, Some(terminal_view())]
                .into_iter()
                .flatten()
                .filter(|item| !shown(item))
                .collect();
            menu.push(edit(prompt_id));
            menu.push(remove(prompt_id));
            let title = if retried {
                "Still not sent"
            } else {
                "Not sent"
            };
            Some(card(prompt_id, "failed", title, &sentence, &buttons, &menu))
        }
        crate::session::constants::DELIVERY_UNCONFIRMED => {
            let sentence = if reason.is_empty() {
                "This left the queue, but the conversation doesn't show it."
            } else {
                reason
            };
            Some(card(
                prompt_id,
                "needs",
                "Not confirmed",
                sentence,
                &[terminal_view()],
                &[remove(prompt_id)],
            ))
        }
        // A message held on this computer while the session's computer is offline
        // (session/computer.rs): Cancel puts it back in the chat box.
        "held" => {
            if reason.starts_with("Sending") {
                return Some(card(prompt_id, "working", "Sending", reason, &[], &[]));
            }
            let cancel = Item {
                id: "cancel",
                label: "Cancel",
                icon: None,
                command: Some(json!({ "type": "removeQueue", "promptId": prompt_id })),
            };
            Some(card(
                prompt_id,
                "needs",
                "Not sent yet",
                reason,
                &[cancel],
                &[],
            ))
        }
        _ if reason.is_empty() => None,
        // Waiting a while, or going again by itself after a failure time can fix.
        state => {
            let queued = state == "queued";
            let title = if queued { "Waiting to send" } else { "Sending" };
            let mut menu = Vec::new();
            if queued && can_remove {
                menu.push(remove(prompt_id));
            }
            if queued {
                menu.push(terminal_view());
            }
            Some(card(prompt_id, "working", title, reason, &[], &menu))
        }
    }
}

/// Every waiting or failed plain message's card, by `promptId`, and the failed message whose
/// notice card one of them now stands for (so the notice is not drawn a second time).
pub fn project(state: &ChatState) -> (BTreeMap<String, Value>, Option<String>) {
    let notice = TerminalNotice::parse(state.session.terminal_notice.as_ref());
    let notice_prompt = notice
        .as_ref()
        .and_then(send_failure_prompt_id)
        .map(str::to_string);
    let can_remove = crate::composer::document::queue(state)
        .capabilities
        .can_remove;
    let mut cards = BTreeMap::new();
    let mut taken_over = None;
    for message in &state.messages.composed {
        let Some(delivery) = message.startup_delivery.as_ref() else {
            continue;
        };
        if !plain_user_message(message) {
            continue;
        }
        let prompt_id = delivery.prompt_id.as_str();
        let stands_for = notice_prompt.as_deref() == Some(prompt_id);
        let retried = state
            .composer
            .retried_prompt_ids
            .iter()
            .any(|id| id == prompt_id);
        let Some(card) = delivery_card(
            delivery,
            notice.as_ref().filter(|_| stands_for),
            retried,
            can_remove,
        ) else {
            continue;
        };
        if stands_for && delivery.state == "failed" {
            taken_over = Some(prompt_id.to_string());
        }
        cards.insert(prompt_id.to_string(), card);
    }
    (cards, taken_over)
}
