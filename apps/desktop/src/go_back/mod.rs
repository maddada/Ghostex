//! Going back to the version this computer ran before an update.
//!
//! CDXC:Release 2026-11-17 DECISION:
//! User, after 11.8.0 and 11.8.1 crashed on every launch for some and nobody could go back: "what
//! i really want is the ability to update but see a message on desktop if update fails so u can
//! revert to older version easily (we keep both versions basically there)", then "we need to work
//! on being able to go back to older version from settings -> updates without crashes (just
//! because something is buggy etc)". Picked on the design page (docs/2026-11-17/release-safety-simple):
//! Q3728 the message shows after 2 crashes in the first day after an update, Q3729 the whole old
//! app is kept, Q3730 an automatic release check keeps the database safe to go back to.
//!
//! How it works: every launch of a release install is recorded in `go-back.json` (state folder):
//! the version, when it first ran here, the version before it, and whether the last launch ended
//! cleanly. An update keeps the running app before it is replaced (keep.rs: Sparkle's
//! will-install on macOS, the finished download on Windows, the AppImage swap on Linux). Two
//! launches of a new version that end without a clean quit in its first day show the message
//! (window.rs) before any window opens; Settings > Updates offers the same Go back any time.
//! Going back (swap.rs) swaps the kept app in after Ghostex quits and starts it; the version gone
//! back from is skipped until a newer one comes. Linux packages (deb, rpm) can't be swapped by
//! Ghostex, so they get the one command that installs the older package; AUR, tarballs and a
//! computer with no kept copy get the GitHub release page.
//!
//! A release that changes stored data in a way the version before can't read sets
//! `noGoBackBelow` in release-go-back.json to its own version (ai/database-rules.md; the release
//! check tooling/release-gpui/go-back-gate.mjs makes sure it does), and then no Go back is offered
//! to any version below it.
//! SEE-ALSO: keep.rs, swap.rs, window.rs, app/window/settings_modal/tabs/updates/go_back.rs.

pub(crate) mod keep;
pub(crate) mod swap;
pub(crate) mod window;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// The version this build is.
pub(crate) const THIS_VERSION: &str = env!("GHOSTEX_BUILD_MARKETING_VERSION");

/// Where older versions can always be downloaded.
pub(crate) const RELEASES_URL: &str = "https://github.com/maddada/Ghostex/releases";

/// The message shows only in a new version's first day here.
const FIRST_DAY_MS: i64 = 24 * 60 * 60 * 1000;
/// Launches of a new version that ended without a clean quit before the message shows (Q3728).
const CRASHES_BEFORE_MESSAGE: u32 = 2;

const STATE_FILE: &str = "go-back.json";
/// The kept app and what it is, in the data folder: `previous-version/app` (on macOS
/// `previous-version/<Name>.app`) and `previous-version/kept.json`.
const KEPT_DIR: &str = "previous-version";
const KEPT_FILE: &str = "kept.json";

/// The release's "can't go back" mark (ai/database-rules.md).
const RELEASE_GO_BACK: &str = include_str!("../../../../release-go-back.json");

/// What `go-back.json` remembers between launches.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct GoBackState {
    /// The version that ran last.
    pub(crate) version: Option<String>,
    /// When that version first ran here.
    pub(crate) since_ms: i64,
    /// The version that ran here before it, when it was older (an update).
    pub(crate) previous_version: Option<String>,
    /// A launch is running; a clean quit clears it, so a launch that finds it set follows one that
    /// crashed.
    pub(crate) running: bool,
    /// Launches of `version` in its first day that ended without a clean quit.
    pub(crate) crashes: u32,
    /// Versions gone back from: not offered again as an update (a newer one is).
    pub(crate) skipped: Vec<String>,
    /// Set while going back, until the version gone back to starts.
    pub(crate) going_back_to: Option<String>,
}

/// The kept app.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KeptApp {
    pub(crate) version: String,
    /// macOS only: the build number Sparkle compares (CFBundleVersion), to skip the version gone back from.
    #[serde(default)]
    pub(crate) build: Option<String>,
    pub(crate) kept_at_ms: i64,
}

