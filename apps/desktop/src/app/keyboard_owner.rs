//! Keyboard ownership for the desktop shell: one intent, one resolver, one render-time handoff.
//!
//! CDXC:FocusRouting 2026-09-11 WHY:
//! The shell used to keep four separately maintained truths about who owns typing: the shell-focus model, GPUI's focused handle, the AppKit first responder, and the macOS keyboard-router owner.
//! Only clicks fed the physical truths back into the model; every programmatic focus change had to remember to arm its own per-surface handoff slot, and the ones that forgot (hiding the command pane, closing its last tab, closing the floating companion) left the keyboard on the previous owner.
//! Now `focus_shell_target` is the only way to move focus programmatically, `shell_keyboard_owner` is the only place that decides which surface occupies the focused target, and `drain_pending_keyboard_handoff` is the only code that performs the physical handoff.
//! Physical focus changes the model in one place only: `reconcile_shell_focus_with_first_responder_target`, fed by the AppKit first-responder observer after a user click.
//! SEE-ALSO: model/focus_and_keyboard.rs (`PendingKeyboardHandoff`, `ShellKeyboardOwner`), focus/shell_focus.rs (`set_shell_focus_with_terminal_handoff` staleness), terminal_input/text_input_handoff.rs (`focus_gpui_engine_terminal_view`), helpers/os_cli/keyboard_router.rs.

