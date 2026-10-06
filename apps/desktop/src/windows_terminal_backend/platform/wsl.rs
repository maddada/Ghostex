use super::{
    ResolvedWindowsTerminalBackend, WindowsTerminalBackendPreference, WindowsWslReadiness,
};
use std::{
    env,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use std::os::windows::process::CommandExt as _;

use super::*;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const WSL_STORAGE_PATHS_SCRIPT: &str = r#"set -eu
case "${GHOSTEX_HOME:-}" in
    /*)
        ghostex_data_dir="$GHOSTEX_HOME"
        ghostex_state_dir="$GHOSTEX_HOME/state"
        ;;
    *)
        case "${XDG_DATA_HOME:-}" in
            /*) ghostex_data_dir="${XDG_DATA_HOME%/}/ghostex" ;;
            *) ghostex_data_dir="$HOME/.local/share/ghostex" ;;
        esac
        case "${XDG_STATE_HOME:-}" in
            /*) ghostex_state_dir="${XDG_STATE_HOME%/}/ghostex" ;;
            *) ghostex_state_dir="$HOME/.local/state/ghostex" ;;
        esac
        ;;
esac
printf '%s\n%s\n' "$ghostex_data_dir" "$ghostex_state_dir"
"#;

#[derive(Clone, Debug)]
pub(super) struct WslGhostexPaths {
    data_dir: String,
    state_dir: String,
}

impl WslGhostexPaths {
    pub(super) fn data_path(&self, relative: &str) -> String {
        wsl_path_join(&self.data_dir, relative)
    }

    pub(super) fn state_path(&self, relative: &str) -> String {
        wsl_path_join(&self.state_dir, relative)
    }
}

pub(in crate::windows_terminal_backend) fn wsl_path_for_windows_path(
    path: &Path,
) -> Result<String, String> {
    if super::current_preference() == WindowsTerminalBackendPreference::PowerShell {
        return super::native::path(path);
    }
    let ResolvedWindowsTerminalBackend::Wsl { distribution } =
        resolve(super::current_preference())?
    else {
        return Err("This feature requires the WSL environment. Native PowerShell supports Windows agents and project terminals.".into());
    };
    let output = hidden_command("wsl.exe")
        .args([
            "--distribution",
            distribution.as_str(),
            "--exec",
            "wslpath",
            "-a",
            "-u",
            "--",
        ])
        .arg(path)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|_| "Could not translate the selected Windows folder into WSL.".to_string())?;
    if !output.status.success() {
        return Err("Could not translate the selected Windows folder into WSL.".to_string());
    }
    let translated = decode_windows_command_output(&output.stdout);
    let translated = translated.trim();
    if !translated.starts_with('/')
        || translated.len() > 32_768
        || translated
            .chars()
            .any(|ch| ch == '\0' || ch == '\r' || ch == '\n')
    {
        return Err("WSL returned an invalid path for the selected Windows folder.".to_string());
    }
    Ok(translated.to_string())
}

pub(in crate::windows_terminal_backend) fn windows_path_for_wsl_path(
    path: &Path,
) -> Result<PathBuf, String> {
    if super::current_preference() == WindowsTerminalBackendPreference::PowerShell {
        return super::native::path(path).map(PathBuf::from);
    }
    /*
    CDXC:PlatformSupport 2026-08-09:
    Windows project paths are authoritative paths inside the selected WSL2
    distribution. Docs performs filesystem operations in the native GPUI
    process, so translate that exact WSL path through the retained
    distribution into its Win32 drive or UNC representation before probing
    it. Never treat a POSIX path as a native Windows path and never select a
    second distribution from the path itself.
    */
    let wsl_path = path.to_string_lossy();
    validated_wsl_path(&wsl_path)
        .ok_or_else(|| "The project path is not a valid WSL path.".to_string())?;
    let ResolvedWindowsTerminalBackend::Wsl { distribution } =
        resolve(super::current_preference())?
    else {
        return Err("This feature requires the WSL environment. Native PowerShell supports Windows agents and project terminals.".into());
    };
    let output = hidden_command("wsl.exe")
        .args([
            "--distribution",
            distribution.as_str(),
            "--exec",
            "wslpath",
            "-a",
            "-w",
            "--",
        ])
        .arg(path)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|_| "Could not translate the WSL project path into Windows.".to_string())?;
    if !output.status.success() {
        return Err("Could not translate the WSL project path into Windows.".to_string());
    }
    let translated = decode_windows_command_output(&output.stdout);
    let translated = translated.trim();
    let translated_path = PathBuf::from(translated);
    if translated.is_empty()
        || translated.len() > 32_768
        || translated.chars().any(char::is_control)
        || !translated_path.is_absolute()
    {
        return Err("WSL returned an invalid Windows project path.".to_string());
    }
    Ok(translated_path)
}

