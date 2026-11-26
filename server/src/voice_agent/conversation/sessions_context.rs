//! The call's own session list: what the model knows about the user's sessions without a tool
//! (`[Ghostex sessions]` messages that ask for no reply) and what a client draws as the call's
//! sessions (the `sessions` frame). Both come from the announcement watcher's list, which reaches
//! every call every two seconds (`Input::Sessions`), and from one read when the first client
//! attaches.
//!
//! CDXC:VoiceMode 2026-11-26 DECISION: User (translated): "I want it to start, know all sessions
//! right away, and then I can give instructions directly", and a list of the call's sessions on the
//! phone's voice screen, a tap making the call about that session. A call about all sessions hands
//! the model the list at its start and the changes since; every call except one with a GBot sends
//! clients the list. Sub-threads are left out of both: they are a coordinator's work, which its row
//! counts (the same rule as the announcements, `Announcement::reaches_all_sessions_call`).
//!
//! CDXC:VoiceMode 2026-11-26 WHY: the bounds keep the model's context small (at most
//! `MODEL_ROWS` rows, roughly 600 to 800 tokens) and the frame small (`CLIENT_ROWS`); a longer
//! list ends with how many were left out, and the model reads those with `list_sessions`. A state
//! counts as changed only once two lists in a row agree, as the announcements do, so a turn that
//! flickers idle between tool calls changes nothing; the model hears changes at most every
//! `CHANGES_EVERY`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::super::sessions::{SessionInfo, VoiceState};
use super::{Conversation, Ended};

/// Rows the model gets at most in a `[Ghostex sessions]` message.
pub(in crate::voice_agent) const MODEL_ROWS: usize = 25;
/// Rows a `sessions` frame carries at most.
pub(in crate::voice_agent) const CLIENT_ROWS: usize = 40;
/// Characters of a title the model gets.
const TITLE_CHARS: usize = 60;
/// How often the model hears changes at most.
pub(in crate::voice_agent) const CHANGES_EVERY: Duration = Duration::from_secs(15);
/// More changes than this send the whole list again.
const MAX_CHANGES: usize = 10;
/// The prefix of the messages that carry the list, beside `[Ghostex update]`.
pub(in crate::voice_agent) const SESSIONS_PREFIX: &str = "[Ghostex sessions]";

/// One top-level session as the call shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    reference: String,
    project_id: String,
    session_id: String,
    server_id: Option<String>,
    computer: Option<String>,
    title: String,
    project: String,
    agent: String,
    state: VoiceState,
    waiting: Option<String>,
    sub_threads: Option<usize>,
    older_open_questions: usize,
}

impl Row {
    fn of(session: &SessionInfo, state: VoiceState) -> Self {
        Self {
            reference: session.reference(),
            project_id: session.project_id.clone(),
            session_id: session.session_id.clone(),
            server_id: session.computer.as_ref().map(|c| c.server_id.clone()),
            computer: session.computer.as_ref().map(|c| c.name.clone()),
            title: session.title.clone(),
            project: session.project.clone(),
            agent: session.agent.clone(),
            state,
            waiting: session.waiting.clone(),
            sub_threads: session.threads,
            older_open_questions: session.older_open_questions,
        }
    }

    /// `"Title" (project, on Omarchy, a coordinator running 3 sub-threads)`.
    fn named(&self) -> String {
        let title: String = self.title.chars().take(TITLE_CHARS).collect();
        let mut about = Vec::new();
        if !self.project.is_empty() {
            about.push(self.project.clone());
        }
        if let Some(computer) = &self.computer {
            about.push(format!("on {computer}"));
        }
        if let Some(threads) = self.sub_threads {
            about.push(match threads {
                1 => "a coordinator running 1 sub-thread".to_string(),
                threads => format!("a coordinator running {threads} sub-threads"),
            });
        }
        if about.is_empty() {
            format!("\"{title}\"")
        } else {
            format!("\"{title}\" ({})", about.join(", "))
        }
    }

    /// Its state in words, with what it waits on.
    fn spoken_state(&self) -> String {
        let mut state = self.state.as_str().to_string();
        if let Some(waiting) = &self.waiting {
            state.push_str(&format!(": {waiting}"));
        }
        if self.older_open_questions > 0 {
            state.push_str(&format!(
                " ({} older open question{} in its chat)",
                self.older_open_questions,
                if self.older_open_questions == 1 { "" } else { "s" }
            ));
        }
        state
    }

