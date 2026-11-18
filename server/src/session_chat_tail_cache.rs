//! The newest rows of a transcript, kept while the file is unchanged, and a log line naming who
//! reads one transcript far too often.
//!
//! CDXC:SessionChat 2026-11-18 WHY:
//! GitHub #668 (gxserver 11.8.1, headless Linux): about 2.6 cores went to opening one idle 10 MB
//! coordinator transcript some 20 times a second, reading about 600 KB each time, while nothing
//! wrote to it; it stopped when the coordinator's open questions were answered. Many readers read a
//! transcript's tail with no check that the file changed (the coordinator drift check and the
//! notes under every message to a coordinator, the queue's lifecycle gate, notification bodies,
//! the question resend, the stall and delivery checks, the chat read), and which one looped there
//! was not found. Every `read_session_chat_tail_page` now answers from the last read of the same
//! rows while the file has the same identity, size and modified time, so an idle transcript costs
//! one `stat` however often it is asked; the answer is the one a new read would give, because the
//! rows decode from the file's bytes alone. And when one transcript is asked for more than
//! `HOT_READS` times in `HOT_WINDOW`, one warning names the callers and how many of the asks
//! actually read the file, so a loop like #668's names itself in gxserver.jsonl.

use std::collections::HashMap;
use std::panic::Location;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::json;

use crate::logging::{GxserverLogInput, GxserverLogger, LogLevel};
use crate::session_chat::{SessionChatTranscriptAgent, TranscriptFileVersion};
use crate::session_chat_tail::SessionChatTailPage;

/// The bytes of transcript the kept answers may span together; the oldest go first past it.
const KEPT_BYTES: u64 = 48 * 1024 * 1024;
/// A page spanning more than this is large: large pages are kept too (the heaviest transcripts
/// are the ones worth it, GitHub #668) but within their own budget, `LARGE_PAGES_BYTES`, so they
/// never push small ones out.
const LARGE_PAGE_BYTES: u64 = KEPT_BYTES / 4;
/// Room for two large pages (an alternating pair of heavy transcripts each evicted the other
/// within half the cap, review of e31727594); the one just kept is never evicted.
const LARGE_PAGES_BYTES: u64 = KEPT_BYTES / 4 * 3;
/// What large pages may hold once the whole cache is over `KEPT_BYTES`, so ordinary pages in
/// rotation are not evicted in turn with them (review of d53b95d78).
const LARGE_PAGES_WHEN_FULL: u64 = KEPT_BYTES / 2;
/// The answers kept at most, whatever their size (`KEPT_BYTES` bounds the memory).
const KEPT_ANSWERS: usize = 256;
/// Asks for one transcript within `HOT_WINDOW` past which its callers are logged.
const HOT_READS: u32 = 40;
const HOT_WINDOW: Duration = Duration::from_secs(10);
/// One warning per transcript at most this often.
const HOT_LOG_EVERY: Duration = Duration::from_secs(60);

#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    agent: std::mem::Discriminant<SessionChatTranscriptAgent>,
    path: PathBuf,
    limit: usize,
    before_offset: Option<u64>,
}

struct Kept {
    version: TranscriptFileVersion,
    /// `session_chat_pending_rewind_generation` when it was read: a rewind recorded or cleared
    /// since changes the page without touching the file.
    rewinds: u64,
    /// Shared, so a hit copies the rows after letting the cache's lock go.
    page: std::sync::Arc<SessionChatTailPage>,
    /// The bytes the page was read from, for `KEPT_BYTES`.
    bytes: u64,
    used: Instant,
}

fn kept() -> &'static Mutex<HashMap<Key, Kept>> {
    static KEPT: OnceLock<Mutex<HashMap<Key, Kept>>> = OnceLock::new();
    KEPT.get_or_init(Default::default)
}

/// `read`'s page of `path`, or the page the same read gave while the file is unchanged since.
/// A file that cannot be stat'ed, and a read that fails, are never kept.
pub(crate) fn tail_page(
    agent: SessionChatTranscriptAgent,
    path: &Path,
    limit: usize,
    before_offset: Option<u64>,
    caller: &'static Location<'static>,
    read: impl FnOnce() -> std::io::Result<SessionChatTailPage>,
) -> std::io::Result<SessionChatTailPage> {
    let tail = before_offset.is_none();
    let Ok(version) = crate::session_chat::read_transcript_file_version(path) else {
        note_read(path, caller, true, tail);
        return read();
    };
    let rewinds = crate::session_chat_rewind_state::session_chat_pending_rewind_generation();
    let key = Key {
        agent: std::mem::discriminant(&agent),
        path: path.to_path_buf(),
        limit,
        before_offset,
    };
    if let Ok(mut kept) = kept().lock() {
        if let Some(entry) = kept
            .get_mut(&key)
            .filter(|entry| entry.version == version && entry.rewinds == rewinds)
        {
            entry.used = Instant::now();
            let page = entry.page.clone();
            drop(kept);
            note_read(path, caller, false, tail);
            return Ok((*page).clone());
        }
    }
    note_read(path, caller, true, tail);
    let page = read()?;
    // Still the file (and the rewinds) that was read: a change during the read keeps nothing.
    if crate::session_chat::read_transcript_file_version(path)
        .ok()
        .as_ref()
        == Some(&version)
        && crate::session_chat_rewind_state::session_chat_pending_rewind_generation() == rewinds
    {
        // A cursor past the end (a stitched one encodes its hop) counts as the file's end.
        let bytes = match &page {
            SessionChatTailPage::Page {
                before_offset: from,
                ..
            } => before_offset
                .unwrap_or(version.size)
                .min(version.size)
                .saturating_sub(*from),
            SessionChatTailPage::NotFound => 0,
        };
        // A page larger than the whole cache is never kept.
        if bytes <= KEPT_BYTES {
            let shared = std::sync::Arc::new(page.clone());
            if let Ok(mut kept) = kept().lock() {
                kept.insert(
                    key.clone(),
                    Kept {
                        version,
                        rewinds,
                        page: shared,
                        bytes,
                        used: Instant::now(),
                    },
                );
                trim(&mut kept, &key);
            }
        }
    }
    Ok(page)
}

