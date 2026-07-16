//! Windows terminal/backend integration.
//!
//! PowerShell remains a native ConPTY backend.  WSL mode deliberately runs
//! the existing Linux gxserver + zmx package inside an initialized WSL2
//! distribution instead of pretending the Unix persistence stack is native
//! Windows software.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WindowsTerminalBackendPreference {
    Automatic,
    Wsl,
    PowerShell,
}

impl WindowsTerminalBackendPreference {
    pub(crate) fn from_settings_value(value: Option<&str>) -> Self {
        match value {
            Some("wsl") => Self::Wsl,
            Some("powershell") => Self::PowerShell,
            _ => Self::Automatic,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ResolvedWindowsTerminalBackend {
    Wsl { distribution: String },
    PowerShell,
}

pub(crate) fn current_preference() -> WindowsTerminalBackendPreference {
    let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
    WindowsTerminalBackendPreference::from_settings_value(
        settings
            .object()
            .get("windowsTerminalBackend")
            .and_then(serde_json::Value::as_str),
    )
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{ResolvedWindowsTerminalBackend, WindowsTerminalBackendPreference};
    use std::{
        env,
        ffi::OsString,
        fs, io,
        path::{Path, PathBuf},
        process::{Command, Stdio},
        sync::{Mutex, OnceLock},
    };

    use std::os::windows::process::CommandExt as _;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const WSL_GXSERVER_PATH: &str = "$HOME/.ghostex/gxserver/package/bin/gxserver";
    const WSL_ZMX_PATH: &str = "$HOME/.ghostex/gxserver/package/bin/zmx";
    const WSL_PACKAGE_IDENTITY_PATH: &str = "$HOME/.ghostex/gxserver/windows-app-runtime.sha256";

    struct PackagedGxserver {
        archive_path: PathBuf,
        sha256: Option<String>,
    }

    #[derive(Default)]
    struct WindowsWslState {
        detection_complete: bool,
        distribution: Option<String>,
        auth_token: Option<String>,
        package_update_required: bool,
    }

    static STATE: OnceLock<Mutex<WindowsWslState>> = OnceLock::new();

    fn state() -> &'static Mutex<WindowsWslState> {
        STATE.get_or_init(|| Mutex::new(WindowsWslState::default()))
    }

    pub(super) fn reset() {
        if let Ok(mut state) = state().lock() {
            *state = WindowsWslState::default();
        }
    }

    pub(super) fn mark_package_update_required() {
        if let Ok(mut state) = state().lock() {
            state.package_update_required = true;
        }
    }

    pub(super) fn auth_token() -> Option<String> {
        state().lock().ok()?.auth_token.clone()
    }

    pub(super) fn resolve(
        preference: WindowsTerminalBackendPreference,
    ) -> Result<ResolvedWindowsTerminalBackend, String> {
        if preference == WindowsTerminalBackendPreference::PowerShell {
            if let Ok(mut state) = state().lock() {
                state.auth_token = None;
            }
            return Ok(ResolvedWindowsTerminalBackend::PowerShell);
        }

        let cached = state()
            .lock()
            .ok()
            .and_then(|state| state.detection_complete.then(|| state.distribution.clone()));
        let distribution = match cached {
            Some(distribution) => distribution,
            None => {
                let detected = detect_initialized_wsl2_distribution();
                if let Ok(mut state) = state().lock() {
                    state.detection_complete = true;
                    state.distribution = detected.clone();
                    if detected.is_none() {
                        state.auth_token = None;
                    }
                }
                detected
            }
        };

        match (preference, distribution) {
            (_, Some(distribution)) => Ok(ResolvedWindowsTerminalBackend::Wsl { distribution }),
            (WindowsTerminalBackendPreference::Automatic, None) => {
                Ok(ResolvedWindowsTerminalBackend::PowerShell)
            }
            (WindowsTerminalBackendPreference::Wsl, None) => Err(
                "WSL mode requires WSL2 and an initialized Linux distribution. Install and open a distribution once, then retry; Ghostex will not run wsl --install automatically."
                    .to_string(),
            ),
            (WindowsTerminalBackendPreference::PowerShell, None) => {
                Ok(ResolvedWindowsTerminalBackend::PowerShell)
            }
        }
    }

    pub(super) fn prepare_gxserver(
        preference: WindowsTerminalBackendPreference,
    ) -> Result<ResolvedWindowsTerminalBackend, String> {
        let backend = resolve(preference)?;
        let ResolvedWindowsTerminalBackend::Wsl { distribution } = &backend else {
            return Ok(backend);
        };
        if let Ok(mut state) = state().lock() {
            // A failed restart must never leave a previously read daemon token
            // available to a new sidebar bootstrap.
            state.auth_token = None;
        }

        let package = resolve_packaged_gxserver().ok_or_else(|| {
            "The Ghostex installer does not contain the WSL gxserver runtime for this Windows architecture. Reinstall this Ghostex build or select PowerShell in Settings."
                .to_string()
        })?;
        let update_required = state()
            .lock()
            .map(|state| state.package_update_required)
            .unwrap_or(false);
        let installed = run_wsl_status(distribution, &format!("test -x {WSL_GXSERVER_PATH}"));
        let installed_package_matches = package.sha256.as_deref().is_none_or(|expected| {
            run_wsl_capture(
                distribution,
                &format!("test -r {WSL_PACKAGE_IDENTITY_PATH} && cat {WSL_PACKAGE_IDENTITY_PATH}"),
            )
            .is_some_and(|actual| actual.trim() == expected)
        });
        if update_required || !installed || !installed_package_matches {
            install_packaged_gxserver(distribution, &package)?;
            if let Ok(mut state) = state().lock() {
                state.package_update_required = false;
            }
        }

        let start_script = format!(
            "set -eu; test -x {WSL_GXSERVER_PATH}; {WSL_GXSERVER_PATH} start --json >/dev/null"
        );
        if !run_wsl_status(distribution, &start_script) {
            return Err("gxserver could not start inside the selected WSL2 distribution.".into());
        }
        let token = run_wsl_capture(
            distribution,
            "set -eu; token_file=\"$HOME/.ghostex/gxserver/auth/token\"; test -f \"$token_file\"; cat \"$token_file\"",
        )
        .and_then(|value| validated_auth_token(&value))
        .ok_or_else(|| "gxserver started in WSL, but its authentication token is unavailable.".to_string())?;
        if let Ok(mut state) = state().lock() {
            state.distribution = Some(distribution.clone());
            state.auth_token = Some(token);
        }
        Ok(backend)
    }

    pub(super) fn terminal_invocation(
        command: Option<String>,
        working_directory: Option<&std::path::Path>,
    ) -> (String, Vec<String>) {
        match resolve(super::current_preference()) {
            Ok(ResolvedWindowsTerminalBackend::Wsl { distribution }) => {
                let mut command = command.unwrap_or_else(|| {
                    "if [ -n \"${SHELL:-}\" ] && [ -x \"$SHELL\" ]; then exec \"$SHELL\" -l; elif [ -x /bin/bash ]; then exec /bin/bash -l; else exec /bin/sh -l; fi".to_string()
                });
                let mut args = vec![
                    "--distribution".to_string(),
                    distribution,
                    "--".to_string(),
                    "sh".to_string(),
                    "-lc".to_string(),
                ];
                if let Some(working_directory) = working_directory {
                    /*
                    Pass the Windows path as an argv value, never shell text.
                    wslpath performs the drive/UNC translation inside the
                    selected distribution before the requested command runs.
                    Attach payloads already contain authoritative WSL paths
                    from gxserver and therefore normally have no host cwd here.
                    */
                    command = format!(
                        "wsl_cwd=$(wslpath -a -u \"$1\") && cd \"$wsl_cwd\" && {command}"
                    );
                    args.push(command);
                    args.push("ghostex-wsl".to_string());
                    args.push(working_directory.to_string_lossy().into_owned());
                } else {
                    args.push(command);
                }
                ("wsl.exe".to_string(), args)
            }
            Ok(ResolvedWindowsTerminalBackend::PowerShell) => powershell_invocation(command),
            Err(_) => (
                "powershell.exe".to_string(),
                vec![
                    "-NoLogo".to_string(),
                    "-NoExit".to_string(),
                    "-Command".to_string(),
                    "Write-Error 'Ghostex WSL mode requires WSL2 and an initialized Linux distribution. Open Settings and select PowerShell, or finish WSL setup.'; exit 1".to_string(),
                ],
            ),
        }
    }

    fn powershell_invocation(command: Option<String>) -> (String, Vec<String>) {
        match command {
            Some(command) => (
                "powershell.exe".to_string(),
                vec!["-NoLogo".to_string(), "-Command".to_string(), command],
            ),
            None => ("powershell.exe".to_string(), vec!["-NoLogo".to_string()]),
        }
    }

    pub(super) fn spawn_zmx_refresh(
        distribution: &str,
        session_name: &str,
        rows: u16,
        columns: u16,
    ) -> Result<std::process::Child, String> {
        let script = format!(
            "exec {WSL_ZMX_PATH} refresh-if-stale {} {} {}",
            posix_single_quote(session_name),
            rows,
            columns,
        );
        hidden_command("wsl.exe")
            .args([
                "--distribution",
                distribution,
                "--",
                "sh",
                "-lc",
                script.as_str(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "Could not request a WSL zmx viewport refresh.".to_string())
    }

    fn detect_initialized_wsl2_distribution() -> Option<String> {
        let output = hidden_command("wsl.exe")
            .args(["--list", "--verbose"])
            .stdin(Stdio::null())
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let listing = decode_windows_command_output(&output.stdout);
        let mut default_candidate = None;
        let mut other_candidates = Vec::new();
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
            if is_default {
                default_candidate = Some(name);
            } else {
                other_candidates.push(name);
            }
        }
        default_candidate.or_else(|| other_candidates.into_iter().next())
    }

    fn wsl_distribution_is_initialized(distribution: &str) -> bool {
        run_wsl_status(
            distribution,
            "test -n \"${HOME:-}\" && test -r /etc/os-release && command -v sh >/dev/null",
        )
    }

    fn install_packaged_gxserver(
        distribution: &str,
        package: &PackagedGxserver,
    ) -> Result<(), String> {
        let mut archive = fs::File::open(&package.archive_path)
            .map_err(|_| "The packaged WSL gxserver runtime could not be read.".to_string())?;
        let script = "set -eu; install_root=\"$HOME/.ghostex/gxserver\"; release_dir=\"$install_root/releases/windows-app-$(date +%s)-$$\"; mkdir -p \"$release_dir\"; tar -xzf - -C \"$release_dir\"; test -x \"$release_dir/bin/gxserver\"; \"$release_dir/bin/gxserver\" setup --install-root \"$install_root\" --release-dir \"$release_dir\" >/dev/null; if [ -n \"$1\" ]; then printf '%s\\n' \"$1\" >\"$install_root/windows-app-runtime.sha256\"; else rm -f \"$install_root/windows-app-runtime.sha256\"; fi";
        let mut child = hidden_command("wsl.exe")
            .args([
                "--distribution",
                distribution,
                "--",
                "sh",
                "-lc",
                script,
                "ghostex-wsl-installer",
                package.sha256.as_deref().unwrap_or(""),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "Could not start WSL to install gxserver.".to_string())?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Could not stream the gxserver runtime into WSL.".to_string())?;
        if io::copy(&mut archive, &mut stdin).is_err() {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Could not stream the gxserver runtime into WSL.".to_string());
        }
        drop(stdin);
        let status = child
            .wait()
            .map_err(|_| "The WSL gxserver installer did not finish.".to_string())?;
        status.success().then_some(()).ok_or_else(|| {
            "The WSL gxserver runtime could not be installed in the selected distribution."
                .to_string()
        })
    }

    fn resolve_packaged_gxserver() -> Option<PackagedGxserver> {
        if let Some(path) = env::var_os("GHOSTEX_WSL_GXSERVER_ARCHIVE") {
            let path = PathBuf::from(path);
            if path.is_absolute() && path.is_file() {
                let sha256 = packaged_archive_identity(&path);
                return Some(PackagedGxserver {
                    archive_path: path,
                    sha256,
                });
            }
            return None;
        }
        #[cfg(target_arch = "x86_64")]
        const ARCHIVE_NAME: &str = "gxserver-linux-x64.tar.gz";
        #[cfg(target_arch = "aarch64")]
        const ARCHIVE_NAME: &str = "gxserver-linux-arm64.tar.gz";
        let executable_dir = env::current_exe().ok()?.parent()?.to_path_buf();
        let archive_path = executable_dir
            .join("resources")
            .join("wsl")
            .join(ARCHIVE_NAME);
        archive_path.is_file().then(|| PackagedGxserver {
            sha256: packaged_archive_identity(&archive_path),
            archive_path,
        })
    }

    fn packaged_archive_identity(archive_path: &Path) -> Option<String> {
        let mut sidecar_name: OsString = archive_path.as_os_str().to_owned();
        sidecar_name.push(".sha256");
        let value = fs::read_to_string(PathBuf::from(sidecar_name)).ok()?;
        let sha256 = value.trim();
        (sha256.len() == 64
            && sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
        .then(|| sha256.to_string())
    }

    fn run_wsl_status(distribution: &str, script: &str) -> bool {
        hidden_command("wsl.exe")
            .args(["--distribution", distribution, "--", "sh", "-lc", script])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    fn run_wsl_capture(distribution: &str, script: &str) -> Option<String> {
        let output = hidden_command("wsl.exe")
            .args(["--distribution", distribution, "--", "sh", "-lc", script])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| decode_windows_command_output(&output.stdout))
    }

    fn validated_auth_token(value: &str) -> Option<String> {
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

    fn hidden_command(program: &str) -> Command {
        let mut command = Command::new(program);
        command.creation_flags(CREATE_NO_WINDOW);
        command
    }

    fn posix_single_quote(value: &str) -> String {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn mark_package_update_required() {
    platform::mark_package_update_required();
}

#[cfg(target_os = "windows")]
pub(crate) fn reset() {
    platform::reset();
}

#[cfg(target_os = "windows")]
pub(crate) fn auth_token() -> Option<String> {
    if current_preference() == WindowsTerminalBackendPreference::PowerShell {
        return None;
    }
    platform::auth_token()
}

#[cfg(target_os = "windows")]
pub(crate) fn resolve_current() -> Result<ResolvedWindowsTerminalBackend, String> {
    platform::resolve(current_preference())
}

#[cfg(target_os = "windows")]
pub(crate) fn prepare_gxserver_for_current_settings()
-> Result<ResolvedWindowsTerminalBackend, String> {
    platform::prepare_gxserver(current_preference())
}

#[cfg(target_os = "windows")]
pub(crate) fn terminal_invocation(
    command: Option<String>,
    working_directory: Option<&std::path::Path>,
) -> (String, Vec<String>) {
    platform::terminal_invocation(command, working_directory)
}

#[cfg(target_os = "windows")]
pub(crate) fn spawn_zmx_refresh(
    distribution: &str,
    session_name: &str,
    rows: u16,
    columns: u16,
) -> Result<std::process::Child, String> {
    platform::spawn_zmx_refresh(distribution, session_name, rows, columns)
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn auth_token() -> Option<String> {
    None
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn mark_package_update_required() {}

#[cfg(not(target_os = "windows"))]
pub(crate) fn reset() {}
