//! Reading and writing the sidebar's own state in the client storage the sidebar has always kept
//! it in.
//!
//! CDXC:Sidebar 2026-09-20 WHY:
//! The values live in the `preferences` table of the client-storage database, under the keys the
//! TypeScript sidebar wrote them under, because an installation that upgrades keeps its collapsed
//! groups, its Space, its hidden items and its filters, and a build from before the port must
//! still read them. This was a second door into that database: the client-storage service in
//! QuickJS owned the first one until 2026-09-25, and there is a `CDXC:Settings` decision saying all storage goes
//! through one system so it cannot silently fill up. The second door is what the port is for, and
//! it carries that decision's obligations itself rather than dropping them: the catalog's entry
//! bound is enforced below, a refused or failed write is counted and reported
//! (`gxStore.sidebarUi.write.warning`), and the three keys have no functional subscriber, only the
//! Settings storage inspector, which reads the database rather than the event stream. What it does
//! not do is meter these writes into `recordStorageEvent`, so the inspector's writes-per-minute
//! figure does not count them. The collapse map is deliberately not pruned, here or on the other
//! side: an entry belongs to a project the user may have merely parked, and dropping it would
//! reopen that project expanded.
//!
//! The database is opened on a background thread with its own connections, kept between calls, and
//! every write is applied inside one immediate transaction, so a collapse envelope is never read
//! by one writer while the other replaces it.
//!
//! SEE-ALSO: packages/client-storage/catalog.ts (the entry and store bounds this mirrors),
//! packages/client-storage-native/src/storage_records.rs (the `records` door to the same file).

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use ghostex_gx_core::{
    COLLAPSE_STORAGE_KEY, HIDDEN_ITEMS_STORAGE_KEY, MACHINE_TAB_STORAGE_KEY,
    PROJECT_COLLECTIONS_STORAGE_KEY, SIDEBAR_WINDOW_SCOPE_ID, SidebarCollapseDiff,
    SidebarCollapseState, SidebarHiddenItems, collapse_state_from_storage,
    hidden_items_from_storage, machine_tab_from_storage, sidebar_window_storage_key,
};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde_json::Value;

/// How long a read or a write waits for another writer's transaction (the QuickJS service thread's
/// until 2026-09-25).
const BUSY_TIMEOUT: Duration = Duration::from_millis(500);
/// How long the sizes the storage bounds are measured against are reused before they are read
/// again. They are a guard against filling a shared budget, so a few seconds of drift in what
/// OTHER keys hold is not worth a scan of the whole table per write; this app's own keys are kept
/// exact whatever the age.
const TOTALS_MAX_AGE: Duration = Duration::from_secs(10);
/// `maxEntryBytes` of the stores these keys belong to (`packages/client-storage/catalog.ts`). The
/// client-storage service refuses a larger entry, and this door refuses it too so the catalog's
/// bound holds for every writer of these keys.
const MAX_ENTRY_BYTES: usize = 64 * 1024;
/// The `workspaceGroups` and `collections` rows, which carry the SAME numbers
/// (`{ maxEntryBytes: 256 * KiB, maxBytes: 256 * KiB }` on top of the singleton defaults). Their
/// own constants because those two documents are the keys here allowed to be larger than the
/// 64 KiB the sidebar's own three share.
const MAX_ENTRY_BYTES_CLIENT_DOCUMENT: usize = 256 * 1024;
const MAX_STORE_BYTES_CLIENT_DOCUMENT: usize = 256 * 1024;
/// `maxBytes` of the collapse store; the other two keep the 128 KiB default.
const MAX_STORE_BYTES_COLLAPSE: usize = 256 * 1024;
const MAX_STORE_BYTES_DEFAULT: usize = 128 * 1024;
/// `maxEntries`: a collection store holds 2,000 keys, a singleton exactly one.
const MAX_ENTRIES_COLLECTION: usize = 2_000;
const MAX_ENTRIES_SINGLETON: usize = 1;
/// `STORAGE_BUDGETS.local`, shared with every other preference key the app has.
const MAX_BACKEND_BYTES: usize = 2 * 1024 * 1024;

/// `storageBytes(key, raw)`: two per UTF-16 code unit of the key and the value together. Not the
/// length of the value in bytes, which is what the bound is most easily mistaken for and admits
/// about twice as much. One implementation for both tables, in the crate the `records` door's
/// bounds live in, because two stores measured by two counters is a bound that disagrees with
/// itself.
pub(super) use ghostex_client_storage::storage_bytes;

