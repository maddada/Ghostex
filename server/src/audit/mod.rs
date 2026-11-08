//! The security log: an append-only, hash-chained record of every sensitive action (who asked,
//! what, when, which approval signed it and how it ended), with a notification for the important
//! ones (`notify::notifies`).
//!
//! The API other code calls is one function:
//!
//! ```ignore
//! crate::audit::record(
//!     crate::audit::Action::new("easyConnect.enable", "Turned on Easy Connect"),
//!     crate::audit::Actor::current(),          // who made the request being served
//!     crate::audit::Outcome::Done,             // or Pending / Approved / Denied / Refused(..) / Failed(..)
//!     None,                                    // Some(ApprovalRef) once a signed approval allowed it
//! );
//! ```
//!
//! - `Action::new(kind, summary)`: `kind` is a stable dotted id (`collab.share`, `pairing.pair`,
//!   `easyConnect.allowList`, `computers.grant`, `autopilot.answer`, `settings.relax`); `summary`
//!   is the plain sentence the user reads in Settings > Security, the bell and the phone's push.
//!   `.target(..)` names the chat, device or computer it was about. The approvals thread's
//!   sensitive-action enum converts into `Action` with `From`.
//! - `Actor::current()` reads who made the HTTP request being served (set once per request in
//!   `server/http_dispatch.rs` from the token that authorized it); outside a request it is
//!   `Actor::Ghostex` (gxserver acting on its own).
//! - `record` never fails the caller: a log that cannot be written is reported in gxserver's own
//!   log, and the action's own result stands.
//!
//! Reading: `/api/securityLog` (`entries` newest first and `check`, the chain verification),
//! `ghostex security log`, and Settings > Security on the desktop.
//!
//! CDXC:Security 2026-11-08 DECISION:
//! User (Q1724, GitHub #495): "Hash-chained log of sensitive actions + a notification for each." Each line carries the SHA-256 of the line before it, so editing, removing or reordering an entry breaks the chain from that line on and Settings > Security says so. gxserver also keeps the newest hash in memory, so a log cut short or rewritten while it runs is caught too.
//! CDXC:Security 2026-11-08 WHY: agents run as the same user and can read and rewrite this file; a chain anyone can recompute does not stop a careful rewrite while gxserver is stopped. Sealing the newest hash with the owner key in the OS key store (the approvals thread's key) closes that; until then the log catches edits, deletions and any rewrite made while gxserver runs.

mod caller;
mod chain;
mod http;
mod notify;
mod types;

pub use caller::CALLER_SESSION_HEADER;
pub(crate) use caller::{note_approval, with_caller, Caller};
pub use chain::{AuditLog, ChainCheck, ChainProblem, StoredEntry};
pub(crate) use http::{handle_security_log_http, SECURITY_LOG_ENDPOINT};
pub(crate) use notify::install_notifier;
pub use types::{Action, Actor, ApprovalRef, Outcome};

use std::sync::{Arc, OnceLock};

/// The live log of this gxserver (`<gxserver state>/security/audit-log.jsonl`).
pub fn live_log() -> &'static Arc<AuditLog> {
    static LOG: OnceLock<Arc<AuditLog>> = OnceLock::new();
    LOG.get_or_init(|| {
        // Unit tests drive handlers that log; they must never write the user's real log.
        if cfg!(test) {
            let dir =
                std::env::temp_dir().join(format!("gxserver-audit-test-{}", std::process::id()));
            return Arc::new(AuditLog::open(AuditLog::default_path(&dir)));
        }
        let paths = crate::paths::get_gxserver_paths(None);
        Arc::new(AuditLog::open(AuditLog::default_path(&paths.root_dir)))
    })
}

/// Records one sensitive action in the security log and sends its notification. Returns the
/// stored entry, or `None` when the log could not be written (logged by gxserver).
pub fn record(
    action: Action,
    actor: Actor,
    outcome: Outcome,
    approval: Option<ApprovalRef>,
) -> Option<StoredEntry> {
    // An entry for a request the approvals gate let through names the approval that allowed it.
    let approval = approval.or_else(caller::request_approval);
    let outcome = match outcome {
        Outcome::Done if approval.is_some() => Outcome::Approved,
        other => other,
    };
    match live_log().append(&action, &actor, &outcome, approval.as_ref()) {
        Ok(entry) => {
            notify::announce(&entry);
            Some(entry)
        }
        Err(error) => {
            notify::report_write_failure(&action, &error);
            None
        }
    }
}
