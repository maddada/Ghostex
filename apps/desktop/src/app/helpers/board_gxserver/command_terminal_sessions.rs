// C1 wave-1 deferred split: apps/desktop/src/app/helpers/board_gxserver.rs
// (~4.3k lines) further divided into responsibility-scoped submodules (pure
// move, no logic changes). This file holds command-pane (Commands surface) gxserver session
// create/close/rename/surface-transfer and attach-plan helpers, plus the
// URI-encoding and random UUID utilities they depend on.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{fs, io::Read, thread, time::Duration};

#[cfg(target_os = "windows")]
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
};

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_create_command_terminal_gxserver_session(
    input: &GpuiCommandTerminalCreateInput,
) -> Result<GpuiLocalWorkspaceSessionKey, String> {
    let result = gpui_gxserver_rpc_result(
        "/api/createSession",
        &gpui_command_terminal_create_session_params(input),
        Duration::from_secs(15),
    )?;
    let session = result
        .get("session")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "gxserver did not create a command terminal session.".to_string())?;
    let project_id = gpui_trimmed_json_string_field(session, "projectId")
        .ok_or_else(|| "gxserver did not return a command terminal project id.".to_string())?
        .to_string();
    let session_id = gpui_trimmed_json_string_field(session, "sessionId")
        .ok_or_else(|| "gxserver did not return a command terminal session id.".to_string())?
        .to_string();
    if !gpui_remote_sidebar_project_id_allowed(project_id.as_str())
        || !gpui_remote_sidebar_session_id_allowed(session_id.as_str())
    {
        return Err("gxserver returned an invalid command terminal session id.".to_string());
    }
    Ok(GpuiLocalWorkspaceSessionKey {
        project_id,
        session_id,
    })
}

pub(crate) fn gpui_close_command_terminal_gxserver_session(key: &GpuiLocalWorkspaceSessionKey) -> bool {
    let _ = gpui_gxserver_rpc_result(
        "/api/transitionSession",
        &serde_json::json!({
            "action": "close",
            "projectId": key.project_id.as_str(),
            "reason": "closeTerminal",
            "sessionId": key.session_id.as_str(),
        }),
        Duration::from_secs(30),
    );
    if gpui_gxserver_rpc_result(
        "/api/removeSession",
        &serde_json::json!({
            "projectId": key.project_id.as_str(),
            "reason": "closeTerminal",
            "sessionId": key.session_id.as_str(),
        }),
        Duration::from_secs(10),
    )
    .is_ok()
    {
        return true;
    }
    gpui_gxserver_rpc_result(
        "/api/listSessions",
        &serde_json::json!({ "projectId": key.project_id.as_str() }),
        Duration::from_secs(10),
    )
    .ok()
    .and_then(|result| {
        result
            .get("sessions")
            .and_then(serde_json::Value::as_array)
            .cloned()
    })
    .is_some_and(|sessions| {
        !sessions.iter().any(|session| {
            session.get("projectId").and_then(serde_json::Value::as_str)
                == Some(key.project_id.as_str())
                && session.get("sessionId").and_then(serde_json::Value::as_str)
                    == Some(key.session_id.as_str())
        })
    })
}

pub(crate) fn gpui_update_command_terminal_gxserver_session_title(
    key: &GpuiLocalWorkspaceSessionKey,
    title: &str,
) -> Result<(), String> {
    /*
    CDXC:GPUICommandPaneGxserverRestore 2026-07-04:
    Command-pane renames update the gxserver session row directly so restart
    and sidebar projections use the same title as the local tab. Send only the
    validated title plus gxserver ids; no terminal input, command text, cwd,
    attach command, or renderer payload is part of this update.
    */
    let title = title.trim();
    if title.is_empty()
        || title.chars().count() > GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        || title.contains('\0')
        || title.chars().any(char::is_control)
    {
        return Err("The command terminal title is invalid.".to_string());
    }
    let result = gpui_gxserver_rpc_result(
        "/api/updateSession",
        &serde_json::json!({
            "projectId": key.project_id.as_str(),
            "sessionId": key.session_id.as_str(),
            "title": title,
        }),
        Duration::from_secs(10),
    )?;
    if result
        .get("session")
        .and_then(serde_json::Value::as_object)
        .is_some()
    {
        Ok(())
    } else {
        Err("gxserver returned an invalid command terminal rename result.".to_string())
    }
}

