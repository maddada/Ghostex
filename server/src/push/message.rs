use serde_json::{json, Map, Value};

use super::registration::{is_printable_id, NOTIFICATION_ID_MAX, SESSION_ID_MAX};
use super::store::PushKinds;

/// How long an alert may wait for the phone, counted from the event.
pub(crate) const ALERT_TTL_MS: i64 = 3_600_000;
/// How long a background-only start message may wait for the phone, counted from the start.
pub(crate) const SILENT_TTL_MS: i64 = 60_000;
/// Expo's limit for one message.
const MESSAGE_BYTES_MAX: usize = 4096;
const TEXT_CHARS_MAX: usize = 120;

/// CDXC:Notifications 2026-10-31 WHY:
/// Field names and values follow Expo's page "Send notifications with the Expo Push Service" (docs.expo.dev/push-notifications/sending-notifications, read 2026-10-31): `contentAvailable` sets iOS `aps.content-available` and replaces the deprecated `_contentAvailable`; `sound` is "default" or left out for no sound; `ttl` is in seconds; `priority` is default, normal or high.
/// An alert names the agent and the project and says what happened in a few fixed words; it never carries the prompt, the reply, the feed row's text, the session title or tool details. It carries `contentAvailable` too, so the app may refresh its Live Activity, and plays the notification sound because the Live Activity has none of its own. A start message has no title, body or sound and only lets iOS start the app briefly in the background.
const CONTENT_AVAILABLE_FIELD: &str = "contentAvailable";
const ALERT_SOUND: &str = "default";
const ALERT_PRIORITY: &str = "high";
const SILENT_PRIORITY: &str = "normal";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AlertKind {
    Finished,
    NeedsInput,
    Failed,
    /// A sensitive action recorded in the security log (server/src/audit/notify.rs).
    Security,
    /// A session not moving that nobody heals (the red dot): the phone's Error switch, as `failed`.
    Stuck,
    /// A session blocked on every account being out of usage (the red dot): the phone's Error
    /// switch, as `failed`. An error recovery could not get past is a `Failed` row already.
    Blocked,
}

impl AlertKind {
    fn wire(self) -> &'static str {
        match self {
            Self::Finished => "finished",
            Self::NeedsInput => "needsInput",
            // CDXC:Notifications 2026-11-08: the red dots ride the phone's existing `failed`
            // kind (its Error switch and Failed section); only the words differ.
            Self::Failed | Self::Stuck | Self::Blocked => "failed",
            Self::Security => "security",
        }
    }

    fn words(self, has_question: bool) -> &'static str {
        match self {
            Self::Finished => "Finished",
            Self::NeedsInput if has_question => "Has a question",
            Self::NeedsInput => "Waiting for you",
            Self::Failed => "Ended with an error",
            Self::Security => "Security: check this was you",
            Self::Stuck => "Not moving: it may be stuck",
            Self::Blocked => "Blocked: out of usage on every account",
        }
    }

    /// The phone's switch for this kind: finished, waiting (needs input) or error (failed).
    pub(crate) fn enabled(self, kinds: &PushKinds) -> bool {
        match self {
            Self::Finished => kinds.finished,
            Self::NeedsInput => kinds.waiting,
            Self::Failed | Self::Stuck | Self::Blocked => kinds.error,
            // CDXC:Security 2026-11-08 WHY: the point of the notice is that the owner sees every sensitive action at once, so no per-kind switch turns it off.
            Self::Security => true,
        }
    }
}

/// One committed feed row to announce on the phones.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AlertEvent {
    pub(crate) kind: AlertKind,
    pub(crate) project_id: String,
    pub(crate) session_id: String,
    /// The feed row's id, so the phone shows the push and the row as one notice.
    pub(crate) notification_id: String,
    pub(crate) agent_name: String,
    pub(crate) project_name: String,
    /// The session's title as the app shows it; an alert's title uses it only for a phone whose "Show session names"
    /// switch is on (`PushRegistration::session_names`).
    pub(crate) session_title: String,
    /// When the event happened, in wall-clock milliseconds; the alert expires an hour later.
    pub(crate) occurred_at_ms: i64,
}

