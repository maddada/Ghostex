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
//! CDXC:VoiceMode 2026-11-26 WHY: the model gets each session's title, project, computer and state
//! (with "waiting on a question" or "waiting on an approval") and nothing of what a session asks:
//! a question's options or an approval's command, path or tool input never go to the provider
//! unasked, and the read-out rules forbid saying them anyway (`get_pending_prompt` reads them when
//! the user asks). The bounds keep the model's context small (at most `MODEL_ROWS` rows and
//! `MODEL_BYTES` bytes, names cut short) and the frame small (`CLIENT_ROWS`); a longer list ends with
//! how many were left out, and the model reads those with `list_sessions`. A state counts as
//! changed only once two lists in a row agree, as the announcements do, so a turn that flickers
//! idle between tool calls changes nothing; the model hears changes at most every `CHANGES_EVERY`.
//!
//! CDXC:VoiceMode 2026-11-26 WHY: changes are worked out against what the model actually got, never
//! against a message still waiting: until the first list reached it, only the whole list goes (the
//! latest one), and a change at the bound (a listed session moving past it, a counted one moving
//! into it, a different number left out) or a connected computer that could not be read or answers
//! again sends the whole list too. A computer that could not be read is named as such and its
//! sessions are never called closed. The list goes in only while nothing is said, asked for,
//! spoken or run (`give_sessions_context`), and a call about all sessions says its intro once the
//! model has it (`open_with_sessions`), so the first answer already knows the sessions.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::super::sessions::{SessionInfo, SessionsRead, VoiceState};
use super::{Conversation, Ended};

/// Rows the model gets at most in a `[Ghostex sessions]` message.
pub(in crate::voice_agent) const MODEL_ROWS: usize = 25;
/// Bytes a `[Ghostex sessions]` message holds at most; the rows past it are counted, like those
/// past `MODEL_ROWS`.
pub(in crate::voice_agent) const MODEL_BYTES: usize = 8_000;
/// Rows a `sessions` frame carries at most.
pub(in crate::voice_agent) const CLIENT_ROWS: usize = 40;
/// Characters of a title the model gets.
const TITLE_CHARS: usize = 60;
/// Characters of a project's or a computer's name the model gets.
const NAME_CHARS: usize = 40;
/// Characters of why a computer could not be read.
const REASON_CHARS: usize = 240;
/// Room kept in `MODEL_BYTES` for the line that counts the rows left out.
const LEFT_LINE_BYTES: usize = 100;
/// How often the model hears changes at most.
pub(in crate::voice_agent) const CHANGES_EVERY: Duration = Duration::from_secs(15);
/// More changes than this send the whole list again.
const MAX_CHANGES: usize = 10;
/// How long a call about all sessions holds its intro for the first session list; without one by
/// then it opens anyway and the list follows on its own.
pub(in crate::voice_agent) const START_LIST_WAIT: Duration = Duration::from_secs(3);
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
    sub_threads: Option<usize>,
    older_open_questions: usize,
}