/// Drops answers past the budgets, never `inserted` (the page just kept): the oldest large page
/// while large pages together pass `LARGE_PAGES_BYTES`, or pass `LARGE_PAGES_WHEN_FULL` while the
/// whole cache passes `KEPT_BYTES`; then the least recently used while all of them pass
/// `KEPT_ANSWERS` or `KEPT_BYTES`.
fn trim(kept: &mut HashMap<Key, Kept>, inserted: &Key) {
    loop {
        let (bytes, large) = kept.values().fold((0u64, 0u64), |(all, large), entry| {
            (
                all.saturating_add(entry.bytes),
                if entry.bytes > LARGE_PAGE_BYTES {
                    large.saturating_add(entry.bytes)
                } else {
                    large
                },
            )
        });
        let only_large =
            large > LARGE_PAGES_BYTES || (bytes > KEPT_BYTES && large > LARGE_PAGES_WHEN_FULL);
        if !only_large && kept.len() <= KEPT_ANSWERS && bytes <= KEPT_BYTES {
            return;
        }
        let Some(oldest) = kept
            .iter()
            .filter(|(key, entry)| {
                *key != inserted && (!only_large || entry.bytes > LARGE_PAGE_BYTES)
            })
            .min_by_key(|(_, entry)| entry.used)
            .map(|(key, _)| key.clone())
        else {
            return;
        };
        kept.remove(&oldest);
    }
}

#[derive(Default)]
struct Asks {
    window_start: Option<Instant>,
    asks: u32,
    reads: u32,
    callers: HashMap<&'static Location<'static>, u32>,
    logged: Option<Instant>,
}

fn asks() -> &'static Mutex<HashMap<PathBuf, Asks>> {
    static ASKS: OnceLock<Mutex<HashMap<PathBuf, Asks>>> = OnceLock::new();
    ASKS.get_or_init(Default::default)
}

/// Counts one ask for `path` by `caller` (`read` when the file was actually read), and logs the
/// callers once the transcript is asked for more than `HOT_READS` times in `HOT_WINDOW`. Only
/// asks for the newest rows (`tail`) count: walking back through older pages (a turn's work, a
/// delivery check, history) asks many times in a row by design.
pub(crate) fn note_read(path: &Path, caller: &'static Location<'static>, read: bool, tail: bool) {
    if !tail {
        return;
    }
    let now = Instant::now();
    let Ok(mut asks) = asks().lock() else {
        return;
    };
    // Transcripts nobody read for a while are forgotten.
    if asks.len() > 256 {
        asks.retain(|_, entry| {
            entry
                .window_start
                .is_some_and(|start| now.duration_since(start) < HOT_LOG_EVERY)
        });
    }
    let entry = asks.entry(path.to_path_buf()).or_default();
    if entry
        .window_start
        .is_none_or(|start| now.duration_since(start) >= HOT_WINDOW)
    {
        entry.window_start = Some(now);
        entry.asks = 0;
        entry.reads = 0;
        entry.callers.clear();
    }
    entry.asks += 1;
    if read {
        entry.reads += 1;
    }
    *entry.callers.entry(caller).or_default() += 1;
    if entry.asks <= HOT_READS
        || entry
            .logged
            .is_some_and(|logged| now.duration_since(logged) < HOT_LOG_EVERY)
    {
        return;
    }
    entry.logged = Some(now);
    let mut callers: Vec<(String, u32)> = entry
        .callers
        .iter()
        .map(|(location, count)| (format!("{}:{}", location.file(), location.line()), *count))
        .collect();
    callers.sort_by(|a, b| b.1.cmp(&a.1));
    let details = json!({
        "transcript": path.file_name().map(|name| name.to_string_lossy().to_string()),
        "asks": entry.asks,
        "reads": entry.reads,
        "windowSeconds": HOT_WINDOW.as_secs(),
        "callers": callers
            .into_iter()
            .map(|(location, count)| json!({ "at": location, "asks": count }))
            .collect::<Vec<_>>(),
    });
    drop(asks);
    let _ = logger().log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "sessionChatTranscriptReadOften".to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(details),
    });
}

/// Under test the log lives in a temporary home instead of the user's own.
fn logger() -> &'static GxserverLogger {
    static LOGGER: OnceLock<GxserverLogger> = OnceLock::new();
    LOGGER.get_or_init(|| {
        #[cfg(test)]
        let home = Some(
            std::env::temp_dir().join(format!("gxserver-tail-cache-tests-{}", std::process::id())),
        );
        #[cfg(not(test))]
        let home = None;
        GxserverLogger::new(crate::paths::get_gxserver_paths(home))
    })
}
