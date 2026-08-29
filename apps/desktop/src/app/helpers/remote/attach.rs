// C1 wave-1 deferred split: apps/desktop/src/app/helpers/remote.rs (~8.2k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds remote attach-session/plan preparation
// and remote workspace terminal creation. See
// docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::time::{Duration, Instant};

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_remote_scoped_session_id(
    remote_machine_id: &str,
    project_id: &str,
    session_id: &str,
) -> String {
    format!("remote:{remote_machine_id}:session:{project_id}:{session_id}")
}

pub(crate) fn gpui_remote_scoped_project_id(remote_machine_id: &str, project_id: &str) -> String {
    format!("remote:{remote_machine_id}:project:{project_id}")
}

pub(crate) fn gpui_remote_project_reference_from_project_id(
    project_id: &str,
) -> Option<GpuiRemoteProjectReference> {
    let rest = project_id.strip_prefix("remote:")?;
    let (remote_machine_id, project_id) = rest.split_once(":project:")?;
    let remote_machine_id = gpui_normalize_remote_machine_id(remote_machine_id)?;
    if !gpui_remote_sidebar_project_id_allowed(project_id) {
        return None;
    }
    Some(GpuiRemoteProjectReference {
        remote_machine_id,
        project_id: project_id.to_string(),
    })
}

pub(crate) fn gpui_remote_sidebar_session_id_allowed(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        && !value.contains('/')
        && !value.contains('\\')
        && !value.contains(':')
        && !value.contains('\0')
        && !value.chars().any(char::is_control)
}

pub(crate) fn gpui_prepare_remote_attach_terminal_plan(
    config: &GpuiRemoteMachineConfig,
    target: &GpuiRemoteGxserverRequestTarget,
    reference: &GpuiRemoteAttachSessionReference,
    wake_session: bool,
    interactive_attach: bool,
) -> Result<GpuiRemoteAttachTerminalPlan, String> {
    /*
    CDXC:GPUIRemoteAttach 2026-06-24-19:06:
    Remote attach validates session/project ownership through the Rust-owned gxserver tunnel before creating a GPUI terminal. The interactive pane runs the authoritative attach command returned by that validation over a fresh SSH connection; asking the remote CLI to resolve the same ids again would repeat its full session-inventory RPC sequence before input becomes available. The human-facing copy command remains `ghostex attach`, and renderer text, gxserver bearer tokens, remote paths, stdout/stderr, and daemon bodies are never logged or copied to CEF.

    CDXC:RemoteProjectActions 2026-08-29:
    `wake_session` and `interactive_attach` answer different questions and must
    stay separate. The first picks the RPC that resolves attach metadata: wake
    for a session whose provider may be asleep, plain metadata for one whose
    provider this app just started with its own startup command — re-waking
    that one would restart the command. The second decides whether the caller
    is going to run the returned SSH command in a terminal the user types into,
    which is the only case that needs the saved-password askpass helper. A
    metadata-only plan that still spawns ssh (a remote Action launch) needs
    askpass exactly as much as a woken one, or ssh falls back to prompting for
    the password on the pane's own TTY.
    */
    let path = if wake_session {
        "/api/wakeSession"
    } else {
        "/api/attachSessionMetadata"
    };
    let attach_rpc_started = Instant::now();
    let result = gpui_remote_gxserver_rpc_result(
        target,
        path,
        &serde_json::json!({
            "projectId": reference.project_id.as_str(),
            "sessionId": reference.session_id.as_str(),
        }),
        Duration::from_secs(15),
    )?;
    support_logs::append_temporary(
        support_logs::GpuiSupportLog::TerminalFocus,
        "TEMP.remoteNewTerminal.attachRpcCompleted",
        serde_json::json!({
            "durationMs": attach_rpc_started.elapsed().as_millis() as u64,
            "operation": if wake_session { "wake" } else { "metadata" },
        }),
    );
    gpui_remote_attach_terminal_plan_from_result(
        config,
        target,
        reference,
        &result,
        interactive_attach,
    )
}

