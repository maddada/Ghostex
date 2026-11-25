/*
CDXC:Telemetry 2026-08-26:
The closed event/property table from `docs/2026-08-26/anonymous-analytics/PLAN.md`.

This module is the reason the privacy promise is structural rather than a review
convention. A property value can only be a number, a bool, an enum string picked
from a compile-time `&'static str` set, a list of such enum strings, or the one
deliberately clamped free-form shape: a marketing version (`app_version`),
charset- and length-clamped. There is no `PropertyValue` variant that carries a
runtime-owned `String` a caller could fill with a prompt, a path, a custom
executable name, or a project name, so "someone adds a field with user text in
it" is a compile error, not a bug.
The one runtime string added since (2026-10-27) is an official Store extension
id inside `PropertyValue::Extension`/`ExtensionList`, and `encode_property`
re-checks it against the Store's own catalog, so it cannot carry a name the
Store did not publish (`telemetry::extensions`).

That property survived the addendum-v2 restructure intact. Person profiles are
now ON, so the heartbeat carries a `$set` object — but `$set` is modelled as
`PropertyValue::PersonSet`, a nested list of the SAME key/value pairs, checked
by the SAME `encode_property` against `person_property_spec`. It is not a
`serde_json::Map` a caller can fill in, so turning person profiles on did not
open a hole for free-form text.

Anything that does not match the table below is dropped with a debug log.
*/

use serde_json::{Map, Number, Value};

/// Every agent id the catalog knows (`server/src/agents/helpers.rs`,
/// `packages/shared/sidebar-agents.ts` (deleted 2026-10-01)), plus the literal `custom` bucket every
/// unknown — i.e. user-authored — agent id collapses into.
pub const KNOWN_AGENT_IDS: &[&str] = &[
    "amp",
    "antigravity",
    "claude",
    "codebuddy",
    "codex",
    "command-code",
    "copilot",
    "cursor",
    "custom",
    "mastra",
    "devin",
    "droid",
    "dsh",
    "empryo",
    "freebuff",
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
    "zcode",
];

/// The bucket an unknown agent id becomes. Custom agent ids are user-authored
/// text and must never leave the machine.
pub const CUSTOM_AGENT_ID: &str = "custom";

/*
CDXC:Telemetry 2026-11-24 (privacy overhaul, Q23):
Custom agent commands are inspected locally ONLY to map known catalog agent
binaries (`claude`, `codex`, `cursor-agent`, `agy`, ...) back to their catalog
id. If the executable does not match a known catalog agent, it collapses strictly
to `"custom"` and the raw binary basename NEVER leaves the machine (preventing
internal company wrapper script names from leaking).

Wrapper launchers (`npx`, `uvx`, `bun`, `node`, ...) are skipped locally so that
`npx @anthropic-ai/claude-code` or `env claude --profile work` still maps to
its catalog id when recognized.
*/
const COMMAND_LAUNCHERS: &[&str] = &[
    "bash", "bun", "bunx", "cargo", "deno", "env", "node", "npm", "npx", "pipx", "pnpm", "pnpx",
    "python", "python3", "sh", "sudo", "uv", "uvx", "yarn", "zsh",
];
/// Sub-verbs a launcher takes before the real executable (`uv run x`,
/// `pnpm dlx x`, `npm exec x`).
const COMMAND_LAUNCHER_VERBS: &[&str] = &["dlx", "exec", "run"];
const COMMAND_EXECUTABLE_SUFFIXES: &[&str] = &[".exe", ".cmd", ".bat"];

pub const PROMPT_SOURCES: &[&str] = &[
    "automation",
    "board",
    "chat",
    "cli",
    "queue",
    "quick_action",
    "terminal",
];

/*
CDXC:Telemetry 2026-10-27:
User: "which views being accessesd the most". The original nine surfaces left
out most of the views a user can open in the side panel (Changes, Terminal, the
Bot automations feed, the Work page, Claude, every extension view) and the
phone entirely, so "most used view" could not be answered. The names below are
the views as users know them; `docs` (Files), `code` (VS Code) and
`extensions_store` keep their 2026-08-26 spelling so older events still group
with newer ones. `extension` always travels with an `extension` property
naming the built-in or official Store id, or `custom`
(`telemetry::extensions`). The `mobile_*` members are the phone's screens and
travel with `client: mobile`.
*/
pub const SURFACES: &[&str] = &[
    "agents",
    "agents_hub",
    "automate",
    "automations",
    "browser",
    "changes",
    "claude",
    "code",
    "docs",
    "extension",
    "extensions_store",
    "find",
    "kanban",
    "mobile_add_project",
    "mobile_beads",
    "mobile_browser",
    "mobile_changes",
    "mobile_files",
    "mobile_find",
    "mobile_machines",
    "mobile_sessions",
    "mobile_settings",
    "mobile_terminal",
    "mobile_voice",
    "previous_sessions",
    "quick_access",
    "settings",
    "terminal",
    "work",
];

/// Which app opened a surface. Absent on desktop events sent before 2026-10-27.
pub const SURFACE_CLIENTS: &[&str] = &["desktop", "mobile"];

