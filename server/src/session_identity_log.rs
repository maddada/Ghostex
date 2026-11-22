//! One `sessionIdentity.changed` line in gxserver's log whenever a session's agent conversation
//! (`runtimeSettings.agentSessionId` / `agentSessionPath`) really changes, naming who changed it.

use std::cell::RefCell;
use std::marker::PhantomData;

use serde_json::{json, Map, Value};

#[cfg(not(test))]
use crate::logging::GxserverLogger;
use crate::logging::{GxserverLogInput, LogLevel};

/*
CDXC:SessionIdentity 2026-11-05 DECISION:
User (Q1283) approved logging every change of a session's agent session id: thread G1y7g's id was
swapped for an unrelated September conversation and the log held no trace of when or by whom, so
the cause had to be pieced together from the hook store and Claude's pid files. Every write
reaches SQLite through `DomainRepository` (`update_session_row`, the one INSERT) or one of the two
raw statements that touch these keys (the send-failure reset and nothing else), so the line is
written there, comparing the row before and after, and only when the id or the transcript path
actually differs: a hook that repeats the stored id writes nothing.
The writer does not know why it writes, so callers name themselves with `identity_write_source`
for as long as they run; scopes nest and the line shows the whole chain, outermost first
(`endpoint:/api/ingestAgentHookEvent > hooklessObserver:SessionStart > identity:passive`). Each
HTTP domain endpoint opens one, so a write from a path nobody labelled still names its endpoint;
only a background write with no scope says `unattributed`.
The log writer redacts paths, so a transcript path is summarised as the Claude config folder it
lives in (`transcriptAccount`: `claude`, `cswap-3`, `other`), which is what told the G1y7g swap
apart (cswap-5 to cswap-3).
SEE-ALSO: server/src/domain/repository/session.rs (the writer), server/src/session_chat_send_failure.rs.
*/

thread_local! {
    static SOURCES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Names the code that may write a session's identity until it is dropped. Not `Send`, so it
/// cannot be held across an `.await` and leak onto another task on the same thread.
#[must_use = "the source is named only while the scope lives"]
pub(crate) struct IdentityWriteSource {
    _not_send: PhantomData<*const ()>,
}

pub(crate) fn identity_write_source(source: impl Into<String>) -> IdentityWriteSource {
    SOURCES.with(|sources| sources.borrow_mut().push(source.into()));
    IdentityWriteSource {
        _not_send: PhantomData,
    }
}

impl Drop for IdentityWriteSource {
    fn drop(&mut self) {
        SOURCES.with(|sources| {
            sources.borrow_mut().pop();
        });
    }
}

fn current_source() -> String {
    SOURCES.with(|sources| {
        let sources = sources.borrow();
        let mut chain: Vec<&str> = Vec::new();
        for source in sources.iter().map(String::as_str) {
            if chain.last() != Some(&source) {
                chain.push(source);
            }
        }
        if chain.is_empty() {
            "unattributed".to_string()
        } else {
            chain.join(" > ")
        }
    })
}

/// The identity fields of a session row's `runtimeSettings`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StoredIdentity {
    pub(crate) agent_session_id: Option<String>,
    pub(crate) agent_session_path: Option<String>,
}