pub(crate) fn gpui_update_command_terminal_gxserver_session_surface(
    key: &GpuiLocalWorkspaceSessionKey,
    surface: &str,
) -> Result<(), String> {
    /*
    CDXC:GPUICommandWorkspaceTransfer 2026-08-01:
    A command tab dragged into the Agents workspace keeps its live daemon
    session, so the gxserver row has to change surfaces with it. This is not
    cosmetic: the sidebar only projects `workspace` sessions, and an Agents tab
    missing from that projection is reconciled away along with its shell
    session. Send only the fixed surface enum plus gxserver ids; no terminal
    input, command text, cwd, attach command, or renderer payload.
    */
    if surface != "workspace" && surface != "commands" {
        return Err("The command terminal surface is invalid.".to_string());
    }
    let result = gpui_gxserver_rpc_result(
        "/api/updateSession",
        &serde_json::json!({
            "projectId": key.project_id.as_str(),
            "sessionId": key.session_id.as_str(),
            "surface": surface,
        }),
        Duration::from_secs(10),
    )?;
    if result
        .get("session")
        .and_then(serde_json::Value::as_object)
        .is_some()
    {
        Ok(())
    } else {
        Err("gxserver returned an invalid command terminal surface update result.".to_string())
    }
}

pub(crate) fn gpui_prepare_command_terminal_attach_plan(
    input: GpuiCommandTerminalCreateInput,
) -> Result<GpuiCommandTerminalAttachPlan, String> {
    let reusable_key = gpui_reusable_command_terminal_gxserver_session_key(&input)?;
    let (key, created) = match reusable_key {
        Some(key) => (key, false),
        None => (gpui_create_command_terminal_gxserver_session(&input)?, true),
    };
    match gpui_prepare_command_terminal_attach_plan_for_key(
        key.clone(),
        input.startup_text.as_deref(),
        None,
    ) {
        Ok(plan) => Ok(plan),
        Err(message) => {
            if created {
                gpui_close_command_terminal_gxserver_session(&key);
            }
            Err(message)
        }
    }
}

pub(crate) fn gpui_reusable_command_terminal_gxserver_session_key(
    input: &GpuiCommandTerminalCreateInput,
) -> Result<Option<GpuiLocalWorkspaceSessionKey>, String> {
    let Some(command_id) = input.command_id.as_deref() else {
        return Ok(None);
    };
    /*
    CDXC:GPUICommandPaneActions 2026-08-09:
    Command-pane layout is client state, but the zmx-backed command session is
    daemon state and survives a GPUI rebuild/relaunch. When the local tab was
    not restored, reclaim the live command-surface session with the same stable
    Action id instead of creating a second daemon session and losing the old
    pane's terminal history.
    */
    let result = gpui_gxserver_rpc_result(
        "/api/readPresentationSnapshot",
        &serde_json::json!({}),
        Duration::from_secs(15),
    )?;
    let sessions = result
        .get("snapshot")
        .and_then(|snapshot| snapshot.get("sessions"))
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "gxserver did not return command terminal sessions.".to_string())?;
    let session = sessions.iter().find_map(|session| {
        let object = session.as_object()?;
        let same_action = gpui_trimmed_json_string_field(object, "projectId")
            == Some(input.project_id.as_str())
            && gpui_trimmed_json_string_field(object, "commandId") == Some(command_id)
            && gpui_trimmed_json_string_field(object, "kind") == Some("terminal")
            && gpui_trimmed_json_string_field(object, "surface") == Some("commands");
        if !same_action {
            return None;
        }
        let lifecycle = gpui_trimmed_json_string_field(object, "lifecycleState")?;
        if !matches!(lifecycle, "running" | "sleeping") {
            return None;
        }
        let session_id = gpui_trimmed_json_string_field(object, "sessionId")?;
        gpui_remote_sidebar_session_id_allowed(session_id).then(|| GpuiLocalWorkspaceSessionKey {
            project_id: input.project_id.clone(),
            session_id: session_id.to_string(),
        })
    });
    Ok(session)
}

