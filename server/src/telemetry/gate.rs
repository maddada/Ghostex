/*
CDXC:AnonymousAnalytics 2026-08-26:
The single opt-out chokepoint. Consulted at CAPTURE and again at FLUSH, so a
user who turns analytics off mid-run both stops producing events and discards
whatever is already queued — the queue is dropped, never drained on the way out.

There is deliberately exactly one of these functions in the whole codebase. A
second PostHog client, or an emitter that skips this call, is the bug class this
design exists to make impossible.
*/

use std::{
    sync::{Mutex, OnceLock},
    time::{Duration, Instant, SystemTime},
};

use serde_json::Value;

use crate::paths::GxserverPaths;

use super::taxonomy;

/// The cache is keyed on the settings file's MTIME, not on elapsed time: the
/// moment the app rewrites the file, the very next capture re-reads it, so
/// opting out takes effect immediately rather than after a timeout. This TTL is
/// only the ceiling for the degenerate case where the platform gives us no
/// mtime at all (and where the file is absent, so the answer is the default
/// anyway).
const SETTINGS_CACHE_TTL: Duration = Duration::from_secs(5);

pub const SETTINGS_FILE_NAME: &str = "native-sidebar-settings.json";

/*
CDXC:AnonymousAnalytics 2026-08-27 (addendum v2, §3):
Everything this module reads out of the settings file, in ONE struct.

The `interface` profile property rides every event and comes from the same JSON
document the opt-out flag does. Giving it its
own reader would have meant a second `fs::metadata` + `read_to_string` on every
capture — literally doubling the syscall cost of the hottest path in telemetry
— so they are parsed during the read the gate was already doing and cached
behind the same mtime key. One stat per capture before this change; one stat per
capture after it.

`interface` is `Option` rather than defaulted: when the
settings file does not exist yet the honest answer is "unknown", and the addendum
is explicit that an unavailable profile field is OMITTED rather than guessed.
(The gate's own flag is different — it has a shipped default of ON, and a
missing file means the user has not opted out.)
*/
#[derive(Clone, Copy, Debug)]
pub struct SettingsProfile {
    pub analytics_enabled: bool,
    pub interface: Option<&'static str>,
}

struct SettingsCache {
    checked_at: Instant,
    modified_at: Option<SystemTime>,
    profile: SettingsProfile,
}

static SETTINGS_CACHE: OnceLock<Mutex<Option<SettingsCache>>> = OnceLock::new();
static ENVIRONMENT_OPT_OUT: OnceLock<bool> = OnceLock::new();
static INSTALL_ROLE_OPT_OUT: OnceLock<bool> = OnceLock::new();

fn settings_cache() -> &'static Mutex<Option<SettingsCache>> {
    SETTINGS_CACHE.get_or_init(|| Mutex::new(None))
}

/// Process-lifetime opt-outs. Environment variables cannot change under a
/// running process, so this is resolved once.
fn environment_opts_out() -> bool {
    *ENVIRONMENT_OPT_OUT.get_or_init(|| {
        if env_is_truthy("GHOSTEX_TELEMETRY_DISABLED") {
            return true;
        }
        if env_is_truthy("DO_NOT_TRACK") {
            return true;
        }
        /*
        An escape hatch for a launch environment that knows it is a helper but
        was not installed as one. The rule that actually covers remote SSH
        helpers is `install_role_opts_out` below: it is durable, so it does not
        depend on any particular start command remembering to export this.
        */
        analytics_role_is_remote()
    })
}

/*
The durable half of the remote rule: a file written once by
`gxserver setup --analytics-role remote`, read here instead of trusting every
launch route to carry an environment variable. See `telemetry::role`.

Cached for the process lifetime because it is immutable after install — nothing
converts a remote helper into a desktop install without reinstalling it.
*/
fn install_role_opts_out(paths: &GxserverPaths) -> bool {
    *INSTALL_ROLE_OPT_OUT.get_or_init(|| super::role::marker_is_remote(paths))
}

