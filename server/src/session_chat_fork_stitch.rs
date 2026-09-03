use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Value};

use crate::session_chat::{
    parse_json_object, timestamp_ms, SessionChatBlock, SessionChatMessage, SessionChatRole,
    SessionChatSource, SessionChatTranscriptAgent,
};
use crate::session_chat_paths::read_transcript_head_complete_lines;
use crate::session_chat_successor::{
    codex_rollout_session_id, collect_all_codex_day_directories, read_codex_session_meta,
    SUCCESSOR_CHAIN_LIMIT,
};
use crate::session_chat_tail::{read_session_chat_tail_page, SessionChatTailPage};

/*
CDXC:SessionFork 2026-08-28:
`codex fork` (codex-cli >= 0.149) does NOT replay history into the file it
opens: the new rollout starts with a `session_meta` naming its predecessor in
`payload.forked_from_id` and then continues appending. The follower already
adopts the successor for LIVE streaming (CDXC:SessionIdentity 2026-08-24),
but chat pagination pages by byte offset inside ONE file, so scroll-back used to
dead-end at the fork point while the terminal scrollback still showed the whole
thread. This module stitches the lineage back together on the read path.

Cursor encoding — the wire contract is a single `beforeOffset: u64` that clients
echo back verbatim and never do arithmetic on, so the hop has to ride inside it:

    hop 0 (current file) : cursor = raw byte offset          (< ANCESTOR_CURSOR_BASE)
    hop 1..=8 (ancestors): cursor = ANCESTOR_CURSOR_BASE
                                  + (hop << ANCESTOR_CURSOR_HOP_SHIFT)
                                  + within-file byte offset

`ANCESTOR_CURSOR_BASE` is 2^50 (1 PiB), far above any real transcript offset, so
a hop-0 cursor can never be mistaken for an ancestor cursor. The hop field is
capped at `SUCCESSOR_CHAIN_LIMIT` (8) — the same bound the successor walk uses —
and the offset field is 42 bits (4 TiB), so the largest cursor is
2^50 + (8 << 42) + 2^42 - 1 < 2^53 and survives the JSON-safe-integer range the
mobile RN bridge validates against.

Because a stitched cursor is not a monotonically decreasing raw offset, the read
path MUST report `hasMoreExact: true` (it already does, unconditionally) so the
client's `sessionChatPageHasMore` trusts `hasMore` instead of comparing cursors.

Codex-only: the gate is "transcript agent is Codex AND the path is a
`rollout-<ts>-<uuid>` file whose opening `session_meta` carries
`forked_from_id`". Every other agent takes the untouched single-file path.
*/

/// First cursor value that names an ancestor hop instead of a raw byte offset.
pub const ANCESTOR_CURSOR_BASE: u64 = 1 << 50;
const ANCESTOR_CURSOR_HOP_SHIFT: u32 = 42;
const ANCESTOR_CURSOR_OFFSET_MASK: u64 = (1u64 << ANCESTOR_CURSOR_HOP_SHIFT) - 1;

pub const FORK_BOUNDARY_MESSAGE_ID_PREFIX: &str = "fork-boundary:";
/// Shown when the pre-fork rows are loaded (or still loadable) above the marker.
const FORK_BOUNDARY_TEXT_SHARED: &str = "Session forked from an earlier thread. Messages above this point are shared with sibling branches.";
/// Shown when the predecessor rollout is not on disk any more, so nothing older
/// can ever be paged in. This is defined behavior, not a fallback: the boundary
/// is real and the user is told the history behind it is gone.
const FORK_BOUNDARY_TEXT_UNAVAILABLE: &str =
    "Session forked from an earlier thread. The earlier history is no longer available.";

fn encode_stitched_cursor(hop: usize, offset: u64) -> u64 {
    if hop == 0 {
        return offset;
    }
    ANCESTOR_CURSOR_BASE
        + ((hop as u64) << ANCESTOR_CURSOR_HOP_SHIFT)
        + offset.min(ANCESTOR_CURSOR_OFFSET_MASK)
}

fn decode_stitched_cursor(cursor: u64) -> (usize, u64) {
    if cursor < ANCESTOR_CURSOR_BASE {
        return (0, cursor);
    }
    let raw = cursor - ANCESTOR_CURSOR_BASE;
    (
        (raw >> ANCESTOR_CURSOR_HOP_SHIFT) as usize,
        raw & ANCESTOR_CURSOR_OFFSET_MASK,
    )
}

