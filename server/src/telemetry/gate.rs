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

/// The cache is keyed on the settings file's MTIME, not on elapsed time: the
/// moment the app rewrites the file, the very next capture re-reads it, so
/// opting out takes effect immediately rather than after a timeout. This TTL is
/// only the ceiling for the degenerate case where the platform gives us no
/// mtime at all (and where the file is absent, so the answer is the default
/// anyway).
const SETTINGS_CACHE_TTL: Duration = Duration::from_secs(5);

pub const SETTINGS_FILE_NAME: &str = "native-sidebar-settings.json";

struct SettingsCache {
    checked_at: Instant,
    modified_at: Option<SystemTime>,
    analytics_enabled: bool,
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

/// `analyticsEnabled` from the shared sidebar settings file. Absent file,
/// unparseable file, or absent key all mean the shipped default: **on**.
/// Only an explicit `false` opts out, so a corrupt settings file never silently
/// disables a feature the user believes is running.
fn settings_allow(paths: &GxserverPaths) -> bool {
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
            return entry.analytics_enabled;
        }
        if modified_at.is_none()
            && entry.modified_at.is_none()
            && entry.checked_at.elapsed() < SETTINGS_CACHE_TTL
        {
            return entry.analytics_enabled;
        }
    }

    let analytics_enabled = std::fs::read_to_string(&settings_path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|settings| settings.get("analyticsEnabled").and_then(Value::as_bool))
        .unwrap_or(true);
    *cache = Some(SettingsCache {
        checked_at: Instant::now(),
        modified_at,
        analytics_enabled,
    });
    analytics_enabled
}

/// The one question every capture and every flush asks.
pub fn is_enabled(paths: &GxserverPaths) -> bool {
    !environment_opts_out() && !install_role_opts_out(paths) && settings_allow(paths)
}
