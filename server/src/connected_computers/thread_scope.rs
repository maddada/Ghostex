//! Which remote threads a caller here may add and remove links on: the relay's rule for the two
//! endpoints that act on one session of another computer by its ids (`ghostex link add|remove
//! --session <ref>`).

use rusqlite::Connection;
use serde_json::Value;

use crate::security::{Capability, Principal, SessionRole};

/// The endpoints [`refusal`] judges. Both are peer endpoints (scope.rs).
pub(crate) const THREAD_LINK_ENDPOINTS: &[&str] =
    &["/api/addSessionLink", "/api/removeSessionLink"];

/*
CDXC:SessionLinks 2026-11-08 FIX:
GitHub #497: the coordinator playbook has a coordinator link each thread to its PR, and for a thread on another computer the relay refused it. The other computer cannot tell whose thread a session is (a coordinator's threads there are linked on the coordinator's own computer, `coordinator_remote_threads`), and a peer token names only the computer. So the check is made here, before the request leaves: a coordinator session may link or unlink only the remote threads linked to it; the token file (the CLI in the user's terminal) keeps thread management of any coordinator's threads (decision 3A of #495); the owner's own session may do either on any session; any other session, an agent that is no coordinator included, nothing.
SEE-ALSO: server/src/server/security_gates.rs (`coordinator_refusal`, the same rule for threads on this computer).
*/
/// Why `caller` may not call `endpoint` on the session in `body` on computer `server_id`, or
/// `None` when it may.
pub(crate) fn refusal(
    db: Option<&Connection>,
    caller: &Principal,
    server_id: &str,
    endpoint: &str,
    body: &[u8],
) -> Option<&'static str> {
    if !THREAD_LINK_ENDPOINTS.contains(&endpoint) {
        return None;
    }
    if caller.is_owner_device() {
        return None;
    }
    let ids = session_ids(body);
    let thread = db.and_then(|db| {
        let (project_id, session_id) = ids?;
        crate::coordinators::read_remote_thread(
            db,
            &(server_id.to_string(), project_id, session_id),
        )
        .ok()
        .flatten()
    });
    let Some(thread) = thread else {
        return Some(
            "Links can only be changed on a thread that a coordinator here started on that computer.",
        );
    };
    match caller {
        Principal::Session {
            project_id,
            session_id,
            role: SessionRole::Coordinator,
        } if thread.coordinator_key() == (project_id.clone(), session_id.clone()) => None,
        Principal::Session { .. } => Some("A session can only manage its own threads."),
        other if other.has(Capability::ManageAnyThreads) => None,
        _ => Some("Only a coordinator can change the links of its own threads."),
    }
}

/// The project and session a request names (`params.projectId`, `params.sessionId`).
fn session_ids(body: &[u8]) -> Option<(String, String)> {
    let body: Value = serde_json::from_slice(body).ok()?;
    let params = body.get("params")?;
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    Some((text("projectId")?, text("sessionId")?))
}
