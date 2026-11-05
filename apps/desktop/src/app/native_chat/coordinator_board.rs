//! A coordinator's board beside its chat: the Board pill opens it as its own column docked on the
//! right when the pane has room, else as a floating card over the chat box like the other pill
//! menus. Groups, rows, tags, folds and counts come from the core
//! (packages/gx-chat-core/src/extras/coordinator_board.rs, where the user's decision lives); this
//! decides where the board goes and draws it.
//!
//! The same board also opens as columns over the chat, one per group with its rows as cards: a
//! large chat menu window like the Chat Resources modal (resource_modal.rs). Which of the two the
//! Board pill opens is the `coordinatorBoardView` setting, which the switch at the top of either
//! view changes.
//!
//! CDXC:Coordinators 2026-11-05 DECISION:
//! User, of the day-in-the-life columns on the design page: "I like that board view too btw! Want
//! the ability to see that as pop up please not just the sidebar one", then "i want both the list
//! showing as a side pane and the board view to show as an overlay (user can switch which gets
//! shown and we save that as default for them until they change it)". So the Board pill opens the
//! view last picked: the list beside the chat (docked, or floating where the chat is narrow) or the
//! columns over it. A switch in each view's header picks the other and saves it as the setting
//! (Settings > Chat > Coordinator board opens as), shared by every chat and window of this Ghostex.
//! The web page cannot save settings, so there the switch lasts while the page is open. The phone
//! shows the list as a sheet.
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
pub(crate) const BOARD_WIDTH: f32 = 380.0;
/// The chat keeps at least this much (unscaled) beside a docked board; narrower, the board floats.
const CHAT_MIN_WIDTH: f32 = 620.0;
/// The pill's menu-toggle key and its pill-menu id (chat_pills.rs).
pub(super) const BOARD_TRIGGER: &str = "chat-pill-board";
pub(super) const BOARD_MENU: &str = ghostex_gx_chat_core::extras::coordinator_board::BOARD_PILL;
/// The columns overlay's menu-toggle key and pill-menu id.
pub(super) const BOARD_COLUMNS_TRIGGER: &str = "chat-board-columns";
pub(super) const BOARD_COLUMNS_MENU: &str = "boardColumns";
/// The setting that says which view the pill opens, and its two values.
const VIEW_SETTING: &str = "coordinatorBoardView";
const LIST_VIEW: &str = "list";
const COLUMNS_VIEW: &str = "columns";
/// The Done rail's width in the columns view, unscaled.
const DONE_RAIL: f32 = 36.0;
/// The overlay's distance from the chat's edges, and a column's least width, unscaled.
const OVERLAY_MARGIN: f32 = 16.0;
const COLUMN_MIN_WIDTH: f32 = 200.0;

/// Where a header is drawn: the docked column, the floating card, or the columns overlay.
#[derive(Clone, Copy, PartialEq)]
enum BoardPlace {
    Docked,
    Floating,
    Columns,
}

/// Fixed heights (unscaled), so the floating card knows its size before it opens. Every card has
/// the same two lines, so the same height.
const HEADER_HEIGHT: f32 = 34.0;
const GROUP_HEIGHT: f32 = 34.0;
const CARD_HEIGHT: f32 = 72.0;
const CARD_GAP: f32 = 8.0;
const MORE_HEIGHT: f32 = 28.0;
/// The chat's body text size (render.rs), which the board's titles use; its chips one step down.
const TITLE_SIZE: f32 = 14.0;
const CHIP_SIZE: f32 = 12.5;
/// A card's side padding and border, and the room a column keeps for the floating × badge.
const CARD_PAD: f32 = 12.0;
const BADGE_ROOM: f32 = 8.0;
/// Chip measures (unscaled): the gap between chips, a character's width at `CHIP_SIZE` (generous,
/// so an estimate never runs past the card), the widest a chip may grow, the narrowest a truncated
/// chip may shrink to, and the "+N" chip.
const CHIP_GAP: f32 = 6.0;
const CHIP_CHAR: f32 = 7.6;
const CHIP_MAX: f32 = 170.0;
const CHIP_MIN: f32 = 56.0;
const PLUS_WIDTH: f32 = 34.0;

/// A chip's width (unscaled) before any cap: its label, its icon and its padding.
fn chip_width(tag: &Value) -> f32 {
    let chars = text(tag, "label").chars().count() as f32;
    let icon = if chip_icon(&text(tag, "kind")).is_some() {
        18.0
    } else {
        0.0
    };
    chars * CHIP_CHAR + icon + 22.0
}

