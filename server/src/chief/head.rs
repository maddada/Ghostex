//! The head role: the auth gate makes Chief's own session token the head for one request, when
//! and only when that token belongs to the session in Chief's record.
//!
//! CDXC:Chief 2026-11-08 WHY:
//! The brief's security rule: the head capability must be impossible for any other session to
//! get, and must never cover the owner-only sensitive actions. So no token is ever stored with
//! the head role (`SessionRole::stored`), the role is worked out here on every request from the
//! record gxserver keeps for the session it created itself, and what the role skips is one rule
//! (`SecurityState::skips_approval`). New Chat and Change Agent therefore take the role away from
//! the old chat at once, with no token to revoke.
//! SEE-ALSO: server/src/security/capability.rs (`SessionRole::Head`), server/src/security/policy.rs (`skips_approval`),
//! server/src/server/route_http/dispatch.rs (where the gate calls this).

use crate::coordinators::SessionKey;
use crate::paths::GxserverPaths;
use crate::security::{Principal, SessionRole};

/// `principal` as the auth gate should see it: the head when it is Chief's session, otherwise
/// unchanged (a head role it was somehow handed is taken back).
pub(crate) fn settle_principal(principal: Principal, paths: &GxserverPaths) -> Principal {
    if !matches!(principal, Principal::Session { .. }) {
        return principal;
    }
    let chief = super::chief_key(paths);
    settle_with(principal, chief.as_ref())
}

/// The rule of [`settle_principal`] with its inputs given: `chief` is the recorded Chief session.
pub(crate) fn settle_with(principal: Principal, chief: Option<&SessionKey>) -> Principal {
    match principal {
        Principal::Session {
            project_id,
            session_id,
            role,
        } => {
            let is_chief = chief.is_some_and(|(chief_project, chief_session)| {
                *chief_project == project_id && *chief_session == session_id
            });
            let role = if is_chief {
                SessionRole::Head
            } else {
                role.stored()
            };
            Principal::Session {
                project_id,
                session_id,
                role,
            }
        }
        other => other,
    }
}
