//! The pills on the working card (working_card.rs) and the floating menu each one opens. Always
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
use crate::app::helpers::ThrottledAnimationExt as _;
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, Context, FontFeatures, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, StatefulInteractiveElement as _, Styled as _, Window, canvas, div,
    px, rgb, svg,
};
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc, time::Duration};

/// A menu's header line, the gap between its sections and its footer line ("N more"), unscaled.
pub(super) const MENU_HEADER_HEIGHT: f32 = 24.0;
pub(super) const MENU_SECTION_GAP: f32 = 4.0;
pub(super) const MENU_FOOTER_HEIGHT: f32 = 22.0;

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

/// The corner dot's offsets from a pill's padding box: inside its edge, in the top right corner,
/// so the pills' scroller never cuts it.
pub(super) const CORNER_DOT_TOP: f32 = 1.0;
pub(super) const CORNER_DOT_RIGHT: f32 = 2.0;

/// Each pill's frame at its last paint, which its menu opens against.
#[derive(Default)]
pub(super) struct ChatPillBounds {
    threads: Rc<Cell<Bounds<Pixels>>>,
    tasks: Rc<Cell<Bounds<Pixels>>>,
    subagents: Rc<Cell<Bounds<Pixels>>>,
    background: Rc<Cell<Bounds<Pixels>>>,
    /// The Resources pill, which the question list (questions_list.rs) opens against.
    resources: Rc<Cell<Bounds<Pixels>>>,
    /// The Links pill (links_pill.rs), which the session's links list lines up with.
    pub(super) links: Rc<Cell<Bounds<Pixels>>>,
}

impl ChatPillBounds {
    fn cell(&self, id: &str) -> Rc<Cell<Bounds<Pixels>>> {
        match id {
            "threads" => self.threads.clone(),
            "tasks" => self.tasks.clone(),
            "background" => self.background.clone(),
            super::questions_list::QUESTIONS_MENU => self.resources.clone(),
            _ => self.subagents.clone(),
        }
    }
}

/// The amber dot in a pill's top right corner, the one shape for the ? pill's waiting questions
/// (question_ping.rs) and a running pill (`running_dot`), so the two cannot drift apart again.
pub(super) fn corner_dot(s: f32) -> gpui::Div {
    tinted_corner_dot(s, rgb(ATTENTION_COLOR).into())
}

fn tinted_corner_dot(s: f32, color: gpui::Hsla) -> gpui::Div {
    let dot = super::scroll_bottom::question_ping_dot();
    div()
        .absolute()
        .top(px(CORNER_DOT_TOP * s))
        .right(px(CORNER_DOT_RIGHT * s))
        .size(px(dot * s))
        .rounded_full()
        .bg(color)
}

/// A pill's corner (gx-chat-core extras/chat_pills.rs, CDXC:SessionChat 2026-10-27): the pulsing
/// amber dot while something runs, the sidebar's red or pink dot while something waits on the
/// user, and both overlapping with the sidebar's 1px ring (`sized_overlapped_dots`) when both
/// hold, the waiting one in front. `ring` is what the pill sits on.
fn corner_indicator(pill: &Value, s: f32, ring: gpui::Hsla) -> Option<AnyElement> {
    let running = pill["running"] == true;
    let waiting = waiting_color(&text(pill, "waiting"));
    match (running, waiting) {
        (true, None) => Some(running_dot(s)),
        (false, Some(color)) => Some(tinted_corner_dot(s, color).into_any_element()),
        (true, Some(color)) => Some(
            // The ring sits outside each dot, so the stack starts one ring further out and the
            // front dot lands where the single dot does.
            div()
                .absolute()
                .top(px((CORNER_DOT_TOP - 1.0) * s))
                .right(px((CORNER_DOT_RIGHT - 1.0) * s))
                .child(crate::app::native_sidebar::status::sized_overlapped_dots(
                    vec![rgb(ATTENTION_COLOR).into(), color],
                    super::scroll_bottom::question_ping_dot(),
                    s,
                    ring,
                    None,
                ))
                .into_any_element(),
        ),
        (false, None) => None,
    }
}

/// A running pill's corner dot (`running`, extras/chat_pills.rs, where the user's decision lives).
/// It fades on the 1.6 s beat of the Subagents menu's running dot (agent_fleet.rs), and stays still
/// with Reduce motion on.
fn running_dot(s: f32) -> AnyElement {
    let dot = corner_dot(s);
    if crate::app::helpers::gpui_macos_reduce_motion_enabled() {
        return dot.into_any_element();
    }
    dot.with_throttled_animation(
        "chat-pill-running-dot",
        Duration::from_millis(1600),
        |dot, phase| {
            let progress = if phase < 0.5 {
                phase * 2.0
            } else {
                (1.0 - phase) * 2.0
            };
            dot.opacity(1.0 - 0.65 * progress)
        },
    )
    .into_any_element()
}

