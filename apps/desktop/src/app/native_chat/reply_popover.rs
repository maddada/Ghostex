//! The transcript menu's Reply popover: a small frosted card where the menu was, with a field for
//! the reply and its two actions, Add to prompt (Enter) and Send reply (Cmd+Enter, Ctrl+Enter off
//! a Mac). The row and the draft it leaves are the core's (CDXC:SessionChat 2026-10-29 DECISION on
//! `reply_item` in `packages/gx-chat-core/src/composer/transcript_menu.rs`).
//!
//! CDXC:SessionChat 2026-10-29 WHY:
//! It is a child window like the chat's menus because GPUI blurs only behind a whole window
//! (frosted_overlay_window.rs), and floating cards take the frosted menu fill.

use super::{appearance::ChatAppearance, option_menu, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Bounds, Context, Entity, FocusHandle, Focusable as _, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Point, Render,
    StatefulInteractiveElement as _, Styled as _, Subscription, WeakEntity, Window, WindowBounds,
    WindowOptions, div, px, size,
};
use gpui_component::Root;
use gpui_component::input::{InputEvent, Textarea, TextareaState};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// The card's size before the chat's zoom.
const WIDTH: f32 = 360.0;
const HEIGHT: f32 = 132.0;
/// The line that says why the last send failed (`reopen_failed_reply`).
const ERROR_LINE: f32 = 22.0;
const RADIUS: f32 = 12.0;
/// How long the press on the Reply row still counts as "a press just now" once the card is open:
/// a little over the 0.3s of `GhostexGpuiPointerPressedRecently`.
pub(super) const OPENING_PRESS: Duration = Duration::from_millis(350);

/// The open popover's window, one per chat.
#[derive(Default)]
pub(in crate::app::native_chat) struct ReplyPopoverState {
    handle: Option<gpui::WindowHandle<Root>>,
    /// The last popover's command and the window it opened from, so a work item Reply whose send
    /// failed opens again where it was (`reopen_failed_reply`).
    last: Option<(gpui::AnyWindowHandle, serde_json::Value)>,
    /// This session's drafts were asked of gxserver (`load_kept_replies`).
    loaded: bool,
    /// Drafts came from gxserver: once the board is in, those of work items it no longer shows go.
    check_loaded: bool,
    /// Rows edited since their last save, saved `SAVE_AFTER` after the last edit.
    unsaved: HashSet<String>,
    save_generation: u64,
    /// Saves and drops waiting, sent one at a time so a late save cannot bring back a draft that
    /// was just sent (`pump_kept_replies`).
    queue: std::collections::VecDeque<serde_json::Value>,
    sending: bool,
}

/// How long after the last edit a draft is saved: one write per pause, not per keystroke.
const SAVE_AFTER: Duration = Duration::from_millis(400);

/// What was typed in a question's or work item's Reply box and not sent, by the chat's session and
/// the row (`kept_reply_key`). Written on every edit, so no way the box closes (a click away,
/// Escape, another row's Reply, the pane or session going away) loses it, and saved to the
/// session's gxserver soon after (`note_reply_edit`, server/src/session_reply_drafts.rs), so a
/// quit, restart, rebuild or crash does not either: a chat reads its session's back when it opens.
///
/// CDXC:Coordinators 2026-11-11 DECISION:
/// User: "for these kinds of boxes where i reply PLEASE keep in memory what i wrote there so we
/// restore it when i click away by mistake. i lost what i entered there many times". Reply on the
/// same row opens with the words and the caret after them; Send answer or Add to prompt clears
/// them, and a row that holds some says "Draft" on its Reply. Then, of drafts kept only in memory:
/// "not good enough fam. pls fix." gxserver keeps them, as it keeps the chat box's draft.
static KEPT_REPLIES: std::sync::LazyLock<std::sync::Mutex<HashMap<String, KeptReply>>> =
    std::sync::LazyLock::new(Default::default);

struct KeptReply {
    text: String,
    /// Its row has been seen open on the dock or the board, so its going away from both means
    /// the ask was answered, dismissed or withdrawn (`prune_kept_replies`). A draft on a question
    /// that never showed there (an older one in the transcript) is never dropped that way.
    seen: bool,
}

fn kept_replies() -> std::sync::MutexGuard<'static, HashMap<String, KeptReply>> {
    KEPT_REPLIES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The row a Reply belongs to, from its `question` object: a work item ask's card key
