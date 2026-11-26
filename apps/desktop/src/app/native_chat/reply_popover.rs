//! The transcript menu's Reply popover: a small frosted card where the menu was, with a field for
//! the reply and its two actions, Send reply (Enter, as in the chat box) and Add to prompt
//! (Option+Enter, Alt+Enter off a Mac). The row and the draft it leaves are the core's
//! (CDXC:SessionChat 2026-10-29 DECISION on `reply_item` in
//! `packages/gx-chat-core/src/composer/transcript_menu.rs`).
//!
//! CDXC:SessionChat 2026-10-29 WHY:
//! It is a child window like the chat's menus because GPUI blurs only behind a whole window
//! (frosted_overlay_window.rs), and floating cards take the frosted menu fill.

use super::{appearance::ChatAppearance, option_menu, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Bounds, Context, Entity, FocusHandle, Focusable as _, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Point, Render, ScrollHandle,
    StatefulInteractiveElement as _, Styled as _, Subscription, WeakEntity, Window, WindowBounds,
    WindowOptions, div, px, rgb, size,
};
use gpui_component::Root;
use gpui_component::input::{InputEvent, Textarea, TextareaState};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// The card's size before the chat's zoom; wider when its button row needs it (`card_width`).
const WIDTH: f32 = 400.0;
/// The card's padding, the row's icon buttons (Dictate, Attach) and the gap between the row's parts.
const PADDING: f32 = 8.0;
const ICON_BUTTON: f32 = 26.0;
const ROW_GAP: f32 = 2.0;
/// An action button's padding either side and the gap between its label and its key.
const ACTION_PADDING: f32 = 8.0;
const ACTION_GAP: f32 = 8.0;
const HEIGHT: f32 = 132.0;
/// The line that says why the last send failed (`reopen_failed_reply`).
const ERROR_LINE: f32 = 22.0;
const RADIUS: f32 = 12.0;
/// Compact mention menu metrics inside the Reply popover (unscaled).
const MENTION_ROW_HEIGHT: f32 = 26.0;
const MENTION_HEADING_HEIGHT: f32 = 18.0;
const MENTION_PADDING: f32 = 4.0;
const MENTION_MAX_HEIGHT: f32 = 188.0;
/// Small white dot at the top-right of an inline Reply button when the session has an unsent reply
/// draft (`has_reply_draft`). Never shown on Inbox dock Reply buttons, where many sit side by side.
pub(super) fn reply_draft_dot(p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    let color: gpui::Hsla = if p.light {
        p.foreground
    } else {
        rgb(0xffffff).into()
    };
    div()
        .absolute()
        .top(px(-2.0 * s))
        .right(px(-2.0 * s))
        .size(px(6.0 * s))
        .rounded_full()
        .bg(color)
        .into_any_element()
}

/// Inset variant of [`reply_draft_dot`] for stacked inline-answer rows clipped by `overflow_hidden`.
pub(super) fn reply_draft_dot_inset(p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    let color: gpui::Hsla = if p.light {
        p.foreground
    } else {
        rgb(0xffffff).into()
    };
    div()
        .absolute()
        .top(px(3.0 * s))
        .right(px(3.0 * s))
        .size(px(5.5 * s))
        .rounded_full()
        .bg(color)
        .into_any_element()
}
/// How long the press on the Reply row still counts as "a press just now" once the card is open:
/// a little over the 0.3s of `GhostexGpuiPointerPressedRecently`.
pub(super) const OPENING_PRESS: Duration = Duration::from_millis(350);

