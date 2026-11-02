//! Opening, showing and closing side chat tabs, and the two chat host actions that ask for it.

use gpui::Window;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn has_side_chat_tabs(&self) -> bool {
        self.view_strip_layout.side_chat_tabs().next().is_some()
    }

    /// The side chat `TitlebarMode::SideChat` shows: the one picked last, else the first open one.
    fn current_side_chat_tab(&self) -> Option<SideChatTabId> {
        let layout = &self.view_strip_layout;
        layout
            .active_side_chat
            .filter(|tab_id| layout.has_side_chat_tab(*tab_id))
            .or_else(|| layout.side_chat_tabs().next())
    }

    /// The side chat on screen: `current_side_chat_tab` while the panel shows side chats.
    pub(crate) fn side_chat_tab_on_screen(&self) -> Option<SideChatTabId> {
        (self.active_mode == TitlebarMode::SideChat)
            .then(|| self.current_side_chat_tab())
            .flatten()
    }

    /// The tab's label: the side chat session's title as the store has it now. Titles are never
    /// saved with the tab.
    pub(crate) fn side_chat_tab_title(&self, tab_id: SideChatTabId) -> String {
        tab_id
            .key()
            .and_then(|key| {
                let session = self.gx_store.core.presentation().session(&key)?;
                session
                    .display_title
                    .as_deref()
                    .map(str::trim)
                    .filter(|title| !title.is_empty())
                    .or_else(|| Some(session.title.trim()).filter(|title| !title.is_empty()))
                    .map(str::to_string)
            })
            .unwrap_or_else(|| TitlebarMode::SideChat.display_label().to_string())
    }

    /// Opens `key`'s side chat as a tab of the project on screen, or brings its tab forward, and
    /// opens the panel onto it.
    pub(crate) fn open_side_chat_tab(
        &mut self,
        key: ghostex_gx_core::SessionKey,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let tab_id = SideChatTabId::for_key(&key);
        if !self.view_strip_layout.has_side_chat_tab(tab_id) {
            self.append_view_strip_tab(ViewStripTabKey::SideChat(tab_id));
        }
        self.show_side_chat_tab(tab_id, window, cx);
    }

    /// Puts an open side chat tab on screen, the way a click on it does.
    pub(crate) fn show_side_chat_tab(
        &mut self,
        tab_id: SideChatTabId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.view_strip_layout.has_side_chat_tab(tab_id) {
            return;
        }
        self.view_strip_layout.active_side_chat = Some(tab_id);
        // With side chats already on screen this still runs the activation, which moves the focus
        // to the panel and reconciles the chats for the tab that just changed.
        self.open_view_tab(TitlebarMode::SideChat, window, cx);
        self.reconcile_side_panel_chats(cx);
        self.persist_shell_layout_state();
        cx.notify();
    }

    /// Closes a side chat tab. Closing the tab on screen shows its neighbour, as closing any tab of
    /// the strip does (`view_strip_successor`), or follows the last-tab rule of `close_view_tab`.
    /// The side chat itself keeps running; its chat's Delete is what ends it.
    pub(crate) fn close_side_chat_tab(
        &mut self,
        tab_id: SideChatTabId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let key = ViewStripTabKey::SideChat(tab_id);
        if !self.view_strip_layout.order.contains(&key) {
            return;
        }
        let shown = self.side_chat_tab_on_screen() == Some(tab_id);
        let successor = if shown {
            self.view_strip_successor(&self.view_strip_entries(), key, |_| false)
        } else {
            None
        };
        let layout = &mut self.view_strip_layout;
        layout.order.retain(|existing| *existing != key);
        layout.pinned.retain(|existing| *existing != key);
        if layout.active_side_chat == Some(tab_id) {
            layout.active_side_chat = None;
        }
        if !self.has_side_chat_tabs() {
            self.open_views
                .retain(|mode| *mode != TitlebarMode::SideChat);
        }
        if shown {
            match successor {
                Some(next) => self.show_view_strip_entry(next, window, cx),
                None => self.show_view_tab_successor(None, cx),
            }
        }
        self.reconcile_side_panel_chats(cx);
        self.persist_shell_layout_state();
        cx.notify();
    }

    /// `close_view_tab` for the one `SideChat` mode (Cmd+W on the panel): the side chat on screen.
    pub(crate) fn close_current_side_chat_tab(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(tab_id) = self.current_side_chat_tab() {
            self.close_side_chat_tab(tab_id, window, cx);
        }
    }

    /// The session a side chat host action names, on the machine of the chat that sent it.
    fn side_chat_key_from_host_action(
        &self,
        session_id: TerminalSessionId,
        message: &serde_json::Value,
    ) -> Option<ghostex_gx_core::SessionKey> {
        let field = |name: &str| {
            message
                .get(name)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        let (project_id, side_session_id) = (field("projectId")?, field("sessionId")?);
        Some(
            match self.workspace_terminal_key_for_shell_session(session_id) {
                Some(GpuiWorkspaceTerminalSessionKey::Remote(key)) => {
                    ghostex_gx_core::SessionKey::remote(
                        key.remote_machine_id,
                        project_id,
                        side_session_id,
                    )
                }
                _ => ghostex_gx_core::SessionKey::local(project_id, side_session_id),
            },
        )
    }

    /// `openSideChat`: a row of the main chat's side chats panel, a new side chat, or `/btw`.
    pub(crate) fn open_side_chat_from_host_action(
        &mut self,
        session_id: TerminalSessionId,
        message: &serde_json::Value,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(key) = self.side_chat_key_from_host_action(session_id, message) {
            self.open_side_chat_tab(key, window, cx);
        }
    }

    /// `sideChatDeleted`, from the main chat's menu or the side chat's own Delete: its tab closes
    /// here, and in every other project's saved strip that still lists it.
    pub(crate) fn close_deleted_side_chat_tab(
        &mut self,
        session_id: TerminalSessionId,
        message: &serde_json::Value,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(key) = self.side_chat_key_from_host_action(session_id, message) else {
            return;
        };
        let tab_id = SideChatTabId::for_key(&key);
        let strip_key = ViewStripTabKey::SideChat(tab_id);
        for state in self.project_view_states_by_project.values_mut() {
            let layout = &mut state.view_strip_layout;
            layout.order.retain(|existing| *existing != strip_key);
            layout.pinned.retain(|existing| *existing != strip_key);
            if layout.active_side_chat == Some(tab_id) {
                layout.active_side_chat = None;
            }
            if layout.side_chat_tabs().next().is_none() {
                state
                    .open_views
                    .retain(|mode| *mode != TitlebarMode::SideChat);
                if state.active_mode == TitlebarMode::SideChat {
                    state.active_mode = TitlebarMode::Agents;
                }
            }
        }
        self.close_side_chat_tab(tab_id, window, cx);
    }
}
