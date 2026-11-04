//! The pill strip along the chat box's top edge (composer.rs): a faintly tinted band inside the box,
//! with a hairline under it and the box's top corners, holding the chat's pills on the right: the
//! armed timers, the ? (a question waits), Resources, Workers, Links, Tasks, Subagents and
//! Background, and the ↓ jump-to-bottom at the left of them while the strip shows. Pills that don't
//! fit scroll sideways and fade at the edge that hides more. With no pill there is no strip; it grows
//! in and shrinks away with the chat box's own ease. Each pill opens its menu above the box, lined up
//! with the pill.
//!
//! The strip's left holds the view status chip ("Summary view" or "Detailed view"), pinned there
//! while the pills scroll (gx-chat-core extras/chat_pills.rs `view_chips` has the user's words).
//! Before it, while a draft saved elsewhere is offered, the Saved draft pill (saved_draft_pill.rs),
//! which alone is enough to show the strip, also on a chat with no rows yet.
//!
//! CDXC:SessionChat 2026-10-29 SEE-ALSO:
//! The user's words (option E) are on packages/gx-chat-core/src/extras/chat_pills.rs. The phone draws the same strip in apps/mobile/app/src/chat/native/cards/PillsStrip.tsx.

use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    Animation, AnimationExt as _, AnyElement, Bounds, Context, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, ScrollHandle, StatefulInteractiveElement as _,
    Styled as _, canvas, div, point, px, relative, svg,
};
use serde_json::Value;
use std::{cell::Cell, rc::Rc, time::Duration};
use web_time::Instant;

/// The strip's height (unscaled): a pill's 22px with 5px above and below, and its 1px hairline.
const STRIP_HEIGHT: f32 = 22.0 + 2.0 * 5.0;

/// The width of the fade at an edge that hides pills (unscaled).
pub(super) const PILLS_FADE: f32 = 24.0;

/// The band's tint over the box's fill.
const STRIP_TINT: f32 = 0.035;

/// The ramp over a sideways scroller's edge that hides more: `edge`'s colour at the edge, clear
/// `width` in. The read-picture strips fade the same way (read_pictures.rs).
pub(super) fn edge_fade(toward_left: bool, edge: gpui::Hsla, width: f32) -> gpui::Div {
    let ramp = div().absolute().top_0().bottom_0().w(px(width));
    let ramp = if toward_left {
        ramp.left_0()
    } else {
        ramp.right_0()
    };
    ramp.bg(gpui::linear_gradient(
        90.0,
        gpui::linear_color_stop(if toward_left { edge } else { edge.opacity(0.0) }, 0.0),
        gpui::linear_color_stop(if toward_left { edge.opacity(0.0) } else { edge }, 1.0),
    ))
}

/// What the strip measured and is animating.
#[derive(Default)]
pub(super) struct PillsStripCells {
    /// The strip's border box at its last paint, inside the chat box the pills' menus open above.
    bounds: Rc<Cell<Bounds<Pixels>>>,
    scroll: ScrollHandle,
    /// Pills hide past the scroll's left and right edges.
    fades: Rc<Cell<(bool, bool)>>,
    /// Whether the strip showed at the last render, and the last change: when it was first drawn,
    /// which way, and a counter naming its animation.
    shown: Cell<bool>,
    change: Cell<Option<(Option<Instant>, bool, u64)>>,
    /// The change whose end already has a redraw booked.
    settle_booked: Cell<Option<u64>>,
}

impl PillsStripCells {
    /// The chat box the strip sits in: the strip's border box one border width out.
    pub(super) fn chat_box(&self) -> Bounds<Pixels> {
        let strip = self.bounds.get();
        Bounds::new(
            point(strip.left() - px(1.0), strip.top() - px(1.0)),
            gpui::size(strip.size.width + px(2.0), strip.size.height + px(1.0)),
        )
    }

