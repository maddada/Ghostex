//! A coordinator's board beside its chat: the Board pill opens it as its own column docked on the
//! right when the pane has room, else as a floating card over the chat box like the other pill
//! menus. Groups, rows, tags, folds and counts come from the core
//! (packages/gx-chat-core/src/extras/coordinator_board.rs, where the user's decision lives); this
//! decides where the board goes and draws it.
//!
//! SEE-ALSO: apps/mobile/app/src/chat/native/cards/CoordinatorBoard.tsx (the phone's sheet),
//! session_details/mod.rs (the other docked column, whose slide this reuses).

use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, ScrollHandle,
    StatefulInteractiveElement as _, Styled as _, Window, div, px, rgb, svg,
};
use serde_json::{Value, json};

/// The column's width, docked or floating, unscaled.
pub(crate) const BOARD_WIDTH: f32 = 340.0;
/// The chat keeps at least this much (unscaled) beside a docked board; narrower, the board floats.
const CHAT_MIN_WIDTH: f32 = 620.0;
/// The pill's menu-toggle key and its pill-menu id (chat_pills.rs).
pub(super) const BOARD_TRIGGER: &str = "chat-pill-board";
pub(super) const BOARD_MENU: &str = ghostex_gx_chat_core::extras::coordinator_board::BOARD_PILL;

/// Fixed heights (unscaled), so the floating card knows its size before it opens.
const HEADER_HEIGHT: f32 = 30.0;
const GROUP_HEIGHT: f32 = 26.0;
const ROW_HEIGHT: f32 = 40.0;
const MORE_HEIGHT: f32 = 22.0;

/// Each group's icon and colour: the sidebar's question pink, approval red and working amber, so a
/// row reads as its sidebar dot does.
fn group_look(group: &str) -> (&'static str, Option<u32>) {
    match group {
        "needs" => (
            "titlebar/circle-dot.svg",
            Some(crate::app::native_sidebar::status::QUESTION_COLOR),
        ),
        "blocked" => (
            "titlebar/circle-minus-filled.svg",
            Some(crate::app::native_sidebar::status::APPROVAL_COLOR),
        ),
        "moving" => (
            "titlebar/circle-half.svg",
            Some(crate::app::native_sidebar::status::WORKING_COLOR),
        ),
        "done" => ("titlebar/circle-check-filled.svg", Some(GOOD_COLOR)),
        "ideas" => ("titlebar/bulb.svg", None),
        _ => ("titlebar/circle-dashed.svg", None),
    }
}

const GOOD_COLOR: u32 = 0x2fa865;
const HELD_COLOR: u32 = 0x8b78f0;

/// A tag's tint (`BoardTag::tone`), or `None` for the muted chip.
fn tone_color(tone: &str) -> Option<u32> {
    match tone {
        "question" => Some(crate::app::native_sidebar::status::QUESTION_COLOR),
        "bad" => Some(crate::app::native_sidebar::status::APPROVAL_COLOR),
        "running" | "warn" => Some(crate::app::native_sidebar::status::WORKING_COLOR),
        "good" => Some(GOOD_COLOR),
        "held" => Some(HELD_COLOR),
        _ => None,
    }
}

/// The board's content height (unscaled): its header, each group's line, an open group's rows
/// and its "N more" line.
pub(super) fn board_height(board: &Value) -> f32 {
    let mut height = HEADER_HEIGHT + 8.0;
    for group in board["groups"].as_array().into_iter().flatten() {
        height += GROUP_HEIGHT;
        let rows = group["rows"].as_array().map_or(0, Vec::len);
        height += rows as f32 * ROW_HEIGHT;
        if !text(group, "moreLabel").is_empty() {
            height += MORE_HEIGHT;
        }
    }
    height
}

/// What the view keeps about its board between frames.
#[derive(Default)]
pub(crate) struct CoordinatorBoardView {
    /// The reader opened the docked board (it stays open across chats' frames until closed).
    pub(super) open: bool,
    /// It is docked this frame.
    pub(super) docked: bool,
    /// The docked column's slide in and out.
    pub(super) motion: crate::app::panel_motion::PanelMotion,
    /// The pane had room to dock last frame; a change of it snaps instead of sliding.
    fits: bool,
    pub(super) scroll: ScrollHandle,
}

