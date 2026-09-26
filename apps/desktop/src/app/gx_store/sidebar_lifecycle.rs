//! Sleep and wake, performed by the store instead of by the old runtime.
//!
//! CDXC:SessionSleep 2026-09-20 WHY:
//! The decision is gx-core's (`sidebar_actions/lifecycle.rs`) and this file is only the two edges
//! it cannot have: the daemon call and the workspace selection. The order is the whole point and
//! it is the TypeScript's order, not a convenience: the call goes first, the answer is read, and
//! only an ACCEPTED answer is allowed to change what the row shows. An optimistic value applied
//! before the call would show a session asleep that the daemon declined to sleep, which is the
//! bug the KeepAwake fix of 2026-08-19 removed from the TypeScript and which this port must not
//! reintroduce.
//!
//! The echo guard is the store's own overlay, not a timer and not a stamp: the patch records the
//! value it predicted FROM, so a daemon row that merely repeats that value leaves the prediction
//! in place, one that agrees retires it, and one that says anything else replaces it at once
//! (`StoredPatch::verdict`).
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_actions/lifecycle.rs.

use std::time::Duration;

use ghostex_gx_core::{
    ActiveGroup, CloseAnswer, CloseFollowUp, CloseRequest, Event, FocusOptions, ForkFollowUp,
    ForkRequest, Intent, LifecycleAnswer, LifecycleCall, LifecycleFollowUp, LifecycleRequest,
    SessionKey, apply_close_answer, apply_fork_answer, apply_lifecycle_answer,
    close_optimistic_follow_ups, owns_close_message, owns_fork_message, owns_lifecycle_message,
    plan_close_request, plan_fork_request, plan_lifecycle_request,
};
use serde_json::Value;

use super::host::now_ms;
use super::rpc::gxserver_rpc_result_task;
use crate::GhostexGpuiApp;
use crate::app::model::{
    GpuiPreferredAgentInterface, GpuiSidebarWorkspaceTerminalFocusMessage,
    GpuiWorkspaceTerminalFocusPlacement,
};

/// The same bound the app's other wake uses (`session_chat_fork_branches.rs`): a wake starts a
/// provider and can take a while, and a call that times out must read as "no answer" rather than
/// as a refusal.
const LIFECYCLE_RPC_TIMEOUT: Duration = Duration::from_secs(60);

/// The same bound for the sibling action files, so one number covers every sidebar call.
pub(super) fn rpc_timeout() -> Duration {
    LIFECYCLE_RPC_TIMEOUT
}

/// What this app run did with the lifecycle actions the store owns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SidebarLifecycleCounters {
    pub(crate) sleeps: u64,
    pub(crate) wakes: u64,
    pub(crate) accepted: u64,
    pub(crate) declined: u64,
    pub(crate) failed: u64,
    /// An accepted answer whose optimistic value the store had nothing to overlay, because the
    /// daemon row already said it.
    pub(crate) already_agreed: u64,
    pub(crate) focus_follow_ups: u64,
    /// Payloads the store owns but did not answer because the renderer is not drawing its list.
    pub(crate) declined_source: u64,
    pub(crate) closes: u64,
    /// Closes the daemon confirmed.
    pub(crate) closes_accepted: u64,
    /// Closes whose row came back because the daemon never confirmed. Every one of these is a row
    /// the TypeScript would have left missing for the rest of the run.
    pub(crate) closes_restored: u64,
    pub(crate) forks: u64,
    pub(crate) forks_placed: u64,
    pub(crate) forks_failed: u64,
    /// Forks whose SOURCE row sits in a user-made group, so the group was activated before the
    /// call, and of those the ones whose answer really moved the new session into that group. The
    /// second is lower than the first whenever the group was deleted while the call was in flight.
    pub(crate) forks_from_group: u64,
    pub(crate) forks_joined_group: u64,
    /// Full Reloads answered here, and the legs they ran.
    pub(crate) reloads: u64,
    pub(crate) reload_legs: u64,
    /// Full Reloads whose sleep call failed, so the wake was never asked for.
    pub(crate) reloads_stopped: u64,
    /// Set reloads (a project's Full Reload, a user-made group's Full Reload), the rows they
    /// planned, and the ones a failed row stopped before their end.
    pub(crate) reload_sets: u64,
    pub(crate) reload_set_rows: u64,
    pub(crate) reload_sets_stopped: u64,
    /// Wakes that asked the workspace to tear the dead terminal down first, which is the leg that
    /// makes a Full Reload a reload rather than a sleep and a wake.
    pub(crate) remounts: u64,
    /// Split Rights answered here, by what the row needed.
    pub(crate) splits: u64,
    pub(crate) splits_woken: u64,
    /// Selections that really asked for a new pane, whichever of the two branches asked.
    pub(crate) splits_placed: u64,
}

