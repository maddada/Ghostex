//! One status lane (`BoardLane`): its header with the count and a `+` that creates a ticket in
//! the lane, and its scrolling column of cards, which is also where a dragged card drops.

use std::ops::Range;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, ElementId, InteractiveElement as _, IntoElement, ListAlignment, ListState,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, div, list, px,
};
use gpui_component::scroll::Scrollbar;

use super::board_scroll::kanban_scrollbar;
use super::card::KanbanCardDrag;
use super::model::{BoardColumn, MAX_VISIBLE_TICKETS_PER_LANE};
use super::palette::KanbanPalette;
use super::state::KanbanLaneList;
use super::widgets::{CARD_RADIUS, LANE_RADIUS};
use crate::GhostexGpuiApp;
use crate::app::helpers::{
    ThrottledAnimationExt as _, gpui_macos_reduce_motion_enabled, titlebar_svg_icon,
};

/// A lane never narrows past this; with more lanes than fit, the board scrolls sideways.
pub(crate) const LANE_MIN_WIDTH: f32 = 230.0;
/// Space between two lanes.
pub(crate) const LANE_GAP: f32 = 10.0;
/// Space between two cards in a lane.
const CARD_GAP: f32 = 8.0;
/// The height a card is assumed to have until it is first drawn, so a lane's scroll range is
/// about right before every card was measured.
const CARD_HEIGHT_HINT: f32 = 96.0;
/// How far above and below a lane's view cards are still built, so scrolling doesn't pop.
const CARD_OVERDRAW: f32 = 240.0;

/// The width the lanes need side by side at their narrowest.
pub(crate) fn lane_row_min_width(lane_count: usize) -> f32 {
    let count = lane_count as f32;
    count * LANE_MIN_WIDTH + (count - 1.0).max(0.0) * LANE_GAP
}

/// The lanes that can be on screen, for a board scrolled `scroll_left` pixels right in a view at
/// most `max_view_width` wide (the window's width: the board's own width is only known after
/// layout). A window resize can clamp the scroll after this runs, so the range also reaches back
/// to where the clamp could land.
pub(crate) fn lanes_in_view(
    lane_count: usize,
    scroll_left: f32,
    max_view_width: f32,
) -> Range<usize> {
    let row_width = lane_row_min_width(lane_count);
    if max_view_width >= row_width {
        return 0..lane_count;
    }
    let stride = LANE_MIN_WIDTH + LANE_GAP;
    let left = scroll_left.min(row_width - max_view_width).max(0.0);
    let right = scroll_left.max(0.0) + max_view_width;
    let first = (left / stride).floor() as usize;
    let end = (right / stride).ceil() as usize;
    first.min(lane_count)..end.min(lane_count)
}

/// Title lines of each skeleton card, per lane, so the lanes don't fill in lockstep.
const SKELETON_LANES: [&[u8]; 4] = [&[3, 2, 1, 2, 1], &[1, 2, 3, 1], &[2, 1], &[1, 2, 1, 2]];

