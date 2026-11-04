//! The session's resource index: every question form, page and chart its conversation held, kept
//! in the `session_chat_resources` table and sent to the chat as `resourceIndex` on its frames, so
//! the Resources pill and list cover the conversation and not only the rows the chat has loaded.
//!
//! The index is fed by the open-questions ledger (`session_chat_open_questions.rs`), which already
//! reads each session's transcript incrementally off every UI path, a sweep every few seconds and
//! each turn end; it is never read on a UI path. A session's first read of a long transcript is the
//! backfill: it runs on a background thread, and a stopped thread of a coordinator that has no
//! index yet is backfilled the same way (`backfill_threads`). Entries are only added or updated,
//! never dropped when a transcript is compacted or rewritten.
//!
//! A coordinator's index also lists the pages its threads posted, resolved threads included
//! (`ThreadRef`), read from each thread's own stored index.
//!
//! The user's decision and the design are on gx-chat-core's questions/resource_index.rs.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use ghostex_gx_chat_core::questions::resource_index::{merge, ResourceEntry, ThreadRef};
use rusqlite::OptionalExtension;
use serde_json::Value;

use crate::domain::DomainRepository;
use crate::server::AppState;

/// How many entries a session's index keeps at most; the oldest are dropped first.
const ENTRY_LIMIT: usize = 2_000;

/// Bumped by every write to the table, so a frame's snapshot knows its cached index is stale and
/// the phone's long poll sees the change (`SessionChatQueueSnapshot::revision`).
static EPOCH: AtomicU64 = AtomicU64::new(1);

pub(crate) fn epoch() -> u64 {
    EPOCH.load(Ordering::Relaxed)
}

/// Frames read the index for each snapshot; the composed index (a coordinator's own entries plus
/// its threads' pages) is cached until the next write anywhere.
fn cache() -> &'static Mutex<HashMap<(String, String), (u64, Arc<Vec<Value>>)>> {
    static CACHE: OnceLock<Mutex<HashMap<(String, String), (u64, Arc<Vec<Value>>)>>> =
        OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// A session's stored entries, oldest first.