    /// What the menu of the pill painted at `pill` opens against: the pill's left and right edges
    /// at the chat box's top edge, and the box to stay inside (`MenuSide::Pill`).
    ///
    /// CDXC:SessionChat 2026-10-29 DECISION:
    /// User, of the strip's menus: "please make the menus always appear above the composer not below ... but it shouldn't be above the bar above the composer anymore, should be above the composer itself instead", and then "wtf this menu is opening so far from the trigger button's position". Every menu the strip opens (Resources, Workers, Tasks, Subagents, Background and Links) opens upward from the chat box's top edge with its 4px gap, over the working bar rather than above it, never below. Across, it lines up with its own pill: its right edge on the pill's (the pills sit at the strip's right), its left edge on the pill's for a pill in the box's left half, switching sides or shifting to stay inside the box (`strip_menu_left`, the sidebar's button-menu rule). Supersedes the same day's "aligned to the box's left edge" and the 2026-10-18 menus 4px above the working card.
    pub(super) fn menu_anchor(
        &self,
        pill: Bounds<Pixels>,
    ) -> (Bounds<Pixels>, super::option_menu::MenuSide) {
        let chat_box = self.chat_box();
        let anchor = Bounds::new(
            point(pill.left(), chat_box.top()),
            gpui::size(pill.size.width, px(1.0)),
        );
        (anchor, super::option_menu::MenuSide::Pill(chat_box))
    }
}

/// The left edge of a `width`-wide strip menu for the pill at `pill` inside the chat box `within`:
/// right-aligned with a pill in the box's right half, left-aligned with one in its left half, and
/// kept inside the box (`PillsStripCells::menu_anchor` has the user's words).
pub(super) fn strip_menu_left(
    pill: Bounds<Pixels>,
    within: Bounds<Pixels>,
    width: Pixels,
) -> Pixels {
    crate::app::native_sidebar::menus::menu_left_at_button(
        pill,
        pill.center().x > within.center().x,
        within.left(),
        within.right(),
        width,
    )
}

impl NativeChatView {
    /// The panel pills, the ? and the armed timers that call for the strip; the ↓ only joins them.
    fn strip_pills(&self, p: &ChatAppearance, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let ids: Vec<&str> = self.snapshot["workingCard"]["pills"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let mut panel_pills: Vec<AnyElement> = ids
            .iter()
            .filter_map(|id| {
                self.snapshot["chatPills"]
                    .as_array()?
                    .iter()
                    .find(|pill| pill["id"] == *id)
            })
            .map(|pill| self.chat_pill(pill, p, cx))
            .collect();
        // The Links pill is the last one (links_pill.rs has the user's words).
        if let Some(links) = &self.links_pill {
            panel_pills.push(self.links_pill_element(links, p, cx));
        }
        let mut pills = self.armed_action_items(p, cx);
        pills.extend(self.previous_question_icon_pill(p, cx));
        pills.extend(panel_pills);
        pills
    }

    /// The view status chip at the strip's left: a click goes back to Normal view, and it shows an X
    /// while hovered (gx-chat-core extras/chat_pills.rs `view_chips` has the user's words).
    fn view_chips(&self, p: &ChatAppearance, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let s = p.scale;
        let chips = self.snapshot["viewChips"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        chips
            .into_iter()
            .map(|chip| {
                let id = chip["id"].as_str().unwrap_or_default().to_string();
                let group = format!("chat-view-chip-{id}");
                let command = chip["command"].clone();
                div()
                    .id(gpui::ElementId::Name(group.clone().into()))
                    .group(group.clone())
                    .role(gpui::Role::Button)
                    .aria_label(chip["ariaLabel"].as_str().unwrap_or_default().to_string())
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .gap(px(4.0 * s))
                    .pl(px(8.0 * s))
                    .pr(px(5.0 * s))
                    .py(px(2.0 * s))
                    .rounded_full()
                    // A pill's height (its border included), quieter: a faint fill, no outline.
                    .border(px(s))
                    .border_color(gpui::transparent_black())
                    .bg(p.foreground.opacity(0.04))
                    .text_size(px(11.5 * s))
                    .line_height(px(16.0 * s))
                    .whitespace_nowrap()
                    .text_color(p.card_muted.opacity(0.85))
                    .chat_cursor_pointer()
                    .hover(|style| style.bg(p.foreground.opacity(0.07)))
                    .child(chip["label"].as_str().unwrap_or_default().to_string())
                    .child(
                        // Its room is kept while hidden, so the hover moves nothing.
                        svg()
                            .path("titlebar/x.svg")
                            .size(px(10.0 * s))
                            .flex_shrink_0()
                            .text_color(p.foreground)
                            .opacity(0.0)
                            .group_hover(group, |style| style.opacity(0.75)),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.invoke(command.clone(), cx);
                    }))
                    .into_any_element()
            })
            .collect()
    }

