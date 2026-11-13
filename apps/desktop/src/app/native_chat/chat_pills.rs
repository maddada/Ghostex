//! The pills on the working card (working_card.rs) and the floating menu each one opens (Tasks
//! opens the Session details panel instead wherever that panel can show, session_details/). Always
//! leftmost, two icon pills: "Scroll to bottom" and "Previous question". Then Threads, Tasks,
//! Subagents and Background, whose labels, counts and order come from the core (`chatPills`,
//! packages/gx-chat-core/src/extras/chat_pills.rs, where the user's decision lives); this opens the
//! menu, a chat option menu window (`option_menu/pill_menu.rs`) that draws the panel's rows with
//! this view's own row builders (coordinator_threads.rs, agent_tasks.rs, agent_fleet.rs,
//! background_tasks.rs). The same menu window serves the Files and links menu.
//!
//! CDXC:SessionChat 2026-10-18 SEE-ALSO:
//! The phone draws the same pills and menus in apps/mobile/app/src/chat/native/cards/ChatPills.tsx.

use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, Context, FontFeatures, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, StatefulInteractiveElement as _, Styled as _, Window, canvas, div,
    px, rgb, svg,
};
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc};

/// A menu's header line, the gap between its sections and its footer line ("N more"), unscaled.
pub(super) const MENU_HEADER_HEIGHT: f32 = 28.0;
pub(super) const MENU_SECTION_GAP: f32 = 4.0;
pub(super) const MENU_FOOTER_HEIGHT: f32 = 26.0;

/// The amber of the ? pill's corner dot and a running pill's (`corner_dot`).
pub(super) const ATTENTION_COLOR: u32 = 0xf5a524;

/// The sidebar's colour for what a pill's menu waits on (gx-chat-core `ChatPill::waiting`): its red
/// approval dot or its pink question dot (native_sidebar/status.rs), so the two places cannot drift.
pub(super) fn waiting_color(waiting: &str) -> Option<gpui::Hsla> {
    match waiting {
        "approval" => Some(rgb(crate::app::native_sidebar::status::APPROVAL_COLOR).into()),
        "question" => Some(rgb(crate::app::native_sidebar::status::QUESTION_COLOR).into()),
        _ => None,
    }
}

/// Each pill's frame at its last paint, which its menu opens against.
#[derive(Default)]
pub(super) struct ChatPillBounds {
    threads: Rc<Cell<Bounds<Pixels>>>,
    tasks: Rc<Cell<Bounds<Pixels>>>,
    subagents: Rc<Cell<Bounds<Pixels>>>,
    background: Rc<Cell<Bounds<Pixels>>>,
    side_chats: Rc<Cell<Bounds<Pixels>>>,
    /// The Board pill (coordinator_board.rs), which its floating card opens against.
    board: Rc<Cell<Bounds<Pixels>>>,
    /// The Resources pill, which the question list (questions_list.rs) opens against.
    resources: Rc<Cell<Bounds<Pixels>>>,
    /// The Links pill (links_pill.rs), which the session's links list lines up with.
    pub(super) links: Rc<Cell<Bounds<Pixels>>>,
    /// The "+N" pill (strip_overflow.rs), which its list of the pills the row has no room for opens against.
    overflow: Rc<Cell<Bounds<Pixels>>>,
    /// The Saved draft pill (saved_draft_pill.rs), which its menu opens against.
    pub(super) saved_draft: Rc<Cell<Bounds<Pixels>>>,
    /// The Approvals pill (relayed_approval.rs), which its menu of thread cards opens against.
    approvals: Rc<Cell<Bounds<Pixels>>>,
    /// The coordinator tips' pill (coordinator_intro.rs).
    intro: Rc<Cell<Bounds<Pixels>>>,
}

impl ChatPillBounds {
    pub(super) fn cell(&self, id: &str) -> Rc<Cell<Bounds<Pixels>>> {
        match id {
            "threads" => self.threads.clone(),
            "tasks" => self.tasks.clone(),
            "background" => self.background.clone(),
            SIDE_CHATS_PILL => self.side_chats.clone(),
            super::coordinator_board::BOARD_MENU => self.board.clone(),
            super::questions_list::QUESTIONS_MENU => self.resources.clone(),
            super::strip_overflow::OVERFLOW_MENU => self.overflow.clone(),
            super::saved_draft_pill::SAVED_DRAFT_MENU => self.saved_draft.clone(),
            super::relayed_approval::APPROVALS_MENU => self.approvals.clone(),
            super::coordinator_intro::INTRO_MENU => self.intro.clone(),
            _ => self.subagents.clone(),
        }
    }
}