/// The open popover's window, one per chat.
#[derive(Default)]
pub(in crate::app::native_chat) struct ReplyPopoverState {
    pub(super) handle: Option<gpui::WindowHandle<Root>>,
    /// The open card's field, for Dictate and the pictures saved into it (reply_attachments.rs).
    pub(super) input: Option<WeakEntity<TextareaState>>,
    /// Pictures being saved into it, and why the last one could not be.
    pub(super) pending: usize,
    pub(super) attach_error: Option<String>,
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
/// them. Then, of drafts kept only in memory:
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

/// The one Reply draft a session keeps: every Reply card in the session, whatever question or
/// selection it opens on, opens with it and writes to it, until a send clears it.
///
/// CDXC:Coordinators 2026-11-16 DECISION:
/// User: "if i write something in a floating reply bubble and i click away or w/e then we MUST
/// keep that content in that reply bubble". One draft per session, not per question: the last
/// words, pictures and dictation typed into Reply, there again on whichever Reply opens next, kept
/// through a click away, a closed card, another session, a quit or a crash (gxserver's Reply
/// drafts, server/src/session_reply_drafts.rs, the chat box draft's counterpart); sending clears
/// it. Supersedes the 2026-11-11 one draft per row. The phone keeps the same row
/// (apps/mobile/app/src/chat/native/cards/replyDrafts.ts).
pub(super) const SESSION_REPLY_ROW: &str = "reply";

/// The row a Reply belongs to, from its `question` object: the session's one Reply draft
/// (`SESSION_REPLY_ROW`) for any question, form, page or work item. `None` for an object that
/// names none.
fn kept_reply_row(reply: &serde_json::Value) -> Option<String> {
    legacy_reply_row(reply).map(|_| SESSION_REPLY_ROW.to_owned())
}

/// The row a draft was kept under before one draft per session (2026-11-11 to 2026-11-16): a
/// work item ask's `work-ask:W7:Q12`, an item's `work-item:W7`, else its key or the one-click
/// question's form and tag. Such drafts read back from gxserver are taken into the session's one.
fn legacy_reply_row(reply: &serde_json::Value) -> Option<String> {
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
pub(super) struct QuestionReply {
    form: Option<String>,
    tag: String,
    /// A coordinator's work item ask (`W7`) and its card key: the words answer that ask
    /// (gx-chat-core `coordinator_board::answer` with op `reply`).
    work: Option<(String, String)>,
    /// A whole form's or page's Reply (its dock row's, gx-chat-core `work_cards`): its id and what
    /// the words go after, its open tags ("Q12 Q13") or, for a page that asks nothing, its
    /// mention. The words go to the agent as one message ("Q12 Q13: words", `questionsReply`).
    questions: Option<(String, String)>,
    /// What its box calls it: a page's or form's title, a question's words (gx-chat-core
    /// `reply_name`), never its tags; empty when the opener has no name for it.
    ///
    /// CDXC:Coordinators 2026-11-15 DECISION:
    /// User, of a Reply reading "Your answer to Q3310 Q3311 … Q3324…": "Please stop showing Q1,
    /// Q2, Q3, Q4 like this. Just say the main title of what you want me to answer about."
    name: String,
}

/// What the popover's actions do with the chat box.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ReplyAction {
    AddToPrompt,
    Send,
}

/// Active compact mention menu (`*` Boosts, `+` agents, `/` or `$` skills) inside [`ReplyPopover`].
struct ReplyMentionMenu {
    kind: String,
    trigger_start: usize,
    heading: String,
    rows: Vec<Value>,
    status: Option<String>,
    selected: usize,
    scroll: ScrollHandle,
}

impl ReplyMentionMenu {
    /// Unscaled height the compact menu takes inside the popover, capped at [`MENTION_MAX_HEIGHT`].
    fn height(&self) -> f32 {
        let mut h = MENTION_PADDING * 2.0 + MENTION_HEADING_HEIGHT + 1.0;
        if self.status.is_some() {
            h += MENTION_ROW_HEIGHT;
        }
        for row in &self.rows {
            if row["section"].is_string() {
                h += MENTION_HEADING_HEIGHT;
            }
            h += MENTION_ROW_HEIGHT;
        }
        h.min(MENTION_MAX_HEIGHT)
    }
}

/// Anchor context kept on [`ReplyPopover`] so the card can grow/shrink cleanly when its compact
/// mention menu opens or closes.
struct PopoverPlacement {
    at: Point<Pixels>,
    within: Bounds<Pixels>,
    content_origin: Point<Pixels>,
    parent_view: *mut std::ffi::c_void,
    width: f32,
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
    placement: PopoverPlacement,
    /// Current unscaled window height, tracked so we only resize when the menu height changes.
    window_height: f32,
    closed: bool,
    was_active: bool,
    /// True until the press that opened the card has aged out (`OPENING_PRESS`).
    settling: bool,
    /// How many times this card has taken key status back.
    rekeys: u32,
    /// Where its words are kept until sent (`KEPT_REPLIES`), and whether its row was open on the
    /// dock or the board when it opened; `None` for a quoted selection.
    kept: Option<(String, String, bool)>,
    /// The paperclip's file picker is open: the key it takes is not a click away.
    picking: bool,
    /// The field's pills (its pictures, Boosts, sessions, skills), parsed from the text.
    references: Vec<super::composer_references::ComposerReference>,
    reference_draft: Option<String>,
    /// Active `*` / `+` / `/` / `$` mention menu, or `None` when no trigger matches or dismissed.
    mention_menu: Option<ReplyMentionMenu>,
    /// Last `(text, utf16_caret)` queried against `replyMentions`.
    last_mention_query: Option<(String, usize)>,
    /// Set when the parent chat notifies (e.g. Boosts or skills finished loading).
    mentions_dirty: bool,
    /// Trigger dismissed with Escape: `(kind, trigger_start)`.
    dismissed_trigger: Option<(String, usize)>,
    /// Trigger start for which `refreshBoosts` was already asked.
    last_boosts_refresh: Option<usize>,
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
        let question = question_reply_of(command);
        let quote = command["quote"].as_str().unwrap_or_default();
        if quote.is_empty() && question.is_none() {
            return;
        }
        self.close_reply_popover(cx);
        self.reply_popover.last = Some((window.window_handle(), command.clone()));
        // Opened again after a failed send: the words typed, and why it did not go. Else what was
        // typed on this row before and not sent (`KEPT_REPLIES`).
        // Every Reply card, a quoted selection's too, keeps the session's one draft.
        let kept = {
            let row = SESSION_REPLY_ROW.to_owned();
            Some((self.kept_reply_key(&row), row, false))
        };
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
        let width = card_width(question.is_some(), &appearance, window);
        let base_height = HEIGHT + if !error.is_empty() { ERROR_LINE } else { 0.0 };
        let anchor = content.origin + at;
        let bounds = popover_bounds_sized(anchor, within, width, base_height, scale);
        let parent = super::child_window::window_native_view(window)
            .unwrap_or(self.config.parent_native_view);
        let display_id =
            crate::app::window::popup_frame::PopupOwner::of(window, cx).display_for(bounds, cx);
        let glass = crate::app::helpers::window_glass_active();
        let corner_radius =
            crate::app::window::popup_frame::popup_card_radius(px(RADIUS * scale), glass);
        let chat = cx.weak_entity();
        let quote = quote.to_owned();
        let placement = PopoverPlacement {
            at: anchor,
            within,
            content_origin: content.origin,
            parent_view: parent,
            width,
        };
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
                                chat,
                                quote,
                                question,
                                typed,
                                error,
                                kept,
                                appearance,
                                placement,
                                base_height,
                                window,
                                cx,
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
                    chat.error = Some(crate::app::native_chat::child_window::window_open_failed(
                        error,
                    ));
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
                // The session's one draft goes only when it is sent.
                if row == SESSION_REPLY_ROW {
                    return true;
                }
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
            // A draft kept per row before one draft per session becomes the session's one when it
            // has none yet, and the old rows go.
            let mut legacy = Vec::new();
            let mut adopted = false;
            {
                let mut kept = kept_replies();
                let shared = format!("{scope}{SESSION_REPLY_ROW}");
                // `get`, never `drafts[..]`: a `Map` index panics on a missing row, and a session
                // with no Reply draft yet (`{}`) or only older per-question ones has none.
                if let Some(text) = drafts
                    .get(SESSION_REPLY_ROW)
                    .and_then(serde_json::Value::as_str)
                    .filter(|text| !text.trim().is_empty())
                {
                    kept.entry(shared.clone()).or_insert_with(|| KeptReply {
                        text: text.to_owned(),
                        seen: false,
                    });
                }
                for (row, text) in drafts {
                    if row == SESSION_REPLY_ROW {
                        continue;
                    }
                    legacy.push(row.clone());
                    if let Some(text) = text.as_str().filter(|text| !text.trim().is_empty())
                        && !kept.contains_key(&shared)
                    {
                        kept.insert(
                            shared.clone(),
                            KeptReply {
                                text: text.to_owned(),
                                seen: false,
                            },
                        );
                        adopted = true;
                    }
                }
            }
            if adopted {
                chat.reply_popover
                    .unsaved
                    .insert(SESSION_REPLY_ROW.to_owned());
                chat.save_kept_replies(cx);
            }
            if !legacy.is_empty() {
                chat.forget_kept_replies(legacy, cx);
            }
            chat.reply_popover.check_loaded = true;
            chat.prune_kept_replies(&serde_json::Value::Null, cx);
            cx.notify();
        });
    }

    /// The session's Reply draft as kept now (`SESSION_REPLY_ROW`).
    pub(super) fn kept_session_reply(&self) -> String {
        kept_replies()
            .get(&self.kept_reply_key(SESSION_REPLY_ROW))
            .map(|draft| draft.text.clone())
            .unwrap_or_default()
    }

    /// Whether the session currently has unsent text in its Reply draft, driving the top-right dot
    /// on inline Reply buttons under questions (never on the Inbox dock's Reply buttons).
    pub(super) fn has_reply_draft(&self) -> bool {
        !self.kept_session_reply().trim().is_empty()
    }

    /// Keeps `text` as the session's Reply draft and saves it soon: words or pictures that
    /// arrived for a Reply card (Dictate, a picture being saved) are kept whether or not the card
    /// is still open.
    pub(super) fn keep_session_reply(&mut self, text: &str, cx: &mut Context<Self>) {
        let had_draft = self.has_reply_draft();
        keep_reply(&self.kept_reply_key(SESSION_REPLY_ROW), text, false);
        self.note_reply_edit(SESSION_REPLY_ROW.to_owned(), cx);
        if had_draft != self.has_reply_draft() {
            cx.notify();
        }
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
        let had_draft = self.has_reply_draft();
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
        if had_draft != self.has_reply_draft() {
            cx.notify();
        }
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
            // Reply in chat box puts what it answers in the chat box as a pill inside the text,
            // by its title, with the words after it; the tags ride in the pill's link only. It
            // goes also with no words yet, to say them in the chat box (by voice, with pictures,
            // or beside other answers). (`questions::answer_pills`, CDXC:Coordinators 2026-11-16.)
            if action == ReplyAction::AddToPrompt {
                if let Some(pill) = answer_pill(question) {
                    self.append_prompt(&format!("{pill}{reply}"), cx);
                    return;
                }
            }
            if reply.is_empty() {
                return;
            }
            // A whole form or page: the words go to the agent after its open tags, and its
            // questions stay open.
            if let Some((id, to)) = &question.questions {
                let words = if to.is_empty() {
                    reply.to_owned()
                } else {
                    format!("{to}: {reply}")
                };
                match action {
                    ReplyAction::Send => self.invoke(
                        serde_json::json!({"type": "questionsReply", "id": id, "text": words}),
                        cx,
                    ),
                    ReplyAction::AddToPrompt => self.append_prompt(&words, cx),
                }
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

/// What a Reply command answers (a question, a work item ask, a whole form or page), `None` for a
/// quoted selection's Reply.
pub(super) fn question_reply_of(command: &Value) -> Option<QuestionReply> {
    // A work item with no open question has no tag: its words go to the item ("W7: words").
    let item = command["question"]["item"].as_str().unwrap_or_default();
    let questions = command["question"]["questions"]
        .as_str()
        .filter(|id| !id.is_empty())
        .map(|id| {
            (
                id.to_owned(),
                command["question"]["to"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
            )
        });
    command["question"]["tag"]
        .as_str()
        .filter(|tag| !tag.is_empty() || !item.is_empty() || questions.is_some())
        .map(|tag| QuestionReply {
            name: command["question"]["name"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            questions: questions.clone(),
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
        })
}

/// The pill Reply in chat box puts in the chat box for `question`: what it answers by its title,
/// its tags riding in the pill's link only (`questions::answer_pills`). `None` when it names no
/// tag, as a page that only waits to be seen.
pub(super) fn answer_pill(question: &QuestionReply) -> Option<String> {
    let (tags, settles) = match (&question.questions, &question.work) {
        (Some(_), _) => (question.tag.clone(), false),
        (None, Some((item, _))) => (format!("{item} {}", question.tag), true),
        (None, None) => (question.tag.clone(), true),
    };
    ghostex_gx_chat_core::questions::answer_pills::pill(&question.name, &tags, settles)
}

/// The card's width before the chat's zoom: `WIDTH`, or what its button row takes when that is
/// more. The key hint is the same narrow ⌥⏎ glyphs on every platform.
fn card_width(question: bool, appearance: &ChatAppearance, window: &Window) -> f32 {
    let scale = appearance.scale;
    let mut style = window.text_style();
    style.font_family = appearance.font.clone().into();
    style.font_weight = FontWeight::MEDIUM;
    let text_width = |text: &str| {
        window
            .text_system()
            .shape_line(
                text.to_owned().into(),
                px(13.0 * scale),
                &[style.to_run(text.len())],
                None,
            )
            .width
            .as_f32()
            / scale
    };
    let (add, send) = action_labels(question);
    let actions: f32 = [(add, ReplyAction::AddToPrompt), (send, ReplyAction::Send)]
        .into_iter()
        .map(|(label, action)| {
            text_width(label)
                + shortcut_label(action).map_or(0.0, |key| text_width(key) + ACTION_GAP)
                + ACTION_PADDING * 2.0
        })
        .sum();
    // Both icons, the status line's own padding, the four gaps and a little air.
    let row = ICON_BUTTON * 2.0 + 8.0 + ROW_GAP * 4.0 + actions + 8.0;
    WIDTH.max((row + PADDING * 2.0).ceil())
}

/// Where the card opens: its top-left corner at the press, kept inside the chat with a margin, and
/// above the press when there is no room below it.
fn popover_bounds_sized(
    at: Point<Pixels>,
    within: Bounds<Pixels>,
    width: f32,
    height: f32,
    scale: f32,
) -> Bounds<Pixels> {
    let margin = px(12.0 * scale);
    let width = px(width * scale).min(within.size.width - margin * 2.0);
    // A compact parent panel (such as the floating Ask modal, ~200pt tall) must not squash the
    // popover when its mention menu opens; allow the popover to grow up to its full menu height.
    let min_allowed = px((HEIGHT + MENTION_MAX_HEIGHT + ERROR_LINE) * scale);
    let max_height = (within.size.height - margin * 2.0).max(min_allowed);
    let height = px(height * scale).min(max_height);
    let x =
        at.x.min(within.right() - width - margin)
            .max(within.left() + margin);
    let y = if within.size.height >= height + margin * 2.0 {
        let candidate = if at.y + px(4.0 * scale) + height < within.bottom() - margin {
            at.y + px(4.0 * scale)
        } else {
            at.y - height - px(4.0 * scale)
        };
        candidate
            .min(within.bottom() - height - margin)
            .max(within.top() + margin)
    } else {
        // Parent panel is shorter than the expanded popover: keep its top aligned to the panel's
        // top margin so the mention menu expands downward over the chat cleanly.
        within.top() + margin
    };
    Bounds::new(Point::new(x, y), size(width, height))
}

/// Whether losing key status closes the card. A press only counts once the one on the Reply row
/// has aged out, and a window that keeps taking key status back wins after `MAX_REKEYS`.
pub(super) fn key_loss_closes(settling: bool, user_dismissed: bool, rekeys: u32) -> bool {
    rekeys >= option_menu::MAX_REKEYS || (user_dismissed && !settling)
}

/// The reply box's right-click menu: every text field's (window/text_field_menu.rs), with Save as
/// Boost under it while words are selected, as in the chat box (CDXC:ContextMenus 2026-11-27 there).
fn show_reply_menu(
    input: &WeakEntity<TextareaState>,
    chat: &WeakEntity<NativeChatView>,
    window: &mut Window,
    cx: &mut gpui::App,
) {
    use crate::app::window::text_field_menu::{
        TextFieldMenuAction, TextFieldMenuRow, request_for, show_text_field_menu,
    };
    let Some(input) = input.upgrade() else {
        return;
    };
    let state = input.read(cx);
    let request = request_for(
        state.focus_handle(cx),
        state.context_menu_capabilities(),
        Some(state.selected_text().to_string()),
        window,
    );
    let boost = request.selected_text.as_ref().and_then(|selected| {
        chat.upgrade()?
            .read(cx)
            .selection_for_boost(selected.as_ref())
    });
    let extra = boost
        .map(|text| {
            let chat = chat.clone();
            TextFieldMenuRow {
                label: "Save as Boost".into(),
                icon: "titlebar/sparkles.svg",
                disabled: false,
                action: TextFieldMenuAction::Run(std::rc::Rc::new(move |window, cx| {
                    let action = super::actions::NativeChatAction {
                        command: json!({"type": "saveAsBoost", "text": text}),
                    };
                    let _ = chat.update(cx, |chat, cx| chat.handle_action(&action, window, cx));
                })),
            }
        })
        .into_iter()
        .collect();
    show_text_field_menu(&request, extra, window, cx);
}

impl ReplyPopover {
    #[allow(clippy::too_many_arguments)]
    fn new(
        chat: WeakEntity<NativeChatView>,
        quote: String,
        question: Option<QuestionReply>,
        typed: String,
        error: String,
        kept: Option<(String, String, bool)>,
        appearance: ChatAppearance,
        placement: PopoverPlacement,
        window_height: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = match &question {
            Some(question) if !question.name.is_empty() => {
                format!("Reply about {}…", question.name)
            }
            Some(_) => "Your answer…".to_string(),
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
        // CDXC:SessionChat 2026-11-20 DECISION: User: "Enter sends the reply right away, matching
        // the regular chat box"; Option+Enter (Alt+Enter off a Mac) adds it to the chat box (`key`
        // below). Enter is the field's own key (GPUI runs key bindings before key-down
        // listeners): with `submit_on_enter` a plain Enter reports `PressEnter` instead of a new
        // line, Cmd+Enter (Ctrl+Enter off a Mac) reports it as `secondary` and sends too, as in the
        // chat box, and Shift+Enter stays a new line.
        let enter = cx.subscribe(&input, |popover: &mut Self, _, event: &InputEvent, cx| {
            if let InputEvent::PressEnter { shift: false, .. } = event {
                if popover
                    .mention_menu
                    .as_ref()
                    .is_some_and(|menu| !menu.rows.is_empty())
                {
                    return;
                }
                popover.close(Some(ReplyAction::Send), true, cx);
            }
        });
        // Every edit is kept at once (`KEPT_REPLIES`), whatever closes the card after.
        let keep = cx.subscribe(
            &input,
            |popover: &mut Self, input, event: &InputEvent, cx| {
                if let (InputEvent::Change, Some((key, row, seen))) = (event, &popover.kept) {
                    let had = kept_replies().contains_key(key);
                    let value = input.read(cx).value();
                    keep_reply(key, &value, *seen);
                    let has = !value.trim().is_empty();
                    let row = row.clone();
                    let _ = popover.chat.update(cx, |chat, cx| {
                        chat.note_reply_edit(row, cx);
                        if had != has {
                            cx.notify();
                        }
                    });
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
        // Dictate, pictures being saved, and Boosts/skills catalogs are the chat's: the card
        // redraws as they change.
        let mut observers = Vec::new();
        if let Some(view) = chat.upgrade() {
            view.update(cx, |view, _| {
                view.reply_popover.input = Some(input.downgrade());
                view.reply_popover.pending = 0;
                view.reply_popover.attach_error = None;
            });
            observers.push(cx.observe(&view, |popover: &mut Self, _, cx| {
                popover.mentions_dirty = true;
                cx.notify();
            }));
        }
        Self {
            chat,
            quote,
            question,
            error,
            input,
            focus,
            appearance,
            placement,
            window_height,
            closed: false,
            was_active: window.is_window_active(),
            settling: true,
            rekeys: 0,
            kept,
            picking: false,
            references: Vec::new(),
            reference_draft: None,
            mention_menu: None,
            last_mention_query: None,
            mentions_dirty: false,
            dismissed_trigger: None,
            last_boosts_refresh: None,
            _subscriptions: [activation, press, enter, keep]
                .into_iter()
                .chain(observers)
                .collect(),
        }
    }

    /// Key status went elsewhere. The user sent it there (a press since the card opened, or another
    /// app) and the card closes, or something else took it and the card takes it back, as the
    /// chat's menus do (CDXC:SessionChat 2026-09-22 on `ChatOptionMenu::check_active`).
    fn key_lost(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // GPUI reports activation from a queued task, so the report can be out of date already.
        // The paperclip's file picker takes the key while it is open; the card stays.
        if self.closed || self.picking || window.is_window_active() {
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

    /// Escape closes the mention menu first (when open), else the card without touching the chat
    /// box; Option+Enter (Alt+Enter off a Mac) adds the reply to the chat box.
    fn key(&mut self, event: &gpui::KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke;
        if key.key == "escape" {
            cx.stop_propagation();
            window.prevent_default();
            if self.dismiss_mention_menu(window, cx) {
                return;
            }
            self.close(None, true, cx);
        } else if key.key == "enter"
            && key.modifiers.alt
            && !key.modifiers.shift
            && !key.modifiers.control
            && !key.modifiers.platform
        {
            cx.stop_propagation();
            window.prevent_default();
            self.close(Some(ReplyAction::AddToPrompt), true, cx);
        }
    }

    /// Reads the input's text and UTF-16 caret offset for `replyMentions`.
    fn input_text_and_caret(&self, cx: &Context<Self>) -> (String, usize) {
        let input = self.input.read(cx);
        let text = input.value().to_string();
        let offset = input.cursor().min(text.len());
        let caret = text[..offset].encode_utf16().count();
        (text, caret)
    }

    /// Recomputes the compact `*` / `+` / `/` / `$` mention menu from `gx-chat-core` whenever the
    /// text, caret or chat catalogs changed, and resizes the popover window to fit.
    fn sync_mention_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (text, caret) = self.input_text_and_caret(cx);
        let same_query = self
            .last_mention_query
            .as_ref()
            .is_some_and(|(old_text, old_caret)| *old_text == text && *old_caret == caret);
        if same_query && !self.mentions_dirty {
            return;
        }
        self.mentions_dirty = false;
        self.last_mention_query = Some((text.clone(), caret));

        let result = self.chat.upgrade().and_then(|chat| {
            chat.read(cx).runtime.as_ref().and_then(|runtime| {
                runtime.query(
                    "replyMentions",
                    vec![json!(text), json!(caret)],
                    Duration::from_millis(80),
                )
            })
        });

        let Some(popup) = result.filter(Value::is_object) else {
            self.mention_menu = None;
            self.dismissed_trigger = None;
            self.last_boosts_refresh = None;
            self.sync_window_height(window, cx);
            return;
        };

        let kind = popup["kind"].as_str().unwrap_or_default().to_string();
        let trigger_start = popup["triggerStart"].as_u64().unwrap_or(0) as usize;
        if kind == "boost" && self.last_boosts_refresh != Some(trigger_start) {
            self.last_boosts_refresh = Some(trigger_start);
            let _ = self.chat.update(cx, |chat, cx| {
                chat.invoke(json!({"type": "refreshBoosts"}), cx)
            });
        }
        if self
            .dismissed_trigger
            .as_ref()
            .is_some_and(|(d_kind, d_start)| *d_kind == kind && *d_start == trigger_start)
        {
            self.mention_menu = None;
            self.sync_window_height(window, cx);
            return;
        }
        if self
            .dismissed_trigger
            .as_ref()
            .is_some_and(|(d_kind, d_start)| *d_kind != kind || *d_start != trigger_start)
        {
            self.dismissed_trigger = None;
        }

        let rows = popup["rows"].as_array().cloned().unwrap_or_default();
        let heading = popup["heading"].as_str().unwrap_or_default().to_string();
        let status = popup["status"].as_str().map(str::to_string);
        let same_trigger = self.mention_menu.as_ref().is_some_and(|menu| {
            menu.kind == kind && menu.trigger_start == trigger_start && same_query
        });
        let selected = if same_trigger {
            self.mention_menu
                .as_ref()
                .map_or(0, |menu| menu.selected)
                .min(rows.len().saturating_sub(1))
        } else {
            0
        };
        let scroll = self
            .mention_menu
            .take()
            .filter(|menu| menu.kind == kind && menu.trigger_start == trigger_start)
            .map(|menu| menu.scroll)
            .unwrap_or_default();
        self.mention_menu = Some(ReplyMentionMenu {
            kind,
            trigger_start,
            heading,
            rows,
            status,
            selected,
            scroll,
        });
        self.sync_window_height(window, cx);
    }

    fn desired_window_height(&self) -> f32 {
        let base = HEIGHT
            + if !self.error.is_empty() {
                ERROR_LINE
            } else {
                0.0
            };
        let menu = self
            .mention_menu
            .as_ref()
            .map_or(0.0, ReplyMentionMenu::height);
        base + menu
    }

    /// Grows or shrinks the Reply popover window cleanly as the compact mention menu opens, filters
    /// or closes, keeping the card inside `placement.within`.
    fn sync_window_height(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = self.desired_window_height();
        if (self.window_height - target).abs() < 0.5 {
            return;
        }
        self.window_height = target;
        let scale = self.appearance.scale;
        let next = popover_bounds_sized(
            self.placement.at,
            self.placement.within,
            self.placement.width,
            target,
            scale,
        );
        let frame_in_parent = Bounds::new(next.origin - self.placement.content_origin, next.size);
        let moved = super::child_window::move_child_window(
            window.window_handle(),
            self.placement.parent_view,
            frame_in_parent,
            cx,
        );
        if !moved {
            window.resize(next.size);
        }
    }

    fn dismiss_mention_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(menu) = self.mention_menu.take() else {
            return false;
        };
        self.dismissed_trigger = Some((menu.kind, menu.trigger_start));
        self.sync_window_height(window, cx);
        cx.notify();
        true
    }

    /// Handles `up`, `down`, `enter`, `tab`, `escape` while `"ReplyPopoverMenu"` is active.
    fn mention_menu_key(
        &mut self,
        action: &super::keyboard::ReplyPopoverMenuKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = self.mention_menu.as_mut() else {
            return;
        };
        match action.key.as_str() {
            "escape" => {
                cx.stop_propagation();
                window.prevent_default();
                self.dismiss_mention_menu(window, cx);
            }
            "up" | "down" => {
                if menu.rows.is_empty() {
                    return;
                }
                cx.stop_propagation();
                window.prevent_default();
                let count = menu.rows.len();
                menu.selected = if action.key == "up" {
                    (menu.selected + count - 1) % count
                } else {
                    (menu.selected + 1) % count
                };
                let scroll_index = menu.selected + 1 + usize::from(menu.status.is_some());
                menu.scroll.scroll_to_item(scroll_index);
                cx.notify();
            }
            "enter" | "tab" => {
                if menu.rows.is_empty() {
                    return;
                }
                let index = menu.selected;
                cx.stop_propagation();
                window.prevent_default();
                self.pick_mention(index, window, cx);
            }
            _ => {}
        }
    }

    /// Inserts the picked Boost, agent or skill token (matching the main composer's exact text and
    /// spacing) into the reply box, restores the caret right after it, and closes the menu.
    fn pick_mention(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let (text, caret) = self.input_text_and_caret(cx);
        let completed = self.chat.upgrade().and_then(|chat| {
            chat.read(cx).runtime.as_ref().and_then(|runtime| {
                runtime.query_for_gesture(
                    "completeReplyMention",
                    vec![json!(text), json!(caret), json!(index)],
                    Duration::from_millis(200),
                )
            })
        });
        let Some((next_text, next_caret)) = completed.as_ref().and_then(|result| {
            Some((
                result["text"].as_str()?.to_owned(),
                result["caret"].as_u64()? as usize,
            ))
        }) else {
            return;
        };
        let byte_caret = super::form_attachments::utf16_to_byte(&next_text, next_caret);
        self.input.update(cx, |input, cx| {
            input.replace_all(next_text.clone(), window, cx);
            input.set_selected_range(byte_caret..byte_caret, cx);
            input.focus_handle(cx).focus(window, cx);
        });
        if let Some((key, row, seen)) = &self.kept {
            let had = kept_replies().contains_key(key);
            keep_reply(key, &next_text, *seen);
            let has = !next_text.trim().is_empty();
            let row = row.clone();
            let _ = self.chat.update(cx, |chat, cx| {
                chat.note_reply_edit(row, cx);
                if had != has {
                    cx.notify();
                }
            });
        }
        self.mention_menu = None;
        self.dismissed_trigger = None;
        self.last_mention_query = Some((next_text, next_caret));
        self.sync_window_height(window, cx);
        cx.notify();
    }

    /// Draws the compact mention menu (`*` Boosts, `+` agents, `/` or `$` skills) inside the Reply
    /// popover card, reusing the composer suggestions' icons and section headers.
    fn render_mention_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.mention_menu.as_ref()?;
        let p = &self.appearance;
        let s = p.scale;
        let (hover, edge) = menu_ink_washes(p.light);
        let sessions = menu.kind == "session";
        let boosts = menu.kind == "boost";
        let selected = menu.selected;
        let heading_row = |id: gpui::ElementId, text: String| {
            div()
                .id(id)
                .role(gpui::Role::Heading)
                .aria_label(text.clone())
                .h(px(MENTION_HEADING_HEIGHT * s))
                .px(px(6.0 * s))
                .flex()
                .items_center()
                .text_size(px(10.0 * s))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.muted)
                .child(text.to_uppercase())
        };
        let mut list = div()
            .id("reply-mention-list")
            .role(gpui::Role::ListBox)
            .aria_label(match menu.kind.as_str() {
                "boost" => "Boosts",
                "session" => "Agents",
                _ => "Available skills",
            })
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&menu.scroll)
            .py(px(MENTION_PADDING * s))
            .child(heading_row(
                "reply-mention-heading".into(),
                menu.heading.clone(),
            ));
        if let Some(status) = &menu.status {
            list = list.child(
                div()
                    .id("reply-mention-status")
                    .h(px(MENTION_ROW_HEIGHT * s))
                    .px(px(6.0 * s))
                    .flex()
                    .items_center()
                    .text_size(px(12.0 * s))
                    .text_color(p.muted)
                    .child(status.clone()),
            );
        }
        for (index, row) in menu.rows.iter().enumerate() {
            let label = row["label"].as_str().unwrap_or_default().to_owned();
            let detail = row["detail"].as_str().unwrap_or_default().to_owned();
            let section = row["section"].as_str().map(str::to_owned);
            let working = row["working"] == true;
            let logo = sessions.then(|| {
                let (logo, color) = super::markdown_links::session_pill_icon(
                    row["agentIcon"].as_str().unwrap_or_default(),
                    row["coordinator"] == true,
                    p,
                );
                let color = if row["mainCoordinator"] == true {
                    rgb(if p.light {
                        crate::app::native_sidebar::threads::MAIN_COORDINATOR_GOLD_LIGHT
                    } else {
                        crate::app::native_sidebar::threads::MAIN_COORDINATOR_GOLD_DARK
                    })
                    .into()
                } else {
                    color
                };
                (logo, color)
            });
            let option = div()
                .id(("reply-mention-row", index))
                .role(gpui::Role::ListBoxOption)
                .aria_selected(index == selected)
                .aria_label(format!("{label} {detail}"))
                .flex()
                .items_center()
                .gap(px(6.0 * s))
                .w_full()
                .min_w_0()
                .h(px(MENTION_ROW_HEIGHT * s))
                .px(px(6.0 * s))
                .rounded(px(6.0 * s))
                .chat_cursor_pointer()
                .when(index == selected, |el| el.bg(hover))
                .when(boosts, |el| {
                    el.child(
                        gpui::svg()
                            .path("titlebar/sparkles.svg")
                            .size(px(13.0 * s))
                            .flex_shrink_0()
                            .text_color(super::boosts::accent(p.light)),
                    )
                })
                .when_some(logo, |el, (logo, color)| {
                    el.child(
                        gpui::svg()
                            .path(logo)
                            .size(px(13.0 * s))
                            .flex_shrink_0()
                            .text_color(color),
                    )
                })
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .flex_shrink_0()
                        .max_w(gpui::relative(0.55))
                        .font_weight(FontWeight::MEDIUM)
                        .text_size(px(12.5 * s))
                        .text_color(p.foreground)
                        .child(label),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(11.5 * s))
                        .text_color(if working {
                            rgb(super::chat_pills::ATTENTION_COLOR).into()
                        } else {
                            p.muted
                        })
                        .child(detail),
                )
                .on_mouse_move(cx.listener(move |popover, _, _, cx| {
                    if let Some(menu) = popover.mention_menu.as_mut()
                        && menu.selected != index
                    {
                        menu.selected = index;
                        cx.notify();
                    }
                }))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(move |popover, _, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        popover.pick_mention(index, window, cx);
                    }),
                );
            list = match section {
                Some(section) => list.child(
                    div()
                        .flex()
                        .flex_col()
                        .w_full()
                        .child(heading_row(
                            ("reply-mention-section", index).into(),
                            section,
                        ))
                        .child(option),
                ),
                None => list.child(option),
            };
        }
        Some(
            div()
                .w_full()
                .min_w_0()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .border_t_1()
                .border_color(edge)
                .child(list)
                .into_any_element(),
        )
    }

    /// The field's picture pills, drawn as the chat box draws them.
    fn sync_references(&mut self, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().to_string();
        if self.reference_draft.as_deref() == Some(text.as_str()) {
            return;
        }
        let parsed = serde_json::to_value(
            ghostex_gx_chat_core::composer::queries::composer_references(&text),
        )
        .unwrap_or(serde_json::Value::Null);
        self.references = super::composer_references::parse(&text, &parsed);
        self.reference_draft = Some(text);
        let replacements =
            super::composer_references::replacements(&self.references, &self.appearance, 13.0);
        self.input.update(cx, |input, cx| {
            input.set_inline_replacements(replacements, cx)
        });
    }

    /// The microphone: Dictate records into this field (reply_attachments.rs), through the chat's
    /// own window, which holds the recording.
    fn toggle_dictation(&mut self, cx: &mut Context<Self>) {
        let chat = self.chat.clone();
        cx.defer(move |cx| {
            let Some(main) = chat.upgrade().and_then(|chat| chat.read(cx).main_window) else {
                return;
            };
            let _ = main.update(cx, |_, window, cx| {
                let _ = chat.update(cx, |chat, cx| chat.toggle_reply_dictation(window, cx));
            });
        });
    }

    /// The paperclip: pictures or files picked are saved into the field as pasted ones are.
    fn pick_files(&mut self, cx: &mut Context<Self>) {
        self.picking = true;
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: None,
        });
        let input = self.input.clone();
        cx.spawn(async move |popover, cx| {
            let picked = paths
                .await
                .ok()
                .and_then(Result::ok)
                .flatten()
                .unwrap_or_default();
            let _ = popover.update(cx, |popover, cx| {
                popover.picking = false;
                let chat = popover.chat.clone();
                cx.defer(move |cx| {
                    let Some(main) = chat.upgrade().and_then(|chat| chat.read(cx).main_window)
                    else {
                        return;
                    };
                    let _ = main.update(cx, |_, window, cx| {
                        let _ = chat.update(cx, |chat, cx| {
                            chat.attach_reply_paths(picked, input, window, cx)
                        });
                    });
                });
            });
        })
        .detach();
    }

    fn icon_button(
        &self,
        id: &'static str,
        icon: &'static str,
        label: &'static str,
        live: bool,
        cx: &mut Context<Self>,
        press: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        let p = &self.appearance;
        let s = p.scale;
        let (hover, _) = menu_ink_washes(p.light);
        div()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(label)
            .chat_cursor_pointer()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .size(px(ICON_BUTTON * s))
            .rounded(px(6.0 * s))
            .when(live, |button| button.bg(hover))
            .hover(move |button| button.bg(hover))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(label).build(window, cx)
            })
            .child(
                gpui::svg()
                    .path(icon)
                    .size(px(15.0 * s))
                    .text_color(if live { p.ring } else { p.muted }),
            )
            .on_click(cx.listener(move |popover, _, _, cx| press(popover, cx)))
            .into_any_element()
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
            .aria_label(match action {
                ReplyAction::Send => format!("{label} (Enter)"),
                ReplyAction::AddToPrompt => format!("{label} ({}+Enter)", option_key_name()),
            })
            .chat_cursor_pointer()
            .flex()
            .min_w_0()
            .overflow_hidden()
            .items_center()
            .gap(px(ACTION_GAP * s))
            .h(px(26.0 * s))
            .px(px(ACTION_PADDING * s))
            .rounded(px(6.0 * s))
            .hover(move |row| row.bg(hover))
            .child(div().flex_shrink_0().text_color(p.foreground).child(label))
            // In a chat too narrow for the whole row the key gives way first.
            .when_some(shortcut, |button, shortcut| {
                button.child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(p.muted)
                        .child(shortcut),
                )
            })
            .on_click(cx.listener(move |popover, _, _, cx| popover.close(Some(action), true, cx)))
            .into_any_element()
    }
}

