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
//! CDXC:SessionStatus 2026-11-05 DECISION:
//! User (Q1285, "Yes, typed answers count"): a question the user answers in plain words in the chat ("Q12: yes", "Q13 = B", "Q14 skip"), after it was asked, on a line that is not a quote, in a message that is not a thread's report, is answered: it leaves the count, the pink dot and `answer requested (n)`, the chat's ? pill and the coordinator board's Needs you. Supersedes, in part, the 2026-10-20 rule above that only a form's, a page's or queued answers, a withdrawal or the same tags asked again clear a question; a message that only names a tag ("Q12 what does it mean?") still clears nothing. The reading is gx-chat-core's (`transcript/open_questions.rs` `said_in`, `Found::Said`), so every client agrees.
//! CDXC:Notifications 2026-10-20 DECISION:
//! Same request: one notification per turn that asked. When a turn ends with new open questions from that turn, its Finished feed row turns into Needs input, saying how many it asked; a turn that ended without attention (agents whose finished turn is idle) is raised to attention once, which sounds, shows the system notification and writes the feed row like any other wait for input. Questions from earlier turns never notify again.
//! SEE-ALSO: packages/gx-chat-core/src/transcript/open_questions.rs (what counts as asked, answered and withdrawn), server/src/presentation/session_projection.rs (`pendingQuestionCount`), server/src/voice_agent/question_forms.rs (the voice agent lists and answers these), server/src/visual_pages/questions.rs.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use ghostex_gx_chat_core::protocol::ChatMessage;
use ghostex_gx_chat_core::questions::resource_index::{self, ResourceEntry};
use ghostex_gx_chat_core::transcript::open_questions::{
    asker_entries, askers, linkable_tags, mark_inherited, settle_askers, share_answers,
    tags_answered_in_words, AskerKind, Found, TagState,
};
use ghostex_gx_chat_core::transcript::page_answers::answers_page_id;
use ghostex_gx_chat_core::transcript::question_forms::{question_forms, QuestionForm};
use rusqlite::OptionalExtension;
use serde_json::{json, Map, Value};

