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
/// A collapsed column's strip in the columns view, unscaled.
const STRIP_WIDTH: f32 = 36.0;
/// The overlay's distance from the chat's edges, and a column's least width, unscaled.
const OVERLAY_MARGIN: f32 = 16.0;
const COLUMN_MIN_WIDTH: f32 = 240.0;
/// The setting that lists the columns collapsed to their strip (group ids, comma-separated).
const COLLAPSED_SETTING: &str = "coordinatorBoardCollapsedColumns";

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
/// The panel's padding under its list, which its height counts too.
const LIST_BOTTOM: f32 = 16.0;
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
    let icon = if chip_icon(&text(tag, "kind")).is_some() || !text(tag, "icon").is_empty() {
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
pub(super) fn group_look(group: &str) -> (&'static str, Option<u32>) {
    match group {
        "needs" => (
            "titlebar/circle-dot.svg",
            Some(crate::app::native_sidebar::status::QUESTION_COLOR),
        ),
        "blocked" => (
            "titlebar/circle-minus-filled.svg",
            Some(crate::app::native_sidebar::status::APPROVAL_COLOR),
        ),
        // On hold: a pause in a calm blue-grey, never the red of Blocked.
        "hold" => ("titlebar/player-pause.svg", Some(HOLD_COLOR)),
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
/// On hold's colour, a calm blue-grey (the phone's `GROUP_COLOR.hold`).
const HOLD_COLOR: u32 = 0x7fa1c4;

/// A tag's tint (`BoardTag::tone`), or `None` for the muted chip.
pub(super) fn tone_color(tone: &str) -> Option<u32> {
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
/// Whether a board action leaves the board for a chat: a thread's chat, or a question or page
/// (`jumpToQuestion`, or the question link a card's chip carries).
fn board_action_jumps(action: &Value) -> bool {
    match action["type"].as_str() {
        Some("jumpToQuestion" | "openCoordinatorThread") => true,
        Some("openMarkdownLink") => action["href"].as_str().is_some_and(|href| {
            href.starts_with(ghostex_gx_chat_core::transcript::open_questions::QUESTION_LINK_PREFIX)
        }),
        _ => false,
    }
}

fn chip_icon(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "pr" => "titlebar/git-pull-request.svg",
        "issue" => "titlebar/brand-github.svg",
        "linear" => "titlebar/brand-linear.svg",
        "branch" => "titlebar/git-branch.svg",
        "link" => "titlebar/link.svg",
        "question" => "titlebar/help-circle.svg",
        // A work item's chips (gx-chat-core work_panel.rs).
        "owner" => "titlebar/user.svg",
        "steps" => "titlebar/checklist.svg",
        _ => return None,
    })
}

/// The fill of a group a dragged card is over.
fn p_drop(p: &ChatAppearance) -> gpui::Hsla {
    p.foreground.opacity(0.06)
}

/// A `#rrggbb` colour (a user tag's), or `None`.
fn hex_color(hex: &str) -> Option<gpui::Hsla> {
    let digits = hex.strip_prefix('#')?;
    if digits.len() != 6 {
        return None;
    }
    u32::from_str_radix(digits, 16)
        .ok()
        .map(|color| rgb(color).into())
}

/// A card's answers with Reply after them when it offers both (work_actions.rs `reply_answer`):
/// locked once one is given, showing the words once a reply was sent from here (`replied`).
fn with_reply(row: &Value, answers: &[Value]) -> Vec<Value> {
    let mut all = answers.to_vec();
    let reply = &row["actions"]["reply"];
    if !answers.is_empty() && reply.is_object() {
        let locked = answers.iter().any(|answer| answer["locked"] == true);
        all.push(NativeChatView::reply_answer(
            reply,
            &text(row, "replied"),
            locked,
        ));
    }
    all
}

/// An open card's height (unscaled), estimated from its words: its title and latest line wrap at
/// the list's width, then its chips and its buttons.
fn expanded_card_height(row: &Value) -> f32 {
    let room = (BOARD_WIDTH - 2.0 * 10.0 - 2.0 * CARD_PAD - BADGE_ROOM) / 7.0;
    let lines = |words: &str| (words.chars().count() as f32 / room).ceil().max(1.0);
    let line = Some(text(row, "detail"))
        .filter(|line| !line.is_empty())
        .unwrap_or_else(|| text(row, "note"));
    let body = if line.is_empty() {
        0.0
    } else {
        lines(&line) * 19.0 + 6.0
    };
    // A work ask's whole detail under its title (`board_card`).
    let detail = match text(row, "body").as_str() {
        "" => 0.0,
        words => lines(words) * 19.0 + 6.0,
    };
    20.0 + lines(&text(row, "title")) * 20.0 + detail + body + 28.0 + 32.0
}

/// The board's content height (unscaled): its header, each group's line, an open group's rows
/// and its "N more" line.
pub(super) fn board_height(board: &Value) -> f32 {
    // The header, and room under the last group's line: the panel's own bottom padding cut the
    // last line ("Finished today") when everything fit.
    let mut height = HEADER_HEIGHT + 8.0 + LIST_BOTTOM;
    for group in board["groups"].as_array().into_iter().flatten() {
        height += GROUP_HEIGHT;
        for row in group["rows"].as_array().into_iter().flatten() {
            height += CARD_GAP
                + if row["expanded"] == true {
                    expanded_card_height(row).max(CARD_HEIGHT)
                } else {
                    CARD_HEIGHT
                };
        }
        if !text(group, "moreLabel").is_empty() {
            height += MORE_HEIGHT;
        }
    }
    height
}

/// Measures the card it sits in and scrolls `scroll`'s list until the whole card is in view (just
/// below the list's top when it has to move), a few tries at most.
fn scroll_into_view(
    scroll: ScrollHandle,
    tries: std::rc::Rc<std::cell::Cell<u8>>,
    s: f32,
) -> impl IntoElement {
    gpui::canvas(
        move |card, window, _| {
            if tries.get() == 0 {
                return;
            }
            tries.set(tries.get() - 1);
            let list = scroll.bounds();
            if list.size.height <= px(0.0) {
                window.refresh();
                return;
            }
            if card.top() >= list.top() && card.bottom() <= list.bottom() {
                tries.set(0);
                return;
            }
            let offset = scroll.offset();
            let max = scroll.max_offset();
            let y = (offset.y - (card.top() - list.top() - px(8.0 * s))).clamp(-max.y, px(0.0));
            if (y - offset.y).abs() < px(1.0) {
                tries.set(0);
                return;
            }
            scroll.set_offset(gpui::point(offset.x, y));
            window.refresh();
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
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
    /// The floating card's list.
    menu_scroll: ScrollHandle,
    /// Each column's rows in the columns view, by group id.
    column_scrolls: std::cell::RefCell<std::collections::HashMap<String, ScrollHandle>>,
    /// The columns view's row, which scrolls sideways when the open columns do not fit.
    columns_scroll: ScrollHandle,
    /// The columns collapsed here, ahead of the settings file (and on the web page, which cannot
    /// save settings, for as long as the page is open).
    collapsed_override: Option<Vec<String>>,
    /// A drag on the row's background that scrolls it: where the pointer went down and the row's
    /// offset then.
    pan: Option<(gpui::Pixels, gpui::Pixels)>,
    /// A card took this mouse press, so the row under it does not start a pan.
    card_pressed: bool,
    /// The card a board pill in the chat asked to see (`focus` in the core's board), ringed for a
    /// moment once its list has scrolled to it.
    focus: Option<BoardFocus>,
    /// The card whose menu is open (a right-click, or its ⋯), and where.
    menu: Option<BoardCardMenu>,
    /// Where a count on the dock's status line was clicked: the next focus opens the panel as its
    /// floating card there, over the chat, even where it could dock (work_cards.rs).
    pub(super) float_at: Option<gpui::Bounds<gpui::Pixels>>,
    /// The dock's top edge at its last paint, `None` while it is not drawn: the work items list
    /// its header's button opens sits above the dock instead of over it (work_cards.rs).
    pub(super) dock_top: std::rc::Rc<std::cell::Cell<Option<gpui::Pixels>>>,
    /// The dock's rows' width at its last paint (unscaled), 0 before the first: what decides
    /// whether a row's answers fit beside its question (work_cards.rs `dock_row`).
    pub(super) dock_width: std::rc::Rc<std::cell::Cell<f32>>,
    /// When each page row last asked its gxserver for its counts (work_cards.rs
    /// `refresh_page_counts`), by page id.
    pub(super) page_counts_asked:
        std::cell::RefCell<std::collections::HashMap<String, web_time::Instant>>,
}

/// An open card menu: its card, the window point it opened at, whether it shows its Tags, or only
/// the card's Snooze lengths (its Snooze button), and the window its board is in (the docked
/// chat's, or the floating board's or the columns' own) with that window's size.
pub(crate) struct BoardCardMenu {
    key: String,
    at: gpui::Point<gpui::Pixels>,
    tags: bool,
    snooze: bool,
    parent: gpui::AnyWindowHandle,
    viewport: gpui::Size<gpui::Pixels>,
}

/// The card menu's width and inset (unscaled); its rows are the sidebar menu's.
const BOARD_MENU_WIDTH: f32 = 220.0;
const BOARD_MENU_PADDING: f32 = 6.0;
/// The Snooze lengths' width (unscaled): their words are short.
const BOARD_SNOOZE_MENU_WIDTH: f32 = 160.0;
use crate::app::native_sidebar::{SIDEBAR_MENU_ROW_HEIGHT, SIDEBAR_MENU_SEPARATOR_HEIGHT};

/// A card being dragged to another group, and the picture that follows the pointer.
#[derive(Clone)]
pub(crate) struct BoardCardDrag {
    title: String,
    /// The core's `moveTo`, which the drop completes with the group.
    move_to: Value,
    /// The group it is dragged from; dropping it back there does nothing.
    from: String,
    scale: f32,
    foreground: gpui::Hsla,
    surface: gpui::Hsla,
}

impl gpui::Render for BoardCardDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let s = self.scale;
        div()
            .max_w(px(280.0 * s))
            .px(px(12.0 * s))
            .py(px(8.0 * s))
            .rounded(px(10.0 * s))
            .bg(self.surface)
            .border(px(s))
            .border_color(self.foreground.opacity(0.14))
            .shadow(vec![gpui::BoxShadow {
                color: gpui::black().opacity(0.3),
                offset: gpui::point(px(0.0), px(8.0 * s)),
                blur_radius: px(20.0 * s),
                spread_radius: px(0.0),
                inset: false,
            }])
            .text_size(px(TITLE_SIZE * s))
            .text_color(self.foreground)
            .truncate()
            .child(self.title.clone())
    }
}

/// The groups a card can be dragged to, with their titles, in the board's order (the core's
/// `GROUPS`; Removed only while it shows).
const DROP_GROUPS: [(&str, &str); 7] = [
    ("ideas", "Backlog"),
    ("later", "To do"),
    ("needs", "Needs you"),
    ("blocked", "Blocked"),
    ("hold", "On hold"),
    ("moving", "In progress"),
    ("done", "Done today"),
];

/// A board pill's click: the card to show, and what is left to do for it.
pub(crate) struct BoardFocus {
    key: String,
    seq: u64,
    /// The board still has to open (done in the next frame, which has the window).
    open: bool,
    /// Scroll attempts left; the card stops scrolling its list once it is in view.
    tries: std::rc::Rc<std::cell::Cell<u8>>,
}

/// How long a shown card keeps its ring.
const FOCUS_RING_FOR: std::time::Duration = std::time::Duration::from_millis(2200);

/// How long a row's first × click waits for the second, as the links list's ×
/// (native_sidebar/links_list.rs) does.
const REMOVE_ARMED_FOR: std::time::Duration = std::time::Duration::from_secs(3);

impl NativeChatView {
    /// The panel's name from the core: "Work" over the coordinator's work items, "Board" from a
    /// gxserver that sends only the board.
    fn board_title(&self) -> String {
        match self.snapshot["coordinatorBoard"]["title"].as_str() {
            Some(title) if !title.is_empty() => title.to_string(),
            _ => "Board".to_string(),
        }
    }

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

    /// The dock header's work items button (work_cards.rs) was clicked: in the list view the
    /// docked column comes or goes where there is room, else the floating card opens or shuts; in
    /// the columns view the overlay does.
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
    ///
    /// CDXC:Coordinators 2026-11-05 WHY:
    /// The floating card and the columns are menu windows of their own, so a click on their switch
    /// arrives with that window, which the switch then closes. The next view was opened from it
    /// (its timer, deferral and frame belonged to the closing window), so nothing opened and the
    /// board just shut. The switch therefore always runs from the window the chat is drawn in.
    fn switch_board_view(
        &mut self,
        view: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(main) = self.open_main_window(cx)
            && main != window.window_handle()
        {
            let chat = cx.weak_entity();
            cx.defer(move |cx| {
                let _ = main.update(cx, |_, window, cx| {
                    chat.update(cx, |this, cx| this.switch_board_view(view, window, cx))
                });
            });
            return;
        }
        self.board.view_override = Some(view);
        self.host("setCoordinatorBoardView", json!({ "view": view }), cx);
        if view == COLUMNS_VIEW {
            self.open_board_columns_after_dock(window, cx);
        } else {
            let p = ChatAppearance::current(&self.snapshot);
            if self.board_fits(&p) {
                self.close_pill_menu(cx);
                self.board.open = true;
            } else {
                // The card replaces the columns as one menu replaces another. Closing the columns
                // first handed focus back to the chat, which shut the card that opened after it.
                let pill = self.pill_bounds.cell(BOARD_MENU).get();
                self.toggle_pill_menu(BOARD_MENU, pill, window, cx);
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
    fn board_restore_button(
        &self,
        row: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let s = p.scale;
        let restore = row["restore"].clone();
        if !restore.is_object() {
            return None;
        }
        Some(
            div()
                .id(gpui::SharedString::from(format!(
                    "chat-board-restore:{}",
                    text(row, "key")
                )))
                .role(gpui::Role::Button)
                .aria_label("Restore to the board")
                .flex_shrink_0()
                .px(px(6.0 * s))
                .rounded(px(5.0 * s))
                .text_size(px(CHIP_SIZE * s))
                .text_color(p.card_muted)
                .chat_cursor_pointer()
                .hover(|style| {
                    style
                        .bg(p.foreground.opacity(0.08))
                        .text_color(p.foreground)
                })
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
                .size(px(crate::app::window::round_close_button::ROUND_CLOSE_BUTTON_SIZE * s))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border(px(s))
                // The toast's light-gray × (window/toast.rs): see-through light gray with a darker
                // gray ×, a step stronger on hover; armed, a soft red.
                //
                // CDXC:Coordinators 2026-11-05 DECISION:
                // User, of the solid black circle: "too black the x button please fix!!!! ... i hate
                // black in transparent design looks bad".
                .border_color(if armed {
                    red.opacity(0.45)
                } else {
                    crate::app::window::toast::gpui_app_toast_close_button_border_color(p.light)
                })
                .bg(if armed {
                    gpui::Hsla::from(rgb(0xfde2e2)).opacity(0.88)
                } else {
                    crate::app::window::toast::gpui_app_toast_close_button_color(p.light)
                })
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
                        .when(!armed, |inner| {
                            let light = p.light;
                            inner.hover(move |style| {
                                style.bg(
                                    crate::app::window::toast::gpui_app_toast_close_button_hover_color(
                                        light,
                                    ),
                                )
                            })
                        })
                        .child(
                            crate::app::window::round_close_button::round_close_x_icon(
                                s,
                                if armed {
                                    red
                                } else {
                                    crate::app::window::toast::gpui_app_toast_close_button_icon_color(
                                        p.light,
                                    )
                                },
                            ),
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

    /// The columns collapsed to their strip: the setting, or the switch's last click here. Finished
    /// today starts collapsed.
    fn board_collapsed_columns(&self) -> Vec<String> {
        if let Some(collapsed) = &self.board.collapsed_override {
            return collapsed.clone();
        }
        let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
        match settings
            .object()
            .get(COLLAPSED_SETTING)
            .and_then(Value::as_str)
        {
            Some(list) => list
                .split(',')
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(str::to_string)
                .collect(),
            None => vec!["done".to_string()],
        }
    }

    /// Collapses a column to its strip, or opens it again, and saves the choice.
    fn toggle_board_column(&mut self, group: &str, cx: &mut Context<Self>) {
        let mut collapsed = self.board_collapsed_columns();
        if let Some(at) = collapsed.iter().position(|id| id == group) {
            collapsed.remove(at);
        } else {
            collapsed.push(group.to_string());
        }
        self.host(
            "setCoordinatorBoardCollapsed",
            json!({ "columns": collapsed.join(",") }),
            cx,
        );
        self.board.collapsed_override = Some(collapsed);
        cx.notify();
    }

    /// The columns overlay: the header with its switch, then one column per group in the board's
    /// order, each scrolling on its own, or collapsed to its strip. Empty groups are left out.
    ///
    /// CDXC:Coordinators 2026-11-10 DECISION:
    /// User, of the opened Finished today lane floating over the others: "please make done show as
    /// a column not show floating. also please allow me to collapse not just done, i wanna be able
    /// to collapse other ones too so they appear like narrower and other swimlanes have more
    /// space. if too many open then allow horizontal scroll or grabbing on the bg and dragging to
    /// scroll". Every column collapses to the narrow strip Done had (its glyph and count, a click
    /// opens it) and opens in its place in the row; the open ones share the width, never under
    /// `COLUMN_MIN_WIDTH`, and the row scrolls sideways (trackpad, Shift and the wheel, or a drag
    /// on its background). Which are collapsed is the `coordinatorBoardCollapsedColumns` setting;
    /// Finished today starts collapsed. Supersedes 2026-11-05's Done lane over the right side.
    pub(super) fn board_columns_body(
        &mut self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let board = self.board_value()?.clone();
        let s = p.scale;
        let groups = board["groups"].as_array().cloned().unwrap_or_default();
        let collapsed = self.board_collapsed_columns();
        let is_collapsed = |group: &Value| collapsed.iter().any(|id| group["id"] == id.as_str());
        let strips = groups.iter().filter(|group| is_collapsed(group)).count() as f32;
        let open = groups.len() as f32 - strips;
        // Each open column's width, so its cards fit their chips to it: the overlay, less the
        // strips, shared out, never under `COLUMN_MIN_WIDTH` (the row then scrolls sideways).
        let column = if open > 0.0 {
            ((self.board_columns_size().0
                - 24.0
                - strips * (STRIP_WIDTH + 10.0)
                - 10.0 * (open - 1.0))
                / open)
                .max(COLUMN_MIN_WIDTH)
        } else {
            COLUMN_MIN_WIDTH
        };
        let scroll = self.board.columns_scroll.clone();
        let mut row = div()
            .id("chat-coordinator-board-columns-row")
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .gap(px(10.0 * s))
            .pt(px(6.0 * s))
            .overflow_x_scroll()
            // A wheel turns the row only with Shift (the columns scroll up and down under it).
            .restrict_scroll_to_axis()
            .track_scroll(&scroll);
        for group in &groups {
            row = row.child(if is_collapsed(group) {
                self.board_column_strip(group, p, cx)
            } else {
                self.board_column(group, column, p, cx)
            });
        }
        let wheel_scroll = scroll.clone();
        let down_scroll = scroll.clone();
        let move_scroll = scroll;
        let row = row
            // Shift and a mouse wheel scroll sideways where the system leaves the wheel vertical.
            .on_scroll(move |event: &gpui::ScrollEvent, window, cx| {
                if !event.modifiers.shift {
                    return;
                }
                let delta = event.delta.pixel_delta(window.line_height());
                if delta.x != px(0.0) || delta.y == px(0.0) {
                    return;
                }
                let offset = wheel_scroll.offset();
                let max = wheel_scroll.max_offset().x;
                let x = (offset.x + delta.y).clamp(-max, px(0.0));
                wheel_scroll.set_offset(gpui::point(x, offset.y));
                cx.stop_propagation();
                window.refresh();
            })
            // A drag on the background between cards scrolls the row.
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, _, _| {
                    let pressed_card = std::mem::take(&mut this.board.card_pressed);
                    this.board.pan =
                        (!pressed_card).then(|| (event.position.x, down_scroll.offset().x));
                }),
            )
            .on_mouse_move(
                cx.listener(move |this, event: &gpui::MouseMoveEvent, window, _| {
                    let Some((from, start)) = this.board.pan else {
                        return;
                    };
                    if event.pressed_button != Some(gpui::MouseButton::Left) {
                        this.board.pan = None;
                        return;
                    }
                    let offset = move_scroll.offset();
                    let max = move_scroll.max_offset().x;
                    let x = (start + event.position.x - from).clamp(-max, px(0.0));
                    if x != offset.x {
                        move_scroll.set_offset(gpui::point(x, offset.y));
                        window.refresh();
                    }
                }),
            )
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, _| this.board.pan = None),
            )
            .on_mouse_up_out(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, _| this.board.pan = None),
            );
        Some(
            div()
                .id("chat-coordinator-board-columns")
                .role(gpui::Role::Group)
                .aria_label(self.board_title())
                .size_full()
                .min_h_0()
                .flex()
                .flex_col()
                .child(self.board_header(&board, BoardPlace::Columns, p, cx))
                .child(row)
                .into_any_element(),
        )
    }

    /// A collapsed column: a narrow muted strip with its group's glyph and count; a click opens
    /// it again.
    fn board_column_strip(
        &self,
        group: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let id = text(group, "id");
        let (icon, color) = group_look(&id);
        let icon_color: gpui::Hsla = color.map_or(p.muted, |color| rgb(color).into());
        let count = group["count"].as_u64().unwrap_or(0);
        let label = format!("Show {}", text(group, "title"));
        let tooltip = label.clone();
        let drop_fill = p_drop(p);
        let toggle = id.clone();
        let drop_group = id.clone();
        div()
            .id(gpui::SharedString::from(format!("chat-board-strip:{id}")))
            .role(gpui::Role::Button)
            .aria_label(format!("{label}, {count}"))
            .aria_expanded(false)
            .flex_shrink_0()
            .w(px(STRIP_WIDTH * s))
            .mb(px(8.0 * s))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(8.0 * s))
            .pt(px(12.0 * s))
            .rounded(px(10.0 * s))
            .border(px(s))
            .border_color(p.foreground.opacity(0.08))
            .bg(p.foreground.opacity(0.03))
            .opacity(0.75)
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.foreground.opacity(0.08)).opacity(1.0))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            })
            .child(
                svg()
                    .path(icon)
                    .size(px(14.0 * s))
                    .text_color(icon_color.opacity(0.8)),
            )
            .child(
                div()
                    .text_size(px(CHIP_SIZE * s))
                    .text_color(p.card_muted)
                    .child(count.to_string()),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.toggle_board_column(&toggle, cx);
            }))
            .drag_over::<BoardCardDrag>(move |style, _, _, _| style.bg(drop_fill).opacity(1.0))
            .on_drop(cx.listener(move |this, drag: &BoardCardDrag, _, cx| {
                this.drop_board_card(drag, &drop_group, cx);
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
        let scroll = self
            .board
            .column_scrolls
            .borrow_mut()
            .entry(id.clone())
            .or_default()
            .clone();
        let mut cards = div()
            .id(gpui::SharedString::from(format!(
                "chat-board-column-rows:{id}"
            )))
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .restrict_scroll_to_axis()
            .track_scroll(&scroll)
            .flex()
            .flex_col()
            .gap(px(CARD_GAP * s))
            // Room for the × badge that floats over a card's top-right corner.
            .pt(px(BADGE_ROOM * s))
            .pr(px(BADGE_ROOM * s))
            .pb(px(8.0 * s));
        let inner = width - BADGE_ROOM - 2.0 * CARD_PAD - 2.0;
        for row in &rows {
            cards = cards.child(
                div()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, _| this.board.card_pressed = true),
                    )
                    .child(self.board_card(row, inner, &scroll, &id, p, cx)),
            );
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
        let drop_group = id.clone();
        let toggle = id.clone();
        let drop_fill = p_drop(p);
        div()
            .id(gpui::SharedString::from(format!("chat-board-column:{id}")))
            .flex_shrink_0()
            .w(px(width * s))
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(6.0 * s))
            .rounded(px(10.0 * s))
            .drag_over::<BoardCardDrag>(move |style, _, _, _| style.bg(drop_fill))
            .on_drop(cx.listener(move |this, drag: &BoardCardDrag, _, cx| {
                this.drop_board_card(drag, &drop_group, cx);
            }))
            .child(
                div()
                    .id(gpui::SharedString::from(format!(
                        "chat-board-column-head:{id}"
                    )))
                    .flex_shrink_0()
                    .h(px(GROUP_HEIGHT * s))
                    // The header collapses the column to its strip.
                    .chat_cursor_pointer()
                    .tooltip(|window, cx| {
                        gpui_component::tooltip::Tooltip::new("Collapse").build(window, cx)
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_board_column(&toggle, cx);
                    }))
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
                            .flex_1()
                            .text_color(p.card_muted)
                            .child(group["count"].as_u64().unwrap_or(0).to_string()),
                    )
                    .child(
                        svg()
                            .path("titlebar/chevron-left.svg")
                            .size(px(12.0 * s))
                            .flex_shrink_0()
                            .text_color(p.muted),
                    ),
            )
            .child(cards)
            .into_any_element()
    }

    /// One line of work as a card, the same in the list and the columns. Closed, it is two lines:
    /// its title (with the agent button that opens its thread, and Restore under Removed), then the
    /// work's chips (the user's tag first, then PR, issue, Linear ticket, branch, release) and how
    /// long ago it moved, which gives way to a ⋯ menu button while the card is hovered. Open, it
    /// shows the whole title, its latest line, every chip, and its Open and Mention buttons.
    ///
    /// CDXC:Coordinators 2026-11-05 DECISION:
    /// User, of the first board list: "so crowded man. no need to show first line or the name of the
    /// thread ... make this more focused on the branch/pr/issue/linear so i can click on those ... the
    /// sub thread from ghostex itself shouldnt appear when i click on these ones basically except if
    /// we add a button that shows agent icon then clicking just that should do it. i also wants cards
    /// around each one, and we must have same number of lines for all please ... font size needs to be
    /// same font size as chat too". Supersedes the rows with an id chip, tags and the report's first
    /// line, and the click that opened the thread.
    ///
    /// CDXC:Coordinators 2026-11-06 DECISION:
    /// User, on the card-actions page: a plain click opens the card in place, one at a time ("clicking
    /// on another one should open that one instead ok?"; the core's `toggle`); ⌥-click, the open
    /// card's Mention button and the menu's Mention in chat put it in the chat box (Q1451); the menu
    /// opens on a right-click and from the ⋯ that takes the age's place on hover (Q1454); a card
    /// changes group only by a drag ("we should move with drag drop not picking option in 'move'
    /// menu"), the same in the list and the columns (Q1457). Every closed card keeps its two lines;
    /// only the open one grows. Supersedes the 2026-11-05 click that put the card in the chat box.
    fn board_card(
        &self,
        row: &Value,
        inner_width: f32,
        scroll: &ScrollHandle,
        group: &str,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let key = text(row, "key");
        let focus = self.board.focus.as_ref().filter(|focus| focus.key == key);
        let expanded = row["expanded"] == true;
        let title = text(row, "title");
        let command = row["command"].clone();
        let toggle = row["toggle"].clone();
        let reference = text(row, "reference");
        let clickable = command.is_object() || toggle.is_object();
        let has_menu = row["menu"].as_array().is_some_and(|menu| !menu.is_empty());
        let hover_group = gpui::SharedString::from(format!("chat-board-card-hover:{key}"));
        let age = text(row, "age");
        let age_width = age.chars().count() as f32 * CHIP_CHAR * 0.95 + 4.0;
        let tags = row["tags"].as_array().cloned().unwrap_or_default();
        let answers = row["answers"].as_array().cloned().unwrap_or_default();
        // A thread waiting on a relayed approval shows that card's buttons in place of its chips
        // (relayed_approval.rs; the user's Q1297).
        let relayed = Some(text(&row["openThread"], "sessionId"))
            .filter(|session| !session.is_empty())
            .and_then(|session| self.relayed_card_for_thread(&session));
        // Line 2 closed, or the chip block open: every chip when open (they wrap), else the chips
        // that fit and a "+N" for the rest.
        //
        // CDXC:Coordinators 2026-11-05 DECISION:
        // User: "make the time bottom right in the card (right aligned on 2nd line)".
        let (fitted, hidden) = if !answers.is_empty() || relayed.is_some() {
            (Vec::new(), 0)
        } else if expanded {
            (
                tags.iter()
                    .map(|tag| (tag.clone(), chip_width(tag).min(inner_width)))
                    .collect(),
                0,
            )
        } else {
            fit_chips(&tags, inner_width - age_width - CHIP_GAP)
        };
        let mut chips = div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(CHIP_GAP * s))
            .when(expanded, |chips| chips.flex_wrap())
            .when(!expanded, |chips| chips.overflow_hidden());
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
                    .id(gpui::SharedString::from(format!(
                        "chat-board-more-chips:{key}"
                    )))
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
        // shows the coordinator's note for it, and only a card with nothing at all says so (an open
        // card shows its line of work below the title instead).
        let note = text(row, "note");
        if let Some(relayed) = &relayed {
            chips = chips.child(self.board_relay_buttons(relayed, CHIP_SIZE, p, cx));
        } else if !answers.is_empty() && !expanded {
            // A Work panel ask names the work it belongs to before its answers (gx-chat-core
            // work_panel.rs). Opened, its answers are the dock's buttons below (work_actions.rs).
            // Only where the answers keep their room: a narrow column shows them alone.
            let lead = text(row, "lead");
            if !lead.is_empty() && inner_width >= 260.0 {
                chips = chips.child(
                    // It gives way before the answers do.
                    div()
                        .flex_shrink(8.0)
                        .min_w(px(40.0 * s))
                        .truncate()
                        .text_size(px(CHIP_SIZE * s))
                        .text_color(p.card_muted)
                        .child(lead.clone()),
                );
            }
            // Beside a lead the answers keep their width; alone on a narrow card they give way.
            let answers_pill = self.board_answers(&key, &with_reply(row, &answers), p, cx);
            chips = if !lead.is_empty() && inner_width >= 260.0 {
                chips.child(div().flex_shrink_0().child(answers_pill))
            } else {
                chips.child(answers_pill)
            };
        } else if tags.is_empty() && !expanded {
            chips = chips.child(
                div()
                    .id(gpui::SharedString::from(format!("chat-board-note:{key}")))
                    .min_w_0()
                    .truncate()
                    .text_size(px(CHIP_SIZE * s))
                    .text_color(if note.is_empty() {
                        p.muted
                    } else {
                        p.card_muted
                    })
                    .child(if note.is_empty() {
                        "No links yet".to_string()
                    } else {
                        note.clone()
                    }),
            );
        }
        let open_thread = row["openThread"].clone();
        let agent_icon = text(row, "agentIcon");
        let agent_button = open_thread.is_object().then(|| {
            let open_thread = open_thread.clone();
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
                    this.invoke_board_jump(open_thread.clone(), cx);
                }))
                .into_any_element()
        });
        // The age, and over it while the card is hovered, the ⋯ that opens the card's menu.
        let age_slot = div()
            .relative()
            .flex_shrink_0()
            .min_w(px(24.0 * s))
            .h(px(22.0 * s))
            .flex()
            .items_center()
            .justify_end()
            .child(
                div()
                    .text_size(px(CHIP_SIZE * s))
                    .text_color(p.muted)
                    .when(has_menu, |age| {
                        age.group_hover(hover_group.clone(), |style| style.opacity(0.0))
                    })
                    .child(age),
            )
            .when(has_menu, |slot| {
                slot.child(
                    self.board_menu_button(&key, p, cx)
                        .absolute()
                        .top_0()
                        .right(px(-2.0 * s))
                        .opacity(0.0)
                        .group_hover(hover_group.clone(), |style| style.opacity(1.0)),
                )
            });
        let mut card = div()
            .id(gpui::SharedString::from(format!("chat-board-card:{key}")))
            .group(hover_group.clone())
            .relative()
            .role(gpui::Role::Group)
            .aria_label(title.clone())
            .aria_expanded(expanded)
            .w_full()
            .flex_shrink_0()
            .when(!expanded, |card| {
                card.h(px(CARD_HEIGHT * s)).justify_center()
            })
            .when(expanded, |card| {
                card.min_h(px(CARD_HEIGHT * s)).py(px(10.0 * s))
            })
            .flex()
            .flex_col()
            .gap(px(6.0 * s))
            .px(px(CARD_PAD * s))
            .rounded(px(10.0 * s))
            .border(px(s))
            .border_color(p.foreground.opacity(if expanded { 0.14 } else { 0.08 }))
            .bg(p.foreground.opacity(if expanded { 0.055 } else { 0.035 }))
            .when(clickable, |this| {
                this.chat_cursor_pointer()
                    .hover(|style| style.bg(p.foreground.opacity(0.06)))
            })
            // The card a board pill asked to see: ringed, and its list scrolls to it.
            .when_some(focus, |this, focus| {
                this.border_color(p.accent)
                    .bg(p.accent.opacity(0.08))
                    .child(scroll_into_view(scroll.clone(), focus.tries.clone(), s))
            })
            .child(
                div()
                    .min_h(px(24.0 * s))
                    .flex()
                    .items_center()
                    .gap(px(8.0 * s))
                    .min_w_0()
                    .child(
                        div()
                            .id(gpui::SharedString::from(format!("chat-board-title:{key}")))
                            .flex_1()
                            .min_w_0()
                            .when(!expanded, |title| title.truncate())
                            .text_size(px(TITLE_SIZE * s))
                            .text_color(p.foreground)
                            // No tooltip repeats the card's title and line: a click opens the
                            // card to its whole text.
                            //
                            // CDXC:Coordinators 2026-11-11 DECISION:
                            // User, of the large tooltip over a Work panel card: "dont show this
                            // tooltip since clicking expands it". The chips', buttons' and marks'
                            // own small tooltips stay.
                            // A work ask's title links its references (`#562`) as the dock does.
                            .map(|element| {
                                if row["titleRuns"].is_array() {
                                    element.child(self.ask_title(
                                        &key,
                                        &title,
                                        &row["titleRuns"],
                                        !expanded,
                                        p.foreground,
                                        TITLE_SIZE,
                                        TITLE_SIZE * 1.45,
                                        p,
                                        cx,
                                    ))
                                } else if !text(row, "count").is_empty() {
                                    // Its count follows the page's draft while the card is drawn.
                                    if let Some(page_id) = key.strip_prefix("question:") {
                                        self.refresh_page_counts(page_id, cx);
                                    }
                                    // A page's name, then how much it still asks (work_cards.rs
                                    // `page_count`); the name gives way first.
                                    element.child(
                                        div()
                                            .min_w_0()
                                            .flex()
                                            .gap(px(6.0 * s))
                                            .child(div().min_w_0().truncate().child(title.clone()))
                                            .child(
                                                div()
                                                    .flex_shrink_0()
                                                    .whitespace_nowrap()
                                                    .text_color(p.card_muted)
                                                    .child(text(row, "count")),
                                            ),
                                    )
                                } else {
                                    element.child(title.clone())
                                }
                            }),
                    )
                    .children(agent_button)
                    .children(self.board_restore_button(row, p, cx)),
            );
        if expanded {
            // A work ask's longer explanation under its short title, in the quieter colour, whole
            // (work_cards.rs `ask_markdown`).
            let body = text(row, "body");
            if !body.is_empty() {
                card = card.child(self.ask_markdown(&key, &body, p.card_muted, 13.0, 19.0, p, cx));
            }
            // Its latest line (a thread's report or what it is on, an item's note), whole.
            let line = Some(text(row, "detail"))
                .filter(|line| !line.is_empty())
                .unwrap_or_else(|| note.clone());
            if !line.is_empty() {
                card = card.child(
                    div()
                        .text_size(px(13.0 * s))
                        .line_height(px(19.0 * s))
                        .text_color(p.card_muted)
                        .child(line),
                );
            }
            card = card.child(div().flex().min_w_0().child(chips));
            card = card.child(self.board_card_buttons(row, age_slot, p, cx));
        } else {
            card = card.child(
                div()
                    .h(px(22.0 * s))
                    .flex()
                    .items_center()
                    .gap(px(CHIP_GAP * s))
                    .min_w_0()
                    .child(chips)
                    .child(age_slot),
            );
        }
        let move_to = row["moveTo"].clone();
        let menu_key = key.clone();
        card.children(self.board_remove_badge(row, &hover_group, p, cx))
            .children(self.board_card_menu(row, p, cx))
            .when(clickable, |this| {
                let key = key.clone();
                this.on_click(cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
                    this.close_board_card_menu(cx);
                    if event.modifiers().alt && !reference.is_empty() {
                        this.insert_board_reference(&reference, cx);
                    } else if toggle.is_object() {
                        this.board.focus = this.board.focus.take().filter(|focus| focus.key != key);
                        this.invoke(toggle.clone(), cx);
                    } else {
                        this.invoke_board_jump(command.clone(), cx);
                    }
                }))
            })
            .when(has_menu, |this| {
                this.on_mouse_down(
                    gpui::MouseButton::Right,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.open_board_card_menu(&menu_key, event.position, window, cx);
                    }),
                )
            })
            .when(move_to.is_object(), |this| {
                let drag = BoardCardDrag {
                    title: title.clone(),
                    move_to,
                    from: group.to_string(),
                    scale: s,
                    foreground: p.foreground,
                    surface: p.menu_opaque(),
                };
                this.on_drag(drag, |drag, _, _, cx| {
                    gpui::AppContext::new(cx, |_| drag.clone())
                })
            })
            .into_any_element()
    }

    /// The ⋯ that opens a card's menu where it was clicked.
    fn board_menu_button(
        &self,
        key: &str,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let s = p.scale;
        let key = key.to_string();
        div()
            .id(gpui::SharedString::from(format!(
                "chat-board-menu-button:{key}"
            )))
            .role(gpui::Role::Button)
            .aria_label("More actions")
            .flex_shrink_0()
            .w(px(26.0 * s))
            .h(px(22.0 * s))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0 * s))
            .border(px(s))
            .border_color(p.foreground.opacity(0.1))
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.foreground.opacity(0.08)))
            .child(
                svg()
                    .path("titlebar/dots.svg")
                    .size(px(13.0 * s))
                    .text_color(p.card_muted),
            )
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_board_card_menu(&key, event.position, window, cx);
                }),
            )
    }

    /// An open card's last line: Mention, then its age and ⋯. It has no Open button: the card's link
    /// chip already opens its link and its agent button its thread.
    ///
    /// CDXC:Coordinators 2026-11-06 DECISION:
    /// User, of an opened card showing the "#433" chip and an "Open issue #433" button under it: "I
    /// don't like the #433 pill AND the open issue 433 both being shown when card in board is
    /// expanded. pls make this better, we shouldn't show same button/info twice like this". Each
    /// thing shows once (AGENTS.md "Show each thing once"): the chip is the link's open button and
    /// the agent button the thread's, so the opened card adds only Mention. Open stays in the card's
    /// menu, a surface of its own. Same on the phone. Supersedes the open card's Open button.
    fn board_card_buttons(
        &self,
        row: &Value,
        age_slot: gpui::Div,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let key = text(row, "key");
        let reference = text(row, "reference");
        // An opened card offers what the dock's opened row does, drawn by the same code
        // (work_actions.rs): its answers as the dock's filled buttons, then the actions every item
        // repeats as icon buttons (Reply, Open, Show in chat, Dismiss, Snooze, Mention).
        let answers: Vec<Value> = row["answers"].as_array().cloned().unwrap_or_default();
        let answers = with_reply(row, &answers);
        // Reply is a button among the answers when it shows there, not an icon below them.
        let mut actions = row["actions"].clone();
        if answers.iter().any(|answer| answer["reply"].is_object()) {
            actions["reply"] = Value::Null;
        }
        let icons = self.work_action_icons(&key, &actions, &reference, true, p, cx);
        let mut column = div().w_full().min_w_0().flex().flex_col().gap(px(6.0 * s));
        if !answers.is_empty() {
            column = column.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(6.0 * s))
                    .children(self.work_answer_buttons(&key, &answers, false, p, cx)),
            );
        }
        column
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0 * s))
                    .min_w_0()
                    .children(icons)
                    .child(div().flex_1())
                    .child(age_slot),
            )
            .into_any_element()
    }

    /// Opens a card's menu at `at` (window coordinates of `window`, the window its board is in),
    /// on its first page.
    fn open_board_card_menu(
        &mut self,
        key: &str,
        at: gpui::Point<gpui::Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.board.menu = Some(BoardCardMenu {
            key: key.to_string(),
            at,
            tags: false,
            snooze: false,
            parent: window.window_handle(),
            viewport: window.viewport_size(),
        });
        self.sync_board_card_menu_host(cx);
        cx.notify();
    }

    /// Opens the card's menu as its Snooze lengths alone (the card's Snooze button) at `at`, or
    /// closes it when that is what is open: the card menu's surface, so under window glass it is
    /// the same frosted window, and its frame opens upward where it would pass the window's bottom.
    ///
    /// CDXC:Coordinators 2026-11-10 DECISION:
    /// The user runs the app with window transparency and dislikes dark opaque fills on glass; the
    /// Work panel's Snooze lengths were a small menu of their own in the opaque sidebar menu
    /// colours, cut by the floating Work card's bottom edge. They are now a page of the card's
    /// right-click menu, which already takes the frosted window under glass and flips up.
    pub(super) fn toggle_board_card_snooze(
        &mut self,
        key: &str,
        at: gpui::Point<gpui::Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .board
            .menu
            .as_ref()
            .is_some_and(|menu| menu.key == key && menu.snooze)
        {
            self.close_board_card_menu(cx);
            return;
        }
        self.board.menu = Some(BoardCardMenu {
            key: key.to_string(),
            at,
            tags: false,
            snooze: true,
            parent: window.window_handle(),
            viewport: window.viewport_size(),
        });
        self.sync_board_card_menu_host(cx);
        cx.notify();
    }

    /// Closes the card's menu and its frosted window.
    fn close_board_card_menu(&mut self, cx: &mut Context<Self>) {
        if self.board.menu.take().is_some() {
            crate::app::window::frosted_host::hide_frosted_host(
                crate::app::window::frosted_host::FrostedHostKind::BoardCardMenu,
                cx,
            );
            cx.notify();
        }
    }

    /// Shows the card's menu on its Tags page, or back on its first page.
    fn show_board_card_menu_tags(&mut self, tags: bool, cx: &mut Context<Self>) {
        if let Some(menu) = self.board.menu.as_mut() {
            menu.tags = tags;
        }
        self.sync_board_card_menu_host(cx);
        cx.notify();
    }

    /// The open card's row, wherever its board lists it.
    fn board_menu_row(&self, key: &str) -> Option<Value> {
        let board = self.board_value()?;
        board["groups"]
            .as_array()?
            .iter()
            .flat_map(|group| {
                group["rows"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .chain(group["columnRows"].as_array().into_iter().flatten())
            })
            .find(|row| text(row, "key") == key)
            .cloned()
    }

    /// The menu's frame in its window: its rows' height at the sidebar menu's metrics, opening at
    /// the click and kept inside the window.
    fn board_card_menu_frame(&self, menu: &BoardCardMenu) -> Option<gpui::Bounds<gpui::Pixels>> {
        let row = self.board_menu_row(&menu.key)?;
        let s = ChatAppearance::current(&self.snapshot).scale;
        let rows = if menu.snooze {
            row["actions"]["snooze"].as_array().map_or(0, Vec::len) as f32 * SIDEBAR_MENU_ROW_HEIGHT
        } else if menu.tags {
            let options = self
                .board_value()
                .and_then(|board| board["tagOptions"].as_array().map(Vec::len))
                .unwrap_or(0);
            let untag = usize::from(!text(&row, "tag").is_empty());
            (1 + untag + options) as f32 * SIDEBAR_MENU_ROW_HEIGHT + SIDEBAR_MENU_SEPARATOR_HEIGHT
        } else {
            row["menu"].as_array().map_or(0, Vec::len) as f32 * SIDEBAR_MENU_ROW_HEIGHT
        };
        let width = if menu.snooze {
            BOARD_SNOOZE_MENU_WIDTH
        } else {
            BOARD_MENU_WIDTH
        };
        let size = gpui::size(
            px(width * s),
            px((rows + 2.0 * BOARD_MENU_PADDING) * s + 2.0),
        );
        let margin = px(8.0);
        let x = menu
            .at
            .x
            .min(menu.viewport.width - size.width - margin)
            .max(margin);
        let y = if menu.at.y + size.height + margin > menu.viewport.height {
            (menu.at.y - size.height).max(margin)
        } else {
            menu.at.y
        };
        Some(gpui::Bounds::new(gpui::point(x, y), size))
    }

    /// Under window glass the menu draws in a frosted window of its own over its board's window,
    /// as the sidebar's menu does; otherwise in place (`board_card_menu`).
    fn sync_board_card_menu_host(&mut self, cx: &mut Context<Self>) {
        use crate::app::window::frosted_host::{
            FrostedHostKind, frosted_hosting_active, hide_frosted_host, show_frosted_host,
        };
        let frame = self.board.menu.as_ref().and_then(|menu| {
            Some((
                menu.parent,
                menu.key.clone(),
                self.board_card_menu_frame(menu)?,
            ))
        });
        match frame {
            Some((parent, key, frame)) if frosted_hosting_active() => {
                let view = cx.entity();
                show_frosted_host(
                    FrostedHostKind::BoardCardMenu,
                    parent,
                    frame,
                    None,
                    std::rc::Rc::new(move |_, cx| {
                        view.read(cx)
                            .board_card_menu_panel(&key, true, view.clone(), cx)
                            .unwrap_or_else(|| div().into_any_element())
                    }),
                    None,
                    cx,
                );
            }
            _ => hide_frosted_host(FrostedHostKind::BoardCardMenu, cx),
        }
    }

    /// The card's menu where its card is drawn, while it is open: the panel itself, or under
    /// window glass an invisible stand-in that holds its place and closes it on a click outside
    /// (the frosted window draws it, `sync_board_card_menu_host`).
    fn board_card_menu(
        &self,
        row: &Value,
        _p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self
            .board
            .menu
            .as_ref()
            .filter(|menu| menu.key == text(row, "key"))?;
        let frame = self.board_card_menu_frame(menu)?;
        let hosted = crate::app::window::frosted_host::frosted_hosting_active();
        let content = if hosted {
            div()
                .w(frame.size.width)
                .h(frame.size.height)
                .occlude()
                .into_any_element()
        } else {
            self.board_card_menu_panel(&menu.key, false, cx.entity(), cx)?
        };
        Some(
            gpui::deferred(
                gpui::anchored().position(frame.origin).child(
                    div()
                        .id("chat-board-card-menu-place")
                        .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                            this.close_board_card_menu(cx);
                        }))
                        .child(content),
                ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    /// The card menu's panel, built as the sessions sidebar's right-click menu is
    /// (`native_sidebar/menus.rs`): its surface, outline, ink and hover from the shared popup
    /// menu colours, its 34px rows, 6px inset, 8px corners and 16px icons; frosted (`frosted`) it
    /// takes the frosted menus' surface over its window's blur.
    ///
    /// CDXC:Coordinators 2026-11-06 DECISION:
    /// User, of the board card's right-click / ⋯ menu drawn in the chat's opaque menu tone ("Open
    /// link, Mention in chat, Tags, Mark done, Remove from board" on near-black): "pls make this
    /// context menu look like the one we have in the sessions sidebar. it's so dark now". The card
    /// menu and its Tags page take the sidebar menu's surface, rows, icons, hover and spacing, in the
    /// docked board, the floating board and the columns; under window glass it is a frosted window
    /// of its own (`FrostedHostKind::BoardCardMenu`), as the sidebar's menu is, never a black slab
    /// on the glass. Supersedes the card menu in `menu_opaque`.
    fn board_card_menu_panel(
        &self,
        key: &str,
        frosted: bool,
        view: gpui::Entity<Self>,
        _cx: &gpui::App,
    ) -> Option<AnyElement> {
        use crate::app::helpers::{
            chrome_uses_light_appearance, popup_menu_surface, titlebar_popup_menu_background,
            titlebar_popup_menu_border_color, titlebar_popup_menu_foreground,
            titlebar_popup_menu_hover_color,
        };
        let menu = self.board.menu.as_ref().filter(|menu| menu.key == key)?;
        let row = self.board_menu_row(key)?;
        let s = ChatAppearance::current(&self.snapshot).scale;
        let foreground = titlebar_popup_menu_foreground();
        let hover = titlebar_popup_menu_hover_color();
        let light = chrome_uses_light_appearance();
        // A tag's colour as the sidebar's menu draws it: on a light menu a step darker (the
        // favourite's star stays gold).
        let tint = |hex: &str, icon: &str| -> gpui::Hsla {
            let Some(color) = u32::from_str_radix(hex.trim_start_matches('#'), 16).ok() else {
                return foreground;
            };
            let color = if !light {
                color
            } else if icon == "star-filled" {
                0xf6c944
            } else {
                let channel = |shift: u32| ((color >> shift & 255) as f32 * 0.42).round() as u32;
                channel(16) << 16 | channel(8) << 8 | channel(0)
            };
            rgb(color).into()
        };
        let item =
            |id: String, label: String, icon: Option<(String, gpui::Hsla)>, checked: bool| {
                div()
                    .id(gpui::SharedString::from(id))
                    .role(gpui::Role::MenuItem)
                    .aria_label(label.clone())
                    .w_full()
                    .h(px(SIDEBAR_MENU_ROW_HEIGHT * s))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .px(px(10.0 * s))
                    .gap(px(8.0 * s))
                    .rounded(px(6.0 * s))
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .when_some(icon, |row, (path, color)| {
                        row.child(
                            svg()
                                .path(path)
                                .size(px(16.0 * s))
                                .flex_shrink_0()
                                .text_color(color),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(label),
                    )
                    .when(checked, |row| {
                        row.child(
                            svg()
                                .path("titlebar/check.svg")
                                .size(px(14.0 * s))
                                .flex_shrink_0()
                                .text_color(foreground),
                        )
                    })
            };
        let mut panel = div()
            .id("chat-board-card-menu")
            .role(gpui::Role::Menu)
            .aria_label("Card actions")
            .font_family(crate::ui_fonts::UI_FONT)
            .when(frosted, |panel| panel.size_full())
            .when(!frosted, |panel| {
                panel.w(px(if menu.snooze {
                    BOARD_SNOOZE_MENU_WIDTH
                } else {
                    BOARD_MENU_WIDTH
                } * s))
            })
            .flex()
            .flex_col()
            .p(px(BOARD_MENU_PADDING * s))
            .rounded(px(8.0 * s))
            .border_1()
            .border_color(titlebar_popup_menu_border_color())
            .bg(if frosted {
                popup_menu_surface()
            } else {
                titlebar_popup_menu_background()
            })
            .text_color(foreground)
            .font_weight(gpui::FontWeight::NORMAL)
            .text_size(px(13.0 * s))
            .occlude()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(gpui::MouseButton::Right, |_, _, cx| cx.stop_propagation());
        if menu.snooze {
            for (index, choice) in row["actions"]["snooze"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .enumerate()
            {
                let command = choice["command"].clone();
                let view = view.clone();
                panel = panel.child(
                    item(
                        format!("chat-board-card-snooze:{index}"),
                        text(&choice, "label"),
                        None,
                        false,
                    )
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        let command = command.clone();
                        view.update(cx, |this, cx| {
                            this.close_board_card_menu(cx);
                            this.invoke(command, cx);
                        });
                    }),
                );
            }
        } else if menu.tags {
            let back = view.clone();
            panel = panel.child(
                item(
                    "chat-board-card-menu-back".to_string(),
                    "Tags".to_string(),
                    Some(("titlebar/chevron-left.svg".to_string(), foreground)),
                    false,
                )
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    back.update(cx, |this, cx| this.show_board_card_menu_tags(false, cx));
                }),
            );
            panel = panel.child(
                div()
                    .h(px(SIDEBAR_MENU_SEPARATOR_HEIGHT * s))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .h(px(1.0))
                            .w_full()
                            .bg(titlebar_popup_menu_border_color()),
                    ),
            );
            let current = text(&row, "tag");
            let set_tag = row["setTag"].clone();
            let pick = |command: Value| {
                let view = view.clone();
                move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut gpui::App| {
                    cx.stop_propagation();
                    let command = command.clone();
                    view.update(cx, |this, cx| {
                        this.close_board_card_menu(cx);
                        this.invoke(command, cx);
                    });
                }
            };
            if !current.is_empty() {
                let mut clear = set_tag.clone();
                clear["tag"] = json!("");
                panel = panel.child(
                    item(
                        "chat-board-card-menu-untag".to_string(),
                        "No tag".to_string(),
                        Some(("titlebar/tag-off.svg".to_string(), foreground)),
                        false,
                    )
                    .on_click(pick(clear)),
                );
            }
            let options = self
                .board_value()
                .and_then(|board| board["tagOptions"].as_array().cloned())
                .unwrap_or_default();
            for option in options {
                let value = text(&option, "value");
                let icon = text(&option, "icon");
                let mut command = set_tag.clone();
                command["tag"] = json!(value);
                panel = panel.child(
                    item(
                        format!("chat-board-card-menu-tag:{value}"),
                        text(&option, "label"),
                        Some((
                            format!("titlebar/{icon}.svg"),
                            tint(&text(&option, "color"), &icon),
                        )),
                        value == current,
                    )
                    .on_click(pick(command)),
                );
            }
        } else {
            let agent_icon = text(&row, "agentIcon");
            for entry in row["menu"].as_array().cloned().unwrap_or_default() {
                let label = text(&entry, "label");
                let icon = match text(&entry, "icon").as_str() {
                    "agent" => format!("agent-icons/{agent_icon}.svg"),
                    icon => format!("titlebar/{icon}.svg"),
                };
                let tags = text(&entry, "submenu") == "tags";
                let command = entry["command"].clone();
                let mut row_element = item(
                    format!("chat-board-card-menu:{label}"),
                    label,
                    Some((icon, foreground)),
                    false,
                );
                if tags {
                    row_element = row_element.child(
                        svg()
                            .path("titlebar/chevron-right.svg")
                            .size(px(14.0 * s))
                            .flex_shrink_0()
                            .text_color(foreground),
                    );
                }
                let view = view.clone();
                panel = panel.child(row_element.on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    let command = command.clone();
                    view.update(cx, |this, cx| {
                        if tags {
                            this.show_board_card_menu_tags(true, cx);
                            return;
                        }
                        this.close_board_card_menu(cx);
                        if command["type"] == "insertBoardReference" {
                            this.insert_board_reference(&text(&command, "reference"), cx);
                        } else {
                            this.invoke_board_jump(command.clone(), cx);
                            if command["type"] == "openMarkdownLink" {
                                this.close_pill_menu(cx);
                            }
                        }
                    });
                }));
            }
        }
        Some(panel.into_any_element())
    }

    /// Runs a board click. One that jumps into a chat (a question or page chip, a card's question
    /// or page, a thread row that opens its chat) closes the board first, whichever way it is
    /// showing (docked, the floating card, the columns), so the chat is visible at the target.
    /// Everything that stays on the board (a drag, Done, the ×, a menu) keeps it open.
    ///
    /// CDXC:Coordinators 2026-11-09 DECISION:
    /// User: "when i click in the board to jump to a question please close the board".
    fn invoke_board_jump(&mut self, action: Value, cx: &mut Context<Self>) {
        if board_action_jumps(&action) {
            self.board.open = false;
            self.close_pill_menu(cx);
            cx.notify();
        }
        self.invoke(action, cx);
    }

    /// A card dropped on group `group`: it moves there (onto Done it is marked done, onto Removed
    /// it is removed: the core's `coordinatorBoardChange`).
    fn drop_board_card(&mut self, drag: &BoardCardDrag, group: &str, cx: &mut Context<Self>) {
        if drag.from == group {
            return;
        }
        let mut command = drag.move_to.clone();
        command["group"] = json!(group);
        self.invoke(command, cx);
    }

    /// A new board pill click in `snapshot` (its board's `focus.seq` moved since `before`): the
    /// board opens at that card. A chat seen for the first time leaves an older click alone.
    pub(super) fn adopt_board_focus(&mut self, before: &Value, cx: &mut Context<Self>) {
        let focus = &self.snapshot["coordinatorBoard"]["focus"];
        let (Some(key), Some(seq)) = (focus["key"].as_str(), focus["seq"].as_u64()) else {
            return;
        };
        if !before["coordinatorBoard"].is_object()
            || before["coordinatorBoard"]["focus"]["seq"].as_u64() == Some(seq)
        {
            return;
        }
        self.board.focus = Some(BoardFocus {
            key: key.to_string(),
            seq,
            open: true,
            tries: std::rc::Rc::new(std::cell::Cell::new(4)),
        });
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(FOCUS_RING_FOR).await;
            let _ = this.update(cx, |this, cx| {
                if this
                    .board
                    .focus
                    .as_ref()
                    .is_some_and(|focus| focus.seq == seq)
                {
                    this.board.focus = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Opens the board for a board pill's click, the way the Board pill would, unless it is open.
    fn open_board_for_focus(
        &mut self,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A count on the dock's status line: the list floats over the chat at that group, the
        // transcript staying as it is (the user's "when i click to see what's in progress let's
        // show it floating, let's not expand it in the chat transcript").
        if let Some(at) = self.board.float_at.take() {
            if self.board.open {
                self.board.open = false;
            }
            if !self.chat_menu_is_open(BOARD_TRIGGER) {
                cx.defer_in(window, move |this, window, cx| {
                    this.toggle_pill_menu(BOARD_MENU, at, window, cx);
                });
            }
            return;
        }
        if self.board_view() == COLUMNS_VIEW {
            if !self.chat_menu_is_open(BOARD_COLUMNS_TRIGGER) {
                self.open_board_columns_after_dock(window, cx);
            }
        } else if self.board_fits(p) {
            self.board.open = true;
        } else if !self.chat_menu_is_open(BOARD_TRIGGER) {
            let pill = self.pill_bounds.cell(BOARD_MENU).get();
            cx.defer_in(window, move |this, window, cx| {
                this.toggle_pill_menu(BOARD_MENU, pill, window, cx);
            });
        }
    }

    /// Puts a card's reference in the chat box at the caret (or over the selection) and gives the
    /// box the keyboard, so the user can comment on it; a floating board closes out of the way.
    pub(super) fn insert_board_reference(&mut self, reference: &str, cx: &mut Context<Self>) {
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

    /// A closed card's answers: the dock's filled answer buttons at the card's size
    /// (work_actions.rs `work_compact_answers`), so no outlined segmented pill is left anywhere.
    fn board_answers(
        &self,
        key: &str,
        answers: &[Value],
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.work_compact_answers(key, answers, p, cx)
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
        if let Some(focus) = self.board.focus.as_mut().filter(|focus| focus.open) {
            focus.open = false;
            self.open_board_for_focus(p, window, cx);
        }
        // A card menu outlives neither its card nor its board (its frosted window would stay up).
        let menu_lost = self.board.menu.as_ref().is_some_and(|menu| {
            self.board_menu_row(&menu.key).is_none()
                // The docked board, or the floating board or the columns, which are the chat's
                // menu windows (opened from their switch they carry no menu-toggle record).
                || !(self.board.open || self.option_menu.is_some())
        });
        if menu_lost {
            self.close_board_card_menu(cx);
        }
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
            .aria_label(self.board_title())
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
                    .child(self.board_groups(&board, &self.board.scroll, p, cx)),
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
                .aria_label(self.board_title())
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
                        .track_scroll(&self.board.menu_scroll)
                        // Room for the × badge over a card's top-right corner.
                        .px(px(10.0 * p.scale))
                        .pt(px(4.0 * p.scale))
                        .child(self.board_groups(&board, &self.board.menu_scroll, p, cx)),
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
            (
                "Show as a list beside the chat",
                "titlebar/layout-sidebar-right-expand.svg",
                LIST_VIEW,
            )
        } else {
            (
                "Show as columns",
                "titlebar/layout-columns.svg",
                COLUMNS_VIEW,
            )
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
            .hover(|style| {
                style
                    .bg(p.foreground.opacity(0.06))
                    .text_color(p.foreground)
            })
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
                    .child(self.board_title()),
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
        scroll: &ScrollHandle,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let mut list = div().flex().flex_col().w_full().min_w_0();
        // While a card is dragged, the groups it can land in show even when empty (an empty group
        // is otherwise left out: the 2026-11-05 decision in the core).
        let mut groups = board["groups"].as_array().cloned().unwrap_or_default();
        if cx.has_active_drag() {
            for (index, (id, title)) in DROP_GROUPS.iter().enumerate() {
                if groups.iter().any(|group| group["id"] == *id) {
                    continue;
                }
                let at = groups
                    .iter()
                    .position(|group| {
                        DROP_GROUPS
                            .iter()
                            .position(|(other, _)| group["id"] == *other)
                            .is_none_or(|other| other > index)
                    })
                    .unwrap_or(groups.len());
                groups.insert(
                    at,
                    json!({"id": id, "title": title, "count": 0, "open": false, "rows": []}),
                );
            }
        }
        for group in &groups {
            let id = text(group, "id");
            let open = group["open"] == true;
            let count = group["count"].as_u64().unwrap_or(0);
            let (icon, color) = group_look(&id);
            let icon_color: gpui::Hsla = color.map_or(p.muted, |color| rgb(color).into());
            let toggle = json!({"type": "toggleCoordinatorBoardGroup", "group": id});
            let drop_group = id.clone();
            let drop_fill = p_drop(p);
            let mut section = div()
                .id(gpui::SharedString::from(format!("chat-board-drop:{id}")))
                .flex()
                .flex_col()
                .rounded(px(10.0 * s))
                .drag_over::<BoardCardDrag>(move |style, _, _, _| style.bg(drop_fill))
                .on_drop(cx.listener(move |this, drag: &BoardCardDrag, _, cx| {
                    this.drop_board_card(drag, &drop_group, cx);
                }));
            section = section.child(
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
                cards = cards.child(self.board_card(row, inner, scroll, &id, p, cx));
            }
            list = list.child(section.child(cards));
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
        // The user's own tag: the sidebar's glyph in the tag's colour (`userTag`).
        let user_color = hex_color(&text(tag, "color"));
        let tint = tone_color(&text(tag, "tone"));
        let color: gpui::Hsla = user_color
            .or(tint.map(|color| rgb(color).into()))
            .unwrap_or(p.card_muted);
        let user_icon = Some(text(tag, "icon"))
            .filter(|icon| !icon.is_empty())
            .map(|icon| format!("titlebar/{icon}.svg"));
        let command = tag["command"].clone();
        let clickable = command.is_object();
        let icon: Option<gpui::SharedString> = user_icon
            .map(Into::into)
            .or(chip_icon(&kind).map(Into::into));
        let label = text(tag, "label");
        let tip = match text(tag, "title") {
            title if title.is_empty() => label.clone(),
            title => title,
        };
        div()
            .id(gpui::SharedString::from(format!(
                "chat-board-tag:{kind}:{label}"
            )))
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
            .border_color(
                if (tint.is_some() && kind == "release") || user_color.is_some() {
                    color.opacity(0.45)
                } else {
                    p.foreground.opacity(0.1)
                },
            )
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
                    .hover(|style| {
                        style
                            .bg(p.foreground.opacity(0.08))
                            .text_color(p.foreground)
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.invoke_board_jump(command.clone(), cx);
                    }))
            })
            .into_any_element()
    }
}
