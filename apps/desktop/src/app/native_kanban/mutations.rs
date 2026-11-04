//! Board edits: moving a card between lanes, saving and creating tickets, deleting them, and
//! editing the board's extra columns. Each updates the board locally first and lets bd catch up in
//! the background, the way the React board does.

use std::time::Duration;

use gpui::{Context, Window};
use serde_json::json;

use super::beads::{
    KanbanCreateDraft, KanbanSaveDraft, beads_call, create_issue, generate_and_store_title,
    persist_ticket,
};
use super::model::{BeadsIssue, board_status_beads_value, board_status_for, tshirt_to_estimate};
use super::state::{KanbanFormMode, KanbanPanel, KanbanPendingMove, KanbanRefreshMode};
use super::text::draft_title;
use crate::GhostexGpuiApp;

/// `PROJECT_BOARD_GENERATED_TITLE_DELAY_MS`: title generation waits until the create has settled.
const GENERATED_TITLE_DELAY: Duration = Duration::from_secs(2);

impl GhostexGpuiApp {
    pub(crate) fn native_kanban_set_local_status(&mut self, ticket_id: &str, beads_status: &str) {
        let state = &mut self.native_kanban;
        if let Some(issue) = state.issues.iter_mut().find(|issue| issue.id == ticket_id) {
            issue.status = beads_status.to_string();
        }
        let board_status = board_status_for(beads_status, &state.columns);
        if let Some(KanbanPanel::Ticket(form)) = state.panel.as_mut()
            && form.mode
                == (KanbanFormMode::Edit {
                    ticket_id: ticket_id.to_string(),
                })
        {
            form.status = board_status;
        }
        self.native_kanban_rebuild_tickets();
    }

    fn native_kanban_upsert_local_issue(&mut self, issue: BeadsIssue) {
        let issues = &mut self.native_kanban.issues;
        match issues.iter_mut().find(|candidate| candidate.id == issue.id) {
            Some(existing) => *existing = issue,
            None => issues.push(issue),
        }
        self.native_kanban_rebuild_tickets();
    }

    /// `moveTicket`: drag between lanes.
    pub(crate) fn native_kanban_move_ticket(
        &mut self,
        ticket_id: &str,
        status_key: &str,
        cx: &mut Context<Self>,
    ) {
        let state = &mut self.native_kanban;
        let Some(column) = state
            .columns
            .iter()
            .find(|column| column.key == status_key)
            .cloned()
        else {
            return;
        };
        let Some(previous_status) = state
            .ticket(ticket_id)
            .filter(|ticket| ticket.board_status != status_key)
            .map(|ticket| ticket.issue.status.clone())
        else {
            return;
        };
        state.move_serial += 1;
        let token = state.move_serial;
        state.pending_moves.insert(
            ticket_id.to_string(),
            KanbanPendingMove {
                beads_status: column.beads_status.clone(),
                token,
            },
        );
        self.native_kanban_set_local_status(ticket_id, &column.beads_status);
        let issue_id = ticket_id.to_string();
        let status = column.beads_status.clone();
        let ticket_id = ticket_id.to_string();
        self.native_kanban_spawn(
            move |context| {
                beads_call(
                    context,
                    json!({ "action": "updateStatus", "issueId": issue_id, "status": status }),
                )
            },
            move |this, result, cx| {
                if this
                    .native_kanban
                    .pending_moves
                    .get(&ticket_id)
                    .is_none_or(|pending| pending.token != token)
                {
                    return;
                }
                this.native_kanban.pending_moves.remove(&ticket_id);
                match result {
                    Ok(_) => this.native_kanban_refresh(KanbanRefreshMode::Background, cx),
                    Err(error) => {
                        this.native_kanban_set_local_status(&ticket_id, &previous_status);
                        this.native_kanban_toast("Could not move the ticket", &error, cx);
                        this.native_kanban_notify(cx);
                    }
                }
            },
            cx,
        );
        self.native_kanban_notify(cx);
    }