// ---------------------------------------------------------------------------
// Lineage resolution
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct SessionChatForkAncestor {
    /// The rollout uuid named by the child's `forked_from_id`.
    pub session_id: String,
    /// `None` when the rollout is not on disk any more, or when its own
    /// `session_meta` does not declare this id. The chain stops there.
    pub path: Option<PathBuf>,
    /// This ancestor's OWN `forked_from_id`, i.e. the next hop.
    pub forked_from_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SessionChatForkLineage {
    /// Session id declared by the current rollout's own `session_meta`.
    pub session_id: String,
    pub forked_from_id: Option<String>,
    /// hop 1 = immediate predecessor, hop 2 = its predecessor, and so on.
    pub ancestors: Vec<SessionChatForkAncestor>,
}

impl SessionChatForkLineage {
    fn session_id_at(&self, hop: usize) -> Option<&str> {
        if hop == 0 {
            return Some(self.session_id.as_str());
        }
        Some(self.ancestors.get(hop - 1)?.session_id.as_str())
    }

    fn forked_from_at(&self, hop: usize) -> Option<&str> {
        if hop == 0 {
            return self.forked_from_id.as_deref();
        }
        self.ancestors.get(hop - 1)?.forked_from_id.as_deref()
    }

    /// Readable file for `hop`; hop 0 is the caller's own transcript path.
    fn ancestor_path(&self, hop: usize) -> Option<&Path> {
        if hop == 0 {
            return None;
        }
        self.ancestors.get(hop - 1)?.path.as_deref()
    }

    pub fn fork_info(&self) -> Option<SessionChatForkInfo> {
        Some(SessionChatForkInfo {
            forked_from_id: self.forked_from_id.clone()?,
            ancestor_ids: self
                .ancestors
                .iter()
                .map(|ancestor| ancestor.session_id.clone())
                .collect(),
        })
    }
}

/// Shape mirrored by `GxserverReadSessionChatResult["forkInfo"]`.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionChatForkInfo {
    pub forked_from_id: String,
    pub ancestor_ids: Vec<String>,
}

impl SessionChatForkInfo {
    pub fn to_value(&self) -> Value {
        json!({
            "forkedFromId": self.forked_from_id,
            "ancestorIds": self.ancestor_ids,
        })
    }
}

fn is_codex_rollout_path(path: &Path) -> bool {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| stem.starts_with("rollout-"))
        .and_then(codex_rollout_session_id)
        .is_some()
}

/*
Every rollout uuid on disk, mapped to its file. An ancestor's day directory is
`<=` the child's, so the forward-only date filter the successor scan uses cannot
be reused here: the walk covers every day directory under the sessions root.
Built at most once per lineage resolution, and only for a rollout that actually
declares a `forked_from_id`.
*/
fn codex_rollout_index(rollout_path: &Path) -> Option<HashMap<String, PathBuf>> {
    let sessions_root = rollout_path
        .parent()?
        .parent()?
        .parent()?
        .parent()?
        .to_path_buf();
    let mut index: HashMap<String, PathBuf> = HashMap::new();
    for day_directory in collect_all_codex_day_directories(&sessions_root) {
        let Ok(entries) = fs::read_dir(&day_directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(session_id) = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .filter(|stem| stem.starts_with("rollout-"))
                .and_then(codex_rollout_session_id)
            else {
                continue;
            };
            index.entry(session_id).or_insert(path);
        }
    }
    Some(index)
}

fn resolve_codex_fork_lineage(rollout_path: &Path) -> Option<SessionChatForkLineage> {
    let meta = read_codex_session_meta(rollout_path)?;
    let mut lineage = SessionChatForkLineage {
        session_id: meta.session_id,
        forked_from_id: meta.forked_from_id.clone(),
        ancestors: Vec::new(),
    };
    let Some(mut next_session_id) = meta.forked_from_id else {
        return Some(lineage);
    };
    let Some(index) = codex_rollout_index(rollout_path) else {
        lineage.ancestors.push(SessionChatForkAncestor {
            session_id: next_session_id,
            path: None,
            forked_from_id: None,
        });
        return Some(lineage);
    };
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(lineage.session_id.clone());
    for _ in 1..=SUCCESSOR_CHAIN_LIMIT {
        // Cycle guard: a hand-edited `forked_from_id` must not loop forever.
        if !visited.insert(next_session_id.clone()) {
            break;
        }
        let candidate = index.get(&next_session_id).cloned();
        // The ancestor must declare its OWN filename uuid, exactly like the
        // successor proof does, before its rows are stitched into this chat.
        let ancestor_meta = candidate
            .as_deref()
            .and_then(read_codex_session_meta)
            .filter(|meta| meta.session_id == next_session_id);
        let forked_from_id = ancestor_meta
            .as_ref()
            .and_then(|meta| meta.forked_from_id.clone());
        let path = ancestor_meta.is_some().then_some(candidate).flatten();
        let resolved = path.is_some();
        lineage.ancestors.push(SessionChatForkAncestor {
            session_id: next_session_id,
            path,
            forked_from_id: forked_from_id.clone(),
        });
        match forked_from_id {
            Some(parent) if resolved => next_session_id = parent,
            _ => break,
        }
    }
    Some(lineage)
}

