//! The indexer: one low-priority thread that keeps the chat search index up to date with every
//! agent session's transcript, reading only the bytes each file grew by.
//!
//! Every [`HOT_INTERVAL`] it checks the transcripts of running sessions and of sessions whose row
//! changed; every [`FULL_INTERVAL`] it checks every session's transcript (one `stat` each when
//! nothing changed) and forgets sessions that were deleted. The first full pass is the backfill,
//! newest sessions first, so recent chats are searchable within seconds of the start.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ghostex_gx_chat_core::protocol::{ChatMessage, ChatRole};
use ghostex_gx_chat_core::transcript::message_numbers::{is_own_prompt, shown_text};
use ghostex_gx_chat_core::transcript::noise::is_hidden_message;
use ghostex_gx_chat_core::transcript::prose::prose_markdown;
use rusqlite::{params, Connection};
use serde_json::Value;

use super::store::{self, TranscriptRow};
use crate::domain::DomainRepository;
use crate::server::{read_runtime_text, AppState};
use crate::session_chat::{
    boundary_fingerprint, read_incremental_transcript_messages, read_transcript_file_version,
    resolve_session_chat_transcript_agent, resolve_session_chat_transcript_path,
    session_chat_line_decoder, SessionChatBlock, SessionChatIncrementalState, SessionChatMessage,
    SessionChatTranscriptAgent,
};
use crate::session_chat_follower::session_chat_agent_for_session;
use crate::storage::open_gxserver_database;

const HOT_INTERVAL: Duration = Duration::from_secs(2);
const FULL_INTERVAL: Duration = Duration::from_secs(60);
/// A reply longer than this is indexed by its start only: long code dumps make the index large and
/// are rarely what someone searches for.
const MAX_MESSAGE_BYTES: usize = 32 * 1024;
/// Messages written per transaction.
const COMMIT_EVERY: usize = 400;
/// Session rows resolved per index commit (`Indexer::resolve_changed`).
const RESOLVE_COMMIT_EVERY: usize = 200;
/// Pauses that keep the backfill gentle on a busy computer, on top of the thread's background
/// priority: after every commit, and after each transcript of a full pass as long as reading it
/// took (at most [`MAX_FILE_PAUSE`]), so the backfill uses at most about half of one core.
const BATCH_PAUSE: Duration = Duration::from_millis(20);
const MAX_FILE_PAUSE: Duration = Duration::from_millis(500);

/// What the index knows about a session row, so an unchanged row is never read again.
struct KnownSession {
    updated_at: String,
    transcript: Option<(i64, PathBuf, SessionChatTranscriptAgent)>,
}

pub(super) struct Indexer {
    state: Arc<AppState>,
    db: Connection,
    sessions: HashMap<(String, String), KnownSession>,
    /// Open readers of the transcripts read this run, keyed by transcript id: they hold the line a
    /// transcript is still writing.
    readers: HashMap<i64, SessionChatIncrementalState>,
    /// Transcripts whose stored read position was checked against the file this run.
    verified: HashSet<i64>,
    last_full: Option<Instant>,
    shutdown: tokio::sync::broadcast::Receiver<()>,
}