pub const CLIENT_KINDS: &[&str] = &["mobile", "web"];

/*
CDXC:Telemetry 2026-09-03:
The OS a web or mobile CLIENT runs on, as opposed to `platform`, which is the
machine gxserver itself runs on. Before this table every mobile attach looked
identical, so "how many people use the phone app, on which OS" had no answer.
Mobile sends its OS explicitly through `ghostex client-hello`; the web app's OS
is derived server-side from the browser's User-Agent family. `other` is the
bucket for anything the derivation does not recognise, so a new browser or OS
cannot leak its raw UA into the dimension.
*/
pub const CLIENT_PLATFORMS: &[&str] = &[
    "android", "chromeos", "ios", "linux", "macos", "other", "windows",
];

/// The subset of `CLIENT_PLATFORMS` a mobile ping may claim.
pub const MOBILE_PLATFORMS: &[&str] = &["android", "ios"];

pub const EXTENSION_SOURCES: &[&str] = &["local", "store"];

pub const INTERFACE_KINDS: &[&str] = &["chat", "terminal"];

/// The bucketed project count carried on EVERY event. The raw number is a
/// heartbeat-only property: bucketing keeps a high-cardinality per-user integer
/// off every single event while still separating power users from light ones.
/// Listed in ordinal order rather than the alphabetical order the other tables
/// use, because these members are a scale and reading them out of order makes
/// the gaps hard to check. Membership is order-independent either way.
pub const PROJECT_BUCKETS: &[&str] = &["0", "1-2", "3-5", "6-10", "10+"];

/// CDXC:Telemetry 2026-11-20 (privacy review of 591004b37): the small counts the Boosts event and
/// the settings snapshot carry (accounts per provider, workspaces, the user's own Boosts, edited
/// Boost prompts, extensions not from the Store) go as these ranges, so an outlier such as 137
/// accounts cannot single an install out. Ordinal order, like `PROJECT_BUCKETS`.
pub const COUNT_BUCKETS: &[&str] = &["0", "1", "2", "3", "4-5", "6-10", "11+"];

pub fn count_bucket(count: usize) -> &'static str {
    match count {
        0 => "0",
        1 => "1",
        2 => "2",
        3 => "3",
        4..=5 => "4-5",
        6..=10 => "6-10",
        _ => "11+",
    }
}

/// `std::env::consts::OS` values we recognise. A platform outside this table is
/// simply omitted rather than sent, which is why the table does not need to be
/// exhaustive over every Rust target.
pub const PLATFORMS: &[&str] = &[
    "android",
    "dragonfly",
    "freebsd",
    "ios",
    "linux",
    "macos",
    "netbsd",
    "openbsd",
    "solaris",
    "windows",
];

/// `std::env::consts::ARCH` values we recognise, same rule as `PLATFORMS`.
pub const ARCHES: &[&str] = &[
    "aarch64",
    "arm",
    "loongarch64",
    "powerpc",
    "powerpc64",
    "riscv32",
    "riscv64",
    "s390x",
    "x86",
    "x86_64",
];

/*
CDXC:Telemetry 2026-11-20:
User: "add PostHog tracking for Boosts used in a message and for extensions ...
also for settings like transparency, which Linux distro is used, and other
useful settings; how many accounts people have and which subscription; what
kind of sidebar people use and their sidebar settings". The tables below are
the closed sets those answers may take. Anything outside them is `other` (a
distro, a desktop) or left out (a plan), never the raw text: `/etc/os-release`
and `XDG_CURRENT_DESKTOP` are machine-provided, but a hand-built distro or a
custom session name is still free text.
*/

/// `ID=` from `/etc/os-release`, for the distributions Ghostex users are likely to run.
pub const LINUX_DISTROS: &[&str] = &[
    "almalinux",
    "alpine",
    "arch",
    "bazzite",
    "cachyos",
    "centos",
    "debian",
    "elementary",
    "endeavouros",
    "fedora",
    "garuda",
    "gentoo",
    "kali",
    "linuxmint",
    "manjaro",
    "neon",
    "nixos",
    "nobara",
    "opensuse-leap",
    "opensuse-tumbleweed",
    "other",
    "pop",
    "raspbian",
    "rhel",
    "rocky",
    "solus",
    "tuxedo",
    "ubuntu",
    "void",
    "zorin",
];

/// The desktop environment or compositor, from `XDG_CURRENT_DESKTOP` (or
/// `XDG_SESSION_DESKTOP`), lowercased with any `X-` prefix removed.
pub const LINUX_DESKTOPS: &[&str] = &[
    "budgie", "cinnamon", "cosmic", "deepin", "gnome", "hyprland", "i3", "kde", "labwc", "lxde",
    "lxqt", "mate", "niri", "other", "pantheon", "river", "sway", "unity", "wayfire", "xfce",
];

/// `XDG_SESSION_TYPE`.
pub const LINUX_SESSION_TYPES: &[&str] = &["other", "tty", "wayland", "x11"];

