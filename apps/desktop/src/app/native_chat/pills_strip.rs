//! The pill strip along the chat box's top edge (composer.rs): a faintly tinted band inside the box,
//! with a hairline under it and the box's top corners, holding the chat's pills on the right: the
//! armed timers, the ? (a question waits), Resources, Workers, Links, Tasks, Subagents and
//! Background, and the ↓ jump-to-bottom at the left of them while the strip shows. Pills that don't
//! fit scroll sideways, each fading out before an edge that hides more reaches it (`pill_opacities`). With no pill there is no strip; it grows
//! in and shrinks away with the chat box's own ease. Each pill opens its menu above the box, lined up
//! with the pill.
//!
//! A Ghostex page put aside shows here as a pill of its own, first among the panel pills
//! (minimized_pages.rs), and is enough to show the strip.
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
    IntoElement, ParentElement as _, Pixels, StatefulInteractiveElement as _, Styled as _, canvas,
    div, point, px, svg,
};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
    time::Duration,
};
use web_time::Instant;

/// A view chip's icon, the one its View menu row shows (actions.rs `view_modes_row`).
fn view_chip_icon(id: &str) -> &'static str {
    match id {
        "summary" => "titlebar/list-check.svg",
        // Quiet View's chip (gx-chat-core `transcript/quiet_view.rs` `view_chip`).
        "quiet" => "titlebar/eye-off.svg",
        "full" => "titlebar/eye.svg",
        _ => "titlebar/list-details.svg",
    }
}

/// The strip's height (unscaled): a pill's 25px with 5px above and below, and its 1px hairline.
const STRIP_HEIGHT: f32 = 25.0 + 2.0 * 5.0;

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
    /// Each pill's border box at the last paint, in row order.
    pill_bounds: Rc<RefCell<Vec<Bounds<Pixels>>>>,
    /// What each drawn pill is, in row order (`pill_bounds`'s kinds).
    items: Rc<RefCell<Vec<StripItem>>>,
    /// Every panel pill in row order with its label's width (unscaled), the hidden ones too.
    order: Rc<RefCell<Vec<(String, f32)>>>,
    /// Each panel pill's width (unscaled) with its label and without, as last drawn each way.
    widths: Rc<RefCell<HashMap<String, (Option<f32>, Option<f32>)>>>,
    /// A hidden page's pill's widths (unscaled) at its three tiers, known before it is drawn
    /// (minimized_pages.rs).
    presets: Rc<RefCell<HashMap<String, (f32, f32, f32)>>>,
    /// Room (unscaled) kept at the row's end for a page pill's floating x when that pill is last.
    end_room: Rc<Cell<f32>>,
    /// Which pills drop their label and how many of the last ones wait in the "+N" pill.
    plan: Rc<RefCell<StripPlan>>,
    /// The "+N" pill's width at its last paint (unscaled).
    overflow_width: Rc<Cell<Option<f32>>>,
    /// The pills in the "+N" pill's list, as the core described them.
    pub(super) hidden: RefCell<Vec<Value>>,
    /// Whether the strip showed at the last render, and the last change: when it was first drawn,
    /// which way, and a counter naming its animation.
    shown: Cell<bool>,
    change: Cell<Option<(Option<Instant>, bool, u64)>>,
    /// The change whose end already has a redraw booked.
    settle_booked: Cell<Option<u64>>,
}

/// What a drawn pill is: a fixed one (timers, ?, ↓, Links) always shows; a panel pill can drop its
/// label or go into the "+N" pill (`plan_strip`).
#[derive(Clone, Debug, PartialEq)]
pub(super) enum StripItem {
    Fixed,
    Panel {
        id: String,
        collapsed: bool,
        /// The label's width (unscaled), which dropping it frees.
        label: f32,
    },
    /// A hidden page's pill (minimized_pages.rs), at a width known before it is drawn.
    Page,
    Overflow,
}

