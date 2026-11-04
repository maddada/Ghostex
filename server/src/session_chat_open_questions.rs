//! The questions a session's agent asked that are still waiting on the user: every ```questions
//! form and question page (`ghostex show` with questions) in the whole conversation that nothing
//! has answered, withdrawn or asked again since. They are read by gx-chat-core's own parser (the
//! one behind the chat's Previous question pill), so the sidebar and the chat never disagree.
//!
//! Each session keeps a small ledger in memory, fed only the bytes its transcript grew by: the
//! replies that can ask or withdraw questions, the answers that came back, and how many messages
//! the user sent. A sweep every few seconds and every turn start and end bring it up to date, so
//! the count costs a file `stat` while nothing changes.
//!
//! CDXC:SessionStatus 2026-10-20 DECISION:
//! User: "when a session has open questions we must show an indicator in the sidebar and as notification ... i didn't know this session needed answers from me which is bad", then: "the pink dot ... should also appear when a session has open questions listed in its status text ... Please fix that so the pink dot shows whenever there are any open questions on a session, including inline HTML ones." gxserver records the session's open questions (`sessionChatOpenQuestions`) and the presentation adds them to `pendingQuestionCount`, so every client draws the pink question dot ("Answer requested") that Codex's own questions already use, also while the agent works. It never clears by looking at the session or by sending something else: only an answer (from the form, the page, or answers waiting in the session's queue), a withdrawal, or the same tags asked again clear a question. Supersedes the same day's rule that counted only the last turn and cleared on any message. Orchestrator threads are left out: their questions go to their orchestrator in its thread report.
//! CDXC:Notifications 2026-10-20 DECISION:
//! Same request: one notification per turn that asked. When a turn ends with new open questions from that turn, its Finished feed row turns into Needs input, saying how many it asked; a turn that ended without attention (agents whose finished turn is idle) is raised to attention once, which sounds, shows the system notification and writes the feed row like any other wait for input. Questions from earlier turns never notify again.
//! SEE-ALSO: packages/gx-chat-core/src/transcript/open_questions.rs (what counts as asked, answered and withdrawn), server/src/presentation/session_projection.rs (`pendingQuestionCount`), server/src/voice_agent/question_forms.rs (the voice agent lists and answers these), server/src/visual_pages/questions.rs.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use ghostex_gx_chat_core::protocol::ChatMessage;
use ghostex_gx_chat_core::questions::resource_index::{self, ResourceEntry};
use ghostex_gx_chat_core::transcript::open_questions::{
    asker_entries, askers, mark_inherited, settle_askers, AskerKind, Found,
};
use ghostex_gx_chat_core::transcript::page_answers::answers_page_id;
use ghostex_gx_chat_core::transcript::question_forms::{question_forms, QuestionForm};
use rusqlite::OptionalExtension;
use serde_json::{json, Map, Value};

use crate::domain::DomainRepository;
use crate::logging::{GxserverLogInput, LogLevel};
use crate::server::{read_runtime_text, AppState};
use crate::session_chat::{
    read_incremental_transcript_messages, read_transcript_file_version,
    resolve_session_chat_transcript_agent, resolve_session_chat_transcript_path,
    session_chat_line_decoder, SessionChatBlock, SessionChatIncrementalState, SessionChatMessage,
    SessionChatRole, SessionChatTranscriptAgent,
};
use crate::session_chat_follower::session_chat_agent_for_session;
use crate::session_status::TURN_COMPLETE_ATTENTION_SOURCE;
use crate::storage::open_gxserver_database;

/// Where a session's open questions are kept.
const RUNTIME_KEY: &str = "sessionChatOpenQuestions";
/// Where the forms and pages the user put off "For later" are kept (form or page ids). They stay
/// answerable but leave the count, so no pink dot, no `answer requested (n)` and no notification
/// (the SessionStatus 2026-10-27 DECISION in gx-chat-core's questions/later.rs).
const LATER_KEY: &str = "sessionChatQuestionsForLater";
/// Where the forms and pages the user removed without answering are kept (form or page ids). They
/// no longer wait, count or notify (the SessionChat 2026-10-30 DECISION in gx-chat-core's
/// questions/later.rs).
const REMOVED_KEY: &str = "sessionChatQuestionsRemoved";
/// How many removals a session keeps at most; the oldest are dropped first.
const REMOVED_LIMIT: usize = 500;
/// How many For later marks a session keeps at most; answered ones are dropped as they settle.
const LATER_LIMIT: usize = 500;
/// The agent's last words reach its transcript a moment after the turn-end hook.
const SETTLE_DELAY: Duration = Duration::from_millis(1_500);
/// How often running sessions are brought up to date.
const SWEEP_INTERVAL: Duration = Duration::from_secs(10);

