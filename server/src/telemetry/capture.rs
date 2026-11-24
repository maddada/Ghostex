/*
CDXC:Telemetry 2026-08-26:
The emitter surface every instrumentation site calls. One function per event, so
a call site never assembles a property map by hand and therefore never has the
opportunity to put a string of its own into one.

Each of these is fire-and-forget and cheap: a gate check, a validation, and a
push onto an in-memory queue. None of them touch the network, the disk, or
SQLite, so they are safe to call from inside a request handler or a DB write.
*/

use std::{
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

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
pub const RELIABILITY_ERROR_THROTTLE: Duration = Duration::from_secs(5 * 60);

/// How long an observed mobile `client-hello` suppresses the legacy keep-awake
/// `client_os = null` fallback. 11.2.1+ phones always send `ghostex client-hello`
/// with their real OS (`android`/`ios`), while keep-awake renews every 60s and
/// can otherwise race ahead of the hourly hello after a restart or window expiry.
const MOBILE_HELLO_SUPPRESS_DAYS: i64 = 30;
/// Grace window on a fresh install before emitting the legacy `client_os = null`
/// fallback, so an initial keep-awake renewal that arrives a second before
/// `client-hello` cannot double-fire. Pre-2026-09-03 phones renew every 60s and
/// still emit on their second renewal.
const LEGACY_MOBILE_GRACE: Duration = Duration::from_secs(30);

#[derive(Default)]
struct MobileHelloState {
    loaded_from_db: bool,
    last_hello_utc: Option<chrono::DateTime<chrono::Utc>>,
    first_legacy_seen: Option<Instant>,
}

fn mobile_hello_state() -> &'static Mutex<MobileHelloState> {
    static STATE: OnceLock<Mutex<MobileHelloState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(MobileHelloState::default()))
}

fn load_mobile_hello_if_needed(state: &mut MobileHelloState) {
    if state.loaded_from_db {
        return;
    }
    state.loaded_from_db = true;
    let Some(telemetry) = super::queue::handle() else {
        return;
    };
    if let Ok(db) = crate::storage::open_gxserver_database(telemetry.paths()) {
        state.last_hello_utc = super::state::read_last_mobile_hello_at(&db);
    }
}

fn note_mobile_hello_seen() {
    let now = chrono::Utc::now();
    let should_persist = {
        let mut state = mobile_hello_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        load_mobile_hello_if_needed(&mut state);
        let stale = state
            .last_hello_utc
            .is_none_or(|previous| now.signed_duration_since(previous).num_minutes() >= 55);
        state.last_hello_utc = Some(now);
        state.loaded_from_db = true;
        stale
    };
    if !should_persist {
        return;
    }
    let Some(telemetry) = super::queue::handle() else {
        return;
    };
    if !telemetry.is_enabled() {
        return;
    }
    if let Ok(db) = crate::storage::open_gxserver_database(telemetry.paths()) {
        super::state::write_last_mobile_hello_at(
            &db,
            &now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        );
    }
}

/// Clear in-memory mobile-hello memory when analytics are disabled.
pub(super) fn reset_mobile_hello_cache() {
    if let Ok(mut state) = mobile_hello_state().lock() {
        *state = MobileHelloState::default();
    }
}

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

/// A web or mobile client attached. Throttled per `(client, client_os)` so a
/// legacy `client_os = None` keep-awake ping can never suppress an explicit
/// `client-hello` carrying `android`/`ios`, and an iPhone plus Android phone
/// using the same machine both report cleanly.
pub fn client_connected(client: &'static str, platform: ClientPlatform) {
    if client == "mobile" && platform.os.is_some() {
        note_mobile_hello_seen();
    }
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
    let throttle_key = format!("{client}:{}", platform.os.unwrap_or("unknown"));
    capture_throttled(
        taxonomy::EVENT_CLIENT_CONNECTED,
        &throttle_key,
        CLIENT_CONNECTED_THROTTLE,
        &properties,
    );
}

