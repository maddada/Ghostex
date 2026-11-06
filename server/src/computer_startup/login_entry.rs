//! The one login entry each platform gets while Start Ghostex's server when you log in is on: it
//! runs `gxserver --at-login`, which starts gxserver and, when asked, opens the app.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

use crate::paths::GxserverPaths;

/// The argument the login entry runs gxserver with (`at_login.rs`).
pub(crate) const AT_LOGIN_COMMAND: &str = "--at-login";

/// Where this profile's login entry goes.
pub(crate) enum LoginEntryPlace {
    /// The user's own login items: ~/Library/LaunchAgents, the HKCU Run key, ~/.config/autostart.
    System,
    /// CDXC:ComputerStartup 2026-11-06 WHY: an isolated profile (GHOSTEX_HOME, a portable copy or a dev port, the same rule as CDXC:ServerDaemon 2026-11-06 in server/src/server/macos_login_recovery.rs) never registers a login item, which would start that test server at every login or replace the user's own entry. It writes the same entry into its own folder instead, so what it would register can be read without touching the computer's login items.
    Profile(PathBuf),
}

impl LoginEntryPlace {
    pub(crate) fn for_profile(paths: &GxserverPaths) -> Result<Self> {
        let port = crate::config::read_selected_local_api_port()?;
        Ok(
            if ghostex_paths::uses_separate_profile()
                || port != crate::constants::GXSERVER_LOCAL_API_PORT
            {
                Self::Profile(paths.app_state_dir.join("login-items"))
            } else {
                Self::System
            },
        )
    }
}

/// Adds or removes the entry so it matches the switch. Both are no-ops when nothing changes.
pub(crate) fn apply(paths: &GxserverPaths, wanted: bool) -> Result<()> {
    let place = LoginEntryPlace::for_profile(paths)?;
    if wanted {
        let gxserver = std::env::current_exe().context("find the gxserver executable")?;
        let gxserver = fs::canonicalize(&gxserver).unwrap_or(gxserver);
        platform::install(&place, &gxserver, paths)
    } else {
        platform::remove(&place)
    }
}

/// Writes `contents` unless the file already holds exactly that, so routine checks never churn
/// the login items folder (macOS shows a Background Items notification for a changed agent).
pub(super) fn write_if_changed(path: &Path, contents: &str) -> Result<()> {
    if fs::read_to_string(path).ok().as_deref() == Some(contents) {
        return Ok(());
    }
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder).with_context(|| format!("create {}", folder.display()))?;
    }
    fs::write(path, contents).with_context(|| format!("write {}", path.display()))
}

pub(super) fn remove_file_if_present(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            Err(error).with_context(|| format!("remove {}", path.display()))
        }
        _ => Ok(()),
    }
}

#[cfg(target_os = "macos")]
#[path = "login_entry_macos.rs"]
mod platform;

#[cfg(windows)]
#[path = "login_entry_windows.rs"]
mod platform;

#[cfg(all(unix, not(target_os = "macos")))]
#[path = "login_entry_linux.rs"]
mod platform;