    fn client_value(&self) -> Value {
        let mut value = json!({
            "ref": self.reference,
            "projectId": self.project_id,
            "sessionId": self.session_id,
            "title": self.title,
            "project": self.project,
            "agent": self.agent,
            "state": client_state(self.state),
            // A call can be about one session of this computer only (`FocusScope::from_sessions`).
            "focusable": self.server_id.is_none(),
        });
        if let Some(server_id) = &self.server_id {
            value["serverId"] = json!(server_id);
        }
        if let Some(computer) = &self.computer {
            value["computer"] = json!(computer);
        }
        if let Some(threads) = self.sub_threads {
            value["subThreads"] = json!(threads);
        }
        value
    }
}

/// The `state` of a `sessions` frame row.
fn client_state(state: VoiceState) -> &'static str {
    match state {
        VoiceState::Working => "working",
        VoiceState::WaitingOnQuestion => "waitingOnQuestion",
        VoiceState::WaitingOnApproval => "waitingOnApproval",
        VoiceState::Finished => "idle",
        VoiceState::Sleeping => "sleeping",
    }
}

/// The list the call follows, and what the model and the client last got of it.
#[derive(Default)]
pub(super) struct SessionsContext {
    /// Each session's state as last read, and the state that held for two reads.
    seen: HashMap<String, VoiceState>,
    settled: HashMap<String, VoiceState>,
    /// The settled list, in the watcher's order (waiting first); `None` before the first list.
    rows: Option<Vec<Row>>,
    /// Every session the model was told about (listed or counted), by ref; `None` until it got a
    /// list.
    model: Option<HashMap<String, Row>>,
    /// A message for the model that waits for a moment without a reply in progress.
    pending: Option<String>,
    last_changes: Option<Instant>,
    /// The last `sessions` frame sent.
    frame: Option<Value>,
}

impl SessionsContext {
    /// Takes in a fresh list. Answers the `sessions` frame to send when it changed. With
    /// `for_model`, the model's message (the whole list the first time, the changes after) waits
    /// in `pending`.
    pub(super) fn update(
        &mut self,
        sessions: &[SessionInfo],
        now: Instant,
        for_model: bool,
    ) -> Option<Value> {
        let mut rows = Vec::new();
        for session in sessions.iter().filter(|session| session.thread_of.is_none()) {
            let reference = session.reference();
            let previous = self.seen.insert(reference.clone(), session.state);
            let settled = match (previous, self.settled.get(&reference)) {
                // A session seen for the first time is listed as it is.
                (None, _) | (_, None) => session.state,
                (Some(previous), _) if previous == session.state => session.state,
                (_, Some(settled)) => *settled,
            };
            self.settled.insert(reference, settled);
            rows.push(Row::of(session, settled));
        }
        let present: std::collections::HashSet<String> =
            rows.iter().map(|row| row.reference.clone()).collect();
        self.seen.retain(|reference, _| present.contains(reference));
        self.settled.retain(|reference, _| present.contains(reference));
        self.rows = Some(rows);
        if for_model {
            self.model_update(now);
        }
        let frame = self.client_frame()?;
        if self.frame.as_ref() == Some(&frame) {
            return None;
        }
        self.frame = Some(frame.clone());
        Some(frame)
    }

    /// The call became one about all sessions (it started so, or it was widened): the model gets
    /// the whole list again.
    pub(super) fn model_restart(&mut self, now: Instant) {
        self.model = None;
        self.pending = None;
        self.model_update(now);
    }

    /// The call is about one session now: the model hears nothing more of the list.
    pub(super) fn model_stop(&mut self) {
        self.model = None;
        self.pending = None;
    }

    /// The model's message, once it is time for it.
    pub(super) fn take_pending(&mut self) -> Option<String> {
        self.pending.take()
    }

    /// The last `sessions` frame, for a client that attaches.
    pub(super) fn last_frame(&self) -> Option<Value> {
        self.frame.clone()
    }

    fn model_update(&mut self, now: Instant) {
        let Some(rows) = &self.rows else { return };
        let Some(known) = &self.model else {
            self.pending = Some(model_list(rows));
            self.model = Some(by_ref(rows));
            self.last_changes = Some(now);
            return;
        };
        let changes = model_changes(known, rows);
        if changes.is_empty()
            || self
                .last_changes
                .is_some_and(|last| now.duration_since(last) < CHANGES_EVERY)
        {
            return;
        }
        self.pending = Some(if changes.len() > MAX_CHANGES {
            model_list(rows)
        } else {
            format!("{SESSIONS_PREFIX} Changes: {}", changes.join(" "))
        });
        self.model = Some(by_ref(rows));
        self.last_changes = Some(now);
    }

