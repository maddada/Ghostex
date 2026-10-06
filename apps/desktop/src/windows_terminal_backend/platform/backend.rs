use super::{
    ResolvedWindowsTerminalBackend, WindowsTerminalBackendPreference, WindowsWslGhostexCliStatus,
    WindowsWslSetupPhase,
};
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Mutex, OnceLock},
};

use super::*;

#[derive(Default)]
pub(super) struct WindowsWslState {
    detection_complete: bool,
    requested_distribution: Option<String>,
    pub(super) distribution: Option<String>,
    auth_token: Option<String>,
    package_update_required: bool,
    gxserver_owner: Option<Child>,
}

pub(super) static STATE: OnceLock<Mutex<WindowsWslState>> = OnceLock::new();

pub(super) fn state() -> &'static Mutex<WindowsWslState> {
    STATE.get_or_init(|| Mutex::new(WindowsWslState::default()))
}

pub(in crate::windows_terminal_backend) fn reset() {
    if let Ok(mut state) = state().lock() {
        *state = WindowsWslState::default();
    }
}

pub(in crate::windows_terminal_backend) fn mark_package_update_required() {
    if let Ok(mut state) = state().lock() {
        state.package_update_required = true;
    }
}

pub(in crate::windows_terminal_backend) fn auth_token() -> Option<String> {
    state().lock().ok()?.auth_token.clone()
}

pub(in crate::windows_terminal_backend) fn ghostex_cli_status()
-> Result<WindowsWslGhostexCliStatus, String> {
    /*
    CDXC:PlatformSupport 2026-08-10:
    Windows terminals and gxserver live in the selected WSL2 distribution,
    so Settings must inspect that distribution's app-managed package and
    public CLI links. The Win32 process PATH/HOME describe the CEF shell,
    not the environment where `ghostex` and `gx` run. Validate the resolved
    commands against the exact installed package inode so an unrelated
    same-named command is never reported as Ghostex-owned.
    */
    let ResolvedWindowsTerminalBackend::Wsl { distribution } =
        resolve(super::current_preference())?
    else {
        return Err("This feature requires the WSL environment. Native PowerShell supports Windows agents and project terminals.".into());
    };
    let paths = resolve_wsl_ghostex_paths(&distribution)?;
    let package_dir = paths.data_path("gxserver/package");
    let script = format!(
        r#"set -eu
package_dir={}
ghostex_target="$package_dir/bin/ghostex"
ghostex_candidate="$(command -v ghostex 2>/dev/null || true)"
gx_candidate="$(command -v gx 2>/dev/null || true)"
ghostex_path=
gx_path=
gx_usable=0
gx_blocked=0
if [ -n "$ghostex_candidate" ] && [ -x "$ghostex_target" ] && [ "$ghostex_candidate" -ef "$ghostex_target" ]; then
  ghostex_path="$ghostex_candidate"
fi
if [ -n "$gx_candidate" ]; then
  gx_path="$gx_candidate"
  if [ -x "$ghostex_target" ] && [ "$gx_candidate" -ef "$ghostex_target" ]; then
    gx_usable=1
  else
    gx_blocked=1
  fi
fi
skills_root="$HOME/.agents/skills"
printf '%s\n' \
  "$HOME" \
  "$ghostex_path" \
  "$gx_path" \
  "$gx_usable" \
  "$gx_blocked" \
  "$(test -f "$skills_root/ghostex-browser-use/SKILL.md" && printf 1 || printf 0)" \
  "$(test -f "$skills_root/ghostex-embedded-browser-use/SKILL.md" && printf 1 || printf 0)" \
  "$(test -f "$skills_root/ghostex-computer-use/SKILL.md" && printf 1 || printf 0)" \
  "$(test -f "$skills_root/ghostex-cli/SKILL.md" && printf 1 || printf 0)" \
  "$(test -f "$skills_root/ghostex-agents/SKILL.md" && printf 1 || printf 0)" \
  "$(test -f "$skills_root/ghostex-manage-beads/SKILL.md" && printf 1 || printf 0)" \
  "$(test -f "$skills_root/ghostex-auto-rename-session/SKILL.md" && printf 1 || printf 0)" \
  "$(test -f "$skills_root/ghostex-move-codex-session/SKILL.md" && printf 1 || printf 0)" \
  "$(test -f "$skills_root/ghostex-help/SKILL.md" && printf 1 || printf 0)"
"#,
        posix_single_quote(&package_dir),
    );
    let output = run_wsl_capture(&distribution, &script).ok_or_else(|| {
        "Could not inspect Ghostex CLI status inside the selected WSL2 distribution.".to_string()
    })?;
    let mut lines = output.lines();
    let home = lines
        .next()
        .and_then(validated_wsl_path)
        .ok_or_else(|| "WSL returned an invalid home directory.".to_string())?;
    let ghostex_path = validated_optional_wsl_path(lines.next())?;
    let gx_path = validated_optional_wsl_path(lines.next())?;
    let gx_usable = parse_status_flag(lines.next())?;
    let gx_blocked_by_existing_command = parse_status_flag(lines.next())?;
    let skill_names = [
        "ghostex-browser-use",
        "ghostex-embedded-browser-use",
        "ghostex-computer-use",
        "ghostex-cli",
        "ghostex-agents",
        "ghostex-manage-beads",
        "ghostex-auto-rename-session",
        "ghostex-move-codex-session",
        "ghostex-help",
    ];
    let mut skill_paths = Vec::with_capacity(skill_names.len());
    for skill_name in skill_names {
        let installed = parse_status_flag(lines.next())?;
        skill_paths.push(installed.then(|| format!("{home}/.agents/skills/{skill_name}/SKILL.md")));
    }
    if lines.next().is_some() {
        return Err("WSL returned an invalid Ghostex CLI status response.".to_string());
    }
    let mut skill_paths = skill_paths.into_iter();
    Ok(WindowsWslGhostexCliStatus {
        browser_skill_path: skill_paths.next().flatten(),
        embedded_browser_skill_path: skill_paths.next().flatten(),
        computer_use_skill_path: skill_paths.next().flatten(),
        cli_skill_path: skill_paths.next().flatten(),
        agents_orchestration_skill_path: skill_paths.next().flatten(),
        manage_beads_skill_path: skill_paths.next().flatten(),
        generate_title_skill_path: skill_paths.next().flatten(),
        move_codex_session_skill_path: skill_paths.next().flatten(),
        help_skill_path: skill_paths.next().flatten(),
        ghostex_path,
        gx_blocked_by_existing_command,
        gx_path,
        gx_usable,
    })
}

