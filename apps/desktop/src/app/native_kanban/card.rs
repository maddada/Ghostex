//! One ticket card (`TicketCard` in board-lane-card.tsx): id and assignee, priority and title,
//! then the details the card-view toggles allow. Cards drag between lanes and open on click.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, ElementId, FontWeight, Hsla, InteractiveElement as _,
    IntoElement, MouseButton, MouseDownEvent, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};

use super::model::{BoardTicket, estimate_to_tshirt, priority_label, ticket_creator_name};
use super::palette::KanbanPalette;
use super::widgets::CARD_RADIUS;
use crate::GhostexGpuiApp;
use crate::app::helpers::titlebar_svg_icon;

/// What a dragged card carries; lanes accept it and move the ticket.
#[derive(Clone)]
pub(crate) struct KanbanCardDrag {
    pub(crate) ticket_id: String,
    title: SharedString,
    font: SharedString,
    background: Hsla,
    border: Hsla,
    foreground: Hsla,
}

impl Render for KanbanCardDrag {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(240.0))
            .px(px(12.0))
            .py(px(10.0))
            .rounded(px(CARD_RADIUS))
            .bg(self.background)
            .border_1()
            .border_color(self.border)
            .opacity(0.92)
            .font_family(self.font.clone())
            .text_size(px(13.0))
            .text_color(self.foreground)
            .truncate()
            .child(self.title.clone())
    }
}

/// `TicketPriorityIcon`: a filled square for Urgent, otherwise three rising bars.
fn priority_icon(priority: Option<i64>, p: &KanbanPalette) -> AnyElement {
    let tone = p.priority_tone(priority);
    let value = priority.unwrap_or(2);
    if value <= 0 {
        return div()
            .size(px(13.0))
            .rounded(px(3.5))
            .bg(tone)
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(10.0))
            .font_weight(FontWeight::BOLD)
            .text_color(gpui::rgb(0x0e0e0e))
            .child("!")
            .into_any_element();
    }
    let filled = match value {
        1 => 3,
        2 => 2,
        _ => 1,
    };
    div()
        .flex()
        .items_end()
        .gap(px(1.5))
        .h(px(12.0))
        .children((0..3).map(|bar| {
            div()
                .w(px(3.0))
                .h(px(4.0 + bar as f32 * 3.5))
                .rounded(px(1.0))
                .bg(if bar < filled {
                    tone
                } else {
                    tone.opacity(0.25)
                })
        }))
        .into_any_element()
}

fn chip(
    icon: Option<(&'static str, Hsla)>,
    dot: Option<Hsla>,
    text: String,
    p: &KanbanPalette,
) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(5.0))
        .min_w_0()
        .max_w_full()
        .px(px(8.0))
        .py(px(2.0))
        .rounded_full()
        .border_1()
        .border_color(p.border)
        .text_size(px(11.0))
        .text_color(p.muted)
        .when_some(dot, |this, dot| {
            this.child(div().size(px(7.0)).flex_none().rounded_full().bg(dot))
        })
        .when_some(icon, |this, (icon, color)| {
            this.child(titlebar_svg_icon(icon, 12.0, color))
        })
        .child(div().truncate().child(text))
}

