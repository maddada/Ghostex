//! What Focus Chat Box (Shift+Esc), Copy Last Code Block (Cmd+Shift+;) and Copy Last Reply
//! (Cmd+Shift+C, Mac only) do to one chat. The keys themselves are app hotkeys routed to the chat
//! of the focused session (`focused_chat_hotkeys.rs`), because the chat's key context only covers
//! its chat box: bound there, Shift+Esc could only fire when the chat box already had focus.
//! SEE-ALSO: packages/shared/ghostex-hotkeys.ts (CDXC:Hotkeys 2026-09-25 on createAgentSession).

use super::state::NativeChatView;
use gpui::{Context, Window};
use serde_json::Value;

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
    pub(crate) fn focus_composer_from_hotkey(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_requested = true;
        self.ensure_input(window, cx);
        cx.notify();
    }

    /// Copies the agent's last reply, or with `code_block` the last fenced code block it wrote.
    /// Returns false when the chat has nothing to copy.
    pub(crate) fn copy_last_reply_from_hotkey(
        &mut self,
        code_block: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let final_ids = self.snapshot["finalIds"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let replies = replies_newest_first(&self.items, &final_ids);
        let text = if code_block {
            replies
                .iter()
                .find_map(|reply| last_fenced_code_block(reply))
        } else {
            replies.first().map(|reply| reply.to_string())
        };
        let Some(text) = text else {
            return false;
        };
        crate::app::helpers::gpui_copy_to_clipboard(gpui::ClipboardItem::new_string(text), cx);
        true
    }
}
