//! Connecting the saved remote machines at start-up and after Settings enable one, and retrying a
//! dropped machine on a ladder of 2s, 5s, 15s, 30s, then every 60s.
//!
//! CDXC:RemoteMachines 2026-07-21:
//! Rust-owned SSH tunnels are process-local and therefore never survive an app restart. Reconnect
//! every saved machine at start-up so cached last-seen rows become live again and the machine tab
//! receives the normal connecting/connected status sequence. Start-up attempts enter the same
//! reconnect ladder used after sleep/wake, so retryable failures back off without exhausting a
//! launch-only budget.
//!
//! CDXC:RemoteMachines 2026-09-25 WHY:
//! This ran in the QuickJS app runtime (`remote-machines.ts`), which posted `reconnectRemoteMachine`
//! back to Rust for every attempt. It moved here with the runtime port; the bookkeeping sits at the
//! top of `handle_gpui_reconnect_remote_machine_message`, the one door every connect goes through
//! (the machine tab's icon, the install dialog, a remote Recent Project's restore, and the ladder
//! itself), so a manual connect resets the ladder and an automatic one never doubles an attempt that
//! is still in flight.
//!
//! SEE-ALSO: apps/desktop/src/app/remote_conn/reconnect.rs,
//! apps/desktop/src/app/os_integration/toast_and_status_dispatch.rs (the status funnel).

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crate::GhostexGpuiApp;
use crate::app::helpers::gpui_remote_machine_id_from_value;
use crate::shared_settings;

/// `GPUI_REMOTE_MACHINE_RECONNECT_DELAYS_MS`; the last step repeats.
const RECONNECT_LADDER_MS: [u64; 5] = [2_000, 5_000, 15_000, 30_000, 60_000];

/// States after which the machine is retried.
fn is_retry_state(state: &str) -> bool {
    matches!(
        state,
        "disconnected"
            | "failed"
            | "keychainFailed"
            | "presentationStreamFailed"
            | "presentationSubscribeFailed"
            | "sshFailed"
            | "tokenUnavailable"
            | "tunnelFailed"
    )
}

/// States of an attempt that is still running.
fn is_progress_state(state: &str) -> bool {
    matches!(
        state,
        "connecting" | "downloadingRemoteServerPackage" | "installing"
    )
}

/// States that need the user (approve an install, fix the machine's settings), so no retry.
fn is_stop_state(state: &str) -> bool {
    matches!(
        state,
        "installApprovalRequired"
            | "installFailed"
            | "invalid"
            | "unsupported"
            | "unsupportedRemotePlatform"
    )
}

/// The ladder's memory for this app run.
#[derive(Default)]
pub(crate) struct RemoteReconnectLadder {
    started: bool,
    /// The enabled machines the last settings pass saw, so a newly enabled one is connected once.
    enabled: HashSet<String>,
    attempts: HashMap<String, usize>,
    in_flight: HashSet<String>,
    /// The generation of each armed retry timer; a timer that finds another generation was
    /// cancelled.
    timers: HashMap<String, u64>,
    timer_generation: u64,
}

impl RemoteReconnectLadder {
    fn reset(&mut self, machine_id: &str) {
        self.timers.remove(machine_id);
        self.attempts.remove(machine_id);
        self.in_flight.remove(machine_id);
    }
}

/// The saved machines that are not disabled in the sidebar, in Settings order.
fn enabled_remote_machine_ids() -> Vec<String> {
    let settings = shared_settings::shared_sidebar_settings_snapshot();
    settings
        .object()
        .get("remoteMachines")
        .and_then(serde_json::Value::as_array)
        .map(|machines| {
            machines
                .iter()
                .filter(|machine| {
                    machine.get("disabled").and_then(serde_json::Value::as_bool) != Some(true)
                })
                .filter_map(gpui_remote_machine_id_from_value)
                .collect()
        })
        .unwrap_or_default()
}