impl NativeChatView {
    fn board_value(&self) -> Option<&Value> {
        Some(&self.snapshot["coordinatorBoard"]).filter(|board| board.is_object())
    }

    /// The pane has room for the docked board beside the chat (and the docked Session details).
    fn board_fits(&self, p: &ChatAppearance) -> bool {
        let pane = self.session_details_pane_bounds().get().size.width.as_f32();
        let details = if self.session_details.docked {
            super::session_details::PANEL_WIDTH
        } else {
            0.0
        };
        pane >= (BOARD_WIDTH + CHAT_MIN_WIDTH + details) * p.scale
    }

    /// The Board pill was clicked: the docked column comes or goes where there is room, else the
    /// floating card opens or shuts.
    pub(super) fn toggle_coordinator_board(
        &mut self,
        pill: gpui::Bounds<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let p = ChatAppearance::current(&self.snapshot);
        if self.board_fits(&p) {
            self.board.open = !self.board.open;
            cx.notify();
        } else {
            self.toggle_pill_menu(BOARD_MENU, pill, window, cx);
        }
    }

    /// The docked board is on screen (its pill reads as pressed).
    pub(super) fn coordinator_board_docked(&self) -> bool {
        self.board.docked
    }

    /// Decides this frame's dock; called before anything measures the transcript's width.
    pub(super) fn sync_coordinator_board(
        &mut self,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let fits = self.board_fits(p);
        let docked = self.board.open
            && fits
            && self.board_value().is_some()
            && self.maximized_window.is_none()
            && !self.pane_hidden;
        if self.board.docked != docked {
            self.board.docked = docked;
            self.list.remeasure();
        }
        if self.pane_hidden || fits != self.board.fits {
            self.board.fits = fits;
            self.board.motion.reset();
        }
        let frame = self.board.motion.sample(
            docked,
            BOARD_WIDTH * p.scale,
            0.0,
            cx.reduce_motion(),
            web_time::Instant::now(),
        );
        if frame.animating {
            window.request_animation_frame();
        }
    }

