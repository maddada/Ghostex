//! Windows terminal/backend integration.
//!
//! Windows currently runs only through WSL2, using Linux gxserver, zmx,
//! Source/code-server runtimes inside an initialized distribution.
//! PowerShell support remains a later phase and is never selected as a fallback.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WindowsTerminalBackendPreference {
    Automatic,
    Wsl,
    PowerShell,
}

impl WindowsTerminalBackendPreference {
    pub(crate) fn from_settings_value(_value: Option<&str>) -> Self {
        Self::Wsl
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
        fs,
        io::{self, BufRead, BufReader},
        path::{Path, PathBuf},
        process::{Child, Command, Stdio},
        sync::{Mutex, OnceLock},
    };

    use std::os::windows::process::CommandExt as _;

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

    struct PackagedGxserver {
        archive_path: PathBuf,
        sha256: Option<String>,
    }

    struct PackagedSourceRuntime {
        archive_path: PathBuf,
        component_version: String,
    }

    #[derive(Clone, Debug)]
    struct WslGhostexPaths {
        data_dir: String,
        state_dir: String,
    }

    impl WslGhostexPaths {
        fn data_path(&self, relative: &str) -> String {
            wsl_path_join(&self.data_dir, relative)
        }

        fn state_path(&self, relative: &str) -> String {
            wsl_path_join(&self.state_dir, relative)
        }
    }

    #[derive(Default)]
    struct WindowsWslState {
        detection_complete: bool,
        requested_distribution: Option<String>,
        distribution: Option<String>,
        auth_token: Option<String>,
        package_update_required: bool,
        gxserver_owner: Option<Child>,
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

    pub(super) fn t3_runtime_launch_plan() -> Result<(PathBuf, PathBuf), String> {
        /*
        CDXC:GPUIWindowsT3Code 2026-07-26:
        Windows does not execute the managed T3 server on the Win32 host. Resolve
        the exact packaged Node/entrypoint paths from the selected WSL
        distribution so the WSL gxserver can validate and own the launch plan.
        Do not translate these paths through the Windows filesystem or probe a
        second distribution.
        */
        let ResolvedWindowsTerminalBackend::Wsl { distribution } =
            resolve(super::current_preference())?
        else {
            unreachable!("PowerShell is not a selectable Windows terminal backend")
        };
        let paths = resolve_wsl_ghostex_paths(&distribution)?;
        let node_path = paths.data_path("source-runtime/package/t3code-server/lib/node");
        let entrypoint_path = paths.data_path("source-runtime/package/t3code-server/dist/bin.mjs");
        let output = run_wsl_capture(
            &distribution,
            &format!(
                "set -eu; node={}; entrypoint={}; test -x \"$node\"; test -f \"$entrypoint\"; printf '%s\\n%s\\n' \"$node\" \"$entrypoint\"",
                posix_single_quote(&node_path),
                posix_single_quote(&entrypoint_path),
            ),
        )
        .ok_or_else(|| {
            "The managed T3 Code runtime is unavailable in the selected WSL2 distribution. Reinstall this Ghostex build."
                .to_string()
        })?;
        let mut lines = output
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty());
        let node_path = lines
            .next()
            .and_then(validated_wsl_path)
            .ok_or_else(|| "WSL returned an invalid T3 Code Node path.".to_string())?;
        let entrypoint_path = lines
            .next()
            .and_then(validated_wsl_path)
            .ok_or_else(|| "WSL returned an invalid T3 Code entrypoint path.".to_string())?;
        if lines.next().is_some() {
            return Err("WSL returned an invalid T3 Code launch plan.".to_string());
        }
        Ok((PathBuf::from(node_path), PathBuf::from(entrypoint_path)))
    }

