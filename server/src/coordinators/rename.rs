//! `/api/renameCoordinatorThread`: a thread renames itself (`ghostex orchestrator rename-self`),
//! or its orchestrator renames it (`ghostex orchestrator rename`).

use serde_json::{json, Map, Value};

use super::records::{read_thread, ThreadRecord};
use crate::domain::{DomainRepository, DomainStateError};

/// The longest thread title an orchestrator may give.
const THREAD_TITLE_MAX_CHARS: usize = 120;

/// CDXC:Coordinators 2026-10-24 DECISION:
/// User: "sub-threads under you should rename themselves to match the latest work they're doing ... make it actually work ... so the names stay current." Supersedes the 2026-10-14 wording (the user had picked "Let each thread rename itself when it starts a new task"; the user's own renames always win), which named a thread after the broad area of its task and put the rule only in the first brief, so threads hours later, often compacted, kept old names. A thread's title names its latest task in 3 to 6 words: the brief says so (brief.rs), and every message its own orchestrator sends it through `ghostex agents send` ends with `thread_title_reminder` naming its current title, so it renames itself with `ghostex orchestrator rename-self` (`selfRename`, `titleSource: "thread"`) before it starts a task the title does not describe. The orchestrator's own `ghostex orchestrator rename` stays as a quiet tool (`titleSource: "orchestrator"`), which the user approved for renaming the threads that already exist; the orchestrator does not rename threads on its own. Either way the title is written straight to the session, never typed into the agent as `/rename`, so the agent's auto-naming stays out of it and `keeps_its_given_title` keeps it. A thread the user renamed (a rename request from the sidebar or `ghostex rename-session` leaves `pendingAgentTitleRequestTitleSource: "user"`) is refused, keeps its name and gets no reminder.
/// SEE-ALSO: server/src/coordinators/title.rs (`keeps_its_given_title`), server/src/agents/fork.rs (`request_session_rename`, where a user rename is recorded), server/src/ghostex_cli/coordinator/threads.rs (`rename`), server/src/coordinators/role.md.
pub fn rename_thread(
    repository: &DomainRepository<'_>,
    db: &rusqlite::Connection,
    params: &Map<String, Value>,
) -> Result<(Value, (String, String)), DomainStateError> {
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let (Some(project_id), Some(session_id)) = (text("projectId"), text("sessionId")) else {
        return Err(DomainStateError::bad_request(
            "Renaming a thread needs projectId and sessionId.",
        ));
    };
    let self_rename = params.get("selfRename").and_then(Value::as_bool) == Some(true);
    let coordinator = (text("coordinatorProjectId"), text("coordinatorSessionId"));
    if !self_rename && (coordinator.0.is_none() || coordinator.1.is_none()) {
        return Err(DomainStateError::bad_request(
            "Renaming a thread needs the coordinator's coordinatorProjectId and coordinatorSessionId.",
        ));
    }
    let title = text("title")
        .filter(|title| !title.chars().any(char::is_control))
        .filter(|title| title.chars().count() <= THREAD_TITLE_MAX_CHARS)
        .ok_or_else(|| {
            DomainStateError::bad_request(format!(
                "Give the thread a one-line title of 1 to {THREAD_TITLE_MAX_CHARS} characters."
            ))
        })?;
    let thread = read_thread(db, &project_id, &session_id)?
        .filter(|thread| {
            self_rename
                || (coordinator.0.as_deref() == Some(thread.coordinator_project_id.as_str())
                    && coordinator.1.as_deref() == Some(thread.coordinator_session_id.as_str()))
        })
        .ok_or_else(|| {
            DomainStateError::not_found(if self_rename {
                "This session is not a coordinator's thread, so rename-self does not apply. Only a thread renames itself this way."
            } else {
                "That session is not a thread of this coordinator, so it was not renamed."
            })
        })?;
    let session = repository
        .get_session(&thread.project_id, &thread.session_id)?
        .ok_or_else(|| DomainStateError::not_found("The thread's session no longer exists."))?;
    let current = session["title"].as_str().unwrap_or_default().to_string();
    if user_renamed(&session) {
        return Err(DomainStateError {
            code: "userRenamed",
            message: if self_rename {
                format!(
                    "The user renamed you to \"{current}\" themselves, so keep that name and do not rename yourself."
                )
            } else {
                format!(
                    "The user renamed this thread to \"{current}\" themselves, so it keeps that name. Ask the user if it should change."
                )
            },
        });
    }
    let key = (thread.project_id.clone(), thread.session_id.clone());
    if current == title {
        return Ok((
            json!({ "changed": false, "title": title, "previousTitle": current }),
            key,
        ));
    }
    let mut runtime = session["runtimeSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    runtime.insert(
        "titleSource".into(),
        json!(if self_rename {
            "thread"
        } else {
            "orchestrator"
        }),
    );
    repository.update_session(
        json!({
            "projectId": thread.project_id,
            "sessionId": thread.session_id,
            "title": title,
            "runtimeSettings": runtime,
        })
        .as_object()
        .expect("object literal"),
    )?;
    Ok((
        json!({ "changed": true, "title": title, "previousTitle": current }),
        key,
    ))
}