/// The sidebar state as storage holds it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct StoredSidebarUi {
    pub(super) collapse: SidebarCollapseState,
    pub(super) hidden_items: SidebarHiddenItems,
    pub(super) selected_machine_id: String,
    /// The stored collections, as JSON; the view model reads them only while the daemon has none.
    pub(super) project_collections: Option<Value>,
}

/// A key a write could not store, and the bound that refused it. Reported rather than retried:
/// the payload does not get smaller by trying again, and the other door refuses it too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SidebarWriteRefusal {
    pub(super) key: &'static str,
    pub(super) bound: &'static str,
}

/// What one write managed, and where its time went. An empty list of refusals is a write that
/// stored everything it was given.
#[derive(Clone, Debug, Default)]
pub(super) struct SidebarWriteReport {
    pub(super) refused: Vec<SidebarWriteRefusal>,
    /// Opening the connection, which only the first write of a run pays.
    pub(super) open_us: u64,
    /// Reading the sizes every bound is measured against, outside the transaction.
    pub(super) totals_us: u64,
    /// Waiting for the other writer's transaction to end.
    pub(super) begin_us: u64,
    /// The reads and writes inside the transaction.
    pub(super) stored_us: u64,
    pub(super) commit_us: u64,
    /// The whole call, filled in by the caller on the thread that ran it.
    pub(super) call_us: u64,
}

/// What one write has to store. A field left empty is not written at all.
#[derive(Clone, Debug, Default)]
pub(super) struct SidebarUiWrite {
    /// What the state changed about the collapse envelope since it was last written.
    pub(super) collapse: Option<(SidebarCollapseDiff, SidebarCollapseState)>,
    pub(super) hidden_items: Option<String>,
    pub(super) selected_machine_id: Option<String>,
}

impl SidebarUiWrite {
    pub(super) fn is_empty(&self) -> bool {
        self.collapse.is_none() && self.hidden_items.is_none() && self.selected_machine_id.is_none()
    }
}

/// The two connections, opened once and kept. A connection that errors is dropped, so the next
/// call opens a fresh one rather than reusing a handle whose file was replaced.
#[derive(Default)]
struct Connections {
    read: Option<Connection>,
    write: Option<Connection>,
    /// The sizes the bounds were last measured against, and when.
    totals: Option<(Instant, Totals)>,
}

fn connections() -> &'static Mutex<Connections> {
    static CONNECTIONS: OnceLock<Mutex<Connections>> = OnceLock::new();
    CONNECTIONS.get_or_init(|| Mutex::new(Connections::default()))
}

pub(super) fn client_storage_path() -> PathBuf {
    crate::shared_settings::ghostex_storage_paths()
        .state_dir
        .join("client-storage.sqlite3")
}

/// `PRIMARY_AGENT_LAUNCHER_STORAGE_KEY` (packages/core-ui/primary-agent-launcher.ts).
const PRIMARY_AGENT_LAUNCHER_STORAGE_KEY: &str = "ghostex-sidebar-project-terminal-launcher";
/// `SIDEBAR_KEEP_AWAKE_RUNTIME_STORAGE_KEY` (packages/core-ui/sidebar-app/collapse-state.ts).
const KEEP_AWAKE_RUNTIME_STORAGE_KEY: &str = "ghostex.titlebar.keepAwakeRuntime";

fn collapse_key() -> String {
    sidebar_window_storage_key(COLLAPSE_STORAGE_KEY, SIDEBAR_WINDOW_SCOPE_ID)
}

fn machine_tab_key() -> String {
    sidebar_window_storage_key(MACHINE_TAB_STORAGE_KEY, SIDEBAR_WINDOW_SCOPE_ID)
}

/// The two client-storage values the sidebar's menus read that are not part of its own state:
/// the agent the user launched last, and the keep-awake duration that is running. Both are
/// read fresh (behind a short cache) rather than restored once, because either can change while
/// the app runs (the keep-awake duration is written by the titlebar page).
pub(super) fn read_menu_host_state() -> Result<(Option<String>, Option<i64>), &'static str> {
    let mut held = connections()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if held.read.is_none() {
        held.read = Some(open(OpenFlags::SQLITE_OPEN_READ_ONLY)?);
    }
    let connection = held.read.as_ref().expect("opened above");
    let result = read_menu_host(connection);
    if result.is_err() {
        held.read = None;
    }
    result
}

