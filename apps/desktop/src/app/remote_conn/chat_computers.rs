// The desktop's half of the chat's offline-computer card (packages/gx-chat-core/src/session/computer.rs):
// what each remote machine's chats are told about it, the check a chat asks for when a message to
// it gets no answer, and the card's Retry.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use ghostex_gx_chat_core::session::computer::SessionComputer;
use serde_json::Value;

use crate::app::helpers::*;
use crate::*;

/// How long the check a chat asks for waits for the machine's gxserver to answer.
const CHECK_TIMEOUT: Duration = Duration::from_secs(4);

/// What the last check of each machine found, until its connection changes.
#[derive(Clone, Copy, Default)]
struct Checked {
    /// The check got no answer: the chats say it is offline before the tunnel watchdog does.
    silent: bool,
    /// When a check last got an answer, so the chat's "checking…" line ends.
    answered_at_ms: Option<f64>,
}

fn checks() -> MutexGuard<'static, HashMap<String, Checked>> {
    static CHECKS: OnceLock<Mutex<HashMap<String, Checked>>> = OnceLock::new();
    CHECKS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as f64)
        .unwrap_or_default()
}

impl GhostexGpuiApp {
    /// Publishes every saved remote machine for the chats (`gx_chat/computers.rs`), on each
    /// connect transition and after each check.
    ///
    /// Offline is a machine whose last attempt failed or that is not connected. A machine that is
    /// trying again (connecting) stays offline when it already was, so the card does not blink
    /// with every attempt of the reconnect ladder.
    pub(crate) fn publish_session_chat_computers(&mut self) {
        let machines: Vec<String> = shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .get("remoteMachines")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(gpui_remote_machine_id_from_value)
            .collect();
        let (_, previous) = crate::app::gx_chat::computers::current();
        let mut computers = BTreeMap::new();
        for machine_id in machines {
            let state = self
                .remote_machine_connect_states
                .get(&machine_id)
                .map(String::as_str)
                .unwrap_or("disconnected");
            let connected = state == GpuiRemoteGxserverConnectState::Connected.wire_status_state();
            let mut checks = checks();
            if connected && self.remote_gxserver_connections.contains_key(&machine_id) {
                // A connection that is (back) up ends a silent check.
                if let Some(check) = checks.get_mut(&machine_id) {
                    check.silent = false;
                }
            }
            let check = checks.get(&machine_id).copied().unwrap_or_default();
            drop(checks);
            let was_offline = previous
                .get(&machine_id)
                .is_some_and(|computer| computer.offline);
            let trying = ghostex_gx_core::machine_state_is_busy(state);
            let offline = check.silent || (!connected && (!trying || was_offline));
            let last_seen_ms = if connected && !check.silent {
                None
            } else {
                crate::app::gx_store::remote_last_connected_ms(&machine_id).map(|at| at as f64)
            };
            computers.insert(
                machine_id.clone(),
                SessionComputer {
                    name: gpui_remote_machine_name_from_settings(&machine_id)
                        .unwrap_or_else(|| machine_id.clone()),
                    remote: true,
                    offline,
                    last_seen_ms,
                    server_id:
                        crate::app::remote_conn::connected_computers::gpui_remote_machine_server_id(
                            &machine_id,
                        ),
                    machine_id: Some(machine_id.clone()),
                    answered_at_ms: check.answered_at_ms,
                },
            );
        }
        crate::app::gx_chat::computers::publish(computers);
    }

    /// A chat's message to `machine_id` got no answer for 10 s (Q1838 "check fast"): ask its
    /// gxserver directly instead of waiting up to a minute for the tunnel to drop.
    pub(crate) fn check_session_chat_computer(
        &mut self,
        machine_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let target = self.gpui_remote_gxserver_request_target(&machine_id);
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let answered = background
                .spawn(async move {
                    target.is_some_and(|target| {
                        matches!(
                            gpui_remote_health_probe(
                                target.local_port,
                                &target.token,
                                CHECK_TIMEOUT
                            ),
                            GpuiRemoteHealthProbe::Healthy(_)
                        )
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                {
                    let mut checks = checks();
                    let check = checks.entry(machine_id.clone()).or_default();
                    check.silent = !answered;
                    if answered {
                        check.answered_at_ms = Some(now_ms());
                    }
                }
                this.publish_session_chat_computers();
                cx.notify();
            });
        })
        .detach();
    }

    /// Retry on the card: reconnect the machine now, as its sidebar tab's connect button does.
    pub(crate) fn retry_session_chat_computer(
        &mut self,
        machine_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.remote_gxserver_connections.contains_key(machine_id) {
            self.check_session_chat_computer(machine_id.to_string(), cx);
            return;
        }
        self.remote_reconnect_from_sidebar(machine_id, cx);
    }
}
