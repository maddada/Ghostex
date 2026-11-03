//! Focused calls: a voice call about one session. The scope a call has (one session, or all of
//! them), what the agent may do with which session in it, which announcements reach it, and the
//! words gxserver hands the model when the scope is set, changes or the session finishes a turn.
//! Client contract: "Focused calls" in docs/2026-10-20/voice-mode/voice-agent-client-contract.md.
//!
//! CDXC:VoiceMode 2026-10-23 DECISION:
//! User, asked "Which way should we build it?", picked "The voice agent you have, limited to one session for the call"; asked "Which sessions can you start a call about?", picked "Any session: a thread, an orchestrator like Main Coordinator, or a plain session"; asked "Can a call switch between one session and all sessions without hanging up?", picked "Yes, both ways: All sessions widens it, and "Voice call about this session" on another session moves it there" (by voice it can only widen; moving to one session is always a click). A focused call is the same `/api/voice` conversation with a scope: its instructions and tools are swapped mid-call, and every other session is refused here, in gxserver, whatever the model asks.
//! CDXC:VoiceMode 2026-10-23 DECISION:
//! User, asked "In a call about an orchestrator, what may it do with that orchestrator's threads?", picked "Read them and answer their questions, but send prompts only to the orchestrator".
//! CDXC:VoiceMode 2026-10-23 DECISION:
//! User, asked "During a call about one session, does it mention your other sessions at all?", picked "No, only this session".
//! CDXC:VoiceMode 2026-10-23 WHY:
//! A call about an orchestrator also hears its threads' questions and approvals (not their finished turns): the user chose that such a call may answer them, and it could not answer what it never hears about.
//! CDXC:VoiceMode 2026-10-23 DECISION:
//! User, asked "When the session finishes its turn during the call, what does the voice agent say?", picked "That it finished, and the gist of its reply in a sentence or two". The finished turn reaches the model with the reply made speakable by Read aloud's own rules (`speakable_segments` in gx-chat-core), so code and tables arrive as a short note.

use std::collections::HashSet;

use serde_json::{json, Value};

use super::conversation::Announcement;
use super::sessions::SessionInfo;

/// The words the agent says when a tool is asked about another session.
pub(super) fn refusal(title: &str) -> String {
    format!(
        "This call is about {title} only. Say \"all sessions\" or press All sessions to talk about the others."
    )
}

/// The words for a prompt or draft aimed at a thread of the focused orchestrator.
pub(super) fn thread_send_refusal(title: &str) -> String {
    format!(
        "In this call, prompts go only to {title}; its threads can be read and answered. Send it to {title}, or say \"all sessions\"."
    )
}

/// What a tool does with a session, for the scope check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Purpose {
    /// Reading its messages or what it waits on, or showing it.
    Read,
    /// Answering its question card, approval or question forms.
    Answer,
    /// Sending it a prompt or a draft.
    Send,
}

/// The session a focused call is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FocusScope {
    pub(super) project_id: String,
    pub(super) session_id: String,
    pub(super) title: String,
    pub(super) project: String,
    pub(super) agent: String,
    /// The orchestrator's title, when the session is one of its threads.
    pub(super) orchestrator: Option<String>,
    pub(super) is_orchestrator: bool,
    /// Its open threads' refs, when it is an orchestrator.
    pub(super) threads: HashSet<String>,
    /// The session was closed during the call.
    pub(super) closed: bool,
}

impl FocusScope {
    /// The scope for `reference` (`projectId:sessionId`), when that session is open.
    pub(super) fn from_sessions(sessions: &[SessionInfo], reference: &str) -> Option<Self> {
        let session = sessions
            .iter()
            .find(|session| session.reference() == reference.trim())?;
        let mut scope = Self {
            project_id: session.project_id.clone(),
            session_id: session.session_id.clone(),
            title: session.title.clone(),
            project: session.project.clone(),
            agent: session.agent.clone(),
            orchestrator: None,
            is_orchestrator: false,
            threads: HashSet::new(),
            closed: false,
        };
        scope.refresh(sessions);
        Some(scope)
    }

