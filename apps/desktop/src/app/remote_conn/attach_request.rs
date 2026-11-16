use std::collections::HashMap;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

#[derive(Clone, Copy)]
pub(crate) enum RemoteAttachRequestIntent {
    Open {
        requested_pane_id: Option<WorkspacePaneId>,
        placement: AgentsWorkspaceNewTerminalPlacement,
        preview_pane_id: Option<WorkspacePaneId>,
    },
    Restore {
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
    },
}

struct PendingRemoteAttach {
    id: u64,
    local_port: u16,
    connect_generation: Option<u64>,
    intent: RemoteAttachRequestIntent,
    restore_slot: Option<(WorkspacePaneId, TerminalSessionId)>,
}

/// CDXC:RemoteMachines 2026-09-22 WHY:
/// A launch, sidebar click, and surfaced-tab restore can all request the same remote attach before its first wake finishes. Share that preparation, keeping the latest explicit open intent, and invalidate it before hiding or replacing the SSH tunnel so its late result cannot reopen a tab or report a cancelled request as an invalid HTTP response.
#[derive(Default)]
pub(crate) struct RemoteAttachRequests {
    next_id: u64,
    pending: HashMap<GpuiRemoteAttachSessionKey, PendingRemoteAttach>,
}

impl RemoteAttachRequests {
    pub(crate) fn cancel_machine(&mut self, machine_id: &str) {
        self.pending
            .retain(|key, _| key.remote_machine_id != machine_id);
    }

    pub(crate) fn clear(&mut self) {
        self.pending.clear();
    }
}

impl GhostexGpuiApp {
    fn remote_attach_request_is_current(&self, key: &GpuiRemoteAttachSessionKey, id: u64) -> bool {
        self.remote_attach_requests
            .pending
            .get(key)
            .is_some_and(|pending| {
                pending.id == id
                    && self
                        .remote_gxserver_connect_generations
                        .get(&key.remote_machine_id)
                        .copied()
                        == pending.connect_generation
                    && self
                        .remote_gxserver_connections
                        .get(&key.remote_machine_id)
                        .is_some_and(|connection| connection.local_port == pending.local_port)
            })
    }

    fn remote_attach_request_is_focused(&self, key: &GpuiRemoteAttachSessionKey) -> bool {
        self.sidebar_gxserver_presentation_focus_state
            .focused_session_id
            .as_deref()
            == Some(
                gpui_remote_scoped_session_id(
                    &key.remote_machine_id,
                    &key.project_id,
                    &key.session_id,
                )
                .as_str(),
            )
    }

