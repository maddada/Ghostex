//! Transcript search, Cmd+F.
//!
//! The bar is a real sibling region at the top of the chat pane, above the
//! transcript list: no overlay, no hit-test routing. Matching, the selected
//! occurrence and next/previous live in the core (`extras/search.rs` over
//! `extras/transcript_search.rs` in gx-chat-core), so GPUI only draws the bar,
//! scrolls the list to the selected row and highlights the words that matched.
//!
//! Every occurrence of the query the reader can see is highlighted, the selected
//! one more strongly: in the row's Markdown by its text views, and in the one-line
//! labels (a tool row's name and preview, a thinking headline) by `find_label`.
//! What is searched follows the folds this view reports with `searchFolds`.

use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext as _, Context, Focusable as _, InteractiveElement as _,
    IntoElement, ParentElement as _, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::input::{Input, InputEvent, InputState};
use serde_json::json;
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(super) struct OpenChatSearch;

struct ChatSearchRegistered;
impl gpui::Global for ChatSearchRegistered {}

/// CDXC:SessionChat 2026-09-18 WHY:
/// The chat field is a gpui-component input, and that crate binds Cmd+F inside its own `Input`
/// context to its in-field Search panel. A binding on `NativeChat` alone loses to it by depth and
/// Cmd+F does nothing while the composer has focus, so transcript search claims both the pane and
/// the focused field, the way React's window-level handler covered the whole chat surface.
pub(super) fn register(cx: &mut App) {
    if cx.has_global::<ChatSearchRegistered>() {
        return;
    }
    cx.set_global(ChatSearchRegistered);
    let bindings = ["NativeChat", "NativeChat > Input"].map(|context| {
        crate::app::hotkeys::gpui_runtime_key_binding(
            &super::keyboard::primary_chord("f"),
            OpenChatSearch,
            Some(context),
            cx,
        )
    });
    cx.bind_keys(bindings);
}

impl NativeChatView {
    pub(super) fn open_search_action(
        &mut self,
        _: &OpenChatSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_search(window, cx);
        cx.stop_propagation();
        window.prevent_default();
    }