/// The menu-toggle key of a pill (menu_toggle.rs).
fn trigger_id(id: &str) -> &'static str {
    match id {
        "threads" => "chat-pill-threads",
        "tasks" => "chat-pill-tasks",
        "background" => "chat-pill-background",
        // The Resources pill opens the question list.
        super::questions_list::QUESTIONS_MENU => super::questions_list::QUESTIONS_TRIGGER,
        super::resource_modal::RESOURCE_MODAL_MENU => super::resource_modal::RESOURCE_MODAL_TRIGGER,
        _ => "chat-pill-subagents",
    }
}

fn icon(id: &str) -> &'static str {
    match id {
        "threads" => "titlebar/users-group.svg",
        "resources" => "titlebar/stack.svg",
        "tasks" => "titlebar/list-check.svg",
        "background" => "titlebar/terminal-2.svg",
        _ => "titlebar/users.svg",
    }
}

fn menu_width(id: &str) -> f32 {
    match id {
        "tasks" => 360.0,
        super::questions_list::QUESTIONS_MENU => super::questions_list::QUESTIONS_MENU_WIDTH,
        super::resource_modal::RESOURCE_MODAL_MENU => super::resource_modal::RESOURCE_MODAL_WIDTH,
        _ => 440.0,
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
        .child(div().flex_shrink_0().text_color(p.foreground).child(title))
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
        .size(px(14.0 * s))
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
        .w(px(28.0 * s))
        .py(px(2.0 * s))
        .rounded_full()
        .border(px(s))
        .chat_cursor_pointer()
        .focus_visible(|style| style.border_color(p.ring))
        .child(div().h(px(16.0 * s)).flex().items_center().child(content))
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
        let color = p.card_muted;
        let label = text(open, "accessibilityLabel");
        let content = if self.question_ping.showing_count() {
            div()
                .text_size(px(11.5 * p.scale))
                .line_height(px(16.0 * p.scale))
                .font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
                .text_color(color)
                .child(open["count"].as_u64().unwrap_or(0).to_string())
                .into_any_element()
        } else {
            pill_glyph("titlebar/question-mark.svg", color, p.scale)
        };
        Some(
            icon_pill("chat-previous-question", content, p)
                .relative()
                .aria_label(label)
                .border_color(p.foreground.opacity(0.1))
                .hover(|style| style.bg(p.foreground.opacity(0.06)))
                .on_hover(cx.listener(|chat, hovered: &bool, _, cx| {
                    chat.question_pill_hovered(*hovered, cx)
                }))
                .on_click(cx.listener(|chat, _, _, cx| {
                    cx.stop_propagation();
                    chat.previous_question(cx)
                }))
                .children(super::question_ping::attention_dot(
                    self.question_ping_frame(),
                    p,
                ))
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

    /// One panel pill in the Worked row's chip look (completed_work_row.rs): icon, label, count, and
    /// the amber dot when something in it needs the user. A click opens its menu.
    pub(super) fn chat_pill(
        &self,
        pill: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let id = text(pill, "id");
        let trigger = trigger_id(&id);
        let open = self.chat_menu_is_open(trigger);
        let bounds = self.pill_bounds.cell(&id);
        let measured = bounds.clone();
        let count = text(pill, "count");
        // Two overlapping corner dots reach half a dot further in than one, so the pill makes that
        // room on its right instead of drawing over its count.
        let stacked = pill["running"] == true && waiting_color(&text(pill, "waiting")).is_some();
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
            .px(px(8.0 * s))
            .when(stacked, |pill| {
                pill.pr(px(
                    (8.0 + super::scroll_bottom::question_ping_dot() / 2.0) * s
                ))
            })
            .py(px(2.0 * s))
            .rounded_full()
            .border(px(s))
            .border_color(p.foreground.opacity(0.1))
            .text_size(px(11.5 * s))
            .line_height(px(16.0 * s))
            .whitespace_nowrap()
            .text_color(p.card_muted)
            .chat_cursor_pointer()
            .when(open, |pill| pill.bg(p.foreground.opacity(0.08)))
            .hover(|style| style.bg(p.foreground.opacity(0.06)))
            .children(corner_indicator(pill, s, p.composer_background))
            .child(
                svg()
                    .path(icon(&id))
                    .size(px(12.0 * s))
                    .flex_shrink_0()
                    .text_color(p.card_muted),
            )
            .child(text(pill, "label"))
            .when(!count.is_empty(), |pill| {
                pill.child(
                    div()
                        .text_color(p.foreground)
                        .font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
                        .child(count),
                )
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.toggle_pill_menu(&id, bounds.get(), window, cx);
                if id == super::questions_list::QUESTIONS_MENU && this.chat_menu_is_open(trigger) {
                    this.keep_question_ages_current(cx);
                }
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
        let (trigger, side) = self.pills_strip.menu_anchor(pill);
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
        if id == super::resource_modal::RESOURCE_MODAL_MENU {
            return self.resource_modal_height();
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
            super::questions_list::QUESTIONS_MENU => self.questions_menu_body(p, window, cx),
            super::resource_modal::RESOURCE_MODAL_MENU => self.resource_modal_body(p, window, cx),
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
    }
}