/// (`work-ask:W7:Q12`, or the item's own key when it asks no question), else the one-click
/// question's form and tag. `None` for a reply to a quoted selection.
fn kept_reply_row(reply: &serde_json::Value) -> Option<String> {
    let tag = reply["tag"].as_str().unwrap_or_default();
    // A work item's: by its reference and ask, which outlive its card key (an item's own Reply's
    // key carries the time it last changed).
    if let Some(item) = reply["item"].as_str().filter(|item| !item.is_empty()) {
        return Some(if tag.is_empty() {
            format!("work-item:{item}")
        } else {
            format!("work-ask:{item}:{tag}")
        });
    }
    let key = reply["key"].as_str().unwrap_or_default();
    if !key.is_empty() {
        return Some(key.to_owned());
    }
    (!tag.is_empty()).then(|| {
        format!(
            "question:{}:{tag}",
            reply["form"].as_str().unwrap_or_default()
        )
    })
}

/// Keeps `text` as the draft at `key`, or drops it when nothing is left in it. `seen`: its row is
/// open on the dock or the board now (`KeptReply::seen`).
fn keep_reply(key: &str, text: &str, seen: bool) {
    let mut kept = kept_replies();
    if text.trim().is_empty() {
        kept.remove(key);
    } else {
        kept.entry(key.to_owned())
            .and_modify(|draft| {
                draft.text = text.to_owned();
                draft.seen |= seen;
            })
            .or_insert_with(|| KeptReply {
                text: text.to_owned(),
                seen,
            });
    }
}

/// Every Reply row open on the dock or the board: `reply` objects anywhere in either (the board's
/// Later column keeps a snoozed ask, so snoozing does not drop its draft).
fn open_reply_rows(value: &serde_json::Value, rows: &mut HashSet<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (field, child) in map {
                if field == "reply"
                    && let Some(row) = kept_reply_row(child)
                {
                    rows.insert(row);
                }
                open_reply_rows(child, rows);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                open_reply_rows(item, rows);
            }
        }
        _ => {}
    }
}

/// The one-click question a Reply popover answers (inline_answers.rs): its form when the click
/// came from the question's own pill, else its tag alone (its own Q-tag pill), which the core
/// resolves to the newest copy.
#[derive(Clone)]
struct QuestionReply {
    form: Option<String>,
    tag: String,
    /// A coordinator's work item ask (`W7`) and its card key: the words answer that ask
    /// (gx-chat-core `coordinator_board::answer` with op `reply`).
    work: Option<(String, String)>,
}

/// What the popover's actions do with the chat box.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ReplyAction {
    AddToPrompt,
    Send,
}

struct ReplyPopover {
    chat: WeakEntity<NativeChatView>,
    /// The selection quoted as Add to Chat quotes it (the Reply row's `quote`).
    quote: String,
    /// Set when it answers a one-click question instead of quoting a selection.
    question: Option<QuestionReply>,
    /// Why the last send of these words failed, shown beside the buttons (`reopen_failed_reply`).
    error: String,
    input: Entity<TextareaState>,
    focus: FocusHandle,
    appearance: ChatAppearance,
    closed: bool,
    was_active: bool,
    /// True until the press that opened the card has aged out (`OPENING_PRESS`).
    settling: bool,
    /// How many times this card has taken key status back.
    rekeys: u32,
    /// Where its words are kept until sent (`KEPT_REPLIES`), and whether its row was open on the
    /// dock or the board when it opened; `None` for a quoted selection.
    kept: Option<(String, String, bool)>,
    _subscriptions: Vec<Subscription>,
}

