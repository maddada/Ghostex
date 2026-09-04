// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: sidebar project-context reconciliation, project switching, browser tabs/panes

use std::collections::HashMap;
use std::collections::HashSet;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use crate::terminal_surface_host::NativeTerminalSurfaceHost;
use crate::terminal_surface_lifecycle::NativeTerminalSurfaceLifecycleState;
use gpui::ClipboardItem;
use gpui::Entity;
use gpui::Pixels;
use gpui::Window;
use gpui_component::WindowExt;
use gpui_component::native_menu::NativeMenu;
use gpui_component::notification::Notification;

use crate::app::actions::*;
use crate::app::consts::*;
use crate::app::element::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::window::*;
use crate::*;
impl GhostexGpuiApp {
    pub(crate) fn receive_sidebar_global_actions_payload(
        &mut self,
        payload: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:AgentLauncher 2026-08-01-16:00:
        GPUI accepts the Global Actions list only through the fixed sidebar
        bridge and keeps the parsed bounded rows in runtime memory for tab strip
        rendering. Malformed payloads are ignored without logging raw JSON,
        command text, URLs, or ids, and without clearing the last good list — a
        rejected payload must not blank the strip.
        */
        let Ok(next_actions) = gpui_sidebar_global_actions_from_json(payload) else {
            return;
        };
        if self.sidebar_global_actions == next_actions {
            return;
        }
        self.sidebar_global_actions = next_actions;
        cx.notify();
    }

    pub(crate) fn receive_sidebar_session_status_indicators_payload(
        &mut self,
        payload: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:StatusPet 2026-06-26-04:38:
        GPUI accepts status-indicator state only through the fixed sidebar bridge and stores the parsed bounded presentation model in runtime memory. Malformed payloads are ignored without logging raw JSON, paths, titles beyond the bounded UI strings, command text, terminal output, URLs, tokens, or fallback status data.
        */
        let Ok(next_state) = gpui_sidebar_session_status_indicators_from_json(payload) else {
            return;
        };
        if self.sidebar_session_status_indicators == next_state {
            self.sidebar_session_status_indicators_snapshot_seen = true;
            return;
        }
        let attention_notifications = if self.sidebar_session_status_indicators_snapshot_seen {
            gpui_session_attention_notification_candidates(
                &self.sidebar_session_status_indicators,
                &next_state,
            )
        } else {
            Vec::new()
        };
        self.sidebar_session_status_indicators_snapshot_seen = true;
        self.sidebar_session_status_indicators = next_state;
        self.apply_gpui_menu_bar_status_item_state();
        self.deliver_gpui_session_attention_notifications(attention_notifications, cx);
        cx.notify();
    }

    pub(crate) fn deliver_gpui_session_attention_notifications(
        &mut self,
        candidates: Vec<GpuiSessionAttentionNotificationCandidate>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Notifications 2026-06-26-06:56:
        GPUI session attention banners fire only on sanitized status rows newly entering `attention` after the first status snapshot. Saved `showMacOSAttentionNotifications` defaults to true, notification sound stays nil, and runtime rate limiting mirrors native without persisting or logging titles, ids, raw payloads, paths, URLs, command text, stdout/stderr, settings JSON, tokens, or terminal content.
        */
        if candidates.is_empty() || !gpui_macos_attention_notifications_enabled() {
            return;
        }
        let now = Instant::now();
        for candidate in candidates {
            if self
                .session_attention_notification_rate_limiter
                .consume(candidate.session_id.as_str(), now)
            {
                self.deliver_gpui_macos_session_attention_notification(candidate, cx);
            }
        }
    }