/// Ancestor rollouts are dead files whose opening `session_meta` never changes,
/// so one resolution per rollout path is enough for the lifetime of the daemon.
/// The map is cleared wholesale once it grows past the cap rather than carrying
/// an LRU: entries are tiny and a re-resolution is one head read plus one
/// directory walk.
const FORK_LINEAGE_CACHE_LIMIT: usize = 256;

fn fork_lineage_cache() -> &'static Mutex<HashMap<PathBuf, Option<SessionChatForkLineage>>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Option<SessionChatForkLineage>>>> =
        OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Fork lineage of a Codex rollout. `None` for any other agent, for a path that
/// is not a rollout, and for a rollout with no readable `session_meta`.
pub fn codex_fork_lineage(
    agent: SessionChatTranscriptAgent,
    file_path: &Path,
) -> Option<SessionChatForkLineage> {
    if agent != SessionChatTranscriptAgent::Codex || !is_codex_rollout_path(file_path) {
        return None;
    }
    if let Ok(cache) = fork_lineage_cache().lock() {
        if let Some(cached) = cache.get(file_path) {
            return cached.clone();
        }
    }
    let lineage = resolve_codex_fork_lineage(file_path);
    if let Ok(mut cache) = fork_lineage_cache().lock() {
        if cache.len() >= FORK_LINEAGE_CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(file_path.to_path_buf(), lineage.clone());
    }
    lineage
}

/// Cheap "does scroll-back continue past the top of this file" probe for the
/// follower's frames. Cached with the lineage, so the follower pays for it once
/// per adopted transcript path and never per frame.
pub fn codex_transcript_has_fork_ancestor(
    agent: SessionChatTranscriptAgent,
    file_path: &Path,
) -> bool {
    codex_fork_lineage(agent, file_path).is_some_and(|lineage| lineage.forked_from_id.is_some())
}

// ---------------------------------------------------------------------------
// Stitched page read
// ---------------------------------------------------------------------------

/// Head budget for the forked file's opening `session_meta` when only its
/// timestamp is needed; same size the successor proof uses for the full record.
const FORK_BOUNDARY_SESSION_META_HEAD_BYTES: u64 = 128 * 1024;

/// Timestamp of a rollout's opening `session_meta` record, in ms. Used only as
/// the divider's fallback ordering key when the forked file carries no
/// timestamped message yet.
fn session_meta_timestamp_ms(path: &Path) -> Option<i64> {
    let head = read_transcript_head_complete_lines(path, FORK_BOUNDARY_SESSION_META_HEAD_BYTES)?;
    let record = parse_json_object(head.lines().next()?)?;
    if record.get("type").and_then(Value::as_str) != Some("session_meta") {
        return None;
    }
    timestamp_ms(record.get("timestamp"))
}

/*
CDXC:SessionFork 2026-08-28 (divider ordering):
The client assembler sorts by timestamp and maps a null timestamp to -Infinity,
which threw a timestamp-less divider to the very top of the whole thread instead
of the fork seam. The divider therefore carries a synthesized timestamp: the
forked file's first timestamped message minus 1ms, else that file's own
`session_meta` timestamp. Both inputs are frozen (an ancestor rollout is dead,
and even the live rollout's opening rows never change), so the value is
deterministic across reads — the merger replaces same-id rows in place, and a
drifting timestamp would make the one divider jump around between pages.
A "newest ancestor row + 1ms" fallback was deliberately NOT used: which ancestor
rows are in view depends on the request window, so it could not be deterministic.
*/
fn fork_boundary_timestamp_ms(
    forked_file_rows: &[SessionChatMessage],
    forked_file_path: &Path,
) -> Option<i64> {
    forked_file_rows
        .iter()
        .find_map(|message| message.timestamp)
        .map(|first_message_ms| first_message_ms - 1)
        .or_else(|| session_meta_timestamp_ms(forked_file_path))
}

fn fork_boundary_message(
    session_id: &str,
    ancestor_available: bool,
    timestamp: Option<i64>,
) -> SessionChatMessage {
    SessionChatMessage {
        // Deterministic across reads: the merger dedups by id, so paging over
        // the same boundary twice must not produce two dividers.
        id: format!("{FORK_BOUNDARY_MESSAGE_ID_PREFIX}{session_id}"),
        role: SessionChatRole::System,
        blocks: vec![SessionChatBlock::Text {
            text: if ancestor_available {
                FORK_BOUNDARY_TEXT_SHARED.to_string()
            } else {
                FORK_BOUNDARY_TEXT_UNAVAILABLE.to_string()
            },
        }],
        timestamp,
        source: SessionChatSource::Transcript,
        turn_id: None,
        byte_offset: Some(0),
        queued: false,
    }
}

