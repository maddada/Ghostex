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

pub(crate) fn gpui_finish_desktop_control_setup(
    driver_installed: bool,
    was_update: bool,
) -> Result<String, String> {
    /*
    CDXC:Extensions 2026-08-09:
    The Cua Driver installer/updater runs in a visible command-pane terminal.
    This completion step installs the bundled Ghostex Computer Use skill through
    the fixed ownership-verified Ghostex CLI helper and reports command failure
    without claiming Desktop Control is ready.
    */
    if !driver_installed {
        return Err(
            if was_update {
                "The Trycua update did not finish successfully. Its terminal tab shows what happened; plugin status was refreshed."
            } else {
                "The Trycua installer did not finish successfully. Its terminal tab shows what happened; plugin status was refreshed."
            }
            .to_string(),
        );
    }

    match gpui_install_bundled_ghostex_skill(
        &["computer-use", "install-skill"],
        "Ghostex Computer Use",
    ) {
        Ok(_) => Ok(if was_update {
            "Trycua is up to date. Ghostex Computer Use is ready.".to_string()
        } else {
            "Trycua installed. Grant macOS Accessibility and Screen Recording permissions if needed."
                .to_string()
        }),
        Err(message) => Err(format!(
            "Trycua {}, but Ghostex Computer Use skill could not be installed. {message}",
            if was_update { "updated" } else { "installed" }
        )),
    }
}