/// `ghostex-sidebar-project-terminal-launcher` (`readPrimaryAgentLauncherId`) and
/// `ghostex.titlebar.keepAwakeRuntime` (`readSidebarKeepAwakeRuntime`).
fn read_menu_host(connection: &Connection) -> Result<(Option<String>, Option<i64>), &'static str> {
    let primary = read_preference(connection, PRIMARY_AGENT_LAUNCHER_STORAGE_KEY)?
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let keep_awake = read_preference(connection, KEEP_AWAKE_RUNTIME_STORAGE_KEY)?
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|value| {
            let minutes = value.get("durationMinutes").and_then(Value::as_i64)?;
            // An expired runtime reads as none, the way `readSidebarKeepAwakeRuntime` drops it.
            let fire_at = value.get("fireAtMs").and_then(Value::as_f64);
            let now_ms = super::host::now_ms() as f64;
            match fire_at {
                Some(fire_at) if fire_at <= now_ms => None,
                _ => Some(minutes),
            }
        });
    Ok((primary, keep_awake))
}

/// Reads everything the sidebar state is seeded from. `Err` is a fixed word saying which step
/// failed, never the database's own message, which can carry the file's path.
pub(super) fn read_sidebar_ui_state() -> Result<StoredSidebarUi, &'static str> {
    let mut held = connections()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if held.read.is_none() {
        held.read = Some(open(OpenFlags::SQLITE_OPEN_READ_ONLY)?);
    }
    let connection = held.read.as_ref().expect("opened above");
    let result = read_all(connection);
    if result.is_err() {
        held.read = None;
    }
    result
}

fn read_all(connection: &Connection) -> Result<StoredSidebarUi, &'static str> {
    let scoped = read_preference(connection, &collapse_key())?;
    let legacy = read_preference(connection, COLLAPSE_STORAGE_KEY)?;
    let collections_raw = read_preference(connection, PROJECT_COLLECTIONS_STORAGE_KEY)?;
    let machine_tab = read_preference(connection, &machine_tab_key())?;
    let hidden = read_preference(connection, HIDDEN_ITEMS_STORAGE_KEY)?;
    Ok(StoredSidebarUi {
        collapse: collapse_state_from_storage(
            scoped.as_deref(),
            legacy.as_deref(),
            collections_raw.as_deref(),
        ),
        hidden_items: hidden_items_from_storage(hidden.as_deref()),
        selected_machine_id: machine_tab_from_storage(machine_tab.as_deref()),
        project_collections: collections_raw
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .filter(Value::is_object),
    })
}

/// Stores what changed. The collapse envelope is re-read inside the transaction and the state's
/// own difference applied to it, so a field another writer owns survives.
///
/// CDXC:Sidebar 2026-09-20 WHY:
/// The sizes the bounds are measured against are read BEFORE the transaction opens, and reused for
/// a few seconds. They were read inside it, which made the transaction as long as a scan of every
/// preference key in the database, and that is the exact window the other writer's busy timeout
/// has to sit through. The bounds are a guard against filling a shared budget, not an invariant:
/// measuring them against the database as it was a moment ago is the right trade for not holding
/// an immediate transaction open while doing it. The keys this app writes are always exact,
/// because it applies its own accepted writes to the cached totals itself.
///
/// `Err` is a transient failure the caller owes again; a value too large for one of the bounds
/// comes back as a refusal in the report instead, because owing it again would retry a write that
/// cannot succeed. A refused key does not take the others down with it.
pub(super) fn write_sidebar_ui_state(
    write: &SidebarUiWrite,
) -> Result<SidebarWriteReport, &'static str> {
    if write.is_empty() {
        return Ok(SidebarWriteReport::default());
    }
    let mut report = SidebarWriteReport::default();
    let mut held = connections()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if held.write.is_none() {
        let started = Instant::now();
        held.write = Some(open(OpenFlags::SQLITE_OPEN_READ_WRITE)?);
        report.open_us = started.elapsed().as_micros() as u64;
    }
    // Reused while it is recent, and always before the transaction opens.
    let cached = held
        .totals
        .take()
        .filter(|(read_at, _)| read_at.elapsed() < TOTALS_MAX_AGE)
        .map(|(_, totals)| totals);
    let connection = held.write.as_ref().expect("opened above");
    let mut totals = match cached {
        Some(totals) => totals,
        None => {
            let started = Instant::now();
            let totals = Totals::read(connection)?;
            report.totals_us = started.elapsed().as_micros() as u64;
            totals
        }
    };
    let result = write_in_transaction(connection, write, &mut totals, &mut report);
    match &result {
        Ok(()) => held.totals = Some((Instant::now(), totals)),
        Err(_) => {
            held.write = None;
            held.totals = None;
        }
    }
    result.map(|()| report)
}