/*
The plan an account's provider reports, as Claude Swap and Codex Swap list it
(`cswap list --json` `plan.organizationType` + `plan.tier`, `xswap list --json`
`plan`). Never the account's email, name, organization or id.
*/
pub const CLAUDE_PLANS: &[&str] = &[
    "enterprise",
    "free",
    "max",
    "max_20x",
    "max_5x",
    "other",
    "pro",
    "team",
];
pub const CODEX_PLANS: &[&str] = &[
    "business",
    "edu",
    "enterprise",
    "free",
    "go",
    "other",
    "plus",
    "pro",
    "prolite",
    "team",
];

/// The same plans from best to least, so `claude_top_plan` and `codex_top_plan`
/// name the best plan among an install's accounts.
pub const CLAUDE_PLAN_RANK: &[&str] = &[
    "enterprise",
    "max_20x",
    "max",
    "max_5x",
    "team",
    "pro",
    "free",
    "other",
];
pub const CODEX_PLAN_RANK: &[&str] = &[
    "enterprise",
    "business",
    "pro",
    "team",
    "edu",
    "prolite",
    "plus",
    "go",
    "free",
    "other",
];

/// Where a built-in extension's on/off change was noticed: the daily snapshot
/// compares the built-in extensions that are on with the last one it sent.
pub const EXTENSION_TOGGLE_KINDS: &[&str] = &["built_in", "installed"];

/// Event names. Only these reach PostHog.
pub const EVENT_HEARTBEAT: &str = "heartbeat";
pub const EVENT_INSTALL_WEEKLY_COUNT: &str = "install.weekly_count";
pub const EVENT_APP_LAUNCHED: &str = "app.launched";
pub const EVENT_CLIENT_CONNECTED: &str = "client.connected";
pub const EVENT_SESSION_STARTED: &str = "session.started";
pub const EVENT_PROMPT_SENT: &str = "prompt.sent";
pub const EVENT_SURFACE_OPENED: &str = "surface.opened";
pub const EVENT_EXTENSION_INSTALLED: &str = "extension.installed";
pub const EVENT_EXTENSION_UNINSTALLED: &str = "extension.uninstalled";
pub const EVENT_EXTENSION_ENABLED: &str = "extension.enabled";
pub const EVENT_EXTENSION_DISABLED: &str = "extension.disabled";
pub const EVENT_BOOSTS_USED: &str = "boosts.used";
pub const EVENT_SETTINGS_SNAPSHOT: &str = "settings.snapshot";
pub const EVENT_SIDEBAR_LAYOUT_CHANGED: &str = "sidebar_layout.changed";
pub const EVENT_RAIL_INTRO_SHOWN: &str = "rail_intro.shown";
pub const EVENT_RAIL_INTRO_ANSWERED: &str = "rail_intro.answered";
pub const EVENT_ONBOARDING_STEP: &str = "onboarding.step";
pub const EVENT_RELIABILITY_ERROR: &str = "reliability.error";
pub const EVENT_SESSION_MODE_STATE: &str = "session_mode.state";
pub const EVENT_SESSION_MODE_SWITCHED: &str = "session_mode.switched";
pub const EVENT_SESSION_MODE_DAILY_ROLLUP: &str = "session_mode.daily_rollup";

/// Zero-PII onboarding funnel milestones emitted by desktop onboarding.
pub const ONBOARDING_STEPS: &[&str] = &[
    "cli_detected",
    "completed",
    "first_prompt",
    "first_session",
    "project_added",
    "project_opened",
    "skipped",
    "started",
];

/// Closed set of anonymous reliability failure categories. Never carries raw
/// paths, prompts, error messages, or stack traces.
pub const RELIABILITY_ERROR_CATEGORIES: &[&str] = &[
    "cli_not_found",
    "daemon_bootstrap_failed",
    "loopback_telemetry_failed",
    "mobile_connect_failed",
    "native_chat_init_failed",
    "prompt_delivery_failed",
    "provider_auth_failed",
    "session_launch_failed",
    "update_failed",
    "wmx_start_failed",
    "zmx_attach_failed",
];

/*
CDXC:Telemetry 2026-11-24:
Native mode adoption tracking (no PII: closed enums and counts only).
- `session_mode.state`: per-install setting for each Native-capable agent (`antigravity_mode`,
  `claude_mode`, `codex_mode` in `SESSION_MODES`) plus `preferred_view`, emitted on app start and
  whenever those settings change (also written to `$set` person properties).
- `session_mode.switched`: Terminal <-> Native switches with `agent`, `direction`, `source`
  (`pill`, `more_menu`, `bulk_modal`, `settings`, `unknown`) and `client`.
- `session_mode.daily_rollup`: daily counts of live sessions by agent family x mode plus switch
  counts by direction x entry point since the last rollup.
*/
pub const NATIVE_AGENT_FAMILIES: &[&str] = &[
    "antigravity",
    "claude",
    "codex",
    "cursor",
    "grok",
    "hermes",
    "omp",
    "opencode",
    "pi",
];
pub const SESSION_MODES: &[&str] = &["native", "terminal"];
pub const SESSION_MODE_STATE_TRIGGERS: &[&str] = &["app_start", "changed"];
pub const SESSION_MODE_SWITCH_DIRECTIONS: &[&str] = &["native_to_terminal", "terminal_to_native"];
pub const SESSION_MODE_SWITCH_SOURCES: &[&str] =
    &["bulk_modal", "more_menu", "pill", "settings", "unknown"];
