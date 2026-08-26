/*
CDXC:AnonymousAnalytics 2026-08-26:
The emitter surface every instrumentation site calls. One function per event, so
a call site never assembles a property map by hand and therefore never has the
opportunity to put a string of its own into one.

Each of these is fire-and-forget and cheap: a gate check, a validation, and a
push onto an in-memory queue. None of them touch the network, the disk, or
SQLite, so they are safe to call from inside a request handler or a DB write.
*/

use std::time::Duration;

use super::{
    queue::{capture, capture_throttled},
    taxonomy::{self, PropertyValue},
};

/// Both client-attach and surface-open events are noisy by nature — a browser
/// reconnects on every network blip, a user flips between mode tabs constantly —
/// and the question they answer ("does this person use the web app / the Kanban
/// board?") is answered just as well once an hour.
pub const CLIENT_CONNECTED_THROTTLE: Duration = Duration::from_secs(60 * 60);
pub const SURFACE_OPENED_THROTTLE: Duration = Duration::from_secs(60 * 60);

/// A new agent session was created. Terminal sessions are deliberately not
/// counted here: they carry no agent, and reporting every one of them as
/// `custom` would drown the agent-CLI distribution this event exists to measure.
pub fn session_started(agent_id: Option<&str>) {
    capture(
        taxonomy::EVENT_SESSION_STARTED,
        &[(
            "agent",
            PropertyValue::Enum(taxonomy::normalize_agent_id(agent_id)),
        )],
    );
}

/// A prompt was dispatched to an agent. `source` must already be one of the
/// taxonomy's members — callers resolve it through
/// [`super::taxonomy::match_enum`] or one of the mapping helpers below, so an
/// unrecognised route emits nothing rather than inventing a bucket.
pub fn prompt_sent(agent_id: Option<&str>, source: &'static str) {
    capture(
        taxonomy::EVENT_PROMPT_SENT,
        &[
            (
                "agent",
                PropertyValue::Enum(taxonomy::normalize_agent_id(agent_id)),
            ),
            ("source", PropertyValue::Enum(source)),
        ],
    );
}

/// A web or mobile client attached. Throttled per client KIND, so a browser tab
/// that reconnects ten times in a minute is one data point.
pub fn client_connected(client: &'static str) {
    capture_throttled(
        taxonomy::EVENT_CLIENT_CONNECTED,
        client,
        CLIENT_CONNECTED_THROTTLE,
        &[("client", PropertyValue::Enum(client))],
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
