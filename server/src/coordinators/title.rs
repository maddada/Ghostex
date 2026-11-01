//! The names a coordinator and its threads keep: the one the user gave a coordinator in the New
//! Coordinator dialog (or `ghostex coordinator create --title`), and the one a coordinator gave a
//! thread with `--title`, stay whatever the agent later calls the conversation.

use rusqlite::Connection;
use serde_json::Value;

use super::records::{read_coordinator, read_thread};
use super::role::{coordinator_role_is_skill, COORDINATOR_SKILL_ONLY_START_SETTING};

/// True for an orchestrator that started with nothing but the skill line (a handoff, whose message
/// waits as a draft, or a `create` with no task): its placeholder "Coordinator" stays, because
/// anything the agent names it from that first message is the skill line.
///
/// CDXC:Coordinators 2026-11-02 DECISION:
/// User: "the skill line must never become a title". A handed-off orchestrator was named "Ghostex orchestrator skill" by its agent, which had only `/ghostex-orchestrator` to go on, instead of keeping the "Coordinator" every orchestrator starts with. It keeps "Coordinator" (like an unnamed `create`, which the agent's first real request names) until the user renames it; this supersedes, for a start without a task, the 2026-10-03 rule that the agent's first name replaces the placeholder.
fn is_skill_only_start(session: &Value) -> bool {
    coordinator_role_is_skill(session)
        && session
            .pointer(&format!(
                "/runtimeSettings/{COORDINATOR_SKILL_ONLY_START_SETTING}"
            ))
            .and_then(Value::as_bool)
            == Some(true)
}

/// True when an agent-chosen title (its terminal title, or its own session metadata such as a
/// Codex thread name) must not replace this session's title.
///
/// CDXC:Coordinators 2026-10-03 DECISION:
/// User: "we should keep the name i assigned for the coordinator at the start. no auto renaming for coordinators." Claude's terminal title and Codex's thread name are adopted for every other session, so a coordinator was renamed from its first message. A coordinator therefore ignores the agent's naming; a rename the user asks for (the sidebar's Rename, which types `/rename` and waits for the agent's metadata to confirm it) still applies. A coordinator created without a name is saved as a placeholder "Coordinator", so the agent's first name replaces it once and is then kept like a typed one.
/// CDXC:Coordinators 2026-10-05 DECISION:
/// User chose to have threads keep the title the coordinator gives them with `--title`, like coordinators keep theirs: Claude's or Codex's own auto-title must not replace it. This supersedes the 2026-10-03 rule that threads keep naming themselves. A short brief let Claude name thread G9c61 after its coordinator (the only name in its first message, observed 2026-10-05), and a title that follows the agent also changed whenever a thread's identity briefly pointed at another conversation. The user's own rename of a thread still applies, and a thread started without a title (a placeholder) is named once by its agent and then kept.
pub fn keeps_its_given_title(db: &Connection, session: &Value) -> bool {
    let title_source = session
        .pointer("/runtimeSettings/titleSource")
        .and_then(Value::as_str);
    if title_source == Some("placeholder") {
        return is_skill_only_start(session);
    }
    let text = |key: &str| session.get(key).and_then(Value::as_str).unwrap_or_default();
    let (project_id, session_id) = (text("projectId"), text("sessionId"));
    read_coordinator(db, project_id, session_id)
        .ok()
        .flatten()
        .is_some()
        || read_thread(db, project_id, session_id)
            .ok()
            .flatten()
            .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn coordinator(title_source: &str, skill_only_start: bool) -> Value {
        let mut runtime = json!({ "coordinatorRole": "skill", "titleSource": title_source });
        if skill_only_start {
            runtime["coordinatorSkillOnlyStart"] = json!(true);
        }
        json!({ "projectId": "P1", "sessionId": "S1", "title": "Coordinator", "runtimeSettings": runtime })
    }

    #[test]
    fn a_coordinator_that_started_on_the_skill_line_alone_keeps_its_placeholder() {
        let db = Connection::open_in_memory().unwrap();
        assert!(keeps_its_given_title(
            &db,
            &coordinator("placeholder", true)
        ));
    }

    #[test]
    fn a_coordinator_whose_first_message_had_a_request_may_be_named_from_it() {
        let db = Connection::open_in_memory().unwrap();
        assert!(!keeps_its_given_title(
            &db,
            &coordinator("placeholder", false)
        ));
    }
}