fn cut(text: &str, chars: usize) -> String {
    text.chars().take(chars).collect()
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
            sub_threads: session.threads,
            older_open_questions: session.older_open_questions,
        }
    }

    /// `"Title" (project, on Omarchy, a coordinator running 3 sub-threads)`.
    fn named(&self) -> String {
        let title = cut(&self.title, TITLE_CHARS);
        let mut about = Vec::new();
        if !self.project.is_empty() {
            about.push(cut(&self.project, NAME_CHARS));
        }
        if let Some(computer) = &self.computer {
            about.push(format!("on {}", cut(computer, NAME_CHARS)));
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

    /// Its state in words.
    fn spoken_state(&self) -> String {
        let mut state = self.state.as_str().to_string();
        if self.older_open_questions > 0 {
            state.push_str(&format!(
                " ({} older open question{} in its chat)",
                self.older_open_questions,
                if self.older_open_questions == 1 { "" } else { "s" }
            ));
        }
        state
    }

    /// Its line in the model's list.
    fn line(&self) -> String {
        format!("\n- {}: {}", self.named(), self.spoken_state())
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

/// What a whole list tells the model.
#[derive(Clone, Debug, Default)]
struct Told {
    /// The rows it lists, in order.
    listed: Vec<Row>,
    /// The open sessions past the bound, which it only counts, by ref.
    counted: HashSet<String>,
    /// How many those are, and how many of them sleep.
    left: (usize, usize),
    /// The connected computers that could not be read, with why, by name.
    unavailable: Vec<(String, String)>,
    /// The connected computers with sessions in the list, by server id.
    computers: HashSet<String>,
}

/// A message for the model that waits for a quiet moment, what the model knows once it got it,
/// and when it was made.
struct Pending {
    text: String,
    told: Told,
    at: Instant,
}

/// The list the call follows, and what the model and the client last got of it.
#[derive(Default)]
pub(super) struct SessionsContext {
    /// Each session's state as last read, and the state that held for two reads.
    seen: HashMap<String, VoiceState>,
    settled: HashMap<String, VoiceState>,
    /// The settled list, in the watcher's order (waiting first); `None` before the first list.
    rows: Option<Vec<Row>>,
    /// The connected computers the last read could not reach, with why, by name.
    unavailable: Vec<(String, String)>,
    /// What the model got of the list; `None` until a whole list reached it.
    model: Option<Told>,
    /// The model's next message.
    pending: Option<Pending>,
    /// When the model's last message was made: the next change waits `CHANGES_EVERY` after it.
    last_changes: Option<Instant>,
    /// The last `sessions` frame sent.
    frame: Option<Value>,
}

impl SessionsContext {
    /// Takes in a fresh read. Answers the `sessions` frame to send when it changed. With
    /// `for_model`, the model's message (the whole list until one reached it, the changes after)
    /// waits in `pending`.
    pub(super) fn update(
        &mut self,
        read: &SessionsRead,
        now: Instant,
        for_model: bool,
    ) -> Option<Value> {
        let mut rows = Vec::new();
        for session in read
            .sessions
            .iter()
            .filter(|session| session.thread_of.is_none())
        {
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
        let present: HashSet<String> = rows.iter().map(|row| row.reference.clone()).collect();
        self.seen.retain(|reference, _| present.contains(reference));
        self.settled.retain(|reference, _| present.contains(reference));
        self.rows = Some(rows);
        let mut unavailable: Vec<(String, String)> = read
            .unavailable
            .iter()
            .map(|computer| (computer.computer.clone(), computer.reason.clone()))
            .collect();
        unavailable.sort();
        self.unavailable = unavailable;
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

    /// The model's message, handed to it now: from here on, changes are worked out against it.
    pub(super) fn take_pending(&mut self) -> Option<String> {
        let pending = self.pending.take()?;
        self.model = Some(pending.told);
        self.last_changes = Some(pending.at);
        Some(pending.text)
    }

    /// Whether a whole list reached the model.
    pub(super) fn model_has_list(&self) -> bool {
        self.model.is_some()
    }

    /// The last `sessions` frame, for a client that attaches.
    pub(super) fn last_frame(&self) -> Option<Value> {
        self.frame.clone()
    }

    fn model_update(&mut self, now: Instant) {
        let Some(rows) = &self.rows else { return };
        let (whole, told) = model_list(rows, &self.unavailable);
        let Some(known) = &self.model else {
            // Nothing reached the model yet: the latest whole list, never changes to a list it
            // did not get.
            self.pending = Some(Pending {
                text: whole,
                told,
                at: now,
            });
            return;
        };
        // A message already waiting is kept current; a new one waits for the window.
        if self.pending.is_none()
            && self
                .last_changes
                .is_some_and(|last| now.duration_since(last) < CHANGES_EVERY)
        {
            return;
        }
        let text = match model_changes(known, &told) {
            Some(changes) if changes.is_empty() => {
                self.pending = None;
                return;
            }
            Some(changes) if changes.len() <= MAX_CHANGES => {
                let text = format!("{SESSIONS_PREFIX} Changes: {}", changes.join(" "));
                if text.len() <= MODEL_BYTES {
                    text
                } else {
                    whole
                }
            }
            _ => whole,
        };
        let at = self.pending.as_ref().map_or(now, |pending| pending.at);
        self.pending = Some(Pending { text, told, at });
    }

    fn client_frame(&self) -> Option<Value> {
        let rows = self.rows.as_ref()?;
        let mut frame = json!({
            "type": "sessions",
            "sessions": rows.iter().take(CLIENT_ROWS).map(Row::client_value).collect::<Vec<_>>(),
            "more": rows.len().saturating_sub(CLIENT_ROWS),
        });
        // Optional: a client that doesn't know it shows the rows it got, as before.
        if !self.unavailable.is_empty() {
            frame["unavailable"] = json!(self
                .unavailable
                .iter()
                .map(|(computer, _)| computer.as_str())
                .collect::<Vec<_>>());
        }
        Some(frame)
    }
}

/// The whole list for the model, at most `MODEL_ROWS` rows and `MODEL_BYTES` bytes, and what it
/// tells.
fn model_list(rows: &[Row], unavailable: &[(String, String)]) -> (String, Told) {
    let mut text = format!(
        "{SESSIONS_PREFIX} The user's open sessions, waiting ones first ({} in all; sub-threads are counted under their coordinator). This is context, not something to say.",
        rows.len()
    );
    let unread: String = unavailable
        .iter()
        .map(|(computer, reason)| {
            format!(
                "\nNot listed: the sessions on {}. {}",
                cut(computer, NAME_CHARS),
                cut(reason, REASON_CHARS)
            )
        })
        .collect();
    let room = MODEL_BYTES.saturating_sub(text.len() + unread.len() + LEFT_LINE_BYTES);
    let mut listed = Vec::new();
    let mut used = 0;
    for row in rows.iter().take(MODEL_ROWS) {
        let line = row.line();
        if used + line.len() > room {
            break;
        }
        used += line.len();
        text.push_str(&line);
        listed.push(row.clone());
    }
    text.push_str(&unread);
    let left = &rows[listed.len()..];
    let sleeping = left
        .iter()
        .filter(|row| row.state == VoiceState::Sleeping)
        .count();
    if !left.is_empty() {
        text.push_str(&format!(
            "\n{} more not listed ({sleeping} of them sleeping); call list_sessions for them.",
            left.len()
        ));
    }
    let told = Told {
        listed,
        counted: left.iter().map(|row| row.reference.clone()).collect(),
        left: (left.len(), sleeping),
        unavailable: unavailable.to_vec(),
        computers: rows.iter().filter_map(|row| row.server_id.clone()).collect(),
    };
    (text, told)
}

/// What changed for the model between what it was told (`known`) and what a whole list would tell
/// it now: sessions that opened, were renamed, changed state or closed among the rows it gets.
/// `None` when only the whole list says it right: the rows at the bound, the number left out or
/// the computers that could not be read changed, or a connected computer's sessions are all gone
/// (it was disconnected, which says nothing about its sessions).
fn model_changes(known: &Told, now: &Told) -> Option<Vec<String>> {
    if known.left != now.left || known.unavailable != now.unavailable {
        return None;
    }
    let mut changes = Vec::new();
    for row in &now.listed {
        match known
            .listed
            .iter()
            .find(|before| before.reference == row.reference)
        {
            None if known.counted.contains(&row.reference) => return None,
            None => changes.push(format!("{} opened: {}.", row.named(), row.spoken_state())),
            Some(before) => {
                if before.title != row.title {
                    changes.push(format!(
                        "\"{}\" is now called {}.",
                        cut(&before.title, TITLE_CHARS),
                        row.named()
                    ));
                }
                if before.state != row.state
                    || before.older_open_questions != row.older_open_questions
                {
                    changes.push(format!("{} is now {}.", row.named(), row.spoken_state()));
                }
            }
        }
    }
    for before in &known.listed {
        if now
            .listed
            .iter()
            .any(|row| row.reference == before.reference)
        {
            continue;
        }
        if now.counted.contains(&before.reference)
            || before
                .server_id
                .as_ref()
                .is_some_and(|server_id| !now.computers.contains(server_id))
        {
            return None;
        }
        changes.push(format!("{} was closed.", before.named()));
    }
    Some(changes)
}

impl Conversation {
    /// A fresh session list for the call's own list (`SessionsContext`): a changed list goes to
    /// the client, and in a call about all sessions the model's message waits for a quiet moment.
    pub(super) fn follow_sessions(&mut self, read: &SessionsRead) {
        if self.focus.as_ref().is_some_and(|scope| scope.gbot.is_some()) {
            return;
        }
        let for_model = self.focus.is_none();
        if let Some(frame) = self
            .sessions_context
            .update(read, Instant::now(), for_model)
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
            if let Ok((sessions, unavailable)) =
                super::super::sessions::list_everywhere_async(&state).await
            {
                let read = SessionsRead {
                    sessions,
                    unavailable,
                };
                let _ = tx.send(super::Input::Sessions(std::sync::Arc::new(read)));
            }
        });
    }

    /// Hands the model the waiting `[Ghostex sessions]` message, without asking for a reply, while
    /// nothing goes on: no reply asked for, started or playing, the user not speaking, no tool
    /// running and no tool result waiting for its reply. Otherwise it waits for the next tick.
    pub(super) async fn give_sessions_context(&mut self) -> Result<(), Ended> {
        if !self.quiet() || self.responses_requested > 0 || self.outputs_waiting {
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

    /// A call about all sessions says its intro once the model has the session list (handed over
    /// first), or at `intro_waits_until` without it, and only while nothing else goes on.
    pub(super) async fn open_with_sessions(&mut self, now: Instant) -> Result<(), Ended> {
        let Some(until) = self.intro_waits_until else {
            return Ok(());
        };
        self.give_sessions_context().await?;
        if !self.sessions_context.model_has_list() && now < until {
            return Ok(());
        }
        if !self.quiet() || self.responses_requested > 0 {
            return Ok(());
        }
        self.intro_waits_until = None;
        self.say_intro().await
    }
}
