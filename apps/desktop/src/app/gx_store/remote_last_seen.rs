//! The last-seen presentation of a remote machine: the rows the sidebar draws, faded, for a
//! machine that has not connected in this run, and the copy this app keeps so the next run has
//! them.
//!
//! CDXC:RemoteMachines 2026-09-21 DECISION:
//! Asked "today, when a remote machine is offline, the old sidebar still shows its sessions from
//! the last time it connected, greyed out; the Rust sidebar cannot do that yet, so an offline
//! machine shows nothing until it connects. Do you want that last-seen view kept?", the user
//! answered "yes pls". This file is the store's half of keeping it: the read that seeds a machine
//! at launch, and the write that keeps the copy fresh.
//!
//! **A different table from every other Rust storage door, and that is the whole reason this file
//! exists.** The three sidebar keys and the three client-owned documents are `local` catalog rows
//! and live in the `preferences` table as `(key, value)` with `value` the raw string. This key is
//! catalogued on the **indexeddb** backend (`remotePresentations` in
//! `packages/client-storage/catalog.ts` takes `cache`, which takes `disk`, which sets
//! `backend: 'indexeddb'`), so it lives in the **`records`** table as a whole row object with the
//! payload under `raw`. `records_storage.rs` is that door; pointing `read_preference_value` at this
//! key would find nothing, for ever, and look like a machine that had simply never connected.
//!
//! The key is one per machine, `ghostex-gpui-remote-last-seen-presentations:machine:<encoded id>`,
//! and the value is a `GxserverPresentationSnapshot`: the same shape the live stream and
//! `/api/readPresentationSnapshot` deliver, which is why gx-core needs no codec of its own for it
//! and why a build from before this port still reads what this one wrote.
//!
//! This is the only writer since 2026-09-25: the old runtime still READS the key for its own rows,
//! and a machine removed from Settings loses its copy in `remote_last_seen_prune.rs`.
//!
//! SEE-ALSO: packages/gx-core/src/presentation_store/snapshot_out.rs
//! (the store written back out as one snapshot), packages/gx-core/src/presentation_store/apply.rs
//! (`seed_last_seen`, and the revision rules that make these rows "held, not live").

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use ghostex_gx_core::{MachineId, encode_uri_component, snapshot_storage_json};

use super::records_storage::{RecordRead, RecordStore, RecordWrite, read_record_raw, write_record};
use super::remote_clients::RemoteClientCounters;
use crate::GhostexGpuiApp;

/// How often the summary line repeats, the same interval `gxStore.sidebarShadow.summary` uses.
const SUMMARY_INTERVAL: Duration = Duration::from_secs(60);

/// The age at which the catalog stops answering with a `remotePresentations` row: the `cache`
/// base's `maxAgeMs`, 30 days (`packages/client-storage/catalog.ts`). A machine nobody has
/// connected to in a month draws nothing rather than a month-old list, which is what the
/// TypeScript reader of this key already does.
const CACHE_MAX_AGE_MS: i64 = 30 * 24 * 60 * 60 * 1_000;

/// The catalog row (`remotePresentations`, `packages/client-storage/catalog.ts`): the `cache` base
/// on the indexeddb backend, with its own entry, store and entry-count bounds and that base's age
/// limit.
pub(super) const STORE: RecordStore = RecordStore {
    id: "remotePresentations",
    version: 1,
    max_entry_bytes: 8 * 1024 * 1024,
    max_bytes: 24 * 1024 * 1024,
    max_entries: 32,
    max_age_ms: Some(CACHE_MAX_AGE_MS),
};

/// The catalog row's `key` prefix, with the per-machine infix `RemoteLastSeenStore` appends.
/// `GPUI_REMOTE_LAST_SEEN_PRESENTATIONS_STORAGE_KEY` in the deleted
/// `apps/desktop/sidebar/gxserver-runtime/constants.ts`, then `:machine:`.
pub(super) const MACHINE_KEY_PREFIX: &str = "ghostex-gpui-remote-last-seen-presentations:machine:";

