//! The Tree layout's list: section cards (Needs a decision, one per epic, Not in an epic), dense
//! two-line rows, and the "N done" folds, drawn as one virtualized list.

use ghostex_gx_core::beads_view::{
    BeadAgentState, BeadGlyph, BeadMeta, BeadRowModel, BeadsSection, BeadsSectionKind,
    BeadsTreeModel,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, Window, div, px, relative,
};

use super::menus::{BeadsMenu, BeadsMenuKind};
use super::view::BeadsView;
use crate::app::helpers::titlebar_svg_icon;
use crate::app::native_kanban::palette::KanbanPalette;

/// One entry of the virtualized list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TreeItem {
    Header {
        section: usize,
    },
    Row {
        section: usize,
        index: usize,
        done: bool,
        last: bool,
    },
    DoneFold {
        section: usize,
        last: bool,
    },
}

pub(crate) fn flatten(tree: &BeadsTreeModel) -> Vec<TreeItem> {
    let mut items = Vec::new();
    for (section_index, section) in tree.sections.iter().enumerate() {
        items.push(TreeItem::Header {
            section: section_index,
        });
        if section.collapsed {
            continue;
        }
        let has_fold = section.done_count > 0;
        let open_count = section.rows.len();
        for index in 0..open_count {
            items.push(TreeItem::Row {
                section: section_index,
                index,
                done: false,
                last: !has_fold && index + 1 == open_count,
            });
        }
        if has_fold {
            let done_count = section.done_rows.len();
            items.push(TreeItem::DoneFold {
                section: section_index,
                last: done_count == 0,
            });
            for index in 0..done_count {
                items.push(TreeItem::Row {
                    section: section_index,
                    index,
                    done: true,
                    last: index + 1 == done_count,
                });
            }
        }
    }
    items
}

pub(crate) fn glyph_icon(glyph: BeadGlyph) -> &'static str {
    match glyph {
        BeadGlyph::Open => "titlebar/circle.svg",
        BeadGlyph::InProgress => "titlebar/circle-half.svg",
        BeadGlyph::Blocked => "titlebar/barrier-block.svg",
        BeadGlyph::Snoozed => "titlebar/clock.svg",
        BeadGlyph::OnIce => "titlebar/player-pause.svg",
        BeadGlyph::Review => "titlebar/eye.svg",
        BeadGlyph::Decision => "titlebar/help-circle.svg",
        BeadGlyph::Closed => "titlebar/circle-check.svg",
        BeadGlyph::Epic => "titlebar/stack.svg",
    }
}

const SECTION_RADIUS: f32 = 12.0;
const SECTION_GAP: f32 = 10.0;

/// The card piece every list entry draws, so a section reads as one rounded card.
fn card_piece(top: bool, bottom: bool, p: &KanbanPalette) -> gpui::Div {
    div()
        .w_full()
        .bg(p.lane)
        .border_color(p.border)
        .border_l_1()
        .border_r_1()
        .when(top, |this| {
            this.border_t_1()
                .rounded_t(px(SECTION_RADIUS))
                .mt(px(SECTION_GAP))
        })
        .when(bottom, |this| {
            this.border_b_1().rounded_b(px(SECTION_RADIUS)).pb(px(6.0))
        })
}

pub(crate) fn priority_tag(text: &str, p: &KanbanPalette) -> gpui::Div {
    div()
        .flex_none()
        .px(px(6.0))
        .h(px(20.0))
        .flex()
        .items_center()
        .rounded(px(5.0))
        .border_1()
        .border_color(p.border_strong)
        .font_family(crate::app::native_chat::fonts::CHAT_MONO)
        .text_size(px(11.0))
        .text_color(p.foreground)
        .child(text.to_string())
}

pub(crate) fn label_chip(label: &str, p: &KanbanPalette) -> gpui::Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(4.0))
        .px(px(6.0))
        .h(px(20.0))
        .rounded(px(5.0))
        .bg(p.control)
        .text_color(p.muted)
        .child(titlebar_svg_icon("titlebar/tag.svg", 11.0, p.faint))
        .child(label.to_string())
}