impl Indexer {
    pub(super) fn new(state: Arc<AppState>) -> rusqlite::Result<Self> {
        let db = store::open(&store::database_file(&state.paths))?;
        let mut sessions = HashMap::new();
        {
            let mut statement = db.prepare(
                "SELECT s.projectId, s.sessionId, s.updatedAt, s.transcriptId, t.path, t.agent
                 FROM sessions s LEFT JOIN transcripts t ON t.id = s.transcriptId",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            })?;
            for row in rows.flatten() {
                let (project_id, session_id, updated_at, id, path, agent) = row;
                let transcript = match (id, path, agent) {
                    (Some(id), Some(path), Some(agent)) => {
                        resolve_session_chat_transcript_agent(Some(&agent))
                            .map(|agent| (id, PathBuf::from(path), agent))
                    }
                    _ => None,
                };
                sessions.insert(
                    (project_id, session_id),
                    KnownSession {
                        updated_at,
                        transcript,
                    },
                );
            }
        }
        Ok(Self {
            db,
            sessions,
            readers: HashMap::new(),
            verified: HashSet::new(),
            last_full: None,
            shutdown: state.shutdown_tx.subscribe(),
            state,
        })
    }

    /// Runs until gxserver shuts down.
    pub(super) fn run(mut self) {
        lower_thread_priority();
        while !self.shutting_down() {
            let full = self
                .last_full
                .is_none_or(|last| last.elapsed() >= FULL_INTERVAL);
            let started = Instant::now();
            self.sweep(full);
            if full {
                self.last_full = Some(started);
                super::note_full_sweep(started.elapsed());
            }
            std::thread::sleep(HOT_INTERVAL);
        }
    }

    fn shutting_down(&mut self) -> bool {
        !matches!(
            self.shutdown.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        )
    }

    fn sweep(&mut self, full: bool) {
        let Some(rows) = self.session_rows() else {
            return;
        };
        let seen: HashSet<(String, String)> = rows
            .iter()
            .map(|row| (row.project_id.clone(), row.session_id.clone()))
            .collect();
        if full {
            self.forget_deleted(&seen);
        }
        let Some(changed_keys) = self.resolve_changed(&rows) else {
            return;
        };
        // Newest first (the domain list is ordered by updatedAt), so recent chats are searchable
        // first during the backfill.
        let mut done = HashSet::new();
        let total = rows.len();
        let mut last_live = Instant::now();
        for (position, row) in rows.iter().enumerate() {
            if full && position % 50 == 0 {
                super::note_progress(position, total);
            }
            if full && last_live.elapsed() >= HOT_INTERVAL {
                // A backfill takes minutes: the running chats stay current meanwhile.
                if self.shutting_down() {
                    return;
                }
                self.catch_up_live(&rows);
                last_live = Instant::now();
            }
            let key = (row.project_id.clone(), row.session_id.clone());
            let changed = changed_keys.contains(&key);
            if !(full || changed || row.live) {
                continue;
            }
            let Some((id, path, agent)) = self
                .sessions
                .get(&key)
                .and_then(|known| known.transcript.clone())
            else {
                continue;
            };
            if !done.insert(id) {
                continue;
            }
            let started = Instant::now();
            if self.catch_up(id, &path, agent) && full {
                std::thread::sleep(started.elapsed().min(MAX_FILE_PAUSE));
            }
        }
        if full {
            super::note_progress(total, total);
        }
    }

    /// Reads what the running sessions' transcripts gained.
    fn catch_up_live(&mut self, rows: &[SessionRow]) {
        for row in rows.iter().filter(|row| row.live) {
            let key = (row.project_id.clone(), row.session_id.clone());
            if let Some((id, path, agent)) = self
                .sessions
                .get(&key)
                .and_then(|known| known.transcript.clone())
            {
                self.catch_up(id, &path, agent);
            }
        }
    }

    /// Every agent session row, newest first, with only the columns the sweep compares.
    fn session_rows(&self) -> Option<Vec<SessionRow>> {
        let db = open_gxserver_database(&self.state.paths).ok()?;
        let mut statement = db
            .prepare(
                "SELECT projectId, sessionId, updatedAt, lifecycleState FROM sessions
                 WHERE kind = 'agent' ORDER BY updatedAt DESC",
            )
            .ok()?;
        let rows = statement
            .query_map([], |row| {
                let lifecycle: String = row.get(3)?;
                Ok(SessionRow {
                    project_id: row.get(0)?,
                    session_id: row.get(1)?,
                    updated_at: row.get(2)?,
                    live: matches!(lifecycle.as_str(), "running" | "sleeping"),
                })
            })
            .ok()?
            .flatten()
            .collect();
        Some(rows)
    }

    /// Resolves every row that is new or changed since the last sweep (`resolve`) and returns their
    /// keys for the sweep to read. One gxserver connection serves them all, and the index writes
    /// are committed [`RESOLVE_COMMIT_EVERY`] at a time: after a restart every row is new, and a
    /// connection and a commit per row made that first pass crawl. `None` when gxserver is
    /// shutting down.
    fn resolve_changed(&mut self, rows: &[SessionRow]) -> Option<HashSet<(String, String)>> {
        let changed: Vec<&SessionRow> = rows
            .iter()
            .filter(|row| {
                self.sessions
                    .get(&(row.project_id.clone(), row.session_id.clone()))
                    .is_none_or(|known| known.updated_at != row.updated_at)
            })
            .collect();
        let mut keys = HashSet::new();
        if changed.is_empty() {
            return Some(keys);
        }
        let Ok(domain) = open_gxserver_database(&self.state.paths) else {
            return Some(keys);
        };
        for batch in changed.chunks(RESOLVE_COMMIT_EVERY) {
            if self.shutting_down() {
                return None;
            }
            let _ = self.db.execute_batch("BEGIN");
            for row in batch {
                self.resolve(&domain, row);
                keys.insert((row.project_id.clone(), row.session_id.clone()));
            }
            let _ = self.db.execute_batch("COMMIT");
        }
        Some(keys)
    }

    /// Reads a changed session's row and finds the transcript its chat reads.
    fn resolve(&mut self, domain: &rusqlite::Connection, row: &SessionRow) {
        let session = DomainRepository::new(domain, self.state.metadata.server_id.as_str())
            .get_session(&row.project_id, &row.session_id)
            .ok()
            .flatten();
        let transcript = session
            .as_ref()
            .and_then(transcript_of)
            .and_then(|(path, agent)| {
                let id =
                    store::insert_transcript(&self.db, &path.to_string_lossy(), agent_label(agent))
                        .ok()?;
                Some((id, path, agent))
            });
        let _ = self.db.execute(
            "INSERT INTO sessions (projectId, sessionId, updatedAt, transcriptId)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(projectId, sessionId) DO UPDATE
               SET updatedAt = excluded.updatedAt, transcriptId = excluded.transcriptId",
            params![
                row.project_id,
                row.session_id,
                row.updated_at,
                transcript.as_ref().map(|(id, _, _)| *id)
            ],
        );
        self.sessions.insert(
            (row.project_id.clone(), row.session_id.clone()),
            KnownSession {
                updated_at: row.updated_at.clone(),
                transcript,
            },
        );
    }

    /// Drops sessions that are gone from gxserver, and the transcripts only they read.
    fn forget_deleted(&mut self, seen: &HashSet<(String, String)>) {
        let gone: Vec<(String, String)> = self
            .sessions
            .keys()
            .filter(|key| !seen.contains(*key))
            .cloned()
            .collect();
        if gone.is_empty() {
            return;
        }
        let Ok(tx) = self.db.unchecked_transaction() else {
            return;
        };
        for (project_id, session_id) in &gone {
            let _ = tx.execute(
                "DELETE FROM sessions WHERE projectId = ?1 AND sessionId = ?2",
                params![project_id, session_id],
            );
            self.sessions
                .remove(&(project_id.clone(), session_id.clone()));
        }
        let _ = store::drop_orphans(&tx);
        let _ = tx.commit();
        let live: HashSet<i64> = self
            .sessions
            .values()
            .filter_map(|known| known.transcript.as_ref().map(|(id, _, _)| *id))
            .collect();
        self.readers.retain(|id, _| live.contains(id));
    }

    /// Indexes what `path` gained since the last read. Returns whether it read anything.
    fn catch_up(&mut self, id: i64, path: &Path, agent: SessionChatTranscriptAgent) -> bool {
        let path_text = path.to_string_lossy();
        let Ok(Some(mut row)) = store::transcript(&self.db, &path_text) else {
            return false;
        };
        let Ok(version) = read_transcript_file_version(path) else {
            return false;
        };
        let mtime_ms = version.mtime_ms as i64;
        if row.identity == version.identity
            && row.size == version.size
            && row.mtime_ms == mtime_ms
            && self.verified.contains(&id)
        {
            return false;
        }
        // A different file at that path, a shorter one, or one whose bytes before where the index
        // stopped changed (rewritten in place, checked once per run): read it again from the start.
        let rewritten = row.identity != version.identity
            || version.size < row.read_to
            || (!self.verified.contains(&id)
                && row.read_to > 0
                && boundary_fingerprint(path, row.read_to).ok().as_deref()
                    != Some(row.boundary.as_str()));
        // One transaction per stretch of rows: committing often is what a background thread's
        // throttled disk writes make slow. A crash mid-way leaves rows a later read writes again
        // under the same ids.
        let db = &self.db;
        if db.execute_batch("BEGIN").is_err() {
            return false;
        }
        if rewritten {
            if store::clear_messages(db, id).is_err() {
                let _ = db.execute_batch("ROLLBACK");
                return false;
            }
            self.readers.remove(&id);
            row.read_to = 0;
        }
        self.verified.insert(id);
        let mut reader = self
            .readers
            .remove(&id)
            .unwrap_or_else(|| SessionChatIncrementalState::starting_at(row.read_to));
        if agent == SessionChatTranscriptAgent::OpenCode {
            crate::session_chat_opencode::refresh_for_path(path);
        }
        let mut last_at = 0i64;
        let mut pending = 0usize;
        let mut write = |messages: Vec<SessionChatMessage>| {
            for message in &messages {
                if let Some((role, text)) = indexed_text(message) {
                    let at = message.timestamp.unwrap_or(0);
                    last_at = last_at.max(at);
                    let _ = store::insert_message(db, id, &message.id, role, at, &text);
                    pending += 1;
                }
            }
            if pending >= COMMIT_EVERY {
                let _ = db.execute_batch("COMMIT");
                std::thread::sleep(BATCH_PAUSE);
                let _ = db.execute_batch("BEGIN");
                pending = 0;
            }
        };
        let read = read_incremental_transcript_messages(
            path,
            &mut reader,
            session_chat_line_decoder(agent),
            Some(&mut write),
            None,
            None,
            None,
        );
        match read {
            Ok(messages) => write(messages),
            Err(_) => {
                let _ = db.execute_batch("COMMIT");
                return false;
            }
        }
        let read_to = reader.resume_offset();
        let saved = TranscriptRow {
            identity: version.identity.clone(),
            read_to,
            boundary: if read_to > 0 {
                boundary_fingerprint(path, read_to).unwrap_or_default()
            } else {
                String::new()
            },
            size: version.size,
            mtime_ms,
        };
        let _ = store::save_progress(db, id, &saved, last_at);
        let _ = db.execute_batch("COMMIT");
        self.readers.insert(id, reader);
        true
    }
}

struct SessionRow {
    project_id: String,
    session_id: String,
    updated_at: String,
    live: bool,
}

/// The transcript a session's chat reads, as the open-questions ledger finds it.
fn transcript_of(session: &Value) -> Option<(PathBuf, SessionChatTranscriptAgent)> {
    let agent =
        resolve_session_chat_transcript_agent(session_chat_agent_for_session(session).as_deref())?;
    let path = resolve_session_chat_transcript_path(
        agent,
        read_runtime_text(session, "agentSessionId").as_deref(),
        read_runtime_text(session, "agentSessionPath").as_deref(),
    )?;
    Some((path, agent))
}

fn agent_label(agent: SessionChatTranscriptAgent) -> &'static str {
    match agent {
        SessionChatTranscriptAgent::Antigravity => "antigravity",
        SessionChatTranscriptAgent::Claude => "claude",
        SessionChatTranscriptAgent::Codex => "codex",
        SessionChatTranscriptAgent::Copilot => "copilot",
        SessionChatTranscriptAgent::Cursor => "cursor",
        SessionChatTranscriptAgent::Dsh => "dsh",
        SessionChatTranscriptAgent::Empryo => "empryo",
        SessionChatTranscriptAgent::Freebuff => "freebuff",
        SessionChatTranscriptAgent::Grok => "grok",
        SessionChatTranscriptAgent::Hermes => "hermes",
        SessionChatTranscriptAgent::OpenCode => "opencode",
        SessionChatTranscriptAgent::Pi => "pi",
        SessionChatTranscriptAgent::Zcode => "zcode",
    }
}