/// Legacy mobile attach signal inferred from a `mobile-*` keep-awake lease.
/// Suppressed whenever this install has seen an explicit `ghostex client-hello`
/// (all 11.2.1+ phones) within 30 days, and delayed briefly on first sight so a
/// keep-awake lease racing ahead of `client-hello` on connect never emits a
/// duplicate `client_os = null` event. Pre-2026-09-03 phones without
/// `client-hello` still report on their next 60s renewal.
pub fn mobile_keep_awake_connected() {
    let should_emit = {
        let mut state = mobile_hello_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        load_mobile_hello_if_needed(&mut state);
        let recent_hello = state.last_hello_utc.is_some_and(|seen| {
            chrono::Utc::now().signed_duration_since(seen).num_days() < MOBILE_HELLO_SUPPRESS_DAYS
        });
        if recent_hello {
            false
        } else {
            let now = Instant::now();
            match state.first_legacy_seen {
                None => {
                    state.first_legacy_seen = Some(now);
                    false
                }
                Some(first) => now.duration_since(first) >= LEGACY_MOBILE_GRACE,
            }
        }
    };
    if should_emit {
        client_connected("mobile", ClientPlatform::default());
    }
}

/// Zero-PII onboarding funnel milestone (`taxonomy::ONBOARDING_STEPS`).
pub fn onboarding_step(step: &'static str) {
    capture(
        taxonomy::EVENT_ONBOARDING_STEP,
        &[("step", PropertyValue::Enum(step))],
    );
}

/// Zero-PII reliability failure counter (`taxonomy::RELIABILITY_ERROR_CATEGORIES`).
/// Throttled per `(category, agent_id)` so a retry loop never floods the queue.
pub fn reliability_error(category: &'static str, agent_id: Option<&str>) {
    let normalized_agent = agent_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| taxonomy::normalize_agent_id(Some(value)));
    let mut properties = vec![("category", PropertyValue::Enum(category))];
    if let Some(agent) = normalized_agent {
        properties.push(("agent_id", PropertyValue::Enum(agent)));
    }
    let throttle_key = format!("{category}:{}", normalized_agent.unwrap_or("none"));
    capture_throttled(
        taxonomy::EVENT_RELIABILITY_ERROR,
        &throttle_key,
        RELIABILITY_ERROR_THROTTLE,
        &properties,
    );
}

/// [`reliability_error`] resolving the agent id from a session row, omitting
/// `agent_id` for plain terminal sessions.
pub fn reliability_error_for_session(category: &'static str, session: &Value) {
    let has_agent = session
        .get("agentId")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
        || session
            .pointer("/runtimeSettings/agentCommand")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty());
    let agent = has_agent.then(|| session_agent_report(session).agent);
    reliability_error(category, agent);
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
    // A range, not the number, and no raw total beside it that would give the number back.
    properties.push((
        "custom_boost_count",
        PropertyValue::Enum(taxonomy::count_bucket(custom_count)),
    ));
    properties.push(("boosts", PropertyValue::EnumList(built_in)));
    capture(taxonomy::EVENT_BOOSTS_USED, &properties);
}

/// The sidebar's layout changed (Settings, or the Meet the rail intro), forwarded from
/// `/api/recordClientEvent`.
pub fn sidebar_layout_changed(layout: &'static str, source: &'static str) {
    capture(
        taxonomy::EVENT_SIDEBAR_LAYOUT_CHANGED,
        &[
            ("layout", PropertyValue::Enum(layout)),
            ("source", PropertyValue::Enum(source)),
        ],
    );
}

/// The one-time Meet the rail pop-up opened on `layout`.
pub fn rail_intro_shown(layout: &'static str) {
    capture(
        taxonomy::EVENT_RAIL_INTRO_SHOWN,
        &[("layout", PropertyValue::Enum(layout))],
    );
}

/// The Meet the rail pop-up closed keeping `chose`.
pub fn rail_intro_answered(chose: &'static str) {
    capture(
        taxonomy::EVENT_RAIL_INTRO_ANSWERED,
        &[("chose", PropertyValue::Enum(chose))],
    );
}

/// Per-install effective session mode setting for every Native-capable agent family, plus
/// preferred view. Values are strictly closed enum members (`"native"` / `"terminal"`, `"chat"` /
/// `"terminal"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstallSessionModes {
    pub preferred_view: &'static str,
    pub antigravity: &'static str,
    pub claude: &'static str,
    pub codex: &'static str,
}

