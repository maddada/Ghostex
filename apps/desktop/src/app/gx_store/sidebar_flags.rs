//! Pin, park, tag and favorite: one call for four sidebar commands.
//!
//! CDXC:Sessions 2026-09-20 WHY:
//! The decision is gx-core's (`sidebar_actions/flags.rs`) and this file is the call and the two
//! things that follow it. Nothing happens before the call, which was checked rather than assumed:
//! every caller of `updateSessionFlags` was found, and the only local-first session writes in the
//! codebase belonged to the React sidebar and never reached the native path. A call that fails
//! leaves the row exactly as the daemon has it, and shows nothing, which is what the TypeScript did
//! when its promise rejected.
//!
//! Parking can also sleep, and that sleep is NOT reimplemented here: it is handed to the lifecycle
//! path, which already owns its call, its three answers and its echo guard.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_actions/flags.rs.

use ghostex_gx_core::{
    Event, FLAGS_MESSAGE_TYPES, FlagsFollowUp, FlagsRequest, Intent, apply_flags_answer,
    owns_flags_message, plan_flags_request,
};
use serde_json::{Value, json};

use super::host::now_ms;
use super::rpc::gxserver_rpc_result_task;
use crate::GhostexGpuiApp;

/// What this app run did with the flag actions the store owns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SidebarFlagsCounters {
    pub(crate) calls: u64,
    pub(crate) accepted: u64,
    pub(crate) failed: u64,
    /// Accepted calls whose optimistic value the overlay had nothing to do with, because the
    /// daemon row already said it.
    pub(crate) already_agreed: u64,
    /// Parks that also asked for a sleep.
    pub(crate) parks_that_sleep: u64,
    pub(crate) declined_source: u64,
}

impl GhostexGpuiApp {
    /// Answers `setSessionPinned`, `setSessionParked`, `setSessionTag` and `setSessionFavorite`
    /// when the store owns them.
    pub(crate) fn gx_store_run_sidebar_flags(
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
        if !owns_flags_message(message) {
            return false;
        }
        if !self.gx_store_sidebar_list_ready() {
            if message
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| FLAGS_MESSAGE_TYPES.contains(&kind))
            {
                self.gx_store.sidebar_flags.declined_source += 1;
            }
            return false;
        }
        let Some(request) = plan_flags_request(message, self.gx_store_sleep_session_when_parking())
        else {
            return false;
        };
        self.gx_store.sidebar_flags.calls += 1;
        if request.then_sleep {
            self.gx_store.sidebar_flags.parks_that_sleep += 1;
        }
        let path = request.rpc_path;
        let params = request.rpc_params.clone();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let started = web_time::Instant::now();
            let result = gxserver_rpc_result_task(
                &background,
                path,
                params,
                super::sidebar_lifecycle::rpc_timeout(),
            )
            .await;
            let round_trip_ms = started.elapsed().as_millis() as u64;
            let _ = this.update(cx, |this, cx| {
                this.gx_store_apply_flags_answer(&request, result.is_ok(), round_trip_ms, cx);
            });
        })
        .detach();
        true
    }

    fn gx_store_apply_flags_answer(
        &mut self,
        request: &FlagsRequest,
        accepted: bool,
        round_trip_ms: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        match accepted {
            true => self.gx_store.sidebar_flags.accepted += 1,
            false => self.gx_store.sidebar_flags.failed += 1,
        }
        let now = now_ms();
        let mut moved = false;
        let mut sleep: Option<ghostex_gx_core::SessionKey> = None;
        for follow_up in apply_flags_answer(request, accepted, now) {
            match follow_up {
                FlagsFollowUp::Patch { session, patch } => {
                    let output = self
                        .gx_store
                        .core
                        .handle(Event::Intent(Intent::PatchSession { session, patch }), now);
                    if output.changes.is_empty() {
                        self.gx_store.sidebar_flags.already_agreed += 1;
                    } else {
                        moved = true;
                        self.gx_store.sidebar_list.note_changes(&output.changes);
                    }
                }
                FlagsFollowUp::Sleep { session } => sleep = Some(session),
            }
        }
        if moved {
            self.gx_store_update_sidebar_list(cx);
        }
        if let Some(session) = sleep {
            // The ordinary sleep, through the path that owns it: its own call, its own declined
            // leg, its own overlay. Parking must not grow a second copy of any of that.
            self.gx_store_run_sidebar_lifecycle(
                &json!({
                    "type": "command",
                    "message": {
                        "type": "setSessionSleeping",
                        "sessionId": session.to_sidebar_session_id(),
                        "sleeping": true,
                    },
                }),
                cx,
            );
        }
        self.gx_store.diagnostics.sidebar_flags_ran(
            accepted,
            round_trip_ms,
            self.gx_store.sidebar_flags,
        );
    }

    /// `sleepSessionWhenParking`, which decides whether parking also sleeps. Default false, the
    /// same default the shared settings carry. The remote park reads it through here too.
    pub(super) fn gx_store_sleep_session_when_parking(&self) -> bool {
        crate::shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .get("sleepSessionWhenParking")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }
}