pub(in crate::windows_terminal_backend) fn ghostex_cli_invocation(
    args: &[&str],
) -> Result<(PathBuf, Vec<String>), String> {
    let ResolvedWindowsTerminalBackend::Wsl { distribution } =
        resolve(super::current_preference())?
    else {
        return Err("Select a WSL distribution before installing WSL agent skills.".to_string());
    };
    let cli = ghostex_cli_status()?.ghostex_path.ok_or_else(|| {
        "The Ghostex CLI is missing from the selected WSL distribution. Repair its Ghostex runtime before installing agent skills.".to_string()
    })?;
    let mut invocation = vec![
        "--distribution".to_string(),
        distribution,
        "--exec".to_string(),
        cli,
    ];
    invocation.extend(args.iter().map(|arg| (*arg).to_string()));
    Ok((PathBuf::from("wsl.exe"), invocation))
}

fn validated_optional_wsl_path(line: Option<&str>) -> Result<Option<String>, String> {
    let Some(path) = line else {
        return Err("WSL returned an invalid Ghostex CLI status response.".to_string());
    };
    if path.is_empty() {
        return Ok(None);
    }
    validated_wsl_path(path)
        .map(Some)
        .ok_or_else(|| "WSL returned an invalid Ghostex CLI path.".to_string())
}

fn parse_status_flag(line: Option<&str>) -> Result<bool, String> {
    match line {
        Some("0") => Ok(false),
        Some("1") => Ok(true),
        _ => Err("WSL returned an invalid Ghostex CLI status response.".to_string()),
    }
}