    /// Whether the strip shows: a draft is offered, or the transcript has rows and a pill, timer or
    /// view chip calls for it.
    pub(super) fn pills_strip_shown(&self) -> bool {
        if self.saved_draft_offered() {
            return true;
        }
        let armed = self
            .armed_actions
            .as_array()
            .is_some_and(|actions| !actions.is_empty());
        let chips = self.snapshot["viewChips"]
            .as_array()
            .is_some_and(|chips| !chips.is_empty());
        self.list.item_count() > 0
            && (armed
                || chips
                || self.links_pill.is_some()
                || self.snapshot["workingCard"]["pills"]
                    .as_array()
                    .is_some_and(|pills| !pills.is_empty())
                || self.open_question_pill_text().is_some())
    }

    /// The strip for the top of the chat box, or `None` once it has shrunk away.
    pub(super) fn render_pills_strip(
        &mut self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let s = p.scale;
        let shown = self.pills_strip_shown();
        let cells = &self.pills_strip;
        if cells.shown.replace(shown) != shown {
            let generation = cells
                .change
                .get()
                .map_or(0, |(_, _, generation)| generation + 1);
            cells.change.set(Some((None, shown, generation)));
        }
        let duration = Duration::from_millis(super::composer_animation::METRICS.duration_ms as u64);
        let reduced = crate::app::helpers::gpui_macos_reduce_motion_enabled();
        // The ease starts on the first frame that draws the change.
        let mut change = cells.change.get();
        if let Some((None, showing, generation)) = change {
            change = Some((Some(Instant::now()), showing, generation));
            cells.change.set(change);
        }
        let animating = change
            .filter(|_| !reduced)
            .filter(|(at, _, _)| at.is_some_and(|at| at.elapsed() < duration));
        if let Some((_, _, generation)) = animating
            && cells.settle_booked.replace(Some(generation)) != Some(generation)
        {
            // Draw once more when the ease ends, so a strip that shrank away drops out.
            cx.spawn(async move |chat, cx| {
                cx.background_executor().timer(duration).await;
                let _ = chat.update(cx, |_, cx| cx.notify());
            })
            .detach();
        }
        if !shown && animating.is_none() {
            return None;
        }
        let left_bottom = super::scroll_bottom::transcript_left_bottom(&self.list);
        let (chips, pills) = if shown {
            let mut chips: Vec<AnyElement> =
                self.saved_draft_pill_element(p, cx).into_iter().collect();
            chips.extend(self.view_chips(p, cx));
            (chips, self.strip_pills(p, cx))
        } else {
            (Vec::new(), Vec::new())
        };
        let row = div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap(px(6.0 * s))
            // The ↓ at the left of the pills, so it appearing moves none of them.
            .children(
                (shown && left_bottom)
                    .then(|| self.scroll_bottom_icon_pill(p, cx))
                    .flatten(),
            )
            .children(pills);
        let tint = p.foreground.opacity(STRIP_TINT);
        let edge = p.composer_background.blend(tint);
        let measured = self.pills_strip.bounds.clone();
        let strip = div()
            .id("chat-pills-strip")
            .role(gpui::Role::Toolbar)
            .aria_label("Chat pills")
            .relative()
            .w_full()
            .flex_shrink_0()
            .overflow_hidden()
            // The box's own top corners, inside its 1px border.
            .rounded_t(px(21.0 * s))
            .bg(tint)
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(STRIP_HEIGHT * s))
                    .flex_shrink_0()
                    .px(px(10.0 * s))
                    .flex()
                    .items_center()
                    .gap(px(8.0 * s))
                    // The Saved draft pill and the chips stay pinned left; the pills scroll in what
                    // is left beside them.
                    .when(!chips.is_empty(), |band| {
                        band.child(
                            div()
                                .flex()
                                .items_center()
                                .flex_shrink_0()
                                .gap(px(6.0 * s))
                                .children(chips),
                        )
                    })
                    .child(self.pills_strip_scroll(row, edge, p, cx)),
            )
            // The hairline under the band, the box's border colour.
            .child(div().h(px(1.0)).flex_shrink_0().bg(p.composer_border))
            .child(
                canvas(move |bounds, _, _| measured.set(bounds), |_, _, _, _| {})
                    .absolute()
                    .inset_0(),
            );
        let full = STRIP_HEIGHT * s + 1.0;
        Some(match animating {
            Some((_, showing, generation)) => {
                let easing = super::composer_animation::METRICS.easing;
                strip
                    .with_animation(
                        gpui::ElementId::Name(format!("chat-pills-strip-{generation}").into()),
                        Animation::new(duration).with_easing(move |t: f32| {
                            super::composer_animation::eased(&easing, t)
                        }),
                        move |strip, delta| {
                            let open = if showing { delta } else { 1.0 - delta };
                            strip.h(px(full * open)).opacity(open)
                        },
                    )
                    .into_any_element()
            }
            None => strip.h(px(full)).into_any_element(),
        })
    }

    /// The pills' scroll: right-aligned while they fit, scrolling sideways when they don't, with a
    /// fade into the band's colour at an edge that hides more of them.
    ///
    /// CDXC:SessionChat 2026-10-28 DECISION:
    /// User, of the pills' row: "I don't like how the background gets cut off on the right. I think we can do something better than that to show that it's going off screen. I don't like inner shadow." The pills scroll inside the strip, inset from the box's rounded edge, and fade into the band's colour over 24px at an edge that hides more of them (the right edge until scrolled to the end, the left once scrolled); no fade while everything shows. The phone matches in PillsStrip.tsx.
    fn pills_strip_scroll(
        &self,
        row: gpui::Div,
        edge: gpui::Hsla,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let scroll = self.pills_strip.scroll.clone();
        let fades = self.pills_strip.fades.clone();
        let (left, right) = fades.get();
        let chat = cx.entity_id();
        let fade = |toward_left: bool| edge_fade(toward_left, edge, PILLS_FADE * s);
        div()
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                div()
                    .id("chat-pills-strip-scroll")
                    .size_full()
                    .flex()
                    .items_center()
                    .overflow_x_scroll()
                    .track_scroll(&scroll)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_end()
                            .flex_shrink_0()
                            .min_w(relative(1.0))
                            .child(row.relative()),
                    ),
            )
            .when(left, |area| area.child(fade(true)))
            .when(right, |area| area.child(fade(false)))
            .child(
                canvas(
                    move |_, _, cx| {
                        let offset = -f32::from(scroll.offset().x);
                        let max = f32::from(scroll.max_offset().x);
                        let next = (offset > 0.5, max - offset > 0.5);
                        if fades.replace(next) != next {
                            cx.notify(chat);
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::strip_menu_left;
    use gpui::{Bounds, Pixels, point, px, size};

    fn rect(left: f32, width: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(left), px(500.0)), size(px(width), px(22.0)))
    }

    /// A chat box from x 100 to 900.
    fn chat_box() -> Bounds<Pixels> {
        Bounds::new(point(px(100.0), px(499.0)), size(px(800.0), px(33.0)))
    }

    #[test]
    fn a_pill_in_the_right_half_puts_the_menu_s_right_edge_on_its_own() {
        // The Tasks pill at 760..840: a 360-wide menu ends at 840, not at the box's left.
        assert_eq!(
            strip_menu_left(rect(760.0, 80.0), chat_box(), px(360.0)),
            px(480.0)
        );
    }

    #[test]
    fn a_pill_in_the_left_half_puts_the_menu_s_left_edge_on_its_own() {
        assert_eq!(
            strip_menu_left(rect(200.0, 80.0), chat_box(), px(440.0)),
            px(200.0)
        );
    }

    #[test]
    fn a_menu_that_would_leave_the_box_shifts_inside_it() {
        // A right-half pill at 480..560: right-aligned, a 470-wide menu would start at 90, left
        // of the box, and left-aligned it would end at 950, past it; it starts at the box's left.
        assert_eq!(
            strip_menu_left(rect(480.0, 80.0), chat_box(), px(470.0)),
            px(100.0)
        );
        // A left-half pill at 300..480 with a 700-wide menu ends at the box's right instead.
        assert_eq!(
            strip_menu_left(rect(300.0, 180.0), chat_box(), px(700.0)),
            px(200.0)
        );
    }

    #[test]
    fn a_menu_wider_than_the_box_starts_at_its_left_edge() {
        assert_eq!(
            strip_menu_left(rect(760.0, 80.0), chat_box(), px(1000.0)),
            px(100.0)
        );
    }
}