fn dot() -> &'static str {
    "·"
}

/// The second line of a row: type, what it waits on, dates, status, labels, comments.
pub(crate) fn meta_line(row: &BeadRowModel, p: &KanbanPalette) -> gpui::Div {
    let mut line = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(6.0))
        .min_w_0()
        .text_size(px(12.0))
        .text_color(p.muted);
    let mut first = true;
    let mut push = |line: gpui::Div, part: AnyElement| -> gpui::Div {
        let line = if first {
            line
        } else {
            line.child(div().text_color(p.faint).child(dot()))
        };
        first = false;
        line.child(part)
    };
    for meta in &row.meta {
        let part = match meta {
            BeadMeta::Type(label) => div().child(label.clone()).into_any_element(),
            BeadMeta::InEpic(title) => div()
                .flex()
                .gap(px(4.0))
                .items_center()
                .child(titlebar_svg_icon("titlebar/stack.svg", 12.0, p.faint))
                .child(format!("In {title}"))
                .into_any_element(),
            BeadMeta::WaitingOn { title, more } => div()
                .flex()
                .gap(px(4.0))
                .items_center()
                .min_w_0()
                .child(titlebar_svg_icon(
                    "titlebar/barrier-block.svg",
                    12.0,
                    p.faint,
                ))
                .child("Waiting on")
                .child(
                    div()
                        .truncate()
                        .text_color(p.foreground)
                        .font_weight(FontWeight::MEDIUM)
                        .child(title.clone()),
                )
                .when(*more > 0, |this| this.child(format!("+{more}")))
                .into_any_element(),
            BeadMeta::Until(date) => div()
                .flex()
                .gap(px(4.0))
                .items_center()
                .child(titlebar_svg_icon("titlebar/clock.svg", 12.0, p.faint))
                .child(format!("Until {date}"))
                .into_any_element(),
            BeadMeta::OnIce => div()
                .flex()
                .gap(px(4.0))
                .items_center()
                .child(titlebar_svg_icon(
                    "titlebar/player-pause.svg",
                    12.0,
                    p.faint,
                ))
                .child("On ice")
                .into_any_element(),
            BeadMeta::Due(date) => div()
                .flex()
                .gap(px(4.0))
                .items_center()
                .child(titlebar_svg_icon("titlebar/clock.svg", 12.0, p.faint))
                .child(format!("Due {date}"))
                .into_any_element(),
            BeadMeta::Status(status) => div()
                .flex()
                .gap(px(4.0))
                .items_center()
                .child(titlebar_svg_icon("titlebar/eye.svg", 12.0, p.faint))
                .child(status.clone())
                .into_any_element(),
        };
        line = push(line, part);
    }
    if !row.labels.is_empty() {
        let labels = div()
            .flex()
            .gap(px(4.0))
            .children(row.labels.iter().map(|label| label_chip(label, p)))
            .into_any_element();
        line = push(line, labels);
    }
    if row.comment_count > 0 {
        let comments = div()
            .flex()
            .gap(px(4.0))
            .items_center()
            .child(titlebar_svg_icon(
                "titlebar/message-circle.svg",
                12.0,
                p.faint,
            ))
            .child(row.comment_count.to_string())
            .into_any_element();
        line = push(line, comments);
    }
    line
}

/// "Claude ● working": the first linked agent, green only while it works.
pub(crate) fn agent_chip(row: &BeadRowModel, p: &KanbanPalette) -> Option<gpui::Div> {
    let agent = row
        .agents
        .iter()
        .find(|agent| agent.state != BeadAgentState::Gone)?;
    let (dot_color, word) = match agent.state {
        BeadAgentState::Working => (gpui::rgb(0x3ccf8e).into(), "working"),
        BeadAgentState::Idle => (p.faint, "idle"),
        BeadAgentState::Sleeping => (p.faint, "asleep"),
        BeadAgentState::Gone => (p.faint, "ended"),
    };
    Some(
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.0))
            .text_size(px(12.0))
            .text_color(p.foreground)
            .child(titlebar_svg_icon("titlebar/robot.svg", 13.0, p.muted))
            .child(agent.agent.clone())
            .child(div().size(px(7.0)).rounded_full().bg(dot_color))
            .child(div().text_color(p.muted).child(word)),
    )
}