pub(super) fn validated_wsl_path(path: &str) -> Option<String> {
    (path.starts_with('/')
        && path.len() <= 32_768
        && !path
            .chars()
            .any(|ch| ch == '\0' || ch == '\r' || ch == '\n'))
    .then(|| path.to_string())
}

pub(super) fn resolve_wsl_ghostex_paths(distribution: &str) -> Result<WslGhostexPaths, String> {
    let output = run_wsl_capture(distribution, WSL_STORAGE_PATHS_SCRIPT).ok_or_else(|| {
        "Could not resolve Ghostex storage paths inside the selected WSL2 distribution.".to_string()
    })?;
    let mut lines = output.lines();
    let data_dir = lines
        .next()
        .and_then(validated_wsl_path)
        .ok_or_else(|| "WSL returned an invalid Ghostex data directory.".to_string())?;
    let state_dir = lines
        .next()
        .and_then(validated_wsl_path)
        .ok_or_else(|| "WSL returned an invalid Ghostex state directory.".to_string())?;
    if lines.next().is_some() {
        return Err("WSL returned invalid Ghostex storage paths.".to_string());
    }
    Ok(WslGhostexPaths {
        data_dir,
        state_dir,
    })
}

fn wsl_path_join(root: &str, relative: &str) -> String {
    format!("{}/{}", root.trim_end_matches('/'), relative)
}

pub(super) fn source_runtime_wsl_path(path: &Path) -> Result<String, String> {
    let path = path.to_string_lossy();
    if path.starts_with('/') {
        return validated_wsl_path(&path)
            .ok_or_else(|| "Source path is not a valid WSL path.".to_string());
    }
    wsl_path_for_windows_path(Path::new(path.as_ref()))
}