fn write_in_transaction(
    connection: &Connection,
    write: &SidebarUiWrite,
    totals: &mut Totals,
    report: &mut SidebarWriteReport,
) -> Result<(), &'static str> {
    let started = Instant::now();
    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(|_| "begin")?;
    report.begin_us = started.elapsed().as_micros() as u64;
    let started = Instant::now();
    let result = write_inside_transaction(connection, write, totals, report);
    report.stored_us = started.elapsed().as_micros() as u64;
    if result.is_ok() {
        let started = Instant::now();
        connection.execute_batch("COMMIT").map_err(|_| "commit")?;
        report.commit_us = started.elapsed().as_micros() as u64;
    } else {
        let _ = connection.execute_batch("ROLLBACK");
    }
    result
}

fn write_inside_transaction(
    connection: &Connection,
    write: &SidebarUiWrite,
    totals: &mut Totals,
    report: &mut SidebarWriteReport,
) -> Result<(), &'static str> {
    let store = |connection: &Connection,
                 report: &mut SidebarWriteReport,
                 totals: &mut Totals,
                 name: &'static str,
                 key: &str,
                 raw: &str,
                 bounds: StoreBounds,
                 store_prefix: &str|
     -> Result<(), &'static str> {
        match totals.admits(key, raw, bounds, store_prefix) {
            Some(bound) => {
                report
                    .refused
                    .push(SidebarWriteRefusal { key: name, bound });
                Ok(())
            }
            None => {
                write_preference(connection, key, raw)?;
                totals.accept(key, raw);
                Ok(())
            }
        }
    };
    if let Some((diff, fallback)) = &write.collapse {
        let key = collapse_key();
        let stored = read_preference(connection, &key)?;
        let raw = diff.apply(stored.as_deref(), fallback);
        store(
            connection,
            report,
            totals,
            "collapse",
            &key,
            &raw,
            StoreBounds::sidebar(MAX_STORE_BYTES_COLLAPSE, MAX_ENTRIES_COLLECTION),
            COLLAPSE_STORAGE_KEY,
        )?;
    }
    if let Some(hidden) = &write.hidden_items {
        store(
            connection,
            report,
            totals,
            "hiddenItems",
            HIDDEN_ITEMS_STORAGE_KEY,
            hidden,
            StoreBounds::sidebar(MAX_STORE_BYTES_DEFAULT, MAX_ENTRIES_SINGLETON),
            HIDDEN_ITEMS_STORAGE_KEY,
        )?;
    }
    if let Some(machine_id) = &write.selected_machine_id {
        let key = machine_tab_key();
        store(
            connection,
            report,
            totals,
            "machineTab",
            &key,
            machine_id,
            StoreBounds::sidebar(MAX_STORE_BYTES_DEFAULT, MAX_ENTRIES_COLLECTION),
            MACHINE_TAB_STORAGE_KEY,
        )?;
    }
    Ok(())
}

/// What one store admits, from its row in the catalog.
#[derive(Clone, Copy)]
struct StoreBounds {
    max_entry_bytes: usize,
    max_bytes: usize,
    max_entries: usize,
}

impl StoreBounds {
    /// The three the sidebar's own keys share, differing only in the store's byte and entry counts.
    const fn sidebar(max_bytes: usize, max_entries: usize) -> Self {
        Self {
            max_entry_bytes: MAX_ENTRY_BYTES,
            max_bytes,
            max_entries,
        }
    }

    /// The `workspaceGroups` and `collections` rows, which are the same numbers. One function
    /// rather than two identical ones: if the catalog ever gives them different budgets this has
    /// to split, and a copy would let one of them drift silently.
    const fn client_document() -> Self {
        Self {
            max_entry_bytes: MAX_ENTRY_BYTES_CLIENT_DOCUMENT,
            max_bytes: MAX_STORE_BYTES_CLIENT_DOCUMENT,
            max_entries: MAX_ENTRIES_SINGLETON,
        }
    }
}

