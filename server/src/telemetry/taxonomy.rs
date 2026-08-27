/*
CDXC:AnonymousAnalytics 2026-08-26:
The closed event/property table from `docs/2026-08-26/anonymous-analytics/PLAN.md`.

This module is the reason the privacy promise is structural rather than a review
convention. A property value can only be a number, a bool, an enum string picked
from a compile-time `&'static str` set, a list of such enum strings, or the one
deliberately free-form field (`app_version`) which is charset- and
length-clamped. There is no `PropertyValue` variant that carries a runtime-owned
`String` a caller could fill with a prompt, a path, or a project name, so
"someone adds a field with user text in it" is a compile error, not a bug.

Anything that does not match the table below is dropped with a debug log.
*/

use serde_json::{Map, Number, Value};

/// Every agent id the catalog knows (`server/src/agents/helpers.rs`,
/// `packages/shared/sidebar-agents.ts`), plus the literal `custom` bucket every
/// unknown — i.e. user-authored — agent id collapses into.
pub const KNOWN_AGENT_IDS: &[&str] = &[
    "amp",
    "antigravity",
    "campfire",
    "claude",
    "codebuddy",
    "codex",
    "command-code",
    "copilot",
    "cursor",
    "custom",
    "devin",
    "droid",
    "gemini",
    "grok",
    "hermes-agent",
    "kimi",
    "kiro",
    "omp",
    "openclaude",
    "opencode",
    "pi",
    "qoder",
    "rovodev",
];

/// The bucket an unknown agent id becomes. Custom agent ids are user-authored
/// text and must never leave the machine.
pub const CUSTOM_AGENT_ID: &str = "custom";

pub const PROMPT_SOURCES: &[&str] = &[
    "automation",
    "board",
    "chat",
    "cli",
    "queue",
    "quick_action",
    "terminal",
];

pub const SURFACES: &[&str] = &[
    "agents",
    "automate",
    "browser",
    "code",
    "docs",
    "extensions_store",
    "find",
    "kanban",
    "settings",
];

pub const CLIENT_KINDS: &[&str] = &["mobile", "web"];

pub const EXTENSION_SOURCES: &[&str] = &["local", "store"];

pub const INTERFACE_KINDS: &[&str] = &["chat", "terminal"];

pub const SIDEBAR_VERSIONS: &[&str] = &["v1", "v2"];

pub const SIDEBAR_V2_LAYOUTS: &[&str] = &["byProject", "flat"];

/// Event names. Only these reach PostHog.
pub const EVENT_HEARTBEAT: &str = "heartbeat";
pub const EVENT_APP_LAUNCHED: &str = "app.launched";
pub const EVENT_CLIENT_CONNECTED: &str = "client.connected";
pub const EVENT_SESSION_STARTED: &str = "session.started";
pub const EVENT_PROMPT_SENT: &str = "prompt.sent";
pub const EVENT_SURFACE_OPENED: &str = "surface.opened";
pub const EVENT_EXTENSION_INSTALLED: &str = "extension.installed";
pub const EVENT_EXTENSION_UNINSTALLED: &str = "extension.uninstalled";

/// The two events the desktop app is allowed to push over the loopback API.
/// Everything else it might send is dropped by `/api/recordClientEvent`.
pub const CLIENT_PING_EVENTS: &[&str] = &[EVENT_APP_LAUNCHED, EVENT_SURFACE_OPENED];

const MAX_VERSION_LENGTH: usize = 32;

/// The only shapes a property value can take. Note the absence of a
/// `String(String)` variant — see the module note.
#[derive(Clone, Debug)]
pub enum PropertyValue {
    Number(f64),
    Bool(bool),
    /// An enum member. `&'static str` by construction, so it can only ever be a
    /// literal that exists in this file (or in another compile-time table).
    Enum(&'static str),
    /// A sorted list of enum members (`agents_used`).
    EnumList(Vec<&'static str>),
    /// The one clamped free-form field: a marketing version string.
    Version(String),
}

/// What a given `(event, key)` pair is allowed to carry.
#[derive(Clone, Copy, Debug)]
enum PropertySpec {
    Number,
    Enum(&'static [&'static str]),
    EnumList(&'static [&'static str]),
    Version,
}

pub fn is_known_event(event: &str) -> bool {
    matches!(
        event,
        EVENT_HEARTBEAT
            | EVENT_APP_LAUNCHED
            | EVENT_CLIENT_CONNECTED
            | EVENT_SESSION_STARTED
            | EVENT_PROMPT_SENT
            | EVENT_SURFACE_OPENED
            | EVENT_EXTENSION_INSTALLED
            | EVENT_EXTENSION_UNINSTALLED
    )
}

pub fn is_client_ping_event(event: &str) -> bool {
    CLIENT_PING_EVENTS.contains(&event)
}

