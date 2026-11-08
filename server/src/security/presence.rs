//! `require_owner_presence`: the server-side gate every sensitive action goes through.

use ghostex_gx_protocol::approval::{ApprovalRequest, ApprovalRequester};

use super::{
    approvals::{ActionRequest, ApprovalError},
    Capability, Principal, SecurityEvent, SecurityState,
};

/// What the gate decided.
#[derive(Clone, Debug)]
pub enum PresenceOutcome {
    /// The caller presented a fresh approval signed for exactly this change; it is now spent.
    /// Run the action, then call [`SecurityState::record_performed`].
    Granted(PresenceGrant),
    /// The owner hasn't approved yet: answer with this request (`approvalRequired`). The caller
    /// repeats its call with `approvalId` once it is approved.
    Pending(ApprovalRequest),
    /// The owner said no to the presented approval.
    Denied(ApprovalRequest),
    /// This caller may not even ask (a guest, a peer, an extension).
    Refused { message: String },
}

#[derive(Clone, Debug)]
pub struct PresenceGrant {
    pub approval: ApprovalRequest,
}

impl SecurityState {
    /// The gate for a sensitive action. Every caller needs a signed approval, the owner's own
    /// desktop session included: a stolen owner session or a GUI-driving agent must not be
    /// enough. `presented_approval_id` is the call's `approvalId`, if any. `requester_label` is
    /// what the approval card shows as the asker (the session's title, the device's name).
    pub fn require_owner_presence(
        &self,
        caller: &Principal,
        requester_label: &str,
        action: ActionRequest,
        presented_approval_id: Option<&str>,
        now_ms: u64,
    ) -> PresenceOutcome {
        if !caller.has(Capability::RequestSensitiveAction) {
            return PresenceOutcome::Refused {
                message: format!(
                    "This needs the owner's approval, and a {} can't ask for it.",
                    caller.kind()
                ),
            };
        }
        if let Some(id) = presented_approval_id.filter(|id| !id.trim().is_empty()) {
            match self.approvals.consume(id.trim(), caller, &action, now_ms) {
                Ok(approval) => {
                    self.publish(SecurityEvent::SensitiveActionAuthorized {
                        action: approval.action.clone(),
                        target: approval.target.clone(),
                        approval_id: approval.id.clone(),
                        caller: caller.clone(),
                    });
                    return PresenceOutcome::Granted(PresenceGrant { approval });
                }
                Err(ApprovalError::StillPending) => {
                    if let Some(request) = self.approvals.read(id.trim(), caller, now_ms) {
                        return PresenceOutcome::Pending(request);
                    }
                }
                Err(ApprovalError::Denied) => {
                    if let Some(request) = self.approvals.read(id.trim(), caller, now_ms) {
                        return PresenceOutcome::Denied(request);
                    }
                }
                // Unknown, spent, expired or made for another change: ask again below.
                Err(_) => {}
            }
        }
        let (request, is_new) = self.approvals.create(
            caller,
            requester_for(caller, requester_label),
            &action,
            now_ms,
        );
        if is_new {
            self.publish(SecurityEvent::ApprovalRequested {
                request: request.clone(),
            });
        }
        PresenceOutcome::Pending(request)
    }
}

/// The approval card's "asked by" line for `caller`.
pub fn requester_for(caller: &Principal, label: &str) -> ApprovalRequester {
    let mut requester = ApprovalRequester {
        kind: caller.kind().to_string(),
        label: label.to_string(),
        ..Default::default()
    };
    match caller {
        Principal::Session {
            project_id,
            session_id,
            ..
        } => {
            requester.project_id = Some(project_id.clone());
            requester.session_id = Some(session_id.clone());
        }
        Principal::Owner { device_id } | Principal::Device { device_id } => {
            requester.device_id = Some(device_id.clone());
        }
        _ => {}
    }
    requester
}
