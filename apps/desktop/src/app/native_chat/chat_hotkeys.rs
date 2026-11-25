//! What Focus Chat Box (Shift+Esc), Copy Last Code Block (Cmd+Shift+;) and Copy Last Reply
//! (Cmd+Shift+C, Mac only) do to one chat. The keys themselves are app hotkeys routed to the chat
//! of the focused session (`focused_chat_hotkeys.rs`), because the chat's key context only covers
//! its chat box: bound there, Shift+Esc could only fire when the chat box already had focus.
//! SEE-ALSO: packages/settings-catalog/src/hotkey_definitions.rs (CDXC:Hotkeys 2026-09-25 on createAgentSession).

use super::state::NativeChatView;
use ghostex_gx_chat_core::TranscriptItem;
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
fn replies_newest_first<'a>(items: &'a [TranscriptItem], final_ids: &[Value]) -> Vec<&'a str> {
    let mut replies = Vec::new();
    for item in items.iter().rev() {
        // The row's fields that can hold messages, in the order its JSON listed them: each
        // variant's field order (serde_json's `preserve_order`). A field left out of the JSON
        // when empty holds nothing either way.
        let fields: Vec<&Value> = match item {
            TranscriptItem::Message { message } => vec![message],
            TranscriptItem::Summary {
                user,
                final_message,
                earlier_replies,
                work,
                outcome,
                ..
            } => std::iter::once(user)
                .chain(final_message)
                .chain(earlier_replies)
                .chain(work)
                .chain(outcome)
                .map(|model| &**model)
                .collect(),
            TranscriptItem::CompletedWork {
                stats,
                files_card,
                files,
                deferred,
                work,
                questions,
                sent_messages,
                workflows,
                artifacts,
                final_message,
                ..
            } => stats
                .iter()
                .chain([files_card])
                .chain(files)
                .chain(deferred)
                .chain(work.iter().map(|model| &**model))
                .chain(questions)
                .chain(sent_messages)
                .chain(workflows)
                .chain(artifacts.iter().map(|model| &**model))
                .chain(final_message.iter().map(|model| &**model))
                .collect(),
            TranscriptItem::GroupedWork {
                stats,
                files_card,
                files,
                work,
                turns,
                ..
            } => stats
                .iter()
                .chain([files_card])
                .chain(files)
                .chain(work.iter().map(|model| &**model))
                .chain(turns.iter().flat_map(|turn| {
                    turn.deferred
                        .iter()
                        .chain(&turn.questions)
                        .chain(&turn.sent_messages)
                        .chain(&turn.workflows)
                        .chain(turn.artifacts.iter().map(|model| &**model))
                        .chain(turn.final_message.iter().map(|model| &**model))
                }))
                .collect(),
            TranscriptItem::Unknown(row) => vec![row],
        };
        let mut messages = Vec::new();
        for value in fields {
            collect_messages(value, &mut messages);
        }
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
        self.board.inbox_focus = None;
        self.ensure_input(window, cx);
        // An explicit request: it takes the keyboard even from the find bar or a note, which the
        // queued `focus_requested` path leaves alone.
        if let Some(input) = &self.input {
            gpui::Focusable::focus_handle(input.read(cx), cx).focus(window, cx);
        }
        cx.notify();
    }

    /// Focus Session Inbox hotkey (Cmd+Shift+I / Ctrl+Shift+I by default): expands the Inbox if
    /// folded, moves keyboard focus to its rows, and highlights the first row for triage.
    pub(crate) fn focus_inbox_from_hotkey(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let dock = &self.snapshot["workDock"];
        let has_rows = dock["rows"].as_array().is_some_and(|rows| !rows.is_empty());
        if !has_rows {
            return false;
        }
        if dock["hidden"] == true {
            self.invoke(serde_json::json!({"type": "showWorkDock"}), cx);
        }
        if self.board.dock_error_active {
            self.board.dock_peek_over_error = true;
        }
        let handle = self
            .board
            .inbox_focus_handle
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        self.board.inbox_focus = Some(0);
        handle.focus(window, cx);
        cx.notify();
        true
    }

    /// The Stash Prompt hotkey: what the More actions row does. Answers false where the chat box
    /// offers no stash, so the chord keeps its ordinary meaning.
    pub(crate) fn stash_prompt_from_hotkey(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.composer_control_available("stash") {
            return false;
        }
        self.stash_prompt_or_open_saved(cx);
        true
    }

    /// The Attach File or Folder hotkey: what the chat box's Attach button does. Answers false
    /// where the chat box offers no attaching, so the chord keeps its ordinary meaning.
    pub(crate) fn attach_from_hotkey(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.composer_control_available("attach") {
            return false;
        }
        let position = self.composer_button_bounds.attach.get().origin;
        self.perform_composer_action("attachPath", position, window, cx);
        true
    }

    /// The Agent Actions hotkey: opens or closes the chat box's More actions menu, the chat's
    /// counterpart of the terminal's Agent Actions buttons.
    pub(crate) fn toggle_more_actions_from_hotkey(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let position = self.composer_button_bounds.more_actions.get().origin;
        self.show_actions(position, window, cx);
    }

    /// The Scroll Terminal to Bottom hotkey: what the chat's own jump to the newest message does.
    pub(crate) fn scroll_to_bottom_from_hotkey(&mut self, cx: &mut Context<Self>) {
        self.jump_to_bottom(cx);
    }

    /// The Scroll Terminal to Top hotkey: the mirror of the jump to the newest message, to the
    /// oldest message loaded. `scroll_to` also stops a chat at the bottom from following its tail
    /// straight back (files_links.rs); older pages load from there as scrolling up does.
    pub(crate) fn scroll_to_top_from_hotkey(&mut self, cx: &mut Context<Self>) {
        self.release_answer_hold();
        self.clear_message_hover();
        self.list.scroll_to(gpui::ListOffset {
            item_ix: 0,
            offset_in_item: gpui::px(0.0),
        });
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