use crate::domain::DomainRepository;
use crate::logging::{GxserverLogInput, LogLevel};
use crate::server::{read_runtime_text, AppState};
use crate::session_chat::{
    boundary_fingerprint, read_incremental_transcript_messages, read_transcript_file_version,
    resolve_session_chat_transcript_agent, resolve_session_chat_transcript_path,
    session_chat_line_decoder, SessionChatBlock, SessionChatIncrementalState, SessionChatMessage,
    SessionChatRole, SessionChatTranscriptAgent, TranscriptFileVersion,
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
    /// Where the copy kept across restarts stops, and when it was written ([`Ledger::save`]).
    saved_offset: u64,
    saved_at: Option<Instant>,
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
                    let answers_id = answers_page_id(&text);
                    if let Some(id) = answers_id.clone() {
                        if let Some(at) = message.byte_offset {
                            self.answers_at.entry(id.clone()).or_insert(at);
                        }
                        self.answered.push(id);
                    }
                    // Answers in the user's own words ("Q12: yes"), and the tags an answers
                    // envelope names ("Q1277": …), settle questions asked before them whatever id
                    // they have now (gx-chat-core's `said_in`, which leaves out a thread's report).
                    if answers_id.is_some() || !tags_answered_in_words(&text).is_empty() {
                        let converted = serde_json::to_value(&message)
                            .ok()
                            .and_then(|value| serde_json::from_value::<ChatMessage>(value).ok());
                        if let Some(converted) = converted {
                            let found =
                                asker_entries(std::slice::from_ref(&converted), &self.found);
                            if !found.is_empty() {
                                self.found.extend(found);
                                self.asking.push((converted, self.turns));
                            }
                        }
                    }
                }
                SessionChatRole::Assistant => {
                    let text = text_of(&message);
                    // One-click answer links ask too (gx-chat-core's transcript/inline_answers.rs).
                    let inline = text.contains("ghostex-answer:");
                    if !(inline || text.contains("```") || text.contains("~~~")) {
                        continue;
                    }
                    // Only a fenced block naming questions can ask or withdraw any; one naming
                    // a drawing or a page is a resource too (the index lists them all).
                    let asks = inline || text.contains("questions");
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
        for asker in &mut found {
            if asker.kind == AskerKind::Page && !asker.answered && page_answered(&asker.id) {
                asker.answered = true;
            }
        }
        // A tag answered on one copy is answered on every copy (gx-chat-core's `share_answers`).
        share_answers(&mut found);
        found
            .into_iter()
            .filter(|asker| !asker.answered && !asker.obsolete && !asker.inherited)
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
    /// Every tag the replies asked, uppercased, and whether its question is settled: answered (in
    /// the form, by a one-click link, from a page, by answers waiting in the queue, or in the
    /// user's own words after it was asked, gx-chat-core's `Found::Said`), skipped, withdrawn, or
    /// asked again and settled there.
    /// The board's Needs you items that name a tag read it (coordinators/board.rs
    /// `settle_answered_items`).
    pub(crate) fn tag_states(
        &self,
        also_answered: &[String],
        page_answered: impl Fn(&str) -> bool,
    ) -> HashMap<String, bool> {
        let mut answered = self.answered.clone();
        answered.extend(also_answered.iter().cloned());
        let mut settled = settle_askers(&self.found, &answered);
        for asker in &mut settled {
            if asker.kind == AskerKind::Page && !asker.answered && page_answered(&asker.id) {
                asker.answered = true;
            }
        }
        share_answers(&mut settled);
        linkable_tags(&settled)
            .into_iter()
            .map(|tag| (tag.tag, tag.state != TagState::Open))
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
    catch_up(
        &mut ledger,
        &path,
        &version,
        agent,
        Some(&saved_ledgers_dir(state)),
    );
    let queued = queued_answers(db, project_id, session_id);
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

/// Brings `ledger` up to the end of the transcript at `path`: from where it stopped, else from the
/// copy kept in `saved_dir` when that copy still describes this file, else from the start.
fn catch_up(
    ledger: &mut Ledger,
    path: &Path,
    version: &TranscriptFileVersion,
    agent: SessionChatTranscriptAgent,
    saved_dir: Option<&Path>,
) {
    if ledger.path != path
        || ledger.identity != version.identity
        || version.size < ledger.incremental.offset
    {
        let notified = std::mem::take(&mut ledger.notified);
        *ledger = saved_dir
            .and_then(|dir| Ledger::restore(dir, path, version))
            .unwrap_or_else(|| Ledger {
                path: path.to_path_buf(),
                identity: version.identity.clone(),
                ..Default::default()
            });
        ledger.notified.extend(notified);
    }
    // Batches, so a first read of a long conversation never holds all of it in memory.
    let mut incremental = std::mem::take(&mut ledger.incremental);
    let mut on_batch = |messages| ledger.observe(messages);
    let read = read_incremental_transcript_messages(
        path,
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
    if let Some(dir) = saved_dir {
        ledger.save(dir);
    }
}

/*
CDXC:SessionStatus 2026-11-07 WHY:
The ledger lived in memory only, so every gxserver start (each rebuild, update
or crash) read every running session's transcript from its first byte again,
decoding every row: about 930 MB for the main coordinator alone, on the sweep
that runs right after start (backlog-1107). The ledger is now also kept in
gxserver's state folder, one file per transcript, and a start resumes from it
when it is the same file and has only grown since: same path and file identity,
no shorter than where the copy stops, and the same bytes just before that point
(`boundary_fingerprint`, as session_chat_async_questions.rs checks its cursor).
Anything else reads from the start, as before. The copy holds what the ledger
read (the replies that ask, the answers, the user's message count, the
resource entries, the turns already notified) and not what it derives: `found`
is replayed from the kept replies. It is written at most once a minute while a
transcript grows, so a restart re-reads at most about a minute of rows, and
dropped when unused for two weeks. A copy from another build is not used,
because what the reading keeps may have changed with it; a change to what
`Ledger::observe` keeps within one build bumps `SAVED_LEDGER_FORMAT`.
*/
// 2: `observe` keeps the answers envelopes that name tags (gx-chat-core's `said_in`), so a copy
// written before them is read again (CDXC:SessionStatus 2026-11-08 in
// packages/gx-chat-core/src/transcript/open_questions.rs `tags_answered_in_envelope`).
// 3: the resource entries list one-click questions (gx-chat-core's `resource_index::scan_message`,
// CDXC:SessionStatus 2026-11-08), so a copy whose entries lack them is read again.
const SAVED_LEDGER_FORMAT: u32 = 3;
const SAVED_LEDGERS_DIR: &str = "open-questions";
const SAVED_LEDGER_EVERY: Duration = Duration::from_secs(60);
const SAVED_LEDGER_UNUSED_FOR: Duration = Duration::from_secs(14 * 24 * 60 * 60);

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedLedger {
    format: u32,
    build: String,
    path: PathBuf,
    identity: String,
    /// Where the next unread line starts.
    offset: u64,
    /// The bytes just before `offset`, to tell an append from a rewrite.
    boundary: String,
    last_message_timestamp: Option<i64>,
    asking: Vec<(ChatMessage, usize)>,
    answered: Vec<String>,
    answers_at: HashMap<String, u64>,
    turns: usize,
    notified: Vec<String>,
    entries: Vec<ResourceEntry>,
}

fn saved_ledgers_dir(state: &AppState) -> PathBuf {
    state.paths.root_dir.join(SAVED_LEDGERS_DIR)
}

fn saved_ledger_file(dir: &Path, transcript: &Path) -> PathBuf {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(transcript.to_string_lossy().as_bytes());
    dir.join(format!("{}.json", hex::encode(&digest[..16])))
}

fn saved_ledger_build() -> &'static str {
    env!("GHOSTEX_BUILD_MARKETING_VERSION")
}

impl Ledger {
    /// Writes the copy kept across restarts when the ledger read further than the copy, at most
    /// once every [`SAVED_LEDGER_EVERY`].
    fn save(&mut self, dir: &Path) {
        let offset = self.incremental.line_start();
        if offset <= self.saved_offset
            || self
                .saved_at
                .is_some_and(|at| at.elapsed() < SAVED_LEDGER_EVERY)
        {
            return;
        }
        let Ok(boundary) = boundary_fingerprint(&self.path, offset) else {
            return;
        };
        let saved = SavedLedger {
            format: SAVED_LEDGER_FORMAT,
            build: saved_ledger_build().to_string(),
            path: self.path.clone(),
            identity: self.identity.clone(),
            offset,
            boundary,
            last_message_timestamp: self.incremental.last_message_timestamp(),
            asking: self.asking.clone(),
            answered: self.answered.clone(),
            answers_at: self.answers_at.clone(),
            turns: self.turns,
            notified: self.notified.iter().cloned().collect(),
            entries: self.entries.clone(),
        };
        let Ok(bytes) = serde_json::to_vec(&saved) else {
            return;
        };
        let file = saved_ledger_file(dir, &self.path);
        // Written beside it and renamed over it, so a crash mid-write never leaves half a copy.
        let partial = file.with_extension("json.partial");
        let written = std::fs::create_dir_all(dir)
            .and_then(|_| std::fs::write(&partial, &bytes))
            .and_then(|_| std::fs::rename(&partial, &file));
        if written.is_ok() {
            self.saved_offset = offset;
            self.saved_at = Some(Instant::now());
        }
        forget_unused_saved_ledgers(dir);
    }

    /// The ledger kept for `path`, when it was saved by this build for this same file and the
    /// file has only grown since.
    fn restore(dir: &Path, path: &Path, version: &TranscriptFileVersion) -> Option<Ledger> {
        let bytes = std::fs::read(saved_ledger_file(dir, path)).ok()?;
        let saved: SavedLedger = serde_json::from_slice(&bytes).ok()?;
        if saved.format != SAVED_LEDGER_FORMAT
            || saved.build != saved_ledger_build()
            || saved.path != path
            || saved.identity != version.identity
            || saved.offset > version.size
            || boundary_fingerprint(path, saved.offset).ok()? != saved.boundary
        {
            return None;
        }
        let mut incremental = SessionChatIncrementalState::new();
        incremental.rebase(saved.offset);
        incremental.seed_last_message_timestamp(saved.last_message_timestamp);
        // `found` is what `observe` read off the kept replies, in order.
        let mut found = Vec::new();
        for (message, _) in &saved.asking {
            let more = asker_entries(std::slice::from_ref(message), &found);
            found.extend(more);
        }
        Some(Ledger {
            path: saved.path,
            identity: saved.identity,
            incremental,
            asking: saved.asking,
            answered: saved.answered,
            answers_at: saved.answers_at,
            turns: saved.turns,
            notified: saved.notified.into_iter().collect(),
            entries: saved.entries,
            found,
            stored: None,
            saved_offset: saved.offset,
            saved_at: Some(Instant::now()),
        })
    }
}

/// Drops the kept ledgers no session used for [`SAVED_LEDGER_UNUSED_FOR`], once per gxserver run.
fn forget_unused_saved_ledgers(dir: &Path) {
    static DONE: OnceLock<()> = OnceLock::new();
    if DONE.set(()).is_err() {
        return;
    }
    let Ok(files) = std::fs::read_dir(dir) else {
        return;
    };
    for file in files.flatten() {
        let unused = file
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age > SAVED_LEDGER_UNUSED_FOR);
        if unused {
            let _ = std::fs::remove_file(file.path());
        }
    }
}

/// The forms and pages whose answers wait in the session's queue (failed ones left out).
fn queued_answers(db: &rusqlite::Connection, project_id: &str, session_id: &str) -> Vec<String> {
    crate::session_chat_queue::read_session_chat_queue_snapshot_with(db, project_id, session_id)
        .queue
        .iter()
        .filter(|prompt| prompt.state != "failed")
        .filter_map(|prompt| answers_page_id(&prompt.text))
        .collect()
}

/// Marks done the coordinator's Needs you items whose questions its chat has settled, from the
/// ledger `read_open` just brought up to date (coordinators/board.rs `settle_answered_items`). The
/// supervisor's next tick shows the change on the board.
fn settle_board_items(
    state: &AppState,
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
) {
    let key = (project_id.to_string(), session_id.to_string());
    if !crate::coordinators::has_question_items(db, &key) {
        return;
    }
    let ledger_key = crate::server::session_observer_key(project_id, session_id);
    let Some(entry) = ledgers()
        .lock()
        .ok()
        .and_then(|ledgers| ledgers.get(&ledger_key).cloned())
    else {
        return;
    };
    let queued = queued_answers(db, project_id, session_id);
    let states = match entry.lock() {
        Ok(ledger) => ledger.tag_states(&queued, |id| {
            crate::visual_pages::page_answered(&state.paths.root_dir, id)
        }),
        Err(_) => return,
    };
    let _ = crate::coordinators::settle_answered_items(db, &key, |tag| states.get(tag).copied());
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
    let is_coordinator = crate::coordinators::read_coordinator(&db, project_id, session_id)
        .ok()
        .flatten()
        .is_some();
    if is_coordinator {
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
    if is_coordinator {
        settle_board_items(state, &db, project_id, session_id);
    }
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

    /// A tag reads settled once its question is answered by a one-click link, a form, a page or the
    /// user's own words after it was asked, or withdrawn; a thread's report naming it, or words
    /// sent before it was asked, settle nothing. The board's Needs you items read this.
    #[test]
    fn questions_settle_by_link_form_withdrawal_and_plain_words() {
        use ghostex_gx_chat_core::transcript::inline_answers::inline_questions;
        use ghostex_gx_chat_core::transcript::question_forms::{answers_message, parse_form};
        let inline = "- Reformat the drive? [Yes](ghostex-answer:Q1) [No](ghostex-answer:Q1)";
        let fenced = form("Q2", "Collect audios");
        let mut ledger = Ledger::default();
        ledger.observe(vec![
            message("u0", "user", 0, "Q5: yes, before it was asked"),
            message("a1", "assistant", 1, inline),
            message("a2", "assistant", 2, &fenced),
            message("a3", "assistant", 3, &form("Q3", "Export prompts")),
            message("a4", "assistant", 4, &form("Q4", "Rename")),
            message("a5", "assistant", 5, &form("Q5", "Push")),
        ]);
        let states = ledger.tag_states(&[], |_| false);
        for tag in ["Q1", "Q2", "Q3", "Q4", "Q5"] {
            assert_eq!(states.get(tag), Some(&false), "{tag} waits");
        }
        assert_eq!(states.get("Q9"), None, "never asked");
        let link_form = inline_questions(inline)[0].form();
        let body = fenced
            .trim_start_matches("```questions\n")
            .trim_end_matches("\n```");
        let fenced_form = parse_form(body);
        ledger.observe(vec![
            // One click on an inline answer link.
            message("u1", "user", 6, &answers_message(&link_form, &json!({"Q1": "Yes"}))),
            // The form's Send.
            message("u2", "user", 7, &answers_message(&fenced_form, &json!({"Q2": "A"}))),
            // The agent withdraws Q3.
            message("a6", "assistant", 8, "```questions\n{\"withdraw\":[\"Q3\"]}\n```"),
            // A thread's report quoting Q5 is not the user's answer.
            message(
                "u3",
                "user",
                9,
                "Q5: B\n\nMessage from another agent\nAgent: Claude\nSession: Thread\nSession ID: G1\nAgent ID: claude\nAgent Session ID: x\nReply to: S1:P1:G1",
            ),
        ]);
        let states = ledger.tag_states(&[], |_| false);
        assert_eq!(states.get("Q1"), Some(&true), "inline link");
        assert_eq!(states.get("Q2"), Some(&true), "form");
        assert_eq!(states.get("Q3"), Some(&true), "withdrawn");
        assert_eq!(states.get("Q4"), Some(&false));
        assert_eq!(
            states.get("Q5"),
            Some(&false),
            "words before it was asked, a report"
        );
        // Q4 and Q5 still count toward the sidebar's question dot.
        let open = |ledger: &Ledger| -> usize {
            ledger
                .open(&[], None, |_| false)
                .iter()
                .map(OpenAsker::count)
                .sum()
        };
        assert_eq!(open(&ledger), 2);
        // The user answers in plain words.
        ledger.observe(vec![message("u4", "user", 10, "Q4: B\nQ5 yes")]);
        let states = ledger.tag_states(&[], |_| false);
        assert_eq!(states.get("Q4"), Some(&true), "plain chat");
        assert_eq!(states.get("Q5"), Some(&true), "plain chat");
        assert_eq!(open(&ledger), 0, "the dot clears too");
        let index = ledger.index(&[], None, |_| false);
        assert!(
            index.iter().all(|entry| entry.status != "waiting"),
            "Resources lists them answered"
        );
    }

    /// A one-click question asked in two replies and answered on the first copy (Q1652): the
    /// sidebar's count and the board's tag state read it answered, before and after the second ask.
    #[test]
    fn a_tag_answered_on_an_earlier_copy_settles_every_copy() {
        use ghostex_gx_chat_core::transcript::inline_answers::inline_questions;
        use ghostex_gx_chat_core::transcript::question_forms::answers_message;
        let first = "- Q1652: Include an iOS build?\n  - First I'd apply the crash fix.\n  - [Yes, build iOS too](ghostex-answer:Q1652) [No, Android only](ghostex-answer:Q1652)";
        let again = "- Q1652: Include an iOS build? It brings widgets. [Yes, build iOS too](ghostex-answer:Q1652) [No, Android only](ghostex-answer:Q1652)";
        let form = inline_questions(first)[0].form();
        let answer = answers_message(&form, &json!({"Q1652": "Yes, build iOS too"}));
        let mut ledger = Ledger::default();
        ledger.observe(vec![
            message("a1", "assistant", 1, first),
            message("u1", "user", 2, &answer),
            message("a2", "assistant", 3, again),
        ]);
        assert_eq!(ledger.tag_states(&[], |_| false).get("Q1652"), Some(&true));
        let open: usize = ledger
            .open(&[], None, |_| false)
            .iter()
            .map(OpenAsker::count)
            .sum();
        assert_eq!(open, 0);
    }

    fn claude_reply(id: &str, parent: &str, text: &str) -> String {
        json!({
            "type": "assistant", "uuid": id, "parentUuid": parent,
            "timestamp": "2026-11-07T10:00:01.000Z",
            "message": { "role": "assistant", "content": [{ "type": "text", "text": text }] },
        })
        .to_string()
    }

    fn claude_prompt(id: &str, parent: &str, text: &str) -> String {
        json!({
            "type": "user", "uuid": id, "parentUuid": parent,
            "timestamp": "2026-11-07T10:00:00.000Z",
            "message": { "role": "user", "content": text },
        })
        .to_string()
    }

    fn append_lines(path: &Path, lines: &[String]) {
        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .open(path)
            .unwrap()
            .write_all((lines.join("\n") + "\n").as_bytes())
            .unwrap();
    }

    /// Everything a ledger answers, for comparing two of them.
    fn readings(ledger: &Ledger) -> String {
        let mut open: Vec<_> = ledger
            .open(&[], None, |_| false)
            .into_iter()
            .map(|asker| (asker.id, asker.waiting, asker.latest_turn))
            .collect();
        open.sort();
        let mut tags: Vec<_> = ledger.tag_states(&[], |_| false).into_iter().collect();
        tags.sort();
        format!(
            "open {open:?}\ntags {tags:?}\nindex {:?}\nturns {} answered {:?} answers_at {:?} found {:?}",
            ledger.index(&[], None, |_| false),
            ledger.turns,
            ledger.answered,
            ledger.answers_at.iter().collect::<std::collections::BTreeMap<_, _>>(),
            ledger.found,
        )
    }

    fn caught_up(path: &Path, saved_dir: Option<&Path>) -> Ledger {
        let mut ledger = Ledger::default();
        let version = read_transcript_file_version(path).unwrap();
        catch_up(
            &mut ledger,
            path,
            &version,
            SessionChatTranscriptAgent::Claude,
            saved_dir,
        );
        ledger
    }

    fn restored(saved_dir: &Path, path: &Path) -> Option<Ledger> {
        Ledger::restore(
            saved_dir,
            path,
            &read_transcript_file_version(path).unwrap(),
        )
    }

    /// A gxserver start resumes a session's ledger from the copy the last run kept and reads only
    /// what the transcript grew by since, ending where a read from the start ends; a transcript
    /// rewritten, replaced or cut short since is read from the start.
    #[test]
    fn a_restart_resumes_the_ledger_from_its_kept_copy() {
        let dir = tempfile::tempdir().unwrap();
        let saved_dir = dir.path().join("saved");
        let path = dir.path().join("chat.jsonl");
        let first = [
            claude_prompt("u1", "root", "start"),
            claude_reply("a1", "u1", &form("Q1", "One")),
            claude_reply("a2", "a1", &form("Q2", "Two")),
            claude_prompt("u2", "a2", "Q1: A"),
            claude_reply(
                "a3",
                "u2",
                "```visual\n{\"title\":\"Runs\",\"rows\":[{\"stats\":[{\"label\":\"Passed\",\"value\":3}]}]}\n```",
            ),
        ];
        std::fs::write(&path, first.join("\n") + "\n").unwrap();
        let before = caught_up(&path, Some(&saved_dir));
        assert!(before.saved_at.is_some(), "the first read is kept");
        let kept = restored(&saved_dir, &path).expect("the kept copy reads back");
        assert_eq!(readings(&kept), readings(&before));

        // The chat grows while gxserver is down, with half a line at its end.
        append_lines(
            &path,
            &[
                claude_reply("a4", "a3", &form("Q3", "Three")),
                claude_prompt("u3", "a4", "Q2: B"),
            ],
        );
        let half = claude_reply("a5", "u3", &form("Q4", "Four"));
        {
            use std::io::Write;
            std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap()
                .write_all(&half.as_bytes()[..half.len() / 2])
                .unwrap();
        }
        let mut resumed = restored(&saved_dir, &path).expect("an append keeps the copy");
        let version = read_transcript_file_version(&path).unwrap();
        catch_up(
            &mut resumed,
            &path,
            &version,
            SessionChatTranscriptAgent::Claude,
            None,
        );
        assert_eq!(readings(&resumed), readings(&caught_up(&path, None)));
        assert_eq!(
            resumed.incremental.line_start(),
            caught_up(&path, None).incremental.line_start()
        );

        // Rewritten in place (same file, other bytes), replaced by another file, cut short.
        let other = [
            claude_prompt("v1", "root", "other"),
            claude_reply("b1", "v1", &form("Q9", "Nine")),
        ]
        .join("\n")
            + "\n";
        let mut rewritten = other.clone();
        while (rewritten.len() as u64) < version.size {
            rewritten.push_str(&other);
        }
        caught_up(&path, Some(&saved_dir));
        std::fs::write(&path, &rewritten).unwrap();
        assert!(restored(&saved_dir, &path).is_none(), "rewritten in place");
        caught_up(&path, Some(&saved_dir));
        let replacement = dir.path().join("replacement.jsonl");
        std::fs::write(&replacement, &rewritten).unwrap();
        std::fs::rename(&replacement, &path).unwrap();
        assert!(restored(&saved_dir, &path).is_none(), "replaced");
        let full = caught_up(&path, Some(&saved_dir));
        assert_eq!(readings(&full), readings(&caught_up(&path, None)));
        std::fs::write(&path, &other).unwrap();
        assert!(restored(&saved_dir, &path).is_none(), "cut short");
    }

    /// A copy is rewritten only once the ledger read further, and at most once a minute.
    #[test]
    fn the_kept_copy_is_written_at_most_once_a_minute() {
        let dir = tempfile::tempdir().unwrap();
        let saved_dir = dir.path().join("saved");
        let path = dir.path().join("chat.jsonl");
        std::fs::write(&path, claude_reply("a1", "u1", &form("Q1", "One")) + "\n").unwrap();
        let mut ledger = caught_up(&path, Some(&saved_dir));
        let first = ledger.saved_offset;
        append_lines(&path, &[claude_reply("a2", "a1", &form("Q2", "Two"))]);
        let version = read_transcript_file_version(&path).unwrap();
        catch_up(
            &mut ledger,
            &path,
            &version,
            SessionChatTranscriptAgent::Claude,
            Some(&saved_dir),
        );
        assert_eq!(ledger.saved_offset, first, "within the minute");
        ledger.saved_at = Some(Instant::now() - SAVED_LEDGER_EVERY);
        catch_up(
            &mut ledger,
            &path,
            &version,
            SessionChatTranscriptAgent::Claude,
            Some(&saved_dir),
        );
        assert_eq!(ledger.saved_offset, version.size, "a minute later");
    }

    /// A gxserver start on a long chat: the ledger read from the start (before) against the
    /// kept copy plus the rows appended since (after). Run on a disposable copy of a real
    /// transcript (one row is appended, then cut off again):
    /// `GX_OPEN_QUESTIONS_BENCH_TRANSCRIPT=<copy.jsonl> cargo test --release --lib
    /// a_restart_on_a_long_chat -- --ignored --nocapture`
    #[test]
    #[ignore = "benchmark: needs GX_OPEN_QUESTIONS_BENCH_TRANSCRIPT, a disposable copy of a long Claude transcript"]
    fn a_restart_on_a_long_chat() {
        let path = PathBuf::from(std::env::var("GX_OPEN_QUESTIONS_BENCH_TRANSCRIPT").unwrap());
        let original_size = std::fs::metadata(&path).unwrap().len();
        let dir = tempfile::tempdir().unwrap();
        let saved_dir = dir.path().join("saved");
        let started = Instant::now();
        let full = caught_up(&path, None);
        let full_read = started.elapsed();
        let started = Instant::now();
        caught_up(&path, Some(&saved_dir));
        let with_save = started.elapsed();
        let saved_bytes = std::fs::metadata(saved_ledger_file(&saved_dir, &path))
            .unwrap()
            .len();
        append_lines(
            &path,
            &[claude_reply("bench-a", "bench-p", &form("Q99999", "Bench"))],
        );
        let started = Instant::now();
        let resumed = caught_up(&path, Some(&saved_dir));
        let restart = started.elapsed();
        let full_after = caught_up(&path, None);
        assert_eq!(readings(&resumed), readings(&full_after));
        eprintln!(
            "{:.1} MB, {} kept replies: a start reading from the first byte {:.0} ms (and {:.0} ms with the copy written); a start from the kept copy ({:.1} MB) {:.0} ms",
            original_size as f64 / 1_048_576.0,
            full.asking.len(),
            full_read.as_secs_f64() * 1000.0,
            with_save.as_secs_f64() * 1000.0,
            saved_bytes as f64 / 1_048_576.0,
            restart.as_secs_f64() * 1000.0,
        );
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(original_size)
            .unwrap();
    }

    /// The coordinator's Q1277: a one-click question answered on 2026-11-05 under the id its
    /// question had before 804a8118c, which the sweep counted open again (the pink dot with no
    /// ? pill). The answers envelope's tag settles it, in the count and the board's tag states, also
    /// in a ledger kept across a restart.
    #[test]
    fn an_answer_sent_under_an_older_id_settles_the_tag_it_names() {
        let ask = "\u{2753} **Needs you:**\n- **Q1277:** Should the security audit merge into main now? I recommend having a separate thread review it first.\n  - The thread would catch up with main once more, then push the editor fork, the phone commits and the branch, in that order. After the merge, team owners run `ghostex team deploy`.\n  - [Review, then merge](ghostex-answer:Q1277 \"recommended\") [Merge it now](ghostex-answer:Q1277=merge) [Hold](ghostex-answer:Q1277=hold)";
        let answer = "\n\n<pasted_content id=\"da06\">\n<ghostex-page-answers page=\"e7a1b880ae7522f143561bbf278fbeae\" title=\"The thread would catch up with main once more, then push the editor fork, the phone commits and the branch, in that order. After the merge, team owners run ghostex team deploy.\">\n{\n  \"Q1277\": \"Review, then merge\"\n}\n</ghostex-page-answers>\n</pasted_content id=\"da06\">\n";
        let mut ledger = Ledger::default();
        ledger.observe(vec![
            message("a1", "assistant", 1, ask),
            message("u1", "user", 2, answer),
        ]);
        let open: usize = ledger
            .open(&[], None, |_| false)
            .iter()
            .map(OpenAsker::count)
            .sum();
        assert_eq!(open, 0);
        assert_eq!(ledger.tag_states(&[], |_| false).get("Q1277"), Some(&true));

        // What a restart replays from the kept copy (`Ledger::restore`).
        let mut found = Vec::new();
        for (message, _) in &ledger.asking {
            let more = asker_entries(std::slice::from_ref(message), &found);
            found.extend(more);
        }
        assert!(found
            .iter()
            .any(|entry| matches!(entry, Found::Said(tags) if tags == &["Q1277"])));
    }
}
