//! Linux: an XDG autostart entry, which the desktop session starts at every login.

use std::{
    env,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

use super::{remove_file_if_present, write_if_changed, LoginEntryPlace, AT_LOGIN_COMMAND};
use crate::paths::GxserverPaths;

const FILE_NAME: &str = "ghostex-gxserver.desktop";

/// CDXC:ComputerStartup 2026-11-06 WHY: XDG autostart, not a systemd user service: a user service also starts for an SSH login and has no DISPLAY, where opening the app cannot work, while autostart runs when the desktop session starts, which is what "when you log in" means here.
pub(super) fn install(
    place: &LoginEntryPlace,
    gxserver: &Path,
    _paths: &GxserverPaths,
) -> Result<()> {
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName=Ghostex server\nComment=Starts Ghostex's background server when you log in\nExec={} {AT_LOGIN_COMMAND}\nTerminal=false\nNoDisplay=true\nX-GNOME-Autostart-enabled=true\n",
        exec_argument(&gxserver.to_string_lossy())
    );
    write_if_changed(&folder(place)?.join(FILE_NAME), &entry)
}

pub(super) fn remove(place: &LoginEntryPlace) -> Result<()> {
    remove_file_if_present(&folder(place)?.join(FILE_NAME))
}

fn folder(place: &LoginEntryPlace) -> Result<PathBuf> {
    Ok(match place {
        LoginEntryPlace::Profile(folder) => folder.clone(),
        LoginEntryPlace::System => env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .context("HOME is unavailable")?
            .join("autostart"),
    })
}

/// A quoted `Exec` argument as the Desktop Entry spec reads it.
fn exec_argument(value: &str) -> String {
    let mut quoted = String::from("\"");
    for character in value.chars() {
        match character {
            '"' | '`' | '$' | '\\' => {
                quoted.push('\\');
                quoted.push(character);
            }
            '%' => quoted.push_str("%%"),
            _ => quoted.push(character),
        }
    }
    quoted.push('"');
    quoted
}
