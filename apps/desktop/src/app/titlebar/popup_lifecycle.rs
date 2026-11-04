// C1 wave-4 deferred split: apps/desktop/src/app/titlebar.rs (~3.9k lines)
// further divided into responsibility-scoped submodules, pure move (the
// only edit from the original app/titlebar.rs body is wrapping each group
// of `impl GhostexGpuiApp` methods in its own impl block; multiple impl
// blocks for the same type across files is the established pattern used by
// every sibling file in apps/desktop/src/app/). This file holds titlebar popup open/close state plumbing and generic popup content dispatch.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: titlebar menus, popups, actions, and titlebar render_* builders

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::AppContext as _;
use gpui::Bounds;
use gpui::Pixels;
use gpui::Window;
use gpui::WindowBackgroundAppearance;
use gpui::WindowBounds;
use gpui::WindowKind;
use gpui::WindowOptions;
use gpui_component::menu::PopupMenu;

use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    /// CDXC:Titlebar 2026-09-14 DECISION:
    /// User: clicking another dropdown's trigger closes the current dropdown and opens the clicked dropdown in the same click.
    pub(crate) fn set_gpui_titlebar_popup_open(
        &mut self,
        kind: GpuiTitlebarPopupKind,
        open: bool,
        trigger_bounds: Option<Bounds<Pixels>>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let _profile = crate::profiling::span(crate::profiling::Metric::PopupOpen);
        if !open {
            self.close_gpui_titlebar_popup(Some(kind), window, cx);
            return;
        }
        if self.titlebar_popup_menu_open(kind) {
            return;
        }

        if self.titlebar_extension_popup.is_some() {
            self.close_titlebar_extension_popup(window, cx);
        }
        self.close_gpui_titlebar_popup(None, window, cx);
        /*
        CDXC:Titlebar 2026-09-27 WHY:
        A dropdown can be asked to open before its trigger has painted (a hotkey, a ⋯ menu row, the
        very first frame), so its bounds are unknown. This used to call `request_animation_frame`,
        which is only valid while a view paints: from an event or a task it unwrapped an empty view
        stack and crashed the app, and even when it didn't the dropdown never opened. The request
        is now remembered and the trigger's own prepaint opens it at its real bounds
        (`open_pending_titlebar_popup`).
        */
        let Some(trigger_bounds) = trigger_bounds else {
            self.pending_titlebar_popup_open = Some(kind);
            cx.notify();
            return;
        };
        self.pending_titlebar_popup_open = None;

        let main_app = cx.entity().downgrade();
        let content_height = self.titlebar_popup_content_height(kind);
        let content_width = if kind == GpuiTitlebarPopupKind::ContextMenu {
            self.context_menu
                .as_ref()
                .expect("context menu supplied before opening")
                .content_width(window)
        } else {
            titlebar_popup_menu_width(kind)
        };
        let popup_bounds = titlebar_popup_window_bounds_for_trigger_bounds(
            kind,
            trigger_bounds,
            content_height,
            content_width,
            window,
        );
        let popup_height = popup_bounds.size.height.as_f32();
        // PopupMenu's bordered root owns this chrome outside its inner items column.
        let menu_width =
            (popup_bounds.size.width.as_f32() - TITLEBAR_POPUP_MENU_BORDER_CHROME).max(0.0);
        let menu_max_height = (popup_height - TITLEBAR_POPUP_MENU_BORDER_CHROME).max(0.0);
        /*
        PopupMenu's scroll handle includes its inner chrome. Creating that
        handle for a content-sized menu makes the trailing chrome a tiny,
        blank scroll range on some display scales. Only clipped menus own a
        scroll viewport; content-sized menus remain ordinary layout.
        */
        let menu_scrollable = matches!(
            kind,
            GpuiTitlebarPopupKind::Actions
                | GpuiTitlebarPopupKind::ContextMenu
                | GpuiTitlebarPopupKind::BrowserActions(_)
                | GpuiTitlebarPopupKind::BrowserDeviceView(_)
                | GpuiTitlebarPopupKind::Extensions
                | GpuiTitlebarPopupKind::Git
                | GpuiTitlebarPopupKind::Help
                | GpuiTitlebarPopupKind::More
                | GpuiTitlebarPopupKind::OpenTargets
        ) && content_height > popup_height;
        let content = self.build_gpui_titlebar_popup_content(
            kind,
            main_app.clone(),
            menu_width,
            menu_max_height,
            menu_scrollable,
            window,
            cx,
        );
        // `popup_bounds` is measured from this window's frame.
        let display_id = crate::app::window::popup_frame::PopupOwner::of(window, cx)
            .display_for(popup_bounds, cx);
        /*
        The dropdown is an exact, owned popup window that stays on the trigger
        window's display. macOS and Windows keep it non-activating so opening a
        dropdown does not deactivate the main window and trigger its outside-app
        dismissal observer. The popup root focuses PopupMenu internally, so its
        rows still dispatch typed actions without native window activation.
        Linux retains native focus for its existing popup input path.
        */
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(popup_bounds)),
            display_id,
            focus: !cfg!(any(target_os = "macos", target_os = "windows")),
            show: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            titlebar: None,
            // Under window glass the dropdown blurs what is behind it, rounded to its 8px panel.
            window_background: if window_glass_active() {
                WindowBackgroundAppearance::Blurred
            } else {
                WindowBackgroundAppearance::Transparent
            },
            ..Default::default()
        };
        let popup_window = match cx.open_window(options, {
            let content = content.clone();
            move |popup_window, cx| {
                popup_window.set_background_corner_radius(px(8.0));
                apply_frosted_menu_blur(popup_window);
                prepare_gpui_titlebar_popup_window_chrome(popup_window);
                if matches!(
                    kind,
                    GpuiTitlebarPopupKind::AccountUsage(_) | GpuiTitlebarPopupKind::Notifications
                ) {
                    crate::app::window::popup_frame::fade_in_popup_window(
                        popup_window,
                        cx.reduce_motion(),
                    );
                }
                GpuiTitlebarPopupWindow::new(main_app, kind, content, popup_window, cx)
            }
        }) {
            Ok(popup_window) => popup_window,
            Err(error) => {
                support_logs::append(
                    support_logs::GpuiSupportLog::HostLifecycle,
                    "gpui.titlebarPopup.openWindowError",
                    serde_json::json!({
                        "error": error.to_string(),
                        "kind": kind.diagnostic_label(),
                    }),
                );
                return;
            }
        };
        self.titlebar_popup_menu = Some(GpuiTitlebarPopupState {
            kind,
            trigger_bounds,
        });
        self.titlebar_popup_window = Some(popup_window);
        if kind == GpuiTitlebarPopupKind::Tips {
            self.request_gpui_titlebar_tips_runtime_status(cx);
        }
        cx.notify();
    }

    pub(crate) fn cancel_gpui_titlebar_popup(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(popup) = self.titlebar_popup_window
            && popup
                .update(cx, |popup, window, cx| {
                    popup.dismiss_account_reset_menu(window, cx)
                })
                .unwrap_or(false)
        {
            return;
        }
        self.close_gpui_titlebar_popup(None, window, cx);
    }

    /// Opens a dropdown that was asked for before its trigger had painted, now that the trigger at
    /// `trigger_bounds` has painted. `kinds` are the dropdowns this trigger anchors. Runs deferred
    /// from the trigger's prepaint, outside the paint.
    pub(crate) fn open_pending_titlebar_popup(
        app: gpui::WeakEntity<Self>,
        kinds: &[GpuiTitlebarPopupKind],
        trigger_bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        let Some(entity) = app.upgrade() else {
            return;
        };
        let Some(kind) = entity
            .read(cx)
            .pending_titlebar_popup_open
            .filter(|kind| kinds.contains(kind))
        else {
            return;
        };
        window.defer(cx, move |window, cx| {
            entity.update(cx, |app, cx| {
                if app.pending_titlebar_popup_open == Some(kind) {
                    app.set_gpui_titlebar_popup_open(kind, true, Some(trigger_bounds), window, cx);
                }
            });
        });
    }

    pub(crate) fn close_gpui_titlebar_popup(
        &mut self,
        kind: Option<GpuiTitlebarPopupKind>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if kind.is_none_or(|kind| self.pending_titlebar_popup_open == Some(kind)) {
            self.pending_titlebar_popup_open = None;
        }
        let should_close = self
            .titlebar_popup_menu
            .as_ref()
            .is_some_and(|state| kind.is_none_or(|kind| state.kind == kind));
        if !should_close {
            return;
        }

        if self
            .titlebar_popup_menu
            .as_ref()
            .is_some_and(|state| matches!(state.kind, GpuiTitlebarPopupKind::AccountUsage(_)))
        {
            self.restore_account_usage_keyboard_focus();
        }

        self.context_menu = None;
        self.titlebar_popup_menu = None;
        if let Some(popup_window) = self.titlebar_popup_window.take() {
            let _ = popup_window.update(cx, |_, popup_window, _| {
                popup_window.remove_window();
            });
        }
        if self
            .titlebar_dropdown_focus_handle
            .contains_focused(window, cx)
            && let Some(previous_focus_handle) = self.titlebar_dropdown_previous_focus_handle.take()
        {
            previous_focus_handle.focus(window, cx);
        } else {
            self.titlebar_dropdown_previous_focus_handle = None;
        }
        cx.notify();
    }

    pub(crate) fn clear_gpui_titlebar_popup_from_window(
        &mut self,
        kind: GpuiTitlebarPopupKind,
        cx: &mut gpui::Context<Self>,
    ) {
        let should_clear = self
            .titlebar_popup_menu
            .as_ref()
            .is_some_and(|state| state.kind == kind);
        if should_clear {
            if matches!(kind, GpuiTitlebarPopupKind::AccountUsage(_)) {
                self.restore_account_usage_keyboard_focus();
            }
            self.context_menu = None;
            self.titlebar_popup_menu = None;
            self.titlebar_popup_window = None;
            self.titlebar_dropdown_previous_focus_handle = None;
            cx.notify();
        }
    }

    /// CDXC:Git 2026-09-23 WHY:
    /// The Git dropdown is a separate native window. Notifying the main window leaves its menu frozen at the loading snapshot until it is reopened.
    pub(crate) fn refresh_open_git_popup(&self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if !self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::Git) {
            return;
        }
        let Some(popup) = self.titlebar_popup_window else {
            return;
        };
        let content_height = self.titlebar_git_popup_content_height();
        let bottom_limit = window.bounds().bottom() - px(8.0);
        let _ = popup.update(cx, |popup, window, cx| {
            let bounds = window.bounds();
            let height = px(content_height.min(TITLEBAR_POPUP_MENU_MAX_HEIGHT))
                .min((bottom_limit - bounds.top()).max(px(TITLEBAR_POPUP_MENU_ROW_HEIGHT)));
            let width = (bounds.size.width.as_f32() - TITLEBAR_POPUP_MENU_BORDER_CHROME).max(0.0);
            let max_height = (height.as_f32() - TITLEBAR_POPUP_MENU_BORDER_CHROME).max(0.0);
            let menu = PopupMenu::build(window, cx, |menu, _, _| {
                self.build_gpui_titlebar_git_popup_menu(
                    menu,
                    width,
                    max_height,
                    content_height > height.as_f32(),
                )
            });
            window.resize(size(bounds.size.width, height));
            popup.replace_menu(menu, window, cx);
        });
    }

    pub(crate) fn build_gpui_titlebar_popup_content(
        &self,
        kind: GpuiTitlebarPopupKind,
        main_app: gpui::WeakEntity<GhostexGpuiApp>,
        menu_width: f32,
        menu_max_height: f32,
        menu_scrollable: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> GpuiTitlebarPopupContent {
        let _profile = crate::profiling::span(crate::profiling::Metric::PopupBuild);
        match kind {
            GpuiTitlebarPopupKind::ContextMenu => {
                GpuiTitlebarPopupContent::Menu(PopupMenu::build(window, cx, |menu, _, _| {
                    self.context_menu
                        .as_ref()
                        .expect("context menu supplied before opening")
                        .build(menu, menu_width, menu_max_height, menu_scrollable)
                }))
            }
            GpuiTitlebarPopupKind::BrowserActions(pane_id) => {
                GpuiTitlebarPopupContent::Menu(PopupMenu::build(window, cx, |menu, _, _| {
                    self.build_gpui_browser_actions_popup_menu(
                        menu,
                        pane_id,
                        menu_width,
                        menu_max_height,
                        menu_scrollable,
                    )
                }))
            }
            GpuiTitlebarPopupKind::BrowserDeviceView(pane_id) => {
                GpuiTitlebarPopupContent::Menu(PopupMenu::build(window, cx, |menu, _, _| {
                    self.build_gpui_browser_device_view_popup_menu(
                        menu,
                        pane_id,
                        menu_width,
                        menu_max_height,
                        menu_scrollable,
                    )
                }))
            }
            GpuiTitlebarPopupKind::AccountUsage(id) => {
                let account = self
                    .titlebar_accounts
                    .iter()
                    .find(|account| account["titlebarKey"] == id.as_str())
                    .expect("account checked before opening usage popup")
                    .clone();
                GpuiTitlebarPopupContent::AccountUsage(cx.new(|cx| {
                    crate::app::window::account_usage::AccountUsagePanel::new(
                        main_app, id, account, cx,
                    )
                }))
            }
            GpuiTitlebarPopupKind::RemoteSites => {
                GpuiTitlebarPopupContent::RemoteSites(cx.new(|cx| {
                    crate::app::window::remote_sites::RemoteSitesPanel::new(main_app, cx)
                }))
            }
            GpuiTitlebarPopupKind::SessionLinks => {
                let key = self
                    .native_sidebar
                    .links_list_session
                    .clone()
                    .expect("session chosen before opening its links list");
                let links = self.session_links_of(&key);
                // The dropdown follows what the list shows, up to half the window like the card.
                let max_height = crate::app::native_sidebar::links_list::session_links_max_height(
                    window.bounds().size.height.as_f32(),
                )
                .min((window.bounds().size.height.as_f32() - 28.0).max(180.0));
                GpuiTitlebarPopupContent::SessionLinks(cx.new(|cx| {
                    crate::app::native_sidebar::links_list::SessionLinksList::new(
                        main_app,
                        key,
                        crate::app::native_sidebar::links_list::SessionLinksListHost::Dropdown,
                        links,
                        max_height,
                        cx,
                    )
                }))
            }
            GpuiTitlebarPopupKind::Actions => {
                GpuiTitlebarPopupContent::Menu(PopupMenu::build(window, cx, |menu, _, _| {
                    self.build_gpui_titlebar_actions_popup_menu(
                        menu,
                        menu_width,
                        menu_max_height,
                        menu_scrollable,
                    )
                }))
            }
            GpuiTitlebarPopupKind::Git => {
                GpuiTitlebarPopupContent::Menu(PopupMenu::build(window, cx, |menu, _, _| {
                    self.build_gpui_titlebar_git_popup_menu(
                        menu,
                        menu_width,
                        menu_max_height,
                        menu_scrollable,
                    )
                }))
            }
            GpuiTitlebarPopupKind::Help => {
                GpuiTitlebarPopupContent::Menu(PopupMenu::build(window, cx, |menu, _, _| {
                    super::help_menu::build_gpui_titlebar_help_popup_menu(
                        menu,
                        menu_width,
                        menu_max_height,
                        menu_scrollable,
                    )
                }))
            }
            GpuiTitlebarPopupKind::More => {
                GpuiTitlebarPopupContent::Menu(PopupMenu::build(window, cx, |menu, _, _| {
                    self.build_gpui_titlebar_more_popup_menu(
                        menu,
                        menu_width,
                        menu_max_height,
                        menu_scrollable,
                    )
                }))
            }
            GpuiTitlebarPopupKind::OpenTargets => {
                GpuiTitlebarPopupContent::Menu(PopupMenu::build(window, cx, |menu, _, _| {
                    self.build_gpui_open_targets_popup_menu(
                        menu,
                        menu_width,
                        menu_max_height,
                        menu_scrollable,
                    )
                }))
            }
            GpuiTitlebarPopupKind::Resources => {
                let snapshot = self.gpui_native_resources_snapshot(cx);
                GpuiTitlebarPopupContent::Reading(
                    cx.new(|_| GpuiTitlebarReadingPanel::resources(main_app, snapshot)),
                )
            }
            GpuiTitlebarPopupKind::Tips => {
                let live_agent_ids = self
                    .agents_workspace
                    .terminal_sessions
                    .iter()
                    .filter(|session| session.presentation_state.is_running())
                    .filter_map(|session| session.agent_icon)
                    .filter_map(gpui_default_sidebar_agent_by_icon)
                    .map(|agent| agent.agent_id.to_string())
                    .collect();
                GpuiTitlebarPopupContent::Reading(cx.new(|_| {
                    GpuiTitlebarReadingPanel::tips(
                        main_app,
                        self.titlebar_tips_cli_status.clone(),
                        self.titlebar_tips_agent_hook_status.clone(),
                        live_agent_ids,
                        self.titlebar_tips_sidebar_agent_ids.clone(),
                    )
                }))
            }
            GpuiTitlebarPopupKind::Notifications => {
                let feed = self.notification_feed_state.clone();
                GpuiTitlebarPopupContent::Reading(
                    cx.new(|_| GpuiTitlebarReadingPanel::notifications(main_app, feed)),
                )
            }
            GpuiTitlebarPopupKind::Extensions => {
                GpuiTitlebarPopupContent::Menu(PopupMenu::build(window, cx, |menu, _, _| {
                    self.build_gpui_titlebar_extensions_popup_menu(
                        menu,
                        menu_width,
                        menu_max_height,
                        menu_scrollable,
                    )
                }))
            }
        }
    }
}