impl NativeChatView {
    /// The Reply row's command: opens the popover at the point the menu opened from (`at`, in the
    /// chat window's content coordinates, added by `show_transcript_menu`).
    pub(super) fn open_reply_popover(
        &mut self,
        command: &serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A work item with no open question has no tag: its words go to the item ("W7: words").
        let item = command["question"]["item"].as_str().unwrap_or_default();
        let question = command["question"]["tag"]
            .as_str()
            .filter(|tag| !tag.is_empty() || !item.is_empty())
            .map(|tag| QuestionReply {
                form: command["question"]["form"].as_str().map(str::to_owned),
                tag: tag.to_owned(),
                work: command["question"]["item"]
                    .as_str()
                    .filter(|item| !item.is_empty())
                    .map(|item| {
                        (
                            item.to_owned(),
                            command["question"]["key"]
                                .as_str()
                                .unwrap_or_default()
                                .to_owned(),
                        )
                    }),
            });
        let quote = command["quote"].as_str().unwrap_or_default();
        if quote.is_empty() && question.is_none() {
            return;
        }
        self.close_reply_popover(cx);
        self.reply_popover.last = Some((window.window_handle(), command.clone()));
        // Opened again after a failed send: the words typed, and why it did not go. Else what was
        // typed on this row before and not sent (`KEPT_REPLIES`).
        let kept = kept_reply_row(&command["question"]).map(|row| {
            let mut open = HashSet::new();
            for field in ["workDock", "coordinatorBoard"] {
                open_reply_rows(&self.snapshot[field], &mut open);
            }
            let seen = open.contains(&row);
            (self.kept_reply_key(&row), row, seen)
        });
        let mut typed = command["text"].as_str().unwrap_or_default().to_owned();
        if let Some((key, row, seen)) = &kept {
            if typed.is_empty() {
                typed = kept_replies()
                    .get(key)
                    .map(|draft| draft.text.clone())
                    .unwrap_or_default();
            } else {
                keep_reply(key, &typed, *seen);
                self.note_reply_edit(row.clone(), cx);
            }
        }
        let error = command["error"].as_str().unwrap_or_default().to_owned();
        let coordinate = |index: usize| command["at"][index].as_f64().unwrap_or(0.0) as f32;
        let at = Point::new(px(coordinate(0)), px(coordinate(1)));
        let appearance = ChatAppearance::current(&self.snapshot);
        let scale = appearance.scale;
        let content = super::child_window::content_bounds(window);
        let view = self.bounds.get();
        // It stays inside the chat column (the chat box's width, which is the transcript's) of the
        // chat's own window; opened from the floating Work card or the columns, inside that window.
        //
        // CDXC:Coordinators 2026-11-11 WHY:
        // A Reply near the right edge opened its box past the chat column, out to the window's
        // edge: the box was kept only inside the whole chat view.
        let in_chat_window = self
            .open_main_window(cx)
            .is_none_or(|main| main == window.window_handle());
        let column = self.composer_bounds.get();
        let within = if !in_chat_window {
            content
        } else if view.size.width > px(0.0) && view.size.height > px(0.0) {
            let (left, right) = if column.size.width > px(0.0) {
                (
                    column.left().max(view.left()),
                    column.right().min(view.right()),
                )
            } else {
                (view.left(), view.right())
            };
            Bounds::from_corners(
                content.origin + Point::new(left, view.top()),
                content.origin + Point::new(right, view.bottom()),
            )
        } else {
            content
        };
        let bounds = popover_bounds(content.origin + at, within, scale, !error.is_empty());
        let parent = super::child_window::window_native_view(window)
            .unwrap_or(self.config.parent_native_view);
        let display_id =
            crate::app::window::popup_frame::PopupOwner::of(window, cx).display_for(bounds, cx);
        let glass = crate::app::helpers::window_glass_active();
        let corner_radius =
            crate::app::window::popup_frame::popup_card_radius(px(RADIUS * scale), glass);
        let chat = cx.weak_entity();
        let quote = quote.to_owned();
        cx.defer(move |cx| {
            let result = cx.open_window(
                WindowOptions {
                    kind: crate::app::window::popup_frame::child_window_kind(),
                    window_decorations: crate::app::window::popup_frame::child_window_decorations(),
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    display_id,
                    titlebar: None,
                    focus: true,
                    show: true,
                    is_movable: false,
                    is_resizable: false,
                    is_minimizable: false,
                    app_id: crate::gpui_platform_window_app_id(),
                    icon: crate::gpui_platform_window_icon(),
                    window_background: if glass {
                        gpui::WindowBackgroundAppearance::Blurred
                    } else {
                        gpui::WindowBackgroundAppearance::Transparent
                    },
                    ..Default::default()
                },
                {
                    let chat = chat.clone();
                    move |window, cx| {
                        crate::app::window::popup_frame::round_popup_window(
                            window,
                            corner_radius,
                            glass,
                        );
                        crate::app::helpers::apply_frosted_menu_blur(window);
                        crate::app::window::popup_frame::strip_gpui_popup_window_frame(window);
                        crate::app::window::attach_gpui_app_modal_window_to_main_window(
                            window, parent,
                        );
                        let popover = cx.new(|cx| {
                            ReplyPopover::new(
                                chat, quote, question, typed, error, kept, appearance, window, cx,
                            )
                        });
                        cx.new(|cx| {
                            Root::new(popover, window, cx)
                                .bordered(false)
                                .bg(gpui::transparent_black())
                        })
                    }
                },
            );
            let _ = chat.update(cx, |chat, cx| match result {
                Ok(handle) => chat.reply_popover.handle = Some(handle),
                Err(error) => {
                    chat.error = Some(error.to_string());
                    cx.notify();
                }
            });
        });
    }

