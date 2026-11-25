//! History: the icon pill in the chat box's pill strip, right of the scroll to bottom arrow
//! (pills_strip.rs), and the popover it opens above the chat box: every message the user sent in
//! this chat as cards, oldest at the top and opening at the newest, under a search field and the
//! All / With attachments / Text only filters. A card's click jumps the transcript to its message.
//! Pointing at a card shows Copy and ⋯ at its top right; the ⋯ menu names the rest: Copy reference
//! and Reference in chat, Jump to message and Put back in the chat box, Rewind to here and Fork
//! from here, the rows and look of the ⋯ under a message (message_actions.rs).
//!
//! CDXC:SessionChat 2026-11-26 DECISION:
//! User, of the seven bare icons on a card: "i want 3 dots here next to the copy button on the top
//! right corner so i know what each one does. only show copy and 3 dots on the top right of each
//! card pls". They then picked the order that matches the ⋯ under a message (references first),
//! and kept the pencil for Put back, since the back arrow is Rewind's. The menu is drawn inside
//! the History menu's window, because a second chat menu would close History.
//!
//! The list, its search and its pages are the core's (`sentHistory`,
//! packages/gx-chat-core/src/composer/sent_history.rs, where the user's decision lives); this draws
//! it in the pills' menu window (option_menu/pill_menu.rs), as Files and links is drawn
//! (files_links.rs).
//!
//! CDXC:SessionChat 2026-11-26 SEE-ALSO:
//! The phone draws the same pill and list in apps/mobile/app/src/chat/native/cards/SentHistorySheet.tsx. Change them together.

use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Bounds, Context, InteractiveElement as _, IntoElement,
    ListAlignment, ListState, ParentElement as _, Pixels, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, anchored, canvas, deferred, div, list,
    px, svg,
};
use gpui_component::input::{Input, InputEvent, InputState};
use serde_json::{Value, json};

/// The menu's id among the pill menus, and its trigger's.
pub(super) const SENT_MENU: &str = "sentHistory";
pub(super) const SENT_TRIGGER: &str = "chat-pill-history";
pub(super) const SENT_MENU_WIDTH: f32 = 440.0;

/// The menu's blocks, unscaled.
const GAP: f32 = 8.0;
const FIELD_HEIGHT: f32 = 30.0;
const CHIPS_HEIGHT: f32 = 24.0;
const LINE_HEIGHT: f32 = 22.0;
const LIST_MAX: f32 = 380.0;
/// A card's parts, unscaled: what the menu's height is worked out from before it opens.
const CARD_PAD_Y: f32 = 9.0;
const META_HEIGHT: f32 = 22.0;
const TEXT_LINE: f32 = 19.0;
const ATTACH_HEIGHT: f32 = 44.0;
const CARD_GAP: f32 = 8.0;
/// A picture's thumbnail on a card, unscaled.
const THUMB: f32 = 36.0;
/// Cards from the list's top at which the next older page is asked for.
const MORE_AHEAD: usize = 5;
/// Characters a card's line holds, for the height estimate.
const LINE_CHARS: usize = 58;
/// A card's ⋯ menu, unscaled: the message menu's metrics (option_menu/geometry.rs `REGULAR`).
const CARD_MENU_PADDING: f32 = 6.0;
const CARD_MENU_ROW: f32 = 18.2 + 8.0 * 2.0;
const CARD_MENU_SEPARATOR: f32 = 13.0;
const CARD_MENU_MIN_WIDTH: f32 = 200.0;

/// One row of a card's ⋯ menu, or the line between two groups.
enum CardMenuRow {
    Action(&'static str, &'static str, Value),
    Separator,
}

/// A card's estimated height (unscaled): its time line, up to three lines of words and the row of
/// pictures and files.
fn card_height(item: &Value) -> f32 {
    let preview = text(item, "preview");
    let lines: usize = preview
        .lines()
        .map(|line| line.chars().count().div_ceil(LINE_CHARS).max(1))
        .sum::<usize>()
        .clamp(1, 3);
    let attached = item["images"].as_array().is_some_and(|all| !all.is_empty())
        || item["files"].as_array().is_some_and(|all| !all.is_empty());
    CARD_PAD_Y * 2.0
        + META_HEIGHT
        + 4.0
        + lines as f32 * TEXT_LINE
        + if attached { ATTACH_HEIGHT } else { 0.0 }
        + CARD_GAP
}

