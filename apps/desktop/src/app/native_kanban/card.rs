//! One ticket card (`TicketCard` in board-lane-card.tsx): id and assignee, title and its `P0`–`P4`
//! tag, the meta line, grey label chips and the linked agent, as the card-view toggles allow. Cards drag between lanes and open on click.

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
use crate::app::native_chat::fonts::CHAT_MONO;

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

/// The card's priority as a mono `P0`–`P4` tag in a hairline box.
fn priority_tag(priority: Option<i64>, p: &KanbanPalette) -> gpui::Div {
    div()
        .flex_none()
        .px(px(5.0))
        .rounded(px(5.0))
        .border_1()
        .border_color(p.border_strong)
        .font_family(CHAT_MONO)
        .text_size(px(10.5))
        .line_height(px(16.0))
        .text_color(p.muted)
        .child(format!("P{}", priority.unwrap_or(2).clamp(0, 4)))
}

/// A grey label chip: tag glyph and the label.
fn label_chip(label: String, p: &KanbanPalette) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(4.0))
        .min_w_0()
        .max_w_full()
        .px(px(6.0))
        .py(px(1.0))
        .rounded(px(5.0))
        .bg(p.control)
        .text_size(px(11.0))
        .text_color(p.muted)
        .child(titlebar_svg_icon("titlebar/tag.svg", 11.0, p.faint))
        .child(div().truncate().child(label))
}

/// One item of the meta line: an optional grey outline glyph and its text.
fn meta_item(icon: Option<&'static str>, text: String, p: &KanbanPalette) -> AnyElement {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(3.0))
        .when_some(icon, |this, icon| {
            this.child(titlebar_svg_icon(icon, 11.0, p.faint))
        })
        .child(text)
        .into_any_element()
}

/// bd's issue type as the meta line shows it: `bug` reads "Bug".
fn issue_type_label(issue_type: &str) -> String {
    let mut chars = issue_type.trim().chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
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
        let done = ticket.board_status == "done" || issue.status == "closed";
        let show_top = view.show_id || (view.show_assignee && issue.assignee.is_some());
        let mut meta = Vec::new();
        if view.show_details {
            if let Some(kind) = issue
                .issue_type
                .as_deref()
                .map(issue_type_label)
                .filter(|kind| !kind.is_empty())
            {
                meta.push(meta_item(None, kind, p));
            }
            if blocked_by > 0 {
                meta.push(meta_item(
                    Some("titlebar/link.svg"),
                    format!("Waiting on {blocked_by}"),
                    p,
                ));
            }
            if blocking > 0 {
                meta.push(meta_item(
                    Some("titlebar/alert-triangle.svg"),
                    format!("Blocks {blocking}"),
                    p,
                ));
            }
            if comments > 0 {
                meta.push(meta_item(
                    Some("titlebar/message-circle.svg"),
                    comments.to_string(),
                    p,
                ));
            }
            if let Some(tshirt) = tshirt {
                meta.push(meta_item(Some("titlebar/clock.svg"), tshirt.to_string(), p));
            }
        }
        let labels = if view.show_labels {
            issue.labels.clone()
        } else {
            Vec::new()
        };
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
        let faint = p.faint;
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
                                        .bg(p.control)
                                        .text_color(p.muted)
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
                    .justify_between()
                    .gap(px(8.0))
                    .min_w_0()
                    .child(
                        div()
                            .min_w_0()
                            .text_size(px(13.0))
                            .line_height(px(18.0))
                            .text_color(if done {
                                p.muted
                            } else {
                                p.foreground.opacity(0.95)
                            })
                            .when(done, |this| this.line_through())
                            .child(issue.title.clone()),
                    )
                    .when(view.show_priority, |this| {
                        this.child(
                            div()
                                .id(ElementId::Name(format!("kanban-card-priority-{id}").into()))
                                .flex_none()
                                .mt(px(1.0))
                                .tooltip({
                                    let label = priority_label(issue.priority);
                                    move |window, cx| {
                                        crate::app::helpers::titlebar_tooltip(label, window, cx)
                                    }
                                })
                                .child(priority_tag(issue.priority, p)),
                        )
                    }),
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
            .when(!meta.is_empty(), |this| {
                let mut row = div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(5.0))
                    .text_size(px(12.0))
                    .text_color(p.muted);
                for (index, item) in meta.into_iter().enumerate() {
                    if index > 0 {
                        row = row.child(div().text_color(faint).child("·"));
                    }
                    row = row.child(item);
                }
                this.child(row)
            })
            .when(!labels.is_empty(), |this| {
                this.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(4.0))
                        .mt(px(2.0))
                        .children(labels.into_iter().map(|label| label_chip(label, p))),
                )
            })
            .when_some(link.filter(|_| view.show_links), |this, link| {
                let muted = p.muted;
                let hover = p.control_hover;
                this.child(
                    div()
                        .mt(px(2.0))
                        .pt(px(8.0))
                        .border_t_1()
                        .border_color(p.border)
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
                                .child(titlebar_svg_icon("titlebar/robot.svg", 13.0, faint))
                                .child(div().truncate().child(link.label.clone())),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_none()
                                .items_center()
                                .gap(px(4.0))
                                .when(link.live, |this| {
                                    this.child(div().size(px(6.0)).rounded_full().bg(faint))
                                })
                                .child(
                                    div()
                                        .id(ElementId::Name(
                                            format!("kanban-card-jump-{id}").into(),
                                        ))
                                        .size(px(22.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(6.0))
                                        .when(busy, |this| this.opacity(0.45))
                                        .when(!busy, |this| {
                                            this.cursor_pointer()
                                                .hover(move |style| style.bg(hover))
                                        })
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.native_kanban_jump_to_session(&jump_id, cx);
                                        }))
                                        .child(titlebar_svg_icon(
                                            if link.openable {
                                                "titlebar/arrow-up-right.svg"
                                            } else {
                                                "titlebar/player-play.svg"
                                            },
                                            13.0,
                                            muted,
                                        )),
                                ),
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
            .when(done, |this| this.opacity(0.62))
            .into_any_element()
    }
}