impl InstallSessionModes {
    pub fn native_agent_count(&self) -> usize {
        [self.antigravity, self.claude, self.codex]
            .into_iter()
            .filter(|mode| *mode == "native")
            .count()
    }
}

pub fn resolve_install_session_modes(settings: Option<&Value>) -> InstallSessionModes {
    let map = settings.and_then(Value::as_object);
    let preferred_view = map
        .and_then(|map| map.get("preferredAgentInterface"))
        .and_then(Value::as_str)
        .and_then(|value| taxonomy::match_enum(taxonomy::INTERFACE_KINDS, value))
        .unwrap_or("chat");
    let mode_for = |family: &str| {
        if map.is_some_and(|map| crate::acp::agent_acp_enabled(map, family)) {
            "native"
        } else {
            "terminal"
        }
    };
    InstallSessionModes {
        preferred_view,
        antigravity: mode_for("antigravity"),
        claude: mode_for("claude"),
        codex: mode_for("codex"),
    }
}

/// Per-install session mode state, emitted on app start and whenever `agentSessionMode` or
/// `preferredAgentInterface` changes. Also refreshes the person profile (`$set`).
pub fn session_mode_state(trigger: &'static str, client: &'static str, modes: InstallSessionModes) {
    let person = PropertyValue::PersonSet(vec![
        ("interface", PropertyValue::Enum(modes.preferred_view)),
        ("antigravity_mode", PropertyValue::Enum(modes.antigravity)),
        ("claude_mode", PropertyValue::Enum(modes.claude)),
        ("codex_mode", PropertyValue::Enum(modes.codex)),
    ]);
    capture(
        taxonomy::EVENT_SESSION_MODE_STATE,
        &[
            ("trigger", PropertyValue::Enum(trigger)),
            ("client", PropertyValue::Enum(client)),
            ("preferred_view", PropertyValue::Enum(modes.preferred_view)),
            ("antigravity_mode", PropertyValue::Enum(modes.antigravity)),
            ("claude_mode", PropertyValue::Enum(modes.claude)),
            ("codex_mode", PropertyValue::Enum(modes.codex)),
            (
                "native_agent_count",
                PropertyValue::Number(modes.native_agent_count() as f64),
            ),
            (taxonomy::PERSON_SET_KEY, person),
        ],
    );
}

/// Throttled `session_mode.state` for mobile connect (`client.connected` shares the same 1h window).
pub fn session_mode_state_throttled(
    trigger: &'static str,
    client: &'static str,
    modes: InstallSessionModes,
) {
    let person = PropertyValue::PersonSet(vec![
        ("interface", PropertyValue::Enum(modes.preferred_view)),
        ("antigravity_mode", PropertyValue::Enum(modes.antigravity)),
        ("claude_mode", PropertyValue::Enum(modes.claude)),
        ("codex_mode", PropertyValue::Enum(modes.codex)),
    ]);
    capture_throttled(
        taxonomy::EVENT_SESSION_MODE_STATE,
        &format!("{client}:{trigger}"),
        CLIENT_CONNECTED_THROTTLE,
        &[
            ("trigger", PropertyValue::Enum(trigger)),
            ("client", PropertyValue::Enum(client)),
            ("preferred_view", PropertyValue::Enum(modes.preferred_view)),
            ("antigravity_mode", PropertyValue::Enum(modes.antigravity)),
            ("claude_mode", PropertyValue::Enum(modes.claude)),
            ("codex_mode", PropertyValue::Enum(modes.codex)),
            (
                "native_agent_count",
                PropertyValue::Number(modes.native_agent_count() as f64),
            ),
            (taxonomy::PERSON_SET_KEY, person),
        ],
    );
}

/// One Terminal <-> Native switch (live session switch or Settings mode toggle). Also accumulates
/// into the daily rollup counter when analytics are enabled.
pub fn session_mode_switched(
    agent: &'static str,
    direction: &'static str,
    source: &'static str,
    client: &'static str,
) {
    let Some(telemetry) = super::queue::handle() else {
        return;
    };
    if !telemetry.is_enabled() {
        return;
    }
    super::state::record_pending_switch(direction, source);
    capture(
        taxonomy::EVENT_SESSION_MODE_SWITCHED,
        &[
            ("agent", PropertyValue::Enum(agent)),
            ("direction", PropertyValue::Enum(direction)),
            ("source", PropertyValue::Enum(source)),
            ("client", PropertyValue::Enum(client)),
        ],
    );
}

