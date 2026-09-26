//! The bulk, project and collection actions, fanned out by the store.
//!
//! CDXC:Sessions 2026-09-20 WHY:
//! Nothing here calls the daemon. Every one of these payloads ends in the per-session actions the
//! store already owns, so the host's whole job is to post the messages gx-core resolved back
//! through the one dispatcher, at the interval it named. That is deliberate: a second copy of the
//! declined leg, the replacement focus or the echo guard is exactly what this port keeps failing to
//! afford, and a bulk sleep is a hundred chances to get one of them subtly different.
//!
//! The pacing is the one thing this file really performs. A bulk sleep goes out one request at a
//! time, 350 ms apart, on a spawned task; wake and close go out together. A message the store does
//! not own still reaches the old runtime through the same dispatcher, so a batch that is half ours
//! runs each half where it belongs.
//!
//! **The counters that prove this path fires in the app** are `batches`, `batchMessages`,
//! `bulkRequests` and `bulkMessages` on `gxStore.sidebarBulk`. A run where the user used Sleep
//! Selected, a collection menu or a project's Wake and any of them is zero means the command never
//! reached here, which is the failure the envelope bug of piece 3d was.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_actions/bulk.rs,
//! the deleted sidebar page's `controller.ts` (the `batch` arm).

use std::time::Duration;

use ghostex_gx_core::{
    Event, Intent, SidebarUiIntent, owns_batch_command, owns_bulk_message,
    owns_remote_session_message, plan_batch, plan_bulk_request,
};
use serde_json::Value;

use super::host::now_ms;
use crate::GhostexGpuiApp;

/// What this app run did with the plural actions the store owns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SidebarBulkCounters {
    /// `batch` envelopes answered.
    pub(crate) batches: u64,
    /// Messages those batches posted.
    pub(crate) batch_messages: u64,
    /// Batches that cleared the multi-selection first.
    pub(crate) batches_clearing: u64,
    /// Plural payloads answered.
    pub(crate) bulk_requests: u64,
    /// Messages those payloads fanned out into.
    pub(crate) bulk_messages: u64,
    /// Of those, the ones that went out paced rather than together.
    pub(crate) paced_requests: u64,
    /// A resolved set with nothing in it, which is a real answer: every row of the project is
    /// already in the state the action asks for.
    pub(crate) empty_requests: u64,
    /// Of those, the ones whose project is on a REMOTE machine, so every leg goes down that
    /// machine's tunnel. A run in which the user used a remote project's Sleep, Wake, Sleep
    /// Inactive or Close Inactive and this is zero means the payload was handed back instead.
    pub(crate) remote_requests: u64,
    /// Payloads the store owns but did not answer because the renderer is not drawing its list.
    pub(crate) declined_source: u64,
    /// Paced legs whose answer the next one waited for (`BulkRequest::waits_for_each`).
    pub(crate) paced_legs_waited: u64,
}

impl GhostexGpuiApp {
    /// Answers the renderer's `batch` envelope: clear the multi-selection, then post each message.
    ///
    /// This is a RENDERER command at the top level, like `sessionAction`, and not a wrapped
    /// runtime message (controller.ts:165).
    pub(crate) fn gx_store_run_sidebar_batch(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !owns_batch_command(command) {
            return false;
        }
        if !self.gx_store_sidebar_list_ready() {
            self.gx_store.sidebar_bulk.declined_source += 1;
            return false;
        }
        let Some(plan) = plan_batch(command) else {
            return false;
        };
        self.gx_store.sidebar_bulk.batches += 1;
        self.gx_store.sidebar_bulk.batch_messages += plan.messages.len() as u64;
        if plan.clear_selection {
            self.gx_store.sidebar_bulk.batches_clearing += 1;
            // `ui.selectedSessionIds = []`, through the intent that owns the multi-selection.
            self.gx_store_apply_sidebar_ui_intent(
                SidebarUiIntent::SetSelectedSessions {
                    session_ids: Vec::new(),
                },
                cx,
            );
        }
        let count = plan.messages.len();
        for message in plan.messages {
            self.dispatch_native_sidebar_command(message, cx);
        }
        self.gx_store.diagnostics.sidebar_batch_ran(
            count,
            plan.clear_selection,
            self.gx_store.sidebar_bulk,
        );
        true
    }

