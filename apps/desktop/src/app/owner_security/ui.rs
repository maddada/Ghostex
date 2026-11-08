//! The approval notice: a request an agent (or a phone, or the CLI) made on this computer or a
//! connected one shows in the centred notice modal with Deny and Approve (app/notice_modal_host.rs);
//! Approve runs the system prompt and signs it for that computer's gxserver. The notice goes when
//! the request is decided anywhere (here, on the phone) or expires.

use ghostex_gx_protocol::approval::ApprovalRequest;

use super::Transport;
use crate::GhostexGpuiApp;
use crate::app::window::toast::{GpuiAppToast, GpuiAppToastAction, GpuiAppToastLevel};

fn card_id(machine_id: Option<&str>, request_id: &str) -> String {
    format!(
        "security-approval-{}-{request_id}",
        machine_id.unwrap_or("local")
    )
}

impl GhostexGpuiApp {
    /// `approvalRequestsChanged` from this computer's gxserver (`machine_id: None`) or a
    /// connected computer's.
    pub(crate) fn owner_security_on_approval_change(
        &mut self,
        machine_id: Option<String>,
        change: &str,
        request: Option<ApprovalRequest>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(request) = request else {
            return;
        };
        let id = card_id(machine_id.as_deref(), &request.id);
        if change != "requested" {
            self.remove_gpui_app_toast(&id, cx);
            return;
        }
        // The app's own clicks are approved right where they happen (`approve_own_call`); its
        // background work shows its own card (`owner_security_show_waiting_request`).
        if request.requested_by.kind == "owner" {
            return;
        }
        self.owner_security_show_card(machine_id, request, cx);
    }

    /// The card for a request the app's own background work waits on (a connected computer's
    /// renewal, `remote_conn/connected_computers.rs`): shown once, whoever made it, since that
    /// work never shows the system prompt.
    pub(crate) fn owner_security_show_waiting_request(
        &mut self,
        machine_id: Option<String>,
        request: &ApprovalRequest,
        cx: &mut gpui::Context<Self>,
    ) {
        // Once per request: a card the user closed doesn't come back on the next renewal.
        static SHOWN: std::sync::Mutex<Option<std::collections::HashSet<String>>> =
            std::sync::Mutex::new(None);
        let first = SHOWN
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get_or_insert_with(Default::default)
            .insert(card_id(machine_id.as_deref(), &request.id));
        if first {
            self.owner_security_show_card(machine_id, request.clone(), cx);
        }
    }

    fn owner_security_show_card(
        &mut self,
        machine_id: Option<String>,
        request: ApprovalRequest,
        cx: &mut gpui::Context<Self>,
    ) {
        let id = card_id(machine_id.as_deref(), &request.id);
        // The notice modal draws the request's own words with who asked and on which computer
        // (app/notice_modal_host.rs `notice_item`); the description is that line for any other
        // reader of the toast.
        let mut description = request.summary.clone();
        if let Some(detail) = request
            .detail
            .as_deref()
            .filter(|detail| !detail.is_empty())
        {
            description.push_str(". ");
            description.push_str(detail);
        }
        let computer = machine_id.as_deref().map(|machine| {
            crate::app::helpers::gpui_remote_machine_name_from_settings(machine)
                .unwrap_or_else(|| machine.to_string())
        });
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                id,
                level: GpuiAppToastLevel::Warning,
                title: "Approval needed".to_string(),
                description: Some(description),
                copy_text: None,
                action: Some(GpuiAppToastAction::SecurityApproval {
                    request: Box::new(request),
                    deny: false,
                    machine_id,
                    computer,
                }),
                loading: false,
                persistent: true,
                duration_ms: 0,
                epoch: 0,
            },
            cx,
        );
    }

    /// Approve (the system prompt, then the signature) or Deny on an approval card.
    pub(crate) fn owner_security_decide(
        &mut self,
        request: &ApprovalRequest,
        deny: bool,
        machine_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let transport = match machine_id {
            None => Transport::Local,
            Some(machine) => match self.gpui_remote_gxserver_request_target(machine) {
                Some(target) => Transport::Remote(target),
                None => {
                    self.dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Computer not connected",
                        "Connect to that computer again, then approve.",
                        cx,
                    );
                    return;
                }
            },
        };
        let request = request.clone();
        let card = card_id(machine_id, &request.id);
        let lets_computer_in = matches!(
            request.action,
            ghostex_gx_protocol::approval::SensitiveAction::ComputerGrant
                | ghostex_gx_protocol::approval::SensitiveAction::ComputerRegister
        );
        let decided = cx.background_executor().spawn(async move {
            if !super::request_still_waiting(&transport, &request.id) {
                return Err(None);
            }
            if deny {
                super::deny_request(&transport, &request.id)
            } else {
                super::approve_request(&transport, &request)
            }
            .map_err(Some)
        });
        cx.spawn(async move |this, cx| {
            let outcome = decided.await;
            let _ = this.update(cx, |this, cx| {
                // A connected computer waits on this request: send its call now instead of on
                // its next renewal, well inside the approval's two minutes.
                if outcome.is_ok() && lets_computer_in && !deny {
                    this.renew_connected_computers(true, cx);
                }
                if let Err(None) = outcome {
                    this.remove_gpui_app_toast(&card, cx);
                    this.dispatch_gpui_workspace_action_toast(
                        "info",
                        "Already answered",
                        "That request was already answered or ran out of time.",
                        cx,
                    );
                } else if let Err(Some(error)) = outcome {
                    this.dispatch_gpui_workspace_action_toast(
                        "warning",
                        if deny {
                            "Couldn't deny it"
                        } else {
                            "Not approved"
                        },
                        &error,
                        cx,
                    );
                }
            });
        })
        .detach();
    }
}