    /// A work item Reply that did not go (gx-chat-core `coordinator_board`'s `workReplyFailed`):
    /// its popover opens again where it was, with the words typed and the error, so he can send
    /// again; with no popover to reopen, the words go into the chat box instead.
    ///
    /// CDXC:Coordinators 2026-11-10 DECISION:
    /// The user's typed Reply on a Work panel item failed with a red notice and his text was gone.
    /// A Reply whose record or message fails is never lost: the box comes back holding it.
    pub(super) fn reopen_failed_reply(
        &mut self,
        params: &serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let words = params["text"].as_str().unwrap_or_default().to_owned();
        if words.is_empty() {
            return;
        }
        let key = params["key"].as_str().unwrap_or_default();
        let last = self
            .reply_popover
            .last
            .clone()
            .filter(|(_, command)| command["question"]["key"] == key);
        let Some((handle, mut command)) = last else {
            self.append_prompt(&words, cx);
            return;
        };
        command["text"] = serde_json::json!(words);
        command["error"] = params["message"].clone();
        let view = cx.entity();
        cx.defer(move |cx| {
            let opened = handle.update(cx, |_, window, cx| {
                view.update(cx, |chat, cx| chat.open_reply_popover(&command, window, cx));
            });
            if opened.is_err() {
                view.update(cx, |chat, cx| chat.append_prompt(&words, cx));
            }
        });
    }

    /// A row's place in `KEPT_REPLIES`: the same question in another session is another row.
    fn kept_reply_key(&self, row: &str) -> String {
        format!(
            "{}/{}/{}\n{row}",
            self.config.machine_id, self.config.project_id, self.config.session_id
        )
    }

    /// Whether the Reply of `reply` (a row's `reply` object) holds words not yet sent, for its
    /// "Draft" sign.
    pub(super) fn has_kept_reply(&self, reply: &serde_json::Value) -> bool {
        kept_reply_row(reply)
            .is_some_and(|row| kept_replies().contains_key(&self.kept_reply_key(&row)))
    }

    /// A new snapshot: a draft whose row was open on the dock or the board and is on neither now
    /// goes, the ask being answered, dismissed or withdrawn. The first one asks gxserver for the
    /// session's drafts (`load_kept_replies`); once those are in and so is the board, a work item's
    /// draft whose row it no longer shows goes too (answered or dropped while the app was shut).
    pub(super) fn prune_kept_replies(
        &mut self,
        before: &serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        self.load_kept_replies(cx);
        let scope = self.kept_reply_key("");
        let check_loaded =
            self.reply_popover.check_loaded && self.snapshot["coordinatorBoard"].is_object();
        let fields = ["workDock", "coordinatorBoard"];
        let mut dropped = Vec::new();
        {
            let mut kept = kept_replies();
            if !kept.keys().any(|key| key.starts_with(&scope)) {
                self.reply_popover.check_loaded &= !check_loaded;
                return;
            }
            if !check_loaded
                && fields
                    .iter()
                    .all(|field| before[*field] == self.snapshot[*field])
            {
                return;
            }
            let mut rows = HashSet::new();
            for field in fields {
                open_reply_rows(&self.snapshot[field], &mut rows);
            }
            kept.retain(|key, draft| {
                let Some(row) = key.strip_prefix(&scope) else {
                    return true;
                };
                let open = rows.contains(row);
                draft.seen |= open;
                let gone = !open && (draft.seen || check_loaded && row.starts_with("work-"));
                if gone {
                    dropped.push(row.to_owned());
                }
                !gone
            });
        }
        self.reply_popover.check_loaded &= !check_loaded;
        if !dropped.is_empty() {
            self.forget_kept_replies(dropped, cx);
        }
    }

    /// Asks the session's gxserver for its drafts, once, and keeps them beside any typed since.
    fn load_kept_replies(&mut self, cx: &mut Context<Self>) {
        if self.reply_popover.loaded || self.config.session_id.is_empty() {
            return;
        }
        self.reply_popover.loaded = true;
        self.reply_drafts_call(json!({ "op": "list" }), cx, |chat, answer, cx| {
            let Some(drafts) = answer
                .as_ref()
                .and_then(|answer| answer["drafts"].as_object())
            else {
                return;
            };
            let scope = chat.kept_reply_key("");
            {
                let mut kept = kept_replies();
                for (row, text) in drafts {
                    if let Some(text) = text.as_str().filter(|text| !text.trim().is_empty()) {
                        kept.entry(format!("{scope}{row}"))
                            .or_insert_with(|| KeptReply {
                                text: text.to_owned(),
                                seen: false,
                            });
                    }
                }
            }
            chat.reply_popover.check_loaded = true;
            chat.prune_kept_replies(&serde_json::Value::Null, cx);
            cx.notify();
        });
    }

    /// A Reply box's words changed on `row`: saved `SAVE_AFTER` after the last edit.
    fn note_reply_edit(&mut self, row: String, cx: &mut Context<Self>) {
        self.reply_popover.unsaved.insert(row);
        self.reply_popover.save_generation += 1;
        let generation = self.reply_popover.save_generation;
        cx.spawn(async move |chat, cx| {
            cx.background_executor().timer(SAVE_AFTER).await;
            let _ = chat.update(cx, |chat, cx| {
                if chat.reply_popover.save_generation == generation {
                    chat.save_kept_replies(cx);
                }
            });
        })
        .detach();
    }

