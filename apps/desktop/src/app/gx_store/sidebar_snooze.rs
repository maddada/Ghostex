//! Snooze and unsnooze, performed by the store instead of by the old runtime.
//!
//! CDXC:Sessions 2026-09-20 WHY:
//! The decision is gx-core's (`sidebar_actions/snooze.rs`); this file is the three edges it cannot
//! have. Two are the usual ones, the daemon call and the toast. The third is the CLOCK, and it is
//! the reason this action is different from every other one in the surface: "Tomorrow" and "Next
//! week" mean 09:00 local time on a later day, so the host reads the instant AND the local UTC
//! offset that will be in force at each candidate morning, and hands both over as data. Reading
//! only today's offset would build a wake time an hour out for any snooze that crosses a
//! daylight-saving change, which is exactly the kind of once-in-six-months wrongness nobody would
//! ever trace back to a port.
//!
//! The menu row posts rather than calls: it resolves the preset and sends the same two ordinary
//! commands the renderer sends, the tag first and the snooze second, back through the one
//! dispatcher. So the tag reaches the flags path that already owns it and the snooze reaches the
//! call below, and neither grows a second copy here.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_actions/snooze.rs,
//! tooling/gx-core/sidebar-page-frozen/session-actions.ts (`runNativeSessionAction`),
//! apps/desktop/sidebar/gxserver-runtime/sessions-and-focus.ts (`snoozeSession`,
//! `runSessionLifecycleCommand`).

use chrono::{Days, Local, NaiveTime, TimeZone as _};
use ghostex_gx_core::{
    SnoozeCall, SnoozeClock, SnoozeFollowUp, SnoozeRequest, apply_snooze_answer,
    owns_snooze_action, owns_snooze_message, plan_snooze_action, plan_snooze_request,
};
use serde_json::{Value, json};

use super::rpc::gxserver_rpc_result_task;
use crate::GhostexGpuiApp;

/// What this app run did with the snooze actions the store owns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SidebarSnoozeCounters {
    /// Menu rows answered (`sessionAction: snooze`).
    pub(crate) actions: u64,
    /// Of those, the ones that also set a tag first.
    pub(crate) actions_with_tag: u64,
    /// A menu row whose answer is deliberately nothing: no tag and no preset.
    pub(crate) actions_empty: u64,
    pub(crate) snoozes: u64,
    pub(crate) unsnoozes: u64,
    pub(crate) accepted: u64,
    pub(crate) failed: u64,
    /// Accepted snoozes that went on to ask for a sleep.
    pub(crate) sleeps: u64,
    /// Payloads the store owns but did not answer because the renderer is not drawing its list.
    pub(crate) declined_source: u64,
    /// The menu row named a session the drawn list does not hold, so the old runtime answers. A
    /// context menu can only be opened on a drawn row, so this should stay at zero.
    pub(crate) declined_row: u64,
}

impl GhostexGpuiApp {
    /// Answers the `sessionAction: snooze` menu row when the store owns it.
    ///
    /// This is a RENDERER command and arrives at the top level, not wrapped in
    /// `{ type: 'command', message }` (controller.ts:160 against controller.ts:131). Unwrapping it
    /// finds nothing and the whole path silently does nothing, which is what had happened to the
    /// rename and note dialogs.
    pub(crate) fn gx_store_run_sidebar_snooze_action(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !owns_snooze_action(command) {
            return false;
        }
        if !self.gx_store_sidebar_list_ready() {
            self.gx_store.sidebar_snooze.declined_source += 1;
            return false;
        }
        let clock = gx_store_snooze_clock();
        let Some(action) = plan_snooze_action(self.gx_store.sidebar_list.view(), command, &clock)
        else {
            self.gx_store.sidebar_snooze.declined_row += 1;
            return false;
        };
        self.gx_store.sidebar_snooze.actions += 1;
        // Asked of the messages by name rather than by counting them: a command can carry a tag
        // and no preset, which is one message and is still a tag.
        if action
            .messages
            .iter()
            .any(|message| message.get("type") == Some(&Value::from("setSessionTag")))
        {
            self.gx_store.sidebar_snooze.actions_with_tag += 1;
        }
        if action.messages.is_empty() {
            self.gx_store.sidebar_snooze.actions_empty += 1;
        }
        let messages = action.messages.len();
        // The same `post` the renderer makes: each message goes back through the one dispatcher, so
        // the tag lands on the flags path and the snooze on the call below, and one the store does
        // not own still reaches the old runtime.
        for message in action.messages {
            self.dispatch_native_sidebar_command(message, cx);
        }
        self.gx_store
            .diagnostics
            .sidebar_snooze_action_ran(messages, self.gx_store.sidebar_snooze);
        true
    }