pub const SESSION_MODE_CLIENTS: &[&str] = &["desktop", "mobile", "server"];

/*
CDXC:Telemetry 2026-11-20:
User (rail build brief): "PostHog: which layout each install uses, switches,
and the intro's outcome"; then, for the Meet the rail pop-up: "record the
starting layout, every Switch, and the layout kept on Done". The layout itself
rides the daily `settings.snapshot` (`sidebar_layout`, also on the person);
`sidebar_layout.changed` says which layout a switch went to and whether it came
from Settings or the pop-up's Switch; `rail_intro.shown` carries the layout the
pop-up opened on and `rail_intro.answered` the one kept on Done. Nothing else
about the rail (its pins, clicks or order) is sent.
*/
pub const SIDEBAR_LAYOUTS: &[&str] = &["rail", "classic"];
pub const SIDEBAR_LAYOUT_SOURCES: &[&str] = &["settings", "intro"];

/// PostHog's person-properties key. Overwrite semantics (`$set`), not
/// `$set_once`: the person record should always show the install's CURRENT
/// shape, not whatever it looked like on the day it first reported.
pub const PERSON_SET_KEY: &str = "$set";

/// PostHog's personless-event flag. Attached as `false` to `install.weekly_count`
/// so the minimal weekly active install count never creates or updates a person profile.
pub const PROCESS_PERSON_PROFILE_KEY: &str = "$process_person_profile";

/// The events a client process is allowed to push over the loopback API: the
/// desktop's two pings, plus the mobile hello that `ghostex client-hello`
/// forwards. Everything else is dropped by `/api/recordClientEvent`.
pub const CLIENT_PING_EVENTS: &[&str] = &[
    EVENT_APP_LAUNCHED,
    EVENT_SURFACE_OPENED,
    EVENT_CLIENT_CONNECTED,
    EVENT_SIDEBAR_LAYOUT_CHANGED,
    EVENT_RAIL_INTRO_SHOWN,
    EVENT_RAIL_INTRO_ANSWERED,
    EVENT_ONBOARDING_STEP,
    EVENT_RELIABILITY_ERROR,
    EVENT_SESSION_MODE_SWITCHED,
];

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
    /// A clamped free-form field: a marketing version string.
    Version(String),
    /// An extension by its built-in or official Store id, or `custom`. Checked
    /// against the whitelist again when validated (`telemetry::extensions`).
    Extension(super::extensions::ExtensionRef),
    /// A sorted list of such extensions.
    ExtensionList(Vec<super::extensions::ExtensionRef>),
    /// Official Store extensions with the version installed, sent as `id@version`. Each id is
    /// re-checked against the Store whitelist and each version clamped like `Version`.
    ExtensionVersionList(Vec<(super::extensions::ExtensionRef, String)>),
    /*
    PostHog's `$set` person-properties object, carried by the heartbeat only.
    Nesting `PropertyValue` inside itself is what keeps the privacy guarantee
    intact across person profiles: `$set` is NOT an escape hatch handed a raw
    JSON map, it is a list of the same key/value pairs every other property
    goes through, validated by the same `encode_property` against
    `person_property_spec`. That table never yields `PersonSet`, so the nesting
    is exactly one level deep by construction.
    */
    PersonSet(Vec<(&'static str, PropertyValue)>),
}

/// What a given property key is allowed to carry.
#[derive(Clone, Copy, Debug)]
pub(super) enum PropertySpec {
    Number,
    Bool,
    /// One of the choices Settings itself lists for this settings key
    /// (`telemetry::settings_snapshot::setting_choices`).
    SettingChoice(&'static str),
    /// Ghostex's own Boost ids (gx-chat-core `extras::boosts::BOOSTS`); a user's own Boost is
    /// only ever counted, never named.
    BoostList,
    /// Official Store extension ids with their installed version.
    StoreExtensionVersionList,
    Enum(&'static [&'static str]),
    EnumList(&'static [&'static str]),
    Version,
    /// One extension: built-in, official Store, or `custom`.
    Extension,
    /// Built-in extension ids only.
    BuiltInExtensionList,
    /// Official Store extension ids only.
    StoreExtensionList,
    PersonSet,
}