/// True when the user renamed this session themselves, so its name is theirs to change.
fn user_renamed(session: &Value) -> bool {
    session
        .pointer("/runtimeSettings/pendingAgentTitleRequestTitleSource")
        .and_then(Value::as_str)
        == Some("user")
}

/// The line `ghostex agents send` adds under an orchestrator's message to one of its threads.
pub fn thread_title_reminder(title: &str) -> String {
    let title: String = title
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    format!(
        "Your title is \"{}\". If this message starts a task that title does not describe, rename yourself first: `ghostex coordinator rename-self \"<3 to 6 words>\"`. If it answers that the user renamed you, keep that name.",
        title.trim()
    )
}

/// The reminder for a message from `sender` to the session `session`, whose thread record (if it
/// is a thread) is `thread`: only for a thread of that very orchestrator, and never for a thread
/// the user renamed.
pub fn thread_title_reminder_for(
    thread: Option<&ThreadRecord>,
    sender: (&str, &str),
    session: &Value,
) -> Option<String> {
    let thread = thread?;
    if (
        thread.coordinator_project_id.as_str(),
        thread.coordinator_session_id.as_str(),
    ) != sender
        || user_renamed(session)
    {
        return None;
    }
    Some(thread_title_reminder(
        session["title"].as_str().unwrap_or_default(),
    ))
}

/// `/api/coordinatorThreadTitleReminder`: `{ "reminder": <text or null> }` for a message the
/// session `senderProjectId`/`senderSessionId` is about to send to `projectId`/`sessionId`.
pub fn read_thread_title_reminder(
    repository: &DomainRepository<'_>,
    db: &rusqlite::Connection,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string()
    };
    let (project_id, session_id) = (text("projectId"), text("sessionId"));
    let (sender_project_id, sender_session_id) = (text("senderProjectId"), text("senderSessionId"));
    let thread = read_thread(db, &project_id, &session_id)?;
    let reminder = match &thread {
        Some(_) => repository
            .get_session(&project_id, &session_id)?
            .and_then(|session| {
                thread_title_reminder_for(
                    thread.as_ref(),
                    (sender_project_id.as_str(), sender_session_id.as_str()),
                    &session,
                )
            }),
        None => None,
    };
    Ok(json!({ "reminder": reminder }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thread() -> ThreadRecord {
        ThreadRecord {
            project_id: "P1".into(),
            session_id: "T1".into(),
            coordinator_project_id: "P1".into(),
            coordinator_session_id: "C1".into(),
            task: "task".into(),
            resolved_at: None,
            last_resolved_at: None,
            observed_working: false,
            reported_at: None,
            reported_prompt_key: None,
            last_report: None,
            pending_message: None,
            pending_message_at: None,
            pending_message_send_id: None,
            closed_with_coordinator_at: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn reminds_a_thread_of_its_own_orchestrator() {
        let session = json!({ "title": "Accounts page mockups" });
        assert_eq!(
            thread_title_reminder_for(Some(&thread()), ("P1", "C1"), &session).as_deref(),
            Some("Your title is \"Accounts page mockups\". If this message starts a task that title does not describe, rename yourself first: `ghostex coordinator rename-self \"<3 to 6 words>\"`. If it answers that the user renamed you, keep that name.")
        );
    }

    #[test]
    fn leaves_a_user_renamed_thread_alone() {
        let session = json!({
            "title": "My name",
            "runtimeSettings": { "pendingAgentTitleRequestTitleSource": "user" },
        });
        assert_eq!(
            thread_title_reminder_for(Some(&thread()), ("P1", "C1"), &session),
            None
        );
    }

    #[test]
    fn leaves_messages_from_anyone_else_alone() {
        let session = json!({ "title": "Accounts page mockups" });
        assert_eq!(
            thread_title_reminder_for(Some(&thread()), ("P1", "C2"), &session),
            None
        );
        assert_eq!(
            thread_title_reminder_for(Some(&thread()), ("P2", "C1"), &session),
            None
        );
        assert_eq!(
            thread_title_reminder_for(None, ("P1", "C1"), &session),
            None
        );
    }
}
