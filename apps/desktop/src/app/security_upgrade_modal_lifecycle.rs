//! Open, poll and answer the 11.0.1 security notice (window/security_upgrade_modal.rs): while it is
//! open the app reads the rows' status and the pairing code every 3 seconds, so a phone ticks off
//! as it pairs again and the code stays fresh.
//!
//! Pairing v2 (GitHub #495): the code is `/api/remotePairingCode` with `openTunnel: true`, the call
//! the Show QR sheet in Settings > Remote makes, which answers with a code only to the owner's
//! desktop session; until the desktop holds one, the notice says no code can be made and offers
//! Settings > Remote. A phone that scanned it and waits on the owner's approval is a `pending` row
//! with its three words (server/src/security_upgrade.rs, `phone_statuses_with_pending`); Approve
//! there runs the system prompt and signs (`owner_security::approve_local_request_id`).
use crate::app::gx_store::security_notice::{SECURITY_NOTICE_ENDPOINT, notice_rows};
use crate::app::gx_store::{gx_rpc, remote_last_connected_ms};
use crate::app::window::*;
use crate::*;
use serde_json::{Value, json};
use std::time::Duration;

const POLL_EVERY: Duration = Duration::from_secs(3);
const NO_CODE: &str = "Turn on Easy Connect in Settings \u{203a} Remote to pair phones.";

/// The pairing code from a `/api/remotePairingCode` answer.
pub(crate) fn security_notice_code(answer: Result<&Value, String>) -> SecurityNoticeCode {
    match answer {
        Ok(answer) => match answer
            .pointer("/easyConnect/payload")
            .and_then(Value::as_str)
            .filter(|payload| !payload.is_empty())
        {
            Some(payload) => SecurityNoticeCode::Ready(payload.to_string()),
            None => SecurityNoticeCode::Unavailable(NO_CODE.to_string()),
        },
        Err(error) => SecurityNoticeCode::Unavailable(error),
    }
}

fn security_notice_state(view: &Value, code: SecurityNoticeCode) -> SecurityUpgradeModalState {
    let (phones, computers) = notice_rows(
        view,
        &remote_last_connected_ms,
        chrono::Utc::now().timestamp_millis(),
    );
    SecurityUpgradeModalState {
        phones,
        computers,
        code,
    }
}

impl GhostexGpuiApp {
    pub(crate) fn open_gpui_security_upgrade_modal(
        &mut self,
        view: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let state = security_notice_state(view, SecurityNoticeCode::Loading);
        if state.phones.is_empty() && state.computers.is_empty() {
            return;
        }
        let palette = self.gpui_native_dialog_palette();
        let host = self.native_app_modal_host(cx, |app, command, cx| {
            app.handle_gpui_security_upgrade_command(command, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::SecurityUpgrade,
            SECURITY_UPGRADE_MODAL_WIDTH,
            SECURITY_UPGRADE_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| GpuiSecurityUpgradeModalWindow::new(palette, state, host, window, cx))
            },
            cx,
        );
        cx.spawn(async move |this, cx| {
            loop {
                let view = gx_rpc(None, SECURITY_NOTICE_ENDPOINT, json!({ "op": "read" })).await;
                let code = gx_rpc(
                    None,
                    "/api/remotePairingCode",
                    json!({ "openTunnel": true }),
                )
                .await
                .map_err(|error| error.to_string());
                let open = this
                    .update(cx, |app, cx| {
                        let kind = GpuiAppModalKind::SecurityUpgrade;
                        if app.native_app_modal_kind() != Some(kind) {
                            return false;
                        }
                        if let Ok(view) = &view {
                            let state = security_notice_state(
                                view,
                                security_notice_code(code.as_ref().map_err(Clone::clone)),
                            );
                            app.update_native_app_modal(
                                kind,
                                cx,
                                |modal: &mut GpuiSecurityUpgradeModalWindow, _window, cx| {
                                    modal.set_state(state, cx)
                                },
                            );
                        }
                        true
                    })
                    .unwrap_or(false);
                if !open {
                    break;
                }
                cx.background_executor().timer(POLL_EVERY).await;
            }
        })
        .detach();
    }

    /// Settings > Remote's Pair again: the notice again, due or not, while anything still waits.
    pub(crate) fn reopen_gpui_security_upgrade_modal(&mut self, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |this, cx| {
            if let Ok(view) = gx_rpc(None, SECURITY_NOTICE_ENDPOINT, json!({ "op": "read" })).await
            {
                let _ = this.update(cx, |app, cx| {
                    app.open_gpui_security_upgrade_modal(&view, cx)
                });
            }
        })
        .detach();
    }

    fn handle_gpui_security_upgrade_command(
        &mut self,
        command: SecurityUpgradeModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        if let SecurityUpgradeModalCommand::ApprovePending(approval_id) = &command {
            // The system prompt, then the signed approval; never on the main thread.
            let approval_id = approval_id.clone();
            let approved = cx.background_executor().spawn(async move {
                crate::app::owner_security::approve_local_request_id(&approval_id)
            });
            cx.spawn(async move |this, cx| {
                if let Err(error) = approved.await {
                    let _ = this.update(cx, |app, cx| {
                        app.dispatch_gpui_workspace_action_toast(
                            "warning",
                            "Not approved",
                            &error,
                            cx,
                        )
                    });
                }
            })
            .detach();
            return;
        }
        self.release_native_app_modal_window(GpuiAppModalKind::SecurityUpgrade, cx);
        let answer = |op: &'static str| {
            cx.spawn(async move |_, _| {
                let _ = gx_rpc(None, SECURITY_NOTICE_ENDPOINT, json!({ "op": op })).await;
            })
            .detach();
        };
        match command {
            SecurityUpgradeModalCommand::Later => answer("later"),
            SecurityUpgradeModalCommand::Done => answer("done"),
            SecurityUpgradeModalCommand::PairComputer(machine_id) => {
                self.open_gpui_settings_remote_machine(&machine_id, cx);
            }
            SecurityUpgradeModalCommand::OpenRemoteSettings => {
                self.open_gpui_settings_tab("remote", None, cx);
            }
            SecurityUpgradeModalCommand::ApprovePending(_) => {}
        }
    }

    /// Settings > Remote with that computer's edit dialog open (`initialRemoteMachineId`).
    fn open_gpui_settings_remote_machine(
        &mut self,
        machine_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let modal = GpuiAppModalKind::Settings;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let open_message = json!({
            "initialRemoteMachineId": machine_id,
            "initialTab": "remote",
            "modal": modal.modal_id(),
            "type": "open",
            "latestSidebarStateMessage": sidebar_state_message.clone(),
        });
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, None, cx);
    }
}