/// How the row fits: the panel pills that drop their label, the page pills that drop their count
/// too, and how many of the last panel pills wait in the "+N" pill.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct StripPlan {
    collapsed: HashSet<String>,
    tiny: HashSet<String>,
    hidden: usize,
}

/// The gap between two pills (unscaled), and the "+N" pill's width before it has been drawn once.
pub(super) const PILL_GAP: f32 = 6.0;
const OVERFLOW_ESTIMATE: f32 = 56.0;

/// What a panel pill measures with its label (`full`) and without (`icon`): the other from the one
/// drawn when only that is known (the label, its 5px gap and the 1px of padding a label-less pill
/// takes less on each side).
fn pill_widths(full: Option<f32>, icon: Option<f32>, label: f32) -> Option<(f32, f32)> {
    let freed = if label > 0.0 { label + 7.0 } else { 0.0 };
    match (full, icon) {
        (Some(full), Some(icon)) => Some((full, icon)),
        (Some(full), None) => Some((full, (full - freed).max(0.0))),
        (None, Some(icon)) => Some((icon + freed, icon)),
        (None, None) => None,
    }
}

/// How the row of pills fits `avail`: how many of the last panel pills go into the "+N" pill and how
/// many of the last ones left drop their label, as few as can be, labels before pills. `fixed` are
/// the widths of the pills that always show and `pills` the panel pills' (with label, without).
pub(super) fn plan_strip(
    avail: f32,
    fixed: &[f32],
    pills: &[(f32, f32)],
    overflow: f32,
    gap: f32,
) -> (usize, usize) {
    let tiers: Vec<(f32, f32, f32)> = pills
        .iter()
        .map(|(full, icon)| (*full, *icon, *icon))
        .collect();
    let (hidden, collapsed, _) = plan_strip_tiers(avail, fixed, &tiers, overflow, gap);
    (hidden, collapsed)
}

