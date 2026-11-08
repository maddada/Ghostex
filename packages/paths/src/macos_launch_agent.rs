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