pub fn is_known_event(event: &str) -> bool {
    matches!(
        event,
        EVENT_HEARTBEAT
            | EVENT_INSTALL_WEEKLY_COUNT
            | EVENT_APP_LAUNCHED
            | EVENT_CLIENT_CONNECTED
            | EVENT_SESSION_STARTED
            | EVENT_PROMPT_SENT
            | EVENT_SURFACE_OPENED
            | EVENT_EXTENSION_INSTALLED
            | EVENT_EXTENSION_UNINSTALLED
            | EVENT_EXTENSION_ENABLED
            | EVENT_EXTENSION_DISABLED
            | EVENT_BOOSTS_USED
            | EVENT_SETTINGS_SNAPSHOT
            | EVENT_SIDEBAR_LAYOUT_CHANGED
            | EVENT_RAIL_INTRO_SHOWN
            | EVENT_RAIL_INTRO_ANSWERED
            | EVENT_ONBOARDING_STEP
            | EVENT_RELIABILITY_ERROR
            | EVENT_SESSION_MODE_STATE
            | EVENT_SESSION_MODE_SWITCHED
            | EVENT_SESSION_MODE_DAILY_ROLLUP
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

/// Resolve a session's agent id and base command into a known catalog id, else
/// `custom`. Custom binary basenames are only checked locally against the
/// catalog and never leave the machine.
pub fn resolve_agent(agent_id: Option<&str>, command: Option<&str>) -> &'static str {
    let agent = normalize_agent_id(agent_id);
    if agent != CUSTOM_AGENT_ID {
        return agent;
    }
    let Some(executable) = command.and_then(command_executable) else {
        return CUSTOM_AGENT_ID;
    };
    catalog_agent_for_executable(&executable).unwrap_or(CUSTOM_AGENT_ID)
}

/// The catalog agent whose shipped command runs `executable`, or whose id IS
/// `executable`. Derived from `agents::default_agent_command` so the catalog
/// stays the single source of truth for "which binary is which agent".
fn catalog_agent_for_executable(executable: &str) -> Option<&'static str> {
    KNOWN_AGENT_IDS
        .iter()
        .copied()
        .filter(|id| *id != CUSTOM_AGENT_ID)
        .find(|id| {
            *id == executable
                || crate::agents::default_agent_command(id)
                    .and_then(command_executable)
                    .is_some_and(|catalog| catalog == executable)
        })
}

/// Reduce a command line to the bare, lowercased, charset-clamped name of the
/// program it runs, used strictly locally to match known catalog agents.
/// `None` when nothing survives the reduction. Never sent over the wire.
fn command_executable(command: &str) -> Option<String> {
    let mut after_launcher = false;
    for token in command.split_whitespace() {
        if token.starts_with('-') || token.contains('=') {
            continue;
        }
        let lowered = token.to_ascii_lowercase();
        let basename = lowered.rsplit(['/', '\\']).next().unwrap_or("");
        if COMMAND_LAUNCHERS.contains(&basename) {
            after_launcher = true;
            continue;
        }
        if after_launcher && COMMAND_LAUNCHER_VERBS.contains(&basename) {
            continue;
        }
        let mut name = basename;
        for suffix in COMMAND_EXECUTABLE_SUFFIXES {
            if let Some(stripped) = name.strip_suffix(suffix) {
                name = stripped;
                break;
            }
        }
        return normalize_token(name);
    }
    None
}

/// Collapse a raw project count into the coarse bucket that rides every event.
/// The raw number stays heartbeat-only: a per-user integer on every single
/// event is high-cardinality and, for someone with an unusual count, close to
/// an identifier.
pub fn project_bucket(project_count: usize) -> &'static str {
    match project_count {
        0 => "0",
        1..=2 => "1-2",
        3..=5 => "3-5",
        6..=10 => "6-10",
        _ => "10+",
    }
}

/// Resolve an arbitrary runtime string against a compile-time enum table,
/// yielding the table's own `&'static str` so nothing runtime-owned escapes.
pub fn match_enum(table: &'static [&'static str], value: &str) -> Option<&'static str> {
    let value = value.trim();
    table.iter().copied().find(|member| *member == value)
}

/*
CDXC:Telemetry 2026-08-27 (addendum v2, §4):
The table is in THREE layers, not one `(event, key)` match, because the profile
fields now ride every event and duplicating a row per event name would mean the
next event added to the taxonomy silently loses them:

- `profile_property_spec`  — keys valid on EVERY event (§3).
- `event_property_spec`    — keys valid only on the named event.
- `person_property_spec`   — keys valid INSIDE the heartbeat's `$set` object.

`property_spec` merges the first two. Where a key appears in both layers (the
heartbeat also reports `default_agent` from its own fresh
read), the specs are identical, so the merge order is irrelevant to validation.
*/
fn profile_property_spec(key: &str) -> Option<PropertySpec> {
    match key {
        "interface" => Some(PropertySpec::Enum(INTERFACE_KINDS)),
        "default_agent" => Some(PropertySpec::Enum(KNOWN_AGENT_IDS)),
        "project_bucket" => Some(PropertySpec::Enum(PROJECT_BUCKETS)),
        _ => None,
    }
}

