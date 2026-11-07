//! Step timings of an Ask Ghostex open: one line per step, in ms since the Help menu pick, in the
//! always-on `gpui-help-chat-timing.log` (the web build's console).
//!
//! CDXC:Onboarding 2026-11-07 WHY:
//! User: "clicking here takes ages to open a thread with /ghostex-help in it". Every step that
//! could be timed outside the app took about a second in all (the skill install 0.1 to 0.4 s, the
//! create under 0.1 s, the chat draft at once), while the user's real click was slow and nothing
//! recorded when it happened. Each step of the path now writes its time, so a slow open names its
//! slow step: the pick, the skill install, the store's start, the project add, the create sent and
//! answered, the switch to the Help project, the chat tab shown and activated, the attach plan
//! answered and the draft delivered to the chat composer.
//!
//! SEE-ALSO: apps/desktop/src/app/titlebar/help_menu.rs (`run_gpui_titlebar_help_question`),
//! gx_store/create/os_integration.rs (`gx_store_create_ghostex_help_chat`).

use std::time::Duration;
use web_time::Instant;

use serde_json::{Value, json};

use crate::GhostexGpuiApp;

/// How long after a pick its steps are still logged: well past the two-minute limit of the
/// slowest wait on the path (the skill install), so a stuck open still shows where it stuck.
const HELP_CHAT_TIMING_WINDOW: Duration = Duration::from_secs(180);

/// The Help menu pick being timed, if any; a newer pick replaces it.
#[derive(Default)]
pub(crate) struct HelpChatTiming {
    current: Option<HelpChatPick>,
}

struct HelpChatPick {
    started: Instant,
    /// The created session (project id, session id) once the create answers; the steps after it
    /// are logged only for that session.
    session: Option<(String, String)>,
}

impl HelpChatTiming {
    fn active(&self) -> Option<&HelpChatPick> {
        self.current
            .as_ref()
            .filter(|pick| pick.started.elapsed() < HELP_CHAT_TIMING_WINDOW)
    }

    fn is_session(&self, project_id: &str, session_id: &str) -> bool {
        self.active().is_some_and(|pick| {
            pick.session
                .as_ref()
                .is_some_and(|(project, session)| project == project_id && session == session_id)
        })
    }
}

fn write_step(pick: &HelpChatPick, step: &str, mut details: Value) {
    if let Value::Object(object) = &mut details {
        object.insert(
            "msSincePick".to_string(),
            json!(pick.started.elapsed().as_millis() as u64),
        );
        if let Some((project_id, session_id)) = &pick.session {
            object.insert("projectId".to_string(), json!(project_id));
            object.insert("sessionId".to_string(), json!(session_id));
        }
    }
    crate::support_logs::append_help_chat_timing(&format!("gpui.helpChat.{step}"), details);
}

impl GhostexGpuiApp {
    /// A Help menu row was picked: the clock of the steps below starts here.
    pub(crate) fn help_chat_timing_start(&mut self, question_index: usize) {
        let pick = HelpChatPick {
            started: Instant::now(),
            session: None,
        };
        write_step(&pick, "picked", json!({ "questionIndex": question_index }));
        self.gx_store.create.help_chat_timing.current = Some(pick);
    }

    /// A step of the pick being timed, before its session exists.
    pub(crate) fn help_chat_timing_step(&self, step: &str, details: Value) {
        if let Some(pick) = self.gx_store.create.help_chat_timing.active() {
            write_step(pick, step, details);
        }
    }

    /// The create answered with this session; the later steps follow it.
    pub(crate) fn help_chat_timing_created(&mut self, project_id: &str, session_id: &str) {
        let timing = &mut self.gx_store.create.help_chat_timing;
        if timing.active().is_none() {
            return;
        }
        if let Some(pick) = timing.current.as_mut() {
            pick.session = Some((project_id.to_string(), session_id.to_string()));
            write_step(pick, "createAnswered", json!({}));
        }
    }

    /// A step of the created session's open, logged only for the Help chat's own session.
    pub(crate) fn help_chat_timing_session_step(
        &self,
        project_id: &str,
        session_id: &str,
        step: &str,
        details: Value,
    ) {
        let timing = &self.gx_store.create.help_chat_timing;
        if timing.is_session(project_id, session_id)
            && let Some(pick) = timing.active()
        {
            write_step(pick, step, details);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timing(started: Instant, session: Option<(&str, &str)>) -> HelpChatTiming {
        HelpChatTiming {
            current: Some(HelpChatPick {
                started,
                session: session.map(|(project, id)| (project.to_string(), id.to_string())),
            }),
        }
    }

    #[test]
    fn only_the_created_session_logs_its_steps() {
        let timing = timing(Instant::now(), Some(("P8g86", "G7gvu")));
        assert!(timing.is_session("P8g86", "G7gvu"));
        assert!(!timing.is_session("P8g86", "G9p6t"));
        assert!(!timing.is_session("P3lv0", "G7gvu"));
    }

    #[test]
    fn steps_before_the_create_answers_name_no_session() {
        let timing = timing(Instant::now(), None);
        assert!(timing.active().is_some());
        assert!(!timing.is_session("P8g86", "G7gvu"));
    }

    #[test]
    fn a_pick_older_than_the_window_logs_nothing() {
        let Some(started) =
            Instant::now().checked_sub(HELP_CHAT_TIMING_WINDOW + Duration::from_secs(1))
        else {
            return;
        };
        let timing = timing(started, Some(("P8g86", "G7gvu")));
        assert!(timing.active().is_none());
        assert!(!timing.is_session("P8g86", "G7gvu"));
        assert!(HelpChatTiming::default().active().is_none());
    }
}
