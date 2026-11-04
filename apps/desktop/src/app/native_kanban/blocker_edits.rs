//! Adding and removing a ticket's blockers from the ticket panel. Edit ticket writes each link to
//! bd at once (`dep add` / `dep remove`, the same Beads store the agents write with) and shows it
//! on the board before bd answers; New ticket holds them until the ticket exists.

use gpui::{Context, Window};
use serde_json::json;

use super::beads::beads_call;
use super::blockers::{
    BlockerDirection, BlockerEdge, add_local_blocker, blocked_by_links, blocker_candidates,
    blocker_refusal, blocks_links, plain_blocker_error, remove_local_blocker,
    restore_local_blocker,
};
use super::model::BLOCKS_DEPENDENCY;
use super::state::{KanbanFormMode, KanbanPanel, KanbanRefreshMode, KanbanTicketForm};
use crate::GhostexGpuiApp;

/// How many cards the blocker search lists.
pub(crate) const BLOCKER_RESULT_LIMIT: usize = 6;

impl GhostexGpuiApp {
    fn native_kanban_ticket_form_mut(&mut self) -> Option<&mut KanbanTicketForm> {
        match self.native_kanban.panel.as_mut() {
            Some(KanbanPanel::Ticket(form)) => Some(form),
            _ => None,
        }
    }

    /// The card the open panel edits (`None` for New ticket).
    fn native_kanban_form_card_id(&self) -> Option<String> {
        match self.native_kanban.panel.as_ref() {
            Some(KanbanPanel::Ticket(form)) => match &form.mode {
                KanbanFormMode::Edit { ticket_id } => Some(ticket_id.clone()),
                KanbanFormMode::New => None,
            },
            _ => None,
        }
    }

    /// What the panel calls a card in a message: its board id, or "This card" for a ticket not
    /// created yet.
    pub(crate) fn native_kanban_card_name(&self, id: &str) -> String {
        if id.is_empty() {
            return "This card".to_string();
        }
        self.native_kanban
            .ticket(id)
            .map(|ticket| ticket.display_id.clone())
            .unwrap_or_else(|| id.to_string())
    }

    /// The ids already in the list the blocker search adds to, so it does not offer them again.
    pub(crate) fn native_kanban_linked_blocker_ids(&self, form: &KanbanTicketForm) -> Vec<String> {
        let direction = form.blocker_direction;
        match &form.mode {
            KanbanFormMode::Edit { ticket_id } => {
                let links = match direction {
                    BlockerDirection::BlockedBy => {
                        blocked_by_links(&self.native_kanban.issues, ticket_id)
                    }
                    BlockerDirection::Blocks => blocks_links(&self.native_kanban.issues, ticket_id),
                };
                links.into_iter().map(|link| link.other_id).collect()
            }
            KanbanFormMode::New => form
                .staged_blockers
                .iter()
                .filter_map(|edge| match direction {
                    BlockerDirection::BlockedBy if edge.waiter.is_empty() => {
                        Some(edge.blocker.clone())
                    }
                    BlockerDirection::Blocks if edge.blocker.is_empty() => {
                        Some(edge.waiter.clone())
                    }
                    _ => None,
                })
                .collect(),
        }
    }

    pub(crate) fn native_kanban_set_blocker_direction(
        &mut self,
        direction: BlockerDirection,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.native_kanban_ticket_form_mut() {
            form.blocker_direction = direction;
            form.blocker_message = None;
        }
        self.native_kanban_notify(cx);
    }

    /// Enter in the blocker search adds its first result.
    pub(crate) fn native_kanban_add_first_blocker_match(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(KanbanPanel::Ticket(form)) = self.native_kanban.panel.as_ref() else {
            return;
        };
        let query = form.blocker_query.read(cx).value().to_string();
        let card_id = self.native_kanban_form_card_id();
        let linked = self.native_kanban_linked_blocker_ids(form);
        let Some(first) = blocker_candidates(
            &self.native_kanban.tickets,
            &query,
            card_id.as_deref(),
            &linked,
            1,
        )
        .first()
        .map(|ticket| ticket.issue.id.clone()) else {
            return;
        };
        self.native_kanban_add_blocker(&first, window, cx);
    }

    /// Links `other_id` to the open ticket in the search's direction, unless bd would refuse it.
    pub(crate) fn native_kanban_add_blocker(
        &mut self,
        other_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(KanbanPanel::Ticket(form)) = self.native_kanban.panel.as_ref() else {
            return;
        };
        let card_id = self.native_kanban_form_card_id().unwrap_or_default();
        let edge = match form.blocker_direction {
            BlockerDirection::BlockedBy => BlockerEdge {
                waiter: card_id.clone(),
                blocker: other_id.to_string(),
            },
            BlockerDirection::Blocks => BlockerEdge {
                waiter: other_id.to_string(),
                blocker: card_id.clone(),
            },
        };
        let staged = form.staged_blockers.clone();
        let refusal = blocker_refusal(&self.native_kanban.issues, &staged, &edge, |id| {
            self.native_kanban_card_name(id)
        });
        let query = form.blocker_query.clone();
        let editing = matches!(form.mode, KanbanFormMode::Edit { .. });
        if let Some(message) = refusal {
            if let Some(form) = self.native_kanban_ticket_form_mut() {
                form.blocker_message = Some(message);
            }
            self.native_kanban_notify(cx);
            return;
        }
        query.update(cx, |input, cx| input.set_value("", window, cx));
        if let Some(form) = self.native_kanban_ticket_form_mut() {
            form.blocker_message = None;
            if !editing && !form.staged_blockers.contains(&edge) {
                form.staged_blockers.push(edge.clone());
            }
        }
        if editing {
            self.native_kanban_write_blocker(edge, true, cx);
        }
        self.native_kanban_notify(cx);
    }