pub(super) fn configured_wsl_distribution() -> Result<Option<String>, String> {
    let configured = env::var("GHOSTEX_WINDOWS_WSL_DISTRIBUTION")
        .ok()
        .or_else(|| {
            crate::shared_settings::shared_sidebar_settings_snapshot()
                .object()
                .get("windowsWslDistribution")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();
    let configured = configured.trim();
    if configured.is_empty() {
        return Ok(None);
    }
    if configured.len() > 128 || configured.chars().any(char::is_control) {
        return Err(
                "The configured WSL distribution name is invalid. Use the exact name from `wsl.exe --list --verbose`."
                    .to_string(),
            );
    }
    Ok(Some(configured.to_string()))
}

#[derive(Clone)]
struct Wsl2Distribution {
    name: String,
    is_default: bool,
}

fn initialized_wsl2_distributions() -> Vec<Wsl2Distribution> {
    let output = hidden_command("wsl.exe")
        .args(["--list", "--verbose"])
        .stdin(Stdio::null())
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let listing = decode_windows_command_output(&output.stdout);
    let mut candidates = Vec::new();
    for raw_line in listing.lines().skip(1) {
        let line = raw_line.trim_matches(|ch: char| ch == '\0' || ch.is_whitespace());
        if line.is_empty() {
            continue;
        }
        let is_default = line.starts_with('*');
        let columns = line
            .trim_start_matches('*')
            .trim()
            .split_whitespace()
            .collect::<Vec<_>>();
        if columns.len() < 3 || columns.last().copied() != Some("2") {
            continue;
        }
        // NAME may contain spaces; STATE and VERSION are the final two columns.
        let name = columns[..columns.len() - 2].join(" ");
        let normalized_name = name.to_ascii_lowercase();
        if name.is_empty()
            || normalized_name == "docker-desktop"
            || normalized_name == "docker-desktop-data"
            || !wsl_distribution_is_initialized(&name)
        {
            continue;
        }
        candidates.push(Wsl2Distribution { name, is_default });
    }
    candidates
}

pub(in crate::windows_terminal_backend) fn readiness() -> WindowsWslReadiness {
    if !hidden_command("wsl.exe")
        .args(["--list", "--quiet"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
    {
        return WindowsWslReadiness::MissingWsl;
    }
    let requested_distribution = match configured_wsl_distribution() {
        Ok(distribution) => distribution,
        Err(_) => return WindowsWslReadiness::MissingDistribution,
    };
    let distributions = initialized_wsl2_distributions();
    if let Some(requested) = requested_distribution {
        return if distributions
            .iter()
            .any(|distribution| distribution.name.eq_ignore_ascii_case(&requested))
        {
            WindowsWslReadiness::Ready
        } else {
            WindowsWslReadiness::ConfiguredDistributionUnavailable(requested)
        };
    }
    match distributions.as_slice() {
        [] => WindowsWslReadiness::MissingDistribution,
        [_] => WindowsWslReadiness::Ready,
        _ => WindowsWslReadiness::ChooseDistribution(
            distributions
                .into_iter()
                .map(|distribution| distribution.name)
                .collect(),
        ),
    }
}

pub(super) fn detect_initialized_wsl2_distribution() -> Option<String> {
    let candidates = initialized_wsl2_distributions();
    candidates
        .iter()
        .find(|candidate| candidate.is_default)
        .or_else(|| candidates.first())
        .map(|candidate| candidate.name.clone())
}

pub(super) fn resolve_initialized_wsl2_distribution(requested: &str) -> Option<String> {
    initialized_wsl2_distributions()
        .into_iter()
        .find(|candidate| candidate.name.eq_ignore_ascii_case(requested))
        .map(|candidate| candidate.name)
}

fn wsl_distribution_is_initialized(distribution: &str) -> bool {
    run_wsl_status(
        distribution,
        "test -n \"${HOME:-}\" && test -r /etc/os-release && command -v sh >/dev/null",
    )
}

pub(super) fn run_wsl_status(distribution: &str, script: &str) -> bool {
    hidden_command("wsl.exe")
        .args([
            "--distribution",
            distribution,
            "--exec",
            "sh",
            "-lc",
            script,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub(super) fn run_wsl_capture(distribution: &str, script: &str) -> Option<String> {
    let output = hidden_command("wsl.exe")
        .args([
            "--distribution",
            distribution,
            "--exec",
            "sh",
            "-lc",
            script,
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| decode_windows_command_output(&output.stdout))
}

pub(super) fn run_wsl_checked(distribution: &str, script: &str) -> Result<(), String> {
    let output = hidden_command("wsl.exe")
        .args([
            "--distribution",
            distribution,
            "--exec",
            "sh",
            "-lc",
            script,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("Could not run wsl.exe: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = decode_windows_command_output(&output.stderr);
    let detail = stderr.trim().chars().take(4096).collect::<String>();
    if detail.is_empty() {
        Err(format!("wsl.exe {}", output.status))
    } else {
        Err(format!("wsl.exe {}: {detail}", output.status))
    }
}

pub(super) fn spawn_gxserver_owner(
    distribution: &str,
    runtime_file: &str,
) -> Result<Option<Child>, String> {
    /*
    A detached Linux daemon does not keep a WSL distribution alive. Retain
    one hidden Windows-owned `wsl.exe` execution for the lifetime of the
    exact gxserver pid that startup validated. A boot-id/pid marker makes
    this idempotent across GPUI relaunches without turning a systemd unit or
    dummy process into a second lifecycle authority.
    */
    let script = format!(
        r#"set -eu
runtime_file={}
test -r "$runtime_file"
server_pid="$(sed -n 's/.*"pid"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*/\1/p' "$runtime_file" | head -n 1)"
case "$server_pid" in ''|*[!0-9]*) exit 1;; esac
kill -0 "$server_pid" 2>/dev/null
runtime_dir="${{runtime_file%/server.json}}"
owner_dir="$runtime_dir/windows-app-owner"
owner_record="$owner_dir/process"
boot_id="$(cat /proc/sys/kernel/random/boot_id)"
if ! mkdir "$owner_dir" 2>/dev/null; then
  set -- $(cat "$owner_record" 2>/dev/null || true)
  if [ "${{1:-}}" = "$boot_id" ] && [ "${{3:-}}" = "$server_pid" ] && kill -0 "${{2:-0}}" 2>/dev/null; then
    printf 'existing\n'
    exit 0
  fi
  rm -f "$owner_record"
  rmdir "$owner_dir" 2>/dev/null || exit 1
  mkdir "$owner_dir"
fi
owner_value="$boot_id $$ $server_pid"
printf '%s\n' "$owner_value" > "$owner_record"
cleanup_owner() {{
  current_value="$(cat "$owner_record" 2>/dev/null || true)"
  if [ "$current_value" = "$owner_value" ]; then
    rm -f "$owner_record"
    rmdir "$owner_dir" 2>/dev/null || true
  fi
}}
trap cleanup_owner EXIT HUP INT TERM
printf 'ready\n'
exec 1>&-
while kill -0 "$server_pid" 2>/dev/null; do sleep 5; done"#,
        posix_single_quote(runtime_file),
    );
    let mut child = hidden_command("wsl.exe")
        .args([
            "--distribution",
            distribution,
            "--exec",
            "sh",
            "-lc",
            script.as_str(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Could not start the WSL gxserver lifetime owner.".to_string())?;
    let mut readiness = String::new();
    let readiness_result = child.stdout.take().ok_or(()).and_then(|stdout| {
        BufReader::new(stdout)
            .read_line(&mut readiness)
            .map_err(|_| ())
    });
    match (readiness_result, readiness.trim()) {
        (Ok(_), "ready") => Ok(Some(child)),
        (Ok(_), "existing") => match child.wait() {
            Ok(status) if status.success() => Ok(None),
            _ => {
                Err("The existing WSL gxserver lifetime owner could not be validated.".to_string())
            }
        },
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            Err("The WSL gxserver lifetime owner exited before startup completed.".to_string())
        }
    }
}

pub(super) fn validated_auth_token(value: &str) -> Option<String> {
    let token = value.trim_matches(|ch: char| ch == '\0' || ch.is_whitespace());
    (!token.is_empty()
        && token.len() <= 256
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')))
    .then(|| token.to_string())
}

fn decode_windows_command_output(bytes: &[u8]) -> String {
    if bytes.len() >= 2
        && (bytes.starts_with(&[0xff, 0xfe])
            || bytes
                .iter()
                .skip(1)
                .step_by(2)
                .take(8)
                .any(|byte| *byte == 0))
    {
        let start = usize::from(bytes.starts_with(&[0xff, 0xfe])) * 2;
        let units = bytes[start..]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]));
        return String::from_utf16_lossy(&units.collect::<Vec<_>>());
    }
    String::from_utf8_lossy(bytes).replace('\0', "")
}

pub(super) fn hidden_command(program: &str) -> Command {
    let mut command = Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

pub(super) fn posix_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
