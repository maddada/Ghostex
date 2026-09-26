//! A remote row's session actions, performed by the store instead of by the old runtime.
//!
//! A remote row's sleep, wake, close, fork, pin, park, tag, favorite, snooze, unsnooze and Full
//! Reload are calls down that machine's tunnel, and nothing else happens: the old runtime patched
//! nothing, moved no focus and hid no row for them, and the machine's own presentation stream is
//! what redraws the row. The decision (which calls, in which order, which of them are waited for,
//! and what a failed one shows) is gx-core's (`sidebar_actions/remote.rs`). This file only runs the
//! legs, one at a time, through the SAME function the old runtime's bridge message reaches
//! (`remote_conn/sidebar_rpc.rs`), so the allowlist, the shaping, the tunnel's token and the
//! refresh of the old runtime's copy are one implementation and not two.
//!
//! Before this, a remote row's action left Rust for QuickJS, was parsed there, and came straight
//! back to Rust as a bridge message; the round trip bought nothing, because the store's own client
//! for that machine already holds the tunnel's port and token.
//!
//! **The counters that prove this path fires in the app** are `remote.actions` and `remote.legs`
//! on `gxStore.sidebarActions.summary`, which rides the periodic path and writes its first line
//! with every counter at zero. A run in which the user slept, woke, pinned or closed a row on a remote
//! machine tab and `actions` is still zero means the command never reached here.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_actions/remote.rs,
//! apps/desktop/src/app/remote_conn/sidebar_rpc.rs.

use std::time::{Duration, Instant};

use ghostex_gx_core::{
    RemoteActionKind, RemoteCallMode, RemoteSessionPlan, RemoteStep, owns_remote_session_message,
    plan_remote_session_action,
};
use serde_json::{Value, json};

use super::diagnostics::{record, routine_logging_enabled};
use crate::GhostexGpuiApp;
use crate::app::helpers::gpui_normalize_remote_machine_id;
use crate::app::remote_conn::sidebar_rpc::GpuiRemoteSidebarRpcMode;

/// Per-action lines one app run may write. The totals ride in every one, and the periodic summary
/// has its own budget, so a busy run cannot silence the line that says the path is alive.
const MAX_REMOTE_ACTION_RECORDS: u32 = 200;
/// The periodic summary's own budget: one line a minute for a little over three hours of changes.
const MAX_REMOTE_SUMMARY_RECORDS: u32 = 200;
/// How often the periodic summary may write, the same interval the sidebar shadow's uses.
const REMOTE_SUMMARY_INTERVAL: Duration = Duration::from_secs(60);

/// What this app run did with remote rows. Memory only; the log lines are built from it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SidebarRemoteCounters {
    /// Actions answered here, and each kind of them.
    pub(crate) actions: u64,
    pub(crate) sleeps: u64,
    pub(crate) wakes: u64,
    pub(crate) closes: u64,
    pub(crate) forks: u64,
    pub(crate) flags: u64,
    pub(crate) parks_then_sleep: u64,
    pub(crate) snoozes: u64,
    pub(crate) unsnoozes: u64,
    pub(crate) reloads: u64,
    /// Calls sent, and which kind.
    pub(crate) legs: u64,
    pub(crate) awaited_legs: u64,
    pub(crate) fire_and_forget_legs: u64,
    /// Calls that came back, by answer. A fire-and-forget call is counted when it lands even
    /// though nothing waited for it.
    pub(crate) legs_ok: u64,
    pub(crate) legs_failed: u64,
    /// Actions a failed waited call stopped before their last leg, and the toasts that said so.
    pub(crate) stopped: u64,
    pub(crate) failure_toasts: u64,
    /// A remote row's payload the store owns but did not answer because the renderer is not
    /// drawing its list, or because the machine id does not normalize.
    pub(crate) declined_source: u64,
    pub(crate) declined_machine: u64,
    /// A remote row's payload whose own fields did not parse, which the old runtime answers.
    pub(crate) handed_back: u64,
}

/// The counters and the two log budgets.
#[derive(Default)]
pub(crate) struct SidebarRemoteHost {
    pub(crate) counters: SidebarRemoteCounters,
    records: u32,
    summary_records: u32,
    summary_at: Option<Instant>,
    summary_written: Option<(
        SidebarRemoteCounters,
        [u64; 7],
        super::sidebar_remote_focus::SidebarRemoteFocusCounters,
        super::sidebar_state_actions::SidebarStateActionCounters,
        super::sidebar_accounts::SidebarAccountCounters,
        super::sidebar_slot_jump::SlotJumpCounters,
        super::sidebar_session_slot::SessionSlotCounters,
        super::sidebar_close_project::CloseProjectCounters,
        super::sidebar_focus_route::LocalFocusRouteCounters,
        super::remote_project_docs::RemoteProjectDocCounters,
    )>,
}

