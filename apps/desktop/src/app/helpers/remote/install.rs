// C1 wave-1 deferred split: apps/desktop/src/app/helpers/remote.rs (~8.2k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds bundled remote-gxserver install probing,
// packaging, and upload/install execution. See
// docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use futures::channel::mpsc;

use crate::app::helpers::*;
use crate::*;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct GpuiRemoteGxserverInstallProbe {
    pub(crate) installed: bool,
    pub(crate) version: Option<String>,
    /// Ghostex installed that gxserver package (not a Ghostex app's own), so Reinstall can replace it.
    pub(crate) managed: bool,
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub(crate) fn gpui_probe_remote_gxserver_install(
    _config: GpuiRemoteMachineConfig,
) -> GpuiRemoteGxserverInstallProbe {
    GpuiRemoteGxserverInstallProbe::default()
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn gpui_probe_remote_gxserver_install(
    config: GpuiRemoteMachineConfig,
) -> GpuiRemoteGxserverInstallProbe {
    /*
    CDXC:RemoteMachines 2026-08-19:
    The Remote settings action must say Install for a machine with no gxserver
    and Update for one that already has it. Read that state over the saved SSH
    configuration only.

    The probe's own exit code decides which login environment answered: the
    marked payload and the "nothing installed" code can only come from a shell
    that actually ran the script, so any other outcome means this endpoint is
    not a POSIX host. Native Windows OpenSSH is probed for its configured
    PowerShell or WSL runtime before reporting gxserver as missing.
    */
    if config.ssh_host.trim().is_empty() {
        return GpuiRemoteGxserverInstallProbe::default();
    }
    // A machine that only signs in through the user's own SSH connection (GitHub #496) is not
    // probed while that connection is closed: this probe's login could never succeed, and every
    // attempt is one more refused one on the host.
    if config.reuse_ssh_connection && gpui_remote_ssh_reuse_connection_refusal(&config).is_some() {
        return GpuiRemoteGxserverInstallProbe::default();
    }
    let posix_result = gpui_run_remote_ssh(
        &config,
        gpui_remote_installed_gxserver_version_command(),
        GPUI_REMOTE_GXSERVER_INSTALL_PROBE_TIMEOUT,
    );
    gpui_log_remote_gxserver_install_probe(&config, "posix", &posix_result);
    if let Some(probe) = gpui_remote_gxserver_install_probe_from_result(&posix_result) {
        return probe;
    }
    if !posix_result.stderr.trim().is_empty()
        && gpui_remote_process_failure_is_ssh_transport(&posix_result)
    {
        /*
        SSH itself reported why it could not reach the machine, so there is no
        second login environment to try. A bare non-zero exit with no SSH
        diagnosis is what a native Windows shell produces for POSIX script
        text, and that case must still reach the WSL attempt below.
        */
        return GpuiRemoteGxserverInstallProbe::default();
    }
    let Ok(target) = gpui_probe_remote_execution_target(&config) else {
        return GpuiRemoteGxserverInstallProbe::default();
    };
    let command = if matches!(target, GpuiRemoteExecutionTarget::WindowsPowerShell) {
        format!(
            "{}\nWrite-Output '{}'; & $gxExe server version; if ($LASTEXITCODE -ne 0) {{ exit $LASTEXITCODE }}; Write-Output '{}'",
            gpui_remote_windows_cli_setup(),
            GPUI_REMOTE_GXSERVER_INSTALLED_VERSION_START_MARKER,
            GPUI_REMOTE_GXSERVER_INSTALLED_VERSION_END_MARKER
        )
    } else {
        gpui_remote_installed_gxserver_version_command().to_string()
    };
    let result = gpui_run_remote_ssh_in_execution_target(
        &config,
        &target,
        &command,
        GPUI_REMOTE_GXSERVER_CONNECT_TIMEOUT,
    );
    gpui_log_remote_gxserver_install_probe(&config, "configuredEnvironment", &result);
    gpui_remote_gxserver_install_probe_from_result(&result).unwrap_or_default()
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn gpui_remote_gxserver_install_probe_from_result(
    result: &GpuiRemoteProcessResult,
) -> Option<GpuiRemoteGxserverInstallProbe> {
    if result
        .stdout
        .contains(GPUI_REMOTE_GXSERVER_INSTALLED_VERSION_START_MARKER)
    {
        return Some(GpuiRemoteGxserverInstallProbe {
            installed: true,
            version: gpui_extract_remote_installed_gxserver_version(result.stdout.as_str()),
            managed: result
                .stdout
                .contains("__GHOSTEX_REMOTE_GXSERVER_MANAGED__"),
        });
    }
    (result.exit_code == GPUI_REMOTE_GXSERVER_NOT_INSTALLED_EXIT_CODE)
        .then(GpuiRemoteGxserverInstallProbe::default)
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn gpui_log_remote_gxserver_install_probe(
    config: &GpuiRemoteMachineConfig,
    phase: &str,
    result: &GpuiRemoteProcessResult,
) {
    // Bounded machine id, phase, and process outcome only: no hosts, users,
    // ports, paths, tokens, or process output.
    support_logs::append(
        support_logs::GpuiSupportLog::RemoteGxserverInstall,
        "gpui.remoteGxserver.installStateProbe",
        serde_json::json!({
            "exitCode": result.exit_code,
            "machineId": config.remote_machine_id,
            "markedOutput": result
                .stdout
                .contains(GPUI_REMOTE_GXSERVER_INSTALLED_VERSION_START_MARKER),
            "phase": phase,
            "stderrCategory": gpui_remote_process_stderr_category(result),
            "stderrPresent": !result.stderr.trim().is_empty(),
        }),
    );
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn gpui_remote_managed_gxserver_package_update(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
) -> Option<GpuiRemoteManagedPackageUpdate> {
    // Native Windows runs the app's own gxserver, which no remote connect installs; the probe
    // below is a POSIX script. The connect's health check still reads its session host.
    if matches!(
        execution_target,
        GpuiRemoteExecutionTarget::WindowsPowerShell
    ) {
        return None;
    }
    let target_probe = gpui_run_remote_ssh_in_execution_target(
        config,
        execution_target,
        gpui_remote_platform_probe_command_for(execution_target),
        GPUI_REMOTE_GXSERVER_INSTALL_PROBE_TIMEOUT,
    );
    let target = (target_probe.exit_code == 0)
        .then(|| gpui_extract_remote_install_target(target_probe.stdout.as_str()))
        .flatten()?;
    let expected_identity = gpui_bundled_remote_gxserver_package_build_identity(&target);
    let installed_identity = gpui_run_remote_ssh_in_execution_target(
        config,
        execution_target,
        gpui_remote_managed_gxserver_build_identity_command(),
        GPUI_REMOTE_GXSERVER_INSTALL_PROBE_TIMEOUT,
    );
    if installed_identity.exit_code != 0 {
        return None;
    }
    gpui_remote_managed_package_update_from_probe(
        installed_identity.stdout.as_str(),
        expected_identity.as_deref(),
    )
    .or_else(|| {
        // Reinstall replaces only a package Ghostex installed there: one that names its build.
        (config.reinstall_package
            && gpui_extract_remote_managed_gxserver_build_identity(
                installed_identity.stdout.as_str(),
            )
            .is_some())
        .then_some(GpuiRemoteManagedPackageUpdate::Requested)
    })
}

/// The build identity of the package this app itself carries for `target`, in the order the install picks it.
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
fn gpui_bundled_remote_gxserver_package_build_identity(
    target: &GpuiRemoteInstallTarget,
) -> Option<String> {
    if let Some(package_dir) = gpui_bundled_remote_gxserver_package_dir(target) {
        return gpui_bundled_remote_gxserver_build_identity(package_dir.as_path());
    }
    #[cfg(target_os = "windows")]
    if let Some(archive_path) = gpui_bundled_remote_gxserver_archive(target) {
        return gpui_bundled_remote_gxserver_archive_build_identity(archive_path.as_path());
    }
    None
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn gpui_install_bundled_remote_gxserver_and_read_token(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    progress_tx: Option<&mpsc::UnboundedSender<GpuiRemoteGxserverConnectProgress>>,
) -> Result<GpuiRemoteProcessResult, GpuiRemoteGxserverConnectResult> {
    let probe_result = gpui_run_remote_ssh_in_execution_target(
        config,
        execution_target,
        gpui_remote_platform_probe_command_for(execution_target),
        GPUI_REMOTE_GXSERVER_INSTALL_PROBE_TIMEOUT,
    );
    if probe_result.exit_code != 0 {
        return Err(GpuiRemoteGxserverConnectResult::without_connection(
            GpuiRemoteGxserverConnectState::InstallFailed,
            "Could not identify the remote operating system before installing gxserver.",
        ));
    }
    let Some(target) = gpui_extract_remote_install_target(probe_result.stdout.as_str()) else {
        return Err(GpuiRemoteGxserverConnectResult::without_connection(
            GpuiRemoteGxserverConnectState::InstallFailed,
            "Could not identify the remote operating system before installing gxserver.",
        ));
    };
    if let Some(package_dir) = gpui_bundled_remote_gxserver_package_dir(&target) {
        return Ok(gpui_upload_install_bundled_remote_gxserver_and_read_token(
            config,
            execution_target,
            package_dir.as_path(),
        ));
    }
    #[cfg(target_os = "windows")]
    if let Some(archive_path) = gpui_bundled_remote_gxserver_archive(&target) {
        return Ok(gpui_install_gxserver_archive_and_read_token(
            config,
            execution_target,
            archive_path.as_path(),
        ));
    }
    match gpui_on_demand_gxserver_archive(&target, progress_tx) {
        Ok(archive_path) => Ok(gpui_install_gxserver_archive_and_read_token(
            config,
            execution_target,
            archive_path.as_path(),
        )),
        Err(failure) => Err(GpuiRemoteGxserverConnectResult::without_connection(
            failure.state,
            failure.message.as_str(),
        )),
    }
}

pub(crate) fn gpui_remote_install_target_probe_command() -> &'static str {
    concat!(
        "GHOSTEX_REMOTE_OS=\"$(uname -s 2>/dev/null || true)\"; ",
        "GHOSTEX_REMOTE_ARCH=\"$(uname -m 2>/dev/null || true)\"; ",
        "GHOSTEX_REMOTE_DIST=\"\"; ",
        "if [ -r /etc/os-release ]; then ",
        "GHOSTEX_REMOTE_DIST=\"$(sed -n 's/^ID=//p' /etc/os-release 2>/dev/null | head -n 1 | tr -d '\"' || true)\"; ",
        "fi; ",
        "printf '__GHOSTEX_REMOTE_PLATFORM_START__\\n'; ",
        "printf '%s\\n' \"$GHOSTEX_REMOTE_OS\"; ",
        "printf '%s\\n' \"$GHOSTEX_REMOTE_ARCH\"; ",
        "printf '%s\\n' \"$GHOSTEX_REMOTE_DIST\"; ",
        "printf '__GHOSTEX_REMOTE_PLATFORM_END__\\n'"
    )
}

pub(crate) fn gpui_extract_remote_install_target(stdout: &str) -> Option<GpuiRemoteInstallTarget> {
    let payload = if let Some(start) = stdout.find("__GHOSTEX_REMOTE_PLATFORM_START__") {
        let payload_start = start + "__GHOSTEX_REMOTE_PLATFORM_START__".len();
        stdout[payload_start..]
            .find("__GHOSTEX_REMOTE_PLATFORM_END__")
            .map(|end| &stdout[payload_start..payload_start + end])
            .unwrap_or(stdout)
    } else {
        stdout
    };
    /*
    The probe prints the start marker as its own line, so the marked slice
    begins with that line's delimiter. Remove exactly that delimiter before
    assigning OS/arch/distribution fields; broad whitespace trimming would
    hide a genuinely missing OS field. macOS intentionally leaves the third
    distribution line empty.
    */
    let payload = payload
        .strip_prefix("\r\n")
        .or_else(|| payload.strip_prefix('\n'))
        .unwrap_or(payload);
    let lines = payload
        .lines()
        .map(str::trim)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if lines.len() < 2 || lines[0].is_empty() || lines[1].is_empty() {
        return None;
    }
    Some(GpuiRemoteInstallTarget {
        arch: lines[1].clone(),
        distribution: lines
            .get(2)
            .map(String::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        os: lines[0].clone(),
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_bundled_remote_gxserver_package_dir(
    target: &GpuiRemoteInstallTarget,
) -> Option<PathBuf> {
    let names = gpui_bundled_remote_gxserver_package_resource_names(target);
    if names.is_empty() {
        return None;
    }
    let resources_dir = gpui_app_bundle_resources_dir()?;
    for resource_name in names {
        let package_dir = resources_dir.join(resource_name);
        if gpui_is_dir(&package_dir)
            && gpui_bundled_remote_gxserver_package_is_compatible(&package_dir, target)
        {
            return Some(package_dir);
        }
    }
    None
}

#[cfg(target_os = "linux")]
pub(crate) fn gpui_bundled_remote_gxserver_package_dir(
    target: &GpuiRemoteInstallTarget,
) -> Option<PathBuf> {
    if target.normalized_os() != "linux" {
        return None;
    }
    let executable = env::current_exe().ok()?;
    let package_dir = executable.parent()?.join("gxserver");
    gpui_bundled_remote_gxserver_package_is_compatible(&package_dir, target).then_some(package_dir)
}

pub(crate) fn gpui_bundled_remote_gxserver_package_resource_names(
    target: &GpuiRemoteInstallTarget,
) -> Vec<&'static str> {
    let os = target.normalized_os();
    let arch = target.normalized_arch();
    if os == "linux" && arch == "x64" {
        return vec!["Web/gxserver-linux-x64", "Web/gxserver-linux-amd64"];
    }
    if os == "linux" && arch == "arm64" {
        return vec!["Web/gxserver-linux-arm64", "Web/gxserver-linux-aarch64"];
    }
    if os == "darwin" && arch == "arm64" {
        return if gpui_bundled_host_remote_gxserver_package_arch() == "arm64" {
            vec!["Web/gxserver-darwin-arm64", "Web/gxserver"]
        } else {
            vec!["Web/gxserver-darwin-arm64"]
        };
    }
    if os == "darwin" && arch == "x64" {
        return if gpui_bundled_host_remote_gxserver_package_arch() == "x64" {
            vec!["Web/gxserver-darwin-x64", "Web/gxserver"]
        } else {
            vec!["Web/gxserver-darwin-x64"]
        };
    }
    Vec::new()
}

pub(crate) fn gpui_bundled_host_remote_gxserver_package_arch() -> &'static str {
    #[cfg(target_arch = "aarch64")]
    {
        "arm64"
    }
    #[cfg(target_arch = "x86_64")]
    {
        "x64"
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        "unknown"
    }
}

pub(crate) fn gpui_bundled_remote_gxserver_package_is_compatible(
    package_dir: &Path,
    target: &GpuiRemoteInstallTarget,
) -> bool {
    for relative_path in GPUI_REMOTE_PACKAGE_RUNTIME_PATHS {
        if !gpui_is_file(&package_dir.join(relative_path)) {
            return false;
        }
    }
    if target.normalized_os() != "linux" {
        return true;
    }
    // CDXC:Cli 2026-07-13: the public CLI is the native bin/ghostex
    // built from server; packages with only the old CLI/ghostex-cli.mjs
    // Node entrypoint are stale. Linux remote packages no longer ship a Node
    // runtime at all.
    let arch = target.normalized_arch();
    for relative_path in ["bin/gxserver", "bin/ghostex", "bin/zmx"] {
        let path = package_dir.join(relative_path);
        if gpui_is_macho_binary(&path) || !gpui_is_elf_binary(&path, Some(arch.as_str())) {
            return false;
        }
    }
    true
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_app_bundle_resources_dir() -> Option<PathBuf> {
    let executable = env::current_exe().ok()?;
    let bundle_root = find_app_bundle_root(&executable)?;
    Some(bundle_root.join("Contents/Resources"))
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn gpui_unsupported_remote_package_message(target: &GpuiRemoteInstallTarget) -> String {
    format!(
        "This Ghostex app bundle does not include a gxserver package for {}. Install a Ghostex build that includes a matching remote gxserver package, then retry.",
        target.display_label()
    )
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn gpui_upload_install_bundled_remote_gxserver_and_read_token(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    package_dir: &Path,
) -> GpuiRemoteProcessResult {
    let temp_dir = env::temp_dir().join(format!(
        "ghostex-gpui-remote-gxserver-{}-{}",
        std::process::id(),
        gpui_remote_install_unique_id()
    ));
    let archive_path = temp_dir.join("gxserver.tar.gz");
    if fs::create_dir_all(&temp_dir).is_err() {
        return GpuiRemoteProcessResult {
            exit_code: 126,
            stderr: "Could not prepare gxserver upload archive.".to_string(),
            stdout: String::new(),
        };
    }
    let result = gpui_upload_install_bundled_remote_gxserver_and_read_token_inner(
        config,
        execution_target,
        package_dir,
        archive_path.as_path(),
    );
    let _ = fs::remove_dir_all(&temp_dir);
    result
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn gpui_upload_install_bundled_remote_gxserver_and_read_token_inner(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    package_dir: &Path,
    archive_path: &Path,
) -> GpuiRemoteProcessResult {
    let mut tar_environment: HashMap<String, String> = env::vars().collect();
    tar_environment.insert("COPYFILE_DISABLE".to_string(), "1".to_string());
    let tar_arguments = gpui_remote_package_tar_arguments(archive_path, package_dir);
    let tar_result = gpui_run_remote_process(
        &gpui_remote_tar_executable(),
        &tar_arguments,
        Some(tar_environment.clone()),
        GPUI_REMOTE_GXSERVER_ARCHIVE_TIMEOUT,
    );
    if tar_result.exit_code != 0 {
        return GpuiRemoteProcessResult {
            exit_code: tar_result.exit_code,
            stderr: "Could not archive bundled gxserver package.".to_string(),
            stdout: String::new(),
        };
    }
    // What is about to be uploaded, read back, so a package that would arrive with a link instead
    // of a program never reaches the machine.
    let listing = gpui_run_remote_process(
        &gpui_remote_tar_executable(),
        &["-tvzf".to_string(), gpui_path_string(archive_path)],
        Some(tar_environment),
        GPUI_REMOTE_GXSERVER_ARCHIVE_TIMEOUT,
    );
    if listing.exit_code != 0 || !gpui_remote_package_listing_is_complete(listing.stdout.as_str()) {
        return GpuiRemoteProcessResult {
            exit_code: 125,
            stderr: GPUI_REMOTE_PACKAGE_INCOMPLETE_MARKER.to_string(),
            stdout: String::new(),
        };
    }
    gpui_install_gxserver_archive_and_read_token(config, execution_target, archive_path)
}

/// What a remote machine cannot run sessions without, in every package.
const GPUI_REMOTE_PACKAGE_RUNTIME_PATHS: [&str; 3] = ["bin/gxserver", "bin/ghostex", "bin/zmx"];

/// CDXC:RemoteMachines 2026-10-31 WHY:
/// The macOS bundle's `bin/zmx` was a link out of the package (`../../bin/zmx`) and its `bin/ghostex`
/// a link to `gxserver` (CDXC:Release 2026-10-23 in build-macos-app.sh), so a copy that keeps links
/// leaves `zmx` dangling on the machine (GitHub #360). `-h` stores both as files; the tests here
/// run this exact command over that layout. SEE-ALSO: `gpui_remote_gxserver_install_command`, which
/// checks the same three paths on the machine for archives this app did not make.
///
/// CDXC:RemotePairing 2026-11-04 WHY: the macOS package now keeps gxserver and zmx in
/// GhostexServer.app and GhostexSessions.app with `bin/` links to them, every link inside the
/// package (package-macos-gxserver-bundles.sh). Those links must arrive as links: a copied
/// `bin/gxserver` is a loose program again, which macOS identifies by its build rather than the
/// bundle, and `-h` would also upload gxserver three times. So `-h` is kept only for packages
/// without the bundles (the Linux ones).
pub(crate) fn gpui_remote_package_tar_arguments(
    archive_path: &Path,
    package_dir: &Path,
) -> Vec<String> {
    let mut arguments = Vec::new();
    if !package_dir
        .join(GPUI_REMOTE_PACKAGE_GXSERVER_BUNDLE)
        .is_dir()
    {
        arguments.push("-h".to_string());
    }
    arguments.extend([
        "-czf".to_string(),
        gpui_path_string(archive_path),
        "-C".to_string(),
        gpui_path_string(package_dir),
        ".".to_string(),
    ]);
    arguments
}

/// The macOS package's gxserver app bundle, beside `bin/`.
const GPUI_REMOTE_PACKAGE_GXSERVER_BUNDLE: &str = "GhostexServer.app";

/// Whether a `tar -tv` listing holds each runtime program as a file: a regular entry, a hard link,
/// which extracts to one, or a symbolic link that leads, within the archive, to one of those. A
/// link that leads out of the archive does not count.
pub(crate) fn gpui_remote_package_listing_is_complete(listing: &str) -> bool {
    let is_file = |path: &str| {
        listing.lines().any(|line| {
            matches!(line.chars().next(), Some('-' | 'h'))
                && line
                    .split_whitespace()
                    .any(|token| token.strip_prefix("./").unwrap_or(token) == path)
        })
    };
    let links: HashMap<String, String> = listing
        .lines()
        .filter(|line| line.starts_with('l'))
        .filter_map(|line| {
            let (entry, target) = line.split_once(" -> ")?;
            let path = entry.split_whitespace().last()?;
            Some((
                path.strip_prefix("./").unwrap_or(path).to_string(),
                target.trim().to_string(),
            ))
        })
        .collect();
    GPUI_REMOTE_PACKAGE_RUNTIME_PATHS.iter().all(|path| {
        let mut path = (*path).to_string();
        // Enough for `bin/ghostex -> gxserver -> ../GhostexServer.app/...`; a loop gives up.
        for _ in 0..8 {
            if is_file(&path) {
                return true;
            }
            let Some(target) = links.get(&path) else {
                return false;
            };
            let Some(resolved) = gpui_archive_link_target(&path, target) else {
                return false;
            };
            path = resolved;
        }
        false
    })
}

/// The archive path a relative link at `link` leads to, `None` when it leaves the archive or is
/// absolute.
fn gpui_archive_link_target(link: &str, target: &str) -> Option<String> {
    if target.starts_with('/') {
        return None;
    }
    let mut parts: Vec<&str> = link.split('/').collect();
    parts.pop();
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

pub(crate) fn gpui_remote_gxserver_install_command(release_id: &str) -> String {
    let token_read = gpui_remote_token_read_command();
    /*
    CDXC:RemotePairing 2026-07-13:
    Package activation (stale-listener stop, package symlink swap, tool links
    into ~/.local/bin, ghostex CLI wrapper) moved into the uploaded package's
    own `gxserver setup` subcommand so every installer shares one Rust
    implementation. The shell script keeps only what must run before the new
    binary exists: extract the upload and invoke setup. The app and its
    remote packages are version-paired through the sealed asset manifest, so
    the uploaded gxserver always understands `setup`.

    CDXC:Telemetry 2026-08-26:
    `--analytics-role remote` is what makes an SSH helper permanently silent.
    Every remote install and every remote upgrade runs this one command, and
    setup persists the role in the remote gxserver's state dir, so the opt-out
    outlives this SSH session and applies to launch routes the desktop never
    issues (a `gx` verb typed in a remote pane, an agent shelling out to the
    CLI, the daemon restarting itself). Do NOT pass this from any local install
    path — the Windows app's WSL install runs the same subcommand.

    CDXC:RemoteMachines 2026-10-31 WHY:
    The new release is checked before `setup` stops the running gxserver, so a package that could
    not start a session (a dangling link, a zmx for another machine) leaves the old install
    working and fails with its own message rather than reading connected (GitHub #360). Archives
    this app did not make go through here too: the Linux on-demand and WSL packages.
    */
    format!(
        r#"set -eu
case "${{GHOSTEX_HOME:-}}" in
  /*) ghostex_data_dir="$GHOSTEX_HOME" ;;
  *) case "${{XDG_DATA_HOME:-}}" in
       /*) ghostex_data_dir="${{XDG_DATA_HOME%/}}/ghostex" ;;
       *) ghostex_data_dir="$HOME/.local/share/ghostex" ;;
     esac ;;
esac
install_root="$ghostex_data_dir/gxserver"
upload_path="$install_root/gxserver-upload.tar.gz"
release_dir="$install_root/releases/{release_id}"
mkdir -p "$release_dir"
tar -xzf "$upload_path" -C "$release_dir"
ghostex_package_broken=""
for ghostex_tool in gxserver ghostex zmx; do
  if [ ! -f "$release_dir/bin/$ghostex_tool" ] || [ ! -x "$release_dir/bin/$ghostex_tool" ]; then ghostex_package_broken=1; fi
done
if [ -z "$ghostex_package_broken" ] && ! "$release_dir/bin/zmx" version >/dev/null 2>&1; then ghostex_package_broken=1; fi
if [ -n "$ghostex_package_broken" ]; then
  rm -rf "$release_dir"
  rm -f "$upload_path"
  printf '%s\n' '{GPUI_REMOTE_PACKAGE_INCOMPLETE_MARKER}' >&2
  exit 125
fi
chmod 755 "$release_dir/bin/gxserver"
"$release_dir/bin/gxserver" setup --install-root "$install_root" --release-dir "$release_dir" --upload-path "$upload_path" --analytics-role remote
{token_read}"#
    )
}

pub(crate) fn gpui_remote_install_unique_id() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

pub(crate) fn gpui_normalize_remote_install_os(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.contains("darwin") {
        return "darwin".to_string();
    }
    if normalized.contains("linux") {
        return "linux".to_string();
    }
    if normalized.is_empty() {
        "unknown".to_string()
    } else {
        normalized
    }
}

pub(crate) fn gpui_normalize_remote_install_arch(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "amd64" | "x86_64" => "x64".to_string(),
        "aarch64" | "arm64" => "arm64".to_string(),
        normalized if normalized.is_empty() => "unknown".to_string(),
        normalized => normalized.to_string(),
    }
}

pub(crate) fn gpui_is_macho_binary(path: &Path) -> bool {
    let Ok(data) = fs::read(path) else {
        return false;
    };
    if data.len() < 4 {
        return false;
    }
    let prefix = [data[0], data[1], data[2], data[3]];
    matches!(
        prefix,
        [0xfe, 0xed, 0xfa, 0xce]
            | [0xce, 0xfa, 0xed, 0xfe]
            | [0xfe, 0xed, 0xfa, 0xcf]
            | [0xcf, 0xfa, 0xed, 0xfe]
            | [0xca, 0xfe, 0xba, 0xbe]
            | [0xbe, 0xba, 0xfe, 0xca]
    )
}

pub(crate) fn gpui_is_elf_binary(path: &Path, arch: Option<&str>) -> bool {
    let Ok(data) = fs::read(path) else {
        return false;
    };
    if data.len() < 20 || &data[..4] != b"\x7fELF" {
        return false;
    }
    let Some(arch) = arch else {
        return true;
    };
    let Some(expected_machine) = gpui_expected_elf_machine(arch) else {
        return false;
    };
    gpui_elf_machine(data.as_slice()) == Some(expected_machine)
}

pub(crate) fn gpui_expected_elf_machine(arch: &str) -> Option<u16> {
    match gpui_normalize_remote_install_arch(arch).as_str() {
        "x64" => Some(0x3e),
        "arm64" => Some(0xb7),
        _ => None,
    }
}

pub(crate) fn gpui_elf_machine(data: &[u8]) -> Option<u16> {
    if data.len() < 20 {
        return None;
    }
    match data[5] {
        1 => Some(u16::from(data[18]) | (u16::from(data[19]) << 8)),
        2 => Some((u16::from(data[18]) << 8) | u16::from(data[19])),
        _ => None,
    }
}

#[cfg(all(test, unix))]
mod remote_package_runtime_tests {
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::Path;
    use std::process::Command;

    use super::*;

    /// A folder of its own under the system temp folder, removed when the test ends.
    struct ThrowawayDir(std::path::PathBuf);

    impl ThrowawayDir {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let dir = env::temp_dir().join(format!(
                "ghostex-remote-package-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            // Resolved: macOS's temp folder is itself behind a link, which `dirname` scripts see.
            Self(fs::canonicalize(dir).unwrap())
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for ThrowawayDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_script(path: &Path, body: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// The macOS bundle's layout before 2026-11-04: `Web/gxserver/bin/{gxserver, ghostex -> gxserver,
    /// zmx -> ../../bin/zmx}` beside the real `Web/bin/zmx`. Returns `Web/gxserver`.
    fn macos_bundle_package(web: &Path, gxserver_body: &str) -> std::path::PathBuf {
        let package = web.join("gxserver");
        write_script(&web.join("bin/zmx"), "exit 0");
        write_script(&package.join("bin/gxserver"), gxserver_body);
        symlink("gxserver", package.join("bin/ghostex")).unwrap();
        symlink("../../bin/zmx", package.join("bin/zmx")).unwrap();
        fs::write(
            package.join("build-identity.json"),
            r#"{"buildIdentity":"gxserver:0.1.0:sha256:test"}"#,
        )
        .unwrap();
        package
    }

    fn tar(args: &[String]) -> std::process::Output {
        Command::new(gpui_remote_tar_executable())
            .args(args)
            .env("COPYFILE_DISABLE", "1")
            .output()
            .unwrap()
    }

    fn listing(archive: &Path) -> String {
        let output = tar(&["-tvzf".to_string(), gpui_path_string(archive)]);
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    /// The installer's own tar command turns the bundle's links into files, including the one that
    /// points out of the package, and the archive check tells that from an archive that kept them.
    #[test]
    fn the_uploaded_archive_holds_the_macos_packages_linked_programs_as_files() {
        let temp = ThrowawayDir::new();
        let package = macos_bundle_package(&temp.path().join("Web"), "exit 0");
        let archive = temp.path().join("package.tar.gz");
        assert!(
            tar(&gpui_remote_package_tar_arguments(&archive, &package))
                .status
                .success()
        );
        assert!(gpui_remote_package_listing_is_complete(&listing(&archive)));

        let extracted = temp.path().join("extracted");
        fs::create_dir_all(&extracted).unwrap();
        assert!(
            tar(&[
                "-xzf".to_string(),
                gpui_path_string(&archive),
                "-C".to_string(),
                gpui_path_string(&extracted),
            ])
            .status
            .success()
        );
        for tool in ["gxserver", "ghostex", "zmx"] {
            let metadata = fs::symlink_metadata(extracted.join("bin").join(tool)).unwrap();
            assert!(metadata.is_file(), "{tool} arrived as a link");
        }

        // The report's archive: the same package with its links kept.
        let kept_links = temp.path().join("kept-links.tar.gz");
        assert!(
            tar(&[
                "-czf".to_string(),
                gpui_path_string(&kept_links),
                "-C".to_string(),
                gpui_path_string(&package),
                ".".to_string(),
            ])
            .status
            .success()
        );
        assert!(!gpui_remote_package_listing_is_complete(&listing(
            &kept_links
        )));
    }

    /// The macOS bundle's layout since 2026-11-04: gxserver and zmx inside their app bundles, the
    /// package's `bin/` links leading there and the app's `Web/bin/zmx` linking into the package.
    /// Returns `Web/gxserver`.
    fn macos_app_bundles_package(web: &Path) -> std::path::PathBuf {
        let package = web.join("gxserver");
        write_script(
            &package.join("GhostexServer.app/Contents/MacOS/gxserver"),
            "exit 0",
        );
        write_script(
            &package.join("GhostexSessions.app/Contents/MacOS/zmx"),
            "exit 0",
        );
        fs::create_dir_all(package.join("bin")).unwrap();
        fs::create_dir_all(web.join("bin")).unwrap();
        symlink(
            "../GhostexServer.app/Contents/MacOS/gxserver",
            package.join("bin/gxserver"),
        )
        .unwrap();
        symlink(
            "../GhostexSessions.app/Contents/MacOS/zmx",
            package.join("bin/zmx"),
        )
        .unwrap();
        symlink("gxserver", package.join("bin/ghostex")).unwrap();
        symlink(
            "../gxserver/GhostexSessions.app/Contents/MacOS/zmx",
            web.join("bin/zmx"),
        )
        .unwrap();
        package
    }

    /// The bundles' links arrive as links that still lead to the programs inside the bundles, so a
    /// remote Mac runs gxserver and zmx as their bundles; and gxserver is uploaded once.
    #[test]
    fn the_uploaded_archive_keeps_the_links_into_the_macos_app_bundles() {
        let temp = ThrowawayDir::new();
        let package = macos_app_bundles_package(&temp.path().join("Web"));
        let archive = temp.path().join("package.tar.gz");
        let arguments = gpui_remote_package_tar_arguments(&archive, &package);
        assert!(!arguments.contains(&"-h".to_string()));
        assert!(tar(&arguments).status.success());
        let listed = listing(&archive);
        assert!(gpui_remote_package_listing_is_complete(&listed), "{listed}");
        assert_eq!(
            listed.lines().filter(|line| line.starts_with('-')).count(),
            2
        );

        let extracted = temp.path().join("extracted");
        fs::create_dir_all(&extracted).unwrap();
        assert!(
            tar(&[
                "-xzf".to_string(),
                gpui_path_string(&archive),
                "-C".to_string(),
                gpui_path_string(&extracted),
            ])
            .status
            .success()
        );
        for (tool, program) in [
            ("gxserver", "GhostexServer.app/Contents/MacOS/gxserver"),
            ("ghostex", "GhostexServer.app/Contents/MacOS/gxserver"),
            ("zmx", "GhostexSessions.app/Contents/MacOS/zmx"),
        ] {
            let link = extracted.join("bin").join(tool);
            assert!(fs::symlink_metadata(&link).unwrap().is_symlink(), "{tool}");
            assert_eq!(
                fs::canonicalize(&link).unwrap(),
                fs::canonicalize(extracted.join(program)).unwrap()
            );
        }
    }

    #[test]
    fn archive_links_resolve_only_inside_the_archive() {
        assert_eq!(
            gpui_archive_link_target("bin/zmx", "../GhostexSessions.app/Contents/MacOS/zmx")
                .as_deref(),
            Some("GhostexSessions.app/Contents/MacOS/zmx")
        );
        assert_eq!(
            gpui_archive_link_target("bin/ghostex", "./gxserver").as_deref(),
            Some("bin/gxserver")
        );
        assert_eq!(gpui_archive_link_target("bin/zmx", "../../bin/zmx"), None);
        assert_eq!(gpui_archive_link_target("bin/zmx", "/usr/bin/zmx"), None);
        let looping = "lrwxr-xr-x  0 me staff 0 Nov  4 20:11 ./bin/gxserver -> ghostex\n\
                       lrwxr-xr-x  0 me staff 0 Nov  4 20:11 ./bin/ghostex -> gxserver\n\
                       -rwxr-xr-x  0 me staff 9 Nov  4 20:11 ./bin/zmx\n";
        assert!(!gpui_remote_package_listing_is_complete(looping));
    }

    #[test]
    fn a_package_missing_a_runtime_program_is_not_a_compatible_package() {
        let temp = ThrowawayDir::new();
        let web = temp.path().join("Web");
        let package = macos_bundle_package(&web, "exit 0");
        let target = GpuiRemoteInstallTarget {
            arch: "arm64".to_string(),
            distribution: None,
            os: "Darwin".to_string(),
        };
        assert!(gpui_bundled_remote_gxserver_package_is_compatible(
            &package, &target
        ));
        // The link now dangles, as it does anywhere outside the bundle.
        fs::remove_file(web.join("bin/zmx")).unwrap();
        assert!(!gpui_bundled_remote_gxserver_package_is_compatible(
            &package, &target
        ));
    }

    /// A stand-in gxserver for the dry install: it records `setup` and links the package the way the
    /// real one does, and writes a token on `start`. The real one must not run here: its `setup`
    /// stops whatever gxserver listens on this computer's port.
    const FAKE_GXSERVER: &str = r#"root="$GHOSTEX_HOME/gxserver"
case "$1" in
  setup) echo setup >> "$GHOSTEX_HOME/setup-ran"; release="$(dirname "$(dirname "$0")")"; rm -rf "$root/package"; ln -s "$release" "$root/package" ;;
  start) mkdir -p "$GHOSTEX_HOME/state/gxserver/auth"; printf token-123 > "$GHOSTEX_HOME/state/gxserver/auth/token" ;;
esac
exit 0"#;

    fn run_remote_script(script: &str, home: &Path) -> std::process::Output {
        Command::new("/bin/sh")
            .args(["-c", script])
            .env("GHOSTEX_HOME", home)
            .env("HOME", home)
            .output()
            .unwrap()
    }

    /// Runs the real install script against an uploaded archive in a throwaway `GHOSTEX_HOME`.
    fn dry_install(home: &Path, archive: &Path) -> std::process::Output {
        let install_root = home.join("gxserver");
        fs::create_dir_all(&install_root).unwrap();
        fs::copy(archive, install_root.join("gxserver-upload.tar.gz")).unwrap();
        run_remote_script(&gpui_remote_gxserver_install_command("release-test"), home)
    }

    #[test]
    fn the_install_script_activates_a_whole_package_and_refuses_a_dangling_zmx() {
        let temp = ThrowawayDir::new();
        let package = macos_bundle_package(&temp.path().join("Web"), FAKE_GXSERVER);

        let good = temp.path().join("good.tar.gz");
        assert!(
            tar(&gpui_remote_package_tar_arguments(&good, &package))
                .status
                .success()
        );
        let home = temp.path().join("good-home");
        let output = dry_install(&home, &good);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("token-123"));
        assert!(home.join("setup-ran").exists());
        assert!(home.join("gxserver/package/bin/zmx").is_file());

        // The same package with its links kept: on the machine `zmx` points at nothing.
        let bad = temp.path().join("bad.tar.gz");
        assert!(
            tar(&[
                "-czf".to_string(),
                gpui_path_string(&bad),
                "-C".to_string(),
                gpui_path_string(&package),
                ".".to_string(),
            ])
            .status
            .success()
        );
        let home = temp.path().join("bad-home");
        let output = dry_install(&home, &bad);
        assert_eq!(output.status.code(), Some(125));
        let failed = GpuiRemoteProcessResult {
            exit_code: 125,
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            stdout: String::new(),
        };
        assert_eq!(
            gpui_remote_sanitized_process_failure("Remote gxserver install failed.", &failed),
            GPUI_REMOTE_PACKAGE_INCOMPLETE_MESSAGE
        );
        // The running install was never touched, and nothing is left behind.
        assert!(!home.join("setup-ran").exists());
        assert!(!home.join("gxserver/package").exists());
        assert!(!home.join("gxserver/releases/release-test").exists());
        assert!(!home.join("gxserver/gxserver-upload.tar.gz").exists());
    }

    /// Writes an installed package as the report found it, a real directory whose `zmx` link points
    /// at `<install root>/bin/zmx`, and runs the connect's real probe over it.
    #[test]
    fn the_connect_probe_finds_a_broken_runtime_at_the_same_build_and_a_repaired_one_is_left_alone()
    {
        let temp = ThrowawayDir::new();
        let home = temp.path();
        let package = home.join("gxserver/package");
        write_script(&package.join("bin/gxserver"), "exit 0");
        symlink("gxserver", package.join("bin/ghostex")).unwrap();
        symlink("../../bin/zmx", package.join("bin/zmx")).unwrap();
        let identity = "gxserver:0.1.0:sha256:same";
        fs::write(
            package.join("build-identity.json"),
            format!(r#"{{"buildIdentity":"{identity}"}}"#),
        )
        .unwrap();
        let probe = || {
            let output =
                run_remote_script(gpui_remote_managed_gxserver_build_identity_command(), home);
            assert!(
                output.status.success(),
                "{:?}: {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8_lossy(&output.stdout).to_string()
        };
        let broken = Some(GpuiRemoteManagedPackageUpdate::BrokenRuntime);
        assert_eq!(
            gpui_remote_managed_package_update_from_probe(&probe(), Some(identity)),
            broken
        );
        // Also when this app has no package to compare with: the machine is still broken.
        assert_eq!(
            gpui_remote_managed_package_update_from_probe(&probe(), None),
            broken
        );
        // The report's workaround puts the program where the link points: nothing to repair.
        write_script(&home.join("gxserver/bin/zmx"), "exit 0");
        assert_eq!(
            gpui_remote_managed_package_update_from_probe(&probe(), Some(identity)),
            None
        );
        assert_eq!(
            gpui_remote_managed_package_update_from_probe(&probe(), Some("gxserver:0.1.0:other")),
            Some(GpuiRemoteManagedPackageUpdate::BuildIdentity)
        );
        assert_eq!(
            gpui_remote_managed_package_update_from_probe(&probe(), None),
            None
        );
    }
}
