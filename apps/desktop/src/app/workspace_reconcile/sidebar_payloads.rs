//! Sidebar payloads the app receives: global actions, status indicators and attention notifications, remote attach focus, project path and command actions.

use std::time::Instant;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::ClipboardItem;
use gpui::Window;

use crate::app::helpers::*;
use crate::app::model::*;
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
        let mut attention_notifications = if self.sidebar_session_status_indicators_snapshot_seen {
            gpui_session_attention_notification_candidates(
                &self.sidebar_session_status_indicators,
                &next_state,
            )
        } else {
            Vec::new()
        };
        // Alert me for: Needs you only drops the banners of attention that does not wait on the
        // user (gx_store/attention/host.rs); the not-moving banner is not filtered.
        attention_notifications.retain(|candidate| {
            crate::app::gx_store::system_notification_session(&candidate.session_id)
                .is_none_or(|session| self.gx_store_attention_alerts(&session))
        });
        self.sidebar_session_status_indicators_snapshot_seen = true;
        self.sidebar_session_status_indicators = next_state;
        self.apply_gpui_menu_bar_status_item_state();
        self.deliver_gpui_session_attention_notifications(attention_notifications, cx);
        self.sync_ghostex_capture(cx);
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
        // One banner per session, from the lead window only (app/workspace_windows/).
        if candidates.is_empty()
            || !self.is_lead_window()
            || !gpui_macos_attention_notifications_enabled()
        {
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

    /// A click on a system notification (a macOS banner, a Windows toast or a Linux notification): a
    /// banked reset warning opens its account (notification_feed/reset_banners.rs), anything else
    /// shows its session on its computer in the window already showing it (app/workspace_windows/).
    pub(crate) fn activate_system_notification(
        &mut self,
        session_id: String,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.activate(true);
        if let Some(page) = session_id
            .strip_prefix(crate::app::browser_page_notifications::BROWSER_PAGE_NOTIFICATION_PREFIX)
        {
            self.open_browser_page_from_notification(page, window, cx);
            return;
        }
        if let Some(account_id) = session_id
            .strip_prefix(crate::notification_feed::reset_banners::ACCOUNT_RESET_BANNER_PREFIX)
        {
            self.open_account_notification(account_id, cx);
            return;
        }
        // The same switch, focus and reveal a click in the notification feed makes, so the
        // sidebar follows a remote session's banner to its computer (issue #401).
        let Some(session) = crate::app::gx_store::system_notification_session(&session_id) else {
            return;
        };
        self.activate_session_in_its_window(&session_id, window, cx, move |app, cx| {
            app.gx_store_show_session_on_its_machine(
                &session,
                crate::app::gx_store::RowFocusOptions::default(),
                cx,
            );
        });
    }

    pub(crate) fn deliver_gpui_macos_session_attention_notification(
        &mut self,
        candidate: GpuiSessionAttentionNotificationCandidate,
        cx: &mut gpui::Context<Self>,
    ) {
        // Linux banners go through GPUI so their click comes back (gpui_show_linux_session_attention_notification).
        #[cfg(target_os = "linux")]
        gpui_show_linux_session_attention_notification(candidate, cx);
        #[cfg(not(target_os = "linux"))]
        {
            let background = cx.background_executor().clone();
            cx.spawn(async move |_this, _cx| {
                let _ = background
                    .spawn(
                        async move { gpui_deliver_macos_session_attention_notification(candidate) },
                    )
                    .await;
            })
            .detach();
        }
    }

    pub(crate) fn apply_gpui_menu_bar_status_item_state(&self) {
        /*
        CDXC:StatusPet 2026-06-26-05:42:
        GPUI applies the macOS menu-bar badge directly from sanitized Rust-owned counts and the saved hideMenuBarSessionStatusIndicators setting.

        CDXC:StatusPet 2026-06-26-06:05:
        The primary-click dropdown shares this Rust-owned status snapshot for bounded project/session rows. AppKit receives explicit copied FFI fields for ids, titles, status, order, and timestamps only; it never receives renderer JSON, paths, URLs, command text, tokens, logs, terminal output, hidden hit regions, or overlay instructions.
        */
        // The status item is the process's one; the lead window feeds it (app/workspace_windows/).
        if self.is_lead_window() {
            apply_gpui_menu_bar_status_item(&self.sidebar_session_status_indicators);
        }
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

    /// Performs a native project-path action: the page's bridge message and the store's own remote
    /// opens (gx_store/sidebar_remote_focus.rs) both end here.
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
                    gpui_copy_to_clipboard(ClipboardItem::new_string(path), cx);
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
}
