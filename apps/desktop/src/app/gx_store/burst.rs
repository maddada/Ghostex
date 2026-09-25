//! What follows a local selection once the user stops moving: the workspace is published and the
//! selection's follow-up runs once, and the heavy work of showing a tab runs for the tab the user
//! landed on.

use std::time::{Duration, Instant};

use futures::channel::mpsc;
use futures::{FutureExt as _, StreamExt as _};

use ghostex_gx_core::protocol::LifecycleState;

use super::focus_perform::RowFocusOptions;
use crate::GhostexGpuiApp;
use crate::app::model::{
    GpuiLocalWorkspaceSessionKey, GpuiPreferredAgentInterface,
    GpuiSidebarWorkspaceTerminalFocusMessage, GpuiWorkspaceTerminalFocusPlacement,
};
use crate::support_logs;

/// A selection's follow-up runs this long after the last local selection or attention acknowledge.
const FINISH_DELAY: Duration = Duration::from_millis(120);
/// Heavy work waits until the selection has not moved for this long.
const SETTLE_DELAY: Duration = Duration::from_millis(80);
/// A selection this long after the previous one starts a new gesture and is shown in full at
/// once. Key repeat runs at 30 to 60 milliseconds, far inside it; a click or a single key press
/// is outside it.
const BURST_GAP: Duration = Duration::from_millis(250);

impl GhostexGpuiApp {
    /// Whether heavy per-selection work is held back right now: mounting and attaching a terminal
    /// viewer, releasing the viewers of tabs that went out of view, creating or reconciling chat
    /// surfaces, and reporting the displayed sessions. Terminals that are already mounted and chat
    /// views that already exist are drawn regardless.
    pub(crate) fn gx_store_selection_is_settling(&self) -> bool {
        self.gx_store.local_focus.settle_due.is_some()
    }

    /// Books a local selection into the burst clock and makes sure the task that ends the burst
    /// runs. `moved` is false when the same session was selected again (an attach completion
    /// repeating the click's selection), which never starts or prolongs a settle.
    pub(super) fn gx_store_note_local_selection(
        &mut self,
        moved: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let now = Instant::now();
        let local_focus = &mut self.gx_store.local_focus;
        if moved {
            // A key repeat is inside a burst by definition: only the first press of a hold is an
            // isolated selection, whatever the platform's delay before the first repeat.
            let in_burst = local_focus.key_held
                || local_focus
                    .last_selection_at
                    .is_some_and(|previous| now.duration_since(previous) < BURST_GAP);
            local_focus.last_selection_at = Some(now);
            if in_burst {
                local_focus.burst_steps += 1;
                local_focus.settle_due = Some(now + SETTLE_DELAY);
            } else if local_focus.settle_due.is_none() {
                local_focus.burst_steps = 1;
            }
        }
        local_focus.finish_due = Some(now + FINISH_DELAY);
        self.gx_store_burst_deadline_booked(cx);
    }

    /// A frame arrived while the selection is still moving: the workspace publish it asks for runs
    /// with the selection's finish rather than per frame.
    pub(super) fn gx_store_book_selection_finish(&mut self, cx: &mut gpui::Context<Self>) {
        let local_focus = &mut self.gx_store.local_focus;
        if local_focus.finish_due.is_none() {
            local_focus.finish_due = Some(Instant::now() + FINISH_DELAY);
        }
        self.gx_store_burst_deadline_booked(cx);
    }

    /// The attention of a session the user is looking at is acknowledged when the selection
    /// finishes.
    pub(crate) fn gx_store_queue_attention_acknowledge(
        &mut self,
        key: GpuiLocalWorkspaceSessionKey,
        cx: &mut gpui::Context<Self>,
    ) {
        let local_focus = &mut self.gx_store.local_focus;
        if !local_focus.pending_attention.contains(&key) {
            local_focus.pending_attention.push(key);
        }
        if local_focus.finish_due.is_none() {
            local_focus.finish_due = Some(Instant::now() + FINISH_DELAY);
        }
        self.gx_store_burst_deadline_booked(cx);
    }

    /// The Browser tab a held key landed on gets its CEF surface when the selection settles.
    pub(crate) fn gx_store_defer_browser_surface(&mut self, cx: &mut gpui::Context<Self>) {
        let local_focus = &mut self.gx_store.local_focus;
        local_focus.browser_surface_wanted = true;
        local_focus.settle_due = Some(Instant::now() + SETTLE_DELAY);
        self.gx_store_burst_deadline_booked(cx);
    }

    /// Chat surfaces are reconciled when the selection settles.
    pub(crate) fn gx_store_defer_chat_reconcile(&mut self) {
        self.gx_store.local_focus.chat_reconcile_wanted = true;
    }

