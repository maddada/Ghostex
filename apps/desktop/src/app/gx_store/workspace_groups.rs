//! The workspace session groups document inside the app: the stored key, the debounced push, and
//! the echo the guard refuses.
//!
//! CDXC:Sessions 2026-09-21 WHY:
//! This is the fourth client-storage key (K4) and it is a different shape from the other three: one
//! writer plus a SYNCHRONISER. The client owns the document, writes it to the key on every edit,
//! and pushes it to gxserver as a debounced write-through with an indefinite retry; between the
//! edit and the push the daemon still holds the old copy and can echo it, and applying that echo is
//! the oscillation the user sees as a row jumping back and then forward. `WorkspaceGroupsSync` in
//! gx-core is that guard; this file is the three edges it cannot have, which are the clock, the
//! stored key and the socket.
//!
//! **The store's own copy is the one the list reads**, through `side_state().workspace_groups`, and
//! the daemon's stream writes that field too. So the echo is not intercepted on the way in (a
//! second place to get the frame path wrong): the daemon's value lands, and the funnel below then
//! asks the guard what it means. An echo the guard refuses is undone by putting the held document
//! back, which is the one and only place that decision is made.
//!
//! **This is the only editor, the only writer of the key and the only pusher.** Every edit (New
//! Group, Rename, Close Group, every order write, placing a created or forked session) is made
//! here since the app runtime port (2026-09-25), so the hand-off from the old runtime and the
//! hand-back to it are gone with it.
//!
//! **The counters that prove this path fires** are `edits`, `storageWrites`, `pushes`,
//! `pushFailures`, `echoesAdopted`, `echoesRefused` and `prunes` on `gxStore.workspaceGroups`. A
//! run in which the user dragged a session inside a user-made group and `edits` is zero means the
//! command never reached here.
//!
//! SEE-ALSO: packages/gx-core/src/workspace_groups/sync.rs,
//! apps/desktop/src/app/gx_store/sidebar_drag.rs.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use ghostex_gx_core::{
    AdoptOutcome, Event, Intent, MachineId, ProjectKey, SideStateUpdate, WorkspaceGroupsDocument,
    WorkspaceGroupsEffect, WorkspaceGroupsSync,
};
use serde_json::Value;

use super::sidebar_ui_storage;
use crate::GhostexGpuiApp;

mod group_commands;
use super::rpc::gxserver_rpc_result_task;

/// `GPUI_WORKSPACE_SESSION_GROUPS_STORAGE_KEY`.
pub(crate) const WORKSPACE_GROUPS_STORAGE_KEY: &str = "ghostex-gpui-workspace-session-groups";

/// The push is a plain write-through, so it gets the same timeout every other sidebar call has.
const PUSH_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a storage write that lost the lock race waits before it is tried again, and how many
/// times. Nothing is lost while it fails: the owed write stays owed and the quit path flushes it.
const STORAGE_RETRY: Duration = Duration::from_millis(400);
const MAX_STORAGE_RETRIES: u32 = 3;
/// How long a failed read of the stored key waits before it is tried again. The same ladder the
/// sidebar's own state uses, for the same reason: a lock race clears in milliseconds and a missing
/// database never will, and giving up entirely would refuse every edit of the user's groups for the
/// rest of the run.
const READ_RETRY: Duration = Duration::from_secs(5);
/// How many fast attempts before the standing slow one takes over. It never gives up.
const MAX_READ_RETRIES: u32 = 6;
const READ_RETRY_SLOW: Duration = Duration::from_secs(30);

/// Native-host messages dropped because no window was active when they arrived.
///
/// CDXC:Sessions 2026-09-21 WHY:
/// A process-wide count, not this document's own, and named for what it really measures: the
/// handler in `session_chat.rs` is shared by every `ghostexNativeHost` message and needs a window,
/// so a message can vanish there with nothing else to say so. It is not specific to this document
/// and is not reported as if it were.
static NATIVE_HOST_MESSAGES_DROPPED: AtomicU64 = AtomicU64::new(0);

