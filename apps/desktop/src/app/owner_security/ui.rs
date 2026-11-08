//! The approval card: a request an agent (or a phone, or the CLI) made shows as a toast with
//! Approve and Deny; Approve runs the system prompt and signs it. The card goes when the request
//! is decided anywhere (here, on the phone) or expires.

use ghostex_gx_protocol::approval::ApprovalRequest;

use crate::GhostexGpuiApp;
use crate::app::window::toast::{GpuiAppToast, GpuiAppToastAction, GpuiAppToastLevel};

fn card_id(request_id: &str) -> String {
    format!("security-approval-{request_id}")
}

impl GhostexGpuiApp {
    /// `approvalRequestsChanged` from this computer's gxserver.
    pub(crate) fn owner_security_on_approval_change(
        &mut self,
        change: &str,
        request: Option<ApprovalRequest>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(request) = request else {
            return;
        };
        if change != "requested" {
            self.remove_gpui_app_toast(&card_id(&request.id), cx);
            return;
        }
        // The app's own clicks are approved right where they happen (`approve_own_call`).
        if request.requested_by.kind == "owner" {
            return;
        }
        let asker = if request.requested_by.label.is_empty() {
            "An agent".to_string()
        } else {
            request.requested_by.label.clone()
        };
        let mut description = format!("{asker} asks to: {}.", request.summary);
        if let Some(detail) = request
            .detail
            .as_deref()
            .filter(|detail| !detail.is_empty())
        {
            description.push(' ');
            description.push_str(detail);
        }
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                id: card_id(&request.id),
                level: GpuiAppToastLevel::Warning,
                title: "Approval needed".to_string(),
                description: Some(description),
                copy_text: None,
                action: Some(GpuiAppToastAction::SecurityApproval {
                    request: Box::new(request),
                    deny: false,
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
        cx: &mut gpui::Context<Self>,
    ) {
        let request = request.clone();
        let decided = cx.background_executor().spawn(async move {
            if deny {
                super::deny_request(&request.id)
            } else {
                super::approve_request(&request)
            }
        });
        cx.spawn(async move |this, cx| {
            let outcome = decided.await;
            let _ = this.update(cx, |this, cx| {
                if let Err(error) = outcome {
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
