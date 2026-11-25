//! Which chat view belongs to which session in the browser build. The desktop's file resolves a shell pane to a workspace terminal key and parks views per project; here a session is its store key, and a view lives for as long as the page does.
use super::state::{NativeChatConfig, NativeChatEvent, NativeChatView};
use crate::*;
use ghostex_gx_core::SessionKey;

impl GhostexGpuiApp {
    pub(crate) fn ensure_native_chat(
        &mut self,
        session: &SessionKey,
        cx: &mut gpui::Context<Self>,
    ) -> Entity<NativeChatView> {
        if let Some((_, view)) = self.native_chats.get(session) {
            return view.clone();
        }
        let shell_session_id = TerminalSessionId(self.native_chats.len() as u64 + 1);
        // A Cloud Box session's chat talks to the box's own gxserver (`gx_store/cloud_boxes.rs`).
        let machine_id = session
            .machine
            .remote_id()
            .unwrap_or(crate::app::gx_chat::LOCAL_MACHINE_ID)
            .to_string();
        let remote = crate::app::gx_store::cloud_box_request_target(&machine_id);
        let config = NativeChatConfig {
            machine_id,
            project_id: session.project_id.clone(),
            session_id: session.session_id.clone(),
            sidebar_session_id: format!(
                "combined-session:{}:{}",
                session.project_id, session.session_id
            ),
            shell_session_id,
            app: Some(cx.weak_entity()),
            parent_native_view: std::ptr::null_mut(),
            client_id: format!(
                "gpui-web-{}",
                crate::app::helpers::gpui_random_uuid_string().unwrap_or_default()
            ),
            remote,
            initial_snapshot: None,
            initial_presentation: self.initial_web_chat_presentation(session),
        };
        let draft_coordinator_mode = self
            .gx_store
            .unified_sidebar
            .carry_coordinator_draft
            .replace(false);
        let view = cx.new(|cx| {
            let mut view = NativeChatView::new(config, cx);
            view.draft_coordinator_mode = draft_coordinator_mode;
            view.draft_setup_changed = draft_coordinator_mode;
            view
        });
        let key = session.clone();
        let subscription =
            cx.subscribe(
                &view,
                move |this, _view, event: &NativeChatEvent, cx| match event {
                    NativeChatEvent::Broker(message) => this.web_relay_chat_broker(&key, message),
                    NativeChatEvent::Host(message) => this.web_chat_host_action(&key, message, cx),
                    NativeChatEvent::ComposerFocused | NativeChatEvent::DraftState(_) => {}
                    // The header's ring is the web app's to draw.
                    NativeChatEvent::SessionDetailsRing | NativeChatEvent::WorkButton => {
                        cx.notify()
                    }
                    // The header leaves the project out while the where pill names it.
                    NativeChatEvent::WherePillProject => cx.notify(),
                },
            );
        view.update(cx, |view, _| view.subscriptions.push(subscription));
        self.native_chats
            .insert(session.clone(), (shell_session_id, view.clone()));
        view
    }

    /// Single-frame follow into a pending target project on `gpui-web` (Q4849 `"follow"` + Q4850
    /// `"keep-space"`): moves the existing `Entity<NativeChatView>` in `self.native_chats` under
    /// the new `SessionKey` and rebinds its runtime in place with zero remount flash.
    pub(crate) fn follow_moved_draft_chat(
        &mut self,
        view: Entity<NativeChatView>,
        send: super::state::PendingDraftSend,
        target_machine_id: String,
        target_project_id: String,
        target_session_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let old_machine = if send.from_machine_id == crate::app::gx_chat::LOCAL_MACHINE_ID {
            ghostex_gx_core::MachineId::Local
        } else {
            ghostex_gx_core::MachineId::Remote(send.from_machine_id.clone())
        };
        let old_key = SessionKey {
            machine: old_machine,
            project_id: send.from_project_id.clone(),
            session_id: send.from_session_id.clone(),
        };
        let shell_session_id = self
            .native_chats
            .remove(&old_key)
            .map(|(id, _)| id)
            .unwrap_or(send.from_shell_session_id);
        let new_machine = if target_machine_id == crate::app::gx_chat::LOCAL_MACHINE_ID {
            ghostex_gx_core::MachineId::Local
        } else {
            ghostex_gx_core::MachineId::Remote(target_machine_id.clone())
        };
        let new_key = SessionKey {
            machine: new_machine,
            project_id: target_project_id.clone(),
            session_id: target_session_id.clone(),
        };
        let remote = crate::app::gx_store::cloud_box_request_target(&target_machine_id);
        let config = NativeChatConfig {
            machine_id: target_machine_id,
            project_id: target_project_id.clone(),
            session_id: target_session_id.clone(),
            sidebar_session_id: format!(
                "combined-session:{target_project_id}:{target_session_id}"
            ),
            shell_session_id,
            app: Some(cx.weak_entity()),
            parent_native_view: std::ptr::null_mut(),
            client_id: format!(
                "gpui-web-{}",
                crate::app::helpers::gpui_random_uuid_string().unwrap_or_default()
            ),
            remote,
            initial_snapshot: None,
            initial_presentation: self.initial_web_chat_presentation(&new_key),
        };
        let key_for_sub = new_key.clone();
        let subscription =
            cx.subscribe(
                &view,
                move |this, _view, event: &NativeChatEvent, cx| match event {
                    NativeChatEvent::Broker(message) => {
                        this.web_relay_chat_broker(&key_for_sub, message)
                    }
                    NativeChatEvent::Host(message) => {
                        this.web_chat_host_action(&key_for_sub, message, cx)
                    }
                    NativeChatEvent::ComposerFocused | NativeChatEvent::DraftState(_) => {}
                    NativeChatEvent::SessionDetailsRing | NativeChatEvent::WorkButton => {
                        cx.notify()
                    }
                    NativeChatEvent::WherePillProject => cx.notify(),
                },
            );
        view.update(cx, |chat, cx| {
            chat.subscriptions.clear();
            chat.subscriptions.push(subscription);
            chat.rebind_moved_session(
                config,
                send.pending_worktree_action,
                Some(send.submission),
                cx,
            );
        });
        self.native_chats
            .insert(new_key.clone(), (shell_session_id, view));
        self.open_session = Some(new_key.clone());
        self.show_terminal = false;
        self.gx_store_select_opened_session(&new_key, cx);
        cx.notify();
    }

    /// The cached presentation plus the session's folder (its cwd, else its project's), as the
    /// desktop's `initial_session_chat_presentation` seeds it, so tool rows and file cards show
    /// paths relative to it from the first render.
    fn initial_web_chat_presentation(&self, session: &SessionKey) -> Option<serde_json::Value> {
        let mut state = self.chat_presentations.get(session).cloned();
        let presentation = self.gx_store.core.presentation();
        let working_directory = presentation
            .session(session)
            .and_then(|record| record.cwd.clone())
            .filter(|cwd| !cwd.trim().is_empty())
            .or_else(|| {
                presentation
                    .project(&ghostex_gx_core::ProjectKey {
                        machine: session.machine.clone(),
                        project_id: session.project_id.clone(),
                    })
                    .and_then(|project| project.path.clone())
                    .filter(|path| !path.trim().is_empty())
            });
        if let Some(working_directory) = working_directory {
            state.get_or_insert_with(|| serde_json::json!({}))["workingDirectory"] =
                serde_json::json!(working_directory);
        }
        state
    }
}