pub(in crate::windows_terminal_backend) fn resolve(
    preference: WindowsTerminalBackendPreference,
) -> Result<ResolvedWindowsTerminalBackend, String> {
    if preference == WindowsTerminalBackendPreference::PowerShell {
        return Ok(ResolvedWindowsTerminalBackend::PowerShell);
    }
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

pub(in crate::windows_terminal_backend) fn prepare_gxserver(
    preference: WindowsTerminalBackendPreference,
    progress: &mut dyn FnMut(WindowsWslSetupPhase),
) -> Result<ResolvedWindowsTerminalBackend, String> {
    progress(WindowsWslSetupPhase::Checking);
    super::native_package::record_app_dir();
    let backend = resolve(preference)?;
    let ResolvedWindowsTerminalBackend::Wsl { distribution } = &backend else {
        super::native_package::refresh_existing()?;
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
        progress(WindowsWslSetupPhase::Installing);
        install_packaged_gxserver(distribution, &gxserver_install_root, &package)?;
    }

    if let Ok(mut state) = state().lock() {
        state.package_update_required = false;
    }

    progress(WindowsWslSetupPhase::Starting);
    let start_script = format!(
        "set -eu; gxserver={}; test -x \"$gxserver\"; \"$gxserver\" start --json >/dev/null",
        posix_single_quote(&gxserver_path),
    );
    // CDXC:ServerDaemon 2026-10-06 WHY:
    // Discarding startup stderr hid health-probe timeouts behind a generic WSL failure, while the unavailable cached token made Chat View look like an authentication problem.
    run_wsl_checked(distribution, &start_script).map_err(|error| {
        format!("gxserver could not start in WSL distribution '{distribution}': {error}")
    })?;
    progress(WindowsWslSetupPhase::Connecting);
    let token_file = paths.state_path("gxserver/auth/token");
    let token_output = run_wsl_capture_checked(
        distribution,
        &format!(
            "set -eu; token_file={}; test -f \"$token_file\"; cat \"$token_file\"",
            posix_single_quote(&token_file),
        ),
    )
    .map_err(|error| {
        format!("gxserver started in WSL, but its authentication token could not be read: {error}")
    })?;
    let token = validated_auth_token(&token_output).ok_or_else(|| {
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
    progress(WindowsWslSetupPhase::Ready);
    Ok(backend)
}

pub(in crate::windows_terminal_backend) fn terminal_invocation(
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
            super::native::invocation(command, working_directory)
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

pub(in crate::windows_terminal_backend) fn spawn_zmx_refresh(
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

pub(in crate::windows_terminal_backend) fn resource_process_snapshot() -> Option<String> {
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

pub(in crate::windows_terminal_backend) fn resource_server_snapshot() -> Option<String> {
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

pub(in crate::windows_terminal_backend) fn resource_process_cwd_snapshot(
    pids: &[u32],
) -> Option<String> {
    if pids.is_empty() {
        return None;
    }
    let ResolvedWindowsTerminalBackend::Wsl { distribution } =
        resolve(super::current_preference()).ok()?
    else {
        return None;
    };
    let pid_list = pids
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    run_wsl_capture(
        &distribution,
        &format!("test -x /usr/bin/lsof && exec /usr/bin/lsof -nP -a -d cwd -p {pid_list} -F pn"),
    )
}

pub(in crate::windows_terminal_backend) fn source_code_server_command(
    project_path: &Path,
    required_node_major: u64,
    bind_address: &str,
    link_vscode_user_config: bool,
    use_vscode_insiders_user_config: bool,
) -> Result<Command, String> {
    let ResolvedWindowsTerminalBackend::Wsl { distribution } =
        resolve(super::current_preference())?
    else {
        return Err("This feature requires the WSL environment. Native PowerShell supports Windows agents and project terminals.".into());
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

pub(in crate::windows_terminal_backend) fn source_code_server_open_file_command(
    file_path: &Path,
    line: Option<u32>,
    column: Option<u32>,
    workspace_folder: &Path,
    required_node_major: u64,
) -> Result<Command, String> {
    let ResolvedWindowsTerminalBackend::Wsl { distribution } =
        resolve(super::current_preference())?
    else {
        return Err("This feature requires the WSL environment. Native PowerShell supports Windows agents and project terminals.".into());
    };
    let wsl_file_path = source_runtime_wsl_path(file_path)?;
    let wsl_workspace_folder = source_runtime_wsl_path(workspace_folder)?;
    let paths = resolve_wsl_ghostex_paths(&distribution)?;
    let repo_root = paths.data_path("source-runtime/package");
    let user_data_dir = paths.data_path("code-server-runtime-gpui/user-data");
    let script = r#"set -eu
repo_root="$1"
user_data_dir="$2"
file_path="$3"
required_node_major="$4"
line="$5"
column="$6"
workspace_folder="$7"
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
if [ "$line" -gt 0 ]; then
    target="$file_path:$line"
    if [ "$column" -gt 0 ]; then
        target="$target:$column"
    fi
    set -- --goto "$target"
else
    set -- "$file_path"
fi
exec "$node" "$repo_root/out/node/entry.js" \
    --user-data-dir "$user_data_dir" \
    --session-socket "$session_socket" \
    --reuse-window \
    --queue-open \
    --open-request-key ghostex-source-file-open \
    --open-workspace-folder "$workspace_folder" \
    "$@"
"#;
    let required_node_major = required_node_major.to_string();
    let line = line.unwrap_or(0).to_string();
    let column = column.unwrap_or(0).to_string();
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
        line.as_str(),
        column.as_str(),
        wsl_workspace_folder.as_str(),
    ]);
    Ok(command)
}
