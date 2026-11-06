//! macOS: a LaunchAgent that launchd runs once in every new desktop login.

use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result};

use super::{remove_file_if_present, write_if_changed, LoginEntryPlace, AT_LOGIN_COMMAND};
use crate::paths::GxserverPaths;

/// CDXC:ComputerStartup 2026-11-06 WHY: the entry is its own LaunchAgent, not RunAtLoad on the app's gxserver job (the app rewrites that plist with RunAtLoad off on every start so loading it never starts gxserver, CDXC:ServerDaemon 2026-07-10) and not the login-recovery agent (each gxserver start kickstarts that one, which would open the app at every restart). Only the plist is written: launchd loads ~/Library/LaunchAgents at the next login, and bootstrapping it now would run it at once. `gxserver --at-login` starts gxserver through the app's launchd job, so the server lands in the desktop login with Keychain access (CDXC:ServerDaemon 2026-10-25 in server/src/cli.rs). `AssociatedBundleIdentifiers` names Ghostex, so Login Items in System Settings lists it under Ghostex.
pub(super) fn install(
    place: &LoginEntryPlace,
    gxserver: &Path,
    paths: &GxserverPaths,
) -> Result<()> {
    let (folder, label) = location(place)?;
    let port = crate::config::read_selected_local_api_port()?;
    let mut variables = vec![("GHOSTEX_GXSERVER_DEV_PORT".to_string(), port.to_string())];
    for key in [
        "GHOSTEX_HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
        "XDG_RUNTIME_DIR",
    ] {
        if let Some(value) = env::var(key).ok().filter(|value| value.starts_with('/')) {
            variables.push((key.to_string(), value));
        }
    }
    // The running server's PATH is the user's tool path the app gave its job, which a detached
    // start (no app job yet) needs to find the agents' CLIs.
    if let Ok(path) = env::var("PATH") {
        variables.push(("PATH".to_string(), path));
    }
    let environment = variables
        .into_iter()
        .map(|(key, value)| {
            format!(
                "<key>{}</key><string>{}</string>",
                xml_escape(&key),
                xml_escape(&value)
            )
        })
        .collect::<String>();
    let launch_log = paths.logs_dir.join("gxserver").join("launch.log");
    let bundle = app_bundle_identifier(gxserver)
        .map(|identifier| {
            format!(
                "<key>AssociatedBundleIdentifiers</key><array><string>{}</string></array>",
                xml_escape(&identifier)
            )
        })
        .unwrap_or_default();
    let plist = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict><key>Label</key><string>{label}</string><key>ProgramArguments</key><array><string>{program}</string><string>{AT_LOGIN_COMMAND}</string></array><key>EnvironmentVariables</key><dict>{environment}</dict>{bundle}<key>LimitLoadToSessionType</key><string>Aqua</string><key>RunAtLoad</key><true/><key>KeepAlive</key><false/><key>AbandonProcessGroup</key><true/><key>ProcessType</key><string>Interactive</string><key>StandardOutPath</key><string>{log}</string><key>StandardErrorPath</key><string>{log}</string></dict></plist>\n",
        label = xml_escape(&label),
        program = xml_escape(&gxserver.to_string_lossy()),
        log = xml_escape(&launch_log.to_string_lossy()),
    );
    write_if_changed(&folder.join(format!("{label}.plist")), &plist)
}

pub(super) fn remove(place: &LoginEntryPlace) -> Result<()> {
    let (folder, label) = location(place)?;
    let plist = folder.join(format!("{label}.plist"));
    if !plist.exists() {
        return Ok(());
    }
    if matches!(place, LoginEntryPlace::System) {
        // Loaded since the last login; unloading it is tidy, and a failure (not loaded) is fine.
        let uid = unsafe { libc::getuid() };
        let _ = Command::new("/bin/launchctl")
            .args(["bootout", &format!("gui/{uid}/{label}")])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    remove_file_if_present(&plist)
}

fn location(place: &LoginEntryPlace) -> Result<(PathBuf, String)> {
    let port = crate::config::read_selected_local_api_port()?;
    let label = format!(
        "{}gxserver-at-login",
        ghostex_paths::launchd_label_prefix(port)
    );
    let folder = match place {
        LoginEntryPlace::System => {
            PathBuf::from(env::var_os("HOME").context("HOME is unavailable")?)
                .join("Library/LaunchAgents")
        }
        LoginEntryPlace::Profile(folder) => folder.clone(),
    };
    Ok((folder, label))
}

/// The bundle id of the app gxserver ships in (`<app>/Contents/Resources/Web/gxserver/bin/gxserver`).
fn app_bundle_identifier(gxserver: &Path) -> Option<String> {
    let bundle = gxserver.ancestors().find(|ancestor| {
        ancestor
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
    })?;
    let output = Command::new("/usr/bin/plutil")
        .args(["-extract", "CFBundleIdentifier", "raw", "-o", "-"])
        .arg(bundle.join("Contents/Info.plist"))
        .stdin(Stdio::null())
        .output()
        .ok()?;
    let identifier = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (output.status.success() && !identifier.is_empty()).then_some(identifier)
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