/// One form or question page still waiting on the user.
#[derive(Clone, Debug)]
pub(crate) struct OpenAsker {
    /// The form's id or the page's id.
    pub(crate) id: String,
    pub(crate) kind: AskerKind,
    /// The form itself, for a ```questions block.
    pub(crate) form: Option<QuestionForm>,
    /// How many of its questions wait on the user (`Asker::waiting`).
    waiting: usize,
    /// It was asked after the user's last message.
    latest_turn: bool,
}

impl OpenAsker {
    fn count(&self) -> usize {
        self.waiting
    }
}

/// What the transcript has said about questions so far.
#[derive(Default)]
pub(crate) struct Ledger {
    path: PathBuf,
    identity: String,
    incremental: SessionChatIncrementalState,
    /// The replies that can ask or withdraw questions, oldest first, with the user's message count
    /// when each arrived.
    asking: Vec<(ChatMessage, usize)>,
    /// The forms and pages whose answers are in the transcript.
    answered: Vec<String>,
    /// Where each answers message starts in the file, by form or page id.
    answers_at: HashMap<String, u64>,
    /// How many messages the user (or anyone) sent.
    turns: usize,
    /// The forms and pages a turn end already notified about.
    notified: HashSet<String>,
    /// Every form, page and chart the transcript holds, as the resource index records them
    /// (statuses decided in [`Ledger::index`]).
    entries: Vec<ResourceEntry>,
    /// What the replies ask and withdraw, in order, kept small: the readings behind `entries`'
    /// statuses (`asking` keeps the messages themselves for the open questions).
    found: Vec<Found>,
    /// The index as last stored, loaded on first use and kept current, so a sweep that finds
    /// nothing new never reads or writes the table.
    stored: Option<Vec<ResourceEntry>>,
}

/// One lock per session, so a long first read holds up only that session's own readers (a turn
/// end waits for the sweep's read instead of reading the file again).
fn ledgers() -> &'static Mutex<HashMap<String, Arc<Mutex<Ledger>>>> {
    static LEDGERS: OnceLock<Mutex<HashMap<String, Arc<Mutex<Ledger>>>>> = OnceLock::new();
    LEDGERS.get_or_init(Default::default)
}