impl GhostexGpuiApp {
    /// Answers a remote row's per-session payload when the store owns it. Returns whether it did,
    /// in which case the command must NOT also reach the old runtime, which would send the same
    /// call to the machine a second time.
    ///
    /// Every payload answered here is a gxserver message, so it arrives WRAPPED as
    /// `{ type: 'command', message }` (controller.ts:131). The one renderer command of the family,
    /// `sessionAction: snooze`, is answered by `sidebar_snooze.rs`, which posts the wrapped
    /// `snoozeSession` that lands here.
    pub(crate) fn gx_store_run_sidebar_remote(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if command.get("type").and_then(Value::as_str) != Some("command") {
            return false;
        }
        let Some(message) = command.get("message") else {
            return false;
        };
        if !owns_remote_session_message(message) {
            return false;
        }
        if !self.gx_store_sidebar_list_ready() {
            self.gx_store.sidebar_remote.counters.declined_source += 1;
            return false;
        }
        match self.gx_store_start_remote(message, cx) {
            Some(task) => {
                task.detach();
                true
            }
            None => false,
        }
    }

    /// The same action as a task the caller can wait for, which resolves once its last leg came
    /// back or a failed one stopped it, to whether it came home without a stop. The paced bulk
    /// sleep and the set reload wait on it; `None` means the old runtime answers the payload.
    pub(super) fn gx_store_start_remote(
        &mut self,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::Task<bool>> {
        let Some(plan) =
            plan_remote_session_action(message, self.gx_store_sleep_session_when_parking())
        else {
            if owns_remote_session_message(message) {
                self.gx_store.sidebar_remote.counters.handed_back += 1;
            }
            return None;
        };
        // The bridge drops a machine id that does not normalize before it looks at anything else;
        // the store's machines come from the same settings normalization, so this refusal is for
        // a hand-built payload, and the old runtime then answers it exactly as it always did.
        let Some(machine_id) = gpui_normalize_remote_machine_id(plan.machine_id()) else {
            self.gx_store.sidebar_remote.counters.declined_machine += 1;
            return None;
        };
        let counters = &mut self.gx_store.sidebar_remote.counters;
        counters.actions += 1;
        match plan.kind {
            RemoteActionKind::Sleep => counters.sleeps += 1,
            RemoteActionKind::Wake => counters.wakes += 1,
            RemoteActionKind::Close => counters.closes += 1,
            RemoteActionKind::Fork => counters.forks += 1,
            RemoteActionKind::Flags => counters.flags += 1,
            RemoteActionKind::ParkThenSleep => counters.parks_then_sleep += 1,
            RemoteActionKind::Snooze => counters.snoozes += 1,
            RemoteActionKind::Unsnooze => counters.unsnoozes += 1,
            RemoteActionKind::Reload => counters.reloads += 1,
        }
        Some(cx.spawn(async move |this, cx| {
            let started = Instant::now();
            let mut index = 0;
            let outcome = loop {
                let Some(leg) = plan.legs.get(index) else {
                    break RemoteStep::Done;
                };
                let mode = match leg.mode {
                    RemoteCallMode::Awaited => GpuiRemoteSidebarRpcMode::Awaited,
                    RemoteCallMode::FireAndForget => GpuiRemoteSidebarRpcMode::FireAndForget,
                };
                let sent = this.update(cx, |this, cx| {
                    let counters = &mut this.gx_store.sidebar_remote.counters;
                    counters.legs += 1;
                    match leg.mode {
                        RemoteCallMode::Awaited => counters.awaited_legs += 1,
                        RemoteCallMode::FireAndForget => counters.fire_and_forget_legs += 1,
                    }
                    this.start_gpui_remote_sidebar_rpc(
                        machine_id.as_str(),
                        leg.path,
                        Some(leg.params.clone()),
                        Duration::from_millis(leg.timeout_ms),
                        mode,
                        cx,
                    )
                });
                let Ok(task) = sent else {
                    return false;
                };
                // A fire-and-forget leg is always its plan's last, so waiting for it here to count
                // its answer delays nothing and decides nothing: `step_after` does not read it.
                let result = task.await;
                let ok = result.is_ok();
                if this
                    .update(cx, |this, _| {
                        let counters = &mut this.gx_store.sidebar_remote.counters;
                        match ok {
                            true => counters.legs_ok += 1,
                            false => counters.legs_failed += 1,
                        }
                    })
                    .is_err()
                {
                    return false;
                }
                match plan.step_after(index, ok) {
                    RemoteStep::Next(next) => index = next,
                    step => {
                        if let (RemoteStep::Stopped { toast: Some(toast) }, Err(error)) =
                            (&step, &result)
                        {
                            let description = toast.description.unwrap_or(error.as_str());
                            let level = toast.level.as_str();
                            let title = toast.title;
                            let shown = this.update(cx, |this, cx| {
                                this.gx_store.sidebar_remote.counters.failure_toasts += 1;
                                this.dispatch_gpui_app_modal_toast(level, title, description, cx);
                            });
                            if shown.is_err() {
                                return false;
                            }
                        }
                        break step;
                    }
                }
            };
            let stopped = matches!(outcome, RemoteStep::Stopped { .. });
            let _ = this.update(cx, |this, _| {
                if stopped {
                    this.gx_store.sidebar_remote.counters.stopped += 1;
                }
                this.gx_store_record_remote_action(
                    &plan,
                    stopped,
                    started.elapsed().as_millis() as u64,
                );
            });
            !stopped
        }))
    }

    /// One line per remote action, once it has finished: which kind, how many legs it planned,
    /// whether a failed call stopped it, and how long the whole thing took. Nothing about the row:
    /// the machine, project and session ids stay out, and so does every body.
    fn gx_store_record_remote_action(
        &mut self,
        plan: &RemoteSessionPlan,
        stopped: bool,
        elapsed_ms: u64,
    ) {
        let host = &mut self.gx_store.sidebar_remote;
        if host.records >= MAX_REMOTE_ACTION_RECORDS || !routine_logging_enabled() {
            return;
        }
        host.records += 1;
        record(
            "gxStore.sidebarRemote",
            json!({
                "kind": plan.kind.as_str(),
                "plannedLegs": plan.legs.len() as u64,
                "stopped": stopped,
                "elapsedMs": elapsed_ms,
                "totals": remote_counters_json(&host.counters),
            }),
        );
    }

    /// The action counters on the periodic path, whether or not anything has happened.
    ///
    /// A record emitted only when an action finishes is a record that is not there in a run where
    /// the user touched no remote row, and "the path never ran" is exactly the answer a live round
    /// needs to be able to read. So this rides the sidebar shadow's periodic summary, which is
    /// proved to reach the log in a quiet run, and its first line goes out with every counter at
    /// zero.
    ///
    /// `remoteFocus` carries the row clicks the store opened itself, which have no periodic line
    /// of their own: `opens` and `splits` are the proof the path fires, `acknowledgements` that the
    /// attention went to the old runtime with each, `tabSelections` that the open's callback moved
    /// the remote focus marks, and `marksMissed` an open that was refused so the marks stayed put.
    ///
    /// `local` carries the counters of the three LOCAL paths this same change made measurable and
    /// that have no periodic line of their own: `reloadsStopped` (a Full Reload whose sleep call
    /// failed, so the wake was never asked for), `pacedLegsWaited` (a paced bulk sleep whose next
    /// request waited for this one's answer), and the set reloads (`reloadSets`, `reloadSetRows`,
    /// `reloadSetsStopped`). The first two used to be impossible, so a non-zero value is the proof
    /// the new wait is real.
    ///
    /// `accounts` carries the two account pages (gx_store/sidebar_accounts.rs): the commands, the
    /// calls on each machine, the answers, the ones a newer command overtook, and the pages,
    /// closes, launches and switch progress they produced. Counts only: no account data.
    ///
    /// `slotJump` carries the project slot hotkeys (gx_store/sidebar_slot_jump.rs).
    pub(super) fn gx_store_sidebar_actions_summary(&mut self) {
        let lifecycle = self.gx_store.sidebar_lifecycle;
        let local = [
            lifecycle.reloads_stopped,
            self.gx_store.sidebar_bulk.paced_legs_waited,
            lifecycle.reload_sets,
            lifecycle.reload_set_rows,
            lifecycle.reload_sets_stopped,
            // The More menu's sort rows, answered with the empty plan (gx_store/sidebar_open.rs).
            // They ride here rather than only on the per-command line, so a run in which the user
            // clicked neither still says so with both at zero.
            self.gx_store.sidebar_open.sort_rows,
            self.gx_store.sidebar_open.sort_rows_declined,
        ];
        let focus = self.gx_store_remote_focus_counters();
        // Delayed Send, the launcher's run and Hide Machine (gx_store/sidebar_state_actions.rs).
        let state = self.gx_store.sidebar_open.state;
        // The two account pages (gx_store/sidebar_accounts.rs).
        let accounts = self.gx_store.sidebar_accounts.counters;
        // The project slot hotkeys' jump (gx_store/sidebar_slot_jump.rs).
        let slot_jump = self.gx_store.slot_jump.counters;
        // The session slot hotkeys, cmd+1..9 (gx_store/sidebar_session_slot.rs).
        let session_slot = self.gx_store.session_slot.counters;
        // Close Project's successor (gx_store/sidebar_close_project.rs).
        let close_project = self.gx_store_close_project_counters();
        // A local row's click, performed by the store's focus route (gx_store/sidebar_focus_route.rs).
        let local_focus = self.gx_store_local_focus_route_counters();
        // The two documents of a REMOTE machine (gx_store/remote_project_docs.rs).
        let remote_docs = self.gx_store_remote_project_doc_counters();
        let host = &mut self.gx_store.sidebar_remote;
        let now = (host.counters, local);
        if host.summary_written
            == Some((
                now.0,
                now.1,
                focus,
                state,
                accounts,
                slot_jump,
                session_slot,
                close_project,
                local_focus,
                remote_docs,
            ))
            || host
                .summary_at
                .is_some_and(|at| at.elapsed() < REMOTE_SUMMARY_INTERVAL)
        {
            return;
        }
        host.summary_at = Some(Instant::now());
        if host.summary_records >= MAX_REMOTE_SUMMARY_RECORDS || !routine_logging_enabled() {
            return;
        }
        host.summary_records += 1;
        host.summary_written = Some((
            now.0,
            now.1,
            focus,
            state,
            accounts,
            slot_jump,
            session_slot,
            close_project,
            local_focus,
            remote_docs,
        ));
        let [
            reloads_stopped,
            paced_legs_waited,
            reload_sets,
            reload_set_rows,
            reload_sets_stopped,
            sort_rows,
            sort_rows_declined,
        ] = local;
        record(
            "gxStore.sidebarActions.summary",
            json!({
                "remote": remote_counters_json(&host.counters),
                "remoteFocus": super::sidebar_remote_focus::remote_focus_counters_json(&focus),
                "local": {
                    "reloadsStopped": reloads_stopped,
                    "pacedLegsWaited": paced_legs_waited,
                    "reloadSets": reload_sets,
                    "reloadSetRows": reload_set_rows,
                    "reloadSetsStopped": reload_sets_stopped,
                    "sortRows": sort_rows,
                    "sortRowsDeclined": sort_rows_declined,
                },
                "state": super::diagnostics_open::state_counters_json(&state),
                "accounts": super::sidebar_accounts::account_counters_json(&accounts),
                "slotJump": super::sidebar_slot_jump::slot_jump_counters_json(&slot_jump),
                "sessionSlot": super::sidebar_session_slot::session_slot_counters_json(&session_slot),
                "closeProject": super::sidebar_close_project::close_project_counters_json(&close_project),
                "localFocus": super::sidebar_focus_route::local_focus_route_counters_json(&local_focus),
                "remoteProjectDocs":
                    super::remote_project_docs::remote_project_doc_counters_json(&remote_docs),
            }),
        );
    }
}

/// The remote counters as both records carry them: 20 keys at depth 2 (inside `totals` on the
/// per-action line, inside `remote` on the summary), under the sanitizer's 32-entry cap and its
/// depth of 4.
fn remote_counters_json(counters: &SidebarRemoteCounters) -> Value {
    json!({
        "actions": counters.actions,
        "sleeps": counters.sleeps,
        "wakes": counters.wakes,
        "closes": counters.closes,
        "forks": counters.forks,
        "flags": counters.flags,
        "parksThenSleep": counters.parks_then_sleep,
        "snoozes": counters.snoozes,
        "unsnoozes": counters.unsnoozes,
        "reloads": counters.reloads,
        "legs": counters.legs,
        "awaitedLegs": counters.awaited_legs,
        "fireAndForgetLegs": counters.fire_and_forget_legs,
        "legsOk": counters.legs_ok,
        "legsFailed": counters.legs_failed,
        "stopped": counters.stopped,
        "failureToasts": counters.failure_toasts,
        "declinedSource": counters.declined_source,
        "declinedMachine": counters.declined_machine,
        "handedBack": counters.handed_back,
    })
}