    pub(crate) fn prepare_gpui_remote_attach_request(
        &mut self,
        reference: GpuiRemoteAttachSessionReference,
        config: GpuiRemoteMachineConfig,
        target: GpuiRemoteGxserverRequestTarget,
        intent: RemoteAttachRequestIntent,
        cx: &mut gpui::Context<Self>,
    ) {
        let key = GpuiRemoteAttachSessionKey::from(&reference);
        let restore_slot = match intent {
            RemoteAttachRequestIntent::Restore {
                pane_id,
                session_id,
            } => Some((pane_id, session_id)),
            RemoteAttachRequestIntent::Open { .. } => None,
        };
        if let Some(pending) = self.remote_attach_requests.pending.get_mut(&key) {
            if matches!(intent, RemoteAttachRequestIntent::Open { .. }) {
                pending.intent = intent;
            }
            if restore_slot.is_some() {
                pending.restore_slot = restore_slot;
            }
            return;
        }
        self.remote_attach_requests.next_id = self.remote_attach_requests.next_id.wrapping_add(1);
        let id = self.remote_attach_requests.next_id;
        self.remote_attach_requests.pending.insert(
            key.clone(),
            PendingRemoteAttach {
                id,
                local_port: target.local_port,
                connect_generation: self
                    .remote_gxserver_connect_generations
                    .get(&key.remote_machine_id)
                    .copied(),
                intent,
                restore_slot,
            },
        );
        let background = cx.background_executor().clone();
        let started = std::time::Instant::now();
        cx.spawn(async move |this, cx| {
            let preview_pane_id = this
                .update(cx, |this, _| {
                    if !this.remote_attach_request_is_current(&key, id) {
                        return None;
                    }
                    match this.remote_attach_requests.pending.get(&key)?.intent {
                        RemoteAttachRequestIntent::Open {
                            preview_pane_id, ..
                        } => preview_pane_id,
                        RemoteAttachRequestIntent::Restore { .. } => None,
                    }
                })
                .ok()
                .flatten();
            if preview_pane_id.is_some() {
                let preview_target = target.clone();
                let preview_reference = reference.clone();
                let preview = background
                    .spawn(async move {
                        gpui_remote_gxserver_rpc_result(
                            &preview_target,
                            "/api/attachSessionMetadata",
                            &serde_json::json!({
                                "projectId": preview_reference.project_id,
                                "sessionId": preview_reference.session_id,
                            }),
                            std::time::Duration::from_secs(15),
                        )
                    })
                    .await;
                if let Ok(metadata) = preview {
                    let _ = this.update(cx, |this, cx| {
                        if this.remote_attach_request_is_current(&key, id)
                            && this.remote_attach_request_is_focused(&key)
                        {
                            let Some(RemoteAttachRequestIntent::Open {
                                preview_pane_id: Some(preview_pane_id),
                                ..
                            }) = this
                                .remote_attach_requests
                                .pending
                                .get(&key)
                                .map(|pending| pending.intent)
                            else {
                                return;
                            };
                            this.show_pending_agents_chat_launch(
                                GpuiWorkspaceTerminalSessionKey::Remote(key.clone()),
                                &metadata,
                                preview_pane_id,
                                this.pending_keep_view_remote_focus.contains(&key),
                                cx,
                            );
                        }
                    });
                }
            }
            if !this
                .update(cx, |this, _| {
                    this.remote_attach_request_is_current(&key, id)
                })
                .unwrap_or(false)
            {
                return;
            }
            let prepare_reference = reference.clone();
            let result = background
                .spawn(async move {
                    gpui_prepare_remote_attach_terminal_plan(
                        &config,
                        &target,
                        &prepare_reference,
                        true,
                        true,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if !this.remote_attach_request_is_current(&key, id) {
                    return;
                }
                let Some(pending) = this.remote_attach_requests.pending.remove(&key) else {
                    return;
                };
                match pending.intent {
                    RemoteAttachRequestIntent::Open {
                        requested_pane_id,
                        placement,
                        ..
                    } => {
                        support_logs::append(
                            support_logs::GpuiSupportLog::TerminalFocus,
                            "gpui.remoteAttach.openPlanLanded",
                            serde_json::json!({
                                "machineId": key.remote_machine_id,
                                "elapsedMs": started.elapsed().as_millis() as u64,
                                "stillFocused": this.remote_attach_request_is_focused(&key),
                                "planReady": result.is_ok(),
                            }),
                        );
                        if !this.remote_attach_request_is_focused(&key) {
                            this.pending_keep_view_remote_focus.remove(&key);
                            if let (Some((pane_id, session_id)), Ok(plan)) =
                                (pending.restore_slot, result)
                            {
                                this.arm_surfaced_remote_workspace_terminal(
                                    &key, pane_id, session_id, plan, cx,
                                );
                            }
                            return;
                        }
                        match result {
                            Ok(plan) => {
                                this.open_gpui_remote_attach_terminal(
                                    reference,
                                    plan,
                                    requested_pane_id,
                                    placement,
                                    GpuiRemoteAttachOpenIntent::AttachExistingSession,
                                    cx,
                                );
                                this.refresh_gpui_remote_gxserver_presentation_in_background(
                                    &key.remote_machine_id,
                                );
                            }
                            Err(message) => {
                                this.pending_keep_view_remote_focus.remove(&key);
                                support_logs::append(
                                    support_logs::GpuiSupportLog::TerminalFocus,
                                    "gpui.remoteAttach.planFailed",
                                    serde_json::json!({ "machineId": key.remote_machine_id }),
                                );
                                this.dispatch_gpui_app_modal_toast(
                                    "warning",
                                    REMOTE_OPEN_FAILED_TITLE,
                                    &remote_open_failure_text(&key.remote_machine_id, &message),
                                    cx,
                                );
                            }
                        }
                    }
                    RemoteAttachRequestIntent::Restore {
                        pane_id,
                        session_id,
                    } => match result {
                        Ok(plan) => this.arm_surfaced_remote_workspace_terminal(
                            &key, pane_id, session_id, plan, cx,
                        ),
                        Err(_) => support_logs::append(
                            support_logs::GpuiSupportLog::TerminalFocus,
                            "gpui.remoteAttach.surfacedRestorePlanFailed",
                            serde_json::json!({ "machineId": key.remote_machine_id }),
                        ),
                    },
                }
            });
        })
        .detach();
    }
}

/// The title of every notice that something on another computer did not open.
pub(crate) const REMOTE_OPEN_FAILED_TITLE: &str = "Couldn't open it";

/// What a failed open of `machine_id`'s session says, in place of the transport's own words.
///
/// CDXC:RemoteMachines 2026-11-14 WHY: a pill click that could not open a thread on another computer toasted "Remote attach unavailable / Remote gxserver request failed." The plan's errors name the layer that failed for the support log, not for the user, so the notice now says which computer and what to do (AGENTS.md, card and notice wording); the cut-off sentences (`remote_cut_off_message`) are already plain and pass as they are.
pub(crate) fn remote_open_failure_text(machine_id: &str, message: &str) -> String {
    if message.starts_with("The answer from the remote computer")
        || message.starts_with("The remote computer's answer")
    {
        return message.to_string();
    }
    let name = gpui_remote_machine_name_from_settings(machine_id)
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "The other computer".to_string());
    if message.contains("did not answer in time") {
        format!("{name} took too long to answer. Try again.")
    } else if message.contains("tunnel is not reachable") {
        format!(
            "Ghostex isn't connected to {name} right now. Reconnect it from the sidebar, then try again."
        )
    } else {
        format!(
            "{name} didn't let Ghostex open it. Check that Ghostex is running and up to date there, then try again."
        )
    }
}