    /// Saves every row edited since its last save, as it stands now (an emptied one is dropped).
    fn save_kept_replies(&mut self, cx: &mut Context<Self>) {
        let rows: Vec<String> = self.reply_popover.unsaved.drain().collect();
        let texts: Vec<(String, String)> = {
            let kept = kept_replies();
            rows.into_iter()
                .map(|row| {
                    let text = kept
                        .get(&self.kept_reply_key(&row))
                        .map(|draft| draft.text.clone())
                        .unwrap_or_default();
                    (row, text)
                })
                .collect()
        };
        for (row, text) in texts {
            self.reply_popover
                .queue
                .push_back(json!({ "op": "set", "row": row, "text": text }));
        }
        self.pump_kept_replies(cx);
    }

    /// Drops these rows' drafts, here and on gxserver: sent, or their question is gone.
    fn forget_kept_replies(&mut self, rows: Vec<String>, cx: &mut Context<Self>) {
        {
            let mut kept = kept_replies();
            for row in &rows {
                kept.remove(&self.kept_reply_key(row));
            }
        }
        for row in &rows {
            self.reply_popover.unsaved.remove(row);
        }
        self.reply_popover
            .queue
            .push_back(json!({ "op": "drop", "rows": rows }));
        self.pump_kept_replies(cx);
    }

    /// Sends the next waiting save or drop once the one before it has answered.
    fn pump_kept_replies(&mut self, cx: &mut Context<Self>) {
        if self.reply_popover.sending {
            return;
        }
        let Some(params) = self.reply_popover.queue.pop_front() else {
            return;
        };
        self.reply_popover.sending = true;
        self.reply_drafts_call(params, cx, |chat, _, cx| {
            chat.reply_popover.sending = false;
            chat.pump_kept_replies(cx);
        });
    }

    /// One `/api/sessionReplyDrafts` call to the session's gxserver (a Cloud Box's own when it is
    /// up; a stopped box is not woken for it), then `then` with its answer (`None` when it failed).
    fn reply_drafts_call(
        &self,
        mut params: serde_json::Value,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, Option<serde_json::Value>, &mut Context<Self>) + 'static,
    ) {
        let config = self.config.clone();
        params["projectId"] = json!(config.project_id);
        params["sessionId"] = json!(config.session_id);
        let task =
            cx.background_executor().spawn(async move {
                let remote =
                    match crate::app::gx_store::cloud_box_send_target(&config.machine_id, false)
                        .await
                    {
                        Ok(Some(target)) => Some(target),
                        Ok(None) => config.remote,
                        Err(_) => return None,
                    };
                super::rpc::request(remote, "/api/sessionReplyDrafts", &params)
                    .await
                    .ok()
            });
        cx.spawn(async move |chat, cx| {
            let answer = task.await;
            let _ = chat.update(cx, |chat, cx| then(chat, answer, cx));
        })
        .detach();
    }

    /// The popover's window, which counts as one of the chat's own child windows.
    pub(super) fn reply_popover_window(&self) -> Option<gpui::WindowId> {
        self.reply_popover.handle.map(|handle| handle.window_id())
    }

    /// Takes the popover down without touching the chat box (the pane went away, a second Reply).
    pub(super) fn close_reply_popover(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.reply_popover.handle.take() {
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            });
        }
    }

    /// A one-click question's Reply popover, opened at `at` (window coordinates) by its ✎ answer
    /// or its own Q-tag pill.
    ///
    /// CDXC:SessionChat 2026-11-05 DECISION:
    /// User: "i have to have a way to write my own answer for these questions pls (show the Reply pop up thingy for that question)". Every one-click answer pill ends with a ✎ segment that opens the transcript menu's Reply popover for that question; Send answer (Cmd+Enter) sends the typed words as the question's answer in the form's usual answer message (`inlineAnswerText`, gx-chat-core questions/inline_answers.rs), and the pill then reads Sent · Change with the words, as a pick does; Add to prompt (Enter) puts "Q12: words" at the end of the chat box, and a chat box message of only such lines goes as the same answer message and card (the SessionChat 2026-11-06 DECISION on `typed_answers` in gx-chat-core questions/inline_answers.rs). The question's own Q-tag pill opens the same popover rather than jumping (the 2026-11-05 decision on `jump_to_tag`).
    pub(super) fn open_question_reply(
        &mut self,
        form: Option<&str>,
        tag: &str,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Window coordinates, as the transcript menu's own `at` (`show_transcript_menu`).
        let command = serde_json::json!({
            "question": {"form": form, "tag": tag},
            "at": [f32::from(at.x), f32::from(at.y)],
        });
        self.open_reply_popover(&command, window, cx);
    }

    /// The popover's action: the reply under the quote at the end of the chat box, then a send
    /// for Send reply, exactly as the Send button sends (queued while the agent works). A question's
    /// reply goes as its answer instead, or as "Q12: words" into the chat box.
    fn apply_reply(
        &mut self,
        quote: &str,
        question: Option<&QuestionReply>,
        reply: &str,
        action: ReplyAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(question) = question {
            let reply = reply.trim();
            if reply.is_empty() {
                return;
            }
            // A work item ask: the words are its answer, recorded once and sent to the
            // coordinator as "W7 Q12: words" (the user's "WE MUST HAVE REPLY BUTTON FOR NEEDS YOU
            // STUFF").
            if let Some((item, key)) = &question.work {
                // "W7 Q12: words", or "W7: words" for an item that asks no question.
                let to = if question.tag.is_empty() {
                    item.clone()
                } else {
                    format!("{item} {}", question.tag)
                };
                match action {
                    ReplyAction::Send => self.invoke(
                        serde_json::json!({
                            "type": "coordinatorBoardAnswer",
                            "op": "reply",
                            "key": key,
                            "item": item,
                            "ask": question.tag,
                            "label": reply,
                            "text": format!("{to}: {reply}"),
                        }),
                        cx,
                    ),
                    ReplyAction::AddToPrompt => self.append_prompt(&format!("{to}: {reply}"), cx),
                }
                return;
            }
            match action {
                ReplyAction::Send => self.invoke(
                    serde_json::json!({"type":"inlineAnswerText","form":question.form,"tag":question.tag,"text":reply}),
                    cx,
                ),
                ReplyAction::AddToPrompt => {
                    self.append_prompt(&format!("{}: {reply}", question.tag), cx)
                }
            }
            return;
        }
        let draft = ghostex_gx_chat_core::composer::transcript_menu::transcript_reply_draft(
            &self.draft,
            quote,
            reply,
        );
        self.insert_prompt(&draft, cx);
        if action == ReplyAction::Send {
            // The Send button's own call: the core queues it while the agent is working.
            self.send(false, window, cx);
        }
    }
}

