// The editor scan on computers without macOS's zsh: finds the Open In editors by looking for
// their commands in the user's tool PATH, with no shell involved in the scan itself.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use crate::app::helpers::*;

/*
CDXC:OsIntegration 2026-10-31 WHY:
The scan used to start `/bin/zsh -lc` on every platform. Linux often has no zsh (bash, fish, dash
or nothing at `/bin/zsh`), so the spawn failed and Open In and Settings > File opening listed only
Files. Off macOS the scan now looks for each editor's command in the user's tool PATH
(`ghostex_paths::tool_path`: the login shell's PATH, whatever shell that is, bounded by the
probe's own time limit; plus the inherited PATH and the usual tool folders), so a missing or slow
shell cannot stop it. Each command is stored as its full path, because the Open In launch runs it
through `/usr/bin/env` with this app's PATH, which on Linux is whatever the launcher gave it
(macOS replaces it at startup).
*/
#[cfg(any(target_os = "linux", test))]
pub(crate) fn gpui_user_tool_path_entries() -> Vec<String> {
    gpui_tool_path_entries_for(
        &ghostex_paths::user_home_dir(),
        std::env::var("PATH").ok().as_deref(),
    )
}

fn gpui_tool_path_entries_for(home: &Path, inherited_path: Option<&str>) -> Vec<String> {
    let login_shell_entries = ghostex_paths::tool_path::login_shell_path_entries(home);
    let joined = ghostex_paths::tool_path::normalized_user_tool_path_with(
        login_shell_entries,
        inherited_path,
        home,
    );
    ghostex_paths::tool_path::split_path(Some(&joined))
}

/// Which built-in editors have a command in `path_entries`, resolved to the command's full path.
/// Pure over its input so tests can point it at a folder of fake commands.
pub(crate) fn gpui_open_target_availability_from_path_entries(
    path_entries: &[String],
) -> GpuiDetectedOpenTargetAvailability {
    let mut available_ids: HashSet<String> = HashSet::from(["finder".to_string()]);
    let mut resolved_commands = HashMap::new();
    for definition in GPUI_BUILT_IN_OPEN_TARGETS {
        for command in definition.commands {
            if let Some(found) = gpui_find_command_in_path_entries(command, path_entries) {
                available_ids.insert(definition.id.to_string());
                resolved_commands.insert(
                    definition.id.to_string(),
                    found.to_string_lossy().into_owned(),
                );
                break;
            }
        }
    }
    GpuiDetectedOpenTargetAvailability {
        available_target_ids: GPUI_BUILT_IN_OPEN_TARGETS
            .iter()
            .filter(|definition| available_ids.contains(definition.id))
            .map(|definition| definition.id.to_string())
            .collect(),
        resolved_commands,
        resolved_app_names: HashMap::new(),
    }
}

fn gpui_find_command_in_path_entries(command: &str, path_entries: &[String]) -> Option<PathBuf> {
    path_entries
        .iter()
        // A relative entry would resolve inside whatever folder the app runs in.
        .filter(|entry| Path::new(entry.as_str()).is_absolute())
        .map(|entry| Path::new(entry).join(command))
        .find(|candidate| gpui_is_executable_file(candidate))
}

