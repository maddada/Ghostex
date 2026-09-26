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
        let config = NativeChatConfig {
            machine_id: "local".to_string(),
            project_id: session.project_id.clone(),
            session_id: session.session_id.clone(),
            sidebar_session_id: format!(
                "combined-session:{}:{}",
                session.project_id, session.session_id
            ),
            shell_session_id,
            app: Some(cx.weak_entity()),
            parent_native_view: std::ptr::null_mut(),
            client_id: format!("gpui-web-{}", crate::app::helpers::gpui_random_uuid_string().unwrap_or_default()),
            remote: None,
            initial_snapshot: None,
            initial_presentation: self.initial_web_chat_presentation(session),
        };
        let view = cx.new(|cx| NativeChatView::new(config, cx));
        let key = session.clone();
        let subscription = cx.subscribe(&view, move |this, _view, event: &NativeChatEvent, cx| {
            match event {
                NativeChatEvent::Broker(message) => this.web_relay_chat_broker(&key, message),
                NativeChatEvent::Host(message) => this.web_chat_host_action(&key, message, cx),
                NativeChatEvent::ComposerFocused | NativeChatEvent::DraftState(_) => {}
            }
        });
        view.update(cx, |view, _| view.subscriptions.push(subscription));
        self.native_chats
            .insert(session.clone(), (shell_session_id, view.clone()));
        view
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
