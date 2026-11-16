//! The gxserver side of the Connect a computer pane: pairing with the computer whose code was
//! pasted, saving it once it approves, following the sign-in, the password when that computer
//! runs an older Ghostex, and Connect both ways.
//! SEE-ALSO: apps/desktop/src/app/window/connect_device_modal/computer_way.rs (the pane and its
//! decision record), server/src/remote_access/computer_pairing.rs and connect_back.rs.
use crate::app::helpers::*;
use crate::app::remote_conn::easy_connect_machines::EasyConnectMachine;
use crate::app::window::connect_device_modal::join_target;
use crate::app::window::settings_modal::tabs::remote::{
    connect_back_code_params, easy_connect_pairing_params,
};
use crate::app::window::*;
use crate::*;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// `/api/pairComputer` waits up to 30 s for the other computer.
const PAIR_START_TIMEOUT: Duration = Duration::from_secs(45);
/// `/api/computerPairingStatus` may wait on the other computer's administrator prompt.
const PAIR_STATUS_TIMEOUT: Duration = Duration::from_secs(200);
const SHORT_TIMEOUT: Duration = Duration::from_secs(15);
/// How long Connect both ways waits for the other computer's request to show here.
const BACK_REQUEST_WAIT: Duration = Duration::from_secs(45);
/// How long it waits for that computer to finish once approved here.
const BACK_FINISH_WAIT: Duration = Duration::from_secs(200);
const BACK_POLL: Duration = Duration::from_millis(800);

fn local_rpc(path: &str, params: Value, timeout: Duration) -> Result<Value, String> {
    gpui_gxserver_rpc_result(path, &params, timeout)
}

/// The saved computer's name, for the window's words.
fn saved_machine_name(remote_machine_id: &str) -> String {
    shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get("remoteMachines")
        .and_then(Value::as_array)
        .and_then(|machines| {
            machines.iter().find(|machine| {
                machine.get("id").and_then(Value::as_str) == Some(remote_machine_id)
            })
        })
        .and_then(|machine| machine.get("name").and_then(Value::as_str))
        .unwrap_or("the other computer")
        .to_string()
}

/// A route the other computer's gxserver doesn't have: it runs a Ghostex from before Connect
/// both ways.
fn is_unknown_route(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    // 11.6.1 answers "No gxserver endpoint for POST /api/connectBackToComputer." (`notFound`).
    lower.contains("no gxserver endpoint")
        || lower.contains("notfound")
        || lower.contains("not found")
        || lower.contains("404")
}

impl GhostexGpuiApp {
    /// Runs `update` on the open Connect window with its window at hand.
    fn update_connect_window_in(
        &mut self,
        cx: &mut gpui::Context<Self>,
        update: impl FnOnce(
            &mut GpuiConnectDeviceWindow,
            &mut gpui::Window,
            &mut gpui::Context<GpuiConnectDeviceWindow>,
        ),
    ) {
        self.update_native_app_modal(
            GpuiAppModalKind::RemoteSetup,
            cx,
            |modal: &mut GpuiConnectDeviceWindow, window, cx| update(modal, window, cx),
        );
    }

