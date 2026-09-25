//! The chat's own hotkeys beside Scroll to Bottom: Focus Chat Box (Shift+Esc), Copy Last Code
//! Block (Cmd+Shift+;) and Copy Last Reply (Cmd+Shift+C, Mac only). Like `scroll_bottom.rs` they
//! are bound in the chat's key context, so they answer only while this chat has focus.
//! SEE-ALSO: packages/shared/ghostex-hotkeys.ts (CDXC:Hotkeys 2026-09-25 on createAgentSession).

use super::state::NativeChatView;
use crate::app::hotkeys::{
    GPUI_DEFAULT_GHOSTEX_HOTKEYS, gpui_keystroke_from_shared_hotkey,
    gpui_migrated_hotkey_for_action, gpui_platform_hotkey_for_action,
};
use gpui::{App, Context, EntityInputHandler as _, Focusable as _, KeyBinding, Window};
use serde_json::Value;
use std::collections::HashSet;

const CHAT_HOTKEY_ACTION_IDS: [&str; 3] = [
    "focusChatComposer",
    "copyLastChatCodeBlock",
    "copyLastChatReply",
];

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(super) struct RunChatHotkey {
    action_id: String,
    shortcut: String,
}

#[derive(Default)]
struct RegisteredChatHotkeys(HashSet<String>);
impl gpui::Global for RegisteredChatHotkeys {}

/// The keystroke the user's settings give `action_id`, through the same defaults and migrations
/// as the app-wide bindings.
fn shortcut(action_id: &str) -> Option<String> {
    let (_, default) = GPUI_DEFAULT_GHOSTEX_HOTKEYS
        .iter()
        .find(|(id, _)| *id == action_id)?;
    let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
    let key = settings
        .object()
        .get("hotkeys")
        .and_then(|hotkeys| hotkeys.get(action_id))
        .and_then(Value::as_str);
    let key = gpui_migrated_hotkey_for_action(action_id, key.unwrap_or(default), default);
    let key = gpui_platform_hotkey_for_action(action_id, key);
    if key.trim().is_empty() {
        return None;
    }
    gpui_keystroke_from_shared_hotkey(key)
}

pub(super) fn register(cx: &mut App) {
    if !cx.has_global::<RegisteredChatHotkeys>() {
        cx.set_global(RegisteredChatHotkeys::default());
    }
    for action_id in CHAT_HOTKEY_ACTION_IDS {
        let Some(shortcut) = shortcut(action_id) else {
            continue;
        };
        if cx
            .global_mut::<RegisteredChatHotkeys>()
            .0
            .insert(format!("{action_id}:{shortcut}"))
        {
            cx.bind_keys([KeyBinding::new(
                &shortcut,
                RunChatHotkey {
                    action_id: action_id.to_string(),
                    shortcut: shortcut.clone(),
                },
                Some("NativeChat"),
            )]);
        }
    }
}

/// Every message the transcript document holds under `value`, in document order.
fn collect_messages<'a>(value: &'a Value, out: &mut Vec<&'a Value>) {
    match value {
        Value::Object(map) if map.contains_key("role") && map.contains_key("copyText") => {
            out.push(value);
        }
        Value::Object(map) => map.values().for_each(|child| collect_messages(child, out)),
        Value::Array(items) => items.iter().for_each(|item| collect_messages(item, out)),
        _ => {}
    }
}

/// The agent's replies, newest first: the rows whose Copy button copies `copyText`. Within one
/// row, the turn's final reply comes ahead of its work messages.
fn replies_newest_first<'a>(items: &'a [Value], final_ids: &[Value]) -> Vec<&'a str> {
    let mut replies = Vec::new();
    for item in items.iter().rev() {
        let mut messages = Vec::new();
        collect_messages(item, &mut messages);
        messages.retain(|message| {
            message["role"] == "assistant"
                && message["actionContent"]["copyable"] == true
                && message["copyText"]
                    .as_str()
                    .is_some_and(|text| !text.trim().is_empty())
        });
        messages.sort_by_key(|message| !final_ids.contains(&message["id"]));
        replies.extend(
            messages
                .into_iter()
                .filter_map(|message| message["copyText"].as_str()),
        );
    }
    replies
}

/// The body of the last fenced code block (``` or ~~~) in `markdown`; an unclosed fence at the end
/// counts, since a reply can end mid-block.
fn last_fenced_code_block(markdown: &str) -> Option<String> {
    fn fence(line: &str) -> Option<(char, usize)> {
        let mark = line
            .chars()
            .next()
            .filter(|mark| matches!(mark, '`' | '~'))?;
        let length = line.chars().take_while(|char| *char == mark).count();
        (length >= 3).then_some((mark, length))
    }
    let mut last = None;
    let mut open: Option<(char, usize, Vec<&str>)> = None;
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        let fence_indent = line.len() - trimmed.len() <= 3;
        match open.as_mut() {
            Some((mark, length, body)) => {
                let closes = fence_indent
                    && fence(trimmed).is_some_and(|(close_mark, close_length)| {
                        close_mark == *mark
                            && close_length >= *length
                            && trimmed[close_length..].trim().is_empty()
                    });
                if closes {
                    last = Some(body.join("\n"));
                    open = None;
                } else {
                    body.push(line);
                }
            }
            None => {
                if let Some((mark, length)) = fence(trimmed).filter(|_| fence_indent) {
                    open = Some((mark, length, Vec::new()));
                }
            }
        }
    }
    if let Some((_, _, body)) = open.filter(|(_, _, body)| !body.is_empty()) {
        last = Some(body.join("\n"));
    }
    last
}

impl NativeChatView {
    pub(super) fn run_chat_hotkey(
        &mut self,
        action: &RunChatHotkey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A rebound chord leaves the old binding registered; only the current one answers.
        if shortcut(&action.action_id).as_deref() != Some(action.shortcut.as_str()) {
            cx.propagate();
            return;
        }
        // An IME composition keeps its keys.
        for input in self
            .input
            .iter()
            .chain(self.answer_input.iter().map(|(_, input)| input))
            .chain(self.async_answer_input.iter().map(|(_, input)| input))
        {
            if input.read(cx).focus_handle(cx).is_focused(window)
                && input.update(cx, |input, cx| {
                    input.marked_text_range(window, cx).is_some()
                })
            {
                cx.propagate();
                return;
            }
        }
        match action.action_id.as_str() {
            "focusChatComposer" => {
                self.focus_requested = true;
                self.ensure_input(window, cx);
                cx.notify();
            }
            "copyLastChatReply" | "copyLastChatCodeBlock" => {
                let final_ids = self.snapshot["finalIds"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let replies = replies_newest_first(&self.items, &final_ids);
                let text = if action.action_id == "copyLastChatReply" {
                    replies.first().map(|reply| reply.to_string())
                } else {
                    replies
                        .iter()
                        .find_map(|reply| last_fenced_code_block(reply))
                };
                if let Some(text) = text {
                    crate::app::helpers::gpui_copy_to_clipboard(
                        gpui::ClipboardItem::new_string(text),
                        cx,
                    );
                }
            }
            _ => cx.propagate(),
        }
    }
}