pub(crate) fn note_native_host_message_dropped() {
    NATIVE_HOST_MESSAGES_DROPPED.fetch_add(1, Ordering::Relaxed);
}

pub(super) fn native_host_messages_dropped() -> u64 {
    NATIVE_HOST_MESSAGES_DROPPED.load(Ordering::Relaxed)
}

/// What this app run did with the document. Memory only; the record lines are built from it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct WorkspaceGroupsCounters {
    /// Local edits, which is one per move that writes.
    pub(crate) edits: u64,
    /// Documents this run STORED, and keys it REMOVED: one per payload that reached the database,
    /// not one per try. An attempt that failed, was retried and then succeeded is one store and
    /// three attempts, and conflating the two made `storageWrites` unable to answer the question
    /// the module doc asks it, which is whether the path fires.
    pub(crate) storage_writes: u64,
    pub(crate) storage_removes: u64,
    /// Every call into the storage door, including the retries and the quit flush.
    pub(crate) storage_attempts: u64,
    pub(crate) storage_failures: u64,
    pub(crate) pushes: u64,
    pub(crate) push_failures: u64,
    /// Echoes the guard refused because a push was outstanding. This is the counter the guard
    /// exists for: a run with moves and a zero here means the window never opened, and a run with
    /// moves and a zero `edits` means nothing reached this file at all. It counts ONLY a real
    /// refusal; "there was no echo to judge" is `echoes_absent`, which it used to be folded into.
    pub(crate) echoes_refused: u64,
    /// Reconciles where the daemon had sent no document at all.
    pub(crate) echoes_absent: u64,
    /// Echoes that were not a document at all. Its OWN counter rather than a share of
    /// `echoes_absent` or `echoes_refused`, for the reason the review round split `NoEcho` out of
    /// `IgnoredPending`: an outcome standing for two makes the number that is supposed to be
    /// evidence count something else. **Structurally unreachable for THIS document** and therefore
    /// the one to watch: `parseGpuiWorkspaceSessionGroupsState` answers an empty document for
    /// anything that is not an object, so a non-zero value here means that rule changed.
    pub(crate) echoes_unparsable: u64,
    pub(crate) echoes_adopted: u64,
    pub(crate) echoes_equal: u64,
    pub(crate) echoes_pushed_back: u64,
    /// Passes of the prune that actually dropped a member. A pass that drops nothing is not an
    /// edit and is not counted, exactly as `pruneWorkspaceGroupAssignments` writes nothing.
    pub(crate) prunes: u64,
    /// Documents a storage bound refused. Not retried: the payload does not shrink by trying again.
    pub(crate) storage_refusals: u64,
    /// Reads of the stored key that failed. Until one succeeds nothing is adopted and nothing is
    /// edited, so a non-zero value here beside a zero `edits` is this app refusing to guess.
    pub(crate) read_failures: u64,
    /// Daemon echoes left unjudged for the same reason. **Read this beside `deferred_recovered`:**
    /// equal means every deferral was judged when the read landed, and `echoes_deferred` ahead of
    /// it means one is still owed, which is the daemon's copy never being adopted at all.
    pub(crate) echoes_deferred: u64,
    pub(crate) deferred_recovered: u64,
    /// Pumps that reported `side_state.workspace_groups` as moved, which is what schedules the
    /// reconcile, and reconciles actually entered.
    ///
    /// CDXC:Sessions 2026-09-21 WHY:
    /// These two exist because a record whose counters are all zero cannot say whether the path ran
    /// and did nothing or never ran at all, and two live rounds were spent on exactly that
    /// question. `reconcile_seen` at zero means the daemon's copy never reached the side state as a
    /// CHANGE, which is a wire or a reducer question; `reconcile_seen` ahead of `reconcile_entered`
    /// means the host saw it and did not act on it.
    pub(crate) reconcile_seen: u64,
    pub(crate) reconcile_entered: u64,
    /// New Group, Rename and Close Group answered here (workspace_groups/group_commands.rs).
    pub(crate) group_commands: u64,
    /// Of those, commands made before the stored key was read that the drop queue could not hold.
    pub(crate) group_commands_dropped: u64,
}

