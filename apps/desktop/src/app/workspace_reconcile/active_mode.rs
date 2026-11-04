//! Switching the active work area mode and seeding the project's first browser tab.

use gpui::Window;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn set_active_mode(
        &mut self,
        mode: TitlebarMode,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.activate_mode(mode, cx) {
            return false;
        }
        self.sync_browser_address_input_if_shown(window, cx);
        true
    }

    /// `set_active_mode` without its one `Window` step, the Browser's address field
    /// (`sync_browser_address_input_if_shown`), for routes that have no window.
    pub(crate) fn activate_mode(
        &mut self,
        mode: TitlebarMode,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.titlebar_mode_available(mode) {
            return false;
        }
        // Claude is never a side panel view: opening it (the Open a view picker, a hotkey, a saved
        // tab) turns Claude on (CDXC:Extensions 2026-10-28 in app/claude_web/takeover.rs).
        if mode.is_claude_web() {
            return self.show_claude(cx);
        }

        let previous_mode = self.active_mode;
        /*
        CDXC:Telemetry 2026-08-26:
        Every workarea switch route — center tabs, the compact titlebar menu,
        Option+1..5, the command palette, and the sidebar focus helpers — funnels
        through set_active_mode, so this is the one place a `surface.opened` ping
        belongs. Re-selecting the current workarea is not a switch, and only the
        fixed spec enum is reportable (extension views send their id, which
        gxserver whitelists or reports as `custom`, 2026-10-27).
        */
        if previous_mode != mode
            && let Some(surface) = gpui_telemetry_surface_for_titlebar_mode(mode)
        {
            record_gpui_surface_opened_telemetry(surface, cx.background_executor());
        }
        let previous_shell_focus = self.shell_focus;
        let previous_first_responder_target = self.first_responder_target;
        self.change_active_mode_with_pane_state(mode, cx);
        // The Terminal view's default focus is its focused command group, so the group has to
        // exist, and be the focused one, before the default focus below is computed.
        if mode == TitlebarMode::Terminal {
            self.seed_terminal_view_for_open(cx);
        }
        /*
        CDXC:CodeEditor 2026-07-05:
        Opening Source, Browser, Kanban, Automate, or Docs is an activation
        route. Match macOS by marking the selected project-editor mode awake
        before render, so deliberate page opens never show the sleeping
        placeholder. Auto-sleep remains limited to inactive modes.
        */
        if mode.is_project_editor_mode() {
            self.mark_project_editor_mode_awake(mode, cx);
        }
        self.agents_terminal_runtime_sessions
            .reconcile_with_workspace(&self.agents_workspace);
        if mode == TitlebarMode::Browser {
            self.seed_current_project_browser_tab_if_empty();
        }
        if mode == TitlebarMode::BotFeed && previous_mode != mode {
            self.native_bot_feed_opened(cx);
        }
        self.focus_default_surface_for_active_mode(cx);
        if mode == TitlebarMode::Terminal {
            self.request_focused_command_terminal_text_focus_handoff();
        }
        let requested_agents_terminal_focus =
            if let Some(FocusedTerminalTextMountTarget::Agents(slot_id)) =
                self.focused_terminal_text_mount_target()
            {
                self.request_agents_session_text_focus_handoff(slot_id, cx);
                Some(slot_id)
            } else {
                None
            };
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.terminalFocus.modeSwitch",
            serde_json::json!({
                "previousMode": format!("{:?}", previous_mode),
                "nextMode": format!("{:?}", mode),
                "previousShellFocus": format!("{:?}", previous_shell_focus),
                "nextShellFocus": format!("{:?}", self.shell_focus),
                "previousFirstResponderTarget": format!("{:?}", previous_first_responder_target),
                "requestedAgentsPane": requested_agents_terminal_focus.map(|slot| slot.pane_id.0),
                "requestedAgentsSession": requested_agents_terminal_focus.map(|slot| slot.session_id.0),
            }),
        );
        if mode == TitlebarMode::Browser && self.project_editor_shell.is_mode_awake(mode) {
            self.sync_active_browser_tab_surface(cx);
        } else {
            if let TitlebarMode::Extension(id) = mode {
                self.ensure_extension_view_runtime_for_current_context(id, cx);
            }
            self.ensure_project_workarea_runtime_cef_surfaces_for_current_context(cx);
            self.update_active_mode_cef_child_visibility(cx);
        }
        // Session Chat is also a native CEF child view. Reconcile it at the
        // same mode-switch boundary as every other surface.
        self.reconcile_agents_pane_surfaces(cx);
        self.scroll_all_active_tab_strips();
        self.persist_shell_layout_state();
        self.schedule_project_editor_auto_sleep_for_inactive_modes(cx);
        true
    }

    /// The Browser's address field follows its focused page once the Browser is the view on
    /// screen and awake.
    pub(crate) fn sync_browser_address_input_if_shown(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.active_mode == TitlebarMode::Browser
            && self
                .project_editor_shell
                .is_mode_awake(TitlebarMode::Browser)
        {
            self.sync_browser_address_input(window, cx);
        }
    }

    pub(crate) fn seed_current_project_browser_tab_if_empty(&mut self) -> bool {
        let Some(active_tab) = self.browser_tabs.active_tab() else {
            return false;
        };
        if self.browser_tabs.tabs.len() != 1 || active_tab.state != BrowserTabState::AddressOnly {
            return false;
        }
        let tab_id = active_tab.id;
        self.assign_new_browser_tab_project_machine(tab_id);

        /*
        CDXC:Browser 2026-07-14:
        A project with no saved Browser tabs starts at its repository origin's
        web URL when available. Reuse the active project's machine-scoped
        Browser home URL, and mutate only the single address-only
        placeholder so existing project tabs are never replaced.
        */
        let default_url = browser_shell_default_url(
            self.latest_sidebar_project_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.browser_home_url.as_deref()),
        );
        let pane_id = self.browser_tabs.focused_pane;
        if self
            .browser_tabs
            .load_pane_active_tab_url(pane_id, default_url.clone())
            .is_none()
        {
            return false;
        }
        // The placeholder keeps its id, and with it whatever place an older tab of that id had in
        // the strip (often the start), so loading it is a new tab for the strip's order too.
        self.reveal_new_browser_tab(tab_id);
        self.browser_url = default_url;
        true
    }

    pub(crate) fn switch_workarea_from_hotkey(
        &mut self,
        mode: TitlebarMode,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Hotkeys 2026-06-22-13:00:
        Option+1 through Option+5 must match the native sidebar workarea switchers and use the same titlebar selection route: Agents, Source, Browser, Kanban, and Manage. Browser, Kanban, and Manage remain gated by titlebar_mode_available through set_active_mode, and sleeping project-editor modes keep the titlebar route's no-wake behavior while awake editors refresh through the existing lifecycle, focus, Browser visibility, and persistence sync.

        CDXC:CommandPalette 2026-06-26-07:24:
        Command-palette workarea switchers must reuse this exact hotkey route instead of mutating active_mode directly, so unavailable project-scoped modes remain guarded and all focus/visibility side effects stay identical to keyboard workarea switching.
        */
        if self.set_active_mode(mode, window, cx) {
            cx.notify();
        }
    }
}