pub(crate) fn read_entries(
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
) -> Vec<ResourceEntry> {
    let text: Option<String> = db
        .query_row(
            "SELECT entriesJson FROM session_chat_resources WHERE projectId = ?1 AND sessionId = ?2",
            rusqlite::params![project_id, session_id],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten();
    text.and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Whether the session has an index row at all (even an empty one: it was backfilled).
pub(crate) fn has_row(db: &rusqlite::Connection, project_id: &str, session_id: &str) -> bool {
    db.query_row(
        "SELECT 1 FROM session_chat_resources WHERE projectId = ?1 AND sessionId = ?2",
        rusqlite::params![project_id, session_id],
        |_| Ok(()),
    )
    .optional()
    .ok()
    .flatten()
    .is_some()
}

/// Whether the stored index holds a form or chart that does not say where its reply starts in the
/// transcript (an index written before offsets were kept): it is read again, so each entry can be
/// opened on its own (`read_resource`).
fn lacks_offsets(db: &rusqlite::Connection, project_id: &str, session_id: &str) -> bool {
    read_entries(db, project_id, session_id)
        .iter()
        .any(|entry| entry.kind != "page" && entry.offset.is_none())
}

fn write_entries(
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
    entries: &[ResourceEntry],
) -> anyhow::Result<()> {
    let pages: Vec<&ResourceEntry> = entries
        .iter()
        .filter(|entry| entry.kind == "page" && entry.url.is_some())
        .collect();
    db.execute(
        r#"INSERT INTO session_chat_resources (projectId, sessionId, entriesJson, pagesJson, entryCount, updatedAt)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(projectId, sessionId) DO UPDATE SET
          entriesJson = excluded.entriesJson, pagesJson = excluded.pagesJson,
          entryCount = excluded.entryCount, updatedAt = excluded.updatedAt"#,
        rusqlite::params![
            project_id,
            session_id,
            serde_json::to_string(entries)?,
            serde_json::to_string(&pages)?,
            entries.len() as i64,
            chrono::Utc::now().to_rfc3339(),
        ],
    )?;
    EPOCH.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

/// Merges what the transcript was just read to hold into the session's stored index (`stored` is
/// the ledger's copy of the table, loaded on first use). Writes the table only when something
/// changed; true when it did.
fn merge_and_write(
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
    stored: &mut Option<Vec<ResourceEntry>>,
    read: Vec<ResourceEntry>,
) -> bool {
    let first = stored.is_none();
    let known = stored.get_or_insert_with(|| read_entries(db, project_id, session_id));
    let merged = merge(known.clone(), read, ENTRY_LIMIT);
    let unchanged = *known == merged;
    // The first read of a session also writes an empty index, so "backfilled, nothing found" is
    // told apart from "never read" (`has_row`).
    if unchanged && !(first && !has_row(db, project_id, session_id)) {
        return false;
    }
    *known = merged;
    write_entries(db, project_id, session_id, known).is_ok()
}

/// Brings the session's stored index up to date with what its transcript was just read to hold,
/// and tells the session's clients (and its coordinator's, for a thread's pages) when it changed.
pub(crate) fn store(
    state: &AppState,
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
    stored: &mut Option<Vec<ResourceEntry>>,
    read: Vec<ResourceEntry>,
) {
    if merge_and_write(db, project_id, session_id, stored, read) {
        announce(state, db, project_id, session_id);
    }
}

/// Pushes a state frame to the session's open chats, and to its coordinator's when it is a thread
/// (its pages are listed there).
fn announce(state: &AppState, db: &rusqlite::Connection, project_id: &str, session_id: &str) {
    let repository = DomainRepository::new(db, state.metadata.server_id.as_str());
    let mut targets = vec![(project_id.to_string(), session_id.to_string())];
    if let Ok(Some(thread)) = crate::coordinators::read_thread(db, project_id, session_id) {
        targets.push(thread.coordinator_key());
    }
    for (project, session) in targets {
        if let Ok(Some(session)) = repository.get_session(&project, &session) {
            crate::session_chat_interactive::emit_session_chat_prompt_state_frame(state, &session);
        }
    }
}

/// The index a session's chat is sent: its own entries and, for a coordinator, the pages each of
/// its threads posted (resolved threads too), as JSON records.
pub(crate) fn frame_index(
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
) -> Arc<Vec<Value>> {
    let epoch = epoch();
    let key = (project_id.to_string(), session_id.to_string());
    if let Some((at, index)) = cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(&key).cloned())
    {
        if at == epoch {
            return index;
        }
    }
    let mut entries: Vec<Value> = read_entries(db, project_id, session_id)
        .iter()
        .filter_map(|entry| serde_json::to_value(entry).ok())
        .collect();
    entries.extend(thread_pages(db, project_id, session_id));
    let index = Arc::new(entries);
    if let Ok(mut cache) = cache().lock() {
        cache.insert(key, (epoch, index.clone()));
    }
    index
}

/// The pages the threads of coordinator `project_id`/`session_id` posted, each tagged with its
/// thread. Empty for a session that is no coordinator.
fn thread_pages(db: &rusqlite::Connection, project_id: &str, session_id: &str) -> Vec<Value> {
    let Ok(threads) = crate::coordinators::list_threads_for(db, project_id, session_id) else {
        return Vec::new();
    };
    let repository = DomainRepository::new(db, "");
    let mut pages = Vec::new();
    for thread in threads {
        let text: Option<String> = db
            .query_row(
                "SELECT pagesJson FROM session_chat_resources WHERE projectId = ?1 AND sessionId = ?2",
                rusqlite::params![thread.project_id, thread.session_id],
                |row| row.get(0),
            )
            .optional()
            .ok()
            .flatten();
        let Some(text) = text else {
            continue;
        };
        let Ok(entries) = serde_json::from_str::<Vec<ResourceEntry>>(&text) else {
            continue;
        };
        if entries.is_empty() {
            continue;
        }
        let title = repository
            .get_session(&thread.project_id, &thread.session_id)
            .ok()
            .flatten()
            .map(|session| crate::coordinators::session_title_of(&session))
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| "Worker".to_string());
        let reference = ThreadRef {
            title,
            project_id: thread.project_id.clone(),
            session_id: thread.session_id.clone(),
        };
        for entry in entries {
            let entry = ResourceEntry {
                thread: Some(reference.clone()),
                ..entry
            };
            if let Ok(value) = serde_json::to_value(&entry) {
                pages.push(value);
            }
        }
    }
    pages
}

/// When each session was last checked for a missing index, so a long poll that reads the chat every
/// half second asks the table about it only now and then.
fn last_checked() -> &'static Mutex<HashMap<(String, String), std::time::Instant>> {
    static CHECKED: OnceLock<Mutex<HashMap<(String, String), std::time::Instant>>> =
        OnceLock::new();
    CHECKED.get_or_init(Default::default)
}