    pub(crate) fn deliver_gpui_macos_session_attention_notification(
        &mut self,
        candidate: GpuiSessionAttentionNotificationCandidate,
        cx: &mut gpui::Context<Self>,
    ) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |_this, _cx| {
            let _ = background
                .spawn(async move { gpui_deliver_macos_session_attention_notification(candidate) })
                .await;
        })
        .detach();
    }

    pub(crate) fn apply_gpui_menu_bar_status_item_state(&self) {
        /*
        CDXC:StatusPet 2026-06-26-05:42:
        GPUI applies the macOS menu-bar badge directly from sanitized Rust-owned counts and the saved hideMenuBarSessionStatusIndicators setting.

        CDXC:StatusPet 2026-06-26-06:05:
        The primary-click dropdown shares this Rust-owned status snapshot for bounded project/session rows. AppKit receives explicit copied FFI fields for ids, titles, status, order, and timestamps only; it never receives renderer JSON, paths, URLs, command text, tokens, logs, terminal output, hidden hit regions, or overlay instructions.
        */
        apply_gpui_menu_bar_status_item(&self.sidebar_session_status_indicators);
    }

    pub(crate) fn receive_sidebar_pet_overlay_state_payload(
        &mut self,
        payload: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:StatusPet 2026-06-26-04:38:
        The pet overlay settings fan-out shares the status bridge privacy boundary: Rust stores only bounded enabled/pet-id/status/activity ids and titles for future GPUI-owned presentation, with no generic IPC, renderer paths, URLs, commands, terminal content, or menu-bar emulation.
        */
        let Ok(next_state) = gpui_sidebar_pet_overlay_state_from_json(payload) else {
            return;
        };
        if self.sidebar_pet_overlay == next_state {
            return;
        }
        self.sidebar_pet_overlay = next_state;
        if !self.sidebar_pet_overlay.enabled {
            self.gpui_pet_overlay_avatar_hovered = false;
        }
        self.refresh_gpui_pet_overlay_animation_state(cx);
        cx.notify();
    }

    pub(crate) fn reconcile_local_app_shot_session_mappings(
        &mut self,
        focus_state: &GpuiGxserverPresentationFocusState,
    ) {
        self.local_app_shot_session_mappings
            .retain(|_, shell_session_id| {
                self.agents_workspace.session(*shell_session_id).is_some()
            });

        let local_visible_session_ids = focus_state
            .visible_session_ids
            .iter()
            .filter(|session_id| {
                gpui_sidebar_local_gxserver_session_id_allowed(session_id.as_str())
            })
            .collect::<Vec<_>>();
        if local_visible_session_ids.len() == 1 {
            if let Some(shell_session_id) = self.single_live_agents_shell_session_for_app_shot() {
                self.local_app_shot_session_mappings
                    .insert(local_visible_session_ids[0].to_string(), shell_session_id);
            }
        }

        let focused_session_id = focus_state
            .focused_session_id
            .as_deref()
            .filter(|session_id| gpui_sidebar_local_gxserver_session_id_allowed(*session_id));
        if let Some(focused_session_id) = focused_session_id {
            if let Some(shell_session_id) = self.focused_live_agents_shell_session_for_app_shot() {
                self.local_app_shot_session_mappings
                    .insert(focused_session_id.to_string(), shell_session_id);
            }
        }

        if self.local_app_shot_session_mappings.len() > GPUI_LOCAL_APP_SHOT_SESSION_MAP_MAX {
            let allowed_ids = focus_state
                .visible_session_ids
                .iter()
                .chain(focus_state.focused_session_id.iter())
                .filter(|session_id| {
                    gpui_sidebar_local_gxserver_session_id_allowed(session_id.as_str())
                })
                .cloned()
                .collect::<HashSet<_>>();
            self.local_app_shot_session_mappings
                .retain(|session_id, _| allowed_ids.contains(session_id));
        }
    }

    pub(crate) fn single_live_agents_shell_session_for_app_shot(
        &self,
    ) -> Option<TerminalSessionId> {
        let mut sessions = self.live_agents_shell_sessions_for_app_shot().into_iter();
        let session_id = sessions.next()?;
        sessions.next().is_none().then_some(session_id)
    }

    pub(crate) fn live_agents_shell_sessions_for_app_shot(&self) -> Vec<TerminalSessionId> {
        #[cfg(target_os = "macos")]
        {
            self.agents_workspace
                .rendered_terminal_body_mount_slots()
                .into_iter()
                .filter(|slot_id| self.agents_terminal_ghostty_surface_matches(*slot_id))
                .map(|slot_id| slot_id.session_id)
                .collect()
        }

        #[cfg(not(target_os = "macos"))]
        {
            Vec::new()
        }
    }

    pub(crate) fn focused_live_agents_shell_session_for_app_shot(
        &self,
    ) -> Option<TerminalSessionId> {
        #[cfg(target_os = "macos")]
        {
            let slot_id = focused_agents_terminal_surface_mount_slot(
                self.active_mode,
                self.shell_focus,
                &self.agents_workspace,
            )?;
            self.agents_terminal_ghostty_surface_matches(slot_id)
                .then_some(slot_id.session_id)
        }

        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    }

    pub(crate) fn receive_sidebar_native_app_shot_prompt_payload(
        &mut self,
        payload: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:AppShots 2026-06-25-23:28:
        Existing-session App Shot insertion accepts only the fixed sidebar App Shot prompt payload: a gxserver presentation session id plus the already formatted prompt string. Rust maps that id to a live Agents shell tab, selects it through normal workspace state if needed, verifies the exact mounted Ghostty owner/runtime id, and returns only a transient boolean result to the sidebar.

        CDXC:AppShots 2026-06-26-04:27:
        Remote App Shot insertion uses the same fixed prompt bridge with a machine-scoped remote presentation session id. It may write only to an already-mounted remote attach Agents terminal in `remote_attach_sessions`; it must not wake, create, or materialize remote tabs, and it stores no prompt, path, SSH, title, URL, or terminal content.
        */
        let Ok(message) = gpui_sidebar_native_app_shot_prompt_from_json(payload) else {
            return;
        };
        let ok = if let Some(reference) =
            gpui_remote_attach_session_reference_from_project_id(message.session_id.as_str())
        {
            self.insert_native_app_shot_prompt_into_remote_agents_session(
                &reference,
                message.prompt.as_str(),
                cx,
            )
        } else {
            self.insert_native_app_shot_prompt_into_local_agents_session(
                message.session_id.as_str(),
                message.prompt.as_str(),
                cx,
            )
        };
        self.dispatch_gpui_native_app_shot_prompt_result(message.session_id.as_str(), ok, cx);
    }

    pub(crate) fn insert_native_app_shot_prompt_into_local_agents_session(
        &mut self,
        session_id: &str,
        prompt: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        #[cfg(target_os = "macos")]
        {
            if !gpui_sidebar_local_gxserver_session_id_allowed(session_id) || prompt.is_empty() {
                return false;
            }
            let Some(shell_session_id) = self
                .local_app_shot_session_mappings
                .get(session_id)
                .copied()
            else {
                return false;
            };
            if !self
                .agents_workspace
                .session(shell_session_id)
                .is_some_and(|session| {
                    session.presentation_state == TerminalSessionPresentationState::Running
                })
            {
                self.local_app_shot_session_mappings.remove(session_id);
                return false;
            }
            let Some(pane_id) = self.agents_workspace.pane_id_for_session(shell_session_id) else {
                self.local_app_shot_session_mappings.remove(session_id);
                return false;
            };

            self.active_mode = TitlebarMode::Agents;
            self.agents_workspace.select_tab(pane_id, shell_session_id);
            self.set_shell_focus_with_terminal_handoff(ShellFocusTarget::AgentsPane(pane_id), true);
            self.scroll_workspace_pane_active_tab(pane_id);

            let slot_id = AgentsTerminalBodyMountSlotId {
                pane_id,
                session_id: shell_session_id,
            };
            if !self.agents_terminal_ghostty_surface_matches(slot_id) {
                return false;
            }
            if !self.send_text_bytes_to_focused_agents_terminal_surface(prompt.as_bytes()) {
                return false;
            }
            self.persist_shell_layout_state();
            cx.notify();
            true
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (session_id, prompt, cx);
            false
        }
    }

    pub(crate) fn insert_native_app_shot_prompt_into_remote_agents_session(
        &mut self,
        reference: &GpuiRemoteAttachSessionReference,
        prompt: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        #[cfg(target_os = "macos")]
        {
            if prompt.is_empty() {
                return false;
            }
            let key = GpuiRemoteAttachSessionKey::from(reference);
            let Some(shell_session_id) = self.remote_attach_sessions.get(&key).copied() else {
                return false;
            };
            let Some(session) = self.agents_workspace.session(shell_session_id) else {
                self.remote_attach_sessions.remove(&key);
                return false;
            };
            if session.presentation_state != TerminalSessionPresentationState::Running {
                return false;
            }
            let Some(pane_id) = self.agents_workspace.pane_id_for_session(shell_session_id) else {
                self.remote_attach_sessions.remove(&key);
                return false;
            };

            self.active_mode = TitlebarMode::Agents;
            self.agents_workspace.select_tab(pane_id, shell_session_id);
            self.set_shell_focus_with_terminal_handoff(ShellFocusTarget::AgentsPane(pane_id), true);
            self.scroll_workspace_pane_active_tab(pane_id);

            let slot_id = AgentsTerminalBodyMountSlotId {
                pane_id,
                session_id: shell_session_id,
            };
            if !self.agents_terminal_ghostty_surface_matches(slot_id) {
                return false;
            }
            if !self.send_text_bytes_to_focused_agents_terminal_surface(prompt.as_bytes()) {
                return false;
            }
            self.persist_shell_layout_state();
            self.set_sidebar_gxserver_remote_attach_focus_state(&key, cx);
            cx.notify();
            true
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (reference, prompt, cx);
            false
        }
    }

    pub(crate) fn dispatch_gpui_native_app_shot_prompt_result(
        &mut self,
        session_id: &str,
        ok: bool,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let message = serde_json::json!({
            "ok": ok,
            "sessionId": session_id,
            "type": GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_RESULT_MESSAGE_TYPE,
            "version": GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_RESULT_MESSAGE_VERSION,
        });
        let script = gpui_native_app_shot_prompt_result_script(&message);
        sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script))
    }

    pub(crate) fn set_sidebar_gxserver_remote_attach_focus_state(
        &mut self,
        key: &GpuiRemoteAttachSessionKey,
        cx: &mut gpui::Context<Self>,
    ) {
        let scoped_session_id = gpui_remote_scoped_session_id(
            key.remote_machine_id.as_str(),
            key.project_id.as_str(),
            key.session_id.as_str(),
        );
        /*
        CDXC:RemoteMachines 2026-07-30:
        The Agents workspace keys remote projects by the machine-scoped id
        (`remote:<machineId>:project:<projectId>`), matching the sidebar's
        active-project snapshot. A raw remote project id here would swap the
        workspace to a nonexistent project key and blank the pane.
        */
        let scoped_project_id =
            gpui_remote_scoped_project_id(key.remote_machine_id.as_str(), key.project_id.as_str());
        let active_project_tab_sessions = (self
            .sidebar_gxserver_presentation_focus_state
            .active_project_id
            .as_deref()
            == Some(scoped_project_id.as_str()))
        .then(|| {
            self.sidebar_gxserver_presentation_focus_state
                .active_project_tab_sessions
                .clone()
        })
        .flatten();
        self.set_sidebar_gxserver_presentation_focus_state(
            GpuiGxserverPresentationFocusState {
                active_project_id: Some(scoped_project_id.clone()),
                active_project_tab_sessions,
                focused_session_id: Some(scoped_session_id.clone()),
                visible_session_ids: vec![scoped_session_id.clone()],
            },
            cx,
        );
        /*
        Remote tab selection must update the live SidebarApp projection as
        well as Rust's persisted focus snapshot. Same-transport bootstrap
        refreshes deliberately do not overwrite live React focus, so send the
        same dedicated one-way tab-selection callback used by local tabs with
        canonical machine-scoped ids.
        */
        self.dispatch_gpui_workspace_tab_session_selected(
            scoped_project_id.as_str(),
            scoped_session_id.as_str(),
            false,
            false,
            cx,
        );
    }

    pub(crate) fn receive_sidebar_native_project_path_action_payload(
        &mut self,
        payload: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Projects 2026-06-24-14:18:
        Sidebar copy/open project path actions in GPUI are native side effects authorized by gxserver project ids, not renderer paths. Parse only the small fixed JSON action contract from the bundled sidebar bridge, resolve the path through authenticated localhost gxserver reads, then perform clipboard/Finder actions without logging paths, daemon response bodies, tokens, project names, or renderer text.

        CDXC:Projects 2026-06-24-13:49:
        Sidebar IDE-open actions reuse this pathless native bridge instead of accepting targetApp, app-name, or editor command data from React. Resolve the gxserver project path in Rust; group IDE opens read the shared Settings default editor while active-workspace VS Code/Zed opens use fixed native action names. Fail with a generic warning when the configured editor is unsupported or unavailable rather than claiming a launch.

        CDXC:Projects 2026-06-24-13:57:
        Settings custom default editor commands for group project IDE opens must stay native-owned: React sends only a fixed action plus gxserver project id, Rust reads shared Settings, accepts only a bounded argv-style command string, appends the resolved project path as a separate argv item, and never logs command text, paths, renderer payloads, stdout, or stderr.

        CDXC:Git 2026-06-24-15:43:
        Git browser/file side effects on this bridge must re-query gxserver in background Rust before launch. Existing PR opens accept no renderer URL, and changed-file IDE opens accept only a relative candidate that must still be present in the current gxserver Git state.

        CDXC:RemoteMachines 2026-06-24-19:06:
        Remote attach/resume side effects share this fixed sidebar-native bridge, but Rust must parse the machine-scoped remote session id and own gxserver metadata reads, SSH command construction, terminal launch payloads, and clipboard writes. CEF cannot pass hosts, users, paths, tokens, daemon bodies, stdout/stderr, or command text.

        CDXC:RemoteMachines 2026-08-14:
        Remote project copy-path, PR browser, IDE, Recent Projects terminal creation, and changed-file open requests reuse the same fixed native bridge, but Rust must parse a machine-scoped project id and revalidate through the live saved-machine gxserver tunnel before any side effect. Clipboard, browser, terminal, and fixed remote-editor actions may proceed only from remote daemon state; local Finder and unsupported/custom editor opens for remote paths fail honestly.
        */
        let Ok(message) = gpui_sidebar_native_project_path_action_from_json(payload) else {
            return;
        };
        if message.action.is_remote_session_action() {
            self.handle_gpui_remote_session_native_action(message, cx);
            return;
        }
        if message.action.is_remote_project_action() {
            self.handle_gpui_remote_project_native_action(message, cx);
            return;
        }
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { execute_gpui_sidebar_native_project_path_action(message) })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(GpuiSidebarNativeProjectPathActionResult::Copied(path)) => {
                    cx.write_to_clipboard(ClipboardItem::new_string(path));
                }
                Ok(GpuiSidebarNativeProjectPathActionResult::Opened) => {}
                Err(message) => {
                    this.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Native action unavailable",
                        &message,
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    pub(crate) fn receive_sidebar_command_action_payload(
        &mut self,
        payload: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:CommandPane 2026-06-24-23:17:
        Shared SidebarApp project Actions must run in GPUI through the same Browser or command-pane path as titlebar Actions. Parse only the fixed sidebar command-action JSON emitted from the gxserver HUD projection, then reuse the window-aware action runner so command text enters only the command-terminal launch payload boundary and never logs, shell-state JSON, paths, fallback project detection, or renderer execution.
        */
        let Ok(action) = gpui_sidebar_command_action_from_json(payload) else {
            return;
        };
        self.run_gpui_titlebar_action(action, window, cx);
    }

    pub(crate) fn receive_sidebar_command_run_end_payload(
        &mut self,
        payload: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:CommandPane 2026-06-25-10:34:
        Shared SidebarApp `endSidebarCommandRun` must close the GPUI command-pane Action tab mapped to the command id and clear sidebar button feedback, matching macOS. Accept only the fixed command-run-end payload; do not accept command text, URLs, cwd/env, run ids, status-file paths, terminal output, project paths, or generic IPC fields.
        */
        let Ok(command_id) = gpui_sidebar_command_run_end_from_json(payload) else {
            return;
        };
        self.close_gpui_sidebar_command_run(&command_id, cx);
    }

    pub(crate) fn receive_sidebar_ghostex_hotkey_action_payload(
        &mut self,
        payload: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:CommandPalette 2026-06-27-08:17:
        Shared SidebarApp and command-palette hotkey rows reach GPUI through the CEF sidebar runtime, not the native WKScriptMessage path. Parse only the fixed action-id selector and feed the existing Rust `runGhostexHotkeyAction` dispatcher so Open Commands Panel uses the shared open/focus/minimize route and focused-pane, Settings, and modal routes do not accept renderer-owned sessions, paths, commands, URLs, or launch metadata.
        */
        let Ok(action_id) = gpui_sidebar_ghostex_hotkey_action_from_json(payload) else {
            return;
        };
        self.handle_gpui_app_modal_sidebar_command(
            serde_json::json!({
                "message": {
                    "actionId": action_id,
                    "type": "runGhostexHotkeyAction",
                },
            }),
            window,
            cx,
        );
    }

    pub(crate) fn land_quick_automations_active_project_on_automate_mode(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:Automations 2026-07-08:
        Mirror macOS `focusQuickAutomationsProject` in `native/sidebar/native-sidebar.tsx`: when the sidebar focuses the quick-automations registry project, select Automate via `set_active_mode` so availability, wake, focus, CEF visibility, and shell-state persistence stay on the reviewed titlebar path.
        */
        if self.active_mode == TitlebarMode::Automate
            || !gpui_project_snapshot_is_quick_automations_overview(
                self.latest_sidebar_project_snapshot.as_ref(),
            )
        {
            return false;
        }
        self.set_active_mode(TitlebarMode::Automate, window, cx)
    }

    pub(crate) fn land_pending_source_file_open_on_source_mode(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let pending_project_matches = self
            .pending_source_file_open
            .as_ref()
            .zip(self.latest_sidebar_project_snapshot.as_ref())
            .is_some_and(|(pending, snapshot)| {
                snapshot.in_memory_project_path.as_ref() == Some(&pending.project_path)
            });
        if !pending_project_matches {
            return false;
        }
        let changed = self.set_active_mode(TitlebarMode::Source, window, cx);
        self.focus_project_editor_surface(TitlebarMode::Source, window, cx);
        changed
    }

    pub(crate) fn receive_sidebar_project_context_payload(
        &mut self,
        payload: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:CefRuntime 2026-06-22-19:32:
        Live sidebar project context is accepted only through the slice-91 active-project contract and stored as runtime memory. Slice 92 intentionally does not replace titlebar availability, remove the strict env bridge, persist project facts, log raw JSON, or infer projects from .git, names, paths, fixtures, workspace labels, URLs, or filesystem markers.

        CDXC:CefRuntime 2026-06-22-19:44:
        Slice 93 makes valid stored sidebar snapshots the runtime availability source for App titlebar/workarea decisions while retaining the strict env bridge for startup/tests before any sidebar message arrives. Malformed payloads must not replace the previous snapshot or coerce the active mode.

        CDXC:Titlebar 2026-06-22-19:57:
        Slice 95 makes the visible titlebar project label runtime-only sidebar snapshot state. A valid active-project payload updates the label from the stored snapshot display name in memory; malformed payloads leave the existing label unchanged and must not persist, log, or derive display labels from env vars, repo folders, .git, workspace names, fixture names, paths, URLs, or sidebar titles.

        CDXC:CefRuntime 2026-06-23-06:53:
        Duplicate valid active-project payloads are accepted as a bridge heartbeat but are not a project change. Only a changed stored snapshot may refresh the titlebar label, coerce project-scoped mode availability, or notify GPUI, and malformed payloads still preserve the prior snapshot.

        CDXC:Workarea 2026-06-29-00:02:
        Active-project changes no longer reconcile Source/Browser/Kanban/Automate/Manage readiness stores. The stored snapshot immediately feeds titlebar availability and the direct runtime URL/CEF gates, so stale proof state cannot keep or block a workarea surface.
        */
        match store_latest_gpui_project_snapshot_from_sidebar_contract_json(
            &mut self.latest_sidebar_project_snapshot,
            payload,
        ) {
            Ok(GpuiProjectSnapshotStoreResult::Changed) => {
                /*
                CDXC:Navigation 2026-07-29:
                The parsed snapshot is stored before the coalescing gate, so a
                collapsed request needs no payload: replaying it applies
                whatever the newest stored snapshot is, and duplicate
                heartbeats that store as `Unchanged` cannot strand the queue.
                */
                let target_project_id = gpui_active_project_id_from_snapshot(
                    self.latest_sidebar_project_snapshot.as_ref(),
                )
                .map(str::to_string);
                if self.project_switch_request_is_coalesced(
                    target_project_id.as_deref(),
                    GpuiProjectSwitchRequestKind::ActiveProjectContext,
                ) {
                    self.enqueue_coalesced_project_switch_request(
                        target_project_id,
                        GpuiPendingProjectSwitchPayload::ActiveProjectContext,
                        cx,
                    );
                    return;
                }
                self.apply_changed_active_project_snapshot(window, cx);
            }
            Ok(GpuiProjectSnapshotStoreResult::Unchanged) | Err(_) => {}
        }
    }

    pub(crate) fn apply_changed_active_project_snapshot(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.project_name = titlebar_project_label_from_latest_sidebar_snapshot(
            self.latest_sidebar_project_snapshot.as_ref(),
        );
        self.restore_gpui_titlebar_project_selections();
        self.refresh_titlebar_actions_in_background(cx);
        self.swap_agents_workspace_for_active_project(cx);
        self.swap_command_pane_for_active_project(window, cx);
        self.swap_browser_tabs_for_active_project(cx);
        self.refresh_project_workarea_runtime_cef_surfaces_from_runtime_state(cx);
        self.refresh_sidebar_gxserver_bootstrap_if_changed(cx);
        self.coerce_active_mode_to_available_project_context(cx);
        self.land_quick_automations_active_project_on_automate_mode(window, cx);
        self.land_pending_source_file_open_on_source_mode(window, cx);
        self.ensure_project_workarea_runtime_cef_surfaces_for_current_context(cx);
        #[cfg(target_os = "windows")]
        {
            let focus_companion =
                self.shell_focus == ShellFocusTarget::ProjectEditorCompanion(self.active_mode);
            self.sync_windows_project_editor_companion_to_presentation_focus(focus_companion, cx);
        }
        self.broadcast_extension_context_changes(cx);
        cx.notify();
    }

    /*
    CDXC:Navigation 2026-07-29:
    The single decision point every sidebar-driven project switch passes
    through. Returns true when the caller must stop and hand its request to the
    trailing flush instead of executing it now.

    - Same-project requests bypass entirely: a different session inside the
      already-active project is a cheap intra-project focus change and must
      stay instant.
    - The first cross-project request while nothing is settling executes
      immediately (leading edge), so single clicks never get slower, and opens
      the settle window that collapses the clicks behind it.
    - An executing request is newer than everything queued behind it, so it
      drops the whole queue. That is what makes A -> B -> A collapse to zero
      extra work: the queued B request is discarded and its swap never runs.
    */
    pub(crate) fn project_switch_request_is_coalesced(
        &mut self,
        target_project_id: Option<&str>,
        kind: GpuiProjectSwitchRequestKind,
    ) -> bool {
        let same_project = target_project_id == self.agents_workspace_project_id.as_deref();
        if !same_project
            && self
                .project_switch_settling_until
                .is_some_and(|until| Instant::now() < until)
        {
            return true;
        }
        self.project_switch_pending_requests.clear();
        if !same_project && kind.opens_settle_window() {
            self.project_switch_settling_until =
                Some(Instant::now() + GPUI_PROJECT_SWITCH_SETTLE_WINDOW);
        }
        false
    }

    /// Queues the collapsed request. The queue keeps the newest request per
    /// bridge kind in arrival order, and only ever holds requests for one
    /// target project, so a newer target discards the stale ones.
    pub(crate) fn enqueue_coalesced_project_switch_request(
        &mut self,
        target_project_id: Option<String>,
        payload: GpuiPendingProjectSwitchPayload,
        cx: &mut gpui::Context<Self>,
    ) {
        let kind = payload.kind();
        self.project_switch_pending_requests.retain(|pending| {
            pending.target_project_id == target_project_id && pending.payload.kind() != kind
        });
        self.project_switch_pending_requests
            .push(GpuiPendingProjectSwitchRequest {
                target_project_id,
                payload,
            });
        let remaining = self
            .project_switch_settling_until
            .map(|until| until.saturating_duration_since(Instant::now()))
            .unwrap_or_default();
        // macOS TerminalFocusDebugLog parity (scenario native.terminal.focus):
        // bounded gxserver ids, kinds, counts, and durations only.
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.terminalFocus.projectSwitchCoalesced",
            serde_json::json!({
                "activeProjectId": self.agents_workspace_project_id,
                "pendingRequestCount": self.project_switch_pending_requests.len(),
                "requestKind": kind.breadcrumb_id(),
                "settleRemainingMs": remaining.as_millis() as u64,
                "targetProjectId": self
                    .project_switch_pending_requests
                    .last()
                    .and_then(|pending| pending.target_project_id.clone()),
            }),
        );
        self.schedule_coalesced_project_switch_flush(remaining, cx);
    }

    pub(crate) fn schedule_coalesced_project_switch_flush(
        &mut self,
        delay: Duration,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.project_switch_flush_scheduled {
            return;
        }
        self.project_switch_flush_scheduled = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.flush_coalesced_project_switch_requests(window, cx);
            });
        })
        .detach();
    }

    /// Trailing edge: replay the newest collapsed request per bridge kind, in
    /// arrival order, through the ordinary handlers.
    pub(crate) fn flush_coalesced_project_switch_requests(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.project_switch_flush_scheduled = false;
        if self.project_switch_pending_requests.is_empty() {
            // The backstop in the bridge dispatcher can land the queue before
            // this timer fires. A late no-op flush must not close a settle
            // window that a newer switch has since opened.
            return;
        }
        self.project_switch_settling_until = None;
        let pending = std::mem::take(&mut self.project_switch_pending_requests);
        for request in pending {
            let kind = request.payload.kind();
            // The queued target can have caught up with the live project (the
            // A -> B -> A shape), in which case the swap guards inside the
            // swap helpers make the replay a no-op instead of a second
            // teardown. Record which case this was.
            let redundant_swap =
                request.target_project_id.as_deref() == self.agents_workspace_project_id.as_deref();
            support_logs::append(
                support_logs::GpuiSupportLog::TerminalFocus,
                "gpui.terminalFocus.projectSwitchTrailingReplay",
                serde_json::json!({
                    "activeProjectId": self.agents_workspace_project_id,
                    "redundantSwap": redundant_swap,
                    "requestKind": kind.breadcrumb_id(),
                    "targetProjectId": request.target_project_id,
                }),
            );
            match request.payload {
                GpuiPendingProjectSwitchPayload::ActiveProjectContext => {
                    if !redundant_swap {
                        self.project_switch_settling_until =
                            Some(Instant::now() + GPUI_PROJECT_SWITCH_SETTLE_WINDOW);
                    }
                    self.apply_changed_active_project_snapshot(window, cx);
                }
                GpuiPendingProjectSwitchPayload::GxserverPresentationFocusState(state) => {
                    self.set_sidebar_gxserver_presentation_focus_state(state, cx);
                }
                GpuiPendingProjectSwitchPayload::WorkspaceTerminalFocus(message) => {
                    self.focus_local_workspace_terminal_from_message(&message, cx);
                }
            }
        }
    }

    pub(crate) fn swap_agents_workspace_for_active_project(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let new_project_id =
            gpui_active_project_id_from_snapshot(self.latest_sidebar_project_snapshot.as_ref())
                .map(str::to_string);
        self.swap_agents_workspace_to_project_id(new_project_id, cx)
    }

    pub(crate) fn swap_agents_workspace_to_project_id(
        &mut self,
        new_project_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.agents_workspace_project_id == new_project_id {
            return false;
        }
        self.source_code_server_runtime
            .pending_remote_prompt_editor_request = None;
        self.capture_outgoing_project_view_state();
        if self.agents_workspace_project_id.is_none()
            && new_project_id.as_ref().is_some_and(|project_id| {
                !self
                    .parked_agents_workspaces_by_project
                    .contains_key(project_id)
                    && self
                        .local_workspace_session_mappings
                        .keys()
                        .all(|key| key.project_id == *project_id)
            })
        {
            self.agents_workspace_project_id = new_project_id;
            self.apply_project_view_state_for_active_project(cx);
            self.persist_shell_layout_state();
            return true;
        }

        /*
        CDXC:Workarea 2026-07-23:
        Agents split/tab topology is project/worktree-owned. Park the complete
        outgoing writer-owned shell model plus its canonical session mappings
        before activating another project. Reconciliation may then update only
        the incoming model, so it can never normalize away another project's
        split branches or delete its shell sessions.
        */
        let outgoing_state = agents_workspace_project_state_to_shell_state_json(
            &self.agents_workspace,
            self.agents_workspace_project_id.as_deref(),
            &self.local_workspace_session_mappings,
            &self.remote_attach_sessions,
            &self.agents_chat_mode_sessions,
            &self.agents_delayed_send_timers,
            &self.agents_send_when_stopped_watchers,
            SystemTime::now(),
        );
        /*
        CDXC:SessionChat 2026-08-26:
        Chat pages park with the terminal owners rather than being destroyed and
        reloaded, so the outgoing project's Chromium browsers survive the switch.
        The bundle is taken before the ownership branch below because it also
        clears the switch-scoped chat state that must go regardless of whether
        there is a project id to park it under.
        */
        let parked_chat_runtime = self.park_all_agents_chat_surfaces(cx);
        match self.agents_workspace_project_id.take() {
            Some(old_project_id) => {
                self.parked_agents_workspaces_by_project
                    .insert(old_project_id.clone(), outgoing_state);
                self.parked_agents_terminal_runtimes_by_project.insert(
                    old_project_id.clone(),
                    ParkedAgentsTerminalRuntime {
                        runtime_sessions: std::mem::take(
                            &mut self.agents_terminal_runtime_sessions,
                        ),
                        gpui_engine_terminals: std::mem::take(
                            &mut self.agents_gpui_engine_terminals,
                        ),
                        runtime_osc_states: std::mem::take(
                            &mut self.agents_terminal_runtime_osc_states,
                        ),
                        gpui_engine_close_confirms: std::mem::take(
                            &mut self.agents_gpui_engine_close_confirms,
                        ),
                    },
                );
                self.parked_agents_chat_runtimes_by_project
                    .insert(old_project_id, parked_chat_runtime);
            }
            // No owning project id means there is nothing to park these pages
            // under and nothing that could ever restore them, so they are
            // destroyed here exactly as the pre-parking teardown did.
            None => drop(parked_chat_runtime),
        }

        let restored_state = new_project_id.as_ref().and_then(|project_id| {
            self.parked_agents_workspaces_by_project
                .remove(project_id)
                .and_then(|state| {
                    agents_workspace_project_state_from_shell_state(&state, Some(project_id))
                })
                .filter(|(_, mappings, _, _, _)| {
                    mappings.keys().all(|key| key.project_id == *project_id)
                })
        });
        let (
            workspace,
            mappings,
            remote_mappings,
            chat_mode_sessions,
            delayed_send_restore_intents,
        ) = restored_state.unwrap_or_else(|| {
            (
                WorkspaceModel::empty_default(),
                HashMap::new(),
                HashMap::new(),
                HashSet::new(),
                Vec::new(),
            )
        });
        self.agents_workspace = workspace;
        self.local_workspace_session_mappings = mappings;
        self.remote_attach_sessions.extend(remote_mappings);
        self.agents_workspace_project_id = new_project_id;
        let restored_terminal_runtime = self
            .agents_workspace_project_id
            .as_ref()
            .and_then(|project_id| {
                self.parked_agents_terminal_runtimes_by_project
                    .remove(project_id)
            })
            .unwrap_or_default();
        self.agents_terminal_runtime_sessions = restored_terminal_runtime.runtime_sessions;
        self.agents_gpui_engine_terminals = restored_terminal_runtime.gpui_engine_terminals;
        self.agents_terminal_runtime_osc_states = restored_terminal_runtime.runtime_osc_states;
        self.agents_gpui_engine_close_confirms =
            restored_terminal_runtime.gpui_engine_close_confirms;
        // The incoming project's chat pages come back with the workspace model
        // whose session ids they are keyed by, so `ensure_agents_chat_surface`
        // finds them and the reconcile pass below only has to make them visible.
        let restored_chat_runtime = self
            .agents_workspace_project_id
            .as_ref()
            .and_then(|project_id| {
                self.parked_agents_chat_runtimes_by_project
                    .remove(project_id)
            })
            .unwrap_or_default();
        self.restore_parked_agents_chat_surfaces(restored_chat_runtime, cx);

        /*
        Shell, pane, and runtime ids are intentionally project-local. Tear down
        the outgoing process-local attachment graph as one ownership unit before
        the incoming model can reuse numeric ids. This drops only local attach
        clients and views; daemon zmx sessions remain alive and reattach through
        the normal sidebar focus path.
        */
        self.local_workspace_attach_pending.clear();
        self.local_workspace_lifecycle_requests.clear();
        self.local_workspace_latest_focus_key = None;
        self.local_app_shot_session_mappings.clear();
        self.agents_chat_mode_sessions = chat_mode_sessions;
        self.agents_terminal_startup_coordinator = AgentsTerminalStartupCoordinator::new();
        self.agents_terminal_surface_host = NativeTerminalSurfaceHost::new();
        self.agents_terminal_surface_lifecycle = NativeTerminalSurfaceLifecycleState::new();
        self.agents_terminal_startup_launch_payload_source =
            AgentsTerminalStartupLaunchPayloadSource::new_empty();
        self.agents_terminal_launch_payload_source = AgentsTerminalLaunchPayloadSource::new_empty();
        self.agents_terminal_startup_body_slot_geometries.clear();
        self.agents_terminal_parked_owner_body_slot_geometries
            .clear();
        self.agents_terminal_mount_slot_bounds.clear();
        self.pending_terminal_paste_confirmation = None;
        self.terminal_paste_confirmation_dialog_open = false;
        self.agents_delayed_send_timers.clear();
        self.agents_send_when_stopped_watchers.clear();
        self.terminal_search_inputs.clear();
        self.terminal_search_input_subscriptions.clear();
        self.terminal_search_focus_pending = None;
        self.project_editor_companion_terminal_session_id = None;
        self.project_editor_companion_secondary_terminal_session_id = None;
        #[cfg(target_os = "macos")]
        {
            self.agents_terminal_ghostty_surfaces.clear();
            self.agents_terminal_parked_runtime_owners.clear();
            self.agents_terminal_close_confirms.pending_by_slot.clear();
            self.agents_terminal_startup_ghostty_surfaces.clear();
            self.agents_terminal_host_native_views.clear();
            self.agents_terminal_startup_host_native_views.clear();
            self.agents_terminal_ghostty_surface_config_requests.clear();
            self.agents_terminal_startup_ghostty_surface_config_requests
                .clear();
            self.agents_terminal_appkit_focused_host = None;
        }
        self.workspace_tab_scroll_handles.clear();
        self.workspace_leaf_layout_bounds.clear();
        self.workspace_split_layout_metrics.clear();
        self.workspace_split_drag = None;
        self.workspace_split_hovering = None;
        self.workspace_split_hover_visible = None;
        self.workspace_drop_feedback = None;
        self.workspace_tab_drag_active = false;
        self.pending_workspace_tab_click = None;

        if matches!(self.shell_focus, ShellFocusTarget::AgentsPane(_)) {
            self.set_shell_focus(ShellFocusTarget::AgentsPane(
                self.agents_workspace.focused_pane,
            ));
        }
        if matches!(
            self.previous_non_command_focus,
            Some(ShellFocusTarget::AgentsPane(_))
        ) {
            self.previous_non_command_focus = Some(ShellFocusTarget::AgentsPane(
                self.agents_workspace.focused_pane,
            ));
        }
        self.restore_gpui_agents_delayed_sends(delayed_send_restore_intents, cx);
        self.apply_project_view_state_for_active_project(cx);
        self.persist_shell_layout_state();
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        cx.notify();
        true
    }

    pub(crate) fn swap_command_pane_for_active_project(
        &mut self,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:CommandPane 2026-07-10:
        macOS parity: command panels are per-project (`NativeProject.commandsPanel`
        in native-sidebar.tsx), so an active-project change swaps the whole
        command pane instead of sharing one global panel. The outgoing pane is
        parked as shell-state JSON keyed by its gxserver project id; parking
        must not kill daemon zmx sessions (only explicit close does), so the
        gxserver mappings are rebuilt from the incoming pane instead of being
        forgotten one-by-one. A legacy pane restored without a project id
        splits its tabs into their owning projects once. The project epoch
        invalidates in-flight attach completions from before the swap.
        */
        let new_project_id =
            gpui_active_project_id_from_snapshot(self.latest_sidebar_project_snapshot.as_ref())
                .map(str::to_string);
        if self.command_pane_project_id == new_project_id {
            return;
        }
        self.command_pane_project_epoch += 1;

        let live_pane_json = command_pane_model_to_shell_state_json_with_delayed_send_timers(
            &self.command_pane,
            &self.command_delayed_send_timers,
            SystemTime::now(),
        );
        match self.command_pane_project_id.take() {
            Some(old_project_id) => {
                self.parked_command_panes_by_project
                    .insert(old_project_id, live_pane_json);
            }
            None => {
                for (project_id, pane_json) in
                    split_command_pane_shell_state_json_by_gxserver_project(
                        &live_pane_json,
                        new_project_id.as_deref(),
                    )
                {
                    self.parked_command_panes_by_project
                        .insert(project_id, pane_json);
                }
            }
        }

        let content_height = command_pane_content_height(window);
        let command_default_height_px = command_pane_default_height_px_from_shared_settings(
            &shared_settings::shared_sidebar_settings_snapshot(),
        );
        let restored_pane = new_project_id.as_ref().and_then(|project_id| {
            self.parked_command_panes_by_project
                .remove(project_id)
                .and_then(|pane_json| {
                    command_pane_model_from_shell_state_with_default_height_px(
                        &pane_json,
                        content_height,
                        command_default_height_px,
                    )
                })
        });
        self.command_pane = restored_pane.unwrap_or_else(|| {
            CommandPaneModel::shell_default_with_default_height_px(
                content_height,
                command_default_height_px,
            )
        });
        self.command_pane_project_id = new_project_id;

        /*
        CDXC:CommandPane 2026-07-10:
        Command session ids are per-pane counters, so ids can collide across
        projects. Every piece of live command runtime state belongs to the
        outgoing pane and is torn down wholesale here instead of pruned by
        session-id membership; dropping engine records and Ghostty owners
        kills only local attach shells and surfaces, never the daemon zmx
        sessions, and the incoming pane remounts through the normal attach
        flow. Delayed Send / Close After Done runtime timers clear like the
        command Sleep contract; their restart checkpoints and armed booleans
        live in the parked shell-state JSON.
        */
        self.command_gxserver_session_mappings =
            command_gxserver_session_mappings_from_command_model(&self.command_pane);
        /*
        CDXC:RemoteMachines 2026-08-29:
        Remote Action tabs park with their project exactly like local command
        tabs: the incoming pane's own remote identities replace the outgoing
        pane's, and the parked project's remote sessions stay alive on their
        machine until that tab is really closed. The askpass helpers belong to
        the torn-down local ssh clients, so they are dropped with them.
        */
        self.command_remote_action_sessions =
            command_remote_action_sessions_from_command_model(&self.command_pane);
        #[cfg(target_os = "macos")]
        self.command_remote_attach_askpass_scripts.clear();
        self.command_gxserver_attach_pending.clear();
        self.command_terminal_launch_payload_source
            .remove_all_payloads();
        self.command_gpui_engine_terminals.clear();
        self.command_gpui_engine_close_confirms.clear();
        #[cfg(target_os = "macos")]
        {
            self.command_terminal_ghostty_surfaces.clear();
            self.command_terminal_parked_runtime_owners.clear();
            self.command_terminal_close_confirms.pending_by_slot.clear();
        }
        self.command_delayed_send_timers.clear();
        self.command_close_after_done_timers.clear();
        self.clear_command_resize_hover_state_if_command_pane_hidden();
        if self.shell_focus == ShellFocusTarget::CommandPane && !self.command_pane.has_sessions() {
            self.restore_previous_non_command_focus_or_default();
        }
        self.scroll_focused_command_active_tab();
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        self.persist_shell_layout_state();
        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        cx.notify();
    }

    pub(crate) fn swap_browser_tabs_for_active_project(&mut self, cx: &mut gpui::Context<Self>) {
        let new_project_id =
            gpui_active_project_id_from_snapshot(self.latest_sidebar_project_snapshot.as_ref())
                .map(str::to_string);
        self.swap_browser_tabs_to_project_id(new_project_id, cx);
    }

    pub(crate) fn swap_browser_tabs_to_project_id(
        &mut self,
        new_project_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.browser_tabs_project_id == new_project_id {
            return;
        }

        // A pre-project-scoping workspace belongs to the first real project
        // that claims it. Subsequent project changes park and restore the
        // complete tab model instead of sharing it across projects.
        if self.browser_tabs_project_id.is_none()
            && new_project_id.as_ref().is_some_and(|project_id| {
                !self.parked_browser_tabs_by_project.contains_key(project_id)
            })
        {
            self.browser_tabs_project_id = new_project_id;
            self.persist_shell_layout_state();
            return;
        }

        self.browser_tabs_project_epoch = self.browser_tabs_project_epoch.wrapping_add(1);

        /*
        CDXC:Browser 2026-08-26:
        Browser ids are project-local, so the live surface/input maps can only
        describe one project at a time — but that is a reason to move the
        outgoing project's runtime out of the way, not to destroy it. The whole
        bundle parks under the outgoing project id (hidden, still loaded) and
        the incoming project's bundle, if it has one, becomes live again, so a
        project switch no longer sleeps and reloads every browser tab of the
        project the user just left.

        The projectless pre-project model is the one exception: it has no key to
        park under and its model is dropped here, so its pages go with it.
        */
        if let Some(old_project_id) = self.browser_tabs_project_id.take() {
            self.parked_browser_tabs_by_project
                .insert(old_project_id.clone(), self.browser_tabs.clone());
            let parked_runtime = self.park_all_browser_surfaces(cx);
            if parked_runtime.holds_runtime_state() {
                self.parked_browser_runtimes_by_project
                    .insert(old_project_id, parked_runtime);
            } else {
                self.parked_browser_runtimes_by_project
                    .remove(&old_project_id);
            }
        } else {
            self.drop_all_browser_surfaces(cx);
        }
        self.browser_tabs = new_project_id
            .as_ref()
            .and_then(|project_id| self.parked_browser_tabs_by_project.remove(project_id))
            .unwrap_or_else(|| {
                BrowserTabModel::shell_address_only_with_profile(
                    self.browser_profiles.active_profile_id(),
                )
            });
        match new_project_id
            .as_ref()
            .and_then(|project_id| self.parked_browser_runtimes_by_project.remove(project_id))
        {
            Some(parked_runtime) => self.restore_parked_browser_surfaces(parked_runtime),
            // A model that never had a parked runtime starts a fresh runtime
            // identity, so no surface parked under an older one can claim it.
            None => self.browser_tabs_runtime_key = self.browser_tabs_project_epoch,
        }
        self.browser_tabs_project_id = new_project_id;
        self.browser_url = self.browser_tabs.active_address_value();

        // A pending media prompt belongs to the page that raised it and cannot
        // be answered from another project's workarea; dropping it releases the
        // page's `getUserMedia()` promise instead of leaving it hanging.
        self.browser_media_permission_prompts.clear();
        self.browser_tab_scroll_handles.clear();
        self.browser_leaf_layout_bounds.clear();
        self.browser_split_layout_metrics.clear();
        self.browser_tab_drop_feedback = None;
        self.browser_tab_drag_active = false;
        self.browser_split_drag = None;
        self.hovered_browser_tab = None;
        self.pending_browser_find_focus = None;
        self.pending_browser_address_focus = None;
        self.pending_browser_content_focus = None;

        if matches!(
            self.shell_focus,
            ShellFocusTarget::BrowserPane(_) | ShellFocusTarget::BrowserSurface
        ) {
            self.set_shell_focus(ShellFocusTarget::BrowserPane(
                self.browser_tabs.focused_pane,
            ));
        }
        /*
        CDXC:Browser 2026-08-26:
        Restored surfaces come back hidden, because parking hid them. Run the
        normal visibility gate so the incoming project's rendered tabs are shown
        again by the one owner of that decision, instead of leaving the Browser
        workarea black until the next unrelated repaint.
        */
        self.update_active_mode_cef_child_visibility(cx);
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn handle_browser_page_metadata_event(
        &mut self,
        tab_id: BrowserTabId,
        event: cef::BrowserPageMetadataEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        match event {
            cef::BrowserPageMetadataEvent::AddressChanged(url) => {
                /*
                CDXC:Browser 2026-06-22-07:23:
                CEF-reported Browser navigation owns the runtime URL for that tab. Update the URL-derived fallback title, refresh the active address field only when the reporting tab is selected, keep the selected tab's own CEF surface visible, and persist through the existing Browser URL sanitizer instead of writing raw navigation details directly.
                */
                // A cross-origin navigation invalidates any prompt the previous
                // document raised; same-origin navigation keeps it pending.
                self.clear_browser_media_permission_prompts_for_navigation(tab_id, &url);
                if !self.browser_tabs.record_page_address_change(tab_id, url) {
                    return;
                }
                if let Some(pane_id) = find_browser_leaf_id_for_tab(&self.browser_tabs.root, tab_id)
                    && self.browser_tabs.active_tab_id_for_pane(pane_id) == Some(tab_id)
                {
                    let address_value = self.browser_tabs.address_value_for_pane(pane_id);
                    if self.browser_tabs.active_tab == tab_id {
                        self.browser_url = address_value.clone();
                    }
                    self.set_browser_address_input_value(pane_id, address_value, window, cx);
                } else if self.browser_tabs.active_tab == tab_id {
                    let address_value = self.browser_tabs.active_address_value();
                    self.browser_url = address_value.clone();
                }
                self.update_active_mode_cef_child_visibility(cx);
                self.persist_shell_layout_state();
                cx.notify();
            }
            cef::BrowserPageMetadataEvent::CloseRequested => {
                self.close_browser_tab(tab_id, window, cx);
            }
            cef::BrowserPageMetadataEvent::TitleChanged(title) => {
                /*
                CDXC:Browser 2026-06-22-07:23:
                CEF page titles change the visible Browser tab-strip label while the app runs.

                CDXC:Browser 2026-07-12:
                Shell-state serialization now persists the bounded last displayed title (`cachedTitle`) so restart keeps the same label; raw titles still never enter history or URL persistence.
                */
                if self.browser_tabs.record_page_title_change(tab_id, title) {
                    cx.notify();
                }
            }
            cef::BrowserPageMetadataEvent::FaviconUrlChanged(favicon_url) => {
                /*
                CDXC:Browser 2026-06-22-09:11:
                CEF favicon metadata may update Browser tab chrome at runtime, but favicon URLs can reveal page-owned or user-specific state. Store them only on the in-memory BrowserTab record, never persist them, and repaint without touching shell-state JSON.
                */
                if self
                    .browser_tabs
                    .record_page_favicon_url_change(tab_id, favicon_url)
                {
                    cx.notify();
                }
            }
            cef::BrowserPageMetadataEvent::FindResult {
                match_count,
                active_match_ordinal,
                final_update,
            } => {
                let Some(find) = self.browser_find_states.get_mut(&tab_id) else {
                    return;
                };
                if find.match_count == match_count
                    && find.active_match_ordinal == active_match_ordinal
                    && find.final_update == final_update
                {
                    return;
                }
                find.match_count = match_count.max(0);
                find.active_match_ordinal = active_match_ordinal.max(0);
                find.final_update = final_update;
                cx.notify();
            }
            cef::BrowserPageMetadataEvent::LoadingStateChanged {
                is_loading,
                can_go_back,
                can_go_forward,
            } => {
                if self.browser_tabs.record_page_loading_state_change(
                    tab_id,
                    is_loading,
                    can_go_back,
                    can_go_forward,
                ) {
                    cx.notify();
                }
            }
        }
    }

    pub(crate) fn ensure_active_browser_surface(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> Option<Entity<CefSurface>> {
        self.ensure_browser_surface_for_pane(self.browser_tabs.focused_pane, cx)
    }

    pub(crate) fn ensure_browser_surface_for_pane(
        &mut self,
        pane_id: BrowserPaneId,
        cx: &mut gpui::Context<Self>,
    ) -> Option<Entity<CefSurface>> {
        let (tab_id, url, profile_id) = self.active_loaded_browser_tab_for_pane(pane_id)?;
        self.ensure_browser_surface_for_tab(tab_id, url, profile_id, cx)
    }

    pub(crate) fn load_browser_cef_url_for_pane(
        &mut self,
        pane_id: BrowserPaneId,
        url: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(surface) = self.ensure_browser_surface_for_pane(pane_id, cx) {
            surface.update(cx, |surface, _| surface.load_url(url));
        }
        self.update_active_mode_cef_child_visibility(cx);
    }

    pub(crate) fn perform_browser_toolbar_action(
        &mut self,
        pane_id: BrowserPaneId,
        action: BrowserToolbarAction,
        cx: &mut gpui::Context<Self>,
    ) {
        if !matches!(
            action,
            BrowserToolbarAction::Back
                | BrowserToolbarAction::Forward
                | BrowserToolbarAction::Reload
                | BrowserToolbarAction::StopLoading
        ) {
            return;
        }
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            return;
        }

        self.active_mode = TitlebarMode::Browser;
        self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
        if !self.browser_tabs.focus_pane(pane_id) {
            return;
        }
        self.set_shell_focus(ShellFocusTarget::BrowserPane(pane_id));

        if let Some(surface) = self.browser_surface_for_pane(pane_id) {
            surface.update(cx, |surface, _| match action {
                BrowserToolbarAction::Back => surface.go_back(),
                BrowserToolbarAction::Forward => surface.go_forward(),
                BrowserToolbarAction::Reload => surface.reload(),
                BrowserToolbarAction::StopLoading => surface.stop_load(),
                BrowserToolbarAction::Home
                | BrowserToolbarAction::FeedbackTool
                | BrowserToolbarAction::ResetZoom
                | BrowserToolbarAction::ResetMediaPermissions
                | BrowserToolbarAction::HistoryMenu
                | BrowserToolbarAction::ProfileMenu
                | BrowserToolbarAction::DevTools => {}
            });
        }

        self.update_active_mode_cef_child_visibility(cx);
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn navigate_browser_home_from_toolbar(
        &mut self,
        pane_id: BrowserPaneId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        The Browser home target is the current project's primary remote web
        page, carried by the active-project snapshot from the owning gxserver
        machine. Navigate the selected tab through the normal
        address commit path so CEF history, tab metadata, focus, and shell
        persistence all stay under their existing owners.
        */
        let home_url = browser_shell_default_url(
            self.latest_sidebar_project_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.browser_home_url.as_deref()),
        );
        self.browser_address_input_editing.remove(&pane_id);
        self.pending_browser_address_focus = None;
        self.commit_browser_address_for_pane(pane_id, home_url.clone(), cx);
        self.set_browser_address_input_value_unchecked(pane_id, home_url, window, cx);
    }

    pub(crate) fn prepare_browser_toolbar_right_action(
        &mut self,
        pane_id: BrowserPaneId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:Titlebar 2026-06-22-15:52:
        Browser toolbar commands are user-facing Browser activation routes. In Quick/projectless GPUI context they must no-op through the same titlebar availability guard as mode clicks and Option workarea hotkeys, instead of directly switching activeMode to Browser.
        */
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            return false;
        }
        if !self.browser_tabs.focus_pane(pane_id) {
            return false;
        }
        self.active_mode = TitlebarMode::Browser;
        self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
        self.set_shell_focus(ShellFocusTarget::BrowserPane(pane_id));
        self.update_active_mode_cef_child_visibility(cx);
        self.persist_shell_layout_state();
        true
    }

    pub(crate) fn run_browser_feedback_tool_from_toolbar(
        &mut self,
        pane_id: BrowserPaneId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.prepare_browser_toolbar_right_action(pane_id, cx) {
            return;
        }
        let address_value = self.browser_tabs.address_value_for_pane(pane_id);
        if browser_feedback_tool_unavailable_url(&address_value) {
            cx.notify();
            return;
        }
        let Some(surface) = self.browser_surface_for_pane(pane_id) else {
            window.push_notification(
                Notification::warning("Open a Browser page before starting feedback."),
                cx,
            );
            cx.notify();
            return;
        };
        let script = browser_agentation_feedback_injection_script();
        let injected = surface.update(cx, |surface, _| {
            surface.inject_feedback_tool_script(&script)
        });
        if !injected {
            window.push_notification(
                Notification::warning(format!(
                    "{} feedback is not ready on this Browser page.",
                    BROWSER_FEEDBACK_TOOL_AGENTATION_LABEL
                )),
                cx,
            );
        }
        cx.notify();
    }

    pub(crate) fn reset_browser_zoom_from_toolbar(
        &mut self,
        pane_id: BrowserPaneId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.prepare_browser_toolbar_right_action(pane_id, cx) {
            return;
        }
        if let Some(surface) = self.browser_surface_for_pane(pane_id) {
            surface.update(cx, |surface, _| surface.reset_zoom());
        } else {
            window.push_notification(
                Notification::warning("Open a Browser page before resetting zoom."),
                cx,
            );
        }
        cx.notify();
    }

    pub(crate) fn toggle_browser_devtools_from_toolbar(
        &mut self,
        pane_id: BrowserPaneId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.prepare_browser_toolbar_right_action(pane_id, cx) {
            return;
        }
        if let Some(surface) = self.browser_surface_for_pane(pane_id) {
            surface.update(cx, |surface, _| surface.toggle_dev_tools());
        } else {
            window.push_notification(
                Notification::warning("Open a Browser page before toggling DevTools."),
                cx,
            );
        }
        cx.notify();
    }

    pub(crate) fn show_browser_profile_menu(
        &mut self,
        pane_id: BrowserPaneId,
        position: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Browser 2026-06-23-11:14:
        Browser Profiles are a normal GPUI Browser toolbar feature. The menu reflects real shell profile state through an OS-owned NativeMenu with checked generated profile rows and New Profile; do not use GPUI overlays, hidden hit regions, hit-test routing, or user-entered profile names.
        */
        if !self.prepare_browser_toolbar_right_action(pane_id, cx) {
            return;
        }

        let selected_profile = self
            .browser_tabs
            .active_tab_for_pane(pane_id)
            .map(|tab| tab.profile_id)
            .unwrap_or_else(|| self.browser_profiles.active_profile_id());
        let mut menu = NativeMenu::new();
        for profile_id in self.browser_profiles.profile_ids() {
            menu = menu.menu_with_check(
                profile_id.display_label(),
                profile_id == selected_profile,
                Box::new(SelectBrowserProfile {
                    pane_id: pane_id.0,
                    profile_id: profile_id.0,
                }),
            );
        }

        menu.separator()
            .menu(
                "New Profile...",
                Box::new(CreateBrowserProfile { pane_id: pane_id.0 }),
            )
            .show(position, window, cx);
    }

    pub(crate) fn select_browser_profile_from_menu(
        &mut self,
        pane_id: BrowserPaneId,
        profile_id: BrowserProfileId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.browser_profile_actions_available() {
            return;
        }
        if !self.browser_profiles.contains_profile(profile_id) {
            return;
        }
        if !self.prepare_browser_toolbar_right_action(pane_id, cx) {
            return;
        }
        let Some(tab_id) = self.browser_tabs.active_tab_id_for_pane(pane_id) else {
            return;
        };
        let selected_profile_changed = self.browser_profiles.select_profile(profile_id);
        let tab_profile_changed = self.browser_tabs.set_tab_profile(tab_id, profile_id);
        if tab_profile_changed {
            self.remove_browser_surface(tab_id, cx);
            self.sync_active_browser_tab_to_surface(window, cx);
        }
        if selected_profile_changed || tab_profile_changed {
            self.persist_shell_layout_state();
            cx.notify();
        }
    }

    pub(crate) fn create_browser_profile_from_menu(
        &mut self,
        pane_id: BrowserPaneId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.browser_profile_actions_available() {
            return;
        }
        if !self.prepare_browser_toolbar_right_action(pane_id, cx) {
            return;
        }
        let Some(profile_id) = self.browser_profiles.create_generated_profile() else {
            window.push_notification(Notification::warning("Browser profile limit reached."), cx);
            cx.notify();
            return;
        };
        if let Some(tab_id) = self.browser_tabs.active_tab_id_for_pane(pane_id) {
            if self.browser_tabs.set_tab_profile(tab_id, profile_id) {
                self.remove_browser_surface(tab_id, cx);
                self.sync_active_browser_tab_to_surface(window, cx);
            }
        }
        self.persist_shell_layout_state();
        window.push_notification(
            Notification::info(format!(
                "Created Browser profile: {}.",
                profile_id.display_label()
            )),
            cx,
        );
        cx.notify();
    }

    pub(crate) fn browser_profile_actions_available(&self) -> bool {
        /*
        CDXC:Browser 2026-06-23-11:28:
        Profile menu actions are registered globally like other NativeMenu commands, so the handler boundary must repeat the Browser availability gate. Stale or direct action dispatch cannot create, select, persist, or touch CEF profile state outside the Browser workspace.
        */
        self.titlebar_mode_available(TitlebarMode::Browser)
    }

    pub(crate) fn show_browser_recent_history_menu(
        &self,
        pane_id: BrowserPaneId,
        position: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let rows = self
            .browser_tabs
            .pane_history_rows(pane_id, BROWSER_HISTORY_MENU_MAX_ROWS);
        if rows.is_empty() {
            return;
        }

        let mut item_count = 0;
        let mut menu = NativeMenu::new()
            .menu_with_disabled("History", true, Box::new(BrowserHistoryMenuLabel))
            .separator();
        for row in rows {
            let Some(sanitized_url) = sanitize_browser_tab_url_for_state(&row.url) else {
                continue;
            };
            item_count += 1;
            menu = menu.menu(
                browser_tab_title_for_url(&sanitized_url),
                Box::new(OpenBrowserHistoryEntryInNewTab {
                    pane_id: pane_id.0,
                    index: row.index as u64,
                }),
            );
        }
        if item_count == 0 {
            return;
        }
        menu.show(position, window, cx);
    }

    pub(crate) fn open_browser_history_entry_in_new_tab(
        &mut self,
        pane_id: BrowserPaneId,
        index: usize,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            return;
        }
        let Some(row) = self
            .browser_tabs
            .pane_history_rows(pane_id, BROWSER_HISTORY_MENU_MAX_ROWS)
            .into_iter()
            .find(|row| row.index == index)
        else {
            return;
        };
        let Some(url) = sanitize_browser_tab_url_for_state(&row.url) else {
            return;
        };
        if !self.browser_tabs.focus_pane(pane_id) {
            return;
        }
        let created_tab_id = self.browser_tabs.add_loaded_popup_tab(
            url.clone(),
            self.browser_profiles.active_profile_id(),
            cef::BrowserPopupPlacement::Selected,
        );
        let Some(created_tab_id) = created_tab_id else {
            return;
        };
        self.request_sidebar_browser_tab_reveal(created_tab_id);
        self.browser_url = url;
        self.active_mode = TitlebarMode::Browser;
        self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
        self.set_shell_focus(ShellFocusTarget::BrowserPane(pane_id));
        self.sync_active_browser_tab_to_surface(window, cx);
        self.scroll_browser_pane_active_tab(pane_id);
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn remove_browser_surface(
        &mut self,
        tab_id: BrowserTabId,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(surface) = self.browser_surfaces.remove(&tab_id) {
            surface.update(cx, |surface, _| surface.set_visible(false));
        }
        // The page that asked is gone with its surface; cancel its request
        // instead of leaving a prompt bound to a dead tab.
        self.clear_browser_media_permission_prompts(tab_id);
    }

    pub(crate) fn sync_active_browser_tab_to_surface(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Browser 2026-06-22-05:56:
        Browser tab selection is shell state in this slice but still has to feel real: selecting a tab updates the active tab id, the toolbar URL state, the address input text, and the active tab's owned CEF surface without reloading existing tab page state.

        CDXC:Browser 2026-06-22-06:59:
        Selecting a Browser tab should reveal that tab's CEF entity and hide any Browser CEF entity that is not the active loaded tab of a rendered Browser leaf. Address-only placeholder tabs deliberately do not create or show a CEF surface, so the Browser body stays empty instead of displaying stale page state from another tab.

        CDXC:Browser 2026-06-22-09:55:
        Selection still materializes only the focused/global active loaded tab for toolbar parity, but the visibility gate now also keeps any other rendered Browser leaf's already-created active loaded surface visible. Inactive restored loaded tabs without CEF entities remain placeholders instead of being created from render or visibility updates.
        */
        let pane_id = self.browser_tabs.focused_pane;
        let address_value = self.browser_tabs.address_value_for_pane(pane_id);
        self.browser_url = address_value.clone();
        self.set_browser_address_input_value(pane_id, address_value, window, cx);
        self.ensure_browser_surface_for_pane(pane_id, cx);
        self.update_active_mode_cef_child_visibility(cx);
    }

    pub(crate) fn select_browser_tab_in_pane(
        &mut self,
        pane_id: BrowserPaneId,
        tab_id: BrowserTabId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            return;
        }
        if self.browser_tabs.select_tab_in_pane(pane_id, tab_id) {
            self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
            self.set_shell_focus(ShellFocusTarget::BrowserPane(pane_id));
            self.sync_active_browser_tab_to_surface(window, cx);
            self.scroll_browser_pane_active_tab(pane_id);
            self.persist_shell_layout_state();
            cx.notify();
        }
    }

    pub(crate) fn focus_browser_pane(
        &mut self,
        pane_id: BrowserPaneId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            return false;
        }
        if self.browser_tabs.focus_pane(pane_id) {
            self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
            self.set_shell_focus(ShellFocusTarget::BrowserPane(pane_id));
            self.sync_active_browser_tab_to_surface(window, cx);
            self.scroll_browser_pane_active_tab(pane_id);
            self.persist_shell_layout_state();
            cx.notify();
            true
        } else {
            false
        }
    }

    pub(crate) fn add_browser_tab(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            return;
        }
        let default_url = browser_shell_default_url(
            self.latest_sidebar_project_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.browser_home_url.as_deref()),
        );
        let created_tab_id = self.browser_tabs.add_loaded_popup_tab(
            default_url.clone(),
            self.browser_profiles.active_profile_id(),
            cef::BrowserPopupPlacement::Selected,
        );
        if let Some(created_tab_id) = created_tab_id {
            self.request_sidebar_browser_tab_reveal(created_tab_id);
        }
        self.browser_url = default_url;
        let pane_id = self.browser_tabs.focused_pane;
        self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
        self.set_shell_focus(ShellFocusTarget::BrowserPane(pane_id));
        self.sync_active_browser_tab_to_surface(window, cx);
        self.scroll_focused_browser_pane_active_tab();
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn add_browser_tab_from_hotkey(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:FocusMode 2026-06-22-12:51:
        Cmd+N is an explicit Browser-opening command in the GPUI shell. Switch to Browser before reusing the normal new-tab helper so the new address-only tab is inserted in the focused Browser pane, Browser lifecycle is marked awake, shell focus moves to Browser, address/CEF visibility sync runs, the active tab scrolls into view, and shell state persists.

        CDXC:Titlebar 2026-06-22-15:52:
        Cmd+N must respect Quick/projectless titlebar availability before switching modes. Browser placeholder tabs stay part of durable shell state, but user-facing Browser creation commands cannot make Browser active when the native titlebar would show it disabled.

        CDXC:CommandPalette 2026-06-26-06:47:
        Focused-pane Browser open commands must match native command-panel parity: CommandPane shell focus no-ops because the native command terminal titlebar branch default-returns, while Agents, Browser, and project-editor focus still create and focus a Browser tab.
        */
        if !gpui_focused_pane_open_browser_hotkey_should_open(self.shell_focus) {
            return;
        }
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            return;
        }
        self.active_mode = TitlebarMode::Browser;
        self.add_browser_tab(window, cx);
    }

    pub(crate) fn add_browser_tab_in_pane_from_action(
        &mut self,
        pane_id: BrowserPaneId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            return;
        }
        if self.browser_tabs.focus_pane(pane_id) {
            self.active_mode = TitlebarMode::Browser;
            self.add_browser_tab(window, cx);
        }
    }

    pub(crate) fn split_browser_pane_with_new_tab_from_action(
        &mut self,
        pane_id: BrowserPaneId,
        zone: WorkspaceDropZone,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            return;
        }
        let default_url = browser_shell_default_url(
            self.latest_sidebar_project_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.browser_home_url.as_deref()),
        );
        if let Some(created_tab_id) = self.browser_tabs.split_new_loaded_tab_to_pane(
            pane_id,
            zone,
            self.browser_profiles.active_profile_id(),
            default_url.clone(),
        ) {
            self.request_sidebar_browser_tab_reveal(created_tab_id);
            self.browser_url = default_url;
            self.active_mode = TitlebarMode::Browser;
            self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
            self.set_shell_focus(ShellFocusTarget::BrowserPane(
                self.browser_tabs.focused_pane,
            ));
            self.sync_active_browser_tab_to_surface(window, cx);
            self.scroll_focused_browser_pane_active_tab();
            self.persist_shell_layout_state();
            cx.notify();
        }
    }

    pub(crate) fn close_browser_tab(
        &mut self,
        tab_id: BrowserTabId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.close_browser_tab_model(tab_id, window, cx) {
            return;
        }
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn select_browser_tab_from_action(
        &mut self,
        pane_id: BrowserPaneId,
        tab_id: BrowserTabId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.select_browser_tab_in_pane(pane_id, tab_id, window, cx);
    }

    pub(crate) fn close_browser_tab_from_action(
        &mut self,
        pane_id: BrowserPaneId,
        tab_id: BrowserTabId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self
            .browser_tabs
            .find_leaf(pane_id)
            .is_some_and(|leaf| leaf.tab_group.has_tab(tab_id))
        {
            self.close_browser_tab(tab_id, window, cx);
        }
    }

    pub(crate) fn close_browser_tab_model(
        &mut self,
        tab_id: BrowserTabId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.browser_tabs.tabs.iter().any(|tab| tab.id == tab_id) {
            return false;
        }
        let source_pane_id = find_browser_leaf_id_for_tab(&self.browser_tabs.root, tab_id);
        self.remove_browser_surface(tab_id, cx);
        self.browser_find_states.remove(&tab_id);
        self.browser_find_inputs.remove(&tab_id);
        self.browser_find_input_subscriptions.remove(&tab_id);
        if self.pending_browser_find_focus == Some(tab_id) {
            self.pending_browser_find_focus = None;
        }
        if !self
            .browser_tabs
            .close_tab(tab_id, self.browser_profiles.active_profile_id())
        {
            return false;
        }
        self.reconcile_browser_address_inputs();
        self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
        self.set_shell_focus(ShellFocusTarget::BrowserPane(
            self.browser_tabs.focused_pane,
        ));
        self.sync_active_browser_tab_to_surface(window, cx);
        if let Some(source_pane_id) = source_pane_id {
            self.scroll_browser_pane_active_tab(source_pane_id);
        }
        self.scroll_focused_browser_pane_active_tab();
        true
    }

    pub(crate) fn open_browser_popup_tab(
        &mut self,
        requested_url: String,
        placement: cef::BrowserPopupPlacement,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Browser 2026-06-22-07:14:
        CEF popup callbacks enter the shell outside normal GPUI mouse/key handlers, so handle them as Browser tab model mutations: create a selected tab, switch to Browser mode, show only that tab's CEF surface, update the address field, and persist through the existing sanitized Browser shell metadata path.

        CDXC:Browser 2026-06-23-11:43:
        Empty-target CEF popups must return handled with no shell side effects: no selected-tab mutation, Browser wake/focus, CEF surface sync/creation, shell-state persistence, notification, or fallback content transfer. Non-empty target URLs still follow the selected-tab Browser activation path.

        CDXC:Titlebar 2026-06-22-15:52:
        Popup callbacks are another Browser activation route. If Quick/projectless context has disabled Browser, discard the popup request before mutating Browser tab state so background CEF callbacks cannot bypass the titlebar guard.

        CDXC:Browser 2026-08-18:
        Middle-click and Cmd/Ctrl-click link opens arrive here with background
        placement. They are not an activation route: append the tab, persist,
        and repaint the tab strip without switching modes, moving shell focus,
        selecting the new tab, or scrolling the strip away from the page the
        user is still reading.
        */
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            return;
        }
        let popup_tab_id = self.browser_tabs.add_loaded_popup_tab(
            requested_url,
            self.browser_profiles.active_profile_id(),
            placement,
        );
        let Some(popup_tab_id) = popup_tab_id else {
            return;
        };
        self.request_sidebar_browser_tab_reveal(popup_tab_id);
        if matches!(placement, cef::BrowserPopupPlacement::Background) {
            /*
            CDXC:Browser 2026-08-18:
            A background tab is the one open with no other visible feedback: the
            page does not change, so with the sidebar chrome collapsed the row it
            created is off screen entirely. Say so, instead of letting the click
            look like it did nothing. (The reveal itself is still queued above, so
            the sections are already expanded when the sidebar comes back.)
            */
            if !gpui_sidebar_chrome_visible(self.sidebar_collapsed) {
                self.upsert_gpui_app_toast(
                    GpuiAppToast {
                        copy_text: None,
                        id: "gpui-browser-tab-created-in-sidebar".to_string(),
                        level: GpuiAppToastLevel::from_raw(Some("info")),
                        title: "New tab created in sidebar".to_string(),
                        description: None,
                        loading: false,
                        persistent: false,
                        duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                        epoch: 0,
                    },
                    cx,
                );
            }
            self.persist_shell_layout_state();
            cx.notify();
            return;
        }
        self.active_mode = TitlebarMode::Browser;
        self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
        self.set_shell_focus(ShellFocusTarget::BrowserPane(
            self.browser_tabs.focused_pane,
        ));
        self.sync_active_browser_tab_to_surface(window, cx);
        self.scroll_focused_browser_pane_active_tab();
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn set_active_mode(
        &mut self,
        mode: TitlebarMode,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.titlebar_mode_available(mode) {
            return false;
        }

        if mode != TitlebarMode::Agents {
            self.terminal_agent_bar_companion_focus_return = None;
        }

        let previous_mode = self.active_mode;
        /*
        CDXC:Telemetry 2026-08-26:
        Every workarea switch route — center tabs, the compact titlebar menu,
        Option+1..5, the command palette, and the sidebar focus helpers — funnels
        through set_active_mode, so this is the one place a `surface.opened` ping
        belongs. Re-selecting the current workarea is not a switch, and only the
        fixed spec enum is reportable (extension workareas send nothing).
        */
        if previous_mode != mode
            && let Some(surface) = gpui_telemetry_surface_for_titlebar_mode(mode)
        {
            record_gpui_surface_opened_telemetry(surface, cx.background_executor());
        }
        let previous_shell_focus = self.shell_focus;
        let previous_first_responder_target = self.first_responder_target;
        self.active_mode = mode;
        /*
        CDXC:CodeEditor 2026-07-05:
        Opening Source, Browser, Kanban, Automate, or Docs is an activation
        route. Match macOS by marking the selected project-editor mode awake
        before render, so deliberate page opens never show the sleeping
        placeholder. Auto-sleep remains limited to inactive modes.
        */
        if mode.is_project_editor_mode() {
            self.mark_project_editor_mode_awake(mode, cx);
        }
        self.agents_terminal_runtime_sessions
            .reconcile_with_workspace(&self.agents_workspace);
        self.sync_project_editor_companion_terminal_selection();
        if mode == TitlebarMode::Browser {
            self.seed_current_project_browser_tab_if_empty();
        }
        let restore_companion_focus = mode == TitlebarMode::Browser
            && self.project_editor_shell.left_companion_visible
            && self
                .project_editor_companion_terminal_slot_for_mode(mode)
                .is_some();
        if !restore_companion_focus {
            self.focus_default_surface_for_active_mode();
        }
        let requested_agents_terminal_focus =
            if let Some(FocusedTerminalTextMountTarget::Agents(slot_id)) =
                self.focused_terminal_text_mount_target()
            {
                self.request_agents_session_text_focus_handoff(slot_id, cx);
                Some(slot_id)
            } else {
                None
            };
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.terminalFocus.modeSwitch",
            serde_json::json!({
                "previousMode": format!("{:?}", previous_mode),
                "nextMode": format!("{:?}", mode),
                "previousShellFocus": format!("{:?}", previous_shell_focus),
                "nextShellFocus": format!("{:?}", self.shell_focus),
                "previousFirstResponderTarget": format!("{:?}", previous_first_responder_target),
                "requestedAgentsPane": requested_agents_terminal_focus.map(|slot| slot.pane_id.0),
                "requestedAgentsSession": requested_agents_terminal_focus.map(|slot| slot.session_id.0),
            }),
        );
        if mode == TitlebarMode::Browser && self.project_editor_shell.is_mode_awake(mode) {
            self.sync_active_browser_tab_to_surface(window, cx);
        } else {
            if let TitlebarMode::Extension(id) = mode {
                self.ensure_extension_view_runtime_for_current_context(id, cx);
            }
            self.ensure_project_workarea_runtime_cef_surfaces_for_current_context(cx);
            self.update_active_mode_cef_child_visibility(cx);
        }
        // Session Chat is also a native CEF child view. Reconcile it at the
        // same mode-switch boundary so entering a project workarea with the
        // companion hidden removes the old Agents-pane chat view immediately.
        self.reconcile_agents_pane_surfaces(cx);
        if restore_companion_focus {
            /*
            CDXC:Browser 2026-07-14:
            Switching workareas may materialize a fresh project Browser tab,
            but it must leave keyboard focus with the most recently active
            terminal/chat rendered in the companion pane. Browser chrome and
            CEF creation stay normal sibling layout; only an explicit Browser
            pane click transfers focus into the page.
            */
            self.focus_project_editor_companion(mode, window, cx);
        }
        self.scroll_all_active_tab_strips();
        self.persist_shell_layout_state();
        self.schedule_project_editor_auto_sleep_for_inactive_modes(cx);
        true
    }

    pub(crate) fn seed_current_project_browser_tab_if_empty(&mut self) -> bool {
        let Some(active_tab) = self.browser_tabs.active_tab() else {
            return false;
        };
        if self.browser_tabs.tabs.len() != 1 || active_tab.state != BrowserTabState::AddressOnly {
            return false;
        }

        /*
        CDXC:Browser 2026-07-14:
        A project with no saved Browser tabs starts at its repository origin's
        web URL when available. Reuse the active project's machine-scoped
        Browser home URL, and mutate only the single address-only
        placeholder so existing project tabs are never replaced.
        */
        let default_url = browser_shell_default_url(
            self.latest_sidebar_project_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.browser_home_url.as_deref()),
        );
        let pane_id = self.browser_tabs.focused_pane;
        if self
            .browser_tabs
            .load_pane_active_tab_url(pane_id, default_url.clone())
            .is_none()
        {
            return false;
        }
        self.browser_url = default_url;
        true
    }

    pub(crate) fn switch_workarea_from_hotkey(
        &mut self,
        mode: TitlebarMode,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Hotkeys 2026-06-22-13:00:
        Option+1 through Option+5 must match the native sidebar workarea switchers and use the same titlebar selection route: Agents, Source, Browser, Kanban, and Manage. Browser, Kanban, and Manage remain gated by titlebar_mode_available through set_active_mode, and sleeping project-editor modes keep the titlebar route's no-wake behavior while awake editors refresh through the existing lifecycle, focus, Browser visibility, and persistence sync.

        CDXC:CommandPalette 2026-06-26-07:24:
        Command-palette workarea switchers must reuse this exact hotkey route instead of mutating active_mode directly, so unavailable project-scoped modes remain guarded and all focus/visibility side effects stay identical to keyboard workarea switching.
        */
        if self.set_active_mode(mode, window, cx) {
            cx.notify();
        }
    }
}
