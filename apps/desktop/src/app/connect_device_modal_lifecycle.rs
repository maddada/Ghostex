//! The gxserver side of the Connect a phone window: turning Easy Connect on, reading the code and
//! the waiting devices every second, approving or denying, and keeping the request's notice away
//! while this window shows it.
//! SEE-ALSO: apps/desktop/src/app/window/connect_device_modal.rs (the window and its decision
//! record), apps/desktop/src/app/owner_security/ (approvals), apps/desktop/src/app/native_app_modal_lifecycle.rs (the shared window path).
use crate::app::helpers::*;
use crate::app::owner_security::Transport;
use crate::app::window::*;
use crate::*;
use serde_json::{Value, json};
use std::time::Duration;

const RPC_TIMEOUT: Duration = Duration::from_secs(15);
/// `/api/enableSshAccess` waits on the admin prompt until the user answers it.
const ENABLE_SSH_TIMEOUT: Duration = Duration::from_secs(300);
/// How long the window follows the Easy Connect helper's install (it downloads and builds).
const INSTALL_TIMEOUT: Duration = Duration::from_secs(20 * 60);
/// How often the window reads the install's step.
const INSTALL_POLL: Duration = Duration::from_secs(1);
/// Starting the helper.
const TURN_ON_TIMEOUT: Duration = Duration::from_secs(60);

/// What turning Easy Connect on still has to do, read from gxserver.
struct PrepareNeeds {
    /// The helper isn't installed: the window offers Install (`installing`: one already runs).
    install: bool,
    installing: bool,
    install_error: Option<String>,
    ssh: bool,
    turn_on: bool,
}

fn rpc(path: &str, params: Value, timeout: Duration) -> Result<Value, String> {
    gpui_gxserver_rpc_result(path, &params, timeout)
}

/// The status inside a `/api/tailcatStatus`, `/api/installTailcat` or `/api/updateTailcatState`
/// answer (`{ status: {...} }`).
///
/// CDXC:RemotePairing 2026-11-11 FIX: GitHub #559. These answers carry the status one level down,
/// and the window read `binaryFound` and `enabled` at the top: the missing helper read as present,
/// so the window never installed it, turned Easy Connect on without it and waited for a code that
/// could not come. Settings > Remote reads it the same way (`read_tailcat_status_result`).
fn tailcat_status(answer: Value) -> Value {
    match answer.get("status") {
        Some(status) if status.is_object() => status.clone(),
        _ => answer,
    }
}

fn read_tailcat_status() -> Result<Value, String> {
    rpc("/api/tailcatStatus", json!({}), RPC_TIMEOUT).map(tailcat_status)
}

fn prepare_needs() -> Result<PrepareNeeds, String> {
    let status = read_tailcat_status()?;
    let access = rpc("/api/remoteAccessStatus", json!({}), RPC_TIMEOUT)?;
    Ok(PrepareNeeds {
        install: status.get("binaryFound").and_then(Value::as_bool) != Some(true),
        installing: status.get("installing").and_then(Value::as_bool) == Some(true),
        install_error: status_error(&status, "installError"),
        ssh: access.pointer("/ssh/enabled").and_then(Value::as_bool) == Some(false),
        turn_on: status.get("enabled").and_then(Value::as_bool) != Some(true),
    })
}

/// The error a status answer carries (`installError`, `lastError`), if any.
fn status_error(status: &Value, key: &str) -> Option<String> {
    status
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|error| !error.is_empty())
        .map(str::to_string)
}

/// One read of the helper's install: still running (with its step), done, or failed (why).
enum InstallStep {
    Running(Option<String>),
    Done,
    Failed(String),
}

fn install_step(status: &Value) -> InstallStep {
    if status.get("installing").and_then(Value::as_bool) == Some(true) {
        return InstallStep::Running(status_error(status, "installProgress"));
    }
    if status.get("binaryFound").and_then(Value::as_bool) == Some(true) {
        return InstallStep::Done;
    }
    InstallStep::Failed(status_error(status, "installError").unwrap_or_else(|| {
        "The Easy Connect helper didn't install. Try again.".to_string()
    }))
}

