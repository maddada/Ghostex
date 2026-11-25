//! Agent-initiated switch-request toasts (`Q4705`-`Q4711`).
//!
//! CDXC:SwitchRequests 2026-11-23 DECISION:
//! User (Q4705-Q4711): agent-started view switches (`focusSession`, `focusGroup`, `switchProject`,
//! `openBrowser`, `openBrowserPane`, `openPaths`, `openSettings`, and `renameCommand` focus hops)
//! never steal focus. They appear as a clickable inline pill at the bottom-center of the existing
//! toast stack (latest wins, 8s auto-dismiss paused on hover, always shown even in Do Not Disturb
//! or Quiet mode). Browser opens include a second "Open external" button. Hotkeys `Cmd+Shift+J`
//! (accept) and `Cmd+Shift+X` (open external) work while the toast is visible and are rebindable
//! in Settings > Hotkeys.

use ghostex_gx_core::protocol::{RendererCommand, SwitchRequest};
use ghostex_gx_core::{MachineId, ProjectKey, SessionKey};
use gpui::{Context, Window};
use serde_json::{Value, json};

use crate::GhostexGpuiApp;
use crate::app::helpers::{gpui_open_external_url, gpui_remote_machine_name_from_settings};
use crate::app::window::toast::{
    GPUI_SWITCH_REQUEST_TOAST_DURATION_MS, GPUI_SWITCH_REQUEST_TOAST_ID, GpuiAppToast,
    GpuiAppToastAction, GpuiAppToastLevel, GpuiSwitchRequestToast, gpui_switch_request_now_ms,
};

