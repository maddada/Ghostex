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
    pub(crate) sidebar_usage_visible: bool,
    pub(crate) gpui_pet_overlay_reduce_motion_enabled: bool,
    pub(crate) app_modal_window: Option<WindowHandle<GpuiAppModalHostWindow>>,
    /// Whether a focused session row is drawn unfocused because a browser tab owns focus; always false until the web build has browser tabs.
    pub(crate) snapshot_browser_focus: bool,
    /// The session the work area shows.
    pub(crate) open_session: Option<ghostex_gx_core::SessionKey>,
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
    /// The native dialog open over the page, under the desktop's field name so its modal files compile unchanged.
    pub(crate) native_app_modal: Option<crate::app::native_app_modal_lifecycle::NativeAppModal>,
    /// The Git menu the header's Git button opens, as the desktop's titlebar holds it (`gx_store/git/hud.rs` writes it).
    pub(crate) titlebar_git_menu_state: Option<crate::app::model::GpuiTitlebarGitMenuState>,
    /// Handoff / Export's Reveal path; an export never lands on a page's disk, so it stays empty.
    pub(crate) pending_export_transcript_reveal_path: Option<String>,
    /// The agents a worktree or New Thread dialog offers (the HUD's `agents`).
    pub(crate) new_thread_picker_agents: Option<Vec<Value>>,
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
}

impl GhostexGpuiApp {
    pub(crate) fn new() -> Self {
        Self {
            native_sidebar: NativeSidebarState::default(),
            gx_store: Default::default(),
            sidebar_width: 300.0,
            sidebar_collapsed: false,
            sidebar_usage_visible: false,
            gpui_pet_overlay_reduce_motion_enabled: false,
            app_modal_window: None,
            snapshot_browser_focus: false,
            open_session: None,
            native_chats: HashMap::new(),
            chat_presentations: HashMap::new(),
            terminals: HashMap::new(),
            show_terminal: false,
            linked_session_opened: false,
            notification_feed_state: Default::default(),
            titlebar_notification_bell_bounds: Rc::new(std::cell::Cell::new(None)),
            web_host: Default::default(),
            native_app_modal: None,
            titlebar_git_menu_state: None,
            pending_export_transcript_reveal_path: None,
            new_thread_picker_agents: None,
            remote_gxserver_connections: HashMap::new(),
            sidebar_runtime_settings_snapshot: Default::default(),
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

    pub(crate) fn render_sidebar_notification_bell(
        &self,
        _appearance: &SidebarAppearance,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        div().into_any_element()
    }

    pub(crate) fn render_sidebar_collapse_button(
        &self,
        _icon_color: Option<Hsla>,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
    }

    /// The Agents Panel toggle folds the panel behind an open view; the browser build opens no views.
    pub(crate) fn render_workarea_header_agents_toggle(
        &self,
        _icon_color: Option<Hsla>,
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
    pub(crate) fn reclaim_gpui_root_for_chrome_input_focus(&mut self) {}
    pub(crate) fn drop_pending_browser_keyboard_handoff(&mut self) {}
    pub(crate) fn persist_shell_layout_state(&self) {}
    /// Session rows cannot be dragged onto panes here, so there is no pane drag to finish.
    pub(crate) fn finish_workspace_tab_drag(&mut self, _cx: &mut Context<Self>) {}
}
