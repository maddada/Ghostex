//! A Claude cloud session's record, `runtimeSettings.claudeCloud`, and its wire shape.

use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;

/// The key of the record in a session's runtime settings.
pub(crate) const RECORD_KEY: &str = "claudeCloud";

/// Where a cloud session is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CloudState {
    /// Created with no task yet: its terminal asks for one.
    Draft,
    /// `claude --cloud` is creating the session on claude.ai.
    Starting,
    /// The session exists on claude.ai (`url` is set).
    Running,
    /// The start failed (`error` says why); a new task can try again.
    Failed,
    /// Brought to this computer (`movedTo` names the local session); the row is parked.
    Moved,
}

impl CloudState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Failed => "failed",
            Self::Moved => "moved",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "draft" => Self::Draft,
            "starting" => Self::Starting,
            "running" => Self::Running,
            "failed" => Self::Failed,
            "moved" => Self::Moved,
            _ => return None,
        })
    }
}

/// `runtimeSettings.claudeCloud` of a cloud session.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CloudRecord {
    pub(crate) state: CloudState,
    /// `https://claude.ai/code/session_…`, once the start reported it.
    pub(crate) url: Option<String>,
    /// `owner/repo` (lowercased) of the GitHub repo the session works on.
    pub(crate) repo: Option<String>,
    /// The branch the session was told to push to (or the ticket's branch).
    pub(crate) branch: Option<String>,
    /// The latest status line ("Pushed 3 commits to claude/x", "Pull request #12 opened").
    pub(crate) status_text: Option<String>,
    pub(crate) pr_url: Option<String>,
    pub(crate) error: Option<String>,
    /// Work page ticket key (`SPX-1245`, `owner/repo#12`) and its label, for sessions started there.
    pub(crate) ticket: Option<String>,
    pub(crate) ticket_label: Option<String>,
    /// The raw record, for keys this struct does not model (the status poller's memory).
    pub(crate) raw: Map<String, Value>,
}

fn text(record: &Map<String, Value>, key: &str) -> Option<String> {
    record
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

impl CloudRecord {
    pub(crate) fn new(state: CloudState) -> Self {
        Self {
            state,
            url: None,
            repo: None,
            branch: None,
            status_text: None,
            pr_url: None,
            error: None,
            ticket: None,
            ticket_label: None,
            raw: Map::new(),
        }
    }

    pub(crate) fn to_value(&self) -> Value {
        let mut value = Value::Object(self.raw.clone());
        value["state"] = json!(self.state.as_str());
        for (key, field) in [
            ("url", &self.url),
            ("repo", &self.repo),
            ("branch", &self.branch),
            ("statusText", &self.status_text),
            ("prUrl", &self.pr_url),
            ("error", &self.error),
            ("ticket", &self.ticket),
            ("ticketLabel", &self.ticket_label),
        ] {
            match field {
                Some(text) => value[key] = json!(text),
                None => {
                    if let Some(object) = value.as_object_mut() {
                        object.remove(key);
                    }
                }
            }
        }
        value
    }

    /// The claude.ai session id (`session_…`) of the URL.
    pub(crate) fn cloud_session_id(&self) -> Option<String> {
        cloud_session_id_of(self.url.as_deref()?)
    }
}

/// `session_01…` from `https://claude.ai/code/session_01…?…`.
pub(crate) fn cloud_session_id_of(url: &str) -> Option<String> {
    let rest = url.split_once("claude.ai/code/")?.1;
    let id: String = rest
        .chars()
        .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
        .collect();
    (id.starts_with("session_") && id.len() > "session_".len()).then_some(id)
}

/// A claude.ai session id as Claude's CLI takes it (`session_…` or `cse_…`), safe on a command line.
pub(crate) fn is_cloud_session_id(id: &str) -> bool {
    (id.starts_with("session_") || id.starts_with("cse_"))
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// A link a user may paste: a claude.ai/code URL, kept without its query string.
pub(crate) fn normalized_session_url(url: &str) -> Option<String> {
    let id = cloud_session_id_of(url.trim())?;
    Some(format!("https://claude.ai/code/{id}"))
}

/// The cloud record of a session, or `None` for every other session.
pub(crate) fn session_cloud_record(session: &Value) -> Option<CloudRecord> {
    let record = session
        .pointer(&format!("/runtimeSettings/{RECORD_KEY}"))?
        .as_object()?;
    let state = CloudState::parse(&text(record, "state")?)?;
    Some(CloudRecord {
        state,
        url: text(record, "url").and_then(|url| normalized_session_url(&url)),
        repo: text(record, "repo"),
        branch: text(record, "branch"),
        status_text: text(record, "statusText"),
        pr_url: text(record, "prUrl"),
        error: text(record, "error"),
        ticket: text(record, "ticket"),
        ticket_label: text(record, "ticketLabel"),
        raw: record.clone(),
    })
}

/// `PresentationSession.claudeCloud` (packages/gx-protocol/src/presentation.rs).
pub(crate) fn presentation_cloud_value(session: &Value) -> Option<Value> {
    let record = session_cloud_record(session)?;
    let mut value = json!({ "state": record.state.as_str() });
    for key in ["accountId", "accountName", "accountEmail", "limitNotice"] {
        if let Some(text) = record.raw.get(key).and_then(Value::as_str) {
            value[key] = json!(text);
        }
    }
    for (key, field) in [
        ("url", &record.url),
        ("branch", &record.branch),
        ("statusText", &record.status_text),
        ("prUrl", &record.pr_url),
        ("error", &record.error),
    ] {
        if let Some(text) = field {
            value[key] = json!(text);
        }
    }
    Some(value)
}

/// Replaces a session's cloud record in one statement, when the session still has one.
///
/// CDXC:ClaudeCloud 2026-10-23 WHY: the start, the status poller and the page reader own only this key, so they write it in place instead of reading and rewriting all runtime settings, which raced the hook, title and activity writers that rewrite the same JSON (the agentbox record is patched the same way).
pub(crate) fn write_cloud_record(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    record: &CloudRecord,
) -> Result<bool, DomainStateError> {
    let changed = db
        .execute(
            "UPDATE sessions SET runtimeSettingsJson = json_set(runtimeSettingsJson, '$.claudeCloud', json(?1)) \
             WHERE projectId = ?2 AND sessionId = ?3 \
             AND json_extract(runtimeSettingsJson, '$.claudeCloud.state') IS NOT NULL",
            params![record.to_value().to_string(), project_id, session_id],
        )
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite Claude cloud state error: {error}"),
        })?;
    Ok(changed > 0)
}