    /// Delete from the card menu or the ticket panel, after the user confirmed it.
    pub(crate) fn native_kanban_confirm_delete(
        &mut self,
        ticket_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ticket) = self.native_kanban.ticket(ticket_id) else {
            return;
        };
        let detail = format!("{} will be deleted from Beads.", ticket.display_id);
        let answer = window.prompt(
            gpui::PromptLevel::Warning,
            "Delete ticket?",
            Some(&detail),
            &["Cancel", "Delete"],
            cx,
        );
        let ticket_id = ticket_id.to_string();
        cx.spawn(async move |this, cx| {
            if answer.await == Ok(1) {
                let _ = this.update(cx, |this, cx| {
                    this.native_kanban_delete_ticket(&ticket_id, cx)
                });
            }
        })
        .detach();
    }

    /// `deleteTicket`: the card leaves at once and returns if bd refuses.
    pub(crate) fn native_kanban_delete_ticket(&mut self, ticket_id: &str, cx: &mut Context<Self>) {
        let state = &mut self.native_kanban;
        let Some(index) = state.issues.iter().position(|issue| issue.id == ticket_id) else {
            return;
        };
        let removed = state.issues.remove(index);
        if let Some(KanbanPanel::Ticket(form)) = state.panel.as_ref()
            && form.mode
                == (KanbanFormMode::Edit {
                    ticket_id: ticket_id.to_string(),
                })
        {
            state.panel = None;
        }
        self.native_kanban_rebuild_tickets();
        let issue_id = ticket_id.to_string();
        self.native_kanban_spawn(
            move |context| beads_call(context, json!({ "action": "delete", "issueId": issue_id })),
            move |this, result, cx| match result {
                Ok(_) => this.native_kanban_refresh(KanbanRefreshMode::Mutation, cx),
                Err(error) => {
                    this.native_kanban_upsert_local_issue(removed);
                    this.native_kanban_toast("Could not delete the ticket", &error, cx);
                    this.native_kanban_notify(cx);
                }
            },
            cx,
        );
        self.native_kanban_notify(cx);
    }

    /// `saveTicketDetail`: the panel closes at once and the card shows the edit; bd follows.
    pub(crate) fn native_kanban_save_form(&mut self, cx: &mut Context<Self>) {
        let Some(KanbanPanel::Ticket(form)) = self.native_kanban.panel.as_ref() else {
            return;
        };
        let (KanbanFormMode::Edit { ticket_id }, Some(ticket)) = (&form.mode, form.ticket.as_ref())
        else {
            return;
        };
        let title = form.title.read(cx).value().trim().to_string();
        if title.is_empty() {
            return;
        }
        let columns = &self.native_kanban.columns;
        let status_changed = form.status != ticket.board_status;
        let draft = KanbanSaveDraft {
            issue_id: ticket_id.clone(),
            title,
            description: form.description.read(cx).value().to_string(),
            priority: form.priority.clone(),
            estimate: tshirt_to_estimate(form.tshirt),
            labels: form.labels.clone(),
            status: status_changed.then(|| board_status_beads_value(&form.status, columns)),
            comment: form.comment.read(cx).value().to_string(),
        };
        let mut optimistic = self
            .native_kanban
            .issues
            .iter()
            .find(|issue| issue.id == draft.issue_id)
            .cloned()
            .unwrap_or_else(|| ticket.issue.clone());
        optimistic.title = draft.title.clone();
        optimistic.description = draft.description.clone();
        optimistic.priority = draft.priority.parse().ok().or(optimistic.priority);
        if draft.estimate.is_some() {
            optimistic.estimate = draft.estimate;
        }
        if !draft.labels.is_empty() {
            optimistic.labels = draft.labels.clone();
        }
        optimistic.status = board_status_beads_value(&form.status, columns);
        self.native_kanban.panel = None;
        self.native_kanban.error = None;
        self.native_kanban_upsert_local_issue(optimistic);
        self.native_kanban_spawn(
            move |context| persist_ticket(context, &draft),
            |this, result, cx| {
                if let Err(error) = result {
                    this.native_kanban.error = Some(error.clone());
                    this.native_kanban_toast("Ticket save failed", &error, cx);
                    this.native_kanban_notify(cx);
                }
                this.native_kanban_refresh(KanbanRefreshMode::Background, cx);
            },
            cx,
        );
        self.native_kanban_notify(cx);
    }

    /// `createTicket`: create in bd, show the card as soon as bd returns its id, then settle the
    /// status and labels, start work if asked, and replace a draft title with a generated one.
    pub(crate) fn native_kanban_create_from_form(
        &mut self,
        start_after_create: bool,
        cx: &mut Context<Self>,
    ) {
        if self.native_kanban.create_in_flight {
            return;
        }
        let Some(KanbanPanel::Ticket(form)) = self.native_kanban.panel.as_ref() else {
            return;
        };
        if form.mode != KanbanFormMode::New {
            return;
        }
        let prompt = form.description.read(cx).value().trim().to_string();
        if prompt.is_empty()
            || (start_after_create && self.native_kanban.conversation.agents.is_empty())
        {
            return;
        }
        let requested_title = form.title.read(cx).value().trim().to_string();
        let generate_title = requested_title.is_empty();
        let title = if generate_title {
            draft_title(&prompt)
        } else {
            requested_title
        };
        let status_key = form.status.clone();
        let target_status = board_status_beads_value(&status_key, &self.native_kanban.columns);
        let draft = KanbanCreateDraft {
            title: title.clone(),
            description: prompt.clone(),
            priority: form.priority.clone(),
            estimate: tshirt_to_estimate(form.tshirt),
            labels: form.labels.clone(),
        };
        let labels = draft.labels.clone();
        let staged_blockers = form.staged_blockers.clone();
        let priority = draft.priority.parse::<i64>().ok();
        let estimate = draft.estimate;
        let known_ids = self
            .native_kanban
            .issues
            .iter()
            .map(|issue| issue.id.clone())
            .collect::<Vec<_>>();
        let prefix = self.native_kanban.issue_prefix.clone();
        self.native_kanban.create_in_flight = true;
        self.native_kanban.panel = None;
        self.native_kanban_spawn(
            move |context| create_issue(context, &prefix, &draft, &known_ids),
            move |this, result, cx| {
                this.native_kanban.create_in_flight = false;
                let mut issue = match result {
                    Ok(issue) => issue,
                    Err(error) => {
                        this.native_kanban.error = Some(error.clone());
                        this.native_kanban_toast("Ticket creation failed", &error, cx);
                        this.native_kanban_notify(cx);
                        return;
                    }
                };
                if issue.description.is_empty() {
                    issue.description = prompt.clone();
                }
                if !labels.is_empty() {
                    issue.labels = labels.clone();
                }
                issue.priority = priority.or(issue.priority);
                if estimate.is_some() {
                    issue.estimate = estimate;
                }
                issue.title = title;
                issue.status = target_status.clone();
                let issue_id = issue.id.clone();
                let needs_status = status_key != "todo" && !start_after_create;
                if needs_status {
                    this.native_kanban.move_serial += 1;
                    this.native_kanban.pending_moves.insert(
                        issue_id.clone(),
                        KanbanPendingMove {
                            beads_status: target_status.clone(),
                            token: this.native_kanban.move_serial,
                        },
                    );
                }
                this.native_kanban_upsert_local_issue(issue);
                this.native_kanban_write_staged_blockers(&issue_id, staged_blockers, cx);
                if start_after_create {
                    this.native_kanban_start_work(&issue_id, cx);
                }
                this.native_kanban_reconcile_created(
                    &issue_id,
                    needs_status,
                    target_status,
                    labels,
                    cx,
                );
                if generate_title {
                    this.native_kanban_schedule_generated_title(issue_id, prompt, cx);
                }
                this.native_kanban_notify(cx);
            },
            cx,
        );
        self.native_kanban_notify(cx);
    }

    fn native_kanban_reconcile_created(
        &mut self,
        issue_id: &str,
        needs_status: bool,
        target_status: String,
        labels: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        let id = issue_id.to_string();
        let issue_id = issue_id.to_string();
        self.native_kanban_spawn(
            move |context| {
                if needs_status {
                    beads_call(
                        context,
                        json!({ "action": "updateStatus", "issueId": id, "status": target_status }),
                    )?;
                }
                if !labels.is_empty() {
                    beads_call(
                        context,
                        json!({ "action": "setLabels", "issueId": id, "labels": labels }),
                    )?;
                }
                Ok::<(), String>(())
            },
            move |this, result, cx| {
                this.native_kanban.pending_moves.remove(&issue_id);
                if let Err(error) = result {
                    this.native_kanban.error = Some(error.clone());
                    this.native_kanban_toast("Ticket update failed", &error, cx);
                    this.native_kanban_notify(cx);
                }
                this.native_kanban_refresh(KanbanRefreshMode::Background, cx);
            },
            cx,
        );
    }

    /// A ticket created without a title gets one from the board's default prompt agent, later
    /// and in the background; failures keep the draft title.
    fn native_kanban_schedule_generated_title(
        &mut self,
        issue_id: String,
        prompt: String,
        cx: &mut Context<Self>,
    ) {
        let conversation = &self.native_kanban.conversation;
        let agent = conversation
            .default_agent_id
            .as_deref()
            .and_then(|id| {
                conversation
                    .agents
                    .iter()
                    .find(|agent| agent.agent_id == id)
            })
            .or_else(|| conversation.agents.first());
        let agent_id = agent
            .map(|agent| agent.agent_id.clone())
            .or_else(|| conversation.default_agent_id.clone())
            .unwrap_or_default();
        let agent_command = agent.and_then(|agent| agent.command.clone());
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(GENERATED_TITLE_DELAY).await;
            let _ = this.update(cx, move |this, cx| {
                let id = issue_id.clone();
                this.native_kanban_spawn(
                    move |context| {
                        generate_and_store_title(
                            context,
                            &id,
                            &prompt,
                            &agent_id,
                            agent_command.as_deref(),
                        )
                    },
                    move |this, result, cx| {
                        if let Ok(title) = result
                            && let Some(issue) = this
                                .native_kanban
                                .issues
                                .iter_mut()
                                .find(|issue| issue.id == issue_id)
                        {
                            issue.title = title;
                            this.native_kanban_rebuild_tickets();
                            this.native_kanban_notify(cx);
                        }
                    },
                    cx,
                );
            });
        })
        .detach();
    }

    /// Writes the board's `status.custom` value and reloads, the way the Columns dialog does.
    pub(crate) fn native_kanban_write_column_config(
        &mut self,
        config: String,
        cx: &mut Context<Self>,
    ) {
        if let Some(KanbanPanel::Columns(form)) = self.native_kanban.panel.as_mut() {
            form.busy = true;
            form.error = None;
        }
        let value = config.clone();
        self.native_kanban_spawn(
            move |context| beads_call(context, json!({ "action": "configSet", "value": value })),
            move |this, result, cx| {
                let error = result.err();
                if let Some(KanbanPanel::Columns(form)) = this.native_kanban.panel.as_mut() {
                    form.busy = false;
                    form.error = error.clone();
                }
                if error.is_none() {
                    this.native_kanban.column_config = config;
                    this.native_kanban_refresh(KanbanRefreshMode::Manual, cx);
                }
                this.native_kanban_notify(cx);
            },
            cx,
        );
        self.native_kanban_notify(cx);
    }
}
