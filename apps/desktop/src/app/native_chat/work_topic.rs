//! The Work panel's Topic (the line under its title that says what the session is about, a click
//! rewrites it in place) and what the window's header asks of the panel: whether this chat has
//! one, whether it is shown, and opening or closing it from the header's Work button.
//!
//! CDXC:Coordinators 2026-11-18 DECISION:
//! User, on the Briefs pages: Q4137 keep the one line at the top of the Work panel for every
//! session, "instead of goal its more like topic", rewritten with a click; and layout B, the
//! panels' buttons in the window's header (Work and Session details), one spot beside the chat,
//! one panel at a time, for coordinators too.
//! SEE-ALSO: server/src/coordinators/session_work.rs (the Topic and the user's decision on work
//! items for any session), apps/desktop/src/app/render/workarea_header/work_button.rs (the button),
//! apps/desktop/src/app/session_chat/session_details.rs (one panel at a time).

use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::AppContext as _;
use gpui::Focusable as _;
use gpui::{
    AnyElement, Bounds, Context, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::input::{Input, InputEvent, InputState};
use serde_json::{Value, json};

/// The Topic line's height (unscaled), for the floating card's size (`board_height`).
const TOPIC_HEIGHT: f32 = 28.0;

/// How much the Topic adds to the panel: only the Work panel has one.
pub(super) fn topic_height(board: &Value) -> f32 {
    if board["topicEditable"] == true {
        TOPIC_HEIGHT
    } else {
        0.0
    }
}

impl NativeChatView {
    /// The Topic under the Work panel's title: its words (or "Add a topic"), a click rewrites it;
    /// while it is being rewritten, the field. `None` on a panel that is not the Work panel.
    pub(super) fn work_topic_row(
        &self,
        board: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // Only where gxserver keeps a Topic (the core's `topicEditable`).
        if board["topicEditable"] != true {
            return None;
        }
        let s = p.scale;
        let row = div()
            .flex_shrink_0()
            .h(px(TOPIC_HEIGHT * s))
            .flex()
            .items_center()
            .px(px(4.0 * s))
            .text_size(px(13.0 * s));
        if let Some((input, _)) = &self.board.topic_edit {
            return Some(
                row.id("chat-work-topic-field")
                    // Its Escape is the chat's key handler's (`work_topic_key_down`), before the
                    // chat box sees it.
                    .capture_key_down(cx.listener(
                        |this, event: &gpui::KeyDownEvent, window, cx| {
                            if this.work_topic_key_down(event, window, cx) {
                                cx.stop_propagation();
                                window.prevent_default();
                            }
                        },
                    ))
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(input)
                                .appearance(false)
                                .bordered(false)
                                .focus_bordered(false)
                                .w_full()
                                .p_0()
                                .text_size(px(13.0 * s))
                                .text_color(p.foreground)
                                .placeholder_color(p.muted.opacity(0.6))
                                .caret_color(p.foreground),
                        ),
                    )
                    .into_any_element(),
            );
        }
        let words = board["topic"]["text"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let empty = words.is_empty();
        Some(
            row.id("chat-work-topic")
                .role(gpui::Role::Button)
                .aria_label("Topic: click to rewrite it")
                .rounded(px(5.0 * s))
                .chat_cursor_pointer()
                .hover(|style| style.bg(p.foreground.opacity(0.06)))
                .tooltip(|window, cx| {
                    gpui_component::tooltip::Tooltip::new("Click to rewrite the topic")
                        .build(window, cx)
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(if empty { p.card_muted } else { p.foreground })
                        .child(if empty {
                            "Add a topic".to_string()
                        } else {
                            words
                        }),
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    this.start_work_topic_edit(window, cx);
                }))
                .into_any_element(),
        )
    }

    /// Escape in the Topic field cancels the rewrite; true when it took the key.
    pub(super) fn work_topic_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let focused = self
            .board
            .topic_edit
            .as_ref()
            .is_some_and(|(input, _)| input.focus_handle(cx).is_focused(window));
        if !focused || event.keystroke.key != "escape" {
            return false;
        }
        self.finish_work_topic_edit(false, cx);
        true
    }

    /// Turns the Topic into a field holding its words, with the keyboard in it.
    fn start_work_topic_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let words = self.snapshot["coordinatorBoard"]["topic"]["text"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(words)
                .placeholder("What this session is about, in one line")
        });
        // The caret after the words, ready to go on typing.
        input.update(cx, |input, cx| {
            input.set_cursor_position(
                gpui_component::input::Position::new(0, u32::MAX),
                window,
                cx,
            );
            input.focus(window, cx);
        });
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this: &mut Self, _, event: &InputEvent, _, cx| match event {
                InputEvent::PressEnter { .. } | InputEvent::Blur => {
                    this.finish_work_topic_edit(true, cx)
                }
                _ => {}
            },
        );
        self.board.topic_edit = Some((input, vec![subscription]));
        cx.notify();
    }

    /// Ends the rewrite: saved (Enter, or a click elsewhere) unless Escape cancelled it. An
    /// unchanged line sends nothing.
    fn finish_work_topic_edit(&mut self, save: bool, cx: &mut Context<Self>) {
        let Some((input, _)) = self.board.topic_edit.take() else {
            return;
        };
        let words = input.read(cx).value().trim().to_string();
        let before = self.snapshot["coordinatorBoard"]["topic"]["text"]
            .as_str()
            .unwrap_or_default();
        if save && words != before {
            self.invoke(json!({ "type": "setWorkTopic", "text": words }), cx);
        }
        cx.notify();
    }

    /// This chat has a Work panel: a coordinator's, or an ordinary session's own while Work items
    /// is on.
    pub(crate) fn work_panel_available(&self) -> bool {
        self.snapshot["coordinatorBoard"]["title"] == "Work"
    }

    /// What the header's Work button counts: what waits on the user (Needs you and Blocked).
    pub(crate) fn work_panel_waiting(&self) -> usize {
        self.snapshot["coordinatorBoard"]["pillCounts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|count| matches!(count[0].as_str(), Some("needs" | "blocked")))
            .filter_map(|count| count[1].as_u64())
            .sum::<u64>() as usize
    }

    /// Tells the app when what the header's Work button shows for this chat changed
    /// (`NativeChatEvent::WorkButton`); checked every frame, cheap.
    pub(super) fn note_work_button(&mut self, cx: &mut Context<Self>) {
        let now = (
            self.work_panel_available(),
            self.work_panel_shown(),
            self.work_panel_waiting(),
        );
        if self.board.work_button_shown != Some(now) {
            self.board.work_button_shown = Some(now);
            cx.emit(super::state::NativeChatEvent::WorkButton);
        }
    }

    /// The Work panel is shown, or its floating card or columns were dismissed by the press now
    /// landing on the header's Work button: that press closes it, so Session details stays.
    pub(crate) fn work_panel_shown_or_closing(&self) -> bool {
        self.work_panel_shown()
            || self
                .menu_toggle
                .just_dismissed(super::coordinator_board::BOARD_TRIGGER)
            || self
                .menu_toggle
                .just_dismissed(super::coordinator_board::BOARD_COLUMNS_TRIGGER)
    }

    /// The Work panel is on screen: docked, floating, or as columns.
    pub(crate) fn work_panel_shown(&self) -> bool {
        self.coordinator_board_docked()
            || self.chat_menu_is_open(super::coordinator_board::BOARD_TRIGGER)
            || self.chat_menu_is_open(super::coordinator_board::BOARD_COLUMNS_TRIGGER)
            || (self.board.open && self.work_panel_available())
    }

    /// The header's Work button: the panel opens beside the chat where there is room (else as a
    /// card under the button, as Session details' does), or closes. `details_put_away`: Session
    /// details was just put away for it, so its width no longer counts against docking.
    pub(crate) fn toggle_work_panel_from_header(
        &mut self,
        anchor: Bounds<Pixels>,
        details_put_away: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if details_put_away {
            if self.session_details.docked {
                self.swap_side_panels_in_place();
            }
            self.session_details.docked = false;
        }
        self.pill_bounds
            .cell(super::coordinator_board::BOARD_MENU)
            .set(anchor);
        let p = ChatAppearance::current(&self.snapshot);
        if self.board_view() == super::coordinator_board::COLUMNS_VIEW || self.board_fits(&p) {
            self.toggle_coordinator_board(anchor, window, cx);
            return;
        }
        if self.chat_menu_toggled_shut(super::coordinator_board::BOARD_TRIGGER, cx) {
            return;
        }
        self.open_work_card_under(anchor, window, cx);
    }

    /// The app's say: where its header's Work button is painted, for the floating card.
    /// `None` for a chat the main header's button doesn't serve (another split pane's, a popup
    /// window's): its card opens at the pane's top-right corner (`work_card_anchor`).
    pub(crate) fn set_work_header_anchor(
        &mut self,
        anchor: Option<std::rc::Rc<std::cell::Cell<Bounds<Pixels>>>>,
    ) {
        self.board.header_anchor = anchor;
    }

    /// Where the floating card opens when the panel can't dock: under the header's Work button
    /// while it serves this chat, else at the pane's top-right corner, as Session details' does.
    pub(super) fn work_card_anchor(&self) -> Bounds<Pixels> {
        self.work_header_anchor().unwrap_or_else(|| {
            let pane = self.bounds.get();
            Bounds::new(
                gpui::point(
                    pane.origin.x + pane.size.width - gpui::px(8.0),
                    pane.origin.y,
                ),
                gpui::size(gpui::px(0.0), gpui::px(0.0)),
            )
        })
    }

    /// The header's Work button's bounds, while it has been painted.
    pub(super) fn work_header_anchor(&self) -> Option<Bounds<Pixels>> {
        self.board
            .header_anchor
            .as_ref()
            .map(|anchor| anchor.get())
            .filter(|anchor| anchor.size.width > gpui::px(0.0))
    }

    /// The Work panel as a floating card under `anchor` (the header's button), as Session
    /// details' card opens.
    pub(super) fn open_work_card_under(
        &mut self,
        anchor: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let trigger = super::coordinator_board::BOARD_TRIGGER;
        let Some(height) = self.coordinator_board_menu_height() else {
            return;
        };
        self.menu_side = Some(super::option_menu::MenuSide::Right);
        // Owned by the Work trigger, so its pressed look and a second press read it as open.
        self.menu_toggle.arm(trigger);
        self.show_chat_menu(
            vec![
                json!({"pillMenu": {"id": super::coordinator_board::BOARD_MENU, "height": height}}),
            ],
            anchor,
            super::coordinator_board::BOARD_WIDTH,
            window,
            cx,
        );
        if let Some(menu) = &self.option_menu {
            menu.update(cx, |menu, _| menu.set_pill(trigger));
        }
    }

    /// The Work panel docked by any way in (its pill, a count, Show in Work): Session details,
    /// sharing the spot beside the chat, goes away (one panel at a time).
    pub(super) fn work_panel_takes_the_spot(&mut self, cx: &mut Context<Self>) {
        if self.session_details.docked {
            self.swap_side_panels_in_place();
        }
        if self.session_details.wanted {
            self.session_details.docked = false;
            self.host("closeSessionDetails", json!({}), cx);
        }
    }

    /// One docked panel takes the other's place: both have the same width (session_details/mod.rs
    /// `PANEL_WIDTH`), so the next frame snaps them instead of sliding one out while the other
    /// slides in, and the chat beside them keeps its width.
    fn swap_side_panels_in_place(&mut self) {
        self.board.motion.reset();
        self.session_details.motion.reset();
    }

    /// Session details opened in this chat's spot: the Work panel makes way.
    pub(crate) fn close_work_panel(&mut self, cx: &mut Context<Self>) {
        if self.board.docked {
            self.swap_side_panels_in_place();
        }
        if self.board.open {
            self.board.open = false;
            cx.notify();
        }
        if self.chat_menu_is_open(super::coordinator_board::BOARD_TRIGGER)
            || self.chat_menu_is_open(super::coordinator_board::BOARD_COLUMNS_TRIGGER)
        {
            self.close_pill_menu(cx);
        }
    }
}
