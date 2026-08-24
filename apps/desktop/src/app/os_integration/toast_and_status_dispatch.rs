// C1 wave-4 deferred split: apps/desktop/src/app/os_integration.rs (~3.6k
// lines) further divided into responsibility-scoped submodules, pure move
// (the only edit from the original app/os_integration.rs body is wrapping
// each group of `impl GhostexGpuiApp` methods in its own impl block;
// multiple impl blocks for the same type across files is the established
// pattern used by every sibling file in apps/desktop/src/app/). This file holds app toast scheduling/removal/window sync and the sidebar/remote/settings status dispatch task runners.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: updater, first-run onboarding, gxserver bootstrap, OS shells, portless, keep-awake

use std::collections::HashSet;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::AppContext as _;
use gpui::Bounds;
use gpui::WindowBackgroundAppearance;
use gpui::WindowBounds;
use gpui::WindowKind;
use gpui::WindowOptions;
use gpui::point;
use gpui::px;
use gpui::size;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn schedule_gpui_app_toast_auto_dismiss(
        &mut self,
        toast_id: String,
        epoch: u64,
        duration_ms: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(duration_ms))
                .await;
            let _ = this.update(cx, |this, cx| {
                // The epoch guard keeps an update-in-place (same toastId) from
                // being dismissed by the superseded toast's timer.
                if this
                    .app_toasts
                    .iter()
                    .any(|toast| toast.id == toast_id && toast.epoch == epoch)
                {
                    this.remove_gpui_app_toast(&toast_id, cx);
                }
            });
        })
        .detach();
    }

    pub(crate) fn remove_gpui_app_toast(&mut self, toast_id: &str, cx: &mut gpui::Context<Self>) {
        let previous_len = self.app_toasts.len();
        self.app_toasts.retain(|toast| toast.id != toast_id);
        if self.app_toasts.len() != previous_len {
            self.sync_gpui_app_toast_window(cx);
        }
    }

    pub(crate) fn sync_gpui_app_toast_window(&mut self, cx: &mut gpui::Context<Self>) {
        if self.app_toasts.is_empty() {
            self.app_toast_window_height = px(0.0);
            if let Some(handle) = self.app_toast_window.take() {
                let _ = handle.update(cx, |_, toast_window, _| {
                    toast_window.remove_window();
                });
            }
            return;
        }
        let Some(anchor) = self.app_toast_anchor else {
            return;
        };
        let stack_height = px(gpui_app_toast_stack_height(&self.app_toasts));
        let toasts = self.app_toasts.clone();
        if let Some(handle) = self.app_toast_window.clone() {
            if self.app_toast_window_height == stack_height {
                let update_result = handle.update(cx, |toast_window_entity, toast_window, cx| {
                    toast_window_entity.set_toasts(toasts.clone(), cx);
                    toast_window.refresh();
                });
                if update_result.is_ok() {
                    return;
                }
                self.app_toast_window = None;
            } else {
                // The popup is exact-sized and bottom-anchored; gpui windows can
                // resize but not move, so a height change recreates the window at
                // the new bottom-anchored bounds.
                let _ = handle.update(cx, |_, toast_window, _| {
                    toast_window.remove_window();
                });
                self.app_toast_window = None;
            }
        }
        let bounds = Bounds {
            origin: point(
                anchor.x - px(GPUI_APP_TOAST_WIDTH / 2.0),
                anchor.y - stack_height - px(GPUI_APP_TOAST_BOTTOM_MARGIN),
            ),
            size: size(px(GPUI_APP_TOAST_WINDOW_WIDTH), stack_height),
        };
        let app = cx.entity().downgrade();
        let main_window_native_view = self.parent_ns_view;
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            focus: false,
            show: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            titlebar: None,
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        };
        self.app_toast_window = cx
            .open_window(options, move |toast_window, cx| {
                remove_gpui_app_toast_popup_window_chrome(toast_window, main_window_native_view);
                cx.new(|_| GpuiAppToastWindow {
                    app,
                    toasts,
                    hovered_toast_id: None,
                })
            })
            .ok();
        self.app_toast_window_height = stack_height;
    }

    pub(crate) fn dispatch_gpui_sidebar_remote_event(
        &mut self,
        message: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUIRemoteMachines 2026-06-24-16:48:
        GPUI remote-machine status and presentation events are Rust-owned sidebar-only messages. Send only sanitized state enums, machine ids, and gxserver presentation snapshots/deltas into SidebarApp; remote auth tokens, SSH host/user/key data, paths from Settings commands, URLs, daemon response bodies, stdout/stderr, and command text must stay out of CEF globals and logs.

        CDXC:GPUIRemoteSessions 2026-06-24-17:19:
        Response-capable remote session requests may dispatch only request ids, success state, generic errors, and explicit safe metadata results such as previous-session search rows. Mutating remote session responses must be sanitized before this event boundary so renderer code never receives launch commands, tokens, SSH details, raw daemon bodies, or provider internals.
        */
        let Some(sidebar) = self.sidebar.clone() else {
            return;
        };
        let script = format!(
            "window.dispatchEvent(new CustomEvent('{GPUI_SIDEBAR_REMOTE_EVENT_NAME}', {{ detail: {} }})); undefined;",
            message
        );
        sidebar.update(cx, |surface, _| {
            surface.execute_app_owned_script(&script);
        });
    }

    pub(crate) fn dispatch_gpui_remote_machine_status(
        &mut self,
        remote_machine_id: &str,
        state: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_gpui_remote_machine_status_with_message(remote_machine_id, state, None, cx);
    }

    /*
    CDXC:GPUIRemoteConnectFeedback 2026-07-12:
    Failure states may carry the already-sanitized connect failure summary
    (the same text shown in the toast) so the sidebar can explain why the
    connect failed inline. Only Rust-authored sanitized messages may pass
    through here — never raw SSH stderr, tokens, hosts, or daemon bodies.
    */
    pub(crate) fn dispatch_gpui_remote_machine_status_with_message(
        &mut self,
        remote_machine_id: &str,
        state: &str,
        message: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        debug_assert!(gpui_remote_gxserver_status_state_is_known(state));
        if gpui_remote_gxserver_status_state_is_broken(state) {
            self.stop_gpui_remote_gxserver_connection(remote_machine_id);
        }
        /*
        CDXC:GPUIRemoteConnectOverlay 2026-08-07:
        Every connect transition funnels through here, so this is where the
        render tree learns a machine's reachability. The terminal body draws a
        status overlay for remote sessions whose machine is not connected, and
        the workspace re-arms restored remote tabs on the connected edge. Only
        the bounded wire state slug is retained — never hosts, tokens, or the
        sanitized message text.
        */
        self.remote_machine_connect_states
            .insert(remote_machine_id.to_string(), state.to_string());
        if state == GpuiRemoteGxserverConnectState::Connected.wire_status_state() {
            self.attach_surfaced_remote_workspace_terminals(remote_machine_id, cx);
        }
        let mut payload = serde_json::json!({
            "machineId": remote_machine_id,
            "state": state,
            "type": "remoteMachineStatus",
        });
        if let Some(message) = message.map(str::trim).filter(|message| !message.is_empty()) {
            payload["message"] = serde_json::Value::String(message.to_string());
        }
        self.dispatch_gpui_sidebar_remote_event(payload, cx);
    }

    pub(crate) fn dispatch_gpui_settings_action_status(
        &mut self,
        action: &str,
        available: bool,
        message: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_open_gpui_app_modal_sidebar_state_payload(
            gpui_settings_action_status_message(action, available, message),
            cx,
        );
    }

    pub(crate) fn run_gpui_app_modal_sidebar_status_task<F>(
        &mut self,
        task: F,
        cx: &mut gpui::Context<Self>,
    ) where
        F: FnOnce() -> serde_json::Value + Send + 'static,
    {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let payload = background.spawn(async move { task() }).await;
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_sidebar_state_payload(payload, cx);
            });
        })
        .detach();
    }

    pub(crate) fn run_gpui_app_modal_and_titlebar_status_task<F>(
        &mut self,
        task: F,
        cx: &mut gpui::Context<Self>,
    ) where
        F: FnOnce() -> serde_json::Value + Send + 'static,
    {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let payload = background.spawn(async move { task() }).await;
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_sidebar_state_payload(payload.clone(), cx);
                this.dispatch_gpui_titlebar_tips_sidebar_state_payload(&payload, cx);
            });
        })
        .detach();
    }

    pub(crate) fn run_gpui_progressive_agent_hook_status_task(
        &mut self,
        requested_agent_ids: Option<Vec<String>>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        macOS parity: hook-status probes run per provider in priority order
        (codex, claude, opencode, pi first) and every partial result posts
        immediately, so Settings/Tips hook warnings populate progressively
        instead of waiting behind the slowest provider probe. Install and
        uninstall stay single batched calls like macOS. One status request
        runs at a time; overlapping requests drop like the macOS in-flight
        guard, and a failed provider probe posts the error payload and stops
        the walk like the macOS catch path.
        */
        if self.agent_hook_status_request_in_flight {
            return;
        }
        self.agent_hook_status_request_in_flight = true;
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let mut merged: Option<serde_json::Value> = None;
            for agent_id in gpui_ordered_agent_hook_status_agent_ids(requested_agent_ids) {
                let payload = background
                    .spawn(async move {
                        gpui_agent_hook_status_message(
                            "/api/readAgentHookStatus",
                            Some(HashSet::from([agent_id])),
                            "Unable to inspect agent hook status.",
                        )
                    })
                    .await;
                let is_error = payload
                    .get("errorMessage")
                    .and_then(serde_json::Value::as_str)
                    .is_some();
                let next = if is_error {
                    payload
                } else {
                    match merged.as_ref() {
                        Some(previous) => gpui_merge_agent_hook_status_messages(previous, &payload),
                        None => payload,
                    }
                };
                let dispatched = this.update(cx, |this, cx| {
                    this.dispatch_open_gpui_app_modal_sidebar_state_payload(next.clone(), cx);
                    this.dispatch_gpui_titlebar_tips_sidebar_state_payload(&next, cx);
                });
                if dispatched.is_err() {
                    return;
                }
                if is_error {
                    break;
                }
                merged = Some(next);
            }
            let _ = this.update(cx, |this, _| {
                this.agent_hook_status_request_in_flight = false;
            });
        })
        .detach();
    }

    pub(crate) fn run_gpui_ghostex_cli_settings_action(
        &mut self,
        action: GpuiGhostexCliSettingsAction,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUISettingsCliInstall 2026-06-24-12:56:
        Settings integration actions in GPUI must mutate only bounded local install targets and then refresh the shared `ghostexCliStatus` contract. CLI repair uses packaged app resources only; skill install/uninstall actions use fixed Ghostex-owned command or directory targets and never accept React-provided shell text, paths, stdout, stderr, URLs, tokens, or environment data.
        */
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            // The status message probe runs `cua-driver check_permissions`
            // (5s timeout plus an unbounded pipe-reader join) — it must run
            // in the same background task as the action, never inside the
            // main-thread completion update (CDXC:GPUISettingsCliInstall
            // 2026-07-11: previously stalled the UI up to 5s per action).
            let (result, status_message) = background
                .spawn(async move {
                    let result = gpui_run_ghostex_cli_settings_action(action);
                    let status_message =
                        gpui_ghostex_cli_status_message(Some(result.message.as_str()));
                    (result, status_message)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_sidebar_state_payload(status_message, cx);
                this.dispatch_gpui_settings_action_status(
                    result.action_id,
                    result.available,
                    result.message.as_str(),
                    cx,
                );
                this.dispatch_gpui_app_modal_toast(
                    result.toast_level,
                    result.toast_title,
                    result.message.as_str(),
                    cx,
                );
            });
        })
        .detach();
    }
}
