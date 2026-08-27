/*
CDXC:AnonymousAnalytics 2026-08-26:
Machine/build properties every event carries, resolved once at init and cloned
onto each capture so no emitter can forget one.

CDXC:AnonymousAnalytics 2026-08-27 (addendum v2, §2):
`$process_person_profile: false` is GONE. Events are now identified, so PostHog
builds a person record per `distinct_id` — which is the point, since
`distinct_id` became per-human (`telemetry::identity`) and person profiles are
what make cohorts, property breakdowns, and retention-by-property work in the
UI. Two rules come with that and are enforced elsewhere in this module:
identify/alias are NEVER called (PostHog creates the person implicitly from the
events; an identify call risks an unrecoverable profile merge), and the person
properties themselves ride the heartbeat's `$set` only.

The per-EVENT profile fields (`interface`, `sidebar_version`, `default_agent`,
`project_bucket`, `identity_source`) deliberately do NOT live here: they change
while the daemon runs, and they must pass the taxonomy validator, so they are
assembled per capture in `telemetry::profile` from cached state. This map is
only for values that are fixed for the life of the process.

`os_version` is a MAJOR version only, and it is read exactly once at startup:
it is a machine attribute, it does not change under a running daemon, and
probing per event would spawn a subprocess on macOS.
*/

use serde_json::{Map, Value};

/// The marketing version baked in by `server/build.rs`, mirroring the desktop
/// crate. Dev builds report `CARGO_PKG_VERSION`, which is the point: they are
/// distinguishable from a shipped build.
pub const SERVER_MARKETING_VERSION: &str = env!("GHOSTEX_BUILD_MARKETING_VERSION");

pub fn build_base_properties() -> Map<String, Value> {
    let mut base = Map::new();
    base.insert(
        "server_version".to_string(),
        Value::String(SERVER_MARKETING_VERSION.to_string()),
    );
    base.insert(
        "platform".to_string(),
        Value::String(std::env::consts::OS.to_string()),
    );
    base.insert(
        "arch".to_string(),
        Value::String(std::env::consts::ARCH.to_string()),
    );
    if let Some(os_version) = read_os_major_version() {
        base.insert("os_version".to_string(), Value::String(os_version));
    }
    base.insert("is_dev".to_string(), Value::Bool(is_dev_build()));
    base
}

/// A dev build is one whose marketing version was never stamped by the release
/// tooling, i.e. it still reads the crate's own placeholder version. Debug
/// profile counts too, since nobody ships one.
pub fn is_dev_build() -> bool {
    cfg!(debug_assertions) || SERVER_MARKETING_VERSION == env!("CARGO_PKG_VERSION")
}

/// Major version only — `26`, not `26.1.3` — so the value stays a coarse
/// population bucket rather than something narrow enough to single a machine out.
fn read_os_major_version() -> Option<String> {
    let raw = read_raw_os_version()?;
    let major: String = raw
        .trim()
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();
    (!major.is_empty()).then_some(major)
}

#[cfg(target_os = "macos")]
fn read_raw_os_version() -> Option<String> {
    let output = std::process::Command::new("/usr/bin/sw_vers")
        .arg("-productVersion")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(target_os = "linux")]
fn read_raw_os_version() -> Option<String> {
    let output = std::process::Command::new("uname")
        .arg("-r")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(target_os = "windows")]
fn read_raw_os_version() -> Option<String> {
    let output = std::process::Command::new("cmd")
        .args(["/C", "ver"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    // `Microsoft Windows [Version 10.0.22631.4317]` -> `10.0.22631.4317`.
    let build = text.split("Version").nth(1)?;
    Some(build.trim().trim_end_matches(']').trim().to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn read_raw_os_version() -> Option<String> {
    None
}
