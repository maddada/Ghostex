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
    extensions::ExtensionRef,
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

/*
CDXC:Telemetry 2026-11-20:
User: "add PostHog tracking for Boosts used in a message and for extensions (I
want to know usage of those two)". Installs and uninstalls now say WHICH
extension: an official Store id (with the version installed) or `custom` for a
local or unlisted one, resolved through `extensions::resolve_extension`, so a
user's own extension name stops here. The id comes from the registry's answer,
not from the request, so it is the extension that was really installed.
*/
pub fn extension_installed(source: &'static str, id: Option<&str>, version: Option<&str>) {
    let mut properties = vec![("source", PropertyValue::Enum(source))];
    if let Some(id) = id {
        let extension = super::extensions::resolve_store_extension(id);
        if let (ExtensionRef::Store(_), Some(version)) = (&extension, version) {
            if let Some(version) = taxonomy::normalize_version_string(version) {
                properties.push(("extension_version", PropertyValue::Version(version)));
            }
        }
        properties.push(("extension", PropertyValue::Extension(extension)));
    }
    capture(taxonomy::EVENT_EXTENSION_INSTALLED, &properties);
}

pub fn extension_uninstalled(id: Option<&str>) {
    let mut properties = Vec::new();
    if let Some(id) = id {
        properties.push((
            "extension",
            PropertyValue::Extension(super::extensions::resolve_store_extension(id)),
        ));
    }
    capture(taxonomy::EVENT_EXTENSION_UNINSTALLED, &properties);
}

/// An extension was switched on or off: an installed one through Settings > Extensions (`kind`
/// `installed`), or a built-in one, noticed by the daily snapshot (`kind` `built_in`).
pub fn extension_toggled(extension: ExtensionRef, kind: &'static str, enabled: bool) {
    capture(
        if enabled {
            taxonomy::EVENT_EXTENSION_ENABLED
        } else {
            taxonomy::EVENT_EXTENSION_DISABLED
        },
        &[
            ("extension", PropertyValue::Extension(extension)),
            ("kind", PropertyValue::Enum(kind)),
        ],
    );
}

/*
CDXC:Telemetry 2026-11-20:
A message went to an agent with Boosts on it. Ghostex's own Boosts are named by
their id (gx-chat-core `extras::boosts::BOOSTS`, re-checked by the taxonomy);
the user's own Boosts are only counted, so their label and prompt never leave.
Sent beside `prompt.sent`, on the same success path, with the same agent and
source.
*/
pub fn boosts_used(
    session: &Value,
    source: &'static str,
    built_in: Vec<&'static str>,
    custom_count: usize,
) {
    if built_in.is_empty() && custom_count == 0 {
        return;
    }
    let mut properties = agent_properties(session_agent_report(session));
    properties.push(("source", PropertyValue::Enum(source)));
    properties.push((
        "boost_count",
        PropertyValue::Number((built_in.len() + custom_count) as f64),
    ));
    properties.push((
        "custom_boost_count",
        PropertyValue::Number(custom_count as f64),
    ));
    properties.push(("boosts", PropertyValue::EnumList(built_in)));
    capture(taxonomy::EVENT_BOOSTS_USED, &properties);
}

/// The desktop startup ping, forwarded from `/api/recordClientEvent`.
pub fn app_launched(
    app_version: String,
    linux_desktop: Option<&'static str>,
    linux_session: Option<&'static str>,
) {
    let mut properties = vec![
        ("client", PropertyValue::Enum("desktop")),
        ("app_version", PropertyValue::Version(app_version)),
    ];
    if let Some(desktop) = linux_desktop {
        properties.push(("linux_desktop", PropertyValue::Enum(desktop)));
    }
    if let Some(session) = linux_session {
        properties.push(("linux_session", PropertyValue::Enum(session)));
    }
    capture(taxonomy::EVENT_APP_LAUNCHED, &properties);
}

/// The properties of one `surface.opened`, built in one place so the loopback
/// handler and the tests agree on its shape.
pub fn surface_opened_properties(
    surface: &'static str,
    extension: Option<&ExtensionRef>,
    client: Option<&'static str>,
) -> Vec<(&'static str, PropertyValue)> {
    let mut properties = vec![("surface", PropertyValue::Enum(surface))];
    if let Some(extension) = extension {
        properties.push(("extension", PropertyValue::Extension(extension.clone())));
    }
    if let Some(client) = client {
        properties.push(("client", PropertyValue::Enum(client)));
    }
    properties
}

/// The throttle window a `surface.opened` claims: one per client, surface and
/// (for an extension view) reported extension id.
pub fn surface_opened_throttle_key(
    surface: &'static str,
    extension: Option<&ExtensionRef>,
    client: Option<&'static str>,
) -> String {
    let client = client.unwrap_or("desktop");
    match extension {
        Some(extension) => format!("{client}:{surface}:{}", extension.throttle_key()),
        None => format!("{client}:{surface}"),
    }
}

/*
CDXC:Telemetry 2026-10-27:
A view opened on the desktop or the phone, forwarded from
`/api/recordClientEvent`. Still one event per view per hour rather than one per
open: the question is "which views get used most", and a raw open count is
dominated by someone flipping between two tabs fifty times in a minute. One
event per view per hour counts the hours a view was in use (and, by distinct
id, how many people use it), which ranks views honestly without flooding
PostHog. Extension views get their own window per reported id, so ten different
extensions opened in one hour are ten data points, not one.
*/
pub fn surface_opened(
    surface: &'static str,
    extension: Option<ExtensionRef>,
    client: Option<&'static str>,
) {
    capture_throttled(
        taxonomy::EVENT_SURFACE_OPENED,
        &surface_opened_throttle_key(surface, extension.as_ref(), client),
        SURFACE_OPENED_THROTTLE,
        &surface_opened_properties(surface, extension.as_ref(), client),
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