/// `GPUI_REMOTE_LAST_SEEN_PRESENTATIONS_PERSIST_DELAY_MS`, the delay the old runtime's writer
/// batched behind before this became the only writer.
const WRITE_DELAY: Duration = Duration::from_millis(2_000);
/// A write that lost the lock race is owed again, a bounded number of times, and the quit path
/// flushes whatever is still owed.
const WRITE_RETRY: Duration = Duration::from_millis(400);
const MAX_WRITE_RETRIES: u32 = 3;

/// The stored key of one machine's last-seen snapshot.
fn machine_key(machine_id: &str) -> String {
    format!("{MACHINE_KEY_PREFIX}{}", encode_uri_component(machine_id))
}

/// The raw payload of one machine's last-seen snapshot, if one is stored and still fresh enough
/// for the catalog to answer with it.
pub(super) fn read_last_seen_raw(
    machine_id: &str,
    now_ms: i64,
) -> Result<RecordRead, &'static str> {
    read_record_raw(STORE, &machine_key(machine_id), now_ms)
}

/// What this run did with the stored copies. Memory only; the record lines are built from it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RemoteLastSeenCounters {
    /// Machines whose rows moved since the last write, so a write was booked.
    pub(crate) dirtied: u64,
    /// Every call into the door, retries and the quit flush included.
    pub(crate) attempts: u64,
    /// Payloads that reached the database.
    pub(crate) writes: u64,
    /// Payloads the stored row already held byte for byte, so nothing was written. High beside a
    /// low `writes` is the two writers agreeing, which is what makes both of them writing safe.
    pub(crate) unchanged: u64,
    /// Payloads a storage bound refused. Not retried: the payload does not shrink by trying again.
    pub(crate) refusals: u64,
    pub(crate) failures: u64,
    /// The largest payload this run built, in bytes. The catalog admits 8 MiB per entry.
    pub(crate) largest_bytes: u64,
    /// The longest time spent building one payload on the UI thread, in microseconds. The write
    /// itself runs on the background executor; this is the part that cannot.
    pub(crate) build_max_us: u64,
}

/// The write side's state: which machines owe a copy, and what is still unwritten.
#[derive(Default)]
pub(crate) struct RemoteLastSeenWriter {
    /// The revision of the snapshot each machine's copy was last built from. A machine whose
    /// revision has not moved has nothing new to store, and a reload that resets the daemon's
    /// counter moves it downwards, which is a change too: the test is inequality.
    written_revision: BTreeMap<String, i64>,
    /// Machines whose copy is owed a build. Ids only: the payload is built when the debounce
    /// fires, because building it clones every row of the machine.
    dirty: BTreeSet<String>,
    /// Payloads that have been built and have not reached the database yet.
    owed: BTreeMap<String, String>,
    retries: u32,
    /// Bumped by every booking, so a fired timer of a booking that was replaced does nothing.
    booking: u64,
    booked: bool,
    /// The periodic summary's throttle. Here rather than beside the other summaries' in
    /// `diagnostics.rs`, which is over the size ceiling and waiting for a quiet window.
    summary_at: Option<Instant>,
    summary_written: Option<(RemoteClientCounters, RemoteLastSeenCounters)>,
    pub(crate) counters: RemoteLastSeenCounters,
}

impl GhostexGpuiApp {
    /// Both halves' counters, on the periodic path the sidebar shadow's summary rides, so a run
    /// with no remote machine at all still says so with every counter at zero.
    pub(super) fn gx_store_last_seen_summary(&mut self) {
        let read = self.gx_store.remote.counters;
        let write = self.gx_store.last_seen.counters;
        let writer = &mut self.gx_store.last_seen;
        if writer
            .summary_written
            .is_some_and(|written| written == (read, write))
            || writer
                .summary_at
                .is_some_and(|at| at.elapsed() < SUMMARY_INTERVAL)
        {
            return;
        }
        writer.summary_at = Some(Instant::now());
        writer.summary_written = Some((read, write));
        self.gx_store
            .diagnostics
            .remote_last_seen_summary(read, write);
    }

