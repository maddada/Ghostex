//! Messages dispatched to the open app modal and Tips panel, app toasts, and quit persistence.

use std::sync::atomic::Ordering;

use gpui::point;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn refresh_open_gpui_app_modal_sidebar_state(
        &mut self,
        sidebar_state_message: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        // The native Settings modal (settings_modal_lifecycle.rs) follows the same hydrate.
        let sidebar_state_message =
            self.with_gpui_command_pane_sidebar_indicators(sidebar_state_message);
        self.refresh_native_settings_modal_sidebar_state(sidebar_state_message, cx);
    }

    pub(crate) fn dispatch_open_gpui_app_modal_sidebar_state_payload(
        &mut self,
        payload: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:AppModal 2026-09-25 WHY:
        Quick Access is a native GPUI window with no modal-host page; its model is gx-core's
        (apps/desktop/src/app/quick_access/host.rs). Its answers (recent projects, saved
        prompts, previous sessions, transcript sizes) go straight to that model.
        */
        if self
            .native_app_modal_kind()
            .and_then(crate::app::window::quick_access::QuickAccessTabId::from_modal_kind)
            .is_some()
        {
            self.quick_access_receive(payload, cx);
            return;
        }
        // The native onboarding reads the same detection, CLI and install answers (onboarding_modal_lifecycle.rs).
        if self.native_app_modal_kind() == Some(GpuiAppModalKind::Onboarding) {
            self.receive_gpui_onboarding_status_payload(payload, cx);
            return;
        }
        // The native Settings modal takes the same status answers (settings_modal_lifecycle.rs).
        self.receive_native_settings_modal_payload(&payload, cx);
    }

    pub(crate) fn dispatch_gpui_titlebar_tips_sidebar_state_payload(
        &mut self,
        payload: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        match payload.get("type").and_then(serde_json::Value::as_str) {
            Some("ghostexCliStatus") => {
                self.titlebar_tips_cli_status = Some(payload.clone());
            }
            Some("agentHookStatus") => {
                self.titlebar_tips_agent_hook_status = Some(payload.clone());
            }
            _ => {}
        }
        if self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::Tips)
            && let Some(handle) = self.titlebar_popup_window
        {
            let payload = payload.clone();
            let _ = handle.update(cx, |popup, window, cx| {
                popup.update_tips_runtime_status(payload, cx);
                window.refresh();
            });
        }
    }

    pub(crate) fn dispatch_open_gpui_app_modal_message(
        &mut self,
        message: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        self.receive_native_app_modal_message(&message, cx);
    }

    pub(crate) fn dispatch_gpui_app_modal_toast(
        &mut self,
        level: &str,
        title: &str,
        description: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        // A native GPUI modal has no toast layer, and the React host's toast
        // dies with its window; after a native modal closes, its outcome goes
        // to the bottom-center app toast window instead of being dropped.
        // The onboarding draws app toasts in its own toast, above its buttons
        // (`GpuiOnboardingWindow::receive_app_toast`).
        if self.native_app_modal_kind() == Some(GpuiAppModalKind::Onboarding) {
            self.show_gpui_onboarding_app_toast(level, title, description, cx);
            return;
        }
        if self.app_modal_window.is_none() {
            self.dispatch_gpui_workspace_action_toast(level, title, description, cx);
            return;
        }
        self.dispatch_open_gpui_app_modal_message(
            serde_json::json!({
                "description": gpui_normalized_app_toast_description(title, Some(description)),
                "level": level,
                "title": title,
                "type": "toast",
            }),
            cx,
        );
    }

    /*
    CDXC:RemoteMachines 2026-08-20:
    Toasts sent to the app-modal host only render while a modal window is open,
    because that host IS the modal window. Sidebar and tab-strip actions run with
    no modal up, so their failures were dropped on the floor and the click looked
    like it did nothing at all. Report those outcomes through the dedicated
    bottom-center app-toast window, which is the same modal-independent surface
    the sidebar bridge and daemon bootstrap already use.
    */
    pub(crate) fn dispatch_gpui_workspace_action_toast(
        &mut self,
        level: &str,
        title: &str,
        description: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        self.app_toast_id_counter = self.app_toast_id_counter.wrapping_add(1);
        let id = format!("gpui-app-toast-{}", self.app_toast_id_counter);
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                id,
                copy_text: None,
                level: GpuiAppToastLevel::from_raw(Some(level)),
                title: title.to_string(),
                description: (!description.is_empty()).then(|| description.to_string()),
                loading: false,
                persistent: false,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                action: None,
                epoch: 0,
            },
            cx,
        );
    }

    /// App toasts from the sidebar bridge (git/worktree/sync/clone progress)
    /// render in a dedicated bottom-center popup window, mirroring the macOS
    /// native toast panels. An in-window layer cannot work here: the workspace
    /// area is covered by native Ghostty/CEF child views that draw above all
    /// GPUI content.
    pub(crate) fn receive_gpui_app_toast_bridge_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        self.app_toast_id_counter = self.app_toast_id_counter.wrapping_add(1);
        let generated_id = format!("gpui-app-toast-{}", self.app_toast_id_counter);
        let Some(toast) = gpui_app_toast_from_bridge_message(message, generated_id) else {
            return;
        };
        self.upsert_gpui_app_toast(toast, cx);
    }

    pub(crate) fn upsert_gpui_app_toast(
        &mut self,
        mut toast: GpuiAppToast,
        cx: &mut gpui::Context<Self>,
    ) {
        toast.description =
            gpui_normalized_app_toast_description(&toast.title, toast.description.as_deref());
        let Some(mut toast) = gpui_app_toast_without_repeats(&self.app_toasts, toast) else {
            return;
        };
        let main_window_bounds = self.main_window_bounds;
        self.app_toast_anchor = Some(point(
            main_window_bounds.origin.x + main_window_bounds.size.width / 2.0,
            main_window_bounds.origin.y + main_window_bounds.size.height,
        ));
        self.app_toast_epoch = self.app_toast_epoch.wrapping_add(1);
        toast.epoch = self.app_toast_epoch;
        let auto_dismiss =
            (!toast.persistent).then(|| (toast.id.clone(), toast.epoch, toast.duration_ms));
        if let Some(existing) = self
            .app_toasts
            .iter_mut()
            .find(|existing| existing.id == toast.id)
        {
            *existing = toast;
        } else {
            self.app_toasts.push(toast);
            while self.app_toasts.len() > GPUI_APP_TOAST_MAX_VISIBLE {
                self.app_toasts.remove(0);
            }
        }
        if let Some((toast_id, epoch, duration_ms)) = auto_dismiss {
            self.schedule_gpui_app_toast_auto_dismiss(toast_id, epoch, duration_ms, cx);
        }
        self.sync_gpui_app_toast_window(cx);
    }

    /// A toast's action button (`GpuiAppToastAction`).
    pub(crate) fn run_gpui_app_toast_action(
        &mut self,
        action: &GpuiAppToastAction,
        cx: &mut gpui::Context<Self>,
    ) {
        match action {
            GpuiAppToastAction::OpenAccounts { provider } => {
                self.open_gpui_settings_accounts_page_for_provider(provider.as_deref(), None, cx);
            }
            GpuiAppToastAction::OpenViewSettings => {
                self.open_gpui_settings_extensions_page(None, cx);
            }
            GpuiAppToastAction::OpenUpdateDownloadPage => {
                #[cfg(target_os = "linux")]
                self.open_linux_update_download_page(cx);
            }
            GpuiAppToastAction::NativeProjectPathAction { payload, .. } => {
                self.receive_sidebar_native_project_path_action_payload(&payload.to_string(), cx);
            }
            GpuiAppToastAction::RunInNewTerminal {
                title,
                command,
                pane_id,
                ..
            } => {
                self.run_toast_command_in_new_terminal(title, command, *pane_id, cx);
            }
        }
    }

    pub(crate) fn show_gpui_gxserver_bootstrap_toast(
        &mut self,
        level: &str,
        title: &str,
        description: &str,
        persistent: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        // Every daemon-bootstrap outcome funnels through this toast, so the
        // sidebar-refresh support log records the same fixed level/title pair
        // (warning/error outcomes persist without the scenario).
        support_logs::append(
            support_logs::GpuiSupportLog::SidebarRefresh,
            if level == "info" {
                "gpui.sidebar.gxserverBootstrapStatus"
            } else {
                "gpui.sidebar.gxserverBootstrapWarning"
            },
            serde_json::json!({ "level": level, "title": title }),
        );
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                copy_text: None,
                id: GPUI_GXSERVER_DAEMON_TOAST_ID.to_string(),
                level: GpuiAppToastLevel::from_raw(Some(level)),
                title: title.to_string(),
                description: (!description.is_empty()).then(|| description.to_string()),
                loading: level == "info" && persistent,
                persistent,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                action: None,
                epoch: 0,
            },
            cx,
        );
    }

    /// Will-terminate persistence flush (macOS `applicationWillTerminate`
    /// parity): persist shell state, restore lid-close sleep by stopping the
    /// Keep Awake runtime, stop the app-owned code-server and remote tunnels,
    /// and deliberately never stop gxserver. CEF owns the durable Browser-profile store and
    /// flushes it during the existing CEF shutdown sequence; GPUI does not
    /// duplicate cookies or site storage in shell state here.
    pub(crate) fn flush_gpui_quit_persistence(&mut self, cx: &mut gpui::Context<Self>) {
        /*
        CDXC:Workarea 2026-07-10:
        App teardown closes the local GPUI/Ghostty renderer that is attached to
        each command zmx session. That renderer exit is not a terminal-session
        exit: command providers and their processes must remain alive so the
        next app process can reattach. Mark the quit boundary before persistence
        or runtime teardown so a final render cannot consume detach as an exit,
        delete the saved tab, and route an explicit gxserver close.
        */
        GPUI_APP_QUIT_IN_PROGRESS.store(true, Ordering::Release);
        support_logs::append(
            support_logs::GpuiSupportLog::HostLifecycle,
            "gpui.host.willTerminate",
            serde_json::json!({ "pid": std::process::id() }),
        );
        self.flush_shell_layout_state();
        self.stop_gpui_keep_awake_runtime();
        self.source_code_server_runtime.stop();
        // The process can exit after this hook without dropping the app, so `Drop for
        // GhostexGpuiApp` must not be the only place the tunnels' ssh children are stopped
        // (tunnel_registry.rs covers a quit that never reaches this hook).
        self.stop_all_gpui_remote_gxserver_connections();
        let _ = cx;
    }
}