pub(crate) fn gpui_repair_ghostex_cli_commands() -> Result<String, String> {
    /*
    CDXC:Cli 2026-06-24-12:56:
    CLI repair is real only when GPUI is running from a packaged app that ships the native `Contents/Resources/CLI/ghostex` binary. Development binaries must report unavailable status instead of synthesizing wrappers to a source checkout, while packaged repair writes public wrappers outside the app and replaces only marked Ghostex wrappers, app-owned CLI symlinks, or broken symlinks.
    */
    let cli_dir = gpui_bundled_ghostex_cli_resource_dir()?;
    let cli_binary_path = cli_dir.join("ghostex");
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
    match gx_result {
        GpuiCliCommandInstallResult::Blocked { existing_path } => Ok(format!(
            "Ghostex CLI linked. The gx alias was not changed because another command already owns that name at {}.",
            gpui_path_string(&existing_path)
        )),
        GpuiCliCommandInstallResult::Unavailable => Ok(
            "Ghostex CLI linked. The gx alias could not be linked because no writable install location was available."
                .to_string(),
        ),
        GpuiCliCommandInstallResult::Current | GpuiCliCommandInstallResult::Repaired => Ok(
            "Ghostex CLI linked. ghostex and gx now launch this GPUI app build where available."
                .to_string(),
        ),
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

pub(crate) fn gpui_auto_repair_stale_ghostex_cli_wrappers() {
    /*
    CDXC:Cli 2026-08-30:
    Sparkle and DMG updates replace the app bundle but never touch the public
    PATH wrappers, so a wrapper written by an older install keeps exec'ing a
    path that no longer exists (pre-2026-07-13 wrappers exec a removed Node
    CLI). Packaged startup therefore refreshes wrappers Ghostex already owns
    through the same repair the Settings action runs. It is strictly a refresh:
    when no Ghostex-owned command exists, or every one already matches the
    canonical wrapper text, nothing is written, so startup never performs a
    first install for a user who did not opt in.
    */
    let Ok(cli_dir) = gpui_bundled_ghostex_cli_resource_dir() else {
        return;
    };
    let cli_binary_path = cli_dir.join("ghostex");
    let wrapper = gpui_ghostex_cli_wrapper_content(&cli_binary_path);
    let path_entries = gpui_cli_path_entries();
    let common_dirs = gpui_common_cli_install_dirs();

    let mut has_stale_wrapper = false;
    'commands: for command in ["ghostex", "gx"] {
        for candidate in gpui_cli_command_path_candidates(command, &path_entries, &common_dirs) {
            if !gpui_path_exists_or_is_symlink(&candidate) {
                continue;
            }
            if !gpui_is_ghostex_owned_command_path(command, &candidate, &cli_dir) {
                continue;
            }
            let is_current = gpui_is_regular_file(&candidate)
                && fs::read_to_string(&candidate)
                    .map(|content| content == wrapper)
                    .unwrap_or(false);
            if !is_current {
                has_stale_wrapper = true;
                break 'commands;
            }
        }
    }
    if !has_stale_wrapper {
        return;
    }
    if let Err(message) = gpui_repair_ghostex_cli_commands() {
        eprintln!("ghostex-gpui could not refresh stale Ghostex CLI wrappers: {message}");
    }
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
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let cli_dir: Option<PathBuf> = None;
    let Some(cli_dir) = cli_dir else {
        return Err(
            "Packaged Ghostex CLI resources are unavailable in this GPUI build. Current integration status was refreshed without changing files."
                .to_string(),
        );
    };
    if gpui_is_file(&cli_dir.join("ghostex")) {
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
    for directory in install_dirs {
        let link_path = directory.join(command);
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
    gpui_unique_paths([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        gpui_home_dir().join(".local/bin"),
    ])
}

pub(crate) fn gpui_cli_install_dirs(
    path_entries: &[PathBuf],
    common_dirs: &[PathBuf],
    cli_dir: &Path,
) -> Vec<PathBuf> {
    let mut owned_dirs = Vec::new();
    for command in ["ghostex", "gx"] {
        for candidate in gpui_cli_command_path_candidates(command, path_entries, common_dirs) {
            if gpui_path_exists_or_is_symlink(&candidate)
                && gpui_is_ghostex_owned_command_path(command, &candidate, cli_dir)
            {
                if let Some(parent) = candidate.parent() {
                    owned_dirs.push(parent.to_path_buf());
                }
            }
        }
    }
    gpui_unique_paths(
        owned_dirs
            .into_iter()
            .chain(path_entries.iter().cloned())
            .chain(common_dirs.iter().cloned()),
    )
}

pub(crate) fn gpui_cli_command_path_candidates(
    command: &str,
    path_entries: &[PathBuf],
    common_dirs: &[PathBuf],
) -> Vec<PathBuf> {
    let found = gpui_which_command(command).into_iter();
    gpui_unique_paths(
        found
            .chain(path_entries.iter().map(|directory| directory.join(command)))
            .chain(common_dirs.iter().map(|directory| directory.join(command))),
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
    if gpui_file_contains_ghostex_cli_wrapper_marker(path) {
        return true;
    }
    let realpath = gpui_realpath_or_self(path);
    if gpui_path_is_relative_to(&realpath, cli_dir) {
        return true;
    }
    gpui_is_ghostex_app_owned_command_realpath(command, &realpath)
}

pub(crate) fn gpui_is_ghostex_app_owned_command_realpath(command: &str, realpath: &Path) -> bool {
    let normalized = gpui_path_string(realpath).to_lowercase();
    let managed_gxserver_dir = crate::shared_settings::ghostex_storage_paths().gxserver_data_dir();
    let is_managed_ghostex_cli = realpath
        .file_name()
        .map(|file_name| file_name.eq_ignore_ascii_case("ghostex"))
        .unwrap_or(false)
        && gpui_path_is_relative_to(realpath, &managed_gxserver_dir);
    is_managed_ghostex_cli
        || normalized.contains(&format!("/ghostex.app/contents/resources/cli/{command}"))
        || normalized.contains(&format!(
            "/ghostex.app/contents/resources/web/cli/{command}"
        ))
        || normalized.contains("/ghostex.app/contents/resources/cli/ghostex-cli.mjs")
        || (command == "ghostex" && normalized.contains("/ghostex.app/contents/macos/ghostex"))
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
    GPUI Settings installs bundled Ghostex skills by resolving the fixed `ghostex` command on PATH and running only the known `ghostex <namespace> install-skill` argv. Command text never comes from React, child stdout/stderr are suppressed, failures are reported generically, and status is refreshed from disk afterward.

    CDXC:AgentSkills 2026-06-24-13:08:
    Executing a PATH `ghostex` command from Settings requires strict Ghostex ownership evidence: a repair marker plus `ghostex-cli.mjs`, or an app-owned realpath recognized by the CLI repair ownership helper. Broad read-only status strings are not sufficient for process execution.
    */
    let Some(ghostex_path) = gpui_which_command("ghostex") else {
        return Err(
            "Ghostex CLI was not found on PATH. Repair the Ghostex CLI before installing bundled skills."
                .to_string(),
        );
    };
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