/// Collapse any agent id to something sendable: a catalog id, else `custom`.
pub fn normalize_agent_id(agent_id: Option<&str>) -> &'static str {
    let Some(agent_id) = agent_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return CUSTOM_AGENT_ID;
    };
    KNOWN_AGENT_IDS
        .iter()
        .copied()
        .find(|known| known.eq_ignore_ascii_case(agent_id))
        .unwrap_or(CUSTOM_AGENT_ID)
}

/// Resolve an arbitrary runtime string against a compile-time enum table,
/// yielding the table's own `&'static str` so nothing runtime-owned escapes.
pub fn match_enum(table: &'static [&'static str], value: &str) -> Option<&'static str> {
    let value = value.trim();
    table.iter().copied().find(|member| *member == value)
}

fn property_spec(event: &str, key: &str) -> Option<PropertySpec> {
    match (event, key) {
        (EVENT_HEARTBEAT, "project_count")
        | (EVENT_HEARTBEAT, "session_count")
        | (EVENT_HEARTBEAT, "running_session_count")
        | (EVENT_HEARTBEAT, "extension_count")
        | (EVENT_HEARTBEAT, "remote_machine_count")
        | (EVENT_HEARTBEAT, "days_since_install") => Some(PropertySpec::Number),
        (EVENT_HEARTBEAT, "agents_used") => Some(PropertySpec::EnumList(KNOWN_AGENT_IDS)),
        (EVENT_HEARTBEAT, "default_agent") => Some(PropertySpec::Enum(KNOWN_AGENT_IDS)),
        (EVENT_HEARTBEAT, "preferred_interface") => Some(PropertySpec::Enum(INTERFACE_KINDS)),
        (EVENT_HEARTBEAT, "sidebar_version") => Some(PropertySpec::Enum(SIDEBAR_VERSIONS)),
        (EVENT_HEARTBEAT, "sidebar_v2_layout") => Some(PropertySpec::Enum(SIDEBAR_V2_LAYOUTS)),
        (EVENT_APP_LAUNCHED, "client") => Some(PropertySpec::Enum(&["desktop"])),
        (EVENT_APP_LAUNCHED, "app_version") => Some(PropertySpec::Version),
        (EVENT_CLIENT_CONNECTED, "client") => Some(PropertySpec::Enum(CLIENT_KINDS)),
        (EVENT_SESSION_STARTED, "agent") => Some(PropertySpec::Enum(KNOWN_AGENT_IDS)),
        (EVENT_PROMPT_SENT, "agent") => Some(PropertySpec::Enum(KNOWN_AGENT_IDS)),
        (EVENT_PROMPT_SENT, "source") => Some(PropertySpec::Enum(PROMPT_SOURCES)),
        (EVENT_SURFACE_OPENED, "surface") => Some(PropertySpec::Enum(SURFACES)),
        (EVENT_EXTENSION_INSTALLED, "source") => Some(PropertySpec::Enum(EXTENSION_SOURCES)),
        _ => None,
    }
}

/// A marketing version is the only string a caller supplies verbatim, so it is
/// clamped to a semver-ish charset and length. Nothing user-authored can hide
/// in `[0-9A-Za-z.-]{1,32}`.
pub fn normalize_version_string(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_VERSION_LENGTH {
        return None;
    }
    if !value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '.' || character == '-')
    {
        return None;
    }
    Some(value.to_string())
}

/// Turn a validated property into JSON, or `None` when it violates the table.
fn encode_property(spec: PropertySpec, value: &PropertyValue) -> Option<Value> {
    match (spec, value) {
        (PropertySpec::Number, PropertyValue::Number(number)) => {
            Number::from_f64(*number).map(Value::Number)
        }
        (PropertySpec::Enum(table), PropertyValue::Enum(member)) => table
            .contains(member)
            .then(|| Value::String((*member).to_string())),
        (PropertySpec::EnumList(table), PropertyValue::EnumList(members)) => members
            .iter()
            .all(|member| table.contains(member))
            .then(|| {
                Value::Array(
                    members
                        .iter()
                        .map(|member| Value::String((*member).to_string()))
                        .collect(),
                )
            }),
        (PropertySpec::Version, PropertyValue::Version(version)) => {
            normalize_version_string(version).map(Value::String)
        }
        _ => None,
    }
}

/// The chokepoint. `Err` carries the offending key (or the event name) purely so
/// the caller can debug-log which rule rejected the capture; it never carries a
/// value.
pub fn validate(
    event: &str,
    properties: &[(&'static str, PropertyValue)],
) -> Result<Map<String, Value>, String> {
    if !is_known_event(event) {
        return Err(format!("unknown event {event}"));
    }
    let mut encoded = Map::new();
    for (key, value) in properties {
        let Some(spec) = property_spec(event, key) else {
            return Err(format!("{event} does not accept property {key}"));
        };
        let Some(json) = encode_property(spec, value) else {
            return Err(format!("{event}.{key} has a value outside its allowed set"));
        };
        encoded.insert((*key).to_string(), json);
    }
    Ok(encoded)
}