fn env_is_truthy(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| {
            let value = value.trim();
            value == "1" || value.eq_ignore_ascii_case("true")
        })
        .unwrap_or(false)
}

pub fn analytics_role_is_remote() -> bool {
    std::env::var("GHOSTEX_ANALYTICS_ROLE")
        .ok()
        .map(|value| value.trim().eq_ignore_ascii_case("remote"))
        .unwrap_or(false)
}

/// `analyticsEnabled` from the shared sidebar settings file, plus the two
/// profile enums that live in the same document. Absent file, unparseable file,
/// or absent key all mean the shipped default for the flag: **on**. Only an
/// explicit `false` opts out, so a corrupt settings file never silently
/// disables a feature the user believes is running.
pub fn settings_profile(paths: &GxserverPaths) -> SettingsProfile {
    let settings_path = paths.app_config_dir.join(SETTINGS_FILE_NAME);
    let modified_at = std::fs::metadata(&settings_path)
        .ok()
        .and_then(|metadata| metadata.modified().ok());

    let mut cache = match settings_cache().lock() {
        Ok(cache) => cache,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(entry) = cache.as_ref() {
        /*
        MTIME FIRST. A changed mtime always forces a re-read, so a user who
        flips the toggle off stops being counted at the next capture rather than
        at the end of some window. The elapsed-time arm below only covers the
        case where no mtime exists to compare — a missing settings file, or a
        filesystem that reports none — where re-reading a file that is not there
        every single capture would be pure syscall churn.
        */
        if modified_at.is_some() && entry.modified_at == modified_at {
            return entry.profile;
        }
        if modified_at.is_none()
            && entry.modified_at.is_none()
            && entry.checked_at.elapsed() < SETTINGS_CACHE_TTL
        {
            return entry.profile;
        }
    }

    let settings = std::fs::read_to_string(&settings_path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    let profile = SettingsProfile {
        analytics_enabled: settings
            .as_ref()
            .and_then(|settings| settings.get("analyticsEnabled").and_then(Value::as_bool))
            .unwrap_or(true),
        interface: read_settings_enum(
            settings.as_ref(),
            "preferredAgentInterface",
            taxonomy::INTERFACE_KINDS,
            "chat",
        ),
    };
    *cache = Some(SettingsCache {
        checked_at: Instant::now(),
        modified_at,
        profile,
    });
    profile
}

/*
`None` only when there is no settings document at all — that is the startup
race the addendum says to answer by omitting the property. Once the document
exists, an unknown or missing value normalizes to the shipped default, because
that default is what the app itself is rendering for the user at that moment;
reporting "unknown" there would understate a real, observable state.
*/
fn read_settings_enum(
    settings: Option<&Value>,
    key: &str,
    table: &'static [&'static str],
    default: &'static str,
) -> Option<&'static str> {
    let settings = settings?;
    Some(
        settings
            .get(key)
            .and_then(Value::as_str)
            .and_then(|value| taxonomy::match_enum(table, value))
            .unwrap_or(default),
    )
}

/// The one question every capture and every flush asks.
pub fn is_enabled(paths: &GxserverPaths) -> bool {
    evaluate(paths).analytics_enabled
}

/// `is_enabled`, but handing back the profile enums the same cached read
/// produced, so the capture path never stats the settings file twice.
/// `analytics_enabled` here is the FULL gate answer (env + install role +
/// settings), not just the settings flag.
pub fn evaluate(paths: &GxserverPaths) -> SettingsProfile {
    /*
    Short-circuit before touching the filesystem, exactly as the original
    `!env && !role && settings_allow(..)` chain did: a process that is opted out
    by environment or by install role must not stat the settings file on every
    capture attempt for an answer it already knows.
    */
    if environment_opts_out() || install_role_opts_out(paths) {
        return SettingsProfile {
            analytics_enabled: false,
            interface: None,
        };
    }
    settings_profile(paths)
}
