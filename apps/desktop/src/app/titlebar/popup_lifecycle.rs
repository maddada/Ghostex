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
    pub(crate) fn set_gpui_titlebar_popup_open(
        &mut self,
        kind: GpuiTitlebarPopupKind,
        open: bool,
        trigger_bounds: Option<Bounds<Pixels>>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        log_gpui_titlebar_popup_repro(
            "gpui.titlebarPopup.setOpenRequested",
            serde_json::json!({
                "currentKind": self
                    .titlebar_popup_menu
                    .as_ref()
                    .map(|state| state.kind.diagnostic_label()),
                "hasPopupWindowHandle": self.titlebar_popup_window.is_some(),
                "kind": kind.diagnostic_label(),
                "mainWindowActive": window.is_window_active(),
                "open": open,
                "triggerBounds": gpui_titlebar_popup_bounds_diagnostic(trigger_bounds),
            }),
        );
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
        let Some(trigger_bounds) = trigger_bounds else {
            log_gpui_titlebar_popup_repro(
                "gpui.titlebarPopup.anchorMissing",
                serde_json::json!({
                    "kind": kind.diagnostic_label(),
                    "mainWindowActive": window.is_window_active(),
                }),
            );
            window.request_animation_frame();
            return;
        };

        let main_app = cx.entity().downgrade();
        let content = self.build_gpui_titlebar_popup_content(kind, main_app.clone(), window, cx);
        let popup_bounds = titlebar_popup_window_bounds_for_trigger_bounds(
            kind,
            trigger_bounds,
            self.titlebar_popup_content_height(kind),
            window,
        );
        let display_id = window.display(cx).map(|display| display.id());
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
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        };
        log_gpui_titlebar_popup_repro(
            "gpui.titlebarPopup.openWindowAttempt",
            serde_json::json!({
                "focusRequested": !cfg!(any(target_os = "macos", target_os = "windows")),
                "kind": kind.diagnostic_label(),
                "mainWindowActive": window.is_window_active(),
                "popupBounds": gpui_titlebar_popup_bounds_diagnostic(Some(popup_bounds)),
                "triggerBounds": gpui_titlebar_popup_bounds_diagnostic(Some(trigger_bounds)),
            }),
        );
        let popup_window = match cx.open_window(options, {
            let content = content.clone();
            move |popup_window, cx| {
                prepare_gpui_titlebar_popup_window_chrome(popup_window);
                GpuiTitlebarPopupWindow::new(main_app, kind, content, popup_window, cx)
            }
        }) {
            Ok(popup_window) => popup_window,
            Err(error) => {
                log_gpui_titlebar_popup_repro(
                    "gpui.titlebarPopup.openWindowError",
                    serde_json::json!({
                        "error": error.to_string(),
                        "kind": kind.diagnostic_label(),
                        "mainWindowActive": window.is_window_active(),
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
        log_gpui_titlebar_popup_repro(
            "gpui.titlebarPopup.openWindowSucceeded",
            serde_json::json!({
                "kind": kind.diagnostic_label(),
                "mainWindowActive": window.is_window_active(),
            }),
        );
        if kind == GpuiTitlebarPopupKind::Tips {
            self.request_gpui_titlebar_tips_runtime_status(cx);
        }
        cx.notify();
    }

    pub(crate) fn close_gpui_titlebar_popup(
        &mut self,
        kind: Option<GpuiTitlebarPopupKind>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let current_kind = self
            .titlebar_popup_menu
            .as_ref()
            .map(|state| state.kind.diagnostic_label());
        let should_close = self
            .titlebar_popup_menu
            .as_ref()
            .is_some_and(|state| kind.is_none_or(|kind| state.kind == kind));
        log_gpui_titlebar_popup_repro(
            "gpui.titlebarPopup.closeRequested",
            serde_json::json!({
                "currentKind": current_kind,
                "hasPopupWindowHandle": self.titlebar_popup_window.is_some(),
                "mainWindowActive": window.is_window_active(),
                "requestedKind": kind.map(GpuiTitlebarPopupKind::diagnostic_label),
                "willClose": should_close,
            }),
        );
        if !should_close {
            return;
        }

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
        log_gpui_titlebar_popup_repro(
            "gpui.titlebarPopup.windowClearedState",
            serde_json::json!({
                "currentKind": self
                    .titlebar_popup_menu
                    .as_ref()
                    .map(|state| state.kind.diagnostic_label()),
                "kind": kind.diagnostic_label(),
                "willClear": should_clear,
            }),
        );
        if should_clear {
            self.titlebar_popup_menu = None;
            self.titlebar_popup_window = None;
            self.titlebar_dropdown_previous_focus_handle = None;
            cx.notify();
        }
    }

    pub(crate) fn build_gpui_titlebar_popup_content(
        &self,
        kind: GpuiTitlebarPopupKind,
        main_app: gpui::WeakEntity<GhostexGpuiApp>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> GpuiTitlebarPopupContent {
        match kind {
            GpuiTitlebarPopupKind::Actions => {
                GpuiTitlebarPopupContent::Menu(PopupMenu::build(window, cx, |menu, _, _| {
                    self.build_gpui_titlebar_actions_popup_menu(menu)
                }))
            }
            GpuiTitlebarPopupKind::Git => {
                GpuiTitlebarPopupContent::Menu(PopupMenu::build(window, cx, |menu, _, _| {
                    self.build_gpui_titlebar_git_popup_menu(menu)
                }))
            }
            GpuiTitlebarPopupKind::OpenTargets => {
                GpuiTitlebarPopupContent::Menu(PopupMenu::build(window, cx, |menu, _, _| {
                    self.build_gpui_open_targets_popup_menu(menu)
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
                    )
                }))
            }
            GpuiTitlebarPopupKind::Extensions => {
                GpuiTitlebarPopupContent::Menu(PopupMenu::build(window, cx, |menu, _, _| {
                    self.build_gpui_titlebar_extensions_popup_menu(menu)
                }))
            }
        }
    }
}