    /// Unlinks `other_id` from the open ticket: `direction` says which list it is in.
    pub(crate) fn native_kanban_remove_blocker(
        &mut self,
        direction: BlockerDirection,
        other_id: &str,
        cx: &mut Context<Self>,
    ) {
        let card_id = self.native_kanban_form_card_id().unwrap_or_default();
        let edge = match direction {
            BlockerDirection::BlockedBy => BlockerEdge {
                waiter: card_id.clone(),
                blocker: other_id.to_string(),
            },
            BlockerDirection::Blocks => BlockerEdge {
                waiter: other_id.to_string(),
                blocker: card_id.clone(),
            },
        };
        if card_id.is_empty() {
            if let Some(form) = self.native_kanban_ticket_form_mut() {
                form.staged_blockers.retain(|staged| *staged != edge);
                form.blocker_message = None;
            }
        } else {
            self.native_kanban_write_blocker(edge, false, cx);
        }
        self.native_kanban_notify(cx);
    }

    /// One `dep add` (or `dep remove`) for Edit ticket: shown at once, put back if bd refuses.
    fn native_kanban_write_blocker(
        &mut self,
        edge: BlockerEdge,
        add: bool,
        cx: &mut Context<Self>,
    ) {
        let removed = if add {
            if !add_local_blocker(&mut self.native_kanban.issues, &edge) {
                return;
            }
            None
        } else {
            match remove_local_blocker(&mut self.native_kanban.issues, &edge) {
                Some(removed) => Some(removed),
                None => return,
            }
        };
        if let Some(form) = self.native_kanban_ticket_form_mut() {
            form.blocker_message = None;
        }
        self.native_kanban_rebuild_tickets();
        let request = if add {
            json!({
                "action": "depAdd",
                "issueId": edge.waiter,
                "dependsOnId": edge.blocker,
                "depType": BLOCKS_DEPENDENCY,
            })
        } else {
            json!({ "action": "depRemove", "issueId": edge.waiter, "dependsOnId": edge.blocker })
        };
        self.native_kanban_spawn(
            move |context| beads_call(context, request),
            move |this, result, cx| {
                match result {
                    Ok(_) => this.native_kanban_refresh(KanbanRefreshMode::Mutation, cx),
                    Err(error) => {
                        let issues = &mut this.native_kanban.issues;
                        match removed {
                            Some(dependency) => restore_local_blocker(issues, &edge, dependency),
                            None => {
                                remove_local_blocker(issues, &edge);
                            }
                        }
                        this.native_kanban_rebuild_tickets();
                        let message = plain_blocker_error(&error);
                        let card = this.native_kanban_form_card_id();
                        let open_here = card.as_deref() == Some(edge.waiter.as_str())
                            || card.as_deref() == Some(edge.blocker.as_str());
                        match this.native_kanban_ticket_form_mut() {
                            Some(form) if open_here => form.blocker_message = Some(message),
                            _ => {
                                let title = if add {
                                    "Could not add the blocker"
                                } else {
                                    "Could not remove the blocker"
                                };
                                this.native_kanban_toast(title, &message, cx);
                            }
                        }
                    }
                }
                this.native_kanban_notify(cx);
            },
            cx,
        );
    }

    /// A New ticket's held links, written once bd has given the ticket its id.
    pub(crate) fn native_kanban_write_staged_blockers(
        &mut self,
        issue_id: &str,
        staged: Vec<BlockerEdge>,
        cx: &mut Context<Self>,
    ) {
        if staged.is_empty() {
            return;
        }
        let edges = staged
            .into_iter()
            .map(|edge| BlockerEdge {
                waiter: if edge.waiter.is_empty() {
                    issue_id.to_string()
                } else {
                    edge.waiter
                },
                blocker: if edge.blocker.is_empty() {
                    issue_id.to_string()
                } else {
                    edge.blocker
                },
            })
            .collect::<Vec<_>>();
        for edge in &edges {
            add_local_blocker(&mut self.native_kanban.issues, edge);
        }
        self.native_kanban_rebuild_tickets();
        self.native_kanban_spawn(
            move |context| {
                let mut failures = Vec::new();
                for edge in &edges {
                    if let Err(error) = beads_call(
                        context,
                        json!({
                            "action": "depAdd",
                            "issueId": edge.waiter,
                            "dependsOnId": edge.blocker,
                            "depType": BLOCKS_DEPENDENCY,
                        }),
                    ) {
                        failures.push(plain_blocker_error(&error));
                    }
                }
                failures
            },
            |this, failures, cx| {
                if let Some(first) = failures.first() {
                    this.native_kanban_toast("Some blockers were not added", first, cx);
                }
                this.native_kanban_refresh(KanbanRefreshMode::Mutation, cx);
                this.native_kanban_notify(cx);
            },
            cx,
        );
    }
}
