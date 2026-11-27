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
    gpui_open_target_availability_from_search(path_entries, &[""], &|_| Vec::new())
}

/// The scan itself: each editor's commands are looked for in `path_entries`, then in that
/// editor's own `install_dirs` (folders only that editor is looked for in, so another app's copy
/// of a command name, such as the `code` Cursor ships, never counts for it), trying each of
/// `extensions` after the command name (`""` is the bare name).
fn gpui_open_target_availability_from_search(
    path_entries: &[String],
    extensions: &[&str],
    install_dirs: &dyn Fn(&GpuiBuiltInOpenTargetDefinition) -> Vec<String>,
) -> GpuiDetectedOpenTargetAvailability {
    let mut available_ids: HashSet<String> = HashSet::from(["finder".to_string()]);
    let mut resolved_commands = HashMap::new();
    for definition in GPUI_BUILT_IN_OPEN_TARGETS {
        if definition.commands.is_empty() {
            continue;
        }
        let own_dirs = install_dirs(definition);
        let found = [path_entries, own_dirs.as_slice()]
            .into_iter()
            .find_map(|entries| {
                definition.commands.iter().find_map(|command| {
                    gpui_find_command_in_path_entries(command, extensions, entries)
                })
            });
        if let Some(found) = found {
            available_ids.insert(definition.id.to_string());
            resolved_commands.insert(
                definition.id.to_string(),
                found.to_string_lossy().into_owned(),
            );
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

fn gpui_find_command_in_path_entries(
    command: &str,
    extensions: &[&str],
    path_entries: &[String],
) -> Option<PathBuf> {
    path_entries
        .iter()
        // A relative entry would resolve inside whatever folder the app runs in.
        .filter(|entry| Path::new(entry.as_str()).is_absolute())
        .flat_map(|entry| {
            extensions
                .iter()
                .map(move |extension| Path::new(entry).join(format!("{command}{extension}")))
        })
        .find(|candidate| gpui_is_executable_file(candidate))
}

/*
CDXC:OsIntegration 2026-11-14 FIX:
Discord (Windows 11, Ghostex 11.6.1): Zed showed as "Not installed" in Settings > Open In apps
although it was installed with Zed's per-user installer (`%LOCALAPPDATA%\Programs\Zed`, `zed` on
PATH). Windows never ran an editor scan at all (the detection returned None), so no editor could
ever be found there. Windows now scans too: the app's PATH, then each editor's usual install
folders (per-user `%LOCALAPPDATA%\Programs`, Program Files, JetBrains Toolbox's scripts and the
versioned JetBrains folders), so a PATH the app inherited before the editor was installed still
finds it. Windows runs only `.exe`, `.cmd` and `.bat` files by name, so those are the names tried:
the extensionless `code` or `zed` beside them is a Git Bash script Windows cannot start.
*/
#[cfg(any(target_os = "windows", test))]
const GPUI_WINDOWS_COMMAND_EXTENSIONS: &[&str] = &[".exe", ".cmd", ".bat"];

#[cfg(target_os = "windows")]
pub(crate) fn gpui_windows_open_target_availability() -> GpuiDetectedOpenTargetAvailability {
    let path_entries: Vec<String> = std::env::var_os("PATH")
        .map(|value| {
            std::env::split_paths(&value)
                .map(|entry| entry.to_string_lossy().trim().to_string())
                .filter(|entry| !entry.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let local_app_data = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let mut program_files: Vec<PathBuf> = Vec::new();
    for name in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(folder) = std::env::var_os(name).map(PathBuf::from)
            && !program_files.contains(&folder)
        {
            program_files.push(folder);
        }
    }
    gpui_open_target_availability_from_search(
        &path_entries,
        GPUI_WINDOWS_COMMAND_EXTENSIONS,
        &|definition| {
            gpui_windows_editor_install_dirs(definition, local_app_data.as_deref(), &program_files)
        },
    )
}

#[cfg(any(target_os = "windows", test))]
const GPUI_JETBRAINS_OPEN_TARGET_IDS: &[&str] = &[
    "idea",
    "aqua",
    "clion",
    "datagrip",
    "dataspell",
    "goland",
    "phpstorm",
    "pycharm",
    "rider",
    "rubymine",
    "rustrover",
    "webstorm",
];

/// The folders one editor's installers put its command in on Windows, per-user first.
#[cfg(any(target_os = "windows", test))]
fn gpui_windows_editor_install_dirs(
    definition: &GpuiBuiltInOpenTargetDefinition,
    local_app_data: Option<&Path>,
    program_files: &[PathBuf],
) -> Vec<String> {
    let mut install_roots: Vec<PathBuf> = local_app_data
        .map(|folder| folder.join("Programs"))
        .into_iter()
        .collect();
    install_roots.extend(program_files.iter().cloned());
    let relative: &[&[&str]] = match definition.id {
        // Zed's CLI is `bin\zed.exe`; `Zed.exe` beside `bin` is the app, which opens a folder too.
        "zed" => &[&["Zed", "bin"], &["Zed"]],
        "vscode" => &[&["Microsoft VS Code", "bin"]],
        "vscode-insiders" => &[&["Microsoft VS Code Insiders", "bin"]],
        "vscodium" => &[&["VSCodium", "bin"]],
        "cursor" => &[&["cursor", "resources", "app", "bin"]],
        "trae" => &[&["Trae", "bin"]],
        "kiro" => &[&["Kiro", "bin"]],
        "antigravity" => &[&["Antigravity", "bin"]],
        _ => &[],
    };
    let mut dirs: Vec<PathBuf> = Vec::new();
    for root in &install_roots {
        for parts in relative {
            dirs.push(
                parts
                    .iter()
                    .fold(root.clone(), |path, part| path.join(part)),
            );
        }
    }
    if GPUI_JETBRAINS_OPEN_TARGET_IDS.contains(&definition.id) {
        // Toolbox writes `idea.cmd`, `pycharm.cmd` and so on here when its shell scripts are on.
        if let Some(local_app_data) = local_app_data {
            dirs.push(
                local_app_data
                    .join("JetBrains")
                    .join("Toolbox")
                    .join("scripts"),
            );
        }
        // The standalone installers use versioned folders (`PyCharm Community Edition 2024.3`,
        // `JetBrains Rider 2025.1`) holding `bin\<command>.bat`; the newest name is tried first.
        let label = definition.label.to_ascii_lowercase();
        for root in install_roots
            .iter()
            .flat_map(|root| [root.join("JetBrains"), root.clone()])
        {
            let Ok(children) = std::fs::read_dir(&root) else {
                continue;
            };
            let mut matching: Vec<PathBuf> = children
                .filter_map(Result::ok)
                .map(|child| child.path())
                .filter(|path| {
                    path.is_dir()
                        && path
                            .file_name()
                            .map(|name| {
                                name.to_string_lossy().to_ascii_lowercase().contains(&label)
                            })
                            .unwrap_or(false)
                })
                .collect();
            matching.sort_by(|a, b| b.cmp(a));
            dirs.extend(matching.into_iter().map(|path| path.join("bin")));
        }
    }
    dirs.into_iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect()
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
    use std::{fs, time::Duration};

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
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        }
        #[cfg(not(unix))]
        let _ = mode;
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

    #[cfg(unix)]
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

    fn definition(id: &str) -> &'static GpuiBuiltInOpenTargetDefinition {
        GPUI_BUILT_IN_OPEN_TARGETS
            .iter()
            .find(|definition| definition.id == id)
            .unwrap()
    }

    fn windows_scan(
        path_entries: &[String],
        local_app_data: &Path,
        program_files: &[PathBuf],
    ) -> GpuiDetectedOpenTargetAvailability {
        gpui_open_target_availability_from_search(
            path_entries,
            GPUI_WINDOWS_COMMAND_EXTENSIONS,
            &|definition| {
                gpui_windows_editor_install_dirs(definition, Some(local_app_data), program_files)
            },
        )
    }

    // Windows: Zed's per-user install, found on PATH by its `zed.exe` (not the Git Bash `zed`
    // script beside it), and VS Code by `code.cmd`.
    #[test]
    fn windows_finds_commands_on_path_by_their_windows_names() {
        let bin = scratch("win-path");
        write_command(&bin, "zed", 0o755);
        write_command(&bin, "zed.exe", 0o755);
        write_command(&bin, "code", 0o755);
        write_command(&bin, "code.cmd", 0o755);
        let empty = scratch("win-path-local");
        let entries = vec![bin.to_string_lossy().into_owned()];

        let found = windows_scan(&entries, &empty, &[]);

        assert_eq!(
            found.resolved_commands.get("zed"),
            Some(&bin.join("zed.exe").to_string_lossy().into_owned())
        );
        assert_eq!(
            found.resolved_commands.get("vscode"),
            Some(&bin.join("code.cmd").to_string_lossy().into_owned())
        );
        let _ = fs::remove_dir_all(&bin);
        let _ = fs::remove_dir_all(&empty);
    }

    #[test]
    fn windows_skips_extensionless_scripts() {
        let bin = scratch("win-bare");
        write_command(&bin, "zed", 0o755);
        let empty = scratch("win-bare-local");

        let found = windows_scan(&[bin.to_string_lossy().into_owned()], &empty, &[]);

        assert_eq!(found.available_target_ids, ["finder".to_string()]);
        let _ = fs::remove_dir_all(&bin);
        let _ = fs::remove_dir_all(&empty);
    }

    // Windows: editors installed per user (or in Program Files) are found with nothing on PATH,
    // and Cursor's own `code.cmd` does not count as VS Code.
    #[test]
    fn windows_finds_per_user_and_program_files_installs_off_path() {
        let local = scratch("win-local");
        let program_files = scratch("win-program-files");
        let programs = local.join("Programs");
        for (dir, command) in [
            (programs.join("Zed").join("bin"), "zed.exe"),
            (
                programs.join("Microsoft VS Code Insiders").join("bin"),
                "code-insiders.cmd",
            ),
            (programs.join("cursor/resources/app/bin"), "cursor.cmd"),
            (programs.join("cursor/resources/app/bin"), "code.cmd"),
            (program_files.join("VSCodium").join("bin"), "codium.cmd"),
            (local.join("JetBrains/Toolbox/scripts"), "idea.cmd"),
            (
                program_files.join("JetBrains/PyCharm Community Edition 2024.1/bin"),
                "pycharm.bat",
            ),
            (
                program_files.join("JetBrains/PyCharm Community Edition 2025.2/bin"),
                "pycharm.bat",
            ),
            (
                program_files.join("JetBrains/JetBrains Rider 2025.1/bin"),
                "rider.bat",
            ),
        ] {
            fs::create_dir_all(&dir).unwrap();
            write_command(&dir, command, 0o755);
        }

        let found = windows_scan(&[], &local, &[program_files.clone()]);

        assert_eq!(
            found.available_target_ids,
            [
                "cursor",
                "vscode-insiders",
                "vscodium",
                "zed",
                "idea",
                "pycharm",
                "rider",
                "finder"
            ]
            .map(String::from)
        );
        assert_eq!(
            found.resolved_commands.get("zed"),
            Some(
                &programs
                    .join("Zed/bin/zed.exe")
                    .to_string_lossy()
                    .into_owned()
            )
        );
        assert_eq!(
            found.resolved_commands.get("pycharm"),
            Some(
                &program_files
                    .join("JetBrains/PyCharm Community Edition 2025.2/bin/pycharm.bat")
                    .to_string_lossy()
                    .into_owned()
            )
        );
        let _ = fs::remove_dir_all(&local);
        let _ = fs::remove_dir_all(&program_files);
    }

    // PATH wins over the install folders, as the user's own PATH choice should.
    #[test]
    fn windows_path_wins_over_install_folders() {
        let bin = scratch("win-order-bin");
        write_command(&bin, "zed.exe", 0o755);
        let local = scratch("win-order-local");
        let zed_bin = local.join("Programs/Zed/bin");
        fs::create_dir_all(&zed_bin).unwrap();
        write_command(&zed_bin, "zed.exe", 0o755);

        let found = windows_scan(&[bin.to_string_lossy().into_owned()], &local, &[]);

        assert_eq!(
            found.resolved_commands.get("zed"),
            Some(&bin.join("zed.exe").to_string_lossy().into_owned())
        );
        assert!(
            gpui_windows_editor_install_dirs(definition("finder"), Some(&local), &[]).is_empty()
        );
        let _ = fs::remove_dir_all(&bin);
        let _ = fs::remove_dir_all(&local);
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
