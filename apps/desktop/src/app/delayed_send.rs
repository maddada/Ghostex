// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: sidebar metadata commands, delayed sends, close-after-done timers

use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use anyhow::Result;
use gpui::ClipboardItem;
use gpui::Window;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;
impl GhostexGpuiApp {
    pub(crate) fn handle_gpui_sidebar_agent_metadata_command(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:AgentLauncher 2026-06-24-20:54:
        Settings > Agents writes in GPUI must enter gxserver's semantic agent/action mutation contract. The app-modal bridge only validates the bounded CEF message shape; gxserver owns hidden-default restoration, custom metadata persistence, order normalization, and refreshed HUD/project rows without logging launcher text, project identity, paths, URLs, tokens, stdout, or stderr.
        */
        let write = match gpui_sidebar_agent_metadata_write_from_command(command) {
            Ok(write) => write,
            Err(_) => {
                self.dispatch_gpui_sidebar_metadata_write_failure(
                    GpuiSidebarMetadataWriteKind::Agents,
                    GPUI_SIDEBAR_METADATA_GENERIC_ERROR,
                    cx,
                );
                return;
            }
        };
        let failure_order_sync_result = write.order_sync_result("error", Vec::new());
        let active_project_id = self.gpui_app_modal_active_project_id();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result =
                background
                    .spawn(async move {
                        gpui_apply_sidebar_agent_metadata_write(write, active_project_id)
                    })
                    .await;
            let _ = this.update(cx, |this, cx| {
                this.finish_gpui_sidebar_metadata_write(
                    GpuiSidebarMetadataWriteKind::Agents,
                    result,
                    failure_order_sync_result,
                    cx,
                );
            });
        })
        .detach();
    }

    pub(crate) fn handle_gpui_sidebar_command_metadata_command(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:AgentLauncher 2026-06-24-20:54:
        Settings > Actions writes in GPUI are active-project scoped but gxserver-owned. Pass the current app-modal active project id into the semantic mutation contract so gxserver resolves worktree parent ownership, deleted default actions, command/browser validation, display order, and refreshed HUD/project rows without local metadata rewrites.
        */
        let active_project_id = self.gpui_app_modal_active_project_id();
        let write =
            match gpui_sidebar_command_metadata_write_from_command(command, active_project_id) {
                Ok(write) => write,
                Err(_) => {
                    self.dispatch_gpui_sidebar_metadata_write_failure(
                        GpuiSidebarMetadataWriteKind::Commands,
                        GPUI_SIDEBAR_METADATA_GENERIC_ERROR,
                        cx,
                    );
                    return;
                }
            };
        let failure_order_sync_result = write.order_sync_result("error", Vec::new());
        if let Some(command_id) = write.deleted_command_id().map(str::to_string) {
            self.close_gpui_deleted_sidebar_command_session(command_id.as_str(), cx);
        }
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { gpui_apply_sidebar_command_metadata_write(write) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.finish_gpui_sidebar_metadata_write(
                    GpuiSidebarMetadataWriteKind::Commands,
                    result,
                    failure_order_sync_result,
                    cx,
                );
            });
        })
        .detach();
    }

    pub(crate) fn close_gpui_deleted_sidebar_command_session(
        &mut self,
        command_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some((group_id, session_id)) = self
            .command_pane
            .take_action_session_slot_for_action_close(command_id)
        else {
            return false;
        };
        self.clear_gpui_command_delayed_send_timer(session_id);
        self.clear_gpui_command_close_after_done_timer(session_id);
        if !self
            .command_pane
            .close_session_from_direct_tab_close(group_id, session_id)
        {
            return false;
        }
        self.forget_command_gxserver_session_for_closed_tab(session_id, cx);
        self.clear_command_resize_hover_state_if_command_pane_hidden();
        if !self.command_pane.has_sessions() && self.shell_focus == ShellFocusTarget::CommandPane {
            self.restore_previous_non_command_focus_or_default();
        }
        self.scroll_command_group_active_tab(group_id);
        self.scroll_focused_command_active_tab();
        self.persist_shell_layout_state();
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        cx.notify();
        true
    }

    pub(crate) fn finish_gpui_sidebar_metadata_write(
        &mut self,
        kind: GpuiSidebarMetadataWriteKind,
        result: Result<Option<serde_json::Value>, String>,
        failure_order_sync_result: Option<serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        match result {
            Ok(order_sync_result) => {
                self.refresh_open_gpui_app_modal_sidebar_state_in_background(cx);
                if let Some(order_sync_result) = order_sync_result {
                    self.dispatch_open_gpui_app_modal_message(order_sync_result, cx);
                }
                cx.notify();
            }
            Err(error_code) => {
                if let Some(order_sync_result) = failure_order_sync_result {
                    self.dispatch_open_gpui_app_modal_message(order_sync_result, cx);
                }
                self.dispatch_gpui_sidebar_metadata_write_failure(kind, error_code.as_str(), cx);
            }
        }
    }

    pub(crate) fn dispatch_gpui_sidebar_metadata_write_failure(
        &mut self,
        kind: GpuiSidebarMetadataWriteKind,
        error_code: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if error_code == GPUI_SIDEBAR_DUPLICATE_ACTION_TITLE_ERROR {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Action title already exists",
                "An action with that title already exists in this project.",
                cx,
            );
            return;
        }
        self.dispatch_gpui_app_modal_toast(
            "warning",
            "Settings were not saved",
            kind.failure_message(),
            cx,
        );
    }

    pub(crate) fn handle_gpui_save_pinned_prompt_command(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:ServerDaemon 2026-06-24-13:30:
        Pinned Prompt saves must use the shared gxserver product-data contract
        while keeping the React `savePinnedPrompt` bridge message unchanged.
        Reject malformed non-string fields locally and let gxserver preserve
        createdAt, stamp updatedAt, normalize titles, and keep prompt text out
        of logs and progress channels.
        */
        let Some(content) = command.get("content").and_then(serde_json::Value::as_str) else {
            return;
        };
        let Some(title) = command.get("title").and_then(serde_json::Value::as_str) else {
            return;
        };
        if command
            .get("promptId")
            .is_some_and(|value| !value.is_null() && !value.is_string())
        {
            return;
        }
        let prompt_id = command
            .get("promptId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if gpui_save_gxserver_pinned_prompt(content, title, prompt_id.as_deref()).is_ok() {
            self.refresh_open_gpui_app_modal_sidebar_state(
                self.gpui_app_modal_sidebar_state_message(),
                cx,
            );
        }
    }

    pub(crate) fn handle_gpui_schedule_delayed_send_command(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:DelayedSend 2026-08-17:
        Remote sidebar rows carry their canonical machine/project/session id,
        but they do not belong to a local command tab or local Agents mapping.
        Return that bounded command to the sidebar runtime so it can submit the
        durable trigger to the gxserver that hosts the session.
        */
        if command
            .get("sessionId")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_remote_attach_session_reference_from_project_id)
            .is_some()
        {
            self.dispatch_gpui_sidebar_host_message(serde_json::Value::Object(command.clone()), cx);
            return;
        }
        /*
        CDXC:DelayedSend 2026-06-25-23:04:
        `scheduleDelayedSend` is a direct command-session sidebar command, so resolve its external `G{u64}` sessionId through the shared live command-tab bridge before reading delayMs. Malformed, legacy numeric, stale, missing, and orphan ids must no-op without falling back to the focused command group or surfacing duration validation for the wrong target.
        */
        let Some((_group_id, session_id)) =
            gpui_app_modal_sidebar_command_live_command_tab(&self.command_pane, command)
        else {
            self.handle_gpui_schedule_agents_delayed_send_command(command, cx);
            return;
        };
        let Some(delay_ms) = command.get("delayMs").and_then(serde_json::Value::as_u64) else {
            return;
        };
        let Some(duration) = gpui_command_delayed_send_duration_from_millis(delay_ms) else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Delayed Send unavailable",
                "Choose a Delayed Send timer between 1 minute and 24 days.",
                cx,
            );
            return;
        };
        if self.schedule_gpui_command_delayed_send(session_id, duration, cx) {
            let description = format!(
                "Presses Enter in {}.",
                gpui_command_delayed_send_duration_label(duration)
            );
            self.dispatch_gpui_app_modal_toast("info", "Delayed Send scheduled", &description, cx);
        } else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Delayed Send unavailable",
                "Select a visible command terminal before scheduling Delayed Send.",
                cx,
            );
        }
    }

    pub(crate) fn handle_gpui_rename_command_session_command(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:CommandPane 2026-06-25-16:33:
        Rename Session submissions from a GPUI command-pane modal are local command-tab title edits. Accept only the command session id and normalized title; generated-title requests require a gxserver-backed agent session and must not write long prompt text into a local command tab.
        */
        let Some(session_id) = command
            .get("sessionId")
            .and_then(gpui_command_session_id_from_modal_value)
        else {
            let session_id_is_sidebar_owned = command
                .get("sessionId")
                .and_then(serde_json::Value::as_str)
                .is_some_and(gpui_app_modal_sidebar_session_id_allowed);
            if session_id_is_sidebar_owned {
                self.dispatch_gpui_sidebar_host_message(
                    serde_json::Value::Object(command.clone()),
                    cx,
                );
            }
            return;
        };
        if command
            .get("shouldGenerateTitle")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
        {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Rename unavailable",
                "Generate Name is not available for local GPUI command tabs yet.",
                cx,
            );
            return;
        }
        let Some(title) = command
            .get("title")
            .and_then(gpui_command_session_rename_title_from_modal_value)
        else {
            return;
        };
        let gxserver_key = self.command_gxserver_session_key_for_command_tab(session_id);
        if !self.command_pane.rename_session(session_id, title.clone()) {
            return;
        }
        if let Some(key) = gxserver_key {
            self.update_command_gxserver_session_title_in_background(key, title, cx);
        }
        self.persist_shell_layout_state();
        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        cx.notify();
    }

    pub(crate) fn handle_gpui_cancel_delayed_send_command(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        if command
            .get("sessionId")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_remote_attach_session_reference_from_project_id)
            .is_some()
        {
            self.dispatch_gpui_sidebar_host_message(serde_json::Value::Object(command.clone()), cx);
            return;
        }
        /*
        CDXC:DelayedSend 2026-06-25-23:04:
        Cancel submissions from the shared sidebar/app-modal bridge must target a live command tab, not a stale stored command-session row. Resolve the external `G{u64}` sessionId through the shared app-modal command bridge so malformed, legacy numeric, missing, orphan, or stale ids no-op before any runtime timer is cleared.
        */
        let Some((_group_id, session_id)) =
            gpui_app_modal_sidebar_command_live_command_tab(&self.command_pane, command)
        else {
            self.handle_gpui_cancel_agents_delayed_send_command(command, cx);
            return;
        };
        if self.clear_gpui_command_delayed_send_timer(session_id) {
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
            self.dispatch_gpui_app_modal_toast("info", "Delayed Send canceled", "", cx);
            self.persist_shell_layout_state();
            cx.notify();
        } else {
            self.dispatch_gpui_app_modal_toast("info", "No Delayed Send timer is active", "", cx);
        }
    }

    pub(crate) fn handle_gpui_toggle_close_after_done_command(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(session_id) = self.gpui_agents_delayed_send_session_id_from_command(command) {
            let _ = self.toggle_gpui_close_after_done_for_agents_session(session_id, cx);
            return;
        }
        let Some((_group_id, session_id)) =
            gpui_app_modal_sidebar_command_live_command_tab(&self.command_pane, command)
        else {
            return;
        };
        self.toggle_gpui_command_close_after_done(session_id, cx);
    }

    pub(crate) fn restore_gpui_command_startup_activity_intents(
        &mut self,
        restore_intents: Vec<GpuiCommandStartupActivityRestoreIntent>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:Workarea 2026-06-25-17:25:
        Startup activity restore mutates only the command-pane model and leaves persistence to the app startup pass after Delayed Send restore also runs. This keeps Working wake hints one-shot without rewriting a restored timer checkpoint before the runtime timer map is installed.
        */
        if !command_pane_apply_startup_activity_restore_intents(
            &mut self.command_pane,
            &restore_intents,
        ) {
            return false;
        }
        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        cx.notify();
        true
    }

    pub(crate) fn restore_gpui_command_delayed_send_timers(
        &mut self,
        restore_timers: Vec<GpuiCommandDelayedSendRestoreTimer>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:DelayedSend 2026-06-25-16:41:
        Startup re-arms restored command Delayed Send timers from the saved remaining-duration checkpoint as normal runtime timers. This keeps native parity without persisting command text, terminal content, paths, titles, Ghostty runtime ids, stdout/stderr, or old deadlines as authority after restart.

        CDXC:DelayedSend 2026-06-25-16:56:
        Restored Delayed Send timers are also startup wake reasons for command-pane tabs. Wake only after loading a safe persisted checkpoint so a restarted timer can reach a command terminal body, while manual in-process Sleep remains parked until the user wakes it.
        */
        let mut changed = false;
        for restore_timer in restore_timers {
            if command_pane_group_for_session(&self.command_pane, restore_timer.session_id)
                .is_none()
                || self
                    .command_pane
                    .session(restore_timer.session_id)
                    .is_none()
            {
                continue;
            }
            self.command_delayed_send_generation =
                self.command_delayed_send_generation.wrapping_add(1);
            let generation = self.command_delayed_send_generation;
            let duration = gpui_command_delayed_send_restore_duration(restore_timer.remaining_ms);
            let deadline_at = SystemTime::now()
                .checked_add(duration)
                .unwrap_or_else(SystemTime::now);
            self.command_delayed_send_timers.insert(
                restore_timer.session_id,
                GpuiCommandDelayedSendTimer {
                    deadline_at,
                    generation,
                },
            );
            command_pane_apply_delayed_send_restore_intent(
                &mut self.command_pane,
                restore_timer.session_id,
            );
            self.schedule_gpui_command_delayed_send_fire(
                restore_timer.session_id,
                generation,
                duration,
                cx,
            );
            changed = true;
        }
        if changed {
            self.ensure_gpui_command_delayed_send_countdown_ticker(cx);
            self.ensure_gpui_command_delayed_send_persistence_ticker(cx);
            self.refresh_sidebar_command_pane_sessions_if_changed(cx);
            cx.notify();
        }
        changed
    }

    pub(crate) fn restore_command_terminal_gxserver_sessions_from_shell_state(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:Workarea 2026-07-04:
        Startup restore validates persisted command-surface daemon sessions by
        running the same wake/attach metadata flow as live command tabs. Awake
        restored tabs get one-shot attach payloads for their command mount
        slots; sleeping tabs keep only their gxserver key and defer attach
        until the user wakes them. Missing daemon sessions close through the
        existing attach-failure path instead of creating a replacement shell.
        */
        let restore_slots = self
            .command_pane
            .flat_tab_ids()
            .into_iter()
            .filter_map(|(group_id, session_id)| {
                let session = self.command_pane.session(session_id)?;
                if session.is_sleeping {
                    return None;
                }
                let key = self.command_gxserver_session_key_for_command_tab(session_id)?;
                Some((
                    CommandTerminalBodyMountSlotId {
                        group_id,
                        session_id,
                    },
                    key,
                ))
            })
            .collect::<Vec<_>>();
        let mut started = false;
        for (slot_id, key) in restore_slots {
            if self
                .command_gxserver_attach_pending
                .contains(&slot_id.session_id)
            {
                continue;
            }
            self.start_existing_command_terminal_gxserver_attach_for_slot(slot_id, key, None, cx);
            started = true;
        }
        if started {
            cx.notify();
        }
        started
    }

    pub(crate) fn schedule_gpui_command_delayed_send(
        &mut self,
        session_id: CommandSessionId,
        duration: Duration,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:DelayedSend 2026-06-25-15:11:
        A GPUI Delayed Send timer may be armed only for a currently mounted command terminal body. This preserves native's exact target-session behavior without falling back to shell focus, titles, command text, persisted state, or another visible terminal when the original command surface is unavailable.
        */
        if command_pane_mounted_slot_for_session(&self.command_pane, session_id).is_none() {
            return false;
        }
        let Some(session) = self.command_pane.session_mut(session_id) else {
            return false;
        };
        if session.is_sleeping {
            return false;
        }
        self.command_delayed_send_generation = self.command_delayed_send_generation.wrapping_add(1);
        let generation = self.command_delayed_send_generation;
        let deadline_at = SystemTime::now()
            .checked_add(duration)
            .unwrap_or_else(SystemTime::now);
        self.command_delayed_send_timers.insert(
            session_id,
            GpuiCommandDelayedSendTimer {
                deadline_at,
                generation,
            },
        );
        session.set_delayed_send_active(true, true);
        self.ensure_gpui_command_delayed_send_countdown_ticker(cx);
        self.ensure_gpui_command_delayed_send_persistence_ticker(cx);
        self.schedule_gpui_command_delayed_send_fire(session_id, generation, duration, cx);
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        self.persist_shell_layout_state();
        cx.notify();
        true
    }

    pub(crate) fn ensure_gpui_command_delayed_send_countdown_ticker(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:DelayedSend 2026-06-25-15:42:
        The command-pane Delayed Send body badge is live countdown chrome. Run a process-local one-second ticker only while timers exist so the centered badge can update without persisting deadlines, logging command content, or creating a renderer-owned timer fallback.
        */
        if self.command_delayed_send_countdown_ticker_active
            || self.command_delayed_send_timers.is_empty()
        {
            return;
        }
        self.command_delayed_send_countdown_ticker_active = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let keep_running = this
                    .update(cx, |this, cx| {
                        if this.command_delayed_send_timers.is_empty() {
                            this.command_delayed_send_countdown_ticker_active = false;
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

    pub(crate) fn ensure_gpui_command_delayed_send_persistence_ticker(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:DelayedSend 2026-06-25-16:41:
        Native refreshes Delayed Send remaining-duration checkpoints once per minute so restart resumes near the live countdown position. GPUI mirrors that with a low-frequency shell-state write while timers exist, still serializing only safe timer metadata through the central writer.
        */
        if self.command_delayed_send_persistence_ticker_active
            || self.command_delayed_send_timers.is_empty()
        {
            return;
        }
        self.command_delayed_send_persistence_ticker_active = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(COMMAND_PANE_DELAYED_SEND_PERSIST_INTERVAL)
                    .await;
                let keep_running = this
                    .update(cx, |this, _cx| {
                        if this.command_delayed_send_timers.is_empty() {
                            this.command_delayed_send_persistence_ticker_active = false;
                            false
                        } else {
                            this.persist_shell_layout_state();
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

    pub(crate) fn schedule_gpui_command_delayed_send_fire(
        &mut self,
        session_id: CommandSessionId,
        generation: u64,
        duration: Duration,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(duration).await;
            let _ = this.update(cx, |this, cx| {
                this.fire_gpui_command_delayed_send(session_id, generation, cx);
            });
        })
        .detach();
    }

    pub(crate) fn fire_gpui_command_delayed_send(
        &mut self,
        session_id: CommandSessionId,
        generation: u64,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(timer) = self.command_delayed_send_timers.get(&session_id).copied() else {
            return false;
        };
        if timer.generation != generation {
            return false;
        }
        let target_was_sleeping = self
            .command_pane
            .session(session_id)
            .is_some_and(|session| session.is_sleeping);
        self.command_delayed_send_timers.remove(&session_id);
        if let Some(session) = self.command_pane.session_mut(session_id) {
            session.set_delayed_send_active(false, false);
        }
        let sent = command_pane_mounted_slot_for_session(&self.command_pane, session_id)
            .is_some_and(|slot_id| {
                self.send_return_key_to_mounted_command_terminal_surface(slot_id, cx)
            });
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        self.persist_shell_layout_state();
        cx.notify();
        if !sent && !target_was_sleeping {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Delayed Send skipped",
                "The command terminal was no longer mounted.",
                cx,
            );
        }
        sent
    }

    pub(crate) fn clear_gpui_command_delayed_send_timer(
        &mut self,
        session_id: CommandSessionId,
    ) -> bool {
        let removed = self
            .command_delayed_send_timers
            .remove(&session_id)
            .is_some();
        if let Some(session) = self.command_pane.session_mut(session_id)
            && session.delayed_send_timer_owned
        {
            session.set_delayed_send_active(false, false);
        }
        removed
    }

    pub(crate) fn enrich_gpui_agents_delayed_send_open_message(
        &self,
        open_message: &mut serde_json::Value,
        session_id: TerminalSessionId,
    ) {
        if let Some(agent_icon) = self
            .agents_workspace
            .session(session_id)
            .and_then(|session| session.agent_icon)
        {
            open_message["agentIcon"] = serde_json::json!(agent_icon);
        }
        open_message["supportsSendWhenAgentStops"] = serde_json::json!(true);
        let supports_project_scope = self
            .local_workspace_session_mappings
            .iter()
            .any(|(_, mapped_session_id)| *mapped_session_id == session_id);
        open_message["supportsSendWhenAllProjectSessionsStop"] =
            serde_json::json!(supports_project_scope);
        /*
        CDXC:DelayedSend 2026-08-19:
        Armed Delayed Sends live on the daemon, and the sidebar row already
        carries that projected trigger state into the open message. Only a
        locally owned watcher/timer may restate it, so this enrichment must not
        blank the daemon-owned trigger back to "After a delay".
        */
        if let Some(watcher) = self
            .agents_send_when_stopped_watchers
            .get(&session_id)
            .cloned()
        {
            let is_project_scope =
                matches!(&watcher.scope, GpuiAgentsSendWhenStoppedScope::Project(_));
            open_message["sendWhenAllProjectSessionsStopActive"] =
                serde_json::json!(is_project_scope);
            open_message["sendWhenAgentStopsActive"] = serde_json::json!(!is_project_scope);
            let is_working = self
                .gpui_agents_send_when_stopped_scope_is_working(session_id, &watcher.scope)
                .unwrap_or(false);
            open_message["delayedSendRemainingLabel"] = serde_json::json!(
                gpui_agents_send_when_stopped_remaining_label(&watcher, is_working, Instant::now(),)
            );
            if let Some(object) = open_message.as_object_mut() {
                object.remove("delayedSendDeadlineAt");
            }
            return;
        }
        if let Some(timer) = self.agents_delayed_send_timers.get(&session_id).copied() {
            let remaining_ms = timer.remaining_ms(SystemTime::now());
            open_message["delayedSendDeadlineAt"] =
                serde_json::json!(gpui_iso8601_utc(timer.deadline_at));
            open_message["delayedSendRemainingLabel"] =
                serde_json::json!(gpui_command_delayed_send_countdown_label(remaining_ms));
        }
    }

    /// Agents Delayed Send uses the same unconditional modal presentation as
    /// Rename. The later schedule command still requires the exact terminal
    /// body to be mounted before it arms a timer.
    pub(crate) fn open_gpui_delayed_send_modal_for_focused_agents_session(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(session_id) = self.focused_agents_or_companion_shell_session_id() else {
            return false;
        };
        let title = self.agents_workspace_tab_display_title(session_id);
        let modal = GpuiAppModalKind::DelayedSend;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = serde_json::json!({
            "modal": modal.modal_id(),
            "sessionId": gpui_agents_session_external_id(session_id),
            "title": title,
            "type": "open",
        });
        self.enrich_gpui_agents_delayed_send_open_message(&mut open_message, session_id);
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, None, cx);
        true
    }

    pub(crate) fn handle_gpui_schedule_agents_delayed_send_command(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(session_id) = self.gpui_agents_delayed_send_session_id_from_command(command)
        else {
            return;
        };
        let Some(key) = self.local_workspace_key_for_shell_session(session_id) else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Delayed Send unavailable",
                "The selected terminal is not attached to a gxserver session.",
                cx,
            );
            return;
        };
        let delay_ms = command.get("delayMs").and_then(serde_json::Value::as_u64);
        let send_when_agent_stops = command
            .get("sendWhenAgentStops")
            .and_then(serde_json::Value::as_bool)
            == Some(true);
        let send_when_all_project_sessions_stop = command
            .get("sendWhenAllProjectSessionsStop")
            .and_then(serde_json::Value::as_bool)
            == Some(true);
        if usize::from(delay_ms.is_some())
            + usize::from(send_when_agent_stops)
            + usize::from(send_when_all_project_sessions_stop)
            != 1
            || delay_ms.is_some_and(|delay_ms| {
                gpui_command_delayed_send_duration_from_millis(delay_ms).is_none()
            })
        {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Delayed Send unavailable",
                "Choose exactly one valid Delayed Send trigger.",
                cx,
            );
            return;
        }
        let params = serde_json::json!({
            "projectId": key.project_id,
            "sessionId": key.session_id,
            "delayMs": delay_ms,
            "sendWhenAgentStops": send_when_agent_stops,
            "sendWhenAllProjectSessionsStop": send_when_all_project_sessions_stop,
        });
        let description = if send_when_agent_stops {
            "Presses Enter after the agent has finished working for 10 seconds.".to_string()
        } else if send_when_all_project_sessions_stop {
            "Presses Enter after all agents in the project have finished working for 10 seconds."
                .to_string()
        } else {
            let duration = Duration::from_millis(delay_ms.unwrap_or_default());
            format!(
                "Presses Enter in {}.",
                gpui_command_delayed_send_duration_label(duration)
            )
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_schedule_agents_delayed_send_with_current_gxserver_build(&params)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if result.is_ok() {
                    this.agents_delayed_send_timers.remove(&session_id);
                    this.agents_send_when_stopped_watchers.remove(&session_id);
                    this.refresh_sidebar_agents_delayed_sends_if_changed(cx);
                    this.persist_shell_layout_state();
                    this.dispatch_gpui_app_modal_toast(
                        "info",
                        "Delayed Send scheduled",
                        &description,
                        cx,
                    );
                } else {
                    this.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Delayed Send unavailable",
                        "gxserver could not persist this Delayed Send.",
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    pub(crate) fn handle_gpui_cancel_agents_delayed_send_command(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(session_id) = self.gpui_agents_delayed_send_session_id_from_command(command)
        else {
            return;
        };
        let Some(key) = self.local_workspace_key_for_shell_session(session_id) else {
            return;
        };
        let response = gpui_gxserver_rpc_result(
            "/api/cancelDelayedSend",
            &serde_json::json!({
                "projectId": key.project_id,
                "sessionId": key.session_id,
            }),
            Duration::from_secs(5),
        );
        let removed_timer = self
            .agents_delayed_send_timers
            .remove(&session_id)
            .is_some();
        let removed_watcher = self
            .agents_send_when_stopped_watchers
            .remove(&session_id)
            .is_some();
        let changed = response
            .ok()
            .and_then(|result| result.get("changed").and_then(serde_json::Value::as_bool))
            .unwrap_or(false);
        if changed || removed_timer || removed_watcher {
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
            self.refresh_sidebar_agents_delayed_sends_if_changed(cx);
            self.persist_shell_layout_state();
            self.dispatch_gpui_app_modal_toast("info", "Delayed Send canceled", "", cx);
            cx.notify();
        } else {
            self.dispatch_gpui_app_modal_toast("info", "No Delayed Send timer is active", "", cx);
        }
    }

    pub(crate) fn gpui_agents_delayed_send_session_id_from_command(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
    ) -> Option<TerminalSessionId> {
        // Sidebar cards retain their combined gxserver identity while focused
        // pane actions use GW ids. Resolve both only at the command boundary.
        let external_session_id = command.get("sessionId")?.as_str()?;
        self.gpui_titlebar_resource_shell_session_id(external_session_id)
    }

    pub(crate) fn restore_gpui_agents_delayed_sends(
        &mut self,
        intents: Vec<GpuiAgentsDelayedSendRestoreIntent>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let mut restored = false;
        for intent in intents {
            if self.agents_workspace.session(intent.session_id).is_none() {
                continue;
            }
            self.agents_delayed_send_generation =
                self.agents_delayed_send_generation.wrapping_add(1);
            let generation = self.agents_delayed_send_generation;
            match intent.trigger {
                GpuiAgentsDelayedSendRestoreTrigger::Timer { remaining_ms } => {
                    let duration = gpui_command_delayed_send_restore_duration(remaining_ms);
                    let deadline_at = SystemTime::now()
                        .checked_add(duration)
                        .unwrap_or_else(SystemTime::now);
                    self.agents_delayed_send_timers.insert(
                        intent.session_id,
                        GpuiCommandDelayedSendTimer {
                            deadline_at,
                            generation,
                        },
                    );
                    self.spawn_gpui_agents_delayed_send_fire(
                        intent.session_id,
                        generation,
                        duration,
                        cx,
                    );
                }
                GpuiAgentsDelayedSendRestoreTrigger::WhenAgentFinishesWorking => {
                    let scope = GpuiAgentsSendWhenStoppedScope::Session;
                    let Some(is_working) = self
                        .gpui_agents_send_when_stopped_scope_is_working(intent.session_id, &scope)
                    else {
                        continue;
                    };
                    self.agents_send_when_stopped_watchers.insert(
                        intent.session_id,
                        GpuiAgentsSendWhenStoppedWatcher {
                            generation,
                            non_working_since: (!is_working).then(Instant::now),
                            scope,
                        },
                    );
                    self.spawn_gpui_agents_send_when_stopped_poll(
                        intent.session_id,
                        generation,
                        cx,
                    );
                }
                GpuiAgentsDelayedSendRestoreTrigger::WhenAllAgentsFinishWorking { project_id } => {
                    let scope = GpuiAgentsSendWhenStoppedScope::Project(project_id);
                    let Some(is_working) = self
                        .gpui_agents_send_when_stopped_scope_is_working(intent.session_id, &scope)
                    else {
                        continue;
                    };
                    self.agents_send_when_stopped_watchers.insert(
                        intent.session_id,
                        GpuiAgentsSendWhenStoppedWatcher {
                            generation,
                            non_working_since: (!is_working).then(Instant::now),
                            scope,
                        },
                    );
                    self.spawn_gpui_agents_send_when_stopped_poll(
                        intent.session_id,
                        generation,
                        cx,
                    );
                }
            }
            restored = true;
        }
        if restored {
            self.ensure_gpui_agents_delayed_send_countdown_ticker(cx);
            self.ensure_gpui_agents_delayed_send_persistence_ticker(cx);
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
            self.refresh_sidebar_agents_delayed_sends_if_changed(cx);
            cx.notify();
        }
        restored
    }

    #[allow(dead_code)] // no caller: Agents delayed send is scheduled through the command-pane delayed-send path
    pub(crate) fn schedule_gpui_agents_delayed_send(
        &mut self,
        session_id: TerminalSessionId,
        duration: Duration,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:DelayedSend 2026-07-21:
        Agents Delayed Send is a terminal-engine-neutral action. The old arm
        gate looked only in the native Ghostty surface map, so every terminal
        owned by the GPUI engine was rejected even though the fire path already
        knew how to press Return in that engine. Resolve the exact live owner
        shared by foreground, background, and project-editor companion
        terminals before arming; never substitute the focused terminal or
        another session.
        */
        if self
            .gpui_agents_delayed_send_mount_target(session_id)
            .is_none()
        {
            return false;
        }
        self.agents_send_when_stopped_watchers.remove(&session_id);
        self.agents_delayed_send_generation = self.agents_delayed_send_generation.wrapping_add(1);
        let generation = self.agents_delayed_send_generation;
        let deadline_at = SystemTime::now()
            .checked_add(duration)
            .unwrap_or_else(SystemTime::now);
        self.agents_delayed_send_timers.insert(
            session_id,
            GpuiCommandDelayedSendTimer {
                deadline_at,
                generation,
            },
        );
        self.spawn_gpui_agents_delayed_send_fire(session_id, generation, duration, cx);
        self.ensure_gpui_agents_delayed_send_countdown_ticker(cx);
        self.ensure_gpui_agents_delayed_send_persistence_ticker(cx);
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        self.refresh_sidebar_agents_delayed_sends_if_changed(cx);
        self.persist_shell_layout_state();
        cx.notify();
        true
    }

    #[allow(dead_code)] // no caller: Agents delayed send is scheduled through the command-pane delayed-send path
    pub(crate) fn schedule_gpui_agents_send_when_stopped(
        &mut self,
        session_id: TerminalSessionId,
        scope: GpuiAgentsSendWhenStoppedScope,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self
            .gpui_agents_delayed_send_mount_target(session_id)
            .is_none()
        {
            return false;
        }
        let Some(is_working) =
            self.gpui_agents_send_when_stopped_scope_is_working(session_id, &scope)
        else {
            return false;
        };
        self.agents_delayed_send_generation = self.agents_delayed_send_generation.wrapping_add(1);
        let generation = self.agents_delayed_send_generation;
        self.agents_delayed_send_timers.remove(&session_id);
        self.agents_send_when_stopped_watchers.insert(
            session_id,
            GpuiAgentsSendWhenStoppedWatcher {
                generation,
                non_working_since: (!is_working).then(Instant::now),
                scope,
            },
        );
        self.spawn_gpui_agents_send_when_stopped_poll(session_id, generation, cx);
        self.ensure_gpui_agents_delayed_send_persistence_ticker(cx);
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        self.refresh_sidebar_agents_delayed_sends_if_changed(cx);
        self.persist_shell_layout_state();
        cx.notify();
        true
    }

    pub(crate) fn spawn_gpui_agents_delayed_send_fire(
        &self,
        session_id: TerminalSessionId,
        generation: u64,
        duration: Duration,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(duration).await;
            let _ = this.update(cx, |this, cx| {
                this.fire_gpui_agents_delayed_send(session_id, generation, cx);
            });
        })
        .detach();
    }

    pub(crate) fn spawn_gpui_agents_send_when_stopped_poll(
        &self,
        session_id: TerminalSessionId,
        generation: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(GPUI_AGENTS_SEND_WHEN_STOPPED_POLL_INTERVAL)
                    .await;
                let keep_watching = this
                    .update(cx, |this, cx| {
                        this.poll_gpui_agents_send_when_stopped(session_id, generation, cx)
                    })
                    .unwrap_or(false);
                if !keep_watching {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn gpui_agents_send_when_stopped_scope_is_working(
        &self,
        session_id: TerminalSessionId,
        scope: &GpuiAgentsSendWhenStoppedScope,
    ) -> Option<bool> {
        match scope {
            GpuiAgentsSendWhenStoppedScope::Session => {
                self.agents_workspace.session(session_id).map(|session| {
                    session.presentation_state == TerminalSessionPresentationState::Running
                        && session.activity == AgentTerminalActivity::Working
                })
            }
            GpuiAgentsSendWhenStoppedScope::Project(project_id) => {
                let target_belongs_to_project =
                    self.local_workspace_session_mappings
                        .iter()
                        .any(|(key, mapped_session_id)| {
                            *mapped_session_id == session_id && key.project_id == *project_id
                        });
                if !target_belongs_to_project {
                    return None;
                }
                let mut found_project_session = false;
                for (key, mapped_session_id) in &self.local_workspace_session_mappings {
                    if key.project_id != *project_id {
                        continue;
                    }
                    let Some(session) = self.agents_workspace.session(*mapped_session_id) else {
                        continue;
                    };
                    found_project_session = true;
                    if session.presentation_state == TerminalSessionPresentationState::Running
                        && session.activity == AgentTerminalActivity::Working
                    {
                        return Some(true);
                    }
                }
                found_project_session.then_some(false)
            }
        }
    }

    pub(crate) fn ensure_gpui_agents_delayed_send_countdown_ticker(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.agents_delayed_send_countdown_ticker_active
            || self.agents_delayed_send_timers.is_empty()
        {
            return;
        }
        self.agents_delayed_send_countdown_ticker_active = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let keep_ticking = this
                    .update(cx, |this, cx| {
                        if this.agents_delayed_send_timers.is_empty() {
                            this.agents_delayed_send_countdown_ticker_active = false;
                            return false;
                        }
                        this.refresh_sidebar_agents_delayed_sends_if_changed(cx);
                        true
                    })
                    .unwrap_or(false);
                if !keep_ticking {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn ensure_gpui_agents_delayed_send_persistence_ticker(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.agents_delayed_send_persistence_ticker_active
            || (self.agents_delayed_send_timers.is_empty()
                && self.agents_send_when_stopped_watchers.is_empty())
        {
            return;
        }
        self.agents_delayed_send_persistence_ticker_active = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(COMMAND_PANE_DELAYED_SEND_PERSIST_INTERVAL)
                    .await;
                let keep_running = this
                    .update(cx, |this, _cx| {
                        if this.agents_delayed_send_timers.is_empty()
                            && this.agents_send_when_stopped_watchers.is_empty()
                        {
                            this.agents_delayed_send_persistence_ticker_active = false;
                            false
                        } else {
                            this.persist_shell_layout_state();
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

    pub(crate) fn poll_gpui_agents_send_when_stopped(
        &mut self,
        session_id: TerminalSessionId,
        generation: u64,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(watcher) = self
            .agents_send_when_stopped_watchers
            .get(&session_id)
            .cloned()
        else {
            return false;
        };
        if watcher.generation != generation {
            return false;
        }
        let Some(presentation_state) = self
            .agents_workspace
            .session(session_id)
            .map(|session| session.presentation_state)
        else {
            self.agents_send_when_stopped_watchers.remove(&session_id);
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
            self.refresh_sidebar_agents_delayed_sends_if_changed(cx);
            self.persist_shell_layout_state();
            cx.notify();
            return false;
        };
        if presentation_state != TerminalSessionPresentationState::Running {
            self.agents_send_when_stopped_watchers.remove(&session_id);
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
            self.refresh_sidebar_agents_delayed_sends_if_changed(cx);
            self.persist_shell_layout_state();
            cx.notify();
            return false;
        }
        let Some(is_working) =
            self.gpui_agents_send_when_stopped_scope_is_working(session_id, &watcher.scope)
        else {
            self.agents_send_when_stopped_watchers.remove(&session_id);
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
            self.refresh_sidebar_agents_delayed_sends_if_changed(cx);
            self.persist_shell_layout_state();
            cx.notify();
            return false;
        };
        if is_working {
            if watcher.non_working_since.is_some() {
                if let Some(current) = self.agents_send_when_stopped_watchers.get_mut(&session_id) {
                    current.non_working_since = None;
                }
                self.refresh_sidebar_agents_delayed_sends_if_changed(cx);
                cx.notify();
            }
            return true;
        }

        let now = Instant::now();
        let Some(non_working_since) = watcher.non_working_since else {
            if let Some(current) = self.agents_send_when_stopped_watchers.get_mut(&session_id) {
                current.non_working_since = Some(now);
            }
            self.refresh_sidebar_agents_delayed_sends_if_changed(cx);
            cx.notify();
            return true;
        };
        if now.saturating_duration_since(non_working_since)
            < GPUI_AGENTS_SEND_WHEN_STOPPED_STABILITY_DURATION
        {
            self.refresh_sidebar_agents_delayed_sends_if_changed(cx);
            return true;
        }

        self.agents_send_when_stopped_watchers.remove(&session_id);
        let sent = self
            .gpui_agents_delayed_send_mount_target(session_id)
            .is_some_and(|target| {
                self.send_return_key_to_gpui_agents_delayed_send_target(target, cx)
            });
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        self.refresh_sidebar_agents_delayed_sends_if_changed(cx);
        self.persist_shell_layout_state();
        cx.notify();
        if !sent {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Delayed Send skipped",
                "The terminal was no longer available.",
                cx,
            );
        }
        false
    }

    pub(crate) fn fire_gpui_agents_delayed_send(
        &mut self,
        session_id: TerminalSessionId,
        generation: u64,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(timer) = self.agents_delayed_send_timers.get(&session_id).copied() else {
            return false;
        };
        if timer.generation != generation {
            return false;
        }
        self.agents_delayed_send_timers.remove(&session_id);
        let session_still_exists = self.agents_workspace.session(session_id).is_some();
        let sent = self
            .gpui_agents_delayed_send_mount_target(session_id)
            .is_some_and(|target| {
                self.send_return_key_to_gpui_agents_delayed_send_target(target, cx)
            });
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        self.refresh_sidebar_agents_delayed_sends_if_changed(cx);
        self.persist_shell_layout_state();
        cx.notify();
        if !sent && session_still_exists {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Delayed Send skipped",
                "The terminal was no longer available.",
                cx,
            );
        }
        sent
    }

    pub(crate) fn gpui_agents_delayed_send_mount_target(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<GpuiAgentsDelayedSendTarget> {
        let session = self.agents_workspace.session(session_id)?;
        if session.presentation_state != TerminalSessionPresentationState::Running {
            return None;
        }
        let runtime_session_id = self
            .agents_terminal_runtime_sessions
            .runtime_session_id_for_shell_session(session_id)?;

        if self
            .agents_gpui_engine_terminals
            .get(&session_id)
            .is_some_and(|record| record.runtime_session_id == runtime_session_id)
        {
            return Some(GpuiAgentsDelayedSendTarget::GpuiEngine {
                session_id,
                runtime_session_id,
            });
        }

        #[cfg(target_os = "macos")]
        if let Some(pane_id) = self.agents_workspace.pane_id_for_session(session_id) {
            let slot_id = AgentsTerminalBodyMountSlotId {
                pane_id,
                session_id,
            };
            if self.agents_terminal_ghostty_surface_matches(slot_id) {
                return Some(GpuiAgentsDelayedSendTarget::AgentsNative(slot_id));
            }
        }

        #[cfg(target_os = "macos")]
        if let Some(slot_id) = self
            .current_project_editor_companion_terminal_body_mount_slots()
            .into_iter()
            .find(|slot_id| {
                slot_id.session_id == session_id
                    && self.project_editor_companion_terminal_ghostty_surface_matches(*slot_id)
            })
        {
            return Some(GpuiAgentsDelayedSendTarget::ProjectEditorCompanionNative(
                slot_id,
            ));
        }

        #[cfg(target_os = "macos")]
        if self
            .agents_terminal_parked_runtime_owners
            .get(&runtime_session_id)
            .is_some_and(|owner| {
                owner.matches_identity(runtime_session_id, session_id, owner.mount_slot_id)
            })
        {
            return Some(GpuiAgentsDelayedSendTarget::AgentsParkedNative(
                runtime_session_id,
            ));
        }

        None
    }

    pub(crate) fn send_return_key_to_gpui_agents_delayed_send_target(
        &mut self,
        target: GpuiAgentsDelayedSendTarget,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        match target {
            GpuiAgentsDelayedSendTarget::GpuiEngine {
                session_id,
                runtime_session_id,
            } => {
                let Some(record) = self.agents_gpui_engine_terminals.get(&session_id) else {
                    return false;
                };
                if record.runtime_session_id != runtime_session_id {
                    return false;
                }
                let view = record.view.clone();
                view.update(cx, |view, cx| view.send_return_key(cx));
                true
            }
            GpuiAgentsDelayedSendTarget::AgentsNative(slot_id) => {
                self.send_return_key_to_mounted_agents_terminal_surface(slot_id, cx)
            }
            GpuiAgentsDelayedSendTarget::ProjectEditorCompanionNative(slot_id) => self
                .send_return_key_to_mounted_project_editor_companion_terminal_surface(slot_id, cx),
            #[cfg(target_os = "macos")]
            GpuiAgentsDelayedSendTarget::AgentsParkedNative(runtime_session_id) => {
                self.send_return_key_to_parked_agents_terminal_surface(runtime_session_id)
            }
        }
    }

    pub(crate) fn prune_gpui_command_delayed_send_timers_for_command_model(&mut self) -> bool {
        let stale_session_ids = command_delayed_send_stale_runtime_timer_session_ids(
            &self.command_pane,
            &self.command_delayed_send_timers,
        );
        let changed = !stale_session_ids.is_empty();
        for session_id in stale_session_ids {
            self.command_delayed_send_timers.remove(&session_id);
            if let Some(session) = self.command_pane.session_mut(session_id)
                && session.delayed_send_timer_owned
            {
                session.set_delayed_send_active(false, false);
            }
        }
        changed
    }

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
        let Some(key) = self.local_workspace_key_for_shell_session(shell_session_id) else {
            return false;
        };
        self.dispatch_gpui_sidebar_host_message(
            serde_json::json!({
                "sessionId": gpui_combined_presentation_session_id(
                    &key.project_id,
                    &key.session_id,
                ),
                "type": "toggleCloseAfterDone",
            }),
            cx,
        )
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

    pub(crate) fn handle_gpui_app_modal_sidebar_command(
        &mut self,
        message: serde_json::Value,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(command) = message
            .get("message")
            .and_then(serde_json::Value::as_object)
        else {
            return;
        };
        let Some(command_type) = command.get("type").and_then(serde_json::Value::as_str) else {
            return;
        };

        match command_type {
            "updateSettings" => {
                self.handle_gpui_app_modal_update_settings_message(
                    &serde_json::Value::Object(command.clone()),
                    cx,
                );
            }
            "updateSettingsPatch" => {
                self.handle_gpui_app_modal_update_settings_patch_message(
                    &serde_json::Value::Object(command.clone()),
                    cx,
                );
            }
            "openExternalUrl" => {
                self.receive_gpui_titlebar_resources_open_external_url_message(
                    &serde_json::Value::Object(command.clone()),
                );
            }
            "listAppIcons" => {
                self.handle_gpui_list_app_icons_message(cx);
            }
            "setAppIcon" => {
                self.handle_gpui_set_app_icon_message(
                    &serde_json::Value::Object(command.clone()),
                    cx,
                );
            }
            "pickAppIconFile" => {
                self.handle_gpui_pick_app_icon_file_message(cx);
            }
            "pickTerminalBackgroundImageFile" => {
                self.handle_gpui_pick_terminal_background_image_message(cx);
            }
            "pickFirstLaunchProjectFolder" => {
                self.handle_gpui_pick_first_launch_project_folder_message(cx);
            }
            "firstLaunchCreateProjectSession" => {
                self.handle_gpui_first_launch_create_project_session_message(command, cx);
            }
            "revealAppIconsFolder" => {
                app_icon::reveal_icons_directory();
            }
            "saveRemoteMachinePassword" => {
                self.handle_gpui_save_remote_machine_password_message(command, cx);
            }
            "reconnectRemoteMachine" => {
                self.handle_gpui_reconnect_remote_machine_message(command, cx);
            }
            "probeRemoteGxserverInstall" => {
                self.handle_gpui_probe_remote_gxserver_install_message(command, cx);
            }
            "remoteGxserverSubscribePresentation" => {
                self.handle_gpui_remote_gxserver_subscribe_presentation_message(command, cx);
            }
            "browseRemoteProjectDirectories" => {
                self.handle_gpui_browse_remote_project_directories_message(command, cx);
            }
            "addRemoteProjectPath" => {
                self.handle_gpui_add_remote_project_path_message(command, cx);
            }
            "addProjectDialogRequest" => {
                self.handle_gpui_add_project_dialog_request_message(command, cx);
            }
            "pickReplacementProjectFolder" => {
                let Some(project_id) = command
                    .get("projectId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|project_id| gpui_remote_sidebar_project_id_allowed(project_id))
                    .map(str::to_string)
                else {
                    return;
                };
                self.handle_gpui_pick_replacement_project_folder_message(project_id, cx);
            }
            /*
            CDXC:SessionNotes 2026-08-24:
            The Session Note dialog's confirm. Like `removeProject`, this is a
            sidebar-owned write that happens to be issued from an app-modal
            window, so it is forwarded to the sidebar runtime rather than acted
            on here: that runtime owns the gxserver client and the local/remote
            machine routing. Only the sidebar session id and the note text
            cross this boundary, and the note is never logged.
            */
            "setSessionNote" => {
                let Some(session_id) = command
                    .get("sessionId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|session_id| gpui_app_modal_sidebar_session_id_allowed(session_id))
                else {
                    return;
                };
                let Some(note) = command.get("note").and_then(serde_json::Value::as_str) else {
                    return;
                };
                let mut message = serde_json::json!({
                    "note": note,
                    "sessionId": session_id,
                    "type": "setSessionNote",
                });
                if let Some(project_id) = command
                    .get("projectId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|project_id| gpui_remote_sidebar_project_id_allowed(project_id))
                {
                    message["projectId"] = serde_json::json!(project_id);
                }
                self.dispatch_gpui_sidebar_host_message(message, cx);
            }
            /*
            CDXC:Spaces 2026-08-27:
            The New/Edit Space dialog's confirm and delete. Like `setSessionNote`
            this is a sidebar-owned write issued from an app-modal window, so it
            is forwarded to the sidebar rather than acted on here — and unlike
            `setSessionNote`, its owner is SidebarApp itself, because the Space
            document lives in React state and the edit must be applied to the
            CURRENT one.
            */
            "sidebarSpaceEditorResult" => {
                self.forward_gpui_sidebar_space_editor_result_to_sidebar(command, cx);
            }
            "confirmAgentHookLaunch" => {
                let bounded_text = |key: &str, max_len: usize| {
                    command
                        .get(key)
                        .and_then(serde_json::Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty() && value.len() <= max_len)
                };
                let Some(agent_id) = bounded_text("agentId", 128) else {
                    return;
                };
                let Some(hook_agent_id) = bounded_text("hookAgentId", 128) else {
                    return;
                };
                let Some(install_hooks) = command
                    .get("installHooks")
                    .and_then(serde_json::Value::as_bool)
                else {
                    return;
                };
                let mut message = serde_json::json!({
                    "agentId": agent_id,
                    "hookAgentId": hook_agent_id,
                    "installHooks": install_hooks,
                    "type": "confirmAgentHookLaunch",
                });
                if let Some(group_id) = bounded_text("groupId", 512) {
                    message["groupId"] = serde_json::json!(group_id);
                }
                self.dispatch_gpui_sidebar_host_message(message, cx);
            }
            "removeProject" => {
                let Some(project_id) = command
                    .get("projectId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|project_id| gpui_remote_sidebar_project_id_allowed(project_id))
                else {
                    return;
                };
                self.dispatch_gpui_sidebar_host_message(
                    serde_json::json!({
                        "projectId": project_id,
                        "type": "removeProject",
                    }),
                    cx,
                );
                self.close_gpui_app_modal_window_and_restore_command_focus(cx);
            }
            "gpuiRemoteGxserverSidebarRequest" => {
                self.handle_gpui_remote_gxserver_sidebar_request_message(command, cx);
            }
            "requestProjectWorktrees"
            | "createProjectWorktree"
            | "confirmDeleteWorktree"
            | "confirmRenameWorktree"
            | "commitWorktreeBeforeDelete" => {
                self.forward_gpui_worktree_modal_command_to_sidebar(command_type, command, cx);
            }
            "confirmSidebarGitCommit"
            | "confirmSidebarGitDirectMerge"
            | "runSidebarGitMultipleCommits"
            | "openSidebarGitChangedFileDiff"
            | "cancelSidebarGitCommit" => {
                self.forward_gpui_git_commit_modal_command_to_sidebar(command_type, command, cx);
            }
            "revealExportedTranscript" => {
                self.reveal_gpui_exported_transcript(cx);
            }
            "cancelExportSessionTranscript"
            | "startExportedTranscriptConversation"
            | "runExportSessionTranscript" => {
                self.forward_gpui_export_transcript_modal_command_to_sidebar(
                    command_type,
                    command,
                    cx,
                );
            }
            "openBrowserPane" => {
                let Some(url) = command
                    .get("url")
                    .and_then(serde_json::Value::as_str)
                    .filter(|url| self.gpui_titlebar_browser_url_allowed(url))
                    .map(str::to_string)
                else {
                    return;
                };
                if self.titlebar_resources_panel_open {
                    self.set_gpui_titlebar_resources_panel_open(false, window, cx);
                } else {
                    self.set_gpui_titlebar_tips_panel_open(false, window, cx);
                }
                self.open_gpui_browser_action_url(url, window, cx);
            }
            "openWorkspaceWelcome" => {
                self.set_gpui_titlebar_tips_panel_open(false, window, cx);
                self.open_gpui_app_modal_from_titlebar(
                    GpuiAppModalKind::FirstLaunchSetup,
                    window,
                    cx,
                );
            }
            "openGhostexTutorialVideo" => {
                self.set_gpui_titlebar_tips_panel_open(false, window, cx);
                self.open_gpui_app_modal_from_titlebar(
                    GpuiAppModalKind::WatchGhostexVideo,
                    window,
                    cx,
                );
            }
            "runGhostexHotkeyAction" => {
                let Some(action_id) = command.get("actionId").and_then(serde_json::Value::as_str)
                else {
                    return;
                };
                /*
                CDXC:FocusMode 2026-06-25-15:01:
                The shared command palette posts focused-session commands as `runGhostexHotkeyAction`. Handle command-pane Sleep/Wake/Close focused-session ids directly in GPUI before modal routing so command-palette rows operate on the shell-focused command tab instead of no-oping or trying to open another modal.

                CDXC:DelayedSend 2026-06-27-06:37:
                The shared Delayed Send row is also a focused-pane action, but native command terminals consume it through the command-panel titlebar default no-op. GPUI must consume the id before generic modal routing without opening the focused command-pane timer modal.

                CDXC:Sessions 2026-06-25-15:24:
                The shared Close After Done row is also a focused command-terminal action. In GPUI command panes it toggles the focused mounted command tab's terminal-scoped watcher before modal routing, matching native command-palette behavior without applying the timer to Agents, Browser, or project-editor focus.

                CDXC:CommandPane 2026-06-25-16:33:
                Rename Active Session is also a focused command-terminal action. When the command pane owns shell focus, open the shared Rename Session modal for the active command tab instead of falling through to unrelated app-modal commands.

                CDXC:CommandPalette 2026-06-25-17:32:
                The shared command palette sends focused-pane split/open/merge actions through the same `runGhostexHotkeyAction` bridge as focused-session actions. Route the supported GPUI pane actions to the existing shell hotkey helpers before modal routing so command-pane focus can create command splits and Browser opens without requiring a separate keybinding event.

                CDXC:CommandPalette 2026-06-26-07:24:
                Command-palette Create Session is ordinary focused hotkey behavior in GPUI. Dispatch it to the same Cmd+T helper before app-modal routing so command-pane focus and Agents-pane focus keep their existing source gates and placeholder semantics.

                CDXC:CommandPalette 2026-06-26-07:24:
                Shared workarea switch rows also arrive as hotkey actions. Route them before app-modal fallback through `switch_workarea_from_hotkey` so command-palette selection uses the same titlebar availability checks, no-wake lifecycle, focus target, Browser visibility, and persistence behavior as Option+1..5.

                CDXC:CommandPalette 2026-06-26-07:36:
                Command-palette focus-navigation rows are shell navigation, not app-modal commands. Route tab cycling and directional focus through the same GPUI keyboard helpers as direct hotkeys so command-pane, Agents, Browser, and project-editor focus keep their existing source gates and layout semantics.

                CDXC:CommandPalette 2026-06-26-10:04:
                Shared previous/next group focus is render-order navigation, not spatial arrow focus. Dispatch `focusPreviousGroup` and `focusNextGroup` directly through the existing render-order workspace traversal only from Agents-pane or command-pane focus so GPUI moves like native focusAdjacentGroup without adding numbered group slots, project jumps, or fallback guessing.

                CDXC:CommandPalette 2026-06-26-10:04:
                Command-palette Start Action 1-5 rows are positional titlebar Actions hotkeys. Dispatch them through the existing titlebar action index runner so GPUI executes the configured project action without adding renderer payloads containing command text, URLs, paths, or session data.

                CDXC:Sidebar 2026-06-26-10:04:
                `toggleSidebarCollapsed` is shell chrome, not a modal command. Route it before app-modal fallback so the command-palette row and Cmd+B hide or restore the GPUI sidebar and divider while preserving the expanded sidebar width.

                CDXC:Sidebar 2026-06-26-23:35:
                `moveSidebar` is also shell chrome, not an app-modal command. Route it beside collapse handling so the shared command-palette row flips GPUI's normal-layout sidebar side and persists `sidebarSide` without opening fallback UI.

                CDXC:CommandPalette 2026-06-26-23:20:
                Numbered session-slot rows are delegated to SidebarApp as nativeHotkey messages because rendered sidebar slot order is the only safe owner for `focusSessionSlot1..9`. Previous/next session remains GPUI tab-cycle routing, and jump-to-project ids must not enter this bounce path because SidebarApp forwards those back to native.

                CDXC:Hotkeys 2026-06-26-23:42:
                Project jump rows also depend on SidebarApp's rendered project order, but they must use the dedicated `gpuiProjectSlotHotkey` host message instead of `nativeHotkey` so SidebarApp resolves the slot locally without forwarding the same `jumpToProject*` id back to GPUI.
                */
                if self.run_gpui_terminal_toolbar_hotkey_action(action_id, window, cx) {
                    return;
                }
                if action_id == "toggleChatView" {
                    /*
                    CDXC:SessionChat 2026-07-31:
                    Chat View toggling must work while the terminal is hidden
                    behind the chat surface, so it resolves the focused Agents
                    session directly instead of requiring a focused terminal
                    view like the other toolbar actions.
                    */
                    self.toggle_agents_session_chat_mode_for_focused_session(cx);
                    return;
                }
                if let Some(mode) = gpui_command_palette_switch_workarea_hotkey_mode(action_id) {
                    self.switch_workarea_from_hotkey(mode, window, cx);
                    return;
                }
                if let Some(action_index) = gpui_command_palette_action_slot_index(action_id) {
                    self.run_configured_gpui_titlebar_action_index(action_index, window, cx);
                    return;
                }
                if let Some(direction) =
                    navigation_history::navigation_history_hotkey_direction(action_id)
                {
                    /*
                    CDXC:Navigation 2026-08-19:
                    Back/Forward is shell navigation, not an app-modal command,
                    and it is owned by the sidebar runtime rather than Rust —
                    the keypress takes the exact same route as a click on the
                    titlebar arrows.
                    */
                    self.request_navigation_history_navigation(direction, cx);
                    return;
                }
                if action_id == "toggleSidebarCollapsed" {
                    self.toggle_gpui_sidebar_collapsed(cx);
                    return;
                }
                if action_id == "toggleCompanionPane" {
                    self.toggle_project_editor_companion_from_hotkey(window, cx);
                    return;
                }
                if action_id == "moveSidebar" {
                    self.move_gpui_sidebar_to_other_side(cx);
                    return;
                }
                if action_id == "openExtensions" {
                    self.open_gpui_settings_extensions_page(Some(window), cx);
                    return;
                }
                if let Some(tab_cycle_action) =
                    gpui_command_palette_tab_cycle_hotkey_action(action_id)
                {
                    self.cycle_focused_tab(tab_cycle_action.reverse(), window, cx);
                    return;
                }
                if let Some(direction) =
                    gpui_command_palette_adjacent_group_focus_direction(action_id)
                {
                    if gpui_command_palette_adjacent_group_focus_source_allowed(self.shell_focus)
                        && self.focus_workspace_direction_by_render_order(direction, cx)
                    {
                        cx.notify();
                    }
                    return;
                }
                if let Some(direction) =
                    WorkspaceFocusDirection::from_command_palette_directional_focus_action_id(
                        action_id,
                    )
                {
                    self.focus_workspace_direction(direction, window, cx);
                    return;
                }
                match gpui_focused_pane_hotkey_action(action_id) {
                    Some(GpuiFocusedPaneHotkeyAction::CreateSession) => {
                        self.add_terminal_placeholder_tab_from_hotkey(window, cx);
                        return;
                    }
                    Some(GpuiFocusedPaneHotkeyAction::OpenCommandsPanel) => {
                        self.open_command_pane_from_command_palette(window, cx);
                        return;
                    }
                    Some(GpuiFocusedPaneHotkeyAction::OpenBrowserPane) => {
                        self.add_browser_tab_from_hotkey(window, cx);
                        return;
                    }
                    Some(GpuiFocusedPaneHotkeyAction::SplitRight) => {
                        self.split_focused_terminal_from_hotkey(
                            FocusedTerminalSplitDirection::Right,
                            cx,
                        );
                        return;
                    }
                    Some(GpuiFocusedPaneHotkeyAction::SplitDown) => {
                        self.split_focused_terminal_from_hotkey(
                            FocusedTerminalSplitDirection::Down,
                            cx,
                        );
                        return;
                    }
                    Some(GpuiFocusedPaneHotkeyAction::MergeAllTabs) => {
                        self.merge_all_agents_tabs_from_hotkey(cx);
                        return;
                    }
                    Some(GpuiFocusedPaneHotkeyAction::RotatePanesClockwise) => {
                        self.rotate_agents_panes_from_hotkey(cx);
                        return;
                    }
                    Some(GpuiFocusedPaneHotkeyAction::RuntimeNoOp(runtime_action)) => {
                        match runtime_action {
                            GpuiFocusedPaneRuntimeAction::ForkSession => {
                                if let Some(shell_session_id) =
                                    self.focused_agents_workspace_shell_session_id()
                                {
                                    let _ = self.dispatch_gpui_workspace_terminal_runtime_action(
                                        "forkSession",
                                        shell_session_id,
                                        cx,
                                    );
                                }
                            }
                            GpuiFocusedPaneRuntimeAction::ReloadSession => {
                                if let Some(shell_session_id) =
                                    self.focused_agents_workspace_shell_session_id()
                                {
                                    let _ = self.dispatch_gpui_workspace_terminal_runtime_action(
                                        "fullReloadSession",
                                        shell_session_id,
                                        cx,
                                    );
                                }
                            }
                            GpuiFocusedPaneRuntimeAction::PopOutPane => {}
                        }
                        return;
                    }
                    Some(GpuiFocusedPaneHotkeyAction::CommandSession(command_action)) => {
                        match command_action {
                            GpuiCommandPaneFocusedSessionHotkeyAction::Rename => {
                                if !self.open_gpui_rename_session_modal_for_focused_command_pane(cx)
                                {
                                    let _ = self
                                        .open_gpui_rename_session_modal_for_focused_agents_session(
                                            cx,
                                        );
                                }
                            }
                            GpuiCommandPaneFocusedSessionHotkeyAction::DelayedSend => {
                                if !self.open_gpui_delayed_send_modal_for_focused_command_pane(cx) {
                                    let _ = self
                                        .open_gpui_delayed_send_modal_for_focused_agents_session(
                                            cx,
                                        );
                                }
                            }
                            GpuiCommandPaneFocusedSessionHotkeyAction::CloseAfterDone => {
                                if !self
                                    .toggle_gpui_command_close_after_done_for_focused_command_pane(
                                        cx,
                                    )
                                {
                                    let _ = self
                                        .toggle_gpui_close_after_done_for_focused_agents_session(
                                            cx,
                                        );
                                }
                            }
                            GpuiCommandPaneFocusedSessionHotkeyAction::Sleep => {
                                self.sleep_focused_command_pane_session(cx);
                            }
                            GpuiCommandPaneFocusedSessionHotkeyAction::Wake => {
                                self.wake_focused_command_pane_session(cx);
                            }
                            GpuiCommandPaneFocusedSessionHotkeyAction::Close => {
                                if focused_command_pane_close_target(
                                    self.shell_focus,
                                    &self.command_pane,
                                )
                                .is_some()
                                {
                                    self.close_focused_surface(window, cx);
                                }
                            }
                        }
                        return;
                    }
                    None => {}
                }
                if let Some(sidebar_action_id) =
                    gpui_command_palette_sidebar_slot_hotkey_action_id(action_id)
                {
                    self.dispatch_gpui_sidebar_host_message(
                        serde_json::json!({
                            "type": "nativeHotkey",
                            "actionId": sidebar_action_id,
                        }),
                        cx,
                    );
                    return;
                }
                if let Some(slot_number) =
                    gpui_command_palette_project_slot_hotkey_number(action_id)
                {
                    self.dispatch_gpui_sidebar_host_message(
                        serde_json::json!({
                            "type": "gpuiProjectSlotHotkey",
                            "slotNumber": slot_number,
                        }),
                        cx,
                    );
                    return;
                }
                let Some(modal) = gpui_app_modal_kind_for_hotkey_action_id(action_id) else {
                    return;
                };
                let sidebar_state_message =
                    self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
                let mut open_message = modal.open_message();
                if modal.requires_sidebar_state() {
                    open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
                }
                self.open_gpui_app_modal_window(
                    modal,
                    open_message,
                    sidebar_state_message,
                    None,
                    cx,
                );
            }
            "refreshDaemonSessions" => {
                self.refresh_gpui_daemon_sessions_state_in_background(None, cx);
            }
            "savePinnedPrompt" => {
                self.handle_gpui_save_pinned_prompt_command(command, cx);
            }
            "renameSession" => {
                self.handle_gpui_rename_command_session_command(command, cx);
            }
            "scheduleDelayedSend" => {
                self.handle_gpui_schedule_delayed_send_command(command, cx);
            }
            "cancelDelayedSend" => {
                self.handle_gpui_cancel_delayed_send_command(command, cx);
            }
            "toggleCloseAfterDone" => {
                self.handle_gpui_toggle_close_after_done_command(command, cx);
            }
            "killDaemonSession" => {
                let project_id = command
                    .get("workspaceId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let session_id = command
                    .get("sessionId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let active_project_id = self.gpui_daemon_sessions_active_project_id();
                let focused_session_id = self.gpui_daemon_sessions_focused_session_id();
                self.run_gpui_app_modal_sidebar_status_task(
                    move || {
                        gpui_close_daemon_session_and_refresh_state(
                            project_id,
                            session_id,
                            active_project_id.as_deref(),
                            focused_session_id.as_deref(),
                        )
                    },
                    cx,
                );
            }
            "killTerminalDaemon" => {
                let dispatched = self.dispatch_gpui_workspace_sleep_all_daemon_sessions(cx);
                self.refresh_gpui_daemon_sessions_state_in_background(
                    (!dispatched).then(|| {
                        "The sidebar runtime is not ready to stop local terminal sessions. The list was refreshed without changing daemon state.".to_string()
                    }),
                    cx,
                );
            }
            "requestGhostexCliStatus" => {
                self.run_gpui_app_modal_and_titlebar_status_task(
                    || gpui_ghostex_cli_status_message(None),
                    cx,
                );
            }
            "installGhostexCli" => {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::InstallGhostexCli,
                    cx,
                );
            }
            "installBrowserControl" => {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::InstallBrowserControl,
                    cx,
                );
            }
            "installBrowserUseSkill" => {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::InstallBrowserUseSkill,
                    cx,
                );
            }
            "installComputerUseSkill" => {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::InstallComputerUseSkill,
                    cx,
                );
            }
            "installCliSkill" => {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::InstallCliSkill,
                    cx,
                );
            }
            "installFable56OrchestrationSkill" => {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::InstallFable56OrchestrationSkill,
                    cx,
                );
            }
            "installManageBeadsSkill" => {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::InstallManageBeadsSkill,
                    cx,
                );
            }
            "installGenerateTitleSkill" => {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::InstallGenerateTitleSkill,
                    cx,
                );
            }
            "installMoveCodexSessionSkill" => {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::InstallMoveCodexSessionSkill,
                    cx,
                );
            }
            "installCuaDriver" => {
                self.handle_gpui_cua_driver_install_or_update(window, cx);
            }
            "uninstallBundledAgentSkills" => {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::UninstallBundledAgentSkills,
                    cx,
                );
            }
            "uninstallBundledAgentSkill" => {
                if let Some(skill_name) = command
                    .get("skillId")
                    .and_then(serde_json::Value::as_str)
                    .and_then(gpui_bundled_agent_skill_name)
                {
                    self.run_gpui_ghostex_cli_settings_action(
                        GpuiGhostexCliSettingsAction::UninstallBundledAgentSkill(skill_name),
                        cx,
                    );
                }
            }
            "requestAgentHookStatus" => {
                let agent_ids = gpui_settings_command_ordered_agent_ids(command);
                self.run_gpui_progressive_agent_hook_status_task(agent_ids, cx);
            }
            "installAgentHooks" => {
                let agent_ids = gpui_settings_command_agent_ids(command);
                self.run_gpui_app_modal_sidebar_status_task(
                    move || {
                        gpui_agent_hook_status_message(
                            "/api/installAgentHooks",
                            agent_ids,
                            "Agent hook install failed.",
                        )
                    },
                    cx,
                );
            }
            "uninstallAgentHooks" => {
                let agent_ids = gpui_settings_command_agent_ids(command);
                self.run_gpui_app_modal_sidebar_status_task(
                    move || {
                        gpui_agent_hook_status_message(
                            "/api/uninstallAgentHooks",
                            agent_ids,
                            "Agent hook uninstall failed.",
                        )
                    },
                    cx,
                );
            }
            "requestOSIntegrationStatus" => {
                self.run_gpui_app_modal_sidebar_status_task(gpui_os_integration_status_message, cx);
            }
            "requestPluginSettingsStatus" => {
                self.request_plugin_settings_status(cx);
            }
            "reinstallPlugin" => {
                if let Some(plugin_id) = command.get("pluginId").and_then(serde_json::Value::as_str)
                {
                    self.reinstall_plugin_from_settings(plugin_id, cx);
                }
            }
            "setOSIntegrationDefaults" => {
                let target = command
                    .get("target")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                self.run_gpui_app_modal_sidebar_status_task(
                    move || gpui_set_os_integration_defaults_status_message(target.as_deref()),
                    cx,
                );
            }
            "requestGhostexFolderStats" => {
                self.run_gpui_app_modal_sidebar_status_task(gpui_ghostex_folder_stats_message, cx);
            }
            "openGhostexFolder" => {
                self.open_gpui_ghostex_folder(cx);
            }
            "requestAgentsHubCatalog" => {
                /*
                CDXC:AgentLauncher 2026-06-24-12:26:
                Agents Hub catalog requests return metadata-only rows through the existing app-modal sidebarState path. File bodies stay out of the open/catalog message and are read only by requestAgentsHubFileContent after Rust validates the selected file against the generated Hub catalog.
                */
                self.run_gpui_app_modal_sidebar_status_task(gpui_agents_hub_catalog_message, cx);
            }
            "requestAgentsHubFileContent" => {
                let file_path = command
                    .get("filePath")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let request_id = command
                    .get("requestId")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                self.run_gpui_app_modal_sidebar_status_task(
                    move || gpui_agents_hub_file_content_message(file_path, request_id),
                    cx,
                );
            }
            "saveAgentsHubFile" => {
                self.handle_gpui_save_agents_hub_file_command(command, cx);
            }
            "openAgentsHubPathInFinder" => {
                self.open_gpui_agents_hub_path_in_finder(command, cx);
            }
            "openAgentsHubFileInBuiltInEditor" => {
                self.open_gpui_agents_hub_file_in_built_in_editor(command, window, cx);
            }
            "openGhosttySettingsDocs" => {
                self.open_gpui_trusted_url(
                    GPUI_GHOSTTY_SETTINGS_DOCS_URL,
                    "openGhosttySettingsDocs",
                    cx,
                );
            }
            "openAccessibilityPreferences" => {
                self.open_gpui_macos_system_settings_url(
                    GPUI_MACOS_ACCESSIBILITY_PREFERENCES_URL,
                    "openAccessibilityPreferences",
                    cx,
                );
            }
            "openScreenRecordingPreferences" => {
                self.open_gpui_macos_system_settings_url(
                    GPUI_MACOS_SCREEN_RECORDING_PREFERENCES_URL,
                    "openScreenRecordingPreferences",
                    cx,
                );
            }
            "openMacOSNotificationSettings" => {
                self.open_gpui_macos_system_settings_url(
                    GPUI_MACOS_NOTIFICATION_SETTINGS_URL,
                    "openMacOSNotificationSettings",
                    cx,
                );
            }
            "requestMacOSNotificationPermission" => {
                self.request_gpui_macos_notification_permission(cx);
            }
            "playCompletionSoundPreview" => {
                self.play_gpui_completion_sound_preview(
                    command.get("sound").and_then(serde_json::Value::as_str),
                    cx,
                );
            }
            "testAgentTaskCompletion" => {
                self.test_gpui_agent_task_completion(cx);
            }
            "applyRecommendedGhosttySettings" => {
                self.update_gpui_ghostty_visible_settings(
                    shared_settings::apply_recommended_ghostty_visible_settings,
                    shared_settings::apply_recommended_ghostty_config_file,
                    cx,
                );
            }
            "resetGhosttySettingsToDefault" => {
                self.update_gpui_ghostty_visible_settings(
                    shared_settings::reset_ghostty_visible_settings_to_defaults,
                    shared_settings::reset_ghostty_config_file_to_defaults,
                    cx,
                );
            }
            "openGhosttyConfigFile" => {
                self.open_gpui_ghostty_config_file(cx);
            }
            "installGte" => {
                self.install_gpui_gte_from_homebrew(cx);
            }
            "runPortlessSettingsAdminAction" | "runPortlessSetupPromptAdminAction" => {
                self.handle_gpui_portless_admin_action_message(command, cx);
            }
            "setPortlessEnabled" => {
                self.handle_gpui_set_portless_enabled_message(command, cx);
            }
            "saveSidebarAgent" | "deleteSidebarAgent" | "syncSidebarAgentOrder" => {
                self.handle_gpui_sidebar_agent_metadata_command(command, cx);
            }
            "saveSidebarCommand"
            | "deleteSidebarCommand"
            | "syncSidebarCommandOrder"
            | "saveGlobalSidebarCommand"
            | "deleteGlobalSidebarCommand"
            | "syncGlobalSidebarCommandOrder" => {
                self.handle_gpui_sidebar_command_metadata_command(command, cx);
            }
            "setProjectWorktreeCommand" => {
                let Some(project_id) = command
                    .get("projectId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                else {
                    return;
                };
                let Some(command_text) = command
                    .get("command")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                else {
                    return;
                };
                self.update_gpui_project_settings_metadata_in_background(
                    GpuiProjectSettingsMetadataUpdate::WorktreeCommand {
                        project_id,
                        command: command_text,
                    },
                    cx,
                );
            }
            "setProjectBeadsDisplayKey" => {
                let Some(project_id) = command
                    .get("projectId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                else {
                    return;
                };
                let Some(display_key) = command
                    .get("displayKey")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                else {
                    return;
                };
                self.update_gpui_project_settings_metadata_in_background(
                    GpuiProjectSettingsMetadataUpdate::BeadsDisplayKey {
                        project_id,
                        display_key,
                    },
                    cx,
                );
            }
            "setProjectBeadsDirectory" => {
                let Some(project_id) = command
                    .get("projectId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                else {
                    return;
                };
                let Some(directory) = command
                    .get("directory")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                else {
                    return;
                };
                self.update_gpui_project_settings_metadata_in_background(
                    GpuiProjectSettingsMetadataUpdate::BeadsDirectory {
                        project_id,
                        directory,
                    },
                    cx,
                );
            }
            "setProjectDocsDirectory" => {
                let Some(project_id) = command
                    .get("projectId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                else {
                    return;
                };
                let Some(directory) = command
                    .get("directory")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                else {
                    return;
                };
                self.update_gpui_project_settings_metadata_in_background(
                    GpuiProjectSettingsMetadataUpdate::DocsDirectory {
                        project_id,
                        directory,
                    },
                    cx,
                );
            }
            "requestPreviousSessions" => {
                let request = gpui_previous_sessions_request_from_command(command);
                let remote_sources = self.connected_gpui_remote_previous_session_sources();
                self.run_gpui_app_modal_sidebar_status_task(
                    move || gpui_previous_sessions_result_message(request, remote_sources),
                    cx,
                );
            }
            "requestSessionTranscriptSizes" => {
                let request = gpui_session_transcript_sizes_request_from_command(command);
                let remote_sources = self.connected_gpui_remote_previous_session_sources();
                self.run_gpui_app_modal_sidebar_status_task(
                    move || gpui_session_transcript_sizes_result_message(request, remote_sources),
                    cx,
                );
            }
            "requestStashedPrompts" => {
                /*
                CDXC:SavedPrompts 2026-07-29:
                The Prompts modal loads stashed prompt-editor saves on demand
                through the local gxserver daemon. The answer is a transient
                `stashedPromptsResult` sidebarState payload the modal host
                forwards as a window message; prompt bodies stay inside that
                round trip and are never logged or stored by Rust.
                */
                let request_id = command
                    .get("requestId")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let project_id = command
                    .get("projectId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                self.run_gpui_app_modal_sidebar_status_task(
                    move || gpui_stashed_prompts_result_message(&request_id, project_id.as_deref()),
                    cx,
                );
            }
            "saveStashedPrompt" => {
                let request_id = command
                    .get("requestId")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let Some(content) = command
                    .get("content")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                else {
                    return;
                };
                let project_id = command
                    .get("projectId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let prompt_id = command
                    .get("promptId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let session_id = command
                    .get("sessionId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let tag_ids = command
                    .get("tagIds")
                    .and_then(serde_json::Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(serde_json::Value::as_str)
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    });
                self.run_gpui_app_modal_sidebar_status_task(
                    move || {
                        gpui_save_stashed_prompt_result_message(
                            &request_id,
                            &content,
                            prompt_id.as_deref(),
                            project_id.as_deref(),
                            session_id.as_deref(),
                            tag_ids.as_deref(),
                        )
                    },
                    cx,
                );
            }
            "saveStashedPromptTag" => {
                /*
                CDXC:SavedPrompts 2026-08-23:
                Tag create/rename runs through the same local gxserver daemon as
                the prompts, and answers with the whole refreshed catalogue so
                the modal's rail cannot drift from what is stored.
                */
                let request_id = command
                    .get("requestId")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let Some(name) = command
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                else {
                    return;
                };
                let color = command
                    .get("color")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let tag_id = command
                    .get("tagId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                self.run_gpui_app_modal_sidebar_status_task(
                    move || {
                        gpui_save_stashed_prompt_tag_result_message(
                            &request_id,
                            &name,
                            color.as_deref(),
                            tag_id.as_deref(),
                        )
                    },
                    cx,
                );
            }
            "deleteStashedPromptTag" => {
                let request_id = command
                    .get("requestId")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let Some(tag_id) = command
                    .get("tagId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                else {
                    return;
                };
                self.run_gpui_app_modal_sidebar_status_task(
                    move || gpui_delete_stashed_prompt_tag_result_message(&request_id, &tag_id),
                    cx,
                );
            }
            "setStashedPromptTags" => {
                let request_id = command
                    .get("requestId")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let Some(prompt_id) = command
                    .get("promptId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                else {
                    return;
                };
                let tag_ids = command
                    .get("tagIds")
                    .and_then(serde_json::Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(serde_json::Value::as_str)
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                self.run_gpui_app_modal_sidebar_status_task(
                    move || {
                        gpui_set_stashed_prompt_tags_result_message(
                            &request_id,
                            &prompt_id,
                            &tag_ids,
                        )
                    },
                    cx,
                );
            }
            "deleteStashedPrompt" => {
                if let Some(prompt_id) = command
                    .get("promptId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                {
                    cx.background_executor()
                        .spawn(async move {
                            let _ = gpui_gxserver_rpc_result(
                                "/api/deleteStashedPrompt",
                                &serde_json::json!({ "promptId": prompt_id }),
                                Duration::from_secs(5),
                            );
                        })
                        .detach();
                }
            }
            "insertStashedPrompt" => {
                let Some(content) = command
                    .get("content")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                else {
                    return;
                };
                let session_id = command
                    .get("sessionId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let inserted = session_id.as_deref().is_some_and(|session_id| {
                    self.insert_stashed_prompt_into_agents_session(session_id, &content, cx)
                });
                if !inserted {
                    /*
                    CDXC:SavedPrompts 2026-07-29:
                    When the originating terminal is gone (closed tab, sleeping
                    session, all-projects row from another project), fall back
                    to the clipboard and say so instead of silently dropping
                    the selected prompt.
                    */
                    cx.write_to_clipboard(ClipboardItem::new_string(content));
                    self.dispatch_gpui_app_modal_toast(
                        "info",
                        "Prompt copied to clipboard",
                        "The session's terminal is not available for direct insert.",
                        cx,
                    );
                }
            }
            "requestRecentProjects" => {
                let machine_id = command
                    .get("machineId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let normalized_machine_id = machine_id
                    .as_deref()
                    .and_then(gpui_normalize_remote_machine_id);
                let remote_target = normalized_machine_id
                    .as_deref()
                    .and_then(|machine_id| self.gpui_remote_gxserver_request_target(machine_id));
                let machine_name = normalized_machine_id
                    .as_deref()
                    .and_then(gpui_remote_machine_name_from_settings);
                let request = GpuiRecentProjectsRequest {
                    machine_id,
                    machine_name,
                    remote_target,
                };
                self.run_gpui_app_modal_sidebar_status_task(
                    move || gpui_recent_projects_result_message(&request),
                    cx,
                );
            }
            "restoreRecentProject" => {
                self.handle_gpui_app_modal_recent_project_mutation(
                    GpuiRecentProjectMutation::Restore,
                    command,
                    cx,
                );
            }
            "closeProjectFromProjects" => {
                self.handle_gpui_app_modal_recent_project_mutation(
                    GpuiRecentProjectMutation::Close,
                    command,
                    cx,
                );
            }
            "focusRecentProject" => {
                if let Some(project_id) =
                    command.get("projectId").and_then(serde_json::Value::as_str)
                {
                    let _ = self.dispatch_gpui_menu_bar_project_activation(project_id, cx);
                }
            }
            "removeRecentProject" => {
                self.handle_gpui_app_modal_recent_project_mutation(
                    GpuiRecentProjectMutation::Remove,
                    command,
                    cx,
                );
            }
            "copyRecentProjectPath" | "openRecentProjectInFinder" | "openRecentProjectTerminal" => {
                self.handle_gpui_app_modal_recent_project_path_action(command_type, command, cx);
            }
            "focusSession" => {
                if let Some(session_id) = command
                    .get("sessionId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                {
                    let _ = self.dispatch_gpui_command_palette_session_focus(&session_id, cx);
                }
            }
            /*
            CDXC:SavedPrompts 2026-08-24:
            Saved Prompts rows carry the raw gxserver ids of the session they
            were stashed from plus that session's provider conversation id. The
            modal closes itself (like the Quick Access rows above), so this arm
            only forwards the bounded selector into the sidebar runtime, which
            owns the present → restore → resume routing.
            */
            "jumpToStashedPromptSession" => {
                let project_id = command
                    .get("projectId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let session_id = command
                    .get("sessionId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let agent_session_id = command
                    .get("agentSessionId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let _ = self.dispatch_gpui_stashed_prompt_session_jump(
                    project_id.as_deref(),
                    session_id.as_deref(),
                    agent_session_id.as_deref(),
                    cx,
                );
            }
            "runSidebarCommand" => {
                if let Some(command_id) = command
                    .get("commandId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                {
                    let run_mode = command
                        .get("runMode")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string);
                    let _ = self.dispatch_gpui_command_palette_run_sidebar_command(
                        &command_id,
                        run_mode.as_deref(),
                        cx,
                    );
                }
            }
            "restorePreviousSession" => {
                if let Some(history_id) = command
                    .get("historyId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                {
                    let remote_sources = self.connected_gpui_remote_previous_session_sources();
                    let background = cx.background_executor().clone();
                    cx.spawn(async move |this, cx| {
                        let restored = background
                            .spawn(async move {
                                gpui_restore_previous_session_from_history_id(
                                    &history_id,
                                    &remote_sources,
                                )
                            })
                            .await;
                        let Some(restored) = restored else {
                            return;
                        };
                        let _ = this.update(cx, |this, cx| {
                            match restored {
                                GpuiPreviousSessionRestoreResult::Local {
                                    project_id,
                                    session_id,
                                } => {
                                    /*
                                    CDXC:Sessions 2026-07-11:
                                    macOS restores a previous terminal by creating its
                                    replacement row and then running the normal attach
                                    sequence. A focus-only dispatch lets the presentation
                                    reconciler create a placeholder, but does not provide
                                    that placeholder with gxserver's resume/attach payload,
                                    leaving an empty shell. Start the same local attach path
                                    directly here, using the currently focused Agents pane
                                    as the restore placement target.
                                    */
                                    if let Some(focus_id) =
                                        gpui_combined_presentation_session_focus_id(
                                            &project_id,
                                            &session_id,
                                        )
                                    {
                                        let _ = this
                                            .dispatch_gpui_command_palette_session_focus(
                                                &focus_id,
                                                cx,
                                            );
                                    }
                                    let key = GpuiLocalWorkspaceSessionKey {
                                        project_id,
                                        session_id,
                                    };
                                    this.local_workspace_latest_focus_key = Some(key.clone());
                                    this.refresh_sidebar_gxserver_bootstrap_if_changed(cx);
                                    let requested_pane_id = this.agents_workspace.focused_pane;
                                    if this.focus_existing_gpui_local_workspace_terminal(&key, cx) {
                                        return;
                                    }
                                    let attach_intent = this.local_workspace_attach_intent_for_key(&key);
                                    if !this.local_workspace_attach_pending.insert(key.clone()) {
                                        return;
                                    }
                                    let background = cx.background_executor().clone();
                                    cx.spawn(async move |this, cx| {
                                        let prepare_key = key.clone();
                                        let result = background
                                            .spawn(async move {
                                                gpui_prepare_local_workspace_attach_terminal_plan(
                                                    &prepare_key,
                                                    attach_intent,
                                                )
                                            })
                                            .await;
                                        let _ = this.update(cx, |this, cx| {
                                            this.local_workspace_attach_pending.remove(&key);
                                            if this.local_workspace_latest_focus_key.as_ref()
                                                != Some(&key)
                                            {
                                                return;
                                            }
                                            match result {
                                                Ok(plan) => {
                                                    let _ = this
                                                        .open_gpui_local_workspace_terminal(
                                                            key,
                                                            plan,
                                                            requested_pane_id,
                                                            false,
                                                            cx,
                                                        );
                                                }
                                                Err(message) => this.dispatch_gpui_app_modal_toast(
                                                    "warning",
                                                    "Session restore unavailable",
                                                    message.as_str(),
                                                    cx,
                                                ),
                                            }
                                        });
                                    })
                                    .detach();
                                }
                                GpuiPreviousSessionRestoreResult::Remote {
                                    remote_machine_id,
                                    project_id,
                                    session_id,
                                } => {
                                    this.refresh_gpui_remote_gxserver_presentation_in_background(
                                        remote_machine_id.clone(),
                                        false,
                                        cx,
                                    );
                                    this.handle_gpui_remote_session_native_action(
                                        GpuiSidebarNativeProjectPathActionMessage {
                                            action:
                                                GpuiSidebarNativeProjectPathAction::OpenRemoteSessionTerminal,
                                            file_path: None,
                                            placement:
                                                GpuiWorkspaceTerminalFocusPlacement::Tab,
                                            preferred_interface:
                                                GpuiPreferredAgentInterface::Terminal,
                                            project_id: gpui_remote_scoped_session_id(
                                                remote_machine_id.as_str(),
                                                project_id.as_str(),
                                                session_id.as_str(),
                                            ),
                                        },
                                        cx,
                                    );
                                }
                            }
                        });
                    })
                    .detach();
                }
            }
            "deletePreviousSession" => {
                if let Some(history_id) = command
                    .get("historyId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                {
                    let remote_sources = self.connected_gpui_remote_previous_session_sources();
                    let background = cx.background_executor().clone();
                    cx.spawn(async move |_this, _cx| {
                        background
                            .spawn(async move {
                                gpui_delete_previous_session_from_history_id(
                                    &history_id,
                                    &remote_sources,
                                );
                            })
                            .await;
                    })
                    .detach();
                }
            }
            "searchPreviousSessionsByText" => {
                /*
                CDXC:Sessions 2026-06-24-11:53:
                The shared Previous Sessions modal no longer renders Search by Text launch buttons, and GPUI does not yet have enough current-project launch authority here to recreate macOS's direct text-search terminal honestly. Keep the legacy command harmless and response-free instead of faking a terminal launch or claiming success.
                */
            }
            "postponePortlessSetupPrompt" | "cancelPortlessSetupPrompt" => {
                self.suppress_gpui_portless_setup_prompt_for_this_run();
                self.refresh_open_gpui_app_modal_sidebar_state_in_background(cx);
            }
            command_type if gpui_app_modal_unsupported_settings_command_noop(command_type) => {}
            _ => {}
        }
    }
}