/// Where the card opens: its top-left corner at the press, kept inside the chat with a margin, and
/// above the press when there is no room below it.
fn popover_bounds(
    at: Point<Pixels>,
    within: Bounds<Pixels>,
    scale: f32,
    error: bool,
) -> Bounds<Pixels> {
    let margin = px(12.0 * scale);
    let width = px(WIDTH * scale).min(within.size.width - margin * 2.0);
    // A send that failed says why on a line of its own above the buttons.
    let height = px((HEIGHT + if error { ERROR_LINE } else { 0.0 }) * scale);
    let y = if at.y + px(4.0 * scale) + height < within.bottom() - margin {
        at.y + px(4.0 * scale)
    } else {
        at.y - height - px(4.0 * scale)
    };
    let x =
        at.x.min(within.right() - width - margin)
            .max(within.left() + margin);
    let y = y
        .min(within.bottom() - height - margin)
        .max(within.top() + margin);
    Bounds::new(Point::new(x, y), size(width, height))
}

/// Whether losing key status closes the card. A press only counts once the one on the Reply row
/// has aged out, and a window that keeps taking key status back wins after `MAX_REKEYS`.
pub(super) fn key_loss_closes(settling: bool, user_dismissed: bool, rekeys: u32) -> bool {
    rekeys >= option_menu::MAX_REKEYS || (user_dismissed && !settling)
}

