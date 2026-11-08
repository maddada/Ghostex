//! Open, poll and answer the pair-again notice (window/security_upgrade_modal.rs): while it is open
//! the app reads the rows' status every 3 seconds, so a device ticks off as it pairs again, and,
//! while an Easy Connect phone's page is open, the pairing code too. It also runs Deny and an Easy
//! Connect computer's pairing (`/api/pairComputer`, then the machine's new code saved).
//!
//! Pairing v2 (GitHub #495): the code is `/api/remotePairingCode` with `openTunnel: true`, the call
//! the Show QR sheet in Settings > Remote makes, which answers with a code only to the owner's
//! desktop session; until the desktop holds one, the notice says no code can be made and offers
//! Settings > Remote. A phone that scanned it and waits on the owner's approval is a `pending` row
//! with its three words (server/src/security_upgrade.rs, `phone_statuses_with_pending`); Approve
//! there runs the system prompt and signs (`owner_security::approve_local_request_id`).
use crate::app::gx_store::security_notice::{SECURITY_NOTICE_ENDPOINT, notice_rows, ssh_rows};
use crate::app::gx_store::{gx_rpc, remote_last_connected_ms};
use crate::app::window::*;
use crate::*;
use serde_json::{Value, json};
use std::time::Duration;

const POLL_EVERY: Duration = Duration::from_secs(3);
/// How soon a click on Show code is noticed between rounds.
const SHOW_CODE_TICK: Duration = Duration::from_millis(200);
const COMPUTER_PAIRING_POLL: Duration = Duration::from_millis(1500);
/// `/api/pairComputer` opens a tunnel and waits for the other computer's answer.
const COMPUTER_PAIRING_START_TIMEOUT: Duration = Duration::from_secs(60);
/// Approving can wait on the other computer's Windows administrator prompt.
const COMPUTER_PAIRING_STATUS_TIMEOUT: Duration = Duration::from_secs(190);
const NO_CODE: &str = "Turn on Easy Connect in Settings \u{203a} Remote to pair phones.";

/// The pairing code from a `/api/remotePairingCode` answer, and its text for Copy Easy Connect code
/// (the bare `ghostex-ec1:` code, with its one-time secret since the QR payload is there).
pub(crate) fn security_notice_code(
    answer: Result<&Value, String>,
) -> (SecurityNoticeCode, Option<String>) {
    match answer {
        Ok(answer) => match answer
            .pointer("/easyConnect/payload")
            .and_then(Value::as_str)
            .filter(|payload| !payload.is_empty())
        {
            Some(payload) => (
                SecurityNoticeCode::Ready(payload.to_string()),
                answer
                    .pointer("/easyConnect/text")
                    .and_then(Value::as_str)
                    .filter(|text| !text.is_empty())
                    .map(str::to_string),
            ),
            None => (SecurityNoticeCode::Unavailable(NO_CODE.to_string()), None),
        },
        Err(error) => (SecurityNoticeCode::Unavailable(error), None),
    }
}

fn security_notice_state(
    view: &Value,
    (code, code_text): (SecurityNoticeCode, Option<String>),
    computer_pairing: Vec<(String, NoticeComputerPairing)>,
) -> SecurityUpgradeModalState {
    let (phones, computers) = notice_rows(
        view,
        &remote_last_connected_ms,
        chrono::Utc::now().timestamp_millis(),
    );
    let (ssh_phones, ssh_computers) = ssh_rows(view);
    SecurityUpgradeModalState {
        phones,
        computers,
        ssh_phones,
        ssh_computers,
        code,
        code_text,
        computer_pairing,
    }
}