use gpui::Focusable as _;
use gpui::Window;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    /// Move shell focus and hand the keyboard to whatever occupies the target on the next render.
    /// This is the programmatic focus API; `set_shell_focus` alone only records intent and is reserved for the responder-driven reconcile and for chrome whose GPUI input owns the keys itself (address bar, search inputs).
    pub(crate) fn focus_shell_target(
        &mut self,
        target: ShellFocusTarget,
        cx: &mut gpui::Context<Self>,
    ) {
        self.set_shell_focus(target);
        self.request_keyboard_handoff_for_shell_focus(cx);
    }

    /// `focus_shell_target` for callers that already hold the window: the handoff runs immediately instead of on the next render.
    pub(crate) fn focus_shell_target_now(
        &mut self,
        target: ShellFocusTarget,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.focus_shell_target(target, cx);
        self.drain_pending_keyboard_handoff(window, cx);
    }

    pub(crate) fn request_keyboard_handoff(&mut self, request: PendingKeyboardHandoff) {
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.terminalFocus.keyboardHandoffRequested",
            serde_json::json!({
                "target": format!("{:?}", request.target),
                "sessionId": request.session_id.map(|id| id.0),
                "commandSessionId": request.command_session_id.map(|id| id.0),
                "shellFocus": format!("{:?}", self.shell_focus),
                "firstResponderTarget": format!("{:?}", self.first_responder_target),
            }),
        );
        self.pending_keyboard_handoff = Some(request);
        self.pending_keyboard_handoff_returns_from_modal = false;
    }

    /// Ask for the keyboard to follow the current shell focus. The occupant is resolved when the handoff runs, not now, so a pane that is still mounting or a chat page that is still loading is waited for rather than skipped.
    pub(crate) fn request_keyboard_handoff_for_shell_focus(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let request = match self.shell_focus {
            ShellFocusTarget::AgentsPane(pane_id) => PendingKeyboardHandoff {
                target: self.shell_focus,
                session_id: self.agents_workspace.active_session_in_pane(pane_id),
                command_session_id: None,
            },
            ShellFocusTarget::CommandPane => PendingKeyboardHandoff {
                target: self.shell_focus,
                session_id: None,
                command_session_id: self
                    .command_pane
                    .focused_group_active_session_id()
                    .map(|(_group_id, session_id)| session_id),
            },
            ShellFocusTarget::BrowserSurface
            | ShellFocusTarget::BrowserPane(_)
            | ShellFocusTarget::ProjectEditorSurface(_) => PendingKeyboardHandoff {
                target: self.shell_focus,
                session_id: None,
                command_session_id: None,
            },
        };
        self.request_keyboard_handoff(request);
        if request
            .session_id
            .is_some_and(|session_id| self.agents_chat_mode_sessions.contains(&session_id))
        {
            // The chat page must exist before its composer can report ready.
            self.reconcile_agents_pane_surfaces(cx);
        }
        cx.notify();
    }

    /// Ask for the keyboard to reach a specific session once its pane shows it (chat launch, chat mode switch). Drops silently when the session's pane is not the shell-focused one by the time the handoff runs.
    pub(crate) fn request_keyboard_handoff_for_session(&mut self, session_id: TerminalSessionId) {
        let target = self
            .agents_workspace
            .pane_id_for_session(session_id)
            .map(ShellFocusTarget::AgentsPane);
        let Some(target) = target else {
            return;
        };
        self.request_keyboard_handoff(PendingKeyboardHandoff {
            target,
            session_id: Some(session_id),
            command_session_id: None,
        });
    }

    pub(crate) fn pending_keyboard_handoff_targets_session(
        &self,
        session_id: TerminalSessionId,
    ) -> bool {
        self.pending_keyboard_handoff
            .is_some_and(|pending| pending.session_id == Some(session_id))
    }

    pub(crate) fn drop_pending_keyboard_handoff_for_session(
        &mut self,
        session_id: TerminalSessionId,
    ) {
        if self.pending_keyboard_handoff_targets_session(session_id) {
            self.pending_keyboard_handoff = None;
        }
    }

    /// The session whose surface physically owns the keyboard right now, if any.
    /// A composited terminal counts only while the GPUI window is the responder; a click into the sidebar or a modal leaves GPUI focus on the terminal but moves the responder away.
    pub(crate) fn keyboard_owner_session(&self) -> Option<KeyboardOwnerSession> {
        #[cfg(target_os = "macos")]
        {
            match self.first_responder_target {
                FirstResponderTarget::GpuiWindow => {}
                FirstResponderTarget::CefSurface(_)
                | FirstResponderTarget::Other
                | FirstResponderTarget::None => return None,
            }
        }
        match self.composited_terminal_keyboard_owner {
            Some((_, GpuiEngineTerminalEventTarget::Agents(session_id))) => {
                Some(KeyboardOwnerSession::Agents(session_id))
            }
            Some((_, GpuiEngineTerminalEventTarget::Command(session_id))) => {
                Some(KeyboardOwnerSession::Command(session_id))
            }
            None => None,
        }
    }

    pub(crate) fn keyboard_owner_session_was_removed(
        &self,
        before: Option<KeyboardOwnerSession>,
    ) -> bool {
        match before {
            Some(KeyboardOwnerSession::Agents(session_id)) => {
                !self.agents_workspace.has_session(session_id)
            }
            Some(KeyboardOwnerSession::Command(session_id)) => {
                self.command_pane.session(session_id).is_none()
            }
            None => false,
        }
    }

    /// CDXC:FocusRouting 2026-09-11 WHY:
    /// A tab removed by something other than the user (process exit, close-after-done, a failed attach) must not pull the keyboard out of whatever the user is typing into.
    /// Shell focus follows the model only when it pointed at the removed surface's pane, and the physical handoff runs only when the removed tab was the keyboard owner captured before the mutation.
    pub(crate) fn follow_shell_focus_after_surface_removed(
        &mut self,
        target: ShellFocusTarget,
        keyboard_owner_before: Option<KeyboardOwnerSession>,
        cx: &mut gpui::Context<Self>,
    ) {
        let removed_owned_keyboard = self.keyboard_owner_session_was_removed(keyboard_owner_before);
        let shell_focus_on_same_surface = match target {
            ShellFocusTarget::CommandPane => self.shell_focus == ShellFocusTarget::CommandPane,
            ShellFocusTarget::AgentsPane(_) => {
                matches!(self.shell_focus, ShellFocusTarget::AgentsPane(_))
            }
            ShellFocusTarget::BrowserSurface
            | ShellFocusTarget::BrowserPane(_)
            | ShellFocusTarget::ProjectEditorSurface(_) => self.shell_focus == target,
        };
        if !shell_focus_on_same_surface && !removed_owned_keyboard {
            return;
        }
        if target == ShellFocusTarget::CommandPane {
            self.remember_current_non_command_focus();
        }
        self.set_shell_focus(target);
        if removed_owned_keyboard {
            self.request_keyboard_handoff_for_shell_focus(cx);
        }
    }

    /// The command pane lost its last tab to a non-user event: leave the command surface in the model, and pull the keyboard along only if that tab had it.
    pub(crate) fn restore_non_command_focus_after_surface_removed(
        &mut self,
        keyboard_owner_before: Option<KeyboardOwnerSession>,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.shell_focus != ShellFocusTarget::CommandPane {
            return;
        }
        let focus = restored_non_command_shell_focus_or_default_with_browser_tabs(
            self.previous_non_command_focus,
            self.active_mode,
            &self.agents_workspace,
            &self.project_editor_shell,
            &self.browser_tabs,
        );
        self.set_shell_focus(focus);
        if self.keyboard_owner_session_was_removed(keyboard_owner_before) {
            self.request_keyboard_handoff_for_shell_focus(cx);
        }
    }

    /// The single resolver: which surface occupies the shell-focused target right now.
    /// Chat mode wins over the parked terminal of the same session, a mounting terminal is reported as waiting rather than absent, and sleeping placeholders own nothing so the root keeps wake-on-type.
    pub(crate) fn shell_keyboard_owner(&self) -> ShellKeyboardOwner {
        match self.shell_focus {
            ShellFocusTarget::AgentsPane(pane_id) => {
                if !self.agents_workspace_visible() {
                    return ShellKeyboardOwner::Nothing;
                }
                let Some(session_id) = self.agents_workspace.active_session_in_pane(pane_id) else {
                    return ShellKeyboardOwner::Nothing;
                };
                let mount = match self.focused_terminal_text_mount_target() {
                    Some(mount @ FocusedTerminalTextMountTarget::Agents(slot_id))
                        if slot_id.session_id == session_id =>
                    {
                        Some(mount)
                    }
                    _ => None,
                };
                self.agents_session_keyboard_owner(session_id, mount)
            }
            ShellFocusTarget::CommandPane => {
                let Some(FocusedTerminalTextMountTarget::Command(slot_id)) =
                    self.focused_terminal_text_mount_target()
                else {
                    return ShellKeyboardOwner::Nothing;
                };
                if self
                    .command_pane
                    .session(slot_id.session_id)
                    .is_none_or(|session| session.is_sleeping)
                {
                    return ShellKeyboardOwner::Nothing;
                }
                if let Some(record) = self.command_gpui_engine_terminals.get(&slot_id.session_id) {
                    return ShellKeyboardOwner::EngineTerminal {
                        target: GpuiEngineTerminalEventTarget::Command(slot_id.session_id),
                        view: record.view.clone(),
                        session_id: None,
                    };
                }
                ShellKeyboardOwner::TerminalMounting
            }
            ShellFocusTarget::BrowserPane(pane_id) => {
                if self.active_mode == TitlebarMode::Browser
                    && self.browser_tabs.find_leaf(pane_id).is_some()
                {
                    ShellKeyboardOwner::BrowserPage(pane_id)
                } else {
                    ShellKeyboardOwner::Nothing
                }
            }
            ShellFocusTarget::BrowserSurface => {
                if self.active_mode == TitlebarMode::Browser {
                    ShellKeyboardOwner::BrowserPage(self.browser_tabs.focused_pane)
                } else {
                    ShellKeyboardOwner::Nothing
                }
            }
            ShellFocusTarget::ProjectEditorSurface(mode) => {
                if mode == TitlebarMode::Agents {
                    // The view panel with no view in it: the picker owns the keys, or nothing does.
                    if self.view_picker_open() {
                        ShellKeyboardOwner::GpuiViewPanelSurface
                    } else {
                        ShellKeyboardOwner::Nothing
                    }
                } else if !self.view_page_on_screen(mode) {
                    ShellKeyboardOwner::Nothing
                } else if mode == TitlebarMode::SideChat {
                    // The side chat on screen takes the keys in its composer, as an Agents chat does.
                    match self.side_panel_chat_on_screen_shell_session() {
                        Some(session_id) => ShellKeyboardOwner::ChatComposer(session_id),
                        None => ShellKeyboardOwner::GpuiViewPanelSurface,
                    }
                } else if self.website_home_setup_visible(mode)
                    || mode == TitlebarMode::Kanban
                    || mode == TitlebarMode::Manage
                {
                    // Kanban and Files are drawn natively: no page to hand keys to.
                    ShellKeyboardOwner::GpuiViewPanelSurface
                } else if mode.is_project_editor_mode() {
                    ShellKeyboardOwner::WorkareaPage(mode)
                } else {
                    ShellKeyboardOwner::GpuiViewPanelSurface
                }
            }
        }
    }

    fn agents_session_keyboard_owner(
        &self,
        session_id: TerminalSessionId,
        mount: Option<FocusedTerminalTextMountTarget>,
    ) -> ShellKeyboardOwner {
        if self.agents_chat_mode_sessions.contains(&session_id) {
            return ShellKeyboardOwner::ChatComposer(session_id);
        }
        let Some(session) = self.agents_workspace.session(session_id) else {
            return ShellKeyboardOwner::Nothing;
        };
        match session.presentation_state {
            TerminalSessionPresentationState::Running => {}
            TerminalSessionPresentationState::Mounting => {
                return ShellKeyboardOwner::TerminalMounting;
            }
            TerminalSessionPresentationState::Sleeping
            | TerminalSessionPresentationState::StartupFailed
            | TerminalSessionPresentationState::RestoredUnmounted
            | TerminalSessionPresentationState::PoppedOutPlaceholder => {
                return ShellKeyboardOwner::Nothing;
            }
        }
        if mount.is_none() {
            // Running but not rendered in the focused pane (focus mode or an inactive tab): nothing to focus.
            return ShellKeyboardOwner::Nothing;
        }
        if let Some(record) = self.agents_gpui_engine_terminals.get(&session_id) {
            return ShellKeyboardOwner::EngineTerminal {
                target: GpuiEngineTerminalEventTarget::Agents(session_id),
                view: record.view.clone(),
                session_id: Some(session_id),
            };
        }
        ShellKeyboardOwner::TerminalMounting
    }

    /// Whether a GPUI text field of the main window's own chrome holds the keyboard.
    fn main_window_text_field_owns_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        self.native_sidebar_input_owns_focus(window, cx)
            || self.terminal_search_input_owns_keyboard_focus(window, cx)
            || self.browser_find_input_owns_keyboard_focus(window, cx)
            || self
                .browser_address_inputs
                .values()
                .any(|input| input.read(cx).focus_handle(cx).is_focused(window))
    }

    /// The render-time handoff. Runs once per request: it is dropped when shell focus moved or the requested tab is no longer in front, kept while the occupant is still mounting or loading, and executed exactly once otherwise.
    pub(crate) fn drain_pending_keyboard_handoff(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(pending) = self.pending_keyboard_handoff else {
            return;
        };
        // CDXC:FocusRouting 2026-09-25 WHY:
        // An open dialog owns the keyboard. A handoff asked for before it opened (a new browser tab
        // whose page was still being created, a pane still mounting) used to land when its surface
        // arrived, focusing that surface under the dialog: typing into Add Worktree went nowhere
        // until the pane was hidden. It waits for the dialog to close, then lands as it would have.
        if self.native_app_modal.is_some() || self.app_modal_window.is_some() {
            return;
        }
        /*
        CDXC:FocusRouting 2026-09-26 WHY:
        Quick Access and the new-thread picker close when they lose activation, through the same path as Escape. A click into a main-window text field (sidebar rename, terminal search, browser find or address bar) dismisses them, and the modal's return handoff must not pull the keyboard back out of that field.
        */
        if std::mem::take(&mut self.pending_keyboard_handoff_returns_from_modal)
            && self.main_window_text_field_owns_focus(window, cx)
        {
            self.pending_keyboard_handoff = None;
            return;
        }
        if pending.target != self.shell_focus {
            self.pending_keyboard_handoff = None;
            return;
        }
        if let Some(session_id) = pending.session_id {
            let still_in_front = match pending.target {
                ShellFocusTarget::AgentsPane(pane_id) => {
                    self.agents_workspace.active_session_in_pane(pane_id) == Some(session_id)
                }
                _ => true,
            };
            if !still_in_front {
                self.pending_keyboard_handoff = None;
                return;
            }
        }
        if let Some(command_session_id) = pending.command_session_id
            && self
                .command_pane
                .focused_group_active_session_id()
                .map(|(_group_id, session_id)| session_id)
                != Some(command_session_id)
        {
            self.pending_keyboard_handoff = None;
            return;
        }
        match self.shell_keyboard_owner() {
            ShellKeyboardOwner::EngineTerminal {
                target,
                view,
                session_id,
            } => {
                self.pending_keyboard_handoff = None;
                self.focus_gpui_engine_terminal_view(target, &view, window, cx);
                if let Some(session_id) = session_id {
                    self.deliver_pending_session_terminal_composer_insert(session_id, cx);
                }
            }
            ShellKeyboardOwner::TerminalMounting => {
                self.hold_unfocused_keyboard_on_root(window, cx);
            }
            ShellKeyboardOwner::ChatComposer(session_id) => {
                if self
                    .native_chat_view_for_shell_session(session_id)
                    .is_none()
                {
                    // Native input can take focus before its background runtime restores the draft.
                    self.hold_unfocused_keyboard_on_root(window, cx);
                    return;
                }
                self.pending_keyboard_handoff = None;
                self.focus_session_chat_composer(session_id, window, cx);
            }
            ShellKeyboardOwner::BrowserPage(pane_id) => {
                if self.browser_surface_for_pane(pane_id).is_none()
                    && self.active_loaded_browser_tab_for_pane(pane_id).is_some()
                {
                    // The loaded tab's CEF surface is still being materialized.
                    return;
                }
                self.pending_keyboard_handoff = None;
                self.focus_browser_pane_surface(pane_id, window, cx);
            }
            ShellKeyboardOwner::WorkareaPage(mode) => {
                if !self.project_editor_shell.is_mode_awake(mode) {
                    // A sleeping workarea has no page to type into; its click-to-wake body owns the next activation.
                    self.pending_keyboard_handoff = None;
                    return;
                }
                // Code keeps a page per window; the one on screen takes the keys.
                let shown_code_slot = self.active_code_editor_slot();
                let Some(surface) = self
                    .project_workarea_runtime_cef_surfaces
                    .iter()
                    .find(|(slot_key, _)| {
                        slot_key.titlebar_mode() == mode
                            && (!slot_key.is_code_editor() || **slot_key == shown_code_slot)
                    })
                    .map(|(_, owned_surface)| owned_surface.surface.clone())
                else {
                    // The surface is still being created.
                    return;
                };
                self.pending_keyboard_handoff = None;
                let focus_handle = surface.read(cx).focus_handle.clone();
                focus_handle.focus(window, cx);
                surface.update(cx, |surface, _| surface.focus());
            }
            ShellKeyboardOwner::GpuiViewPanelSurface => {
                self.pending_keyboard_handoff = None;
                // The page is GPUI's own drawing, so the keys come back to the root view; the
                // terminal surface sync releases a Ghostty first responder in the same pass,
                // because shell focus no longer names an Agents pane.
                self.reclaim_gpui_root_for_chrome_input_focus();
            }
            ShellKeyboardOwner::Nothing => {
                self.pending_keyboard_handoff = None;
                self.hold_unfocused_keyboard_on_root(window, cx);
            }
        }
    }

    /// CDXC:FocusRouting 2026-10-10 WHY:
    /// GPUI sends keys along the focused element's path, and with no live focus (the focused terminal's tab was closed, or a hidden one was released) only the window's outermost element sees them, above the root that holds every app hotkey and the wake-on-type and terminal forwarding listeners. A handoff with nothing to focus yet (a sleeping or empty pane, a terminal still mounting, a chat still loading) therefore parks the keyboard on that root's own handle, so hotkeys keep working until the surface arrives and the pending handoff moves it there. A focus that is still live (a sidebar rename, a search field) is left alone.
    pub(crate) fn hold_unfocused_keyboard_on_root(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if window.focused(cx).is_none() {
            window.focus(&self.root_action_focus_handle, cx);
        }
    }

    /// Makes the main window key again once an app modal's window is gone.
    ///
    /// CDXC:FocusRouting 2026-09-30 DECISION:
    /// User: after Cmd+P then Escape, or closing a modal with its X, typing did not reach the chat and Cmd+P did nothing until they clicked the window; "we need to focus the main window of the app automatically in all these cases".
    /// The modal is its own key window, and AppKit does not hand key status back to the main window when it closes, so the in-window handoff focused the composer of a window that received no keys.
    /// Skipped while another modal is up (a Quick Access action that opens one) and when Ghostex is no longer the active app (the modal closed because the user switched apps), so it never pulls Ghostex forward.
    pub(crate) fn activate_main_window_after_app_modal(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(main) = self.main_window_handle else {
            return;
        };
        let app = cx.weak_entity();
        cx.defer(move |cx| {
            let another_modal_open = app.read_with(cx, |app, _| {
                app.native_app_modal.is_some() || app.app_modal_window.is_some()
            });
            if another_modal_open.unwrap_or(true) || !ghostex_application_is_active() {
                return;
            }
            let _ = main.update(cx, |_, window, _| {
                if !window.is_window_active() {
                    window.activate_window();
                }
            });
        });
    }

    /// Physical half of a chat handoff: the chat view's composer, then any draft waiting for that composer.
    pub(crate) fn focus_session_chat_composer(
        &mut self,
        session_id: TerminalSessionId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // A Claude cloud session's pane is its claude.ai page, which takes the keys instead.
        if let Some(surface) = self.claude_cloud_chat_page_surface(session_id) {
            let focus_handle = surface.read(cx).focus_handle.clone();
            focus_handle.focus(window, cx);
            surface.update(cx, |surface, _| surface.focus());
            return;
        }
        let Some(view) = self.native_chat_view_for_shell_session(session_id) else {
            return;
        };
        self.reclaim_gpui_root_for_native_chat_composer(window);
        view.update(cx, |view, cx| {
            view.focus_requested = true;
            view.ensure_input(window, cx);
        });
        self.deliver_held_session_chat_composer_text(session_id, cx);
    }

    /// Physical half of a browser handoff: the pane's page surface takes GPUI and native focus; a pane without a page blurs GPUI chrome so the address bar does not keep the caret.
    pub(crate) fn focus_browser_pane_surface(
        &mut self,
        pane_id: BrowserPaneId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(surface) = self.browser_surface_for_pane(pane_id) {
            let focus_handle = surface.read(cx).focus_handle.clone();
            focus_handle.focus(window, cx);
            surface.update(cx, |surface, _| surface.focus());
        } else {
            window.blur(cx);
        }
    }

    /// Whether a composited terminal is on screen right now (its pane rendered, its tab active, not replaced by Chat, the command pane expanded).
    pub(crate) fn engine_terminal_target_is_displayed(
        &self,
        target: GpuiEngineTerminalEventTarget,
    ) -> bool {
        match target {
            GpuiEngineTerminalEventTarget::Agents(session_id) => {
                if self.agents_chat_mode_sessions.contains(&session_id) {
                    return false;
                }
                self.agents_workspace_visible()
                    && self
                        .agents_workspace
                        .rendered_terminal_body_mount_slots()
                        .iter()
                        .any(|slot_id| slot_id.session_id == session_id)
            }
            GpuiEngineTerminalEventTarget::Command(session_id) => {
                // Rendered slots are already limited to the docks on screen.
                self.command_pane
                    .rendered_terminal_body_mount_slots()
                    .iter()
                    .any(|slot_id| slot_id.session_id == session_id)
            }
        }
    }

    /// CDXC:FocusRouting 2026-10-25 WHY:
    /// The Browser's page row keeps its focus handle while the row is gone (its last page closed, the Browser view closed, or `browserPagesInViewRow` turned off), and GPUI then sends the keys to the window's outermost element, past the view picker's keys and the app's actions. Release it here, on the render that no longer shows the row, as hidden terminals are released below; with the Browser still on screen the keys go back to its page.
    pub(crate) fn sync_browser_page_row_focus(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.browser_page_row_focus_handle.is_focused(window) {
            return;
        }
        let browser_shown = self.active_mode == TitlebarMode::Browser
            && self
                .project_editor_shell
                .is_mode_awake(TitlebarMode::Browser);
        if browser_shown && self.browser_pages_in_view_row {
            return;
        }
        window.focus(&self.root_action_focus_handle, cx);
        // A view or panel change asks for its own handoff; turning the row off leaves the Browser
        // on screen with nothing asked for.
        if browser_shown {
            self.request_keyboard_handoff_for_shell_focus(cx);
        }
    }

    /// CDXC:FocusRouting 2026-09-11 WHY:
    /// A composited terminal reports its blur from its own prepaint, so a terminal that simply stops rendering (command pane hidden, tab switched away, pane replaced by Chat) never reports one.
    /// The keyboard router then keeps treating it as the owner, and Tab plus owner-gated hotkeys keep going to a terminal nobody can see. Release it here, on the render that no longer shows it.
    pub(crate) fn sync_composited_terminal_keyboard_owner(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some((root, target)) = self.composited_terminal_keyboard_owner else {
            return;
        };
        if self.engine_terminal_target_is_displayed(target) {
            return;
        }
        self.composited_terminal_keyboard_owner = None;
        #[cfg(target_os = "macos")]
        update_gpui_keyboard_router_composited_terminal_focus(
            root as *mut std::ffi::c_void,
            target,
            false,
            self.first_responder_target,
        );
        #[cfg(not(target_os = "macos"))]
        let _ = root;
        // Onto the root's handle rather than nowhere (`hold_unfocused_keyboard_on_root`): a blur
        // left only the window's outermost element on the key path, so hotkeys stopped working.
        if let Some(view) = self.gpui_engine_terminal_view_for_target(target)
            && view.read(cx).focus_handle(cx).is_focused(window)
        {
            window.focus(&self.root_action_focus_handle, cx);
        }
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.terminalFocus.hiddenCompositedTerminalReleased",
            serde_json::json!({
                "target": format!("{target:?}"),
                "shellFocus": format!("{:?}", self.shell_focus),
            }),
        );
    }
}

/// Whether Ghostex is the frontmost app. Only macOS leaves the main window un-keyed after a modal
/// closes, so other platforms never re-activate it.
#[cfg(target_os = "macos")]
fn ghostex_application_is_active() -> bool {
    unsafe extern "C" {
        fn GhostexGpuiApplicationIsActive() -> bool;
    }
    unsafe { GhostexGpuiApplicationIsActive() }
}

#[cfg(not(target_os = "macos"))]
fn ghostex_application_is_active() -> bool {
    false
}
