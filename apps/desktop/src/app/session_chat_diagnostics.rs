use crate::app::model::*;
use crate::*;
use std::cell::RefCell;
use std::collections::VecDeque;

#[derive(Default)]
pub(crate) struct SessionChatDiagnostics {
    rendered: RefCell<HashMap<TerminalSessionId, serde_json::Value>>,
    recent: RefCell<VecDeque<serde_json::Value>>,
}

impl GhostexGpuiApp {
    /// CDXC:SessionChat 2026-09-07 WHY:
    /// React cannot explain a native loading placeholder after its browser was removed.
    /// Keep the last 24 native lifecycle transitions in memory so a loading regression warning carries evidence even after routine diagnostics expire.
    pub(crate) fn record_session_chat_lifecycle(
        &self,
        session_id: TerminalSessionId,
        event: &str,
        reason: &str,
    ) {
        let key = self.workspace_terminal_key_for_shell_session(session_id);
        let details = serde_json::json!({
            "atMs": epoch_ms(),
            "event": event,
            "reason": reason,
            "sessionId": session_id.0,
            "projectId": self.agents_workspace_project_id,
            "mappedSessionId": key.map(|key| match key {
                GpuiWorkspaceTerminalSessionKey::Local(key) => key.session_id,
                GpuiWorkspaceTerminalSessionKey::Remote(key) => key.session_id,
            }),
            "pageGeneration": self.agents_chat_page_states.get(&session_id).map(|state| state.generation),
            "hasView": self.native_chat_views.contains_key(&session_id),
            "composerReady": self.session_chat_composer_ready_sessions.contains(&session_id),
            "composerEmpty": self.session_chat_composer_empty_reports.get(&session_id),
        });
        let mut recent = self.session_chat_diagnostics.recent.borrow_mut();
        recent.push_back(details.clone());
        if recent.len() > 24 {
            recent.pop_front();
        }
        support_logs::append(support_logs::GpuiSupportLog::SessionChat, event, details);
    }

    pub(crate) fn record_session_chat_render(&self, session_id: TerminalSessionId) {
        let has_view = self.native_chat_views.contains_key(&session_id);
        let switching = self.session_account_switch_progress(session_id).is_some();
        let snapshot = serde_json::json!({
            "projectId": self.agents_workspace_project_id,
            "hasView": has_view,
            "accountSwitching": switching,
            "bootstrapAvailable": self.sidebar_gxserver_bootstrap.is_some(),
            "pageGeneration": self.agents_chat_page_states.get(&session_id).map(|state| state.generation),
        });
        let mut rendered = self.session_chat_diagnostics.rendered.borrow_mut();
        if rendered.get(&session_id) == Some(&snapshot) {
            return;
        }
        let previous = rendered.insert(session_id, snapshot.clone());
        // Bound diagnostic memory even when shell session counters keep growing.
        if rendered.len() > 64 {
            if let Some(old_id) = rendered.keys().copied().find(|id| *id != session_id) {
                rendered.remove(&old_id);
            }
        }
        let regression = !has_view
            && !switching
            && previous.as_ref().is_some_and(|old| {
                old["hasView"] == true && old["projectId"] == snapshot["projectId"]
            });
        let event = if regression {
            "sessionChat.nativeLoadingRegressionWarning"
        } else {
            "sessionChat.nativeViewChanged"
        };
        support_logs::append(
            support_logs::GpuiSupportLog::SessionChat,
            event,
            serde_json::json!({
                "atMs": epoch_ms(),
                "sessionId": session_id.0,
                "previous": previous,
                "current": snapshot,
                "recent": if regression { Some(self.session_chat_diagnostics.recent.borrow().clone()) } else { None },
            }),
        );
    }
}

/// Wall-clock milliseconds, so the in-memory lifecycle ring can be ordered against logged events after routine logs expired.
fn epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since_epoch| since_epoch.as_millis() as u64)
        .unwrap_or_default()
}