/// Makes sure a session that is being read has an index, and so do the threads of a coordinator
/// being read: a stopped session is never swept, and a chat opened on one must not wait for the
/// next time it runs. The reads happen on background threads; the chat is sent the index in the
/// next frame (`store` announces it).
pub(crate) fn ensure_indexed(
    state: &AppState,
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
) {
    const RECHECK: std::time::Duration = std::time::Duration::from_secs(30);
    let key = (project_id.to_string(), session_id.to_string());
    if let Ok(mut checked) = last_checked().lock() {
        if checked.get(&key).is_some_and(|at| at.elapsed() < RECHECK) {
            return;
        }
        checked.insert(key.clone(), std::time::Instant::now());
    }
    if !has_row(db, project_id, session_id) || lacks_offsets(db, project_id, session_id) {
        start_index(state, project_id, session_id);
    }
    if crate::coordinators::read_coordinator(db, project_id, session_id)
        .ok()
        .flatten()
        .is_some()
    {
        backfill_threads(state, db, project_id, session_id);
    }
}

fn start_index(state: &AppState, project_id: &str, session_id: &str) {
    let key = (project_id.to_string(), session_id.to_string());
    let started = in_flight()
        .lock()
        .map(|mut flying| flying.insert(key.clone()))
        .unwrap_or(false);
    if !started {
        return;
    }
    let state = state.clone();
    std::thread::spawn(move || {
        crate::session_chat_open_questions::index_only(&state, &key.0, &key.1);
        if let Ok(mut flying) = in_flight().lock() {
            flying.remove(&key);
        }
    });
}

/// The largest reply read back for one resource; a bigger one is not a form someone wrote.
const RECORD_MAX_BYTES: u64 = 16 * 1024 * 1024;

