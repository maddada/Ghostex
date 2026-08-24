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
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_probe_remote_gxserver_install(
    _config: GpuiRemoteMachineConfig,
) -> GpuiRemoteGxserverInstallProbe {
    GpuiRemoteGxserverInstallProbe::default()
}

#[cfg(target_os = "macos")]
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
    not a POSIX host. Native Windows OpenSSH is exactly that case and keeps
    gxserver inside WSL2, so the same script is re-run in the saved (or
    default) distribution instead of reporting the machine as missing gxserver.
    */
    if config.ssh_host.trim().is_empty() {
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
    /*
    A WSL2 distribution that no command has entered yet has to boot before it
    can answer, so give this attempt the longer connect budget instead of the
    short probe budget used for an already-running POSIX login shell.
    */
    let wsl_result = gpui_run_remote_ssh_in_windows_wsl(
        &config,
        config.wsl_distribution.as_deref(),
        gpui_remote_installed_gxserver_version_command(),
        GPUI_REMOTE_GXSERVER_CONNECT_TIMEOUT,
    );
    gpui_log_remote_gxserver_install_probe(&config, "windowsWsl", &wsl_result);
    gpui_remote_gxserver_install_probe_from_result(&wsl_result).unwrap_or_default()
}

#[cfg(target_os = "macos")]
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
        });
    }
    (result.exit_code == GPUI_REMOTE_GXSERVER_NOT_INSTALLED_EXIT_CODE)
        .then(GpuiRemoteGxserverInstallProbe::default)
}

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
pub(crate) fn gpui_remote_managed_gxserver_package_needs_update(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
) -> bool {
    let target_probe = gpui_run_remote_ssh_in_execution_target(
        config,
        execution_target,
        gpui_remote_install_target_probe_command(),
        GPUI_REMOTE_GXSERVER_INSTALL_PROBE_TIMEOUT,
    );
    let Some(target) = (target_probe.exit_code == 0)
        .then(|| gpui_extract_remote_install_target(target_probe.stdout.as_str()))
        .flatten()
    else {
        return false;
    };
    let Some(package_dir) = gpui_bundled_remote_gxserver_package_dir(&target) else {
        return false;
    };
    let Some(expected_identity) =
        gpui_bundled_remote_gxserver_build_identity(package_dir.as_path())
    else {
        return false;
    };
    let installed_identity = gpui_run_remote_ssh_in_execution_target(
        config,
        execution_target,
        gpui_remote_managed_gxserver_build_identity_command(),
        GPUI_REMOTE_GXSERVER_INSTALL_PROBE_TIMEOUT,
    );
    if installed_identity.exit_code != 0 {
        return false;
    }
    gpui_extract_remote_managed_gxserver_build_identity(installed_identity.stdout.as_str())
        .is_some_and(|identity| identity != expected_identity)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_install_bundled_remote_gxserver_and_read_token(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    progress_tx: Option<&mpsc::UnboundedSender<GpuiRemoteGxserverConnectProgress>>,
) -> Result<GpuiRemoteProcessResult, GpuiRemoteGxserverConnectResult> {
    let probe_result = gpui_run_remote_ssh_in_execution_target(
        config,
        execution_target,
        gpui_remote_install_target_probe_command(),
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
    for relative_path in ["bin/gxserver", "bin/zmx", "bin/bd"] {
        if !gpui_is_file(&package_dir.join(relative_path)) {
            return false;
        }
    }
    if target.normalized_os() != "linux" {
        return true;
    }
    // CDXC:GhostexRustCli 2026-07-13: the public CLI is the native bin/ghostex
    // built from server; packages with only the old CLI/ghostex-cli.mjs
    // Node entrypoint are stale. Linux remote packages no longer ship a Node
    // runtime at all.
    let arch = target.normalized_arch();
    for relative_path in ["bin/gxserver", "bin/ghostex", "bin/zmx", "bin/bd"] {
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

#[cfg(target_os = "macos")]
pub(crate) fn gpui_unsupported_remote_package_message(target: &GpuiRemoteInstallTarget) -> String {
    format!(
        "This Ghostex app bundle does not include a gxserver package for {}. Install a Ghostex build that includes a matching remote gxserver package, then retry.",
        target.display_label()
    )
}

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
pub(crate) fn gpui_upload_install_bundled_remote_gxserver_and_read_token_inner(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    package_dir: &Path,
    archive_path: &Path,
) -> GpuiRemoteProcessResult {
    let mut tar_environment = HashMap::new();
    tar_environment.insert("COPYFILE_DISABLE".to_string(), "1".to_string());
    let tar_arguments = vec![
        "-czf".to_string(),
        gpui_path_string(archive_path),
        "-C".to_string(),
        gpui_path_string(package_dir),
        ".".to_string(),
    ];
    let tar_result = gpui_run_remote_process(
        "/usr/bin/tar",
        &tar_arguments,
        Some(tar_environment),
        GPUI_REMOTE_GXSERVER_ARCHIVE_TIMEOUT,
    );
    if tar_result.exit_code != 0 {
        return GpuiRemoteProcessResult {
            exit_code: tar_result.exit_code,
            stderr: "Could not archive bundled gxserver package.".to_string(),
            stdout: String::new(),
        };
    }
    gpui_install_gxserver_archive_and_read_token(config, execution_target, archive_path)
}

pub(crate) fn gpui_remote_gxserver_install_command(release_id: &str) -> String {
    let token_read = gpui_remote_token_read_command();
    /*
    CDXC:RemoteMinimalDeps 2026-07-13:
    Package activation (stale-listener stop, package symlink swap, tool links
    into ~/.local/bin, ghostex CLI wrapper) moved into the uploaded package's
    own `gxserver setup` subcommand so every installer shares one Rust
    implementation. The shell script keeps only what must run before the new
    binary exists: extract the upload and invoke setup. The app and its
    remote packages are version-paired through the sealed asset manifest, so
    the uploaded gxserver always understands `setup`.
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
chmod 755 "$release_dir/bin/gxserver"
"$release_dir/bin/gxserver" setup --install-root "$install_root" --release-dir "$release_dir" --upload-path "$upload_path"
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
