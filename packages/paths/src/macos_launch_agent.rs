//! What Ghostex's LaunchAgents in ~/Library/LaunchAgents share on macOS: the app they name in
//! `AssociatedBundleIdentifiers` and a write that leaves an unchanged plist alone.

use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

/*
CDXC:BackgroundItems 2026-11-08 WHY: GitHub #510. A plist in ~/Library/LaunchAgents is a "legacy
agent" to macOS Background Task Management (BTM), which lists it in System Settings > General >
Login Items & Extensions under the app named by `AssociatedBundleIdentifiers`, or else under the
code signature's developer name ("Mohamad Youssef", generic icon). BTM honours the key only when
the job's executable has a Team ID that matches the named app (its log: "Bundle identifiers from
launchd plist ignored because the executable doesn't have a Team ID"); gxserver and the app are both
signed by Developer ID team KTKP595G3B. The id is read from the app's Info.plist, never hard-coded:
release builds are `com.madda.ghostex.host`, dev builds `com.madda.ghostex.gpui`.
*/
/// The bundle id of the outermost `.app` that contains `executable`, read from its Info.plist.
/// gxserver ships as `<Ghostex.app>/Contents/Resources/Web/gxserver/bin/gxserver`, a symlink into
/// the nested `GhostexServer.app`, so the outermost bundle is the one that names Ghostex whether
/// the path was resolved or not. `None` outside an app bundle (a dev binary in target/).
pub fn app_bundle_identifier(executable: &Path) -> Option<String> {
    let bundle = executable
        .ancestors()
        .filter(|ancestor| {
            ancestor
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
        })
        .last()?;
    let output = Command::new("/usr/bin/plutil")
        .args(["-extract", "CFBundleIdentifier", "raw", "-o", "-"])
        .arg(bundle.join("Contents/Info.plist"))
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let identifier = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (output.status.success() && !identifier.is_empty()).then_some(identifier)
}

/// The `AssociatedBundleIdentifiers` entry naming `bundle_identifier` (XML-escaped), for a plist
/// `<dict>`.
pub fn associated_bundle_entry(bundle_identifier: &str) -> String {
    format!(
        "<key>AssociatedBundleIdentifiers</key><array><string>{}</string></array>",
        bundle_identifier
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    )
}