impl Render for ReplyPopover {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.appearance.clone();
        let s = p.scale;
        let (_, edge) = menu_ink_washes(p.light);
        let error_color = p.error();
        self.sync_references(cx);
        self.sync_mention_menu(window, cx);
        let menu_open = self.mention_menu.is_some();
        let mention_menu = self.render_mention_menu(cx);
        let paste = NativeChatView::reply_paste_handler(self.chat.clone(), self.input.clone());
        // Dictate, and the pictures being saved, as the chat holds them (reply_attachments.rs).
        let (dictation, pending, attach_error) = self
            .chat
            .upgrade()
            .map(|chat| {
                let chat = chat.read(cx);
                (
                    chat.reply_dictation(cx),
                    chat.reply_popover.pending,
                    chat.reply_popover.attach_error.clone(),
                )
            })
            .unwrap_or((super::reply_attachments::ReplyDictation::Idle, 0, None));
        let status = match (dictation, pending, attach_error) {
            (super::reply_attachments::ReplyDictation::Transcribing, _, _) => {
                Some(("Transcribing\u{2026}".to_string(), false))
            }
            (super::reply_attachments::ReplyDictation::Recording, _, _) => {
                Some(("Listening\u{2026}".to_string(), false))
            }
            (_, pending, _) if pending > 0 => {
                Some(("Saving the picture\u{2026}".to_string(), false))
            }
            (_, _, Some(error)) => Some((error, true)),
            _ => None,
        };
        div()
            .id("chat-reply-popover")
            .role(gpui::Role::Dialog)
            .aria_label("Reply")
            .track_focus(&self.focus)
            .when(menu_open, |card| {
                card.key_context("ReplyPopoverMenu")
                    .capture_action(cx.listener(Self::mention_menu_key))
            })
            .size_full()
            .flex()
            .flex_col()
            .justify_between()
            .rounded(px(RADIUS * s))
            .border_1()
            .border_color(edge)
            .bg(p.menu_surface())
            .p(px(PADDING * s))
            .font_family(p.font)
            .text_size(px(13.0 * s))
            .text_color(p.foreground)
            .on_key_down(cx.listener(Self::key))
            .child(
                div().flex_shrink_0().px(px(4.0 * s)).pt(px(2.0 * s)).child(
                    Textarea::new(&self.input)
                        .context_menu({
                            let input = self.input.downgrade();
                            let chat = self.chat.clone();
                            move |menu, window, cx| {
                                show_reply_menu(&input, &chat, window, cx);
                                menu
                            }
                        })
                        .on_paste(paste)
                        .appearance(false)
                        .bordered(false)
                        .caret_color(p.foreground)
                        .text_size(px(13.0 * s)),
                ),
            )
            .children(mention_menu)
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
                // The status line takes the slack, so the actions sit at the right. No
                // `justify_end`: a row wider than the card then spilled out past its left border,
                // taking the microphone and paperclip with it; now its right end is cut instead.
                div()
                    .flex_shrink_0()
                    .pt(px(4.0 * s))
                    .flex()
                    .min_w_0()
                    .overflow_hidden()
                    .items_center()
                    .gap(px(ROW_GAP * s))
                    .font_weight(FontWeight::MEDIUM)
                    .child(self.icon_button(
                        "chat-reply-dictate",
                        "titlebar/microphone.svg",
                        match dictation {
                            super::reply_attachments::ReplyDictation::Idle => "Dictate",
                            _ => "Stop dictating",
                        },
                        dictation != super::reply_attachments::ReplyDictation::Idle,
                        cx,
                        |popover, cx| popover.toggle_dictation(cx),
                    ))
                    .child(self.icon_button(
                        "chat-reply-attach",
                        "titlebar/paperclip.svg",
                        "Attach pictures or files",
                        false,
                        cx,
                        |popover, cx| popover.pick_files(cx),
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .px(px(4.0 * s))
                            .truncate()
                            .font_weight(FontWeight::NORMAL)
                            .text_size(px(12.0 * s))
                            .when_some(status, |line, (status, error)| {
                                line.text_color(if error { error_color } else { p.muted })
                                    .child(status)
                            }),
                    )
                    .child(self.action_button(
                        "chat-reply-add",
                        action_labels(self.question.is_some()).0,
                        ReplyAction::AddToPrompt,
                        cx,
                    ))
                    .child(self.action_button(
                        "chat-reply-send",
                        action_labels(self.question.is_some()).1,
                        ReplyAction::Send,
                        cx,
                    )),
            )
    }
}

/// The two actions' labels: a question's answer, or a quoted reply.
fn action_labels(question: bool) -> (&'static str, &'static str) {
    if question {
        ("Reply in chat box", "Send answer")
    } else {
        ("Add to prompt", "Send reply")
    }
}

/// The action's key hint, drawn as glyphs on every platform (never "Alt+Enter") so it stays
/// narrow: Add to prompt shows ⌥⏎; Send shows none, Enter being the chat box's own send key.
fn shortcut_label(action: ReplyAction) -> Option<&'static str> {
    match action {
        ReplyAction::AddToPrompt => Some("⌥⏎"),
        ReplyAction::Send => None,
    }
}

/// The Add to prompt key's name for a screen reader: Option on a Mac keyboard, Alt elsewhere.
fn option_key_name() -> &'static str {
    if super::keyboard::chat_mac_keys() {
        "Option"
    } else {
        "Alt"
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
