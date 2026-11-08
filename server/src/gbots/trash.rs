//! Moving a folder to the user's Trash (the macOS Trash, the freedesktop.org Trash on Linux, the
//! Recycle Bin on Windows) under a name of the caller's choosing, so a GBot's folder (its
//! instructions, memory and files) can be put back by hand after the bot is removed.

use std::path::Path;

/// A name in `dir` not taken yet: `<name>`, then `<name> 2`, …
fn free_trash_name(dir: &Path, name: &str) -> String {
    (1..)
        .map(|n| {
            if n == 1 {
                name.to_string()
            } else {
                format!("{name} {n}")
            }
        })
        .find(|name| !dir.join(name).exists())
        .unwrap_or_else(|| name.to_string())
}

/// Moves a folder to the user's Trash (macOS `~/.Trash`).
#[cfg(target_os = "macos")]
pub(super) fn move_to_trash(home: &Path, folder: &Path, name: &str) -> Result<(), String> {
    let trash = home.join(".Trash");
    std::fs::create_dir_all(&trash).map_err(|error| error.to_string())?;
    let target = trash.join(free_trash_name(&trash, name));
    std::fs::rename(folder, &target)
        .map_err(|error| format!("could not move {} to the Trash: {error}", folder.display()))
}

/// Moves a folder to the user's Trash (the freedesktop.org Trash, with its `.trashinfo`, so file
/// managers can put it back).
#[cfg(all(unix, not(target_os = "macos")))]
pub(super) fn move_to_trash(home: &Path, folder: &Path, name: &str) -> Result<(), String> {
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".local").join("share"));
    let trash = data_home.join("Trash");
    let (files, info) = (trash.join("files"), trash.join("info"));
    std::fs::create_dir_all(&files).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&info).map_err(|error| error.to_string())?;
    let name = free_trash_name(&files, name);
    let deleted_at = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S");
    std::fs::write(
        info.join(format!("{name}.trashinfo")),
        format!(
            "[Trash Info]\nPath={}\nDeletionDate={deleted_at}\n",
            folder.display()
        ),
    )
    .map_err(|error| error.to_string())?;
    std::fs::rename(folder, files.join(&name)).map_err(|error| {
        let _ = std::fs::remove_file(info.join(format!("{name}.trashinfo")));
        format!("could not move {} to the Trash: {error}", folder.display())
    })
}

/// Moves a folder to the Recycle Bin.
#[cfg(windows)]
pub(super) fn move_to_trash(_home: &Path, folder: &Path, _name: &str) -> Result<(), String> {
    let status = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory($env:GHOSTEX_TRASH_PATH, 'OnlyErrorDialogs', 'SendToRecycleBin')",
        ])
        .env("GHOSTEX_TRASH_PATH", folder)
        .stdin(std::process::Stdio::null())
        .status()
        .map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "could not move {} to the Recycle Bin",
            folder.display()
        ))
    }
}
