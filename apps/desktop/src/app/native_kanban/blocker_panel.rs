//! The ticket panel's Blockers section: the cards this one waits on, the cards waiting on it, and
//! a card search that adds to either list.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};

use super::blocker_edits::BLOCKER_RESULT_LIMIT;
use super::blockers::{
    BlockerDirection, BlockerLink, blocked_by_links, blocker_candidates, blocks_links,
};
use super::model::{BLOCKS_DEPENDENCY, BoardTicket, board_status_label};
use super::palette::KanbanPalette;
use super::state::{KanbanFormMode, KanbanTicketForm};
use super::widgets::{choice_pill, text_field};
use crate::GhostexGpuiApp;
use crate::app::helpers::titlebar_svg_icon;

impl GhostexGpuiApp {
    /// CDXC:ProjectBoard 2026-10-31 DECISION: User (issue #358, "Build it"): a ticket's blockers are seen and edited when the ticket is opened or created: which cards block it and which it blocks, add one by searching cards, remove one, and open a blocker card from there; edits go through bd, the store the agents set them in, so what an agent sets shows here too.
    pub(crate) fn render_native_kanban_blockers(
        &self,
        form: &KanbanTicketForm,
        p: &KanbanPalette,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let editing = matches!(form.mode, KanbanFormMode::Edit { .. });
        let (blocked_by, blocks) = match &form.mode {
            KanbanFormMode::Edit { ticket_id } => (
                blocked_by_links(&self.native_kanban.issues, ticket_id),
                blocks_links(&self.native_kanban.issues, ticket_id),
            ),
            KanbanFormMode::New => {
                let staged = |pick: fn(&super::blockers::BlockerEdge) -> Option<&String>| {
                    form.staged_blockers
                        .iter()
                        .filter_map(pick)
                        .map(|id| BlockerLink {
                            other_id: id.clone(),
                            kind: BLOCKS_DEPENDENCY.to_string(),
                        })
                        .collect::<Vec<_>>()
                };
                (
                    staged(|edge| edge.waiter.is_empty().then_some(&edge.blocker)),
                    staged(|edge| edge.blocker.is_empty().then_some(&edge.waiter)),
                )
            }
        };
        let list = |direction: BlockerDirection,
                    heading: &'static str,
                    empty: &'static str,
                    links: Vec<BlockerLink>,
                    cx: &mut Context<Self>| {
            let rows = links
                .into_iter()
                .map(|link| self.render_native_kanban_blocker_row(direction, link, editing, p, cx))
                .collect::<Vec<_>>();
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(div().text_size(px(12.0)).text_color(p.muted).child(heading))
                .when(rows.is_empty(), |this| {
                    this.child(div().text_size(px(12.0)).text_color(p.faint).child(empty))
                })
                .children(rows)
        };
        let query = form.blocker_query.read(cx).value().to_string();
        let card_id = match &form.mode {
            KanbanFormMode::Edit { ticket_id } => Some(ticket_id.as_str()),
            KanbanFormMode::New => None,
        };
        let linked = self.native_kanban_linked_blocker_ids(form);
        let results = blocker_candidates(
            &self.native_kanban.tickets,
            &query,
            card_id,
            &linked,
            BLOCKER_RESULT_LIMIT,
        )
        .into_iter()
        .map(|ticket| self.render_native_kanban_blocker_result(ticket, p, cx))
        .collect::<Vec<_>>();
        let searching = !query.trim().is_empty();
        let direction = form.blocker_direction;
        let direction_pill =
            |value: BlockerDirection, label: &'static str, cx: &mut Context<Self>| {
                choice_pill(
                    ElementId::Name(format!("kanban-blocker-direction-{label}").into()),
                    label.into(),
                    direction == value,
                    p,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.native_kanban_set_blocker_direction(value, cx);
                }))
            };
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(list(
                BlockerDirection::BlockedBy,
                "Blocked by",
                "Nothing blocks this card.",
                blocked_by,
                cx,
            ))
            .child(list(
                BlockerDirection::Blocks,
                "Blocks",
                "This card doesn't block any card.",
                blocks,
                cx,
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(div().text_size(px(12.0)).text_color(p.muted).child("Add"))
                            .child(direction_pill(
                                BlockerDirection::BlockedBy,
                                "Blocked by",
                                cx,
                            ))
                            .child(direction_pill(BlockerDirection::Blocks, "Blocks", cx)),
                    )
                    .child(text_field(&form.blocker_query, p, window, cx))
                    .when(searching && results.is_empty(), |this| {
                        this.child(
                            div()
                                .text_size(px(12.0))
                                .text_color(p.faint)
                                .child("No other card matches."),
                        )
                    })
                    .children(results)
                    .when_some(form.blocker_message.clone(), |this, message| {
                        this.child(
                            div()
                                .text_size(px(12.0))
                                .text_color(p.danger)
                                .child(message),
                        )
                    }),
            )
    }

    /// One linked card: board id, title and lane; a click opens it (Edit ticket only, since New
    /// ticket would lose its draft) and the x unlinks it.
    fn render_native_kanban_blocker_row(
        &self,
        direction: BlockerDirection,
        link: BlockerLink,
        openable: bool,
        p: &KanbanPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ticket = self.native_kanban.ticket(&link.other_id);
        let open = openable && ticket.is_some();
        let (display_id, title, lane) = match ticket {
            Some(ticket) => (
                ticket.display_id.clone(),
                ticket.issue.title.clone(),
                Some(board_status_label(
                    &ticket.board_status,
                    &self.native_kanban.columns,
                )),
            ),
            // An external reference or a card no longer on the board.
            None => (link.other_id.clone(), "Not on this board".to_string(), None),
        };
        let kind = (link.kind != BLOCKS_DEPENDENCY).then(|| link.kind.clone());
        let slug = match direction {
            BlockerDirection::BlockedBy => "by",
            BlockerDirection::Blocks => "blocks",
        };
        let hover = p.card_hover;
        let remove_hover = p.control_hover;
        let open_id = link.other_id.clone();
        let remove_id = link.other_id.clone();
        div()
            .id(ElementId::Name(
                format!("kanban-blocker-{slug}-{}", link.other_id).into(),
            ))
            .flex()
            .items_center()
            .gap(px(8.0))
            .pl(px(10.0))
            .pr(px(4.0))
            .py(px(4.0))
            .rounded(px(8.0))
            .bg(p.card)
            .border_1()
            .border_color(p.border)
            .when(open, |this| {
                this.cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.native_kanban_open_ticket(&open_id, window, cx);
                    }))
            })
            .child(
                div()
                    .flex_none()
                    .text_size(px(12.0))
                    .text_color(p.muted)
                    .child(display_id),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(13.0))
                    .text_color(if ticket.is_some() {
                        p.foreground
                    } else {
                        p.faint
                    })
                    .child(title),
            )
            .when_some(kind, |this, kind| {
                this.child(
                    div()
                        .flex_none()
                        .px(px(6.0))
                        .rounded_full()
                        .border_1()
                        .border_color(p.border)
                        .text_size(px(11.0))
                        .text_color(p.muted)
                        .child(kind),
                )
            })
            .when_some(lane, |this, lane| {
                this.child(
                    div()
                        .flex_none()
                        .text_size(px(11.0))
                        .text_color(p.faint)
                        .child(lane),
                )
            })
            .child(
                div()
                    .id(ElementId::Name(
                        format!("kanban-blocker-remove-{slug}-{}", link.other_id).into(),
                    ))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(24.0))
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .hover(move |style| style.bg(remove_hover))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.native_kanban_remove_blocker(direction, &remove_id, cx);
                    }))
                    .child(titlebar_svg_icon("titlebar/x.svg", 12.0, p.muted)),
            )
            .into_any_element()
    }

    /// One card the blocker search found; a click adds it to the chosen list.
    fn render_native_kanban_blocker_result(
        &self,
        ticket: &BoardTicket,
        p: &KanbanPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hover = p.control_hover;
        let id = ticket.issue.id.clone();
        div()
            .id(ElementId::Name(
                format!("kanban-blocker-result-{}", ticket.issue.id).into(),
            ))
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(10.0))
            .py(px(5.0))
            .rounded(px(8.0))
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.native_kanban_add_blocker(&id, window, cx);
            }))
            .child(titlebar_svg_icon("titlebar/plus.svg", 12.0, p.muted))
            .child(
                div()
                    .flex_none()
                    .text_size(px(12.0))
                    .text_color(p.muted)
                    .child(ticket.display_id.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(13.0))
                    .text_color(p.foreground)
                    .child(ticket.issue.title.clone()),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(11.0))
                    .text_color(p.faint)
                    .child(board_status_label(
                        &ticket.board_status,
                        &self.native_kanban.columns,
                    )),
            )
            .into_any_element()
    }
}