/// `plan_strip` for pills with a third, narrowest width (a hidden page's pill, minimized_pages.rs:
/// full, icon and count, icon and dot; a panel pill's last two are the same). The last pills shown
/// take their narrowest width only once every pill shown has dropped its label, and before any
/// goes into the "+N" pill: `(hidden, collapsed, tiny)`, `tiny` of the last `collapsed`.
pub(super) fn plan_strip_tiers(
    avail: f32,
    fixed: &[f32],
    pills: &[(f32, f32, f32)],
    overflow: f32,
    gap: f32,
) -> (usize, usize, usize) {
    let n = pills.len();
    for hidden in 0..=n {
        let visible = n - hidden;
        for collapsed in 0..=visible {
            let tiny_most = if collapsed == visible { visible } else { 0 };
            for tiny in 0..=tiny_most {
                let mut total: f32 = fixed.iter().sum();
                let mut count = fixed.len();
                for (index, (full, icon, least)) in pills[..visible].iter().enumerate() {
                    total += if index >= visible - tiny {
                        *least
                    } else if index >= visible - collapsed {
                        *icon
                    } else {
                        *full
                    };
                    count += 1;
                }
                if hidden > 0 {
                    total += overflow;
                    count += 1;
                }
                total += gap * count.saturating_sub(1) as f32;
                if total <= avail + 0.5 {
                    return (hidden, collapsed, tiny);
                }
            }
        }
    }
    (n, 0, 0)
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
    /// The panel pills the core lists for the strip, less the Work pill and the red Blocked pill:
    /// on the computer the dock's header has their counts and the button that opens the list
    /// (work_cards.rs `render_work_dock` has the user's words). The phone keeps both pills.
    fn strip_pill_ids(&self) -> Vec<&str> {
        self.snapshot["workingCard"]["pills"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|id| {
                *id != super::coordinator_board::BOARD_MENU
                    && *id != ghostex_gx_chat_core::extras::coordinator_board::BLOCKED_PILL
            })
            .collect()
    }

    /// The panel pills, the ? and the armed timers that call for the strip; the ↓ only joins them.
    /// A panel pill the row has no room for drops its label, or goes into the "+N" pill
    /// (`StripPlan`).
    fn strip_pills(
        &self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Vec<(StripItem, AnyElement)> {
        let ids = self.strip_pill_ids();
        // A hidden page's pill comes first among them, so it is the last to shrink or go into
        // the "+N" pill (minimized_pages.rs).
        let mut presets = HashMap::new();
        let pages = self.minimized_pages.iter().map(|page| {
            presets.insert(page.pill_id(), self.page_pill_widths(page, p, cx));
            page.overflow_row()
        });
        let panel: Vec<Value> = pages
            .collect::<Vec<_>>()
            .into_iter()
            .chain(ids.iter().filter_map(|id| {
                self.snapshot["chatPills"]
                    .as_array()?
                    .iter()
                    .find(|pill| pill["id"] == *id)
                    .cloned()
            }))
            .collect();
        *self.pills_strip.presets.borrow_mut() = presets;
        let plan = self.pills_strip.plan.borrow().clone();
        let hidden = plan.hidden.min(panel.len());
        let visible = panel.len() - hidden;
        let labels: Vec<f32> = panel
            .iter()
            .map(|pill| {
                if self
                    .minimized_page(&super::transcript::text(pill, "id"))
                    .is_some()
                {
                    0.0
                } else {
                    self.pill_label_width(&super::transcript::text(pill, "label"), p, cx)
                }
            })
            .collect();
        *self.pills_strip.order.borrow_mut() = panel
            .iter()
            .zip(&labels)
            .map(|(pill, label)| (super::transcript::text(pill, "id"), *label))
            .collect();
        *self.pills_strip.hidden.borrow_mut() = panel[visible..].to_vec();
        let mut pills: Vec<(StripItem, AnyElement)> = self
            .armed_action_items(p, cx)
            .into_iter()
            .chain(self.previous_question_icon_pill(p, cx))
            .map(|pill| (StripItem::Fixed, pill))
            .collect();
        for (pill, label) in panel[..visible].iter().zip(&labels) {
            let id = super::transcript::text(pill, "id");
            let collapsed = plan.collapsed.contains(&id);
            if let Some(page) = self.minimized_page(&id) {
                let tier = if plan.tiny.contains(&id) {
                    super::minimized_pages::PageTier::Icon
                } else if collapsed {
                    super::minimized_pages::PageTier::Count
                } else {
                    super::minimized_pages::PageTier::Full
                };
                pills.push((StripItem::Page, self.minimized_page_pill(page, tier, p, cx)));
                continue;
            }
            pills.push((
                StripItem::Panel {
                    id,
                    collapsed,
                    label: *label,
                },
                self.chat_pill(pill, collapsed, p, cx),
            ));
        }
        if hidden > 0 {
            pills.push((StripItem::Overflow, self.strip_overflow_pill(hidden, p, cx)));
        }
        // The Links pill is the last one (links_pill.rs has the user's words).
        if let Some(links) = &self.links_pill {
            pills.push((StripItem::Fixed, self.links_pill_element(links, p, cx)));
        }
        pills
    }

    /// How wide `label` is in a pill (unscaled), which dropping it frees.
    fn pill_label_width(&self, label: &str, p: &ChatAppearance, cx: &gpui::App) -> f32 {
        super::fonts::label_width(label, &p.font, gpui::FontWeight::NORMAL, 13.0 * p.scale, cx)
            / p.scale
    }

    /// The view status chip at the strip's left: a click goes back to Normal view, and it shows an X
    /// while hovered in the place of its icon (gx-chat-core extras/chat_pills.rs `view_chips` has the user's words).
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
                    .gap(px(5.0 * s))
                    .px(px(8.0 * s))
                    .py(px(2.0 * s))
                    .rounded_full()
                    // A pill's height (its border included), quieter: a faint fill, no outline.
                    .border(px(s))
                    .border_color(gpui::transparent_black())
                    .bg(p.foreground.opacity(0.04))
                    .text_size(px(13.0 * s))
                    .line_height(px(19.0 * s))
                    .whitespace_nowrap()
                    .text_color(p.card_muted.opacity(0.85))
                    .chat_cursor_pointer()
                    .hover(|style| style.bg(p.foreground.opacity(0.07)))
                    .child(
                        // The view's own icon, which the X replaces in the same spot and size
                        // while the chip is hovered, so no room is kept for the X.
                        div()
                            .relative()
                            .size(px(14.0 * s))
                            .flex_shrink_0()
                            .child(
                                svg()
                                    .path(view_chip_icon(&id))
                                    .absolute()
                                    .inset_0()
                                    .size(px(14.0 * s))
                                    .text_color(p.card_muted.opacity(0.85))
                                    .group_hover(group.clone(), |style| style.opacity(0.0)),
                            )
                            .child(
                                svg()
                                    .path("titlebar/x.svg")
                                    .absolute()
                                    .inset_0()
                                    .size(px(14.0 * s))
                                    .text_color(p.foreground)
                                    .opacity(0.0)
                                    .group_hover(group, |style| style.opacity(0.75)),
                            ),
                    )
                    .child(chip["label"].as_str().unwrap_or_default().to_string())
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
        if self.saved_draft_offered() || !self.minimized_pages.is_empty() {
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
                || !self.strip_pill_ids().is_empty()
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
        // The ↓ at the left of the pills, so it appearing moves none of them.
        let scrolling: Vec<(StripItem, AnyElement)> = (shown && left_bottom)
            .then(|| self.scroll_bottom_icon_pill(p, cx))
            .flatten()
            .into_iter()
            .map(|pill| (StripItem::Fixed, pill))
            .chain(pills)
            .collect();
        // A page pill's floating x overhangs its corner; at the row's end the row keeps room for it,
        // which the fit counts (`pills_strip_fit`).
        let end_room = match scrolling.last() {
            Some((StripItem::Page, _))
                if !crate::app::window::round_close_button::close_buttons_on_left() =>
            {
                super::minimized_pages::X_OUTSET
            }
            _ => 0.0,
        };
        self.pills_strip.end_room.set(end_room);
        let row = div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap(px(6.0 * s))
            .pr(px(end_room * s))
            .children(self.measured_strip_pills(scrolling, s));
        let tint = p.foreground.opacity(STRIP_TINT);
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
                    // The Saved draft pill and the chips stay pinned left; the pills fit in what is
                    // left beside them.
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
                    .child(self.pills_strip_fit(row, p, cx)),
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

    /// The pills, each measured: its border box for the "+N" and menus, and a panel pill's width for
    /// the fit (`pills_strip_fit`), with its label or without as it was drawn.
    fn measured_strip_pills(&self, pills: Vec<(StripItem, AnyElement)>, s: f32) -> Vec<AnyElement> {
        let measured = self.pills_strip.pill_bounds.clone();
        {
            let mut bounds = measured.borrow_mut();
            bounds.clear();
            bounds.resize(pills.len(), Bounds::default());
        }
        *self.pills_strip.items.borrow_mut() = pills.iter().map(|(item, _)| item.clone()).collect();
        let widths = self.pills_strip.widths.clone();
        let overflow_width = self.pills_strip.overflow_width.clone();
        pills
            .into_iter()
            .enumerate()
            .map(|(index, (item, pill))| {
                let measured = measured.clone();
                let widths = widths.clone();
                let overflow_width = overflow_width.clone();
                div()
                    .relative()
                    .flex()
                    .flex_shrink_0()
                    .child(pill)
                    .child(
                        canvas(
                            move |bounds, _, _| {
                                if let Some(slot) = measured.borrow_mut().get_mut(index) {
                                    *slot = bounds;
                                }
                                let width = f32::from(bounds.size.width) / s;
                                if width <= 0.0 {
                                    return;
                                }
                                match &item {
                                    StripItem::Panel {
                                        id,
                                        collapsed,
                                        label,
                                    } => {
                                        let mut widths = widths.borrow_mut();
                                        let entry = widths.entry(id.clone()).or_default();
                                        if *label <= 0.0 {
                                            *entry = (Some(width), Some(width));
                                        } else if *collapsed {
                                            entry.1 = Some(width);
                                        } else {
                                            entry.0 = Some(width);
                                        }
                                    }
                                    StripItem::Overflow => overflow_width.set(Some(width)),
                                    StripItem::Fixed | StripItem::Page => {}
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .inset_0(),
                    )
                    .into_any_element()
            })
            .collect()
    }

    /// The pills' room: they sit at the right, whole. No pill is ever scrolled, cut or faded; when the
    /// row is too wide the last panel pills drop their label first, then go into one "+N" pill
    /// (`plan_strip`, strip_overflow.rs has the user's words).
    ///
    /// CDXC:SessionChat 2026-11-10 DECISION:
    /// User, of the row scrolling sideways behind edge fades (Board and Autopilot faded at its two
    /// edges once the pills grew to 13px): "the pills hiding is still bad, please make this simpler".
    /// Supersedes the 2026-10-28 and 2026-11-08 edge fades and the sideways scroll.
    fn pills_strip_fit(
        &self,
        row: gpui::Div,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let cells = &self.pills_strip;
        let pill_bounds = cells.pill_bounds.clone();
        let items = cells.items.clone();
        let order = cells.order.clone();
        let widths = cells.widths.clone();
        let plan = cells.plan.clone();
        let overflow_width = cells.overflow_width.clone();
        let presets = cells.presets.clone();
        let end_room = cells.end_room.clone();
        let chat = cx.entity_id();
        div()
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_end()
            .child(row.relative())
            .child(
                canvas(
                    move |view, _, cx| {
                        let avail = f32::from(view.size.width) / s - end_room.get();
                        let fixed: Vec<f32> = items
                            .borrow()
                            .iter()
                            .zip(pill_bounds.borrow().iter())
                            .filter(|(item, _)| **item == StripItem::Fixed)
                            .map(|(_, bounds)| f32::from(bounds.size.width) / s)
                            .collect();
                        let order = order.borrow();
                        let current = plan.borrow().clone();
                        let measured: Option<Vec<(f32, f32, f32)>> = order
                            .iter()
                            .map(|(id, label)| {
                                if let Some(preset) = presets.borrow().get(id) {
                                    return Some(*preset);
                                }
                                let (full, icon) =
                                    widths.borrow().get(id).copied().unwrap_or_default();
                                pill_widths(full, icon, *label)
                                    .map(|(full, icon)| (full, icon, icon))
                            })
                            .collect();
                        let next = match measured {
                            Some(pills) => {
                                let (hidden, collapsed, tiny) = plan_strip_tiers(
                                    avail,
                                    &fixed,
                                    &pills,
                                    overflow_width.get().unwrap_or(OVERFLOW_ESTIMATE),
                                    PILL_GAP,
                                );
                                let visible = order.len() - hidden;
                                let ids = |from: usize| {
                                    order[visible - from..visible]
                                        .iter()
                                        .map(|(id, _)| id.clone())
                                        .collect()
                                };
                                StripPlan {
                                    collapsed: ids(collapsed),
                                    tiny: ids(tiny),
                                    hidden,
                                }
                            }
                            // A pill never drawn has no width yet: show them all once to measure it.
                            None if current.hidden > 0 => StripPlan::default(),
                            None => return,
                        };
                        if next != current {
                            *plan.borrow_mut() = next;
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
