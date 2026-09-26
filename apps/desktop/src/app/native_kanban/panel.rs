//! The ticket panel beside the lanes: New ticket and Edit ticket (`NewTicketDialog` and
//! `EditTicketDialog`). It is a sibling of the lanes rather than a dialog over them.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};

use super::model::{PRIORITY_OPTIONS, TSHIRT_OPTIONS, board_status_label, known_labels};
use super::palette::KanbanPalette;
use super::state::{KanbanFormMode, KanbanTicketForm};
use super::text::{format_short_date, parse_comment};
use super::widgets::{
    KanbanButtonKind, LANE_RADIUS, choice_pill, kanban_button, section_title, text_area, text_field,
};
use crate::GhostexGpuiApp;
use crate::app::helpers::titlebar_svg_icon;

pub(crate) const PANEL_WIDTH: f32 = 400.0;

fn field(label: &'static str, p: &KanbanPalette, body: impl IntoElement) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(section_title(label, p))
        .child(body)
}

impl GhostexGpuiApp {
    fn render_native_kanban_pill_row(
        &self,
        id_prefix: &'static str,
        options: Vec<(String, String)>,
        selected: &str,
        p: &KanbanPalette,
        cx: &mut Context<Self>,
        apply: fn(&mut KanbanTicketForm, &str),
    ) -> gpui::Div {
        div()
            .flex()
            .flex_wrap()
            .gap(px(6.0))
            .children(options.into_iter().map(|(label, value)| {
                let is_selected = value == selected;
                choice_pill(
                    ElementId::Name(format!("{id_prefix}-{value}").into()),
                    label.into(),
                    is_selected,
                    p,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(form) = this.native_kanban_form_mut() {
                        apply(form, &value);
                    }
                    this.native_kanban_notify(cx);
                }))
            }))
    }

    fn render_native_kanban_labels(
        &self,
        form: &KanbanTicketForm,
        p: &KanbanPalette,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let typed = form.label_input.read(cx).value().trim().to_lowercase();
        let suggestions = known_labels(&self.native_kanban.issues)
            .into_iter()
            .filter(|label| !form.labels.contains(label))
            .filter(|label| typed.is_empty() || label.to_lowercase().contains(&typed))
            .take(10)
            .collect::<Vec<_>>();
        let label_chip = |label: String, selected: bool, cx: &mut Context<Self>| {
            let tone = KanbanPalette::chip_tone(&label);
            let hover = p.control_hover;
            let value = label.clone();
            div()
                .id(ElementId::Name(
                    format!("kanban-form-label-{}-{label}", u8::from(selected)).into(),
                ))
                .flex()
                .items_center()
                .gap(px(5.0))
                .px(px(8.0))
                .py(px(3.0))
                .rounded_full()
                .border_1()
                .border_color(if selected { p.border_strong } else { p.border })
                .text_size(px(12.0))
                .text_color(if selected { p.foreground } else { p.muted })
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.native_kanban_toggle_form_label(&value, cx);
                }))
                .child(div().size(px(7.0)).rounded_full().bg(tone))
                .child(label)
                .when(selected, |this| {
                    this.child(titlebar_svg_icon("titlebar/x.svg", 11.0, p.muted))
                })
        };
        let selected = form
            .labels
            .iter()
            .map(|label| label_chip(label.clone(), true, cx))
            .collect::<Vec<_>>();
        let suggested = suggestions
            .into_iter()
            .map(|label| label_chip(label, false, cx))
            .collect::<Vec<_>>();
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .when(!selected.is_empty(), |this| {
                this.child(div().flex().flex_wrap().gap(px(6.0)).children(selected))
            })
            .child(text_field(&form.label_input, p, window, cx))
            .when(!suggested.is_empty(), |this| {
                this.child(div().flex().flex_wrap().gap(px(6.0)).children(suggested))
            })
    }

    fn render_native_kanban_comments(
        &self,
        form: &KanbanTicketForm,
        p: &KanbanPalette,
    ) -> gpui::Div {
        let comments = form
            .ticket
            .as_ref()
            .map(|ticket| ticket.issue.comments.clone())
            .unwrap_or_default();
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .when(comments.is_empty(), |this| {
                this.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(p.faint)
                        .child("No comments yet."),
                )
            })
            .children(comments.into_iter().map(|comment| {
                let (body, agent) = parse_comment(&comment.text);
                let date = format_short_date(comment.created_at.as_deref());
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .p(px(10.0))
                    .rounded(px(8.0))
                    .bg(p.card)
                    .border_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .justify_between()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .flex()
                                    .gap(px(6.0))
                                    .min_w_0()
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(13.0))
                                            .text_color(p.foreground.opacity(0.9))
                                            .child(if comment.author.is_empty() {
                                                "Comment".to_string()
                                            } else {
                                                comment.author.clone()
                                            }),
                                    )
                                    .when_some(agent, |this, agent| {
                                        this.child(
                                            div()
                                                .text_size(px(12.0))
                                                .text_color(p.muted)
                                                .child(format!("({agent})")),
                                        )
                                    }),
                            )
                            .child(div().text_size(px(11.0)).text_color(p.faint).child(date)),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(18.0))
                            .text_color(p.primary)
                            .child(if body.is_empty() { comment.text } else { body }),
                    )
            }))
    }

    pub(crate) fn render_native_kanban_ticket_panel(
        &self,
        form: &KanbanTicketForm,
        p: &KanbanPalette,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = &self.native_kanban;
        let editing = matches!(form.mode, KanbanFormMode::Edit { .. });
        let ticket = form.ticket.as_ref();
        let (title, subtitle): (&str, String) = match (&form.mode, ticket) {
            (KanbanFormMode::Edit { .. }, Some(ticket)) => (
                "Edit ticket",
                format!("{} · {}", ticket.display_id, ticket.issue.id),
            ),
            _ => (
                "New Ticket",
                format!(
                    "Leave the title empty to auto-generate it from the prompt. Creates in {}.",
                    board_status_label(&form.status, &state.columns)
                ),
            ),
        };
        let has_prompt = !form.description.read(cx).value().trim().is_empty();
        let no_agents = state.conversation.agents.is_empty();
        let ticket_id = ticket
            .map(|ticket| ticket.issue.id.clone())
            .unwrap_or_default();
        let link = state.conversation.primary_link_for(&ticket_id);
        let busy = state.busy_ticket.is_some();
        let status_options = state
            .columns
            .iter()
            .map(|column| (column.label.clone(), column.key.clone()))
            .collect::<Vec<_>>();
        let priority_options = PRIORITY_OPTIONS
            .iter()
            .map(|(label, value)| ((*label).to_string(), (*value).to_string()))
            .collect::<Vec<_>>();
        let mut estimate_options = vec![("None".to_string(), "none".to_string())];
        estimate_options.extend(
            TSHIRT_OPTIONS
                .iter()
                .map(|(label, _)| ((*label).to_string(), (*label).to_string())),
        );
        let people = ticket
            .map(|ticket| {
                [
                    ticket
                        .issue
                        .assignee
                        .as_ref()
                        .map(|assignee| format!("Assigned to {assignee}")),
                    super::model::ticket_creator_name(&ticket.issue)
                        .map(|creator| format!("Created by {creator}")),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ")
            })
            .filter(|line| !line.is_empty());
        let body = div()
            .id("kanban-ticket-panel-body")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .p(px(16.0))
                    .child(field("Title", p, text_field(&form.title, p, window, cx)))
                    .child(field(
                        "Prompt",
                        p,
                        text_area(&form.description, 168.0, p, window, cx),
                    ))
                    .when(editing, |this| {
                        this.child(field(
                            "Status",
                            p,
                            self.render_native_kanban_pill_row(
                                "kanban-form-status",
                                status_options,
                                &form.status,
                                p,
                                cx,
                                |form, value| form.status = value.to_string(),
                            ),
                        ))
                    })
                    .child(field(
                        "Priority",
                        p,
                        self.render_native_kanban_pill_row(
                            "kanban-form-priority",
                            priority_options,
                            &form.priority,
                            p,
                            cx,
                            |form, value| form.priority = value.to_string(),
                        ),
                    ))
                    .child(field(
                        "Estimate",
                        p,
                        self.render_native_kanban_pill_row(
                            "kanban-form-estimate",
                            estimate_options,
                            form.tshirt.unwrap_or("none"),
                            p,
                            cx,
                            |form, value| {
                                form.tshirt = TSHIRT_OPTIONS
                                    .iter()
                                    .find(|(label, _)| *label == value)
                                    .map(|(label, _)| *label);
                            },
                        ),
                    ))
                    .child(field(
                        "Labels",
                        p,
                        self.render_native_kanban_labels(form, p, window, cx),
                    ))
                    .when_some(people, |this, people| {
                        this.child(div().text_size(px(12.0)).text_color(p.muted).child(people))
                    })
                    .when(editing, |this| {
                        this.child(field(
                            "Comments",
                            p,
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(10.0))
                                .child(self.render_native_kanban_comments(form, p))
                                .child(text_area(&form.comment, 84.0, p, window, cx)),
                        ))
                    }),
            );
        let footer = if editing {
            let (primary_label, primary_command): (SharedString, &str) = match link {
                Some(link) if link.openable => ("Go to Session".into(), "jump"),
                Some(_) => ("Resume Session".into(), "jump"),
                None => ("Start work".into(), "start"),
            };
            let primary_disabled = busy || (link.is_none() && no_agents);
            let delete_id = ticket_id.clone();
            let start_id = ticket_id.clone();
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    kanban_button(
                        "kanban-form-delete",
                        Some("titlebar/trash.svg"),
                        Some("Delete".into()),
                        KanbanButtonKind::Danger,
                        false,
                        p,
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.native_kanban_confirm_delete(&delete_id, window, cx);
                    })),
                )
                .child(div().flex_1())
                .child(
                    kanban_button(
                        "kanban-form-start",
                        Some("titlebar/player-play.svg"),
                        Some(primary_label),
                        KanbanButtonKind::Secondary,
                        primary_disabled,
                        p,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if primary_disabled {
                            return;
                        }
                        if primary_command == "jump" {
                            this.native_kanban_jump_to_session(&start_id, cx);
                        } else {
                            this.native_kanban.panel = None;
                            this.native_kanban_start_work(&start_id, cx);
                        }
                    })),
                )
                .child(
                    kanban_button(
                        "kanban-form-save",
                        None,
                        Some("Save".into()),
                        KanbanButtonKind::Primary,
                        false,
                        p,
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.native_kanban_save_form(cx))),
                )
        } else {
            let start_disabled = !has_prompt || no_agents || busy;
            div()
                .flex()
                .items_center()
                .justify_end()
                .gap(px(8.0))
                .child(
                    kanban_button(
                        "kanban-form-create",
                        None,
                        Some("Create".into()),
                        KanbanButtonKind::Secondary,
                        !has_prompt,
                        p,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if has_prompt {
                            this.native_kanban_create_from_form(false, cx);
                        }
                    })),
                )
                .child(
                    kanban_button(
                        "kanban-form-create-start",
                        Some("titlebar/player-play.svg"),
                        Some("Create & Start".into()),
                        KanbanButtonKind::Primary,
                        start_disabled,
                        p,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !start_disabled {
                            this.native_kanban_create_from_form(true, cx);
                        }
                    })),
                )
        };
        let hover = p.control_hover;
        div()
            .id("kanban-ticket-panel")
            .w(px(PANEL_WIDTH))
            .flex_none()
            .h_full()
            .min_h_0()
            .flex()
            .flex_col()
            .rounded(px(LANE_RADIUS))
            .bg(p.panel)
            .border_1()
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_start()
                    .justify_between()
                    .gap(px(12.0))
                    .px(px(16.0))
                    .pt(px(14.0))
                    .pb(px(10.0))
                    .border_b_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(3.0))
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(15.0))
                                    .text_color(p.foreground)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(p.muted)
                                    .child(subtitle),
                            ),
                    )
                    .child(
                        div()
                            .id("kanban-ticket-panel-close")
                            .size(px(26.0))
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .rounded(px(7.0))
                            .cursor_pointer()
                            .hover(move |style| style.bg(hover))
                            .on_click(
                                cx.listener(|this, _, _, cx| this.native_kanban_close_panel(cx)),
                            )
                            .child(titlebar_svg_icon("titlebar/x.svg", 14.0, p.muted)),
                    ),
            )
            .child(body)
            .child(
                div()
                    .flex_none()
                    .px(px(16.0))
                    .py(px(12.0))
                    .border_t_1()
                    .border_color(p.border)
                    .child(footer),
            )
            .into_any_element()
    }
}
