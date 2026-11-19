//! Settings > Updates > Checking for updates: how often Ghostex looks for updates on its own, one
//! setting per kind (Ghostex itself, agents, tools, extensions), each a time or Only on Check now.
//! gxserver reads them for the Updates list (server/src/updates/items.rs, per group), the desktop
//! for its own release check and Sparkle (apps/desktop/src/app/os_integration/updater.rs).
//!
//! CDXC:Updates 2026-11-19 DECISION:
//! User (issue #673, Q4180 picked C with changes): "make it a collapsed area by default that can be expanded to show these settings / we always skip agents that are turned off fam / also we need to split agents and tools and extensions each one by itself with a dropdown for each / ghostex every 1h check and others every 1h check by default". Four dropdowns (every 15 minutes, 30 minutes, hour, day, or Only on Check now), every hour by default, replacing the app's 15-minute and the list's 30-minute checks. Check now always checks everything. Agents turned off in Settings > Agents are never checked, with no setting for it.
use std::time::Duration;

use serde_json::Value;

use crate::json::{opt, Opt};

/// Ghostex's own releases (the desktop's check, and Sparkle's schedule on macOS).
pub const UPDATES_CHECK_GHOSTEX: &str = "updatesCheckGhostex";
/// The agents turned on in Settings > Agents.
pub const UPDATES_CHECK_AGENTS: &str = "updatesCheckAgents";
/// What Ghostex installs for you: account switchers, tools, the Easy Connect helper.
pub const UPDATES_CHECK_TOOLS: &str = "updatesCheckTools";
/// Extensions from the catalog.
pub const UPDATES_CHECK_EXTENSIONS: &str = "updatesCheckExtensions";

/// The value of "Only on Check now".
pub const UPDATE_CHECK_MANUAL: &str = "manual";
/// The default of all four.
pub const UPDATE_CHECK_DEFAULT: &str = "1h";

/// The choices of every one of the four dropdowns.
pub const UPDATE_CHECK_OPTIONS: &[Opt] = &[
    opt("Every 15 minutes", "15m"),
    opt("Every 30 minutes", "30m"),
    opt("Every hour", "1h"),
    opt("Every day", "1d"),
    opt("Only on Check now", UPDATE_CHECK_MANUAL),
];

/// One kind the Updates page lists, with its setting and the `group` its items carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UpdateCheckKind {
    Ghostex,
    Agents,
    Tools,
    Extensions,
}

impl UpdateCheckKind {
    pub const ALL: [UpdateCheckKind; 4] = [
        UpdateCheckKind::Ghostex,
        UpdateCheckKind::Agents,
        UpdateCheckKind::Tools,
        UpdateCheckKind::Extensions,
    ];

    pub fn setting_key(self) -> &'static str {
        match self {
            UpdateCheckKind::Ghostex => UPDATES_CHECK_GHOSTEX,
            UpdateCheckKind::Agents => UPDATES_CHECK_AGENTS,
            UpdateCheckKind::Tools => UPDATES_CHECK_TOOLS,
            UpdateCheckKind::Extensions => UPDATES_CHECK_EXTENSIONS,
        }
    }

    /// The `group` of the Updates items of this kind.
    pub fn group(self) -> &'static str {
        match self {
            UpdateCheckKind::Ghostex => "ghostex",
            UpdateCheckKind::Agents => "agents",
            UpdateCheckKind::Tools => "tools",
            UpdateCheckKind::Extensions => "extensions",
        }
    }
}

/// How long a stored value waits between checks; `None` for Only on Check now. A value this
/// version doesn't know reads as the default.
pub fn update_check_interval(value: Option<&str>) -> Option<Duration> {
    let minutes = match value.unwrap_or(UPDATE_CHECK_DEFAULT) {
        UPDATE_CHECK_MANUAL => return None,
        "15m" => 15,
        "30m" => 30,
        "1d" => 24 * 60,
        _ => 60,
    };
    Some(Duration::from_secs(minutes * 60))
}

/// [`update_check_interval`] of `kind` in a settings object (`None` settings: the defaults).
pub fn update_check_interval_in(
    settings: Option<&Value>,
    kind: UpdateCheckKind,
) -> Option<Duration> {
    update_check_interval(
        settings
            .and_then(|settings| settings.get(kind.setting_key()))
            .and_then(Value::as_str),
    )
}

/// [`update_check_interval_in`] over a settings map.
pub fn update_check_interval_in_map(
    settings: &serde_json::Map<String, Value>,
    kind: UpdateCheckKind,
) -> Option<Duration> {
    update_check_interval(settings.get(kind.setting_key()).and_then(Value::as_str))
}