impl BeadsView {
    pub(crate) fn render_tree_item(
        &mut self,
        index: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(p) = self.palette.clone() else {
            return div().into_any_element();
        };
        let Some(item) = self.items.get(index).copied() else {
            return div().into_any_element();
        };
        match item {
            TreeItem::Header { section } => {
                let section = self.tree.sections[section].clone();
                let bottom = section.collapsed;
                card_piece(true, bottom, &p)
                    .child(self.render_section_header(&section, &p, cx))
                    .into_any_element()
            }
            TreeItem::Row {
                section,
                index,
                done,
                last,
            } => {
                let section_model = &self.tree.sections[section];
                let row = if done {
                    section_model.done_rows[index].clone()
                } else {
                    section_model.rows[index].clone()
                };
                let decision = matches!(section_model.kind, BeadsSectionKind::NeedsDecision);
                let indent = matches!(section_model.kind, BeadsSectionKind::Epic(_));
                card_piece(false, last, &p)
                    .px(px(6.0))
                    .child(self.render_row(&row, decision, indent, &p, cx))
                    .into_any_element()
            }
            TreeItem::DoneFold { section, last } => {
                let section_model = &self.tree.sections[section];
                let key = section_model.key.clone();
                let open = section_model.done_open;
                let count = section_model.done_count;
                let hover = p.card_hover;
                card_piece(false, last, &p)
                    .px(px(6.0))
                    .child(
                        div()
                            .id(gpui::SharedString::from(format!("beads-done-{key}")))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .h(px(32.0))
                            .pl(px(28.0))
                            .rounded(px(8.0))
                            .cursor_pointer()
                            .text_size(px(13.0))
                            .text_color(p.muted)
                            .hover(move |style| style.bg(hover))
                            .child(titlebar_svg_icon(
                                if open {
                                    "titlebar/chevron-down.svg"
                                } else {
                                    "titlebar/chevron-right.svg"
                                },
                                13.0,
                                p.faint,
                            ))
                            .child(titlebar_svg_icon(
                                "titlebar/circle-check.svg",
                                14.0,
                                p.faint,
                            ))
                            .child(format!("{count} done"))
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.toggle_done_fold(&key, cx)),
                            ),
                    )
                    .into_any_element()
            }
        }
    }

    fn render_section_header(
        &mut self,
        section: &BeadsSection,
        p: &KanbanPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = section.key.clone();
        match &section.kind {
            BeadsSectionKind::Epic(epic) => {
                let epic = (**epic).clone();
                let summary = section.summary.clone();
                let progress = summary
                    .as_ref()
                    .map(|summary| {
                        if summary.total == 0 {
                            0.0
                        } else {
                            summary.done as f32 / summary.total as f32
                        }
                    })
                    .unwrap_or(0.0);
                let hovered = self.hovered_row.as_deref() == Some(epic.id.as_str());
                let epic_id = epic.id.clone();
                let hover_id = epic.id.clone();
                let add_id = epic.id.clone();
                let menu_id = epic.id.clone();
                let collapse_key = key.clone();
                let hover = p.card_hover;
                let can_create_child = self.has_capability(
                    ghostex_gx_protocol::beads_view::capability::CREATE_TYPE_PARENT,
                );
                div()
                    .id(gpui::SharedString::from(format!("beads-epic-{key}")))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .px(px(12.0))
                    .py(px(10.0))
                    .rounded_t(px(SECTION_RADIUS))
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        this.set_hovered(&hover_id, *hovered, cx)
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_details(&epic_id, window, cx)
                    }))
                    .child(
                        div()
                            .id(gpui::SharedString::from(format!("beads-epic-fold-{key}")))
                            .flex_none()
                            .cursor_pointer()
                            .child(titlebar_svg_icon(
                                if section.collapsed {
                                    "titlebar/chevron-right.svg"
                                } else {
                                    "titlebar/chevron-down.svg"
                                },
                                14.0,
                                p.muted,
                            ))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.toggle_collapsed(&collapse_key, cx)
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(titlebar_svg_icon("titlebar/stack.svg", 14.0, p.muted))
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(14.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(epic.title.clone()),
                                    )
                                    .when(hovered, |this| {
                                        this.child(
                                            div()
                                                .font_family(
                                                    crate::app::native_chat::fonts::CHAT_MONO,
                                                )
                                                .text_size(px(11.0))
                                                .text_color(p.faint)
                                                .child(epic.id.clone()),
                                        )
                                    }),
                            )
                            .when_some(summary, |this, summary| {
                                this.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(10.0))
                                        .text_size(px(12.0))
                                        .text_color(p.muted)
                                        .child(
                                            div()
                                                .flex_none()
                                                .w(px(120.0))
                                                .h(px(4.0))
                                                .rounded_full()
                                                .bg(p.control_hover)
                                                .child(
                                                    div()
                                                        .h_full()
                                                        .rounded_full()
                                                        .bg(p.muted)
                                                        .w(relative(progress)),
                                                ),
                                        )
                                        .child(div().truncate().child(summary.text)),
                                )
                            }),
                    )
                    .when(hovered, |this| {
                        this.child(
                            div()
                                .flex()
                                .flex_none()
                                .gap(px(4.0))
                                .when(can_create_child, |this| {
                                    this.child(
                                        super::render::small_button(
                                            format!("beads-add-child-{add_id}"),
                                            Some("titlebar/plus.svg"),
                                            Some("Add child"),
                                            p,
                                        )
                                        .on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                cx.stop_propagation();
                                                this.open_create(
                                                    Some(add_id.clone()),
                                                    None,
                                                    window,
                                                    cx,
                                                )
                                            }),
                                        ),
                                    )
                                })
                                .child(
                                    super::render::small_button(
                                        format!("beads-epic-menu-{menu_id}"),
                                        Some("titlebar/dots.svg"),
                                        None,
                                        p,
                                    )
                                    .on_click(cx.listener(
                                        move |this, event: &ClickEvent, _, cx| {
                                            cx.stop_propagation();
                                            this.open_menu(
                                                BeadsMenu::new(
                                                    BeadsMenuKind::Row(menu_id.clone()),
                                                    event.position(),
                                                ),
                                                cx,
                                            )
                                        },
                                    )),
                                ),
                        )
                    })
                    // CDXC:ProjectBoard 2026-11-13 DECISION: User: the time next to an epic's priority is not editable and not relevant, so epic rows show only the priority.
                    .when_some(epic.priority.clone(), |this, priority| {
                        this.child(priority_tag(&priority, p))
                    })
                    .into_any_element()
            }
            BeadsSectionKind::NeedsDecision | BeadsSectionKind::NotInEpic => {
                let icon = if matches!(section.kind, BeadsSectionKind::NeedsDecision) {
                    "titlebar/help-circle.svg"
                } else {
                    "titlebar/list.svg"
                };
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(14.0))
                    .pt(px(12.0))
                    .pb(px(6.0))
                    .child(titlebar_svg_icon(icon, 14.0, p.muted))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(section.title.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(p.faint)
                            .child(section.count.to_string()),
                    )
                    .into_any_element()
            }
        }
    }

    pub(crate) fn set_hovered(&mut self, id: &str, hovered: bool, cx: &mut Context<Self>) {
        let next = if hovered {
            Some(id.to_string())
        } else if self.hovered_row.as_deref() == Some(id) {
            None
        } else {
            return;
        };
        if self.hovered_row != next {
            self.hovered_row = next;
            cx.notify();
        }
    }

    fn render_row(
        &mut self,
        row: &BeadRowModel,
        decision: bool,
        indent: bool,
        p: &KanbanPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hovered = self.hovered_row.as_deref() == Some(row.id.as_str());
        let narrow = self.is_narrow();
        let selected = self
            .details
            .as_ref()
            .is_some_and(|details| details.id == row.id);
        let id = row.id.clone();
        let hover_id = row.id.clone();
        let claim_id = row.id.clone();
        let menu_id = row.id.clone();
        let answer_id = row.id.clone();
        let hover = p.card_hover;
        let can_claim = row.claimable
            && self.has_capability(ghostex_gx_protocol::beads_view::capability::CLAIM);
        let agent = agent_chip(row, p);
        div()
            .id(gpui::SharedString::from(format!("beads-row-{}", row.id)))
            .flex()
            .items_center()
            .gap(px(10.0))
            .pl(px(if indent { 34.0 } else { 10.0 }))
            .pr(px(8.0))
            .py(px(8.0))
            .rounded(px(9.0))
            .cursor_pointer()
            .when(selected, |this| {
                this.bg(p.card_hover)
                    .border_1()
                    .border_color(p.border_strong)
            })
            .when(!selected, |this| this.hover(move |style| style.bg(hover)))
            .when(row.closed, |this| this.opacity(0.6))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                this.set_hovered(&hover_id, *hovered, cx)
            }))
            .on_click(cx.listener(move |this, _, window, cx| this.open_details(&id, window, cx)))
            .child(
                div()
                    .flex_none()
                    .pt(px(2.0))
                    .self_start()
                    .child(titlebar_svg_icon(glyph_icon(row.glyph), 15.0, p.muted)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(4.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(14.0))
                                    .when(row.closed, |this| this.line_through())
                                    .child(row.title.clone()),
                            )
                            .when(hovered, |this| {
                                this.child(
                                    div()
                                        .flex_none()
                                        .font_family(crate::app::native_chat::fonts::CHAT_MONO)
                                        .text_size(px(11.0))
                                        .text_color(p.faint)
                                        .child(row.id.clone()),
                                )
                            }),
                    )
                    .child(meta_line(row, p)),
            )
            .when(decision && !row.closed, |this| {
                this.child(
                    super::render::small_button(
                        format!("beads-answer-{answer_id}"),
                        Some("titlebar/message.svg"),
                        Some("Answer"),
                        p,
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.open_details(&answer_id, window, cx);
                        this.start_answer(window, cx);
                    })),
                )
            })
            .when(hovered && !decision, |this| {
                this.child(
                    div()
                        .flex()
                        .flex_none()
                        .gap(px(4.0))
                        .when(can_claim, |this| {
                            this.child(
                                super::render::small_button(
                                    format!("beads-claim-{claim_id}"),
                                    Some("titlebar/user.svg"),
                                    Some("Claim"),
                                    p,
                                )
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.simple_action(
                                            "claim",
                                            &claim_id,
                                            serde_json::json!({}),
                                            "Couldn't claim this bead",
                                            window,
                                            cx,
                                        );
                                    },
                                )),
                            )
                        })
                        .child(
                            super::render::small_button(
                                format!("beads-row-menu-{menu_id}"),
                                Some("titlebar/dots.svg"),
                                None,
                                p,
                            )
                            .on_click(cx.listener(
                                move |this, event: &ClickEvent, _, cx| {
                                    cx.stop_propagation();
                                    this.open_menu(
                                        BeadsMenu::new(
                                            BeadsMenuKind::Row(menu_id.clone()),
                                            event.position(),
                                        ),
                                        cx,
                                    )
                                },
                            )),
                        ),
                )
            })
            .when(!(hovered && !decision), |this| this.children(agent))
            .when_some(row.priority.clone(), |this, priority| {
                this.child(priority_tag(&priority, p))
            })
            // In a narrow panel the title gets the room the age took.
            .when(!narrow, |this| {
                this.child(
                    div()
                        .flex_none()
                        .w(px(30.0))
                        .text_right()
                        .text_size(px(12.0))
                        .text_color(p.faint)
                        .child(row.changed.clone()),
                )
            })
            .into_any_element()
    }
}
