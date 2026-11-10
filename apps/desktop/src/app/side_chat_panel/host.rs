//! The chat view a side chat tab shows: created when the tab is first on screen, paused while
//! another tab or view is, dropped when the tab closes.

use gpui::{AppContext as _, Entity, Window};

use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::native_chat::state::{NativeChatConfig, NativeChatEvent, NativeChatView};
use crate::*;

/// Panel chats get shell ids from here up, far above the Agents workspace's own counter, so a
/// panel chat's id can never name an Agents tab.
const SIDE_PANEL_CHAT_SHELL_ID_BASE: u64 = 1 << 62;

pub(crate) struct SidePanelChat {
    /// The id the chat view and its host actions carry. It names no Agents tab;
    /// `workspace_terminal_key_for_shell_session` resolves it to `key`.
    pub(crate) shell_session_id: TerminalSessionId,
    pub(crate) key: GpuiWorkspaceTerminalSessionKey,
    pub(crate) view: Entity<NativeChatView>,
    /// Whether the panel showed it at the last reconcile; its runtime is paused while it does not.
    shown: bool,
}

impl GhostexGpuiApp {
    fn side_panel_chat_shell_session_id(tab_id: SideChatTabId) -> TerminalSessionId {
        TerminalSessionId(SIDE_PANEL_CHAT_SHELL_ID_BASE + tab_id.0)
    }