/// A status dot's colour (gx-chat-core `PillDot::tone`): the sidebar's question pink, its approval
/// red (also Blocked), and its working amber.
pub(super) fn pill_dot_color(tone: &str) -> gpui::Hsla {
    rgb(match tone {
        "question" => crate::app::native_sidebar::status::QUESTION_COLOR,
        "approval" | "blocked" => crate::app::native_sidebar::status::APPROVAL_COLOR,
        _ => crate::app::native_sidebar::status::WORKING_COLOR,
    })
    .into()
}

/// A pill's status, inside it after its label: each tone's dot followed by its count, in tabular
/// figures so the pill keeps its width as counts change (gx-chat-core extras/chat_pills.rs
/// `pill_dots`, where the user's decision lives). No pill draws a corner dot.
pub(super) fn inline_dots(dots: &[(gpui::Hsla, String)], p: &ChatAppearance) -> gpui::Div {
    let s = p.scale;
    let mut row = div().flex().items_center().gap(px(5.0 * s));
    for (color, count) in dots {
        row = row.child(
            div()
                .flex()
                .items_center()
                .gap(px(3.0 * s))
                .child(
                    div()
                        .size(px(7.0 * s))
                        .rounded_full()
                        .flex_shrink_0()
                        .bg(*color),
                )
                .child(
                    div()
                        .text_color(p.foreground)
                        .font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
                        .child(count.clone()),
                ),
        );
    }
    row
}

/// The coordinator model warning (gx-chat-core extras/coordinator_quality.rs), drawn in the
/// attention amber; its click opens its own rows, like a switch pill's.
const QUALITY_PILL: &str = ghostex_gx_chat_core::extras::coordinator_quality::QUALITY_PILL;

/// Whether a click on `pill` opens its own `menu` rows: a switch pill (Autopilot) or the model
/// warning.
pub(super) fn opens_row_menu(pill: &Value) -> bool {
    pill["switch"].as_bool().is_some()
        || pill["menu"].as_array().is_some_and(|rows| !rows.is_empty())
}

/// The dot of a switch pill that is on (Autopilot): green, apart from the status dots' tones.
pub(super) const AUTOPILOT_ON_COLOR: u32 = 0x22c55e;

/// The Autopilot pill's menu-toggle key, and its menu's width (unscaled): a small row menu in the
/// sidebar menu's style (`autopilot_menu`).
const AUTOPILOT_TRIGGER: &str = "chat-pill-autopilot";
const AUTOPILOT_MENU_WIDTH: f32 = 280.0;

/// The Stashed pill's id (gx-chat-core extras/chat_pills.rs `stashed_pill`): it opens Saved Prompts
/// rather than a menu of its own.
const STASHED_PILL: &str = "stashed";

/// The Side chats pill's id (gx-chat-core extras/side_chats.rs); its menu is side_chats.rs.
const SIDE_CHATS_PILL: &str = ghostex_gx_chat_core::extras::side_chats::SIDE_CHATS_PILL;

/// The menu-toggle key of a pill (menu_toggle.rs).
fn trigger_id(id: &str) -> &'static str {
    match id {
        "threads" => "chat-pill-threads",
        "tasks" => "chat-pill-tasks",
        "background" => "chat-pill-background",
        SIDE_CHATS_PILL => "chat-pill-side-chats",
        super::coordinator_board::BOARD_MENU => super::coordinator_board::BOARD_TRIGGER,
        // The Resources pill opens the question list.
        super::questions_list::QUESTIONS_MENU => super::questions_list::QUESTIONS_TRIGGER,
        super::resource_modal::RESOURCE_MODAL_MENU => super::resource_modal::RESOURCE_MODAL_TRIGGER,
        super::strip_overflow::OVERFLOW_MENU => super::strip_overflow::OVERFLOW_TRIGGER,
        super::saved_draft_pill::SAVED_DRAFT_MENU => super::saved_draft_pill::SAVED_DRAFT_TRIGGER,
        super::relayed_approval::APPROVALS_MENU => super::relayed_approval::APPROVALS_TRIGGER,
        super::coordinator_intro::INTRO_MENU => super::coordinator_intro::INTRO_TRIGGER,
        ghostex_gx_chat_core::extras::autopilot::AUTOPILOT_PILL => AUTOPILOT_TRIGGER,
        _ => "chat-pill-subagents",
    }
}