/// The first load's placeholder cards: the card's shell holding an id, title lines and the
/// creator line in place of text, pulsing with the shared skeleton pulse.
fn skeleton_cards(position: usize, p: &KanbanPalette) -> AnyElement {
    let bar = p.foreground.opacity(if p.glass { 0.08 } else { 0.07 });
    let text = move |width: gpui::DefiniteLength, height: f32| {
        div().w(width).h(px(height)).rounded_full().bg(bar)
    };
    let cards = div().flex().flex_col().gap(px(8.0)).children(
        SKELETON_LANES[position % SKELETON_LANES.len()]
            .iter()
            .enumerate()
            .map(|(index, lines)| {
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .p(px(12.0))
                    .rounded(px(CARD_RADIUS))
                    .bg(p.card)
                    .border_1()
                    .border_color(p.border)
                    .child(text(px(52.0).into(), 7.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .children((0..*lines).map(|line| {
                                let last = line + 1 == *lines;
                                let width = if last && *lines > 1 {
                                    0.45 + (index % 3) as f32 * 0.1
                                } else {
                                    0.9 - (index % 2) as f32 * 0.08
                                };
                                text(gpui::relative(width), 10.0)
                            })),
                    )
                    .child(text(px(128.0).into(), 7.0))
            }),
    );
    if gpui_macos_reduce_motion_enabled() {
        return cards.into_any_element();
    }
    let (period, min) = crate::app::session_chat_skeleton::skeleton_pulse();
    cards
        .with_throttled_animation(
            ElementId::Name(format!("kanban-skeleton-pulse-{position}").into()),
            period,
            move |cards, frame| {
                let dip = 1.0 - (frame * std::f32::consts::TAU).cos();
                cards.opacity(1.0 - (1.0 - min) * dip * 0.5)
            },
        )
        .into_any_element()
}

impl GhostexGpuiApp {
    /// `in_view` is false for a lane scrolled out of view sideways, which draws only its header.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_native_kanban_lane(
        &mut self,
        column: &BoardColumn,
        indices: &[usize],
        empty_hint: Option<&'static str>,
        skeleton: Option<usize>,
        in_view: bool,
        measure_key: u64,
        p: &KanbanPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = column.key.clone();
        let add_key = column.key.clone();
        let lane_drop = p.lane_drop;
        let border_strong = p.border_strong;
        let control_hover = p.control_hover;
        let count = indices.len();
        let body = if let Some(position) = skeleton {
            Self::native_kanban_lane_static_body(Some(skeleton_cards(position, p)))
        } else if count == 0 {
            Self::native_kanban_lane_static_body(empty_hint.map(|hint| {
                div()
                    .px(px(4.0))
                    .py(px(8.0))
                    .text_size(px(12.0))
                    .text_color(p.faint)
                    .child(hint)
                    .into_any_element()
            }))
        } else if !in_view {
            div().flex_1().min_h_0().into_any_element()
        } else {
            self.render_native_kanban_lane_cards(&column.key, indices, measure_key, p, cx)
        };
        div()
            .id(ElementId::Name(
                format!("kanban-lane-{}", column.key).into(),
            ))
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(LANE_MIN_WIDTH))
            .h_full()
            .min_h_0()
            .rounded(px(LANE_RADIUS))
            .bg(p.lane)
            .border_1()
            .border_color(p.border)
            .drag_over::<KanbanCardDrag>(move |style, _, _, _| {
                style.bg(lane_drop).border_color(border_strong)
            })
            .on_drop(cx.listener(move |this, drag: &KanbanCardDrag, _, cx| {
                this.native_kanban_move_ticket(&drag.ticket_id, &key, cx);
            }))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_between()
                    .h(px(42.0))
                    .pl(px(12.0))
                    .pr(px(6.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .min_w_0()
                            .child(
                                div()
                                    .size(px(6.0))
                                    .flex_none()
                                    .rounded_full()
                                    .bg(p.tone(column.tone)),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(13.0))
                                    .text_color(p.foreground.opacity(0.9))
                                    .child(column.label.clone()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(2.0))
                            .when(skeleton.is_none(), |this| {
                                this.child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(p.muted)
                                        .child(count.to_string()),
                                )
                            })
                            .child(
                                div()
                                    .id(ElementId::Name(
                                        format!("kanban-lane-add-{}", column.key).into(),
                                    ))
                                    .size(px(26.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(7.0))
                                    .cursor_pointer()
                                    .hover(move |style| style.bg(control_hover))
                                    .tooltip({
                                        let label = format!("Add ticket to {}", column.label);
                                        move |window, cx| {
                                            crate::app::helpers::titlebar_tooltip(
                                                label.clone(),
                                                window,
                                                cx,
                                            )
                                        }
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.native_kanban_open_new_ticket(&add_key, window, cx);
                                    }))
                                    .child(titlebar_svg_icon("titlebar/plus.svg", 14.0, p.muted)),
                            ),
                    ),
            )
            .child(body)
            .into_any_element()
    }

    /// A lane body that is not a card list: the first load's skeleton cards or the empty hint.
    fn native_kanban_lane_static_body(content: Option<AnyElement>) -> AnyElement {
        div()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .px(px(10.0))
                    .pt(px(2.0))
                    .pb(px(10.0))
                    .children(content),
            )
            .into_any_element()
    }

    /// CDXC:ProjectBoard 2026-09-29 WHY:
    /// Scrolling redraws the lanes' view on every frame, and building every card of every lane (up to `MAX_VISIBLE_TICKETS_PER_LANE` each) made scrolling lag. A lane's cards are a GPUI list that builds only the cards in view plus `CARD_OVERDRAW`, and lanes scrolled out of view sideways build none (`lanes_in_view`).
    fn render_native_kanban_lane_cards(
        &mut self,
        lane_key: &str,
        indices: &[usize],
        measure_key: u64,
        p: &KanbanPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let total = indices.len();
        let visible = total.min(MAX_VISIBLE_TICKETS_PER_LANE);
        let hidden = total - visible;
        let state =
            self.native_kanban_lane_list(lane_key, visible + usize::from(hidden > 0), measure_key);
        let indices: Rc<[usize]> = indices[..visible].into();
        let scrollbar = kanban_scrollbar(Scrollbar::vertical(&state), p).id(ElementId::Name(
            format!("kanban-lane-scrollbar-{lane_key}").into(),
        ));
        let p = p.clone();
        let cards = list(
            state,
            cx.processor(move |this, index: usize, _window, cx| {
                let row = div().px(px(10.0)).pb(px(CARD_GAP));
                match indices
                    .get(index)
                    .and_then(|ticket| this.native_kanban.tickets.get(*ticket))
                {
                    Some(ticket) => row.child(this.render_native_kanban_card(ticket, &p, cx)),
                    None => row.child(
                        div()
                            .px(px(12.0))
                            .py(px(10.0))
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(p.border)
                            .text_size(px(12.0))
                            .text_color(p.muted)
                            .child(format!(
                                "Showing {visible} of {total}. Use search or filters to narrow this lane."
                            )),
                    ),
                }
                .into_any_element()
            }),
        )
        .size_full()
        .pt(px(2.0))
        .pb(px(10.0 - CARD_GAP));
        // The bar runs in the cards' side gutter, over the padding of the rows.
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(cards)
            .child(scrollbar)
            .into_any_element()
    }

    /// The lane's list state, brought up to `count` items. Measured card heights are dropped when
    /// `measure_key` (tickets, card details shown, session links, font) moved, and the lane keeps
    /// its scroll position through both.
    fn native_kanban_lane_list(
        &mut self,
        lane_key: &str,
        count: usize,
        measure_key: u64,
    ) -> ListState {
        let lane = self
            .native_kanban
            .lane_lists
            .entry(lane_key.to_string())
            .or_insert_with(|| KanbanLaneList {
                list: ListState::new(count, ListAlignment::Top, px(CARD_OVERDRAW))
                    .with_uniform_item_height(px(CARD_HEIGHT_HINT)),
                measure_key,
            });
        let old_count = lane.list.item_count();
        if old_count != count {
            let top = lane.list.logical_scroll_top();
            lane.list.splice(0..old_count, count);
            let _ = lane
                .list
                .clone()
                .with_uniform_item_height(px(CARD_HEIGHT_HINT));
            lane.list.scroll_to(top);
        } else if lane.measure_key != measure_key {
            lane.list.remeasure_items(0..count);
        }
        lane.measure_key = measure_key;
        lane.list.clone()
    }
}
