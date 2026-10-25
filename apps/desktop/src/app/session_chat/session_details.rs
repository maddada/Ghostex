//! The Session details panel at the app's level: one app-wide switch behind the header's context
//! ring (a chat's Tasks pill turns it on too), the chat it opens the panel of (the focused
//! session's, while it shows its chat), and the jobs the panel's rows hand to the app (Agents Hub
//! on a skill, Settings > Connectors, the Git menu's Commit and Create PR). The panel itself is the
//! chat's (native_chat/session_details/).
//!
//! CDXC:SessionDetails 2026-10-25 DECISION:
//! User (Q3): the button lives "only in the app header": one button for the window, which in a split opens the panel of the chat last clicked (the focused session's). Q6 was left open, so the switch is app-wide and remembered across restarts, like Claude Desktop's.

use crate::*;
use serde_json::Value;
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// A press on the button this soon after a click on the chat closed the floating card is that same
/// click reaching the button: the card stays closed.
const REOPEN_GUARD: Duration = Duration::from_millis(350);

/// The header button's app-wide switch and where it was painted.
pub(crate) struct SessionDetailsSwitch {
    pub(crate) open: bool,
    /// The button's bounds in the window's content, which the floating card opens under.
    pub(crate) button_bounds: Rc<Cell<gpui::Bounds<gpui::Pixels>>>,
    dismissed_at: Option<Instant>,
}

fn switch_path() -> PathBuf {
    crate::app::helpers::ghostex_state_root().join("gpui-session-details-panel.json")
}

impl Default for SessionDetailsSwitch {
    fn default() -> Self {
        let open = std::fs::read_to_string(switch_path())
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .and_then(|value| value.get("open").and_then(Value::as_bool))
            .unwrap_or(false);
        Self {
            open,
            button_bounds: Rc::default(),
            dismissed_at: None,
        }
    }
}

impl SessionDetailsSwitch {
    fn set_open(&mut self, open: bool) {
        self.open = open;
        let _ = std::fs::write(
            switch_path(),
            serde_json::json!({ "open": open }).to_string(),
        );
    }
}

impl GhostexGpuiApp {
    /// The chat the button opens the panel of: the focused session's, while it shows its chat.
    pub(crate) fn session_details_target_chat(
        &self,
    ) -> Option<gpui::Entity<crate::app::native_chat::state::NativeChatView>> {
        let session_id = self.focused_agents_or_companion_shell_session_id()?;
        if !self.agents_chat_mode_sessions.contains(&session_id) {
            return None;
        }
        self.native_chat_views.get(&session_id).cloned()
    }

    /// The header button.
    pub(crate) fn toggle_session_details(&mut self, cx: &mut gpui::Context<Self>) {
        let open = !self.session_details.open;
        if open
            && self
                .session_details
                .dismissed_at
                .is_some_and(|at| at.elapsed() < REOPEN_GUARD)
        {
            return;
        }
        self.session_details.set_open(open);
        cx.notify();
    }

    /// A chat's Tasks pill: the switch goes on for that chat, which becomes the focused one first
    /// when it was not (in a split the panel belongs to the focused session's chat).
    fn show_session_details_for_chat(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.record_shell_focus_for_session_chat(session_id) == Some(true) {
            self.persist_shell_layout_state();
        }
        if !self.session_details.open {
            self.session_details.set_open(true);
        }
        cx.notify();
    }

    /// A click on the chat closed the floating card: the switch goes off.
    pub(crate) fn session_details_dismissed(&mut self, cx: &mut gpui::Context<Self>) {
        self.session_details.dismissed_at = Some(Instant::now());
        if self.session_details.open {
            self.session_details.set_open(false);
            cx.notify();
        }
    }

    /// Tells every chat whether its panel is wanted this frame: only the button's chat, and only
    /// while the switch is on.
    pub(crate) fn sync_session_details_chats(&self, cx: &mut gpui::Context<Self>) {
        let target = self
            .session_details
            .open
            .then(|| self.session_details_target_chat())
            .flatten()
            .map(|chat| chat.entity_id());
        let anchor = self.session_details.button_bounds.clone();
        for chat in self.native_chat_views.values() {
            let wanted = target == Some(chat.entity_id());
            chat.update(cx, |chat, cx| {
                chat.set_session_details_wanted(wanted, Some(anchor.clone()), cx)
            });
        }
    }

    fn open_agents_hub_on_skill(
        &mut self,
        name: &str,
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.open_gpui_agents_hub_modal(
            &serde_json::json!({"initialTab": "skills", "initialSkill": name}),
            cx,
        );
    }

    fn open_settings_page_from_chat(
        &mut self,
        page: &str,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let modal = GpuiAppModalKind::Settings;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = serde_json::json!({
            "initialTab": page,
            "modal": modal.modal_id(),
            "type": "open",
        });
        open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        self.open_gpui_app_modal_window(
            modal,
            open_message,
            sidebar_state_message,
            Some(window),
            cx,
        );
    }

    /// Commit and Create PR are the Git menu's own (gx_store/git/actions.rs), on the window's project.
    fn run_session_details_git_action(
        &mut self,
        action: &str,
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let selector = match action {
            "commit" => "commit",
            "createPullRequest" => "pr",
            _ => return,
        };
        self.dispatch_gpui_titlebar_git_action_selector(selector, cx);
    }

    /// The panel's jobs for the app, from the chat's host actions. True when `action` was one.
    pub(crate) fn handle_session_details_host_action(
        &mut self,
        session_id: TerminalSessionId,
        action: &str,
        message: &Value,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        match action {
            "sessionDetailsDismissed" => self.session_details_dismissed(cx),
            "showSessionDetails" => self.show_session_details_for_chat(session_id, cx),
            "openAgentsHubSkill" => {
                let name = message
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                self.open_agents_hub_on_skill(&name, window, cx);
            }
            "openSettingsPage" => {
                let page = message
                    .get("page")
                    .and_then(Value::as_str)
                    .unwrap_or("connectors")
                    .to_string();
                self.open_settings_page_from_chat(&page, window, cx);
            }
            "gitAction" => {
                let git = message
                    .get("action")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                self.run_session_details_git_action(&git, window, cx);
            }
            _ => return false,
        }
        true
    }
}