/// The one transcript record that starts at `offset`, decoded like the chat's own reads, with its
/// position set. Reads that record alone: the cost is its own size, however long the file is.
fn read_record_at(
    path: &std::path::Path,
    decode: crate::session_chat::SessionChatLineDecoder,
    offset: u64,
) -> Option<crate::session_chat::SessionChatMessage> {
    use std::io::{BufRead, Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut line = Vec::new();
    std::io::BufReader::new(file.take(RECORD_MAX_BYTES))
        .read_until(b'\n', &mut line)
        .ok()?;
    let line = String::from_utf8(line).ok()?;
    let fallback = crate::session_chat::transcript_fallback_id(path, offset);
    let mut message = decode(line.trim_end_matches(['\n', '\r']), &fallback)?;
    message.byte_offset = Some(offset);
    Some(message)
}

/// `/api/readSessionChatResource` `{projectId, sessionId, id}`: the reply that holds a form, page
/// or chart the session's index lists, and the message that answered it, read back from the
/// transcript by the byte offsets the index kept. The chat asks for it when the reader opens a
/// resource older than the rows it has loaded, so it needs no frame to carry a body and no
/// transcript read: two seeks, whatever the file's size.
///
/// A resource that cannot be read says so (`notFound`): it is not in the index, the index has no
/// position for it yet (it is read again in the background), the transcript is gone, or the
/// record at that position is no longer the one that held it (the transcript was rewritten).
pub(crate) fn read_resource(
    repository: &DomainRepository<'_>,
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
    id: &str,
) -> Result<Value, crate::domain::DomainStateError> {
    let missing = |message: &str| crate::domain::DomainStateError::not_found(message.to_string());
    let entry = read_entries(db, project_id, session_id)
        .into_iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| missing("That item is not in this conversation's index."))?;
    let offset = entry
        .offset
        .ok_or_else(|| missing("Ghostex has not recorded where that item sits yet."))?;
    let session = repository
        .get_session(project_id, session_id)?
        .ok_or_else(|| missing("The session no longer exists."))?;
    let agent = crate::session_chat::resolve_session_chat_transcript_agent(
        crate::session_chat_follower::session_chat_agent_for_session(&session).as_deref(),
    )
    .ok_or_else(|| missing("This session has no transcript Ghostex reads."))?;
    let path = crate::session_chat::resolve_session_chat_transcript_path(
        agent,
        crate::server::read_runtime_text(&session, "agentSessionId").as_deref(),
        crate::server::read_runtime_text(&session, "agentSessionPath").as_deref(),
    )
    .ok_or_else(|| missing("The transcript is not there any more."))?;
    let decode = crate::session_chat::session_chat_line_decoder(agent);
    let holder = read_record_at(&path, decode, offset)
        .ok_or_else(|| missing("The transcript no longer has that reply."))?;
    // The record at that position must still hold the item.
    let holds = serde_json::to_value(&holder)
        .ok()
        .and_then(|value| serde_json::from_value::<ghostex_gx_protocol::ChatMessage>(value).ok())
        .is_some_and(|message| {
            ghostex_gx_chat_core::questions::resource_index::scan_message(&message)
                .iter()
                .any(|held| held.id == id)
        });
    if !holds {
        return Err(missing(
            "The transcript has changed since that was recorded.",
        ));
    }
    let mut messages = vec![serde_json::to_value(&holder).unwrap_or_default()];
    if let Some(answers) = entry
        .answers_offset
        .and_then(|at| read_record_at(&path, decode, at))
    {
        messages.push(serde_json::to_value(&answers).unwrap_or_default());
    }
    Ok(serde_json::json!({ "id": id, "messages": messages }))
}

/// Threads being backfilled right now, so a sweep never starts the same read twice.
fn in_flight() -> &'static Mutex<HashSet<(String, String)>> {
    static IN_FLIGHT: OnceLock<Mutex<HashSet<(String, String)>>> = OnceLock::new();
    IN_FLIGHT.get_or_init(Default::default)
}