    pub(super) fn t3_owner_bearer_token() -> Result<String, String> {
        let ResolvedWindowsTerminalBackend::Wsl { distribution } =
            resolve(super::current_preference())?
        else {
            unreachable!("PowerShell is not a selectable Windows terminal backend")
        };
        let paths = resolve_wsl_ghostex_paths(&distribution)?;
        let token_file = paths.data_path("t3-runtime/auth-state.json");
        run_wsl_capture(
            &distribution,
            &format!(
                "set -eu; token_file={}; test -r \"$token_file\"; cat \"$token_file\"",
                posix_single_quote(&token_file),
            ),
        )
        .and_then(|text| {
            let value = serde_json::from_str::<serde_json::Value>(&text).ok()?;
            let object = value.as_object()?;
            (object.get("provider").and_then(serde_json::Value::as_str) == Some("t3code"))
                .then_some(())?;
            object
                .get("ownerBearerToken")
                .and_then(serde_json::Value::as_str)
                .and_then(validated_t3_owner_bearer_token)
        })
        .ok_or_else(|| "T3 owner authorization is unavailable in WSL.".to_string())
    }

    pub(super) fn resolve(
        _preference: WindowsTerminalBackendPreference,
    ) -> Result<ResolvedWindowsTerminalBackend, String> {
        let requested_distribution = configured_wsl_distribution()?;
        let cached = state().lock().ok().and_then(|state| {
            (state.detection_complete && state.requested_distribution == requested_distribution)
                .then(|| state.distribution.clone())
        });
        let distribution = match cached {
            Some(distribution) => distribution,
            None => {
                let detected = match requested_distribution.as_deref() {
                    Some(requested) => resolve_initialized_wsl2_distribution(requested)
                        .ok_or_else(|| {
                            format!(
                                "The configured WSL distribution '{requested}' is not an initialized WSL2 distribution. Update Windows Settings > Terminal > WSL Distribution using the exact name from `wsl.exe --list --verbose`."
                            )
                        })
                        .map(Some)?,
                    None => detect_initialized_wsl2_distribution(),
                };
                if let Ok(mut state) = state().lock() {
                    state.detection_complete = true;
                    state.requested_distribution = requested_distribution.clone();
                    state.distribution = detected.clone();
                    if detected.is_none() {
                        state.auth_token = None;
                    }
                }
                detected
            }
        };

        distribution
            .map(|distribution| ResolvedWindowsTerminalBackend::Wsl { distribution })
            .ok_or_else(|| {
                "Ghostex for Windows requires WSL2 and an initialized Linux distribution. Install and open a distribution once, or set Windows Settings > Terminal > WSL Distribution to its exact name; Ghostex will not run `wsl --install` automatically."
                    .to_string()
            })
    }