    /// Shows the find bar with its query selected and gives it the keyboard.
    ///
    /// CDXC:SessionChat 2026-10-01 WHY:
    /// The bar is drawn by the chat itself, and since keystrokes repaint only the composer's own view (composer_host.rs) nothing else draws the chat until the core's next snapshot, so the field Cmd+F focused stayed off screen for that round trip. The chat draws again at once.
    pub(crate) fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.invoke(json!({"type":"searchOpen"}), cx);
        self.search_pending_open = Some(true);
        self.ensure_search_input(window, cx);
        if let Some(input) = self.search_input.clone() {
            let end = input.read(cx).value().len();
            input.update(cx, |input, cx| {
                input.focus(window, cx);
                input.set_selected_range(0..end, cx);
            });
        }
        cx.notify();
    }

    /// Enter / Shift+Enter, the arrows and Escape while the field has focus.
    /// Returns true when search consumed the key.
    pub(super) fn search_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(input) = self.search_input.clone() else {
            return false;
        };
        if !input.read(cx).focus_handle(cx).is_focused(window) {
            return false;
        }
        let key = &event.keystroke;
        let command = match key.key.as_str() {
            "enter" if key.modifiers.shift => "searchPrevious",
            "enter" => "searchNext",
            "up" => "searchPrevious",
            "down" => "searchNext",
            "escape" => {
                self.close_search(cx);
                return true;
            }
            _ => return false,
        };
        self.invoke(json!({ "type": command }), cx);
        true
    }

    /// Whether the find bar is shown.
    ///
    /// CDXC:SessionChat 2026-09-24 WHY:
    /// `searchOpen` and `searchClose` reach the shared runtime asynchronously, so for a frame or more the snapshot still reports the old state. Rendering from the snapshot alone dropped the field Cmd+F had just focused (and briefly revived it after Escape), which sent the next keystrokes to the composer. The pane's own request wins until the snapshot catches up.
    pub(super) fn search_open(&self) -> bool {
        self.search_pending_open
            .unwrap_or(self.snapshot["transcriptSearch"]["open"] == true)
    }

    fn close_search(&mut self, cx: &mut Context<Self>) {
        self.search_pending_open = Some(false);
        self.search_input = None;
        self.search_subscription = None;
        self.search_scrolled_revision = -1;
        self.search_folds_sent = None;
        self.focus_requested = true;
        self.invoke(json!({"type":"searchClose"}), cx);
    }

    /// Reveals the selected occurrence once per explicit navigation.
    pub(super) fn sync_search_scroll(&mut self) {
        let search = &self.snapshot["transcriptSearch"];
        if !search.is_object() {
            self.search_scrolled_revision = -1;
            return;
        }
        let revision = search["revision"].as_i64().unwrap_or(0);
        let Some(item) = search["activeItem"].as_u64() else {
            return;
        };
        if revision == self.search_scrolled_revision {
            return;
        }
        self.search_scrolled_revision = revision;
        self.list.scroll_to_reveal_item(item as usize);
        // CDXC:SessionChat 2026-10-01 WHY: `scroll_to_reveal_item` leaves a list that follows its tail following it, so a chat sitting at the bottom snapped straight back on its next layout and the selected match never came into view. `scroll_to` the same offset stops the following, and scrolling back to the bottom resumes it as usual.
        self.list.scroll_to(self.list.logical_scroll_top());
    }

    /// Starts drawing row `index` of the main transcript: when it holds a match, its text views
    /// highlight the query, numbering the occurrences across the row so the selected one (the
    /// core's `activeInRow`) is marked and scrolled into view once per navigation.
    ///
    /// CDXC:SessionChat 2026-10-20 DECISION:
    /// User: "Can we please highlight the word that was found in the message?" (2026-10-01) and "don't highlight the block that has found text". Every occurrence gets a background behind its letters, the selected one (n of the total) a stronger one, the way a browser's find does, and the message or block around it gets no background of its own. Supersedes the 2026-10-01 row tint.
    pub(super) fn begin_row_find(&mut self, index: usize, main: bool, p: &ChatAppearance) {
        self.row_find = None;
        self.row_label_find = None;
        let search = &self.snapshot["transcriptSearch"];
        if !main || !search.is_object() {
            return;
        }
        let row = index as u64;
        if !search["items"]
            .as_array()
            .is_some_and(|items| items.iter().any(|value| value.as_u64() == Some(row)))
        {
            return;
        }
        let query = text(search, "query");
        if query.trim().is_empty() {
            return;
        }
        let (background, active_background) = search_highlight_colors(p);
        let active_row = search["activeItem"].as_u64() == Some(row);
        let active = active_row
            .then(|| search["activeInRow"].as_u64())
            .flatten()
            .map(|occurrence| occurrence as usize);
        let label = &search["activeLabel"];
        self.row_label_find = Some(RowLabelFind {
            needle: query.trim().to_lowercase(),
            background,
            active_background,
            active: active_row
                .then(|| {
                    Some((
                        label["key"].as_str()?.to_string(),
                        label["occurrence"].as_u64()? as usize,
                    ))
                })
                .flatten(),
        });
        self.row_find = Some(
            gpui_component::text::TextFind::new(query, background, active_background)
                .counter(Default::default())
                .active(active)
                .reveal(search["revision"].as_u64().unwrap_or(0)),
        );
    }

    /// `text` with the find's occurrences highlighted, when `key` is a label the core searches
    /// (a tool row's name or preview, a thinking headline) in a row that holds a match.
    pub(super) fn find_label(&self, key: &str, text: String) -> AnyElement {
        let highlights = self.label_highlights(key, &text);
        if highlights.is_empty() {
            return text.into_any_element();
        }
        gpui::StyledText::new(text)
            .with_highlights(highlights.into_iter().map(|(range, background)| {
                (
                    range,
                    gpui::HighlightStyle {
                        background_color: Some(background),
                        ..Default::default()
                    },
                )
            }))
            .into_any_element()
    }

    /// The byte ranges of the find's occurrences in label `key`, each with its background.
    pub(super) fn label_highlights(
        &self,
        key: &str,
        text: &str,
    ) -> Vec<(Range<usize>, gpui::Hsla)> {
        let Some(find) = &self.row_label_find else {
            return Vec::new();
        };
        if !SEARCHED_LABELS.iter().any(|prefix| key.starts_with(prefix)) {
            return Vec::new();
        }
        let lower = text.to_lowercase();
        // Lowercasing changed the length (a handful of scripts): offsets would not line up, so the
        // label is left plain rather than marked in the wrong place.
        if lower.len() != text.len() {
            return Vec::new();
        }
        let active = find
            .active
            .as_ref()
            .filter(|(active_key, _)| active_key == key)
            .map(|(_, occurrence)| *occurrence);
        lower
            .match_indices(find.needle.as_str())
            .enumerate()
            .map(|(occurrence, (start, found))| {
                let background = if Some(occurrence) == active {
                    find.active_background
                } else {
                    find.background
                };
                (start..start + found.len(), background)
            })
            .collect()
    }

    /// Reports the folds this view has open while the find bar is open, so the core searches only
    /// what the reader can see (gx-chat-core `transcript_search.rs`, `visible_segments`).
    fn sync_search_folds(&mut self, p: &ChatAppearance, cx: &mut Context<Self>) {
        let mut expanded: Vec<&String> = self.expanded.iter().collect();
        let mut collapsed: Vec<&String> = self.collapsed.iter().collect();
        expanded.sort();
        collapsed.sort();
        let folds = json!({
            "type": "searchFolds",
            "expanded": expanded,
            "collapsed": collapsed,
            "simple": p.simple,
            "workOpen": p.work_open,
        });
        if self.search_folds_sent.as_ref() == Some(&folds) {
            return;
        }
        self.search_folds_sent = Some(folds.clone());
        self.invoke(folds, cx);
    }

    fn ensure_search_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_input.is_some() {
            return;
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        self.search_subscription =
            Some(
                cx.subscribe_in(&input, window, |chat, input, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        let query = input.read(cx).value().to_string();
                        chat.invoke(json!({"type":"searchQuery","query":query}), cx);
                    }
                }),
            );
        self.search_input = Some(input);
    }

    pub(super) fn render_search_bar(
        &mut self,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let snapshot_open = self.snapshot["transcriptSearch"]["open"] == true;
        if self.search_pending_open == Some(snapshot_open) {
            self.search_pending_open = None;
        }
        if !self.search_open() {
            if self.search_input.is_some() {
                self.search_input = None;
                self.search_subscription = None;
            }
            self.search_folds_sent = None;
            return None;
        }
        self.sync_search_folds(p, cx);
        self.ensure_search_input(window, cx);
        let s = p.scale;
        let label = text(&self.snapshot["transcriptSearch"], "label");
        let input = self.search_input.as_ref()?.clone();
        Some(
            div()
                .id("chat-search")
                .role(gpui::Role::Search)
                .w_full()
                .flex_shrink_0()
                .flex()
                .justify_center()
                .px(px(16.0 * s))
                .pt(px(8.0 * s))
                .child(
                    div()
                        .w_full()
                        .max_w(px(768.0 * s))
                        .flex()
                        .items_center()
                        .gap(px(6.0 * s))
                        .rounded(px(8.0 * s))
                        .border_1()
                        .border_color(p.input_border)
                        .bg(p.composer_background)
                        .px(px(10.0 * s))
                        .py(px(4.0 * s))
                        .child(
                            gpui::svg()
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
                        )
                        .when(!label.is_empty(), |this| {
                            this.child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(12.0 * s))
                                    .text_color(p.muted)
                                    .child(label),
                            )
                        })
                        .child(self.icon_command(
                            "chat-search-previous",
                            "Previous result",
                            "titlebar/chevron-up.svg",
                            json!({"type":"searchPrevious"}),
                            p,
                            cx,
                        ))
                        .child(self.icon_command(
                            "chat-search-next",
                            "Next result",
                            "titlebar/chevron-down.svg",
                            json!({"type":"searchNext"}),
                            p,
                            cx,
                        ))
                        .child(
                            div()
                                .id("chat-search-close")
                                .role(gpui::Role::Button)
                                .aria_label("Close search")
                                .chat_cursor_pointer()
                                .size(px(24.0 * s))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(5.0 * s))
                                .hover(|style| style.bg(p.border))
                                .child(
                                    gpui::svg()
                                        .path("titlebar/x.svg")
                                        .size(px(14.0 * s))
                                        .text_color(p.primary),
                                )
                                .on_click(cx.listener(|chat, _, _, cx| chat.close_search(cx))),
                        ),
                )
                .into_any_element(),
        )
    }
}

/// The labels gx-chat-core's `visible_segments` searches, by the prefix of their key: a tool
/// row's name, its preview line, and a reasoning row's headline.
const SEARCHED_LABELS: [&str; 3] = ["tool:", "tool-preview:", "reasoning:"];

/// The find for one row's labels: the query, the two backgrounds, and the selected occurrence
/// when it is in one of this row's labels (the core's `activeLabel`).
pub(super) struct RowLabelFind {
    needle: String,
    background: gpui::Hsla,
    active_background: gpui::Hsla,
    active: Option<(String, usize)>,
}

/// The find highlight behind each occurrence and the stronger one behind the selected occurrence:
/// a browser find's yellow and orange, kept translucent so the text stays readable on either theme.
fn search_highlight_colors(p: &ChatAppearance) -> (gpui::Hsla, gpui::Hsla) {
    let yellow = gpui::Hsla::from(gpui::rgb(0xfacc15));
    let orange = gpui::Hsla::from(gpui::rgb(0xf97316));
    if p.light {
        (yellow.opacity(0.45), orange.opacity(0.45))
    } else {
        (yellow.opacity(0.3), orange.opacity(0.6))
    }
}