pub(crate) fn gpui_remote_attach_terminal_plan_from_result(
    config: &GpuiRemoteMachineConfig,
    target: &GpuiRemoteGxserverRequestTarget,
    reference: &GpuiRemoteAttachSessionReference,
    result: &serde_json::Value,
    interactive_attach: bool,
) -> Result<GpuiRemoteAttachTerminalPlan, String> {
    gpui_validate_remote_attach_metadata(&result)?;
    let attach = result
        .get("attach")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "Remote attach metadata is unavailable.".to_string())?;
    let attach_command = attach
        .get("attachCommand")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .ok_or_else(|| "Remote attach metadata is unavailable.".to_string())?;
    let agent_icon = gpui_workspace_attach_agent_icon(attach);
    let title = gpui_workspace_attach_title(attach);
    let clipboard_command =
        gpui_remote_ghostex_attach_ssh_command(config, &target.execution_target, reference);
    let terminal_remote_command =
        format!("printf '\\033]2;{TEMP_REMOTE_SSH_READY_TITLE}\\007'; {attach_command}");
    let terminal_ssh_command = gpui_remote_ssh_shell_command(
        config,
        &target.execution_target,
        terminal_remote_command.as_str(),
        true,
        false,
    );
    let terminal_command = gpui_remote_attach_terminal_process_command(
        &terminal_ssh_command,
        config.ssh_host.as_str(),
        config.ssh_port,
    );
    #[cfg(target_os = "macos")]
    let askpass = if interactive_attach {
        gpui_remote_ssh_askpass_script(config)?
    } else {
        None
    };
    Ok(GpuiRemoteAttachTerminalPlan {
        agent_icon,
        #[cfg(target_os = "macos")]
        askpass,
        clipboard_command,
        terminal_command,
        title,
    })
}

pub(crate) fn gpui_create_remote_project_workspace_terminal(
    config: &GpuiRemoteMachineConfig,
    target: &GpuiRemoteGxserverRequestTarget,
    project: &GpuiRemoteProjectReference,
) -> Result<
    (
        GpuiRemoteAttachSessionReference,
        GpuiRemoteAttachTerminalPlan,
    ),
    String,
> {
    let create_started = Instant::now();
    /*
    CDXC:GPUIRemoteNewTerminal 2026-08-20:
    Ctrl+G in a remote pane can only reach that machine's code-server prompt
    editor when the daemon serving the session understands the selector. The
    remote runs the gxserver package this app installed for it, which can predate
    the mode, and such a daemon rejects the whole create request instead of
    ignoring an unknown editor. Ask for it only from a daemon that advertised
    `codeServerPromptEditor`; every other machine creates the terminal with its
    own shell editor, exactly like the remote attach and wake paths already do.
    */
    let mut params = serde_json::Map::new();
    params.insert(
        "projectId".to_string(),
        serde_json::Value::String(project.project_id.clone()),
    );
    if target.capabilities.code_server_prompt_editor {
        params.insert(
            "promptEditor".to_string(),
            serde_json::Value::String("code-server".to_string()),
        );
    }
    let (status_code, body) = gpui_remote_gxserver_post_typed_operation(
        target,
        "/api/createWorkspaceTerminal",
        &serde_json::Value::Object(params),
        Duration::from_secs(30),
    )?;
    if !(200..300).contains(&status_code) {
        /*
        The remote daemon is the gxserver package this app installed for that
        machine, so a rejected create is a client/daemon mismatch rather than a
        bad click. Say which side has to move, without forwarding daemon
        response text, remote paths, or request details into the toast.
        */
        return Err(if (400..500).contains(&status_code) {
            "The remote machine's Ghostex daemon refused to create the terminal. Reconnect that machine to update its gxserver, then try again.".to_string()
        } else {
            "Remote gxserver request failed.".to_string()
        });
    }
    let result = parse_gpui_gxserver_rpc_result(&body)?;
    support_logs::append_temporary(
        support_logs::GpuiSupportLog::TerminalFocus,
        "TEMP.remoteNewTerminal.createRpcCompleted",
        serde_json::json!({
            "durationMs": create_started.elapsed().as_millis() as u64,
        }),
    );
    let session = result
        .get("session")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "Remote gxserver did not create a terminal session.".to_string())?;
    let project_id = gpui_trimmed_json_string_field(session, "projectId")
        .ok_or_else(|| "Remote gxserver did not return a terminal project id.".to_string())?
        .to_string();
    let session_id = gpui_trimmed_json_string_field(session, "sessionId")
        .ok_or_else(|| "Remote gxserver did not return a terminal session id.".to_string())?
        .to_string();
    if project_id != project.project_id
        || !gpui_remote_sidebar_project_id_allowed(project_id.as_str())
        || !gpui_remote_sidebar_session_id_allowed(session_id.as_str())
    {
        return Err("Remote gxserver returned an invalid terminal session.".to_string());
    }
    let reference = GpuiRemoteAttachSessionReference {
        remote_machine_id: project.remote_machine_id.clone(),
        project_id,
        session_id,
    };
    let plan_started = Instant::now();
    let plan =
        gpui_remote_attach_terminal_plan_from_result(config, target, &reference, &result, true)?;
    support_logs::append_temporary(
        support_logs::GpuiSupportLog::TerminalFocus,
        "TEMP.remoteNewTerminal.planCompleted",
        serde_json::json!({
            "durationMs": plan_started.elapsed().as_millis() as u64,
            "totalDurationMs": create_started.elapsed().as_millis() as u64,
        }),
    );
    Ok((reference, plan))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiRemoteAttachOpenIntent {
    AttachExistingSession,
    CreatedByThisAction,
}

