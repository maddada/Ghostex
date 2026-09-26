// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: app modal / titlebar panel windows and shared-settings save fan-out

use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::SystemTime;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::Window;
use gpui::WindowBounds;
use gpui::WindowOptions;
use gpui::point;
use gpui_component::WindowExt;
use gpui_component::notification::Notification;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::hotkeys::*;
use crate::app::model::*;
use crate::app::window::*;
use crate::*;
impl GhostexGpuiApp {
    pub(crate) fn open_gpui_app_modal_from_titlebar(
        &mut self,
        modal: GpuiAppModalKind,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:AppModal 2026-06-24-10:42:
        GPUI titlebar Settings, Hotkeys, and Command Palette actions must open the shared React app-modal host in a real GPUI CEF window. The route reuses the macOS modal ids and latest Settings-compatible sidebar hydrate, with no duplicated React modal UI, WebKit/WKWebView, transparent overlay, hidden hit-test region, synthetic mouse routing, or generic fallback surface.

        CDXC:Sessions 2026-06-24-11:53:
        Previous Sessions joins the same GPUI-owned CEF app-modal window path as Settings, Hotkeys, and Command Palette. The titlebar route must open the shared React modal component directly and let its gxserver-backed query resolve through sidebarCommand responses, not duplicated GPUI UI, overlays, hit-test routing, or stored hydrate rewrites.

        CDXC:Settings 2026-06-24-12:22:
        Configure Agents, Configure Actions, and Open Targets are Settings-modal entry points in the shared React host. GPUI must preserve their modal ids, attach the latest Settings-compatible sidebar hydrate, and reuse this production CEF app-modal route instead of adding duplicate React UI, stubs, fallback routing, overlays, or hidden hit regions.

        CDXC:AgentLauncher 2026-06-24-12:26:
        Agents Hub opens through this same CEF app-modal host so command-palette bridge requests and titlebar menu actions present the existing shared React modal. Its catalog/content are separate sidebarCommand responses and should not be bundled into the open message.
        */
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = modal.open_message();
        if modal.requires_sidebar_state() {
            open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        }

        self.open_gpui_app_modal_window(
            modal,
            open_message,
            sidebar_state_message,
            Some(window),
            cx,
        );
    }