pub(super) fn pill_icon(id: &str) -> &'static str {
    if super::minimized_pages::page_pill_key(id).is_some() {
        return super::minimized_pages::PAGE_ICON;
    }
    match id {
        "threads" => "titlebar/users-group.svg",
        super::coordinator_board::BOARD_MENU => "titlebar/layout-board-split.svg",
        ghostex_gx_chat_core::extras::coordinator_board::BLOCKED_PILL => {
            "titlebar/circle-minus-filled.svg"
        }
        "resources" => "titlebar/stack.svg",
        // The More actions row's own icon for Stash prompt (actions.rs).
        STASHED_PILL => "titlebar/stack-push.svg",
        "tasks" => "titlebar/list-check.svg",
        "background" => "titlebar/terminal-2.svg",
        SIDE_CHATS_PILL => "titlebar/messages.svg",
        super::relayed_approval::APPROVALS_MENU => "titlebar/shield-check.svg",
        super::coordinator_intro::INTRO_MENU => "titlebar/bulb.svg",
        ghostex_gx_chat_core::extras::autopilot::AUTOPILOT_PILL => "titlebar/plane.svg",
        QUALITY_PILL => "titlebar/alert-triangle.svg",
        _ => "titlebar/users.svg",
    }
}

fn menu_width(id: &str) -> f32 {
    match id {
        "tasks" => 420.0,
        super::coordinator_board::BOARD_MENU => super::coordinator_board::BOARD_WIDTH,
        super::strip_overflow::OVERFLOW_MENU => super::strip_overflow::OVERFLOW_MENU_WIDTH,
        super::questions_list::QUESTIONS_MENU => super::questions_list::QUESTIONS_MENU_WIDTH,
        super::resource_modal::RESOURCE_MODAL_MENU => super::resource_modal::RESOURCE_MODAL_WIDTH,
        super::saved_draft_pill::SAVED_DRAFT_MENU => {
            super::saved_draft_pill::SAVED_DRAFT_MENU_WIDTH
        }
        super::relayed_approval::APPROVALS_MENU => super::relayed_approval::APPROVALS_MENU_WIDTH,
        super::coordinator_intro::INTRO_MENU => super::coordinator_intro::INTRO_MENU_WIDTH,
        _ => 520.0,
    }
}

/// The menu's content height (unscaled) for `id`, or `None` when its panel is gone.
fn panel_menu_height(id: &str, snapshot: &Value) -> Option<f32> {
    let (key, height): (&str, fn(&Value) -> f32) = match id {
        "threads" => (
            "coordinatorThreadsPanel",
            super::coordinator_threads::threads_menu_height,
        ),
        "tasks" => ("agentTasksPanel", super::agent_tasks::tasks_menu_height),
        "subagents" => ("agentFleetStrip", super::agent_fleet::fleet_menu_height),
        "background" => (
            "backgroundTasksPanel",
            super::background_tasks::background_menu_height,
        ),
        SIDE_CHATS_PILL => ("sideChatsPanel", super::side_chats::side_chats_menu_height),
        _ => return None,
    };
    let panel = &snapshot[key];
    panel.is_object().then(|| height(panel))
}

/// A menu's first line: the panel's icon and title, its muted detail, and an optional ornament.
///
/// CDXC:SessionChat 2026-10-11 DECISION: User: the muted detail ("3 working · 207 threads") is the same size as the title beside it, still muted, on every panel that shares this header. (Moved here from the deleted panel_card.rs when the panels became pill menus, 2026-10-15.)
pub(super) fn pill_menu_header(
    icon: &'static str,
    title: &'static str,
    meta: String,
    trailing: Option<AnyElement>,
    p: &ChatAppearance,
) -> AnyElement {
    let s = p.scale;
    div()
        .flex_shrink_0()
        .h(px(MENU_HEADER_HEIGHT * s))
        .flex()
        .items_center()
        .gap(px(8.0 * s))
        .px(px(4.0 * s))
        .text_size(px(13.0 * s))
        .child(
            svg()
                .path(icon)
                .size(px(14.0 * s))
                .text_color(p.muted)
                .flex_shrink_0(),
        )
        .child(
            div()
                .flex_shrink_0()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(p.foreground)
                .child(title),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(p.card_muted)
                .child(meta),
        )
        .children(trailing)
        .into_any_element()
}

