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
}

impl AlertKind {
    fn wire(self) -> &'static str {
        match self {
            Self::Finished => "finished",
            Self::NeedsInput => "needsInput",
            Self::Failed => "failed",
        }
    }

    fn words(self) -> &'static str {
        match self {
            Self::Finished => "Finished",
            Self::NeedsInput => "Needs input",
            Self::Failed => "Failed",
        }
    }

    /// The phone's switch for this kind: finished, waiting (needs input) or error (failed).
    pub(crate) fn enabled(self, kinds: &PushKinds) -> bool {
        match self {
            Self::Finished => kinds.finished,
            Self::NeedsInput => kinds.waiting,
            Self::Failed => kinds.error,
        }
    }
}

/// One committed feed row to announce on the phones.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AlertEvent {
    pub(crate) kind: AlertKind,
    pub(crate) session_id: String,
    /// The feed row's id, so the phone shows the push and the row as one notice.
    pub(crate) notification_id: String,
    pub(crate) agent_name: String,
    pub(crate) project_name: String,
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
pub(crate) fn alert_message(
    token: &str,
    machine_id: &str,
    event: &AlertEvent,
    ttl_secs: i64,
) -> Option<String> {
    let mut message = Map::new();
    message.insert("to".into(), json!(token));
    message.insert(
        "title".into(),
        json!(alert_title(&event.agent_name, &event.project_name)),
    );
    message.insert("body".into(), json!(event.kind.words()));
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

/// `<agent> in <project>`, or the one name that is there, on one line and at most 120 characters.
fn alert_title(agent_name: &str, project_name: &str) -> String {
    let agent = one_line(agent_name);
    let project = one_line(project_name);
    let title = match (agent.is_empty(), project.is_empty()) {
        (false, false) => format!("{agent} in {project}"),
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