/// Every preference row's size, so the bounds `admission` checks can be checked here too.
///
/// CDXC:Sidebar 2026-09-20 WHY:
/// The entry bound alone is not what the client-storage decision is about: a second door that
/// respects only its own entry size can still push the shared 2 MiB local budget past the limit,
/// which is exactly the "storage silently fills up" the first door exists to prevent. Every one of
/// `admission`'s four refusals is checked here against the same numbers, in the same UTF-16
/// accounting, inside the same transaction that writes: the entry size, the store's own bytes, the
/// store's entry count, and the shared backend total. Cache eviction is the one part not ported,
/// because none of these keys is a cache store and `admission` only evicts for those.
struct Totals {
    rows: Vec<(String, usize)>,
}

impl Totals {
    fn read(connection: &Connection) -> Result<Self, &'static str> {
        let mut query = connection
            .prepare("SELECT key, value FROM preferences")
            .map_err(|_| "query")?;
        let rows = query
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|_| "query")?
            .map(|row| row.map(|(key, value)| (key.clone(), storage_bytes(&key, &value))))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "query")?;
        Ok(Self { rows })
    }

    /// The bound this value would break, or `None` when all three admit it.
    fn admits(
        &self,
        key: &str,
        raw: &str,
        bounds: StoreBounds,
        store_prefix: &str,
    ) -> Option<&'static str> {
        let next = storage_bytes(key, raw);
        if next > bounds.max_entry_bytes {
            return Some("entry");
        }
        let previous = self
            .rows
            .iter()
            .find(|(held, _)| held == key)
            .map_or(0, |(_, bytes)| *bytes);
        let others: Vec<usize> = self
            .rows
            .iter()
            .filter(|(held, _)| held.starts_with(store_prefix) && held != key)
            .map(|(_, bytes)| *bytes)
            .collect();
        if others.iter().sum::<usize>() + next > bounds.max_bytes {
            return Some("store");
        }
        if others.len() + 1 > bounds.max_entries {
            return Some("entries");
        }
        let backend: usize = self.rows.iter().map(|(_, bytes)| *bytes).sum();
        (backend - previous + next > MAX_BACKEND_BYTES).then_some("backend")
    }

    /// Records a key this transaction removed, so the next bound measured in it sees the space back.
    fn forget(&mut self, key: &str) {
        self.rows.retain(|(held, _)| held != key);
    }

    /// Records a value this transaction stored, so the next key in it measures against the total
    /// as it now stands.
    fn accept(&mut self, key: &str, raw: &str) {
        let next = storage_bytes(key, raw);
        match self.rows.iter_mut().find(|(held, _)| held == key) {
            Some((_, bytes)) => *bytes = next,
            None => self.rows.push((key.to_string(), next)),
        }
    }
}

fn open(flags: OpenFlags) -> Result<Connection, &'static str> {
    let connection = Connection::open_with_flags(
        &client_storage_path(),
        flags | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| "open")?;
    connection.busy_timeout(BUSY_TIMEOUT).map_err(|_| "busy")?;
    Ok(connection)
}

/// Runs one read against the pooled read-only connection, dropping it when the read failed so the
/// next caller opens a fresh one.
///
/// The `records` door (`remote_last_seen.rs`) borrows this rather than opening the same file a
/// third time: the pool, the 500 ms busy timeout and the `&'static str` error vocabulary are the
/// ones every key in this database already uses, and a second pool would mean a second set of
/// handles to keep in step with a file that can be replaced.
pub(crate) fn with_read_connection<T>(
    read: impl FnOnce(&Connection) -> Result<T, &'static str>,
) -> Result<T, &'static str> {
    let mut held = connections()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if held.read.is_none() {
        held.read = Some(open(OpenFlags::SQLITE_OPEN_READ_ONLY)?);
    }
    let result = read(held.read.as_ref().expect("opened above"));
    if result.is_err() {
        held.read = None;
    }
    result
}

/// Runs one write against the pooled read-write connection. The `records` door
/// (`records_storage.rs`) borrows it for the same reason it borrows the read side: one pool, one
/// busy timeout and one error vocabulary for a file two tables of which this app now writes.
///
/// A failed call drops the connection AND the cached preference totals, because a connection whose
/// statement failed may have left the transaction open and the totals were measured against a
/// database this process can no longer vouch for.
pub(crate) fn with_write_connection<T>(
    write: impl FnOnce(&Connection) -> Result<T, &'static str>,
) -> Result<T, &'static str> {
    let mut held = connections()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if held.write.is_none() {
        held.write = Some(open(
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?);
    }
    let result = write(held.write.as_ref().expect("opened above"));
    if result.is_err() {
        held.write = None;
        held.totals = None;
    }
    result
}