    /// A deadline (settle or finish) was booked or moved: make sure the one task that serves them
    /// runs, and that it is not asleep past the new deadline. Every place that sets one of the
    /// deadlines calls this afterwards.
    ///
    /// CDXC:FocusRouting 2026-09-19 WHY:
    /// The task used to sleep until the nearest deadline it knew when it went to sleep. Once a later deadline was booked ahead (the retired runtime tell's 1.5 s check), a settle (80 ms) or finish (120 ms) booked while the task slept towards it was served only when it woke: a landing tab stayed unmounted and its terminal parked for up to 1.2 s after a short hold.
    /// Why every booking is now served on time. The task records the instant it sleeps towards (`burst_sleeping_until`, `None` while it is awake or not running) and sleeps on "timer or wake message, whichever comes first". (1) When it goes to sleep, that instant is the minimum of all three deadlines, so none is earlier. (2) A booking made while it sleeps runs this function: a deadline earlier than the recorded instant sends a wake message, the task wakes on its next executor turn, runs what is due and recomputes the minimum; a deadline at or after the recorded instant needs nothing, because the task wakes at the recorded instant, before it, and recomputes then. (3) A deadline that is only pushed later or cleared never needs a wake: waking early is harmless, the turn finds nothing due and sleeps again. (4) A booking made while the task is awake can only happen inside the task's own turn (one UI thread), before that turn computes the minimum, so it is included; a wake message it may have sent costs one empty turn. (5) With no task running, this function starts one, whose first turn computes the minimum immediately. So a deadline `D` booked at any moment is served at `D` plus executor latency and the 1 ms floor below.
    /// It cannot spin: a turn either clears a due deadline or sleeps, a wake message is sent at most once per booking, and bookings come from user input or a frame during a burst. It ends when both deadlines are clear or the app entity is gone.
    fn gx_store_burst_deadline_booked(&mut self, cx: &mut gpui::Context<Self>) {
        let local_focus = &mut self.gx_store.local_focus;
        if local_focus.burst_task_running {
            let earliest = [local_focus.settle_due, local_focus.finish_due]
            .into_iter()
            .flatten()
            .min();
            if let (Some(earliest), Some(sleeping_until), Some(wake)) = (
                earliest,
                local_focus.burst_sleeping_until,
                local_focus.burst_wake.as_ref(),
            ) && earliest < sleeping_until
            {
                // The turn this wakes records a new instant; until then further bookings need no
                // second message.
                local_focus.burst_sleeping_until = None;
                let _ = wake.unbounded_send(());
            }
            return;
        }
        local_focus.burst_task_running = true;
        let (wake, mut wakes) = mpsc::unbounded::<()>();
        local_focus.burst_wake = Some(wake);
        cx.spawn(async move |this, cx| {
            loop {
                let wait = this.update(cx, |this, cx| this.gx_store_run_due_burst_work(cx));
                let Ok(Some(wait)) = wait else {
                    return;
                };
                let mut timer = cx.background_executor().timer(wait).fuse();
                let mut woken = wakes.next().fuse();
                futures::select_biased! {
                    _ = woken => {}
                    _ = timer => {}
                }
            }
        })
        .detach();
    }

    /// Performs what is due and returns how long to sleep, or `None` when nothing is pending.
    fn gx_store_run_due_burst_work(&mut self, cx: &mut gpui::Context<Self>) -> Option<Duration> {
        // Awake: bookings made during this turn are seen by the minimum computed at its end.
        self.gx_store.local_focus.burst_sleeping_until = None;
        let now = Instant::now();
        if self
            .gx_store
            .local_focus
            .settle_due
            .is_some_and(|due| due <= now)
        {
            self.gx_store_selection_settled(cx);
        }
        if self
            .gx_store
            .local_focus
            .finish_due
            .is_some_and(|due| due <= now)
        {
            // Frames that arrived during the burst asked for a publish even when no selection is
            // left to finish.
            if !self.gx_store_flush_local_selection(cx) {
                self.gx_store_publish_workspace_focus(cx);
            }
        }
        let local_focus = &mut self.gx_store.local_focus;
        let next_due = [local_focus.settle_due, local_focus.finish_due]
        .into_iter()
        .flatten()
        .min();
        let Some(next_due) = next_due else {
            local_focus.burst_task_running = false;
            local_focus.burst_wake = None;
            return None;
        };
        // Never zero, so a deadline that is due "now" cannot turn the loop into a busy wait.
        let wait = next_due
            .saturating_duration_since(Instant::now())
            .max(Duration::from_millis(1));
        local_focus.burst_sleeping_until = Some(Instant::now() + wait);
        Some(wait)
    }

