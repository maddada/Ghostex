use rusqlite::Connection;
use serde_json::{Map, Value};

use crate::domain::{DomainRepository, DomainStateError};

use super::*;

/*
CDXC:SessionFork 2026-08-28:
`/api/sessionForkBranches` is the read side of the same derivation Previous
Sessions hides superseded ancestors with. A chat header asks it "what else
shares this conversation's history", and gets every registry row in the family —
including the ancestors the list surface hides, flagged `ancestor: true`, so a
user can still walk back into a pre-fork branch that no longer has a card.

It reuses `SessionForkFamilies` rather than repeating the edge rules, because a
switcher that disagreed with the list about who is related would be worse than
having no switcher at all.
*/
pub fn list_session_fork_branches(
    db: &Connection,
    server_id: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let project_id = params
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| DomainStateError {
            code: "invalidParams",
            message: "Invalid projectId.".to_string(),
        })?
        .to_string();
    let session_id = params
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| DomainStateError {
            code: "invalidParams",
            message: "Invalid sessionId.".to_string(),
        })?
        .to_string();

    let repository = DomainRepository::new(db, server_id);
    let sessions = repository.list_sessions(None)?;
    let families = SessionForkFamilies::build(&sessions);

    let mut branches: Vec<(i64, String, Value)> = Vec::new();
    for member_id in families.family_session_ids(&session_id) {
        let Some(session) = sessions.iter().find(|candidate| {
            string_field(candidate, "sessionId").as_deref() == Some(member_id.as_str())
        }) else {
            continue;
        };
        let mut branch = Map::new();
        branch.insert("projectId".to_string(), value_field(session, "projectId"));
        branch.insert("sessionId".to_string(), Value::String(member_id.clone()));
        branch.insert(
            "title".to_string(),
            Value::String(fork_branch_title(session)),
        );
        branch.insert(
            "lifecycleState".to_string(),
            Value::String(effective_lifecycle_state(session)),
        );
        let last_active_ms = parse_iso_ms(&last_active_at(session)).unwrap_or(0);
        branch.insert(
            "lastActiveMs".to_string(),
            Value::Number(last_active_ms.into()),
        );
        /*
        The caller's own row. Matching on the session id alone is enough: the
        family is built from registry ids, which are unique across projects.
        */
        branch.insert(
            "current".to_string(),
            Value::Bool(member_id.as_str() == session_id.as_str()),
        );
        if families.is_superseded(member_id) {
            branch.insert("ancestor".to_string(), Value::Bool(true));
        }
        insert_optional_string(
            &mut branch,
            "agentSessionId",
            read_runtime_text(session, "agentSessionId"),
        );
        branches.push((last_active_ms, member_id.clone(), Value::Object(branch)));
    }
    /*
    A session with no relatives still answers with itself, so a caller can render
    the same control unconditionally instead of branching on an empty list.
    */
    if branches.is_empty() {
        if let Some(session) = sessions.iter().find(|candidate| {
            string_field(candidate, "projectId").as_deref() == Some(project_id.as_str())
                && string_field(candidate, "sessionId").as_deref() == Some(session_id.as_str())
        }) {
            let last_active_ms = parse_iso_ms(&last_active_at(session)).unwrap_or(0);
            let mut branch = Map::new();
            branch.insert("projectId".to_string(), value_field(session, "projectId"));
            branch.insert("sessionId".to_string(), Value::String(session_id.clone()));
            branch.insert(
                "title".to_string(),
                Value::String(fork_branch_title(session)),
            );
            branch.insert(
                "lifecycleState".to_string(),
                Value::String(effective_lifecycle_state(session)),
            );
            branch.insert(
                "lastActiveMs".to_string(),
                Value::Number(last_active_ms.into()),
            );
            branch.insert("current".to_string(), Value::Bool(true));
            insert_optional_string(
                &mut branch,
                "agentSessionId",
                read_runtime_text(session, "agentSessionId"),
            );
            branches.push((last_active_ms, session_id.clone(), Value::Object(branch)));
        }
    }

    branches.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    let mut output = Map::new();
    output.insert(
        "branches".to_string(),
        Value::Array(branches.into_iter().map(|(_, _, value)| value).collect()),
    );
    Ok(Value::Object(output))
}

fn fork_branch_title(session: &Value) -> String {
    let title = project_session_title(session);
    ["displayTitle", "primaryTitle", "title"]
        .into_iter()
        .find_map(|key| {
            title
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
        .or_else(|| string_field(session, "title"))
        .unwrap_or_default()
}
