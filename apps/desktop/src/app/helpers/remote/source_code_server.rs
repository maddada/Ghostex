// C1 wave-1 deferred split: apps/desktop/src/app/helpers/remote.rs (~8.2k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the remote "source" code-server
// component runtime spawn/payload/launch and the remote manage-docs resource
// bridge. See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{
    env, fs,
    io::{Read, Write},
    path::Path,
    process::Stdio,
    sync::atomic::Ordering,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};

use crate::app::helpers::*;
use crate::*;

/// The line after the Windows Code launch script on SSH stdin; the input stays open after it.
const SOURCE_CODE_SERVER_REMOTE_WINDOWS_SCRIPT_END: &str = "#ghostex-code-launch-end";

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
/// CDXC:CodeEditor 2026-09-23 WHY:
/// Remote preflight and payload transfer have their own bounded timeouts; charging them against editor readiness could exhaust its budget before any tunnel starts. Windows sends the launch script through unused SSH stdin so long workspace paths cannot exceed OpenSSH's command-line limit.
pub(crate) fn source_code_server_spawn_remote_runtime(
    target: &SourceCodeServerRuntimeTarget,
    startup_deadline: Instant,
) -> Result<SourceCodeServerRuntimeStartOutput, String> {
    let readiness_timeout = startup_deadline.saturating_duration_since(Instant::now());
    let SourceCodeServerRuntimeEndpoint::Remote {
        component_platform,
        execution_target,
        machine_config,
        ..
    } = &target.endpoint
    else {
        return Err("Remote Source runtime target is invalid.".to_string());
    };
    let native_windows = matches!(
        execution_target,
        GpuiRemoteExecutionTarget::WindowsPowerShell
    );
    let macos_host = source_code_server_component_platform_is_macos(component_platform);
    if native_windows {
        let check = gpui_run_remote_ssh_in_execution_target(
            machine_config,
            execution_target,
            &gpui_remote_windows_code_setup(),
            Duration::from_secs(15),
        );
        if check.exit_code != 0 {
            return Err(SOURCE_CODE_SERVER_REMOTE_EDITOR_MISSING_MESSAGE.into());
        }
    } else if macos_host {
        let link = gpui_run_remote_ssh_in_execution_target(
            machine_config,
            execution_target,
            &source_code_server_remote_macos_editor_link_command(component_platform),
            Duration::from_secs(15),
        );
        if link.exit_code == SOURCE_CODE_SERVER_REMOTE_MACOS_EDITOR_MISSING_EXIT_CODE {
            return Err(SOURCE_CODE_SERVER_REMOTE_EDITOR_MISSING_MESSAGE.into());
        }
        if link.exit_code != 0 {
            return Err("Could not prepare the remote computer's Code editor over SSH.".into());
        }
    } else {
        let store = on_demand_component_store()?.ok_or_else(|| {
            "The sealed code-server component manifest is unavailable.".to_string()
        })?;
        let store_platform = source_code_server_component_store_platform(target)
            .unwrap_or_else(|| component_platform.clone());
        let installed =
            store.query_current_for_platform(SOURCE_CODE_SERVER_COMPONENT_NAME, &store_platform)?;
        if !installed.installed {
            return Err("The Linux code-server component is not installed.".to_string());
        }
        #[cfg(not(target_os = "windows"))]
        source_code_server_validate_remote_linux_payload(&installed.path)?;
        source_code_server_ensure_remote_payload(machine_config, execution_target, &installed)?;
    }

    let startup_deadline = Instant::now() + readiness_timeout;
    // Sent over the SSH input, never on the remote command line other users can list.
    let runtime_key = source_code_server_new_launch_key();
    for local_port in source_code_server_remote_candidate_ports() {
        if Instant::now() >= startup_deadline {
            break;
        }
        let askpass = gpui_remote_ssh_askpass_script(machine_config)?;
        let mut arguments = gpui_remote_ssh_tunnel_options(machine_config.has_saved_password);
        arguments.extend([
            "-o".to_string(),
            "ExitOnForwardFailure=yes".to_string(),
            "-L".to_string(),
            format!("127.0.0.1:{local_port}:127.0.0.1:{SOURCE_CODE_SERVER_REMOTE_PORT}"),
        ]);
        arguments.extend(gpui_remote_ssh_target_arguments(machine_config)?);
        arguments.push(gpui_remote_command_for_execution_target(
            execution_target,
            &if native_windows {
                format!(
                    "$gxLaunch=[Text.StringBuilder]::new(); while (($gxLine=[Console]::In.ReadLine()) -ne $null -and $gxLine -ne '{SOURCE_CODE_SERVER_REMOTE_WINDOWS_SCRIPT_END}') {{ [void]$gxLaunch.AppendLine($gxLine) }}; & ([ScriptBlock]::Create($gxLaunch.ToString()))"
                )
            } else {
                source_code_server_remote_launch_command(&target.project_path, macos_host)
            },
        ));
        let mut command = gpui_remote_background_command(&gpui_remote_ssh_executable());
        command
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(if native_windows {
                Stdio::null()
            } else {
                Stdio::piped()
            })
            .stderr(Stdio::null());
        if let Some(environment) = gpui_remote_ssh_askpass_environment(askpass.as_ref()) {
            command.envs(environment);
        }
        let started_at = Instant::now();
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(_) => continue,
        };
        if native_windows {
            let script = format!(
                "{}\n{SOURCE_CODE_SERVER_REMOTE_WINDOWS_SCRIPT_END}\n",
                gpui_remote_windows_code_launch(&target.project_path, &runtime_key)
            );
            let Some(mut stdin) = child.stdin.take() else {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Could not open the remote Code launch input.".to_string());
            };
            let (written, result) = std::sync::mpsc::sync_channel(1);
            std::thread::spawn(move || {
                let result = stdin
                    .write_all(script.as_bytes())
                    .and_then(|()| stdin.flush());
                let _ = written.send(result.map(|()| stdin));
            });
            match result.recv_timeout(startup_deadline.saturating_duration_since(Instant::now())) {
                // The open input owns the remote Code's life (gpui_remote_windows_code_launch).
                Ok(Ok(stdin)) => child.stdin = Some(stdin),
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(
                        "Could not send the remote Code launch script over SSH.".to_string()
                    );
                }
            }
        } else {
            // The launch script reads the key as its first input line; the input then stays
            // open for the editor's life (source_code_supervisor.js).
            let key_sent = child.stdin.as_mut().is_some_and(|stdin| {
                stdin
                    .write_all(format!("{runtime_key}\n").as_bytes())
                    .and_then(|()| stdin.flush())
                    .is_ok()
            });
            if !key_sent {
                let _ = child.kill();
                let _ = child.wait();
                continue;
            }
            let Some(mut stdout) = child.stdout.take() else {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Could not open the remote Code startup receipt.".to_string());
            };
            let (sent, received) = std::sync::mpsc::sync_channel(1);
            let reader = std::thread::spawn(move || {
                let mut receipt = [0_u8; b"GHOSTEX_CODE_READY\n".len()];
                let ready =
                    stdout.read_exact(&mut receipt).is_ok() && receipt == *b"GHOSTEX_CODE_READY\n";
                let _ = sent.send(ready);
            });
            let acknowledged = matches!(
                received.recv_timeout(startup_deadline.saturating_duration_since(Instant::now())),
                Ok(true)
            );
            if !acknowledged {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                continue;
            }
            let _ = reader.join();
        }
        let readiness = source_code_server_wait_until_responsive_at(
            local_port,
            startup_deadline.saturating_duration_since(Instant::now()),
        );
        let child_running = child.try_wait().ok().flatten().is_none();
        if readiness.is_ready() && child_running {
            return Ok(SourceCodeServerRuntimeStartOutput {
                child,
                runtime_origin: format!("http://127.0.0.1:{local_port}"),
                runtime_key: Some(runtime_key),
                prompt_editor_ipc_ready: readiness.prompt_editor_ipc_ready,
                started_at,
                http_runtime_ready: readiness.http_runtime_ready,
            });
        }
        let _ = child.kill();
        let _ = child.wait();
    }
    Err("Remote Source runtime did not become reachable through SSH.".to_string())
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn source_code_server_ensure_remote_payload(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    installed: &component_store::InstalledComponent,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let linux_archive = component_store::verify_installed_windows_code_server_component(
        &installed.path,
        &installed.version,
        &installed.platform,
    )?;
    #[cfg(target_os = "windows")]
    let component_key = format!(
        "{}:{}",
        linux_archive.component_version, linux_archive.platform
    );
    #[cfg(not(target_os = "windows"))]
    let component_key = format!("{}:{}", installed.version, installed.platform);
    let ready_command = source_code_server_remote_payload_ready_command(component_key.as_str());
    let ready = gpui_run_remote_ssh_in_execution_target(
        config,
        execution_target,
        ready_command.as_str(),
        Duration::from_secs(15),
    );
    if ready.exit_code == 0 {
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    let archive_path = installed.path.join(format!(
        "code-server-{}-{}.tar.gz",
        linux_archive.component_version, linux_archive.platform
    ));
    #[cfg(not(target_os = "windows"))]
    let archive_path = {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let archive_path = env::temp_dir().join(format!(
            "ghostex-code-server-{}-{unique:x}.tar.gz",
            std::process::id()
        ));
        let archive_result = gpui_remote_background_command(&gpui_remote_tar_executable())
            .args(["-czf"])
            .arg(&archive_path)
            .arg("-C")
            .arg(&installed.path)
            .arg(".")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|_| "Could not package the Linux code-server component.".to_string())?;
        if !archive_result.success() {
            let _ = fs::remove_file(&archive_path);
            return Err("Could not package the Linux code-server component.".to_string());
        }
        archive_path
    };
    let upload_command = source_code_server_remote_payload_upload_command(component_key.as_str());
    let upload = gpui_run_remote_ssh_with_stdin_file_in_execution_target(
        config,
        execution_target,
        upload_command.as_str(),
        archive_path.as_path(),
        Duration::from_secs(180),
    );
    #[cfg(not(target_os = "windows"))]
    let _ = fs::remove_file(&archive_path);
    if upload.exit_code != 0 {
        return Err("Could not upload the Linux code-server component over SSH.".to_string());
    }
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn source_code_server_remote_data_root_script() -> &'static str {
    r#"if [ -n "${GHOSTEX_HOME:-}" ] && [ "${GHOSTEX_HOME#/}" != "$GHOSTEX_HOME" ]; then ghostex_data="$GHOSTEX_HOME"; elif [ -n "${XDG_DATA_HOME:-}" ]; then ghostex_data="$XDG_DATA_HOME/ghostex"; else ghostex_data="$HOME/.local/share/ghostex"; fi
code_root="$ghostex_data/code-server""#
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn source_code_server_remote_payload_ready_command(component_key: &str) -> String {
    format!(
        "set -eu\n{}\ntest -f \"$code_root/package/.ghostex-component-key\"\ntest \"$(cat \"$code_root/package/.ghostex-component-key\")\" = {}\ntest -x \"$code_root/package/lib/node\"\ntest -f \"$code_root/package/out/node/entry.js\"\ntest -f \"$code_root/package/lib/vscode/out/server-main.js\"",
        source_code_server_remote_data_root_script(),
        gpui_shell_single_quote(component_key),
    )
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn source_code_server_remote_payload_upload_command(component_key: &str) -> String {
    format!(
        "set -eu\n{}\numask 077\nreleases=\"$code_root/releases\"\nmkdir -p \"$releases\"\nstage=\"$releases/.install-$$\"\nrelease=\"$releases/{}\"\npointer_stage=\"$code_root/.package-$$\"\ncleanup() {{ rm -rf -- \"$stage\"; rm -f -- \"$pointer_stage\"; }}\ntrap cleanup EXIT HUP INT TERM\nfor abandoned in \"$releases\"/.install-* \"$code_root\"/.package-*; do\n  if [ ! -e \"$abandoned\" ] && [ ! -L \"$abandoned\" ]; then continue; fi\n  abandoned_pid=${{abandoned##*-}}\n  case \"$abandoned_pid\" in ''|*[!0-9]*) continue ;; esac\n  if [ ! -d \"/proc/$abandoned_pid\" ]; then rm -rf -- \"$abandoned\"; fi\ndone\nrm -rf -- \"$stage\"\nrm -f -- \"$pointer_stage\"\nmkdir \"$stage\"\ntar -xzf - -C \"$stage\"\ntest -x \"$stage/lib/node\"\ntest -f \"$stage/out/node/entry.js\"\ntest -f \"$stage/lib/vscode/out/server-main.js\"\nprintf '%s' {} >\"$stage/.ghostex-component-key\"\nif [ -e \"$release\" ]; then\n  test -f \"$release/.ghostex-component-key\"\n  test \"$(cat \"$release/.ghostex-component-key\")\" = {}\n  test -x \"$release/lib/node\"\n  test -f \"$release/out/node/entry.js\"\n  test -f \"$release/lib/vscode/out/server-main.js\"\n  rm -rf -- \"$stage\"\nelif ! mv -T \"$stage\" \"$release\"; then\n  test -f \"$release/.ghostex-component-key\"\n  test \"$(cat \"$release/.ghostex-component-key\")\" = {}\n  test -x \"$release/lib/node\"\n  test -f \"$release/out/node/entry.js\"\n  test -f \"$release/lib/vscode/out/server-main.js\"\n  rm -rf -- \"$stage\"\nfi\nprevious_release=$(readlink -f \"$code_root/package\" 2>/dev/null || true)\nln -s \"releases/{}\" \"$pointer_stage\"\nmv -Tf \"$pointer_stage\" \"$code_root/package\"\nfor candidate in \"$releases\"/*; do\n  [ -d \"$candidate\" ] || continue\n  if [ \"$candidate\" = \"$release\" ] || [ \"$candidate\" = \"$previous_release\" ]; then continue; fi\n  if find \"$candidate\" -maxdepth 0 -mmin +10 -print -quit | grep -q .; then rm -rf -- \"$candidate\"; fi\ndone\ntrap - EXIT HUP INT TERM",
        source_code_server_remote_data_root_script(),
        installed_key_path_fragment(component_key),
        gpui_shell_single_quote(component_key),
        gpui_shell_single_quote(component_key),
        gpui_shell_single_quote(component_key),
        installed_key_path_fragment(component_key),
    )
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn installed_key_path_fragment(component_key: &str) -> String {
    component_key
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
/// CDXC:CodeEditor 2026-09-24 WHY:
/// A rapid remote reopen reached the previous server through the shared port while its SSH shell was still shutting down. Hold the shared runtime lock through owned child teardown, terminate that child when SSH stdin closes, and require its own bind receipt before accepting HTTP health; the shared session socket remains stable for remote prompt-editor clients.
/// CDXC:CodeEditor 2026-10-16 WHY:
/// macOS has no flock(1), so `set -eu` stopped the Mac launch before Code started. Perl, which every macOS ships, takes the same flock(2) lock on the shell's fd 9; the lock belongs to the open file and so stays held through the exec into Code, as `flock 9` does on Linux.
pub(crate) fn source_code_server_remote_launch_command(
    project_path: &Path,
    macos_host: bool,
) -> String {
    let lock = if macos_host {
        "/usr/bin/perl -MFcntl=:flock -e 'open(my $lock, \">&=\", 9) or exit 1; flock($lock, LOCK_EX) or exit 1'"
    } else {
        "flock 9"
    };
    format!(
        "set -eu\n{}\npackage=\"$code_root/package\"\nruntime=\"$code_root/runtime\"\nmkdir -p \"$runtime/user-data\" \"$runtime/extensions\"\nexec 9>\"$runtime/launch.lock\"\n{lock}\ncd {}\nIFS= read -r GHOSTEX_CODE_SERVER_KEY\nexport GHOSTEX_CODE_SERVER_KEY\nexec \"$package/lib/node\" --eval {} -- \"$package/out/node/entry.js\" --auth none --log info --bind-addr 127.0.0.1:{} --disable-telemetry --disable-update-check --disable-workspace-trust --disable-getting-started-override --ignore-last-opened --app-name 'ghostex Code' --user-data-dir \"$runtime/user-data\" --extensions-dir \"$runtime/extensions\"",
        source_code_server_remote_data_root_script(),
        gpui_shell_single_quote(gpui_path_string(project_path).as_str()),
        gpui_shell_single_quote(include_str!("source_code_supervisor.js")),
        SOURCE_CODE_SERVER_REMOTE_PORT,
    )
}

pub(crate) const SOURCE_CODE_SERVER_REMOTE_EDITOR_MISSING_MESSAGE: &str =
    "The remote Ghostex installation has no Code editor.";

pub(crate) const SOURCE_CODE_SERVER_REMOTE_MACOS_EDITOR_MISSING_EXIT_CODE: i32 = 3;

pub(crate) fn source_code_server_component_platform_is_macos(component_platform: &str) -> bool {
    component_platform.starts_with("darwin-")
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
/// CDXC:CodeEditor 2026-10-16 WHY:
/// A Mac remote runs the editor its own Ghostex has, found the way that Mac's app finds it (inside the app, the local-start build it points to, or the downloaded component), and links it as `$code_root/package` so the launch, remote file opens and gxserver's prompt editor all use one path. A Mac whose Ghostex never downloaded the editor exits with the missing code instead of starting nothing.
/// SEE-ALSO: source_code_server_repo_root_candidates in helpers/source_server/code_server.rs, code_server_prompt_editor_command in server/src/ghostex_cli/editors/prompt.rs.
pub(crate) fn source_code_server_remote_macos_editor_link_command(
    component_platform: &str,
) -> String {
    const LINK: &str = r#"ghostex_code_ok() { [ -x "$1/lib/node" ] && [ -f "$1/out/node/entry.js" ] && [ -f "$1/lib/vscode/out/server-main.js" ]; }
ghostex_code_find() {
  for ghostex_app in /Applications/Ghostex.app "$HOME/Applications/Ghostex.app"; do
    ghostex_web="$ghostex_app/Contents/Resources/Web"
    if ghostex_code_ok "$ghostex_web/code-server"; then printf '%s' "$ghostex_web/code-server"; return 0; fi
    if [ -f "$ghostex_web/local-start-code-server-root" ]; then
      ghostex_local_start=$(command cat "$ghostex_web/local-start-code-server-root")
      if [ -n "$ghostex_local_start" ] && ghostex_code_ok "$ghostex_local_start"; then printf '%s' "$ghostex_local_start"; return 0; fi
    fi
  done
  if [ -d "$ghostex_data/components/code-server" ]; then
    command find "$ghostex_data/components/code-server" -mindepth 2 -maxdepth 2 -type d -name "$platform" | command sort -r | while IFS= read -r ghostex_component; do
      if [ -f "$ghostex_component/.ghostex-component.json" ] && ghostex_code_ok "$ghostex_component"; then printf '%s' "$ghostex_component"; break; fi
    done
  fi
}
editor=$(ghostex_code_find || true)
[ -n "$editor" ] || exit "$editor_missing"
mkdir -p "$code_root"
if [ "$(command readlink "$code_root/package" 2>/dev/null || true)" != "$editor" ]; then
  command rm -f "$code_root/.package-$$"
  command ln -s "$editor" "$code_root/.package-$$"
  command mv -fh "$code_root/.package-$$" "$code_root/package"
fi"#;
    format!(
        "set -eu\n{}\nplatform={}\neditor_missing={}\n{LINK}",
        source_code_server_remote_data_root_script(),
        gpui_shell_single_quote(component_platform),
        SOURCE_CODE_SERVER_REMOTE_MACOS_EDITOR_MISSING_EXIT_CODE,
    )
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn source_code_server_remote_candidate_ports() -> Vec<u16> {
    let range =
        u64::from(SOURCE_CODE_SERVER_TUNNEL_PORT_MAX - SOURCE_CODE_SERVER_TUNNEL_PORT_MIN + 1);
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
        ^ u64::from(std::process::id());
    (0..SOURCE_CODE_SERVER_TUNNEL_ATTEMPTS)
        .map(|offset| SOURCE_CODE_SERVER_TUNNEL_PORT_MIN + ((seed + offset as u64) % range) as u16)
        .collect()
}

pub(crate) fn source_code_server_validate_remote_linux_payload(
    repo_root: &Path,
) -> Result<(), String> {
    for relative_path in [
        "lib/node",
        "out/node/entry.js",
        "lib/vscode/out/server-main.js",
    ] {
        if !repo_root.join(relative_path).is_file() {
            return Err("The Linux code-server component is incomplete.".to_string());
        }
    }
    Ok(())
}

pub(crate) fn run_remote_manage_files_bridge_request_for_project_snapshot(
    payload: &str,
    snapshot: Option<&GpuiProjectSnapshot>,
    additional_docs_folders_text: &str,
    reference: &GpuiRemoteProjectReference,
    target: Option<&GpuiRemoteGxserverRequestTarget>,
    project_scope: bool,
) -> ManageFilesBridgeOutcome {
    /*
    CDXC:Docs 2026-08-06:
    Remote Docs uses the owning gxserver as the filesystem boundary. The CEF
    request must still match the active machine-scoped project/editor snapshot,
    then Rust replaces that scoped presentation id with the daemon's raw
    project id and forwards only the fixed Docs action fields through the live
    authenticated tunnel. No remote path, host, token, or generic endpoint
    authority crosses the renderer bridge; a disconnected machine fails
    directly instead of probing the same path on the local Mac.
    */
    let request = serde_json::from_str::<serde_json::Value>(payload).unwrap_or_default();
    let action = manage_request_string(&request, "action").unwrap_or_default();
    let request_id = manage_request_string(&request, "requestId").unwrap_or_default();
    let result = (|| {
        let snapshot =
            snapshot.ok_or_else(|| "No active project is available for Files.".to_string())?;
        manage_validate_request_identity(&request, snapshot)?;
        if !matches!(
            action.as_str(),
            "list"
                | "read"
                | "gitBaseline"
                | "stat"
                | "save"
                | "rename"
                | "delete"
                | "duplicate"
                | "createFolder"
                | "move"
                | "copyFullPath"
                | "addToSessionContext"
                | "search"
                | "sqlite"
        ) {
            return if action == "openWithSystemApp" {
                Err("Files on a remote computer can't open in this computer's apps.".to_string())
            } else if action == "revealInFinder" {
                Err("Open Location is unavailable for remote files.".to_string())
            } else if action == "openInCodeView" {
                Err("Open in VS Code view is unavailable for remote files.".to_string())
            } else {
                Err("Unsupported Files action.".to_string())
            };
        }
        let target =
            target.ok_or_else(|| "Reconnect the remote machine to use Files.".to_string())?;
        let mut params = serde_json::Map::new();
        params.insert(
            "action".to_string(),
            serde_json::Value::String(action.clone()),
        );
        params.insert(
            "requestId".to_string(),
            serde_json::Value::String(request_id.clone()),
        );
        params.insert(
            "projectId".to_string(),
            serde_json::Value::String(reference.project_id.clone()),
        );
        params.insert(
            "additionalDocsFolders".to_string(),
            serde_json::Value::String(additional_docs_folders_text.to_string()),
        );
        if project_scope {
            params.insert(
                "scope".to_string(),
                serde_json::Value::String("project".to_string()),
            );
        }
        for key in [
            "path",
            "newPath",
            "content",
            "revision",
            "query",
            "table",
            "sql",
            "sessionId",
        ] {
            if let Some(value) = manage_request_string(&request, key) {
                params.insert(key.to_string(), serde_json::Value::String(value));
            }
        }
        for key in ["directoryOnly", "deferGitBaseline", "force"] {
            if let Some(value) = request.get(key).and_then(serde_json::Value::as_bool) {
                params.insert(key.to_string(), serde_json::Value::Bool(value));
            }
        }
        for key in ["offset", "limit"] {
            if let Some(value) = request.get(key).and_then(serde_json::Value::as_u64) {
                params.insert(key.to_string(), serde_json::Value::from(value));
            }
        }
        let response = gpui_remote_gxserver_rpc_result(
            target,
            "/api/runProjectDocsAction",
            &serde_json::Value::Object(params),
            Duration::from_secs(30),
        )?;
        if !response.is_object()
            || response.get("action").and_then(serde_json::Value::as_str) != Some(action.as_str())
            || response
                .get("requestId")
                .and_then(serde_json::Value::as_str)
                != Some(request_id.as_str())
        {
            return Err("The remote Files service returned an invalid response.".to_string());
        }
        Ok(response)
    })();
    manage_files_bridge_outcome(action, request_id, result)
}

pub(crate) fn read_remote_manage_docs_resource(
    target: Option<&GpuiRemoteGxserverRequestTarget>,
    project_id: &str,
    relative_path: &str,
    additional_docs_folders_text: &str,
    project_scope: bool,
) -> Option<Vec<u8>> {
    let target = target?;
    let request_id = format!(
        "docs-resource-{}",
        MANAGE_REMOTE_RESOURCE_REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let response = gpui_remote_gxserver_rpc_result(
        target,
        "/api/runProjectDocsAction",
        &serde_json::json!({
            "action": "readResource",
            "additionalDocsFolders": additional_docs_folders_text,
            "path": relative_path,
            "projectId": project_id,
            "requestId": request_id,
            "scope": if project_scope { "project" } else { "docs" },
        }),
        Duration::from_secs(30),
    )
    .ok()?;
    if !response.is_object()
        || response.get("action").and_then(serde_json::Value::as_str) != Some("readResource")
        || response
            .get("requestId")
            .and_then(serde_json::Value::as_str)
            != Some(request_id.as_str())
    {
        return None;
    }
    let encoded = response
        .get("dataBase64")
        .and_then(serde_json::Value::as_str)?;
    let data = BASE64_STANDARD.decode(encoded).ok()?;
    (data.len() <= MANAGE_REMOTE_RESOURCE_MAX_BYTES).then_some(data)
}