impl GhostexGpuiApp {
    /// Shows (or replaces in place, latest wins) the switch-request toast.
    /// Always shown regardless of Do Not Disturb / Quiet mode (Q4711).
    pub(crate) fn show_gpui_switch_request_toast(
        &mut self,
        machine: MachineId,
        request: SwitchRequest,
        cx: &mut Context<Self>,
    ) {
        let machine_id = machine.remote_id().map(str::to_string);
        let remote_computer_name = machine_id
            .as_deref()
            .and_then(gpui_remote_machine_name_from_settings);
        let description = match (request.subtitle.as_deref(), remote_computer_name.as_deref()) {
            (Some(sub), Some(computer)) if !sub.contains(computer) => {
                Some(format!("{sub} · {computer}"))
            }
            (Some(sub), _) => Some(sub.to_string()),
            (None, Some(computer)) => Some(computer.to_string()),
            (None, None) => None,
        };
        let primary_label = if request.primary_label.trim().is_empty() {
            "Switch".to_string()
        } else {
            request.primary_label.clone()
        };
        let switch_request = GpuiSwitchRequestToast {
            external_url: request.external_url.clone(),
            icon: request.icon.clone(),
            primary_label,
            machine_id,
            request: request.clone(),
            started_at_ms: gpui_switch_request_now_ms(),
            remaining_ms: GPUI_SWITCH_REQUEST_TOAST_DURATION_MS,
            paused: false,
        };
        let title = if request.title.trim().is_empty() {
            "Switch view".to_string()
        } else {
            request.title
        };
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                id: GPUI_SWITCH_REQUEST_TOAST_ID.to_string(),
                level: GpuiAppToastLevel::Info,
                title,
                description,
                copy_text: None,
                action: Some(GpuiAppToastAction::SwitchRequest(Box::new(switch_request))),
                loading: false,
                persistent: false,
                duration_ms: GPUI_SWITCH_REQUEST_TOAST_DURATION_MS,
                epoch: 0,
            },
            cx,
        );
    }

    pub(crate) fn has_gpui_switch_request_toast(&self) -> bool {
        self.app_toasts
            .iter()
            .any(|toast| toast.id == GPUI_SWITCH_REQUEST_TOAST_ID)
    }

    pub(crate) fn pause_gpui_switch_request_toast(&mut self, cx: &mut Context<Self>) {
        let now = gpui_switch_request_now_ms();
        let mut changed = false;
        self.app_toast_epoch = self.app_toast_epoch.wrapping_add(1);
        let next_epoch = self.app_toast_epoch;
        if let Some(toast) = self
            .app_toasts
            .iter_mut()
            .find(|toast| toast.id == GPUI_SWITCH_REQUEST_TOAST_ID)
        {
            if let Some(GpuiAppToastAction::SwitchRequest(sr)) = &mut toast.action {
                if !sr.paused {
                    let elapsed = now.saturating_sub(sr.started_at_ms);
                    sr.remaining_ms = sr.remaining_ms.saturating_sub(elapsed).max(250);
                    sr.paused = true;
                    toast.epoch = next_epoch;
                    changed = true;
                }
            }
        }
        if changed {
            self.sync_gpui_app_toast_window(cx);
        }
    }

    pub(crate) fn resume_gpui_switch_request_toast(&mut self, cx: &mut Context<Self>) {
        let now = gpui_switch_request_now_ms();
        let mut schedule: Option<(String, u64, u64)> = None;
        self.app_toast_epoch = self.app_toast_epoch.wrapping_add(1);
        let next_epoch = self.app_toast_epoch;
        if let Some(toast) = self
            .app_toasts
            .iter_mut()
            .find(|toast| toast.id == GPUI_SWITCH_REQUEST_TOAST_ID)
        {
            if let Some(GpuiAppToastAction::SwitchRequest(sr)) = &mut toast.action {
                if sr.paused {
                    sr.paused = false;
                    sr.started_at_ms = now;
                    toast.epoch = next_epoch;
                    schedule = Some((toast.id.clone(), next_epoch, sr.remaining_ms));
                }
            }
        }
        if let Some((toast_id, epoch, remaining_ms)) = schedule {
            self.schedule_gpui_app_toast_auto_dismiss(toast_id, epoch, remaining_ms, cx);
            self.sync_gpui_app_toast_window(cx);
        }
    }

    /// Accepts the currently visible switch-request toast (primary Switch / Open button or
    /// `acceptSwitchRequestToast` hotkey). Returns `true` if a toast was visible and accepted.
    pub(crate) fn accept_gpui_switch_request_toast(
        &mut self,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(sr) = self.take_gpui_switch_request_toast(cx) else {
            return false;
        };
        if let Some(remote_machine_id) = sr.machine_id.as_deref() {
            self.perform_remote_switch_request(remote_machine_id, &sr.request, window, cx);
            return true;
        }
        let command = RendererCommand {
            action: sr.request.action.clone(),
            command_id: sr.request.id.clone(),
            created_at: sr.request.created_at.clone(),
            payload: sr.request.payload.clone(),
            timeout_ms: 15_000,
        };
        if let Some(window) = window {
            let _ =
                self.gx_store_perform_renderer_command_allowing_rename_hop(&command, window, cx);
            return true;
        }
        if let Some(main_window) = self.main_window_handle {
            let app = cx.entity().downgrade();
            cx.defer(move |cx| {
                let _ = main_window.update(cx, |_, window, cx| {
                    let _ = app.update(cx, |app, cx| {
                        let _ = app.gx_store_perform_renderer_command_allowing_rename_hop(
                            &command, window, cx,
                        );
                    });
                });
            });
            return true;
        }
        if let Some(url) = sr.external_url.as_deref() {
            gpui_open_external_url(url, cx);
        }
        true
    }

    /// The browser toast's Always (#762): turns on "Let agents open and switch pages without
    /// asking" with the same save a Settings change takes, then opens the page as Open does.
    pub(crate) fn always_accept_gpui_switch_request_toast(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut patch = serde_json::Map::new();
        patch.insert(
            ghostex_settings_catalog::help::AGENT_PAGES_OPEN_WITHOUT_ASKING_SETTING.to_string(),
            Value::Bool(true),
        );
        self.handle_gpui_app_modal_update_settings_patch_message(
            &json!({ "patch": patch, "source": "toast:always", "type": "updateSettingsPatch" }),
            cx,
        );
        self.accept_gpui_switch_request_toast(None, cx)
    }

    /// Opens a visible browser switch-request toast's URL in the default external browser instead
    /// of Ghostex's built-in Browser (`Open external` button or `openSwitchRequestExternal`
    /// hotkey). Returns `true` if a browser switch-request toast was visible and handled.
    pub(crate) fn open_gpui_switch_request_external(&mut self, cx: &mut Context<Self>) -> bool {
        let url = self
            .app_toasts
            .iter()
            .find(|toast| toast.id == GPUI_SWITCH_REQUEST_TOAST_ID)
            .and_then(|toast| match &toast.action {
                Some(GpuiAppToastAction::SwitchRequest(sr)) => sr.external_url.clone(),
                _ => None,
            });
        let Some(url) = url else {
            return false;
        };
        self.remove_gpui_app_toast(GPUI_SWITCH_REQUEST_TOAST_ID, cx);
        gpui_open_external_url(&url, cx);
        true
    }

    fn take_gpui_switch_request_toast(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<GpuiSwitchRequestToast> {
        let sr = self
            .app_toasts
            .iter()
            .find(|toast| toast.id == GPUI_SWITCH_REQUEST_TOAST_ID)
            .and_then(|toast| match &toast.action {
                Some(GpuiAppToastAction::SwitchRequest(sr)) => Some((**sr).clone()),
                _ => None,
            })?;
        self.remove_gpui_app_toast(GPUI_SWITCH_REQUEST_TOAST_ID, cx);
        Some(sr)
    }

    fn perform_remote_switch_request(
        &mut self,
        remote_machine_id: &str,
        request: &SwitchRequest,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        match request.action.as_str() {
            "focusSession" => {
                if let (Some(project_id), Some(session_id)) =
                    (request.project_id.as_deref(), request.session_id.as_deref())
                {
                    let key = SessionKey {
                        machine: MachineId::Remote(remote_machine_id.to_string()),
                        project_id: project_id.to_string(),
                        session_id: session_id.to_string(),
                    };
                    let _ = self.dispatch_gpui_status_activation(&key.to_sidebar_session_id(), cx);
                }
            }
            "focusGroup" | "switchProject" => {
                if let Some(project_id) = request.project_id.as_deref() {
                    let key = ProjectKey {
                        machine: MachineId::Remote(remote_machine_id.to_string()),
                        project_id: project_id.to_string(),
                    };
                    let _ = self
                        .dispatch_gpui_menu_bar_project_activation(&key.to_sidebar_group_id(), cx);
                }
            }
            "openBrowser" | "openBrowserPane" => {
                let mut payload = request.payload.clone();
                if let Some(map) = payload.as_object_mut() {
                    map.remove("projectId");
                }
                let command = RendererCommand {
                    action: request.action.clone(),
                    command_id: request.id.clone(),
                    created_at: request.created_at.clone(),
                    payload,
                    timeout_ms: 15_000,
                };
                if let Some(window) = window {
                    let _ = self.gx_store_perform_renderer_command_allowing_rename_hop(
                        &command, window, cx,
                    );
                } else if let Some(main_window) = self.main_window_handle {
                    let app = cx.entity().downgrade();
                    cx.defer(move |cx| {
                        let _ = main_window.update(cx, |_, window, cx| {
                            let _ = app.update(cx, |app, cx| {
                                let _ = app.gx_store_perform_renderer_command_allowing_rename_hop(
                                    &command, window, cx,
                                );
                            });
                        });
                    });
                } else if let Some(url) = request.external_url.as_deref() {
                    gpui_open_external_url(url, cx);
                }
            }
            "openSettings" => {
                let mut message = json!({ "modal": "settings", "type": "open" });
                if let Some(tab) = request.payload.get("tab").and_then(Value::as_str) {
                    message["initialTab"] = json!(tab);
                }
                if let Some(query) = request.payload.get("searchQuery").and_then(Value::as_str) {
                    message["initialSearchQuery"] = json!(query);
                }
                self.open_app_modal_from_bridge(message, cx);
            }
            _ => {
                if let Some(url) = request.external_url.as_deref() {
                    gpui_open_external_url(url, cx);
                }
            }
        }
    }
}