fn gpui_is_executable_file(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt, time::Duration};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ghostex-open-target-scan-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_command(dir: &Path, name: &str, mode: u32) {
        let path = dir.join(name);
        fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn finds_editors_on_path_without_any_shell() {
        let bin = scratch("finds");
        write_command(&bin, "code", 0o755);
        write_command(&bin, "cursor", 0o755);
        write_command(&bin, "zeditor", 0o755);
        let entries = vec![bin.to_string_lossy().into_owned()];

        let found = gpui_open_target_availability_from_path_entries(&entries);

        assert_eq!(
            found.available_target_ids,
            ["cursor", "vscode", "zed", "finder"].map(String::from)
        );
        assert_eq!(
            found.resolved_commands.get("vscode"),
            Some(&bin.join("code").to_string_lossy().into_owned())
        );
        // Zed answers to a second command name.
        assert_eq!(
            found.resolved_commands.get("zed"),
            Some(&bin.join("zeditor").to_string_lossy().into_owned())
        );
        assert!(found.resolved_app_names.is_empty());
        let _ = fs::remove_dir_all(&bin);
    }

    #[test]
    fn skips_files_that_are_not_executable_and_relative_entries() {
        let bin = scratch("skips");
        write_command(&bin, "code", 0o644);
        let entries = vec![
            "relative/bin".to_string(),
            bin.to_string_lossy().into_owned(),
        ];

        let found = gpui_open_target_availability_from_path_entries(&entries);

        assert_eq!(found.available_target_ids, ["finder".to_string()]);
        let _ = fs::remove_dir_all(&bin);
    }

    #[test]
    fn earlier_path_entry_wins() {
        let first = scratch("first");
        let second = scratch("second");
        write_command(&first, "code", 0o755);
        write_command(&second, "code", 0o755);
        let entries = vec![
            first.to_string_lossy().into_owned(),
            second.to_string_lossy().into_owned(),
        ];

        let found = gpui_open_target_availability_from_path_entries(&entries);

        assert_eq!(
            found.resolved_commands.get("vscode"),
            Some(&first.join("code").to_string_lossy().into_owned())
        );
        let _ = fs::remove_dir_all(&first);
        let _ = fs::remove_dir_all(&second);
    }

    #[test]
    fn nothing_installed_still_lists_finder_only() {
        let bin = scratch("empty");
        let entries = vec![bin.to_string_lossy().into_owned()];

        let found = gpui_open_target_availability_from_path_entries(&entries);

        assert_eq!(found.available_target_ids, ["finder".to_string()]);
        assert!(found.resolved_commands.is_empty());
        let _ = fs::remove_dir_all(&bin);
    }

    // The scan never starts `/bin/zsh` itself, and the shared probe it asks for the login PATH
    // skips a configured shell that does not exist. Slow shells are bounded by the probe's own
    // time limit (tests in `ghostex_paths::tool_path`).
    #[test]
    fn scan_finds_editors_when_the_configured_shell_is_missing() {
        let bin = scratch("shell-missing-bin");
        write_command(&bin, "code", 0o755);
        write_command(&bin, "cursor", 0o755);
        let home = scratch("shell-missing-home");
        let previous = std::env::var_os("SHELL");
        // SAFETY: no other test in this crate reads or writes SHELL.
        unsafe { std::env::set_var("SHELL", "/nonexistent/zsh") };

        let started = std::time::Instant::now();
        let entries = gpui_tool_path_entries_for(&home, Some(&bin.to_string_lossy()));
        let found = gpui_open_target_availability_from_path_entries(&entries);
        let elapsed = started.elapsed();

        unsafe {
            match previous {
                Some(value) => std::env::set_var("SHELL", value),
                None => std::env::remove_var("SHELL"),
            }
        }
        assert!(found.available_target_ids.contains(&"vscode".to_string()));
        assert!(found.available_target_ids.contains(&"cursor".to_string()));
        assert!(elapsed < Duration::from_secs(5), "took {elapsed:?}");
        let _ = fs::remove_dir_all(&bin);
        let _ = fs::remove_dir_all(&home);
    }

    // A real scan of this computer, as the Linux app runs it (prints with --nocapture).
    #[test]
    fn real_scan_of_this_computer_lists_finder_and_what_is_installed() {
        let started = std::time::Instant::now();
        let found = gpui_open_target_availability_from_path_entries(&gpui_user_tool_path_entries());
        eprintln!(
            "real scan: {:?} in {:?}\ncommands: {:?}",
            found.available_target_ids,
            started.elapsed(),
            found.resolved_commands
        );
        assert!(found.available_target_ids.contains(&"finder".to_string()));
    }
}
