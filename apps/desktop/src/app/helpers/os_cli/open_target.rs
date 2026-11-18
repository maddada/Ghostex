#[cfg(target_os = "macos")]
use std::time::Duration;
use std::{collections::HashMap, path::Path};

use anyhow::Result;

use crate::*;

pub(crate) fn gpui_spawn_open_target_command(
    command: &str,
    base_args: &[&str],
    custom_args: &[String],
    project_path: &Path,
) -> Result<(), String> {
    // Windows has no `/usr/bin/env`: the command (a full path from the scan, or a bare name Rust
    // looks up on PATH) starts directly; std runs a `.cmd`/`.bat` through cmd.exe with its
    // arguments escaped. No console: `code.cmd` or the `zed.exe` CLI would otherwise flash one.
    #[cfg(target_os = "windows")]
    let mut process = {
        use std::os::windows::process::CommandExt;
        let mut process = std::process::Command::new(command);
        process.creation_flags(crate::app::helpers::CREATE_NO_WINDOW);
        process
    };
    #[cfg(not(target_os = "windows"))]
    let mut process = {
        let mut process = std::process::Command::new("/usr/bin/env");
        crate::restore_wayland_display_for_process_command(&mut process);
        process.arg(command);
        process
    };
    for arg in base_args {
        process.arg(arg);
    }
    for arg in custom_args {
        process.arg(arg);
    }
    process
        .arg(project_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn_reaped()
        .map_err(|_| "Could not launch the selected Open In target.".to_string())
}

pub(crate) struct GpuiDetectedOpenTargetAvailability {
    // Catalog-ordered, always contains "finder", never duplicates.
    pub(crate) available_target_ids: Vec<String>,
    pub(crate) resolved_commands: HashMap<String, String>,
    pub(crate) resolved_app_names: HashMap<String, String>,
}

#[cfg(target_os = "macos")]
pub(crate) const GPUI_OPEN_TARGET_DETECTION_TIMEOUT: Duration = Duration::from_secs(45);

#[cfg(target_os = "macos")]
pub(crate) fn gpui_spawn_keep_awake_caffeinate(
    duration_minutes: shared_settings::SharedKeepAwakeDurationMinutes,
    allow_display_sleep: bool,
) -> Result<std::process::Child, String> {
    /*
    CDXC:KeepAwake 2026-06-24-13:16:
    GPUI Keep Awake must start macOS caffeinate directly with fixed argv and suppressed stdio. Use `-dis` for normal display+idle sleep prevention, `-is` when Settings allows display sleep, and add bounded `-t` seconds only for the 2-hour and 5-hour shared durations.
    */
    let mut command = std::process::Command::new("/usr/bin/caffeinate");
    command.arg(if allow_display_sleep { "-is" } else { "-dis" });
    if duration_minutes.minutes() > 0 {
        command
            .arg("-t")
            .arg((duration_minutes.minutes() * 60).to_string());
    }
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| "caffeinate spawn failed".to_string())
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_spawn_open_target_app_name(
    app_name: &str,
    project_path: &Path,
) -> Result<(), String> {
    std::process::Command::new("/usr/bin/open")
        .arg("-a")
        .arg(app_name)
        .arg(project_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn_reaped()
        .map_err(|_| "Could not launch the selected Open In target.".to_string())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_spawn_open_target_app_name(
    _app_name: &str,
    _project_path: &Path,
) -> Result<(), String> {
    Err("Could not launch the selected Open In target.".to_string())
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_spawn_os_open(target: &std::ffi::OsStr) -> Result<(), String> {
    std::process::Command::new("/usr/bin/open")
        .arg(target)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn_reaped()
        .map_err(|_| {
            "The operating system could not open the requested Ghostex Settings target.".to_string()
        })
}

/// Opens a file or link with one app, as Finder's Open With does.
#[cfg(target_os = "macos")]
pub(crate) fn gpui_open_with_application(
    app: &Path,
    target: &std::ffi::OsStr,
) -> Result<(), String> {
    std::process::Command::new("/usr/bin/open")
        .arg("-a")
        .arg(app)
        .arg(target)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn_reaped()
        .map_err(|_| "The app could not open the file.".to_string())
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_open_with_application(
    app: &Path,
    target: &std::ffi::OsStr,
) -> Result<(), String> {
    super::windows_open_with_application(app, target)
}

/// Open With lists no apps on Linux, so nothing asks for one here.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn gpui_open_with_application(
    _app: &Path,
    target: &std::ffi::OsStr,
) -> Result<(), String> {
    gpui_spawn_os_open(target)
}

pub(crate) const GPUI_FILE_MANAGER_NAME: &str = "the file manager";

/// CDXC:Projects 2026-11-18 WHY:
/// User: "clicking on open folder on this project on the mac i'm on just doesn't work". The
/// project lived on an external drive that wasn't mounted, and `/usr/bin/open` on a missing path
/// starts fine, exits 1 with "does not exist" on a stderr nobody read, so the click did nothing.
/// A project folder that is gone now says why before anything is launched: the drive under
/// `/Volumes` that isn't connected (matched case-insensitively, since some projects carry
/// `/volumes/`, which a case-insensitive disk treats the same), or the folder itself.
pub(crate) fn gpui_missing_project_folder_message(path: &Path) -> Option<String> {
    if path.exists() {
        return None;
    }
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    if let Some(volume) = gpui_volume_mount_point(path)
        && !volume.exists()
    {
        return Some(format!(
            "Can't open {name}: {} isn't connected. Connect the drive and try again.",
            volume.display()
        ));
    }
    Some(format!(
        "Can't open {name}: the folder {} doesn't exist anymore.",
        path.display()
    ))
}

/// `/Volumes/<name>` for a path on a mounted volume, whatever the case of `Volumes` in the path.
fn gpui_volume_mount_point(path: &Path) -> Option<std::path::PathBuf> {
    use std::path::Component;
    let mut components = path.components();
    if components.next() != Some(Component::RootDir) {
        return None;
    }
    match (components.next(), components.next()) {
        (Some(Component::Normal(volumes)), Some(Component::Normal(name)))
            if volumes.eq_ignore_ascii_case("Volumes") =>
        {
            Some(Path::new("/Volumes").join(name))
        }
        _ => None,
    }
}

/// Opens a project folder in the file manager and waits for the opener's answer, so a failure
/// (its stderr, or a missing folder) reaches the caller instead of vanishing. Blocks briefly: call
/// it off the UI thread.
#[cfg(target_os = "macos")]
pub(crate) fn gpui_open_project_folder(path: &Path) -> Result<(), String> {
    if let Some(message) = gpui_missing_project_folder_message(path) {
        return Err(message);
    }
    let output = std::process::Command::new("/usr/bin/open")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|error| format!("Couldn't start Finder's opener: {error}."))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim();
    Err(if stderr.is_empty() {
        format!(
            "Finder couldn't open {} ({}).",
            path.display(),
            output.status
        )
    } else {
        format!("Finder couldn't open {}: {stderr}", path.display())
    })
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_open_project_folder(path: &Path) -> Result<(), String> {
    if let Some(message) = gpui_missing_project_folder_message(path) {
        return Err(message);
    }
    gpui_spawn_os_open(path.as_os_str())
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_reveal_path_in_finder(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err("Select an item to reveal.".to_string());
    }
    std::process::Command::new("/usr/bin/open")
        .arg("-R")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn_reaped()
        .map_err(|_| format!("Could not reveal that item in {GPUI_FILE_MANAGER_NAME}."))
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_reveal_path_in_finder(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err("Select an item to reveal.".to_string());
    }
    // Explorer's own selection syntax: one "/select,<path>" argument, no space.
    let mut selection = std::ffi::OsString::from("/select,");
    selection.push(path.as_os_str());
    std::process::Command::new("explorer.exe")
        .arg(selection)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn_reaped()
        .map_err(|_| format!("Could not reveal that item in {GPUI_FILE_MANAGER_NAME}."))
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn gpui_reveal_path_in_finder(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err("Select an item to reveal.".to_string());
    }
    // No portable "select this item" verb here, so open the containing folder.
    let folder = if path.is_dir() {
        path
    } else {
        path.parent()
            .ok_or_else(|| "That item has no containing folder.".to_string())?
    };
    let mut command = std::process::Command::new("xdg-open");
    crate::restore_wayland_display_for_process_command(&mut command);
    command
        .arg(folder)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn_reaped()
        .map_err(|_| format!("Could not reveal that item in {GPUI_FILE_MANAGER_NAME}."))
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn gpui_spawn_os_open(target: &std::ffi::OsStr) -> Result<(), String> {
    // `gio open` covers desktops that ship GLib's opener without xdg-utils.
    for (program, verb) in [("xdg-open", None), ("gio", Some("open"))] {
        let mut command = std::process::Command::new(program);
        crate::restore_wayland_display_for_process_command(&mut command);
        command.args(verb);
        let started = command
            .arg(target)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn_reaped();
        if started.is_ok() {
            return Ok(());
        }
    }
    Err(
        "Neither xdg-open nor gio is installed, so Linux has no program to open it with."
            .to_string(),
    )
}

/// CDXC:PlatformSupport 2026-10-28 WHY:
/// GPUI's `cx.open_url` on Linux runs xdg-open with the display variables GPUI read at startup, after main.rs had hidden `WAYLAND_DISPLAY` to force X11, so it removes `WAYLAND_DISPLAY` from the opener. A Wayland-only browser that is not running yet (Omarchy's Chromium) then exits with "Failed to connect to Wayland display" while xdg-open still returns 0, so GPUI never falls back to the desktop portal and the link does nothing. Links the app opens itself go through `gpui_spawn_os_open`, which gives the opener the session's Wayland display back.
pub(crate) fn gpui_open_external_url(url: &str, cx: &gpui::App) {
    #[cfg(all(unix, not(target_os = "macos")))]
    if gpui_spawn_os_open(std::ffi::OsStr::new(url)).is_ok() {
        return;
    }
    cx.open_url(url);
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_spawn_os_open(target: &std::ffi::OsStr) -> Result<(), String> {
    std::process::Command::new("rundll32.exe")
        .arg("url.dll,FileProtocolHandler")
        .arg(target)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn_reaped()
        .map_err(|_| "The OS opener is unavailable for this Ghostex Settings action.".to_string())
}

pub(crate) const GPUI_AGENTS_HUB_MAX_FILE_BYTES: u64 = 128 * 1024;
