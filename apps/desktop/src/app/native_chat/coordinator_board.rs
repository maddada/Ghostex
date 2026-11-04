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
/// The overlay's distance from the chat's edges, and a column's least width, unscaled.
const OVERLAY_MARGIN: f32 = 16.0;
const COLUMN_MIN_WIDTH: f32 = 220.0;

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

    /// A row's × (a first click arms it red, a second moves the row to Removed) or, under Removed,
    /// its Restore. The × shows while the row is hovered or armed. Removing only hides a row: a
    /// thread is not closed or resolved.
    fn board_row_action(
        &self,
        row: &Value,
        hover_group: &gpui::SharedString,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let s = p.scale;
        let key = text(row, "key");
        let restore = row["restore"].clone();
        if restore.is_object() {
            return Some(
                div()
                    .id(gpui::SharedString::from(format!("chat-board-restore:{key}")))
                    .role(gpui::Role::Button)
                    .aria_label("Restore to the board")
                    .flex_shrink_0()
                    .px(px(5.0 * s))
                    .rounded(px(4.0 * s))
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
            );
        }
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
                .flex_shrink_0()
                .size(px(16.0 * s))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0 * s))
                .chat_cursor_pointer()
                .when(armed, |button| button.bg(red.opacity(0.16)))
                .when(!armed, |button| {
                    button
                        .opacity(0.0)
                        .group_hover(hover_group.clone(), |style| style.opacity(1.0))
                        .hover(|style| style.bg(p.foreground.opacity(0.1)))
                })
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(label).build(window, cx)
                })
                .child(
                    svg()
                        .path("titlebar/x.svg")
                        .size(px(11.0 * s))
                        .text_color(if armed { red } else { p.muted }),
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
    /// on its own. Ideas gets a column only while it has something.
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
            .min_h_0()
            .flex()
            .gap(px(10.0 * s))
            .pt(px(6.0 * s))
            .overflow_x_scroll();
        for group in board["groups"].as_array().into_iter().flatten() {
            let id = text(group, "id");
            let count = group["count"].as_u64().unwrap_or(0);
            // Ideas and Removed take a column only while they hold something.
            if matches!(id.as_str(), "ideas" | "removed") && count == 0 {
                continue;
            }
            columns = columns.child(self.board_column(group, p, cx));
        }
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

    /// One group's column: its title and count, then its rows as cards.
    fn board_column(&self, group: &Value, p: &ChatAppearance, cx: &mut Context<Self>) -> AnyElement {
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
            .pb(px(8.0 * s));
        for row in &rows {
            cards = cards.child(self.board_card(row, p, cx));
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
                    .flex_shrink_0()
                    .h(px(GROUP_HEIGHT * s))
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
    fn board_card(&self, row: &Value, p: &ChatAppearance, cx: &mut Context<Self>) -> AnyElement {
        let s = p.scale;
        let key = text(row, "key");
        let title = text(row, "title");
        let tooltip = [title.as_str(), text(row, "detail").as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let command = row["command"].clone();
        let clickable = command.is_object();
        let hover_group = gpui::SharedString::from(format!("chat-board-card-hover:{key}"));
        let mut chips = div()
            .flex()
            .items_center()
            .gap(px(6.0 * s))
            .min_w_0()
            .overflow_hidden()
            .h(px(22.0 * s));
        let tags = row["tags"].as_array().cloned().unwrap_or_default();
        for tag in &tags {
            chips = chips.child(self.board_tag(tag, p, cx));
        }
        if tags.is_empty() {
            chips = chips.child(
                div()
                    .text_size(px(CHIP_SIZE * s))
                    .text_color(p.muted)
                    .child("No links yet"),
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
            .role(gpui::Role::Group)
            .aria_label(title.clone())
            .flex_shrink_0()
            .h(px(CARD_HEIGHT * s))
            .flex()
            .flex_col()
            .justify_center()
            .gap(px(6.0 * s))
            .px(px(12.0 * s))
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
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(CHIP_SIZE * s))
                            .text_color(p.muted)
                            .child(text(row, "age")),
                    )
                    .children(agent_button)
                    .children(self.board_row_action(row, &hover_group, p, cx)),
            )
            .child(div().min_w_0().child(chips))
            .when(clickable, |this| {
                this.on_click(cx.listener(move |this, _, _, cx| {
                    this.invoke(command.clone(), cx);
                }))
            })
            .into_any_element()
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
            for row in group["rows"].as_array().into_iter().flatten() {
                cards = cards.child(self.board_card(row, p, cx));
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
    fn board_tag(&self, tag: &Value, p: &ChatAppearance, cx: &mut Context<Self>) -> AnyElement {
        let s = p.scale;
        let kind = text(tag, "kind");
        let tint = tone_color(&text(tag, "tone"));
        let color: gpui::Hsla = tint.map_or(p.card_muted, |color| rgb(color).into());
        let command = tag["command"].clone();
        let clickable = command.is_object();
        let icon = chip_icon(&kind);
        div()
            .id(gpui::SharedString::from(format!(
                "chat-board-tag:{}:{}",
                kind,
                text(tag, "label")
            )))
            .flex_shrink_0()
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
            .when_some(icon, |this, icon| {
                this.child(
                    svg()
                        .path(icon)
                        .size(px(13.0 * s))
                        .flex_shrink_0()
                        .text_color(color),
                )
            })
            .child(text(tag, "label"))
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
