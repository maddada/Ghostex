//! The head role: the auth gate makes Chief's own session token the head for one request, when
//! and only when that token belongs to the session in Chief's record and the extension is on.
//!
//! CDXC:Chief 2026-11-08 WHY:
//! The brief's security rule: the head capability must be impossible for any other session to
//! get, must vanish when the extension is turned off, and must never cover the owner-only
//! sensitive actions. So no token is ever stored with the head role (`SessionRole::stored`), the
//! role is worked out here on every request from two things no agent can write (the record
//! gxserver keeps for the session it created itself, and the user-only `chiefHidden` setting), and
//! what the role skips is one rule (`Principal::skips_approval`). Turning the extension off, New
//! Chat and Change Agent therefore take the role away at once, with no token to revoke.
//! SEE-ALSO: server/src/security/capability.rs (`SessionRole::Head`, `skips_approval`),
//! server/src/server/route_http/dispatch.rs (where the gate calls this).

use crate::coordinators::SessionKey;
use crate::paths::GxserverPaths;
use crate::security::{Principal, SessionRole};

/// `principal` as the auth gate should see it: the head when it is Chief's session and Chief is
/// on, otherwise unchanged (a head role it was somehow handed is taken back).
pub(crate) fn settle_principal(principal: Principal, paths: &GxserverPaths) -> Principal {
    let Principal::Session {
        project_id,
        session_id,
        ..
    } = &principal
    else {
        return principal;
    };
    let chief = super::chief_key(paths);
    let is_chief = chief
        .as_ref()
        .is_some_and(|(chief_project, chief_session)| {
            chief_project == project_id && chief_session == session_id
        });
    // The settings file is read only for Chief's own requests.
    let on = is_chief && super::chief_on(paths);
    settle_with(principal, chief.as_ref(), on)
}

/// The rule of [`settle_principal`] with its inputs given: `chief` is the recorded Chief session,
/// `on` whether the extension is on.
pub(crate) fn settle_with(principal: Principal, chief: Option<&SessionKey>, on: bool) -> Principal {
    match principal {
        Principal::Session {
            project_id,
            session_id,
            role,
        } => {
            let is_chief = chief.is_some_and(|(chief_project, chief_session)| {
                *chief_project == project_id && *chief_session == session_id
            });
            let role = if is_chief && on {
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