impl ReplyPopover {
    fn new(
        chat: WeakEntity<NativeChatView>,
        quote: String,
        question: Option<QuestionReply>,
        typed: String,
        error: String,
        kept: Option<(String, String, bool)>,
        appearance: ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = match &question {
            Some(QuestionReply {
                tag,
                work: Some((item, _)),
                ..
            }) if tag.is_empty() => format!("Your reply on {item}…"),
            Some(question) => format!("Your answer to {}…", question.tag),
            None => "Reply…".to_string(),
        };
        let typed_any = !typed.is_empty();
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(3, 3)
                .auto_text_direction(true)
                .submit_on_enter(true)
                .placeholder(placeholder)
                .default_value(typed.clone())
        });
        // Enter is the field's own key (GPUI runs key bindings before key-down listeners): with
        // `submit_on_enter` a plain Enter reports `PressEnter` instead of a new line, Cmd+Enter
        // (Ctrl+Enter off a Mac) reports it as `secondary`, and Shift+Enter stays a new line.
        let enter = cx.subscribe(&input, |popover: &mut Self, _, event: &InputEvent, cx| {
            if let InputEvent::PressEnter {
                secondary,
                shift: false,
            } = event
            {
                popover.close(
                    Some(if *secondary {
                        ReplyAction::Send
                    } else {
                        ReplyAction::AddToPrompt
                    }),
                    true,
                    cx,
                );
            }
        });
        // Every edit is kept at once (`KEPT_REPLIES`), whatever closes the card after.
        let keep = cx.subscribe(
            &input,
            |popover: &mut Self, input, event: &InputEvent, cx| {
                if let (InputEvent::Change, Some((key, row, seen))) = (event, &popover.kept) {
                    keep_reply(key, &input.read(cx).value(), *seen);
                    let row = row.clone();
                    let _ = popover
                        .chat
                        .update(cx, |chat, cx| chat.note_reply_edit(row, cx));
                }
            },
        );
        let focus = cx.focus_handle();
        // Words kept from before: the caret after them, ready to go on typing.
        let end = typed.split('\n').count().saturating_sub(1) as u32;
        input.update(cx, |input, cx| {
            if typed_any {
                input.set_cursor_position(
                    gpui_component::input::Position::new(end, u32::MAX),
                    window,
                    cx,
                );
            }
            input.focus_handle(cx).focus(window, cx);
        });
        let activation = cx.observe_window_activation(window, |popover: &mut Self, window, cx| {
            if window.is_window_active() {
                popover.was_active = true;
            } else if popover.was_active {
                crate::app::window::popup_dismissal::dismiss_on_focus_loss(
                    popover,
                    window,
                    cx,
                    |popover, window, cx| popover.key_lost(window, cx),
                );
            }
        });
        // A card that opened while the app was still being activated never became key, and so was
        // never told it lost it (the same check as `ChatOptionMenu::settle_key`).
        cx.spawn_in(window, async move |popover, cx| {
            cx.background_executor().timer(OPENING_PRESS).await;
            let _ = popover.update_in(cx, |popover, window, _| {
                popover.settling = false;
                if !window.is_window_active() && !option_menu::key_loss_is_dismissal() {
                    popover.retake_key(window);
                }
            });
        })
        .detach();
        let press = crate::app::window::popup_dismissal::observe_main_window_press(
            window,
            cx,
            |popover: &mut Self, _, cx| popover.close(None, false, cx),
        );
        Self {
            chat,
            quote,
            question,
            error,
            input,
            focus,
            appearance,
            closed: false,
            was_active: window.is_window_active(),
            settling: true,
            rekeys: 0,
            kept,
            _subscriptions: vec![activation, press, enter, keep],
        }
    }

    /// Key status went elsewhere. The user sent it there (a press since the card opened, or another
    /// app) and the card closes, or something else took it and the card takes it back, as the
    /// chat's menus do (CDXC:SessionChat 2026-09-22 on `ChatOptionMenu::check_active`).
    fn key_lost(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // GPUI reports activation from a queued task, so the report can be out of date already.
        if self.closed || window.is_window_active() {
            return;
        }
        if key_loss_closes(
            self.settling,
            option_menu::key_loss_is_dismissal(),
            self.rekeys,
        ) {
            self.close(None, false, cx);
        } else {
            self.retake_key(window);
        }
    }

    fn retake_key(&mut self, window: &mut Window) {
        self.rekeys += 1;
        window.activate_window();
    }

    /// Closes the card, then hands the reply to the chat for `action` (none for Escape or a click
    /// away). `to_chat` gives the chat's window the keyboard back, which a click away already did.
    fn close(&mut self, action: Option<ReplyAction>, to_chat: bool, cx: &mut Context<Self>) {
        if self.closed {
            return;
        }
        self.closed = true;
        let reply = self.input.read(cx).value().to_string();
        // Sent or added to the prompt: the draft has done its job. A send that fails opens the
        // card again with the words (`reopen_failed_reply`), which keeps them again.
        if let Some((_, row, _)) = &self.kept {
            let row = row.clone();
            let sent = action.is_some() && !reply.trim().is_empty();
            let _ = self.chat.update(cx, |chat, cx| {
                if sent {
                    chat.forget_kept_replies(vec![row], cx);
                } else {
                    // Closed without sending: saved now rather than after the pause.
                    chat.save_kept_replies(cx);
                }
            });
        }
        let quote = self.quote.clone();
        let question = self.question.clone();
        let chat = self.chat.clone();
        cx.defer(move |cx| {
            let Ok((handle, main)) = chat.update(cx, |chat, _| {
                (chat.reply_popover.handle.take(), chat.main_window)
            }) else {
                return;
            };
            if let Some(handle) = handle {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            }
            let Some(main) = main else {
                return;
            };
            let _ = main.update(cx, |_, window, cx| {
                if to_chat {
                    window.activate_window();
                }
                let _ = chat.update(cx, |chat, cx| {
                    if let Some(action) = action {
                        chat.apply_reply(&quote, question.as_ref(), &reply, action, window, cx);
                    }
                    cx.notify();
                });
            });
        });
    }

    /// Escape closes the card without touching the chat box (the field lets it through).
    fn key(&mut self, event: &gpui::KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" {
            cx.stop_propagation();
            window.prevent_default();
            self.close(None, true, cx);
        }
    }

    fn action_button(
        &self,
        id: &'static str,
        label: &'static str,
        action: ReplyAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = &self.appearance;
        let s = p.scale;
        let (hover, _) = menu_ink_washes(p.light);
        let shortcut = shortcut_label(action);
        div()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(format!("{label} ({shortcut})"))
            .chat_cursor_pointer()
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .h(px(26.0 * s))
            .px(px(8.0 * s))
            .rounded(px(6.0 * s))
            .hover(move |row| row.bg(hover))
            .child(div().text_color(p.foreground).child(label))
            .child(div().text_color(p.muted).child(shortcut))
            .on_click(cx.listener(move |popover, _, _, cx| popover.close(Some(action), true, cx)))
            .into_any_element()
    }
}