/// What the index stores for a row: the words the user wrote (as the chat shows them, without
/// Ghostex's notes) or the words of the agent's reply, never tool calls, tool output or rows the
/// chat hides. `0` is the user, `1` the agent.
fn indexed_text(message: &SessionChatMessage) -> Option<(i64, String)> {
    if message.queued
        || !message
            .blocks
            .iter()
            .any(|block| matches!(block, SessionChatBlock::Text { .. }))
    {
        return None;
    }
    let row: ChatMessage = serde_json::to_value(message)
        .ok()
        .and_then(|value| serde_json::from_value(value).ok())?;
    let (role, text) = match row.role {
        ChatRole::User if is_own_prompt(&row) => (0, shown_text(&row)),
        ChatRole::Assistant if !is_hidden_message(&row) => (1, prose_markdown(&row.blocks)),
        _ => return None,
    };
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut end = text.len().min(MAX_MESSAGE_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Some((role, text[..end].to_string()))
}

/// Puts this thread in the background band: macOS throttles its CPU and disk use while anything
/// else wants them; Linux gives it the lowest nice value.
fn lower_thread_priority() {
    #[cfg(target_os = "macos")]
    unsafe {
        libc::setpriority(libc::PRIO_DARWIN_THREAD, 0, libc::PRIO_DARWIN_BG);
    }
    #[cfg(target_os = "linux")]
    unsafe {
        let tid = libc::syscall(libc::SYS_gettid);
        libc::setpriority(libc::PRIO_PROCESS, tid as libc::id_t, 19);
    }
}
