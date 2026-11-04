//! Account recovery never restarts an agent, or types its Continue prompt, into a session that has no conversation.
//!
//! SEE-ALSO: recovery.rs (`plan_session`, the restart branch), agents/resume_plan.rs (`agent_conversation_written`), session_chat_queue_runtime/send_heal.rs (`has_conversation_to_resume`, which cannot be used here: it answers yes for every Claude session that has an id).
use chrono::Utc;
use serde_json::{json, Map, Value};

pub(super) const NO_CONVERSATION: &str = "No saved conversation is available to resume.";

/// CDXC:AgentProviders 2026-10-31 DECISION:
/// GitHub #366, user: "Fix it". A Claude session nobody had typed into lost its agent when gxserver died; on restore recovery woke it (which starts Claude fresh under the same id) and typed "Continue the unfinished task…" into a brand-new conversation, and the agent spent a turn looking for the task.
/// The agent publishes its session id at launch but writes no transcript until the first prompt, so the id alone proves nothing. A restart that recovery would follow with Continue now needs the conversation on disk, judged by the rule the wake itself uses (`agent_conversation_written`); an agent without that rule keeps the old test (a saved session id).
/// Such a session is marked `needsAttention` with the existing "No saved conversation" reason and is left alone: no restart, no Continue. The user sees the agent-exited card they already had, plus a Recovery line saying why nothing was retried. A silent fresh restart was rejected because the exit may be deliberate and there is nothing to recover; leaving it as a plain waiting retry was rejected because its countdown would promise a retry that never comes.
/// Not gated: the non-restart triggers (the agent is alive, so an error card means it was prompted) and "Retry now" (the user's own click).
pub(super) fn withhold_continue(
    recovery: &mut Value,
    project: &Value,
    session: &Value,
    settings: &Map<String, Value>,
) -> bool {
    let has_session_id = session
        .pointer("/runtimeSettings/agentSessionId")
        .and_then(Value::as_str)
        .is_some_and(|id| !id.trim().is_empty());
    if has_session_id
        && crate::agents::agent_conversation_written(project, session, settings) != Some(false)
    {
        return false;
    }
    recovery["status"] = json!("needsAttention");
    recovery["reason"] = json!(NO_CONVERSATION);
    recovery["nextAttemptAt"] = Value::Null;
    recovery["updatedAt"] = json!(Utc::now().to_rfc3339());
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    // The two notices that restart the agent before Continue (recovery.rs, `plan_session`).
    const TRIGGERS: [&str; 2] = ["agentExited", "usageLimit"];

    fn project() -> Value {
        json!({ "path": "/tmp/project", "customAgents": [], "launchSettings": {} })
    }

    fn session(agent: &str, id: Option<&str>, path: Option<&std::path::Path>) -> Value {
        let mut runtime = json!({ "agentCommand": agent, "titleSource": "terminal-auto" });
        if let Some(id) = id {
            runtime["agentSessionId"] = json!(id);
        }
        if let Some(path) = path {
            runtime["agentSessionPath"] = json!(path.to_string_lossy());
        }
        json!({ "agentId": agent, "launchSettings": {}, "runtimeSettings": runtime, "title": "t" })
    }

    fn waiting(trigger: &str) -> Value {
        json!({ "status": "waiting", "trigger": trigger, "attempt": 0, "nextAttemptAt": "2026-10-31T00:00:00Z" })
    }

    fn settings() -> Map<String, Value> {
        crate::agents::normalize_agent_settings(None)
    }

    fn held(
        agent: &str,
        id: Option<&str>,
        path: Option<&std::path::Path>,
        trigger: &str,
    ) -> (bool, Value) {
        let mut recovery = waiting(trigger);
        let held = withhold_continue(
            &mut recovery,
            &project(),
            &session(agent, id, path),
            &settings(),
        );
        (held, recovery)
    }

    #[test]
    fn a_claude_session_nobody_prompted_gets_no_continue_under_each_trigger() {
        // A fresh id: Claude launched with `--session-id` and wrote nothing.
        let id = uuid::Uuid::new_v4().to_string();
        for trigger in TRIGGERS {
            let (held, recovery) = held("claude", Some(&id), None, trigger);
            assert!(held, "{trigger}");
            assert_eq!(recovery["status"], "needsAttention", "{trigger}");
            assert_eq!(recovery["reason"], NO_CONVERSATION, "{trigger}");
            assert_eq!(recovery["trigger"], trigger);
            assert!(recovery["nextAttemptAt"].is_null(), "{trigger}");
        }
    }

    #[test]
    fn a_claude_session_whose_stored_path_does_not_exist_is_not_a_conversation() {
        let id = uuid::Uuid::new_v4().to_string();
        let missing = std::env::temp_dir().join(format!("gx-missing-{id}.jsonl"));
        for trigger in TRIGGERS {
            assert!(
                held("claude", Some(&id), Some(&missing), trigger).0,
                "{trigger}"
            );
        }
    }

    #[test]
    fn a_codex_session_nobody_prompted_gets_no_continue_under_each_trigger() {
        let id = uuid::Uuid::new_v4().to_string();
        for trigger in TRIGGERS {
            let (held, recovery) = held("codex", Some(&id), None, trigger);
            assert!(held, "{trigger}");
            assert_eq!(recovery["status"], "needsAttention", "{trigger}");
        }
    }

    #[test]
    fn a_session_with_no_saved_id_still_needs_attention() {
        for agent in ["claude", "codex"] {
            for trigger in TRIGGERS {
                assert!(held(agent, None, None, trigger).0, "{agent} {trigger}");
            }
        }
    }

    #[test]
    fn a_claude_session_with_a_written_transcript_recovers_as_before() {
        let dir = tempfile::tempdir().expect("tempdir");
        let id = uuid::Uuid::new_v4().to_string();
        let transcript = dir.path().join(format!("{id}.jsonl"));
        std::fs::write(&transcript, "{}\n").expect("transcript");
        for trigger in TRIGGERS {
            let (held, recovery) = held("claude", Some(&id), Some(&transcript), trigger);
            assert!(!held, "{trigger}");
            assert_eq!(
                recovery,
                waiting(trigger),
                "{trigger}: the record is untouched"
            );
        }
    }

    #[test]
    fn a_codex_session_with_a_written_rollout_recovers_as_before() {
        let dir = tempfile::tempdir().expect("tempdir");
        let id = uuid::Uuid::new_v4().to_string();
        let rollout = dir.path().join(format!("rollout-{id}.jsonl"));
        std::fs::write(&rollout, "{}\n").expect("rollout");
        for trigger in TRIGGERS {
            let (held, recovery) = held("codex", Some(&id), Some(&rollout), trigger);
            assert!(!held, "{trigger}");
            assert_eq!(recovery, waiting(trigger), "{trigger}");
        }
    }

    #[test]
    fn an_agent_without_the_transcript_rule_keeps_the_saved_id_test() {
        assert!(!held("opencode", Some("ses_abc123"), None, "agentExited").0);
        assert!(held("opencode", None, None, "agentExited").0);
    }
}