pub(crate) fn gpui_prepare_existing_command_terminal_attach_plan(
    key: GpuiLocalWorkspaceSessionKey,
    initial_input: Option<String>,
) -> Result<GpuiCommandTerminalAttachPlan, String> {
    /*
    CDXC:GPUICommandPaneGxserverRestore 2026-08-13:
    A local rebuild restarts gxserver before opening the replacement app. Shell
    restoration can reach this worker before the daemon has rebound its port or
    republished its token; that is startup readiness, not evidence that the
    persisted command session is gone. Keep the restored tab while retrying only
    those transport-readiness failures, then let authoritative attach metadata
    decide whether the session is valid.
    */
    const STARTUP_ATTEMPTS: usize = 61;
    const STARTUP_RETRY_DELAY: Duration = Duration::from_millis(250);

    let mut last_error = None;
    for attempt in 0..STARTUP_ATTEMPTS {
        match gpui_prepare_command_terminal_attach_plan_for_key(
            key.clone(),
            None,
            initial_input.clone(),
        ) {
            Ok(plan) => return Ok(plan),
            Err(message)
                if attempt + 1 < STARTUP_ATTEMPTS
                    && gpui_command_terminal_attach_startup_not_ready(&message) =>
            {
                last_error = Some(message);
                thread::sleep(STARTUP_RETRY_DELAY);
            }
            Err(message) => return Err(message),
        }
    }
    Err(last_error.unwrap_or_else(|| "gxserver is not ready.".to_string()))
}

pub(crate) fn gpui_command_terminal_attach_startup_not_ready(message: &str) -> bool {
    matches!(
        message,
        "gxserver auth token is unavailable."
            | "gxserver auth token is empty."
            | "gxserver is not reachable on 127.0.0.1:58744."
            | "Could not send gxserver request."
            | "Could not read gxserver response."
    )
}

pub(crate) fn gpui_prepare_command_terminal_attach_plan_for_key(
    key: GpuiLocalWorkspaceSessionKey,
    startup_text: Option<&str>,
    initial_input: Option<String>,
) -> Result<GpuiCommandTerminalAttachPlan, String> {
    let plan = gpui_prepare_local_workspace_attach_terminal_plan_with_startup_text(
        &key,
        startup_text,
        GpuiLocalWorkspaceAttachIntent::Wake,
    )?;
    Ok(GpuiCommandTerminalAttachPlan {
        attach_command: plan.attach_command,
        command_id: plan.command_id,
        initial_input,
        key,
        title: plan.title,
        working_directory: plan.working_directory,
        zmx_name: plan.zmx_name,
    })
}

pub(crate) fn gpui_encode_uri_component(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'~'
            | b'!'
            | b'*'
            | b'\''
            | b'('
            | b')' => encoded.push(byte as char),
            _ => {
                encoded.push('%');
                encoded.push(HEX[(byte >> 4) as usize] as char);
                encoded.push(HEX[(byte & 0x0f) as usize] as char);
            }
        }
    }
    encoded
}

pub(crate) fn gpui_random_uuid_string() -> Result<String, String> {
    let mut bytes = [0_u8; 16];
    #[cfg(target_os = "windows")]
    {
        let status = unsafe {
            BCryptGenRandom(
                std::ptr::null_mut(),
                bytes.as_mut_ptr(),
                bytes.len() as u32,
                BCRYPT_USE_SYSTEM_PREFERRED_RNG,
            )
        };
        if status != 0 {
            return Err("Could not generate a project identity.".to_string());
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let mut file = fs::File::open("/dev/urandom")
            .map_err(|_| "Could not generate a project identity.".to_string())?;
        file.read_exact(&mut bytes)
            .map_err(|_| "Could not generate a project identity.".to_string())?;
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(format!(
        "{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    ))
}

