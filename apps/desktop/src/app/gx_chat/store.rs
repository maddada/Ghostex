//! The retained chats: one `ChatCore` per session, and the four limits that decide how many.
//!
//! CDXC:SessionChat 2026-09-22 WHY:
//! One core per SESSION rather than one brain per open view, which is what removes the warm pool.
//! Booting a QuickJS chat view cost about 300 ms against about 25 ms for a warm paint, so
//! `session_chat_prewarm.rs` kept 24 of them alive and `session_chat_warm_pool.rs` evicted them by
//! last render. A retained core in this map costs neither, so switching chats is a lookup: the
//! folded transcript, the pagination window and the read clocks are already there.
//!
//! The limits are the deleted QuickJS retained store's (`store.ts`), which the user approved:
//! chat data and live subscriptions are retained independently of mounted views.

use std::collections::BTreeMap;

use ghostex_gx_chat_core::ChatCore;
use web_time::Instant;

use super::identity::ChatIdentity;

/// `MAX_RETAINED_SESSIONS`.
const MAX_RETAINED_SESSIONS: usize = 12;
/// `IDLE_RETENTION_MS`: how long a chat nobody is watching stays folded.
const IDLE_RETENTION: std::time::Duration = std::time::Duration::from_secs(5 * 60);
/// `MAX_RETAINED_MESSAGES`.
const MAX_RETAINED_MESSAGES: usize = 1_200;
/// `MAX_RETAINED_BYTES`.
const MAX_RETAINED_BYTES: usize = 4 * 1024 * 1024;
/// How often an idle chat's size is re-measured. Measuring it is a walk of the whole transcript.
const SIZE_RECHECK: std::time::Duration = std::time::Duration::from_secs(5);

/// Counts the UTF-16 code units of the JSON written to it without keeping any of it, and stops the
/// walk once `limit` is passed.
///
/// CDXC:SessionChat 2026-10-23 WHY: the size check used to build the document's whole JSON string
/// and then its UTF-16 copy (megabytes on a long chat) every five seconds per idle chat, only to
/// count them.
struct Utf16Count {
    units: usize,
    limit: usize,
}

