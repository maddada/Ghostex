//! Shell focus, first-responder reconciliation, CEF keyboard ownership and hotkey passthrough, sidebar focus border handoff, leaf borders and the default focus per mode.

use std::time::Instant;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::App;
use gpui::Focusable as _;
use gpui::Window;

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
                if self.reconcile_shell_focus_with_first_responder_target() {
                    self.persist_shell_layout_state();
                    cx.notify();
                }
            }
            return;
        }
        self.command_pane_auto_minimize.idle_since = None;
        self.first_responder_target = target;
        self.first_responder_transition_suppressed_by_programmatic_focus =
            suppressed_by_programmatic_focus;
        self.reconcile_project_workarea_cef_keyboard_ownership(window, cx);
        self.reconcile_browser_cef_keyboard_ownership(window, cx);
        if !suppressed_by_programmatic_focus {
            if self.reconcile_shell_focus_with_first_responder_target() {
                self.persist_shell_layout_state();
            }
            self.reconcile_sidebar_focus_border_handoff_after_responder_transition();
        }
        cx.notify();
    }

    /// CDXC:FocusRouting 2026-09-14 WHY:
    /// Publishing session selections from native responder observations caused the sidepane beside a view to alternate between the outgoing and incoming sessions through sidebar focus echoes.
    /// Responder observations only record shell focus; explicit session actions own selection and sidebar publication.
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

        match self.first_responder_target {
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
            )) if self.view_page_on_screen(slot_key.titlebar_mode()) => {
                self.set_shell_focus(ShellFocusTarget::ProjectEditorSurface(
                    slot_key.titlebar_mode(),
                ));
            }
            _ => return false,
        }

        self.shell_focus != previous_focus
            || self.browser_tabs.focused_pane != previous_browser_pane
            || self.browser_tabs.active_tab != previous_browser_tab
    }

    /// Records shell focus for the Agents pane that shows `session_id`'s chat, without a keyboard handoff.
    /// Shared by the CEF responder observation and the native composer's focus edge.
    /// Returns `None` when no visible pane shows that chat, otherwise whether shell focus changed.
    pub(crate) fn record_shell_focus_for_session_chat(
        &mut self,
        session_id: TerminalSessionId,
    ) -> Option<bool> {
        let previous_focus = self.shell_focus;
        let pane_id = self.agents_workspace.pane_id_for_session(session_id)?;
        if self.agents_workspace.active_session_in_pane(pane_id) != Some(session_id) {
            return None;
        }
        self.agents_workspace.focus_pane(pane_id);
        self.set_shell_focus(ShellFocusTarget::AgentsPane(pane_id));
        Some(self.shell_focus != previous_focus)
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn source_workarea_cef_owns_native_focus(&self) -> bool {
        matches!(
            self.first_responder_target,
            FirstResponderTarget::CefSurface(FirstResponderCefSurface::ProjectWorkarea(
                ProjectWorkareaCefSurfaceSlotKey::Source
                    | ProjectWorkareaCefSurfaceSlotKey::CodeWindow(_)
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
                ProjectWorkareaCefSurfaceSlotKey::Source
                    | ProjectWorkareaCefSurfaceSlotKey::CodeWindow(_)
                    | ProjectWorkareaCefSurfaceSlotKey::Manage
            ))
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
        /*
        CDXC:GPUICefFirstResponderPastePassthrough 2026-08-27:
        Every CEF surface qualifies, not just the editable Source/Manage/Chat
        renderers: `shell_focus` keeps naming a terminal pane while a modal,
        the sidebar, a Kanban page, a titlebar popup, or a browser tab holds
        AppKit's first responder, so without this gate the Cmd+V binding
        shadow-pastes the same clipboard into that pane's hidden terminal
        composer. That phantom draft is what a later chat send's
        draft-preservation step sweeps into Saved Prompts. A Chromium first
        responder always owns its own paste.
        */
        #[cfg(target_os = "macos")]
        let renderer_edit_cef_owns_native_focus = matches!(
            self.first_responder_target,
            FirstResponderTarget::CefSurface(_)
        );
        #[cfg(not(target_os = "macos"))]
        let renderer_edit_cef_owns_native_focus = false;

        if !renderer_edit_cef_owns_native_focus {
            return false;
        }
        cx.propagate();
        true
    }

    /// CDXC:Clipboard 2026-10-12 WHY: the app-wide Cmd+V binding outranks the `Input` context's own `cmd-v` (a binding without a context matches at the deepest depth and binds later), so a focused GPUI text field (chat composer, sidebar rename, terminal search, address bar) pastes only when this passes the chord on to its `Paste`. A native Ghostty surface holding AppKit's responder keeps its paste even if GPUI focus stayed on a field.
    pub(crate) fn propagate_gpui_text_input_paste_passthrough(
        &self,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        #[cfg(target_os = "macos")]
        if self.first_responder_target != FirstResponderTarget::GpuiWindow {
            return false;
        }
        if !window
            .context_stack()
            .iter()
            .any(|context| context.contains("Input"))
        {
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

        if let Some(surface) = self.cef_surface_containing_responder(responder, cx) {
            return FirstResponderTarget::CefSurface(surface);
        }
        if cef::native_view_contains_responder(self.parent_ns_view, responder) {
            return FirstResponderTarget::GpuiWindow;
        }
        FirstResponderTarget::Other
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn cef_surface_containing_responder(
        &self,
        responder: *mut std::ffi::c_void,
        cx: &mut gpui::Context<Self>,
    ) -> Option<FirstResponderCefSurface> {
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
        if let Some(handle) = self.app_modal_window
            && handle
                .update(cx, |host, _window, cx| {
                    host.surface.as_ref().is_some_and(|surface| {
                        surface.read(cx).native_view_contains_responder(responder)
                    })
                })
                .unwrap_or(false)
        {
            return Some(FirstResponderCefSurface::AppModal);
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
        if self.shell_focus != focus {
            self.command_pane_auto_minimize.idle_since = None;
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
        if self
            .pending_keyboard_handoff
            .is_some_and(|pending| pending.target != focus)
        {
            self.pending_keyboard_handoff = None;
        }
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
        if self
            .sidebar_focus_border_handoff
            .and_then(|handoff| handoff.target_session_id)
            .is_none()
        {
            return;
        }
        if self.first_responder_target == FirstResponderTarget::GpuiWindow {
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
        if leaf
            .tab_group
            .active_session_id()
            .is_some_and(|session_id| self.gx_store_shell_session_has_attention(session_id))
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
            FirstResponderTarget::CefSurface(_)
            | FirstResponderTarget::Other
            | FirstResponderTarget::None => false,
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

    /// CDXC:FocusRouting 2026-09-11 DECISION:
    /// User: typing must reach the pane that is visible in front of them after the command pane is expanded and hidden again (F12, the chevron, or any other route), without clicking to re-activate it.
    /// `set_shell_focus` only records intent, so hiding the command pane left AppKit first responder on the GPUI root and GPUI focus on the now-unrendered command terminal: chat panes received nothing and terminal panes lost Enter, Backspace, arrows, and chords.
    /// Restore therefore always ends with the same keyboard handoff the click, tab-select, and mode-switch routes perform.
    pub(crate) fn restore_previous_non_command_focus_or_default(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let focus = restored_non_command_shell_focus_or_default_with_browser_tabs(
            self.previous_non_command_focus,
            self.active_mode,
            &self.agents_workspace,
            &self.project_editor_shell,
            &self.browser_tabs,
        );
        self.focus_shell_target(focus, cx);
    }

    pub(crate) fn focus_default_surface_for_active_mode(&mut self, cx: &mut gpui::Context<Self>) {
        // With the side panel closed, Claude's page is what is on screen in place of the sessions
        // (app/claude_web/takeover.rs).
        if self.active_mode == TitlebarMode::Agents
            && !self.view_panel_picker_open
            && self.claude_page_on_screen()
        {
            self.focus_shell_target(
                ShellFocusTarget::ProjectEditorSurface(TitlebarMode::Extension(
                    crate::app::claude_web::claude_web_view_id(),
                )),
                cx,
            );
            return;
        }
        self.focus_shell_target(
            default_shell_focus_for_mode(
                self.active_mode,
                &self.agents_workspace,
                &self.project_editor_shell,
            ),
            cx,
        );
    }
}