/// How this computer can go back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GoBackHow {
    /// Swap the kept app in and start it.
    Swap,
    /// Run this command in a terminal (deb, rpm).
    Command(String),
    /// Download it from its GitHub release page.
    Download(String),
}

/// Going back from this version to an older one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GoBackOffer {
    pub(crate) current: String,
    pub(crate) previous: String,
    pub(crate) how: GoBackHow,
}

static STATE_LOCK: Mutex<()> = Mutex::new(());

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

fn state_path() -> PathBuf {
    crate::shared_settings::ghostex_storage_paths()
        .state_dir
        .join(STATE_FILE)
}

pub(crate) fn kept_dir() -> PathBuf {
    crate::shared_settings::ghostex_storage_paths()
        .data_dir
        .join(KEPT_DIR)
}

pub(crate) fn read_state() -> GoBackState {
    std::fs::read(state_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write_state(state: &GoBackState) {
    let path = state_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(state) {
        let _ = ghostex_paths::durable_file::write_durably(&path, &bytes);
    }
}

fn update_state(change: impl FnOnce(&mut GoBackState)) -> GoBackState {
    let _guard = STATE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let mut state = read_state();
    change(&mut state);
    write_state(&state);
    state
}

pub(crate) fn read_kept() -> Option<KeptApp> {
    std::fs::read(kept_dir().join(KEPT_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

pub(crate) fn write_kept(kept: &KeptApp) {
    if let Ok(bytes) = serde_json::to_vec_pretty(kept) {
        let _ = ghostex_paths::durable_file::write_durably(&kept_dir().join(KEPT_FILE), &bytes);
    }
}

/// `11.8.2` as numbers; `None` for anything else.
pub(crate) fn parse_version(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.trim().trim_start_matches('v').split('.');
    let parsed = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.split(['-', '+']).next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(parsed)
}

fn older(a: &str, b: &str) -> bool {
    matches!((parse_version(a), parse_version(b)), (Some(a), Some(b)) if a < b)
}

/// The oldest version this release can go back to, from release-go-back.json.
fn no_go_back_below() -> Option<String> {
    serde_json::from_str::<serde_json::Value>(RELEASE_GO_BACK)
        .ok()?
        .get("noGoBackBelow")?
        .as_str()
        .map(str::to_string)
}

/// Whether this build may offer going back to `previous`.
fn allowed(previous: &str) -> bool {
    no_go_back_below().is_none_or(|floor| !older(previous, &floor))
}

/// Only a release install (Sparkle on macOS, Velopack on Windows, the AppImage or a package on
/// Linux) keeps versions; a development build would see the same version number come and go.
pub(crate) fn release_install() -> bool {
    #[cfg(target_os = "macos")]
    {
        keep::running_app_bundle().is_some_and(|bundle| {
            bundle
                .join("Contents/Frameworks/Sparkle.framework")
                .is_dir()
        })
    }
    #[cfg(target_os = "windows")]
    {
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join("sq.version").is_file()))
            .unwrap_or(false)
    }
    #[cfg(target_os = "linux")]
    {
        // A Linux `cargo xtask start` installs into the packaged path too (build-linux-rust.sh).
        !cfg!(feature = "local-start")
            && (crate::linux_updater::appimage_install().is_some()
                || crate::linux_updater::is_packaged_install())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        false
    }
}

/// Records this launch, first thing after the single-instance hand-off, and says whether the
/// "isn't working" message should show before any window opens.
pub(crate) fn note_launch() -> Option<GoBackOffer> {
    if !release_install() {
        return None;
    }
    let now = now_ms();
    let state = update_state(|state| record_launch(state, THIS_VERSION, now));
    crash_message_due(&state, now)
        .then(|| offer_from(&state))
        .flatten()
}

/// The bookkeeping of one launch of `version` at `now`.
fn record_launch(state: &mut GoBackState, version: &str, now: i64) {
    if state.version.as_deref() != Some(version) {
        let last = state.version.take();
        // An update: the version before is what going back returns to. Going back itself, or
        // installing an older copy by hand, leaves nothing to go back to.
        state.previous_version = last
            .filter(|last| older(last, version) && state.going_back_to.as_deref() != Some(version));
        state.version = Some(version.to_string());
        state.since_ms = now;
        state.crashes = 0;
        state.going_back_to = None;
        // A skipped version stays skipped only until a newer one runs.
        state.skipped.retain(|skipped| older(version, skipped));
    } else if state.running && now - state.since_ms < FIRST_DAY_MS {
        state.crashes = state.crashes.saturating_add(1);
    }
    state.running = true;
}

fn crash_message_due(state: &GoBackState, now: i64) -> bool {
    state.crashes >= CRASHES_BEFORE_MESSAGE
        && now - state.since_ms < FIRST_DAY_MS
        && state.previous_version.is_some()
}

/// A clean quit: the next launch doesn't count this one as a crash.
pub(crate) fn note_clean_quit() {
    if release_install() {
        update_state(|state| state.running = false);
    }
}

/// Try again on the message: count crashes from zero again.
pub(crate) fn note_try_again() {
    update_state(|state| state.crashes = 0);
}

/// Going back from this version: skip it as an update and remember where to.
pub(crate) fn note_going_back(previous: &str) {
    update_state(|state| {
        if !state.skipped.iter().any(|skipped| skipped == THIS_VERSION) {
            state.skipped.push(THIS_VERSION.to_string());
        }
        state.going_back_to = Some(previous.to_string());
    });
}

/// Whether an update to `version` is one this computer went back from (Windows and Linux; macOS
/// skips it through Sparkle's own SUSkippedVersion, swap.rs).
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub(crate) fn skipped(version: &str) -> bool {
    read_state()
        .skipped
        .iter()
        .any(|skipped| skipped.trim_start_matches('v') == version.trim_start_matches('v'))
}

/// Going back as Settings > Updates offers it at any time: the version that ran here before this
/// one, when this release may go back to it.
pub(crate) fn offer() -> Option<GoBackOffer> {
    if !release_install() {
        return None;
    }
    offer_from(&read_state())
}

fn offer_from(state: &GoBackState) -> Option<GoBackOffer> {
    if state.version.as_deref() != Some(THIS_VERSION) {
        return None;
    }
    let previous = state.previous_version.clone()?;
    if !allowed(&previous) {
        return None;
    }
    let how = match read_kept() {
        Some(kept) if kept.version == previous && keep::kept_app_path().is_some() => {
            GoBackHow::Swap
        }
        _ => package_command(&previous)
            .map(GoBackHow::Command)
            .unwrap_or_else(|| GoBackHow::Download(release_page(&previous))),
    };
    Some(GoBackOffer {
        current: THIS_VERSION.to_string(),
        previous,
        how,
    })
}

pub(crate) fn release_page(version: &str) -> String {
    format!("{RELEASES_URL}/tag/v{}", version.trim_start_matches('v'))
}

/// The one command that installs `version` over a deb or rpm install.
fn package_command(version: &str) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        if !crate::linux_updater::is_packaged_install() {
            return None;
        }
        linux_package_command(version, std::env::consts::ARCH, Path::new("/"))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = version;
        None
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn linux_package_command(version: &str, arch: &str, root: &Path) -> Option<String> {
    let download = format!("{RELEASES_URL}/download/v{version}");
    if root.join("var/lib/dpkg/info/ghostex.list").is_file() {
        let arch = match arch {
            "x86_64" => "amd64",
            "aarch64" => "arm64",
            _ => return None,
        };
        let file = format!("ghostex_{version}_{arch}.deb");
        return Some(format!(
            "curl -fLo /tmp/{file} {download}/{file} && sudo apt install --allow-downgrades -y /tmp/{file}"
        ));
    }
    let rpm = format!("{download}/ghostex-{version}-1.{arch}.rpm");
    if !matches!(arch, "x86_64" | "aarch64") {
        return None;
    }
    if root.join("usr/bin/dnf").exists() {
        return Some(format!("sudo dnf downgrade -y {rpm}"));
    }
    if root.join("usr/bin/zypper").exists() {
        return Some(format!("sudo zypper install -y --oldpackage {rpm}"));
    }
    None
}