    /// Answers `snoozeSession` and `unsnoozeSession` when the store owns them.
    pub(crate) fn gx_store_run_sidebar_snooze(
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
        if !owns_snooze_message(message) {
            return false;
        }
        if !self.gx_store_sidebar_list_ready() {
            self.gx_store.sidebar_snooze.declined_source += 1;
            return false;
        }
        // A browser row and a remote row are refused inside the planner, each for a reason written
        // down there.
        let Some(request) = plan_snooze_request(message) else {
            return false;
        };
        match request.call {
            SnoozeCall::Snooze => self.gx_store.sidebar_snooze.snoozes += 1,
            SnoozeCall::Unsnooze => self.gx_store.sidebar_snooze.unsnoozes += 1,
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
                this.gx_store_apply_snooze_answer(&request, result.is_ok(), round_trip_ms, cx);
            });
        })
        .detach();
        true
    }

    fn gx_store_apply_snooze_answer(
        &mut self,
        request: &SnoozeRequest,
        accepted: bool,
        round_trip_ms: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        match accepted {
            true => self.gx_store.sidebar_snooze.accepted += 1,
            false => self.gx_store.sidebar_snooze.failed += 1,
        }
        for follow_up in apply_snooze_answer(request, accepted) {
            match follow_up {
                SnoozeFollowUp::Sleep { session } => {
                    self.gx_store.sidebar_snooze.sleeps += 1;
                    // The ordinary sleep, through the path that owns its call, its declined leg and
                    // its overlay. A snoozed session is always asleep, and the sleep follows the
                    // snooze so a refused one leaves the session running.
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
                SnoozeFollowUp::Toast {
                    level,
                    title,
                    description,
                } => {
                    self.dispatch_gpui_app_modal_toast(level.as_str(), title, description, cx);
                }
            }
        }
        self.gx_store.diagnostics.sidebar_snooze_ran(
            request.call.as_str(),
            accepted,
            round_trip_ms,
            self.gx_store.sidebar_snooze,
        );
    }
}

/// The clock facts the wake-time rule needs, read once per click.
///
/// `morning_offset_ms[d]` is the offset at 09:00 local `d` local days from today, which is the one
/// JavaScript's `setHours(9, 0, 0, 0)` would use for that target. A local 09:00 that does not
/// exist (a daylight-saving gap that swallows it, which no mainstream zone has) keeps today's
/// offset rather than inventing one.
fn gx_store_snooze_clock() -> SnoozeClock {
    let now = Local::now();
    let offset_ms = i64::from(now.offset().local_minus_utc()) * 1_000;
    let mut clock = SnoozeClock::fixed(now.timestamp_millis(), offset_ms);
    let Some(morning) = NaiveTime::from_hms_opt(9, 0, 0) else {
        return clock;
    };
    for (days, slot) in clock.morning_offset_ms.iter_mut().enumerate() {
        let Some(date) = now.date_naive().checked_add_days(Days::new(days as u64)) else {
            continue;
        };
        if let Some(instant) = Local
            .from_local_datetime(&date.and_time(morning))
            .earliest()
        {
            *slot = i64::from(instant.offset().local_minus_utc()) * 1_000;
        }
    }
    clock
}