    pub(super) fn reference(&self) -> String {
        format!("{}:{}", self.project_id, self.session_id)
    }

    /// Takes in a fresh session list: its title, its threads, and whether it was closed. Answers
    /// whether anything a client draws changed (the title or `closed`).
    pub(super) fn refresh(&mut self, sessions: &[SessionInfo]) -> bool {
        let reference = self.reference();
        let Some(session) = sessions
            .iter()
            .find(|session| session.reference() == reference)
        else {
            let changed = !self.closed;
            self.closed = true;
            return changed;
        };
        let changed = self.closed || self.title != session.title;
        self.closed = false;
        self.title = session.title.clone();
        self.project = session.project.clone();
        self.agent = session.agent.clone();
        self.orchestrator = session
            .thread_of
            .as_ref()
            .map(|parent| parent.title.clone());
        self.is_orchestrator = session.threads.is_some();
        self.threads = sessions
            .iter()
            .filter(|row| {
                row.thread_of
                    .as_ref()
                    .is_some_and(|parent| parent.reference == reference)
            })
            .map(SessionInfo::reference)
            .collect();
        changed
    }

    /// The `focus` object of the contract.
    pub(super) fn to_client(&self) -> Value {
        json!({
            "projectId": self.project_id,
            "sessionId": self.session_id,
            "title": self.title,
            "project": self.project,
            "agent": self.agent,
            "orchestrator": self.orchestrator,
            "isOrchestrator": self.is_orchestrator,
            "closed": self.closed,
        })
    }

    /// The sessions a tool may name in this call: the focused one, and an orchestrator's threads.
    pub(super) fn reachable<'a>(&self, sessions: &'a [SessionInfo]) -> Vec<&'a SessionInfo> {
        let reference = self.reference();
        sessions
            .iter()
            .filter(|session| {
                let own = session.reference();
                own == reference || self.threads.contains(&own)
            })
            .collect()
    }

    /// Whether a tool may do `purpose` with the session `reference`; the refusal to say if not.
    pub(super) fn allows(&self, reference: &str, purpose: Purpose) -> Result<(), String> {
        if reference == self.reference() {
            return Ok(());
        }
        if self.threads.contains(reference) {
            return match purpose {
                Purpose::Read | Purpose::Answer => Ok(()),
                Purpose::Send => Err(thread_send_refusal(&self.title)),
            };
        }
        Err(refusal(&self.title))
    }

    /// Whether an announcement about another session's change reaches this call.
    pub(super) fn wants(&self, announcement: &Announcement) -> bool {
        // A call about one session is about one of this computer's (`from_sessions` takes a
        // `projectId:sessionId` ref), never another computer's.
        if announcement.server_id.is_some() {
            return false;
        }
        let reference = format!("{}:{}", announcement.project_id, announcement.session_id);
        if reference == self.reference() {
            return true;
        }
        self.threads.contains(&reference)
            && matches!(announcement.kind, "waitingOnQuestion" | "waitingOnApproval")
    }
}

/// `ready.focus` and the `focus` frame's value: the scope, or `null` for all sessions.
pub(super) fn client_view(scope: Option<&FocusScope>) -> Value {
    scope.map_or(Value::Null, FocusScope::to_client)
}

/// The prefix of the messages that set or change a call's scope, beside `[Ghostex update]`.
const FOCUS_PREFIX: &str = "[Ghostex focus]";

/// The briefing the model gets when a call starts about a session or moves to one. `facts` is
/// what `tools::briefing` read (state, messages, open questions, draft, threads).
pub(super) fn briefing_message(scope: &FocusScope, facts: &Value, moved: bool) -> String {
    let opening = if moved {
        format!(
            "The user moved this call to one session: \"{}\". From now on it is only about that session.",
            scope.title
        )
    } else {
        format!("This call is about one session: \"{}\".", scope.title)
    };
    format!(
        "{FOCUS_PREFIX} {opening} What it is and where it stands: {facts}\nIn one short sentence, name the session and say where it stands (what it last did or what it waits on), then ask what the user wants. Do not read the messages out."
    )
}