/// The chips that fit in `room` (unscaled) with the width each gets, and how many are left for a
/// "+N" chip: each chip at its width up to `CHIP_MAX`; the last that fits only in part is cut to
/// the room left (with an ellipsis) when that is at least `CHIP_MIN`.
///
/// CDXC:Coordinators 2026-11-05 WHY:
/// User, of chips running past a card's edge ("Held \u{b7} next full release + Tes"): "stuff cut off
/// man". A card's chip line never overflows: long chips are cut with an ellipsis (their tooltip
/// keeps the words) and what does not fit is counted in a "+N" chip, so every card keeps two lines.
fn fit_chips(tags: &[Value], room: f32) -> (Vec<(Value, f32)>, usize) {
    let mut out = Vec::new();
    let mut used = 0.0;
    for (index, tag) in tags.iter().enumerate() {
        let gap = if index == 0 { 0.0 } else { CHIP_GAP };
        let rest = tags.len() - index - 1;
        let reserve = if rest > 0 { CHIP_GAP + PLUS_WIDTH } else { 0.0 };
        let left = room - used - gap - reserve;
        let want = chip_width(tag).min(CHIP_MAX);
        if want <= left {
            out.push((tag.clone(), want));
            used += gap + want;
        } else if left >= CHIP_MIN {
            out.push((tag.clone(), left));
            return (out, rest);
        } else {
            return (out, tags.len() - index);
        }
    }
    (out, 0)
}

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
        // Backlog.
        "ideas" => ("titlebar/archive.svg", None),
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
        "held" | "merged" => Some(HELD_COLOR),
        "closed" => Some(0x8a8f98),
        _ => None,
    }
}

/// A chip's icon by what it links to.
fn chip_icon(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "pr" => "titlebar/git-pull-request.svg",
        "issue" => "titlebar/brand-github.svg",
        "linear" => "titlebar/brand-linear.svg",
        "branch" => "titlebar/git-branch.svg",
        "link" => "titlebar/link.svg",
        "question" => "titlebar/help-circle.svg",
        _ => return None,
    })
}