    pub(crate) fn handle_gpui_connect_computer_command(
        &mut self,
        command: ConnectDeviceCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        match command {
            ConnectDeviceCommand::JoinComputer { code } => self.join_computer(code, cx),
            ConnectDeviceCommand::JoinStatus { pending_id } => {
                cx.spawn(async move |this, cx| {
                    let answer = cx
                        .background_executor()
                        .spawn(async move {
                            local_rpc(
                                "/api/computerPairingStatus",
                                json!({ "pendingId": pending_id }),
                                PAIR_STATUS_TIMEOUT,
                            )
                        })
                        .await;
                    let _ = this.update(cx, |app, cx| {
                        app.update_connect_window_in(cx, |modal, _window, cx| {
                            modal.join_status(answer, cx)
                        });
                    });
                })
                .detach();
            }
            ConnectDeviceCommand::CancelJoin { pending_id } => {
                cx.background_executor()
                    .spawn(async move {
                        let _ = local_rpc(
                            "/api/cancelComputerPairing",
                            json!({ "pendingId": pending_id }),
                            SHORT_TIMEOUT,
                        );
                    })
                    .detach();
            }
            ConnectDeviceCommand::SaveJoined {
                target,
                key_installed,
            } => {
                let id = self.save_easy_connect_machine(
                    &EasyConnectMachine {
                        name: target.name,
                        easy_connect_address: target.address,
                        ssh_user: target.user,
                        ssh_port: target.ssh_port,
                    },
                    cx,
                );
                self.update_connect_window_in(cx, |modal, window, cx| {
                    modal.join_saved(id, key_installed, window, cx)
                });
            }
            ConnectDeviceCommand::ReadSignIn { remote_machine_id } => {
                let state = self
                    .remote_machine_connect_states
                    .get(&remote_machine_id)
                    .cloned();
                let message = self
                    .remote_machine_status_messages
                    .get(&remote_machine_id)
                    .cloned();
                self.update_connect_window_in(cx, |modal, _window, cx| {
                    modal.sign_in_state(state, message, cx)
                });
            }
            ConnectDeviceCommand::SubmitPassword {
                remote_machine_id,
                user,
                password,
            } => {
                self.set_saved_machine_user(&remote_machine_id, &user, cx);
                let mut command = serde_json::Map::new();
                command.insert("remoteMachineId".into(), json!(remote_machine_id));
                command.insert("password".into(), json!(password));
                // Saving the password signs in again when the last try was refused
                // (`finish_gpui_remote_machine_password_save`).
                self.handle_gpui_save_remote_machine_password_message(&command, cx);
            }
            ConnectDeviceCommand::Reconnect { remote_machine_id } => {
                self.reconnect_saved_machine(&remote_machine_id, cx);
            }
            ConnectDeviceCommand::ConnectBack {
                remote_machine_id,
                code,
            } => self.connect_back(remote_machine_id, code, cx),
            ConnectDeviceCommand::OpenAddComputerSsh => {
                self.connect_device_window_closed(cx);
                self.open_gpui_settings_remote_section(Some("addComputerSsh"), cx);
            }
            _ => {}
        }
    }

    fn reconnect_saved_machine(&mut self, remote_machine_id: &str, cx: &mut gpui::Context<Self>) {
        let mut command = serde_json::Map::new();
        command.insert("remoteMachineId".into(), json!(remote_machine_id));
        command.insert("type".into(), json!("reconnectRemoteMachine"));
        self.handle_gpui_reconnect_remote_machine_message(&command, cx);
    }

    /// The username typed with the password replaces the one the code named.
    fn set_saved_machine_user(
        &mut self,
        remote_machine_id: &str,
        user: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut machines: Vec<Value> = shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .get("remoteMachines")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let Some(machine) = machines
            .iter_mut()
            .find(|machine| machine.get("id").and_then(Value::as_str) == Some(remote_machine_id))
        else {
            return;
        };
        if machine.get("sshUser").and_then(Value::as_str) == Some(user) {
            return;
        }
        machine["sshUser"] = json!(user);
        let machines =
            ghostex_gx_core::normalize_remote_machine_settings(Some(&Value::Array(machines)));
        self.handle_gpui_app_modal_update_settings_patch_message(
            &json!({
                "patch": { "remoteMachines": machines },
                "source": "settings:remoteMachines",
                "type": "updateSettingsPatch",
            }),
            cx,
        );
    }

    fn join_computer(&mut self, code: String, cx: &mut gpui::Context<Self>) {
        let (Some(params), Some(target)) = (easy_connect_pairing_params(&code), join_target(&code))
        else {
            self.update_connect_window_in(cx, |modal, _window, cx| {
                modal.join_started(
                    Err("That isn't a computer's code. On the other computer, click Connect a Computer and copy its code.".to_string()),
                    cx,
                )
            });
            return;
        };
        cx.spawn(async move |this, cx| {
            let answer = cx
                .background_executor()
                .spawn(async move { local_rpc("/api/pairComputer", params, PAIR_START_TIMEOUT) })
                .await;
            let result = answer.and_then(|answer| {
                let pending_id = answer
                    .get("pendingId")
                    .and_then(Value::as_str)
                    .ok_or("The other computer sent an unreadable answer.")?
                    .to_string();
                let words: Vec<String> = answer
                    .get("words")
                    .and_then(Value::as_array)
                    .ok_or("The other computer sent an unreadable answer.")?
                    .iter()
                    .filter_map(|word| word.as_str().map(str::to_string))
                    .collect();
                let mut target = target;
                if let Some(name) = answer
                    .get("computerName")
                    .and_then(Value::as_str)
                    .filter(|name| !name.trim().is_empty())
                {
                    target.name = name.trim().to_string();
                }
                Ok((pending_id, words, target))
            });
            let _ = this.update(cx, |app, cx| {
                app.update_connect_window_in(cx, |modal, _window, cx| {
                    modal.join_started(result, cx)
                });
            });
        })
        .detach();
    }