    fn client_frame(&self) -> Option<Value> {
        let rows = self.rows.as_ref()?;
        Some(json!({
            "type": "sessions",
            "sessions": rows.iter().take(CLIENT_ROWS).map(Row::client_value).collect::<Vec<_>>(),
            "more": rows.len().saturating_sub(CLIENT_ROWS),
        }))
    }
}

fn by_ref(rows: &[Row]) -> HashMap<String, Row> {
    rows.iter()
        .map(|row| (row.reference.clone(), row.clone()))
        .collect()
}

/// The whole list for the model, at most `MODEL_ROWS` rows.
fn model_list(rows: &[Row]) -> String {
    let mut text = format!(
        "{SESSIONS_PREFIX} The user's open sessions, waiting ones first ({} in all; sub-threads are counted under their coordinator). This is context, not something to say.",
        rows.len()
    );
    for row in rows.iter().take(MODEL_ROWS) {
        text.push_str(&format!("\n- {}: {}", row.named(), row.spoken_state()));
    }
    let left = &rows[rows.len().min(MODEL_ROWS)..];
    if !left.is_empty() {
        let sleeping = left
            .iter()
            .filter(|row| row.state == VoiceState::Sleeping)
            .count();
        text.push_str(&format!(
            "\n{} more not listed ({sleeping} of them sleeping); call list_sessions for them.",
            left.len()
        ));
    }
    text
}

/// What changed for the model since `known`: sessions that opened, were renamed or changed state
/// among the rows it gets (the first `MODEL_ROWS`), and sessions that closed.
fn model_changes(known: &HashMap<String, Row>, rows: &[Row]) -> Vec<String> {
    let listed = &rows[..rows.len().min(MODEL_ROWS)];
    let mut changes = Vec::new();
    for row in listed {
        match known.get(&row.reference) {
            None => changes.push(format!("{} opened: {}.", row.named(), row.spoken_state())),
            Some(before) => {
                if before.title != row.title {
                    changes.push(format!(
                        "\"{}\" is now called {}.",
                        before.title.chars().take(TITLE_CHARS).collect::<String>(),
                        row.named()
                    ));
                }
                if before.state != row.state || before.waiting != row.waiting {
                    changes.push(format!("{} is now {}.", row.named(), row.spoken_state()));
                }
            }
        }
    }
    let all: std::collections::HashSet<&str> =
        rows.iter().map(|row| row.reference.as_str()).collect();
    for before in known.values() {
        if !all.contains(before.reference.as_str()) {
            changes.push(format!("{} was closed.", before.named()));
        }
    }
    changes
}

impl Conversation {
    /// A fresh session list for the call's own list (`SessionsContext`): a changed list goes to
    /// the client, and in a call about all sessions the model's message waits for a quiet moment.
    pub(super) fn follow_sessions(&mut self, sessions: &[SessionInfo]) {
        if self.focus.as_ref().is_some_and(|scope| scope.gbot.is_some()) {
            return;
        }
        let for_model = self.focus.is_none();
        if let Some(frame) = self
            .sessions_context
            .update(sessions, Instant::now(), for_model)
        {
            self.to_client(frame);
        }
    }

    /// Reads the session list once in the background when a call about all sessions starts, so
    /// the model knows it at once instead of at the watcher's next pass; it arrives as
    /// `Input::Sessions`. A call about one session waits for the watcher: the list it reads also
    /// decides whether that session was closed (`follow_focus`), which stays the watcher's call.
    pub(super) fn read_sessions(&self) {
        if self.focus.is_some() {
            return;
        }
        let (state, tx) = (self.state.clone(), self.tx.clone());
        tokio::spawn(async move {
            if let Ok(sessions) = super::super::sessions::list_sessions_async(&state).await {
                let _ = tx.send(super::Input::Sessions(std::sync::Arc::new(sessions)));
            }
        });
    }

    /// Hands the model the waiting `[Ghostex sessions]` message, without asking for a reply,
    /// unless a reply or a tool call is in progress (it then waits for the next tick).
    pub(super) async fn give_sessions_context(&mut self) -> Result<(), Ended> {
        if self.response_active || !self.pending_calls.is_empty() {
            return Ok(());
        }
        let Some(message) = self.sessions_context.take_pending() else {
            return Ok(());
        };
        self.provider_send(json!({
            "type": "conversation.item.create",
            "item": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": message}]},
        }))
        .await
    }
}