/// A menu's column: header, list and footer, `MENU_SECTION_GAP` apart.
pub(super) fn pill_menu_column(
    id: &'static str,
    label: &'static str,
    header: AnyElement,
    list: Option<AnyElement>,
    footer: Option<AnyElement>,
    p: &ChatAppearance,
) -> AnyElement {
    div()
        .id(id)
        .role(gpui::Role::Group)
        .aria_label(label)
        .w_full()
        // Fills the menu's fixed height and lets a list that marks itself `flex_1().min_h_0()`
        // shrink and scroll when the content outgrows the window.
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .gap(px(MENU_SECTION_GAP * p.scale))
        .child(header)
        .children(list)
        .children(footer)
        .into_any_element()
}

/// An icon pill's glyph.
fn pill_glyph(icon: &'static str, color: gpui::Hsla, s: f32) -> AnyElement {
    svg()
        .path(icon)
        .size(px(16.0 * s))
        .text_color(color)
        .into_any_element()
}

/// An icon-only pill, the label pills' height: built the way they are (a 16px line, 2px above and
/// below, a 1px edge), so the two never drift apart under a zoom. Its width is fixed, so the ? pill
/// swapping its glyph for a count never moves the pills beside it.
fn icon_pill(
    id: &'static str,
    content: AnyElement,
    p: &ChatAppearance,
) -> gpui::Stateful<gpui::Div> {
    let s = p.scale;
    div()
        .id(id)
        .tab_index(0)
        .role(gpui::Role::Button)
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .w(px(32.0 * s))
        .py(px(2.0 * s))
        .rounded_full()
        .border(px(s))
        .chat_cursor_pointer()
        .focus_visible(|style| style.border_color(p.ring))
        .child(div().h(px(19.0 * s)).flex().items_center().child(content))
}

