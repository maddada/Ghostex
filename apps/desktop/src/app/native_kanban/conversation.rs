//! The board's agent-conversation half: the agents Start work can use, the sessions linked to
//! beads, Start work itself and jumping to a linked session. Like the React board, these go to the
//! Rust store (gx_store/create/board.rs), which owns agents, sessions and focus routing, through
//! the same board request; its answer comes back through `native_kanban_receive_conversation_response`.

use std::time::Duration;

use gpui::Context;
use serde_json::{Value, json};

use super::model::KanbanConversationState;
use super::state::{KanbanConversationRequest, KanbanPendingMove, KanbanRefreshMode};
use super::text::agent_work_prompt;
use crate::GhostexGpuiApp;
use crate::app::window::{GPUI_APP_TOAST_DEFAULT_DURATION_MS, GpuiAppToast, GpuiAppToastLevel};

const REQUEST_PREFIX: &str = "native-kanban-";
/// The React board's bridge timeout, after which a request that never came back stops blocking.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

impl GhostexGpuiApp {
    fn native_kanban_conversation_request(
        &mut self,
        action: &str,
        extra: Value,
        kind: KanbanConversationRequest,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(project) = self.native_kanban.project.clone() else {
            return false;
        };
        let request_id = format!("{REQUEST_PREFIX}{}", uuid::Uuid::new_v4());
        let mut request = json!({
            "action": action,
            "projectPath": project.project_path,
            "requestId": request_id,
        });
        if let Some(project_id) = project.project_id {
            request["projectId"] = json!(project_id);
        }
        if let Some(remote_machine_id) = project.remote_machine_id {
            request["remoteMachineId"] = json!(remote_machine_id);
        }
        if let Some(editor_id) = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.surface_ids.kanban_board_id.clone())
        {
            request["projectEditorId"] = json!(editor_id);
        }
        if let (Some(request), Some(extra)) = (request.as_object_mut(), extra.as_object()) {
            for (key, value) in extra {
                request.insert(key.clone(), value.clone());
            }
        }
        self.native_kanban
            .conversation_requests
            .insert(request_id.clone(), kind);
        if !self.dispatch_gpui_project_board_conversation_request(&request, cx) {
            self.native_kanban.conversation_requests.remove(&request_id);
            return false;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REQUEST_TIMEOUT).await;
            let _ = this.update(cx, |this, cx| {
                match this.native_kanban.conversation_requests.remove(&request_id) {
                    Some(
                        KanbanConversationRequest::StartWork { .. }
                        | KanbanConversationRequest::Jump,
                    ) => {
                        this.native_kanban.busy_ticket = None;
                        this.native_kanban_notify(cx);
                    }
                    Some(KanbanConversationRequest::State) | None => {}
                }
            });
        })
        .detach();
        true
    }

    pub(crate) fn native_kanban_request_conversation_state(&mut self, cx: &mut Context<Self>) {
        let already_pending = self
            .native_kanban
            .conversation_requests
            .values()
            .any(|kind| matches!(kind, KanbanConversationRequest::State));
        if !already_pending {
            self.native_kanban_conversation_request(
                "getState",
                json!({}),
                KanbanConversationRequest::State,
                cx,
            );
        }
    }

    /// `startTicketWork`: launch the ticket's agent on the bead, then move it to In Progress.
    pub(crate) fn native_kanban_start_work(&mut self, ticket_id: &str, cx: &mut Context<Self>) {
        if self.native_kanban.busy_ticket.is_some() {
            return;
        }
        let Some(ticket) = self.native_kanban.ticket(ticket_id).cloned() else {
            return;
        };
        let Some(agent) = self
            .native_kanban
            .conversation
            .start_agent_for(ticket.issue.assignee.as_deref())
            .cloned()
        else {
            self.native_kanban_toast(
                "Could not start ticket work",
                "No agent is configured to start work with.",
                cx,
            );
            return;
        };
        let request = json!({
            "agentId": agent.agent_id,
            "beadDisplayId": ticket.display_id,
            "beadId": ticket.issue.id,
            "prompt": agent_work_prompt(&ticket),
            "startLocation": "currentProject",
            "ticketTitle": ticket.issue.title,
        });
        if self.native_kanban_conversation_request(
            "startWork",
            request,
            KanbanConversationRequest::StartWork {
                ticket_id: ticket.issue.id.clone(),
            },
            cx,
        ) {
            self.native_kanban.busy_ticket = Some(ticket.issue.id.clone());
        } else {
            self.native_kanban_toast(
                "Could not start ticket work",
                "The Ghostex sidebar runtime is not available.",
                cx,
            );
        }
        self.native_kanban_notify(cx);
    }

    /// `jumpToConversation`: open (or resume) the session linked to a bead.
    pub(crate) fn native_kanban_jump_to_session(
        &mut self,
        ticket_id: &str,
        cx: &mut Context<Self>,
    ) {
        if self.native_kanban.busy_ticket.is_some() {
            return;
        }
        let Some(link) = self
            .native_kanban
            .conversation
            .primary_link_for(ticket_id)
            .cloned()
        else {
            return;
        };
        if self.native_kanban_conversation_request(
            "jumpToConversation",
            json!({ "beadId": link.bead_id, "sessionId": link.ghostex_session_id }),
            KanbanConversationRequest::Jump,
            cx,
        ) {
            self.native_kanban.busy_ticket = Some(ticket_id.to_string());
        }
        self.native_kanban_notify(cx);
    }

    /// Takes the store's answer when it belongs to a native Kanban request. Returns
    /// whether it did.
    pub(crate) fn native_kanban_receive_conversation_response(
        &mut self,
        response: &Value,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(request_id) = response["requestId"]
            .as_str()
            .filter(|id| id.starts_with(REQUEST_PREFIX))
        else {
            return false;
        };
        let Some(kind) = self.native_kanban.conversation_requests.remove(request_id) else {
            return true;
        };
        let ok = response["ok"] == true;
        let mut changed = false;
        if ok && response["payload"].is_object() {
            let conversation = KanbanConversationState::from_value(&response["payload"]);
            if conversation != self.native_kanban.conversation {
                self.native_kanban.conversation = conversation;
                changed = true;
            }
        }
        match kind {
            KanbanConversationRequest::State => {}
            KanbanConversationRequest::Jump => {
                changed = true;
                self.native_kanban.busy_ticket = None;
                if !ok {
                    let message = response["error"]
                        .as_str()
                        .unwrap_or("Could not jump to the linked conversation.")
                        .to_string();
                    self.native_kanban_toast("Could not open the session", &message, cx);
                }
            }
            KanbanConversationRequest::StartWork { ticket_id } => {
                changed = true;
                self.native_kanban.busy_ticket = None;
                if ok {
                    self.native_kanban_mark_in_progress(&ticket_id, cx);
                } else {
                    let message = response["error"]
                        .as_str()
                        .unwrap_or("Could not start ticket work.")
                        .to_string();
                    self.native_kanban_toast("Could not start ticket work", &message, cx);
                }
            }
        }
        // The poll asks for this state every few seconds; redraw only when it moved.
        if changed {
            self.native_kanban_notify(cx);
        }
        true
    }

    /// After Start work: the card moves to In Progress at once, and bd follows in the background.
    fn native_kanban_mark_in_progress(&mut self, ticket_id: &str, cx: &mut Context<Self>) {
        let state = &mut self.native_kanban;
        state.move_serial += 1;
        let token = state.move_serial;
        state.pending_moves.insert(
            ticket_id.to_string(),
            KanbanPendingMove {
                beads_status: "in_progress".to_string(),
                token,
            },
        );
        self.native_kanban_set_local_status(ticket_id, "in_progress");
        let issue_id = ticket_id.to_string();
        self.native_kanban_spawn(
            move |context| {
                super::beads::beads_call(
                    context,
                    json!({ "action": "updateStatus", "issueId": issue_id, "status": "in_progress" }),
                )
            },
            {
                let ticket_id = ticket_id.to_string();
                move |this, result, cx| {
                    if this
                        .native_kanban
                        .pending_moves
                        .get(&ticket_id)
                        .is_some_and(|pending| pending.token == token)
                    {
                        this.native_kanban.pending_moves.remove(&ticket_id);
                    }
                    if let Err(error) = result {
                        this.native_kanban.error = Some(error);
                        this.native_kanban_notify(cx);
                    }
                    this.native_kanban_refresh(KanbanRefreshMode::Background, cx);
                }
            },
            cx,
        );
    }

    pub(crate) fn native_kanban_toast(
        &mut self,
        title: &str,
        description: &str,
        cx: &mut Context<Self>,
    ) {
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                copy_text: None,
                id: format!("native-kanban-{}", title.to_lowercase().replace(' ', "-")),
                level: GpuiAppToastLevel::from_raw(Some("error")),
                title: title.to_string(),
                description: Some(description.to_string()).filter(|text| !text.is_empty()),
                loading: false,
                persistent: false,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                epoch: 0,
            },
            cx,
        );
    }
}
