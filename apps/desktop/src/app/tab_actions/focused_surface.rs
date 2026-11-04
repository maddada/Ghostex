//! Focusing the command pane and project editor surfaces, cycling tabs, closing the focused surface and agents focus mode.

use gpui::Window;

use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn focus_command_pane(&mut self, cx: &mut gpui::Context<Self>) {
        if self.command_pane.has_sessions() {
            self.remember_current_non_command_focus();
            self.focus_shell_target(ShellFocusTarget::CommandPane, cx);
            self.persist_shell_layout_state();
        }
    }

    pub(crate) fn command_pane_directional_focus_session_for_app_route(
        command_pane: &mut CommandPaneModel,
        target_group_id: Option<CommandPaneGroupId>,
    ) -> Option<CommandSessionId> {
        /*
        CDXC:FocusRouting 2026-06-25-23:35:
        Cmd-Opt directional focus into command panes must use a live expanded command-panel route. Specific command-group targets validate and focus that group; generic command-pane targets require the current focused group to still resolve, so stale shell focus never falls back to another command session.
        */
        if !command_pane.any_dock_visible() || !command_pane.has_sessions() {
            return None;
        }

        if let Some(group_id) = target_group_id {
            if !command_pane.group_dock_visible(group_id) {
                return None;
            }
            let active_session_id = command_pane
                .find_leaf(group_id)
                .and_then(|leaf| leaf.tab_group.active_session_id())?;
            if command_pane.session(active_session_id).is_none()
                || !command_pane.focus_group(group_id)
            {
                return None;
            }
        }

        if !command_pane.focused_group_dock_visible() {
            return None;
        }
        let (_group_id, session_id) = command_pane.focused_group_active_session_id()?;
        command_pane.session(session_id).map(|_| session_id)
    }

    pub(crate) fn focus_command_pane_directional_target(
        &mut self,
        target_group_id: Option<CommandPaneGroupId>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(session_id) = Self::command_pane_directional_focus_session_for_app_route(
            &mut self.command_pane,
            target_group_id,
        ) else {
            return false;
        };

        self.focus_command_pane(cx);
        if self.shell_focus != ShellFocusTarget::CommandPane {
            return false;
        }
        self.request_focused_command_terminal_text_focus_handoff();

        /*
        CDXC:FocusRouting 2026-06-25-23:55:
        Cmd-Opt spatial and render-order focus into a live expanded command pane must reveal the focused active command tab in both the target command group and collapsed strip, matching other command activation paths. Collapsed, stale, or orphan command targets return before scrolling, persistence, sidebar refresh, or Attention acknowledgement.
        */
        self.scroll_focused_command_active_tab();

        let attention_acknowledged = self
            .command_pane
            .acknowledge_attention_for_session_activation(session_id);
        if attention_acknowledged {
            self.persist_shell_layout_state();
            self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        }
        true
    }

    pub(crate) fn focus_project_editor_surface(
        &mut self,
        mode: TitlebarMode,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // Claude's page is on screen in the sessions column's place (app/claude_web/takeover.rs).
        if self.view_page_on_screen(mode) {
            if mode == TitlebarMode::Terminal {
                if self.seed_terminal_view_for_open(cx) {
                    self.focus_command_pane(cx);
                    self.request_focused_command_terminal_text_focus_handoff();
                }
                return;
            }
            self.mark_project_editor_mode_awake(mode, cx);
            let focus = match mode {
                TitlebarMode::Agents => {
                    ShellFocusTarget::AgentsPane(self.agents_workspace.focused_pane)
                }
                TitlebarMode::Browser => {
                    ShellFocusTarget::BrowserPane(self.browser_tabs.focused_pane)
                }
                TitlebarMode::Terminal => ShellFocusTarget::CommandPane,
                TitlebarMode::Source
                | TitlebarMode::Kanban
                | TitlebarMode::Automate
                | TitlebarMode::Manage
                | TitlebarMode::BotFeed
                | TitlebarMode::Changes
                | TitlebarMode::SideChat
                | TitlebarMode::Work
                | TitlebarMode::Extension(_) => ShellFocusTarget::ProjectEditorSurface(mode),
            };
            self.focus_shell_target(focus, cx);
            if mode == TitlebarMode::Browser {
                self.sync_active_browser_tab_to_surface(window, cx);
            } else {
                // A view shown asleep never went through `activate_mode`, so its extension server
                // may not be running yet.
                if let TitlebarMode::Extension(id) = mode {
                    self.ensure_extension_view_runtime_for_current_context(id, cx);
                }
                self.ensure_project_workarea_runtime_cef_surfaces_for_current_context(cx);
                self.update_active_mode_cef_child_visibility(cx);
            }
            self.persist_shell_layout_state();
        }
    }

    pub(crate) fn focus_project_editor_surface_for_keyboard(
        &mut self,
        mode: TitlebarMode,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.view_page_on_screen(mode) {
            return false;
        }

        if mode == TitlebarMode::Terminal {
            self.focus_project_editor_surface(mode, window, cx);
            self.drain_pending_keyboard_handoff(window, cx);
            return self.shell_focus == ShellFocusTarget::CommandPane;
        }

        if !mode.is_project_editor_mode() {
            return false;
        }
        if self.project_editor_shell.is_mode_awake(mode) {
            /*
            CDXC:FocusRouting 2026-07-29-05:03:
            Left/right focus must transfer real keyboard ownership to the view, not only update shell border state. Browser focuses the current page surface; Source, Kanban, Automate, and Docs focus their exact project-workarea CEF surface after the ordinary shell/lifecycle transition.
            */
            if mode == TitlebarMode::Browser {
                let pane_id = self.browser_tabs.focused_pane;
                return self.focus_browser_content_for_pane(pane_id, window, cx);
            }

            self.focus_project_editor_surface(mode, window, cx);
            self.drain_pending_keyboard_handoff(window, cx);
            return self.shell_focus == ShellFocusTarget::ProjectEditorSurface(mode);
        }

        /*
        CDXC:FocusRouting 2026-06-22-09:44:
        Directional keyboard focus onto a selected sleeping project-editor main surface only updates shell focus and Browser visibility. It must not mark the lifecycle awake, refresh recency, create or sync a Browser CEF surface, or bypass the explicit click-to-wake body activation path.
        */
        let focus = match mode {
            TitlebarMode::Browser => ShellFocusTarget::BrowserSurface,
            TitlebarMode::Source
            | TitlebarMode::Kanban
            | TitlebarMode::Automate
            | TitlebarMode::Manage
            | TitlebarMode::BotFeed
            | TitlebarMode::Changes
            | TitlebarMode::SideChat
            | TitlebarMode::Work
            | TitlebarMode::Extension(_) => ShellFocusTarget::ProjectEditorSurface(mode),
            TitlebarMode::Agents | TitlebarMode::Terminal => return false,
        };
        self.focus_shell_target(focus, cx);
        self.update_active_mode_cef_child_visibility(cx);
        self.persist_shell_layout_state();
        self.shell_focus == focus
    }

    pub(crate) fn focused_command_pane_tab_cycle_target(
        shell_focus: ShellFocusTarget,
        command_pane: &CommandPaneModel,
    ) -> Option<(CommandPaneGroupId, CommandSessionId)> {
        /*
        CDXC:CommandPane 2026-06-25-23:20:
        Ctrl-Tab and Ctrl-Shift-Tab over command focus are live command-panel routes. Cycle only while the command pane is expanded and the stored focused command group still resolves, so collapsed command strips and stale command focus no-op instead of mutating hidden or fallback tabs.

        CDXC:CommandPane 2026-06-25-23:20:
        Keyboard cycling shares direct command-tab activation semantics: after a successful cycle, acknowledge only the selected Attention command session through the existing command attention path.
        */
        if shell_focus != ShellFocusTarget::CommandPane
            || !command_pane.focused_group_dock_visible()
        {
            return None;
        }
        command_pane.focused_group_active_session_id()
    }

    pub(crate) fn cycle_focused_command_pane_tab_for_app_route(
        shell_focus: ShellFocusTarget,
        command_pane: &mut CommandPaneModel,
        reverse: bool,
    ) -> Option<bool> {
        Self::focused_command_pane_tab_cycle_target(shell_focus, command_pane)?;
        if !command_pane.cycle_active_session(reverse) {
            return None;
        }

        let attention_acknowledged =
            if let Some((_group_id, session_id)) = command_pane.focused_group_active_session_id() {
                command_pane.acknowledge_attention_for_session_activation(session_id)
            } else {
                false
            };
        Some(attention_acknowledged)
    }

    pub(crate) fn cycle_focused_tab(
        &mut self,
        reverse: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let step_started = Instant::now();
        let changed = if self.shell_focus == ShellFocusTarget::CommandPane {
            if let Some(attention_acknowledged) = Self::cycle_focused_command_pane_tab_for_app_route(
                self.shell_focus,
                &mut self.command_pane,
                reverse,
            ) {
                self.scroll_focused_command_active_tab();
                if attention_acknowledged {
                    self.refresh_sidebar_command_pane_sessions_if_changed(cx);
                }
                true
            } else {
                false
            }
        } else if matches!(self.shell_focus, ShellFocusTarget::AgentsPane(_))
            || self.open_view_mode().is_none()
        {
            let pane_id = match self.shell_focus {
                ShellFocusTarget::AgentsPane(pane_id) => pane_id,
                _ => self.agents_workspace.focused_pane,
            };
            // CDXC:FocusRouting 2026-09-19 WHY: the pane used to be focused before the step, which announced the tab being left as the selected session and left the tab the step landed on to whatever focus event came next. The step comes first, so the one selection this records (store focus, sidebar highlight, attention, the coalesced tell) is the tab now in front.
            let cycled = self.agents_workspace.cycle_tab_in_pane(pane_id, reverse);
            self.focus_agents_pane(pane_id, cx);
            cycled
        } else if self.active_mode == TitlebarMode::Browser
            && matches!(
                self.shell_focus,
                ShellFocusTarget::BrowserSurface | ShellFocusTarget::BrowserPane(_)
            )
        {
            /*
            CDXC:FocusRouting 2026-06-22-09:18:
            Ctrl-Tab in Browser mode is pane-local while split panes are shell-owned placeholders. Cycle only the focused Browser pane's loaded and address-only tab ids, then reuse Browser tab selection sync so the mode wakes, the shared address toolbar follows the selected tab, the focused loaded tab materializes if needed, already-created active loaded surfaces in other rendered Browser leaves stay visible, shell focus remains Browser, and shell state persists.
            */
            if self
                .browser_tabs
                .cycle_tab_in_focused_pane(reverse)
                .is_some()
            {
                self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
                self.focus_shell_target(
                    ShellFocusTarget::BrowserPane(self.browser_tabs.focused_pane),
                    cx,
                );
                if self.gx_store_key_is_held() {
                    // CDXC:Browser 2026-09-19 WHY: a held Previous/Next Tab in Pane now repeats (helpers/os_cli/keyboard_router.rs). Selecting a restored tab creates its CEF surface, which loads the page and starts a renderer process, so a hold over twenty restored tabs would start twenty. A held step moves only the selection (tab strip, address bar, and the surface of a tab that already has one); the surface of the tab the key is released on is created when the selection settles (gx_store/burst.rs).
                    let pane_id = self.browser_tabs.focused_pane;
                    let address_value = self.browser_tabs.address_value_for_pane(pane_id);
                    self.browser_url = address_value.clone();
                    self.set_browser_address_input_value(pane_id, address_value, window, cx);
                    self.update_active_mode_cef_child_visibility(cx);
                    self.gx_store_defer_browser_surface(cx);
                } else {
                    self.sync_active_browser_tab_to_surface(window, cx);
                }
                self.scroll_focused_browser_pane_active_tab();
                true
            } else {
                false
            }
        } else if self.focused_view_mode().is_some() {
            self.cycle_focused_view_strip_tab(reverse, window, cx)
        } else {
            false
        };

        if changed {
            // CDXC:FocusRouting 2026-07-04 WHY: keyboard tab cycling must end in the same
            // focus state as clicking the terminal body; before the unified handoff it only
            // moved the model and left the CEF sidebar as first responder.
            self.drain_pending_keyboard_handoff(window, cx);
            self.scroll_all_active_tab_strips();
            self.persist_shell_layout_state();
            cx.notify();
            self.gx_store_log_tab_step(step_started, reverse);
        }
    }

    pub(crate) fn close_focused_surface(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:FocusRouting 2026-06-22-06:02:
        Cmd-W is surface-aware in the GPUI placeholder shell. Command focus closes the active command placeholder, Browser surface focus closes the active browser tab, Agents mode closes the active workspace tab, and a focused side-panel view closes its own tab (CDXC:Workarea 2026-10-01 in model/focus_close_targets.rs).

        CDXC:Terminal 2026-06-26-23:59:
        Cmd-W in Agents delegates to the same close helper as pane-tab close. Mapped workspace sessions bypass Ghostty close-confirm and go through the store's lifecycle (formerly SidebarApp's), while unmapped exact mounted Running surfaces can still request `ghostty_surface_request_close` before shell removal.

        CDXC:Terminal 2026-06-23-05:21:
        Cmd-W with command-pane focus must match command tab close parity: an exact current mounted command surface gets a Ghostty close request and stays in the command model until a confirmed close callback is consumed. Non-mounted command placeholders continue to close through the existing command shell model.

        CDXC:CommandPane 2026-06-25-17:37:
        Cmd-W over command-pane focus must use the same clicked command-tab close path as hover, middle-click, scoped menus, and Close After Done. That shared helper owns mounted close requests, timer cleanup, final-panel focus restore, shell persistence, and sidebar refresh.

        CDXC:FocusMode 2026-06-27-02:58:
        Keep the executable Cmd-W route aligned with the pure focused-close decision helper so native parity stays testable without a GPUI window: command focus wins first, BrowserSurface or exact BrowserPane focus closes Browser tabs, and focus on the open view closes that view's tab.

        CDXC:Workarea 2026-09-20 WHY:
        An Agents pane owns Cmd-W whatever the view panel shows, because it is on screen either way. This supersedes the 2026-07-29 rule that gave the chord to a focused companion session.
        */
        match focused_surface_close_decision(self.shell_focus, self.active_mode, &self.command_pane)
        {
            FocusedSurfaceCloseDecision::CloseCommandTab {
                group_id,
                session_id,
            } => {
                self.close_command_pane_tab(group_id, session_id, cx);
            }
            FocusedSurfaceCloseDecision::InterceptNoOp | FocusedSurfaceCloseDecision::NoOp => {}
            FocusedSurfaceCloseDecision::CloseAgentsActiveTab => {
                let pane_id = self.agents_workspace.focused_pane;
                if let Some(session_id) = self
                    .agents_workspace
                    .find_leaf(pane_id)
                    .and_then(|leaf| leaf.tab_group.active_session_id())
                {
                    self.close_agents_tab(pane_id, session_id, "tabShortcut", cx);
                }
            }
            FocusedSurfaceCloseDecision::CloseViewTab(mode) => {
                match (mode, self.code_windows.active) {
                    // One of Code's extra windows closes on its own, not with the Code tab.
                    (TitlebarMode::Source, Some(id)) => self.close_code_window(id, window, cx),
                    _ => self.close_view_tab(mode, window, cx),
                }
            }
            FocusedSurfaceCloseDecision::CloseViewPanel => {
                if self.view_picker_open() {
                    self.close_view_panel(cx);
                }
            }
            FocusedSurfaceCloseDecision::CloseBrowserActiveTab => {
                if let Some(tab_id) = self.browser_tabs.active_tab().map(|tab| tab.id) {
                    let changed = self.close_browser_tab_model(tab_id, window, cx);
                    if changed {
                        self.persist_shell_layout_state();
                        cx.notify();
                    }
                }
            }
        }
    }

    pub(crate) fn toggle_agents_focus_mode(&mut self, cx: &mut gpui::Context<Self>) {
        self.toggle_agents_focus_mode_for_pane(self.agents_workspace.focused_pane, cx);
    }

    pub(crate) fn toggle_agents_focus_mode_for_pane(
        &mut self,
        pane_id: WorkspacePaneId,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(pane_id) = self.agents_workspace.resolve_action_pane_id(pane_id) else {
            return;
        };
        self.agents_workspace.focus_pane(pane_id);

        if self.agents_workspace.toggle_focus_mode() {
            self.focus_shell_target(
                ShellFocusTarget::AgentsPane(self.agents_workspace.focused_pane),
                cx,
            );
            self.persist_shell_layout_state();
            cx.notify();
        }
    }
}