/// How many questions wait on the user in this session, as last recorded.
pub(crate) fn open_question_count(session: &Value) -> usize {
    session
        .pointer(&format!("/runtimeSettings/{RUNTIME_KEY}/count"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize
}

/// The forms and pages this session's user put off For later, as last recorded.
pub(crate) fn later_ids(session: &Value) -> Vec<String> {
    session
        .pointer(&format!("/runtimeSettings/{LATER_KEY}"))
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The forms and pages this session's user removed without answering, as last recorded.
pub(crate) fn removed_ids(session: &Value) -> Vec<String> {
    session
        .pointer(&format!("/runtimeSettings/{REMOVED_KEY}"))
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// [`later_ids`] read straight from the database, for the chat's frames (`laterQuestionIds`).
pub(crate) fn read_later_ids(
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
) -> Vec<String> {
    read_id_list(db, project_id, session_id, LATER_KEY)
}

/// [`removed_ids`] read straight from the database, for the chat's frames (`removedQuestionIds`).
pub(crate) fn read_removed_ids(
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
) -> Vec<String> {
    read_id_list(db, project_id, session_id, REMOVED_KEY)
}

fn read_id_list(
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
    key: &str,
) -> Vec<String> {
    let value: Option<String> = db
        .query_row(
            &format!(
                "SELECT json_extract(runtimeSettingsJson, '$.{key}') FROM sessions WHERE projectId = ?1 AND sessionId = ?2"
            ),
            rusqlite::params![project_id, session_id],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten()
        .flatten();
    value
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Writes the session's For later list (`None` or empty clears it) and tells every client: the
/// chat's frame carries the list, the sidebar's presentation the count.
fn write_later(
    state: &AppState,
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
    ids: &[String],
) -> anyhow::Result<()> {
    write_id_list(state, db, project_id, session_id, LATER_KEY, ids)
}

fn write_id_list(
    state: &AppState,
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
    key: &str,
    ids: &[String],
) -> anyhow::Result<()> {
    let changed = if ids.is_empty() {
        db.execute(
            &format!(
                r#"UPDATE sessions SET runtimeSettingsJson = json_remove(runtimeSettingsJson, '$.{key}')
                WHERE projectId = ?1 AND sessionId = ?2
                AND json_extract(runtimeSettingsJson, '$.{key}') IS NOT NULL"#
            ),
            rusqlite::params![project_id, session_id],
        )?
    } else {
        db.execute(
            &format!(
                r#"UPDATE sessions SET runtimeSettingsJson = json_set(runtimeSettingsJson, '$.{key}', json(?3))
                WHERE projectId = ?1 AND sessionId = ?2"#
            ),
            rusqlite::params![project_id, session_id, serde_json::to_string(ids)?],
        )?
    };
    if changed > 0 {
        let repository = DomainRepository::new(db, state.metadata.server_id.as_str());
        if let Some(session) = repository.get_session(project_id, session_id)? {
            crate::session_chat_interactive::emit_session_chat_prompt_state_frame(state, &session);
        }
    }
    Ok(())
}

/// `answerSessionChatPrompt` with `kind: "questionForLater"`: puts a form or page off For later
/// (`later`), or back to unanswered, then brings the session's count up to date. Blocking.
pub(crate) fn set_later(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    id: &str,
    later: bool,
) -> anyhow::Result<()> {
    let db = open_gxserver_database(&state.paths)?;
    let mut ids = read_later_ids(&db, project_id, session_id);
    let known = ids.iter().any(|known| known == id);
    if known == later {
        return Ok(());
    }
    if later {
        ids.push(id.to_string());
        if ids.len() > LATER_LIMIT {
            ids.drain(..ids.len() - LATER_LIMIT);
        }
    } else {
        ids.retain(|known| known != id);
    }
    write_later(state, &db, project_id, session_id, &ids)?;
    refresh(state, project_id, session_id, None);
    Ok(())
}

/// `answerSessionChatPrompt` with `kind: "questionRemove"`: removes a form or page without
/// answering it (it leaves the For later list too), then brings the session's count up to date.
/// Nothing reaches the agent. Blocking.
pub(crate) fn set_removed(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    id: &str,
) -> anyhow::Result<()> {
    let db = open_gxserver_database(&state.paths)?;
    let mut removed = read_removed_ids(&db, project_id, session_id);
    if removed.iter().any(|known| known == id) {
        return Ok(());
    }
    removed.push(id.to_string());
    if removed.len() > REMOVED_LIMIT {
        removed.drain(..removed.len() - REMOVED_LIMIT);
    }
    let mut later = read_later_ids(&db, project_id, session_id);
    if later.iter().any(|known| known == id) {
        later.retain(|known| known != id);
        write_later(state, &db, project_id, session_id, &later)?;
    }
    write_id_list(state, &db, project_id, session_id, REMOVED_KEY, &removed)?;
    refresh(state, project_id, session_id, None);
    Ok(())
}

/// When this session was forked from another, in epoch ms: the time Ghostex created it. The chat
/// carries it as `forkedAt`, and questions asked before it belong to the parent
/// (gx-chat-core's `mark_inherited`). `None` when the session is no fork.
pub(crate) fn read_forked_at(
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
) -> Option<i64> {
    let created: Option<String> = db
        .query_row(
            r#"SELECT createdAt FROM sessions WHERE projectId = ?1 AND sessionId = ?2
            AND (json_type(runtimeSettingsJson, '$.forkedFromSessionId') = 'text'
              OR json_type(launchSettingsJson, '$.forkedFromSessionId') = 'text')"#,
            rusqlite::params![project_id, session_id],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten();
    chrono::DateTime::parse_from_rfc3339(&created?)
        .ok()
        .map(|at| at.timestamp_millis())
}

fn text_of(message: &SessionChatMessage) -> String {
    message
        .blocks
        .iter()
        .filter_map(|block| match block {
            SessionChatBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

impl Ledger {
    pub(crate) fn observe(&mut self, messages: Vec<SessionChatMessage>) {
        for message in messages.into_iter().filter(|message| !message.queued) {
            match message.role {
                SessionChatRole::User => {
                    // A user message ends the turn every earlier reply still open sits in.
                    if let Some(at) = message.byte_offset {
                        for entry in self.entries.iter_mut().rev() {
                            if entry.turn_end.is_some() {
                                break;
                            }
                            if entry.offset.is_some_and(|offset| offset < at) {
                                entry.turn_end = Some(at);
                            }
                        }
                    }
                    let text = text_of(&message);
                    if text.trim().is_empty() {
                        continue;
                    }
                    self.turns += 1;
                    if let Some(id) = answers_page_id(&text) {
                        if let Some(at) = message.byte_offset {
                            self.answers_at.entry(id.clone()).or_insert(at);
                        }
                        self.answered.push(id);
                    }
                }
                SessionChatRole::Assistant => {
                    let text = text_of(&message);
                    if !(text.contains("```") || text.contains("~~~")) {
                        continue;
                    }
                    // Only a fenced block naming questions can ask or withdraw any; one naming
                    // a drawing or a page is a resource too (the index lists them all).
                    let asks = text.contains("questions");
                    if !asks && !text.contains("visual") && !text.contains("vega") {
                        continue;
                    }
                    let converted = serde_json::to_value(&message)
                        .ok()
                        .and_then(|value| serde_json::from_value::<ChatMessage>(value).ok());
                    if let Some(converted) = converted {
                        resource_index::append_unique(
                            &mut self.entries,
                            resource_index::scan_message(&converted),
                        );
                        if asks {
                            let found =
                                asker_entries(std::slice::from_ref(&converted), &self.found);
                            self.found.extend(found);
                            self.asking.push((converted, self.turns));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// The open forms and pages, given the answers waiting in the queue or sent from a page.
    fn open(
        &self,
        also_answered: &[String],
        forked_at: Option<i64>,
        page_answered: impl Fn(&str) -> bool,
    ) -> Vec<OpenAsker> {
        let messages: Vec<ChatMessage> = self
            .asking
            .iter()
            .map(|(message, _)| message.clone())
            .collect();
        let turn_of: HashMap<&str, usize> = self
            .asking
            .iter()
            .map(|(message, turn)| (message.id.as_str(), *turn))
            .collect();
        let mut answered = self.answered.clone();
        answered.extend(also_answered.iter().cloned());
        let forms = question_forms(&messages);
        let mut found = askers(&messages, &answered);
        mark_inherited(&mut found, &messages, forked_at);
        found
            .into_iter()
            .filter(|asker| !asker.answered && !asker.obsolete && !asker.inherited)
            .filter(|asker| asker.kind != AskerKind::Page || !page_answered(&asker.id))
            .map(|asker| {
                let form = forms.iter().find(|form| form.id == asker.id).cloned();
                OpenAsker {
                    waiting: asker.waiting(form.as_ref()),
                    form,
                    latest_turn: turn_of.get(asker.message_id.as_str()) == Some(&self.turns),
                    id: asker.id,
                    kind: asker.kind,
                }
            })
            .collect()
    }
}

impl Ledger {
    /// The session's resource index as the transcript reads now: every form, page and chart, with
    /// the status the answers and withdrawals so far give each. `forked_at` leaves out what a fork
    /// inherited from its parent.
    pub(crate) fn index(
        &self,
        also_answered: &[String],
        forked_at: Option<i64>,
        page_answered: impl Fn(&str) -> bool,
    ) -> Vec<ResourceEntry> {
        let mut answered = self.answered.clone();
        answered.extend(also_answered.iter().cloned());
        let mut settled = settle_askers(&self.found, &answered);
        if let Some(forked_at) = forked_at {
            for asker in &mut settled {
                asker.inherited = asker.at.is_some_and(|at| at < forked_at);
            }
        }
        let mut entries = resource_index::settle(self.entries.clone(), &settled);
        for entry in &mut entries {
            if entry.kind == "page" && entry.status == "waiting" && page_answered(&entry.id) {
                entry.status = "answered".to_string();
            }
            if entry.status == "answered" {
                entry.answers_offset = self.answers_at.get(&entry.id).copied();
            }
        }
        entries
    }
}

/// The forms and pages waiting on the user in this session, reading only what its transcript
/// added since the last call. With `notify_turn`, also answers the ones the latest turn asked that
/// no turn end announced yet, and remembers them as announced. `None` when the session has no
/// transcript Ghostex reads.
fn read_open(
    state: &AppState,
    db: &rusqlite::Connection,
    session: &Value,
    notify_turn: bool,
) -> Option<(Vec<OpenAsker>, Vec<OpenAsker>)> {
    let project_id = session.get("projectId").and_then(Value::as_str)?;
    let session_id = session.get("sessionId").and_then(Value::as_str)?;
    let agent =
        resolve_session_chat_transcript_agent(session_chat_agent_for_session(session).as_deref())?;
    let path = resolve_session_chat_transcript_path(
        agent,
        read_runtime_text(session, "agentSessionId").as_deref(),
        read_runtime_text(session, "agentSessionPath").as_deref(),
    )?;
    if agent == SessionChatTranscriptAgent::OpenCode {
        crate::session_chat_opencode::refresh_for_path(&path);
    }
    let version = read_transcript_file_version(&path).ok()?;
    let key = crate::server::session_observer_key(project_id, session_id);
    let entry = ledgers().lock().ok()?.entry(key).or_default().clone();
    let mut ledger = entry.lock().ok()?;
    if ledger.path != path
        || ledger.identity != version.identity
        || version.size < ledger.incremental.offset
    {
        let notified = std::mem::take(&mut ledger.notified);
        *ledger = Ledger {
            path: path.clone(),
            identity: version.identity.clone(),
            notified,
            ..Default::default()
        };
    }
    // Batches, so a first read of a long conversation never holds all of it in memory.
    let mut incremental = std::mem::take(&mut ledger.incremental);
    let mut on_batch = |messages| ledger.observe(messages);
    let read = read_incremental_transcript_messages(
        &path,
        &mut incremental,
        session_chat_line_decoder(agent),
        Some(&mut on_batch),
        None,
        None,
        None,
    );
    if let Ok(messages) = read {
        ledger.observe(messages);
    }
    ledger.incremental = incremental;
    let queued: Vec<String> = crate::session_chat_queue::read_session_chat_queue_snapshot_with(
        db, project_id, session_id,
    )
    .queue
    .iter()
    .filter(|prompt| prompt.state != "failed")
    .filter_map(|prompt| answers_page_id(&prompt.text))
    .collect();
    let forked_at = read_forked_at(db, project_id, session_id);
    let open = ledger.open(&queued, forked_at, |id| {
        crate::visual_pages::page_answered(&state.paths.root_dir, id)
    });
    // The resource index: what the whole transcript holds, kept in its own table (survives a
    // restart, a compaction and a partly loaded chat).
    let read = ledger.index(&queued, forked_at, |id| {
        crate::visual_pages::page_answered(&state.paths.root_dir, id)
    });
    crate::session_chat_resources::store(
        state,
        db,
        project_id,
        session_id,
        &mut ledger.stored,
        read,
    );
    let newly: Vec<OpenAsker> = if notify_turn {
        open.iter()
            .filter(|asker| asker.latest_turn && ledger.notified.insert(asker.id.clone()))
            .cloned()
            .collect()
    } else {
        Vec::new()
    };
    Some((open, newly))
}

/// The forms and pages waiting on the user in this session, brought up to date first. Blocking.
pub(crate) fn open_askers(state: &AppState, project_id: &str, session_id: &str) -> Vec<OpenAsker> {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return Vec::new();
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
        return Vec::new();
    };
    read_open(state, &db, &session, false)
        .map(|(open, _)| open)
        .unwrap_or_default()
}

/// Reads the session's transcript once into its resource index, without counting or announcing
/// anything: the backfill of a thread that is not running. Blocking.
pub(crate) fn index_only(state: &AppState, project_id: &str, session_id: &str) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
        return;
    };
    // Without a transcript Ghostex can read there is nothing to index: write the empty index, so
    // the thread is not tried again every sweep.
    if read_open(state, &db, &session, false).is_none() {
        let mut stored = None;
        crate::session_chat_resources::store(
            state,
            &db,
            project_id,
            session_id,
            &mut stored,
            Vec::new(),
        );
    }
}

/// Brings the session's record up to date. `turn_end` is set when a turn just ended (`true` when
/// it ended idle, without attention), which may notify about the questions it asked. Blocking.
pub(crate) fn refresh(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    turn_end: Option<bool>,
) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
        return;
    };
    let is_thread = crate::coordinators::read_thread(&db, project_id, session_id)
        .ok()
        .flatten()
        .is_some_and(|thread| !thread.is_resolved());
    // A coordinator's stopped threads have no sweep of their own: build the index of any that has
    // none yet, so their pages are listed (`session_chat_resources::backfill_threads`).
    if crate::coordinators::read_coordinator(&db, project_id, session_id)
        .ok()
        .flatten()
        .is_some()
    {
        crate::session_chat_resources::backfill_threads(state, &db, project_id, session_id);
    }
    let (open, newly) = if is_thread {
        // A running thread's questions go to its coordinator, but its pages are indexed.
        let _ = read_open(state, &db, &session, false);
        (Vec::new(), Vec::new())
    } else {
        match read_open(state, &db, &session, turn_end.is_some()) {
            Some(found) => found,
            None => return,
        }
    };
    // For later questions still wait, but they are not counted and never notify. A mark whose
    // question was answered, withdrawn or asked again is forgotten.
    let (open, newly) = leave_out_removed(open, newly, &removed_ids(&session));
    let later = later_ids(&session);
    let (open, newly, still) = leave_out_later(open, newly, &later);
    if !is_thread && still.len() != later.len() {
        let _ = write_later(state, &db, project_id, session_id, &still);
    }
    let ids: Vec<&str> = open.iter().map(|asker| asker.id.as_str()).collect();
    let count: usize = open.iter().map(OpenAsker::count).sum();
    let recorded = session.pointer(&format!("/runtimeSettings/{RUNTIME_KEY}"));
    let next = (!open.is_empty()).then(|| json!({ "ids": ids, "count": count }));
    if recorded != next.as_ref()
        && write_record(state, &db, project_id, session_id, next.as_ref()).is_err()
    {
        return;
    }
    if !newly.is_empty() {
        notify(
            state,
            &db,
            &repository,
            project_id,
            session_id,
            &newly,
            turn_end == Some(true),
        );
    }
}

/// Drops the questions the user removed without answering: they are not counted and never notify.
fn leave_out_removed(
    open: Vec<OpenAsker>,
    newly: Vec<OpenAsker>,
    removed: &[String],
) -> (Vec<OpenAsker>, Vec<OpenAsker>) {
    let kept = |asker: &OpenAsker| !removed.contains(&asker.id);
    (
        open.into_iter().filter(kept).collect(),
        newly.into_iter().filter(kept).collect(),
    )
}

/// Splits the For later questions off: what is counted, what may notify, and the marks still worth
/// keeping (those whose question still waits).
fn leave_out_later(
    open: Vec<OpenAsker>,
    newly: Vec<OpenAsker>,
    later: &[String],
) -> (Vec<OpenAsker>, Vec<OpenAsker>, Vec<String>) {
    let still: Vec<String> = later
        .iter()
        .filter(|id| open.iter().any(|asker| asker.id == **id))
        .cloned()
        .collect();
    let counted = |asker: &OpenAsker| !later.contains(&asker.id);
    (
        open.into_iter().filter(counted).collect(),
        newly.into_iter().filter(counted).collect(),
        still,
    )
}

/// Called from every place an agent's activity changes, beside the notification feed: a turn
/// that starts or ends brings the record up to date on a background thread, and a turn that ends
/// may notify about the questions it asked.
pub(crate) fn observe_activity(state: &AppState, params: &Map<String, Value>, result: &Value) {
    let Some(session) = result.get("session") else {
        return;
    };
    let previous = result
        .get("previousActivity")
        .and_then(Value::as_str)
        .unwrap_or("idle");
    let next = result
        .pointer("/activity/activity")
        .and_then(Value::as_str)
        .unwrap_or("idle");
    let id = |key: &str| {
        session
            .get(key)
            .and_then(Value::as_str)
            .or_else(|| params.get(key).and_then(Value::as_str))
            .map(str::to_string)
    };
    let (Some(project_id), Some(session_id)) = (id("projectId"), id("sessionId")) else {
        return;
    };
    let entered_turn_end_attention = result.get("enteredAttention").and_then(Value::as_bool)
        == Some(true)
        && result
            .pointer("/activity/attentionSource")
            .and_then(Value::as_str)
            == Some(TURN_COMPLETE_ATTENTION_SOURCE);
    let ended_idle = previous == "working" && next == "idle";
    // A turn start brings the answers it was started with.
    let started = next == "working" && previous != "working";
    let turn_end = if entered_turn_end_attention || ended_idle {
        Some(ended_idle)
    } else if started {
        None
    } else {
        return;
    };
    let state = state.clone();
    std::thread::spawn(move || {
        std::thread::sleep(SETTLE_DELAY);
        refresh(&state, &project_id, &session_id, turn_end);
    });
}

/// Brings every running session's record up to date every few seconds, without notifying: a
/// session whose transcript did not change costs a file `stat`.
pub(crate) fn spawn_open_questions_task(state: &Arc<AppState>) -> tokio::task::JoinHandle<()> {
    let state = state.clone();
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        loop {
            let sweep_state = state.clone();
            let _ = tokio::task::spawn_blocking(move || sweep(&sweep_state)).await;
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = tokio::time::sleep(SWEEP_INTERVAL) => {}
            }
        }
    })
}

fn sweep(state: &AppState) {
    let sessions = {
        let Ok(db) = open_gxserver_database(&state.paths) else {
            return;
        };
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        repository
            .list_sessions_with_lifecycle_state("running")
            .unwrap_or_default()
    };
    let mut active = HashSet::new();
    for session in sessions {
        let (Some(project_id), Some(session_id)) = (
            session.get("projectId").and_then(Value::as_str),
            session.get("sessionId").and_then(Value::as_str),
        ) else {
            continue;
        };
        active.insert(crate::server::session_observer_key(project_id, session_id));
        refresh(state, project_id, session_id, None);
    }
    if let Ok(mut ledgers) = ledgers().lock() {
        ledgers.retain(|key, _| active.contains(key));
    }
}

/// Records (or, with `None`, clears) the session's open questions and tells every client.
fn write_record(
    state: &AppState,
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
    record: Option<&Value>,
) -> anyhow::Result<()> {
    let changed = match record {
        Some(record) => db.execute(
            r#"UPDATE sessions SET runtimeSettingsJson = json_set(runtimeSettingsJson, '$.sessionChatOpenQuestions', json(?3))
            WHERE projectId = ?1 AND sessionId = ?2"#,
            rusqlite::params![project_id, session_id, record.to_string()],
        )?,
        None => db.execute(
            r#"UPDATE sessions SET runtimeSettingsJson = json_remove(runtimeSettingsJson, '$.sessionChatOpenQuestions')
            WHERE projectId = ?1 AND sessionId = ?2
            AND json_extract(runtimeSettingsJson, '$.sessionChatOpenQuestions') IS NOT NULL"#,
            rusqlite::params![project_id, session_id],
        )?,
    };
    if changed > 0 {
        let repository = DomainRepository::new(db, state.metadata.server_id.as_str());
        crate::server::schedule_presentation_session_delta(
            state,
            db,
            &repository,
            project_id,
            session_id,
        )?;
    }
    Ok(())
}

fn notification_body(newly: &[OpenAsker]) -> String {
    let count: usize = newly.iter().map(OpenAsker::count).sum();
    let waiting = match count {
        1 => "1 question is waiting for your answer".to_string(),
        count => format!("{count} questions are waiting for your answers"),
    };
    let first_title = newly
        .iter()
        .find_map(|asker| asker.form.as_ref()?.questions.first())
        .map(|question| question.title.trim().to_string())
        .filter(|title| !title.is_empty());
    match first_title {
        Some(title) => format!("{waiting}: {title}"),
        None => waiting,
    }
}

fn notify(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    newly: &[OpenAsker],
    raise_attention: bool,
) {
    let body =
        crate::notification_feed::store::bounded_notification_body(&notification_body(newly));
    if raise_attention {
        // The turn ended without attention, so nothing sounded yet: raise it once, the way any
        // other wait for input is raised.
        let mut params = Map::new();
        params.insert("projectId".to_string(), json!(project_id));
        params.insert("sessionId".to_string(), json!(session_id));
        params.insert("activity".to_string(), json!("attention"));
        let endpoint = "/api/updateAgentActivity";
        let Ok(output) = crate::agents::dispatch_agent_endpoint(
            repository,
            db,
            &state.paths.home_dir,
            endpoint,
            &params,
            None,
        ) else {
            return;
        };
        crate::server::log_agent_activity_transition(state, endpoint, &params, &output.result);
        crate::notification_feed::observe_agent_activity_result(
            state,
            endpoint,
            &params,
            &output.result,
        );
        let _ = crate::server::schedule_presentation_session_delta(
            state, db, repository, project_id, session_id,
        );
    }
    match crate::notification_feed::store::mark_unread_session_notification_needs_input(
        db, session_id, &body,
    ) {
        Ok(changed) if changed > 0 => {
            crate::notification_feed::broadcast_notification_feed_changed(state)
        }
        Ok(_) => {}
        Err(error) => {
            let _ = state.logger.log(GxserverLogInput {
                level: LogLevel::Warn,
                event: "notificationFeed.openQuestionsFailed".to_string(),
                server_id: Some(state.metadata.server_id.clone()),
                request_id: None,
                client: None,
                duration_ms: None,
                error: Some(error.message),
                details: Some(json!({ "sessionId": session_id })),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asker(id: &str, waiting: usize) -> OpenAsker {
        OpenAsker {
            id: id.to_string(),
            kind: AskerKind::Page,
            form: None,
            waiting,
            latest_turn: true,
        }
    }

    #[test]
    fn for_later_questions_are_not_counted_and_never_notify() {
        let open = vec![asker("a", 2), asker("b", 3)];
        let newly = vec![asker("b", 3)];
        let later = vec!["b".to_string(), "answered-since".to_string()];
        let (counted, notify, still) = leave_out_later(open, newly, &later);
        assert_eq!(counted.iter().map(OpenAsker::count).sum::<usize>(), 2);
        assert!(notify.is_empty());
        assert_eq!(
            still,
            vec!["b".to_string()],
            "a mark whose question settled is forgotten"
        );
    }

    #[test]
    fn removed_questions_are_not_counted_and_never_notify() {
        let open = vec![asker("a", 2), asker("b", 3)];
        let newly = vec![asker("b", 3)];
        let (counted, notify) = leave_out_removed(open, newly, &["b".to_string()]);
        assert_eq!(counted.iter().map(OpenAsker::count).sum::<usize>(), 2);
        assert!(notify.is_empty());
    }

    #[test]
    fn the_removed_list_reads_from_the_session_record() {
        let session = json!({"runtimeSettings": {REMOVED_KEY: ["p1", 3, "f2"], LATER_KEY: ["x"]}});
        assert_eq!(
            removed_ids(&session),
            vec!["p1".to_string(), "f2".to_string()]
        );
        assert!(removed_ids(&json!({})).is_empty());
    }

    #[test]
    fn the_later_list_reads_from_the_session_record() {
        let session = json!({"runtimeSettings": {LATER_KEY: ["p1", 3, "f2"]}});
        assert_eq!(
            later_ids(&session),
            vec!["p1".to_string(), "f2".to_string()]
        );
        assert!(later_ids(&json!({})).is_empty());
    }

    fn message(id: &str, role: &str, at: i64, text: &str) -> SessionChatMessage {
        serde_json::from_value(json!({
            "id": id, "role": role, "source": "transcript", "timestamp": at,
            "blocks": [{"type": "text", "text": text}],
        }))
        .expect("message")
    }

    fn form(tag: &str, title: &str) -> String {
        format!(
            "```questions\n{{\"title\":\"{title}\",\"questions\":[{{\"tag\":\"{tag}\",\"title\":\"{title}?\",\"choices\":[\"A\",\"B\"]}}]}}\n```"
        )
    }

    /// The ledger reads the whole transcript once, in batches (the backfill), and the index it
    /// gives covers every form, page and chart with the status the answers so far decide, however
    /// far back they are.
    #[test]
    fn the_ledger_indexes_every_resource_in_the_transcript() {
        let page = "```visual\n{\"page\":{\"title\":\"Notes\",\"url\":\"http://127.0.0.1:1/visual/0123456789abcdef0123456789abcdef\"}}\n```";
        let chart = "```visual\n{\"title\":\"Runs\",\"rows\":[{\"stats\":[{\"label\":\"Passed\",\"value\":3}]}]}\n```";
        let mut ledger = Ledger::default();
        let mut first = Vec::new();
        for n in 0..250 {
            first.push(message(
                &format!("a{n}"),
                "assistant",
                n,
                &form(&format!("Q{n}"), &format!("Title {n}")),
            ));
        }
        ledger.observe(first);
        ledger.observe(vec![
            message("p", "assistant", 300, page),
            message("c", "assistant", 301, chart),
            message("plain", "assistant", 302, "No fences here."),
        ]);
        let index = ledger.index(&[], None, |_| false);
        assert_eq!(
            index.len(),
            252,
            "250 forms, a page and a chart; the plain reply adds none"
        );
        assert!(index
            .iter()
            .filter(|e| e.kind == "form")
            .all(|e| e.status == "waiting"));
        let page_entry = index.iter().find(|e| e.kind == "page").unwrap();
        assert_eq!(
            (page_entry.title.as_str(), page_entry.status.as_str()),
            ("Notes", "other")
        );
        assert!(page_entry
            .url
            .as_deref()
            .is_some_and(|url| url.contains("/visual/")));
        assert_eq!(
            index
                .iter()
                .find(|e| e.kind == "chart")
                .map(|e| e.title.as_str()),
            Some("Runs")
        );
        // A queued answer settles a form; a fork leaves out what it inherited from its parent.
        let first_form = index.iter().find(|e| e.tags == ["Q0"]).unwrap().id.clone();
        let settled = ledger.index(&[first_form.clone()], None, |_| false);
        assert_eq!(
            settled.iter().find(|e| e.id == first_form).unwrap().status,
            "answered"
        );
        let forked = ledger.index(&[], Some(100), |_| false);
        assert!(forked
            .iter()
            .all(|e| e.kind != "form" || e.at_ms >= Some(100)));
    }
}
