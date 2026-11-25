//! Fields an agent session's own token may not change, because Ghostex reads its limits from them:
//! a session's folder (`cwd`) and agent mark (`agentId`), and a project's folder (`path`).
//!
//! CDXC:Security 2026-11-26 DECISION:
//! Coordinator, on the user's behalf (re-check of #762, findings N1 and N4): the folders a session's browser tools may upload from are its project's folder and its own (server/src/server/cef_devtools_scope.rs `session_upload_roots`), and a session without an agent mark counts as the user's plain terminal, which gets no switch toasts and no pinned or logged opens (server/src/switch_requests.rs `caller_agent_session`). So a session token (`Principal::Session`, agents, GBot API chats and coordinator threads) may not change those fields through `/api/updateSession` and `/api/updateProject`: a request that would is refused, while one that sends the value already stored (a whole row sent back) passes. The user's devices (the desktop app and its token file, the web build, paired phones and computers) keep changing them, and gxserver's own moves (a session moved into or out of a worktree, a renamed worktree, a draft's run folder) write them directly, so those keep working. A session may still start another session or project in a folder of its choice (`create-session --cwd`, `create-agent --path`); that session's folder is then its own, as its shell's already is.
//! Coordinator, on the user's behalf (re-check 2 of #762, R1): the update turns any id into text (an array of one string becomes that string), while the guards look the stored row up only for string ids, so an id sent another way skipped every guard that needs the row: these, the GBot marks, the refusal to park or pin a coordinator's thread, and the kept approval level and read-only mark. Both update routes now refuse ids that are not strings, for every caller (`require_string_ids`), and a session token's limited field is refused when no stored row was found.

use serde_json::{Map, Value};

use crate::domain::DomainStateError;
use crate::security::Principal;

/// A session's fields its own token may not change.
const SESSION_FIELDS: &[&str] = &["cwd", "agentId"];
/// A project's fields a session token may not change.
const PROJECT_FIELDS: &[&str] = &["path"];

/// The text a field holds, as the update would store it: trimmed, empty as none.
fn text_of(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

/// The fields of `limited` that `params` would change from what `current` holds.
fn changed_fields<'a>(
    limited: &[&'a str],
    current: Option<&Value>,
    params: &Map<String, Value>,
) -> Vec<&'a str> {
    limited
        .iter()
        .copied()
        .filter(|field| params.contains_key(*field))
        .filter(|field| {
            text_of(params.get(*field)) != text_of(current.and_then(|row| row.get(*field)))
        })
        .collect()
}

/// Refuses an update whose `ids` are not JSON strings (each must be sent): the guards before the
/// update read the stored row with them as text, and anything else would let the update find a
/// row the guards did not.
pub(crate) fn require_string_ids(
    params: &Map<String, Value>,
    ids: &[&str],
) -> Result<(), DomainStateError> {
    match ids
        .iter()
        .find(|id| !params.get(**id).is_some_and(Value::is_string))
    {
        Some(id) => Err(DomainStateError::bad_request(format!("Give {id} as text."))),
        None => Ok(()),
    }
}

/// The limited fields `params` sends, when no stored row was found to compare them with.
fn sent_fields<'a>(limited: &[&'a str], params: &Map<String, Value>) -> Vec<&'a str> {
    limited
        .iter()
        .copied()
        .filter(|field| params.contains_key(*field))
        .collect()
}

fn refusal(kind: &str, fields: &[&str]) -> DomainStateError {
    DomainStateError::bad_request(format!(
        "An agent session cannot change a {kind}'s {}: Ghostex reads what its tools may reach from it. Ask the user to change it in Ghostex.",
        fields.join(" or ")
    ))
}

/// Refuses an `/api/updateSession` from a session token that would change a session's folder or
/// agent mark (`current` is the stored session, if found).
pub(crate) fn refuse_session_limit_changes(
    caller: &Principal,
    current: Option<&Value>,
    params: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    if !matches!(caller, Principal::Session { .. }) {
        return Ok(());
    }
    let changed = match current {
        Some(_) => changed_fields(SESSION_FIELDS, current, params),
        None => sent_fields(SESSION_FIELDS, params),
    };
    if changed.is_empty() {
        return Ok(());
    }
    Err(refusal("session", &changed))
}