    /// The session key of a panel chat's shell id, for `workspace_terminal_key_for_shell_session`.
    pub(crate) fn side_panel_chat_key_for_shell_session(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<GpuiWorkspaceTerminalSessionKey> {
        self.side_panel_chats
            .values()
            .find(|chat| chat.shell_session_id == session_id)
            .map(|chat| chat.key.clone())
    }

    /// A chat view by the shell id it carries: an Agents tab's, or a side chat tab's.
    pub(crate) fn native_chat_view_for_shell_session(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<Entity<NativeChatView>> {
        self.native_chat_views
            .get(&session_id)
            .cloned()
            .or_else(|| {
                self.side_panel_chats
                    .values()
                    .find(|chat| chat.shell_session_id == session_id)
                    .map(|chat| chat.view.clone())
            })
    }

    /// The shell id of the side chat the panel shows, which owns the keys while the panel has focus.
    pub(crate) fn side_panel_chat_on_screen_shell_session(&self) -> Option<TerminalSessionId> {
        let tab_id = self.side_chat_tab_on_screen()?;
        self.side_panel_chats
            .get(&tab_id)
            .map(|chat| chat.shell_session_id)
    }

    /// The side chat the panel shows, for the shown-sessions and chat grid reports.
    pub(crate) fn side_panel_chat_shown_key(&self) -> Option<GpuiWorkspaceTerminalSessionKey> {
        let tab_id = self.side_chat_tab_on_screen()?;
        Some(side_chat_workspace_key(&tab_id.key()?))
    }

    pub(crate) fn side_panel_chat_views(&self) -> impl Iterator<Item = &Entity<NativeChatView>> {
        self.side_panel_chats.values().map(|chat| &chat.view)
    }

    /// The tab's chat view, made the first time it is asked for.
    pub(crate) fn ensure_side_panel_chat(
        &mut self,
        tab_id: SideChatTabId,
        cx: &mut gpui::Context<Self>,
    ) -> Option<Entity<NativeChatView>> {
        if let Some(chat) = self.side_panel_chats.get(&tab_id) {
            return Some(chat.view.clone());
        }
        let session = tab_id.key()?;
        let key = side_chat_workspace_key(&session);
        // The same machine id and storage scope an Agents tab's chat of this session would use
        // (`ensure_native_chat`), so its drafts and option pills are the session's own.
        let (machine_id, remote, sidebar_session_id) = match &key {
            GpuiWorkspaceTerminalSessionKey::Local(key) => (
                crate::app::gx_chat::LOCAL_MACHINE_ID.to_string(),
                None,
                gpui_combined_presentation_session_id(&key.project_id, &key.session_id),
            ),
            GpuiWorkspaceTerminalSessionKey::Remote(key) => (
                key.remote_machine_id.clone(),
                Some(self.gpui_remote_gxserver_request_target(&key.remote_machine_id)?),
                gpui_remote_scoped_session_id(
                    &key.remote_machine_id,
                    &key.project_id,
                    &key.session_id,
                ),
            ),
        };
        let shell_session_id = Self::side_panel_chat_shell_session_id(tab_id);
        let config = NativeChatConfig {
            machine_id,
            project_id: session.project_id.clone(),
            session_id: session.session_id.clone(),
            sidebar_session_id,
            shell_session_id,
            app: Some(cx.weak_entity()),
            parent_native_view: self.parent_ns_view,
            client_id: format!("native-desktop-{}", std::process::id()),
            remote,
            initial_snapshot: None,
            initial_presentation: self.initial_session_chat_presentation(Some(&key)),
        };
        let view = cx.new(|cx| NativeChatView::new(config, cx));
        let view_id = view.entity_id();
        let subscription =
            cx.subscribe(
                &view,
                move |this, view, event: &NativeChatEvent, cx| match event {
                    NativeChatEvent::Broker(message) => {
                        if message["method"] == "presentation"
                            && let Some(key) =
                                this.side_panel_chats.get(&tab_id).map(|c| c.key.clone())
                        {
                            this.cache_session_chat_presentation(key, &message["params"]["state"]);
                        }
                    }
                    NativeChatEvent::ComposerFocused => {
                        this.reclaim_gpui_root_for_chrome_input_focus();
                        let panel = ShellFocusTarget::ProjectEditorSurface(TitlebarMode::SideChat);
                        if this.side_chat_tab_on_screen() == Some(tab_id)
                            && this.shell_focus != panel
                        {
                            this.set_shell_focus(panel);
                            this.persist_shell_layout_state();
                            cx.notify();
                        }
                    }
                    NativeChatEvent::DraftState(_) => {}
                    NativeChatEvent::SessionDetailsRing => cx.notify(),
                    NativeChatEvent::Host(message) => {
                        let message = message.clone();
                        // In the side chat's own window (CDXC:SessionChat 2026-11-11 on
                        // `update_in_chat_window` in native_chat/binding.rs).
                        let chat_window = view.read(cx).main_window;
                        cx.spawn(async move |this, cx| {
                            Self::update_in_chat_window(
                                &this,
                                chat_window,
                                cx,
                                move |this, window, cx| {
                                    // A view the tab has since replaced or dropped speaks for nothing.
                                    if this
                                        .side_panel_chats
                                        .get(&tab_id)
                                        .is_none_or(|chat| chat.view.entity_id() != view_id)
                                    {
                                        return;
                                    }
                                    this.receive_side_panel_chat_host_action(
                                        shell_session_id,
                                        &message,
                                        window,
                                        cx,
                                    );
                                },
                            );
                        })
                        .detach();
                    }
                },
            );
        view.update(cx, |view, _| view.subscriptions.push(subscription));
        self.side_panel_chats.insert(
            tab_id,
            SidePanelChat {
                shell_session_id,
                key,
                view: view.clone(),
                shown: false,
            },
        );
        // A side chat on a remote machine needs that machine's chat socket even when no Agents
        // chat of it is open.
        self.refresh_session_chat_runtime_endpoints(false, cx);
        Some(view)
    }

    /// Drops the chats of tabs that closed (or belong to another project's strip now), makes the
    /// one on screen, and pauses every other one: a hidden side chat stays retained and followed
    /// by the chat host and catches up in one drain when it is shown again, as a hidden Agents chat
    /// does (`pause_session_chat_runtime`).
    pub(crate) fn reconcile_side_panel_chats(&mut self, cx: &mut gpui::Context<Self>) {
        let open = self.view_strip_layout.side_chat_tabs().collect::<Vec<_>>();
        let closed = self
            .side_panel_chats
            .keys()
            .copied()
            .filter(|tab_id| !open.contains(tab_id))
            .collect::<Vec<_>>();
        for tab_id in closed {
            if let Some(chat) = self.side_panel_chats.remove(&tab_id) {
                chat.view
                    .update(cx, |view, cx| view.dismiss_windows_for_hidden_pane(cx));
            }
        }
        let on_screen = self.side_chat_tab_on_screen();
        if let Some(tab_id) = on_screen {
            self.ensure_side_panel_chat(tab_id, cx);
        }
        for (tab_id, chat) in &mut self.side_panel_chats {
            let shown = on_screen == Some(*tab_id);
            if chat.shown == shown {
                continue;
            }
            chat.shown = shown;
            chat.view.update(cx, |view, cx| {
                if let Some(runtime) = &view.runtime {
                    runtime.set_paused(!shown);
                }
                if shown {
                    view.restore_windows_for_shown_pane(cx);
                    cx.notify();
                } else {
                    view.dismiss_windows_for_hidden_pane(cx);
                }
            });
        }
    }

    /// Whether the chat on screen was reconciled as shown; a frame that draws it before the
    /// reconcile ran asks for one.
    pub(crate) fn side_panel_chat_marked_shown(&self, tab_id: SideChatTabId) -> bool {
        self.side_panel_chats
            .get(&tab_id)
            .is_some_and(|chat| chat.shown)
    }

    /// A panel chat's host action. Most go through the Agents chat's own switch
    /// (`receive_session_chat_host_action`), which resolves the panel chat's shell id to its
    /// session; the few that act on an Agents tab or its terminal stop here.
    fn receive_side_panel_chat_host_action(
        &mut self,
        shell_session_id: TerminalSessionId,
        message: &serde_json::Value,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        match message.get("action").and_then(serde_json::Value::as_str) {
            // Composer readiness and draft handoffs are the Agents tab's bookkeeping, keyed by tab.
            Some(
                "composerReady"
                | "composerDraftState"
                | "draftHandoffToChatComplete"
                | "draftHandoffToChatRetry"
                | "draftHandoffToTerminalComplete"
                | "draftHandoffToTerminalFailed",
            ) => {}
            // A side chat has no Agents tab, so there is no terminal to switch to or type into.
            Some(
                "terminalView"
                | "switchToTerminal"
                | "agentPickerTerminalView"
                | "promptEditor"
                | "attachPath",
            ) => self.dispatch_gpui_app_modal_toast(
                "info",
                "A side chat has no terminal view",
                "Side chats open as a chat in the side panel.",
                cx,
            ),
            _ => self.receive_session_chat_host_action(
                shell_session_id,
                &message.to_string(),
                window,
                cx,
            ),
        }
    }

    /// Bring back or Delete from a side chat tab's menu, sent as the side chat's own chat action
    /// so the chat core makes the call and settles it (a toast, or `sideChatDeleted`).
    pub(crate) fn invoke_side_chat_tab_action(
        &mut self,
        tab_id: SideChatTabId,
        action: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(view) = self.ensure_side_panel_chat(tab_id, cx) else {
            return;
        };
        view.update(cx, |view, cx| {
            view.invoke(serde_json::json!({ "type": action }), cx)
        });
    }
}

/// The desktop's key for a side chat's session.
fn side_chat_workspace_key(key: &ghostex_gx_core::SessionKey) -> GpuiWorkspaceTerminalSessionKey {
    match &key.machine {
        ghostex_gx_core::MachineId::Local => {
            GpuiWorkspaceTerminalSessionKey::Local(GpuiLocalWorkspaceSessionKey {
                project_id: key.project_id.clone(),
                session_id: key.session_id.clone(),
            })
        }
        ghostex_gx_core::MachineId::Remote(machine_id) => {
            GpuiWorkspaceTerminalSessionKey::Remote(GpuiRemoteAttachSessionKey {
                remote_machine_id: machine_id.clone(),
                project_id: key.project_id.clone(),
                session_id: key.session_id.clone(),
            })
        }
    }
}
