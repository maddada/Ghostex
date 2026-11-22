//! How a box session comes back after a reboot, sleep, wire cycle or any provider restart.

use serde_json::{json, Value};

use super::command::box_attach_command;
use super::create::with_claude_sign_in;
use super::session::session_agentbox;
use super::status::{claude_signed_in_for_boxes, list_boxes};
use crate::agents::{as_atuin_ignored_shell_input, wrap_restored_terminal_resume_command};

/// Whether a box that was never confirmed exists now. An unanswerable `agentbox list` counts as
/// "exists": reattaching to a missing box fails visibly, while creating a second launch for a box
/// that does exist could start over. Blocking (one `agentbox list`): only reached until the
/// poller has seen the box once.
fn box_exists_now(box_name: &str) -> bool {
    let Ok(home) = crate::accounts::launch::home() else {
        return true;
    };
    match list_boxes(&home) {
        Ok(boxes) => boxes.iter().any(|listed| listed.name == box_name),
        Err(_) => true,
    }
}

/// The resume plan of a box session: `agentbox <agent> attach <box> --inline` once the box exists,
/// and the box's create command (with the Claude sign-in when it is still needed) until then.
///
/// CDXC:AgentBox 2026-10-01 WHY: the conversation lives inside the box, so a restore must reattach to that box (agentbox starts or unpauses it and restarts the agent's session there when needed); it never runs a local `claude --resume`, which would open an unrelated conversation on this computer. A box whose create never finished (sign-in cancelled, Docker down, the pane closed or slept during the minute a box takes) does not exist to attach to, so until `agentbox list` has shown it the restore reruns the create, first prompt included, instead of an attach that can only fail.
pub(crate) fn box_resume_plan(session: &Value) -> Option<Value> {
    let agentbox = session_agentbox(session)?;
    if !agentbox.created && !box_exists_now(&agentbox.box_name) {
        if let Some(launch) = agentbox.launch_command.as_deref() {
            let signed_in =
                crate::accounts::launch::home().is_ok_and(|home| claude_signed_in_for_boxes(&home));
            let command = if agentbox.agent == "claude" && !signed_in {
                with_claude_sign_in(launch)
            } else {
                launch.to_string()
            };
            return Some(json!({
                "agentId": agentbox.agent,
                "copyCommand": command,
                "displayCommand": command,
                "primaryCommand": command,
                "startupText": as_atuin_ignored_shell_input(&command),
                "startupTextDisposition": "queueAfterTerminalReady",
            }));
        }
    }
    let command = box_attach_command(&agentbox.agent, &agentbox.box_name);
    let startup = wrap_restored_terminal_resume_command(&command, &command, None);
    Some(json!({
        "agentId": agentbox.agent,
        "copyCommand": command,
        "displayCommand": command,
        "primaryCommand": command,
        "startupText": as_atuin_ignored_shell_input(&startup),
        "startupTextDisposition": "queueAfterTerminalReady",
    }))
}