impl GhostexGpuiApp {
    /// Connects every enabled saved machine once per app run (`connectSavedRemoteMachinesOnStartup`),
    /// and after that reconciles the ladder with Settings (`reconcileRemoteMachineRetryTargets`):
    /// a machine that was disabled or removed stops retrying, one that was enabled connects.
    pub(crate) fn remote_reconnect_sync_with_settings(&mut self, cx: &mut gpui::Context<Self>) {
        // The same two moments are when a machine can stop being saved, so its stored last-seen
        // copy goes here too (gx_store/remote_last_seen_prune.rs).
        self.gx_store_prune_remote_last_seen();
        let enabled = enabled_remote_machine_ids();
        let ladder = &mut self.gx_store.remote_reconnect;
        let to_connect: Vec<String> = if !ladder.started {
            ladder.started = true;
            enabled.clone()
        } else {
            let retrying: HashSet<String> = ladder
                .attempts
                .keys()
                .chain(ladder.in_flight.iter())
                .chain(ladder.timers.keys())
                .cloned()
                .collect();
            for machine_id in retrying {
                if !enabled.contains(&machine_id) {
                    ladder.reset(&machine_id);
                }
            }
            enabled
                .iter()
                .filter(|machine_id| !ladder.enabled.contains(*machine_id))
                .cloned()
                .collect()
        };
        ladder.enabled = enabled.into_iter().collect();
        for machine_id in to_connect {
            self.remote_reconnect_start(&machine_id, true, cx);
        }
    }

    /// The first settings pass of this app run, once the store comes up.
    pub(crate) fn remote_reconnect_on_launch(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.gx_store.remote_reconnect.started {
            self.remote_reconnect_sync_with_settings(cx);
        }
    }

    /// Starts one attempt through the connect door.
    fn remote_reconnect_start(
        &mut self,
        machine_id: &str,
        automatic: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut command = serde_json::Map::new();
        command.insert("automatic".into(), automatic.into());
        command.insert("installApproved".into(), false.into());
        command.insert("remoteMachineId".into(), machine_id.into());
        command.insert("type".into(), "reconnectRemoteMachine".into());
        self.handle_gpui_reconnect_remote_machine_message(&command, cx);
    }

    /// The machine tab's connect icon: a manual attempt.
    pub(crate) fn remote_reconnect_from_sidebar(
        &mut self,
        machine_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        self.remote_reconnect_start(machine_id, false, cx);
    }

    /// Called by the connect door for every attempt. A manual attempt resets the ladder; an
    /// automatic one is dropped while another attempt for the machine is still running. Returns
    /// whether the attempt may go ahead.
    pub(crate) fn remote_reconnect_admit(&mut self, machine_id: &str, automatic: bool) -> bool {
        let ladder = &mut self.gx_store.remote_reconnect;
        if !automatic {
            ladder.reset(machine_id);
        } else if ladder.in_flight.contains(machine_id) {
            return false;
        }
        ladder.timers.remove(machine_id);
        ladder.in_flight.insert(machine_id.to_string());
        true
    }

    /// Every status transition of a machine, from the status funnel.
    pub(crate) fn remote_reconnect_on_status(
        &mut self,
        machine_id: &str,
        state: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let ladder = &mut self.gx_store.remote_reconnect;
        if state == "connected" || is_stop_state(state) {
            ladder.reset(machine_id);
        } else if is_progress_state(state) {
            ladder.in_flight.insert(machine_id.to_string());
            ladder.timers.remove(machine_id);
        } else if is_retry_state(state) {
            ladder.in_flight.remove(machine_id);
            self.remote_reconnect_schedule(machine_id, cx);
        }
    }

    /// `scheduleRemoteReconnect`: arms the next step of the ladder, unless an attempt or a timer is
    /// already pending or the machine is no longer an enabled saved machine.
    fn remote_reconnect_schedule(&mut self, machine_id: &str, cx: &mut gpui::Context<Self>) {
        let ladder = &mut self.gx_store.remote_reconnect;
        if ladder.in_flight.contains(machine_id) || ladder.timers.contains_key(machine_id) {
            return;
        }
        if !enabled_remote_machine_ids()
            .iter()
            .any(|id| id == machine_id)
        {
            ladder.reset(machine_id);
            return;
        }
        let attempts = ladder.attempts.get(machine_id).copied().unwrap_or(0);
        let delay = RECONNECT_LADDER_MS[attempts.min(RECONNECT_LADDER_MS.len() - 1)];
        ladder.timer_generation += 1;
        let generation = ladder.timer_generation;
        ladder.timers.insert(machine_id.to_string(), generation);
        let machine_id = machine_id.to_string();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(delay))
                .await;
            let _ = this.update(cx, |this, cx| {
                let ladder = &mut this.gx_store.remote_reconnect;
                if ladder.timers.get(&machine_id) != Some(&generation) {
                    return;
                }
                ladder.timers.remove(&machine_id);
                if !enabled_remote_machine_ids().contains(&machine_id) {
                    ladder.reset(&machine_id);
                    return;
                }
                ladder.attempts.insert(machine_id.clone(), attempts + 1);
                this.remote_reconnect_start(&machine_id, true, cx);
            });
        })
        .detach();
    }
}