    /// The selection has been stable: do the work that was held back, for the tab in front now.
    fn gx_store_selection_settled(&mut self, cx: &mut gpui::Context<Self>) {
        let local_focus = &mut self.gx_store.local_focus;
        local_focus.settle_due = None;
        local_focus.counters.settles += 1;
        let steps = std::mem::take(&mut local_focus.burst_steps);
        let chat_reconcile = std::mem::take(&mut local_focus.chat_reconcile_wanted);
        if chat_reconcile {
            self.reconcile_agents_chat_surfaces(cx);
        }
        self.gx_store_attach_surfaced_terminals(cx);
        if std::mem::take(&mut self.gx_store.local_focus.browser_surface_wanted)
            && self.active_mode == crate::app::model::TitlebarMode::Browser
        {
            // What a single press does at once (`sync_active_browser_tab_to_surface`), for the
            // tab in front now.
            self.ensure_browser_surface_for_pane(self.browser_tabs.focused_pane, cx);
            self.update_active_mode_cef_child_visibility(cx);
        }
        let (walk_reveal, walk_ask) = self.gx_store_take_walk_landing();
        if let Some(row_id) = walk_reveal {
            self.native_sidebar.pending_reveal = Some(
                crate::app::native_sidebar::model::NativeSidebarRevealRequest {
                    session_id: row_id,
                    request_id: 0,
                },
            );
        }
        if let Some(row_id) = walk_ask {
            // The row the held key landed on has a staged tab and no terminal: it is focused now,
            // which wakes or attaches it, as a click would have. The selection it finishes is
            // flushed first, so the focus moves from the store's newest one.
            self.gx_store_flush_local_selection(cx);
            self.gx_store_focus_session_row(&row_id, RowFocusOptions::default(), cx);
        }
        let counters = self.gx_store.local_focus.counters;
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.terminalFocus.tabBurstSettled",
            serde_json::json!({
                "steps": steps,
                "chatReconciled": chat_reconcile,
                "localStamp": self.gx_store.core.focus().local_stamp,
                "localSelections": counters.local_selections,
                "unplacedSelections": counters.unplaced_selections,
                "finishes": counters.finishes,
                "attentionAcknowledges": counters.attention_acknowledges,
                "settles": counters.settles,
            }),
        );
        // The render that follows mounts the terminal viewer of the active tab, releases the
        // viewers that went out of view, and reports the displayed sessions.
        cx.notify();
    }

    /// A restored Running tab with no terminal behind it is attached once it is in front.
    ///
    /// CDXC:FocusRouting 2026-09-19 WHY:
    /// The old runtime used to answer a `localRuntimeMissing` tab selection with a focus request that re-entered the attach path (2026-07-11), one bridge round trip per selected tab. The surfaced-restore attach already attaches exactly the tabs that are active in a rendered pane, Running, and without a terminal, and it re-checks all three when its plan returns, so a tab the user only passed through is never attached. The selection's finish still keeps the flags, because the reply also covered a tab that is sleeping locally while the daemon reports it running (`gx_store_flush_local_selection`).
    pub(super) fn gx_store_attach_surfaced_terminals(&mut self, cx: &mut gpui::Context<Self>) {
        // Before the first published tab list the restored tabs have not been checked against the
        // daemon, and that first publish runs this same pass itself.
        if self
            .sidebar_gxserver_presentation_focus_state
            .active_project_tab_sessions
            .is_none()
        {
            return;
        }
        let focus_state = self.sidebar_gxserver_presentation_focus_state.clone();
        self.attach_surfaced_local_workspace_terminals(&focus_state, cx);
    }

    /// Runs the follow-up of the newest local selection now: the focus state file, the attention of
    /// what the user landed on, the remembered session of each project the burst passed through,
    /// and the workspace publish. Runs when the finish deadline passes, and before a focus that
    /// starts from the store's newest selection (a sidebar command, a remote selection).
    ///
    /// CDXC:FocusRouting 2026-06-27-00:33:
    /// MacOS reconciles stale native sleeping pane tabs when gxserver presentation already reports the canonical P/G session running. Preserve the one-way tab-selection path for ordinary clicks, but if the selected mapped tab is sleeping locally, or restored with no terminal behind it (2026-07-11), while the daemon's row is running, send one bounded focus request so the existing tab is reused and attached instead of leaving an inert sleeping placeholder.
    ///
    /// Returns whether there was anything to finish.
    pub(crate) fn gx_store_flush_local_selection(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        let local_focus = &mut self.gx_store.local_focus;
        local_focus.finish_due = None;
        let pending = local_focus.pending_finish.take();
        let stamp = self.gx_store.core.focus().local_stamp;
        let nothing_new = pending.is_none()
            && self.gx_store.local_focus.pending_attention.is_empty()
            && self.gx_store.local_focus.pending_remembered.is_empty()
            && self.gx_store.local_focus.finished_stamp == stamp;
        if nothing_new {
            // Called before every focus that starts from the newest selection; usually there is
            // nothing to finish.
            return false;
        }
        self.gx_store_persist_focus_state_file();
        // The attention of what the user stopped on is the store's to acknowledge
        // (gx_store/attention/).
        let pending_attention = std::mem::take(&mut self.gx_store.local_focus.pending_attention);
        for key in pending_attention {
            // A tab the user only passed through is not acknowledged: acknowledging means the
            // user saw it, and what the user sees is what is in front of a pane now.
            if !self.gx_store_session_is_in_front(&key) {
                continue;
            }
            self.gx_store.local_focus.counters.attention_acknowledges += 1;
            self.gx_store_acknowledge_attention(
                ghostex_gx_core::SessionKey::local(key.project_id, key.session_id),
                cx,
            );
        }
        self.gx_store_persist_remembered_sessions(cx);
        self.gx_store_publish_workspace_focus(cx);
        self.gx_store.local_focus.finished_stamp = self.gx_store.core.focus().local_stamp;
        let Some(pending) = pending else {
            return true;
        };
        self.gx_store.local_focus.counters.finishes += 1;
        if pending.local_was_sleeping || pending.local_runtime_missing {
            let running = self
                .gx_store
                .core
                .presentation()
                .loaded(&ghostex_gx_core::MachineId::Local)
                .and_then(|loaded| {
                    loaded.server_session(&pending.key.project_id, &pending.key.session_id)
                })
                .is_some_and(|row| row.lifecycle_state == LifecycleState::Running);
            if running {
                self.gx_store_request_workspace_focus(
                    GpuiSidebarWorkspaceTerminalFocusMessage {
                        force_remount: false,
                        placement: GpuiWorkspaceTerminalFocusPlacement::Tab,
                        placement_target_session_id: None,
                        preferred_interface: GpuiPreferredAgentInterface::Terminal,
                        project_id: pending.key.project_id.clone(),
                        session_id: pending.key.session_id.clone(),
                        startup_restore: false,
                        keep_view: false,
                        wake_sleeping: false,
                        keep_sleeping: false,
                    },
                    cx,
                );
            }
        }
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.terminalFocus.selectionFinished",
            serde_json::json!({
                "focusStamp": self.gx_store.core.focus().local_stamp,
                "finishes": self.gx_store.local_focus.counters.finishes,
                "localSelections": self.gx_store.local_focus.counters.local_selections,
            }),
        );
        // Two more readers of the focused session hear about the selection here, once per burst.
        self.refresh_sidebar_gxserver_bootstrap_if_changed(cx);
        self.broadcast_extension_context_changes(cx);
        true
    }

    /// Whether a session is the active tab of a rendered Agents pane or fills a companion slot.
    fn gx_store_session_is_in_front(&self, key: &GpuiLocalWorkspaceSessionKey) -> bool {
        let Some(shell_session_id) = self.local_workspace_session_mappings.get(key).copied() else {
            return false;
        };
        self.agents_workspace
            .rendered_leaf_order()
            .into_iter()
            .any(|pane_id| {
                self.agents_workspace.active_session_in_pane(pane_id) == Some(shell_session_id)
            })
    }

    /// One line per next or previous tab step while the `native.terminal.focus` scenario is on:
    /// the time from the hotkey handler's entry to the repaint request, and whether the heavy
    /// work of this step waits for the selection to settle.
    pub(crate) fn gx_store_log_tab_step(&self, started: Instant, reverse: bool) {
        let local_focus = &self.gx_store.local_focus;
        let (layout_marks, layout_serializations) = self.gx_store.layout_persist.counters();
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.terminalFocus.tabStep",
            serde_json::json!({
                "elapsedUs": started.elapsed().as_micros() as u64,
                "heavyWorkDeferred": local_focus.settle_due.is_some(),
                "burstStep": local_focus.burst_steps,
                "finishPending": local_focus.pending_finish.is_some(),
                "reverse": reverse,
                "localStamp": self.gx_store.core.focus().local_stamp,
                "finishes": local_focus.counters.finishes,
                "layoutMarks": layout_marks,
                "layoutSerializations": layout_serializations,
            }),
        );
    }
}
