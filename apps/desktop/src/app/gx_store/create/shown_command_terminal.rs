//! Run in terminal for a command Ghostex shows (the Linux update command first): gxserver makes a
//! Main Hub terminal with the command typed in and not run, on this computer or down a remote
//! machine's tunnel, and this opens it so the user only presses Enter.
//!
//! SEE-ALSO: server/src/shown_commands/ (the ids and the terminal), `ghostex shown-command open`
//! (the phone's way in), command_terminal.rs (a chat's Run and Paste in terminal).

use ghostex_gx_core::{SessionKey, open_remote_session_terminal};
use serde_json::{Value, json};

use super::super::gx_rpc;
use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    /// `machine_id` is a saved remote machine's id, or `None` for this computer; `command_id` is
    /// one gxserver advertised next to the command it shows (e.g. `linuxUpdate.runInTerminal`).
    pub(crate) fn gx_store_open_shown_command_terminal(
        &mut self,
        machine_id: Option<String>,
        command_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let remote = match machine_id.as_deref() {
            Some(machine_id) => match self.gpui_remote_gxserver_request_target(machine_id) {
                Some(target) => Some(target),
                None => {
                    self.gx_store_create_toast(
                        "warning",
                        "Could not open a terminal",
                        Some("That computer is not connected."),
                        cx,
                    );
                    return;
                }
            },
            None => None,
        };
        cx.spawn(async move |this, cx| {
            let opened = gx_rpc(
                remote,
                "/api/openShownCommandTerminal",
                json!({ "commandId": command_id }),
            )
            .await;
            let ids = opened.as_ref().ok().and_then(|opened| {
                Some((
                    opened["projectId"].as_str()?.to_string(),
                    opened["sessionId"].as_str()?.to_string(),
                ))
            });
            let _ = this.update(cx, |this, cx| {
                let Some((project_id, session_id)) = ids else {
                    let message = opened.err().map(|error| error.message);
                    this.gx_store_create_toast(
                        "warning",
                        "Could not open a terminal",
                        message.as_deref(),
                        cx,
                    );
                    return;
                };
                match machine_id {
                    Some(machine_id) => {
                        let session = SessionKey::remote(&machine_id, project_id, session_id);
                        let payload: Value = open_remote_session_terminal(
                            &session.to_sidebar_session_id(),
                            false,
                            None,
                        );
                        this.receive_sidebar_native_project_path_action_payload(
                            &payload.to_string(),
                            cx,
                        );
                    }
                    None => {
                        this.gx_store_focus_created_session(
                            &project_id,
                            &session_id,
                            false,
                            Some("terminal"),
                            cx,
                        );
                        this.show_created_terminal_tab_while_attaching(
                            &project_id,
                            &session_id,
                            cx,
                        );
                    }
                }
            });
        })
        .detach();
    }
}