impl Render for ReplyPopover {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.appearance.clone();
        let s = p.scale;
        let (_, edge) = menu_ink_washes(p.light);
        let error_color = p.error();
        div()
            .id("chat-reply-popover")
            .role(gpui::Role::Dialog)
            .aria_label("Reply")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .justify_between()
            .rounded(px(RADIUS * s))
            .border_1()
            .border_color(edge)
            .bg(p.menu_surface())
            .p(px(8.0 * s))
            .font_family(p.font)
            .text_size(px(13.0 * s))
            .text_color(p.foreground)
            .on_key_down(cx.listener(Self::key))
            .child(
                div().px(px(4.0 * s)).pt(px(2.0 * s)).child(
                    Textarea::new(&self.input)
                        .appearance(false)
                        .bordered(false)
                        .caret_color(p.foreground)
                        .text_size(px(13.0 * s)),
                ),
            )
            .when(!self.error.is_empty(), |card| {
                card.child(
                    div()
                        .id("chat-reply-error")
                        .w_full()
                        .min_w_0()
                        .px(px(4.0 * s))
                        .truncate()
                        .text_color(error_color)
                        .child(self.error.clone())
                        .tooltip({
                            let error = self.error.clone();
                            move |window, cx| {
                                gpui_component::tooltip::Tooltip::new(error.clone())
                                    .build(window, cx)
                            }
                        }),
                )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(2.0 * s))
                    .font_weight(FontWeight::MEDIUM)
                    .child(self.action_button(
                        "chat-reply-add",
                        "Add to prompt",
                        ReplyAction::AddToPrompt,
                        cx,
                    ))
                    .child(self.action_button(
                        "chat-reply-send",
                        if self.question.is_some() {
                            "Send answer"
                        } else {
                            "Send reply"
                        },
                        ReplyAction::Send,
                        cx,
                    )),
            )
    }
}

/// The action's key as the reference popover shows it: ⏎ and ⌘⏎ on a Mac keyboard, spelled out
/// elsewhere (Ctrl+Enter sends there).
fn shortcut_label(action: ReplyAction) -> &'static str {
    match (action, super::keyboard::chat_mac_keys()) {
        (ReplyAction::AddToPrompt, true) => "⏎",
        (ReplyAction::Send, true) => "⌘⏎",
        (ReplyAction::AddToPrompt, false) => "Enter",
        (ReplyAction::Send, false) => "Ctrl+Enter",
    }
}

/// The chat menus' hover wash and hairline (`option_menu/render.rs`), so the card reads as one of
/// them.
pub(super) fn menu_ink_washes(light: bool) -> (gpui::Hsla, gpui::Hsla) {
    let foreground: gpui::Hsla = gpui::rgb(if light { 0x292929 } else { 0xfcfcfc }).into();
    let border: gpui::Hsla = gpui::rgba(if light { 0x0000001f } else { 0xffffff1f }).into();
    (foreground.opacity(if light { 0.06 } else { 0.08 }), border)
}

#[cfg(test)]
mod tests {
    use super::{key_loss_closes, option_menu::MAX_REKEYS};

    /// The bug: the press on the Reply row is still "recent" when the main window takes key status
    /// back, and that closed the card before anything could be typed.
    #[test]
    fn the_press_that_opened_the_card_does_not_close_it() {
        assert!(!key_loss_closes(true, true, 0));
    }

    #[test]
    fn a_key_loss_nobody_caused_does_not_close_it() {
        assert!(!key_loss_closes(false, false, 0));
        assert!(!key_loss_closes(true, false, 0));
    }

    #[test]
    fn a_press_after_it_opened_closes_it() {
        assert!(key_loss_closes(false, true, 0));
    }

    #[test]
    fn a_window_that_keeps_taking_key_status_wins_eventually() {
        assert!(!key_loss_closes(false, false, MAX_REKEYS - 1));
        assert!(key_loss_closes(false, false, MAX_REKEYS));
        assert!(key_loss_closes(true, true, MAX_REKEYS));
    }
}
