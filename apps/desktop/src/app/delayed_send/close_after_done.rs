//! Close After Done toggles and timers for command and agents sessions.

use std::time::Duration;
use std::time::SystemTime;

use crate::app::consts::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn toggle_gpui_command_close_after_done_for_focused_command_pane(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some((_group_id, session_id)) =
            focused_command_pane_close_after_done_target(self.shell_focus, &self.command_pane)
        else {
            return false;
        };
        self.toggle_gpui_command_close_after_done(session_id, cx)
    }

    pub(crate) fn toggle_gpui_close_after_done_for_focused_agents_session(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(shell_session_id) = self.focused_agents_or_companion_shell_session_id() else {
            return false;
        };
        self.toggle_gpui_close_after_done_for_agents_session(shell_session_id, cx)
    }

    pub(crate) fn toggle_gpui_close_after_done_for_agents_session(
        &mut self,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        // Local or remote: the store toggles either, as the sidebar row's Close After Done does.
        self.prune_local_workspace_session_mappings();
        let Some(session) = self.workspace_terminal_session_key(shell_session_id) else {
            return false;
        };
        self.gx_store_toggle_close_after_done(&session.to_sidebar_session_id(), cx);
        true
    }

    pub(crate) fn toggle_gpui_command_close_after_done_for_command_pane_tab(
        &mut self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:ContextMenus 2026-06-25-17:37:
        Close After Done from a clicked command tab is session-scoped like native. Validate the clicked tab membership before toggling so retained command-tab action handlers cannot arm a stale or unrelated command session.

        CDXC:ContextMenus 2026-06-25-18:33:
        Retained clicked-tab action handlers focus the clicked command terminal before dispatch. GPUI Close After Done should therefore make the clicked command tab the command-pane focus before toggling the armed flag, without expanding collapsed command chrome.

        CDXC:ContextMenus 2026-06-27-01:55:
        This path is no longer emitted by the command-tab right-click menu; focused command-palette/sidebar/modal routes still use it to preserve exact target validation.
        */
        if !self.focus_command_pane_tab_for_context_session_action(
            CommandPaneTabSessionAction::CloseAfterDone,
            group_id,
            session_id,
            cx,
        ) {
            return false;
        }
        self.toggle_gpui_command_close_after_done(session_id, cx)
    }

    pub(crate) fn toggle_gpui_command_close_after_done(
        &mut self,
        session_id: CommandSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:Sessions 2026-06-25-15:24:
        The command-palette Close After Done row is terminal-scoped command-pane behavior. Toggle the focused command session's armed flag, start the three-minute countdown only once the session is done/non-working, and keep deadlines/countdowns runtime-only.

        CDXC:Sessions 2026-06-25-16:52:
        Sleeping command tabs may still toggle Close After Done like native focused-session routing. Arming a sleeping tab persists only the boolean intent; no countdown starts until the tab wakes and becomes Done.
        */
        match gpui_command_close_after_done_toggle_target(&self.command_pane, session_id) {
            GpuiCommandCloseAfterDoneToggleTarget::ClearStoredSession => {
                self.clear_gpui_command_close_after_done_timer(session_id);
                self.dispatch_gpui_app_modal_toast("info", "Close After Done canceled", "", cx);
                self.refresh_sidebar_command_pane_sessions_if_changed(cx);
                self.persist_shell_layout_state();
                cx.notify();
                true
            }
            GpuiCommandCloseAfterDoneToggleTarget::ArmLiveSession => {
                let Some(session) = self.command_pane.session_mut(session_id) else {
                    return false;
                };
                session.close_after_done_armed = true;
                self.refresh_gpui_command_close_after_done_timer_for_session(session_id, cx);
                self.dispatch_gpui_app_modal_toast(
                    "info",
                    "Close After Done enabled",
                    "Closes after Done stays visible for 3m.",
                    cx,
                );
                self.refresh_sidebar_command_pane_sessions_if_changed(cx);
                self.persist_shell_layout_state();
                cx.notify();
                true
            }
            GpuiCommandCloseAfterDoneToggleTarget::NoOp => false,
        }
    }

    pub(crate) fn clear_gpui_command_close_after_done_timer(
        &mut self,
        session_id: CommandSessionId,
    ) -> bool {
        let removed = self
            .command_close_after_done_timers
            .remove(&session_id)
            .is_some();
        let mut changed = removed;
        if let Some(session) = self.command_pane.session_mut(session_id)
            && session.close_after_done_armed
        {
            session.close_after_done_armed = false;
            changed = true;
        }
        changed
    }

    pub(crate) fn refresh_gpui_command_close_after_done_timers(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let mut changed = self.prune_gpui_command_close_after_done_timers_for_command_model();
        let session_ids = self
            .command_pane
            .flat_tab_ids()
            .into_iter()
            .map(|(_group_id, session_id)| session_id)
            .collect::<Vec<_>>();
        for session_id in session_ids {
            changed = self.refresh_gpui_command_close_after_done_timer_for_session(session_id, cx)
                || changed;
        }
        changed
    }

    pub(crate) fn refresh_gpui_command_close_after_done_timer_for_session(
        &mut self,
        session_id: CommandSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some((_group_id, session)) =
            gpui_command_close_after_done_runtime_timer_member(&self.command_pane, session_id)
        else {
            return self
                .command_close_after_done_timers
                .remove(&session_id)
                .is_some();
        };
        if !session.close_after_done_armed {
            return self
                .command_close_after_done_timers
                .remove(&session_id)
                .is_some();
        }
        if !gpui_command_close_after_done_session_marked_done(session) {
            return self
                .command_close_after_done_timers
                .remove(&session_id)
                .is_some();
        }
        if self
            .command_close_after_done_timers
            .contains_key(&session_id)
        {
            return false;
        }

        self.command_close_after_done_generation =
            self.command_close_after_done_generation.wrapping_add(1);
        let generation = self.command_close_after_done_generation;
        let deadline_at = SystemTime::now()
            .checked_add(COMMAND_PANE_CLOSE_AFTER_DONE_DELAY)
            .unwrap_or_else(SystemTime::now);
        self.command_close_after_done_timers.insert(
            session_id,
            GpuiCommandCloseAfterDoneTimer {
                deadline_at,
                generation,
            },
        );
        self.schedule_gpui_command_close_after_done_fire(
            session_id,
            generation,
            COMMAND_PANE_CLOSE_AFTER_DONE_DELAY,
            cx,
        );
        self.ensure_gpui_command_close_after_done_countdown_ticker(cx);
        true
    }

    pub(crate) fn ensure_gpui_command_close_after_done_countdown_ticker(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:DelayedSend 2026-06-25-17:09:
        Close After Done remaining labels are sidebar/titlebar projection chrome, matching native's one-second publish loop. Keep this ticker process-local and active only while runtime countdowns exist; it must not persist countdown labels, inspect command output, or read status-file paths.
        */
        if self.command_close_after_done_countdown_ticker_active
            || self.command_close_after_done_timers.is_empty()
        {
            return;
        }
        self.command_close_after_done_countdown_ticker_active = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let keep_running = this
                    .update(cx, |this, cx| {
                        if this.command_close_after_done_timers.is_empty() {
                            this.command_close_after_done_countdown_ticker_active = false;
                            cx.notify();
                            false
                        } else {
                            this.refresh_sidebar_command_pane_sessions_if_changed(cx);
                            cx.notify();
                            true
                        }
                    })
                    .unwrap_or(false);
                if !keep_running {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn schedule_gpui_command_close_after_done_fire(
        &mut self,
        session_id: CommandSessionId,
        generation: u64,
        duration: Duration,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(duration).await;
            let _ = this.update(cx, |this, cx| {
                this.fire_gpui_command_close_after_done(session_id, generation, cx);
            });
        })
        .detach();
    }

    pub(crate) fn fire_gpui_command_close_after_done(
        &mut self,
        session_id: CommandSessionId,
        generation: u64,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(timer) = self
            .command_close_after_done_timers
            .get(&session_id)
            .copied()
        else {
            return false;
        };
        if timer.generation != generation {
            return false;
        }
        let _deadline_at = timer.deadline_at;
        let Some((group_id, session_ready)) =
            gpui_command_close_after_done_runtime_timer_member(&self.command_pane, session_id).map(
                |(group_id, session)| {
                    (
                        group_id,
                        session.close_after_done_armed
                            && gpui_command_close_after_done_session_marked_done(session),
                    )
                },
            )
        else {
            self.command_close_after_done_timers.remove(&session_id);
            self.refresh_sidebar_command_pane_sessions_if_changed(cx);
            cx.notify();
            return false;
        };
        if !session_ready {
            self.command_close_after_done_timers.remove(&session_id);
            self.refresh_sidebar_command_pane_sessions_if_changed(cx);
            cx.notify();
            return false;
        }

        self.command_close_after_done_timers.remove(&session_id);
        if let Some(session) = self.command_pane.session_mut(session_id) {
            session.close_after_done_armed = false;
        }
        self.close_command_pane_tab(group_id, session_id, cx)
    }

    pub(crate) fn prune_gpui_command_close_after_done_timers_for_command_model(&mut self) -> bool {
        let stale_session_ids = gpui_command_close_after_done_stale_runtime_timer_session_ids(
            &self.command_pane,
            &self.command_close_after_done_timers,
        );
        let changed = !stale_session_ids.is_empty();
        for session_id in stale_session_ids {
            self.command_close_after_done_timers.remove(&session_id);
        }
        changed
    }
}
