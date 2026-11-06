// C1 wave-1 deferred split: apps/desktop/src/app/helpers/remote.rs (~8.2k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds remote attach-session/plan preparation
// and remote workspace terminal creation. See
// docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{collections::HashMap, time::Duration};

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

/// The attach key `session_id` has in the remote project the Agents workarea shows, or `None` when that project is local or the session is not one of its remote tabs.
///
/// CDXC:RemoteMachines 2026-11-02 WHY:
/// Shell session ids are numbered per project, and `remote_attach_sessions` keeps the attach keys of every remote project the window has shown, so a parked project on another machine can hold the same number.
/// Matching by number alone took whichever key the map yielded first: pasting an image into a remote terminal then asked a machine that had no tunnel in this window and answered "Reconnect the remote machine" while the session's own machine was connected and its chat view pasted images fine.
/// Matching the shown project's machine and project as well gives the key the session's tab really has.
pub(crate) fn gpui_remote_attach_key_for_workarea_session(
    remote_attach_sessions: &HashMap<GpuiRemoteAttachSessionKey, TerminalSessionId>,
    workarea_project_id: Option<&str>,
    session_id: TerminalSessionId,
) -> Option<GpuiRemoteAttachSessionKey> {
    let remote_project = gpui_remote_project_reference_from_project_id(workarea_project_id?)?;
    remote_attach_sessions.iter().find_map(|(key, mapped)| {
        (*mapped == session_id
            && key.remote_machine_id == remote_project.remote_machine_id
            && key.project_id == remote_project.project_id)
            .then(|| key.clone())
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
    CDXC:RemoteMachines 2026-06-24-19:06:
    Remote attach validates session/project ownership through the Rust-owned gxserver tunnel before creating a GPUI terminal. The interactive pane runs the authoritative attach command returned by that validation over a fresh SSH connection; asking the remote CLI to resolve the same ids again would repeat its full session-inventory RPC sequence before input becomes available. Renderer text, gxserver bearer tokens, remote paths, stdout/stderr, and daemon bodies are never logged or copied to CEF.

    CDXC:RemoteMachines 2026-08-29:
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
    // A Cloud Box's session has no SSH machine: its pane attaches through the box's gxserver
    // (helpers/remote/cloud_box_attach.rs).
    if let Some(box_id) = ghostex_gx_core::protocol::cloud_box::cloud_box_id_from_machine_id(
        reference.remote_machine_id.as_str(),
    ) {
        return gpui_prepare_cloud_box_attach_terminal_plan(
            target,
            reference,
            box_id,
            wake_session,
        );
    }
    let path = if wake_session {
        "/api/wakeSession"
    } else {
        "/api/attachSessionMetadata"
    };
    let mut params = serde_json::json!({
        "projectId": reference.project_id.as_str(),
        "sessionId": reference.session_id.as_str(),
    });
    if target.capabilities.code_server_prompt_editor {
        params["promptEditor"] = serde_json::json!("code-server");
    }
    let result = gpui_remote_gxserver_rpc_result(target, path, &params, Duration::from_secs(15))?;
    gpui_remote_attach_terminal_plan_from_result(config, target, &result, interactive_attach)
}

pub(crate) fn gpui_remote_attach_terminal_plan_from_result(
    config: &GpuiRemoteMachineConfig,
    target: &GpuiRemoteGxserverRequestTarget,
    result: &serde_json::Value,
    interactive_attach: bool,
) -> Result<GpuiRemoteAttachTerminalPlan, String> {
    gpui_validate_remote_attach_metadata(result)?;
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
    let terminal_remote_command = if matches!(
        target.execution_target,
        GpuiRemoteExecutionTarget::WindowsPowerShell
    ) {
        format!(
            "[Console]::Write([string][char]27 + ']2;{TEMP_REMOTE_SSH_READY_TITLE}' + [char]7); {attach_command}"
        )
    } else {
        format!("printf '\\033]2;{TEMP_REMOTE_SSH_READY_TITLE}\\007'; {attach_command}")
    };
    let terminal_ssh_command = gpui_remote_ssh_shell_command(
        config,
        &target.execution_target,
        terminal_remote_command.as_str(),
        true,
        false,
    )?;
    let terminal_command = gpui_remote_attach_terminal_process_command(
        &terminal_ssh_command,
        config.ssh_host.as_str(),
        config.ssh_port,
    );
    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    let askpass = if interactive_attach {
        gpui_remote_ssh_askpass_script(config)?
    } else {
        None
    };
    Ok(GpuiRemoteAttachTerminalPlan {
        agent_icon,
        #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
        askpass,
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
    /*
    CDXC:RemoteMachines 2026-08-20:
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
    let plan = gpui_remote_attach_terminal_plan_from_result(config, target, &result, true)?;
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

pub(crate) fn gpui_remote_ssh_shell_command(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    remote_command: &str,
    force_tty: bool,
    interactive_login_shell: bool,
) -> Result<String, String> {
    let target_arguments = gpui_remote_ssh_target_arguments(config)?;
    let mut arguments = vec!["ssh".to_string()];
    if force_tty {
        arguments.push("-tt".to_string());
    }
    arguments.extend(gpui_remote_ssh_client_options(config.has_saved_password));
    arguments.extend(target_arguments);
    let command = if interactive_login_shell {
        gpui_remote_command_for_execution_target(execution_target, remote_command)
    } else {
        match execution_target {
            GpuiRemoteExecutionTarget::PosixHost => {
                gpui_noninteractive_login_shell_remote_command(remote_command)
            }
            GpuiRemoteExecutionTarget::WindowsWsl { .. }
            | GpuiRemoteExecutionTarget::WindowsPowerShell => {
                gpui_remote_command_for_execution_target(execution_target, remote_command)
            }
        }
    };
    arguments.push(command);
    #[cfg(windows)]
    return Ok(gpui_windows_remote_ssh_terminal_command(&arguments[1..]));
    #[cfg(not(windows))]
    Ok(arguments
        .iter()
        .map(|argument| gpui_remote_shell_command_arg(argument))
        .collect::<Vec<_>>()
        .join(" "))
}

pub(crate) fn gpui_noninteractive_login_shell_remote_command(command: &str) -> String {
    /*
    CDXC:RemoteMachines 2026-08-15:
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

#[cfg(test)]
mod workarea_session_attach_key_tests {
    use std::collections::HashMap;

    use super::*;

    fn attach_key(machine: &str, project: &str, session: &str) -> GpuiRemoteAttachSessionKey {
        GpuiRemoteAttachSessionKey {
            remote_machine_id: machine.to_string(),
            project_id: project.to_string(),
            session_id: session.to_string(),
        }
    }

    /// The window has shown a remote project on the mini and one on a second machine; each
    /// project numbered its first terminal tab 1.
    fn two_remote_projects_sharing_tab_number_one()
    -> HashMap<GpuiRemoteAttachSessionKey, TerminalSessionId> {
        HashMap::from([
            (
                attach_key("remote-mini", "P1mini", "S1"),
                TerminalSessionId(1),
            ),
            (
                attach_key("remote-oldmac", "P2old", "S7"),
                TerminalSessionId(1),
            ),
        ])
    }

    #[test]
    fn a_tab_of_the_shown_remote_project_resolves_to_its_attach_key() {
        let sessions = HashMap::from([(
            attach_key("remote-mini", "P1mini", "S1"),
            TerminalSessionId(1),
        )]);
        let shown = gpui_remote_scoped_project_id("remote-mini", "P1mini");
        assert_eq!(
            gpui_remote_attach_key_for_workarea_session(
                &sessions,
                Some(shown.as_str()),
                TerminalSessionId(1),
            ),
            Some(attach_key("remote-mini", "P1mini", "S1")),
        );
    }

    #[test]
    fn a_parked_project_with_the_same_tab_number_never_shadows_the_shown_one() {
        let sessions = two_remote_projects_sharing_tab_number_one();
        // Both directions: a lookup that ignores the shown project returns the same key for
        // both, whatever order the map yields, so one of these fails for it.
        for (machine, project, session) in [
            ("remote-mini", "P1mini", "S1"),
            ("remote-oldmac", "P2old", "S7"),
        ] {
            let shown = gpui_remote_scoped_project_id(machine, project);
            assert_eq!(
                gpui_remote_attach_key_for_workarea_session(
                    &sessions,
                    Some(shown.as_str()),
                    TerminalSessionId(1),
                ),
                Some(attach_key(machine, project, session)),
                "shown project {shown}",
            );
        }
    }

    #[test]
    fn a_disconnected_shown_machine_is_still_the_one_asked_so_the_reconnect_toast_applies() {
        // Only the parked project's machine (remote-oldmac) has a tunnel in this window. The paste
        // must ask for the mini's tunnel, find none and show the reconnect toast, never upload
        // the image to the other machine.
        let sessions = two_remote_projects_sharing_tab_number_one();
        let shown = gpui_remote_scoped_project_id("remote-mini", "P1mini");
        let machine = gpui_remote_attach_key_for_workarea_session(
            &sessions,
            Some(shown.as_str()),
            TerminalSessionId(1),
        )
        .map(|key| key.remote_machine_id);
        assert_eq!(machine.as_deref(), Some("remote-mini"));
    }

    #[test]
    fn a_local_project_or_a_tab_the_shown_project_lacks_is_not_a_remote_destination() {
        let sessions = two_remote_projects_sharing_tab_number_one();
        assert_eq!(
            gpui_remote_attach_key_for_workarea_session(
                &sessions,
                Some("P1local"),
                TerminalSessionId(1),
            ),
            None,
        );
        assert_eq!(
            gpui_remote_attach_key_for_workarea_session(&sessions, None, TerminalSessionId(1)),
            None,
        );
        let shown = gpui_remote_scoped_project_id("remote-mini", "P1mini");
        assert_eq!(
            gpui_remote_attach_key_for_workarea_session(
                &sessions,
                Some(shown.as_str()),
                TerminalSessionId(2),
            ),
            None,
        );
    }
}