/// Whether Close puts this session to sleep instead of closing it: a cloud session that is awake
/// and exists (or is being created) on claude.ai. Close on it once it is asleep closes it as
/// before, so the row can still be taken off the list.
///
/// CDXC:ClaudeCloud 2026-10-23 DECISION:
/// User, asked whether a cloud session's row should survive Close since the session goes on running on claude.ai: "Keep it listed, asleep". So Close on an awake cloud row sleeps it (its terminal's cloud client stops, the claude.ai session does not), and the row stays in the list with its cloud mark, where a click still opens Claude's page. A draft, a failed start and a session brought to this computer have nothing left running on claude.ai, so their Close closes as every row's does. Kill (`/api/killSession`: the CLI's `kill` and `close-session`, an agent's `agents close`, the phone's Kill Session) is unchanged and still closes the row. Every client asks this one rule: gxserver applies it to `/api/transitionSession`'s close (zmx/endpoint.rs), and the clients whose Close sends something else ask it too.
/// SEE-ALSO: packages/gx-core/src/claude_cloud.rs (`close_keeps_claude_cloud_row`, the desktop's sidebar Close), apps/mobile/app/src/screens/sessions-screen/session-actions.ts (`closeSessionAction`, the phone's Close).
pub(crate) fn close_keeps_session_asleep(session: &Value) -> bool {
    session_cloud_record(session)
        .is_some_and(|record| matches!(record.state, CloudState::Starting | CloudState::Running))
        && crate::presentation::effective_lifecycle_state(session) == "running"
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::close_keeps_session_asleep;

    /// A session row as the repository returns it.
    fn session(lifecycle: &str, provider: &str, cloud: Option<&str>) -> Value {
        let mut session = json!({
            "lifecycleState": lifecycle,
            "providerState": { "lifecycleState": provider, "provider": "zmx" },
            "runtimeSettings": {},
        });
        if let Some(state) = cloud {
            session["runtimeSettings"]["claudeCloud"] = json!({
                "state": state,
                "url": "https://claude.ai/code/session_01Test",
            });
        }
        session
    }

    #[test]
    fn close_keeps_an_awake_cloud_session_asleep() {
        assert!(close_keeps_session_asleep(&session(
            "running",
            "exists",
            Some("running")
        )));
        assert!(close_keeps_session_asleep(&session(
            "running",
            "exists",
            Some("starting")
        )));
        // A row the daemon still calls sleeping while its provider runs is awake.
        assert!(close_keeps_session_asleep(&session(
            "sleeping",
            "exists",
            Some("running")
        )));
    }

    #[test]
    fn close_closes_every_other_session() {
        // Close on an asleep cloud row takes it off the list, as before.
        assert!(!close_keeps_session_asleep(&session(
            "sleeping",
            "missing",
            Some("running")
        )));
        for state in ["draft", "failed", "moved"] {
            assert!(
                !close_keeps_session_asleep(&session("running", "exists", Some(state))),
                "{state}"
            );
        }
        assert!(!close_keeps_session_asleep(&session(
            "running", "exists", None
        )));
    }
}