impl GhostexGpuiApp {
    pub(crate) fn render_native_kanban_card(
        &self,
        ticket: &BoardTicket,
        p: &KanbanPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = self.native_kanban.card_view;
        let issue = &ticket.issue;
        let id = issue.id.clone();
        let blocked_by = issue
            .dependency_count
            .unwrap_or(issue.dependencies.len() as u64);
        let blocking = issue.dependent_count.unwrap_or(0);
        let comments = issue.comment_total();
        let tshirt = estimate_to_tshirt(issue.estimate);
        let link = self.native_kanban.conversation.primary_link_for(&issue.id);
        let show_top = view.show_id || (view.show_assignee && issue.assignee.is_some());
        let show_chips = (view.show_labels && !issue.labels.is_empty())
            || (view.show_details
                && (tshirt.is_some() || blocked_by > 0 || blocking > 0 || comments > 0));
        let hover = p.card_hover;
        let drag = KanbanCardDrag {
            ticket_id: id.clone(),
            title: issue.title.clone().into(),
            font: p.font.clone().into(),
            background: p.card_hover,
            border: p.border_strong,
            foreground: p.foreground,
        };
        let open_id = id.clone();
        let menu_id = id.clone();
        let jump_id = id.clone();
        let busy = self.native_kanban.busy_ticket.is_some();
        div()
            .id(ElementId::Name(format!("kanban-card-{id}").into()))
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .p(px(12.0))
            .rounded(px(CARD_RADIUS))
            .bg(p.card)
            .border_1()
            .border_color(p.border)
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.native_kanban_open_ticket(&open_id, window, cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.show_native_kanban_card_menu(&menu_id, event.position, window, cx);
                }),
            )
            .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
            .when(show_top, |this| {
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .min_w_0()
                        .child(
                            div()
                                .truncate()
                                .text_size(px(11.0))
                                .text_color(p.faint)
                                .child(if view.show_id {
                                    ticket.display_id.clone()
                                } else {
                                    String::new()
                                }),
                        )
                        .when_some(
                            issue.assignee.clone().filter(|_| view.show_assignee),
                            |this, assignee| {
                                let tone = KanbanPalette::chip_tone(&assignee);
                                let initial = assignee
                                    .chars()
                                    .next()
                                    .map(|ch| ch.to_uppercase().to_string())
                                    .unwrap_or_default();
                                this.child(
                                    div()
                                        .size(px(18.0))
                                        .flex_none()
                                        .rounded_full()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .bg(tone.opacity(0.18))
                                        .text_color(tone)
                                        .text_size(px(10.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(initial),
                                )
                            },
                        ),
                )
            })
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(7.0))
                    .min_w_0()
                    .when(view.show_priority, |this| {
                        this.child(
                            div()
                                .id(ElementId::Name(format!("kanban-card-priority-{id}").into()))
                                .flex_none()
                                .mt(px(3.0))
                                .tooltip({
                                    let label = priority_label(issue.priority);
                                    move |window, cx| {
                                        crate::app::helpers::titlebar_tooltip(label, window, cx)
                                    }
                                })
                                .child(priority_icon(issue.priority, p)),
                        )
                    })
                    .child(
                        div()
                            .min_w_0()
                            .text_size(px(13.0))
                            .line_height(px(18.0))
                            .text_color(p.foreground.opacity(0.95))
                            .child(issue.title.clone()),
                    ),
            )
            .when(
                view.show_description && !issue.description.is_empty(),
                |this| {
                    this.child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(18.0))
                            .text_color(p.muted)
                            .line_clamp(2)
                            .child(issue.description.clone()),
                    )
                },
            )
            .when(show_chips, |this| {
                let mut row = div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(4.0))
                    .mt(px(2.0));
                if view.show_labels {
                    for label in &issue.labels {
                        row = row.child(chip(
                            None,
                            Some(KanbanPalette::chip_tone(label)),
                            label.clone(),
                            p,
                        ));
                    }
                }
                if view.show_details {
                    if let Some(tshirt) = tshirt {
                        row = row.child(chip(
                            Some(("titlebar/clock.svg", p.faint)),
                            None,
                            tshirt.to_string(),
                            p,
                        ));
                    }
                    if blocked_by > 0 {
                        row = row.child(chip(
                            Some(("titlebar/square-minus.svg", gpui::rgb(0xf87171).into())),
                            None,
                            format!("{blocked_by} blocked"),
                            p,
                        ));
                    }
                    if blocking > 0 {
                        row = row.child(chip(
                            Some(("titlebar/alert-circle.svg", gpui::rgb(0xfbbf24).into())),
                            None,
                            format!("{blocking} blocking"),
                            p,
                        ));
                    }
                    if comments > 0 {
                        row = row.child(chip(
                            Some(("titlebar/message-circle.svg", gpui::rgb(0x38bdf8).into())),
                            None,
                            comments.to_string(),
                            p,
                        ));
                    }
                }
                this.child(row)
            })
            .when_some(link.filter(|_| view.show_links), |this, link| {
                let icon = if link.openable {
                    "titlebar/external-link.svg"
                } else {
                    "titlebar/player-play.svg"
                };
                let muted = p.muted;
                let hover = p.control_hover;
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .text_size(px(12.0))
                        .text_color(p.muted)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .min_w_0()
                                .child(titlebar_svg_icon(
                                    "titlebar/link.svg",
                                    13.0,
                                    gpui::rgb(0x34d399).into(),
                                ))
                                .child(div().truncate().child(link.label.clone())),
                        )
                        .child(
                            div()
                                .id(ElementId::Name(format!("kanban-card-jump-{id}").into()))
                                .size(px(22.0))
                                .flex()
                                .flex_none()
                                .items_center()
                                .justify_center()
                                .rounded(px(6.0))
                                .when(busy, |this| this.opacity(0.45))
                                .when(!busy, |this| {
                                    this.cursor_pointer().hover(move |style| style.bg(hover))
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.native_kanban_jump_to_session(&jump_id, cx);
                                }))
                                .child(titlebar_svg_icon(icon, 13.0, muted)),
                        ),
                )
            })
            .when_some(
                ticket_creator_name(issue)
                    .filter(|_| view.show_details)
                    .map(str::to_string),
                |this, creator| {
                    this.child(
                        div()
                            .truncate()
                            .text_size(px(11.0))
                            .text_color(p.faint)
                            .child(format!("Created by {creator}")),
                    )
                },
            )
            .into_any_element()
    }
}