/// Builds the index of every thread of this coordinator that has none yet (a resolved or stopped
/// thread is never swept), once, on background threads.
pub(crate) fn backfill_threads(
    state: &AppState,
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
) {
    let Ok(threads) = crate::coordinators::list_threads_for(db, project_id, session_id) else {
        return;
    };
    for thread in threads {
        if has_row(db, &thread.project_id, &thread.session_id) {
            continue;
        }
        start_index(state, &thread.project_id, &thread.session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::get_gxserver_paths;
    use crate::storage::{initialize_gxserver_storage, open_gxserver_database};

    fn database() -> (tempfile::TempDir, crate::storage::GxserverDatabase) {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        (temp, db)
    }

    fn entry(id: &str, kind: &str, status: &str, at: i64) -> ResourceEntry {
        ResourceEntry {
            id: id.to_string(),
            kind: kind.to_string(),
            title: format!("About {id}"),
            status: status.to_string(),
            at_ms: Some(at),
            url: (kind == "page").then(|| format!("http://127.0.0.1:1/visual/{id}")),
            ..ResourceEntry::default()
        }
    }

    /// The first read of a session writes its index even when it holds nothing, so a backfill is
    /// never repeated; a read that finds nothing new writes nothing.
    #[test]
    fn the_first_read_marks_the_session_backfilled_and_later_reads_write_only_changes() {
        let (_temp, db) = database();
        assert!(!has_row(&db, "p", "s"));
        let mut stored = None;
        assert!(merge_and_write(&db, "p", "s", &mut stored, Vec::new()));
        assert!(has_row(&db, "p", "s"));
        assert!(
            !merge_and_write(&db, "p", "s", &mut stored, Vec::new()),
            "a read that finds nothing new writes nothing"
        );
        let found = vec![entry("a", "form", "waiting", 1)];
        assert!(merge_and_write(&db, "p", "s", &mut stored, found.clone()));
        assert!(!merge_and_write(&db, "p", "s", &mut stored, found));
        assert_eq!(read_entries(&db, "p", "s").len(), 1);
    }

    /// A transcript that was compacted or rewritten no longer holds the old entries; the index
    /// still does, and the count survives a restart (a new ledger loads the table).
    #[test]
    fn the_index_survives_a_compaction_and_a_restart() {
        let (_temp, db) = database();
        let mut stored = None;
        let before: Vec<ResourceEntry> = (0..300)
            .map(|n| {
                entry(
                    &format!("r{n}"),
                    if n % 5 == 0 { "page" } else { "form" },
                    "answered",
                    n,
                )
            })
            .collect();
        merge_and_write(&db, "p", "s", &mut stored, before);
        // Compaction: the rewritten transcript holds only the newest 40.
        let after: Vec<ResourceEntry> = (260..300)
            .map(|n| {
                entry(
                    &format!("r{n}"),
                    if n % 5 == 0 { "page" } else { "form" },
                    "answered",
                    n,
                )
            })
            .collect();
        merge_and_write(&db, "p", "s", &mut stored, after);
        assert_eq!(
            read_entries(&db, "p", "s").len(),
            300,
            "nothing was dropped"
        );
        // A restart: a fresh ledger (nothing cached) reads the same table, and a status that moved
        // is updated in place.
        let mut restarted = None;
        let moved = vec![entry("r299", "form", "withdrawn", 299)];
        merge_and_write(&db, "p", "s", &mut restarted, moved);
        let entries = read_entries(&db, "p", "s");
        assert_eq!(entries.len(), 300);
        assert_eq!(entries.last().map(|e| e.status.as_str()), Some("withdrawn"));
    }

    /// A coordinator's frame lists its own entries and the pages (only pages) of every thread it
    /// has, resolved ones included, each with the thread it came from.
    #[test]
    fn a_coordinators_index_adds_the_pages_its_threads_posted() {
        let (_temp, db) = database();
        crate::coordinators::link_thread(&db, "p", "thread-a", "p", "coord", "task a").unwrap();
        crate::coordinators::link_thread(&db, "p", "thread-b", "p", "coord", "task b").unwrap();
        crate::coordinators::set_thread_resolved(&db, "p", "thread-b", true).unwrap();
        crate::coordinators::link_thread(&db, "p", "stranger", "p", "other-coord", "").unwrap();
        let mut none = None;
        merge_and_write(
            &db,
            "p",
            "coord",
            &mut none,
            vec![entry("mine", "form", "answered", 1)],
        );
        let mut a = None;
        merge_and_write(
            &db,
            "p",
            "thread-a",
            &mut a,
            vec![
                entry("a-page", "page", "other", 2),
                entry("a-form", "form", "waiting", 3),
                entry("a-chart", "chart", "other", 4),
            ],
        );
        let mut b = None;
        merge_and_write(
            &db,
            "p",
            "thread-b",
            &mut b,
            vec![entry("b-page", "page", "other", 5)],
        );
        let mut other = None;
        merge_and_write(
            &db,
            "p",
            "stranger",
            &mut other,
            vec![entry("x-page", "page", "other", 6)],
        );

        let index = frame_index(&db, "p", "coord");
        let ids: Vec<&str> = index.iter().filter_map(|v| v["id"].as_str()).collect();
        assert_eq!(
            ids,
            ["mine", "a-page", "b-page"],
            "own entries, then thread pages only"
        );
        let thread_of = |id: &str| {
            index
                .iter()
                .find(|v| v["id"] == id)
                .and_then(|v| v["thread"]["sessionId"].as_str())
                .map(str::to_string)
        };
        assert_eq!(thread_of("a-page").as_deref(), Some("thread-a"));
        assert_eq!(thread_of("b-page").as_deref(), Some("thread-b"));
        assert_eq!(thread_of("mine"), None);
        // A thread's own chat lists its own entries and nothing else.
        assert_eq!(frame_index(&db, "p", "thread-a").len(), 3);
        assert_eq!(
            *frame_index(&db, "p", "coord"),
            *index,
            "read again, the same list"
        );
        let mut c = None;
        merge_and_write(
            &db,
            "p",
            "thread-a",
            &mut c,
            vec![entry("a-page-2", "page", "other", 7)],
        );
        assert_eq!(frame_index(&db, "p", "coord").len(), 4);
    }

    fn claude_line(kind: &str, uuid: &str, text: &str) -> String {
        let message = if kind == "assistant" {
            serde_json::json!({"role": "assistant", "content": [{"type": "text", "text": text}]})
        } else {
            serde_json::json!({"role": "user", "content": text})
        };
        serde_json::json!({
            "type": kind, "uuid": uuid, "parentUuid": null, "isSidechain": false,
            "timestamp": "2026-10-30T10:00:00.000Z", "sessionId": "s", "cwd": "/x", "message": message,
        })
        .to_string()
    }

    /// The index keeps each reply's byte offset (and the answers message's), and `read_record_at`
    /// reads exactly that record back by seeking, past any amount of unrelated transcript.
    #[test]
    fn a_resource_is_read_back_by_its_offset_past_a_long_transcript() {
        use crate::session_chat::{
            read_incremental_transcript_messages, session_chat_line_decoder,
            SessionChatIncrementalState, SessionChatTranscriptAgent,
        };
        let form = "```questions\n{\"title\":\"Pick\",\"questions\":[{\"tag\":\"Q1\",\"title\":\"Which?\",\"choices\":[\"A\",\"B\"]}]}\n```";
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("t.jsonl");
        let filler = "x".repeat(20_000);
        let mut text = String::new();
        // About 20 MB of other replies before the form, then the form, more filler, the answers.
        for n in 0..1_000 {
            text.push_str(&claude_line("assistant", &format!("f{n}"), &filler));
            text.push('\n');
        }
        text.push_str(&claude_line(
            "assistant",
            "the-form",
            &format!("Here.\n\n{form}"),
        ));
        text.push('\n');
        for n in 0..50 {
            text.push_str(&claude_line("assistant", &format!("g{n}"), &filler));
            text.push('\n');
        }
        let form_id = ghostex_gx_chat_core::transcript::question_forms::form_id(
            form.trim_start_matches("```questions\n")
                .trim_end_matches("\n```"),
        );
        let answers = format!(
            "<ghostex-page-answers page=\"{form_id}\" title=\"Pick\">{{\"Which?\":\"A\"}}</ghostex-page-answers>"
        );
        text.push_str(&claude_line("user", "the-answers", &answers));
        text.push('\n');
        std::fs::write(&path, &text).unwrap();

        // The ledger reads the whole file the way a sweep does and records the offsets.
        let mut ledger = crate::session_chat_open_questions::Ledger::default();
        let mut state = SessionChatIncrementalState::default();
        let decode = session_chat_line_decoder(SessionChatTranscriptAgent::Claude);
        let read =
            read_incremental_transcript_messages(&path, &mut state, decode, None, None, None, None)
                .unwrap();
        ledger.observe(read);
        let index = ledger.index(&[], None, |_| false);
        let entry = index
            .iter()
            .find(|e| e.kind == "form")
            .expect("the form is indexed");
        assert_eq!(entry.status, "answered");
        let at = text
            .find(&claude_line(
                "assistant",
                "the-form",
                &format!("Here.\n\n{form}"),
            ))
            .unwrap() as u64;
        let answers_at = text
            .find(&claude_line("user", "the-answers", &answers))
            .unwrap() as u64;
        assert_eq!(entry.offset, Some(at));
        assert_eq!(entry.answers_offset, Some(answers_at));
        assert_eq!(
            entry.turn_end,
            Some(answers_at),
            "the next user message ends the reply's turn"
        );

        // Reading it back touches the records at those two offsets only.
        let holder = read_record_at(&path, decode, at).expect("the form's reply");
        assert!(holder.byte_offset == Some(at));
        let body = serde_json::to_value(&holder).unwrap().to_string();
        assert!(body.contains("Which?"));
        let answered = read_record_at(&path, decode, answers_at).expect("the answers");
        assert!(serde_json::to_value(&answered)
            .unwrap()
            .to_string()
            .contains("ghostex-page-answers"));
        // A position in the middle of a record reads nothing that decodes.
        assert!(read_record_at(&path, decode, at + 7).is_none());
    }
}