impl NativeChatView {
    /// "Previous question" as a ? pill in the Threads pill's neutral look with its amber attention
    /// dot, while the chat has unanswered questions; a click jumps to the previous one
    /// (open_questions.rs), and the dot pings now and then (question_ping.rs).
    ///
    /// CDXC:SessionChat 2026-10-17 DECISION:
    /// User (2026-10-15): "make the Question pill and scroll to bottom pill not floating, make them appear as the leftmost in the pills row and they should both just be icons. (down arrow for scroll to bottom and ? icon for the question button) always leftmost pls", then "make the scroll to bottom appear left most so it doesn't push stuff when it appears", and "I also want the previous question pill to have same height as the threads and scroll to bottom pills (and same color)". So both are icon pills at the left end of the working row's pills, scroll-to-bottom first, built like the label pills, with the ? pill in the Threads pill's edge, icon colour and hover. Then (2026-10-17): "make the Previous question button animate every once in a while to indicate to the user that there's something there", picking the attention dot ping (Q236 G) every 30 s (Q237): the pill keeps that neutral look and adds the Threads pill's amber attention dot, which pings (question_ping.rs, where the cadence lives). Supersedes the 2026-10-15 pill with no dot; the floating spot over the transcript's bottom edge (2026-09-23, 2026-10-14) stays gone, the Escape and "Agent was interrupted" toasts keep it (scroll_bottom.rs). The pills now ride on the working card (working_card.rs, "on the pill", Q259). The phone does the same in ChatPills.tsx.
    ///
    /// CDXC:SessionChat 2026-10-24 DECISION:
    /// User: "for questions if there's 2 or more questions i want to go between showing ? and showing the number pls (every x seconds)". With two or more unanswered questions the pill shows the ? for 2 s, then the count for 2 s, and so on (`countSwapMs` in packages/gx-chat-core/visual/scroll-bottom.json); with one it stays a ?. The swap is a plain cut, not a crossfade, because a GPUI fade redraws the whole chat for every frame of it. One timer per chat drives it (question_ping.rs), dropped when fewer than two wait, and the pill's fixed width keeps the row still. The phone does the same in ChatPills.tsx.
    ///
    /// CDXC:SessionChat 2026-10-28 DECISION:
    /// User: "let's not show the ? pill here unless there's an unanswered question (that i didn't set to 'for later')". The pill shows only while a question form or page is unanswered and not put off For later (`openQuestions.count`); answered, withdrawn and For later ones are reached from the Resources pill (questions_list.rs). A click steps only through the waiting ones. The user's words are on gx-chat-core's questions/open_questions.rs. Supersedes the 2026-10-27 pill that stayed while the chat had any question.
    pub(super) fn previous_question_icon_pill(
        &self,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let open = &self.snapshot["openQuestions"];
        self.open_question_pill_text()?;
        let s = p.scale;
        let label = text(open, "accessibilityLabel");
        let count = open["pillCount"].as_u64().unwrap_or(0);
        // The ? and, while questions wait, the pink dot and their count inside the pill, as every
        // pill flags (`inline_dots`).
        Some(
            div()
                .id("chat-previous-question")
                .tab_index(0)
                .role(gpui::Role::Button)
                .aria_label(label)
                .flex()
                .items_center()
                .flex_shrink_0()
                .gap(px(5.0 * s))
                .px(px(7.0 * s))
                .py(px(2.0 * s))
                .rounded_full()
                .border(px(s))
                .border_color(p.foreground.opacity(0.1))
                .text_size(px(13.0 * s))
                .line_height(px(19.0 * s))
                .whitespace_nowrap()
                .chat_cursor_pointer()
                .focus_visible(|style| style.border_color(p.ring))
                .hover(|style| style.bg(p.foreground.opacity(0.06)))
                .child(
                    div()
                        .h(px(19.0 * s))
                        .flex()
                        .items_center()
                        .child(pill_glyph("titlebar/question-mark.svg", p.card_muted, s)),
                )
                .when(count > 0, |pill| {
                    pill.child(inline_dots(
                        &[(pill_dot_color("question"), count.to_string())],
                        p,
                    ))
                })
                .on_click(cx.listener(|chat, _, _, cx| {
                    cx.stop_propagation();
                    chat.previous_question(cx)
                }))
                .into_any_element(),
        )
    }

