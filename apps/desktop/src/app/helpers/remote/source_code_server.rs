// C1 wave-1 deferred split: apps/desktop/src/app/helpers/remote.rs (~8.2k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the remote "source" code-server
// component runtime spawn/payload/launch and the remote manage-docs resource
// bridge. See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{
    env, fs,
    path::Path,
    process::{Command, Stdio},
    sync::atomic::Ordering,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};

use crate::app::helpers::*;
use crate::*;

#[cfg(target_os = "macos")]
pub(crate) fn source_code_server_spawn_remote_runtime(
    target: &SourceCodeServerRuntimeTarget,
    startup_deadline: Instant,
) -> Result<SourceCodeServerRuntimeStartOutput, String> {
    let SourceCodeServerRuntimeEndpoint::Remote {
        component_platform,
        execution_target,
        machine_config,
        ..
    } = &target.endpoint
    else {
        return Err("Remote Source runtime target is invalid.".to_string());
    };
    let store = on_demand_component_store()?
        .ok_or_else(|| "The sealed code-server component manifest is unavailable.".to_string())?;
    let installed =
        store.query_current_for_platform(SOURCE_CODE_SERVER_COMPONENT_NAME, component_platform)?;
    if !installed.installed {
        return Err("The Linux code-server component is not installed.".to_string());
    }
    source_code_server_validate_remote_linux_payload(&installed.path)?;
    source_code_server_ensure_remote_payload(machine_config, execution_target, &installed)?;

    for local_port in source_code_server_remote_candidate_ports() {
        if Instant::now() >= startup_deadline {
            break;
        }
        let askpass = gpui_remote_ssh_askpass_script(machine_config)?;
        let mut arguments = gpui_remote_ssh_client_options(machine_config.has_saved_password);
        arguments.extend([
            "-o".to_string(),
            "ExitOnForwardFailure=yes".to_string(),
            "-L".to_string(),
            format!("127.0.0.1:{local_port}:127.0.0.1:{SOURCE_CODE_SERVER_REMOTE_PORT}"),
        ]);
        arguments.extend(gpui_remote_ssh_target_arguments(machine_config)?);
        arguments.push(gpui_remote_command_for_execution_target(
            execution_target,
            source_code_server_remote_launch_command(target.project_path.as_path()).as_str(),
        ));
        let mut command = Command::new("/usr/bin/ssh");
        command
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(environment) = gpui_remote_ssh_askpass_environment(askpass.as_ref()) {
            command.envs(environment);
        }
        let started_at = Instant::now();
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(_) => continue,
        };
        let readiness = source_code_server_wait_until_responsive_at(
            local_port,
            startup_deadline.saturating_duration_since(Instant::now()),
        );
        let child_running = child.try_wait().ok().flatten().is_none();
        if readiness.is_ready() && child_running {
            return Ok(SourceCodeServerRuntimeStartOutput {
                child,
                runtime_origin: format!("http://127.0.0.1:{local_port}"),
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

#[cfg(target_os = "macos")]
pub(crate) fn source_code_server_ensure_remote_payload(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    installed: &component_store::InstalledComponent,
) -> Result<(), String> {
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

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let archive_path = env::temp_dir().join(format!(
        "ghostex-code-server-{}-{unique:x}.tar.gz",
        std::process::id()
    ));
    let archive_result = Command::new("/usr/bin/tar")
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
    let upload_command = source_code_server_remote_payload_upload_command(component_key.as_str());
    let upload = gpui_run_remote_ssh_with_stdin_file_in_execution_target(
        config,
        execution_target,
        upload_command.as_str(),
        archive_path.as_path(),
        Duration::from_secs(180),
    );
    let _ = fs::remove_file(&archive_path);
    if upload.exit_code != 0 {
        return Err("Could not upload the Linux code-server component over SSH.".to_string());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn source_code_server_remote_data_root_script() -> &'static str {
    r#"if [ -n "${GHOSTEX_HOME:-}" ] && [ "${GHOSTEX_HOME#/}" != "$GHOSTEX_HOME" ]; then ghostex_data="$GHOSTEX_HOME"; elif [ -n "${XDG_DATA_HOME:-}" ]; then ghostex_data="$XDG_DATA_HOME/ghostex"; else ghostex_data="$HOME/.local/share/ghostex"; fi
code_root="$ghostex_data/code-server""#
}

#[cfg(target_os = "macos")]
pub(crate) fn source_code_server_remote_payload_ready_command(component_key: &str) -> String {
    format!(
        "set -eu\n{}\ntest -f \"$code_root/package/.ghostex-component-key\"\ntest \"$(cat \"$code_root/package/.ghostex-component-key\")\" = {}\ntest -x \"$code_root/package/lib/node\"\ntest -f \"$code_root/package/out/node/entry.js\"\ntest -f \"$code_root/package/lib/vscode/out/server-main.js\"",
        source_code_server_remote_data_root_script(),
        gpui_shell_quote(component_key),
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn source_code_server_remote_payload_upload_command(component_key: &str) -> String {
    format!(
        "set -eu\n{}\numask 077\nreleases=\"$code_root/releases\"\nmkdir -p \"$releases\"\nstage=\"$releases/.install-$$\"\nrelease=\"$releases/{}\"\npointer_stage=\"$code_root/.package-$$\"\ncleanup() {{ rm -rf -- \"$stage\"; rm -f -- \"$pointer_stage\"; }}\ntrap cleanup EXIT HUP INT TERM\nfor abandoned in \"$releases\"/.install-* \"$code_root\"/.package-*; do\n  if [ ! -e \"$abandoned\" ] && [ ! -L \"$abandoned\" ]; then continue; fi\n  abandoned_pid=${{abandoned##*-}}\n  case \"$abandoned_pid\" in ''|*[!0-9]*) continue ;; esac\n  if [ ! -d \"/proc/$abandoned_pid\" ]; then rm -rf -- \"$abandoned\"; fi\ndone\nrm -rf -- \"$stage\"\nrm -f -- \"$pointer_stage\"\nmkdir \"$stage\"\ntar -xzf - -C \"$stage\"\ntest -x \"$stage/lib/node\"\ntest -f \"$stage/out/node/entry.js\"\ntest -f \"$stage/lib/vscode/out/server-main.js\"\nprintf '%s' {} >\"$stage/.ghostex-component-key\"\nif [ -e \"$release\" ]; then\n  test -f \"$release/.ghostex-component-key\"\n  test \"$(cat \"$release/.ghostex-component-key\")\" = {}\n  test -x \"$release/lib/node\"\n  test -f \"$release/out/node/entry.js\"\n  test -f \"$release/lib/vscode/out/server-main.js\"\n  rm -rf -- \"$stage\"\nelif ! mv -T \"$stage\" \"$release\"; then\n  test -f \"$release/.ghostex-component-key\"\n  test \"$(cat \"$release/.ghostex-component-key\")\" = {}\n  test -x \"$release/lib/node\"\n  test -f \"$release/out/node/entry.js\"\n  test -f \"$release/lib/vscode/out/server-main.js\"\n  rm -rf -- \"$stage\"\nfi\nprevious_release=$(readlink -f \"$code_root/package\" 2>/dev/null || true)\nln -s \"releases/{}\" \"$pointer_stage\"\nmv -Tf \"$pointer_stage\" \"$code_root/package\"\nfor candidate in \"$releases\"/*; do\n  [ -d \"$candidate\" ] || continue\n  if [ \"$candidate\" = \"$release\" ] || [ \"$candidate\" = \"$previous_release\" ]; then continue; fi\n  if find \"$candidate\" -maxdepth 0 -mmin +10 -print -quit | grep -q .; then rm -rf -- \"$candidate\"; fi\ndone\ntrap - EXIT HUP INT TERM",
        source_code_server_remote_data_root_script(),
        installed_key_path_fragment(component_key),
        gpui_shell_quote(component_key),
        gpui_shell_quote(component_key),
        gpui_shell_quote(component_key),
        installed_key_path_fragment(component_key),
    )
}

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
pub(crate) fn source_code_server_remote_launch_command(project_path: &Path) -> String {
    format!(
        "set -eu\n{}\npackage=\"$code_root/package\"\nruntime=\"$code_root/runtime\"\nmkdir -p \"$runtime/user-data\" \"$runtime/extensions\"\ncd {}\nchild=\ncleanup() {{ if [ -n \"$child\" ]; then kill \"$child\" 2>/dev/null || true; wait \"$child\" 2>/dev/null || true; fi; }}\ntrap cleanup EXIT HUP INT TERM\n\"$package/lib/node\" \"$package/out/node/entry.js\" --auth none --bind-addr 127.0.0.1:{} --disable-telemetry --disable-update-check --disable-workspace-trust --disable-getting-started-override --ignore-last-opened --app-name 'ghostex Code' --user-data-dir \"$runtime/user-data\" --extensions-dir \"$runtime/extensions\" &\nchild=$!\nwait \"$child\"",
        source_code_server_remote_data_root_script(),
        gpui_shell_quote(gpui_path_string(project_path).as_str()),
        SOURCE_CODE_SERVER_REMOTE_PORT,
    )
}

#[cfg(target_os = "macos")]
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
            snapshot.ok_or_else(|| "No active Docs project is available.".to_string())?;
        manage_validate_request_identity(&request, snapshot)?;
        if !matches!(
            action.as_str(),
            "list"
                | "read"
                | "stat"
                | "save"
                | "rename"
                | "delete"
                | "duplicate"
                | "createFolder"
                | "move"
                | "copyFullPath"
                | "addToSessionContext"
        ) {
            return if action == "revealInFinder" {
                Err("Reveal in Finder is unavailable for remote Docs items.".to_string())
            } else {
                Err("Unsupported Docs file action.".to_string())
            };
        }
        let target =
            target.ok_or_else(|| "Reconnect the remote machine to use Docs.".to_string())?;
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
        for key in ["path", "newPath", "content"] {
            if let Some(value) = manage_request_string(&request, key) {
                params.insert(key.to_string(), serde_json::Value::String(value));
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
            return Err("The remote Docs service returned an invalid response.".to_string());
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
