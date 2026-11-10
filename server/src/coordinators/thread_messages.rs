//! What gxserver answers when an agent session is about to write into another session through
//! the CLI: a thread writing to its own orchestrator is refused, because its final message is
//! already the report.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::records::{read_thread, ThreadRecord};
use crate::domain::DomainStateError;

/// The answer a thread gets when it tries to message or type into its own orchestrator.
pub const THREAD_TO_ORCHESTRATOR_REFUSAL: &str = "You are a thread of this coordinator, so Ghostex did not send this. End your turn with your report; Ghostex forwards your final message to your coordinator as a thread report. If the task had several parts, finish them all before that report, or report after the first part and wait for the coordinator's answer. Questions for the coordinator also go in the final report.";

/// CDXC:Coordinators 2026-10-24 DECISION:
/// User, after a thread sent its orchestrator mid-turn messages ("From the Session links thread, UI patch ready …") that arrived as plain user bubbles with no sender and no handover markers: "this is bad how that session responded... we agreed on a format and he didn't follow it pls fix this in the root cause so it doesn't happen again for any user using agent messaging/orchestrator sessions". So an open thread can never message or type into its own orchestrator: Ghostex refuses and tells it to end its turn with its report.
/// WHY: refused rather than delivered as a "progress" report, because reports come only from the supervisor when the thread stops (server/server/coordinator_runtime.rs): one report per turn, marked in the handover format, recorded on the thread (Threads panel, `orchestrator status`) and delivered when the orchestrator can take it. A mid-turn report would be a second channel the supervisor does not track, and the orchestrator would act on (commit, answer, interrupt) a turn that is still changing files. A thread the orchestrator marked done is not supervised, so its messages are not refused; they carry the sender block like any agent message.
/// SEE-ALSO: server/src/ghostex_cli/agents/outbound.rs (the CLI verbs that ask), server/src/coordinators/brief.rs `thread_brief` (the reporting rules), skills/ghostex-orchestrator/SKILL.md (one task per message).
pub fn thread_to_orchestrator_refusal(
    sender_thread: Option<&ThreadRecord>,
    target: (&str, &str),
) -> Option<&'static str> {
    let thread = sender_thread?;
    (!thread.is_resolved()
        && (
            thread.coordinator_project_id.as_str(),
            thread.coordinator_session_id.as_str(),
        ) == target)
        .then_some(THREAD_TO_ORCHESTRATOR_REFUSAL)
}

/// `/api/checkAgentSessionWrite`: `{ "refusal": <text or null> }` for the session
/// `senderProjectId`/`senderSessionId` about to send to or type into `projectId`/`sessionId`.
pub fn read_agent_session_write_check(
    db: &Connection,
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
    let sender_thread = read_thread(db, &text("senderProjectId"), &text("senderSessionId"))?;
    let refusal = thread_to_orchestrator_refusal(
        sender_thread.as_ref(),
        (text("projectId").as_str(), text("sessionId").as_str()),
    );
    Ok(json!({ "refusal": refusal }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thread(resolved: bool) -> ThreadRecord {
        ThreadRecord {
            project_id: "P1".into(),
            session_id: "T1".into(),
            coordinator_project_id: "P1".into(),
            coordinator_session_id: "C1".into(),
            task: "task".into(),
            resolved_at: resolved.then(|| "2026-10-24T18:00:00.000Z".into()),
            last_resolved_at: None,
            observed_working: true,
            reported_at: None,
            reported_prompt_key: None,
            last_report: None,
            pending_message: None,
            pending_message_at: None,
            closed_with_coordinator_at: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn an_open_thread_writing_to_its_orchestrator_is_refused() {
        let refusal = thread_to_orchestrator_refusal(Some(&thread(false)), ("P1", "C1"));
        assert_eq!(refusal, Some(THREAD_TO_ORCHESTRATOR_REFUSAL));
        assert!(THREAD_TO_ORCHESTRATOR_REFUSAL.contains("End your turn with your report"));
    }

    #[test]
    fn other_writes_go_through() {
        // Another session, or a thread of another orchestrator writing to this one.
        assert_eq!(
            thread_to_orchestrator_refusal(Some(&thread(false)), ("P1", "X9")),
            None
        );
        assert_eq!(
            thread_to_orchestrator_refusal(Some(&thread(false)), ("P2", "C1")),
            None
        );
        // A session that is no thread at all, such as the orchestrator writing to its thread.
        assert_eq!(thread_to_orchestrator_refusal(None, ("P1", "T1")), None);
        // A thread its orchestrator marked done is not supervised, so its report would never come.
        assert_eq!(
            thread_to_orchestrator_refusal(Some(&thread(true)), ("P1", "C1")),
            None
        );
    }
}