/// One admin prompt; a cancelled prompt is not an error (Remote Setup's Connect did the same).
fn enable_ssh() -> Result<(), String> {
    let enabled = rpc("/api/enableSshAccess", json!({}), ENABLE_SSH_TIMEOUT)?;
    if enabled.get("outcome").and_then(Value::as_str) == Some("failed") {
        return Err(enabled
            .get("message")
            .and_then(Value::as_str)
            .filter(|message| !message.is_empty())
            .unwrap_or("SSH access could not be turned on.")
            .to_string());
    }
    Ok(())
}

fn turn_on() -> Result<(), String> {
    let status = rpc(
        "/api/updateTailcatState",
        json!({ "kind": "setEnabled", "enabled": true }),
        TURN_ON_TIMEOUT,
    )
    .map(tailcat_status)?;
    if status.get("enabled").and_then(Value::as_bool) == Some(false) {
        return Err(status_error(&status, "lastError")
            .unwrap_or_else(|| "Easy Connect couldn't be turned on.".to_string()));
    }
    Ok(())
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn is_phone_platform(platform: &str) -> bool {
    matches!(
        platform.trim().to_lowercase().as_str(),
        "android" | "ios" | "iphone" | "ipad" | "mobile" | "phone"
    )
}

/// What a device says it is, in the user's words.
fn kind_label(platform: &str) -> String {
    match platform.trim().to_lowercase().as_str() {
        "ios" | "iphone" => "iPhone".to_string(),
        "ipad" => "iPad".to_string(),
        "android" => "Android".to_string(),
        "mobile" | "phone" => "phone".to_string(),
        _ => "computer".to_string(),
    }
}

/// The waiting devices of a `/api/pairedDevices` answer (`pending`, pairing v2).
pub(crate) fn waiting_devices_of(answer: &Value) -> Vec<WaitingDevice> {
    answer
        .get("pending")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|device| {
            let platform = text(device, "platform").unwrap_or_default();
            Some(WaitingDevice {
                name: text(device, "name").unwrap_or_else(|| "A new device".to_string()),
                kind_label: kind_label(&platform),
                phone: is_phone_platform(&platform),
                words: device
                    .get("words")?
                    .as_array()?
                    .iter()
                    .filter_map(|word| word.as_str().map(str::to_string))
                    .collect(),
                approval_id: text(device, "approvalId").filter(|id| !id.is_empty())?,
            })
        })
        .collect()
}

/// Asks for the code with the door open for a new device (`openTunnel`, which only the desktop's
/// own window may), and for the devices waiting and paired.
fn poll() -> ConnectPoll {
    let mut poll = ConnectPoll::default();
    match rpc(
        "/api/remotePairingCode",
        json!({ "openTunnel": true }),
        RPC_TIMEOUT,
    ) {
        Ok(answer) => {
            if let Some(entry) = answer.get("easyConnect") {
                poll.computer_name = entry
                    .pointer("/code/name")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let payload = text(entry, "payload").unwrap_or_default();
                let code_text = text(entry, "text").unwrap_or_default();
                // Only a code that carries its one-time secret can pair (CDXC:RemotePairing
                // 2026-11-08 FIX in settings_modal/tabs/remote/easy_connect.rs).
                if !payload.is_empty() && !code_text.is_empty() {
                    poll.code = Some(ConnectCode {
                        payload,
                        text: code_text,
                    });
                }
            }
        }
        Err(error) => poll.error = Some(error),
    }
    match rpc("/api/pairedDevices", json!({}), RPC_TIMEOUT) {
        Ok(answer) => {
            poll.waiting = waiting_devices_of(&answer);
            poll.paired_ids = answer
                .get("devices")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|device| text(device, "id"))
                .collect();
        }
        Err(error) => {
            poll.error.get_or_insert(error);
        }
    }
    poll
}

