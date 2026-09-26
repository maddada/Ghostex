//! Focus Chat Box, Copy Last Code Block, Copy Last Reply and Toggle Summary Mode, answered for the
//! chat of the focused session. The per-chat work is in `native_chat/chat_hotkeys.rs`.
use crate::*;

impl GhostexGpuiApp {
    /// CDXC:Hotkeys 2026-09-25 WHY:
    /// The chat's key context only covers its chat box, so binding these there made Shift+Esc fire only when the chat box already had focus (the user found it did nothing). They are app hotkeys instead, acting on the chat shown for the focused session from anywhere in the window. Returns false when that session is not in chat view, so the key keeps its ordinary meaning.
    pub(crate) fn run_focused_chat_hotkey(
        &mut self,
        action_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let session_id = self.focused_agents_or_companion_shell_session_id();
        let chat_mode =
            session_id.is_some_and(|session| self.agents_chat_mode_sessions.contains(&session));
        let view = session_id
            .filter(|_| chat_mode)
            .and_then(|session| self.native_chat_views.get(&session).cloned());
        let composer_focused = view.as_ref().is_some_and(|view| {
            view.read(cx)
                .input
                .as_ref()
                .is_some_and(|input| input.read(cx).focus_handle(cx).is_focused(window))
        });
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.focusedChatHotkey",
            serde_json::json!({
                "actionId": action_id,
                "shellFocus": format!("{:?}", self.shell_focus),
                "agentsWorkspaceVisible": self.agents_workspace_visible(),
                "sessionId": session_id.map(|session| session.0),
                "chatMode": chat_mode,
                "hasChatView": view.is_some(),
                "composerAlreadyFocused": composer_focused,
            }),
        );
        let Some(view) = view else {
            return false;
        };
        match action_id {
            "focusChatComposer" => {
                self.reclaim_gpui_root_for_native_chat_composer(window);
                view.update(cx, |view, cx| view.focus_composer_from_hotkey(window, cx));
                true
            }
            "copyLastChatCodeBlock" => {
                view.update(cx, |view, cx| view.copy_last_reply_from_hotkey(true, cx))
            }
            "copyLastChatReply" => {
                view.update(cx, |view, cx| view.copy_last_reply_from_hotkey(false, cx))
            }
            "toggleChatSummaryMode" => {
                view.update(cx, |view, cx| {
                    view.invoke(serde_json::json!({"type":"toggleSummary"}), cx)
                });
                true
            }
            _ => false,
        }
    }
}