#[derive(Default)]
pub(crate) struct WorkspaceGroupsHost {
    pub(super) sync: WorkspaceGroupsSync,
    pub(crate) counters: WorkspaceGroupsCounters,
    /// Bumped by every booking, so a fired timer of a booking that was replaced does nothing.
    booking: u64,
    restored: bool,
    /// The storage write that has not landed yet: `None` is the REMOVE, `Some(raw)` the value, and
    /// the whole field absent is "nothing owed". Replaced by any newer write and cleared only by
    /// the attempt that was carrying it.
    owed_write: Option<Option<String>>,
    write_retries: u32,
    /// A read is on the background executor right now, so the pump must not book a second one.
    restoring: bool,
    read_retries: u32,
    read_retry_scheduled: bool,
}

impl GhostexGpuiApp {
    /// Whether the stored key is in hand, booking the read that puts it there.
    ///
    /// CDXC:Sessions 2026-09-21 WHY:
    /// **A failed read is not a read, and the read is not on the render thread.** The first cut set
    /// `restored` BEFORE reading and swallowed the error, so a read that lost the lock race left the
    /// held document EMPTY, the first daemon echo was adopted with nothing pending, and the write
    /// that follows an adopt overwrote the stored key: the exact thing the comment on the echo path
    /// says must not happen, done by that path. The fix for that made a second one: the caller on
    /// the pump asks this on EVERY burst of daemon frames, so a read that kept failing re-opened
    /// the database and took the connection mutex synchronously on the main thread for the life of
    /// the run, and that mutex is held across `BEGIN IMMEDIATE` by the write paths. The read runs on
    /// the background executor now, behind one `restoring` flag, exactly as the sidebar's own state
    /// does; this function only ever books it and answers what is known. Until it lands nothing may
    /// adopt an echo or answer a drop, because both would be computed against a document this app
    /// does not have.
    pub(super) fn gx_store_restore_workspace_groups(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.gx_store.workspace_groups.restored {
            return true;
        }
        let groups = &mut self.gx_store.workspace_groups;
        if groups.restoring || groups.read_retry_scheduled {
            return false;
        }
        groups.restoring = true;
        cx.spawn(async move |this, cx| {
            let stored = cx
                .background_executor()
                .spawn(async move {
                    sidebar_ui_storage::read_preference_value(WORKSPACE_GROUPS_STORAGE_KEY)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.gx_store.workspace_groups.restoring = false;
                match stored {
                    Ok(stored) => this.gx_store_adopt_stored_workspace_groups(stored, cx),
                    Err(code) => {
                        this.gx_store.workspace_groups.counters.read_failures += 1;
                        this.gx_store.diagnostics.workspace_groups_read_failed(code);
                        this.gx_store_schedule_workspace_groups_read_retry(cx);
                    }
                }
            });
        })
        .detach();
        false
    }

    /// The stored key, once it has been read.
    fn gx_store_adopt_stored_workspace_groups(
        &mut self,
        stored: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.gx_store.workspace_groups.restored = true;
        // A key that is absent, or holding something that is not JSON, is an EMPTY document and not
        // a failure: that is a first launch, and `parse` answers the same way for a damaged payload.
        //
        // CDXC:Sessions 2026-09-21 WHY:
        // **The store is written at the END of this function, not here.** This reconcile reads the
        // daemon's copy out of the side state itself, so seeding the side state with the STORED
        // document first handed the recovered deferral this app's own document as if it were the
        // daemon's echo: the daemon's copy was never judged, and a group made on another computer
        // never appeared, which is the gap the deferral recovery was added to close. The twin of
        // the same ordering in `client_document.rs`, fixed with it rather than after it.
        if let Some(document) = stored
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .map(|value| WorkspaceGroupsDocument::parse(&value))
        {
            self.gx_store.workspace_groups.sync.restore(document);
        }
        // The daemon's copy arrived while the read was on its way and was deferred. This is its
        // only other chance: nothing else will report that field as changed.
        if self.gx_store.workspace_groups.counters.echoes_deferred
            > self.gx_store.workspace_groups.counters.deferred_recovered
        {
            self.gx_store.workspace_groups.counters.deferred_recovered =
                self.gx_store.workspace_groups.counters.echoes_deferred;
            self.gx_store_reconcile_workspace_groups(cx);
        }
        // Now the store ends up holding what the guard holds. Skipped when there is nothing to hold
        // at all (no stored key and no echo), because writing a default document would tell the
        // list this app has an empty document where it has none.
        if self.gx_store.workspace_groups.sync.has_document() {
            let held = self.gx_store.workspace_groups.sync.document().clone();
            self.gx_store_apply_workspace_groups_to_store(&held, cx);
        }
    }