    /// The docked column, or `None` while the board is not docked.
    pub(super) fn render_coordinator_board_dock(
        &mut self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let frame = self.board.motion.frame();
        if !self.board.docked && frame.extent <= 0.0 {
            return None;
        }
        let board = self.board_value()?.clone();
        let s = p.scale;
        let column = div()
            .id("chat-coordinator-board-dock")
            .role(gpui::Role::Group)
            .aria_label("Board")
            .flex_shrink_0()
            .w(px(BOARD_WIDTH * s))
            .h_full()
            .min_h_0()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(p.border)
            .when(self.under_workarea_header, |column| {
                column.pt(px(crate::app::consts::WORKAREA_HEADER_HEIGHT))
            })
            .child(
                div()
                    .flex_shrink_0()
                    .px(px(10.0 * s))
                    .pt(px(8.0 * s))
                    .child(self.board_header(&board, true, p, cx)),
            )
            .child(
                div()
                    .id("chat-coordinator-board-dock-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.board.scroll)
                    .px(px(10.0 * s))
                    .pb(px(10.0 * s))
                    .opacity(frame.opening_content_opacity() * frame.closing_content_opacity())
                    .child(self.board_groups(&board, p, cx)),
            )
            .into_any_element();
        Some(if frame.animating {
            crate::app::panel_motion::clip_panel_horizontally(frame, true, column)
                .into_any_element()
        } else {
            column
        })
    }

    /// The floating card's content height, capped at most of the pane; its list scrolls.
    pub(super) fn coordinator_board_menu_height(&self) -> Option<f32> {
        let board = self.board_value()?;
        let scale = ChatAppearance::current(&self.snapshot).scale;
        let pane = f32::from(self.bounds.get().size.height) / scale;
        Some(board_height(board).min((pane * 0.7).max(220.0)))
    }

    /// The floating card's body, drawn with this view's listeners.
    pub(super) fn coordinator_board_menu_body(
        &mut self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let board = self.board_value()?.clone();
        Some(
            div()
                .id("chat-coordinator-board-menu")
                .role(gpui::Role::Group)
                .aria_label("Board")
                .size_full()
                .min_h_0()
                .flex()
                .flex_col()
                .child(self.board_header(&board, false, p, cx))
                .child(
                    div()
                        .id("chat-coordinator-board-menu-list")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .child(self.board_groups(&board, p, cx)),
                )
                .into_any_element(),
        )
    }

    /// "Board", its counts line, and (docked) the close button.
    fn board_header(
        &self,
        board: &Value,
        docked: bool,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let close = docked.then(|| {
            div()
                .id("chat-coordinator-board-close")
                .role(gpui::Role::Button)
                .aria_label("Close the board")
                .flex_shrink_0()
                .size(px(20.0 * s))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.0 * s))
                .chat_cursor_pointer()
                .hover(|style| style.bg(p.foreground.opacity(0.06)))
                .child(
                    svg()
                        .path("titlebar/x.svg")
                        .size(px(12.0 * s))
                        .text_color(p.muted),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.board.open = false;
                    cx.notify();
                }))
                .into_any_element()
        });
        let meta = text(board, "meta");
        div()
            .flex_shrink_0()
            .h(px(HEADER_HEIGHT * s))
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .px(px(4.0 * s))
            .text_size(px(13.0 * s))
            .child(
                svg()
                    .path("titlebar/layout-board-split.svg")
                    .size(px(14.0 * s))
                    .flex_shrink_0()
                    .text_color(p.muted),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(p.foreground)
                    .child("Board"),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(p.card_muted)
                    .child(if board["empty"] == true {
                        "Nothing open".to_string()
                    } else {
                        meta
                    }),
            )
            .children(close)
            .into_any_element()
    }

    /// Every group: its line (a click folds it), its rows and its "N more".
    fn board_groups(
        &self,
        board: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let mut list = div().flex().flex_col().w_full().min_w_0();
        for group in board["groups"].as_array().into_iter().flatten() {
            let id = text(group, "id");
            let open = group["open"] == true;
            let count = group["count"].as_u64().unwrap_or(0);
            let (icon, color) = group_look(&id);
            let icon_color: gpui::Hsla = color.map_or(p.muted, |color| rgb(color).into());
            let toggle = json!({"type": "toggleCoordinatorBoardGroup", "group": id});
            list = list.child(
                div()
                    .id(gpui::SharedString::from(format!("chat-board-group:{id}")))
                    .role(gpui::Role::Button)
                    .aria_label(format!("{}, {count}", text(group, "title")))
                    .aria_expanded(open)
                    .h(px(GROUP_HEIGHT * s))
                    .flex()
                    .items_center()
                    .gap(px(6.0 * s))
                    .px(px(4.0 * s))
                    .rounded(px(5.0 * s))
                    .text_size(px(12.0 * s))
                    .chat_cursor_pointer()
                    .hover(|style| style.bg(p.foreground.opacity(0.04)))
                    .child(
                        svg()
                            .path(if open {
                                "titlebar/chevron-down.svg"
                            } else {
                                "titlebar/chevron-right.svg"
                            })
                            .size(px(11.0 * s))
                            .flex_shrink_0()
                            .text_color(p.muted),
                    )
                    .child(
                        svg()
                            .path(icon)
                            .size(px(13.0 * s))
                            .flex_shrink_0()
                            .text_color(icon_color),
                    )
                    .child(div().text_color(p.foreground).child(text(group, "title")))
                    .child(div().text_color(p.card_muted).child(count.to_string()))
                    .on_click(cx.listener(move |this, _, _, cx| this.invoke(toggle.clone(), cx))),
            );
            for row in group["rows"].as_array().into_iter().flatten() {
                list = list.child(self.board_row(row, icon, icon_color, p, cx));
            }
            let more = text(group, "moreLabel");
            if !more.is_empty() {
                let not_shown = group["notShown"].as_u64().unwrap_or(0);
                let command = json!({"type": "coordinatorBoardMore", "group": id});
                list =
                    list.child(
                        div()
                            .id(gpui::SharedString::from(format!("chat-board-more:{id}")))
                            .role(gpui::Role::Button)
                            .h(px(MORE_HEIGHT * s))
                            .flex()
                            .items_center()
                            .gap(px(6.0 * s))
                            .pl(px(41.0 * s))
                            .text_size(px(11.5 * s))
                            .text_color(p.card_muted)
                            .chat_cursor_pointer()
                            .hover(|style| style.text_color(p.foreground))
                            .child(more)
                            .when(not_shown > 0, |this| {
                                this.child(div().text_color(p.muted).child(format!(
                                    "({not_shown} older in ghostex coordinator board)"
                                )))
                            })
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.invoke(command.clone(), cx)),
                            ),
                    );
            }
        }
        list.into_any_element()
    }

    /// One line of work: its group icon, title and age, then its tags and latest status. A click
    /// opens its thread, its question card or its link.
    fn board_row(
        &self,
        row: &Value,
        icon: &'static str,
        icon_color: gpui::Hsla,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let title = text(row, "title");
        let detail = text(row, "detail");
        let tooltip = [title.as_str(), detail.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let command = row["command"].clone();
        let clickable = command.is_object();
        let leaves_chat = command["type"] == "openCoordinatorThread";
        let mut tags = div().flex().items_center().gap(px(4.0 * s)).flex_shrink_0();
        for tag in row["tags"].as_array().into_iter().flatten() {
            tags = tags.child(self.board_tag(tag, p, cx));
        }
        div()
            .id(gpui::SharedString::from(format!(
                "chat-board-row:{}",
                text(row, "key")
            )))
            .role(gpui::Role::Button)
            .aria_label(format!("{title}. {detail}"))
            .h(px(ROW_HEIGHT * s))
            .flex()
            .items_start()
            .gap(px(7.0 * s))
            .pl(px(21.0 * s))
            .pr(px(4.0 * s))
            .pt(px(4.0 * s))
            .rounded(px(6.0 * s))
            .text_size(px(12.0 * s))
            .when(clickable, |this| {
                this.chat_cursor_pointer()
                    .hover(|style| style.bg(p.foreground.opacity(0.06)))
            })
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            })
            .child(
                div().pt(px(2.0 * s)).flex_shrink_0().child(
                    svg().path(icon).size(px(12.0 * s)).text_color(
                        if row["needsApproval"] == true {
                            rgb(crate::app::native_sidebar::status::APPROVAL_COLOR).into()
                        } else {
                            icon_color
                        },
                    ),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.0 * s))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0 * s))
                            .min_w_0()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_color(p.foreground)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(11.0 * s))
                                    .text_color(p.muted)
                                    .child(text(row, "age")),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(5.0 * s))
                            .min_w_0()
                            .text_size(px(11.0 * s))
                            .child(tags)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_color(p.card_muted)
                                    .child(detail),
                            ),
                    ),
            )
            .when(clickable, |this| {
                this.on_click(cx.listener(move |this, _, _, cx| {
                    this.invoke(command.clone(), cx);
                    if leaves_chat {
                        this.close_pill_menu(cx);
                    }
                }))
            })
            .into_any_element()
    }

    /// One tag chip, tinted by its tone; a PR chip opens its PR.
    fn board_tag(&self, tag: &Value, p: &ChatAppearance, cx: &mut Context<Self>) -> AnyElement {
        let s = p.scale;
        let color: gpui::Hsla =
            tone_color(&text(tag, "tone")).map_or(p.card_muted, |color| rgb(color).into());
        let tinted = tone_color(&text(tag, "tone")).is_some();
        let mono = matches!(text(tag, "kind").as_str(), "ref");
        let command = tag["command"].clone();
        let dot = (text(tag, "kind") == "pr" && tinted).then(|| {
            div()
                .size(px(5.0 * s))
                .rounded_full()
                .flex_shrink_0()
                .bg(color)
        });
        div()
            .id(gpui::SharedString::from(format!(
                "chat-board-tag:{}:{}",
                text(tag, "kind"),
                text(tag, "label")
            )))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(3.0 * s))
            .h(px(15.0 * s))
            .px(px(5.0 * s))
            .rounded(px(8.0 * s))
            .border(px(s))
            .border_color(if tinted && !mono {
                color.opacity(0.45)
            } else {
                p.foreground.opacity(0.12)
            })
            .text_size(px(10.5 * s))
            .text_color(if text(tag, "kind") == "pr" {
                p.card_muted
            } else {
                color
            })
            .when(mono, |this| {
                this.font_family(super::fonts::CHAT_MONO)
            })
            .children(dot)
            .child(text(tag, "label"))
            .when(command.is_object(), |this| {
                this.chat_cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.invoke(command.clone(), cx);
                    }))
            })
            .into_any_element()
    }
}