/// The message that tells the model the call is about all sessions again.
pub(super) fn widened_message(previous: Option<&str>) -> String {
    let was = previous
        .map(|title| format!(" instead of only \"{title}\""))
        .unwrap_or_default();
    format!(
        "{FOCUS_PREFIX} This call is about all of the user's sessions now{was}. Your full tools are back. Say in one short sentence that you can talk about all their sessions now."
    )
}

/// The model's line when the focused session closed.
pub(super) fn closed_message(title: &str) -> String {
    format!(
        "\"{title}\" was closed. Tell the user in one short sentence, and offer to talk about all their sessions instead."
    )
}

/// The latest agent message of a chat, as Markdown, when the chat ends on one: the reply the turn
/// that just finished gave.
pub(super) fn last_reply_markdown(chat: &Value) -> Option<String> {
    let messages = chat.get("messages")?.as_array()?;
    let last = messages.iter().rev().find(|message| {
        matches!(
            crate::ghostex_cli::session_chat_transcript::effective_role(message),
            "user" | "assistant"
        ) && message_text(message).is_some()
    })?;
    if crate::ghostex_cli::session_chat_transcript::effective_role(last) != "assistant" {
        return None;
    }
    message_text(last)
}

fn message_text(message: &Value) -> Option<String> {
    let text = message
        .get("blocks")?
        .as_array()?
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    (!text.is_empty()).then_some(text)
}

/// Characters of a finished reply handed to the model: its start and its end when longer.
const REPLY_CHARS: usize = 2_000;

/// A reply's Markdown as Read aloud would say it (prose and lists; code and tables as a short
/// note), cut to its start and end when long.
pub(super) fn speakable_reply(markdown: &str) -> String {
    let text = ghostex_gx_chat_core::read_aloud::speakable_segments(markdown)
        .into_iter()
        .map(|segment| segment.text.trim().to_string())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let count = text.chars().count();
    if count <= REPLY_CHARS {
        return text;
    }
    let half = REPLY_CHARS / 2;
    let head: String = text.chars().take(half).collect();
    let tail: String = text.chars().skip(count - half).collect();
    format!("{head} … {tail}")
}

/// What the model hears when the focused session finished its turn: that it finished and, when
/// the chat has one, the reply to retell.
pub(super) fn turn_finished_message(title: &str, reply: Option<&str>) -> String {
    match reply.map(str::trim).filter(|reply| !reply.is_empty()) {
        Some(reply) => format!(
            "\"{title}\" finished its turn. Its reply: {reply}\nSay that it finished and give the gist of the reply in a sentence or two."
        ),
        None => format!("\"{title}\" finished its turn."),
    }
}