    /// Answers the six plural payloads when the store owns them.
    pub(crate) fn gx_store_run_sidebar_bulk(
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
        if !owns_bulk_message(message) {
            return false;
        }
        if !self.gx_store_sidebar_list_ready() {
            self.gx_store.sidebar_bulk.declined_source += 1;
            return false;
        }
        // A user-made session group and a machine whose rows the store has not loaded are refused
        // inside the planner, each for a reason written down there. A REMOTE project group is
        // answered: its legs are the same per-session messages, carrying that machine's scoped
        // ids, and the dispatcher below routes each one down that machine's tunnel. A project's app
        // tabs are not part of these payloads any more (`CDXC:SessionSleep 2026-09-21 DECISION` on
        // `plan_bulk_request`), so nothing here reads or needs the app-tab list.
        let planned = plan_bulk_request(&self.gx_store.core, message);
        let Some(request) = planned else {
            return false;
        };
        self.gx_store.sidebar_bulk.bulk_requests += 1;
        self.gx_store.sidebar_bulk.bulk_messages += request.messages.len() as u64;
        if request.is_remote_project() {
            self.gx_store.sidebar_bulk.remote_requests += 1;
        }
        if request.messages.is_empty() {
            self.gx_store.sidebar_bulk.empty_requests += 1;
        }
        // `focusProjectId` runs BEFORE the fan-out, and only a project wake asks for it.
        if let Some(project) = request.focus_project.clone() {
            let output = self
                .gx_store
                .core
                .handle(Event::Intent(Intent::FocusProject { project }), now_ms());
            if !output.changes.is_empty() {
                self.gx_store.sidebar_list.note_changes(&output.changes);
                self.gx_store_update_sidebar_list(cx);
            }
        }
        self.gx_store
            .diagnostics
            .sidebar_bulk_ran(&request, self.gx_store.sidebar_bulk);
        if request.interval_ms == 0 {
            for message in request.messages {
                self.dispatch_native_sidebar_command(message, cx);
            }
            return true;
        }
        self.gx_store.sidebar_bulk.paced_requests += 1;
        // One request at a time with the interval between them, which is what
        // `runGpuiSidebarBulkSleepPaced` does: it AWAITS each sleep and only then waits the
        // interval, so a request is never sent while the one before it is still in flight. The
        // wait is between requests and not before the first, so a single-row sleep is as fast as a
        // plain one.
        let interval = Duration::from_millis(request.interval_ms);
        let waits_for_each = request.waits_for_each();
        let messages = request.messages;
        cx.spawn(async move |this, cx| {
            for (index, message) in messages.into_iter().enumerate() {
                if index > 0 {
                    cx.background_executor().timer(interval).await;
                }
                let started = this.update(cx, |this, cx| {
                    this.gx_store_start_paced_message(message, waits_for_each, cx)
                });
                match started {
                    Err(_) => return,
                    Ok(Some(PacedLeg::Local(task))) => {
                        task.await;
                    }
                    Ok(Some(PacedLeg::Remote(task))) => {
                        task.await;
                    }
                    Ok(None) => {}
                }
            }
        })
        .detach();
        true
    }

    /// Sends one message of a paced fan-out and hands back what to wait for.
    ///
    /// The route is the dispatcher's: a remote row's sleep goes down its machine's tunnel, a local
    /// row's through the single-session path, and anything the store does not own to the old
    /// runtime, which is the one leg nothing here can wait for (that runtime runs it on its own).
    fn gx_store_start_paced_message(
        &mut self,
        message: Value,
        waits_for_each: bool,
        cx: &mut gpui::Context<Self>,
    ) -> Option<PacedLeg> {
        if waits_for_each && self.gx_store_sidebar_list_ready() {
            if owns_remote_session_message(&message) {
                if let Some(task) = self.gx_store_start_remote(&message, cx) {
                    self.gx_store.sidebar_bulk.paced_legs_waited += 1;
                    return Some(PacedLeg::Remote(task));
                }
            } else if let Some(task) = self.gx_store_start_lifecycle(&message, cx) {
                self.gx_store.sidebar_bulk.paced_legs_waited += 1;
                return Some(PacedLeg::Local(task));
            }
        }
        self.dispatch_native_sidebar_command(message, cx);
        None
    }
}

/// What a paced leg hands back to wait on.
enum PacedLeg {
    Local(gpui::Task<ghostex_gx_core::LifecycleAnswer>),
    Remote(gpui::Task<bool>),
}