    pub(super) fn prepare_gxserver(
        preference: WindowsTerminalBackendPreference,
    ) -> Result<ResolvedWindowsTerminalBackend, String> {
        let backend = resolve(preference)?;
        let ResolvedWindowsTerminalBackend::Wsl { distribution } = &backend else {
            return Ok(backend);
        };
        let paths = resolve_wsl_ghostex_paths(distribution)?;
        if let Ok(mut state) = state().lock() {
            // A failed restart must never leave a previously read daemon token
            // available to a new sidebar bootstrap.
            state.auth_token = None;
        }

        let package = resolve_packaged_gxserver().ok_or_else(|| {
            "The Ghostex installer does not contain the WSL gxserver runtime for this Windows architecture. Reinstall this Ghostex build."
                .to_string()
        })?;
        let update_required = state()
            .lock()
            .map(|state| state.package_update_required)
            .unwrap_or(false);
        let gxserver_install_root = paths.data_path("gxserver");
        let gxserver_path = paths.data_path("gxserver/package/bin/gxserver");
        let package_identity_path = paths.data_path("gxserver/windows-app-runtime.sha256");
        let installed = run_wsl_status(
            distribution,
            &format!("test -x {}", posix_single_quote(&gxserver_path)),
        );
        let installed_package_matches = package.sha256.as_deref().is_none_or(|expected| {
            run_wsl_capture(
                distribution,
                &format!(
                    "test -r {} && cat {}",
                    posix_single_quote(&package_identity_path),
                    posix_single_quote(&package_identity_path),
                ),
            )
            .is_some_and(|actual| actual.trim() == expected)
        });
        if update_required || !installed || !installed_package_matches {
            install_packaged_gxserver(distribution, &gxserver_install_root, &package)?;
        }

        if let Ok(mut state) = state().lock() {
            state.package_update_required = false;
        }

        let start_script = format!(
            "set -eu; gxserver={}; test -x \"$gxserver\"; GHOSTEX_T3_RUNTIME_COMMAND_SHELL=/bin/sh \"$gxserver\" start --json >/dev/null",
            posix_single_quote(&gxserver_path),
        );
        if !run_wsl_status(distribution, &start_script) {
            return Err("gxserver could not start inside the selected WSL2 distribution.".into());
        }
        let token_file = paths.state_path("gxserver/auth/token");
        let token = run_wsl_capture(
            distribution,
            &format!(
                "set -eu; token_file={}; test -f \"$token_file\"; cat \"$token_file\"",
                posix_single_quote(&token_file),
            ),
        )
        .and_then(|value| validated_auth_token(&value))
        .ok_or_else(|| {
            "gxserver started in WSL, but its authentication token is unavailable.".to_string()
        })?;
        let runtime_file = paths.state_path("gxserver/runtime/server.json");
        let gxserver_owner = spawn_gxserver_owner(distribution, &runtime_file)?;
        if let Ok(mut state) = state().lock() {
            state.distribution = Some(distribution.clone());
            state.auth_token = Some(token);
            if let Some(gxserver_owner) = gxserver_owner {
                state.gxserver_owner = Some(gxserver_owner);
            }
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
                    "--exec".to_string(),
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
                    command =
                        format!("wsl_cwd=$(wslpath -a -u \"$1\") && cd \"$wsl_cwd\" && {command}");
                    args.push(command);
                    args.push("ghostex-wsl".to_string());
                    args.push(working_directory.to_string_lossy().into_owned());
                } else {
                    args.push(command);
                }
                ("wsl.exe".to_string(), args)
            }
            Ok(ResolvedWindowsTerminalBackend::PowerShell) => {
                unreachable!("PowerShell is not a selectable Windows terminal backend")
            }
            Err(message) => (
                "wsl.exe".to_string(),
                vec![
                    "--exec".to_string(),
                    "sh".to_string(),
                    "-lc".to_string(),
                    format!(
                        "printf '%s\\n' {} >&2; exit 1",
                        posix_single_quote(&message)
                    ),
                ],
            ),
        }
    }

    pub(super) fn spawn_zmx_refresh(
        distribution: &str,
        session_name: &str,
        rows: u16,
        columns: u16,
    ) -> Result<std::process::Child, String> {
        let paths = resolve_wsl_ghostex_paths(distribution)?;
        let zmx_path = paths.data_path("gxserver/package/bin/zmx");
        let script = format!(
            "exec {} refresh-if-stale {} {} {}",
            posix_single_quote(&zmx_path),
            posix_single_quote(session_name),
            rows,
            columns,
        );
        hidden_command("wsl.exe")
            .args([
                "--distribution",
                distribution,
                "--exec",
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

    pub(super) fn resource_process_snapshot() -> Option<String> {
        let ResolvedWindowsTerminalBackend::Wsl { distribution } =
            resolve(super::current_preference()).ok()?
        else {
            return None;
        };
        run_wsl_capture(
            &distribution,
            "exec /bin/ps -axo pid=,ppid=,pcpu=,rss=,command=",
        )
    }

    pub(super) fn resource_server_snapshot() -> Option<String> {
        let ResolvedWindowsTerminalBackend::Wsl { distribution } =
            resolve(super::current_preference()).ok()?
        else {
            return None;
        };
        run_wsl_capture(
            &distribution,
            "test -x /usr/bin/lsof && exec /usr/bin/lsof -nP -iTCP -sTCP:LISTEN -F pcn",
        )
    }

    pub(super) fn source_code_server_command(
        project_path: &Path,
        required_node_major: u64,
        bind_address: &str,
        link_vscode_user_config: bool,
        use_vscode_insiders_user_config: bool,
    ) -> Result<Command, String> {
        let ResolvedWindowsTerminalBackend::Wsl { distribution } =
            resolve(super::current_preference())?
        else {
            unreachable!("PowerShell is not a selectable Windows terminal backend")
        };
        ensure_source_runtime_installed(&distribution)?;
        let wsl_project_path = source_runtime_wsl_path(project_path)?;
        let paths = resolve_wsl_ghostex_paths(&distribution)?;
        let repo_root = paths.data_path("source-runtime/package");
        let storage_root = paths.data_path("code-server-runtime-gpui");
        let script = r#"set -eu
repo_root="$1"
storage_root="$2"
project_path="$3"
required_node_major="$4"
bind_address="$5"
link_vscode_user_config="$6"
use_vscode_insiders_user_config="$7"

test -f "$repo_root/out/node/entry.js"
test -d "$project_path"

test -x "$repo_root/lib/node"
node="$repo_root/lib/node"
node_major="$("$node" -p 'process.versions.node.split(".")[0]')"
test "$node_major" = "$required_node_major"

unset VSCODE_IPC_HOOK_CLI
unset CODE_SERVER_PARENT_PID
unset VSCODE_DEV
export NODE_ENV=production
user_data_dir="$storage_root/user-data"
extensions_dir="$storage_root/extensions"
mkdir -p "$user_data_dir" "$extensions_dir"

set -- "$node" "$repo_root/out/node/entry.js"
if [ "$use_vscode_insiders_user_config" = "1" ]; then
    vscode_user_config_dir="$HOME/.config/Code - Insiders/User"
else
    vscode_user_config_dir="$HOME/.config/Code/User"
fi
if [ "$link_vscode_user_config" = "1" ]; then
    set -- "$@" --link-vscode-user-config --vscode-user-config-dir "$vscode_user_config_dir"
fi
if [ "$link_vscode_user_config" != "1" ] || [ ! -f "$vscode_user_config_dir/settings.json" ]; then
    settings_path="$user_data_dir/User/settings.json"
    if [ ! -e "$settings_path" ]; then
        mkdir -p "$user_data_dir/User"
        printf '%s\n' '{' '  "workbench.colorTheme": "Dark 2026"' '}' >"$settings_path"
    fi
fi

cd "$project_path"
exec "$@" \
    --auth none \
    --bind-addr "$bind_address" \
    --disable-telemetry \
    --disable-update-check \
    --disable-workspace-trust \
    --disable-getting-started-override \
    --ignore-last-opened \
    --app-name "ghostex Code" \
    --user-data-dir "$user_data_dir" \
    --extensions-dir "$extensions_dir"
"#;
        let required_node_major = required_node_major.to_string();
        let mut command = hidden_command("wsl.exe");
        command.args([
            "--distribution",
            distribution.as_str(),
            "--exec",
            "sh",
            "-lc",
            script,
            "ghostex-source",
            repo_root.as_str(),
            storage_root.as_str(),
            wsl_project_path.as_str(),
            required_node_major.as_str(),
            bind_address,
            if link_vscode_user_config { "1" } else { "0" },
            if use_vscode_insiders_user_config {
                "1"
            } else {
                "0"
            },
        ]);
        Ok(command)
    }

    pub(super) fn source_code_server_open_file_command(
        file_path: &Path,
        required_node_major: u64,
    ) -> Result<Command, String> {
        let ResolvedWindowsTerminalBackend::Wsl { distribution } =
            resolve(super::current_preference())?
        else {
            unreachable!("PowerShell is not a selectable Windows terminal backend")
        };
        let wsl_file_path = source_runtime_wsl_path(file_path)?;
        let paths = resolve_wsl_ghostex_paths(&distribution)?;
        let repo_root = paths.data_path("source-runtime/package");
        let user_data_dir = paths.data_path("code-server-runtime-gpui/user-data");
        let script = r#"set -eu
repo_root="$1"
user_data_dir="$2"
file_path="$3"
required_node_major="$4"
node="$repo_root/lib/node"
test -x "$node"
test -f "$repo_root/out/node/entry.js"
node_major="$("$node" -p 'process.versions.node.split(".")[0]')"
test "$node_major" = "$required_node_major"
unset VSCODE_IPC_HOOK_CLI
unset CODE_SERVER_PARENT_PID
unset VSCODE_DEV
export NODE_ENV=production
session_socket="$user_data_dir/code-server-ipc.sock"
cd "$(dirname "$file_path")"
exec "$node" "$repo_root/out/node/entry.js" \
    --user-data-dir "$user_data_dir" \
    --session-socket "$session_socket" \
    --reuse-window \
    "$file_path"
"#;
        let required_node_major = required_node_major.to_string();
        let mut command = hidden_command("wsl.exe");
        command.args([
            "--distribution",
            distribution.as_str(),
            "--exec",
            "sh",
            "-lc",
            script,
            "ghostex-source-open-file",
            repo_root.as_str(),
            user_data_dir.as_str(),
            wsl_file_path.as_str(),
            required_node_major.as_str(),
        ]);
        Ok(command)
    }

    pub(super) fn wsl_path_for_windows_path(path: &Path) -> Result<String, String> {
        let ResolvedWindowsTerminalBackend::Wsl { distribution } =
            resolve(super::current_preference())?
        else {
            unreachable!("PowerShell is not a selectable Windows terminal backend")
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
            return Err(
                "WSL returned an invalid path for the selected Windows folder.".to_string(),
            );
        }
        Ok(translated.to_string())
    }

    fn validated_wsl_path(path: &str) -> Option<String> {
        (path.starts_with('/')
            && path.len() <= 32_768
            && !path
                .chars()
                .any(|ch| ch == '\0' || ch == '\r' || ch == '\n'))
        .then(|| path.to_string())
    }

    fn resolve_wsl_ghostex_paths(distribution: &str) -> Result<WslGhostexPaths, String> {
        let output = run_wsl_capture(distribution, WSL_STORAGE_PATHS_SCRIPT).ok_or_else(|| {
            "Could not resolve Ghostex storage paths inside the selected WSL2 distribution."
                .to_string()
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

    fn source_runtime_wsl_path(path: &Path) -> Result<String, String> {
        let path = path.to_string_lossy();
        if path.starts_with('/') {
            return validated_wsl_path(&path)
                .ok_or_else(|| "Source path is not a valid WSL path.".to_string());
        }
        wsl_path_for_windows_path(Path::new(path.as_ref()))
    }

    fn configured_wsl_distribution() -> Result<Option<String>, String> {
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

    fn detect_initialized_wsl2_distribution() -> Option<String> {
        let candidates = initialized_wsl2_distributions();
        candidates
            .iter()
            .find(|candidate| candidate.is_default)
            .or_else(|| candidates.first())
            .map(|candidate| candidate.name.clone())
    }

    fn resolve_initialized_wsl2_distribution(requested: &str) -> Option<String> {
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

    fn install_packaged_gxserver(
        distribution: &str,
        install_root: &str,
        package: &PackagedGxserver,
    ) -> Result<(), String> {
        let mut archive = fs::File::open(&package.archive_path)
            .map_err(|_| "The packaged WSL gxserver runtime could not be read.".to_string())?;
        let script = "set -eu; install_root=\"$1\"; archive_sha256=\"$2\"; release_dir=\"$install_root/releases/windows-app-$(date +%s)-$$\"; mkdir -p \"$release_dir\"; tar -xzf - -C \"$release_dir\"; test -x \"$release_dir/bin/gxserver\"; \"$release_dir/bin/gxserver\" setup --install-root \"$install_root\" --release-dir \"$release_dir\" >/dev/null; if [ -n \"$archive_sha256\" ]; then printf '%s\\n' \"$archive_sha256\" >\"$install_root/windows-app-runtime.sha256\"; else rm -f \"$install_root/windows-app-runtime.sha256\"; fi";
        let mut child = hidden_command("wsl.exe")
            .args([
                "--distribution",
                distribution,
                "--exec",
                "sh",
                "-lc",
                script,
                "ghostex-wsl-installer",
                install_root,
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

    fn install_packaged_source_runtime(
        distribution: &str,
        install_root: &str,
        package: &PackagedSourceRuntime,
    ) -> Result<(), String> {
        let platform = packaged_source_runtime_platform();
        crate::component_store::verify_code_server_archive(
            &package.archive_path,
            &package.component_version,
            platform,
        )?;
        let mut archive = fs::File::open(&package.archive_path)
            .map_err(|_| "The packaged WSL Source runtime could not be read.".to_string())?;
        let payload_checks = crate::component_store::code_server_payload_shell_validation_script()?;
        let script = format!(
            r#"set -eu
install_root="$1"
component_version="$2"
releases_root="$install_root/releases"
release_dir="$releases_root/.install-$component_version-$$"
final_release="$releases_root/$component_version"
previous_release="$releases_root/.previous-$component_version-$$"
package_path="$install_root/package"
marker_path="$install_root/component-version"
marker_next="$install_root/.component-version.next-$$"
marker_previous="$install_root/.component-version.previous-$$"
previous_release_moved=0
new_release_installed=0
package_touched=0
marker_touched=0
had_package=0
had_marker=0
rollback_armed=1
rollback_install() {{
  status="$?"
  trap - EXIT HUP INT TERM
  if [ "$rollback_armed" = 1 ]; then
    if [ "$marker_touched" = 1 ]; then
      if [ "$had_marker" = 1 ]; then
        cp -p -- "$marker_previous" "$marker_path"
      else
        rm -f -- "$marker_path"
      fi
    fi
    if [ "$new_release_installed" = 1 ]; then
      rm -rf -- "$final_release"
    fi
    if [ "$previous_release_moved" = 1 ]; then
      mv -- "$previous_release" "$final_release"
    fi
    if [ "$package_touched" = 1 ]; then
      if [ "$had_package" = 1 ]; then
        ln -sfn -- "$previous_package_target" "$package_path"
      else
        rm -f -- "$package_path"
      fi
    fi
  fi
  rm -rf -- "$release_dir"
  rm -f -- "$marker_next" "$marker_previous"
  exit "$status"
}}
trap rollback_install EXIT HUP INT TERM
mkdir -p "$releases_root"
if [ -L "$package_path" ]; then
  had_package=1
  previous_package_target="$(readlink "$package_path")"
elif [ -e "$package_path" ]; then
  echo "Existing WSL Source package path is not a symlink." >&2
  exit 1
fi
if [ -e "$marker_path" ]; then
  test -f "$marker_path"
  cp -p -- "$marker_path" "$marker_previous"
  had_marker=1
fi
rm -rf -- "$release_dir" "$previous_release"
mkdir -p "$release_dir"
tar -xzf - -C "$release_dir"
code_server_root="$release_dir"
{payload_checks}
"$release_dir/lib/node" "$release_dir/out/node/entry.js" --version >/dev/null
if [ -e "$final_release" ] || [ -L "$final_release" ]; then
  mv -- "$final_release" "$previous_release"
  previous_release_moved=1
fi
mv -- "$release_dir" "$final_release"
new_release_installed=1
package_touched=1
ln -sfn -- "$final_release" "$package_path"
printf '%s\n' "$component_version" >"$marker_next"
marker_touched=1
mv -f -- "$marker_next" "$marker_path"
rollback_armed=0
trap - EXIT HUP INT TERM
rm -rf -- "$previous_release"
rm -f -- "$marker_previous""#
        );
        let mut child = hidden_command("wsl.exe")
            .args([
                "--distribution",
                distribution,
                "--exec",
                "sh",
                "-lc",
                &script,
                "ghostex-wsl-source-installer",
                install_root,
                &package.component_version,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "Could not start WSL to install Source.".to_string())?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Could not stream the Source runtime into WSL.".to_string())?;
        if io::copy(&mut archive, &mut stdin).is_err() {
            drop(stdin);
            let _ = child.wait();
            cleanup_partial_source_runtime_release(
                distribution,
                install_root,
                &package.component_version,
            );
            return Err("Could not stream the Source runtime into WSL.".to_string());
        }
        drop(stdin);
        let status = match child.wait() {
            Ok(status) => status,
            Err(_) => {
                cleanup_partial_source_runtime_release(
                    distribution,
                    install_root,
                    &package.component_version,
                );
                return Err("The WSL Source runtime installer did not finish.".to_string());
            }
        };
        if !status.success() {
            cleanup_partial_source_runtime_release(
                distribution,
                install_root,
                &package.component_version,
            );
            return Err(
                "The WSL Source runtime could not be installed in the selected distribution."
                    .to_string(),
            );
        }
        Ok(())
    }

    fn cleanup_partial_source_runtime_release(
        distribution: &str,
        install_root: &str,
        component_version: &str,
    ) {
        let releases_root = format!("{install_root}/releases");
        let _ = run_wsl_status(
            distribution,
            &format!(
                "releases_root={}; component_version={}; for partial in \"$releases_root/.install-$component_version-\"*; do test -e \"$partial\" || continue; rm -rf -- \"$partial\"; done",
                posix_single_quote(&releases_root),
                posix_single_quote(component_version),
            ),
        );
    }

    fn ensure_source_runtime_installed(distribution: &str) -> Result<(), String> {
        let package = resolve_packaged_source_runtime()?.ok_or_else(|| {
            "Install the VS Code IDE component before opening Source.".to_string()
        })?;
        let paths = resolve_wsl_ghostex_paths(distribution)?;
        let install_root = paths.data_path("source-runtime");
        let runtime_path = paths.data_path("source-runtime/package");
        let identity_path = paths.data_path("source-runtime/component-version");
        let payload_checks = crate::component_store::code_server_payload_shell_validation_script()?;
        let installed = run_wsl_status(
            distribution,
            &format!(
                "code_server_root={}; {payload_checks}",
                posix_single_quote(&runtime_path),
            ),
        );
        let package_matches = run_wsl_capture(
            distribution,
            &format!(
                "test -r {} && cat {}",
                posix_single_quote(&identity_path),
                posix_single_quote(&identity_path),
            ),
        )
        .is_some_and(|actual| actual.trim() == package.component_version);
        if !installed || !package_matches {
            install_packaged_source_runtime(distribution, &install_root, &package)?;
        }
        Ok(())
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

    fn packaged_source_runtime_platform() -> &'static str {
        #[cfg(target_arch = "x86_64")]
        return "linux-x64";
        #[cfg(target_arch = "aarch64")]
        return "linux-arm64";
    }

    fn verified_packaged_source_runtime(
        archive_path: PathBuf,
        component_version: String,
    ) -> Result<PackagedSourceRuntime, String> {
        crate::component_store::verify_code_server_archive(
            &archive_path,
            &component_version,
            packaged_source_runtime_platform(),
        )?;
        Ok(PackagedSourceRuntime {
            archive_path,
            component_version,
        })
    }

    fn packaged_source_runtime_version(archive_path: &Path) -> Result<String, String> {
        let archive_name = archive_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "The packaged WSL Source runtime name is invalid.".to_string())?;
        let prefix = "code-server-";
        let suffix = format!("-{}.tar.gz", packaged_source_runtime_platform());
        archive_name
            .strip_prefix(prefix)
            .and_then(|value| value.strip_suffix(&suffix))
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .ok_or_else(|| {
                format!(
                    "The packaged WSL Source runtime must use the exact canonical archive name for {}.",
                    packaged_source_runtime_platform()
                )
            })
    }

    fn bundled_source_runtime_archive(resources_dir: &Path) -> Result<Option<PathBuf>, String> {
        let wsl_dir = resources_dir.join("wsl");
        let entries = match fs::read_dir(&wsl_dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(format!(
                    "Could not inspect bundled WSL Source runtimes under {}: {error}",
                    wsl_dir.display()
                ));
            }
        };
        let suffix = format!("-{}.tar.gz", packaged_source_runtime_platform());
        let mut archives = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| {
                format!("Could not inspect a bundled WSL Source runtime: {error}")
            })?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if entry
                .file_type()
                .map(|kind| kind.is_file())
                .unwrap_or(false)
                && name.starts_with("code-server-")
                && name.ends_with(&suffix)
            {
                archives.push(entry.path());
            }
        }
        if archives.len() > 1 {
            return Err(format!(
                "The bundled WSL Source runtime contains multiple canonical {} archives.",
                packaged_source_runtime_platform()
            ));
        }
        Ok(archives.pop())
    }

    fn resolve_packaged_source_runtime() -> Result<Option<PackagedSourceRuntime>, String> {
        if let Some(path) = env::var_os("GHOSTEX_WSL_CODE_SERVER_ARCHIVE") {
            let path = PathBuf::from(path);
            if !path.is_absolute() || !path.is_file() {
                return Err(
                    "GHOSTEX_WSL_CODE_SERVER_ARCHIVE must name an existing absolute archive."
                        .to_string(),
                );
            }
            let component_version = packaged_source_runtime_version(&path)?;
            return verified_packaged_source_runtime(path, component_version).map(Some);
        }
        let executable_dir = env::current_exe()
            .map_err(|error| format!("Could not locate the Ghostex executable: {error}"))?
            .parent()
            .ok_or_else(|| "Could not locate the Ghostex executable directory.".to_string())?
            .to_path_buf();
        let resources_dir = executable_dir.join("resources");
        if let Some(archive_path) = bundled_source_runtime_archive(&resources_dir)? {
            let component_version = packaged_source_runtime_version(&archive_path)?;
            return verified_packaged_source_runtime(archive_path, component_version).map(Some);
        }
        let manifest_path = resources_dir.join("on-demand-resources.json");
        if !manifest_path.is_file() {
            return Ok(None);
        }
        let manifest = crate::component_store::OnDemandManifest::load(&manifest_path)?;
        let store = crate::component_store::ComponentStore::from_manifest(manifest)?;
        let installed = store.query_current("code-server")?;
        if !installed.installed {
            return Ok(None);
        }
        let archive_name = format!(
            "code-server-{}-{}.tar.gz",
            installed.version,
            packaged_source_runtime_platform()
        );
        verified_packaged_source_runtime(installed.path.join(archive_name), installed.version)
            .map(Some)
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

    fn run_wsl_capture(distribution: &str, script: &str) -> Option<String> {
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

    fn spawn_gxserver_owner(
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
                _ => Err(
                    "The existing WSL gxserver lifetime owner could not be validated.".to_string(),
                ),
            },
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                Err("The WSL gxserver lifetime owner exited before startup completed.".to_string())
            }
        }
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

    fn validated_t3_owner_bearer_token(value: &str) -> Option<String> {
        let token = value.trim();
        (!token.is_empty()
            && token.chars().count() <= 16 * 1024
            && !token.chars().any(char::is_control))
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
    platform::auth_token()
}

#[cfg(target_os = "windows")]
pub(crate) fn t3_runtime_launch_plan() -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    platform::t3_runtime_launch_plan()
}

#[cfg(target_os = "windows")]
pub(crate) fn t3_owner_bearer_token() -> Result<String, String> {
    platform::t3_owner_bearer_token()
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

#[cfg(target_os = "windows")]
pub(crate) fn resource_process_snapshot() -> Option<String> {
    platform::resource_process_snapshot()
}

#[cfg(target_os = "windows")]
pub(crate) fn resource_server_snapshot() -> Option<String> {
    platform::resource_server_snapshot()
}

#[cfg(target_os = "windows")]
pub(crate) fn source_code_server_command(
    project_path: &std::path::Path,
    required_node_major: u64,
    bind_address: &str,
    link_vscode_user_config: bool,
    use_vscode_insiders_user_config: bool,
) -> Result<std::process::Command, String> {
    platform::source_code_server_command(
        project_path,
        required_node_major,
        bind_address,
        link_vscode_user_config,
        use_vscode_insiders_user_config,
    )
}

#[cfg(target_os = "windows")]
pub(crate) fn source_code_server_open_file_command(
    file_path: &std::path::Path,
    required_node_major: u64,
) -> Result<std::process::Command, String> {
    platform::source_code_server_open_file_command(file_path, required_node_major)
}

#[cfg(target_os = "windows")]
pub(crate) fn wsl_path_for_windows_path(path: &std::path::Path) -> Result<String, String> {
    platform::wsl_path_for_windows_path(path)
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn auth_token() -> Option<String> {
    None
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn mark_package_update_required() {}

#[cfg(not(target_os = "windows"))]
pub(crate) fn reset() {}
