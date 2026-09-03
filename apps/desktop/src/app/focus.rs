// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: shell focus, first-responder reconciliation, directional/spatial focus, leaf borders

use std::time::Duration;
use std::time::Instant;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::App;
use gpui::Focusable as _;
use gpui::Modifiers;
use gpui::MouseButton;
use gpui::Pixels;
use gpui::Point;
use gpui::PressureStage;
use gpui::ScrollDelta;
use gpui::Window;

use crate::app::actions::*;
use crate::app::consts::*;
use crate::app::ffi::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;
impl GhostexGpuiApp {
    pub(crate) fn set_shell_focus(&mut self, focus: ShellFocusTarget) {
        self.set_shell_focus_with_terminal_handoff(focus, false);
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn begin_programmatic_focus(&mut self) {
        self.programmatic_focus_depth = self.programmatic_focus_depth.saturating_add(1);
        let root_key = self.parent_ns_view as usize;
        GPUI_FIRST_RESPONDER_PROGRAMMATIC_DEPTHS.with(|depths| {
            let mut depths = depths.borrow_mut();
            let depth = depths.entry(root_key).or_default();
            *depth = depth.saturating_add(1);
        });
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn end_programmatic_focus(&mut self) {
        self.programmatic_focus_depth = self.programmatic_focus_depth.saturating_sub(1);
        let root_key = self.parent_ns_view as usize;
        GPUI_FIRST_RESPONDER_PROGRAMMATIC_DEPTHS.with(|depths| {
            let mut depths = depths.borrow_mut();
            let Some(depth) = depths.get_mut(&root_key) else {
                return;
            };
            *depth = depth.saturating_sub(1);
            if *depth == 0 {
                depths.remove(&root_key);
            }
        });
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn receive_first_responder_transition(
        &mut self,
        responder: *mut std::ffi::c_void,
        suppressed_by_programmatic_focus: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let target = self.classify_first_responder_target(responder, cx);
        update_gpui_keyboard_router_first_responder(self.parent_ns_view, target);
        /*
        CDXC:Titlebar 2026-07-15:
        Native CEF child views bypass the main GPUI root's outside-click
        capture. Their AppKit mouseDown hook reports the current responder on
        every click, including repeated clicks in an already-focused pane, so
        close any native titlebar popup at this shared CEF boundary. Anchored
        extension popups close at the same boundary unless the click belongs
        to their own CEF surface. Programmatic focus handoffs stay excluded,
        and popup dismissal never changes, reroutes, or synthesizes the
        Chromium mouse event.
        */
        if !suppressed_by_programmatic_focus
            && matches!(target, FirstResponderTarget::CefSurface(_))
        {
            if self.titlebar_popup_menu.is_some() {
                self.close_gpui_titlebar_popup(None, window, cx);
            }
            if self.titlebar_extension_popup.is_some()
                && !matches!(
                    target,
                    FirstResponderTarget::CefSurface(
                        FirstResponderCefSurface::TitlebarExtensionPopup
                    )
                )
            {
                self.close_titlebar_extension_popup(window, cx);
            }
        }
        // Temporary input-stealing diagnosis (2026-07-09): record every raw
        // AppKit first-responder transition so the moment typing dies can be
        // matched to whichever surface took (or dropped) key focus.
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.terminalFocus.firstResponderTransition",
            serde_json::json!({
                "target": format!("{:?}", target),
                "previous": format!("{:?}", self.first_responder_target),
                "suppressedByProgrammaticFocus": suppressed_by_programmatic_focus,
                "responderIsNull": responder.is_null(),
            }),
        );
        if self.first_responder_target == target
            && self.first_responder_transition_suppressed_by_programmatic_focus
                == suppressed_by_programmatic_focus
        {
            if !suppressed_by_programmatic_focus {
                self.reconcile_project_workarea_cef_keyboard_ownership(window, cx);
                self.reconcile_browser_cef_keyboard_ownership(window, cx);
                self.reconcile_session_chat_cef_keyboard_ownership(window, cx);
                if self.reconcile_shell_focus_with_first_responder_target() {
                    self.persist_shell_layout_state();
                    cx.notify();
                }
            }
            return;
        }
        self.first_responder_target = target;
        self.first_responder_transition_suppressed_by_programmatic_focus =
            suppressed_by_programmatic_focus;
        self.reconcile_project_workarea_cef_keyboard_ownership(window, cx);
        self.reconcile_browser_cef_keyboard_ownership(window, cx);
        self.reconcile_session_chat_cef_keyboard_ownership(window, cx);
        if !suppressed_by_programmatic_focus {
            if self.reconcile_shell_focus_with_first_responder_target() {
                self.persist_shell_layout_state();
            }
            self.reconcile_sidebar_focus_border_handoff_after_responder_transition();
        }
        cx.notify();
    }

    pub(crate) fn reconcile_shell_focus_with_first_responder_target(&mut self) -> bool {
        /*
        Native CEF and terminal child views receive mouse input before their
        GPUI layout parent, so the parent on_mouse_down handler is not a
        reliable focus boundary. AppKit's first responder is the authoritative
        owner for those embedded surfaces. Keep the shell model in sync here
        so keyboard ownership, focused-pane commands, and the visible 1px
        focus outline all describe the same pane.
        */
        let previous_focus = self.shell_focus;
        let previous_browser_pane = self.browser_tabs.focused_pane;
        let previous_browser_tab = self.browser_tabs.active_tab;
        let previous_companion_slot = self.project_editor_companion_focused_terminal_slot;

        match self.first_responder_target {
            FirstResponderTarget::CefSurface(FirstResponderCefSurface::SessionChat(session_id))
                if self.active_mode == TitlebarMode::Agents =>
            {
                let Some(pane_id) = self.agents_workspace.pane_id_for_session(session_id) else {
                    return false;
                };
                if self.agents_workspace.active_session_in_pane(pane_id) != Some(session_id) {
                    return false;
                }
                self.agents_workspace.focus_pane(pane_id);
                self.set_shell_focus(ShellFocusTarget::AgentsPane(pane_id));
            }
            FirstResponderTarget::CefSurface(FirstResponderCefSurface::SessionChat(session_id))
                if self.active_mode.is_project_editor_mode()
                    && self.project_editor_shell.left_companion_visible =>
            {
                if self.project_editor_companion_terminal_session_id == Some(session_id) {
                    self.project_editor_companion_focused_terminal_slot =
                        ProjectEditorCompanionTerminalSlot::Top;
                } else if self.project_editor_companion_secondary_terminal_session_id
                    == Some(session_id)
                {
                    self.project_editor_companion_focused_terminal_slot =
                        ProjectEditorCompanionTerminalSlot::Bottom;
                } else {
                    return false;
                }
                self.set_shell_focus(ShellFocusTarget::ProjectEditorCompanion(self.active_mode));
            }
            FirstResponderTarget::CefSurface(FirstResponderCefSurface::BrowserTab(tab_id))
                if self.active_mode == TitlebarMode::Browser =>
            {
                let Some(pane_id) = find_browser_leaf_id_for_tab(&self.browser_tabs.root, tab_id)
                else {
                    return false;
                };
                if !self.browser_tabs.select_tab_in_pane(pane_id, tab_id) {
                    return false;
                }
                self.set_shell_focus(ShellFocusTarget::BrowserPane(pane_id));
            }
            FirstResponderTarget::CefSurface(FirstResponderCefSurface::ProjectWorkarea(
                slot_key,
            )) if self.active_mode == slot_key.titlebar_mode() => {
                self.set_shell_focus(ShellFocusTarget::ProjectEditorSurface(
                    slot_key.titlebar_mode(),
                ));
            }
            FirstResponderTarget::CefSurface(FirstResponderCefSurface::ProjectEditorCompanion)
                if self.active_mode.is_project_editor_mode()
                    && self.project_editor_shell.left_companion_visible =>
            {
                self.set_shell_focus(ShellFocusTarget::ProjectEditorCompanion(self.active_mode));
            }
            FirstResponderTarget::TerminalSurface(
                FirstResponderTerminalSurface::ProjectEditorCompanion(session_id),
            ) if self.active_mode.is_project_editor_mode()
                && self.project_editor_shell.left_companion_visible =>
            {
                if self.project_editor_companion_terminal_session_id == Some(session_id) {
                    self.project_editor_companion_focused_terminal_slot =
                        ProjectEditorCompanionTerminalSlot::Top;
                } else if self.project_editor_companion_secondary_terminal_session_id
                    == Some(session_id)
                {
                    self.project_editor_companion_focused_terminal_slot =
                        ProjectEditorCompanionTerminalSlot::Bottom;
                } else {
                    return false;
                }
                self.set_shell_focus(ShellFocusTarget::ProjectEditorCompanion(self.active_mode));
            }
            _ => return false,
        }

        self.shell_focus != previous_focus
            || self.browser_tabs.focused_pane != previous_browser_pane
            || self.browser_tabs.active_tab != previous_browser_tab
            || self.project_editor_companion_focused_terminal_slot != previous_companion_slot
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn source_workarea_cef_owns_native_focus(&self) -> bool {
        matches!(
            self.first_responder_target,
            FirstResponderTarget::CefSurface(FirstResponderCefSurface::ProjectWorkarea(
                ProjectWorkareaCefSurfaceSlotKey::Source
            ))
        )
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn source_workarea_cef_owns_native_focus(&self) -> bool {
        false
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn reconcile_project_workarea_cef_keyboard_ownership(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let source_cef_owns_focus = self.source_workarea_cef_owns_native_focus();
        let renderer_edit_hotkeys_own_focus = matches!(
            self.first_responder_target,
            FirstResponderTarget::CefSurface(FirstResponderCefSurface::ProjectWorkarea(
                ProjectWorkareaCefSurfaceSlotKey::Source | ProjectWorkareaCefSurfaceSlotKey::Manage
            )) | FirstResponderTarget::CefSurface(FirstResponderCefSurface::SessionChat(_))
        );
        let source_menu_changed =
            self.source_workarea_cef_menu_passthrough_active != source_cef_owns_focus;
        if self.source_workarea_cef_menu_passthrough_active != source_cef_owns_focus {
            self.source_workarea_cef_menu_passthrough_active = source_cef_owns_focus;
            set_ghostex_gpui_main_menus(source_cef_owns_focus, cx);
        }
        if self.renderer_edit_hotkey_passthrough_active != renderer_edit_hotkeys_own_focus {
            self.renderer_edit_hotkey_passthrough_active = renderer_edit_hotkeys_own_focus;
            if !source_menu_changed {
                cef::refresh_application_menu_hooks();
            }
        }
        if let FirstResponderTarget::CefSurface(FirstResponderCefSurface::ProjectWorkarea(
            slot_key,
        )) = self.first_responder_target
        {
            self.focus_project_workarea_cef_gpui_handle(slot_key, window, cx);
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn focus_project_workarea_cef_gpui_handle(
        &mut self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:FocusRouting 2026-08-09:
        Project-workarea CEF clicks can arrive through Chromium's AppKit NSView
        subclass before GPUI's normal mouse hitbox focuses the CefSurface
        handle. When the native first responder proves a workarea CEF view
        owns keyboard focus, move GPUI focus to that same CefSurface handle
        so propagated chords walk the CEF key-context path instead of a
        stale companion-terminal element. Returning to the terminal uses the
        existing terminal click/focus routes, which restore the terminal
        handles before their key listeners run.
        */
        let Some(surface) = self
            .project_workarea_runtime_cef_surfaces
            .get(&slot_key)
            .map(|owned_surface| owned_surface.surface.clone())
        else {
            return;
        };
        let focus_handle = surface.read(cx).focus_handle.clone();
        if !focus_handle.is_focused(window) {
            focus_handle.focus(window, cx);
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn reconcile_browser_cef_keyboard_ownership(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:FocusRouting 2026-07-28:
        Browser page clicks reach Chromium's AppKit NSView directly, so the
        CEF child becomes native first responder while GPUI's own focus stays
        on whatever chrome held it last — usually the pane's address input.
        AppKit routes Cmd chords and Function-flagged keys (arrows, Home/End,
        forward-delete) through the window-wide performKeyEquivalent pass,
        which reaches the GPUI root view regardless of first responder, and
        GPUI resolves them against that stale internal focus: the address
        input consumed Up/Down/Cmd+Z meant for the focused page and kept
        rendering its caret. Mirror the Source-workarea rule: when the native
        first responder proves a Browser CEF view owns keyboard focus, move
        GPUI focus to that surface's handle so the address input blurs and
        equivalents walk the CEF key context, which claims nothing and lets
        AppKit continue to the Chromium responder. Terminal and address-bar
        clicks restore their own handles through their existing click routes.
        */
        let FirstResponderTarget::CefSurface(FirstResponderCefSurface::BrowserTab(tab_id)) =
            self.first_responder_target
        else {
            return;
        };
        if self.active_mode != TitlebarMode::Browser {
            return;
        }
        let Some(surface) = self.browser_surfaces.get(&tab_id).cloned() else {
            return;
        };
        let focus_handle = surface.read(cx).focus_handle.clone();
        if !focus_handle.is_focused(window) {
            focus_handle.focus(window, cx);
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn reconcile_session_chat_cef_keyboard_ownership(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:FocusRouting 2026-08-17:
        Chat body clicks reach Chromium before GPUI's hitbox, so AppKit can
        focus Chat while GPUI remains focused on the command terminal. Mirror
        the other CEF surfaces by synchronizing the existing GPUI handle once
        the registered Chat view owns native focus. Do not force the composer;
        the original Chromium click target remains unchanged.
        */
        let FirstResponderTarget::CefSurface(FirstResponderCefSurface::SessionChat(session_id)) =
            self.first_responder_target
        else {
            return;
        };
        // CDXC:PromptSearch 2026-08-20: the SessionChat responder class
        // means "a CEF surface owns this session's pane", which is true of the
        // Find surface too. Resolve whichever one is mounted.
        let Some(surface) = self.agents_pane_cef_surface(session_id) else {
            return;
        };
        let focus_handle = surface.read(cx).focus_handle.clone();
        if !focus_handle.is_focused(window) {
            focus_handle.focus(window, cx);
        }
    }

    pub(crate) fn propagate_source_workarea_cef_hotkey_passthrough(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:Hotkeys 2026-07-05:
        The Source workarea hosts embedded VSCode inside a CEF NSView. When that
        CEF view is AppKit's first responder, VSCode-owned editing chords
        must reach code-server. The GPUI binding leg calls `cx.propagate()`
        so gpui_macos `handle_key_event` returns NO from the window
        `performKeyEquivalent:` path, leaving AppKit free to continue normal
        first-responder delivery to the CEF view. The menu leg is handled at
        the same native-focus transition by reinstalling the app menu with
        non-allowlisted Ghostex key equivalents stripped; otherwise
        `[NSApp mainMenu] performKeyEquivalent:` consumes menu-backed chords
        such as Cmd-W before the CEF responder can see them. Workarea-switch
        escape hatches and app-reserved quit/hide/minimize actions
        intentionally do not use this gate.
        */
        if !self.source_workarea_cef_owns_native_focus() {
            return false;
        }
        cx.propagate();
        true
    }

    pub(crate) fn propagate_renderer_edit_cef_hotkey_passthrough(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        #[cfg(target_os = "macos")]
        let renderer_edit_cef_owns_native_focus = matches!(
            self.first_responder_target,
            FirstResponderTarget::CefSurface(FirstResponderCefSurface::ProjectWorkarea(
                ProjectWorkareaCefSurfaceSlotKey::Source | ProjectWorkareaCefSurfaceSlotKey::Manage
            )) | FirstResponderTarget::CefSurface(FirstResponderCefSurface::SessionChat(_))
        );
        #[cfg(not(target_os = "macos"))]
        let renderer_edit_cef_owns_native_focus = false;

        if !renderer_edit_cef_owns_native_focus {
            return false;
        }
        cx.propagate();
        true
    }

    pub(crate) fn propagate_source_workarea_cef_configured_hotkey_passthrough(
        &self,
        action_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.source_workarea_cef_owns_native_focus()
            || gpui_source_workarea_allowed_configured_hotkey_action_id(action_id)
        {
            return false;
        }
        cx.propagate();
        true
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn classify_first_responder_target(
        &self,
        responder: *mut std::ffi::c_void,
        cx: &mut gpui::Context<Self>,
    ) -> FirstResponderTarget {
        if responder.is_null() {
            return FirstResponderTarget::None;
        }

        if let Some(session_id) = self.agents_terminal_session_id_containing_responder(responder) {
            return FirstResponderTarget::TerminalSurface(FirstResponderTerminalSurface::Agents(
                session_id,
            ));
        }
        if let Some(session_id) = self.command_terminal_session_id_containing_responder(responder) {
            return FirstResponderTarget::TerminalSurface(FirstResponderTerminalSurface::Command(
                session_id,
            ));
        }
        if let Some(session_id) =
            self.project_editor_companion_terminal_session_id_containing_responder(responder)
        {
            return FirstResponderTarget::TerminalSurface(
                FirstResponderTerminalSurface::ProjectEditorCompanion(session_id),
            );
        }
        if let Some(surface) = self.cef_surface_containing_responder(responder, cx) {
            return FirstResponderTarget::CefSurface(surface);
        }
        if cef::native_view_contains_responder(self.parent_ns_view, responder) {
            return FirstResponderTarget::GpuiWindow;
        }
        FirstResponderTarget::Other
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn agents_terminal_session_id_containing_responder(
        &self,
        responder: *mut std::ffi::c_void,
    ) -> Option<TerminalSessionId> {
        self.agents_terminal_host_native_views
            .iter()
            .find_map(|(slot_id, host_view)| {
                terminal_native_view::app_owned_terminal_host_contains_responder(
                    host_view, responder,
                )
                .then_some(slot_id.session_id)
            })
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn command_terminal_session_id_containing_responder(
        &self,
        responder: *mut std::ffi::c_void,
    ) -> Option<CommandSessionId> {
        self.command_terminal_host_native_views
            .iter()
            .find_map(|(slot_id, host_view)| {
                terminal_native_view::app_owned_terminal_host_contains_responder(
                    host_view, responder,
                )
                .then_some(slot_id.session_id)
            })
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn project_editor_companion_terminal_session_id_containing_responder(
        &self,
        responder: *mut std::ffi::c_void,
    ) -> Option<TerminalSessionId> {
        self.project_editor_companion_terminal_host_native_views
            .iter()
            .find_map(|(slot_id, host_view)| {
                terminal_native_view::app_owned_terminal_host_contains_responder(
                    host_view, responder,
                )
                .then_some(slot_id.session_id)
            })
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn cef_surface_containing_responder(
        &self,
        responder: *mut std::ffi::c_void,
        cx: &mut gpui::Context<Self>,
    ) -> Option<FirstResponderCefSurface> {
        if self
            .sidebar
            .as_ref()
            .is_some_and(|surface| surface.read(cx).native_view_contains_responder(responder))
        {
            return Some(FirstResponderCefSurface::Sidebar);
        }
        if let Some(tab_id) = self.browser_surfaces.iter().find_map(|(tab_id, surface)| {
            surface
                .read(cx)
                .native_view_contains_responder(responder)
                .then_some(*tab_id)
        }) {
            return Some(FirstResponderCefSurface::BrowserTab(tab_id));
        }
        if let Some(slot_key) = self.project_workarea_runtime_cef_surfaces.iter().find_map(
            |(slot_key, owned_surface)| {
                owned_surface
                    .surface
                    .read(cx)
                    .native_view_contains_responder(responder)
                    .then_some(*slot_key)
            },
        ) {
            return Some(FirstResponderCefSurface::ProjectWorkarea(slot_key));
        }
        if let Some(session_id) =
            self.agents_chat_surfaces
                .iter()
                .find_map(|(session_id, surface)| {
                    surface
                        .read(cx)
                        .native_view_contains_responder(responder)
                        .then_some(*session_id)
                })
        {
            /*
            CDXC:SessionChat 2026-07-31:
            The chat pane is a first-class work surface: classifying its
            responder keeps focus arbitration from reclaiming keyboard focus
            while the user types in the chat composer.
            */
            return Some(FirstResponderCefSurface::SessionChat(session_id));
        }
        if self.titlebar_tips_panel.as_ref().is_some_and(|panel| {
            panel
                .read(cx)
                .surface
                .read(cx)
                .native_view_contains_responder(responder)
        }) {
            return Some(FirstResponderCefSurface::TitlebarTips);
        }
        if self
            .titlebar_extension_popup
            .as_ref()
            .and_then(|state| state.panel.as_ref())
            .is_some_and(|panel| {
                panel
                    .read(cx)
                    .surface
                    .read(cx)
                    .native_view_contains_responder(responder)
            })
        {
            return Some(FirstResponderCefSurface::TitlebarExtensionPopup);
        }
        if let Some(handle) = self.app_modal_window.clone() {
            if handle
                .update(cx, |host, _window, cx| {
                    host.surface.as_ref().is_some_and(|surface| {
                        surface.read(cx).native_view_contains_responder(responder)
                    })
                })
                .unwrap_or(false)
            {
                return Some(FirstResponderCefSurface::AppModal);
            }
        }
        None
    }

    pub(crate) fn set_shell_focus_with_terminal_handoff(
        &mut self,
        focus: ShellFocusTarget,
        force_terminal_appkit_focus_handoff: bool,
    ) {
        // Temporary input-stealing diagnosis (2026-07-09): record every shell
        // focus write so responder churn can be attributed to its caller path.
        if self.shell_focus != focus || force_terminal_appkit_focus_handoff {
            support_logs::append(
                support_logs::GpuiSupportLog::TerminalFocus,
                "gpui.terminalFocus.shellFocusSet",
                serde_json::json!({
                    "focus": format!("{:?}", focus),
                    "previous": format!("{:?}", self.shell_focus),
                    "forceHandoff": force_terminal_appkit_focus_handoff,
                }),
            );
        }
        self.shell_focus = focus;
        if let Some(focus) = valid_non_command_shell_focus_with_browser_tabs(
            focus,
            self.active_mode,
            &self.agents_workspace,
            &self.project_editor_shell,
            &self.browser_tabs,
        ) {
            self.previous_non_command_focus = Some(focus);
        }
        #[cfg(target_os = "macos")]
        self.begin_programmatic_focus();
        #[cfg(target_os = "macos")]
        {
            self.sync_agents_terminal_ghostty_surface_focus_with_appkit_handoff(
                force_terminal_appkit_focus_handoff,
            );
            self.sync_command_terminal_ghostty_surface_focus_with_appkit_handoff(
                force_terminal_appkit_focus_handoff,
            );
            self.sync_project_editor_companion_terminal_ghostty_surface_focus_with_appkit_handoff(
                force_terminal_appkit_focus_handoff,
            );
            self.end_programmatic_focus();
        }
        self.clear_pending_agents_terminal_text_focus_if_focus_moved();
        self.clear_pending_command_terminal_text_focus_if_focus_moved();
        self.clear_pending_project_editor_companion_terminal_text_focus_if_focus_moved();
    }

    pub(crate) fn begin_sidebar_focus_border_handoff(&mut self, cx: &mut gpui::Context<Self>) {
        let started_at = Instant::now();
        self.sidebar_focus_border_handoff = Some(SidebarFocusBorderHandoff {
            held_pane_id: self.agents_workspace.focused_pane,
            target_session_id: None,
            started_at,
        });

        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(SIDEBAR_FOCUS_BORDER_HANDOFF_TIMEOUT)
                .await;
            let _ = this.update(cx, |this, cx| {
                if this
                    .sidebar_focus_border_handoff
                    .is_some_and(|handoff| handoff.started_at == started_at)
                {
                    this.cancel_sidebar_focus_border_handoff();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(crate) fn set_sidebar_focus_border_handoff_target(
        &mut self,
        session_id: TerminalSessionId,
    ) {
        if let Some(handoff) = self.sidebar_focus_border_handoff.as_mut() {
            handoff.target_session_id = Some(session_id);
        }
        self.complete_sidebar_focus_border_handoff_if_target_focused();
    }

    pub(crate) fn complete_sidebar_focus_border_handoff_if_target_focused(&mut self) {
        let Some(target_session_id) = self
            .sidebar_focus_border_handoff
            .and_then(|handoff| handoff.target_session_id)
        else {
            return;
        };
        if self.first_responder_target
            == FirstResponderTarget::TerminalSurface(FirstResponderTerminalSurface::Agents(
                target_session_id,
            ))
            || self.first_responder_target == FirstResponderTarget::GpuiWindow
        {
            self.cancel_sidebar_focus_border_handoff();
        }
    }

    pub(crate) fn cancel_sidebar_focus_border_handoff(&mut self) {
        self.sidebar_focus_border_handoff = None;
    }

    pub(crate) fn reconcile_sidebar_focus_border_handoff_after_responder_transition(&mut self) {
        if self.sidebar_focus_border_handoff.is_none() {
            return;
        }
        self.complete_sidebar_focus_border_handoff_if_target_focused();
        if self.sidebar_focus_border_handoff.is_none() {
            return;
        }
        if !matches!(
            self.first_responder_target,
            FirstResponderTarget::CefSurface(FirstResponderCefSurface::Sidebar)
        ) {
            self.cancel_sidebar_focus_border_handoff();
        }
    }

    pub(crate) fn sidebar_focus_border_handoff_holds_pane(&self, pane_id: WorkspacePaneId) -> bool {
        let Some(handoff) = self.sidebar_focus_border_handoff else {
            return false;
        };
        handoff.held_pane_id == pane_id
            && handoff.started_at.elapsed() < SIDEBAR_FOCUS_BORDER_HANDOFF_TIMEOUT
            && matches!(
                self.first_responder_target,
                FirstResponderTarget::CefSurface(FirstResponderCefSurface::Sidebar)
            )
    }

    pub(crate) fn workspace_leaf_border_state(
        &self,
        leaf: &WorkspaceLeaf,
        window: &Window,
        cx: &App,
    ) -> WorkspacePaneBorderState {
        if self
            .agents_workspace
            .active_session_in_pane_has_attention(leaf.pane_id)
        {
            return WorkspacePaneBorderState::Attention;
        }
        if !window.is_window_active() {
            return WorkspacePaneBorderState::Neutral;
        }
        if self.sidebar_focus_border_handoff_holds_pane(leaf.pane_id) {
            return WorkspacePaneBorderState::Focused;
        }
        if self.should_show_focused_agents_leaf_border(leaf, window, cx) {
            WorkspacePaneBorderState::Focused
        } else {
            WorkspacePaneBorderState::Neutral
        }
    }

    pub(crate) fn should_show_focused_agents_leaf_border(
        &self,
        leaf: &WorkspaceLeaf,
        window: &Window,
        cx: &App,
    ) -> bool {
        if self.agents_workspace.focused_pane != leaf.pane_id
            || self.shell_focus != ShellFocusTarget::AgentsPane(leaf.pane_id)
            || !window.is_window_active()
        {
            return false;
        }
        let Some(session_id) = leaf.tab_group.active_session_id() else {
            return false;
        };
        let Some(session) = self.agents_workspace.session(session_id) else {
            return false;
        };
        let active_session_is_in_chat_view = self.agents_chat_mode_sessions.contains(&session_id);
        #[cfg(target_os = "windows")]
        {
            /*
            Windows has no AppKit-style first-responder observer, so
            `first_responder_target` intentionally remains unset there. The
            composited terminal focus handoff instead gives native keyboard
            focus directly to the GPUI root HWND and focuses the exact
            terminal view's FocusHandle. Require both sources so the active
            pane gets macOS-parity chrome while a focused CEF child does not
            leave a stale terminal border behind.
            */
            if active_session_is_in_chat_view {
                return true;
            }
            if session.presentation_state != TerminalSessionPresentationState::Running {
                return cef::gpui_root_view_has_native_focus(self.parent_ns_view);
            }
            return cef::gpui_root_view_has_native_focus(self.parent_ns_view)
                && self
                    .agents_gpui_engine_terminals
                    .get(&session_id)
                    .is_some_and(|record| {
                        record.view.read(cx).focus_handle(cx).is_focused(window)
                    });
        }
        #[cfg(not(target_os = "windows"))]
        match self.first_responder_target {
            FirstResponderTarget::TerminalSurface(FirstResponderTerminalSurface::Agents(
                responder_session_id,
            )) => responder_session_id == session_id,
            FirstResponderTarget::CefSurface(FirstResponderCefSurface::SessionChat(
                responder_session_id,
            )) => active_session_is_in_chat_view && responder_session_id == session_id,
            FirstResponderTarget::GpuiWindow => {
                if active_session_is_in_chat_view {
                    return true;
                }
                if session.presentation_state != TerminalSessionPresentationState::Running {
                    return true;
                }
                let slot_id = AgentsTerminalBodyMountSlotId {
                    pane_id: leaf.pane_id,
                    session_id,
                };
                if self
                    .agents_gpui_engine_terminals
                    .get(&session_id)
                    .is_some_and(|record| record.view.read(cx).focus_handle(cx).is_focused(window))
                {
                    return true;
                }
                self.terminal_text_focus_handle.is_focused(window)
                    && self.terminal_text_input_should_track_agents_slot(slot_id)
            }
            FirstResponderTarget::TerminalSurface(FirstResponderTerminalSurface::Command(_))
            | FirstResponderTarget::TerminalSurface(
                FirstResponderTerminalSurface::ProjectEditorCompanion(_),
            )
            | FirstResponderTarget::CefSurface(_)
            | FirstResponderTarget::Other
            | FirstResponderTarget::None => false,
        }
    }

    pub(crate) fn project_editor_companion_border_state(
        &self,
        mode: TitlebarMode,
        window: &Window,
    ) -> WorkspacePaneBorderState {
        if window.is_window_active()
            && self.shell_focus == ShellFocusTarget::ProjectEditorCompanion(mode)
            && (self.first_responder_target == FirstResponderTarget::GpuiWindow
                || self.first_responder_target
                    == FirstResponderTarget::CefSurface(
                        FirstResponderCefSurface::ProjectEditorCompanion,
                    )
                || self
                    .project_editor_companion_focused_terminal_session_id()
                    .is_some_and(|session_id| {
                        self.first_responder_target
                            == FirstResponderTarget::TerminalSurface(
                                FirstResponderTerminalSurface::ProjectEditorCompanion(session_id),
                            )
                            || (self.agents_chat_mode_sessions.contains(&session_id)
                                && self.first_responder_target
                                    == FirstResponderTarget::CefSurface(
                                        FirstResponderCefSurface::SessionChat(session_id),
                                    ))
                    }))
        {
            WorkspacePaneBorderState::Focused
        } else {
            WorkspacePaneBorderState::Neutral
        }
    }

    pub(crate) fn browser_leaf_border_state(
        &self,
        leaf: &BrowserLeaf,
        window: &Window,
    ) -> WorkspacePaneBorderState {
        if !window.is_window_active() {
            return WorkspacePaneBorderState::Neutral;
        }
        let shell_focuses_this_browser_pane = match self.shell_focus {
            ShellFocusTarget::BrowserPane(focus_pane_id) => focus_pane_id == leaf.pane_id,
            ShellFocusTarget::BrowserSurface => self.browser_tabs.focused_pane == leaf.pane_id,
            _ => false,
        };
        if !shell_focuses_this_browser_pane {
            return WorkspacePaneBorderState::Neutral;
        }
        let Some(tab_id) = leaf.tab_group.active_tab_id() else {
            return WorkspacePaneBorderState::Neutral;
        };
        if self.first_responder_target == FirstResponderTarget::GpuiWindow
            || self.first_responder_target
                == FirstResponderTarget::CefSurface(FirstResponderCefSurface::BrowserTab(tab_id))
        {
            WorkspacePaneBorderState::Focused
        } else {
            WorkspacePaneBorderState::Neutral
        }
    }

    pub(crate) fn project_editor_surface_border_state(
        &self,
        mode: TitlebarMode,
        window: &Window,
    ) -> WorkspacePaneBorderState {
        if !window.is_window_active()
            || self.shell_focus != ShellFocusTarget::ProjectEditorSurface(mode)
        {
            return WorkspacePaneBorderState::Neutral;
        }
        let native_surface_owns_focus = matches!(
            self.first_responder_target,
            FirstResponderTarget::CefSurface(FirstResponderCefSurface::ProjectWorkarea(slot_key))
                if slot_key.titlebar_mode() == mode
        );
        if native_surface_owns_focus
            || self.first_responder_target == FirstResponderTarget::GpuiWindow
        {
            WorkspacePaneBorderState::Focused
        } else {
            WorkspacePaneBorderState::Neutral
        }
    }

    pub(crate) fn remember_current_non_command_focus(&mut self) {
        if let Some(focus) = valid_non_command_shell_focus_with_browser_tabs(
            self.shell_focus,
            self.active_mode,
            &self.agents_workspace,
            &self.project_editor_shell,
            &self.browser_tabs,
        ) {
            self.previous_non_command_focus = Some(focus);
        }
    }

    pub(crate) fn restore_previous_non_command_focus_or_default(&mut self) {
        let focus = restored_non_command_shell_focus_or_default_with_browser_tabs(
            self.previous_non_command_focus,
            self.active_mode,
            &self.agents_workspace,
            &self.project_editor_shell,
            &self.browser_tabs,
        );
        self.set_shell_focus(focus);
    }

    pub(crate) fn focus_default_surface_for_active_mode(&mut self) {
        self.set_shell_focus(default_shell_focus_for_mode(
            self.active_mode,
            &self.agents_workspace,
            &self.project_editor_shell,
        ));
    }

    pub(crate) fn focus_agents_pane(
        &mut self,
        pane_id: WorkspacePaneId,
        cx: &mut gpui::Context<Self>,
    ) {
        self.agents_workspace.focus_pane(pane_id);
        self.sync_project_editor_companion_terminal_selection();
        self.set_shell_focus(ShellFocusTarget::AgentsPane(
            self.agents_workspace.focused_pane,
        ));
        let focused_pane = self.agents_workspace.focused_pane;
        let requested_terminal_focus = self
            .agents_workspace
            .active_session_in_pane(focused_pane)
            .map(|session_id| AgentsTerminalBodyMountSlotId {
                pane_id: focused_pane,
                session_id,
            })
            .filter(|slot_id| {
                self.agents_workspace
                    .is_current_terminal_body_mount_slot(*slot_id)
            });
        if let Some(slot_id) = requested_terminal_focus {
            /*
            CDXC:FocusRouting 2026-07-15:
            Pane-level focus actions (adjacent-group hotkeys, pane chrome, and
            directional navigation) must finish at the selected mounted
            terminal, not only update shell focus. Request the same exact
            mount-slot handoff used by session activation so the next render
            focuses either the GPUI engine handle or the native terminal host.
            */
            self.request_agents_session_text_focus_handoff(slot_id, cx);
        }
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.terminalFocus.agentsPaneFocusRequested",
            serde_json::json!({
                "pane": focused_pane.0,
                "session": requested_terminal_focus.map(|slot_id| slot_id.session_id.0),
                "mountSlotFocusRequested": requested_terminal_focus.is_some(),
            }),
        );
        self.dispatch_gpui_workspace_active_session_attention_acknowledge(focused_pane, cx);
        self.dispatch_gpui_workspace_active_session_selected(focused_pane, cx);
        self.scroll_workspace_pane_active_tab(pane_id);
        self.scroll_workspace_pane_active_tab(self.agents_workspace.focused_pane);
        self.persist_shell_layout_state();
    }

    pub(crate) fn dispatch_gpui_workspace_active_session_attention_acknowledge(
        &mut self,
        pane_id: WorkspacePaneId,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(session_id) = self.agents_workspace.active_session_in_pane(pane_id) {
            self.dispatch_gpui_workspace_session_attention_acknowledge(session_id, cx);
        }
    }

    pub(crate) fn dispatch_gpui_workspace_active_session_selected(
        &mut self,
        pane_id: WorkspacePaneId,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(session_id) = self.agents_workspace.active_session_in_pane(pane_id) else {
            return;
        };
        let Some(key) = self.workspace_terminal_key_for_shell_session(session_id) else {
            return;
        };
        match key {
            GpuiWorkspaceTerminalSessionKey::Local(key) => {
                self.local_workspace_latest_focus_key = Some(key.clone());
                self.dispatch_gpui_workspace_tab_session_selected(
                    key.project_id.as_str(),
                    key.session_id.as_str(),
                    false,
                    false,
                    cx,
                );
            }
            GpuiWorkspaceTerminalSessionKey::Remote(key) => {
                self.set_sidebar_gxserver_remote_attach_focus_state(&key, cx);
            }
        }
    }

    pub(crate) fn acknowledge_agents_pane_attention_from_chrome_click(
        &mut self,
        pane_id: WorkspacePaneId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.dispatch_gpui_workspace_active_session_attention_acknowledge(pane_id, cx);
        if !self
            .agents_workspace
            .acknowledge_attention_for_active_session_in_pane(pane_id)
        {
            return false;
        }
        self.persist_shell_layout_state();
        cx.notify();
        true
    }

    pub(crate) fn select_agents_tab(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:FocusRouting 2026-06-26-23:24:
        Agents pane-tab selection follows the macOS click-to-wake setting. The default keeps sleeping tabs cold until their placeholder body is activated; strict `clickToWakeSleepingSessions: false` requests a gxserver wake for mapped sleeping sessions after selection, while unmapped local tabs use the shell-only wake state.
        */
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let selected_local_was_sleeping =
            self.agents_workspace
                .session(session_id)
                .is_some_and(|session| {
                    session.presentation_state == TerminalSessionPresentationState::Sleeping
                });
        let wake_on_tab_selection =
            !gpui_click_to_wake_sleeping_sessions_from_shared_settings(&settings_snapshot)
                && selected_local_was_sleeping;
        let attention_acknowledged = self
            .agents_workspace
            .session(session_id)
            .is_some_and(|session| session.activity == AgentTerminalActivity::Attention);
        self.agents_workspace.select_tab(pane_id, session_id);
        self.dispatch_gpui_workspace_session_attention_acknowledge(session_id, cx);
        self.sync_project_editor_companion_terminal_selection();
        self.set_shell_focus(ShellFocusTarget::AgentsPane(
            self.agents_workspace.focused_pane,
        ));
        let selected_slot_id = AgentsTerminalBodyMountSlotId {
            pane_id: self.agents_workspace.focused_pane,
            session_id,
        };
        if self
            .agents_workspace
            .is_current_terminal_body_mount_slot(selected_slot_id)
        {
            self.request_agents_session_text_focus_handoff(selected_slot_id, cx);
        }
        self.scroll_workspace_pane_active_tab(pane_id);
        self.persist_shell_layout_state();
        if let Some(key) = self.local_workspace_key_for_shell_session(session_id) {
            let selected_local_runtime_missing =
                self.agents_tab_selected_local_runtime_missing(pane_id, session_id);
            if selected_local_runtime_missing {
                support_logs::append(
                    support_logs::GpuiSupportLog::TerminalFocus,
                    "gpui.terminalFocus.tabSelectedRuntimeMissing",
                    serde_json::json!({
                        "projectId": key.project_id.as_str(),
                        "sessionId": key.session_id.as_str(),
                    }),
                );
            }
            self.dispatch_gpui_workspace_tab_session_selected(
                key.project_id.as_str(),
                key.session_id.as_str(),
                selected_local_was_sleeping,
                selected_local_runtime_missing,
                cx,
            );
        } else if let Some(GpuiWorkspaceTerminalSessionKey::Remote(key)) =
            self.workspace_terminal_key_for_shell_session(session_id)
        {
            if !selected_local_was_sleeping
                && self.agents_tab_selected_local_runtime_missing(pane_id, session_id)
            {
                let _ = self.request_gpui_remote_attach_terminal_open(
                    GpuiRemoteAttachSessionReference {
                        remote_machine_id: key.remote_machine_id,
                        project_id: key.project_id,
                        session_id: key.session_id,
                    },
                    Some(pane_id),
                    AgentsWorkspaceNewTerminalPlacement::Tab,
                    cx,
                );
            } else {
                self.set_sidebar_gxserver_remote_attach_focus_state(&key, cx);
            }
        } else if let Some(key) = self.agents_chat_remote_key_for_session(session_id) {
            self.set_sidebar_gxserver_remote_attach_focus_state(&key, cx);
        }
        if wake_on_tab_selection {
            if self.agents_terminal_session_is_mapped_sleeping(session_id) {
                let _ = self.request_mapped_sleeping_agents_terminal_wake(pane_id, session_id, cx);
            } else if self
                .agents_workspace
                .set_session_sleeping(session_id, false)
            {
                #[cfg(target_os = "windows")]
                self.make_unmapped_windows_agents_wake_startup_eligible(session_id);
                self.persist_shell_layout_state();
                self.sync_gpui_keep_awake_automation_from_current_settings(cx);
            }
        }
        if attention_acknowledged {
            cx.notify();
        }
    }

    pub(crate) fn double_click_agents_workspace_tab(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        if self
            .agents_workspace
            .toggle_focus_mode_from_tab_double_click(pane_id, session_id)
        {
            self.dispatch_gpui_workspace_session_attention_acknowledge(session_id, cx);
            self.set_shell_focus(ShellFocusTarget::AgentsPane(
                self.agents_workspace.focused_pane,
            ));
            self.scroll_workspace_pane_active_tab(pane_id);
            self.scroll_workspace_pane_active_tab(self.agents_workspace.focused_pane);
            self.persist_shell_layout_state();
        }
        cx.notify();
    }

    pub(crate) fn activate_agents_terminal_placeholder(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self
            .agents_workspace
            .session_belongs_to_pane(pane_id, session_id)
        {
            return;
        }

        if let Some(GpuiWorkspaceTerminalSessionKey::Remote(key)) =
            self.workspace_terminal_key_for_shell_session(session_id)
        {
            let _ = self.request_gpui_remote_attach_terminal_open(
                GpuiRemoteAttachSessionReference {
                    remote_machine_id: key.remote_machine_id,
                    project_id: key.project_id,
                    session_id: key.session_id,
                },
                Some(pane_id),
                AgentsWorkspaceNewTerminalPlacement::Tab,
                cx,
            );
            return;
        }

        if self.agents_terminal_session_is_mapped_sleeping(session_id) {
            if !self.request_mapped_sleeping_agents_terminal_wake(pane_id, session_id, cx) {
                return;
            }
            self.dispatch_gpui_workspace_session_attention_acknowledge(session_id, cx);
            let attention_acknowledged = self
                .agents_workspace
                .acknowledge_attention_for_session_activation(session_id);
            /*
            CDXC:SessionSleep 2026-06-26-23:24:
            Placeholder body activation for mapped sleeping gxserver sessions must be a wake request first, matching macOS. Keep the native pane focused while pending, but do not move the tab to Mounting until the sidebar lifecycle result confirms gxserver wake.
            */
            let focus = ShellFocusTarget::AgentsPane(self.agents_workspace.focused_pane);
            let shell_focus_changed = self.shell_focus != focus;
            self.set_shell_focus(focus);
            if shell_focus_changed || attention_acknowledged {
                self.scroll_workspace_pane_active_tab(pane_id);
                self.persist_shell_layout_state();
                cx.notify();
            }
            return;
        }

        #[cfg(target_os = "windows")]
        let was_unmapped_sleeping_windows_terminal = self
            .agents_workspace
            .session(session_id)
            .is_some_and(|session| {
                session.presentation_state == TerminalSessionPresentationState::Sleeping
            });
        let model_changed = activate_agents_terminal_placeholder_with_runtime_attempt_identity(
            &mut self.agents_workspace,
            &mut self.agents_terminal_runtime_sessions,
            pane_id,
            session_id,
        );
        #[cfg(target_os = "windows")]
        if was_unmapped_sleeping_windows_terminal {
            self.make_unmapped_windows_agents_wake_startup_eligible(session_id);
        }
        self.dispatch_gpui_workspace_session_attention_acknowledge(session_id, cx);
        let attention_acknowledged = self
            .agents_workspace
            .acknowledge_attention_for_session_activation(session_id);
        let focus = ShellFocusTarget::AgentsPane(self.agents_workspace.focused_pane);
        let shell_focus_changed = self.shell_focus != focus;
        self.set_shell_focus(focus);
        if model_changed || shell_focus_changed || attention_acknowledged {
            self.scroll_workspace_pane_active_tab(pane_id);
            self.persist_shell_layout_state();
            cx.notify();
        }
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn make_unmapped_windows_agents_wake_startup_eligible(
        &mut self,
        session_id: TerminalSessionId,
    ) {
        /*
        The composited Windows engine tears down an unmapped local child when
        its tab sleeps. There is therefore no parked native owner to reattach:
        waking must enter the normal startup pipeline and create a fresh WSL
        or PowerShell process. Mapped gxserver sessions never reach this path;
        their wake remains daemon-owned and persistent.
        */
        if self
            .local_workspace_key_for_shell_session(session_id)
            .is_some()
        {
            return;
        }
        self.agents_workspace
            .make_mounting_session_startup_eligible(session_id);
    }

    pub(crate) fn focus_agents_terminal_mount_slot(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:FocusRouting 2026-06-22-23:11:
        Running terminal body clicks are focus handoffs, not placeholder activation. Keep the existing body div as the click/drop owner, update the shell focus to the clicked Agents pane, and force one AppKit first-responder handoff for the mounted real surface without adding overlays, hit-test routing, synthetic input routing, process lifecycle, or persistence fields.
        */
        if !self
            .agents_workspace
            .is_current_terminal_body_mount_slot(slot_id)
        {
            return;
        }

        let gpui_engine_view = self
            .agents_gpui_engine_terminals
            .get(&slot_id.session_id)
            .map(|record| record.view.clone());
        if let Some(view) = gpui_engine_view.as_ref() {
            // Reclaim AppKit and clear explicit CEF ownership before sidebar
            // selection notifications can run renderer work during this same
            // mouse event. The terminal FocusHandle remains the final GPUI
            // keyboard target throughout the model updates below.
            self.focus_gpui_engine_terminal_view(
                GpuiEngineTerminalEventTarget::Agents(slot_id.session_id),
                view,
                window,
                cx,
            );
        }

        let workspace_focus_changed = self.agents_workspace.focused_pane != slot_id.pane_id;
        self.agents_workspace.focus_pane(slot_id.pane_id);
        self.dispatch_gpui_workspace_session_attention_acknowledge(slot_id.session_id, cx);
        self.dispatch_gpui_workspace_active_session_selected(slot_id.pane_id, cx);
        let attention_acknowledged = self
            .agents_workspace
            .acknowledge_attention_for_session_activation(slot_id.session_id);
        let focus = ShellFocusTarget::AgentsPane(slot_id.pane_id);
        let shell_focus_changed = self.shell_focus != focus;
        self.set_shell_focus_with_terminal_handoff(focus, true);
        // GPUI-engine slots key/IME-focus their own element focus handle.
        // Native libghostty slots already received the exact AppKit responder
        // handoff above and must keep it: focusing GPUI's legacy text service
        // here steals keyDown from the host NSView, reducing terminal input to
        // committed text and dropping physical/modifier keys such as Tab and
        // Option/Alt chords.
        if workspace_focus_changed || shell_focus_changed || attention_acknowledged {
            self.scroll_workspace_pane_active_tab(slot_id.pane_id);
            self.persist_shell_layout_state();
            cx.notify();
        }
    }

    /*
    CDXC:Zmx 2026-07-06:
    Clicking terminal content, including an already-focused pane, is an explicit
    opportunity to recover from a zmx daemon grid that another client changed.
    Mirrors macOS `refreshZmxPersistenceTerminalIfNeeded(mode: .ifStale)`: use
    zmx's conditional grid-size refresh for click-originated requests only, so a
    normal click inside an already-correct pane never repaints the terminal or
    scrolls it to the visible bottom.
    */
    pub(crate) fn refresh_zmx_persistence_agents_terminal_if_stale(
        &self,
        slot_id: AgentsTerminalBodyMountSlotId,
        cx: &gpui::Context<Self>,
    ) {
        if !self
            .agents_workspace
            .is_current_terminal_body_mount_slot(slot_id)
        {
            return;
        }
        // A parked or just-surfacing engine grid is the zmx resting width,
        // not a displayed size (CDXC:Terminal 2026-09-03).
        if !self.agents_gpui_engine_terminal_zmx_grid_is_displayed(slot_id.session_id, cx) {
            return;
        }
        let session_name = self
            .agents_workspace
            .session(slot_id.session_id)
            .and_then(|session| session.zmx_session_name.clone());
        gpui_spawn_zmx_refresh_if_stale_process(
            session_name,
            self.agents_terminal_refresh_grid_size(slot_id, cx),
            "agentsTerminalContentMouseDown",
        );
    }

    pub(crate) fn refresh_zmx_persistence_command_terminal_if_stale(
        &self,
        slot_id: CommandTerminalBodyMountSlotId,
        cx: &gpui::Context<Self>,
    ) {
        if !self
            .command_pane
            .is_current_terminal_body_mount_slot(slot_id)
        {
            return;
        }
        let session_name = self
            .command_pane
            .session(slot_id.session_id)
            .and_then(|session| session.zmx_session_name.clone());
        let grid_size = {
            #[cfg(target_os = "macos")]
            let native_size = self
                .command_terminal_ghostty_surfaces
                .get(&slot_id)
                .map(|surface| {
                    let size = surface.surface_size();
                    (size.rows, size.columns)
                });
            #[cfg(not(target_os = "macos"))]
            let native_size = None;
            native_size.or_else(|| {
                self.command_gpui_engine_terminals
                    .get(&slot_id.session_id)
                    .map(|record| {
                        let (columns, rows) = record.view.read(cx).model().size();
                        (rows, columns)
                    })
            })
        };
        gpui_spawn_zmx_refresh_if_stale_process(
            session_name,
            grid_size,
            "commandTerminalContentMouseDown",
        );
    }

    pub(crate) fn refresh_zmx_persistence_companion_terminal_if_stale(
        &self,
        mode: TitlebarMode,
        cx: &gpui::Context<Self>,
    ) {
        let Some(slot_id) = self.project_editor_companion_terminal_slot_for_mode(mode) else {
            return;
        };
        // Same gate as the Agents slot: never hand the daemon a parked grid.
        if !self.agents_gpui_engine_terminal_zmx_grid_is_displayed(slot_id.session_id, cx) {
            return;
        }
        let session_name = self
            .agents_workspace
            .session(slot_id.session_id)
            .and_then(|session| session.zmx_session_name.clone());
        let grid_size = {
            #[cfg(target_os = "macos")]
            let native_size = self
                .project_editor_companion_terminal_ghostty_surfaces
                .get(&slot_id)
                .map(|surface| {
                    let size = surface.surface_size();
                    (size.rows, size.columns)
                });
            #[cfg(not(target_os = "macos"))]
            let native_size = None;
            native_size.or_else(|| {
                self.agents_gpui_engine_terminals
                    .get(&slot_id.session_id)
                    .map(|record| {
                        let (columns, rows) = record.view.read(cx).model().size();
                        (rows, columns)
                    })
            })
        };
        gpui_spawn_zmx_refresh_if_stale_process(
            session_name,
            grid_size,
            "companionTerminalContentMouseDown",
        );
    }

    pub(crate) fn agents_terminal_refresh_grid_size(
        &self,
        slot_id: AgentsTerminalBodyMountSlotId,
        cx: &gpui::Context<Self>,
    ) -> Option<(u16, u16)> {
        #[cfg(target_os = "macos")]
        if let Some(surface) = self.agents_terminal_ghostty_surfaces.get(&slot_id) {
            let size = surface.surface_size();
            return Some((size.rows, size.columns));
        }
        self.agents_gpui_engine_terminals
            .get(&slot_id.session_id)
            .map(|record| {
                let (columns, rows) = record.view.read(cx).model().size();
                (rows, columns)
            })
    }

    /*
    CDXC:Zmx 2026-07-06:
    Mirrors macOS `scheduleZmxPersistenceTerminalRefreshAfterResize`: split,
    sidebar, companion-ratio, and window resizes re-arm a trailing-edge 0.8s
    debounce, and the settled pass refreshes every surfaced zmx pane. Unlike
    macOS the settled pass stays size-gated through `refresh-if-stale` instead
    of the unconditional repaint sequence, so a pane whose grid already matches
    never repaints or scrolls.
    */
    pub(crate) fn schedule_zmx_persistence_refresh_after_resize(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        self.zmx_persistence_resize_refresh_generation = self
            .zmx_persistence_resize_refresh_generation
            .wrapping_add(1);
        let generation = self.zmx_persistence_resize_refresh_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(800))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.zmx_persistence_resize_refresh_generation != generation {
                    return;
                }
                this.refresh_zmx_persistence_surfaced_terminals_if_stale(cx);
            });
        })
        .detach();
    }

    /*
    CDXC:Zmx 2026-07-06:
    Mirrors macOS `zmxPersistenceTerminalSessionIdsForSurfacedPanes`: with a
    project editor active only the visible companion refreshes, otherwise the
    visible Agents mount slots do, and rendered command-pane slots always join.
    Hidden panes must never refresh because `refresh-if-stale` conforms the
    daemon grid to the passed size, and a hidden pane's stale size would fight
    the surfaced owner.
    */
    pub(crate) fn refresh_zmx_persistence_surfaced_terminals_if_stale(
        &self,
        cx: &gpui::Context<Self>,
    ) {
        if self.active_mode == TitlebarMode::Agents {
            for slot_id in self.agents_workspace.rendered_terminal_body_mount_slots() {
                self.refresh_zmx_persistence_agents_terminal_if_stale(slot_id, cx);
            }
        } else if let Some(slot_id) =
            self.project_editor_companion_terminal_slot_for_mode(self.active_mode)
        {
            self.refresh_zmx_persistence_companion_terminal_if_stale(slot_id.mode, cx);
        }
        for slot_id in self.command_pane.rendered_terminal_body_mount_slots() {
            self.refresh_zmx_persistence_command_terminal_if_stale(slot_id, cx);
        }
    }

    pub(crate) fn current_zmx_persistence_focused_terminal_slot(
        &self,
    ) -> Option<ZmxPersistenceFocusedTerminalSlot> {
        match self.shell_focus {
            ShellFocusTarget::AgentsPane(pane_id) => {
                if self.active_mode != TitlebarMode::Agents {
                    return None;
                }
                let session_id = self.agents_workspace.active_session_in_pane(pane_id)?;
                let slot_id = AgentsTerminalBodyMountSlotId {
                    pane_id,
                    session_id,
                };
                self.agents_workspace
                    .is_current_terminal_body_mount_slot(slot_id)
                    .then_some(ZmxPersistenceFocusedTerminalSlot::Agents(slot_id))
            }
            ShellFocusTarget::CommandPane => {
                let (group_id, session_id) = self.command_pane.focused_group_active_session_id()?;
                let slot_id = CommandTerminalBodyMountSlotId {
                    group_id,
                    session_id,
                };
                self.command_pane
                    .is_current_terminal_body_mount_slot(slot_id)
                    .then_some(ZmxPersistenceFocusedTerminalSlot::Command(slot_id))
            }
            ShellFocusTarget::ProjectEditorCompanion(mode) => self
                .project_editor_companion_terminal_slot_for_mode(mode)
                .map(ZmxPersistenceFocusedTerminalSlot::Companion),
            _ => None,
        }
    }

    /*
    CDXC:Zmx 2026-07-06:
    Mirrors macOS `refreshZmxPersistenceTerminalIfFocusOrSurfaceChanged` for
    non-click focus movement (keyboard pane navigation, sidebar focus routing,
    programmatic focus). Render-start change detection covers every focus call
    site without threading the refresh through each one; the conditional
    refresh makes redundant firing after click focus a size-matched no-op.
    */
    pub(crate) fn refresh_zmx_persistence_focused_terminal_if_changed(
        &mut self,
        cx: &gpui::Context<Self>,
    ) {
        let focused = self.current_zmx_persistence_focused_terminal_slot();
        if self.zmx_persistence_last_focused_terminal_slot == focused {
            return;
        }
        self.zmx_persistence_last_focused_terminal_slot = focused;
        match focused {
            Some(ZmxPersistenceFocusedTerminalSlot::Agents(slot_id)) => {
                self.refresh_zmx_persistence_agents_terminal_if_stale(slot_id, cx);
            }
            Some(ZmxPersistenceFocusedTerminalSlot::Command(slot_id)) => {
                self.refresh_zmx_persistence_command_terminal_if_stale(slot_id, cx);
            }
            Some(ZmxPersistenceFocusedTerminalSlot::Companion(slot_id)) => {
                self.refresh_zmx_persistence_companion_terminal_if_stale(slot_id.mode, cx);
            }
            None => {}
        }
    }

    pub(crate) fn forward_agents_terminal_mount_slot_mouse_position(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
        window_position: Point<Pixels>,
        modifiers: Modifiers,
    ) -> bool {
        /*
        CDXC:Terminal 2026-06-23-08:32:
        Mounted Running Agents bodies may forward only sanitized body-relative pointer position plus mapped keyboard modifier bits to the exact current Ghostty owner. Mouse movement uses the body event's GPUI modifiers and no capture, selection, drag, paste, keyboard, IME, logging, persistence, or coordinate routing outside the body event itself.
        */
        if !self
            .agents_workspace
            .is_current_terminal_body_mount_slot(slot_id)
        {
            return false;
        }
        let Some(position) = agents_terminal_body_relative_mouse_position_for_slot(
            &self.agents_terminal_mount_slot_bounds,
            slot_id,
            window_position,
        ) else {
            return false;
        };
        let mouse_mods = ghostty_mouse_mods_from_gpui_modifiers(modifiers);

        #[cfg(target_os = "macos")]
        {
            let Some(surface) = self.agents_terminal_ghostty_surfaces.get_mut(&slot_id) else {
                return false;
            };
            if surface.mount_slot_id() != slot_id {
                return false;
            }

            surface.mouse_pos(position.x, position.y, mouse_mods);
            true
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (position, mouse_mods);
            false
        }
    }

    pub(crate) fn forward_agents_terminal_mount_slot_mouse_button(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
        window_position: Point<Pixels>,
        action: ghostty_kit::ffi::ghostty_input_mouse_state_e,
        button: MouseButton,
        modifiers: Modifiers,
    ) -> bool {
        /*
        CDXC:Terminal 2026-06-23-10:23:
        Mounted Running Agents button press/release forwarding uses the existing current-slot and body-boundary gates, verifies the mounted surface owner and runtime identity, then updates pointer position and sends the mapped left/right/middle Ghostty button value. GPUI navigation buttons no-op before position forwarding so parity does not create stored button state or fallback routing.
        */
        let Some(ghostty_button) = ghostty_mouse_button_from_gpui_button(button) else {
            return false;
        };
        if !self
            .agents_workspace
            .is_current_terminal_body_mount_slot(slot_id)
        {
            return false;
        }
        let Some(position) = agents_terminal_body_relative_mouse_position_for_slot(
            &self.agents_terminal_mount_slot_bounds,
            slot_id,
            window_position,
        ) else {
            return false;
        };
        let Some(runtime_session_id) = self
            .agents_terminal_runtime_sessions
            .runtime_session_id_for_shell_session(slot_id.session_id)
        else {
            return false;
        };
        let mouse_mods = ghostty_mouse_mods_from_gpui_modifiers(modifiers);

        #[cfg(target_os = "macos")]
        {
            let Some(surface) = self.agents_terminal_ghostty_surfaces.get_mut(&slot_id) else {
                return false;
            };
            if surface.mount_slot_id() != slot_id
                || surface.runtime_session_id() != runtime_session_id
            {
                return false;
            }

            surface.mouse_pos(position.x, position.y, mouse_mods);
            surface.mouse_button(action, ghostty_button, mouse_mods)
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (
                position,
                runtime_session_id,
                action,
                ghostty_button,
                mouse_mods,
            );
            false
        }
    }

    pub(crate) fn forward_agents_terminal_mount_slot_mouse_release_outside(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
        button: MouseButton,
        modifiers: Modifiers,
    ) -> bool {
        /*
        CDXC:Terminal 2026-06-23-10:23:
        Mounted Agents mouse-up-out remains capture recovery only. Require the current slot and exact Ghostty owner runtime identity to still match, require Ghostty mouse capture, then send the mapped left/right/middle release with mapped modifiers without updating mouse_pos, storing last positions, or synthesizing outside coordinates.
        */
        if !self
            .agents_workspace
            .is_current_terminal_body_mount_slot(slot_id)
        {
            return false;
        }
        let Some(runtime_session_id) = self
            .agents_terminal_runtime_sessions
            .runtime_session_id_for_shell_session(slot_id.session_id)
        else {
            return false;
        };
        let Some(ghostty_button) = ghostty_mouse_button_from_gpui_button(button) else {
            return false;
        };
        let mouse_mods = ghostty_mouse_mods_from_gpui_modifiers(modifiers);

        #[cfg(target_os = "macos")]
        {
            let Some(surface) = self.agents_terminal_ghostty_surfaces.get_mut(&slot_id) else {
                return false;
            };
            if surface.mount_slot_id() != slot_id
                || surface.runtime_session_id() != runtime_session_id
                || !surface.mouse_captured()
            {
                return false;
            }

            surface.mouse_button(
                ghostty_kit::ffi::GHOSTTY_MOUSE_RELEASE,
                ghostty_button,
                mouse_mods,
            )
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (runtime_session_id, ghostty_button, mouse_mods);
            false
        }
    }

    pub(crate) fn forward_agents_terminal_mount_slot_mouse_pressure(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
        window_position: Point<Pixels>,
        stage: PressureStage,
        pressure: f32,
        modifiers: Modifiers,
    ) -> bool {
        /*
        CDXC:Terminal 2026-06-23-09:51:
        Mounted Running Agents body pressure is forwarded only from the exact current body mount slot. Require recorded body bounds and the matching Ghostty surface owner, update the body-relative pointer position with mapped modifiers first, then pass the mapped pressure stage and raw GPUI pressure value without capture, selection, paste, keyboard, IME, logging, persistence, overlays, or routing.
        */
        if !self
            .agents_workspace
            .is_current_terminal_body_mount_slot(slot_id)
        {
            return false;
        }
        let Some(position) = agents_terminal_body_relative_mouse_position_for_slot(
            &self.agents_terminal_mount_slot_bounds,
            slot_id,
            window_position,
        ) else {
            return false;
        };
        let mouse_mods = ghostty_mouse_mods_from_gpui_modifiers(modifiers);
        let pressure_stage = ghostty_mouse_pressure_stage_from_gpui_stage(stage);

        #[cfg(target_os = "macos")]
        {
            let Some(surface) = self.agents_terminal_ghostty_surfaces.get_mut(&slot_id) else {
                return false;
            };
            if surface.mount_slot_id() != slot_id {
                return false;
            }

            surface.mouse_pos(position.x, position.y, mouse_mods);
            surface.mouse_pressure(pressure_stage, f64::from(pressure));
            true
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (position, mouse_mods, pressure_stage, pressure);
            false
        }
    }

    pub(crate) fn forward_agents_terminal_mount_slot_mouse_scroll(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
        window_position: Point<Pixels>,
        delta: ScrollDelta,
        modifiers: Modifiers,
    ) -> bool {
        if !self
            .agents_workspace
            .is_current_terminal_body_mount_slot(slot_id)
        {
            return false;
        }
        let Some(position) = agents_terminal_body_relative_mouse_position_for_slot(
            &self.agents_terminal_mount_slot_bounds,
            slot_id,
            window_position,
        ) else {
            return false;
        };
        let (scroll_x, scroll_y, scroll_mods) = terminal_ghostty_scroll_delta(delta);
        let mouse_mods = ghostty_mouse_mods_from_gpui_modifiers(modifiers);

        #[cfg(target_os = "macos")]
        {
            let Some(surface) = self.agents_terminal_ghostty_surfaces.get_mut(&slot_id) else {
                return false;
            };
            if surface.mount_slot_id() != slot_id {
                return false;
            }

            surface.mouse_pos(position.x, position.y, mouse_mods);
            surface.mouse_scroll(scroll_x, scroll_y, scroll_mods);
            true
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (position, scroll_x, scroll_y, scroll_mods, mouse_mods);
            false
        }
    }

    pub(crate) fn focus_command_terminal_mount_slot(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Terminal 2026-06-23-09:41:
        Mounted command terminal body clicks are command-pane focus handoffs before input forwarding. Accept only the current command body mount slot, focus that command group, keep shell focus on `CommandPane`, force the existing AppKit terminal handoff path, persist the same shell focus state as placeholder clicks, and avoid new persisted fields or hit-test routing.

        CDXC:FocusRouting 2026-06-26-00:00:
        Mounted command terminal body focus must reveal the active command tab in both the expanded group strip and collapsed strip, matching native `focusTerminal(...)->revealActivePaneTab` while leaving mouse forwarding and terminal handoff ownership on the body element.
        */
        if !self
            .command_pane
            .is_current_terminal_body_mount_slot(slot_id)
        {
            return;
        }

        if !self.command_pane.focus_group(slot_id.group_id) {
            return;
        }
        let attention_acknowledged = self
            .command_pane
            .acknowledge_attention_for_session_activation(slot_id.session_id);
        let gpui_engine_view = self
            .command_gpui_engine_terminals
            .get(&slot_id.session_id)
            .map(|record| record.view.clone());
        if let Some(view) = gpui_engine_view.as_ref() {
            self.focus_gpui_engine_terminal_view(
                GpuiEngineTerminalEventTarget::Command(slot_id.session_id),
                view,
                window,
                cx,
            );
        }
        self.remember_current_non_command_focus();
        self.set_shell_focus_with_terminal_handoff(ShellFocusTarget::CommandPane, true);
        self.request_command_terminal_text_focus_handoff(slot_id);
        if gpui_engine_view.is_some() {
            self.pending_command_terminal_text_focus_slot = None;
        }
        self.scroll_command_group_active_tab(slot_id.group_id);
        self.persist_shell_layout_state();
        if attention_acknowledged {
            self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        }
        cx.notify();
    }

    pub(crate) fn forward_command_terminal_mount_slot_mouse_position(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        window_position: Point<Pixels>,
        modifiers: Modifiers,
    ) -> bool {
        /*
        CDXC:Terminal 2026-06-23-09:41:
        Command terminal pointer movement forwards only body-relative coordinates plus mapped keyboard modifier bits from a current command body slot to the exact mounted Ghostty surface. Missing bounds, stale slots, mismatched owners, non-macOS builds, and absent surfaces no-op without logging, persistence, capture, overlays, hidden hit regions, or synthetic routing.
        */
        if !self
            .command_pane
            .is_current_terminal_body_mount_slot(slot_id)
        {
            return false;
        }
        let Some(position) = command_terminal_body_relative_mouse_position_for_slot(
            &self.command_terminal_mount_slot_bounds,
            slot_id,
            window_position,
        ) else {
            return false;
        };
        let mouse_mods = ghostty_mouse_mods_from_gpui_modifiers(modifiers);

        #[cfg(target_os = "macos")]
        {
            let Some(surface) = self.command_terminal_ghostty_surfaces.get_mut(&slot_id) else {
                return false;
            };
            if surface.mount_slot_id() != slot_id {
                return false;
            }

            surface.mouse_pos(position.x, position.y, mouse_mods);
            true
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (position, mouse_mods);
            false
        }
    }

    pub(crate) fn forward_command_terminal_mount_slot_mouse_button(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        window_position: Point<Pixels>,
        action: ghostty_kit::ffi::ghostty_input_mouse_state_e,
        button: MouseButton,
        modifiers: Modifiers,
    ) -> bool {
        /*
        CDXC:Terminal 2026-06-23-10:23:
        Mounted command terminal button press/release forwarding keeps the current command body slot and body-boundary gates, verifies the mounted surface owner and runtime identity, then updates pointer position and sends the mapped left/right/middle Ghostty button value. GPUI navigation buttons no-op before position forwarding so command terminals do not store raw button state or add fallback routing.
        */
        let Some(ghostty_button) = ghostty_mouse_button_from_gpui_button(button) else {
            return false;
        };
        if !self
            .command_pane
            .is_current_terminal_body_mount_slot(slot_id)
        {
            return false;
        }
        let Some(position) = command_terminal_body_relative_mouse_position_for_slot(
            &self.command_terminal_mount_slot_bounds,
            slot_id,
            window_position,
        ) else {
            return false;
        };
        let runtime_session_id = command_terminal_runtime_session_id(slot_id);
        let mouse_mods = ghostty_mouse_mods_from_gpui_modifiers(modifiers);

        #[cfg(target_os = "macos")]
        {
            let Some(surface) = self.command_terminal_ghostty_surfaces.get_mut(&slot_id) else {
                return false;
            };
            if surface.mount_slot_id() != slot_id
                || surface.runtime_session_id() != runtime_session_id
            {
                return false;
            }

            surface.mouse_pos(position.x, position.y, mouse_mods);
            surface.mouse_button(action, ghostty_button, mouse_mods)
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (
                position,
                runtime_session_id,
                action,
                ghostty_button,
                mouse_mods,
            );
            false
        }
    }

    pub(crate) fn forward_command_terminal_mount_slot_mouse_release_outside(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        button: MouseButton,
        modifiers: Modifiers,
    ) -> bool {
        /*
        CDXC:Terminal 2026-06-23-10:23:
        Command terminal mouse-up-out mirrors Agents capture recovery on the mounted body element. It validates the current slot and exact Ghostty owner runtime identity, requires Ghostty mouse capture, and sends only the mapped left/right/middle release with mapped modifiers so outside coordinates never update or get synthesized.
        */
        if !self
            .command_pane
            .is_current_terminal_body_mount_slot(slot_id)
        {
            return false;
        }
        let runtime_session_id = command_terminal_runtime_session_id(slot_id);
        let Some(ghostty_button) = ghostty_mouse_button_from_gpui_button(button) else {
            return false;
        };
        let mouse_mods = ghostty_mouse_mods_from_gpui_modifiers(modifiers);

        #[cfg(target_os = "macos")]
        {
            let Some(surface) = self.command_terminal_ghostty_surfaces.get_mut(&slot_id) else {
                return false;
            };
            if surface.mount_slot_id() != slot_id
                || surface.runtime_session_id() != runtime_session_id
                || !surface.mouse_captured()
            {
                return false;
            }

            surface.mouse_button(
                ghostty_kit::ffi::GHOSTTY_MOUSE_RELEASE,
                ghostty_button,
                mouse_mods,
            )
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (runtime_session_id, ghostty_button, mouse_mods);
            false
        }
    }

    pub(crate) fn forward_command_terminal_mount_slot_mouse_pressure(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        window_position: Point<Pixels>,
        stage: PressureStage,
        pressure: f32,
        modifiers: Modifiers,
    ) -> bool {
        /*
        CDXC:Terminal 2026-06-23-09:51:
        Mounted command body pressure mirrors Agents forwarding through the normal command body element only. Current slot, recorded body bounds, exact Ghostty surface identity, and macOS availability must all match before updating pointer position and sending raw GPUI pressure without fallback behavior, logging, persistence, overlays, hidden hit regions, or input routing.
        */
        if !self
            .command_pane
            .is_current_terminal_body_mount_slot(slot_id)
        {
            return false;
        }
        let Some(position) = command_terminal_body_relative_mouse_position_for_slot(
            &self.command_terminal_mount_slot_bounds,
            slot_id,
            window_position,
        ) else {
            return false;
        };
        let mouse_mods = ghostty_mouse_mods_from_gpui_modifiers(modifiers);
        let pressure_stage = ghostty_mouse_pressure_stage_from_gpui_stage(stage);

        #[cfg(target_os = "macos")]
        {
            let Some(surface) = self.command_terminal_ghostty_surfaces.get_mut(&slot_id) else {
                return false;
            };
            if surface.mount_slot_id() != slot_id {
                return false;
            }

            surface.mouse_pos(position.x, position.y, mouse_mods);
            surface.mouse_pressure(pressure_stage, f64::from(pressure));
            true
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (position, mouse_mods, pressure_stage, pressure);
            false
        }
    }

    pub(crate) fn forward_command_terminal_mount_slot_mouse_scroll(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        window_position: Point<Pixels>,
        delta: ScrollDelta,
        modifiers: Modifiers,
    ) -> bool {
        if !self
            .command_pane
            .is_current_terminal_body_mount_slot(slot_id)
        {
            return false;
        }
        let Some(position) = command_terminal_body_relative_mouse_position_for_slot(
            &self.command_terminal_mount_slot_bounds,
            slot_id,
            window_position,
        ) else {
            return false;
        };
        let (scroll_x, scroll_y, scroll_mods) = terminal_ghostty_scroll_delta(delta);
        let mouse_mods = ghostty_mouse_mods_from_gpui_modifiers(modifiers);

        #[cfg(target_os = "macos")]
        {
            let Some(surface) = self.command_terminal_ghostty_surfaces.get_mut(&slot_id) else {
                return false;
            };
            if surface.mount_slot_id() != slot_id {
                return false;
            }

            surface.mouse_pos(position.x, position.y, mouse_mods);
            surface.mouse_scroll(scroll_x, scroll_y, scroll_mods);
            true
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (position, scroll_x, scroll_y, scroll_mods, mouse_mods);
            false
        }
    }

    pub(crate) fn forward_project_editor_companion_terminal_mount_slot_mouse_scroll(
        &mut self,
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
        window_position: Point<Pixels>,
        delta: ScrollDelta,
        modifiers: Modifiers,
    ) -> bool {
        if !self.is_current_project_editor_companion_terminal_body_mount_slot(slot_id) {
            return false;
        }
        let Some(position) = terminal_body_relative_mouse_position_for_slot(
            &self.project_editor_companion_terminal_mount_slot_bounds,
            slot_id,
            window_position,
        ) else {
            return false;
        };
        let Some(runtime_session_id) = self
            .agents_terminal_runtime_sessions
            .runtime_session_id_for_shell_session(slot_id.session_id)
        else {
            return false;
        };
        let (scroll_x, scroll_y, scroll_mods) = terminal_ghostty_scroll_delta(delta);
        let mouse_mods = ghostty_mouse_mods_from_gpui_modifiers(modifiers);

        #[cfg(target_os = "macos")]
        {
            let Some(surface) = self
                .project_editor_companion_terminal_ghostty_surfaces
                .get_mut(&slot_id)
            else {
                return false;
            };
            if surface.mount_slot_id() != slot_id
                || surface.runtime_session_id() != runtime_session_id
            {
                return false;
            }

            surface.mouse_pos(position.x, position.y, mouse_mods);
            surface.mouse_scroll(scroll_x, scroll_y, scroll_mods);
            true
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (
                position,
                runtime_session_id,
                scroll_x,
                scroll_y,
                scroll_mods,
                mouse_mods,
            );
            false
        }
    }

    pub(crate) fn select_agents_tab_from_action(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        if self
            .agents_workspace
            .find_leaf(pane_id)
            .is_some_and(|leaf| leaf.tab_group.has_session(session_id))
        {
            self.select_agents_tab(pane_id, session_id, cx);
            cx.notify();
        }
    }

    pub(crate) fn close_agents_tab_from_action(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        self.close_agents_tab(pane_id, session_id, cx);
    }

    pub(crate) fn forget_local_workspace_mappings_for_shell_session(
        &mut self,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        self.source_code_server_runtime
            .cancel_remote_prompt_editor_request_for_shell_session(shell_session_id);
        /*
        CDXC:FocusRouting 2026-06-26-06:57:
        Removing a GPUI shell tab must also drop only the process-local gxserver/session mapping for that shell id. Close provider cleanup and acknowledged Sleep transitions remain sidebar-owned through the lifecycle bridge; this cleanup prevents stale GPUI mappings from selecting a deleted tab without fabricating daemon success, deleting gxserver rows, logging ids, or touching persisted private data.
        */
        let scoped_remote_key = self
            .workspace_terminal_key_for_shell_session(shell_session_id)
            .and_then(|key| match key {
                GpuiWorkspaceTerminalSessionKey::Remote(key) => Some(key),
                GpuiWorkspaceTerminalSessionKey::Local(_) => None,
            });
        self.remove_agents_chat_surface_for_session(shell_session_id, cx);
        if let Some(remote_key) = scoped_remote_key.as_ref() {
            self.clear_project_editor_companion_remote_attach_state_for_key(remote_key);
            self.remote_attach_sessions.remove(remote_key);
            #[cfg(target_os = "macos")]
            self.remote_attach_askpass_scripts.remove(remote_key);
        }
        let removed_keys = self
            .local_workspace_session_mappings
            .iter()
            .filter_map(|(key, mapped_session_id)| {
                (*mapped_session_id == shell_session_id).then_some(key.clone())
            })
            .collect::<Vec<_>>();
        if removed_keys.is_empty() {
            self.local_app_shot_session_mappings
                .retain(|_, mapped_session_id| *mapped_session_id != shell_session_id);
            return;
        }
        self.local_workspace_session_mappings
            .retain(|_, mapped_session_id| *mapped_session_id != shell_session_id);
        self.local_workspace_attach_pending
            .retain(|key| !removed_keys.contains(key));
        self.local_workspace_lifecycle_requests
            .retain(|_, request| request.shell_session_id != shell_session_id);
        if self
            .local_workspace_latest_focus_key
            .as_ref()
            .is_some_and(|key| removed_keys.contains(key))
        {
            self.local_workspace_latest_focus_key = None;
        }
        self.local_app_shot_session_mappings
            .retain(|_, mapped_session_id| *mapped_session_id != shell_session_id);
    }

    pub(crate) fn close_agents_tab(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.agents_workspace.can_close_tab(pane_id, session_id) {
            return false;
        }
        let target_key = self.workspace_terminal_key_for_shell_session(session_id);
        let replacement_key = self
            .agents_workspace
            .selected_session_after_direct_tab_close(pane_id, session_id)
            .and_then(|replacement_session_id| {
                self.workspace_terminal_key_for_shell_session(replacement_session_id)
            });
        /*
        CDXC:Workarea 2026-06-26-05:23:
        Direct mapped Close mirrors macOS pane tabs, including final-root close. When there is no pane-local replacement, tell the sidebar runtime not to focus a fallback session; Rust removes the shell tab immediately and leaves the workspace empty if this was the final terminal.

        CDXC:Workarea 2026-06-26-23:59:
        Mapped GPUI workspace close bypasses Ghostty close-confirm, commits the Rust tab mutation locally, and routes only provider cleanup through SidebarApp. Mounted surface close-confirm remains for unmapped/local-only running terminals only.
        */
        let skip_replacement_fallback = replacement_key.is_none();
        match target_key {
            Some(GpuiWorkspaceTerminalSessionKey::Local(_)) => {
                return self.request_local_workspace_terminal_lifecycle(
                    pane_id,
                    session_id,
                    GpuiLocalWorkspaceLifecycleAction::Close,
                    GpuiLocalWorkspaceLifecycleMutationKind::DirectClose,
                    replacement_key.and_then(|key| key.as_local().cloned()),
                    skip_replacement_fallback,
                    None,
                    cx,
                );
            }
            Some(GpuiWorkspaceTerminalSessionKey::Remote(_)) => {
                return self.request_remote_workspace_terminal_lifecycle(
                    pane_id,
                    session_id,
                    GpuiLocalWorkspaceLifecycleAction::Close,
                    GpuiLocalWorkspaceLifecycleMutationKind::DirectClose,
                    replacement_key.and_then(|key| match key {
                        GpuiWorkspaceTerminalSessionKey::Remote(key) => Some(key),
                        GpuiWorkspaceTerminalSessionKey::Local(_) => None,
                    }),
                    skip_replacement_fallback,
                    cx,
                );
            }
            None => {}
        }

        if self.request_close_agents_gpui_engine_terminal(
            AgentsTerminalBodyMountSlotId {
                pane_id,
                session_id,
            },
            cx,
        ) {
            cx.notify();
            return true;
        }
        if self.request_close_agents_running_surface_if_mounted(AgentsTerminalBodyMountSlotId {
            pane_id,
            session_id,
        }) {
            cx.notify();
            return true;
        }

        if !self
            .agents_workspace
            .close_tab_from_direct_tab_close(pane_id, session_id)
        {
            return false;
        }
        self.forget_local_workspace_mappings_for_shell_session(session_id, cx);
        self.set_shell_focus(ShellFocusTarget::AgentsPane(
            self.agents_workspace.focused_pane,
        ));
        self.scroll_workspace_pane_active_tab(self.agents_workspace.focused_pane);
        self.persist_shell_layout_state();
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        cx.notify();
        true
    }

    pub(crate) fn close_agents_tabs_for_scope(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        scope: AgentsWorkspaceTabCloseScope,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:ContextMenus 2026-06-26-06:57:
        Scoped Agents context-menu Close rows are local tab mutations plus asynchronous lifecycle cleanup, not tab-selection actions. Resolve the clicked pane group before mutation, preserve current shell focus for Close Right/Left/Others, and let direct inline close keep using the clicked-tab focus path.
        */
        if scope == AgentsWorkspaceTabCloseScope::Close {
            return self.close_agents_tab(pane_id, session_id, cx);
        }

        let session_ids = self
            .agents_workspace
            .tab_session_ids_for_close_scope(pane_id, session_id, scope);
        if session_ids.is_empty() {
            return false;
        }

        let mut close_requested = false;
        let mut model_changed = false;
        for close_session_id in session_ids {
            if let Some(workspace_key) =
                self.workspace_terminal_key_for_shell_session(close_session_id)
            {
                /*
                CDXC:Workarea 2026-06-26-23:59:
                Scoped mapped close follows macOS by removing the Rust tab immediately and asking SidebarApp to clean up the provider asynchronously, before considering any mounted Ghostty close-confirm path. This prevents either a retryable terminal prompt or a delayed external bridge from blocking local tab removal.
                */
                let requested = match workspace_key {
                    GpuiWorkspaceTerminalSessionKey::Local(_) => self
                        .request_local_workspace_terminal_lifecycle(
                            pane_id,
                            close_session_id,
                            GpuiLocalWorkspaceLifecycleAction::Close,
                            GpuiLocalWorkspaceLifecycleMutationKind::ScopedClose,
                            None,
                            false,
                            None,
                            cx,
                        ),
                    GpuiWorkspaceTerminalSessionKey::Remote(_) => self
                        .request_remote_workspace_terminal_lifecycle(
                            pane_id,
                            close_session_id,
                            GpuiLocalWorkspaceLifecycleAction::Close,
                            GpuiLocalWorkspaceLifecycleMutationKind::ScopedClose,
                            None,
                            false,
                            cx,
                        ),
                };
                if requested {
                    close_requested = true;
                }
                continue;
            }
            if self.request_close_agents_gpui_engine_terminal(
                AgentsTerminalBodyMountSlotId {
                    pane_id,
                    session_id: close_session_id,
                },
                cx,
            ) {
                close_requested = true;
                continue;
            }
            if self.request_close_agents_running_surface_if_mounted(AgentsTerminalBodyMountSlotId {
                pane_id,
                session_id: close_session_id,
            }) {
                close_requested = true;
                continue;
            }
            if self.agents_workspace.close_tab(pane_id, close_session_id) {
                self.forget_local_workspace_mappings_for_shell_session(close_session_id, cx);
                model_changed = true;
            }
        }

        if model_changed {
            self.scroll_workspace_pane_active_tab(self.agents_workspace.focused_pane);
            self.persist_shell_layout_state();
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        }
        if model_changed || close_requested {
            cx.notify();
        }
        model_changed || close_requested
    }

    pub(crate) fn close_agents_tabs_for_scope_from_action(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        scope_value: u8,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(scope) = AgentsWorkspaceTabCloseScope::from_action_value(scope_value) else {
            return;
        };
        self.close_agents_tabs_for_scope(pane_id, session_id, scope, cx);
    }

    pub(crate) fn sleep_agents_tabs_for_scope(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        scope: AgentsWorkspaceTabSleepScope,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:SessionSleep 2026-06-26-06:57:
        Agents context-menu Sleep keeps tabs in the main workspace instead of closing them. Direct Sleep may retarget active selection to an awake sibling from the clicked pane group; Sleep Right/Left/Others preserve shell focus and only mark resolved sessions sleeping so parked-owner detach happens through normal surface reconciliation.
        */
        let session_ids = self
            .agents_workspace
            .tab_session_ids_for_sleep_scope(pane_id, session_id, scope);
        if session_ids.is_empty() {
            return false;
        }

        let mut lifecycle_requested = false;
        let mut model_changed = false;
        for sleep_session_id in session_ids {
            let replacement_key = if scope == AgentsWorkspaceTabSleepScope::Sleep {
                self.agents_workspace
                    .replacement_session_after_direct_tab_sleep(pane_id, session_id)
                    .and_then(|replacement_session_id| {
                        self.workspace_terminal_key_for_shell_session(replacement_session_id)
                    })
            } else {
                None
            };
            let mutation_kind = if scope == AgentsWorkspaceTabSleepScope::Sleep {
                GpuiLocalWorkspaceLifecycleMutationKind::DirectSleep
            } else {
                GpuiLocalWorkspaceLifecycleMutationKind::ScopedSleep
            };
            if let Some(workspace_key) =
                self.workspace_terminal_key_for_shell_session(sleep_session_id)
            {
                let skip_replacement_fallback =
                    scope == AgentsWorkspaceTabSleepScope::Sleep && replacement_key.is_none();
                let requested = match workspace_key {
                    GpuiWorkspaceTerminalSessionKey::Local(_) => self
                        .request_local_workspace_terminal_lifecycle(
                            pane_id,
                            sleep_session_id,
                            GpuiLocalWorkspaceLifecycleAction::Sleep,
                            mutation_kind,
                            replacement_key
                                .clone()
                                .and_then(|key| key.as_local().cloned()),
                            skip_replacement_fallback,
                            None,
                            cx,
                        ),
                    GpuiWorkspaceTerminalSessionKey::Remote(_) => self
                        .request_remote_workspace_terminal_lifecycle(
                            pane_id,
                            sleep_session_id,
                            GpuiLocalWorkspaceLifecycleAction::Sleep,
                            mutation_kind,
                            replacement_key.clone().and_then(|key| match key {
                                GpuiWorkspaceTerminalSessionKey::Remote(key) => Some(key),
                                GpuiWorkspaceTerminalSessionKey::Local(_) => None,
                            }),
                            skip_replacement_fallback,
                            cx,
                        ),
                };
                if requested {
                    lifecycle_requested = true;
                }
                continue;
            }
            if self
                .agents_workspace
                .set_session_sleeping(sleep_session_id, true)
            {
                model_changed = true;
            }
        }
        if scope == AgentsWorkspaceTabSleepScope::Sleep
            && !lifecycle_requested
            && self
                .agents_workspace
                .select_replacement_after_direct_tab_sleep(pane_id, session_id)
        {
            model_changed = true;
        }

        if model_changed {
            self.scroll_workspace_pane_active_tab(self.agents_workspace.focused_pane);
            self.persist_shell_layout_state();
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
            cx.notify();
        }
        if lifecycle_requested {
            cx.notify();
        }
        model_changed || lifecycle_requested
    }

    pub(crate) fn sleep_agents_tabs_for_scope_from_action(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        scope_value: u8,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(scope) = AgentsWorkspaceTabSleepScope::from_action_value(scope_value) else {
            return;
        };
        self.sleep_agents_tabs_for_scope(pane_id, session_id, scope, cx);
    }

    pub(crate) fn toggle_agents_focus_mode_for_tab_from_action(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self
            .agents_workspace
            .toggle_focus_mode_from_tab_double_click(pane_id, session_id)
        {
            return;
        }
        self.set_shell_focus(ShellFocusTarget::AgentsPane(
            self.agents_workspace.focused_pane,
        ));
        self.scroll_workspace_pane_active_tab(self.agents_workspace.focused_pane);
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn request_close_agents_running_surface_if_mounted(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) -> bool {
        /*
        CDXC:Terminal 2026-06-23-04:49:
        User close on a real mounted Running Agents terminal asks Ghostty to run its normal close path instead of deleting the shell tab first. This is idempotent per surface and falls back to existing placeholder close behavior only when no exact current Running Ghostty owner exists.

        CDXC:Terminal 2026-06-26-23:59:
        Callers must resolve mapped workspace sessions before this helper. The helper is the terminal-owned close path for unmapped/local-only mounted surfaces, not the SidebarApp-owned close path for gxserver sessions.
        */
        if !self
            .agents_workspace
            .is_current_terminal_body_mount_slot(slot_id)
            || !self
                .agents_workspace
                .can_close_tab(slot_id.pane_id, slot_id.session_id)
        {
            return false;
        }

        #[cfg(target_os = "macos")]
        {
            if let Some(surface) = self.agents_terminal_ghostty_surfaces.get_mut(&slot_id) {
                surface.request_close();
                return true;
            }
        }

        false
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn consume_confirmed_agents_terminal_ghostty_surface_closes(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let confirmed_slots = confirmed_agents_terminal_ghostty_surface_close_slots(
            &self.agents_workspace,
            &self.agents_terminal_runtime_sessions,
            &self.agents_terminal_ghostty_surfaces,
        );
        let mut changed = false;
        for slot_id in confirmed_slots {
            let target_is_mapped = self
                .local_workspace_key_for_shell_session(slot_id.session_id)
                .is_some();
            if target_is_mapped {
                let replacement_key = self
                    .agents_workspace
                    .selected_session_after_direct_tab_close(slot_id.pane_id, slot_id.session_id)
                    .and_then(|replacement_session_id| {
                        self.local_workspace_key_for_shell_session(replacement_session_id)
                    });
                let skip_replacement_fallback = replacement_key.is_none();
                let _ = self.request_local_workspace_terminal_lifecycle(
                    slot_id.pane_id,
                    slot_id.session_id,
                    GpuiLocalWorkspaceLifecycleAction::Close,
                    GpuiLocalWorkspaceLifecycleMutationKind::DirectClose,
                    replacement_key,
                    skip_replacement_fallback,
                    Some(slot_id),
                    cx,
                );
                continue;
            }
            if self
                .agents_workspace
                .close_tab(slot_id.pane_id, slot_id.session_id)
            {
                self.agents_terminal_close_confirms
                    .pending_by_slot
                    .remove(&slot_id);
                self.forget_local_workspace_mappings_for_shell_session(slot_id.session_id, cx);
                changed = true;
            }
        }
        changed
    }

    /*
    CDXC:Terminal 2026-07-04:
    GPUI-engine close requests decide confirmation at request time from live
    process/prompt state (mirroring ghostty `needsConfirmQuit`: exited never
    confirms; `confirm-close-surface = true` skips confirmation at a
    shell-integration prompt). A confirmation-needed request becomes a
    pending entry for the same normal-layout banner the native path renders;
    a no-confirm request returns false so the caller's existing direct
    model-close path runs and the engine record is pruned by sync.
    */
    pub(crate) fn request_close_agents_gpui_engine_terminal(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self
            .agents_workspace
            .is_current_terminal_body_mount_slot(slot_id)
            || !self
                .agents_workspace
                .can_close_tab(slot_id.pane_id, slot_id.session_id)
        {
            return false;
        }
        let Some(record) = self.agents_gpui_engine_terminals.get(&slot_id.session_id) else {
            return false;
        };
        let behavior = record.confirm_close_behavior;
        let view = record.view.clone();
        if view.update(cx, |view, _cx| view.needs_confirm_close(behavior)) {
            self.agents_gpui_engine_close_confirms.insert(slot_id);
            true
        } else {
            false
        }
    }
}