fn event_property_spec(event: &str, key: &str) -> Option<PropertySpec> {
    match (event, key) {
        (EVENT_HEARTBEAT, "project_count")
        | (EVENT_HEARTBEAT, "session_count")
        | (EVENT_HEARTBEAT, "running_session_count")
        | (EVENT_HEARTBEAT, "extension_count")
        | (EVENT_HEARTBEAT, "remote_machine_count")
        | (EVENT_HEARTBEAT, "days_since_install") => Some(PropertySpec::Number),
        (EVENT_HEARTBEAT, "agents_used") => Some(PropertySpec::EnumList(KNOWN_AGENT_IDS)),
        (EVENT_HEARTBEAT, "custom_extensions_enabled") => Some(PropertySpec::Number),
        (EVENT_HEARTBEAT, "built_in_extensions_on") => Some(PropertySpec::BuiltInExtensionList),
        (EVENT_HEARTBEAT, "store_extensions_enabled") => Some(PropertySpec::StoreExtensionList),
        (EVENT_HEARTBEAT, "preferred_interface") => Some(PropertySpec::Enum(INTERFACE_KINDS)),
        /*
        Person properties are attached ONLY to the heartbeat. Every other event
        would just rewrite the same person record with staler numbers, and a
        `$set` on a high-frequency event is how person properties end up
        flapping in the PostHog UI.
        */
        (EVENT_HEARTBEAT, PERSON_SET_KEY) => Some(PropertySpec::PersonSet),
        (EVENT_APP_LAUNCHED, "client") => Some(PropertySpec::Enum(&["desktop"])),
        (EVENT_APP_LAUNCHED, "app_version") => Some(PropertySpec::Version),
        (EVENT_CLIENT_CONNECTED, "client") => Some(PropertySpec::Enum(CLIENT_KINDS)),
        (EVENT_CLIENT_CONNECTED, "client_os") => Some(PropertySpec::Enum(CLIENT_PLATFORMS)),
        (EVENT_CLIENT_CONNECTED, "client_os_version")
        | (EVENT_CLIENT_CONNECTED, "client_app_version") => Some(PropertySpec::Version),
        (EVENT_SESSION_STARTED, "agent") => Some(PropertySpec::Enum(KNOWN_AGENT_IDS)),
        (EVENT_PROMPT_SENT, "agent") => Some(PropertySpec::Enum(KNOWN_AGENT_IDS)),
        (EVENT_PROMPT_SENT, "source") => Some(PropertySpec::Enum(PROMPT_SOURCES)),
        (EVENT_SURFACE_OPENED, "surface") => Some(PropertySpec::Enum(SURFACES)),
        (EVENT_SURFACE_OPENED, "extension") => Some(PropertySpec::Extension),
        (EVENT_SURFACE_OPENED, "client") => Some(PropertySpec::Enum(SURFACE_CLIENTS)),
        (EVENT_EXTENSION_INSTALLED, "source") => Some(PropertySpec::Enum(EXTENSION_SOURCES)),
        (EVENT_APP_LAUNCHED, "linux_desktop") => Some(PropertySpec::Enum(LINUX_DESKTOPS)),
        (EVENT_APP_LAUNCHED, "linux_session") => Some(PropertySpec::Enum(LINUX_SESSION_TYPES)),
        /*
        CDXC:Telemetry 2026-11-20:
        Which extension, now that installs, uninstalls and on/off changes are
        counted per extension: a built-in id, an official Store id or `custom`
        (re-checked by `ExtensionRef::sendable`), and the version only for an
        official Store extension.
        */
        (EVENT_EXTENSION_INSTALLED, "extension")
        | (EVENT_EXTENSION_UNINSTALLED, "extension")
        | (EVENT_EXTENSION_ENABLED, "extension")
        | (EVENT_EXTENSION_DISABLED, "extension") => Some(PropertySpec::Extension),
        (EVENT_EXTENSION_INSTALLED, "extension_version") => Some(PropertySpec::Version),
        (EVENT_EXTENSION_ENABLED, "kind") | (EVENT_EXTENSION_DISABLED, "kind") => {
            Some(PropertySpec::Enum(EXTENSION_TOGGLE_KINDS))
        }
        (EVENT_BOOSTS_USED, "agent") => Some(PropertySpec::Enum(KNOWN_AGENT_IDS)),
        (EVENT_BOOSTS_USED, "source") => Some(PropertySpec::Enum(PROMPT_SOURCES)),
        (EVENT_BOOSTS_USED, "boosts") => Some(PropertySpec::BoostList),
        (EVENT_BOOSTS_USED, "custom_boost_count") => Some(PropertySpec::Enum(COUNT_BUCKETS)),
        (EVENT_SIDEBAR_LAYOUT_CHANGED, "layout") => Some(PropertySpec::Enum(SIDEBAR_LAYOUTS)),
        (EVENT_SIDEBAR_LAYOUT_CHANGED, "source") => {
            Some(PropertySpec::Enum(SIDEBAR_LAYOUT_SOURCES))
        }
        (EVENT_RAIL_INTRO_SHOWN, "layout") => Some(PropertySpec::Enum(SIDEBAR_LAYOUTS)),
        (EVENT_RAIL_INTRO_ANSWERED, "chose") => Some(PropertySpec::Enum(SIDEBAR_LAYOUTS)),
        (EVENT_ONBOARDING_STEP, "step") => Some(PropertySpec::Enum(ONBOARDING_STEPS)),
        (EVENT_ONBOARDING_STEP, "platform") => Some(PropertySpec::Enum(PLATFORMS)),
        (EVENT_RELIABILITY_ERROR, "category") => {
            Some(PropertySpec::Enum(RELIABILITY_ERROR_CATEGORIES))
        }
        (EVENT_RELIABILITY_ERROR, "agent_id") => Some(PropertySpec::Enum(KNOWN_AGENT_IDS)),
        (EVENT_RELIABILITY_ERROR, "platform") => Some(PropertySpec::Enum(PLATFORMS)),
        (EVENT_SESSION_MODE_STATE, "trigger") => {
            Some(PropertySpec::Enum(SESSION_MODE_STATE_TRIGGERS))
        }
        (EVENT_SESSION_MODE_STATE, "client") => Some(PropertySpec::Enum(SESSION_MODE_CLIENTS)),
        (EVENT_SESSION_MODE_STATE, "preferred_view") => Some(PropertySpec::Enum(INTERFACE_KINDS)),
        (EVENT_SESSION_MODE_STATE, "antigravity_mode")
        | (EVENT_SESSION_MODE_STATE, "claude_mode")
        | (EVENT_SESSION_MODE_STATE, "codex_mode") => Some(PropertySpec::Enum(SESSION_MODES)),
        (EVENT_SESSION_MODE_STATE, "native_agent_count") => Some(PropertySpec::Number),
        (EVENT_SESSION_MODE_STATE, PERSON_SET_KEY) => Some(PropertySpec::PersonSet),
        (EVENT_SESSION_MODE_SWITCHED, "agent") => Some(PropertySpec::Enum(NATIVE_AGENT_FAMILIES)),
        (EVENT_SESSION_MODE_SWITCHED, "direction") => {
            Some(PropertySpec::Enum(SESSION_MODE_SWITCH_DIRECTIONS))
        }
        (EVENT_SESSION_MODE_SWITCHED, "source") => {
            Some(PropertySpec::Enum(SESSION_MODE_SWITCH_SOURCES))
        }
        (EVENT_SESSION_MODE_SWITCHED, "client") => Some(PropertySpec::Enum(SESSION_MODE_CLIENTS)),
        (EVENT_SESSION_MODE_DAILY_ROLLUP, "preferred_view") => {
            Some(PropertySpec::Enum(INTERFACE_KINDS))
        }
        (EVENT_SESSION_MODE_DAILY_ROLLUP, "antigravity_mode")
        | (EVENT_SESSION_MODE_DAILY_ROLLUP, "claude_mode")
        | (EVENT_SESSION_MODE_DAILY_ROLLUP, "codex_mode") => {
            Some(PropertySpec::Enum(SESSION_MODES))
        }
        (EVENT_SESSION_MODE_DAILY_ROLLUP, "live_antigravity_native")
        | (EVENT_SESSION_MODE_DAILY_ROLLUP, "live_antigravity_terminal")
        | (EVENT_SESSION_MODE_DAILY_ROLLUP, "live_claude_native")
        | (EVENT_SESSION_MODE_DAILY_ROLLUP, "live_claude_terminal")
        | (EVENT_SESSION_MODE_DAILY_ROLLUP, "live_codex_native")
        | (EVENT_SESSION_MODE_DAILY_ROLLUP, "live_codex_terminal")
        | (EVENT_SESSION_MODE_DAILY_ROLLUP, "live_native_total")
        | (EVENT_SESSION_MODE_DAILY_ROLLUP, "live_terminal_total")
        | (EVENT_SESSION_MODE_DAILY_ROLLUP, "switches_to_native_pill")
        | (EVENT_SESSION_MODE_DAILY_ROLLUP, "switches_to_native_more_menu")
        | (EVENT_SESSION_MODE_DAILY_ROLLUP, "switches_to_native_bulk_modal")
        | (EVENT_SESSION_MODE_DAILY_ROLLUP, "switches_to_native_settings")
        | (EVENT_SESSION_MODE_DAILY_ROLLUP, "switches_to_native_other")
        | (EVENT_SESSION_MODE_DAILY_ROLLUP, "switches_to_native_total")
        | (EVENT_SESSION_MODE_DAILY_ROLLUP, "switches_to_terminal_more_menu")
        | (EVENT_SESSION_MODE_DAILY_ROLLUP, "switches_to_terminal_settings")
        | (EVENT_SESSION_MODE_DAILY_ROLLUP, "switches_to_terminal_other")
        | (EVENT_SESSION_MODE_DAILY_ROLLUP, "switches_to_terminal_total") => {
            Some(PropertySpec::Number)
        }
        (EVENT_INSTALL_WEEKLY_COUNT, "analytics_enabled")
        | (EVENT_INSTALL_WEEKLY_COUNT, PROCESS_PERSON_PROFILE_KEY) => Some(PropertySpec::Bool),
        (EVENT_SETTINGS_SNAPSHOT, PERSON_SET_KEY) => Some(PropertySpec::PersonSet),
        (EVENT_SETTINGS_SNAPSHOT, key) => super::settings_snapshot::property_spec(key),
        _ => None,
    }
}