/// Refuses an `/api/updateProject` from a session token that would change a project's folder
/// (`current` is the stored project, if found).
pub(crate) fn refuse_project_limit_changes(
    caller: &Principal,
    current: Option<&Value>,
    params: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    if !matches!(caller, Principal::Session { .. }) {
        return Ok(());
    }
    let changed = match current {
        Some(_) => changed_fields(PROJECT_FIELDS, current, params),
        None => sent_fields(PROJECT_FIELDS, params),
    };
    if changed.is_empty() {
        return Ok(());
    }
    Err(refusal("project", &changed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn session_caller() -> Principal {
        Principal::Session {
            project_id: "P1".to_string(),
            session_id: "G1".to_string(),
            role: crate::security::SessionRole::Agent,
        }
    }

    fn params(value: Value) -> Map<String, Value> {
        value.as_object().cloned().unwrap()
    }

    #[test]
    fn a_session_cannot_move_its_own_limits() {
        let stored = json!({ "projectId": "P1", "sessionId": "G1", "cwd": "/work/app", "agentId": "claude" });
        for update in [
            json!({ "projectId": "P1", "sessionId": "G1", "cwd": "/Users/me/Documents" }),
            json!({ "projectId": "P1", "sessionId": "G1", "agentId": null }),
            json!({ "projectId": "P1", "sessionId": "G1", "agentId": "" }),
            json!({ "projectId": "P1", "sessionId": "G1", "cwd": null, "title": "x" }),
        ] {
            assert!(
                refuse_session_limit_changes(
                    &session_caller(),
                    Some(&stored),
                    &params(update.clone())
                )
                .is_err(),
                "{update}"
            );
        }
        let project = json!({ "projectId": "P1", "path": "/work/app" });
        assert!(refuse_project_limit_changes(
            &session_caller(),
            Some(&project),
            &params(json!({ "projectId": "P1", "path": "/Users/me" }))
        )
        .is_err());
    }

    #[test]
    fn its_title_and_a_row_sent_back_whole_still_pass() {
        let stored = json!({ "projectId": "P1", "sessionId": "G1", "cwd": "/work/app", "agentId": "claude" });
        for update in [
            json!({ "projectId": "P1", "sessionId": "G1", "title": "Fix the build" }),
            json!({ "projectId": "P1", "sessionId": "G1", "cwd": "/work/app ", "agentId": "claude", "isPinned": true }),
        ] {
            assert!(
                refuse_session_limit_changes(
                    &session_caller(),
                    Some(&stored),
                    &params(update.clone())
                )
                .is_ok(),
                "{update}"
            );
        }
        let project = json!({ "projectId": "P1", "path": "/work/app" });
        assert!(refuse_project_limit_changes(
            &session_caller(),
            Some(&project),
            &params(json!({ "projectId": "P1", "name": "App", "path": "/work/app" }))
        )
        .is_ok());
    }

    #[test]
    fn ids_must_be_text_and_no_row_means_no_limited_field() {
        let ids = ["projectId", "sessionId"];
        assert!(require_string_ids(
            &params(json!({ "projectId": "P1", "sessionId": "G1" })),
            &ids
        )
        .is_ok());
        for update in [
            json!({ "projectId": ["P1"], "sessionId": ["G1"], "agentId": null }),
            json!({ "projectId": "P1", "sessionId": 7 }),
            json!({ "projectId": "P1" }),
            json!({ "projectId": { "id": "P1" }, "sessionId": "G1" }),
        ] {
            assert!(
                require_string_ids(&params(update.clone()), &ids).is_err(),
                "{update}"
            );
        }
        // No stored row: a limited field is refused even when it clears the value.
        let cleared = params(json!({ "projectId": "P1", "sessionId": "G9", "agentId": null }));
        assert!(refuse_session_limit_changes(&session_caller(), None, &cleared).is_err());
        let title = params(json!({ "projectId": "P1", "sessionId": "G9", "title": "x" }));
        assert!(refuse_session_limit_changes(&session_caller(), None, &title).is_ok());
        let path = params(json!({ "projectId": "P9", "path": null }));
        assert!(refuse_project_limit_changes(&session_caller(), None, &path).is_err());
    }

    #[test]
    fn the_users_devices_keep_changing_them() {
        let stored = json!({ "projectId": "P1", "sessionId": "G1", "cwd": "/work/app", "agentId": "claude" });
        let update = params(
            json!({ "projectId": "P1", "sessionId": "G1", "cwd": "/work/app-wt", "agentId": "codex" }),
        );
        for caller in [Principal::LocalAgent, Principal::Anonymous] {
            assert!(refuse_session_limit_changes(&caller, Some(&stored), &update).is_ok());
        }
    }
}
