//! The web build's `GhostexGpuiApp`: the fields and methods the shared drawing code reads, with the browser's answer behind each. Same type name as the desktop's so the shared files compile unchanged.
use gpui::{AnyElement, Context, IntoElement, WindowHandle, div};
use serde_json::Value;

use crate::app::native_sidebar::appearance::SidebarAppearance;
use crate::app::native_sidebar::state::NativeSidebarState;
use crate::app::titlebar::account_usage::{GpuiAccountUsageMeter, GpuiAccountUsageMeterHost};
use crate::*;

/// Stand-in for the desktop's native modal host window; the browser build opens no child windows.
pub(crate) struct GpuiAppModalHostWindow;

impl gpui::Render for GpuiAppModalHostWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl GpuiAppModalHostWindow {
    pub(crate) fn refresh_project_view_scope_options(
        &mut self,
        _changed: &[(&str, Value)],
        _cx: &mut Context<Self>,
    ) {
    }
}

pub(crate) struct GhostexGpuiApp {
    pub(crate) native_sidebar: NativeSidebarState,
    pub(crate) gx_store: crate::app::gx_store::GxStoreHost,
    pub(crate) sidebar_width: f32,
    pub(crate) sidebar_collapsed: bool,
    /// The header's Session details button: each chat's choice for this page (the page has no
    /// settings file, so it starts closed), where it was painted (the floating card opens under
    /// it), and when a click on the chat last closed that card.
    pub(crate) session_details_choices:
        crate::app::native_chat::session_details::SessionDetailsChoices,
    pub(crate) session_details_button_bounds: Rc<std::cell::Cell<Bounds<Pixels>>>,
    pub(crate) session_details_dismissed_at: Option<web_time::Instant>,
    pub(crate) sidebar_usage_visible: bool,
    pub(crate) gpui_reduce_motion_enabled: bool,
    pub(crate) app_modal_window: Option<WindowHandle<GpuiAppModalHostWindow>>,
    /// Whether a focused session row is drawn unfocused because a browser tab owns focus; always false until the web build has browser tabs.
    pub(crate) snapshot_browser_focus: bool,
    /// The session the work area shows.
    pub(crate) open_session: Option<ghostex_gx_core::SessionKey>,
    /// The keep-awake lease on the open session (`gx_store/terminal_lifecycle/shown_sessions.rs`).
    pub(crate) shown_sessions:
        crate::app::gx_store::terminal_lifecycle::shown_sessions::WebShownSessions,
    /// The chat grid lease on the chat the page shows (`gx_store/terminal_lifecycle/chat_grid_claims.rs`).
    pub(crate) chat_grid_claims:
        crate::app::gx_store::terminal_lifecycle::chat_grid_claims::WebChatGridClaims,
    /// Every chat opened in this page, with the shell id its view was given.
    pub(crate) terminals:
        HashMap<ghostex_gx_core::SessionKey, Entity<crate::terminal_element::TerminalView>>,
    /// Whether the work area shows the open session's terminal instead of its chat.
    pub(crate) show_terminal: bool,
    pub(crate) linked_session_opened: bool,
    /// The presentation each chat last published, handed back as `initialPresentation` when its view is made again.
    pub(crate) chat_presentations: HashMap<ghostex_gx_core::SessionKey, Value>,
    pub(crate) native_chats: HashMap<
        ghostex_gx_core::SessionKey,
        (
            TerminalSessionId,
            Entity<crate::app::native_chat::state::NativeChatView>,
        ),
    >,
    pub(crate) notification_feed_state: crate::notification_feed::GpuiNotificationFeedState,
    pub(crate) titlebar_notification_bell_bounds: Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    /// Toasts and the page's other stand-ins for the desktop's app-level services (`web_host/`).
    pub(crate) web_host: crate::app::web_host::WebHostState,
    /// Toasts with a button, waiting in the centred notice modal (the desktop's notice_modal_host.rs).
    pub(crate) app_notices: crate::app::notice_modal_host::AppNotices,
    /// The native dialog open over the page, under the desktop's field name so its modal files compile unchanged.
    pub(crate) native_app_modal: Option<crate::app::native_app_modal_lifecycle::NativeAppModal>,
    /// The Git menu the header's Git button opens, as the desktop's titlebar holds it (`gx_store/git/hud.rs` writes it).
    pub(crate) titlebar_git_menu_state: Option<crate::app::model::GpuiTitlebarGitMenuState>,
    /// The Changes view (the desktop's `app/native_changes/`), drawn beside the chat while
    /// `changes_open` (`web_host/changes.rs`).
    pub(crate) native_changes: Option<Entity<crate::app::native_changes::ChangesView>>,
    pub(crate) native_changes_preset: Option<crate::app::native_changes::ChangesPreset>,
    pub(crate) changes_open: bool,
    /// The Beads view (the desktop's `app/native_beads/`), drawn beside the chat while
    /// `beads_open` (`web_host/beads.rs`).
    pub(crate) native_beads: Option<Entity<crate::app::native_beads::BeadsView>>,
    pub(crate) beads_open: bool,
    pub(crate) beads_width: f32,
    /// The Beads view's Board (the desktop's `app/native_kanban/`; `web_host/kanban.rs`).
    pub(crate) native_kanban: crate::app::native_kanban::state::NativeKanbanState,
    /// Whether the Beads panel shows the Board now, so its poll runs only while it is seen.
    pub(crate) native_kanban_shown: bool,
    /// The side chat drawn beside the chat (`web_host/side_chat.rs`), opened by `openSideChat`.
    pub(crate) side_chat_open: Option<ghostex_gx_core::SessionKey>,
    pub(crate) changes_width: f32,
    /// Handoff / Export's Reveal path; an export never lands on a page's disk, so it stays empty.
    pub(crate) pending_export_transcript_reveal_path: Option<String>,
    /// The agents a worktree or New Thread dialog offers (the HUD's `agents`).
    pub(crate) new_thread_picker_agents: Option<Vec<Value>>,
    /// The terminal bar's Switch Account page (`gx_store/sidebar_accounts.rs` reads it); the page has no terminal bar, so it stays empty.
    pub(crate) agents_terminal_action_bar_account_page: Option<(String, Vec<Value>)>,
    /// Remote machine tunnels; always empty in a page (`remote_conn/`).
    pub(crate) remote_gxserver_connections:
        HashMap<String, crate::app::remote_conn::GpuiRemoteGxserverConnection>,
    /// The saved settings the HUD is composed from (see `cef.rs`).
    pub(crate) sidebar_runtime_settings_snapshot: crate::cef::SidebarRuntimeSettingsSnapshot,
    /// The active project as the desktop's project snapshot carries it (a browser open reads it); set from the store's focus.
    pub(crate) latest_sidebar_project_snapshot: Option<crate::app::model::GpuiProjectSnapshot>,
    /// The desktop queues project switches behind a pane attach; the page switches at once, so this stays empty.
    pub(crate) project_switch_pending_requests: Vec<()>,
    /// Quick Access (`app/quick_access/`, the desktop's host and window).
    pub(crate) quick_access: crate::app::quick_access::host::QuickAccessHost,
    /// Action run states for Quick Access's Commands rows; the page runs no Actions, so it stays empty.
    pub(crate) sidebar_command_run_feedback_states:
        HashMap<String, crate::app::model::GpuiSidebarCommandRunFeedbackState>,
    /// The desktop gives a command pane its keyboard focus back when a dialog closes; the page has no command pane.
    pub(crate) app_modal_command_return_focus_target: Option<()>,
    /// Where the keyboard is parked while nothing is focused, so Tab and the root's other keys
    /// still reach it (`render` in shell.rs, as the desktop's handle of the same name).
    ///
    /// CDXC:FocusRouting 2026-11-12 WHY:
    /// GPUI sends a key along the focused element's path, and with nothing focused that path holds only the window's outermost element, above the `Root` that binds Tab. The desktop parks the keyboard on a handle of its own (`root_action_focus_handle`); the page had none, so Tab from "nothing focused" (a fresh page, a click on empty space) did nothing (GitHub #557).
    pub(crate) root_action_focus_handle: gpui::FocusHandle,
}