/*
CDXC:BackgroundItems 2026-11-08 WHY: the user kept seeing "App Background Activity: Software from
"Mohamad Youssef" can run in the background". Any change in ~/Library/LaunchAgents makes BTM
re-check every legacy agent there, and an agent whose plist or executable changed gets a new record
reset to "not notified", which posts that notification again (BTM's log on 2026-11-08 showed both
Ghostex agents "updated" at every app rebuild, then "Exceeded max notifications for Ghostex" 25
times). So a plist is written only when its bytes differ, which leaves the folder untouched on
routine starts.
*/
/// Writes `contents` to `path` (mode 0600) unless the file already holds exactly that. Returns
/// whether it wrote.
pub fn write_plist_if_changed(path: &Path, contents: &str) -> std::io::Result<bool> {
    if fs::read(path).ok().as_deref() == Some(contents.as_bytes()) {
        return Ok(false);
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(contents.as_bytes())?;
    Ok(true)
}

/*
CDXC:BackgroundItems 2026-11-08 DECISION: GitHub #510, User: "do it now for 11.2.0". Ghostex's
helpers ship inside the app (Contents/Library/LaunchAgents) and the app registers them with
SMAppService, so Login Items lists them under Ghostex and macOS announces them once per install:
an update that replaces gxserver posts nothing (tested 2026-11-08 on macOS 27 with a Developer ID
build: register posted one "“<app>” can run in the background" notification, a re-signed update
and an unregister/register posted none). The plists in ~/Library/LaunchAgents stay as the fallback
whenever registration fails, needs approval, or this is an isolated profile.

The labels sit outside `launchd_label_prefix` (`com.madda.ghostex.`), so `stop-all`'s bootout of
every Ghostex job never unloads a registered helper, which only SMAppService may do.
*/
/// The bundled gxserver job (run on demand by the app, like the legacy `…gxserver` job).
pub const BUNDLED_GXSERVER: &str = "com.madda.ghostex-app.gxserver";
/// The bundled login-recovery agent (runs at each desktop login and on kickstart).
pub const BUNDLED_LOGIN_RECOVERY: &str = "com.madda.ghostex-app.gxserver-login-recovery";
/// The bundled start-at-login agent (registered only while the switch is on).
pub const BUNDLED_AT_LOGIN: &str = "com.madda.ghostex-app.gxserver-at-login";
/// The legacy labels the bundled agents replace, for the default profile.
pub const LEGACY_GXSERVER: &str = "com.madda.ghostex.gxserver";
pub const LEGACY_LOGIN_RECOVERY: &str = "com.madda.ghostex.gxserver-login-recovery";
pub const LEGACY_AT_LOGIN: &str = "com.madda.ghostex.gxserver-at-login";
/// Set to 1 in every bundled plist: gxserver then sets up what a legacy plist used to pass.
pub const BUNDLED_AGENT_ENV: &str = "GHOSTEX_GXSERVER_BUNDLED_AGENT";
/// Where the bundled plists live inside the app, and the program they run (`BundleProgram`).
pub const BUNDLED_AGENTS_DIR: &str = "Contents/Library/LaunchAgents";
/// The `bin/gxserver` link, the path the legacy plist runs too: gxserver derives its package root
/// and the hook commands it writes from the path it was started by (see build-macos-app.sh).
pub const BUNDLED_GXSERVER_PROGRAM: &str = "Contents/Resources/Web/gxserver/bin/gxserver";

fn uid() -> u32 {
    unsafe { libc::getuid() }
}

/// `launchctl print gui/<uid>/<label>` succeeds: launchd has the job (for a bundled agent, it is
/// registered and allowed).
pub fn gui_job_loaded(label: &str) -> bool {
    Command::new("/bin/launchctl")
        .args(["print", &format!("gui/{}/{label}", uid())])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// `~/Library/LaunchAgents/<label>.plist`.
pub fn legacy_plist_path(label: &str) -> std::path::PathBuf {
    crate::user_home_dir()
        .join("Library/LaunchAgents")
        .join(format!("{label}.plist"))
}

/// Boots out and deletes a legacy agent. Call only once its bundled replacement is loaded.
pub fn remove_legacy_agent(label: &str) -> std::io::Result<()> {
    let _ = Command::new("/bin/launchctl")
        .args(["bootout", &format!("gui/{}/{label}", uid())])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match fs::remove_file(legacy_plist_path(label)) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// The outermost `.app` that contains `path`.
pub fn outermost_app_bundle(path: &Path) -> Option<std::path::PathBuf> {
    path.ancestors()
        .filter(|ancestor| {
            ancestor
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
        })
        .last()
        .map(Path::to_path_buf)
}

/// The app's record of its bundled agents: which gxserver the bundled job runs (launchd reports a
/// bundled job's program only relative to its app) and the digest of each plist it registered,
/// so a changed plist is registered again (launchd keeps the copy it was given).
fn bundled_agents_state_file() -> std::path::PathBuf {
    crate::GhostexPaths::resolve()
        .gxserver_state_dir()
        .join("bundled-launch-agents.json")
}

pub fn read_bundled_agents_state() -> serde_json::Map<String, serde_json::Value> {
    fs::read(bundled_agents_state_file())
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

pub fn write_bundled_agents_state(
    state: &serde_json::Map<String, serde_json::Value>,
) -> std::io::Result<()> {
    let path = bundled_agents_state_file();
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    let text = serde_json::to_string_pretty(state).unwrap_or_default();
    write_plist_if_changed(&path, &text).map(|_| ())
}

/// The program of the bundled gxserver job, when it is loaded and the app recorded it.
pub fn bundled_gxserver_program() -> Option<std::path::PathBuf> {
    if !gui_job_loaded(BUNDLED_GXSERVER) {
        return None;
    }
    read_bundled_agents_state()
        .get("program")
        .and_then(serde_json::Value::as_str)
        .map(std::path::PathBuf::from)
}

/*
CDXC:BackgroundItems 2026-11-08 WHY: a bundled plist is the same for every user, so it cannot
carry what the app used to write into ~/Library/LaunchAgents: launchd gives the job only
/usr/bin:/bin:/usr/sbin:/sbin (seen in the 2026-11-08 test), and StandardOutPath cannot name a
per-user log. The port and folders need nothing: bundled agents are used only for the default
profile, whose port and folders are the defaults. So a process started by a bundled agent asks
the login shell for the user's PATH (as the app does at start, at most `STARTUP_PROBE_WAIT`) and
sends its output to gxserver's launch log. The marker is removed at once, so nothing gxserver
starts (terminals, `gxserver start` typed in one) inherits it.
*/
/// Call first thing in gxserver's `main`, before any thread starts.
pub fn prepare_bundled_agent_process() {
    if std::env::var_os(BUNDLED_AGENT_ENV).is_none() {
        return;
    }
    let home = crate::user_home_dir();
    let current = std::env::var("PATH").ok();
    let login_shell = crate::tool_path::login_shell_path_entries_within(
        &home,
        crate::tool_path::STARTUP_PROBE_WAIT,
    )
    .unwrap_or_default();
    let path =
        crate::tool_path::normalized_user_tool_path_with(login_shell, current.as_deref(), &home);
    // SAFETY: called from `main` before the runtime or any other thread starts.
    unsafe {
        std::env::remove_var(BUNDLED_AGENT_ENV);
        std::env::set_var("PATH", path);
    }
    let log = crate::GhostexPaths::resolve()
        .logs_dir
        .join("gxserver")
        .join("launch.log");
    if let Some(folder) = log.parent() {
        let _ = fs::create_dir_all(folder);
    }
    if let Ok(file) = fs::OpenOptions::new().create(true).append(true).open(&log) {
        use std::os::fd::AsRawFd;
        unsafe {
            libc::dup2(file.as_raw_fd(), 1);
            libc::dup2(file.as_raw_fd(), 2);
        }
    }
}