/// The sessions of a focused call's briefing that are threads of it, as the tools list them.
pub(super) fn thread_rows(scope: &FocusScope, sessions: &[SessionInfo]) -> Vec<Value> {
    sessions
        .iter()
        .filter(|session| scope.threads.contains(&session.reference()))
        .map(SessionInfo::to_tool_value)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice_agent::sessions::{ThreadParent, VoiceState};

    fn session(project: &str, id: &str, title: &str) -> SessionInfo {
        SessionInfo {
            project_id: project.to_string(),
            session_id: id.to_string(),
            title: title.to_string(),
            project: "Ghostex".to_string(),
            agent: "Claude".to_string(),
            state: VoiceState::Finished,
            waiting: None,
            last_active_at: String::new(),
            turn_event: None,
            has_chat: true,
            thread_of: None,
            threads: None,
            computer: None,
        }
    }

    fn orchestra() -> Vec<SessionInfo> {
        let mut main = session("P1", "Main", "Main Coordinator");
        main.threads = Some(1);
        let mut thread = session("P1", "T1", "Voice mode");
        thread.thread_of = Some(ThreadParent {
            reference: "P1:Main".to_string(),
            title: "Main Coordinator".to_string(),
            resolved: false,
        });
        vec![main, thread, session("P2", "Other", "Sidebar fix")]
    }

    fn announcement(reference: &str, kind: &'static str) -> Announcement {
        let (project_id, session_id) = reference.split_once(':').unwrap();
        Announcement {
            text: format!("{reference} {kind}"),
            kind,
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
            server_id: None,
            model_text: None,
        }
    }

    #[test]
    fn scope_of_a_thread_names_its_orchestrator() {
        let scope = FocusScope::from_sessions(&orchestra(), "P1:T1").unwrap();
        assert_eq!(scope.title, "Voice mode");
        assert_eq!(scope.orchestrator.as_deref(), Some("Main Coordinator"));
        assert!(!scope.is_orchestrator);
        assert!(scope.threads.is_empty());
        let view = scope.to_client();
        assert_eq!(view["orchestrator"], "Main Coordinator");
        assert_eq!(view["closed"], false);
        assert_eq!(view["projectId"], "P1");
        assert_eq!(view["sessionId"], "T1");
        assert_eq!(view["project"], "Ghostex");
    }

    #[test]
    fn scope_of_an_orchestrator_knows_its_threads() {
        let scope = FocusScope::from_sessions(&orchestra(), "P1:Main").unwrap();
        assert!(scope.is_orchestrator);
        assert_eq!(scope.orchestrator, None);
        assert!(scope.threads.contains("P1:T1"));
        assert_eq!(scope.to_client()["isOrchestrator"], true);
    }

    #[test]
    fn a_missing_session_has_no_scope() {
        assert!(FocusScope::from_sessions(&orchestra(), "P9:Gone").is_none());
        assert_eq!(client_view(None), Value::Null);
    }

    #[test]
    fn other_sessions_are_refused_with_the_agreed_words() {
        let scope = FocusScope::from_sessions(&orchestra(), "P1:T1").unwrap();
        assert_eq!(scope.allows("P1:T1", Purpose::Send), Ok(()));
        assert_eq!(
            scope.allows("P2:Other", Purpose::Read),
            Err("This call is about Voice mode only. Say \"all sessions\" or press All sessions to talk about the others.".to_string())
        );
        // A thread's call cannot reach its orchestrator either.
        assert!(scope.allows("P1:Main", Purpose::Read).is_err());
    }

    #[test]
    fn an_orchestrator_call_reads_and_answers_threads_but_sends_only_to_the_orchestrator() {
        let scope = FocusScope::from_sessions(&orchestra(), "P1:Main").unwrap();
        assert_eq!(scope.allows("P1:T1", Purpose::Read), Ok(()));
        assert_eq!(scope.allows("P1:T1", Purpose::Answer), Ok(()));
        assert_eq!(
            scope.allows("P1:T1", Purpose::Send),
            Err(thread_send_refusal("Main Coordinator"))
        );
        assert_eq!(scope.allows("P1:Main", Purpose::Send), Ok(()));
        assert!(scope.allows("P2:Other", Purpose::Read).is_err());
        let reachable: Vec<String> = scope
            .reachable(&orchestra())
            .into_iter()
            .map(SessionInfo::reference)
            .collect();
        assert_eq!(reachable, vec!["P1:Main", "P1:T1"]);
    }

    #[test]
    fn only_the_focused_sessions_announcements_reach_the_call() {
        let thread_call = FocusScope::from_sessions(&orchestra(), "P1:T1").unwrap();
        assert!(thread_call.wants(&announcement("P1:T1", "turnFinished")));
        assert!(thread_call.wants(&announcement("P1:T1", "waitingOnApproval")));
        assert!(!thread_call.wants(&announcement("P2:Other", "turnFinished")));
        assert!(!thread_call.wants(&announcement("P2:Other", "waitingOnQuestion")));
        assert!(!thread_call.wants(&announcement("P1:Main", "turnFinished")));

        let main_call = FocusScope::from_sessions(&orchestra(), "P1:Main").unwrap();
        assert!(main_call.wants(&announcement("P1:Main", "turnFinished")));
        assert!(main_call.wants(&announcement("P1:T1", "waitingOnQuestion")));
        assert!(main_call.wants(&announcement("P1:T1", "waitingOnApproval")));
        assert!(!main_call.wants(&announcement("P1:T1", "turnFinished")));
        assert!(!main_call.wants(&announcement("P2:Other", "waitingOnApproval")));
    }

    #[test]
    fn refresh_follows_renames_and_notices_a_closed_session() {
        let mut sessions = orchestra();
        let mut scope = FocusScope::from_sessions(&sessions, "P1:T1").unwrap();
        assert!(!scope.refresh(&sessions));
        sessions[1].title = "Voice mode fixes".to_string();
        assert!(scope.refresh(&sessions));
        assert_eq!(scope.title, "Voice mode fixes");
        sessions.remove(1);
        assert!(scope.refresh(&sessions));
        assert!(scope.closed);
        assert_eq!(scope.to_client()["closed"], true);
        // Still closed: nothing new to tell.
        assert!(!scope.refresh(&sessions));
    }

    #[test]
    fn the_finished_reply_is_made_speakable() {
        let chat = json!({"messages": [
            {"role": "user", "blocks": [{"type": "text", "text": "Fix the panel."}]},
            {"role": "assistant", "blocks": [{"type": "tool_use", "name": "Edit"}]},
            {"role": "assistant", "blocks": [{"type": "text", "text": "Done. The panel keeps its place.\n\n```rust\nfn main() {}\n```\n\nShould I do Windows too?"}]},
        ]});
        let markdown = last_reply_markdown(&chat).unwrap();
        let spoken = speakable_reply(&markdown);
        assert!(spoken.contains("The panel keeps its place."));
        assert!(spoken.contains("Should I do Windows too?"));
        assert!(!spoken.contains("fn main"));
        let message = turn_finished_message("Voice mode", Some(&spoken));
        assert!(message.starts_with("\"Voice mode\" finished its turn. Its reply: Done."));
        assert!(message.contains("gist"));
        assert_eq!(
            turn_finished_message("Voice mode", None),
            "\"Voice mode\" finished its turn."
        );
    }

    #[test]
    fn a_chat_ending_on_the_user_has_no_reply() {
        let chat = json!({"messages": [
            {"role": "assistant", "blocks": [{"type": "text", "text": "Earlier reply."}]},
            {"role": "user", "blocks": [{"type": "text", "text": "Now do this."}]},
        ]});
        assert_eq!(last_reply_markdown(&chat), None);
    }

    #[test]
    fn a_long_reply_keeps_its_start_and_end() {
        let long = format!("Start. {} End.", "word ".repeat(1_000));
        let spoken = speakable_reply(&long);
        assert!(spoken.starts_with("Start."));
        assert!(spoken.ends_with("End."));
        assert!(spoken.chars().count() <= REPLY_CHARS + 3);
    }

    #[test]
    fn briefing_and_widening_words() {
        let scope = FocusScope::from_sessions(&orchestra(), "P1:T1").unwrap();
        let briefing = briefing_message(&scope, &json!({"state": "idle"}), false);
        assert!(
            briefing.starts_with("[Ghostex focus] This call is about one session: \"Voice mode\".")
        );
        assert!(briefing.contains("\"state\":\"idle\""));
        let moved = briefing_message(&scope, &json!({}), true);
        assert!(moved.contains("moved this call"));
        assert!(widened_message(Some("Voice mode"))
            .contains("all of the user's sessions now instead of only \"Voice mode\""));
        assert!(closed_message("Voice mode").contains("\"Voice mode\" was closed"));
    }
}