impl GhostexGpuiApp {
    /// Answers a `setSessionSleeping` command in Rust when the store owns it. Returns whether it
    /// did, in which case the command must NOT also be sent to the old runtime, which would call
    /// the daemon a second time.
    pub(crate) fn gx_store_run_sidebar_lifecycle(
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
        if !owns_lifecycle_message(message) {
            return false;
        }
        if !self.gx_store_sidebar_list_ready() {
            self.gx_store.sidebar_lifecycle.declined_source += 1;
            return false;
        }
        match self.gx_store_start_lifecycle(message, cx) {
            Some(task) => {
                task.detach();
                true
            }
            None => false,
        }
    }

    /// The same sleep or wake, as a task the caller can WAIT for, which resolves to the answer.
    ///
    /// Full Reload needs the sleep to have come home before the wake goes out, which is what the
    /// TypeScript's two awaits do and which issuing both at once would race, and it needs the
    /// ANSWER, because a sleep whose call failed stops the reload (`reload_continues_after`). The
    /// paced bulk sleep waits on it too. Rather than a second copy of the call, the single-session
    /// path hands back its task and the composition awaits it (`gx_store/sidebar_reload.rs`,
    /// `gx_store/sidebar_bulk.rs`). `None` means this path does not own the message, which is the
    /// same answer the boolean above reports.
    pub(super) fn gx_store_start_lifecycle(
        &mut self,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::Task<LifecycleAnswer>> {
        // A browser row, a remote row and the bulk payloads are refused inside the planner and
        // stay the old runtime's, each one for a reason written down there.
        let request = plan_lifecycle_request(&self.gx_store.core, message)?;
        match request.call {
            LifecycleCall::Sleep => self.gx_store.sidebar_lifecycle.sleeps += 1,
            LifecycleCall::Wake => self.gx_store.sidebar_lifecycle.wakes += 1,
        }
        // The Quick Automations row: the TypeScript returned before the call, so this is answered
        // and nothing happens. It resolves as accepted because the TypeScript's `await` of it
        // resolved: a reload of that row goes on to its wake, which returns early the same way.
        if request.rpc_path.is_empty() {
            self.gx_store.diagnostics.sidebar_lifecycle_ran(
                &request,
                "quick",
                0,
                self.gx_store.sidebar_lifecycle,
            );
            return Some(gpui::Task::ready(LifecycleAnswer::Accepted));
        }
        let path = request.rpc_path;
        let params = request.rpc_params.clone();
        let background = cx.background_executor().clone();
        Some(cx.spawn(async move |this, cx| {
            let started = web_time::Instant::now();
            let result =
                gxserver_rpc_result_task(&background, path, params, LIFECYCLE_RPC_TIMEOUT).await;
            let round_trip_ms = started.elapsed().as_millis() as u64;
            this.update(cx, |this, cx| {
                this.gx_store_apply_lifecycle_answer(&request, result, round_trip_ms, cx)
            })
            // The app is gone, so nothing waits on this answer any more.
            .unwrap_or(LifecycleAnswer::Failed)
        }))
    }

    /// Fork for a session's own chat or terminal controls, run as the same store fork its sidebar
    /// row's Fork runs. `false` when the store does not own that fork (a chat project's session, a
    /// row it does not hold yet); the caller used to hand those to the runtime, which was deleted on
    /// 2026-09-25.
    pub(crate) fn gx_store_run_workspace_session_fork(
        &mut self,
        project_id: &str,
        session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let session = SessionKey::local(project_id, session_id);
        self.gx_store_run_sidebar_fork(
            &serde_json::json!({
                "type": "command",
                "message": {
                    "type": "forkSession",
                    "sessionId": session.to_sidebar_session_id(),
                },
            }),
            cx,
        )
    }

    /// Answers a `forkSession` command in Rust when the store owns it.
    ///
    /// Nothing local happens until the daemon has made the session, so there is no optimistic
    /// half here and nothing to take back: the only thing that moves before the call is which
    /// group is active, and a failed fork leaves the user looking at the row they clicked
    /// (packages/gx-core/src/sidebar_actions/fork.rs).
    ///
    /// The source group is the ROW's, so a fork from inside a user-made group activates that group
    /// and the new session joins it once the daemon answers. That makes this a reader of the
    /// workspace session groups document, which is why the stored copy has to be in hand first,
    /// exactly as the order writes demand it.
    pub(crate) fn gx_store_run_sidebar_fork(
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
        if !owns_fork_message(message) {
            return false;
        }
        if !self.gx_store_sidebar_list_ready() {
            self.gx_store.sidebar_lifecycle.declined_source += 1;
            return false;
        }
        if !self.gx_store_restore_workspace_groups(cx) {
            self.gx_store.sidebar_lifecycle.declined_source += 1;
            return false;
        }
        let Some(request) = plan_fork_request(&self.gx_store.core, message) else {
            return false;
        };
        self.gx_store.sidebar_lifecycle.forks += 1;
        if let Some(group) = request.activate.clone() {
            // A chat project's fork is refused in the planner, so `Chats` cannot arrive; if it ever
            // did, the fork still goes out and only the activation is skipped, because the group
            // the store would activate is one its own list draws no row for.
            let intent = match group {
                ActiveGroup::Subgroup { project, group_id } => {
                    self.gx_store.sidebar_lifecycle.forks_from_group += 1;
                    Some(Intent::FocusSubgroup { project, group_id })
                }
                ActiveGroup::Project(project) => Some(Intent::FocusProject { project }),
                ActiveGroup::Chats(_) => None,
            };
            if let Some(intent) = intent {
                let output = self.gx_store.core.handle(Event::Intent(intent), now_ms());
                if !output.changes.is_empty() {
                    self.gx_store.sidebar_list.note_changes(&output.changes);
                    self.gx_store_update_sidebar_list(cx);
                }
            }
        }
        let path = request.rpc_path;
        let params = request.rpc_params.clone();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let started = web_time::Instant::now();
            let result =
                gxserver_rpc_result_task(&background, path, params, LIFECYCLE_RPC_TIMEOUT).await;
            let round_trip_ms = started.elapsed().as_millis() as u64;
            let _ = this.update(cx, |this, cx| {
                this.gx_store_apply_fork_answer(&request, result, round_trip_ms, cx);
            });
        })
        .detach();
        true
    }

    fn gx_store_apply_fork_answer(
        &mut self,
        request: &ForkRequest,
        result: Result<Value, String>,
        round_trip_ms: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        let follow_ups = {
            let document = self.gx_store.workspace_groups.sync.document();
            apply_fork_answer(document, request, result.as_ref().map_err(String::as_str))
        };
        let mut placed = false;
        for follow_up in follow_ups {
            match follow_up {
                ForkFollowUp::EditDocument { document } => {
                    self.gx_store.sidebar_lifecycle.forks_joined_group += 1;
                    self.gx_store_edit_workspace_groups(document, cx);
                }
                ForkFollowUp::PlacePane {
                    session,
                    placement_target,
                } => {
                    placed = true;
                    self.gx_store.sidebar_lifecycle.forks_placed += 1;
                    self.gx_store_place_local_workspace_session(
                        &session,
                        Some(&placement_target),
                        cx,
                    );
                }
                ForkFollowUp::Toast { level, title, .. } => {
                    self.gx_store.sidebar_lifecycle.forks_failed += 1;
                    // The description is the failure text, which is the daemon's or the
                    // transport's. It reaches the user and never a log line.
                    let description = result.as_ref().err().cloned().unwrap_or_else(|| {
                        "gxserver did not return the forked session.".to_string()
                    });
                    self.dispatch_gpui_app_modal_toast(level.as_str(), &title, &description, cx);
                }
            }
        }
        self.gx_store.diagnostics.sidebar_fork_ran(
            placed,
            round_trip_ms,
            self.gx_store.sidebar_lifecycle,
        );
    }

    /// Answers a `closeSession` command in Rust when the store owns it.
    ///
    /// The row goes at once, before the call, which is the TypeScript's order and the reason the
    /// click feels instant. What is NOT the TypeScript's is the answer: a call that does not come
    /// home puts the row back instead of leaving it missing (packages/gx-core/src/sidebar_actions/close.rs).
    pub(crate) fn gx_store_run_sidebar_close(
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
        if !owns_close_message(message) {
            return false;
        }
        if !self.gx_store_sidebar_list_ready() {
            self.gx_store.sidebar_lifecycle.declined_source += 1;
            return false;
        }
        let Some(request) = plan_close_request(&self.gx_store.core, message) else {
            return false;
        };
        self.gx_store.sidebar_lifecycle.closes += 1;
        self.gx_store_run_close_follow_ups(close_optimistic_follow_ups(&request), cx);
        let path = request.rpc_path;
        let params = request.rpc_params.clone();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let started = web_time::Instant::now();
            let result =
                gxserver_rpc_result_task(&background, path, params, LIFECYCLE_RPC_TIMEOUT).await;
            let elapsed = started.elapsed();
            // A call that spent the whole timeout and came back with an error never answered; the
            // distinction changes no follow-up and is kept because it is the one case the two
            // clients could not both reach (the TypeScript's `fetch` had no timeout at all).
            let answer = match (
                CloseAnswer::read(result.as_ref().map_err(String::as_str)),
                elapsed >= LIFECYCLE_RPC_TIMEOUT,
            ) {
                (CloseAnswer::Failed, true) => CloseAnswer::NeverAnswered,
                (answer, _) => answer,
            };
            let _ = this.update(cx, |this, cx| {
                this.gx_store_apply_close_answer(&request, answer, elapsed.as_millis() as u64, cx);
            });
        })
        .detach();
        true
    }

    fn gx_store_apply_close_answer(
        &mut self,
        request: &CloseRequest,
        answer: CloseAnswer,
        round_trip_ms: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        match answer {
            CloseAnswer::Accepted => self.gx_store.sidebar_lifecycle.closes_accepted += 1,
            _ => self.gx_store.sidebar_lifecycle.closes_restored += 1,
        }
        self.gx_store_run_close_follow_ups(apply_close_answer(request, answer), cx);
        self.gx_store.diagnostics.sidebar_close_ran(
            answer.as_str(),
            round_trip_ms,
            self.gx_store.sidebar_lifecycle,
        );
    }

    fn gx_store_run_close_follow_ups(
        &mut self,
        follow_ups: Vec<CloseFollowUp>,
        cx: &mut gpui::Context<Self>,
    ) {
        let now = now_ms();
        let mut moved = false;
        let mut focus_target: Option<SessionKey> = None;
        for follow_up in follow_ups {
            match follow_up {
                CloseFollowUp::Hide { session } => {
                    let output = self
                        .gx_store
                        .core
                        .handle(Event::Intent(Intent::HideSession { session }), now);
                    if !output.changes.is_empty() {
                        moved = true;
                        self.gx_store.sidebar_list.note_changes(&output.changes);
                    }
                }
                CloseFollowUp::Unhide { session } => {
                    let output = self
                        .gx_store
                        .core
                        .handle(Event::Intent(Intent::UnhideSession { session }), now);
                    if !output.changes.is_empty() {
                        moved = true;
                        self.gx_store.sidebar_list.note_changes(&output.changes);
                    }
                }
                CloseFollowUp::Focus { session } => {
                    self.gx_store.sidebar_lifecycle.focus_follow_ups += 1;
                    focus_target = Some(session);
                }
            }
        }
        if moved {
            self.gx_store_update_sidebar_list(cx);
        }
        if let Some(session) = focus_target {
            self.gx_store_focus_local_workspace_session(&session, cx);
        }
    }

    /// The half after the round trip. Runs on the UI thread, so the focus it reads is the focus
    /// the user has now and not the one the call left with.
    fn gx_store_apply_lifecycle_answer(
        &mut self,
        request: &LifecycleRequest,
        result: Result<Value, String>,
        round_trip_ms: u64,
        cx: &mut gpui::Context<Self>,
    ) -> LifecycleAnswer {
        let answer = LifecycleAnswer::read(result.as_ref().map_err(String::as_str));
        match answer {
            LifecycleAnswer::Accepted => self.gx_store.sidebar_lifecycle.accepted += 1,
            LifecycleAnswer::Declined => self.gx_store.sidebar_lifecycle.declined += 1,
            LifecycleAnswer::Failed => self.gx_store.sidebar_lifecycle.failed += 1,
        }
        let now = now_ms();
        let focused_now = self.gx_store.core.focus().focused_session.clone();
        let follow_ups = apply_lifecycle_answer(request, answer, focused_now.as_ref(), now);
        let mut moved = false;
        let mut focus_target: Option<(SessionKey, FocusOptions)> = None;
        for follow_up in follow_ups {
            match follow_up {
                LifecycleFollowUp::Patch { session, patch } => {
                    let output = self.gx_store.core.handle(
                        Event::Intent(Intent::PatchSession {
                            session: session.clone(),
                            patch,
                        }),
                        now,
                    );
                    if output.changes.is_empty() {
                        // The daemon row already says what the call asked for, so the overlay had
                        // nothing to do and was not stored. Not an error, and worth counting: a
                        // run where every accepted answer lands here means the daemon is ahead of
                        // us and the optimistic value is never on screen at all.
                        self.gx_store.sidebar_lifecycle.already_agreed += 1;
                    } else {
                        moved = true;
                        self.gx_store.sidebar_list.note_changes(&output.changes);
                    }
                }
                LifecycleFollowUp::Focus { session, options } => {
                    self.gx_store.sidebar_lifecycle.focus_follow_ups += 1;
                    if options.force_remount {
                        self.gx_store.sidebar_lifecycle.remounts += 1;
                    }
                    if options.split_right {
                        self.gx_store.sidebar_lifecycle.splits_placed += 1;
                    }
                    focus_target = Some((session, options));
                }
            }
        }
        if moved {
            self.gx_store_update_sidebar_list(cx);
        }
        if let Some((session, options)) = focus_target {
            self.gx_store_select_local_workspace_session(&session, None, options, cx);
        }
        self.gx_store.diagnostics.sidebar_lifecycle_ran(
            request,
            answer.as_str(),
            round_trip_ms,
            self.gx_store.sidebar_lifecycle,
        );
        answer
    }

    /// `focusLocalWorkspaceSession` with no options, which is what most of these paths send: the
    /// ordinary selection, never a remount and never a wake intent, because whatever wake was
    /// needed has already happened.
    fn gx_store_focus_local_workspace_session(
        &mut self,
        session: &SessionKey,
        cx: &mut gpui::Context<Self>,
    ) {
        self.gx_store_select_local_workspace_session(session, None, FocusOptions::default(), cx);
    }

    /// The same selection with a placement target, which is what a fork sends: the new pane is
    /// appended beside the row it was forked from rather than beside whichever pane happens to be
    /// focused when the call comes back.
    ///
    /// CDXC:SessionFork 2026-09-24 DECISION:
    /// User: forking a session from its sidebar row's Fork or from the chat's More actions > Fork switches to the forked session.
    /// Its attach completes as `GpuiLocalWorkspaceAttachOrigin::Fork`, which lands the fork unless the user selected something else meanwhile. It must not use the ordinary sidebar-focus check that the published focus names the session: the store cannot take the fork as a local selection while its row has not arrived, so a routine publish in between (still naming the source) dropped the ready attach with the source left on screen. Setting that copy by hand before the attach was tried and lost the same race. A fork from another project's row takes that project's workspace first, as a row click does.
    fn gx_store_place_local_workspace_session(
        &mut self,
        session: &SessionKey,
        placement_target: Option<&SessionKey>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.swap_agents_workspace_to_project_id(Some(session.project_id.clone()), cx);
        // The store's focus takes the fork (held until its row arrives), so no publish before the
        // attach returns pulls the workspace back to the source's project (focus_publish.rs).
        self.gx_store_select_opened_session(session, cx);
        self.gx_store_select_local_workspace_session(
            session,
            placement_target,
            FocusOptions::default(),
            cx,
        );
    }

    /// The one workspace selection every sidebar action goes through.
    ///
    /// The two options are the caller's, not the row's: Full Reload's second leg asks for the
    /// remount that tears down the terminal its own sleep killed, and Split Right asks for the new
    /// pane. They are carried as data from gx-core rather than decided here, so the gate can
    /// compare which leg asked for what.
    pub(super) fn gx_store_select_local_workspace_session(
        &mut self,
        session: &SessionKey,
        placement_target: Option<&SessionKey>,
        options: FocusOptions,
        cx: &mut gpui::Context<Self>,
    ) {
        self.focus_local_workspace_terminal_from_message(
            &GpuiSidebarWorkspaceTerminalFocusMessage {
                force_remount: options.force_remount,
                placement: match options.split_right {
                    true => GpuiWorkspaceTerminalFocusPlacement::SplitRight,
                    false => GpuiWorkspaceTerminalFocusPlacement::Tab,
                },
                placement_target_session_id: placement_target
                    .map(|target| target.session_id.clone()),
                preferred_interface: GpuiPreferredAgentInterface::Terminal,
                project_id: session.project_id.clone(),
                session_id: session.session_id.clone(),
                startup_restore: false,
                keep_view: false,
                wake_sleeping: false,
                keep_sleeping: false,
            },
            cx,
        );
    }
}
