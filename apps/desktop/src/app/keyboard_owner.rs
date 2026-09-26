//! Keyboard ownership for the desktop shell: one intent, one resolver, one render-time handoff.
//!
//! CDXC:FocusRouting 2026-09-11 WHY:
//! The shell used to keep four separately maintained truths about who owns typing: the shell-focus model, GPUI's focused handle, the AppKit first responder, and the macOS keyboard-router owner.
//! Only clicks fed the physical truths back into the model; every programmatic focus change had to remember to arm its own per-surface handoff slot, and the ones that forgot (hiding the command pane, closing its last tab, closing the floating companion) left the keyboard on the previous owner.
//! Now `focus_shell_target` is the only way to move focus programmatically, `shell_keyboard_owner` is the only place that decides which surface occupies the focused target, and `drain_pending_keyboard_handoff` is the only code that performs the physical handoff.
//! Physical focus changes the model in one place only: `reconcile_shell_focus_with_first_responder_target`, fed by the AppKit first-responder observer after a user click.
//! SEE-ALSO: model/focus_and_keyboard.rs (`PendingKeyboardHandoff`, `ShellKeyboardOwner`), focus.rs (`set_shell_focus_with_terminal_handoff` staleness), terminal_input.rs (`focus_gpui_engine_terminal_view`), helpers/os_cli/keyboard_router.rs.

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
        self.schedule_untouched_agent_chat_review(cx);
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
                FirstResponderTarget::TerminalSurface(FirstResponderTerminalSurface::Agents(
                    session_id,
                )) => return Some(KeyboardOwnerSession::Agents(session_id)),
                FirstResponderTarget::TerminalSurface(FirstResponderTerminalSurface::Command(
                    session_id,
                )) => return Some(KeyboardOwnerSession::Command(session_id)),
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
                let Some(mount @ FocusedTerminalTextMountTarget::Command(slot_id)) =
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
                #[cfg(target_os = "macos")]
                if self.command_terminal_ghostty_surface_matches(slot_id) {
                    return ShellKeyboardOwner::NativeTerminal(mount);
                }
                let _ = mount;
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
                } else if self.active_mode != mode {
                    ShellKeyboardOwner::Nothing
                } else if self.website_home_setup_visible(mode)
                    || mode == TitlebarMode::Kanban
                    || (mode == TitlebarMode::Manage
                        && crate::app::native_docs::render::native_docs_enabled())
                {
                    // Kanban and native Docs are drawn natively: no page to hand keys to.
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
        let Some(mount) = mount else {
            // Running but not rendered in the focused pane (focus mode or an inactive tab): nothing to focus.
            return ShellKeyboardOwner::Nothing;
        };
        if let Some(record) = self.agents_gpui_engine_terminals.get(&session_id) {
            return ShellKeyboardOwner::EngineTerminal {
                target: GpuiEngineTerminalEventTarget::Agents(session_id),
                view: record.view.clone(),
                session_id: Some(session_id),
            };
        }
        #[cfg(target_os = "macos")]
        {
            let native_surface_matches = match mount {
                FocusedTerminalTextMountTarget::Agents(slot_id) => {
                    self.agents_terminal_ghostty_surface_matches(slot_id)
                }
                FocusedTerminalTextMountTarget::Command(_) => false,
            };
            if native_surface_matches {
                return ShellKeyboardOwner::NativeTerminal(mount);
            }
        }
        let _ = mount;
        ShellKeyboardOwner::TerminalMounting
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
            #[cfg(target_os = "macos")]
            ShellKeyboardOwner::NativeTerminal(mount) => {
                self.pending_keyboard_handoff = None;
                match mount {
                    FocusedTerminalTextMountTarget::Agents(slot_id) => {
                        self.sync_agents_terminal_ghostty_surface_focus_with_appkit_handoff(true);
                        self.deliver_pending_session_terminal_composer_insert(
                            slot_id.session_id,
                            cx,
                        );
                    }
                    FocusedTerminalTextMountTarget::Command(_) => {
                        self.sync_command_terminal_ghostty_surface_focus_with_appkit_handoff(true);
                    }
                }
            }
            ShellKeyboardOwner::TerminalMounting => {}
            ShellKeyboardOwner::ChatComposer(session_id) => {
                if !self.native_chat_views.contains_key(&session_id) {
                    // Native input can take focus before its background runtime restores the draft.
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
                let Some(surface) = self
                    .project_workarea_runtime_cef_surfaces
                    .iter()
                    .find(|(slot_key, _)| slot_key.titlebar_mode() == mode)
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
            }
        }
    }

    /// Physical half of a chat handoff: the chat view's composer, then any draft waiting for that composer.
    pub(crate) fn focus_session_chat_composer(
        &mut self,
        session_id: TerminalSessionId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(view) = self.native_chat_views.get(&session_id).cloned() else {
            return;
        };
        self.reclaim_gpui_root_for_native_chat_composer(window);
        view.update(cx, |view, cx| {
            view.focus_requested = true;
            view.ensure_input(window, cx);
        });
        if let Some(content) = self
            .pending_session_chat_composer_insert
            .remove(&session_id)
        {
            self.insert_prompt_into_session_chat(session_id, &content, cx);
        }
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
        if let Some(view) = self.gpui_engine_terminal_view_for_target(target)
            && view.read(cx).focus_handle(cx).is_focused(window)
        {
            window.blur(cx);
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