#[derive(Debug)]
pub struct SessionChatStitchedPage {
    pub page: SessionChatTailPage,
    /// Present only when the current rollout declares a `forked_from_id`.
    pub fork_info: Option<SessionChatForkInfo>,
}

fn unstitched(
    agent: SessionChatTranscriptAgent,
    file_path: &Path,
    limit: usize,
    before_offset: Option<u64>,
) -> std::io::Result<SessionChatStitchedPage> {
    Ok(SessionChatStitchedPage {
        page: read_session_chat_tail_page(agent, file_path, limit, before_offset)?,
        fork_info: None,
    })
}

/*
Pagination across the fork lineage. Reads the hop named by the cursor, and keeps
walking to older ancestors while the page is short and the chain continues, so
one request answers with `limit` real rows no matter how many files they span.

`hasMore` is true when the current hop's file still holds older rows OR a
loadable ancestor remains; it is false at the root of the lineage and at a
boundary whose ancestor is gone, which is exactly where the divider says so.
*/
pub fn read_session_chat_tail_page_stitched(
    agent: SessionChatTranscriptAgent,
    file_path: &Path,
    limit: usize,
    before_offset: Option<u64>,
) -> std::io::Result<SessionChatStitchedPage> {
    if limit == 0 {
        return unstitched(agent, file_path, limit, before_offset);
    }
    let Some(lineage) =
        codex_fork_lineage(agent, file_path).filter(|lineage| lineage.forked_from_id.is_some())
    else {
        return unstitched(agent, file_path, limit, before_offset);
    };
    let fork_info = lineage.fork_info();

    let (mut hop, mut window_end) = match before_offset {
        Some(cursor) => {
            let (hop, offset) = decode_stitched_cursor(cursor);
            (hop, Some(offset))
        }
        None => (0usize, None),
    };
    let mut messages: Vec<SessionChatMessage> = Vec::new();
    let mut row_count = 0usize;
    let mut lifecycle = None;
    let mut result_cursor = before_offset.unwrap_or(0);
    let mut has_more = false;
    let mut first_read = true;

    loop {
        let hop_path = if hop == 0 {
            Some(file_path)
        } else {
            lineage.ancestor_path(hop)
        };
        let Some(hop_path) = hop_path else {
            break;
        };
        let page = read_session_chat_tail_page(
            agent,
            hop_path,
            limit.saturating_sub(row_count),
            window_end,
        )?;
        let (mut chunk, page_lifecycle, file_has_more, page_before_offset) = match page {
            SessionChatTailPage::NotFound => {
                if first_read {
                    return Ok(SessionChatStitchedPage {
                        page: SessionChatTailPage::NotFound,
                        fork_info,
                    });
                }
                // An ancestor that vanished between the lineage walk and this
                // read: the chain simply ends where it is readable.
                break;
            }
            SessionChatTailPage::Page {
                messages,
                lifecycle,
                has_more,
                before_offset,
            } => (messages, lifecycle, has_more, before_offset),
        };
        if first_read {
            // Only a live tail read of the current file may carry lifecycle;
            // `read_session_chat_tail_page` already suppresses it for a
            // pagination window, and ancestor reads are history by definition.
            lifecycle = page_lifecycle;
            first_read = false;
        }
        row_count += chunk.len();
        result_cursor = encode_stitched_cursor(hop, page_before_offset);

        if file_has_more {
            has_more = true;
            chunk.extend(messages);
            messages = chunk;
            break;
        }
        // The window reached this file's first record, so its boundary (if any)
        // is visible and gets its divider.
        let boundary_session_id = lineage
            .forked_from_at(hop)
            .and(lineage.session_id_at(hop))
            .map(str::to_string);
        let ancestor_path_exists = lineage.ancestor_path(hop + 1).is_some();
        if let Some(boundary_session_id) = boundary_session_id {
            let boundary_timestamp = fork_boundary_timestamp_ms(&chunk, hop_path);
            chunk.insert(
                0,
                fork_boundary_message(
                    &boundary_session_id,
                    ancestor_path_exists,
                    boundary_timestamp,
                ),
            );
        } else {
            // Root of the lineage: nothing older exists at all.
            chunk.extend(messages);
            messages = chunk;
            break;
        }
        chunk.extend(messages);
        messages = chunk;
        if !ancestor_path_exists {
            break;
        }
        if row_count >= limit {
            has_more = true;
            break;
        }
        hop += 1;
        window_end = None;
    }

    Ok(SessionChatStitchedPage {
        page: SessionChatTailPage::Page {
            messages,
            lifecycle,
            has_more,
            before_offset: result_cursor,
        },
        fork_info,
    })
}
