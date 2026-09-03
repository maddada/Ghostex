/*
CDXC:Telemetry 2026-08-26:
The emitter surface every instrumentation site calls. One function per event, so
a call site never assembles a property map by hand and therefore never has the
opportunity to put a string of its own into one.

Each of these is fire-and-forget and cheap: a gate check, a validation, and a
push onto an in-memory queue. None of them touch the network, the disk, or
SQLite, so they are safe to call from inside a request handler or a DB write.
*/

use std::time::Duration;

use serde_json::Value;

use super::{
    queue::{capture, capture_throttled},
    taxonomy::{self, AgentReport, PropertyValue},
};

/// Resolve what a session row may report about its agent: the `agentId`
/// through the catalog, and for a user-added agent the executable of the base
/// command gxserver stamped into `runtimeSettings.agentCommand` at creation.
/// Rules in `taxonomy::resolve_agent`.
pub fn session_agent_report(session: &Value) -> AgentReport {
    let agent_id = session.get("agentId").and_then(Value::as_str);
    let command = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("agentCommand"))
        .and_then(Value::as_str);
    taxonomy::resolve_agent(agent_id, command)
}

fn agent_properties(report: AgentReport) -> Vec<(&'static str, PropertyValue)> {
    let mut properties = vec![("agent", PropertyValue::Enum(report.agent))];
    if let Some(executable) = report.executable {
        properties.push(("agent_executable", PropertyValue::Token(executable)));
    }
    properties
}

/// Both client-attach and surface-open events are noisy by nature — a browser
/// reconnects on every network blip, a user flips between mode tabs constantly —
/// and the question they answer ("does this person use the web app / the Kanban
/// board?") is answered just as well once an hour.
pub const CLIENT_CONNECTED_THROTTLE: Duration = Duration::from_secs(60 * 60);
pub const SURFACE_OPENED_THROTTLE: Duration = Duration::from_secs(60 * 60);

/// A new agent session was created. Terminal sessions are deliberately not
/// counted here: they carry no agent, and reporting every one of them as
/// `custom` would drown the agent-CLI distribution this event exists to measure.
pub fn session_started(session: &Value) {
    capture(
        taxonomy::EVENT_SESSION_STARTED,
        &agent_properties(session_agent_report(session)),
    );
}

/// A prompt was dispatched to an agent. `source` must already be one of the
/// taxonomy's members — callers resolve it through
/// [`super::taxonomy::match_enum`] or one of the mapping helpers below, so an
/// unrecognised route emits nothing rather than inventing a bucket.
pub fn prompt_sent(session: &Value, source: &'static str) {
    let mut properties = agent_properties(session_agent_report(session));
    properties.push(("source", PropertyValue::Enum(source)));
    capture(taxonomy::EVENT_PROMPT_SENT, &properties);
}

/// What a connecting web or mobile client said about itself. Every field is
/// optional because the two legacy signals (a keep-awake lease from an older
/// phone app, a browser with an unrecognised User-Agent) carry none of it.
#[derive(Clone, Debug, Default)]
pub struct ClientPlatform {
    /// A `taxonomy::CLIENT_PLATFORMS` member.
    pub os: Option<&'static str>,
    /// Already clamped by `taxonomy::normalize_version_string`.
    pub os_version: Option<String>,
    pub app_version: Option<String>,
}

/// A web or mobile client attached. Throttled per client KIND, so a browser tab
/// that reconnects ten times in a minute is one data point.
pub fn client_connected(client: &'static str, platform: ClientPlatform) {
    let mut properties = vec![("client", PropertyValue::Enum(client))];
    if let Some(os) = platform.os {
        properties.push(("client_os", PropertyValue::Enum(os)));
    }
    if let Some(os_version) = platform.os_version {
        properties.push(("client_os_version", PropertyValue::Version(os_version)));
    }
    if let Some(app_version) = platform.app_version {
        properties.push(("client_app_version", PropertyValue::Version(app_version)));
    }
    capture_throttled(
        taxonomy::EVENT_CLIENT_CONNECTED,
        client,
        CLIENT_CONNECTED_THROTTLE,
        &properties,
    );
}

pub fn extension_installed(source: &'static str) {
    capture(
        taxonomy::EVENT_EXTENSION_INSTALLED,
        &[("source", PropertyValue::Enum(source))],
    );
}

pub fn extension_uninstalled() {
    capture(taxonomy::EVENT_EXTENSION_UNINSTALLED, &[]);
}

/// The desktop startup ping, forwarded from `/api/recordClientEvent`.
pub fn app_launched(app_version: String) {
    capture(
        taxonomy::EVENT_APP_LAUNCHED,
        &[
            ("client", PropertyValue::Enum("desktop")),
            ("app_version", PropertyValue::Version(app_version)),
        ],
    );
}

/// The desktop mode-tab / modal ping, forwarded from `/api/recordClientEvent`.
/// Throttled per surface.
pub fn surface_opened(surface: &'static str) {
    capture_throttled(
        taxonomy::EVENT_SURFACE_OPENED,
        surface,
        SURFACE_OPENED_THROTTLE,
        &[("surface", PropertyValue::Enum(surface))],
    );
}

/*
The `diagnosticInputSource` tag that already rides every `/api/sendSessionMessage`
job, mapped onto the prompt-source enum.

This is an explicit ALLOW table, not a prefix sniff, and that matters: the same
endpoint carries gxserver's own non-prompt writes (auto-title generation, the
fork rename, the remote rename). Those tags are simply absent from the table, so
they resolve to `None` and emit nothing — a rename is not a prompt, and counting
it as one would inflate every per-user prompt metric.

`external-api` is the default tag for an untagged caller, and the only untagged
caller that actually sends a user's prompt is `gx sendMessage`, so it maps to
`cli`.
*/
pub fn prompt_source_for_diagnostic_input_source(source: &str) -> Option<&'static str> {
    match source.trim() {
        "automation" => Some("automation"),
        "board" => Some("board"),
        "cli" | "external-api" => Some("cli"),
        "quick_action" => Some("quick_action"),
        "terminal" => Some("terminal"),
        "session-chat-message" => Some("chat"),
        "worktree-first-prompt" => Some("chat"),
        _ => None,
    }
}