    /// "Scroll to bottom" as a down-arrow pill, while the reader is away from the bottom; its tooltip
    /// names it and its shortcut (scroll_bottom.rs).
    ///
    /// CDXC:SessionChat 2026-10-17 DECISION:
    /// User: "make the scroll to bottom pill always appear on the leftmost so it doesn't push other pills when it appears". On the working card it takes the leftmost place among the card's pills and timers, which are laid out from the card's right edge, so appearing moves none of them; the label gives up the room. With no card to carry it, it stands alone over the transcript's bottom edge at the rows' right end (working_card.rs, `render_lone_scroll_bottom`).
    pub(super) fn scroll_bottom_icon_pill(
        &self,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        if !super::scroll_bottom::transcript_left_bottom(&self.list) {
            return None;
        }
        let label = super::scroll_bottom::label().to_string();
        let tooltip = label.clone();
        let shortcut = super::scroll_bottom::shortcut_label().unwrap_or_default();
        Some(
            icon_pill(
                "chat-scroll-bottom",
                pill_glyph("titlebar/arrow-down.svg", p.primary, p.scale),
                p,
            )
            .aria_label(label)
            .border_color(p.foreground.opacity(0.1))
            .hover(|style| style.bg(p.foreground.opacity(0.06)))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip.clone())
                    .keys(shortcut.clone())
                    .build(window, cx)
            })
            .on_click(cx.listener(|chat, _, _, cx| {
                cx.stop_propagation();
                chat.jump_to_bottom(cx)
            }))
            .into_any_element(),
        )
    }

    /// What a click on the panel pill `id` does, painted at `bounds`; the "+N" pill's list runs the
    /// same for a pill that does not fit the row.
    pub(super) fn activate_chat_pill(
        &mut self,
        id: &str,
        switch: bool,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A hidden page's pill, from the "+N" pill's list, brings the page back.
        if let Some(key) = super::minimized_pages::page_pill_key(id) {
            self.host("showMinimizedPage", json!({ "key": key }), cx);
            return;
        }
        let trigger = trigger_id(id);
        if switch {
            self.open_autopilot_menu(id, None, window, cx);
            return;
        }
        if id == "tasks" && self.show_session_details_for_tasks(cx) {
            return;
        }
        if id == STASHED_PILL {
            // The list the Stash prompt row opens from an empty chat box (toolbar.rs).
            self.host("stashedPrompts", json!({}), cx);
            return;
        }
        if id == super::coordinator_board::BOARD_MENU {
            self.toggle_coordinator_board(bounds, window, cx);
            return;
        }
        // Blocked opens the board at its Blocked group (the core's `show_group`).
        if id == ghostex_gx_chat_core::extras::coordinator_board::BLOCKED_PILL {
            self.invoke(
                json!({"type": "showCoordinatorBoardGroup", "group": "blocked"}),
                cx,
            );
            return;
        }
        self.toggle_pill_menu(id, bounds, window, cx);
        if id == super::questions_list::QUESTIONS_MENU && self.chat_menu_is_open(trigger) {
            self.keep_question_ages_current(cx);
        }
    }

    /// One panel pill in the Worked row's chip look (completed_work_row.rs): icon, label, count, and
    /// the amber dot when something in it needs the user. A click opens its menu.
    ///
    /// `collapsed` drops the label when the row has no room for it (pills_strip.rs): the icon and
    /// the count or dots stay, and the tooltip says what the pill is.
    pub(super) fn chat_pill(
        &self,
        pill: &Value,
        collapsed: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let id = text(pill, "id");
        let trigger = trigger_id(&id);
        let board = id == super::coordinator_board::BOARD_MENU;
        let open = self.chat_menu_is_open(trigger)
            || (board
                && (self.coordinator_board_docked()
                    || self.chat_menu_is_open(super::coordinator_board::BOARD_COLUMNS_TRIGGER)));
        let bounds = self.pill_bounds.cell(&id);
        let measured = bounds.clone();
        // What the pill flags, inside it after its label; with any, they stand in for its count.
        let dots: Vec<(gpui::Hsla, String)> = pill["dots"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|dot| (pill_dot_color(&text(dot, "tone")), text(dot, "count")))
            .collect();
        let count = if dots.is_empty() {
            text(pill, "count")
        } else {
            String::new()
        };
        // The Work pill says its counts in words while it has its label ("Work · 4 need you · 7
        // in progress", gx-chat-core `coordinator_board::pill`); the coloured counts stand in for
        // them once a tight row drops the label.
        let dots = (!dots.is_empty() && (collapsed || !board)).then(|| inline_dots(&dots, p));
        // A pill with no label (Blocked) is drawn like the ? pill: its icon and dot, its word in
        // the tooltip.
        let name = text(pill, "label");
        let label = if collapsed {
            String::new()
        } else {
            name.clone()
        };
        // A switch pill (Autopilot) opens its small menu; its tooltip says where it stands.
        let switch = pill["switch"].as_bool();
        let row_menu = opens_row_menu(pill);
        let warning = id == QUALITY_PILL;
        let tone = if warning {
            rgb(ATTENTION_COLOR).into()
        } else {
            p.card_muted
        };
        let tooltip = if row_menu || (collapsed && !name.is_empty()) {
            Some(text(pill, "ariaLabel"))
        } else {
            Some(text(pill, "meta")).filter(|meta| label.is_empty() && !meta.is_empty())
        };
        div()
            .id(trigger)
            .relative()
            .role(gpui::Role::Button)
            .aria_label(text(pill, "ariaLabel"))
            .aria_expanded(open)
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap(px(5.0 * s))
            .px(px(if label.is_empty() { 7.0 } else { 8.0 } * s))
            .py(px(2.0 * s))
            .rounded_full()
            .border(px(s))
            .border_color(if warning {
                gpui::Hsla::from(rgb(ATTENTION_COLOR)).opacity(0.45)
            } else {
                p.foreground.opacity(0.1)
            })
            .text_size(px(13.0 * s))
            .line_height(px(19.0 * s))
            .whitespace_nowrap()
            .text_color(tone)
            .chat_cursor_pointer()
            .when_some(tooltip, |pill, tooltip| {
                pill.tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
            })
            .when(open, |pill| pill.bg(p.foreground.opacity(0.08)))
            .hover(|style| style.bg(p.foreground.opacity(0.06)))
            .child(
                svg()
                    .path(pill_icon(&id))
                    .size(px(14.0 * s))
                    .flex_shrink_0()
                    .text_color(tone),
            )
            .when(!label.is_empty(), |pill| pill.child(label))
            .when(switch == Some(true), |pill| {
                pill.child(
                    div()
                        .size(px(7.0 * s))
                        .rounded_full()
                        .flex_shrink_0()
                        .bg(rgb(AUTOPILOT_ON_COLOR)),
                )
            })
            .children(dots)
            .when(!count.is_empty(), |pill| {
                pill.child(
                    div()
                        .text_color(p.foreground)
                        .font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
                        .child(count),
                )
            })
            .when(row_menu, |pill| {
                // A right-click opens the same menu, at the pointer.
                let id = id.clone();
                pill.on_mouse_down(
                    gpui::MouseButton::Right,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.open_autopilot_menu(&id, Some(event.position), window, cx);
                    }),
                )
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.activate_chat_pill(&id, row_menu, bounds.get(), window, cx);
            }))
            .child(
                // The pill's own border box, which its menu lines up with
                // (`PillsStripCells::menu_anchor`). An absolute child with no insets sits at the content start (border plus
                // padding in) at the padding box's size, so it measured the pill 7px too far right
                // and 1px too low; insets on the padding box, one border width out, give the border
                // box (the composer measures its card the same way, composer.rs).
                canvas(move |bounds, _, _| measured.set(bounds), |_, _, _, _| {})
                    .absolute()
                    .inset(px(-s)),
            )
            .into_any_element()
    }

    /// The Autopilot pill's menu: the toggle that fits this chat and "Autopilot settings...", the
    /// pill's own `menu` rows (gx-chat-core `extras/autopilot.rs` has the user's words), drawn as
    /// the chat's other row menus are. A click lines it up with the pill above the chat box, a
    /// right-click opens it at the pointer; the pill itself never toggles.
    fn open_autopilot_menu(
        &mut self,
        id: &str,
        at: Option<gpui::Point<Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let trigger = trigger_id(id);
        if self.chat_menu_toggled_shut(trigger, cx) {
            return;
        }
        let rows = self.snapshot["chatPills"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|pill| pill["id"] == id)
            .and_then(|pill| pill["menu"].as_array().cloned())
            .unwrap_or_default();
        match at {
            Some(at) => self.show_chat_menu_at(rows, at, AUTOPILOT_MENU_WIDTH, window, cx),
            None => {
                let (anchor, side) = self
                    .pills_strip
                    .menu_anchor(self.pill_bounds.cell(id).get());
                self.menu_side = Some(side);
                self.show_chat_menu(rows, anchor, AUTOPILOT_MENU_WIDTH, window, cx);
            }
        }
    }

    /// Opens `id`'s menu against its pill, painted at `pill`, or shuts it when it is the one on
    /// screen.
    pub(super) fn toggle_pill_menu(
        &mut self,
        id: &str,
        pill: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let trigger_id = trigger_id(id);
        if self.chat_menu_toggled_shut(trigger_id, cx) {
            return;
        }
        let Some(height) = self.pill_menu_height(id) else {
            return;
        };
        // Upward from the chat box's top edge, lined up with the pill (`PillsStripCells::menu_anchor`
        // has the user's words).
        let (mut trigger, side) = self.pills_strip.menu_anchor(pill);
        // The work items list opens from the dock's header, so it stands above the dock rather
        // than over the button that opened it.
        if id == super::coordinator_board::BOARD_MENU
            && let Some(top) = self.board.dock_top.get()
        {
            trigger.origin.y = top;
        }
        self.menu_side = Some(side);
        self.show_chat_menu(
            vec![json!({"pillMenu": {"id": id, "height": height}})],
            trigger,
            menu_width(id),
            window,
            cx,
        );
        if let Some(menu) = &self.option_menu {
            menu.update(cx, |menu, _| menu.set_pill(trigger_id));
        }
    }

    /// The menu's content height (unscaled) for `id`, or `None` when its panel is gone. The Files
    /// and links menu is capped at 60% of the pane; its list scrolls inside.
    pub(super) fn pill_menu_height(&self, id: &str) -> Option<f32> {
        if id == super::session_details::SESSION_DETAILS_MENU {
            return self.session_details_menu_height();
        }
        if id == super::questions_list::QUESTIONS_MENU {
            return self.questions_menu_height();
        }
        if id == super::coordinator_board::BOARD_MENU {
            return self.coordinator_board_menu_height();
        }
        if id == super::coordinator_board::BOARD_COLUMNS_MENU {
            return self.board_columns_height();
        }
        if id == super::resource_modal::RESOURCE_MODAL_MENU {
            return self.resource_modal_height();
        }
        if id == super::strip_overflow::OVERFLOW_MENU {
            return self.strip_overflow_height();
        }
        if id == super::saved_draft_pill::SAVED_DRAFT_MENU {
            return self.saved_draft_menu_height();
        }
        if id == super::relayed_approval::APPROVALS_MENU {
            return self.approvals_menu_height();
        }
        if id == super::coordinator_intro::INTRO_MENU {
            return self.coordinator_intro_menu_height();
        }
        if id == "filesLinks" {
            let scale = ChatAppearance::current(&self.snapshot).scale;
            let pane = f32::from(self.bounds.get().size.height) / scale;
            return super::files_links::files_links_menu_height(
                &self.snapshot["filesLinks"],
                (pane * 0.6).max(160.0),
            );
        }
        panel_menu_height(id, &self.snapshot)
    }

    /// The body `id`'s menu draws, built with this view's listeners so its rows act on this chat;
    /// `window` is the menu's own.
    pub(super) fn pill_menu_body(
        &mut self,
        id: &str,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        match id {
            "filesLinks" => self.files_links_menu_body(p, window, cx),
            "threads" => self.threads_menu_body(p, cx),
            "tasks" => self.tasks_menu_body(p, cx),
            "subagents" => self.fleet_menu_body(p, cx),
            "background" => self.background_menu_body(p),
            SIDE_CHATS_PILL => self.side_chats_menu_body(p, cx),
            super::questions_list::QUESTIONS_MENU => self.questions_menu_body(p, window, cx),
            super::resource_modal::RESOURCE_MODAL_MENU => self.resource_modal_body(p, window, cx),
            super::strip_overflow::OVERFLOW_MENU => self.strip_overflow_body(p, cx),
            super::saved_draft_pill::SAVED_DRAFT_MENU => self.saved_draft_menu_body(p, cx),
            super::relayed_approval::APPROVALS_MENU => self.approvals_menu_body(p, cx),
            super::coordinator_intro::INTRO_MENU => self.coordinator_intro_menu_body(p, cx),
            super::coordinator_board::BOARD_MENU => self.coordinator_board_menu_body(p, cx),
            super::coordinator_board::BOARD_COLUMNS_MENU => self.board_columns_body(p, cx),
            super::session_details::SESSION_DETAILS_MENU => {
                self.session_details_menu_body(p, window, cx)
            }
            _ => None,
        }
    }

    /// Closes the open pill menu, after a row that leaves the chat (a thread opened).
    pub(super) fn close_pill_menu(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = self.option_menu.clone() {
            menu.update(cx, |menu, cx| menu.close(None, cx));
        }
    }

    /// A pill's menu went away. The Threads list goes back to its first step, as folding the panel
    /// did (CDXC:Coordinators 2026-10-18 in gx-chat-core's coordinator_threads.rs).
    pub(super) fn pill_menu_closed(&mut self, trigger: &'static str, cx: &mut Context<Self>) {
        if trigger == "chat-pill-tasks" {
            // The done list comes back in its short form the next time the menu opens.
            self.invoke(json!({"type":"toggleAgentTasksCompleted","step":0}), cx);
        }
        if trigger == "chat-pill-threads" {
            self.invoke(json!({"type":"toggleCoordinatorThreadsMore","step":0}), cx);
        }
        if trigger == super::files_links::FILES_LINKS_TRIGGER {
            self.files_links_menu_closed(cx);
        }
        if trigger == super::session_details::SESSION_DETAILS_TRIGGER {
            self.session_details_menu_closed(cx);
        }
        if trigger == super::questions_list::QUESTIONS_TRIGGER {
            self.questions_menu_closed(cx);
        }
        if trigger == super::resource_modal::RESOURCE_MODAL_TRIGGER {
            self.resource_modal_closed(cx);
        }
        if trigger == super::coordinator_intro::INTRO_TRIGGER {
            self.coordinator_intro_menu_closed(cx);
        }
    }
}
