//! The Session details panel at the app's level: each chat's choice behind the header's context
//! ring (a chat's Tasks pill opens that chat's too), the chat it opens the panel of (the focused
//! session's, while it shows its chat), and the jobs the panel's rows hand to the app (Agents Hub
//! on a skill, Settings > Connectors, the Git menu's Commit and Create PR). The panel itself is the
//! chat's (native_chat/session_details/).
//!
//! CDXC:SessionDetails 2026-10-25 DECISION:
//! User (Q3): the button lives "only in the app header": one button for the window, which in a split opens the panel of the chat last clicked (the focused session's). Whether the panel is open is each chat's own and remembered across restarts (`SessionDetailsChoices`, which supersedes Q6's app-wide switch).

use crate::app::native_chat::session_details::{DISMISS_SETTLE, SessionDetailsChoices};
use crate::*;
use serde_json::Value;
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// A press on the button this soon after a click on the chat closed the floating card is that same
/// click reaching the button: the card stays closed.
const REOPEN_GUARD: Duration = Duration::from_millis(350);

/// Each chat's choice and where the header's button was painted.
pub(crate) struct SessionDetailsSwitch {
    choices: SessionDetailsChoices,
    /// The button's bounds in the window's content, which the floating card opens under.
    pub(crate) button_bounds: Rc<Cell<gpui::Bounds<gpui::Pixels>>>,
    dismissed_at: Option<Instant>,
    /// The chat session that last had focus, which the button keeps following while focus is
    /// somewhere that is not a chat (the sidebar, Files, a terminal, another window).
    last_chat: Cell<Option<TerminalSessionId>>,
}

fn switch_path() -> PathBuf {
    crate::app::helpers::ghostex_state_root().join("gpui-session-details-panel.json")
}

impl Default for SessionDetailsSwitch {
    fn default() -> Self {
        let choices = std::fs::read_to_string(switch_path())
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .map(|value| SessionDetailsChoices::from_json(&value))
            .unwrap_or_default();
        Self {
            choices,
            button_bounds: Rc::default(),
            dismissed_at: None,
            last_chat: Cell::new(None),
        }
    }
}

impl SessionDetailsSwitch {
    fn save(&self) {
        let _ = std::fs::write(switch_path(), self.choices.to_json().to_string());
    }
}

impl GhostexGpuiApp {
    /// The session whose chat the button opens the panel of: the focused session's while it shows
    /// its chat, else the chat session that last had focus.
    ///
    /// CDXC:SessionDetails 2026-10-25 DECISION:
    /// User: "when i click away from the chat session it hides the Show session details button. this shouldn't happen please, should keep it for the last focused chat session". Focus moving to the sidebar, Files, a terminal or another window keeps the button on the last focused chat; only another chat taking focus moves it, and it hides only while no chat has had focus (or that chat is gone).
    fn session_details_target_session(&self) -> Option<TerminalSessionId> {
        let shows_chat = |session_id: &TerminalSessionId| {
            self.agents_chat_mode_sessions.contains(session_id)
                && self.native_chat_views.contains_key(session_id)
        };
        let focused = self
            .focused_agents_or_companion_shell_session_id()
            .filter(shows_chat);
        if focused.is_some() {
            self.session_details.last_chat.set(focused);
        }
        focused.or_else(|| self.session_details.last_chat.get().filter(shows_chat))
    }

    /// The chat the button opens the panel of.
    pub(crate) fn session_details_target_chat(
        &self,
    ) -> Option<gpui::Entity<crate::app::native_chat::state::NativeChatView>> {
        let session_id = self.session_details_target_session()?;
        self.native_chat_views.get(&session_id).cloned()
    }

    /// The saved name of a chat's choice: its gxserver session, which outlives the app's tab ids.
    fn session_details_chat_key(&self, session_id: TerminalSessionId) -> Option<String> {
        self.workspace_terminal_session_key(session_id)
            .map(|key| key.to_sidebar_session_id())
    }

    /// The button's chat has its panel open.
    pub(crate) fn session_details_target_open(&self) -> bool {
        self.session_details_target_session()
            .and_then(|session_id| self.session_details_chat_key(session_id))
            .is_some_and(|key| self.session_details.choices.is_open(&key))
    }

    /// The header button.
    pub(crate) fn toggle_session_details(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(key) = self
            .session_details_target_session()
            .and_then(|session_id| self.session_details_chat_key(session_id))
        else {
            return;
        };
        let open = !self.session_details.choices.is_open(&key);
        if open
            && self
                .session_details
                .dismissed_at
                .is_some_and(|at| at.elapsed() < REOPEN_GUARD)
        {
            return;
        }
        self.session_details.choices.set(&key, open);
        self.session_details.save();
        cx.notify();
    }

    /// A chat's Tasks pill: that chat's panel opens, and the chat becomes the focused one first
    /// when it was not (the panel shown is the focused session's chat's).
    fn show_session_details_for_chat(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.record_shell_focus_for_session_chat(session_id) == Some(true) {
            self.persist_shell_layout_state();
        }
        let Some(key) = self.session_details_chat_key(session_id) else {
            return;
        };
        if !self.session_details.choices.is_open(&key) {
            self.session_details.choices.set(&key, true);
            self.session_details.save();
        }
        cx.notify();
    }

    /// The floating card of `session_id`'s chat was dismissed: once the press that did it has
    /// landed, its panel goes off if that chat is still the one on screen.
    pub(crate) fn session_details_dismissed(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(key) = self.session_details_chat_key(session_id) else {
            return;
        };
        self.session_details.dismissed_at = Some(Instant::now());
        self.session_details.choices.dismissed(&key, Instant::now());
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DISMISS_SETTLE).await;
            let _ = this.update(cx, |this, cx| {
                let shown = this
                    .session_details_target_session()
                    .filter(|session_id| this.native_chat_visible_sessions.contains(session_id))
                    .and_then(|session_id| this.session_details_chat_key(session_id));
                if this
                    .session_details
                    .choices
                    .settle(shown.as_deref(), Instant::now())
                {
                    this.session_details.save();
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Tells every chat whether its panel is wanted this frame: only the button's chat, and only
    /// while that chat's panel is open.
    pub(crate) fn sync_session_details_chats(&self, cx: &mut gpui::Context<Self>) {
        let target = self
            .session_details_target_open()
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
        path: Option<&str>,
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.open_gpui_agents_hub_modal(
            &serde_json::json!({
                "initialTab": "skills",
                "initialSkill": name,
                "initialSkillPath": path,
            }),
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
            "sessionDetailsDismissed" => self.session_details_dismissed(session_id, cx),
            "showSessionDetails" => self.show_session_details_for_chat(session_id, cx),
            "openAgentsHubSkill" => {
                let name = message
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let path = message
                    .get("path")
                    .and_then(Value::as_str)
                    .filter(|path| !path.is_empty())
                    .map(str::to_string);
                self.open_agents_hub_on_skill(&name, path.as_deref(), window, cx);
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