impl GhostexGpuiApp {
    /// Opens the Connect a phone window for the `remoteSetup` open message (sidebar menu >
    /// Connect a phone; `device: "computer"` from Settings > Remote's Connect a computer).
    pub(crate) fn open_gpui_connect_device_modal(
        &mut self,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        if message.get("modal").and_then(Value::as_str)
            != Some(GpuiAppModalKind::RemoteSetup.modal_id())
        {
            return;
        }
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        let tailscale_enabled = settings
            .object()
            .get("remoteTailscaleEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let config = ConnectDeviceConfig {
            palette: self.gpui_native_dialog_palette(),
            kind: ConnectDeviceKind::from_open_message(message),
            computer_name: crate::host_name::local_computer_name(),
            tailscale_enabled,
            require_presence: crate::app::owner_security::plain::asks_presence_first(
                &Transport::Local,
            ),
        };
        let host = self.native_app_modal_host(cx, |app, command, cx| {
            app.handle_gpui_connect_device_command(command, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::RemoteSetup,
            CONNECT_DEVICE_MODAL_WIDTH,
            CONNECT_DEVICE_MODAL_INITIAL_HEIGHT,
            move |window, cx| cx.new(|cx| GpuiConnectDeviceWindow::new(config, host, window, cx)),
            cx,
        );
    }

    /// Runs `update` on the open Connect a phone window, if it is open.
    fn update_connect_device_window<R>(
        &mut self,
        cx: &mut gpui::Context<Self>,
        update: impl FnOnce(
            &mut GpuiConnectDeviceWindow,
            &mut gpui::Context<GpuiConnectDeviceWindow>,
        ) -> R,
    ) -> Option<R> {
        self.update_native_app_modal(
            GpuiAppModalKind::RemoteSetup,
            cx,
            |modal: &mut GpuiConnectDeviceWindow, _window, cx| update(modal, cx),
        )
    }

    fn handle_gpui_connect_device_command(
        &mut self,
        command: ConnectDeviceCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        match command {
            ConnectDeviceCommand::Prepare => self.prepare_connect_device(cx),
            ConnectDeviceCommand::InstallHelper => self.install_connect_device_helper(true, cx),
            ConnectDeviceCommand::Poll => {
                cx.spawn(async move |this, cx| {
                    let answer = cx.background_executor().spawn(async move { poll() }).await;
                    let _ = this.update(cx, |app, cx| {
                        // The window shows this request now: its notice, if it is up, goes.
                        for device in &answer.waiting {
                            app.owner_security_hide_local_card(&device.approval_id, cx);
                        }
                        app.update_connect_device_window(cx, |modal, cx| {
                            modal.poll_answered(answer, cx)
                        });
                    });
                })
                .detach();
            }
            ConnectDeviceCommand::Decide {
                approval_id,
                approve,
            } => {
                let id = approval_id.clone();
                cx.spawn(async move |this, cx| {
                    let result = cx
                        .background_executor()
                        .spawn(async move {
                            if approve {
                                crate::app::owner_security::approve_local_request_id(&id)
                            } else {
                                crate::app::owner_security::deny_request(&Transport::Local, &id)
                            }
                        })
                        .await;
                    let _ = this.update(cx, |app, cx| {
                        app.update_connect_device_window(cx, |modal, cx| {
                            modal.decided(approval_id, approve, result, cx)
                        });
                    });
                })
                .detach();
            }
            ConnectDeviceCommand::Copied => gpui_copy_feedback(cx),
            ConnectDeviceCommand::OpenExternalUrl(url) => {
                self.receive_gpui_titlebar_resources_open_external_url_message(&json!({
                    "type": "openExternalUrl",
                    "url": url,
                }));
            }
            ConnectDeviceCommand::OpenTailscale => {
                self.connect_device_window_closed(cx);
                self.open_gpui_settings_remote_section("tailscale", cx);
            }
            ConnectDeviceCommand::Close => self.connect_device_window_closed(cx),
        }
    }

    /// Starts the helper's install (`start`; else follows the one that runs) and shows each step
    /// in the window; once the helper is in, the rest of turning on runs and the code appears.
    fn install_connect_device_helper(&mut self, start: bool, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |this, cx| {
            let background = cx.background_executor().clone();
            let first = background
                .spawn(async move {
                    if start {
                        rpc("/api/installTailcat", json!({}), RPC_TIMEOUT).map(tailcat_status)
                    } else {
                        read_tailcat_status()
                    }
                })
                .await;
            let started = std::time::Instant::now();
            let mut answer = first;
            let outcome = loop {
                let step = match answer {
                    Ok(status) => install_step(&status),
                    Err(error) => InstallStep::Failed(error),
                };
                match step {
                    InstallStep::Running(note) => {
                        let open = this
                            .update(cx, |app, cx| {
                                app.update_connect_device_window(cx, |modal, cx| {
                                    modal.helper_install_progress(note, cx)
                                })
                                .is_some()
                            })
                            .unwrap_or(false);
                        // The window closed: the install goes on in gxserver.
                        if !open {
                            return;
                        }
                    }
                    InstallStep::Done => break Ok(()),
                    InstallStep::Failed(error) => break Err(error),
                }
                if started.elapsed() > INSTALL_TIMEOUT {
                    break Err(
                        "The Easy Connect helper's install is taking very long. Check that this computer is online, then try again."
                            .to_string(),
                    );
                }
                background.timer(INSTALL_POLL).await;
                answer = background.spawn(async { read_tailcat_status() }).await;
            };
            let _ = this.update(cx, |app, cx| match outcome {
                Ok(()) => {
                    app.update_connect_device_window(cx, |modal, cx| modal.helper_installed(cx));
                    app.prepare_connect_device(cx);
                }
                Err(error) => {
                    app.update_connect_device_window(cx, |modal, cx| {
                        modal.needs_helper(Some(error), cx)
                    });
                }
            });
        })
        .detach();
    }

    /// Offer the helper's install if it is missing (follow it if one runs), turn SSH access on if
    /// it is off, turn Easy Connect on; the window shows each step and then asks for the code.
    fn prepare_connect_device(&mut self, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |this, cx| {
            let background = cx.background_executor().clone();
            let progress = |this: &gpui::WeakEntity<Self>, cx: &mut gpui::AsyncApp, note: &str| {
                let note = note.to_string();
                let _ = this.update(cx, |app, cx| {
                    app.update_connect_device_window(cx, |modal, cx| {
                        modal.prepare_progress(note, cx)
                    });
                });
            };
            let needs = match background.spawn(async { prepare_needs() }).await {
                Ok(needs) => needs,
                Err(error) => {
                    let _ = this.update(cx, |app, cx| {
                        app.update_connect_device_window(cx, |modal, cx| {
                            modal.prepared(Err(error), cx)
                        });
                    });
                    return;
                }
            };
            if needs.install {
                let _ = this.update(cx, |app, cx| {
                    if needs.installing {
                        app.install_connect_device_helper(false, cx);
                    } else {
                        app.update_connect_device_window(cx, |modal, cx| {
                            modal.needs_helper(needs.install_error, cx)
                        });
                    }
                });
                return;
            }
            let result: Result<(), String> = async {
                if needs.ssh {
                    progress(
                        &this,
                        cx,
                        "Turning on SSH access. Your computer may ask for your password.",
                    );
                    background.spawn(async { enable_ssh() }).await?;
                }
                if needs.turn_on {
                    progress(&this, cx, "Turning on Easy Connect…");
                    background.spawn(async { turn_on() }).await?;
                }
                Ok(())
            }
            .await;
            let _ = this.update(cx, |app, cx| {
                app.update_connect_device_window(cx, |modal, cx| modal.prepared(result, cx));
            });
        })
        .detach();
    }

    /// The window went away (closed, or replaced by another modal): a request still waiting gets
    /// its notice back, since nothing shows it now.
    pub(crate) fn connect_device_window_closed(&mut self, cx: &mut gpui::Context<Self>) {
        self.release_native_app_modal_window(GpuiAppModalKind::RemoteSetup, cx);
        self.owner_security_show_pending_pairing_requests(cx);
    }

    /// Settings on the Remote page, with `section` (`tailscale`) open.
    fn open_gpui_settings_remote_section(&mut self, section: &str, cx: &mut gpui::Context<Self>) {
        let modal = GpuiAppModalKind::Settings;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = json!({
            "initialRemoteSection": section,
            "initialTab": "remote",
            "modal": modal.modal_id(),
            "type": "open",
        });
        open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, None, cx);
    }
}