impl std::io::Write for Utf16Count {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        for &byte in bytes {
            // Every UTF-8 lead byte starts one UTF-16 unit; a four-byte sequence is a surrogate
            // pair. serde_json only writes valid UTF-8.
            if byte & 0xC0 != 0x80 {
                self.units += 1;
            }
            if byte >= 0xF0 {
                self.units += 1;
            }
        }
        if self.units > self.limit {
            return Err(std::io::ErrorKind::FileTooLarge.into());
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Whether what a retained chat holds passes `MAX_RETAINED_BYTES`, in the UTF-16 bytes the
/// TypeScript store measured its snapshot in.
///
/// CDXC:SessionChat 2026-10-23 WHY: only the document used to be measured, which carries no
/// transcript rows, so the cap never tripped. The transcript is held several times over: the
/// authoritative list, the composed list, the retained fold, each message's projection and the
/// built rows. All of them are counted, except that a built row shares its message's cached model
/// (2026-10-24, `ProjectedMessage` in gx-chat-core), so the cache's models are not counted twice.
fn retained_past_limit(core: &ChatCore) -> bool {
    let state = core.state();
    let mut count = Utf16Count {
        units: 0,
        limit: MAX_RETAINED_BYTES / 2,
    };
    let walked = (|| -> serde_json::Result<()> {
        serde_json::to_writer(&mut count, core.document())?;
        serde_json::to_writer(&mut count, &state.messages.list)?;
        serde_json::to_writer(&mut count, &state.messages.composed)?;
        serde_json::to_writer(&mut count, &state.messages.snapshot)?;
        serde_json::to_writer(&mut count, &*state.transcript_view.items)?;
        serde_json::to_writer(&mut count, &state.transcript_view.deferred)?;
        for projected in state.transcript_view.projected.values() {
            serde_json::to_writer(&mut count, &projected.source)?;
            if let Some(arrived) = &projected.arrived {
                serde_json::to_writer(&mut count, arrived)?;
            }
        }
        Ok(())
    })();
    // A value that does not serialize at all says nothing about its size, as before.
    walked.is_err() && count.units > count.limit
}

/// What decides whether a retained chat could have changed size since it was last measured.
#[derive(Clone, Copy, PartialEq)]
struct SizeFingerprint {
    revision: u64,
    /// The built rows' allocation, which a rebuild replaces.
    items: usize,
    messages: usize,
    composed: usize,
    projected: usize,
}

impl SizeFingerprint {
    fn of(core: &ChatCore) -> Self {
        let state = core.state();
        Self {
            revision: core.revision(),
            items: std::sync::Arc::as_ptr(&state.transcript_view.items) as usize,
            messages: state.messages.list.len(),
            composed: state.messages.composed.len(),
            projected: state.transcript_view.projected.len(),
        }
    }
}

/// One retained chat.
pub(super) struct Retained {
    /// Who this chat is, which the chat socket follows it by.
    pub(super) identity: ChatIdentity,
    pub(super) core: ChatCore,
    /// The storage session key, built once because every record's suffix starts from it.
    pub(super) session_key: String,
    /// How many views are watching. A chat with none is a prune candidate.
    pub(super) listeners: usize,
    /// When a view last called into it, which is what the LRU and the idle expiry read.
    pub(super) touched_at: Instant,
    /// When this chat's size was last measured, what it measured, and whether it was too big.
    measured: Option<(Instant, SizeFingerprint, bool)>,
}

impl Retained {
    fn new(identity: ChatIdentity) -> Self {
        let session_key = identity.storage_session_key();
        Self {
            identity,
            core: ChatCore::new(),
            session_key,
            listeners: 0,
            touched_at: Instant::now(),
            measured: None,
        }
    }

    /// Whether this chat has outgrown what the store keeps.
    ///
    /// The size is measured at most every five seconds, because it is a walk of the whole
    /// transcript, and not again while nothing it holds has changed.
    fn oversized(&mut self) -> bool {
        if self.core.state().messages.composed.len() > MAX_RETAINED_MESSAGES {
            return true;
        }
        let fingerprint = SizeFingerprint::of(&self.core);
        if let Some((at, measured, oversized)) = self.measured
            && (at.elapsed() < SIZE_RECHECK || measured == fingerprint)
        {
            return oversized;
        }
        // UTF-16 accounting, the same conservative measure the TypeScript took of its snapshot.
        let oversized = retained_past_limit(&self.core);
        self.measured = Some((Instant::now(), fingerprint, oversized));
        oversized
    }
}

/// Every retained chat, keyed by `JSON.stringify([machineId, projectId, sessionId])`.
#[derive(Default)]
pub(super) struct ChatStore {
    retained: BTreeMap<String, Retained>,
    /// Chats dropped by a limit since the last summary.
    pub(super) evicted: u64,
}

impl ChatStore {
    /// The chat for an identity, created on first sight.
    pub(super) fn entry(&mut self, identity: &ChatIdentity) -> &mut Retained {
        self.retained
            .entry(identity.retention_key())
            .or_insert_with(|| Retained::new(identity.clone()))
    }

    pub(super) fn get_mut(&mut self, key: &str) -> Option<&mut Retained> {
        self.retained.get_mut(key)
    }

    pub(super) fn get(&self, key: &str) -> Option<&Retained> {
        self.retained.get(key)
    }

    /// Every retained chat's key, for a push that reaches them all.
    pub(super) fn keys(&self) -> Vec<String> {
        self.retained.keys().cloned().collect()
    }

    pub(super) fn len(&self) -> usize {
        self.retained.len()
    }

    /// Drops one chat, whatever its limits say. The caller owns the reason.
    pub(super) fn remove(&mut self, key: &str) {
        self.retained.remove(key);
    }

    /// Drops what the four limits no longer keep, and names what it dropped.
    ///
    /// A chat with a listener is never dropped, whatever its size: the view attached to it is
    /// drawing from it. An oversized chat is DISPOSED rather than sliced, because slicing the
    /// transcript would leave the server's pagination cursor pointing at a row that is gone.
    ///
    /// The answer is the retention key of each dropped chat, because the maps BESIDE this one are
    /// keyed by it and a chat's entry in them must not outlive the chat: the timers, the retry
    /// worker, the park answer, the held requests and the delivery ids are all per chat. The saves
    /// in flight are deliberately not among them (`worker.rs`'s `purge`).
    pub(super) fn prune(&mut self) -> Vec<String> {
        let mut dropped: Vec<String> = Vec::new();
        let mut expired: Vec<String> = Vec::new();
        for (key, retained) in self.retained.iter_mut() {
            if retained.listeners > 0 {
                continue;
            }
            if retained.touched_at.elapsed() >= IDLE_RETENTION || retained.oversized() {
                expired.push(key.clone());
            }
        }
        for key in expired {
            if self.retained.remove(&key).is_some() {
                dropped.push(key);
            }
            self.evicted += 1;
        }
        while self.retained.len() > MAX_RETAINED_SESSIONS {
            let Some(oldest) = self
                .retained
                .iter()
                .filter(|(_, retained)| retained.listeners == 0)
                .min_by_key(|(_, retained)| retained.touched_at)
                .map(|(key, _)| key.clone())
            else {
                // Every retained chat has a view watching it. The cap is a retention limit, not a
                // reason to tear down a chat somebody is looking at.
                break;
            };
            if self.retained.remove(&oldest).is_some() {
                dropped.push(oldest);
            }
            self.evicted += 1;
        }
        dropped
    }
}