impl StoredIdentity {
    /// Read from a whole session value (`runtimeSettings.agentSessionId` / `agentSessionPath`).
    pub(crate) fn of_session(session: &Value) -> Self {
        let text = |key: &str| {
            session
                .pointer(&format!("/runtimeSettings/{key}"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        Self {
            agent_session_id: text("agentSessionId"),
            agent_session_path: text("agentSessionPath"),
        }
    }
}

/// The Claude config folder a transcript path lives in, without any part of the path itself.
fn transcript_account(path: Option<&str>) -> Option<String> {
    let path = path?;
    if let Some((_, rest)) = path.split_once("/.claude-swap-backup/sessions/") {
        let slot = rest.split(['-', '/']).next().unwrap_or_default();
        if !slot.is_empty() && slot.bytes().all(|byte| byte.is_ascii_digit()) {
            return Some(format!("cswap-{slot}"));
        }
    }
    if path.contains("/.claude/") {
        return Some("claude".to_string());
    }
    Some("other".to_string())
}

/// The line's details, or `None` when nothing a reader could see changed.
pub(crate) fn identity_change_details(
    session_ref: &str,
    agent_id: Option<&str>,
    before: &StoredIdentity,
    after: &StoredIdentity,
    source: &str,
) -> Option<Value> {
    if before == after {
        return None;
    }
    let mut details = Map::new();
    details.insert("sessionRef".into(), json!(session_ref));
    if let Some(agent_id) = agent_id {
        details.insert("agentId".into(), json!(agent_id));
    }
    details.insert(
        "previousAgentSessionId".into(),
        json!(before.agent_session_id),
    );
    details.insert("agentSessionId".into(), json!(after.agent_session_id));
    details.insert(
        "idChanged".into(),
        json!(before.agent_session_id != after.agent_session_id),
    );
    details.insert(
        "transcriptChanged".into(),
        json!(before.agent_session_path != after.agent_session_path),
    );
    details.insert(
        "previousTranscriptAccount".into(),
        json!(transcript_account(before.agent_session_path.as_deref())),
    );
    details.insert(
        "transcriptAccount".into(),
        json!(transcript_account(after.agent_session_path.as_deref())),
    );
    details.insert("source".into(), json!(source));
    Some(Value::Object(details))
}

#[cfg(not(test))]
static LOGGER: std::sync::OnceLock<GxserverLogger> = std::sync::OnceLock::new();

// Unit tests write sessions through the real repository, and the logger would append their lines
// to the user's own gxserver log; they read them from here instead.
#[cfg(test)]
thread_local! {
    static CAPTURED: RefCell<Vec<Value>> = const { RefCell::new(Vec::new()) };
}

/// The lines this test thread would have written, oldest first, emptying the capture.
#[cfg(test)]
pub(crate) fn take_captured_identity_changes() -> Vec<Value> {
    CAPTURED.with(|captured| std::mem::take(&mut *captured.borrow_mut()))
}

/// Writes the line when `before` and `after` differ; the source is the calling scope chain.
pub(crate) fn log_identity_change(
    server_id: &str,
    project_id: &str,
    session_id: &str,
    agent_id: Option<&str>,
    before: &StoredIdentity,
    after: &StoredIdentity,
) {
    let session_ref = crate::ids::create_global_session_ref(server_id, project_id, session_id);
    let Some(details) =
        identity_change_details(&session_ref, agent_id, before, after, &current_source())
    else {
        return;
    };
    #[cfg(test)]
    {
        CAPTURED.with(|captured| captured.borrow_mut().push(details));
    }
    #[cfg(not(test))]
    {
        let logger =
            LOGGER.get_or_init(|| GxserverLogger::new(crate::paths::get_gxserver_paths(None)));
        let _ = logger.log_record(GxserverLogInput {
            level: LogLevel::Info,
            event: "sessionIdentity.changed".to_string(),
            server_id: Some(server_id.to_string()),
            request_id: None,
            client: None,
            duration_ms: None,
            error: None,
            details: Some(details),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(id: Option<&str>, path: Option<&str>) -> StoredIdentity {
        StoredIdentity {
            agent_session_id: id.map(str::to_string),
            agent_session_path: path.map(str::to_string),
        }
    }

    const REAL: &str = "086ffbff-18b6-4953-a742-44443b05a623";
    const STALE: &str = "4f56cc17-81b1-4ff1-922e-d1324add22f2";
    const REAL_PATH: &str = "/Users/u/.claude-swap-backup/sessions/5-a_b.com/projects/-p/086ffbff-18b6-4953-a742-44443b05a623.jsonl";
    const STALE_PATH: &str = "/Users/u/.claude-swap-backup/sessions/3-c_d.com/projects/-m/4f56cc17-81b1-4ff1-922e-d1324add22f2.jsonl";

    #[test]
    fn an_unchanged_identity_writes_nothing() {
        let same = identity(Some(REAL), Some(REAL_PATH));
        assert_eq!(
            identity_change_details("S90:P3lv0:G1y7g", Some("claude"), &same, &same, "x"),
            None
        );
    }

    #[test]
    fn a_swapped_conversation_names_both_ids_both_accounts_and_the_source() {
        let _endpoint = identity_write_source("endpoint:/api/ingestAgentHookEvent");
        let _hook = identity_write_source("hooklessObserver:SessionStart");
        let _again = identity_write_source("hooklessObserver:SessionStart");
        let details = identity_change_details(
            "S90:P3lv0:G1y7g",
            Some("claude"),
            &identity(Some(REAL), Some(REAL_PATH)),
            &identity(Some(STALE), Some(STALE_PATH)),
            &current_source(),
        )
        .expect("a change");
        assert_eq!(
            details,
            json!({
                "sessionRef": "S90:P3lv0:G1y7g",
                "agentId": "claude",
                "previousAgentSessionId": REAL,
                "agentSessionId": STALE,
                "idChanged": true,
                "transcriptChanged": true,
                "previousTranscriptAccount": "cswap-5",
                "transcriptAccount": "cswap-3",
                "source": "endpoint:/api/ingestAgentHookEvent > hooklessObserver:SessionStart",
            })
        );
    }

    #[test]
    fn a_path_learned_for_the_same_id_is_a_change_and_scopes_end_with_their_guard() {
        {
            let _scope = identity_write_source("hook:SessionStart");
            assert_eq!(current_source(), "hook:SessionStart");
        }
        assert_eq!(current_source(), "unattributed");
        let details = identity_change_details(
            "S90:P1:G1",
            None,
            &identity(Some(REAL), None),
            &identity(Some(REAL), Some("/Users/u/.claude/projects/-p/x.jsonl")),
            "unattributed",
        )
        .expect("a change");
        assert_eq!(details["idChanged"], json!(false));
        assert_eq!(details["transcriptChanged"], json!(true));
        assert_eq!(details["previousTranscriptAccount"], Value::Null);
        assert_eq!(details["transcriptAccount"], json!("claude"));
    }

    #[test]
    fn the_details_survive_the_log_redaction() {
        let details = identity_change_details(
            "S90:P3lv0:G1y7g",
            Some("claude"),
            &identity(Some(REAL), Some(REAL_PATH)),
            &identity(Some(STALE), Some(STALE_PATH)),
            "endpoint:/api/updateSession",
        )
        .expect("a change");
        let line = crate::logging::normalize_log_entry_for_tests(GxserverLogInput {
            level: LogLevel::Info,
            event: "sessionIdentity.changed".to_string(),
            server_id: Some("S90".to_string()),
            request_id: None,
            client: None,
            duration_ms: None,
            error: None,
            details: Some(details.clone()),
        });
        assert_eq!(line["details"], details);
    }
}
