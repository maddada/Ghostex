//! The mailbox and the snapshot store: memory only, never written to disk, never parsed.
//!
//! An entry is filed under the SHA-256 of its id, so the relay holds no id itself. A mailbox entry (an activity's
//! new update token) is taken once and lives at most `mailbox_lifetime`; a snapshot (an encrypted blob for the
//! widgets) is kept until replaced or `snapshot_lifetime`. Both share one cap on entries and bytes. The relay cannot
//! tell a paired phone's id from anyone else's, so these caps, and the caller's quota for new ids that the route
//! checks through `put`, are what bound the memory a caller can make it hold.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::limits::LimitConfig;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shelf {
    Mailbox,
    Snapshot,
}

impl Shelf {
    pub fn as_str(self) -> &'static str {
        match self {
            Shelf::Mailbox => "mailbox",
            Shelf::Snapshot => "snapshot",
        }
    }
}

/// Why `put` stored nothing.
#[derive(Debug, PartialEq, Eq)]
pub enum PutRefusal {
    /// The caller may create no new id for this long.
    Quota(Duration),
    /// The store is full even without its expired entries.
    Full,
}

struct Entry {
    value: Vec<u8>,
    stored: Instant,
}

#[derive(Default)]
struct Shelves {
    mailbox: HashMap<[u8; 32], Entry>,
    snapshots: HashMap<[u8; 32], Entry>,
    /// Bytes of every value held, both shelves.
    bytes: usize,
}

impl Shelves {
    fn shelf(&mut self, shelf: Shelf) -> &mut HashMap<[u8; 32], Entry> {
        match shelf {
            Shelf::Mailbox => &mut self.mailbox,
            Shelf::Snapshot => &mut self.snapshots,
        }
    }

    fn entries(&self) -> usize {
        self.mailbox.len() + self.snapshots.len()
    }
}

pub struct Store {
    shelves: Mutex<Shelves>,
    max_entries: usize,
    max_bytes: usize,
    mailbox_lifetime: Duration,
    snapshot_lifetime: Duration,
}

/// What the store holds, for the health check.
pub struct StoreCounts {
    pub mailbox: usize,
    pub snapshots: usize,
    pub bytes: usize,
}

impl Store {
    pub fn new(limits: &LimitConfig) -> Self {
        Store {
            shelves: Mutex::new(Shelves::default()),
            max_entries: limits.store_max_entries,
            max_bytes: limits.store_max_bytes,
            mailbox_lifetime: limits.mailbox_lifetime,
            snapshot_lifetime: limits.snapshot_lifetime,
        }
    }

    fn lifetime(&self, shelf: Shelf) -> Duration {
        match shelf {
            Shelf::Mailbox => self.mailbox_lifetime,
            Shelf::Snapshot => self.snapshot_lifetime,
        }
    }

    fn drop_expired(&self, shelves: &mut Shelves, now: Instant) {
        let mut freed = 0;
        for shelf in [Shelf::Mailbox, Shelf::Snapshot] {
            let lifetime = self.lifetime(shelf);
            shelves.shelf(shelf).retain(|_, entry| {
                let keep = now.saturating_duration_since(entry.stored) < lifetime;
                if !keep {
                    freed += entry.value.len();
                }
                keep
            });
        }
        shelves.bytes -= freed;
    }

    /// Removes `key`'s entry from `shelf` when it has expired.
    fn expire(&self, shelves: &mut Shelves, shelf: Shelf, key: &[u8; 32], now: Instant) {
        let lifetime = self.lifetime(shelf);
        let expired = shelves
            .shelf(shelf)
            .get(key)
            .is_some_and(|entry| now.saturating_duration_since(entry.stored) >= lifetime);
        if expired {
            if let Some(entry) = shelves.shelf(shelf).remove(key) {
                shelves.bytes -= entry.value.len();
            }
        }
    }

    /// Files `value` under `key`, replacing what is there. Only an id the store does not hold calls `admit_new`
    /// (the caller's quota for new ids), before anything is stored; a full store refuses a new id, and a
    /// replacement that would grow it past its byte cap, after dropping expired entries.
    pub fn put(
        &self,
        shelf: Shelf,
        key: [u8; 32],
        value: Vec<u8>,
        now: Instant,
        admit_new: impl FnOnce() -> Result<(), Duration>,
    ) -> Result<(), PutRefusal> {
        let mut shelves = self.shelves.lock().expect("store lock");
        self.expire(&mut shelves, shelf, &key, now);
        let held = shelves
            .shelf(shelf)
            .get(&key)
            .map(|entry| entry.value.len());
        if held.is_none() {
            admit_new().map_err(PutRefusal::Quota)?;
        }
        let fits = |shelves: &Shelves| {
            let entries = shelves.entries() + usize::from(held.is_none());
            let bytes = shelves.bytes - held.unwrap_or(0) + value.len();
            entries <= self.max_entries && bytes <= self.max_bytes
        };
        if !fits(&shelves) {
            self.drop_expired(&mut shelves, now);
            if !fits(&shelves) {
                return Err(PutRefusal::Full);
            }
        }
        shelves.bytes = shelves.bytes - held.unwrap_or(0) + value.len();
        shelves
            .shelf(shelf)
            .insert(key, Entry { value, stored: now });
        Ok(())
    }

    /// The mailbox entry under `key`, removed as it is read.
    pub fn take_mailbox(&self, key: &[u8; 32], now: Instant) -> Option<Vec<u8>> {
        let mut shelves = self.shelves.lock().expect("store lock");
        self.expire(&mut shelves, Shelf::Mailbox, key, now);
        let entry = shelves.mailbox.remove(key)?;
        shelves.bytes -= entry.value.len();
        Some(entry.value)
    }

    /// The snapshot under `key`, which stays.
    pub fn snapshot(&self, key: &[u8; 32], now: Instant) -> Option<Vec<u8>> {
        let mut shelves = self.shelves.lock().expect("store lock");
        self.expire(&mut shelves, Shelf::Snapshot, key, now);
        shelves.snapshots.get(key).map(|entry| entry.value.clone())
    }

    /// Drops expired entries; run once a minute.
    pub fn prune(&self, now: Instant) {
        let mut shelves = self.shelves.lock().expect("store lock");
        self.drop_expired(&mut shelves, now);
    }

    pub fn counts(&self) -> StoreCounts {
        let shelves = self.shelves.lock().expect("store lock");
        StoreCounts {
            mailbox: shelves.mailbox.len(),
            snapshots: shelves.snapshots.len(),
            bytes: shelves.bytes,
        }
    }
}