    /// Connect both ways (CDXC:RemotePairing 2026-11-14 in server/src/remote_access/connect_back.rs):
    /// sign this computer's own code, hand it to the other computer over this app's connection to
    /// it, approve its request here, and wait until it has added this computer.
    fn connect_back(
        &mut self,
        remote_machine_id: String,
        code: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let name = saved_machine_name(&remote_machine_id);
        let target = self.gpui_remote_gxserver_request_target(&remote_machine_id);
        cx.spawn(async move |this, cx| {
            let background = cx.background_executor().clone();
            let progress = |this: &gpui::WeakEntity<Self>,
                            cx: &mut gpui::AsyncApp,
                            step: Result<Option<BackStep>, String>| {
                let _ = this.update(cx, |app, cx| {
                    app.update_connect_window_in(cx, |modal, _window, cx| {
                        modal.connect_back_progress(step, cx)
                    });
                });
            };
            let result: Result<(), String> = async {
                let target = target.ok_or_else(|| {
                    format!("This computer isn't connected to {name} right now. Wait until it is, then try again.")
                })?;
                let mut params = connect_back_code_params(&code).ok_or_else(|| {
                    "This computer's code isn't ready. Close this window and open Connect a Computer again.".to_string()
                })?;
                let secret = params
                    .get("secret")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let signed = background
                    .spawn(async move {
                        local_rpc(
                            "/api/signComputerConnectBack",
                            json!({ "secret": secret }),
                            SHORT_TIMEOUT,
                        )
                    })
                    .await?;
                if let (Some(params), Some(signed)) = (params.as_object_mut(), signed.as_object()) {
                    params.extend(signed.clone());
                }
                let asked = crate::app::gx_store::gx_rpc_with_timeout(
                    Some(target.clone()),
                    "/api/connectBackToComputer",
                    params,
                    PAIR_START_TIMEOUT,
                )
                .await
                .map_err(|error| {
                    let message = error.to_string();
                    if is_unknown_route(&message) {
                        format!("Update Ghostex on {name} first, then click Try again to connect both ways.")
                    } else {
                        message
                    }
                })?;
                let pending_id = asked
                    .get("pendingId")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let words: Vec<String> = asked
                    .get("words")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|word| word.as_str().map(str::to_string))
                    .collect();
                if pending_id.is_empty() || words.is_empty() {
                    return Err(format!("{name} sent an unreadable answer. Try again."));
                }
                progress(&this, cx, Ok(Some(BackStep::Approving)));
                // Its request shows here with the same words; the click on Connect both ways was
                // the owner's answer (the approvals rule still applies: Touch ID when asked).
                let started = Instant::now();
                let approval_id = loop {
                    let waiting = background
                        .spawn(async { local_rpc("/api/pairedDevices", json!({}), SHORT_TIMEOUT) })
                        .await
                        .map(|answer| crate::app::connect_device_modal_lifecycle::waiting_devices_of(&answer))
                        .unwrap_or_default();
                    if let Some(device) = waiting.into_iter().find(|device| device.words == words) {
                        break device.approval_id;
                    }
                    if started.elapsed() > BACK_REQUEST_WAIT {
                        return Err(format!("{name}'s request didn't arrive here. Keep this window open and try again."));
                    }
                    background.timer(BACK_POLL).await;
                };
                background
                    .spawn(async move {
                        crate::app::owner_security::approve_local_request_id(&approval_id)
                    })
                    .await?;
                progress(&this, cx, Ok(Some(BackStep::Adding)));
                let started = Instant::now();
                loop {
                    let answer = crate::app::gx_store::gx_rpc_with_timeout(
                        Some(target.clone()),
                        "/api/computerPairingStatus",
                        json!({ "pendingId": pending_id }),
                        PAIR_STATUS_TIMEOUT,
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                    match answer.get("status").and_then(Value::as_str) {
                        Some("approved") => {
                            return if answer.get("machineAdded").and_then(Value::as_bool)
                                == Some(true)
                            {
                                Ok(())
                            } else {
                                Err(format!("Open Ghostex on {name}, then click Try again."))
                            };
                        }
                        Some("pending") => {}
                        Some("denied") => {
                            return Err(format!("{name} was turned away here. Try again."));
                        }
                        _ => {
                            return Err(format!("{name} didn't finish in time. Try again."));
                        }
                    }
                    if started.elapsed() > BACK_FINISH_WAIT {
                        return Err(format!("{name} didn't finish in time. Try again."));
                    }
                    background.timer(BACK_POLL).await;
                }
            }
            .await;
            progress(&this, cx, result.map(|()| None));
        })
        .detach();
    }
}