/// One preference by key, for a caller that owns a single key rather than the sidebar's set. The
/// workspace session groups document (K4) is read and written through here so the connection pool,
/// the busy timeout and the error vocabulary are the ones every other key already uses.
pub(crate) fn read_preference_value(key: &str) -> Result<Option<String>, &'static str> {
    let mut held = connections()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if held.read.is_none() {
        held.read = Some(open(OpenFlags::SQLITE_OPEN_READ_ONLY)?);
    }
    let connection = held.read.as_ref().expect("opened above");
    let result = read_preference(connection, key);
    if result.is_err() {
        held.read = None;
    }
    result
}

/// Writes a client-owned document's key, or REMOVES it when the caller asks (which is what
/// `writeStoredGpuiWorkspaceSessionGroupsState` does with an empty document; the collections key is
/// never removed). `Ok(Some(bound))` is a refusal.
///
/// CDXC:Sessions 2026-09-21 WHY:
/// This goes through the same door the other three keys do, and that is the point rather than
/// tidiness. A second writer that respects only its own entry size can still push the shared 2 MiB
/// local budget past the limit, which is the "storage silently fills up" the first door exists to
/// prevent; the first cut of K4's write was a bare `INSERT ... ON CONFLICT` with none of
/// `admission`'s four refusals, inside no transaction. The bounds are the `workspaceGroups`
/// catalog row's own, which are four times the entry size the other three share.
pub(crate) fn write_client_document_value(
    key: &str,
    raw: Option<&str>,
) -> Result<Option<&'static str>, &'static str> {
    let mut held = connections()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if held.write.is_none() {
        held.write = Some(open(
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?);
    }
    // Read before the transaction opens, for the same reason the sidebar's own write does: the
    // totals read is the slow part and it must not sit inside the window another writer's busy
    // timeout is waiting through.
    let cached = held
        .totals
        .take()
        .filter(|(read_at, _)| read_at.elapsed() < TOTALS_MAX_AGE)
        .map(|(_, totals)| totals);
    let connection = held.write.as_ref().expect("opened above");
    let mut totals = match cached {
        Some(totals) => totals,
        None => Totals::read(connection)?,
    };
    let result = write_client_document_in_transaction(connection, key, raw, &mut totals);
    match &result {
        Ok(_) => held.totals = Some((Instant::now(), totals)),
        Err(_) => {
            held.write = None;
            held.totals = None;
        }
    }
    result
}

fn write_client_document_in_transaction(
    connection: &Connection,
    key: &str,
    raw: Option<&str>,
    totals: &mut Totals,
) -> Result<Option<&'static str>, &'static str> {
    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(|_| "begin")?;
    let result = (|| -> Result<Option<&'static str>, &'static str> {
        let Some(raw) = raw else {
            // A removal only shrinks every total, so no bound can refuse it.
            connection
                .execute("DELETE FROM preferences WHERE key=?1", [key])
                .map_err(|_| "delete")?;
            totals.forget(key);
            return Ok(None);
        };
        if let Some(bound) = totals.admits(key, raw, StoreBounds::client_document(), key) {
            return Ok(Some(bound));
        }
        write_preference(connection, key, raw)?;
        totals.accept(key, raw);
        Ok(None)
    })();
    if result.is_ok() {
        connection.execute_batch("COMMIT").map_err(|_| "commit")?;
    } else {
        let _ = connection.execute_batch("ROLLBACK");
    }
    result
}

fn read_preference(connection: &Connection, key: &str) -> Result<Option<String>, &'static str> {
    connection
        .query_row("SELECT value FROM preferences WHERE key=?1", [key], |row| {
            row.get::<_, String>(0)
        })
        .optional()
        .map_err(|_| "query")
}

fn write_preference(connection: &Connection, key: &str, raw: &str) -> Result<(), &'static str> {
    connection
        .execute(
            "INSERT INTO preferences VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            rusqlite::params![key, raw],
        )
        .map(|_| ())
        .map_err(|_| "write")
}
