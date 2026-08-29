// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: app modal / titlebar panel windows and shared-settings save fan-out

use std::collections::HashSet;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use crate::cef::CefBrowser;
use gpui::Entity;
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
        CDXC:GPUITitlebarAppModalHost 2026-06-24-10:42:
        GPUI titlebar Settings, Hotkeys, and Command Palette actions must open the shared React app-modal host in a real GPUI CEF window. The route reuses the macOS modal ids and latest Settings-compatible sidebar hydrate, with no duplicated React modal UI, WebKit/WKWebView, transparent overlay, hidden hit-test region, synthetic mouse routing, or generic fallback surface.

        CDXC:GPUIPreviousSessionsModal 2026-06-24-11:53:
        Previous Sessions joins the same GPUI-owned CEF app-modal window path as Settings, Hotkeys, and Command Palette. The titlebar route must open the shared React modal component directly and let its gxserver-backed query resolve through sidebarCommand responses, not duplicated GPUI UI, overlays, hit-test routing, or stored hydrate rewrites.

        CDXC:GPUISettingsEntryModals 2026-06-24-12:22:
        Configure Agents, Configure Actions, and Open Targets are Settings-modal entry points in the shared React host. GPUI must preserve their modal ids, attach the latest Settings-compatible sidebar hydrate, and reuse this production CEF app-modal route instead of adding duplicate React UI, stubs, fallback routing, overlays, or hidden hit regions.

        CDXC:GPUIAgentsHubModal 2026-06-24-12:26:
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
        CDXC:GPUICommandTabContextMenu 2026-06-25-17:37:
        Delayed Send from a clicked command tab is session-scoped like native, but GPUI can only execute the later Return through a visible mounted command body. Selecting, expanding, and waking the clicked tab before opening the modal is the normal-layout equivalent of targeting that native command session.
        */
        if !self.command_pane_tab_exists(group_id, session_id) {
            return false;
        }
        let expand_pane = !self.command_pane.is_expanded();
        if !self.select_command_pane_tab(group_id, session_id, expand_pane, window, cx) {
            return false;
        }
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
        CDXC:GPUICommandTabContextMenu 2026-06-25-18:33:
        Legacy clicked-tab action handlers focus the clicked terminal before Rename and Close After Done dispatch. Select and focus the clicked GPUI command tab without expanding a collapsed strip so the action target becomes the command-pane focus while left-click remains the only hidden-open gesture.

        CDXC:GPUICommandAttention 2026-06-25-19:58:
        Primary clicked-tab context actions use that same focus path, so they acknowledge only the clicked Attention command session before opening Rename or toggling Close After Done.

        CDXC:GPUICommandTabContextMenu 2026-06-27-01:55:
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
        self.focus_command_pane();
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

    pub(crate) fn set_gpui_titlebar_tips_panel_open(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUITitlebarTips 2026-06-24-23:17:
        The GPUI info glyph opens the shared React `titlebar-host.html?ghostexTitlebarPanel=tips` document inside an app-owned anchored overlay whose top edge is TITLEBAR_HEIGHT. Because the rendered child is a native CEF view, dropdown state changes must explicitly show/hide the CEF surface instead of relying on GPUI paint removal.
        */
        if open {
            self.close_gpui_titlebar_popup(None, window, cx);
            if self.titlebar_resources_panel_open {
                self.set_gpui_titlebar_resources_panel_open(false, window, cx);
            }
            let Some(panel) = self.ensure_gpui_titlebar_tips_panel(window, cx) else {
                return;
            };
            if !self.titlebar_tips_panel_open {
                self.titlebar_dropdown_previous_focus_handle = window.focused(cx);
            }
            self.titlebar_tips_panel_open = true;
            self.titlebar_dropdown_focus_handle.focus(window, cx);
            panel.update(cx, |panel, cx| {
                panel.set_visible(true, cx);
            });
            self.dispatch_gpui_titlebar_tips_project_state_update(
                self.gpui_titlebar_tips_initial_project_state_update(),
                cx,
            );
            panel.update(cx, |panel, cx| {
                panel.install_unread_count_probe(cx);
            });
            self.request_gpui_titlebar_tips_runtime_status(cx);
        } else {
            self.titlebar_tips_panel_open = false;
            if let Some(panel) = self.titlebar_tips_panel.clone() {
                panel.update(cx, |panel, cx| {
                    panel.set_visible(false, cx);
                });
            }
            if self
                .titlebar_dropdown_focus_handle
                .contains_focused(window, cx)
                && let Some(previous_focus_handle) =
                    self.titlebar_dropdown_previous_focus_handle.take()
            {
                previous_focus_handle.focus(window, cx);
            } else {
                self.titlebar_dropdown_previous_focus_handle = None;
            }
        }
        cx.notify();
    }

    pub(crate) fn ensure_gpui_titlebar_tips_panel(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<Entity<GpuiTitlebarTipsPanel>> {
        if let Some(panel) = self.titlebar_tips_panel.clone() {
            return Some(panel);
        }
        let url = match titlebar_tips_panel_url() {
            Ok(url) => url,
            Err(_) => {
                window.push_notification(
                    Notification::warning("The GPUI titlebar host bundle is missing."),
                    cx,
                );
                return None;
            }
        };
        let parent_ns_view = self.parent_ns_view;
        let event_handler = self.app_modal_host_bridge_event_handler(cx);
        let panel = match GpuiTitlebarTipsPanel::new(parent_ns_view, url, event_handler, cx) {
            Ok(panel) => panel,
            Err(error) => {
                // Same user-visible handling as a missing bundle; the next
                // dropdown open retries creation
                // (CDXC:GPUICefBrowserCreateFallible 2026-07-11).
                support_logs::append(
                    support_logs::GpuiSupportLog::CrashReports,
                    "gpui.cefSurface.createFailed",
                    serde_json::json!({ "surface": "titlebarTips", "error": error }),
                );
                window.push_notification(
                    Notification::warning("The Tips panel could not be created."),
                    cx,
                );
                return None;
            }
        };
        self.titlebar_tips_panel = Some(panel.clone());
        Some(panel)
    }

    pub(crate) fn gpui_titlebar_tips_initial_project_state_update(&self) -> serde_json::Value {
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        serde_json::json!({
            "activeMode": self.active_mode.element_slug(),
            "debuggingMode": settings_snapshot.debugging_mode(),
            "projectName": self.project_name,
            "showBetaFeatures": settings_snapshot.show_beta_features(),
            "sidebarTheme": gpui_app_modal_sidebar_theme_from_settings(settings_snapshot.object()),
        })
    }

    pub(crate) fn set_gpui_titlebar_resources_panel_open(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUIResourcesTitlebar 2026-07-08:
        The GPUI Resources glyph now opens the shared React
        `titlebar-host.html?ghostexTitlebarPanel=resources` document inside the
        same strict app-owned anchored overlay as Tips. Each open creates a fresh
        CEF panel so React owns live ps/lsof polling only while visible; close
        drops the entity instead of hiding a long-lived sampler.
        */
        if open {
            self.close_gpui_titlebar_popup(None, window, cx);
            if self.titlebar_tips_panel_open {
                self.set_gpui_titlebar_tips_panel_open(false, window, cx);
            }
            let was_open = self.titlebar_resources_panel_open;
            let url = match titlebar_resources_panel_url() {
                Ok(url) => url,
                Err(_) => {
                    window.push_notification(
                        Notification::warning("The GPUI titlebar host bundle is missing."),
                        cx,
                    );
                    return;
                }
            };
            self.titlebar_resources_panel_open_generation = self
                .titlebar_resources_panel_open_generation
                .wrapping_add(1);
            let generation = self.titlebar_resources_panel_open_generation;
            if let Some(panel) = self.titlebar_resources_panel.take() {
                panel.update(cx, |panel, cx| {
                    panel.set_visible(false, cx);
                });
            }
            self.titlebar_resources_panel_ready = false;
            self.titlebar_resources_panel_open = true;
            /*
            CDXC:GPUIResourcesTitlebarCrash 2026-07-09:
            CEF can drain the main dispatch queue while synchronously creating
            a child browser. Do that work in a foreground task before
            re-entering `app.update`; otherwise a queued GPUI task can run
            while this update still holds AppCell's mutable borrow.
            */
            if !was_open {
                self.titlebar_dropdown_previous_focus_handle = window.focused(cx);
            }
            self.titlebar_dropdown_focus_handle.focus(window, cx);
            self.refresh_gpui_titlebar_resources_presentation_groups(cx);
            cx.notify();

            /*
            CDXC:GPUIResourcesInstantOpen 2026-07-13:
            Commit the open/loading state for a complete frame before creating
            the fresh Resources CEF browser. CreateBrowserSync may spend one or
            two seconds initializing its request context; running it in the
            same update that toggles `open` prevented GPUI from painting the
            dropdown skeleton first even though loading chrome already existed.
            */
            let app = cx.entity().downgrade();
            window.on_next_frame(move |_window, cx| {
                let _ = app.update(cx, |this, cx| {
                    if !this.titlebar_resources_panel_open
                        || this.titlebar_resources_panel_open_generation != generation
                        || this.titlebar_resources_panel.is_some()
                    {
                        return;
                    }
                    let parent_ns_view = this.parent_ns_view;
                    let event_handler = this.app_modal_host_bridge_event_handler(cx);
                    this.schedule_gpui_titlebar_resources_panel_creation(
                        generation,
                        parent_ns_view,
                        url,
                        event_handler,
                        cx,
                    );
                });
            });
        } else {
            self.titlebar_resources_panel_open_generation = self
                .titlebar_resources_panel_open_generation
                .wrapping_add(1);
            self.titlebar_resources_panel_open = false;
            self.titlebar_resources_panel_ready = false;
            if let Some(panel) = self.titlebar_resources_panel.take() {
                panel.update(cx, |panel, cx| {
                    panel.set_visible(false, cx);
                });
            }
            if self
                .titlebar_dropdown_focus_handle
                .contains_focused(window, cx)
                && let Some(previous_focus_handle) =
                    self.titlebar_dropdown_previous_focus_handle.take()
            {
                previous_focus_handle.focus(window, cx);
            } else {
                self.titlebar_dropdown_previous_focus_handle = None;
            }
        }
        cx.notify();
    }

    pub(crate) fn schedule_gpui_titlebar_resources_panel_creation(
        &self,
        generation: u64,
        parent_ns_view: *mut std::ffi::c_void,
        url: String,
        event_handler: cef::AppModalHostBridgeEventHandler,
        cx: &mut gpui::Context<Self>,
    ) {
        let app = cx.entity().downgrade();
        let foreground = cx.foreground_executor().clone();
        let mut async_cx = cx.to_async();
        foreground
            .spawn(async move {
                let browser = match GpuiTitlebarResourcesPanel::create_browser(
                    parent_ns_view,
                    url,
                    event_handler,
                ) {
                    Ok(browser) => browser,
                    Err(error) => {
                        // The dropdown stays empty for this open; the next
                        // open re-runs creation
                        // (CDXC:GPUICefBrowserCreateFallible 2026-07-11).
                        support_logs::append(
                            support_logs::GpuiSupportLog::CrashReports,
                            "gpui.cefSurface.createFailed",
                            serde_json::json!({
                                "surface": "titlebarResources",
                                "error": error,
                            }),
                        );
                        return;
                    }
                };
                let _ = app.update_in(&mut async_cx, |this, _window, cx| {
                    this.attach_gpui_titlebar_resources_panel(generation, browser, cx);
                });
            })
            .detach();
    }

    pub(crate) fn attach_gpui_titlebar_resources_panel(
        &mut self,
        generation: u64,
        browser: Rc<CefBrowser>,
        cx: &mut gpui::Context<Self>,
    ) {
        let stale = !self.titlebar_resources_panel_open
            || self.titlebar_resources_panel_open_generation != generation
            || self.titlebar_resources_panel.is_some();
        if stale {
            browser.set_visible(false);
            return;
        }
        let browser_for_ready_dispatch = browser.clone();
        let panel = GpuiTitlebarResourcesPanel::from_browser(browser, cx);
        if self.titlebar_resources_panel_ready {
            panel.update(cx, |panel, cx| {
                panel.set_visible(true, cx);
            });
            let project_state_update = self.gpui_titlebar_resources_project_state_update(cx);
            gpui_titlebar_resources_dispatch_project_state_update(
                cx,
                browser_for_ready_dispatch,
                project_state_update,
            );
        }
        self.titlebar_resources_panel = Some(panel);
        cx.notify();
    }

    pub(crate) fn refresh_gpui_titlebar_resources_presentation_groups(
        &self,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUIResourcesDevServers 2026-07-26:
        Read the daemon's presentation snapshot off the main thread once per
        Resources open and project it into the shared titlebar resource-group
        contract. A stale open generation drops the result instead of pushing an
        older session graph into a newer dropdown.
        */
        let generation = self.titlebar_resources_panel_open_generation;
        let active_project_id = self.gpui_daemon_sessions_active_project_id();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let groups = background
                .spawn(async move {
                    gpui_read_gxserver_presentation_snapshot()
                        .map(|snapshot| {
                            gpui_titlebar_resource_groups_from_presentation_snapshot(
                                &snapshot,
                                active_project_id.as_deref(),
                            )
                        })
                        .unwrap_or_default()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.titlebar_resources_panel_open_generation != generation
                    || this.titlebar_resources_presentation_groups == groups
                {
                    return;
                }
                this.titlebar_resources_presentation_groups = groups;
                if this.titlebar_resources_panel_open && this.titlebar_resources_panel_ready {
                    this.dispatch_gpui_titlebar_resources_project_state_update(cx);
                }
            });
        })
        .detach();
    }

    pub(crate) fn dispatch_gpui_titlebar_resources_project_state_update(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(panel) = self.titlebar_resources_panel.clone() else {
            return;
        };
        let project_state_update = self.gpui_titlebar_resources_project_state_update(cx);
        let browser = panel.update(cx, |panel, cx| panel.browser(cx));
        gpui_titlebar_resources_dispatch_project_state_update(cx, browser, project_state_update);
    }

    pub(crate) fn gpui_titlebar_resources_project_state_update(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> serde_json::Value {
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let settings_object = settings_snapshot.object();
        let active_project_id = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.active_project_id.as_ref())
            .map(|project_id| project_id.0.clone());
        let project_name = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .map(|snapshot| snapshot.display_name.clone())
            .unwrap_or_else(|| self.project_name.clone());
        let project_path = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.in_memory_project_path.as_ref())
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default();
        let project_is_quick = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.is_quick_projectless);
        let resource_groups = self.gpui_titlebar_resources_resource_groups(
            active_project_id.as_deref(),
            &project_name,
            &project_path,
        );
        let browser_tabs =
            self.gpui_titlebar_resources_browser_tabs(cx, active_project_id.as_deref());
        let mut code_editor_project_ids = Vec::new();
        if self
            .project_editor_shell
            .is_mode_awake(TitlebarMode::Source)
            && self.source_code_server_runtime.state == SourceCodeServerRuntimeLaunchState::Ready
            && let Some(project_id) = active_project_id.as_ref()
        {
            code_editor_project_ids.push(project_id.clone());
        }

        let mut update = serde_json::json!({
            "activeMode": self.active_mode.element_slug(),
            "browserTabs": browser_tabs,
            "codeEditorProjectIds": code_editor_project_ids,
            "debuggingMode": settings_snapshot.debugging_mode(),
            "gxserverDaemon": gpui_titlebar_gxserver_daemon_status(),
            "portless": gpui_sidebar_portless_state_with_presentation(),
            "projectIsQuick": project_is_quick,
            "projectName": project_name,
            "projectPath": project_path,
            "resourceGroups": resource_groups,
            "showBetaFeatures": settings_snapshot.show_beta_features(),
            "sidebarTheme": gpui_app_modal_sidebar_theme_from_settings(settings_object),
            "webLinkOpenTarget": gpui_titlebar_web_link_open_target_from_settings(settings_object),
        });
        if let Some(project_id) = active_project_id {
            update["projectId"] = serde_json::json!(project_id);
        }
        update
    }

    pub(crate) fn gpui_titlebar_resources_resource_groups(
        &self,
        active_project_id: Option<&str>,
        project_name: &str,
        project_path: &str,
    ) -> Vec<serde_json::Value> {
        let now = SystemTime::now();
        let sessions = self
            .agents_workspace
            .terminal_sessions
            .iter()
            .map(|session| {
                let title = self.agents_workspace_tab_display_title(session.id);
                let mapped_key = self.local_workspace_session_mappings.iter().find_map(
                    |(key, shell_session_id)| (*shell_session_id == session.id).then_some(key),
                );
                let project_id = mapped_key
                    .map(|key| key.project_id.clone())
                    .or_else(|| active_project_id.map(str::to_string));
                let session_id = mapped_key
                    .map(|key| {
                        gpui_combined_presentation_session_id(&key.project_id, &key.session_id)
                    })
                    .unwrap_or_else(|| gpui_agents_session_external_id(session.id));
                let mut value = serde_json::json!({
                    "activity": session.activity.element_slug(),
                    "agentIcon": session.agent_icon,
                    "isLive": session.presentation_state == TerminalSessionPresentationState::Running,
                    "isRunning": session.presentation_state == TerminalSessionPresentationState::Running,
                    "isSleeping": session.presentation_state == TerminalSessionPresentationState::Sleeping,
                    "nativePaneState": gpui_titlebar_resources_native_pane_state(session.presentation_state),
                    "providerSessionState": gpui_titlebar_resources_provider_session_state(
                        session.zmx_session_name.as_deref(),
                    ),
                    "sessionId": session_id,
                    "sessionKind": "terminal",
                    "sessionPersistenceName": session.zmx_session_name.clone(),
                    "sessionPersistenceProvider": "zmx",
                    "terminalTitle": title.clone(),
                    "title": title,
                });
                if let Some(project_id) = project_id {
                    value["projectId"] = serde_json::json!(project_id);
                }
                if let Some(timer) = self.agents_delayed_send_timers.get(&session.id).copied() {
                    let remaining_ms = timer.remaining_ms(now);
                    value["delayedSendDeadlineAt"] =
                        serde_json::json!(gpui_iso8601_utc(timer.deadline_at));
                    value["delayedSendRemainingLabel"] =
                        serde_json::json!(gpui_command_delayed_send_countdown_label(remaining_ms));
                    value["delayedSendRemainingMs"] = serde_json::json!(remaining_ms);
                } else if let Some(watcher) = self
                    .agents_send_when_stopped_watchers
                    .get(&session.id)
                {
                    let is_working = self
                        .gpui_agents_send_when_stopped_scope_is_working(
                            session.id,
                            &watcher.scope,
                        )
                        .unwrap_or(false);
                    value["delayedSendRemainingLabel"] = serde_json::json!(
                        gpui_agents_send_when_stopped_remaining_label(
                            watcher,
                            is_working,
                            Instant::now(),
                        )
                    );
                }
                value
            })
            .collect::<Vec<_>>();

        /*
        CDXC:GPUIResourcesDevServers 2026-07-26:
        The mounted-pane group stays the authority for this window's live
        sessions (activity, pane state, delayed-send countdowns). Every other
        project, and every session of the active project that is not mounted
        here, comes from the cached presentation projection so Dev Servers can
        attribute listeners by project path exactly like macOS.
        */
        let mut presentation_groups = self.titlebar_resources_presentation_groups.clone();
        let active_presentation_group = active_project_id.and_then(|project_id| {
            presentation_groups
                .iter()
                .position(|group| {
                    group.get("projectId").and_then(serde_json::Value::as_str) == Some(project_id)
                })
                .map(|index| presentation_groups.remove(index))
        });
        let mut sessions = sessions;
        if let Some(group) = active_presentation_group.as_ref() {
            let mounted_session_ids = sessions
                .iter()
                .filter_map(|session| {
                    session
                        .get("sessionId")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string)
                })
                .collect::<HashSet<_>>();
            sessions.extend(
                group
                    .get("sessions")
                    .and_then(serde_json::Value::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or_default()
                    .iter()
                    .filter(|session| {
                        session
                            .get("sessionId")
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|session_id| !mounted_session_ids.contains(session_id))
                    })
                    .cloned(),
            );
        }

        if sessions.is_empty() && active_project_id.is_none() {
            return presentation_groups;
        }

        let group_id = active_project_id
            .map(gpui_combined_presentation_project_group_id)
            .unwrap_or_else(|| "gpui-agents".to_string());
        let project_path = if project_path.is_empty() {
            active_presentation_group
                .as_ref()
                .and_then(|group| group.get("projectPath").and_then(serde_json::Value::as_str))
                .unwrap_or_default()
        } else {
            project_path
        };
        let mut group = serde_json::json!({
            "groupId": group_id,
            "isActive": true,
            "projectName": project_name,
            "projectPath": project_path,
            "sessions": sessions,
            "title": project_name,
        });
        if let Some(project_id) = active_project_id {
            group["projectId"] = serde_json::json!(project_id);
        }
        let mut groups = vec![group];
        groups.extend(presentation_groups);
        groups
    }

    pub(crate) fn gpui_titlebar_resources_browser_tabs(
        &self,
        cx: &mut gpui::Context<Self>,
        active_project_id: Option<&str>,
    ) -> Vec<serde_json::Value> {
        let mut tabs = Vec::new();
        for tab in &self.browser_tabs.tabs {
            if tab.state != BrowserTabState::Loaded {
                continue;
            }
            let Some(surface) = self.browser_surfaces.get(&tab.id) else {
                continue;
            };
            let session_id = format!("gpui-browser:{}", tab.id.0);
            tabs.push(serde_json::json!({
                "browserId": surface.read(cx).browser_identifier(),
                "id": format!("browser:{session_id}"),
                "isActive": tab.id == self.browser_tabs.active_tab,
                "kind": "browser",
                "sessionId": session_id,
                "title": tab.display_title(),
                "url": tab.url.clone(),
            }));
        }

        for (slot_key, owned_surface) in &self.project_workarea_runtime_cef_surfaces {
            let Some(project_id) = active_project_id else {
                continue;
            };
            let mode = slot_key.titlebar_mode();
            tabs.push(serde_json::json!({
                "browserId": owned_surface.surface.read(cx).browser_identifier(),
                "id": format!("project-editor:{project_id}:{}", slot_key.privacy_label()),
                "isActive": self.active_mode == mode,
                "kind": gpui_titlebar_resources_project_editor_kind(*slot_key),
                "projectId": project_id,
                "title": mode.display_label(),
                "url": owned_surface.runtime_url.clone().into_cef_url(),
            }));
        }
        tabs
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
    }

    pub(crate) fn receive_gpui_titlebar_tips_unread_count_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(unread_count) = message
            .get("unreadCount")
            .and_then(serde_json::Value::as_u64)
            .filter(|count| *count <= TITLEBAR_TIP_IDS.len() as u64)
        else {
            return;
        };
        if self.titlebar_tips_unread_count == unread_count {
            return;
        }
        self.titlebar_tips_unread_count = unread_count;
        cx.notify();
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
        CDXC:AnonymousAnalytics 2026-08-26:
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
        // The launcher owns modal hydration. Session-scoped callers such as
        // Rename and Delayed Send must not diverge based on their entry point.
        if modal.requires_sidebar_state() {
            open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        }
        let quick_access_sidebar_state_message = matches!(
            modal,
            GpuiAppModalKind::CommandPalette
                | GpuiAppModalKind::PreviousSessions
                | GpuiAppModalKind::RecentProjects
                | GpuiAppModalKind::StashedPrompts
        )
        .then(|| sidebar_state_message.clone());
        self.open_gpui_app_modal_window_inner(
            modal,
            open_message,
            sidebar_state_message,
            source_window,
            true,
            cx,
        );
        if let Some(sidebar_state_message) = quick_access_sidebar_state_message
            && self.app_modal_window.is_some()
        {
            self.refresh_gpui_quick_access_sessions_state_in_background(sidebar_state_message, cx);
        }
    }

    /*
    CDXC:StashedPrompts 2026-08-08:
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
        CDXC:GPUISettingsModalHost 2026-06-24-10:58:
        Settings, Hotkeys, and Command Palette app-modal requests share this one GPUI-owned CEF window launcher so titlebar clicks and sidebar bridge messages cannot diverge into duplicate UI, temporary stubs, hidden overlays, or broad hit-test routing.

        CDXC:GPUIPreviousSessionsModal 2026-06-24-11:53:
        Previous Sessions app-modal requests share this launcher so titlebar/menu actions and modal-host open messages use one CEF window owner, while gxserver result messages remain transient sidebarState events owned by the command bridge.

        CDXC:GPUISettingsEntryModals 2026-06-24-12:22:
        Settings sub-entry modal ids must share this launcher so bridge opens, command-palette commands, and titlebar actions all hydrate the same shared Settings modal while letting the React host choose the initial tab from the modal id.

        CDXC:GPUIAgentsHubModal 2026-06-24-12:26:
        Agents Hub shares the launcher and receives the normal sidebar hydrate for settings-backed UI labels, but its filesystem catalog is not stored in that hydrate. The shared React Hub requests a fresh metadata catalog after open and then selected file content on demand through sidebarCommand.

        CDXC:GPUIAppModalReturnFocus 2026-06-25-22:13:
        Command-pane app modals need the same dismissal focus contract as native child windows. Capture only a runtime command group/session return target at modal open, then restore that exact command tab on close if it still exists; do not persist modal payloads, titles, command text, paths, URLs, stdout/stderr, or fallback to another command group.

        CDXC:GPUILoggingRemoval 2026-06-28-17:06:
        GPUI app-modal open/retry behavior stays functional, but runtime log writers and diagnostic breadcrumbs are intentionally removed until a future requirement adds a narrower diagnostics surface.
        */
        if reset_ready_retry {
            self.app_modal_ready_retry_used = false;
        }
        support_logs::append(
            support_logs::GpuiSupportLog::AppModal,
            "gpui.appModal.lifecycle",
            serde_json::json!({ "action": "open", "modal": modal.modal_id() }),
        );
        let window_size = modal.window_size();
        let window_title = self.gpui_app_modal_window_title(modal);
        let return_focus_target = gpui_app_modal_command_return_focus_target(
            modal,
            &open_message,
            self.shell_focus,
            &self.command_pane,
        );
        if let Some(handle) = self.app_modal_window.clone() {
            let window_configuration_matches = handle
                .update(cx, |host, _modal_window, _cx| {
                    host.current_modal.uses_react_modal_host() == modal.uses_react_modal_host()
                        && host.current_modal.is_resizable() == modal.is_resizable()
                        && (modal.uses_react_modal_host() || host.current_modal == modal)
                        && (modal.is_resizable()
                            || host.current_modal.window_size() == modal.window_size())
                })
                .unwrap_or(false);
            if !window_configuration_matches {
                /*
                CDXC:GPUIAppModalWindowSizing 2026-07-22:
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
                        cx,
                    );
                    modal_window.resize(window_size);
                    modal_window.set_window_title(&window_title);
                    modal_window.activate_window();
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
            window_bounds: Some(window_bounds),
            app_id: gpui_platform_window_app_id(),
            focus: true,
            icon: gpui_platform_window_icon(),
            show: true,
            is_resizable: modal.is_resizable(),
            window_min_size: Some(modal.window_min_size()),
            display_id: self.main_window_display_id,
            titlebar: Some(gpui::TitlebarOptions {
                title: Some(window_title.into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            ..Default::default()
        };
        let event_handler = self.app_modal_host_bridge_event_handler(cx);
        /*
        CDXC:GPUITutorialVideoFullscreen 2026-08-18:
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
        self.app_modal_window = cx
            .open_window(options, |modal_window, cx| {
                modal_window.activate_window();
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
        CDXC:GPUIAppModalReturnFocus 2026-06-25-22:25:
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
        CDXC:GPUIAppModalNativeClose 2026-07-12:
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

        self.app_modal_window = None;
        self.app_modal_window_id.set(None);
        self.app_modal_open_attempt_id = self.app_modal_open_attempt_id.wrapping_add(1);
        self.app_modal_ready_retry_used = false;
        self.restore_gpui_app_modal_command_return_focus_if_needed(cx);
        self.resume_deferred_gpui_portless_setup_prompt(cx);
        if closed_modal == Some(GpuiAppModalKind::FirstLaunchSetup) {
            self.complete_first_launch_setup();
        }
        if closed_modal == Some(GpuiAppModalKind::ExportTranscriptResult) {
            self.pending_export_transcript_reveal_path = None;
        }
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
        let modal = GpuiAppModalKind::FirstLaunchSetup;
        self.open_gpui_app_modal_window(
            modal,
            modal.open_message(),
            sidebar_state_message,
            None,
            cx,
        );
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
        CDXC:GPUIPortlessPromptDeferral 2026-08-18:
        A modal dismissed from React takes the window handle here instead of
        reaching the native-close path, so the deferred Portless prompt has to
        be resumed from this ownership boundary as well.
        */
        self.resume_deferred_gpui_portless_setup_prompt(cx);
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

        self.focus_command_pane();
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
        CDXC:GPUISettingsGhosttyConfig 2026-06-24-12:24:
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
        CDXC:GPUISettingsGxserverAgentPolicy 2026-06-24-11:39:
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
        CDXC:GPUISettingsGxserverAgentPolicy 2026-06-24-12:14:
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
        CDXC:GPUISettingsEntryModals 2026-06-24-12:22:
        Any shared Settings entry point can show agent-owned controls after the React host selects its initial tab. Reconcile gxserver-owned agent policy before hydrating Settings, Hotkeys, Configure Agents, Configure Actions, and Open Targets so entry-specific modal ids do not drift from the canonical Settings route.
        */
        if modal.is_settings_modal_entry() {
            self.reconcile_gpui_gxserver_agent_settings_in_background(cx);
        }
        self.gpui_app_modal_sidebar_state_message()
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
        /*
        CDXC:GPUICommandPane 2026-06-25-10:50:
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
        message
    }

    pub(crate) fn apply_gpui_gxserver_agent_settings_hydration_result(
        &mut self,
        hydration_result: GpuiGxserverAgentSettingsHydrationResult,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUISettingsGxserverAgentPolicy 2026-06-24-12:14:
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
        CDXC:GPUISettingsGxserverAgentPolicy 2026-06-24-11:39:
        gxserver read/update responses are canonical for inherited agent launch policy. If the daemon reports either agent setting differently than the current GPUI render cache, persist those canonical values through the central shared Settings service and refresh the modal/sidebar runtime state again instead of writing a separate cache or logging private daemon details.
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
        CDXC:GPUISettingsPersistence 2026-06-24-11:19:
        After a successful Settings save or gxserver startup/open canonical sync, GPUI refreshes only the settings-dependent runtime state it owns today: app-modal hydrate/sidebarState, sidebar debug/beta booleans through the existing CEF runtime-settings path, project-workarea CEF visibility, project-editor auto-sleep scheduling, supported embedded Ghostty request-map settings, gxserver-owned agent-policy reconciliation, and central-service render reads such as the Browser feedback/profile toolbar controls. This is not full settings fan-out; many action bridges, code-server sync, live Ghostty config reloads, and broad future side effects remain outside this path.
        */
        self.reschedule_project_editor_auto_sleep_if_policy_changed_from_shared_settings(
            settings_snapshot,
            cx,
        );
        self.apply_gpui_sidebar_side_from_saved_settings(settings_snapshot);
        self.apply_gpui_command_pane_side_from_saved_settings(settings_snapshot);
        refresh_gpui_visual_settings(settings_snapshot);
        self.refresh_sidebar_runtime_settings_from_shared_settings(settings_snapshot, cx);
        self.coerce_active_mode_to_available_project_context(cx);
        #[cfg(target_os = "macos")]
        self.refresh_terminal_ghostty_surface_config_requests_from_shared_settings(
            settings_snapshot,
        );
        self.reload_live_gpui_engine_terminal_config(cx);
        let sidebar_state_message =
            self.gpui_app_modal_sidebar_state_message_from_settings_snapshot(settings_snapshot);
        self.refresh_open_gpui_app_modal_sidebar_state(sidebar_state_message, cx);
        let chat_theme = gpui_session_chat_theme_from_settings(settings_snapshot.object());
        let chat_theme_literal =
            serde_json::to_string(chat_theme).unwrap_or_else(|_| "\"dark\"".to_string());
        let chat_theme_script =
            format!("window.ghostexSetSessionChatTheme?.({chat_theme_literal});undefined;");
        let chat_font_family =
            gpui_session_chat_font_family_from_settings(settings_snapshot.object());
        let chat_font_family_literal =
            serde_json::to_string(&chat_font_family).unwrap_or_else(|_| "\"\"".to_string());
        let chat_font_family_script = format!(
            "window.ghostexSetSessionChatFontFamily?.({chat_font_family_literal});undefined;"
        );
        let chat_custom_transcript_width_enabled =
            gpui_session_chat_custom_transcript_width_enabled_from_settings(
                settings_snapshot.object(),
            );
        let chat_custom_transcript_width_script = format!(
            "window.ghostexSetSessionChatCustomTranscriptWidthEnabled?.({chat_custom_transcript_width_enabled});undefined;"
        );
        let chat_transcript_width_percent =
            gpui_session_chat_transcript_width_percent_from_settings(settings_snapshot.object());
        let chat_transcript_width_script = format!(
            "window.ghostexSetSessionChatTranscriptWidthPercent?.({chat_transcript_width_percent});undefined;"
        );
        let chat_verbose_mode =
            gpui_session_chat_verbose_mode_from_settings(settings_snapshot.object());
        let chat_verbose_mode_script =
            format!("window.ghostexSetSessionChatVerboseMode?.({chat_verbose_mode});undefined;");
        for surface in self.agents_chat_surfaces.values() {
            surface.update(cx, |surface, _| {
                surface.execute_app_owned_script(&chat_theme_script);
                surface.execute_app_owned_script(&chat_font_family_script);
                surface.execute_app_owned_script(&chat_custom_transcript_width_script);
                surface.execute_app_owned_script(&chat_transcript_width_script);
                surface.execute_app_owned_script(&chat_verbose_mode_script);
            });
        }
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
        #[cfg(target_os = "macos")]
        if let Some(background) = shared_engine_settings.terminal_background_rgb {
            config.apply_terminal_background(background);
        }

        // This setting is app-owned and is not part of Ghostty's finalized
        // config string on macOS.
        config.view.scroll_to_bottom_when_typing =
            shared_engine_settings.scroll_to_bottom_when_typing;
        config.view.background_image =
            terminal_gpui_engine::terminal_background_image_from_settings(&shared_engine_settings);

        let confirm_close_behavior =
            terminal_gpui_engine::gpui_engine_confirm_close_behavior(&config);
        for record in self
            .agents_gpui_engine_terminals
            .values_mut()
            .chain(self.command_gpui_engine_terminals.values_mut())
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
                cx.notify();
            });
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn refresh_terminal_ghostty_surface_config_requests_from_shared_settings(
        &mut self,
        settings_snapshot: &shared_settings::SharedSidebarSettingsSnapshot,
    ) {
        /*
        CDXC:GPUITerminalSettings 2026-06-24-11:27:
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
        CDXC:GPUISettingsStatusBridge 2026-06-24-11:36:
        Settings status/action responses are transient `sidebarState` messages to the shared React modal host. They must clear modal loading states without replacing the stored full hydrate snapshot used when the app-modal host becomes ready or a Settings save rehydrates the modal.
        */
        let Some(handle) = self.app_modal_window.clone() else {
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

    pub(crate) fn dispatch_gpui_titlebar_tips_project_state_update(
        &mut self,
        project_state_update: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(panel) = self.titlebar_tips_panel.clone() else {
            return;
        };
        panel.update(cx, |panel, cx| {
            panel.dispatch_project_state_update(project_state_update.clone(), cx);
        });
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
        let Some(project_state_update) =
            gpui_titlebar_project_state_update_from_sidebar_state_payload(payload)
        else {
            return;
        };
        self.dispatch_gpui_titlebar_tips_project_state_update(project_state_update, cx);
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
    CDXC:GPUIRemoteNewTerminal 2026-08-20:
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
        CDXC:GPUICommandPaneQuitPersistence 2026-07-10:
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
        self.persist_shell_layout_state();
        self.stop_gpui_keep_awake_runtime();
        self.source_code_server_runtime.stop();
        let _ = cx;
    }
}