fn property_spec(event: &str, key: &str) -> Option<PropertySpec> {
    if event == EVENT_INSTALL_WEEKLY_COUNT {
        return event_property_spec(event, key);
    }
    event_property_spec(event, key).or_else(|| profile_property_spec(key))
}

/// The keys the heartbeat's `$set` object may carry. Note that it deliberately
/// does NOT include `agents_used`: an array makes a poor person property, and
/// it stays an event property. Note also that this table can never return
/// `PersonSet`, so `$set` cannot contain another `$set`.
fn person_property_spec(key: &str) -> Option<PropertySpec> {
    match key {
        "os" => Some(PropertySpec::Enum(PLATFORMS)),
        "arch" => Some(PropertySpec::Enum(ARCHES)),
        "server_version" => Some(PropertySpec::Version),
        "project_count"
        | "session_count"
        | "extension_count"
        | "remote_machine_count"
        | "days_since_install" => Some(PropertySpec::Number),
        "antigravity_mode" | "claude_mode" | "codex_mode" => {
            Some(PropertySpec::Enum(SESSION_MODES))
        }
        // CDXC:Telemetry 2026-11-20: the few snapshot fields worth a breakdown on the person.
        key if super::settings_snapshot::PERSON_KEYS.contains(&key) => {
            super::settings_snapshot::property_spec(key)
        }
        /*
        `interface`, `default_agent`, and `project_bucket` reuse the profile
        table verbatim rather than being restated, so a change to an enum member
        cannot apply to the event copy of a field and not to the person copy.
        */
        _ => profile_property_spec(key),
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

const MAX_TOKEN_LENGTH: usize = 32;

/// Local binary-name normalization used only by `command_executable` when
/// matching custom agent commands against `KNOWN_AGENT_IDS`. Values normalized
/// here are never sent over the wire.
fn normalize_token(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_TOKEN_LENGTH {
        return None;
    }
    if !value.chars().all(|character| {
        character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || matches!(character, '.' | '_' | '-')
    }) {
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
        (PropertySpec::Bool, PropertyValue::Bool(value)) => Some(Value::Bool(*value)),
        (PropertySpec::SettingChoice(key), PropertyValue::Enum(member)) => {
            super::settings_snapshot::setting_choices(key)
                .contains(member)
                .then(|| Value::String((*member).to_string()))
        }
        (PropertySpec::BoostList, PropertyValue::EnumList(members)) => members
            .iter()
            .all(|member| {
                ghostex_gx_chat_core::extras::boosts::boost(member)
                    .is_some_and(|boost| boost.id == *member)
            })
            .then(|| {
                Value::Array(
                    members
                        .iter()
                        .map(|member| Value::String((*member).to_string()))
                        .collect(),
                )
            }),
        (PropertySpec::StoreExtensionVersionList, PropertyValue::ExtensionVersionList(entries)) => {
            if entries.len() > super::extensions::MAX_STORE_EXTENSION_LIST_LENGTH {
                return None;
            }
            entries
                .iter()
                .map(|(extension, version)| {
                    let super::extensions::ExtensionRef::Store(_) = extension else {
                        return None;
                    };
                    let id = extension.sendable()?;
                    let version = normalize_version_string(version)?;
                    Some(Value::String(format!("{id}@{version}")))
                })
                .collect::<Option<Vec<Value>>>()
                .map(Value::Array)
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
        (PropertySpec::Extension, PropertyValue::Extension(extension)) => {
            extension.sendable().map(Value::String)
        }
        (PropertySpec::BuiltInExtensionList, PropertyValue::ExtensionList(extensions)) => {
            encode_extension_list(extensions, |extension| {
                matches!(extension, super::extensions::ExtensionRef::BuiltIn(_))
            })
        }
        (PropertySpec::StoreExtensionList, PropertyValue::ExtensionList(extensions)) => {
            if extensions.len() > super::extensions::MAX_STORE_EXTENSION_LIST_LENGTH {
                return None;
            }
            encode_extension_list(extensions, |extension| {
                matches!(extension, super::extensions::ExtensionRef::Store(_))
            })
        }
        (PropertySpec::PersonSet, PropertyValue::PersonSet(entries)) => {
            let mut person = Map::new();
            for (key, value) in entries {
                /*
                One rejected key fails the WHOLE `$set`, and therefore the whole
                heartbeat. That is deliberate: a `$set` that silently drops the
                key it could not validate would write a half-updated person
                record, which is worse to reason about than a missing heartbeat.
                */
                let spec = person_property_spec(key)?;
                person.insert((*key).to_string(), encode_property(spec, value)?);
            }
            Some(Value::Object(person))
        }
        _ => None,
    }
}

/// A list of extensions of one kind, each re-checked against the whitelist. One
/// entry of the wrong kind, or one that no longer passes, rejects the list.
fn encode_extension_list(
    extensions: &[super::extensions::ExtensionRef],
    kind: impl Fn(&super::extensions::ExtensionRef) -> bool,
) -> Option<Value> {
    extensions
        .iter()
        .map(|extension| {
            kind(extension)
                .then(|| extension.sendable())
                .flatten()
                .map(Value::String)
        })
        .collect::<Option<Vec<Value>>>()
        .map(Value::Array)
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