impl AlertEvent {
    pub(crate) fn is_valid(&self) -> bool {
        is_printable_id(&self.session_id, SESSION_ID_MAX)
            && is_printable_id(&self.notification_id, NOTIFICATION_ID_MAX)
    }
}

/// An agent of a top-level session started working.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentStartEvent {
    pub(crate) session_id: String,
    pub(crate) occurred_at_ms: i64,
}

/// The alert as one JSON object, or `None` if it would pass Expo's size limit.
///
/// CDXC:Notifications 2026-11-04 WHY:
/// The phone shows the same events through its own notifications while the app is open, so the push uses the phone's `agent · project` title and event words and an alert reads the same whether the app is open or closed. The push title is kept to one line and at most 120 characters (the push message's title limit): line breaks in a name become spaces, and a longer title is cut with an ellipsis.
/// CDXC:Notifications 2026-11-11 DECISION: the push's first name follows the phone's "Show session names" switch, as its Live Activity and widgets do: off (the default), the agent's name as the app shows it; on, the session's title (the agent's name when the session has none). The switch reaches the computer with the phone's registration (`sessionNames`), so a session title never goes into a push for a phone that did not turn names on, an older phone included.
pub(crate) fn alert_message(
    token: &str,
    machine_id: &str,
    event: &AlertEvent,
    session_names: bool,
    has_question: bool,
    ttl_secs: i64,
) -> Option<String> {
    let mut message = Map::new();
    message.insert("to".into(), json!(token));
    let session_title = one_line(&event.session_title);
    let leading_name = if session_names && !session_title.is_empty() {
        session_title.as_str()
    } else {
        event.agent_name.as_str()
    };
    message.insert(
        "title".into(),
        json!(alert_title(leading_name, &event.project_name)),
    );
    message.insert("body".into(), json!(event.kind.words(has_question)));
    message.insert(
        "data".into(),
        json!({
            "kind": event.kind.wire(),
            "machineId": machine_id,
            "sessionId": event.session_id,
            "notificationId": event.notification_id,
        }),
    );
    message.insert(CONTENT_AVAILABLE_FIELD.into(), json!(true));
    message.insert("sound".into(), json!(ALERT_SOUND));
    message.insert("ttl".into(), json!(ttl_secs));
    message.insert("priority".into(), json!(ALERT_PRIORITY));
    within_limit(Value::Object(message))
}

/// The background-only start message as one JSON object.
pub(crate) fn silent_message(
    token: &str,
    machine_id: &str,
    session_id: &str,
    ttl_secs: i64,
) -> Option<String> {
    let mut message = Map::new();
    message.insert("to".into(), json!(token));
    message.insert(
        "data".into(),
        json!({ "kind": "agentStarted", "machineId": machine_id, "sessionId": session_id }),
    );
    message.insert(CONTENT_AVAILABLE_FIELD.into(), json!(true));
    message.insert("ttl".into(), json!(ttl_secs));
    message.insert("priority".into(), json!(SILENT_PRIORITY));
    within_limit(Value::Object(message))
}

fn within_limit(message: Value) -> Option<String> {
    let text = message.to_string();
    (text.len() <= MESSAGE_BYTES_MAX).then_some(text)
}

/// `<name> · <project>`, or the one name that is there, on one line and at most 120 characters.
fn alert_title(agent_name: &str, project_name: &str) -> String {
    let agent = one_line(agent_name);
    let project = one_line(project_name);
    let title = match (agent.is_empty(), project.is_empty()) {
        (false, false) => format!("{agent} · {project}"),
        (false, true) => agent,
        (true, false) => project,
        (true, true) => "Ghostex".to_string(),
    };
    if title.chars().count() <= TEXT_CHARS_MAX {
        return title;
    }
    let mut shortened: String = title.chars().take(TEXT_CHARS_MAX - 1).collect();
    shortened.push('…');
    shortened
}

fn one_line(name: &str) -> String {
    name.split(|character: char| character.is_whitespace() || character.is_control())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
