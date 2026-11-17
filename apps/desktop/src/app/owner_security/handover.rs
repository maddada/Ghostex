//! Proving this app's ask to manage a computer whose terminal let this computer in.
//!
//! CDXC:Security 2026-11-17 DECISION at `terminal_may_approve` in server/src/security/policy.rs:
//! that computer approves the ask from its terminal only once this computer shows, through its
//! own SSH login there, the one-time secret that computer gave it with the join. The app runs
//! `computer-code --prove-handover=<ask id>` there over SSH (the secret on stdin), and that
//! computer's gxserver traces the command to its SSH server.

use std::time::Duration;

use crate::app::helpers::*;
use crate::*;

/// How long the proof may take there (an SSH sign-in, then a socket-table lookup).
const PROVE_TIMEOUT: Duration = Duration::from_secs(45);

/// Proves ask `request_id` on the machine behind `machine_id` with `secret`, through this
/// app's SSH login there.
pub(crate) fn prove_handover(
    machine_id: &str,
    request_id: &str,
    secret: &str,
) -> Result<(), String> {
    let command = gpui_remote_prove_handover_command(request_id)
        .ok_or_else(|| "That ask's id can't be sent safely.".to_string())?;
    let settings = shared_settings::shared_sidebar_settings_snapshot();
    let config = gpui_remote_machine_config_from_settings(settings.object(), machine_id)
        .ok_or_else(|| "That computer's SSH settings are missing here.".to_string())?;
    let config = gpui_remote_machine_config_with_easy_connect_forward(config)?;
    let result = gpui_run_remote_ssh_raw_with_stdin_bytes(
        &config,
        &command,
        format!("{}\n", secret.trim()).into_bytes(),
        PROVE_TIMEOUT,
    );
    let answer = serde_json::from_str::<serde_json::Value>(result.stdout.trim()).ok();
    if result.exit_code == 0
        && answer
            .as_ref()
            .and_then(|answer| answer.get("proven"))
            .and_then(serde_json::Value::as_bool)
            == Some(true)
    {
        return Ok(());
    }
    // Only a computer that sent a secret is asked, so its Ghostex knows the flag; its refusal
    // (the last line it printed) says why.
    let said = result.stderr.trim();
    Err(if said.is_empty() {
        "That computer didn't confirm this computer's SSH sign-in.".to_string()
    } else {
        said.lines().last().unwrap_or(said).to_string()
    })
}