    /// A remote machine's rows moved, so its stored copy owes an update.
    ///
    /// CDXC:RemoteMachines 2026-09-21 WHY:
    /// **A Rust write was invisible to a client-storage service that was already running.** The
    /// service kept its own in-memory `rows` map and only re-read the database on a resync
    /// (`window.focus` and `pageshow`, `packages/client-storage/service.ts`). That was harmless
    /// because the one TypeScript reader of this key was `RemoteLastSeenStore.read()`, at the
    /// deleted sidebar page's construction, and the one lossy case (a mutation the page queued
    /// before a Rust write and drained after it) was bounded to a slightly older snapshot of the
    /// SAME machine from the SAME stream, which the "both sides write until the TypeScript writer is
    /// deleted" decision accepted. So this app is the later writer on the quit path rather than a
    /// guard on top of a guard, and with that page gone it is the key's only writer.
    pub(crate) fn gx_store_note_last_seen_change(
        &mut self,
        machine_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let machine = MachineId::Remote(machine_id.to_string());
        // A machine whose rows ARE the stored copy has nothing to write back, and writing it would
        // restamp `updatedAt` on a cache row that has not changed since the previous run.
        let Some(revision) = self
            .gx_store
            .core
            .presentation()
            .loaded(&machine)
            .filter(|loaded| !loaded.last_seen)
            .map(|loaded| loaded.revision)
        else {
            return;
        };
        let writer = &mut self.gx_store.last_seen;
        if writer.written_revision.get(machine_id) == Some(&revision) {
            return;
        }
        if !writer.dirty.insert(machine_id.to_string()) {
            return;
        }
        writer.counters.dirtied += 1;
        self.gx_store_book_last_seen_write(cx);
    }

