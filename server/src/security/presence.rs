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
    /// Nobody was asked: the policy lets this caller do it (`SecurityState::skips_approval`).
    /// `approval` then names the change only; its id was never a request.
    pub by_policy: bool,
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
        if self.skips_approval(caller, &action) {
            return PresenceOutcome::Granted(PresenceGrant {
                approval: policy_approval(caller, requester_label, &action, now_ms),
                by_policy: true,
            });
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
                    return PresenceOutcome::Granted(PresenceGrant {
                        approval,
                        by_policy: false,
                    });
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
        let request = self
            .open_request(
                caller,
                requester_for(caller, requester_label),
                &action,
                now_ms,
            )
            .request;
        PresenceOutcome::Pending(request)
    }
}

/// What a change the policy lets through without asking is named as: never stored, never
/// approvable, `consumed` at once.
fn policy_approval(
    caller: &Principal,
    label: &str,
    action: &ActionRequest,
    now_ms: u64,
) -> ApprovalRequest {
    use ghostex_gx_protocol::approval::ApprovalStatus;
    ApprovalRequest {
        id: "allowed".to_string(),
        action: action.action.clone(),
        target: action.target.clone(),
        summary: action.summary.clone(),
        detail: action.detail.clone(),
        params_digest: action.params_digest(),
        nonce: String::new(),
        requested_by: requester_for(caller, label),
        created_at_ms: now_ms,
        expires_at_ms: now_ms,
        status: ApprovalStatus::Consumed,
        decided_at_ms: Some(now_ms),
        decided_by: Some("policy".to_string()),
        replaced_by: None,
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
