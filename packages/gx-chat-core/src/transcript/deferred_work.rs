//! Reading a completed turn's collapsed work back out of history.
//!
//! Port of `readWork` in `packages/shared/session-chat-presentation/deferred-work.ts`. The
//! TypeScript was one `async` loop that kept asking for the page before the last until it met
//! the section's first message; the core owns no I/O, so the loop is turned inside out: one page
//! per [`crate::Effect::SendRpc`], with the walk's cursor, its seen set and the rows it has
//! collected carried on [`DeferredWalk`] between answers.
//!
//! The LRU cache is load bearing and is ported with it: reopening a section the user already
//! expanded must not walk history again, which on a long conversation is several round trips.

use serde_json::Value;

use crate::jsnum::js_safe_integer;

use crate::wire::ChatMessage;

/// How many sections the cache holds, and how many bytes of them.
///
/// `cache.size > 8 || total > 2 * 1024 * 1024`, measured as `JSON.stringify(messages).length * 2`
/// (UTF-16 code units, two bytes each), which is what the TypeScript charges a section.
pub const DEFERRED_WORK_CACHE_ENTRIES: usize = 8;
/// The cache's byte budget, and also the largest section it will hold at all.
pub const DEFERRED_WORK_CACHE_BYTES: usize = 2 * 1024 * 1024;

/// How many messages one page of the walk asks for.
pub const DEFERRED_WORK_PAGE_LIMIT: i64 = 200;

/// The section changed under the reader: its first message arrived before its last.
pub const WORK_SECTION_CHANGED: &str = "This work section changed. Refresh the conversation.";
/// The walk ran out of pages without meeting the section's first message.
pub const WORK_SECTION_GONE: &str =
    "The original work section is no longer available. Refresh the conversation.";
/// A page came back with an error status and no message of its own.
pub const WORK_HISTORY_UNREADABLE: &str = "Work history could not be loaded.";

/// One `loadWork` walk in flight.
#[derive(Clone, Debug, PartialEq)]
pub struct DeferredWalk {
    /// The turn's user-message id, which is what the row and the answer are keyed by.
    pub turn_id: String,
    /// `JSON.stringify([beforeOffset, startId, endId])`, the cache key.
    pub key: String,
    pub start_id: String,
    pub end_id: String,
    /// The byte offset the next page is read before.
    pub cursor: i64,
    /// The section's LAST message has been seen, so everything from here on belongs to it.
    pub found_end: bool,
    /// The rows collected so far, newest first until the walk finishes and reverses them.
    pub messages: Vec<ChatMessage>,
    /// Every cursor already asked for, so a server that repeats an offset ends the walk.
    pub seen: Vec<i64>,
}

/// What one answered page told the walk to do next.
pub enum WalkStep {
    /// Ask for the page before `cursor`.
    ReadPage { cursor: i64 },
    /// The section is complete, newest-first order already undone.
    Done { messages: Vec<ChatMessage> },
    /// The walk cannot finish, and this is the message the row shows.
    Failed { message: String },
}

impl DeferredWalk {
    /// Starts a walk for one `work` descriptor, or `None` when it carries no offset.
    ///
    /// A malformed descriptor is where the TypeScript read `work.beforeOffset` off `null` and
    /// threw; the core answers the row with [`WORK_HISTORY_UNREADABLE`] instead of panicking,
    /// because a host is free to send anything and a panic here takes the host thread with it.
    pub fn begin(turn_id: String, work: Option<&Value>) -> Option<Self> {
        let work = work?;
        // A JSON number, read the way JavaScript reads it: `1234.0` is 1234. `Value::as_i64`
        // refused that token and drew the row as unreadable where `deferred-work.ts` walks on.
        let cursor = js_safe_integer(work.get("beforeOffset"))?;
        let start_id = work
            .get("startId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let end_id = work
            .get("endId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        Some(Self {
            turn_id,
            key: cache_key(cursor, &start_id, &end_id),
            start_id,
            end_id,
            cursor,
            found_end: false,
            messages: Vec::new(),
            seen: vec![cursor],
        })
    }