/// Live (running) sessions across Native-capable families, split by Native vs Terminal mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LiveSessionModeCounts {
    pub antigravity_native: usize,
    pub antigravity_terminal: usize,
    pub claude_native: usize,
    pub claude_terminal: usize,
    pub codex_native: usize,
    pub codex_terminal: usize,
}

impl LiveSessionModeCounts {
    pub fn record(&mut self, family: &str, is_native: bool) {
        match (family, is_native) {
            ("antigravity", true) => self.antigravity_native += 1,
            ("antigravity", false) => self.antigravity_terminal += 1,
            ("claude", true) => self.claude_native += 1,
            ("claude", false) => self.claude_terminal += 1,
            ("codex", true) => self.codex_native += 1,
            ("codex", false) => self.codex_terminal += 1,
            _ => {}
        }
    }

    pub fn native_total(&self) -> usize {
        self.antigravity_native + self.claude_native + self.codex_native
    }

    pub fn terminal_total(&self) -> usize {
        self.antigravity_terminal + self.claude_terminal + self.codex_terminal
    }
}

/// Daily rollup of live sessions by agent family x mode and switches by direction x entry point.
pub fn session_mode_daily_rollup(
    modes: InstallSessionModes,
    live: LiveSessionModeCounts,
    switches: super::state::SessionModeSwitchCounts,
) {
    capture(
        taxonomy::EVENT_SESSION_MODE_DAILY_ROLLUP,
        &[
            ("preferred_view", PropertyValue::Enum(modes.preferred_view)),
            ("antigravity_mode", PropertyValue::Enum(modes.antigravity)),
            ("claude_mode", PropertyValue::Enum(modes.claude)),
            ("codex_mode", PropertyValue::Enum(modes.codex)),
            (
                "live_antigravity_native",
                PropertyValue::Number(live.antigravity_native as f64),
            ),
            (
                "live_antigravity_terminal",
                PropertyValue::Number(live.antigravity_terminal as f64),
            ),
            (
                "live_claude_native",
                PropertyValue::Number(live.claude_native as f64),
            ),
            (
                "live_claude_terminal",
                PropertyValue::Number(live.claude_terminal as f64),
            ),
            (
                "live_codex_native",
                PropertyValue::Number(live.codex_native as f64),
            ),
            (
                "live_codex_terminal",
                PropertyValue::Number(live.codex_terminal as f64),
            ),
            (
                "live_native_total",
                PropertyValue::Number(live.native_total() as f64),
            ),
            (
                "live_terminal_total",
                PropertyValue::Number(live.terminal_total() as f64),
            ),
            (
                "switches_to_native_pill",
                PropertyValue::Number(switches.to_native_pill as f64),
            ),
            (
                "switches_to_native_more_menu",
                PropertyValue::Number(switches.to_native_more_menu as f64),
            ),
            (
                "switches_to_native_bulk_modal",
                PropertyValue::Number(switches.to_native_bulk_modal as f64),
            ),
            (
                "switches_to_native_settings",
                PropertyValue::Number(switches.to_native_settings as f64),
            ),
            (
                "switches_to_native_other",
                PropertyValue::Number(switches.to_native_other as f64),
            ),
            (
                "switches_to_native_total",
                PropertyValue::Number(switches.to_native_total() as f64),
            ),
            (
                "switches_to_terminal_more_menu",
                PropertyValue::Number(switches.to_terminal_more_menu as f64),
            ),
            (
                "switches_to_terminal_settings",
                PropertyValue::Number(switches.to_terminal_settings as f64),
            ),
            (
                "switches_to_terminal_other",
                PropertyValue::Number(switches.to_terminal_other as f64),
            ),
            (
                "switches_to_terminal_total",
                PropertyValue::Number(switches.to_terminal_total() as f64),
            ),
        ],
    );
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