impl GhostexGpuiApp {
    pub(crate) fn open_gpui_security_upgrade_modal(
        &mut self,
        view: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        self.security_notice_computer_pairing.clear();
        let state = security_notice_state(view, (SecurityNoticeCode::Hidden, None), Vec::new());
        // Computers over SSH need nothing, so they never open the notice by themselves.
        if state.phones.is_empty() && state.computers.is_empty() && state.ssh_phones.is_empty() {
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
        /*
        CDXC:RemotePairing 2026-11-08 FIX:
        User (GitHub #495, 11.1.1): the notice "no longer asks for a live pairing code (and so never
        opens Easy Connect to new devices) until the user clicks to pair something". It used to ask
        `/api/remotePairingCode` with `openTunnel` every 3 seconds from the moment it opened, which
        put a live one-time code on screen and held Easy Connect open to new devices for as long as
        the notice stayed up, against Settings > Remote's rule that the real QR shows only after a
        click. It now opens with the code hidden behind Show code; the poll reads the rows alone
        until that click (`security_notice_code_shown`), and asks for the code from then on.
        */
        self.security_notice_code_shown = false;
        cx.spawn(async move |this, cx| {
            loop {
                let view = gx_rpc(None, SECURITY_NOTICE_ENDPOINT, json!({ "op": "read" })).await;
                let shown = this
                    .update(cx, |app, _| app.security_notice_code_shown)
                    .unwrap_or(false);
                let code = if shown {
                    let answer = gx_rpc(
                        None,
                        "/api/remotePairingCode",
                        json!({ "openTunnel": true }),
                    )
                    .await
                    .map_err(|error| error.to_string());
                    security_notice_code(answer.as_ref().map_err(Clone::clone))
                } else {
                    (SecurityNoticeCode::Hidden, None)
                };
                let open = this
                    .update(cx, |app, cx| {
                        let kind = GpuiAppModalKind::SecurityUpgrade;
                        if app.native_app_modal_kind() != Some(kind) {
                            return false;
                        }
                        // Show code was clicked while this round was out: keep its Loading.
                        if !shown && app.security_notice_code_shown {
                            return true;
                        }
                        if let Ok(view) = &view {
                            let state = security_notice_state(
                                view,
                                code,
                                app.security_notice_computer_pairing.clone(),
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
                // Right after Show code the next round asks at once; otherwise every 3 seconds.
                let mut waited = Duration::ZERO;
                while waited < POLL_EVERY {
                    let asked_now = this
                        .update(cx, |app, _| app.security_notice_code_shown && !shown)
                        .unwrap_or(false);
                    if asked_now {
                        break;
                    }
                    cx.background_executor().timer(SHOW_CODE_TICK).await;
                    waited += SHOW_CODE_TICK;
                }
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
        match command {
            // An Easy Connect phone's page: the poll asks for the live code while it is open.
            SecurityUpgradeModalCommand::ShowCode => {
                self.security_notice_code_shown = true;
                return;
            }
            SecurityUpgradeModalCommand::HideCode => {
                self.security_notice_code_shown = false;
                return;
            }
            // Deny needs no approval: `/api/removePairedDevice` with a pending id denies it.
            SecurityUpgradeModalCommand::DenyPending(pending_id) => {
                cx.spawn(async move |_, _| {
                    let _ = gx_rpc(
                        None,
                        "/api/removePairedDevice",
                        json!({ "deviceId": pending_id }),
                    )
                    .await;
                })
                .detach();
                return;
            }
            SecurityUpgradeModalCommand::PairComputer { machine_id, code } => {
                self.pair_security_notice_computer(machine_id, code, cx);
                return;
            }
            _ => {}
        }
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
            SecurityUpgradeModalCommand::OpenRemoteSettings => {
                self.open_gpui_settings_tab("remote", None, cx);
            }
            _ => {}
        }
    }

    fn set_security_notice_computer_pairing(
        &mut self,
        machine_id: &str,
        pairing: NoticeComputerPairing,
    ) {
        self.security_notice_computer_pairing
            .retain(|(id, _)| id != machine_id);
        self.security_notice_computer_pairing
            .push((machine_id.to_string(), pairing));
    }

    /*
    CDXC:RemotePairing 2026-11-08 DECISION:
    User (11.1.1, Q1876 A): an Easy Connect computer's page in the notice takes the other
    computer's Easy Connect code and pairs right there, as Settings > Remote's machine dialog does
    (`/api/pairComputer`, then `/api/computerPairingStatus` until the owner approves on the other
    computer), and once approved saves that machine's entry with the new code, so its row ticks off.
    */
    fn pair_security_notice_computer(
        &mut self,
        machine_id: String,
        code: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(params) =
            crate::app::window::settings_modal::tabs::remote::easy_connect_pairing_params(&code)
        else {
            self.set_security_notice_computer_pairing(
                &machine_id,
                NoticeComputerPairing::Failed(
                    "That isn't a whole Easy Connect code. On the other computer, open Show QR and copy the Easy Connect code again.".to_string(),
                ),
            );
            return;
        };
        self.set_security_notice_computer_pairing(&machine_id, NoticeComputerPairing::Starting);
        cx.spawn(async move |this, cx| {
            let started = crate::app::gx_store::gx_rpc_with_timeout(
                None,
                "/api/pairComputer",
                params,
                COMPUTER_PAIRING_START_TIMEOUT,
            )
            .await;
            let failed = |message: String| NoticeComputerPairing::Failed(message);
            let (pending_id, words, computer) = match started {
                Ok(value) => {
                    let words: Vec<String> = value["words"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|word| word.as_str().map(str::to_string))
                        .collect();
                    match value["pendingId"].as_str() {
                        Some(pending_id) if words.len() == 3 => (
                            pending_id.to_string(),
                            words,
                            value["computerName"]
                                .as_str()
                                .unwrap_or("the other computer")
                                .to_string(),
                        ),
                        _ => {
                            let _ = this.update(cx, |app, _| {
                                app.set_security_notice_computer_pairing(
                                    &machine_id,
                                    failed("The other computer sent an unreadable answer.".into()),
                                )
                            });
                            return;
                        }
                    }
                }
                Err(error) => {
                    let _ = this.update(cx, |app, _| {
                        app.set_security_notice_computer_pairing(
                            &machine_id,
                            failed(error.to_string()),
                        )
                    });
                    return;
                }
            };
            let _ = this.update(cx, |app, _| {
                app.set_security_notice_computer_pairing(
                    &machine_id,
                    NoticeComputerPairing::Waiting {
                        words,
                        computer: computer.clone(),
                    },
                )
            });
            loop {
                cx.background_executor()
                    .timer(COMPUTER_PAIRING_POLL)
                    .await;
                let answer = crate::app::gx_store::gx_rpc_with_timeout(
                    None,
                    "/api/computerPairingStatus",
                    json!({ "pendingId": pending_id }),
                    COMPUTER_PAIRING_STATUS_TIMEOUT,
                )
                .await;
                let outcome = match answer.as_ref().ok().and_then(|value| value["status"].as_str()) {
                    Some("pending") => continue,
                    Some("approved") => NoticeComputerPairing::Paired,
                    Some("denied") => failed(format!(
                        "{computer} denied this computer. Show a new code there and try again."
                    )),
                    _ => match answer {
                        Err(error) => failed(error.to_string()),
                        Ok(_) => failed(format!(
                            "Nobody approved this computer on {computer} in time. Show a new code there and try again."
                        )),
                    },
                };
                let _ = this.update(cx, |app, cx| {
                    if outcome == NoticeComputerPairing::Paired {
                        app.save_security_notice_computer_code(&machine_id, &code, cx);
                    }
                    app.set_security_notice_computer_pairing(&machine_id, outcome);
                });
                break;
            }
        })
        .detach();
    }

    /// The paired machine's Settings > Remote entry with its new Easy Connect code.
    fn save_security_notice_computer_code(
        &mut self,
        machine_id: &str,
        code: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let saved = crate::shared_settings::shared_sidebar_settings_snapshot();
        let machines: Vec<Value> = saved
            .object()
            .get("remoteMachines")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|machine| {
                if machine.get("id").and_then(Value::as_str) == Some(machine_id) {
                    crate::app::window::settings_modal::tabs::remote::machine_with_easy_connect_code(
                        &machine, code,
                    )
                    .unwrap_or(machine)
                } else {
                    machine
                }
            })
            .collect();
        self.handle_gpui_app_modal_update_settings_patch_message(
            &json!({ "patch": { "remoteMachines": machines } }),
            cx,
        );
    }
}