pub(crate) fn gpui_validate_remote_attach_metadata(
    result: &serde_json::Value,
) -> Result<(), String> {
    let attach = result
        .get("attach")
        .ok_or_else(|| "Remote attach metadata is unavailable.".to_string())?;
    if attach.get("restoreBlocked").is_some() {
        return Err("Remote session restore is blocked on the remote machine.".to_string());
    }
    let attach_command = attach
        .get("attachCommand")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if attach_command.is_empty() {
        return Err("Remote attach metadata is unavailable.".to_string());
    }
    Ok(())
}

pub(crate) fn gpui_prepare_remote_resume_clipboard_command(
    config: &GpuiRemoteMachineConfig,
    target: &GpuiRemoteGxserverRequestTarget,
    reference: &GpuiRemoteAttachSessionReference,
) -> Result<String, String> {
    /*
    CDXC:GPUIRemoteAttach 2026-06-24-19:06:
    Copy Remote Resume asks the owning remote gxserver for its agent resume plan and wraps only the returned copy command in a saved-machine SSH command. The renderer supplies no command text or cwd, and Rust must not log the plan, cwd, SSH target, stdout/stderr, token, or daemon body.
    */
    let result = gpui_remote_gxserver_rpc_result(
        target,
        "/api/readAgentResumePlan",
        &serde_json::json!({
            "projectId": reference.project_id.as_str(),
            "sessionId": reference.session_id.as_str(),
        }),
        Duration::from_secs(15),
    )?;
    let resume_command = result
        .get("plan")
        .and_then(|plan| plan.get("copyCommand"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .ok_or_else(|| "No resume command is available for that remote session.".to_string())?;
    let cwd = result
        .get("session")
        .and_then(|session| session.get("cwd"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|cwd| !cwd.is_empty());
    let remote_command = cwd
        .map(|cwd| format!("cd {} && {resume_command}", gpui_shell_single_quote(cwd)))
        .unwrap_or_else(|| resume_command.to_string());
    Ok(gpui_remote_ssh_shell_command(
        config,
        &target.execution_target,
        remote_command.as_str(),
        false,
        true,
    ))
}

pub(crate) fn gpui_remote_ghostex_attach_ssh_command(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    reference: &GpuiRemoteAttachSessionReference,
) -> String {
    gpui_remote_ssh_shell_command(
        config,
        execution_target,
        gpui_remote_ghostex_attach_command(reference).as_str(),
        true,
        true,
    )
}

pub(crate) fn gpui_remote_ssh_shell_command(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    remote_command: &str,
    force_tty: bool,
    interactive_login_shell: bool,
) -> String {
    let mut arguments = vec!["ssh".to_string()];
    if force_tty {
        arguments.push("-tt".to_string());
    }
    arguments.extend(gpui_remote_ssh_client_options(config.has_saved_password));
    arguments.extend(gpui_remote_ssh_target_arguments(config));
    let command = if interactive_login_shell {
        gpui_remote_command_for_execution_target(execution_target, remote_command)
    } else {
        match execution_target {
            GpuiRemoteExecutionTarget::PosixHost => {
                gpui_noninteractive_login_shell_remote_command(remote_command)
            }
            GpuiRemoteExecutionTarget::WindowsWsl { .. } => {
                gpui_remote_command_for_execution_target(execution_target, remote_command)
            }
        }
    };
    arguments.push(command);
    arguments
        .iter()
        .map(|argument| gpui_remote_shell_command_arg(argument))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn gpui_noninteractive_login_shell_remote_command(command: &str) -> String {
    /*
    CDXC:GPUIRemoteAttach 2026-08-15:
    gxserver returns an authoritative attach program with absolute daemon and
    zmx paths. Starting that program must load login PATH state, but it must not
    source the user's interactive zsh configuration before zmx owns the PTY.
    On macOS, `zsh -lic` can spend hundreds of milliseconds initializing prompt
    plugins and completions for a shell that never becomes the terminal shell.
    */
    let quoted_command = gpui_shell_single_quote(command);
    format!(
        "if [ -x /bin/zsh ]; then exec /bin/zsh -lc {quoted_command}; elif command -v zsh >/dev/null 2>&1; then exec zsh -lc {quoted_command}; else exec /bin/sh -lc {quoted_command}; fi"
    )
}

pub(crate) fn gpui_remote_ghostex_attach_command(
    reference: &GpuiRemoteAttachSessionReference,
) -> String {
    let mut parts = vec![
        "attach".to_string(),
        "--session-id".to_string(),
        gpui_shell_single_quote(reference.session_id.as_str()),
        "--project-id".to_string(),
        gpui_shell_single_quote(reference.project_id.as_str()),
    ];
    /*
    Remote Ctrl+G owns a file on the authoritative remote session. Advertise
    the fixed code-server capability so the remote CLI uses h2fe's remote Code
    runtime IPC; never advertise the Mac-only GhostexEditor capability across
    SSH and never let a remote path fall through to a local editor.
    */
    parts.extend(["--prompt-editor".to_string(), "code-server".to_string()]);
    [
        "case \"${GHOSTEX_HOME:-}\" in /*) ghostex_data_dir=\"$GHOSTEX_HOME\";; *) case \"${XDG_DATA_HOME:-}\" in /*) ghostex_data_dir=\"${XDG_DATA_HOME%/}/ghostex\";; *) ghostex_data_dir=\"$HOME/.local/share/ghostex\";; esac;; esac".to_string(),
        "remote_ghostex=\"$ghostex_data_dir/gxserver/package/bin/ghostex\"".to_string(),
        "if [ ! -x \"$remote_ghostex\" ] && [ -x \"$HOME/.ghostex/gxserver/package/bin/ghostex\" ]; then remote_ghostex=\"$HOME/.ghostex/gxserver/package/bin/ghostex\"; fi".to_string(),
        "if [ ! -x \"$remote_ghostex\" ]; then if [ -x \"$HOME/.local/bin/ghostex\" ]; then remote_ghostex=\"$HOME/.local/bin/ghostex\"; else remote_ghostex=\"ghostex\"; fi; fi".to_string(),
        format!("\"$remote_ghostex\" {}", parts.join(" ")),
    ]
    .join("; ")
}