    /// Books another read after a failure. The first `MAX_READ_RETRIES` come quickly; after that it
    /// keeps trying on a standing slow timer, because giving up means this app never learns the
    /// user's groups and every edit of them is refused for the rest of the run.
    fn gx_store_schedule_workspace_groups_read_retry(&mut self, cx: &mut gpui::Context<Self>) {
        let groups = &mut self.gx_store.workspace_groups;
        if groups.read_retry_scheduled {
            return;
        }
        let exhausted = groups.read_retries >= MAX_READ_RETRIES;
        groups.read_retries += 1;
        groups.read_retry_scheduled = true;
        if exhausted && groups.read_retries == MAX_READ_RETRIES + 1 {
            self.gx_store
                .diagnostics
                .workspace_groups_read_unavailable();
        }
        let delay = match exhausted {
            true => READ_RETRY_SLOW,
            false => READ_RETRY,
        };
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update(cx, |this, cx| {
                this.gx_store.workspace_groups.read_retry_scheduled = false;
                this.gx_store_restore_workspace_groups(cx);
            });
        })
        .detach();
    }

    /// Drops members whose sessions the daemon no longer lists, which is what
    /// `pruneWorkspaceGroupAssignments` did on every `createSidebarGroups`.
    ///
    /// Only projects the loaded presentation actually lists are pruned. A machine whose rows have
    /// not arrived lists none, and pruning its entries against the nothing it has would delete
    /// every group the user made on it.
    pub(crate) fn gx_store_prune_workspace_groups(&mut self, cx: &mut gpui::Context<Self>) {
        // Before the empty check, not after: until the stored key is read the document IS empty,
        // so checking first would return here for ever and this app would never hold the user's
        // groups at all unless a drag or a daemon echo happened to read them.
        if !self.gx_store_restore_workspace_groups(cx) {
            return;
        }
        if self
            .gx_store
            .workspace_groups
            .sync
            .document()
            .projects
            .is_empty()
        {
            return;
        }
        let mut existing: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        {
            let document = self.gx_store.workspace_groups.sync.document();
            for (machine, entry) in self.gx_store.core.presentation().machines() {
                let Some(loaded) = entry.loaded() else {
                    continue;
                };
                for project in loaded.projects() {
                    let key = ProjectKey {
                        machine: machine.clone(),
                        project_id: project.project_id.clone(),
                    }
                    .to_workspace_project_id();
                    if document.projects.contains_key(&key) {
                        existing.entry(key).or_default();
                    }
                }
                for session in loaded.server_sessions() {
                    let key = ProjectKey {
                        machine: machine.clone(),
                        project_id: session.project_id.clone(),
                    }
                    .to_workspace_project_id();
                    if let Some(ids) = existing.get_mut(&key) {
                        ids.insert(session.session_id.clone());
                    }
                }
            }
        }
        let pruned = self
            .gx_store
            .workspace_groups
            .sync
            .document()
            .prune_projects(existing.iter().map(|(key, ids)| (key.as_str(), ids)));
        let Some(pruned) = pruned else {
            return;
        };
        self.gx_store.workspace_groups.counters.prunes += 1;
        self.gx_store_edit_workspace_groups(pruned, cx);
    }

    /// A local edit: the document the move produced. Writes the key, books the push, and puts the
    /// document into the store so the list redraws in this frame.
    pub(crate) fn gx_store_edit_workspace_groups(
        &mut self,
        document: WorkspaceGroupsDocument,
        cx: &mut gpui::Context<Self>,
    ) {
        self.gx_store.workspace_groups.counters.edits += 1;
        let effects = self.gx_store.workspace_groups.sync.edit(document.clone());
        self.gx_store_run_workspace_groups_effects(effects, cx);
        self.gx_store_apply_workspace_groups_to_store(&document, cx);
    }

    /// The daemon's copy just landed in the store. Asks the guard what it means and, when the guard
    /// refuses it, puts the held document back.
    ///
    /// Called from the one place a change summary reports `side_state.workspace_groups`, so there
    /// is no second copy of this decision and no frame path to get wrong.
    pub(crate) fn gx_store_reconcile_workspace_groups(&mut self, cx: &mut gpui::Context<Self>) {
        self.gx_store.workspace_groups.counters.reconcile_entered += 1;
        // The stored key is the instant-edit source and has to be in hand before the first echo is
        // judged, or a cold start would adopt the daemon's copy over a document the user edited
        // offline.
        //
        // CDXC:Sessions 2026-09-21 WHY:
        // An echo that arrives before the read lands is DEFERRED, and the deferral has to be
        // recovered by the read rather than by the next frame, because there is usually no next
        // frame: `side_state.workspace_groups` is true on the frame that moved the document and on
        // no later one, and on a normal launch that frame is the initial snapshot. Leaving it to
        // "the next reconcile" meant the daemon's copy was never adopted for the life of the run,
        // so a group made on the phone or another computer never appeared. `gx_store_adopt_stored_workspace_groups`
        // calls this again when the read lands.
        if !self.gx_store_restore_workspace_groups(cx) {
            self.gx_store.workspace_groups.counters.echoes_deferred += 1;
            return;
        }
        let server_state = self
            .gx_store
            .core
            .presentation()
            .machine(&MachineId::Local)
            .and_then(|machine| machine.side_state().workspace_groups.as_ref())
            .map(WorkspaceGroupsDocument::from_side_state);
        let value = server_state.as_ref().map(WorkspaceGroupsDocument::to_json);
        let (outcome, effects) = self
            .gx_store
            .workspace_groups
            .sync
            .adopt(value.as_ref().filter(|_| server_state.is_some()));
        match outcome {
            AdoptOutcome::NoEcho => self.gx_store.workspace_groups.counters.echoes_absent += 1,
            AdoptOutcome::Unparsable => {
                self.gx_store.workspace_groups.counters.echoes_unparsable += 1
            }
            AdoptOutcome::Adopted => self.gx_store.workspace_groups.counters.echoes_adopted += 1,
            AdoptOutcome::IgnoredEqual => self.gx_store.workspace_groups.counters.echoes_equal += 1,
            AdoptOutcome::IgnoredPending => {
                self.gx_store.workspace_groups.counters.echoes_refused += 1
            }
            AdoptOutcome::ScheduledPush => {
                self.gx_store.workspace_groups.counters.echoes_pushed_back += 1
            }
        }
        self.gx_store_run_workspace_groups_effects(effects, cx);
        // Whatever the guard decided, the store must end up holding the document the guard holds:
        // the daemon's value is already in the side state by the time this runs, so a refusal is
        // only a refusal if it is put back.
        let held = self.gx_store.workspace_groups.sync.document().clone();
        if Some(&held) != server_state.as_ref() {
            self.gx_store_apply_workspace_groups_to_store(&held, cx);
        }
    }

    fn gx_store_run_workspace_groups_effects(
        &mut self,
        effects: Vec<WorkspaceGroupsEffect>,
        cx: &mut gpui::Context<Self>,
    ) {
        for effect in effects {
            match effect {
                WorkspaceGroupsEffect::WriteStorage { document, remove } => {
                    self.gx_store_write_workspace_groups_storage(&document, remove, cx);
                }
                WorkspaceGroupsEffect::SchedulePush { delay_ms } => {
                    self.gx_store_book_workspace_groups_push(delay_ms, cx);
                }
            }
        }
    }

    /// `writeStoredGpuiWorkspaceSessionGroupsState`, which REMOVES the key for an empty document
    /// rather than storing an empty object.
    ///
    /// CDXC:Sessions 2026-09-21 WHY:
    /// A write that does not reach storage is OWED again rather than counted and dropped. The
    /// database is WAL, so reads never block, but a second connection's write really does fail with
    /// "database is locked" after the busy timeout while the QuickJS service holds a write
    /// transaction; a rename or a move that lost that race, followed by a quit before the debounced
    /// push reached the daemon, was gone. The owed write is replaced by any later one, retried a
    /// bounded number of times, and flushed synchronously on the quit path beside the other three
    /// keys.
    fn gx_store_write_workspace_groups_storage(
        &mut self,
        document: &Value,
        remove: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let raw = match remove {
            true => None,
            false => Some(document.to_string()),
        };
        // The newest write replaces whatever was owed: they are the same key and the same document
        // lineage, so an older payload has nothing to add.
        self.gx_store.workspace_groups.owed_write = Some(raw.clone());
        self.gx_store.workspace_groups.write_retries = 0;
        self.gx_store_run_workspace_groups_storage_write(raw, cx);
    }

    fn gx_store_run_workspace_groups_storage_write(
        &mut self,
        raw: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        if raw.is_none() {
            self.gx_store.workspace_groups.counters.storage_removes += 1;
        } else {
            self.gx_store.workspace_groups.counters.storage_writes += 1;
        }
        self.gx_store.workspace_groups.counters.storage_attempts += 1;
        let attempted = raw.clone();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    sidebar_ui_storage::write_client_document_value(
                        WORKSPACE_GROUPS_STORAGE_KEY,
                        raw.as_deref(),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.gx_store_note_workspace_groups_write(attempted, result, cx);
            });
        })
        .detach();
    }

    /// What a storage attempt came back with. A refusal is not retried, because the payload does
    /// not shrink by trying again; a failure is, because it is a lock race.
    fn gx_store_note_workspace_groups_write(
        &mut self,
        attempted: Option<String>,
        result: Result<Option<&'static str>, &'static str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let groups = &mut self.gx_store.workspace_groups;
        // An owed write that a newer one replaced is not this attempt's to clear.
        let current = groups.owed_write.as_ref() == Some(&attempted);
        match result {
            Ok(refusal) => {
                if current {
                    groups.owed_write = None;
                    groups.write_retries = 0;
                }
                match (refusal, attempted.is_none()) {
                    // A refusal stored nothing, so it is neither a write nor a remove.
                    (Some(_), _) => {}
                    (None, true) => groups.counters.storage_removes += 1,
                    (None, false) => groups.counters.storage_writes += 1,
                }
                if let Some(bound) = refusal {
                    groups.counters.storage_refusals += 1;
                    self.gx_store
                        .diagnostics
                        .workspace_groups_write_refused(bound);
                }
            }
            Err(code) => {
                groups.counters.storage_failures += 1;
                let retry = current && groups.write_retries < MAX_STORAGE_RETRIES;
                if current {
                    groups.write_retries += 1;
                }
                self.gx_store
                    .diagnostics
                    .workspace_groups_write_failed(code);
                if retry {
                    self.gx_store_book_workspace_groups_storage_retry(attempted, cx);
                }
            }
        }
    }

    fn gx_store_book_workspace_groups_storage_retry(
        &mut self,
        raw: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(STORAGE_RETRY).await;
            let _ = this.update(cx, |this, cx| {
                // Only if nothing newer is owed, which would already be on its way.
                if this.gx_store.workspace_groups.owed_write.as_ref() == Some(&raw) {
                    this.gx_store_run_workspace_groups_storage_write(raw, cx);
                }
            });
        })
        .detach();
    }

    /// Stores whatever the document still owes, synchronously, on the quit path. Without it a
    /// rename made in the last four hundred milliseconds, or one whose write lost a lock race, dies
    /// with the app: the push has not gone out either.
    pub(crate) fn gx_store_flush_workspace_groups_write(&mut self) {
        let Some(raw) = self.gx_store.workspace_groups.owed_write.take() else {
            return;
        };
        self.gx_store.workspace_groups.counters.storage_attempts += 1;
        match sidebar_ui_storage::write_client_document_value(
            WORKSPACE_GROUPS_STORAGE_KEY,
            raw.as_deref(),
        ) {
            Ok(refusal) => {
                match (refusal, raw.is_none()) {
                    (Some(_), _) => {}
                    (None, true) => self.gx_store.workspace_groups.counters.storage_removes += 1,
                    (None, false) => self.gx_store.workspace_groups.counters.storage_writes += 1,
                }
                if let Some(bound) = refusal {
                    self.gx_store.workspace_groups.counters.storage_refusals += 1;
                    self.gx_store
                        .diagnostics
                        .workspace_groups_write_refused(bound);
                }
            }
            Err(code) => {
                self.gx_store.workspace_groups.counters.storage_failures += 1;
                self.gx_store
                    .diagnostics
                    .workspace_groups_write_failed(code);
            }
        }
    }

    /// Books the push, replacing whatever booking was outstanding. A drag that moves a row five
    /// times therefore pushes once, after the last move.
    fn gx_store_book_workspace_groups_push(&mut self, delay_ms: u64, cx: &mut gpui::Context<Self>) {
        self.gx_store.workspace_groups.booking += 1;
        let booking = self.gx_store.workspace_groups.booking;
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            background.timer(Duration::from_millis(delay_ms)).await;
            let _ = this.update(cx, |this, cx| {
                // A booking that was replaced fires nothing: `clearTimeout` is what the TypeScript
                // does, and a timer that cannot be cancelled has to check instead.
                if this.gx_store.workspace_groups.booking == booking {
                    this.gx_store_push_workspace_groups(cx);
                }
            });
        })
        .detach();
    }

    fn gx_store_push_workspace_groups(&mut self, cx: &mut gpui::Context<Self>) {
        let (document, revision) = self.gx_store.workspace_groups.sync.push_started();
        self.gx_store.workspace_groups.counters.pushes += 1;
        let params = serde_json::json!({ "state": document });
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = gxserver_rpc_result_task(
                &background,
                "/api/updateWorkspaceSessionGroups",
                params,
                PUSH_TIMEOUT,
            )
            .await;
            let _ = this.update(cx, |this, cx| {
                let ok = result.is_ok();
                if !ok {
                    this.gx_store.workspace_groups.counters.push_failures += 1;
                }
                let effects = this
                    .gx_store
                    .workspace_groups
                    .sync
                    .push_finished(revision, ok);
                this.gx_store_run_workspace_groups_effects(effects, cx);
                this.gx_store
                    .diagnostics
                    .workspace_groups_pushed(ok, this.gx_store.workspace_groups.counters);
            });
        })
        .detach();
    }

    /// Puts a document into the store, which is what makes the list draw it. The intent carries no
    /// revision: this document is the client's and the daemon keeps a copy of it, not the reverse.
    fn gx_store_apply_workspace_groups_to_store(
        &mut self,
        document: &WorkspaceGroupsDocument,
        cx: &mut gpui::Context<Self>,
    ) {
        self.gx_store_apply_side_state(
            SideStateUpdate::WorkspaceGroups(document.to_side_state()),
            cx,
        );
    }

    /// The one place a client-owned document reaches the store's side state, shared by all three:
    /// a second copy of this is a second place for a document to arrive without the list noticing.
    pub(crate) fn gx_store_apply_side_state(
        &mut self,
        update: SideStateUpdate,
        cx: &mut gpui::Context<Self>,
    ) {
        let output = self.gx_store.core.handle(
            Event::Intent(Intent::SetSideState {
                machine: MachineId::Local,
                update: Box::new(update),
            }),
            super::host::now_ms(),
        );
        if !output.changes.is_empty() {
            self.gx_store.sidebar_list.note_changes(&output.changes);
            self.gx_store_update_sidebar_list(cx);
        }
    }
}