    pub(crate) fn open_gpui_extension_modal(
        &mut self,
        id: ExtensionId,
        source_window: Option<&mut Window>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(extension) =
            self.extensions_snapshot
                .installed
                .get(id.as_str())
                .filter(|extension| {
                    extension.enabled && extension.placement == Some(GpuiExtensionPlacement::Modal)
                })
        else {
            return false;
        };
        if extension.runtime_url.is_some() {
            self.open_gpui_extension_modal_window(id, source_window, cx);
            return true;
        }

        let params = serde_json::json!({
            "id": id.as_str(),
            "context": self.extension_launch_context_value(),
        });
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_gxserver_rpc_result(
                        "/api/startExtension",
                        &params,
                        Duration::from_secs(65),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let ready_url = result
                    .as_ref()
                    .ok()
                    .and_then(|result| result.get("status"))
                    .and_then(serde_json::Value::as_object)
                    .filter(|status| {
                        status.get("state").and_then(serde_json::Value::as_str) == Some("ready")
                    })
                    .and_then(|status| status.get("url"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let error = match (&result, &ready_url) {
                    (_, Some(_)) => None,
                    (Ok(result), None) => Some(
                        result
                            .get("status")
                            .and_then(serde_json::Value::as_object)
                            .and_then(|status| status.get("error"))
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("The extension did not reach its ready state.")
                            .to_string(),
                    ),
                    (Err(error), None) => Some(error.clone()),
                };
                let Some(ready_url) = ready_url else {
                    this.dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Extensions",
                        error
                            .as_deref()
                            .unwrap_or("The extension could not be opened."),
                        cx,
                    );
                    return;
                };
                let Some(extension) = this.extensions_snapshot.installed.get_mut(id.as_str())
                else {
                    return;
                };
                extension.runtime_url = Some(ready_url);
                this.open_gpui_extension_modal_window(id, None, cx);
                this.refresh_extensions_in_background(cx);
            });
        })
        .detach();
        true
    }

    fn open_gpui_extension_modal_window(
        &mut self,
        id: ExtensionId,
        source_window: Option<&mut Window>,
        cx: &mut gpui::Context<Self>,
    ) {
        let modal = GpuiAppModalKind::Extension(id);
        self.open_gpui_app_modal_window(
            modal,
            modal.open_message(),
            serde_json::Value::Null,
            source_window,
            cx,
        );
    }

    fn extension_modal_runtime(
        &self,
        id: ExtensionId,
    ) -> Option<(String, cef::ExtensionBridgeSurfaceSpec)> {
        let extension =
            self.extensions_snapshot
                .installed
                .get(id.as_str())
                .filter(|extension| {
                    extension.enabled && extension.placement == Some(GpuiExtensionPlacement::Modal)
                })?;
        let url = extension.runtime_url.clone()?;
        let bridge_surface = extension.bridge_surface_spec_for_url(&url)?;
        Some((url, bridge_surface))
    }

    fn extension_modal_bridge_event_handler(
        &self,
        open_attempt_id: u64,
        id: ExtensionId,
        cx: &mut gpui::Context<Self>,
    ) -> cef::ExtensionBridgeEventHandler {
        let app = cx.entity().downgrade();
        let async_cx = cx.to_async();
        let foreground = cx.foreground_executor().clone();
        Rc::new(move |event: cef::ExtensionBridgeEvent| {
            let app = app.clone();
            let mut async_cx = async_cx.clone();
            let foreground = foreground.clone();
            foreground
                .clone()
                .spawn(async move {
                    let _ = app.update_in(&mut async_cx, |this, _window, cx| {
                        let response_app = cx.entity().downgrade();
                        let response_async_cx = cx.to_async();
                        let response_foreground = cx.foreground_executor().clone();
                        let responder: GpuiExtensionBridgeResponder = Rc::new(move |payload| {
                            let response_app = response_app.clone();
                            let mut response_async_cx = response_async_cx.clone();
                            response_foreground
                                .spawn(async move {
                                    let _ = response_app.update_in(
                                        &mut response_async_cx,
                                        |this, _window, cx| {
                                            if this.app_modal_open_attempt_id != open_attempt_id {
                                                return;
                                            }
                                            let Some(handle) = this.app_modal_window.clone() else {
                                                return;
                                            };
                                            let _ = handle.update(cx, |host, _modal_window, cx| {
                                                if host.current_modal
                                                    == GpuiAppModalKind::Extension(id)
                                                {
                                                    host.dispatch_extension_bridge_message(
                                                        &payload, cx,
                                                    );
                                                }
                                            });
                                        },
                                    );
                                })
                                .detach();
                        });
                        let close_app = cx.entity().downgrade();
                        let close_async_cx = cx.to_async();
                        let close_foreground = cx.foreground_executor().clone();
                        let close_handler: GpuiExtensionCloseHandler = Rc::new(move || {
                            let close_app = close_app.clone();
                            let mut close_async_cx = close_async_cx.clone();
                            close_foreground
                                .spawn(async move {
                                    let _ = close_app.update_in(
                                        &mut close_async_cx,
                                        |this, _window, cx| {
                                            if this.app_modal_open_attempt_id != open_attempt_id {
                                                return;
                                            }
                                            let is_current_modal = this
                                                .app_modal_window
                                                .clone()
                                                .and_then(|handle| {
                                                    handle
                                                        .update(cx, |host, _modal_window, _cx| {
                                                            host.current_modal
                                                                == GpuiAppModalKind::Extension(id)
                                                        })
                                                        .ok()
                                                })
                                                .unwrap_or(false);
                                            if is_current_modal {
                                                this.close_gpui_app_modal_window_and_restore_command_focus(cx);
                                            }
                                        },
                                    );
                                })
                                .detach();
                        });
                        this.handle_extension_bridge_event(
                            event,
                            this.extension_surface_context(GpuiExtensionPlacement::Modal),
                            responder,
                            Some(close_handler),
                            cx,
                        );
                    });
                })
                .detach();
        })
    }

    fn gpui_app_modal_window_title(&self, modal: GpuiAppModalKind) -> String {
        match modal {
            GpuiAppModalKind::Extension(id) => self
                .extensions_snapshot
                .installed
                .get(id.as_str())
                .map(|extension| extension.title.clone())
                .unwrap_or_else(|| modal.window_title().to_string()),
            _ => modal.window_title().to_string(),
        }
    }

    pub(crate) fn gpui_command_delayed_send_open_message(
        &self,
        session_id: CommandSessionId,
        title: &str,
    ) -> serde_json::Value {
        let mut message = serde_json::json!({
            "closeAfterDoneActive": self
                .command_pane
                .session(session_id)
                .is_some_and(|session| session.close_after_done_armed),
            "modal": GpuiAppModalKind::DelayedSend.modal_id(),
            "sessionId": gpui_command_session_external_id(session_id),
            "title": title,
            "type": "open",
        });
        if let Some(timer) = self.command_delayed_send_timers.get(&session_id).copied() {
            let remaining_ms = timer.remaining_ms(SystemTime::now());
            message["delayedSendDeadlineAt"] =
                serde_json::json!(gpui_iso8601_utc(timer.deadline_at));
            message["delayedSendRemainingLabel"] =
                serde_json::json!(gpui_command_delayed_send_countdown_label(remaining_ms));
        }
        message
    }

    pub(crate) fn gpui_command_delayed_send_remaining_label_for_session(
        &self,
        session_id: CommandSessionId,
    ) -> Option<String> {
        self.command_delayed_send_timers
            .get(&session_id)
            .copied()
            .and_then(|timer| {
                gpui_command_delayed_send_body_badge_label(Some(timer), SystemTime::now())
            })
    }

    pub(crate) fn command_pane_tab_exists(
        &self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
    ) -> bool {
        self.command_pane
            .find_leaf(group_id)
            .is_some_and(|leaf| leaf.tab_group.has_session(session_id))
            && self.command_pane.session(session_id).is_some()
    }

    pub(crate) fn open_gpui_delayed_send_modal_for_command_pane_tab(
        &mut self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:ContextMenus 2026-06-25-17:37:
        Delayed Send from a clicked command tab is session-scoped like native, but GPUI can only execute the later Return through a visible mounted command body. Selecting, expanding, and waking the clicked tab before opening the modal is the normal-layout equivalent of targeting that native command session.
        */
        if !self.command_pane_tab_exists(group_id, session_id) {
            return false;
        }
        let expand_pane = self.command_pane.dock_for_group(group_id)
            == Some(CommandPaneDock::Panel)
            && !self.command_pane.is_expanded();
        if !self.select_command_pane_tab(group_id, session_id, expand_pane, window, cx) {
            return false;
        }
        self.reveal_command_group_dock(group_id, cx);
        if self
            .command_pane
            .session(session_id)
            .is_some_and(|session| session.is_sleeping)
        {
            self.wake_command_pane_session(group_id, session_id, cx);
        }
        let Some(title) = self
            .command_pane
            .session(session_id)
            .map(|session| session.title.clone())
        else {
            return false;
        };
        let modal = GpuiAppModalKind::DelayedSend;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let open_message = self.gpui_command_delayed_send_open_message(session_id, &title);
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, None, cx);
        true
    }

    pub(crate) fn focus_command_pane_tab_for_context_session_action(
        &mut self,
        action: CommandPaneTabSessionAction,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:ContextMenus 2026-06-25-18:33:
        Legacy clicked-tab action handlers focus the clicked terminal before Rename and Close After Done dispatch. Select and focus the clicked GPUI command tab without expanding a collapsed strip so the action target becomes the command-pane focus while left-click remains the only hidden-open gesture.

        CDXC:Notifications 2026-06-25-19:58:
        Primary clicked-tab context actions use that same focus path, so they acknowledge only the clicked Attention command session before opening Rename or toggling Close After Done.

        CDXC:ContextMenus 2026-06-27-01:55:
        Command-tab right-click no longer exposes these retained handlers because native command-panel payloads are panel-only. Keep the helper for non-menu command-tab dispatch paths without reintroducing visible primary menu rows.
        */
        if command_pane_tab_context_session_action_focus_policy(action)
            != CommandPaneTabContextFocusPolicy::SelectAndFocus
        {
            return false;
        }
        if !self.command_pane_tab_exists(group_id, session_id)
            || !self
                .command_pane
                .select_session_in_group(group_id, session_id)
        {
            return false;
        }

        self.command_pane
            .acknowledge_attention_for_session_activation(session_id);
        self.focus_command_pane(cx);
        self.scroll_command_group_active_tab(group_id);
        self.scroll_focused_command_active_tab();
        self.persist_shell_layout_state();
        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        cx.notify();
        true
    }

    /// Cmd+R with a focused Agents-view or companion-pane terminal renames
    /// the focused mapped gxserver session through the shared Rename Session
    /// modal, matching macOS `promptRenameFocusedNativeHotkeySession`.
    /// Unmapped local placeholder tabs have no gxserver identity to rename
    /// and no-op.
    pub(crate) fn open_gpui_rename_session_modal_for_focused_agents_session(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(shell_session_id) = self.focused_agents_or_companion_shell_session_id() else {
            return false;
        };
        self.open_gpui_rename_session_modal_for_agents_tab(shell_session_id, cx)
    }

    pub(crate) fn open_gpui_rename_session_modal_for_agents_tab(
        &mut self,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(key) = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, mapped)| (*mapped == shell_session_id).then(|| key.clone()))
        else {
            return false;
        };
        let Some(title) = self
            .agents_workspace
            .session(shell_session_id)
            .map(|session| session.title.clone())
        else {
            return false;
        };
        let modal = GpuiAppModalKind::RenameSession;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = serde_json::json!({
            "initialTitle": title,
            "modal": modal.modal_id(),
            "sessionId": gpui_combined_presentation_session_id(
                &key.project_id,
                &key.session_id,
            ),
            "type": "open",
        });
        if modal.requires_sidebar_state() {
            open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        }
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, None, cx);
        true
    }

    pub(crate) fn open_gpui_rename_session_modal_for_focused_command_pane(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some((_group_id, session_id)) =
            focused_command_pane_rename_target(self.shell_focus, &self.command_pane)
        else {
            return false;
        };
        let Some(title) = self
            .command_pane
            .session(session_id)
            .map(|session| session.title.clone())
        else {
            return false;
        };
        let modal = GpuiAppModalKind::RenameSession;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = serde_json::json!({
            "initialTitle": title,
            "modal": modal.modal_id(),
            "sessionId": gpui_command_session_external_id(session_id),
            "type": "open",
        });
        if modal.requires_sidebar_state() {
            open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        }
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, None, cx);
        true
    }

    pub(crate) fn open_gpui_delayed_send_modal_for_focused_command_pane(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some((_group_id, session_id)) =
            focused_command_pane_rename_target(self.shell_focus, &self.command_pane)
        else {
            return false;
        };
        let Some(title) = self
            .command_pane
            .session(session_id)
            .map(|session| session.title.clone())
        else {
            return false;
        };
        let modal = GpuiAppModalKind::DelayedSend;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let open_message = self.gpui_command_delayed_send_open_message(session_id, &title);
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, None, cx);
        true
    }

    pub(crate) fn open_gpui_rename_session_modal_for_command_pane_tab(
        &mut self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.focus_command_pane_tab_for_context_session_action(
            CommandPaneTabSessionAction::Rename,
            group_id,
            session_id,
            cx,
        ) {
            return false;
        }
        let Some(title) = self
            .command_pane
            .session(session_id)
            .map(|session| session.title.clone())
        else {
            return false;
        };
        let modal = GpuiAppModalKind::RenameSession;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = serde_json::json!({
            "initialTitle": title,
            "modal": modal.modal_id(),
            "sessionId": gpui_command_session_external_id(session_id),
            "type": "open",
        });
        if modal.requires_sidebar_state() {
            open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        }
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, None, cx);
        true
    }

    pub(crate) fn request_gpui_titlebar_tips_runtime_status(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        self.run_gpui_app_modal_and_titlebar_status_task(
            || gpui_ghostex_cli_status_message(None),
            cx,
        );
        self.run_gpui_progressive_agent_hook_status_task(None, cx);
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let sidebar_agent_ids = background
                .spawn(async move {
                    gpui_sidebar_hud_from_gxserver(Duration::from_secs(2), None)
                        .map(|hud| gpui_sidebar_default_agent_ids_from_hud_agents(&hud.agents))
                        .ok()
                })
                .await;
            let Some(sidebar_agent_ids) = sidebar_agent_ids else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.titlebar_tips_sidebar_agent_ids = Some(sidebar_agent_ids.clone());
                if this.titlebar_popup_menu_open(GpuiTitlebarPopupKind::Tips)
                    && let Some(handle) = this.titlebar_popup_window.clone()
                {
                    let _ = handle.update(cx, |popup, window, cx| {
                        popup.update_tips_sidebar_agent_ids(sidebar_agent_ids, cx);
                        window.refresh();
                    });
                }
            });
        })
        .detach();
    }

    pub(crate) fn open_gpui_app_modal_window(
        &mut self,
        modal: GpuiAppModalKind,
        mut open_message: serde_json::Value,
        sidebar_state_message: serde_json::Value,
        source_window: Option<&mut Window>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Telemetry 2026-08-26:
        This launcher is the single entry point for every app-modal open route
        (titlebar actions, hotkeys, command palette, sidebar bridge messages), so
        Find, the Extensions store, and Settings report `surface.opened` from
        here. The `_inner` retry path is deliberately not hooked: a retry is the
        same open. Modals outside the spec enum — including per-extension
        modals — send nothing.
        */
        if let Some(surface) = gpui_telemetry_surface_for_app_modal(modal) {
            record_gpui_surface_opened_telemetry(surface, cx.background_executor());
        }
        if modal == GpuiAppModalKind::StashedPrompts {
            self.enrich_gpui_saved_prompts_quick_access_open_message(&mut open_message);
        }
        // Kinds rebuilt in native GPUI leave the React host here (native_app_modal_lifecycle.rs).
        if self.try_open_native_app_modal(modal, &open_message, cx) {
            return;
        }
        // The launcher owns modal hydration. Session-scoped callers such as
        // Rename and Delayed Send must not diverge based on their entry point.
        if modal.requires_sidebar_state() {
            open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        }
        self.open_gpui_app_modal_window_inner(
            modal,
            open_message,
            sidebar_state_message,
            source_window,
            true,
            cx,
        );
    }

    /*
    CDXC:SavedPrompts 2026-08-08:
    Saved Prompts is reachable from the global Quick Access tabs, whose React
    message intentionally carries no project/session authority. Recover only
    the currently focused LOCAL Agents mapping already owned by Rust so a row
    keeps the old direct-insert behavior; remote/unmapped focus stays browse +
    clipboard-only, matching the tab's "Local only for now" notice.
    */
    pub(crate) fn enrich_gpui_saved_prompts_quick_access_open_message(
        &self,
        open_message: &mut serde_json::Value,
    ) {
        if open_message
            .get("sessionId")
            .and_then(serde_json::Value::as_str)
            .is_some()
        {
            return;
        }
        let Some(shell_session_id) = self.focused_agents_or_companion_shell_session_id() else {
            return;
        };
        let Some(key) = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, mapped)| (*mapped == shell_session_id).then(|| key.clone()))
        else {
            return;
        };
        open_message["projectId"] = serde_json::Value::String(key.project_id.clone());
        open_message["sessionId"] = serde_json::Value::String(
            gpui_combined_presentation_session_id(&key.project_id, &key.session_id),
        );
    }

    pub(crate) fn open_gpui_app_modal_window_inner(
        &mut self,
        modal: GpuiAppModalKind,
        open_message: serde_json::Value,
        sidebar_state_message: serde_json::Value,
        source_window: Option<&mut Window>,
        reset_ready_retry: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Settings 2026-06-24-10:58:
        Settings, Hotkeys, and Command Palette app-modal requests share this one GPUI-owned CEF window launcher so titlebar clicks and sidebar bridge messages cannot diverge into duplicate UI, temporary stubs, hidden overlays, or broad hit-test routing.

        CDXC:Sessions 2026-06-24-11:53:
        Previous Sessions app-modal requests share this launcher so titlebar/menu actions and modal-host open messages use one CEF window owner, while gxserver result messages remain transient sidebarState events owned by the command bridge.

        CDXC:Settings 2026-06-24-12:22:
        Settings sub-entry modal ids must share this launcher so bridge opens, command-palette commands, and titlebar actions all hydrate the same shared Settings modal while letting the React host choose the initial tab from the modal id.

        CDXC:AgentLauncher 2026-06-24-12:26:
        Agents Hub shares the launcher and receives the normal sidebar hydrate for settings-backed UI labels, but its filesystem catalog is not stored in that hydrate. The shared React Hub requests a fresh metadata catalog after open and then selected file content on demand through sidebarCommand.

        CDXC:FocusRouting 2026-06-25-22:13:
        Command-pane app modals need the same dismissal focus contract as native child windows. Capture only a runtime command group/session return target at modal open, then restore that exact command tab on close if it still exists; do not persist modal payloads, titles, command text, paths, URLs, stdout/stderr, or fallback to another command group.

        CDXC:Diagnostics 2026-06-28-17:06:
        GPUI app-modal open/retry behavior stays functional, but runtime log writers and diagnostic breadcrumbs are intentionally removed until a future requirement adds a narrower diagnostics surface.
        */
        if !cef::context_initialized() {
            self.defer_gpui_app_modal_open_for_cef(
                modal,
                open_message,
                sidebar_state_message,
                reset_ready_retry,
                cx,
            );
            return;
        }
        if reset_ready_retry {
            self.app_modal_ready_retry_used = false;
        }
        // A native GPUI modal counts as the one open app modal.
        self.remove_native_app_modal_window(cx);
        support_logs::append(
            support_logs::GpuiSupportLog::AppModal,
            "gpui.appModal.lifecycle",
            serde_json::json!({ "action": "open", "modal": modal.modal_id() }),
        );
        let window_size = modal.window_size_for_open(&open_message);
        let window_title = self.gpui_app_modal_window_title(modal);
        let return_focus_target = gpui_app_modal_command_return_focus_target(
            modal,
            &open_message,
            self.shell_focus,
            &self.command_pane,
        );
        /*
        CDXC:Onboarding 2026-09-11 WHY:
        A React-initiated switch out of first-launch setup (the onboarding's
        "Advanced settings later" and queued Remote-settings opens post a plain
        `open` for Settings) replaces or re-targets the one reusable modal
        window here, so the later `completeFirstLaunchSetup` message finds
        Settings as the live modal and is ignored. Count the switch itself as
        finishing setup, otherwise onboarding reappears on the next launch.
        Scoped to the Onboarding modal; the old FirstLaunchSetup (kept in the
        tree under its own id) keeps its previous behaviour unchanged.
        */
        if modal != GpuiAppModalKind::Onboarding
            && self.gpui_app_modal_current_modal(cx) == Some(GpuiAppModalKind::Onboarding)
        {
            self.complete_first_launch_setup();
        }
        let promoted_spare = self.promote_gpui_app_modal_spare(
            modal,
            window_size,
            &open_message,
            &sidebar_state_message,
            cx,
        );
        if let Some(handle) = self.app_modal_window.clone() {
            let window_configuration_matches = handle
                .update(cx, |host, _modal_window, _cx| {
                    host.current_modal.uses_react_modal_host() == modal.uses_react_modal_host()
                        && host.current_modal.has_titlebar() == modal.has_titlebar()
                        && host.current_modal.is_resizable() == modal.is_resizable()
                        && (modal.uses_react_modal_host() || host.current_modal == modal)
                        && (modal.is_resizable() || host.initial_window_size == window_size)
                })
                .unwrap_or(false);
            if !window_configuration_matches {
                /*
                CDXC:AppModal 2026-07-22:
                The reusable React host cannot reuse native window options
                across a resizable/fixed-size transition. In particular, a
                Command Palette window carries the generic 520px minimum width,
                so resizing it cannot produce Delayed Send's exact 470x365
                fixed content size. Replace the child window at this native
                ownership boundary while retaining the destination open
                request, rather than closing it from React before Rust opens
                the next modal.
                */
                self.remove_gpui_app_modal_window_without_focus_restore(cx);
            } else {
                let update_result = handle.update(cx, |host, modal_window, cx| {
                    host.open_modal(
                        open_message.clone(),
                        sidebar_state_message.clone(),
                        modal,
                        self.sidebar_gxserver_bootstrap.clone(),
                        cx,
                    );
                    modal_window.resize(window_size);
                    modal_window.set_window_title(
                        if cfg!(any(target_os = "windows", target_os = "linux"))
                            || modal.has_titlebar()
                        {
                            &window_title
                        } else {
                            ""
                        },
                    );
                    if !promoted_spare {
                        modal_window.activate_window();
                    }
                    modal_window.refresh();
                });
                if update_result.is_ok() {
                    self.app_modal_window_id.set(Some(handle.window_id()));
                    self.app_modal_command_return_focus_target =
                        gpui_app_modal_command_return_focus_target_for_active_modal(
                            self.app_modal_command_return_focus_target,
                            return_focus_target,
                        );
                    return;
                }
                self.clear_lost_gpui_app_modal_window_handle();
            }
        }

        let mut extension_bridge_surface = None;
        let url = if modal.uses_react_modal_host() {
            let Some(url) = app_modal_host_url().ok() else {
                if let Some(window) = source_window {
                    window.push_notification(
                        Notification::warning("The GPUI app-modal host bundle is missing."),
                        cx,
                    );
                }
                return;
            };
            url
        } else if modal == GpuiAppModalKind::FindPrompts {
            let Some(url) = self.agents_find_runtime_url() else {
                if let Some(window) = source_window {
                    window.push_notification(
                        Notification::warning("The GPUI Search by Prompt bundle is missing."),
                        cx,
                    );
                }
                return;
            };
            url
        } else if let GpuiAppModalKind::Extension(id) = modal {
            let Some((url, bridge_surface)) = self.extension_modal_runtime(id) else {
                if let Some(window) = source_window {
                    window.push_notification(
                        Notification::warning("The extension runtime is unavailable."),
                        cx,
                    );
                }
                return;
            };
            extension_bridge_surface = Some(bridge_surface);
            url
        } else {
            GHOSTEX_TUTORIAL_VIDEO_URL.to_string()
        };
        let window_bounds = WindowBounds::Windowed(gpui::Bounds::centered_at(
            self.main_window_bounds.center(),
            window_size,
        ));
        let options = WindowOptions {
            kind: crate::app::window::popup_frame::child_window_kind(),
            #[cfg(target_os = "linux")]
            x11_parent: self.main_window_handle,
            window_bounds: Some(window_bounds),
            app_id: gpui_platform_window_app_id(),
            focus: true,
            icon: gpui_platform_window_icon(),
            show: true,
            is_resizable: modal.is_resizable(),
            window_min_size: Some(modal.window_min_size(&open_message)),
            display_id: crate::app::window::popup_frame::display_at(
                self.main_window_bounds.center(),
                cx,
            )
            .or(self.main_window_display_id),
            titlebar: modal.has_titlebar().then(|| gpui::TitlebarOptions {
                title: Some(window_title.clone().into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            ..Default::default()
        };
        let event_handler = self.app_modal_host_bridge_event_handler(cx);
        /*
        CDXC:Onboarding 2026-08-18:
        Only the tutorial video window loads a third-party page as its own
        document, and it is the only modal that needs a host-side action once
        that page is up. Every bridged modal keeps its React ready handshake
        and receives no load-end callback.
        */
        let page_load_end_handler = (modal == GpuiAppModalKind::WatchGhostexVideo)
            .then(|| self.tutorial_video_page_load_end_handler(cx));
        self.app_modal_open_attempt_id = self.app_modal_open_attempt_id.wrapping_add(1);
        let ready_timeout_attempt_id = self.app_modal_open_attempt_id;
        let extension_bridge = match modal {
            GpuiAppModalKind::Extension(id) => extension_bridge_surface.map(|bridge_surface| {
                (
                    bridge_surface,
                    self.extension_modal_bridge_event_handler(ready_timeout_attempt_id, id, cx),
                )
            }),
            _ => None,
        };
        let ready_timeout_open_message = open_message.clone();
        let ready_timeout_sidebar_state_message = sidebar_state_message.clone();
        let sidebar_has_projects = sidebar_state_message
            .get("hud")
            .and_then(|hud| hud.get("projectSettingsProjects"))
            .and_then(serde_json::Value::as_array)
            .is_some_and(|projects| !projects.is_empty());
        let main_window_native_view = self.parent_ns_view;
        let window_border = self.gpui_native_modal_palette().window_border();
        self.app_modal_window = cx
            .open_window(options, |modal_window, cx| {
                if !modal.has_titlebar() {
                    crate::app::window::popup_frame::frame_app_modal_window(
                        modal_window,
                        window_border,
                    );
                    modal_window.set_window_title(
                        if cfg!(any(target_os = "windows", target_os = "linux")) {
                            &window_title
                        } else {
                            ""
                        },
                    );
                }
                modal_window.activate_window();
                /*
                CDXC:Onboarding 2026-09-15 DECISION:
                User: "the modal must stay on top of the main ghostex app and centered on top of it".
                The onboarding host becomes an AppKit child window of the main window so it never drops behind the workspace and follows the main window when it moves.
                Other app modals keep their independent-window behaviour.
                */
                if modal == GpuiAppModalKind::Onboarding {
                    attach_gpui_app_modal_window_to_main_window(
                        modal_window,
                        main_window_native_view,
                    );
                }
                if matches!(
                    modal,
                    GpuiAppModalKind::FirstLaunchSetup | GpuiAppModalKind::Onboarding
                ) && !sidebar_has_projects
                {
                    /*
                    First-launch setup is required until the sidebar has a
                    project. Reject native close controls and Cmd-W while it
                    is open; the completion bridge removes the window
                    programmatically after persisting completion. Once any
                    project exists the user may leave through the window
                    chrome, and the close handler records completion.
                    */
                    modal_window.on_window_should_close(cx, |_window, _cx| false);
                }
                GpuiAppModalHostWindow::new(
                    modal_window,
                    url,
                    modal,
                    open_message,
                    sidebar_state_message,
                    self.sidebar_gxserver_bootstrap.clone(),
                    event_handler,
                    extension_bridge,
                    page_load_end_handler,
                    cx,
                )
            })
            .ok();
        if let Some(handle) = self.app_modal_window {
            self.app_modal_window_id.set(Some(handle.window_id()));
            self.app_modal_command_return_focus_target = return_focus_target;
            if modal.uses_react_modal_host() {
                self.schedule_gpui_app_modal_ready_timeout(
                    ready_timeout_attempt_id,
                    modal,
                    ready_timeout_open_message,
                    ready_timeout_sidebar_state_message,
                    cx,
                );
            }
        } else {
            self.app_modal_window_id.set(None);
            self.app_modal_command_return_focus_target = None;
        }
    }

    pub(crate) fn schedule_gpui_app_modal_ready_timeout(
        &mut self,
        attempt_id: u64,
        modal: GpuiAppModalKind,
        open_message: serde_json::Value,
        sidebar_state_message: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(APP_MODAL_HOST_READY_TIMEOUT)
                .await;
            let _ = this.update(cx, |this, cx| {
                this.handle_gpui_app_modal_ready_timeout(
                    attempt_id,
                    modal,
                    open_message,
                    sidebar_state_message,
                    cx,
                );
            });
        })
        .detach();
    }

    pub(crate) fn handle_gpui_app_modal_ready_timeout(
        &mut self,
        attempt_id: u64,
        modal: GpuiAppModalKind,
        open_message: serde_json::Value,
        sidebar_state_message: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        if attempt_id != self.app_modal_open_attempt_id {
            return;
        }
        if self.app_modal_window.is_none() || self.gpui_app_modal_window_is_ready(cx) {
            return;
        }

        if !self.app_modal_ready_retry_used {
            self.app_modal_ready_retry_used = true;
            self.remove_gpui_app_modal_window_without_focus_restore(cx);
            self.open_gpui_app_modal_window_inner(
                modal,
                open_message,
                sidebar_state_message,
                None,
                false,
                cx,
            );
            return;
        }

        self.remove_gpui_app_modal_window_without_focus_restore(cx);
        cx.notify();
    }

    pub(crate) fn gpui_app_modal_window_is_ready(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        let Some(handle) = self.app_modal_window.clone() else {
            return false;
        };
        handle
            .update(cx, |host, _window, _cx| host.is_ready())
            .unwrap_or(false)
    }

    pub(crate) fn remove_gpui_app_modal_window_without_focus_restore(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.app_modal_command_return_focus_target = None;
        self.app_modal_window_id.set(None);
        let Some(handle) = self.app_modal_window.take() else {
            return false;
        };
        handle
            .update(cx, |_host, modal_window, _cx| {
                modal_window.remove_window();
            })
            .is_ok()
    }

    pub(crate) fn clear_lost_gpui_app_modal_window_handle(&mut self) {
        /*
        CDXC:FocusRouting 2026-06-25-22:25:
        A failed GPUI app-modal window update means the runtime handle no longer owns a close lifecycle, so clear the paired command return-focus target with the stale handle to prevent a later modal close from consuming it.
        */
        self.app_modal_window = None;
        self.app_modal_window_id.set(None);
        self.app_modal_command_return_focus_target = None;
    }

    pub(crate) fn handle_gpui_app_modal_window_closed(
        &mut self,
        window_id: gpui::WindowId,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:AppModal 2026-07-12:
        A modal window closed through native window chrome does not send the
        React `close` bridge message. End the matching native open attempt at
        GPUI's actual window lifecycle boundary so its pending CEF-ready
        timeout cannot mistake the user-closed window for a failed host and
        recreate it. Programmatic closes take the handle before removing the
        window, and stale retry-window callbacks have a different id, so only
        a user/native close reaches this ownership transition.
        */
        let Some(handle) = self.app_modal_window else {
            return;
        };
        if handle.window_id() != window_id {
            return;
        }

        let closed_modal = handle
            .update(cx, |host, _window, _cx| host.current_modal)
            .ok();
        if matches!(
            closed_modal,
            Some(GpuiAppModalKind::FirstLaunchSetup) | Some(GpuiAppModalKind::Onboarding)
        ) {
            // Native close is only allowed once the sidebar has a project, so
            // leaving through the window chrome counts as finishing setup.
            self.complete_first_launch_setup();
        }

        self.app_modal_window = None;
        self.app_modal_window_id.set(None);
        self.app_modal_open_attempt_id = self.app_modal_open_attempt_id.wrapping_add(1);
        self.app_modal_ready_retry_used = false;
        self.restore_gpui_app_modal_command_return_focus_if_needed(cx);
        self.resume_deferred_gpui_portless_setup_prompt(cx);
        if closed_modal == Some(GpuiAppModalKind::ExportTranscriptResult) {
            self.pending_export_transcript_reveal_path = None;
        }
        self.schedule_gpui_app_modal_spare_preload(cx);
    }

    pub(crate) fn complete_first_launch_setup(&self) {
        let mut state = load_gpui_first_run_onboarding_state();
        if !state.first_launch_setup_complete {
            state.first_launch_setup_complete = true;
            persist_gpui_first_run_onboarding_state(&state);
        }
    }

    pub(crate) fn open_gpui_first_launch_setup_with_sidebar_state(
        &mut self,
        base_sidebar_state: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let sidebar_state_message =
            self.with_gpui_command_pane_sidebar_indicators(base_sidebar_state);
        // CDXC:Onboarding 2026-09-15 DECISION:
        // User: "i want to switch to using the new modal when a new user starts the app instead of the
        // old one", so the automatic first run opens the five-panel Onboarding modal
        // (packages/core-ui/onboarding), the same one the Tips dropdown's "Setup" button and the Quick
        // Access "Setup" command open (titlebar/settings_and_action_state.rs, delayed_send.rs). Only this
        // path adds `"firstRun": true` to the open message: the user decided only the first run ever
        // applies Browser + Docs as the enabled views, never a reopen from Tips > Setup. The old
        // FirstLaunchSetup modal stays in the tree, reachable by its `firstLaunchSetup` id ("keep the old
        // one there might come back to it"), and nothing opens it by default.
        let modal = GpuiAppModalKind::Onboarding;
        let mut open_message = modal.open_message();
        open_message["firstRun"] = serde_json::Value::Bool(true);
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, None, cx);
    }

    pub(crate) fn close_gpui_app_modal_window_and_restore_command_focus(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut removed_modal_window = false;
        if let Some(handle) = self.app_modal_window.take() {
            self.app_modal_window_id.set(None);
            removed_modal_window = handle
                .update(cx, |_host, modal_window, _cx| {
                    modal_window.remove_window();
                })
                .is_ok();
        }
        if !removed_modal_window {
            self.clear_lost_gpui_app_modal_window_handle();
            return;
        }
        self.restore_gpui_app_modal_command_return_focus_if_needed(cx);
        /*
        CDXC:Portless 2026-08-18:
        A modal dismissed from React takes the window handle here instead of
        reaching the native-close path, so the deferred Portless prompt has to
        be resumed from this ownership boundary as well.
        */
        self.resume_deferred_gpui_portless_setup_prompt(cx);
        self.schedule_gpui_app_modal_spare_preload(cx);
    }

    pub(crate) fn restore_gpui_app_modal_command_return_focus_if_needed(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(target) = self.app_modal_command_return_focus_target.take() else {
            return false;
        };
        if !restore_command_pane_app_modal_return_focus(&mut self.command_pane, target) {
            return false;
        }

        self.focus_command_pane(cx);
        self.scroll_command_group_active_tab(target.group_id);
        self.scroll_focused_command_active_tab();
        self.persist_shell_layout_state();
        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        cx.notify();
        true
    }

    pub(crate) fn sync_gpui_ghostty_config_file_after_settings_save(
        &mut self,
        ghostty_config_backed_setting_keys_changed: &[&str],
        settings_snapshot: &shared_settings::SharedSidebarSettingsSnapshot,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Terminal 2026-06-24-12:24:
        Normal GPUI `updateSettings` saves should write generated Ghostty managed terminal settings only when a config-backed terminal value changed. The current GPUI GhosttyKit wrapper has load/create surface FFI but no safe live reload/update API, so this write affects Ghostty's config file, external Ghostty reloads, and future/recreated GPUI surfaces without claiming live embedded terminal reload.
        */
        if ghostty_config_backed_setting_keys_changed.is_empty() {
            return;
        }
        if shared_settings::write_ghostty_terminal_config_from_settings_object(
            settings_snapshot.object(),
            ghostty_config_backed_setting_keys_changed,
        )
        .is_ok()
        {
            return;
        }
        let message = "Settings were saved, but GPUI could not write the managed Ghostty config file. Existing embedded terminals were not live reloaded.";
        self.dispatch_gpui_settings_action_status("ghosttySettings", false, message, cx);
        self.dispatch_gpui_app_modal_toast(
            "warning",
            "Could not update Ghostty config",
            message,
            cx,
        );
    }

    pub(crate) fn sync_gpui_gxserver_agent_settings_after_save(
        &mut self,
        previous_agent_settings: shared_settings::SharedGxserverAgentSettings,
        next_agent_settings: shared_settings::SharedGxserverAgentSettings,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:AgentProviders 2026-06-24-11:39:
        GPUI matches macOS for agent launch policy: shared Settings is the local render cache, while local gxserver owns inherited Accept All and Default Prompt Agent behavior for launchers across clients. After a successful Settings save, post the current two gxserver-owned values only when either changed, and keep token/network/parser failures silent so unavailable gxserver never creates fake daemon state or rolls back the saved local cache.
        */
        if previous_agent_settings == next_agent_settings {
            return;
        }

        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let canonical_agent_settings = background
                .spawn(async move { update_gpui_gxserver_agent_settings(&next_agent_settings) })
                .await
                .ok();
            let Some(canonical_agent_settings) = canonical_agent_settings else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.apply_gpui_gxserver_agent_settings_to_local_settings(
                    canonical_agent_settings,
                    cx,
                );
            });
        })
        .detach();
    }

    pub(crate) fn reconcile_gpui_gxserver_agent_settings_in_background(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:AgentProviders 2026-06-24-12:14:
        GPUI startup/open-time hydration must match macOS migration semantics for gxserver-owned agent policy. Read `/api/readAgentSettings`; if gxserver has no persisted row, seed it once from current shared Settings, otherwise treat daemon values as canonical and refresh the local render cache through the central settings service without logging tokens, response bodies, paths, commands, or user content.
        */
        if self.gxserver_agent_settings_reconciliation_in_flight {
            return;
        }
        self.gxserver_agent_settings_reconciliation_in_flight = true;

        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let hydration_result = background
                .spawn(async move { reconcile_gpui_gxserver_agent_settings_with_daemon() })
                .await
                .ok()
                .flatten();
            let _ = this.update(cx, |this, cx| {
                this.gxserver_agent_settings_reconciliation_in_flight = false;
                if let Some(hydration_result) = hydration_result {
                    this.apply_gpui_gxserver_agent_settings_hydration_result(hydration_result, cx);
                }
            });
        })
        .detach();
    }

    pub(crate) fn gpui_app_modal_sidebar_state_message_for_open(
        &mut self,
        modal: GpuiAppModalKind,
        cx: &mut gpui::Context<Self>,
    ) -> serde_json::Value {
        /*
        CDXC:Settings 2026-06-24-12:22:
        Any shared Settings entry point can show agent-owned controls after the React host selects its initial tab. Reconcile gxserver-owned agent policy before hydrating Settings, Hotkeys, Configure Agents, Configure Actions, and Open Targets so entry-specific modal ids do not drift from the canonical Settings route.
        */
        if modal.is_settings_modal_entry() {
            self.reconcile_gpui_gxserver_agent_settings_in_background(cx);
        }
        self.gpui_app_modal_sidebar_state_message_from_held_hydrate(cx)
    }

    pub(crate) fn gpui_app_modal_sidebar_state_message(&self) -> serde_json::Value {
        self.with_gpui_command_pane_sidebar_indicators(gpui_app_modal_sidebar_state_message(
            self.latest_sidebar_project_snapshot.as_ref(),
        ))
    }

    pub(crate) fn gpui_app_modal_sidebar_state_message_from_settings_snapshot(
        &self,
        settings_snapshot: &shared_settings::SharedSidebarSettingsSnapshot,
    ) -> serde_json::Value {
        self.with_gpui_command_pane_sidebar_indicators(
            gpui_app_modal_sidebar_state_message_from_settings_snapshot(
                settings_snapshot,
                self.latest_sidebar_project_snapshot.as_ref(),
            ),
        )
    }

    pub(crate) fn with_gpui_command_pane_sidebar_indicators(
        &self,
        mut message: serde_json::Value,
    ) -> serde_json::Value {
        message = self.with_remote_project_action_rows(message);
        /*
        CDXC:CommandPane 2026-06-25-10:50:
        App-modal sidebar hydrates must carry the same command-session indicators as the live GPUI sidebar HUD. Reuse the sanitized command-pane summary and gxserver command rows; never compute from command text, paths, status-file paths, terminal output, logs, or persisted shell-state JSON.
        */
        let commands = message
            .get("hud")
            .and_then(|hud| hud.get("commands"))
            .cloned()
            .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
        let sessions = self.command_pane.sidebar_command_session_sources(
            self.shell_focus == ShellFocusTarget::CommandPane,
            &self.command_delayed_send_timers,
            &self.command_close_after_done_timers,
            SystemTime::now(),
        );
        message["hud"]["commandSessionIndicators"] =
            gpui_sidebar_command_session_indicators_from_command_pane_sources(&commands, &sessions);
        self.with_project_view_scope_options(message)
    }

    pub(crate) fn apply_gpui_gxserver_agent_settings_hydration_result(
        &mut self,
        hydration_result: GpuiGxserverAgentSettingsHydrationResult,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:AgentProviders 2026-06-24-12:14:
        Startup/open hydration may finish after the user saves Settings. Apply daemon canonical values only if the shared render cache still matches the local values used for the read/seed decision; a newer save uses the existing save-time gxserver sync path instead of being overwritten by a stale startup response.
        */
        if shared_settings::shared_sidebar_settings_snapshot().gxserver_agent_settings()
            != hydration_result.expected_local_settings
        {
            return;
        }
        self.apply_gpui_gxserver_agent_settings_to_local_settings(
            hydration_result.canonical_settings,
            cx,
        );
    }

    pub(crate) fn apply_gpui_gxserver_agent_settings_to_local_settings(
        &mut self,
        canonical_agent_settings: shared_settings::SharedGxserverAgentSettings,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:AgentProviders 2026-06-24-11:39:
        gxserver read/update responses are canonical for inherited agent launch policy. If the daemon reports either agent setting differently than the current GPUI render cache, persist those canonical values through the central shared Settings service and refresh the modal/sidebar settings state again instead of writing a separate cache or logging private daemon details.
        */
        let latest_settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        if latest_settings_snapshot.gxserver_agent_settings() == canonical_agent_settings {
            return;
        }

        let mut settings_object = latest_settings_snapshot.object().clone();
        canonical_agent_settings.write_to_settings_object(&mut settings_object);
        let Ok(write_result) =
            shared_settings::write_shared_sidebar_settings_object(settings_object)
        else {
            return;
        };
        self.refresh_gpui_shared_settings_consumers_after_save(&write_result.snapshot, cx);
    }

    pub(crate) fn refresh_gpui_shared_settings_consumers_after_save(
        &mut self,
        settings_snapshot: &shared_settings::SharedSidebarSettingsSnapshot,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Settings 2026-06-24-11:19:
        After a successful Settings save or gxserver startup/open canonical sync, GPUI refreshes only the settings-dependent runtime state it owns today: app-modal hydrate/sidebarState, sidebar debug/beta booleans through the existing CEF runtime-settings path, project-workarea CEF visibility, project-editor auto-sleep scheduling, supported embedded Ghostty request-map settings, gxserver-owned agent-policy reconciliation, and central-service render reads such as the Browser feedback/profile toolbar controls. This is not full settings fan-out; many action bridges, code-server sync, live Ghostty config reloads, and broad future side effects remain outside this path.
        */
        self.reschedule_project_editor_auto_sleep_if_policy_changed_from_shared_settings(
            settings_snapshot,
            cx,
        );
        self.apply_gpui_sidebar_visibility_memory_from_saved_settings(settings_snapshot);
        self.apply_gpui_command_pane_side_from_saved_settings(settings_snapshot);
        refresh_gpui_visual_settings(settings_snapshot);
        apply_gpui_component_theme(cx);
        self.native_kanban_notify_appearance(cx);
        self.native_automate_notify_appearance(cx);
        self.refresh_sidebar_runtime_settings_from_shared_settings(settings_snapshot, cx);
        self.coerce_active_mode_to_available_project_context(cx);
        self.prune_project_workarea_runtime_cef_surfaces_for_current_gates(cx);
        self.ensure_project_workarea_runtime_cef_surfaces_for_current_context(cx);
        #[cfg(target_os = "macos")]
        self.refresh_terminal_ghostty_surface_config_requests_from_shared_settings(
            settings_snapshot,
        );
        self.reload_live_gpui_engine_terminal_config(cx);
        let sidebar_state_message =
            self.gpui_app_modal_sidebar_state_message_from_settings_snapshot(settings_snapshot);
        self.refresh_open_gpui_app_modal_sidebar_state(sidebar_state_message, cx);
        self.sync_titlebar_account_privacy(cx);
        // Newly saved hotkey chords bind immediately. The save boundary first
        // adds targeted Unbind markers for the prior Ghostex action chords, so
        // removed/remapped entries stop dispatching without clearing GPUI or
        // gpui-component's unrelated keymap entries.
        cx.bind_keys(gpui_configured_hotkey_key_bindings_from_settings());
        cx.notify();
    }

    pub(crate) fn reload_live_gpui_engine_terminal_config(&mut self, cx: &mut gpui::Context<Self>) {
        let shared_engine_settings =
            shared_settings::shared_sidebar_settings_snapshot().gpui_terminal_engine_settings();
        #[cfg(target_os = "macos")]
        let mut config = {
            let Ok(path) = shared_settings::selected_ghostty_config_path() else {
                return;
            };
            let Ok(config) =
                terminal_ghostty_surface::load_ghostty_terminal_engine_config_from_path(
                    &path,
                    terminal_gpui_engine::ghostty_theme_source(
                        &shared_engine_settings.ghostty_theme,
                    ),
                )
            else {
                return;
            };
            config
        };
        #[cfg(not(target_os = "macos"))]
        let mut config =
            terminal_gpui_engine::GpuiTerminalEngineConfig::from_shared(&shared_engine_settings);
        config.apply_color_scheme(
            &shared_engine_settings,
            gpui_system_uses_light_appearance(),
            gpui_terminal_theme_background(&shared_engine_settings),
        );

        // This setting is app-owned and is not part of Ghostty's finalized
        // config string on macOS.
        config.view.scroll_to_bottom_when_typing =
            shared_engine_settings.scroll_to_bottom_when_typing;
        config.view.background_image =
            terminal_gpui_engine::terminal_background_image_from_settings(&shared_engine_settings);
        config.view.background_alpha = terminal_default_background_alpha();

        let confirm_close_behavior =
            terminal_gpui_engine::gpui_engine_confirm_close_behavior(&config);
        for record in self
            .agents_gpui_engine_terminals
            .values_mut()
            .chain(self.command_gpui_engine_terminals.values_mut())
            .chain(
                self.parked_agents_terminal_runtimes_by_project
                    .values_mut()
                    .flat_map(|runtime| runtime.gpui_engine_terminals.values_mut()),
            )
        {
            record.confirm_close_behavior = confirm_close_behavior;
            let view = record.view.clone();
            let font = config.font.clone();
            let settings = config.view.clone();
            let colors = config.colors.clone();
            let option_as_alt = config.option_as_alt;
            view.update(cx, |view, cx| {
                view.apply_font(font);
                view.apply_settings(settings);
                view.model_mut().set_option_as_alt(option_as_alt);
                if let Some(colors) = colors {
                    let _ = view.model_mut().set_default_colors(
                        colors.foreground,
                        colors.background,
                        colors.cursor,
                        &colors.palette,
                    );
                }
                view.refresh_appearance(cx);
            });
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn refresh_terminal_ghostty_surface_config_requests_from_shared_settings(
        &mut self,
        settings_snapshot: &shared_settings::SharedSidebarSettingsSnapshot,
    ) {
        /*
        CDXC:Terminal 2026-06-24-11:27:
        `updateSettings` fan-out refreshes the GPUI-owned Ghostty request maps so subsequent Agents, command, and startup surface creations use the saved supported terminal settings. Existing live Ghostty surfaces are not reloaded here because this runtime path does not yet expose a safe config-reload/apply contract; do not fake reload by dropping running terminals or logging raw settings.
        */
        let terminal_config =
            gpui_terminal_ghostty_surface_config_from_shared_settings(settings_snapshot);
        for request in self
            .agents_terminal_ghostty_surface_config_requests
            .values_mut()
        {
            request.set_terminal_config(terminal_config);
        }
        for request in self
            .command_terminal_ghostty_surface_config_requests
            .values_mut()
        {
            request.set_terminal_config(terminal_config);
        }
        for request in self
            .agents_terminal_startup_ghostty_surface_config_requests
            .values_mut()
        {
            request.set_terminal_config(terminal_config);
        }
    }

    pub(crate) fn refresh_open_gpui_app_modal_sidebar_state(
        &mut self,
        sidebar_state_message: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(handle) = self.app_modal_window.clone() else {
            return;
        };
        let sidebar_state_message =
            self.with_gpui_command_pane_sidebar_indicators(sidebar_state_message);
        let update_result = handle.update(cx, |host, modal_window, cx| {
            host.refresh_sidebar_state_message(sidebar_state_message.clone(), cx);
            modal_window.refresh();
        });
        if update_result.is_err() {
            self.clear_lost_gpui_app_modal_window_handle();
        }
    }

    pub(crate) fn dispatch_open_gpui_app_modal_sidebar_state_payload(
        &mut self,
        payload: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:StatusPet 2026-06-24-11:36:
        Settings status/action responses are transient `sidebarState` messages to the shared React modal host. They must clear modal loading states without replacing the stored full hydrate snapshot used when the app-modal host becomes ready or a Settings save rehydrates the modal.
        */
        let Some(handle) = self.app_modal_window.clone() else {
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
            }
            return;
        };
        let update_result = handle.update(cx, |host, modal_window, cx| {
            host.dispatch_transient_sidebar_state_message(payload, cx);
            modal_window.refresh();
        });
        if update_result.is_err() {
            self.clear_lost_gpui_app_modal_window_handle();
        }
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
            && let Some(handle) = self.titlebar_popup_window.clone()
        {
            let payload = payload.clone();
            let _ = handle.update(cx, |popup, window, cx| {
                popup.update_tips_runtime_status(payload, cx);
                window.refresh();
            });
        }
    }

    pub(crate) fn gpui_app_modal_current_modal(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> Option<GpuiAppModalKind> {
        let handle = self.app_modal_window.clone()?;
        handle
            .update(cx, |host, _modal_window, _cx| host.current_modal)
            .ok()
    }

    pub(crate) fn dispatch_open_gpui_app_modal_message(
        &mut self,
        message: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.receive_native_app_modal_message(&message, cx) {
            return;
        }
        let Some(handle) = self.app_modal_window.clone() else {
            return;
        };
        let update_result = handle.update(cx, |host, modal_window, cx| {
            host.dispatch_transient_message(message.clone(), cx);
            modal_window.refresh();
        });
        if update_result.is_err() {
            self.clear_lost_gpui_app_modal_window_handle();
        }
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
                epoch: 0,
            },
            cx,
        );
    }

    /// Will-terminate persistence flush (macOS `applicationWillTerminate`
    /// parity): persist shell state, restore lid-close sleep by stopping the
    /// Keep Awake runtime, stop the app-owned code-server, and deliberately
    /// never stop gxserver. CEF owns the durable Browser-profile store and
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
        let _ = cx;
    }
}
