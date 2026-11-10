//! Security events: one stream for the audit log (hash-chained log and a notification per
//! sensitive action) and for the desktop and phone to show approval requests as they arrive.

use ghostex_gx_protocol::approval::{ApprovalRequest, ApproverKey, SensitiveAction};
use serde::Serialize;

use super::Principal;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum SecurityEvent {
    /// A caller asked for a sensitive action; the request now waits for the owner.
    ApprovalRequested {
        request: ApprovalRequest,
    },
    /// An approver key signed the request.
    ApprovalApproved {
        request: ApprovalRequest,
        key_id: String,
    },
    /// The owner said no.
    ApprovalDenied {
        request: ApprovalRequest,
        by: Principal,
    },
    /// Nobody answered in time, or the approval went unused in time.
    ApprovalExpired {
        request: ApprovalRequest,
    },
    /// Withdrawn before anyone answered, because what asked went away (`why`: "the session that
    /// asked was closed"). Cards treat it as expired.
    ApprovalWithdrawn {
        request: ApprovalRequest,
        why: String,
    },
    /// An approval was spent: the action it covers is about to run.
    SensitiveActionAuthorized {
        action: SensitiveAction,
        target: Option<String>,
        approval_id: String,
        caller: Principal,
    },
    /// A handler finished a sensitive action (or a change that is always allowed, such as
    /// turning something off, that the log should still show).
    SensitiveActionPerformed {
        action: SensitiveAction,
        target: Option<String>,
        caller: Principal,
        ok: bool,
    },
    SessionTokenMinted {
        principal: Principal,
    },
    SessionTokenRevoked {
        principal: Principal,
    },
    ApproverKeyAdded {
        key: ApproverKey,
    },
    ApproverKeyRemoved {
        key: ApproverKey,
        by: Principal,
    },
}