    /// Folds one answered page in and says what the walk does next.
    ///
    /// `for (const message of [...page.messages].reverse())`: the page arrives oldest first, so it
    /// is read backwards. The section's LAST message opens the collection and its FIRST closes it;
    /// meeting the first one before the last means the rows moved under the reader.
    pub fn advance(&mut self, page: &Value) -> WalkStep {
        if page.get("status").and_then(Value::as_str) == Some("error") {
            return WalkStep::Failed {
                message: page
                    .get("error")
                    .and_then(Value::as_str)
                    .filter(|error| !error.is_empty())
                    .unwrap_or(WORK_HISTORY_UNREADABLE)
                    .to_string(),
            };
        }
        let rows = page
            .get("messages")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for row in rows.iter().rev() {
            let id = row.get("id").and_then(Value::as_str).unwrap_or_default();
            if !self.start_id.is_empty() && id == self.start_id {
                if !self.found_end {
                    return WalkStep::Failed {
                        message: WORK_SECTION_CHANGED.to_string(),
                    };
                }
                let mut messages = std::mem::take(&mut self.messages);
                messages.reverse();
                return WalkStep::Done { messages };
            }
            if !self.end_id.is_empty() && id == self.end_id {
                self.found_end = true;
            }
            if self.found_end {
                if let Ok(message) = serde_json::from_value::<ChatMessage>(row.clone()) {
                    self.messages.push(message);
                }
            }
        }
        if page.get("hasMore").and_then(Value::as_bool) != Some(true) {
            return WalkStep::Failed {
                message: WORK_SECTION_GONE.to_string(),
            };
        }
        let Some(cursor) = js_safe_integer(page.get("beforeOffset")) else {
            return WalkStep::Failed {
                message: WORK_SECTION_GONE.to_string(),
            };
        };
        if self.seen.contains(&cursor) {
            return WalkStep::Failed {
                message: WORK_SECTION_GONE.to_string(),
            };
        }
        self.seen.push(cursor);
        self.cursor = cursor;
        WalkStep::ReadPage { cursor }
    }
}

/// `JSON.stringify([work.beforeOffset, work.startId, work.endId])`.
fn cache_key(before_offset: i64, start_id: &str, end_id: &str) -> String {
    serde_json::to_string(&(before_offset, start_id, end_id)).unwrap_or_default()
}

/// One section the cache is holding, with what it costs.
#[derive(Clone, Debug, PartialEq)]
pub struct CachedWork {
    pub key: String,
    pub messages: Vec<ChatMessage>,
    pub bytes: usize,
}

/// The walked sections, oldest use first, exactly as the TypeScript's insertion-ordered `Map` was.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeferredWorkCache {
    entries: Vec<CachedWork>,
}

impl DeferredWorkCache {
    /// A hit, moved to the end so the next eviction takes something older.
    pub fn take_fresh(&mut self, key: &str) -> Option<Vec<ChatMessage>> {
        let at = self.entries.iter().position(|entry| entry.key == key)?;
        let entry = self.entries.remove(at);
        let messages = entry.messages.clone();
        self.entries.push(entry);
        Some(messages)
    }

    /// Stores a walked section, unless it alone is over the budget.
    pub fn store(&mut self, key: &str, messages: &[ChatMessage]) {
        let bytes = serialized_bytes(messages);
        if bytes > DEFERRED_WORK_CACHE_BYTES {
            return;
        }
        self.entries.retain(|entry| entry.key != key);
        self.entries.push(CachedWork {
            key: key.to_string(),
            messages: messages.to_vec(),
            bytes,
        });
        let mut total: usize = self.entries.iter().map(|entry| entry.bytes).sum();
        while self.entries.len() > DEFERRED_WORK_CACHE_ENTRIES || total > DEFERRED_WORK_CACHE_BYTES
        {
            if self.entries.is_empty() {
                break;
            }
            total = total.saturating_sub(self.entries[0].bytes);
            self.entries.remove(0);
        }
    }

    /// `invalidateDeferredSessionChatWork`: a transcript that was replaced rather than extended
    /// invalidates every section, because the offsets they were walked from are gone.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// `JSON.stringify(messages).length * 2`: UTF-16 code units, two bytes each.
fn serialized_bytes(messages: &[ChatMessage]) -> usize {
    serde_json::to_string(messages)
        .map(|text| text.chars().map(|character| character.len_utf16()).sum())
        .unwrap_or(0)
        * 2
}