impl GhostexGpuiApp {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        Self {
            root_action_focus_handle: cx.focus_handle().tab_stop(false),
            native_sidebar: NativeSidebarState::default(),
            gx_store: Default::default(),
            sidebar_width: 300.0,
            sidebar_collapsed: false,
            session_details_choices: Default::default(),
            session_details_button_bounds: Rc::default(),
            session_details_dismissed_at: None,
            // Pinned by default, as on the desktop (CDXC:Sidebar 2026-11-03 in shell_layout.rs).
            sidebar_usage_visible: true,
            gpui_reduce_motion_enabled: false,
            app_modal_window: None,
            snapshot_browser_focus: false,
            open_session: None,
            shown_sessions: Default::default(),
            chat_grid_claims: Default::default(),
            native_chats: HashMap::new(),
            native_changes: None,
            native_changes_preset: None,
            changes_open: false,
            native_beads: None,
            beads_open: false,
            beads_width: 980.0,
            native_kanban: Default::default(),
            native_kanban_shown: false,
            side_chat_open: None,
            changes_width: 620.0,
            chat_presentations: HashMap::new(),
            terminals: HashMap::new(),
            show_terminal: false,
            linked_session_opened: false,
            notification_feed_state: Default::default(),
            titlebar_notification_bell_bounds: Rc::new(std::cell::Cell::new(None)),
            web_host: Default::default(),
            app_notices: Default::default(),
            native_app_modal: None,
            titlebar_git_menu_state: None,
            pending_export_transcript_reveal_path: None,
            new_thread_picker_agents: None,
            agents_terminal_action_bar_account_page: None,
            remote_gxserver_connections: HashMap::new(),
            // The page's settings (the computer's, with `?settings=` from lib.rs on top) feed the HUD too;
            // without this a setting the sidebar reads off the HUD (its zoom) never reached the page.
            sidebar_runtime_settings_snapshot: crate::cef::SidebarRuntimeSettingsSnapshot {
                saved_settings_json: serde_json::to_string(
                    crate::shared_settings::shared_sidebar_settings_snapshot().object(),
                )
                .unwrap_or_default(),
                ..Default::default()
            },
            latest_sidebar_project_snapshot: None,
            project_switch_pending_requests: Vec::new(),
            quick_access: Default::default(),
            sidebar_command_run_feedback_states: HashMap::new(),
            app_modal_command_return_focus_target: None,
        }
    }

    pub(crate) fn account_usage_meters(&self) -> Vec<GpuiAccountUsageMeter> {
        Vec::new()
    }

    pub(crate) fn render_account_usage_meter(
        &self,
        _meter: &GpuiAccountUsageMeter,
        _host: &GpuiAccountUsageMeterHost,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        div().into_any_element()
    }

    pub(crate) fn titlebar_notification_bell_visible(&self) -> bool {
        false
    }

    /// The page's menu has no rows for the app's panels (`MenuHost::menu_panels` is false), so
    /// there is none to offer.
    pub(crate) fn sidebar_menu_panel_offered(&self, _panel_id: &str) -> bool {
        false
    }

    pub(crate) fn render_sidebar_notification_bell(
        &self,
        _appearance: &SidebarAppearance,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        div().into_any_element()
    }

    pub(crate) fn render_sidebar_collapse_button(
        &self,
        _in_sidebar: Option<&SidebarAppearance>,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
    }

    /// The bell is never shown here (`titlebar_notification_bell_visible`), so neither is its dropdown.
    pub(crate) fn toggle_gpui_titlebar_notifications_popup(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    /// The desktop routes Focus Chat Box, the copy keys and Toggle Summary Mode to the focused session's chat pane; the page binds none of them.
    pub(crate) fn run_focused_chat_hotkey(
        &mut self,
        _action_id: &str,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> bool {
        false
    }

    /// The desktop's model picker hotkey; the chat's own model pill opens the picker here.
    pub(crate) fn request_focused_session_model_picker(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> bool {
        false
    }

    pub(crate) fn react_to_native_sidebar_session_click(
        &mut self,
        _sidebar_session_id: &str,
        _cx: &mut Context<Self>,
    ) -> NativeSidebarClickReaction {
        NativeSidebarClickReaction::NotApplied
    }

    pub(crate) fn gx_store_sidebar_row_focus(
        &self,
        _row_id: &str,
        _is_browser: bool,
        snapshot_focused: bool,
        snapshot_visible: bool,
    ) -> (bool, bool) {
        (snapshot_focused, snapshot_visible)
    }

    pub(crate) fn gx_store_selection_is_settling(&self) -> bool {
        false
    }

    pub(crate) fn gx_store_note_sidebar_snapshot_browser_focus(&mut self, browser_focus: bool) {
        self.snapshot_browser_focus = browser_focus;
    }

    /// The shell id of the chat the work area shows, which is the only pane this build has.
    pub(crate) fn focused_agents_or_companion_shell_session_id(&self) -> Option<TerminalSessionId> {
        let open = self.open_session.as_ref()?;
        self.native_chats.get(open).map(|(id, _)| *id)
    }

    /// Sleep Space needs the daemon's per-space work list, which the web store does not read yet.
    pub(crate) fn gx_store_space_sleep_plans(
        &self,
        _space_id: &str,
    ) -> Option<ghostex_gx_core::SpaceSleepPlans> {
        None
    }

    pub(crate) fn gx_store_space_sleep_has_work(
        &self,
        _plans: Option<&ghostex_gx_core::SpaceSleepPlans>,
        _scope: ghostex_gx_core::SpaceSleepScope,
    ) -> bool {
        false
    }

    // Native focus and window plumbing the browser has no counterpart for.
    pub(crate) fn reveal_floating_sessions(&mut self, _cx: &mut Context<Self>) {}

    /// The desktop connects to a computer a coordinator's nested thread runs on before opening it
    /// (session_chat_fork_branches.rs); this build streams no other computer, so it never nests one.
    pub(crate) fn open_borrowed_thread_row_when_connected(
        &mut self,
        _sidebar_session_id: &str,
        _cx: &mut Context<Self>,
    ) -> bool {
        false
    }
    pub(crate) fn reclaim_gpui_root_for_chrome_input_focus(&mut self) {}
    pub(crate) fn drop_pending_browser_keyboard_handoff(&mut self) {}
    pub(crate) fn persist_shell_layout_state(&self) {}
    /// Session rows cannot be dragged onto panes here, so there is no pane drag to finish.
    pub(crate) fn finish_workspace_tab_drag(&mut self, _cx: &mut Context<Self>) {}
    /// The page has no Work view (the desktop's `work_view/host.rs`), so a sidebar snapshot never
    /// has one to leave.
    pub(crate) fn work_view_available(&self) -> bool {
        false
    }
    pub(crate) fn leave_work_view_if_unavailable(
        &mut self,
        _was_available: bool,
        _cx: &mut Context<Self>,
    ) {
    }
    /// The page has no view panel (the desktop's `workspace_reconcile/project_switch.rs`), so a
    /// project switch never leaves a saved view waiting to be restored.
    pub(crate) fn finish_project_view_restore(&mut self, _cx: &mut Context<Self>) {}

    // Workspaces (`gx_store/workspaces.rs`): a page is one window with nothing to save it in, so
    // its workspace lasts for the page; it has no other windows and no Settings window.
    pub(crate) fn persist_window_workspace_id(&self) {}
    pub(crate) fn switch_window_workspace(&mut self, workspace_id: String, cx: &mut Context<Self>) {
        self.gx_store_set_window_workspace(Some(workspace_id), cx);
    }
    pub(crate) fn focus_window_showing_workspace(
        &self,
        _workspace_id: &str,
        _cx: &mut Context<Self>,
    ) -> bool {
        false
    }
    /// A page cannot open another window, so every workspace counts as shown and the workspace
    /// menu offers no new-window buttons.
    pub(crate) fn workspaces_shown_in_other_windows(&self, _cx: &Context<Self>) -> Vec<String> {
        self.gx_store_workspaces_state()
            .map(|state| state.workspaces.keys().cloned().collect())
            .unwrap_or_default()
    }
    pub(crate) fn other_window_showing_workspace(
        &self,
        _workspace_id: &str,
        _cx: &mut Context<Self>,
    ) -> Option<gpui::WeakEntity<Self>> {
        None
    }
    pub(crate) fn open_workspace_in_new_window(
        &mut self,
        _workspace_id: &str,
        cx: &mut Context<Self>,
    ) {
        self.dispatch_gpui_workspace_action_toast(
            "info",
            "Not in the browser",
            "Open a workspace in a new window from the Ghostex app.",
            cx,
        );
    }
    pub(crate) fn open_workspace_settings_page(&mut self, cx: &mut Context<Self>) {
        self.dispatch_gpui_workspace_action_toast(
            "info",
            "Not in the browser",
            "Workspace settings are in the Ghostex app's Settings.",
            cx,
        );
    }
}