/// The board's content height (unscaled): its header, each group's line, an open group's rows
/// and its "N more" line.
pub(super) fn board_height(board: &Value) -> f32 {
    let mut height = HEADER_HEIGHT + 8.0;
    for group in board["groups"].as_array().into_iter().flatten() {
        height += GROUP_HEIGHT;
        let rows = group["rows"].as_array().map_or(0, Vec::len);
        height += rows as f32 * (CARD_HEIGHT + CARD_GAP);
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
    /// The view the switch last picked here: it applies at once, before the settings file comes
    /// back with it, and on the web page, which cannot save settings, for as long as the page is open.
    view_override: Option<&'static str>,
    /// The row whose × was clicked once and waits for the second click (`REMOVE_ARMED_FOR`).
    armed: Option<String>,
}

/// How long a row's first × click waits for the second, as the links list's ×
/// (native_sidebar/links_list.rs) does.
const REMOVE_ARMED_FOR: std::time::Duration = std::time::Duration::from_secs(3);

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

    /// The view the Board pill opens: the switch's pick here, else the setting, else the list.
    fn board_view(&self) -> &'static str {
        if let Some(view) = self.board.view_override {
            return view;
        }
        let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
        match settings.object().get(VIEW_SETTING).and_then(Value::as_str) {
            Some(COLUMNS_VIEW) => COLUMNS_VIEW,
            _ => LIST_VIEW,
        }
    }

    /// The Board pill was clicked: in the list view the docked column comes or goes where there is
    /// room, else the floating card opens or shuts; in the columns view the overlay does.
    pub(super) fn toggle_coordinator_board(
        &mut self,
        pill: gpui::Bounds<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.board_view() == COLUMNS_VIEW {
            if !self.chat_menu_toggled_shut(BOARD_COLUMNS_TRIGGER, cx) {
                self.open_board_columns_after_dock(window, cx);
            }
            return;
        }
        let p = ChatAppearance::current(&self.snapshot);
        if self.board_fits(&p) {
            self.board.open = !self.board.open;
            cx.notify();
        } else {
            self.toggle_pill_menu(BOARD_MENU, pill, window, cx);
        }
    }

    /// The switch in a view's header: saves `view` as the setting and shows the board that way.
    fn switch_board_view(&mut self, view: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        self.board.view_override = Some(view);
        self.host("setCoordinatorBoardView", json!({ "view": view }), cx);
        if view == COLUMNS_VIEW {
            self.open_board_columns_after_dock(window, cx);
        } else {
            self.close_pill_menu(cx);
            let p = ChatAppearance::current(&self.snapshot);
            if self.board_fits(&p) {
                self.board.open = true;
            } else {
                let pill = self.pill_bounds.cell(BOARD_MENU).get();
                cx.defer_in(window, move |this, window, cx| {
                    this.toggle_pill_menu(BOARD_MENU, pill, window, cx);
                });
            }
        }
        cx.notify();
    }

    /// Closes the docked list, if it is open, and opens the columns once the chat has taken its
    /// width back: a chat that changes size closes any window over it (child_window.rs
    /// `pane_frame_changed`), so the dock snaps shut instead of sliding and the overlay opens after
    /// it. Always after the click that asked for it, which would otherwise count as a press outside
    /// the new window.
    fn open_board_columns_after_dock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let was_docked = self.board.docked;
        self.board.open = false;
        if was_docked {
            self.board.motion.reset();
        }
        let wait = std::time::Duration::from_millis(if was_docked { 150 } else { 16 });
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let _ = this.update_in(cx, |this, window, cx| this.open_board_columns(window, cx));
        })
        .detach();
        cx.notify();
    }

    /// Restore on a card under Removed, at the end of its title line.
    fn board_restore_button(&self, row: &Value, p: &ChatAppearance, cx: &mut Context<Self>) -> Option<AnyElement> {
        let s = p.scale;
        let restore = row["restore"].clone();
        if !restore.is_object() {
            return None;
        }
        Some(
            div()
                .id(gpui::SharedString::from(format!("chat-board-restore:{}", text(row, "key"))))
                .role(gpui::Role::Button)
                .aria_label("Restore to the board")
                .flex_shrink_0()
                .px(px(6.0 * s))
                .rounded(px(5.0 * s))
                .text_size(px(CHIP_SIZE * s))
                .text_color(p.card_muted)
                .chat_cursor_pointer()
                .hover(|style| style.bg(p.foreground.opacity(0.08)).text_color(p.foreground))
                .child("Restore")
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.invoke(restore.clone(), cx);
                }))
                .into_any_element(),
        )
    }

    /// A card's ×, floating over its top-right corner like a badge while the card is hovered (or
    /// armed): a first click arms it red, a second moves the card to Removed. It takes no room in
    /// the card. Removing only hides a card: a thread is not closed or resolved.
    ///
    /// CDXC:Coordinators 2026-11-05 DECISION:
    /// User: "make the card show the x button top right as floating on the top right corner of the
    /// cards not as taking space in the card itself". Supersedes the × at the end of the title line.
    fn board_remove_badge(
        &self,
        row: &Value,
        hover_group: &gpui::SharedString,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let s = p.scale;
        let key = text(row, "key");
        let remove = row["remove"].clone();
        if !remove.is_object() {
            return None;
        }
        let armed = self.board.armed.as_deref() == Some(key.as_str());
        let red: gpui::Hsla = rgb(crate::app::native_sidebar::status::APPROVAL_COLOR).into();
        let label = if armed {
            "Click again to move it to Removed"
        } else {
            "Remove from the board"
        };
        Some(
            div()
                .id(gpui::SharedString::from(format!("chat-board-remove:{key}")))
                .role(gpui::Role::Button)
                .aria_label(label)
                .absolute()
                .top(px(-7.0 * s))
                .right(px(-7.0 * s))
                .size(px(20.0 * s))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border(px(s))
                .border_color(if armed { red.opacity(0.5) } else { p.foreground.opacity(0.14) })
                .bg(p.background)
                .chat_cursor_pointer()
                .when(!armed, |button| {
                    button
                        .opacity(0.0)
                        .group_hover(hover_group.clone(), |style| style.opacity(1.0))
                })
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(label).build(window, cx)
                })
                .child(
                    div()
                        .size_full()
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(armed, |inner| inner.bg(red.opacity(0.18)))
                        .hover(|style| style.bg(p.foreground.opacity(0.08)))
                        .child(
                            svg()
                                .path("titlebar/x.svg")
                                .size(px(11.0 * s))
                                .text_color(if armed { red } else { p.muted }),
                        ),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    if this.board.armed.as_deref() == Some(key.as_str()) {
                        this.board.armed = None;
                        this.invoke(remove.clone(), cx);
                    } else {
                        this.board.armed = Some(key.clone());
                        let armed_key = key.clone();
                        cx.spawn(async move |this, cx| {
                            cx.background_executor().timer(REMOVE_ARMED_FOR).await;
                            let _ = this.update(cx, |this, cx| {
                                if this.board.armed.as_deref() == Some(armed_key.as_str()) {
                                    this.board.armed = None;
                                    cx.notify();
                                }
                            });
                        })
                        .detach();
                    }
                    cx.notify();
                }))
                .into_any_element(),
        )
    }

    /// The overlay's size (unscaled): the chat less a margin all round.
    fn board_columns_size(&self) -> (f32, f32) {
        let scale = ChatAppearance::current(&self.snapshot).scale;
        let pane = self.bounds.get().size;
        (
            (f32::from(pane.width) / scale - 2.0 * OVERLAY_MARGIN).max(480.0),
            (f32::from(pane.height) / scale - 2.0 * OVERLAY_MARGIN - 16.0).max(280.0),
        )
    }

    /// Opens the columns over the chat, a margin in from its edges: upward from a trigger as wide
    /// as the overlay along the chat's bottom, as the Chat Resources modal opens (the variant that
    /// opens at a point fits its width to text rows, which a panel has none of).
    fn open_board_columns(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.board_value().is_none() {
            return;
        }
        let scale = ChatAppearance::current(&self.snapshot).scale;
        let (width, height) = self.board_columns_size();
        let pane = self.bounds.get();
        let trigger = gpui::Bounds::new(
            gpui::point(
                pane.origin.x + px(OVERLAY_MARGIN * scale),
                pane.origin.y + pane.size.height - px(OVERLAY_MARGIN * scale),
            ),
            gpui::size(px(width * scale), px(0.0)),
        );
        self.menu_side = Some(super::option_menu::MenuSide::Right);
        self.show_chat_menu(
            vec![json!({"pillMenu": {"id": BOARD_COLUMNS_MENU, "height": height}})],
            trigger,
            width,
            window,
            cx,
        );
        if let Some(menu) = &self.option_menu {
            menu.update(cx, |menu, _| menu.set_pill(BOARD_COLUMNS_TRIGGER));
        }
    }

    /// The overlay's content height, or `None` once the board is gone.
    pub(super) fn board_columns_height(&self) -> Option<f32> {
        self.board_value()?;
        Some(self.board_columns_size().1)
    }

    /// The columns overlay: the header with its switch, then one column per group, each scrolling
    /// on its own. Empty groups are left out.
    pub(super) fn board_columns_body(
        &mut self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let board = self.board_value()?.clone();
        let s = p.scale;
        let mut columns = div()
            .id("chat-coordinator-board-columns-row")
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .gap(px(10.0 * s))
            .pt(px(6.0 * s))
            .overflow_x_scroll();
        let groups = board["groups"].as_array().cloned().unwrap_or_default();
        // Done is a slim rail at the right edge instead of a lane.
        //
        // CDXC:Coordinators 2026-11-05 DECISION:
        // User (Q1248 "Slim rail"): Done with lots of cards on the right "is useless and shouldn't be
        // emphasised", keeping the other lanes' width. Done is a narrow muted strip with its check and
        // count, folded by default with the list's own fold of Done; a click opens its lane over the
        // right side, never squeezing the others, and a click on the rail or the lane's header folds
        // it again.
        let done = groups.iter().find(|group| group["id"] == "done").cloned();
        let lanes: Vec<&Value> = groups.iter().filter(|group| group["id"] != "done").collect();
        // Each column's width, so its cards fit their chips to it: the overlay, less the rail,
        // shared out, never under `COLUMN_MIN_WIDTH` (the row then scrolls sideways).
        let rail = if done.is_some() { DONE_RAIL + 10.0 } else { 0.0 };
        let shown = lanes.len().max(1) as f32;
        let column = ((self.board_columns_size().0 - 24.0 - rail - 10.0 * (shown - 1.0)) / shown)
            .max(COLUMN_MIN_WIDTH);
        for group in &lanes {
            columns = columns.child(self.board_column(group, column, p, cx));
        }
        let done_open = done.as_ref().is_some_and(|done| done["open"] == true);
        let rail_element = done.as_ref().map(|done| self.board_done_rail(done, done_open, p, cx));
        let done_lane = done.as_ref().filter(|_| done_open).map(|done| {
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .right(px((DONE_RAIL + 10.0) * s))
                .w(px(column * s))
                .pl(px(10.0 * s))
                .pt(px(6.0 * s))
                .rounded(px(10.0 * s))
                .bg(p.background)
                .shadow_lg()
                .child(self.board_column(done, column, p, cx))
        });
        let columns = div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .gap(px(10.0 * s))
            .child(columns)
            .children(rail_element)
            .children(done_lane);
        Some(
            div()
                .id("chat-coordinator-board-columns")
                .role(gpui::Role::Group)
                .aria_label("Board")
                .size_full()
                .min_h_0()
                .flex()
                .flex_col()
                .child(self.board_header(&board, BoardPlace::Columns, p, cx))
                .child(columns)
                .into_any_element(),
        )
    }

    /// The Done rail: a narrow muted strip with Done's check and count; a click opens or folds the
    /// Done lane.
    fn board_done_rail(
        &self,
        done: &Value,
        open: bool,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let count = done["count"].as_u64().unwrap_or(0);
        let label = if open {
            "Fold Done today"
        } else {
            "Show Done today"
        };
        div()
            .id("chat-board-done-rail")
            .role(gpui::Role::Button)
            .aria_label(format!("{label}, {count}"))
            .aria_expanded(open)
            .flex_shrink_0()
            .w(px(DONE_RAIL * s))
            .mt(px(6.0 * s))
            .mb(px(8.0 * s))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(8.0 * s))
            .pt(px(12.0 * s))
            .rounded(px(10.0 * s))
            .border(px(s))
            .border_color(p.foreground.opacity(0.08))
            .bg(p.foreground.opacity(if open { 0.07 } else { 0.03 }))
            .opacity(if open { 1.0 } else { 0.75 })
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.foreground.opacity(0.08)).opacity(1.0))
            .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(label).build(window, cx))
            .child(
                svg()
                    .path("titlebar/circle-check-filled.svg")
                    .size(px(14.0 * s))
                    .text_color(gpui::Hsla::from(rgb(GOOD_COLOR)).opacity(0.8)),
            )
            .child(
                div()
                    .text_size(px(CHIP_SIZE * s))
                    .text_color(p.card_muted)
                    .child(count.to_string()),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.invoke(
                    json!({"type": "toggleCoordinatorBoardGroup", "group": "done"}),
                    cx,
                );
            }))
            .into_any_element()
    }

    /// One group's column: its title and count, then its rows as cards.
    fn board_column(
        &self,
        group: &Value,
        width: f32,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let id = text(group, "id");
        let (icon, color) = group_look(&id);
        let icon_color: gpui::Hsla = color.map_or(p.muted, |color| rgb(color).into());
        let rows = group["columnRows"].as_array().cloned().unwrap_or_default();
        let not_shown = group["count"]
            .as_u64()
            .unwrap_or(0)
            .saturating_sub(rows.len() as u64);
        let mut cards = div()
            .id(gpui::SharedString::from(format!("chat-board-column-rows:{id}")))
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(CARD_GAP * s))
            // Room for the × badge that floats over a card's top-right corner.
            .pt(px(BADGE_ROOM * s))
            .pr(px(BADGE_ROOM * s))
            .pb(px(8.0 * s));
        let inner = width - BADGE_ROOM - 2.0 * CARD_PAD - 2.0;
        for row in &rows {
            cards = cards.child(self.board_card(row, inner, p, cx));
        }
        if rows.is_empty() {
            cards = cards.child(
                div()
                    .px(px(4.0 * s))
                    .text_size(px(CHIP_SIZE * s))
                    .text_color(p.muted)
                    .child("Nothing here"),
            );
        }
        if not_shown > 0 {
            cards = cards.child(
                div()
                    .px(px(4.0 * s))
                    .text_size(px(CHIP_SIZE * s))
                    .text_color(p.muted)
                    .child(format!("{not_shown} older in ghostex coordinator board")),
            );
        }
        div()
            .flex_1()
            .min_w(px(COLUMN_MIN_WIDTH * s))
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(6.0 * s))
            .child(
                div()
                    .id(gpui::SharedString::from(format!("chat-board-column-head:{id}")))
                    .flex_shrink_0()
                    .h(px(GROUP_HEIGHT * s))
                    // The Done lane's header folds it back into its rail.
                    .when(id == "done", |head| {
                        head.chat_cursor_pointer().on_click(cx.listener(|this, _, _, cx| {
                            this.invoke(
                                json!({"type": "toggleCoordinatorBoardGroup", "group": "done"}),
                                cx,
                            );
                        }))
                    })
                    .flex()
                    .items_center()
                    .gap(px(6.0 * s))
                    .px(px(4.0 * s))
                    .text_size(px(TITLE_SIZE * s))
                    .child(
                        svg()
                            .path(icon)
                            .size(px(13.0 * s))
                            .flex_shrink_0()
                            .text_color(icon_color),
                    )
                    .child(div().text_color(p.foreground).child(text(group, "title")))
                    .child(
                        div()
                            .text_color(p.card_muted)
                            .child(group["count"].as_u64().unwrap_or(0).to_string()),
                    ),
            )
            .child(cards)
            .into_any_element()
    }

    /// One line of work as a card, the same in the list and the columns: two lines, always. The
    /// first is its title (the group's icon is on the group's header only), how long ago it moved, the agent button that opens its
    /// thread, and its × (or Restore); the second is the work's links (PR, issue, Linear ticket,
    /// branch, release), each opening its page, or a quiet "No links yet". The card itself opens
    /// nothing, except a question's, which jumps to its card.
    ///
    /// CDXC:Coordinators 2026-11-05 DECISION:
    /// User, of the first board list: "so crowded man. no need to show first line or the name of the
    /// thread ... make this more focused on the branch/pr/issue/linear so i can click on those ... the
    /// sub thread from ghostex itself shouldnt appear when i click on these ones basically except if
    /// we add a button that shows agent icon then clicking just that should do it. i also wants cards
    /// around each one, and we must have same number of lines for all please ... font size needs to be
    /// same font size as chat too". Supersedes the rows with an id chip, tags and the report's first
    /// line, and the click that opened the thread.
    fn board_card(
        &self,
        row: &Value,
        inner_width: f32,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let key = text(row, "key");
        let title = text(row, "title");
        let tooltip = [title.as_str(), text(row, "detail").as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let command = row["command"].clone();
        let reference = text(row, "reference");
        let clickable = command.is_object() || !reference.is_empty();
        let hover_group = gpui::SharedString::from(format!("chat-board-card-hover:{key}"));
        // Line 2: the chips that fit, a "+N" for the rest, and the age at its right end.
        //
        // CDXC:Coordinators 2026-11-05 DECISION:
        // User: "make the time bottom right in the card (right aligned on 2nd line)".
        let age = text(row, "age");
        let age_width = age.chars().count() as f32 * CHIP_CHAR * 0.95 + 4.0;
        let tags = row["tags"].as_array().cloned().unwrap_or_default();
        let answers = row["answers"].as_array().cloned().unwrap_or_default();
        let (fitted, hidden) = if answers.is_empty() {
            fit_chips(&tags, inner_width - age_width - CHIP_GAP)
        } else {
            (Vec::new(), 0)
        };
        let mut chips = div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(CHIP_GAP * s))
            .overflow_hidden();
        for (tag, width) in &fitted {
            chips = chips.child(self.board_tag(tag, *width, p, cx));
        }
        if hidden > 0 {
            let rest = tags[tags.len() - hidden..]
                .iter()
                .map(|tag| text(tag, "label"))
                .collect::<Vec<_>>()
                .join(", ");
            chips = chips.child(
                div()
                    .id(gpui::SharedString::from(format!("chat-board-more-chips:{key}")))
                    .flex_shrink_0()
                    .h(px(22.0 * s))
                    .px(px(7.0 * s))
                    .flex()
                    .items_center()
                    .rounded(px(11.0 * s))
                    .border(px(s))
                    .border_color(p.foreground.opacity(0.1))
                    .text_size(px(CHIP_SIZE * s))
                    .text_color(p.card_muted)
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(rest.clone()).build(window, cx)
                    })
                    .child(format!("+{hidden}")),
            );
        }
        // An item that asks something shows its answers in place of links; a card with neither
        // shows the coordinator's note for it, and only a card with nothing at all says so.
        if !answers.is_empty() {
            chips = chips.child(self.board_answers(&key, &answers, p, cx));
        } else if tags.is_empty() {
            let note = text(row, "note");
            chips = chips.child(
                div()
                    .id(gpui::SharedString::from(format!("chat-board-note:{key}")))
                    .min_w_0()
                    .truncate()
                    .text_size(px(CHIP_SIZE * s))
                    .text_color(if note.is_empty() { p.muted } else { p.card_muted })
                    .when(!note.is_empty(), |this| {
                        let note = note.clone();
                        this.tooltip(move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(note.clone()).build(window, cx)
                        })
                    })
                    .child(if note.is_empty() { "No links yet".to_string() } else { note }),
            );
        }
        let open_thread = row["openThread"].clone();
        let agent_icon = text(row, "agentIcon");
        let agent_button = open_thread.is_object().then(|| {
            div()
                .id(gpui::SharedString::from(format!("chat-board-open:{key}")))
                .role(gpui::Role::Button)
                .aria_label(format!("Open the thread {title}"))
                .flex_shrink_0()
                .size(px(24.0 * s))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0 * s))
                .border(px(s))
                .border_color(p.foreground.opacity(0.1))
                .chat_cursor_pointer()
                .hover(|style| style.bg(p.foreground.opacity(0.08)))
                .tooltip(|window, cx| {
                    gpui_component::tooltip::Tooltip::new("Open the thread").build(window, cx)
                })
                .child(
                    svg()
                        .path(format!("agent-icons/{agent_icon}.svg"))
                        .size(px(14.0 * s))
                        .text_color(p.foreground),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.invoke(open_thread.clone(), cx);
                    this.close_pill_menu(cx);
                }))
                .into_any_element()
        });
        div()
            .id(gpui::SharedString::from(format!("chat-board-card:{key}")))
            .group(hover_group.clone())
            .relative()
            .role(gpui::Role::Group)
            .aria_label(title.clone())
            .w_full()
            .flex_shrink_0()
            .h(px(CARD_HEIGHT * s))
            .flex()
            .flex_col()
            .justify_center()
            .gap(px(6.0 * s))
            .px(px(CARD_PAD * s))
            .rounded(px(10.0 * s))
            .border(px(s))
            .border_color(p.foreground.opacity(0.08))
            .bg(p.foreground.opacity(0.035))
            .when(clickable, |this| {
                this.chat_cursor_pointer()
                    .hover(|style| style.bg(p.foreground.opacity(0.06)))
            })
            .child(
                div()
                    .h(px(24.0 * s))
                    .flex()
                    .items_center()
                    .gap(px(8.0 * s))
                    .min_w_0()
                    .child(
                        div()
                            .id(gpui::SharedString::from(format!("chat-board-title:{key}")))
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(TITLE_SIZE * s))
                            .text_color(p.foreground)
                            .tooltip(move |window, cx| {
                                gpui_component::tooltip::Tooltip::new(tooltip.clone())
                                    .build(window, cx)
                            })
                            .child(title),
                    )
                    .children(agent_button)
                    .children(self.board_restore_button(row, p, cx)),
            )
            .child(
                div()
                    .h(px(22.0 * s))
                    .flex()
                    .items_center()
                    .gap(px(CHIP_GAP * s))
                    .min_w_0()
                    .child(chips)
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(CHIP_SIZE * s))
                            .text_color(p.muted)
                            .child(age),
                    ),
            )
            .children(self.board_remove_badge(row, &hover_group, p, cx))
            .when(clickable, |this| {
                this.on_click(cx.listener(move |this, _, _, cx| {
                    if reference.is_empty() {
                        this.invoke(command.clone(), cx);
                    } else {
                        this.insert_board_reference(&reference, cx);
                    }
                }))
            })
            .into_any_element()
    }

    /// Puts a card's reference in the chat box at the caret (or over the selection) and gives the
    /// box the keyboard, so the user can comment on it; a floating board closes out of the way.
    fn insert_board_reference(&mut self, reference: &str, cx: &mut Context<Self>) {
        let selection = self
            .input
            .as_ref()
            .map(|input| input.read(cx).selected_range())
            .unwrap_or(self.draft.len()..self.draft.len());
        let start = self.draft[..selection.start.min(self.draft.len())]
            .encode_utf16()
            .count();
        let end = self.draft[..selection.end.min(self.draft.len())]
            .encode_utf16()
            .count();
        self.invoke(
            json!({"type": "insertBoardReference", "reference": reference, "text": self.draft, "start": start, "end": end}),
            cx,
        );
        self.close_pill_menu(cx);
        self.focus_requested = true;
        cx.notify();
    }

    /// A card's answers as one segmented pill, one click each; the answer sent from here stays
    /// marked, and the recommended one is in the foreground.
    fn board_answers(
        &self,
        key: &str,
        answers: &[Value],
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let mut pill = div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .h(px(22.0 * s))
            .rounded(px(11.0 * s))
            .border(px(s))
            .border_color(p.foreground.opacity(0.14))
            .overflow_hidden();
        for (index, answer) in answers.iter().enumerate() {
            let label = text(answer, "label");
            let picked = answer["picked"] == true;
            let recommended = answer["recommended"] == true;
            let command = answer["command"].clone();
            pill = pill.child(
                div()
                    .id(gpui::SharedString::from(format!("chat-board-answer:{key}:{index}")))
                    .role(gpui::Role::Button)
                    .aria_label(format!("Answer {label}"))
                    .h_full()
                    .px(px(10.0 * s))
                    .flex()
                    .items_center()
                    .when(index > 0, |this| this.border_l_1().border_color(p.foreground.opacity(0.14)))
                    .text_size(px(CHIP_SIZE * s))
                    .text_color(if picked || recommended { p.foreground } else { p.card_muted })
                    .when(picked, |this| this.bg(p.foreground.opacity(0.14)))
                    .chat_cursor_pointer()
                    .hover(|style| style.bg(p.foreground.opacity(0.08)))
                    .when(recommended, |this| {
                        this.tooltip(|window, cx| {
                            gpui_component::tooltip::Tooltip::new("Recommended").build(window, cx)
                        })
                    })
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.invoke(command.clone(), cx);
                    })),
            );
        }
        pill.into_any_element()
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
                    .child(self.board_header(&board, BoardPlace::Docked, p, cx)),
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
                .child(self.board_header(&board, BoardPlace::Floating, p, cx))
                .child(
                    div()
                        .id("chat-coordinator-board-menu-list")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        // Room for the × badge over a card's top-right corner.
                        .px(px(10.0 * p.scale))
                        .pt(px(4.0 * p.scale))
                        .child(self.board_groups(&board, p, cx)),
                )
                .into_any_element(),
        )
    }

    /// "Board", its counts line, the switch to the other view, and (docked or as columns) the close
    /// button.
    fn board_header(
        &self,
        board: &Value,
        place: BoardPlace,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let columns = place == BoardPlace::Columns;
        let (switch_label, switch_icon, switch_to) = if columns {
            ("Show as a list beside the chat", "titlebar/layout-sidebar-right-expand.svg", LIST_VIEW)
        } else {
            ("Show as columns", "titlebar/layout-columns.svg", COLUMNS_VIEW)
        };
        let switch = div()
            .id("chat-coordinator-board-switch")
            .role(gpui::Role::Button)
            .aria_label(switch_label)
            .flex_shrink_0()
            .h(px(20.0 * s))
            .flex()
            .items_center()
            .gap(px(4.0 * s))
            .px(px(5.0 * s))
            .rounded(px(5.0 * s))
            .text_size(px(CHIP_SIZE * s))
            .text_color(p.card_muted)
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.foreground.opacity(0.06)).text_color(p.foreground))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(switch_label).build(window, cx)
            })
            .child(
                svg()
                    .path(switch_icon)
                    .size(px(13.0 * s))
                    .text_color(p.muted),
            )
            .child(if columns { "List" } else { "Columns" })
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.switch_board_view(switch_to, window, cx);
            }))
            .into_any_element();
        let docked = place != BoardPlace::Floating;
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
                .on_click(cx.listener(move |this, _, _, cx| {
                    if columns {
                        this.close_pill_menu(cx);
                    } else {
                        this.board.open = false;
                    }
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
            .text_size(px(TITLE_SIZE * s))
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
            .child(switch)
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
                    .gap(px(8.0 * s))
                    .px(px(4.0 * s))
                    .rounded(px(6.0 * s))
                    .text_size(px(TITLE_SIZE * s))
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
            let mut cards = div()
                .flex()
                .flex_col()
                .gap(px(CARD_GAP * s))
                .pb(px(CARD_GAP * s));
            // The docked column's and the floating card's width, less the list's and the card's
            // padding.
            let inner = BOARD_WIDTH - 2.0 * 10.0 - 2.0 * CARD_PAD - 2.0 - BADGE_ROOM;
            for row in group["rows"].as_array().into_iter().flatten() {
                cards = cards.child(self.board_card(row, inner, p, cx));
            }
            list = list.child(cards);
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
                            .pl(px(12.0 * s))
                            .text_size(px(CHIP_SIZE * s))
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

    /// One link chip: its icon in the link's colour (a PR's by its checks, then its state), its
    /// label; a click opens its page.
    fn board_tag(
        &self,
        tag: &Value,
        width: f32,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let kind = text(tag, "kind");
        let tint = tone_color(&text(tag, "tone"));
        let color: gpui::Hsla = tint.map_or(p.card_muted, |color| rgb(color).into());
        let command = tag["command"].clone();
        let clickable = command.is_object();
        let icon = chip_icon(&kind);
        let label = text(tag, "label");
        let tip = match text(tag, "title") {
            title if title.is_empty() => label.clone(),
            title => title,
        };
        div()
            .id(gpui::SharedString::from(format!("chat-board-tag:{kind}:{label}")))
            .flex_shrink_0()
            .max_w(px(width * s))
            .min_w_0()
            .overflow_hidden()
            .flex()
            .items_center()
            .gap(px(5.0 * s))
            .h(px(22.0 * s))
            .px(px(7.0 * s))
            .rounded(px(11.0 * s))
            .border(px(s))
            .border_color(if tint.is_some() && kind == "release" {
                color.opacity(0.45)
            } else {
                p.foreground.opacity(0.1)
            })
            .text_size(px(CHIP_SIZE * s))
            .text_color(if kind == "release" || kind == "question" {
                color
            } else {
                p.card_muted
            })
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
            })
            .when_some(icon, |this, icon| {
                this.child(
                    svg()
                        .path(icon)
                        .size(px(13.0 * s))
                        .flex_shrink_0()
                        .text_color(color),
                )
            })
            .child(div().min_w_0().truncate().child(label))
            .when(clickable, |this| {
                this.chat_cursor_pointer()
                    .hover(|style| style.bg(p.foreground.opacity(0.08)).text_color(p.foreground))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.invoke(command.clone(), cx);
                    }))
            })
            .into_any_element()
    }

}