/// The menu's height (unscaled) while the list is open, the list capped at `LIST_MAX`.
pub(super) fn sent_menu_height(history: &Value) -> Option<f32> {
    if history["open"] != true {
        return None;
    }
    // Summed only until the list is full: a long list is capped anyway.
    let mut list = 0.0;
    for item in history["items"].as_array().into_iter().flatten().rev() {
        list += card_height(item);
        if list >= LIST_MAX {
            break;
        }
    }
    let list = if list == 0.0 {
        LINE_HEIGHT * 2.0
    } else {
        list.min(LIST_MAX)
    };
    Some(
        super::chat_pills::MENU_HEADER_HEIGHT
            + GAP
            + FIELD_HEIGHT
            + GAP
            + CHIPS_HEIGHT
            + GAP
            + list.max(LINE_HEIGHT * 2.0),
    )
}

impl NativeChatView {
    /// The History pill: an icon, "History" on hover, the list on click. Shown once the chat has a
    /// message the user sent (`sentHistory.enabled`).
    pub(super) fn sent_history_pill(
        &self,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        if self.snapshot["sentHistory"]["enabled"] != true {
            return None;
        }
        let open = self.chat_menu_is_open(SENT_TRIGGER);
        let measured = self.sent_history_pill_bounds.clone();
        let s = p.scale;
        Some(
            super::chat_pills::icon_pill(
                SENT_TRIGGER,
                super::chat_pills::pill_glyph("titlebar/history.svg", p.primary, s),
                p,
            )
            .relative()
            .aria_label("History")
            .aria_expanded(open)
            .border_color(p.foreground.opacity(0.1))
            .when(open, |pill| pill.bg(p.foreground.opacity(0.08)))
            .hover(|style| style.bg(p.foreground.opacity(0.06)))
            .tooltip(|window, cx| {
                gpui_component::tooltip::Tooltip::new("History").build(window, cx)
            })
            .on_click(cx.listener(|chat, _, window, cx| {
                cx.stop_propagation();
                chat.toggle_sent_history(window, cx);
            }))
            .child(
                canvas(move |bounds, _, _| measured.set(bounds), |_, _, _, _| {})
                    .absolute()
                    .inset(px(-s)),
            )
            .into_any_element(),
        )
    }

