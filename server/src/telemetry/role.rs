/*
CDXC:Telemetry 2026-08-26:
The durable "this install is a remote helper" marker.

`GHOSTEX_ANALYTICS_ROLE=remote` alone cannot carry this rule. A gxserver on an
SSH box is started by many things that never inherit the desktop's SSH command
environment: `gxserver start` from the desktop connect script, a `gx` verb typed
in a remote terminal pane, an agent session shelling out to the CLI, the
`stop`/`start` pair `gxserver setup` performs during an upgrade, and anything
the user wires up themselves. An env var would have to be repeated at every one
of those, and each new launch route would silently start emitting.

So the role is written to disk ONCE, at install time, by `gxserver setup
--analytics-role remote` — the single command every remote install and upgrade
runs after unpacking the uploaded package. The file sits next to identity.json
in the gxserver state dir, so it survives daemon restarts, reboots, and package
upgrades (upgrades replace `releases/…`, never the state dir), and every launch
route reads it because the gate reads it rather than the environment.

The marker only ever says "remote". There is no "desktop" marker: the absence of
the file is what a normal install looks like, so nothing has to be written on
the overwhelmingly common path, and a local install can never be turned off by a
stale file it never had.
*/

use std::{fs, path::PathBuf};

use crate::paths::GxserverPaths;

pub const ANALYTICS_ROLE_FILE_NAME: &str = "analytics-role";
pub const REMOTE_ROLE_VALUE: &str = "remote";

pub fn analytics_role_file(paths: &GxserverPaths) -> PathBuf {
    paths.root_dir.join(ANALYTICS_ROLE_FILE_NAME)
}

/// `true` when this install was set up as a remote helper. Missing file,
/// unreadable file, or any other content means "not remote", which is the
/// shipped default for every locally installed daemon.
pub fn marker_is_remote(paths: &GxserverPaths) -> bool {
    fs::read_to_string(analytics_role_file(paths))
        .map(|value| value.trim().eq_ignore_ascii_case(REMOTE_ROLE_VALUE))
        .unwrap_or(false)
}

/// Called by `gxserver setup` when the installer declares this a remote
/// install. Best-effort by design: telemetry must never be able to fail an
/// install, and the next upgrade runs setup again on the same state dir.
pub fn write_remote_marker(paths: &GxserverPaths) {
    if let Err(error) = fs::create_dir_all(&paths.root_dir) {
        super::debug_log(format!("telemetry role marker dir failed: {error}"));
        return;
    }
    let path = analytics_role_file(paths);
    if let Err(error) = fs::write(&path, format!("{REMOTE_ROLE_VALUE}\n")) {
        super::debug_log(format!("telemetry role marker write failed: {error}"));
    }
}