    fn gx_store_book_last_seen_write(&mut self, cx: &mut gpui::Context<Self>) {
        if self.gx_store.last_seen.booked {
            return;
        }
        self.gx_store.last_seen.booked = true;
        self.gx_store.last_seen.booking += 1;
        let booking = self.gx_store.last_seen.booking;
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            background.timer(WRITE_DELAY).await;
            let _ = this.update(cx, |this, cx| {
                if this.gx_store.last_seen.booking != booking {
                    return;
                }
                this.gx_store.last_seen.booked = false;
                this.gx_store_build_last_seen_writes();
                this.gx_store_run_last_seen_writes(cx);
            });
        })
        .detach();
    }

    /// Turns every dirty machine into the payload its key holds. On the UI thread, because the
    /// store is, and measured (`build_max_us`) rather than assumed to be cheap.
    fn gx_store_build_last_seen_writes(&mut self) {
        let dirty: Vec<String> = self.gx_store.last_seen.dirty.iter().cloned().collect();
        for machine_id in dirty {
            let machine = MachineId::Remote(machine_id.clone());
            let started = Instant::now();
            let built = self
                .gx_store
                .core
                .presentation()
                .machine(&machine)
                .filter(|entry| entry.loaded().is_some_and(|loaded| !loaded.last_seen))
                .and_then(|entry| entry.to_snapshot())
                .map(|snapshot| (snapshot.revision, snapshot_storage_json(&snapshot)));
            let writer = &mut self.gx_store.last_seen;
            writer.dirty.remove(&machine_id);
            let Some((revision, raw)) = built else {
                continue;
            };
            let elapsed = started.elapsed().as_micros() as u64;
            writer.counters.build_max_us = writer.counters.build_max_us.max(elapsed);
            writer.counters.largest_bytes = writer.counters.largest_bytes.max(raw.len() as u64);
            // Recorded as written BEFORE the door answers: what this field means is "the copy for
            // this revision has been built and handed to the door", and a failure leaves the
            // payload owed rather than the machine dirty, so a second build of the same rows would
            // be waste.
            writer.written_revision.insert(machine_id.clone(), revision);
            writer.owed.insert(machine_id, raw);
        }
    }

    /// Hands everything owed to the door, one machine at a time, on the background executor.
    fn gx_store_run_last_seen_writes(&mut self, cx: &mut gpui::Context<Self>) {
        let owed: Vec<(String, String)> = self
            .gx_store
            .last_seen
            .owed
            .iter()
            .map(|(machine_id, raw)| (machine_id.clone(), raw.clone()))
            .collect();
        if owed.is_empty() {
            self.gx_store.last_seen.retries = 0;
            return;
        }
        let now_ms = super::host::now_ms() as i64;
        for (machine_id, raw) in owed {
            self.gx_store.last_seen.counters.attempts += 1;
            let background = cx.background_executor().clone();
            cx.spawn(async move |this, cx| {
                let attempted = raw.clone();
                let result = background
                    .spawn({
                        let machine_id = machine_id.clone();
                        async move { write_record(STORE, &machine_key(&machine_id), &raw, now_ms) }
                    })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    this.gx_store_note_last_seen_write(&machine_id, &attempted, result, cx);
                });
            })
            .detach();
        }
    }

    /// What one storage attempt came back with. A refusal is not retried, because the payload does
    /// not shrink by trying again; a failure is, because it is a lock race.
    fn gx_store_note_last_seen_write(
        &mut self,
        machine_id: &str,
        attempted: &str,
        result: Result<RecordWrite, &'static str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let writer = &mut self.gx_store.last_seen;
        // A payload a newer build replaced is not this attempt's to clear.
        let current = writer.owed.get(machine_id).map(String::as_str) == Some(attempted);
        match result {
            Ok(outcome) => {
                if current {
                    writer.owed.remove(machine_id);
                    writer.retries = 0;
                }
                match outcome {
                    RecordWrite::Stored => writer.counters.writes += 1,
                    RecordWrite::Unchanged => writer.counters.unchanged += 1,
                    RecordWrite::Refused(bound) => {
                        writer.counters.refusals += 1;
                        self.gx_store.diagnostics.remote_last_seen_refused(bound);
                    }
                }
            }
            Err(code) => {
                writer.counters.failures += 1;
                let retry = current && writer.retries < MAX_WRITE_RETRIES;
                if current {
                    writer.retries += 1;
                }
                self.gx_store.diagnostics.remote_last_seen_failed(code);
                if retry {
                    self.gx_store_book_last_seen_retry(cx);
                }
            }
        }
    }

    fn gx_store_book_last_seen_retry(&mut self, cx: &mut gpui::Context<Self>) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            background.timer(WRITE_RETRY).await;
            let _ = this.update(cx, |this, cx| {
                this.gx_store_run_last_seen_writes(cx);
            });
        })
        .detach();
    }

    /// Stores whatever the copies still owe, synchronously, on the quit path.
    ///
    /// Two reasons rather than one. A machine whose rows moved inside the last two seconds has not
    /// been built yet, and a build whose write lost a lock race is still owed; both die with the
    /// app otherwise, and the copy the next run seeds from is then older than the session that just
    /// ended. It is also what makes this app the LATER writer of a key the sidebar page still
    /// writes too, which is the answer to the page's scheduled flush being able to replace a newer
    /// Rust row with its own queued one.
    pub(crate) fn gx_store_flush_last_seen_writes(&mut self) {
        self.gx_store_build_last_seen_writes();
        let owed: Vec<(String, String)> = self
            .gx_store
            .last_seen
            .owed
            .iter()
            .map(|(machine_id, raw)| (machine_id.clone(), raw.clone()))
            .collect();
        let now_ms = super::host::now_ms() as i64;
        for (machine_id, raw) in owed {
            self.gx_store.last_seen.counters.attempts += 1;
            let result = write_record(STORE, &machine_key(&machine_id), &raw, now_ms);
            let writer = &mut self.gx_store.last_seen;
            writer.owed.remove(&machine_id);
            match result {
                Ok(RecordWrite::Stored) => writer.counters.writes += 1,
                Ok(RecordWrite::Unchanged) => writer.counters.unchanged += 1,
                Ok(RecordWrite::Refused(bound)) => {
                    writer.counters.refusals += 1;
                    self.gx_store.diagnostics.remote_last_seen_refused(bound);
                }
                Err(code) => {
                    writer.counters.failures += 1;
                    self.gx_store.diagnostics.remote_last_seen_failed(code);
                }
            }
        }
    }
}