    /// The pill's click: closes an open list, or asks the core for the newest page and opens the
    /// menu above the chat box, lined up with the pill.
    pub(super) fn toggle_sent_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.chat_menu_toggled_shut(SENT_TRIGGER, cx) {
            return;
        }
        if self.snapshot["sentHistory"]["open"] != true {
            self.invoke(json!({"type": "sentHistory"}), cx);
        }
        self.sent_history_opening = true;
        self.open_sent_history_menu(window, cx);
    }

    /// The menu's height (unscaled), capped at 60% of the pane like Files and links. While the
    /// list is opening (the core's snapshot does not say so yet) it is the loading one, so the
    /// menu is not closed for having no panel (`pill_menu_changed`).
    pub(super) fn sent_history_menu_height(&self) -> Option<f32> {
        let history = &self.snapshot["sentHistory"];
        let height = sent_menu_height(history).or_else(|| {
            self.sent_history_opening
                .then(|| sent_menu_height(&json!({"open": true})))
                .flatten()
        })?;
        let scale = ChatAppearance::current(&self.snapshot).scale;
        let pane = f32::from(self.bounds.get().size.height) / scale;
        let height = height.min((pane * 0.6).max(220.0));
        // An open card menu needs room under the card's top line, so a short list grows for it.
        let menu = self.sent_card_menu_item().map(|item| {
            super::chat_pills::MENU_HEADER_HEIGHT
                + GAP * 3.0
                + FIELD_HEIGHT
                + CHIPS_HEIGHT
                + CARD_PAD_Y
                + META_HEIGHT
                + self.sent_card_menu_height(&item)
                + 12.0
        });
        Some(menu.map_or(height, |menu| height.max(menu)))
    }

    /// The card whose ⋯ menu is open, while it is still listed.
    fn sent_card_menu_item(&self) -> Option<Value> {
        let open = self.sent_history_card_menu.as_deref()?;
        self.snapshot["sentHistory"]["items"]
            .as_array()?
            .iter()
            .rfind(|item| item["id"] == open)
            .cloned()
    }

    fn open_sent_history_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pill: Bounds<Pixels> = self.sent_history_pill_bounds.get();
        let Some(height) = self.sent_history_menu_height() else {
            return;
        };
        let (trigger, side) = self.pills_strip.menu_anchor(pill);
        self.menu_side = Some(side);
        self.show_chat_menu(
            vec![json!({"pillMenu": {"id": SENT_MENU, "height": height}})],
            trigger,
            SENT_MENU_WIDTH,
            window,
            cx,
        );
        if let Some(menu) = &self.option_menu {
            menu.update(cx, |menu, _| menu.set_pill(SENT_TRIGGER));
        }
    }

    /// The menu went away (a click outside, Escape, another menu, a jump): the core drops the list,
    /// and the search field goes with the window it was made in.
    pub(super) fn sent_history_menu_closed(&mut self, cx: &mut Context<Self>) {
        self.sent_history_card_menu = None;
        self.sent_history_input = None;
        self.sent_history_subscription = None;
        self.sent_history_input_window = None;
        self.sent_history_ids.clear();
        self.sent_history_opening = false;
        if self.snapshot["sentHistory"]["open"] == true {
            self.invoke(json!({"type": "sentHistoryClose"}), cx);
        }
    }

    /// Scrolls the transcript to the message a card's jump reached, once per jump (the core bumps
    /// `revision`).
    pub(super) fn sync_sent_history_jump(&mut self) {
        let jump = &self.snapshot["sentHistory"]["jump"];
        let Some(revision) = jump["revision"].as_u64() else {
            return;
        };
        if revision == self.sent_history_jump_revision {
            return;
        }
        self.sent_history_jump_revision = revision;
        let Some(item) = jump["item"].as_u64() else {
            return;
        };
        self.release_answer_hold();
        self.list.scroll_to(gpui::ListOffset {
            item_ix: item as usize,
            offset_in_item: px(0.0),
        });
    }

    /// The search field, made once in the menu's own window, with the caret in it. Its hint is the
    /// core's (`searchPlaceholder`, naming a message number the list holds), kept in step as the
    /// cards arrive.
    fn sync_sent_history_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let here = window.window_handle().window_id();
        let hint = match text(&self.snapshot["sentHistory"], "searchPlaceholder") {
            hint if hint.is_empty() => "Search your messages".to_string(),
            hint => hint,
        };
        if let Some(input) = self.sent_history_input.clone() {
            if self.sent_history_input_window == Some(here) {
                if self.sent_history_hint != hint {
                    self.sent_history_hint = hint.clone();
                    input.update(cx, |state, cx| state.set_placeholder(hint, window, cx));
                }
                return;
            }
        }
        self.sent_history_input_window = Some(here);
        self.sent_history_hint = hint.clone();
        let query = text(&self.snapshot["sentHistory"], "query");
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder(hint);
            state.set_value(query, window, cx);
            state
        });
        input.update(cx, |state, cx| state.focus(window, cx));
        self.sent_history_subscription =
            Some(
                cx.subscribe_in(&input, window, |chat, input, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        let query = input.read(cx).value().to_string();
                        chat.invoke(json!({"type": "sentHistoryFilter", "text": query}), cx);
                    }
                }),
            );
        self.sent_history_input = Some(input);
    }

    /// Keeps the list's rows in step with the core's cards: a new first page starts at the bottom
    /// (the newest), an older page goes above the cards without moving them.
    fn sync_sent_history_list(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<String> = self.snapshot["sentHistory"]["items"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|item| text(item, "id"))
            .collect();
        if ids == self.sent_history_ids {
            return;
        }
        let known = &self.sent_history_ids;
        let older = !known.is_empty() && ids.len() > known.len() && ids.ends_with(known);
        if older {
            self.sent_history_list.splice(0..0, ids.len() - known.len());
        } else {
            self.sent_history_list.reset(ids.len());
            self.sent_history_list.scroll_to_end();
        }
        self.sent_history_ids = ids;
        // The list reaching its top asks for the page before it.
        let chat = cx.entity().downgrade();
        self.sent_history_list
            .set_scroll_handler(move |event, _, cx| {
                let _ = chat.update(cx, |chat, cx| {
                    // A card's menu belongs to where the card was; scrolling closes it.
                    if chat.sent_history_card_menu.take().is_some() {
                        cx.notify();
                    }
                    // Asked for a few cards before the top, so the older page is usually there
                    // before the reader reaches it.
                    let history = &chat.snapshot["sentHistory"];
                    if event.visible_range.start <= MORE_AHEAD
                        && history["hasMore"] == true
                        && history["loadingMore"] != true
                    {
                        chat.invoke(json!({"type": "sentHistoryMore"}), cx);
                    }
                });
            });
    }

    /// The menu's body: the header, the search field, the filters, then the cards (or loading,
    /// why there are none, or the error).
    pub(super) fn sent_history_menu_body(
        &mut self,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // The snapshot's own Arc, not a copy of every card's words each frame.
        let snapshot = std::sync::Arc::clone(&self.snapshot);
        let history = &snapshot["sentHistory"];
        if history["open"] != true && !self.sent_history_opening {
            return None;
        }
        if history["open"] == true {
            self.sent_history_opening = false;
        }
        self.sync_sent_history_input(window, cx);
        self.sync_sent_history_list(cx);
        let s = p.scale;
        let loading = history["loading"] == true || history["open"] != true;
        let header = super::chat_pills::pill_menu_header(
            "titlebar/history.svg",
            "History",
            if history["loadingMore"] == true {
                "Loading older\u{2026}".to_string()
            } else {
                text(history, "caption")
            },
            None,
            p,
        );
        let mut column = div()
            .id("chat-sent-history")
            .role(gpui::Role::Group)
            .aria_label("History")
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(GAP * s))
            .child(header);
        if let Some(input) = self.sent_history_input.clone() {
            column = column.child(
                div()
                    .w_full()
                    .flex_shrink_0()
                    .h(px(FIELD_HEIGHT * s))
                    .flex()
                    .items_center()
                    .gap(px(6.0 * s))
                    .rounded(px(8.0 * s))
                    .border_1()
                    .border_color(p.input_border)
                    .bg(p.composer_background)
                    .px(px(10.0 * s))
                    .child(
                        svg()
                            .path("titlebar/search.svg")
                            .size(px(14.0 * s))
                            .flex_shrink_0()
                            .text_color(p.muted),
                    )
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(&input)
                                .appearance(false)
                                .bordered(false)
                                .focus_bordered(false)
                                .w_full()
                                .p_0()
                                .text_size(px(13.0 * s))
                                .text_color(p.primary)
                                .placeholder_color(p.muted.opacity(0.6))
                                .caret_color(p.foreground),
                        ),
                    ),
            );
        }
        column = column.child(self.sent_history_filters(history, p, cx));
        let line = |child: AnyElement| {
            div()
                .flex_shrink_0()
                .h(px(LINE_HEIGHT * 2.0 * s))
                .flex()
                .items_center()
                .px(px(4.0 * s))
                .text_size(px(13.0 * s))
                .child(child)
                .into_any_element()
        };
        let items: &[Value] = history["items"].as_array().map_or(&[], Vec::as_slice);
        if let Some(error) = history["error"].as_str() {
            column = column.child(line(
                div()
                    .truncate()
                    .text_color(gpui::rgb(0xef9999))
                    .child(error.to_string())
                    .into_any_element(),
            ));
        } else if loading && items.is_empty() {
            column = column.child(line(
                div()
                    .text_color(p.muted)
                    .child("Reading your messages\u{2026}")
                    .into_any_element(),
            ));
        } else if items.is_empty() {
            column = column.child(line(
                div()
                    .truncate()
                    .text_color(p.muted)
                    .child(text(history, "emptyLabel"))
                    .into_any_element(),
            ));
        } else {
            let chat = cx.entity().downgrade();
            let appearance = p.clone();
            column = column.child(
                list(self.sent_history_list.clone(), move |index, _, cx| {
                    chat.update(cx, |chat, cx| {
                        chat.sent_history_card(index, &appearance, cx)
                    })
                    .unwrap_or_else(|_| div().into_any_element())
                })
                .flex_1()
                .min_h_0()
                .w_full(),
            );
        }
        Some(column.into_any_element())
    }

    /// The filters: neutral chips, the picked one filled.
    fn sent_history_filters(
        &self,
        history: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let chips: Vec<AnyElement> = history["filters"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|filter| {
                let id = text(filter, "id");
                let active = filter["active"] == true;
                let label = text(filter, "label");
                div()
                    .id(SharedString::from(format!("sent-history-filter:{id}")))
                    .role(gpui::Role::Button)
                    .aria_label(label.clone())
                    .aria_selected(active)
                    .flex_shrink_0()
                    .h(px(CHIPS_HEIGHT * s))
                    .px(px(9.0 * s))
                    .flex()
                    .items_center()
                    .rounded_full()
                    .border(px(s))
                    .border_color(p.foreground.opacity(0.1))
                    .text_size(px(12.0 * s))
                    .whitespace_nowrap()
                    .text_color(if active { p.foreground } else { p.muted })
                    .when(active, |chip| chip.bg(p.foreground.opacity(0.08)))
                    .hover(|style| style.bg(p.foreground.opacity(0.06)))
                    .chat_cursor_pointer()
                    .on_click(cx.listener(move |chat, _, _, cx| {
                        cx.stop_propagation();
                        chat.invoke(json!({"type": "sentHistoryKind", "filter": id}), cx);
                    }))
                    .child(label)
                    .into_any_element()
            })
            .collect();
        div()
            .flex_shrink_0()
            .h(px(CHIPS_HEIGHT * s))
            .flex()
            .items_center()
            .gap(px(6.0 * s))
            .children(chips)
            .into_any_element()
    }

    /// One card: its time and (on hover) its buttons, up to three lines of its words, and its
    /// pictures and files. A click anywhere else on it jumps to the message.
    fn sent_history_card(
        &mut self,
        index: usize,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(item) = self.snapshot["sentHistory"]["items"].get(index).cloned() else {
            return div().into_any_element();
        };
        let s = p.scale;
        let id = text(&item, "id");
        let group = SharedString::from(format!("sent-card:{id}"));
        let timestamp = item["timestamp"].as_i64();
        let time = item["time"]["label"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let time_title = text(&item["time"], "title");
        let images: Vec<Value> = item["images"].as_array().cloned().unwrap_or_default();
        let files: Vec<Value> = item["files"].as_array().cloned().unwrap_or_default();
        let mut attachments: Vec<AnyElement> = (0..images.len())
            .map(|position| self.image_tile_at(&id, &images, position, true, THUMB, p, cx))
            .collect();
        attachments.extend(files.iter().map(|file| {
            div()
                .flex_shrink_0()
                .h(px(24.0 * s))
                .px(px(8.0 * s))
                .flex()
                .items_center()
                .gap(px(5.0 * s))
                .rounded(px(6.0 * s))
                .border(px(s))
                .border_color(p.foreground.opacity(0.08))
                .text_size(px(12.0 * s))
                .text_color(p.foreground)
                .whitespace_nowrap()
                .child(
                    svg()
                        .path("titlebar/file-code.svg")
                        .size(px(13.0 * s))
                        .flex_shrink_0()
                        .text_color(p.muted),
                )
                .child(text(file, "label"))
                .into_any_element()
        }));
        let jump_id = id.clone();
        let menu_open = self.sent_history_card_menu.as_deref() == Some(id.as_str());
        let menu = menu_open.then(|| self.sent_card_menu(&item, p, cx));
        div()
            .pb(px(CARD_GAP * s))
            .child(
                div()
                    .id(SharedString::from(format!("sent-card:{id}")))
                    .group(group.clone())
                    .role(gpui::Role::Button)
                    .aria_label(format!("Jump to: {}", text(&item, "preview")))
                    .w_full()
                    .flex()
                    .flex_col()
                    .px(px(11.0 * s))
                    .py(px(CARD_PAD_Y * s))
                    .rounded(px(10.0 * s))
                    .border(px(s))
                    .border_color(p.foreground.opacity(0.06))
                    .bg(p.foreground.opacity(0.035))
                    .hover(|style| {
                        style
                            .bg(p.foreground.opacity(0.06))
                            .border_color(p.foreground.opacity(0.1))
                    })
                    .chat_cursor_pointer()
                    .relative()
                    .on_click(cx.listener(move |chat, _, _, cx| {
                        chat.close_pill_menu(cx);
                        chat.invoke(
                            json!({"type": "sentHistoryJump", "messageId": jump_id, "timestamp": timestamp}),
                            cx,
                        );
                    }))
                    .child(
                        div()
                            .h(px(META_HEIGHT * s))
                            .flex()
                            .items_center()
                            .gap(px(7.0 * s))
                            .text_size(px(11.5 * s))
                            .text_color(p.muted)
                            .child(
                                div()
                                    .id(SharedString::from(format!("sent-card-time:{id}")))
                                    .whitespace_nowrap()
                                    .tooltip(move |window, cx| {
                                        gpui_component::tooltip::Tooltip::new(time_title.clone())
                                            .build(window, cx)
                                    })
                                    .child(time),
                            )
                            .child(div().flex_1())
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(2.0 * s))
                                    .when(!menu_open, |buttons| {
                                        buttons
                                            .invisible()
                                            .group_hover(group, |style| style.visible())
                                    })
                                    .children(self.sent_card_buttons(&item, menu_open, p, cx)),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(4.0 * s))
                            .text_size(px(13.0 * s))
                            .line_height(px(TEXT_LINE * s))
                            .text_color(p.primary)
                            .line_clamp(3)
                            .text_ellipsis()
                            .child(text(&item, "preview")),
                    )
                    .when(!attachments.is_empty(), |card| {
                        card.child(
                            div()
                                .mt(px(8.0 * s))
                                .flex()
                                .items_center()
                                .gap(px(6.0 * s))
                                .overflow_hidden()
                                .children(attachments),
                        )
                    })
                    // The ⋯ menu hangs from the ⋯'s bottom right, over the list and unclipped by
                    // it, moved to fit the window (or above the ⋯ when it fits better there).
                    .when_some(menu, |card, menu| {
                        card.child(
                            div()
                                .absolute()
                                .top(px((CARD_PAD_Y + META_HEIGHT + 2.0) * s))
                                .right(px(11.0 * s))
                                .size_0()
                                .child(
                                    deferred(
                                        anchored()
                                            .anchor(gpui::Anchor::TopRight)
                                            .snap_to_window_with_margin(px(8.0 * s))
                                            .child(menu),
                                    )
                                    .with_priority(1),
                                ),
                        )
                    }),
            )
            .into_any_element()
    }

    /// A card's two buttons at its top right: Copy, and ⋯ for the rest (`sent_card_menu`).
    fn sent_card_buttons(
        &self,
        item: &Value,
        menu_open: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        let id = text(item, "id");
        let s = p.scale;
        let button = |key: &str, label: &'static str, icon: &'static str, on: bool| {
            div()
                .id(SharedString::from(format!("sent-card-{key}:{id}")))
                .role(gpui::Role::Button)
                .aria_label(label)
                .size(px(24.0 * s))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0 * s))
                .when(on, |button| button.bg(p.foreground.opacity(0.08)))
                .hover(|style| style.bg(p.foreground.opacity(0.08)))
                .chat_cursor_pointer()
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(label).build(window, cx)
                })
                .child(svg().path(icon).size(px(14.0 * s)).text_color(p.foreground))
        };
        let copy = json!({"type": "copyText", "text": text(item, "copyText")});
        let menu_id = id.clone();
        vec![
            button("copy", "Copy", "titlebar/copy.svg", false)
                .on_click(cx.listener(move |chat, _, _, cx| {
                    cx.stop_propagation();
                    chat.run_sent_card_command(&copy, cx);
                }))
                .into_any_element(),
            button("more", "More actions", "titlebar/dots.svg", menu_open)
                .aria_expanded(menu_open)
                .on_click(cx.listener(move |chat, event: &gpui::ClickEvent, _, cx| {
                    cx.stop_propagation();
                    let press = match event {
                        gpui::ClickEvent::Mouse(click) => Some(click.down.position),
                        _ => None,
                    };
                    // The press on this ⋯ already closed its open menu (`sent_card_menu`).
                    if let Some((closed, at)) = chat.sent_history_card_menu_dismissed.take()
                        && closed == menu_id
                        && Some(at) == press
                    {
                        return;
                    }
                    chat.sent_history_card_menu = (chat.sent_history_card_menu.as_deref()
                        != Some(menu_id.as_str()))
                    .then(|| menu_id.clone());
                    cx.notify();
                }))
                .into_any_element(),
        ]
    }

    /// A card's ⋯ rows in the order the user picked (the ⋯ under a message's): Copy reference and
    /// Reference in chat (a numbered message), Jump to message and Put back in the chat box, then
    /// Rewind to here and Fork from here where the message's own menu offers them, a line between
    /// each group.
    fn sent_card_menu_rows(&self, item: &Value) -> Vec<CardMenuRow> {
        let id = text(item, "id");
        let timestamp = item["timestamp"].as_i64();
        let mut references = Vec::new();
        if let Some(link) = item["referenceLink"].as_str() {
            references.push(CardMenuRow::Action(
                "Copy reference",
                "titlebar/hash.svg",
                json!({"type": "copyText", "text": link}),
            ));
        }
        if let Some(pill) = item["referencePill"].as_str()
            && !self.in_subagent
        {
            references.push(CardMenuRow::Action(
                "Reference in chat",
                "titlebar/quote.svg",
                json!({"type": "appendToDraft", "text": pill, "inline": true}),
            ));
        }
        let reuse = vec![
            CardMenuRow::Action(
                "Jump to message",
                "titlebar/arrow-up-right.svg",
                json!({"type": "sentHistoryJump", "messageId": id, "timestamp": timestamp}),
            ),
            CardMenuRow::Action(
                "Put back in the chat box",
                "titlebar/pencil.svg",
                json!({"type": "appendToDraft", "text": text(item, "draft")}),
            ),
        ];
        let mut branch = Vec::new();
        if !self.in_subagent
            && self.snapshot["rewindAvailable"] == true
            && self.snapshot["rewindEnabled"] == true
        {
            branch.push(CardMenuRow::Action(
                "Rewind to here",
                "chat-actions/rewind",
                json!({"type": "rewindOpen", "messageId": id, "prompt": text(item, "copyText")}),
            ));
        }
        if !self.in_subagent && self.snapshot["forkFromMessageAvailable"] == true {
            branch.push(CardMenuRow::Action(
                "Fork from here",
                "titlebar/git-branch.svg",
                json!({"type": "forkFromMessage", "messageId": id}),
            ));
        }
        let mut rows = Vec::new();
        for group in [references, reuse, branch] {
            if group.is_empty() {
                continue;
            }
            if !rows.is_empty() {
                rows.push(CardMenuRow::Separator);
            }
            rows.extend(group);
        }
        rows
    }

    /// The ⋯ menu's height (unscaled).
    fn sent_card_menu_height(&self, item: &Value) -> f32 {
        CARD_MENU_PADDING * 2.0
            + self
                .sent_card_menu_rows(item)
                .iter()
                .map(|row| match row {
                    CardMenuRow::Action(..) => CARD_MENU_ROW,
                    CardMenuRow::Separator => CARD_MENU_SEPARATOR,
                })
                .sum::<f32>()
    }

    /// A card's ⋯ menu, drawn as the chat's menus are (option_menu/render.rs): the menu's colour
    /// (kept opaque), hairline, ink and row hover, 13px labels after 14px icons, a line between groups. A click
    /// outside it closes it; a row runs and closes it.
    fn sent_card_menu(&self, item: &Value, p: &ChatAppearance, cx: &Context<Self>) -> AnyElement {
        let s = p.scale;
        let glass = super::option_menu::ChatMenuGlass::in_window(p);
        let id = text(item, "id");
        let rows: Vec<AnyElement> = self
            .sent_card_menu_rows(item)
            .into_iter()
            .map(|row| match row {
                CardMenuRow::Separator => div()
                    .flex_shrink_0()
                    .h(px(CARD_MENU_SEPARATOR * s))
                    .px(px(4.0 * s))
                    .py(px((CARD_MENU_SEPARATOR - 1.0) / 2.0 * s))
                    .child(div().h(px(1.0)).bg(glass.edge.opacity(0.7)))
                    .into_any_element(),
                CardMenuRow::Action(label, icon, command) => div()
                    .id(SharedString::from(format!("sent-card-menu:{id}:{label}")))
                    .role(gpui::Role::MenuItem)
                    .aria_label(label)
                    .flex_shrink_0()
                    .w_full()
                    .h(px(CARD_MENU_ROW * s))
                    .px(px(10.0 * s))
                    .flex()
                    .items_center()
                    .gap(px(8.0 * s))
                    .rounded(px(6.0 * s))
                    .whitespace_nowrap()
                    .hover(|style| style.bg(glass.hover))
                    .chat_cursor_pointer()
                    .on_click(cx.listener(move |chat, _, _, cx| {
                        cx.stop_propagation();
                        chat.sent_history_card_menu = None;
                        chat.run_sent_card_command(&command, cx);
                        cx.notify();
                    }))
                    .child(
                        svg()
                            .path(icon)
                            .flex_shrink_0()
                            .size(px(14.0 * s))
                            .text_color(glass.ink),
                    )
                    .child(label)
                    .into_any_element(),
            })
            .collect();
        div()
            .id(SharedString::from(format!("sent-card-menu:{id}")))
            .role(gpui::Role::Menu)
            .aria_label("More actions")
            .min_w(px(CARD_MENU_MIN_WIDTH * s))
            .flex()
            .flex_col()
            .p(px(CARD_MENU_PADDING * s))
            .rounded(glass.radius)
            .border_1()
            .border_color(glass.edge)
            // Opaque, as every card drawn over the chat's own content is (`over_content_fill`):
            // the cards' words sit right under it.
            .bg(p.over_content_fill(p.menu_opaque().alpha(1.0)))
            .text_color(glass.ink)
            .text_size(px(13.0 * s))
            .line_height(px(18.2 * s))
            .occlude()
            .on_mouse_down_out(
                cx.listener(move |chat, event: &gpui::MouseDownEvent, _, cx| {
                    if let Some(closed) = chat.sent_history_card_menu.take() {
                        chat.sent_history_card_menu_dismissed = Some((closed, event.position));
                        cx.notify();
                    }
                }),
            )
            .children(rows)
            .into_any_element()
    }

    /// A card button's command, run as the message menu runs its rows (actions.rs). Copy keeps the
    /// list open; everything else leaves it for the chat.
    fn run_sent_card_command(&mut self, command: &Value, cx: &mut Context<Self>) {
        match command["type"].as_str().unwrap_or_default() {
            "copyText" => {
                if let Some(text) = command["text"].as_str() {
                    crate::app::helpers::gpui_copy_to_clipboard(
                        gpui::ClipboardItem::new_string(text.to_owned()),
                        cx,
                    );
                }
            }
            "appendToDraft" => {
                self.close_pill_menu(cx);
                self.append_to_draft(
                    command["text"].as_str().unwrap_or_default(),
                    command["inline"] == true,
                    cx,
                );
            }
            "forkFromMessage" => {
                self.close_pill_menu(cx);
                self.host("forkFromMessage", command.clone(), cx);
            }
            _ => {
                self.close_pill_menu(cx);
                self.invoke(command.clone(), cx);
            }
        }
    }
}

/// The list state the History menu scrolls, bottom-aligned so it opens at the newest card.
pub(super) fn sent_history_list_state() -> ListState {
    ListState::new(0, ListAlignment::Bottom, px(400.0))
}
