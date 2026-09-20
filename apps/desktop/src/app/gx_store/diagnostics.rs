use std::collections::HashSet;
use std::time::{Duration, Instant};

use ghostex_gx_client::{ClientDiagnostic, StartError, redact_quoted_values};
use ghostex_gx_core::{ConnectionUpdate, Core, Loadable, MachineId, ProjectKey, ResubscribeReason};
use serde_json::json;

use super::host::GxStoreCounters;
use super::shadow_diff::{ShadowCounters, ShadowDiff, ShadowMismatch};
use super::sidebar_list::{LastUpdate, SidebarListCounters, SidebarListSource};
use super::sidebar_scratch_compare::ScratchDifference;
use super::sidebar_shadow::SidebarShadowCounters;
use super::sidebar_shadow_compare::{FieldDiff, MAX_IDS_PER_RECORD, SidebarMismatch};
use super::sidebar_ui::SidebarUiCounters;
use crate::{shared_settings, support_logs};

/// Distinct mismatch records one app run may write; later ones are only counted.
const MAX_DISTINCT_MISMATCH_RECORDS: usize = 200;
/// Records of a difference that never settled. A few are enough to name the fields; the counter
/// carries the rate.
const MAX_NEVER_SETTLED_RECORDS: u32 = 4;
/// Records of the kept list disagreeing with a fresh one. Each one is a bug, so a handful is
/// plenty to name it and the counter carries the rate.
const MAX_SCRATCH_RECORDS: u32 = 4;
/// Records of a sidebar action. One per click is the whole rate, and the totals ride in each one,
/// so the cap only stops a renderer that repeats a command from filling the log.
const MAX_SIDEBAR_ACTION_RECORDS: u32 = 200;
/// Records of an update slow enough to drop a frame. Enough to see whether the spikes are one
/// shape or several; the max in the summary carries the size.
const MAX_SLOW_UPDATE_RECORDS: u32 = 8;
/// Comfortably inside the sanitizer's own 120, so a value is cut here where it can say it was cut
/// rather than there where it cannot.
const LOG_TEXT_MAX_CHARS: usize = 110;
/// `MAX_SANITIZED_STRING_CHARS` in support_logs.rs, which this must stay under.
const SANITIZER_MAX_CHARS: usize = 120;
/// The sanitizer's `take(32)` on every object and array, and its `depth > 4` cap.
const SANITIZER_MAX_ENTRIES: usize = 32;
const SANITIZER_MAX_DEPTH: usize = 4;
/// Unconditional warning lines one app run may write. A daemon that keeps producing a bad row
/// must not be able to fill the disk through this path.
const MAX_WARNING_LINES: u32 = 40;
const SHADOW_SUMMARY_INTERVAL: Duration = Duration::from_secs(60);

/// Log lines of the store, all in the `native.sidebar.refresh` support log.
///
/// Routine lines (`gxStore.loaded`, `gxStore.connection`, `gxStore.shadow.*`) are written only
/// while "Show debug UI controls" and that scenario are on, which the support log enforces.
/// Warnings (a frame that does not parse, snapshot rows that were skipped) are written always,
/// capped per run. Every line holds ids, counts, enum names, and field names: never a title, a
/// path, or frame content.
#[derive(Default)]
pub(crate) struct GxStoreDiagnostics {
    logged_mismatches: HashSet<u64>,
    warning_lines: u32,
    /// When the summary was last considered, so a run with logging off reads the settings at
    /// most once per interval, and the totals it last wrote.
    shadow_summary_considered_at: Option<Instant>,
    shadow_summary_written: ShadowCounters,
    logged_sidebar_mismatches: HashSet<u64>,
    sidebar_summary_considered_at: Option<Instant>,
    sidebar_summary_written: SidebarShadowCounters,
    sidebar_ui_summary_considered_at: Option<Instant>,
    sidebar_ui_summary_written: SidebarUiCounters,
    sidebar_refusal_warnings: u32,
    sidebar_never_settled_records: u32,
    sidebar_scratch_records: u32,
    sidebar_slow_update_records: u32,
    sidebar_storage_warnings: u32,
    sidebar_action_records: u32,
    sidebar_lifecycle_records: u32,
}

/// A count as a whole percent of a total, which is what tells a skip that fires now and then apart
/// from one that is eating the gate.
fn percent(count: u64, total: u64) -> u64 {
    if total == 0 {
        return 0;
    }
    count.saturating_mul(100) / total
}

/// What one view-model update was handed, in the shape both records that carry it use.
fn last_update_details(last_update: &LastUpdate) -> serde_json::Value {
    json!({
        "changesEmpty": last_update.changes_empty,
        "sessionsChanged": last_update.sessions_changed,
        "sessionsRemoved": last_update.sessions_removed,
        "projectsChanged": last_update.projects_changed,
        "projectsRemoved": last_update.projects_removed,
        "sessionOrderChanged": last_update.session_order_changed,
        "projectOrderChanged": last_update.project_order_changed,
        "machinesReloaded": last_update.machines_reloaded,
        "focusChanged": last_update.focus_changed,
        "workspaceGroups": last_update.workspace_groups,
        "projectCollections": last_update.project_collections,
        "spaces": last_update.spaces,
        "customSessionTags": last_update.custom_session_tags,
        "dirty": last_update.dirty,
        "uiGenerationMoved": last_update.ui_generation_moved,
        "settingsMoved": last_update.settings_moved,
    })
}

pub(super) fn routine_logging_enabled() -> bool {
    shared_settings::shared_sidebar_settings_snapshot().debugging_mode()
        && support_logs::scenario_enabled(support_logs::GpuiDiagnosticScenario::SidebarRefresh)
}

fn append(event: &str, details: serde_json::Value) {
    support_logs::append(support_logs::GpuiSupportLog::SidebarRefresh, event, details);
}

impl GxStoreDiagnostics {
    /// One line per (re)load of the local machine: the first one is the store coming up.
    pub(super) fn store_loaded(
        &mut self,
        core: &Core,
        counters: &GxStoreCounters,
        since_connect: Option<Duration>,
    ) {
        let store = core.presentation();
        let Some(loaded) = store.loaded(&MachineId::Local) else {
            return;
        };
        let chat_projects = loaded
            .projects()
            .iter()
            .filter(|project| {
                store.is_chat_project(&ProjectKey::local(project.project_id.as_str()))
            })
            .count();
        append(
            "gxStore.loaded",
            json!({
                "first": counters.reloads == 1,
                "revision": loaded.revision,
                "projects": loaded.projects().len(),
                "groups": loaded.groups().len(),
                "sessions": loaded.session_count(),
                "chatProjects": chat_projects,
                "tabsGeneration": core.tabs_generation(),
                "connectToLoadedMs": since_connect.map(|elapsed| elapsed.as_millis() as u64),
                "clientStarts": counters.client_starts,
                "reloads": counters.reloads,
            }),
        );
    }

    /// One line per (re)load of a remote machine: the moment its rows reach the store.
    pub(super) fn remote_machine_loaded(&mut self, core: &Core, machine: &MachineId) {
        let Some(loaded) = core.presentation().loaded(machine) else {
            return;
        };
        record(
            "gxStore.remote.loaded",
            json!({
                "machineId": log_text(machine.remote_id().unwrap_or_default()),
                "revision": loaded.revision,
                "projects": loaded.projects().len(),
                "groups": loaded.groups().len(),
                "sessions": loaded.session_count(),
            }),
        );
    }

    /// A remote client's thread is gone although nobody stopped it; a new one follows while the
    /// machine is still connected.
    pub(super) fn remote_client_thread_ended(&mut self, machine_id: &str, restart_in: Duration) {
        self.warning(
            "gxStore.remote.clientThreadEnded.warning",
            json!({
                "machineId": log_text(machine_id),
                "restartInMs": restart_in.as_millis() as u64,
            }),
        );
    }

    /// The persisted focus seeded the core at startup. Ids only.
    pub(super) fn focus_restored(&mut self, core: &Core) {
        let focus = core.focus();
        append(
            "gxStore.focusRestored",
            json!({
                "activeProjectId": focus.active_project.as_ref().map(|project| &project.project_id),
                "focusedSessionId": focus.focused_session.as_ref().map(|session| &session.session_id),
            }),
        );
    }

    /// The old runtime sent an empty tab list for a project the store does not see as empty (or
    /// cannot judge yet), so the workspace kept its tabs. A warning: it means the two readers of
    /// the daemon disagree, or the old runtime posted before it had rows.
    pub(super) fn empty_tab_list_disputed(&mut self, core: &Core, total: u64) {
        let store_tabs = match core.active_tab_sessions() {
            Loadable::Loaded(tabs) => Some(tabs.len()),
            Loadable::NotLoaded | Loadable::Missing => None,
        };
        self.warning(
            "gxStore.emptyTabListDisputed.warning",
            json!({
                "storeRevision": store_revision(core),
                "storeActiveTabs": store_tabs,
                "total": total,
            }),
        );
    }

    /// A machine's stream changed state. Named, because since M4d several machines share this
    /// record and a support log that cannot say WHICH one dropped answers nothing.
    pub(super) fn connection(&mut self, machine: &MachineId, update: &ConnectionUpdate) {
        let machine_id = machine.remote_id().unwrap_or("local");
        let details = match update {
            ConnectionUpdate::Connecting { attempt } => {
                json!({ "machineId": log_text(machine_id), "phase": "connecting", "attempt": attempt })
            }
            // `reason`, not `error`: a daemon restart is routine, and the support log writes any
            // line with an error-named key unconditionally.
            ConnectionUpdate::Lost { error } => {
                json!({ "machineId": log_text(machine_id), "phase": "lost", "reason": error })
            }
            _ => return,
        };
        record("gxStore.connection", details);
    }

    pub(super) fn resubscribe_requested(&mut self, reason: &ResubscribeReason) {
        let reason = match reason {
            ResubscribeReason::CurrentWithoutSnapshot => "currentWithoutSnapshot",
            ResubscribeReason::ServerChanged => "serverChanged",
            _ => "other",
        };
        append("gxStore.resubscribeRequested", json!({ "reason": reason }));
    }

    pub(super) fn skipped_rows(
        &mut self,
        projects: usize,
        groups: usize,
        sessions: usize,
        first_error: &str,
    ) {
        self.warning(
            "gxStore.snapshotRowsSkipped.warning",
            json!({
                "projects": projects,
                "groups": groups,
                "sessions": sessions,
                "firstError": redact_quoted_values(first_error),
            }),
        );
    }

    pub(super) fn client_diagnostic(&mut self, diagnostic: &ClientDiagnostic) {
        let details = match diagnostic {
            ClientDiagnostic::FrameParseFailed {
                event_type,
                error,
                resubscribe_scheduled,
            } => json!({
                "kind": "frameParseFailed",
                "frameType": event_type,
                "error": error,
                "resubscribeScheduled": resubscribe_scheduled,
            }),
            ClientDiagnostic::DomainProjectsReadFailed { error } => {
                json!({ "kind": "domainProjectsReadFailed", "error": error })
            }
            ClientDiagnostic::SubscribeNotAcknowledged => {
                json!({ "kind": "subscribeNotAcknowledged" })
            }
            ClientDiagnostic::ProtocolMismatch { received } => {
                json!({ "kind": "protocolMismatch", "received": received })
            }
            ClientDiagnostic::ThreadStopped { reason } => {
                json!({ "kind": "threadStopped", "error": reason })
            }
        };
        self.warning("gxStore.client.warning", details);
    }

    pub(super) fn client_start_failed(&mut self, error: &StartError) {
        self.warning(
            "gxStore.clientStart.error",
            json!({ "error": error.to_string() }),
        );
    }

    /// The client's thread is gone although nobody stopped it; a new client follows.
    pub(super) fn client_thread_ended(&mut self, restart_attempt: u32, restart_in: Duration) {
        self.warning(
            "gxStore.clientThreadEnded.warning",
            json!({
                "restartAttempt": restart_attempt,
                "restartInMs": restart_in.as_millis() as u64,
            }),
        );
    }

    fn warning(&mut self, event: &str, details: serde_json::Value) {
        if self.warning_lines >= MAX_WARNING_LINES {
            return;
        }
        self.warning_lines += 1;
        append(event, details);
    }

    /// One bounded record per distinct mismatch. A mismatch seen while logging is off is not
    /// remembered, so turning the scenario on later still records it when it happens again.
    pub(super) fn shadow_mismatch(&mut self, mismatch: &ShadowMismatch, core: &Core) {
        let signature = mismatch.signature();
        if self.logged_mismatches.contains(&signature)
            || self.logged_mismatches.len() >= MAX_DISTINCT_MISMATCH_RECORDS
            || !routine_logging_enabled()
        {
            return;
        }
        self.logged_mismatches.insert(signature);
        let fields: Vec<serde_json::Value> = mismatch
            .fields
            .iter()
            .map(|(tab, names)| json!({ "tab": tab, "names": names }))
            .collect();
        record(
            "gxStore.shadow.mismatch",
            json!({
                "storeGroup": log_text(mismatch.store_group.as_str()),
                "storeRevision": store_revision(core),
                "oldTabCount": mismatch.old_tab_count,
                "storeTabCount": mismatch.store_tab_count,
                "onlyOld": log_texts(&mismatch.only_old),
                "onlyStore": log_texts(&mismatch.only_store),
                "orderDiffers": mismatch.order_differs,
                "fields": fields,
            }),
        );
    }

    /// The running totals, at most once a minute and only when they moved. Without it a run
    /// with no mismatch would leave no trace that comparisons ran.
    pub(super) fn shadow_summary(&mut self, shadow: &ShadowDiff, core: &Core) {
        let counters = shadow.counters();
        if counters == self.shadow_summary_written
            || self
                .shadow_summary_considered_at
                .is_some_and(|at| at.elapsed() < SHADOW_SUMMARY_INTERVAL)
        {
            return;
        }
        self.shadow_summary_considered_at = Some(Instant::now());
        if !routine_logging_enabled() {
            return;
        }
        self.shadow_summary_written = counters;
        let active_tabs = match core.active_tab_sessions() {
            Loadable::Loaded(tabs) => Some(tabs.len()),
            Loadable::NotLoaded | Loadable::Missing => None,
        };
        record(
            "gxStore.shadow.summary",
            json!({
                "observed": counters.observed,
                "matches": counters.matches,
                "mismatches": counters.mismatches,
                "distinctMismatches": counters.distinct_mismatches,
                "transient": counters.transient,
                "notComparable": counters.not_comparable,
                "remoteSkipped": counters.remote_skipped,
                "iconDifferences": counters.icon_differences,
                "staleExternalFocus": counters.stale_external_focus,
                "pending": shadow.is_pending(),
                "storeRevision": store_revision(core),
                "storeActiveTabs": active_tabs,
                "tabsGeneration": core.tabs_generation(),
            }),
        );
    }
}

impl GxStoreDiagnostics {
    /// One bounded record per distinct sidebar difference: ids, counts, and field names only.
    pub(super) fn sidebar_mismatch(&mut self, mismatch: &SidebarMismatch, snapshot_revision: u64) {
        let signature = mismatch.signature();
        if self.logged_sidebar_mismatches.contains(&signature)
            || self.logged_sidebar_mismatches.len() >= MAX_DISTINCT_MISMATCH_RECORDS
            || !routine_logging_enabled()
        {
            return;
        }
        self.logged_sidebar_mismatches.insert(signature);
        // CDXC:Sidebar 2026-09-20 WHY:
        // Every value in this record sits at depth 4 or less, because the log's sanitizer replaces
        // anything deeper with the string "[depth-capped]" (support_logs.rs:463). `details` itself
        // is depth 0, so an array under it is 1, an object in that array is 2, that object's array
        // is 3 and its strings are 4. The first shape of this record put each differing field in
        // an object inside that array, which put the field NAME at depth 5, and every record
        // printed three capped strings and told nobody anything. A field is one short string here,
        // and `fields` repeats the whole record's names at depth 2 so the answer survives even if
        // this record is ever nested one level deeper.
        let entries = |fields: &[FieldDiff]| -> Vec<serde_json::Value> {
            fields
                .iter()
                .map(|field| serde_json::Value::String(log_text(encode_field(field))))
                .collect()
        };
        let named = |fields: &[(String, Vec<FieldDiff>)]| -> Vec<serde_json::Value> {
            fields
                .iter()
                .map(|(id, names)| json!({ "id": log_text(id.as_str()), "fields": entries(names) }))
                .collect()
        };
        // Every differing field of the whole record, once, at a depth nothing can cap.
        let distinct = distinct_fields(mismatch);
        record(
            "gxStore.sidebarShadow.mismatch",
            json!({
                "snapshotRevision": snapshot_revision,
                "oldGroupCount": mismatch.old_group_count,
                "storeGroupCount": mismatch.store_group_count,
                "onlyOldGroups": log_texts(&mismatch.only_old_groups),
                "onlyStoreGroups": log_texts(&mismatch.only_store_groups),
                "groupOrderDiffers": mismatch.group_order_differs,
                "fields": log_texts(&distinct),
                "topLevel": entries(&mismatch.top_level),
                "groups": named(&mismatch.groups),
                "sessions": named(&mismatch.sessions),
                "onlyOldSessions": log_texts(&mismatch.only_old_sessions),
                "onlyStoreSessions": log_texts(&mismatch.only_store_sessions),
                "questionCountOnly": mismatch.question_count_only,
                "tooltipOnly": mismatch.tooltip_only,
                "onlyFrozenFields": mismatch.only_frozen_fields,
                "onlyTimingFields": mismatch.only_timing_fields,
                "onlyStaleFields": mismatch.only_stale_fields,
                "onlyExplainedFields": mismatch.only_explained_fields,
                // One object per group whose order differs. Separate short values rather than one
                // sentence: the sentence was long enough to be redacted and carried a slash, which
                // redacts on its own. Each value here sits at depth 3.
                "orderDivergence": mismatch
                    .order_divergence
                    .iter()
                    .map(|divergence| {
                        json!({
                            "group": log_text(divergence.group_id.as_str()),
                            "index": divergence.index,
                            "old": divergence.old_id.as_deref().map(log_text),
                            "store": divergence.store_id.as_deref().map(log_text),
                            "oldLen": divergence.old_len,
                            "storeLen": divergence.store_len,
                        })
                    })
                    .collect::<Vec<_>>(),
            }),
        );
    }

    /// The running totals of the sidebar list and its comparison, at most once a minute and only
    /// when they moved.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn sidebar_summary(
        &mut self,
        counters: &SidebarShadowCounters,
        list: &SidebarListCounters,
        source: SidebarListSource,
        deadline_kind: &'static str,
        pending: bool,
        groups: usize,
        rows: usize,
        phases: super::sidebar_snapshot::InstallPhases,
        remote: &super::remote_clients::RemoteClientCounters,
        machines: usize,
    ) {
        if *counters == self.sidebar_summary_written
            || self
                .sidebar_summary_considered_at
                .is_some_and(|at| at.elapsed() < SHADOW_SUMMARY_INTERVAL)
        {
            return;
        }
        self.sidebar_summary_considered_at = Some(Instant::now());
        if !routine_logging_enabled() {
            return;
        }
        self.sidebar_summary_written = *counters;
        record(
            "gxStore.sidebarShadow.summary",
            json!({
                "source": match source {
                    SidebarListSource::Store => "store",
                    SidebarListSource::Projection => "projection",
                },
                "publishes": counters.publishes,
                "comparisons": counters.comparisons,
                // What was compared ON a remote machine's tab. `skipped.remote` at zero is not the
                // gate on its own: every other skip below can hold this at zero beside it.
                "comparisonsRemote": counters.comparisons_remote,
                "rejudged": counters.rejudged,
                "matches": counters.matches,
                "mismatches": counters.mismatches,
                "unexplained": counters.mismatches.saturating_sub(counters.explained_only),
                "distinctMismatches": counters.distinct_mismatches,
                "transient": counters.transient,
                "neverSettled": counters.never_settled,
                "pending": pending,
                "storeGroups": groups,
                "storeRows": rows,
                "deadlineKind": deadline_kind,
                // Grouped rather than flat: the sanitizer keeps the first 32 keys of an object and
                // drops the rest without saying so, and this record passed 32 as it grew.
                "skipped": {
                    "remote": counters.skipped_remote,
                    "machineMismatch": counters.skipped_machine_mismatch,
                    "foreignFocus": counters.skipped_foreign_focus,
                    "notLoaded": counters.skipped_not_loaded,
                    "notLive": counters.skipped_not_live,
                    "notRestored": counters.skipped_not_restored,
                    // As a share of the publishes that reached the comparison, so a skip that is
                    // quietly eating most of them reads as a number rather than as a total nobody
                    // divides. Whole percent; zero publishes reads as zero.
                    "foreignFocusPct": percent(counters.skipped_foreign_focus, counters.publishes),
                    "machineMismatchPct": percent(
                        counters.skipped_machine_mismatch,
                        counters.publishes,
                    ),
                    "remotePct": percent(counters.skipped_remote, counters.publishes),
                },
                "explained": {
                    "explainedOnly": counters.explained_only,
                    "questionCountOnly": counters.question_count_only,
                    "tooltipOnly": counters.tooltip_only,
                    "frozenFieldsOnly": counters.frozen_fields_only,
                    "timingFieldsOnly": counters.timing_fields_only,
                    "staleFieldsOnly": counters.stale_fields_only,
                },
                "scratch": {
                    "checks": counters.scratch_checks,
                    "mismatches": counters.scratch_mismatches,
                },
                // One client per connected remote machine. `machines` is how many tabs the sidebar
                // offers, `starts` how many clients this run opened, and `unloads` how many
                // machines lost their rows because the user disabled or removed them.
                "remote": {
                    "machines": machines,
                    "starts": remote.starts,
                    "stops": remote.stops,
                    "unloads": remote.unloads,
                    "threadExits": remote.thread_exits,
                    "events": remote.events,
                    "reloads": remote.reloads,
                },
                "list": {
                    "updates": list.updates,
                    "updatesIdle": list.idle,
                    "viewChanges": list.view_changes,
                    "installs": list.installs,
                    "installsSkipped": list.installs_skipped,
                    // Publishes accepted while the store's list was drawn, and the ones that moved
                    // a value the list still takes from a publish. Before M4c the two were equal,
                    // because the list carried that publish's menus.
                    "publishesSeen": list.publishes_seen,
                    "installsFromCarry": list.installs_from_carry,
                    "deadlineWakes": list.deadline_wakes,
                    "wakeRowsMoved": list.wake_rows_moved,
                    "wakeRowsMovedMax": list.wake_rows_moved_max,
                },
                "timings": {
                    "updateUs": list.last_update_us,
                    "updateMaxUs": list.update_max_us,
                    "installUs": list.last_install_us,
                    "installMaxUs": list.install_max_us,
                    "compareUs": counters.last_compare_us,
                    "compareMaxUs": counters.compare_max_us,
                    "relabelUs": list.last_relabel_us,
                    "relabelMaxUs": list.relabel_max_us,
                },
                // Where the newest install's time went, and what the caches saved it. A rise in
                // installUs says which part of the build it came from rather than inviting a
                // guess: the shared key, the rows, the group menus, the collection menus, the more
                // menu, or the tail that copies what a publish still owns.
                "install": {
                    "hostUs": phases.host_us,
                    "fingerprintUs": phases.fingerprint_us,
                    "keyUs": phases.key_us,
                    "rowsUs": phases.rows_us,
                    "groupsUs": phases.groups_us,
                    "collectionsUs": phases.collections_us,
                    "moreMenuUs": phases.more_menu_us,
                    "tailUs": phases.tail_us,
                    "rowsBuilt": phases.rows_built,
                    "rowsReused": phases.rows_reused,
                    "groupsBuilt": phases.groups_built,
                    "groupsReused": phases.groups_reused,
                    "collectionsBuilt": phases.collections_built,
                    "collectionsReused": phases.collections_reused,
                    "moreMenuBuilt": phases.more_menu_built,
                    // Why the group phase cost what it did: the shared key missing drops every
                    // cached menu at once, a moved `GroupCore` is the view model having rebuilt
                    // that group, and a moved collection is only the folder it is drawn in.
                    "keyReused": phases.key_reused,
                    "groupsMissing": phases.groups_missing,
                    "groupsCoreMoved": phases.groups_core_moved,
                    "groupsCollectionMoved": phases.groups_collection_moved,
                },
            }),
        );
    }

    /// One record per slow view-model update: what it was handed and what it rebuilt.
    ///
    /// CDXC:Sidebar 2026-09-20 WHY:
    /// `updateMaxUs` is one number with no story: a twenty-millisecond update and a twenty-
    /// microsecond one are the same call from outside, and the difference is always which caches
    /// the update had to drop. Guessing from the median is how a spike gets attributed to whatever
    /// landed in the same release. The counts here answer it outright: `rowsAllDirty` with a row
    /// count is the tag catalog or Debugging Mode moving, `reset` is a machine (un)loading,
    /// `meta` is the project facts, and all three false with a large `rowsBuilt` is the store
    /// having really changed that many rows. `offCpuUs` near `updateUs` with all of them small is
    /// none of those: the thread was not running, and the update is a victim rather than a cause.
    pub(super) fn sidebar_slow_update(
        &mut self,
        update_us: u64,
        cpu_us: Option<u64>,
        last_update: &LastUpdate,
        work: &ghostex_gx_core::SidebarUpdateWork,
    ) {
        if self.sidebar_slow_update_records >= MAX_SLOW_UPDATE_RECORDS || !routine_logging_enabled()
        {
            return;
        }
        self.sidebar_slow_update_records += 1;
        record(
            "gxStore.sidebarList.slowUpdate",
            json!({
                "updateUs": update_us,
                // What the thread actually spent running, and what it spent not running. Nothing
                // inside the update takes a lock or touches the file system, so a large gap is the
                // thread descheduled or in the kernel rather than this code being slow.
                "cpuUs": cpu_us,
                "offCpuUs": cpu_us.map(|cpu| update_us.saturating_sub(cpu)),
                "work": {
                    "reset": work.reset,
                    "rowsAllDirty": work.rows_all_dirty,
                    "metaDirty": work.meta_dirty,
                    "rowsBuilt": work.rows_built,
                    "browserRowsBuilt": work.browser_rows_built,
                    "groupsBuilt": work.groups_built,
                    "machineSummariesBuilt": work.machine_summaries_built,
                    "groups": work.group_count,
                    "rows": work.row_count,
                },
                "lastUpdate": last_update_details(last_update),
            }),
        );
    }

    /// One record per distinct difference between the kept list and one built from nothing.
    ///
    /// CDXC:Sidebar 2026-09-20 WHY:
    /// This is the record for a cache this port failed to invalidate, which is a different animal
    /// from a difference with the old projection: both sides here are this code reading the same
    /// store at the same instant, so one of them is simply wrong. It carries what the newest
    /// update was handed as well as the difference, because the answer is always "which input
    /// moved without the thing that depends on it being dropped", and the update's flags are where
    /// that starts.
    pub(super) fn sidebar_scratch_mismatch(
        &mut self,
        difference: &ScratchDifference,
        last_update: &LastUpdate,
    ) {
        if self.sidebar_scratch_records >= MAX_SCRATCH_RECORDS || !routine_logging_enabled() {
            return;
        }
        self.sidebar_scratch_records += 1;
        record(
            "gxStore.sidebarShadow.scratchMismatch",
            json!({
                "onlyIncrementalGroups": log_texts(&difference.only_incremental_groups),
                "onlyScratchGroups": log_texts(&difference.only_scratch_groups),
                "groupOrderDiffers": difference.group_order_differs,
                "topLevel": log_texts(&difference.top_level),
                "groups": named_fields(&difference.groups),
                "rows": named_fields(&difference.rows),
                "onlyIncrementalRows": log_texts(&difference.only_incremental_rows),
                "onlyScratchRows": log_texts(&difference.only_scratch_rows),
                "lastUpdate": last_update_details(last_update),
            }),
        );
    }

    /// One record for a difference that was replaced before it could settle, a few per run.
    ///
    /// CDXC:Sidebar 2026-09-20 WHY:
    /// A difference that takes a new shape on every judgement never settles, so it never reaches
    /// the mismatch record, and `neverSettled` counted them without ever saying what they were.
    /// A list that genuinely churns and a field that flaps look identical from the counter; the
    /// field names are what tells them apart.
    pub(super) fn sidebar_never_settled(
        &mut self,
        previous_fields: &[String],
        mismatch: &SidebarMismatch,
    ) {
        if self.sidebar_never_settled_records >= MAX_NEVER_SETTLED_RECORDS
            || !routine_logging_enabled()
        {
            return;
        }
        self.sidebar_never_settled_records += 1;
        let next_fields = distinct_fields(mismatch);
        record(
            "gxStore.sidebarShadow.neverSettled",
            json!({
                // What MOVED between the two judgements, which is the thing a shape that never
                // settles is only ever visible through. The constant fields are in neither list.
                "gained": log_texts(
                    next_fields
                        .iter()
                        .filter(|field| !previous_fields.contains(field))
                        .collect::<Vec<_>>(),
                ),
                "lost": log_texts(
                    previous_fields
                        .iter()
                        .filter(|field| !next_fields.contains(field))
                        .collect::<Vec<_>>(),
                ),
                "fields": log_texts(&next_fields),
                "sessions": mismatch
                    .sessions
                    .iter()
                    .map(|(id, _)| serde_json::Value::String(log_text(id.as_str())))
                    .take(MAX_IDS_PER_RECORD)
                    .collect::<Vec<_>>(),
                "groups": mismatch
                    .groups
                    .iter()
                    .map(|(id, _)| serde_json::Value::String(log_text(id.as_str())))
                    .take(MAX_IDS_PER_RECORD)
                    .collect::<Vec<_>>(),
            }),
        );
    }

    /// The running totals of the sidebar's own state and its writes, on the same schedule.
    pub(super) fn sidebar_ui_summary(&mut self, counters: &SidebarUiCounters) {
        if *counters == self.sidebar_ui_summary_written
            || self
                .sidebar_ui_summary_considered_at
                .is_some_and(|at| at.elapsed() < SHADOW_SUMMARY_INTERVAL)
        {
            return;
        }
        self.sidebar_ui_summary_considered_at = Some(Instant::now());
        if !routine_logging_enabled() {
            return;
        }
        self.sidebar_ui_summary_written = *counters;
        record(
            "gxStore.sidebarUi.summary",
            json!({
                "intents": counters.intents,
                "writes": counters.writes,
                "writeFailures": counters.write_failures,
                "readFailures": counters.read_failures,
                "writeRefusals": counters.write_refusals,
                "writeMaxUs": counters.write_max_us,
                "writeOpenMaxUs": counters.write_open_max_us,
                "writeTotalsMaxUs": counters.write_totals_max_us,
                "writeBeginMaxUs": counters.write_begin_max_us,
                "writeStoredMaxUs": counters.write_stored_max_us,
                "writeCommitMaxUs": counters.write_commit_max_us,
                "writeCallMaxUs": counters.write_call_max_us,
            }),
        );
    }

    /// The sidebar's own state could not be read from client storage, so the Rust list is not
    /// drawn and nothing is written. The code is a fixed word: a database error string can carry
    /// the file's path.
    pub(super) fn sidebar_ui_read_failed(&mut self, error: &'static str) {
        self.sidebar_storage_warning("gxStore.sidebarUi.read.warning", error);
    }

    /// A write of the sidebar's own state did not reach client storage. The change is still held
    /// in memory and is written again with the next one.
    pub(super) fn sidebar_ui_write_failed(&mut self, error: &'static str) {
        self.sidebar_storage_warning("gxStore.sidebarUi.write.warning", error);
    }

    /// A storage bound refused one value. Counted apart from a failure, and with its own budget of
    /// lines, because a refusal repeats for as long as the payload stays that size and would
    /// otherwise use up the warnings a real failure needs.
    pub(super) fn sidebar_ui_write_refused(&mut self, key: &'static str, bound: &'static str) {
        if self.sidebar_refusal_warnings >= 3 {
            return;
        }
        self.sidebar_refusal_warnings += 1;
        self.warning(
            "gxStore.sidebarUi.write.refused",
            json!({ "key": key, "bound": bound }),
        );
    }

    fn sidebar_storage_warning(&mut self, event: &'static str, error: &'static str) {
        if self.sidebar_storage_warnings >= 3 {
            return;
        }
        self.sidebar_storage_warnings += 1;
        self.warning(event, json!({ "error": error }));
    }

    /// One line per sleep or wake the store performed: which call it made, what the daemon said,
    /// how long the round trip took, and the run's totals.
    ///
    /// No id and no title: the call name and the answer are a fixed vocabulary, and everything
    /// else a lifecycle payload carries is a project id or a session id. `roundTripMs` is the
    /// daemon's, not ours, and it is the number that says whether the optimistic value was ever
    /// on screen: a round trip under a frame means the daemon answered before the user could see
    /// anything, and a long one is the window the overlay exists for.
    pub(super) fn sidebar_lifecycle_ran(
        &mut self,
        request: &ghostex_gx_core::LifecycleRequest,
        answer: &str,
        round_trip_ms: u64,
        counters: super::sidebar_lifecycle::SidebarLifecycleCounters,
    ) {
        if self.sidebar_lifecycle_records >= MAX_SIDEBAR_ACTION_RECORDS
            || !routine_logging_enabled()
        {
            return;
        }
        self.sidebar_lifecycle_records += 1;
        record(
            "gxStore.sidebarLifecycle",
            json!({
                "call": log_text(request.call.as_str()),
                "answer": log_text(answer),
                "roundTripMs": round_trip_ms,
                "hadReplacementFocus": request.replacement_focus.is_some(),
                "sleeps": counters.sleeps,
                "wakes": counters.wakes,
                "accepted": counters.accepted,
                "declined": counters.declined,
                "failed": counters.failed,
                "alreadyAgreed": counters.already_agreed,
                "focusFollowUps": counters.focus_follow_ups,
                "declinedSource": counters.declined_source,
            }),
        );
    }

    /// One line per `batch` envelope answered: how many messages it posted and whether it cleared
    /// the multi-selection. No ids: a batch names every selected row.
    pub(super) fn sidebar_batch_ran(
        &mut self,
        messages: usize,
        cleared_selection: bool,
        counters: super::sidebar_bulk::SidebarBulkCounters,
    ) {
        if self.sidebar_lifecycle_records >= MAX_SIDEBAR_ACTION_RECORDS
            || !routine_logging_enabled()
        {
            return;
        }
        self.sidebar_lifecycle_records += 1;
        record(
            "gxStore.sidebarBatch",
            json!({
                "messages": messages as u64,
                "clearedSelection": cleared_selection,
                "batches": counters.batches,
                "batchMessages": counters.batch_messages,
                "batchesClearing": counters.batches_clearing,
                "declinedSource": counters.declined_source,
            }),
        );
    }

    /// One line per plural payload answered: which action, how many rows it resolved, and whether
    /// the fan-out is paced. No ids and no project: the counts are what a support log needs.
    ///
    /// `rows` is the number to read. A project Wake that resolves zero is a project with nothing
    /// asleep in it, which is correct; a Sleep Selected that resolves zero when rows were selected
    /// is not, and only this line can tell the two apart.
    pub(super) fn sidebar_bulk_ran(
        &mut self,
        request: &ghostex_gx_core::BulkRequest,
        counters: super::sidebar_bulk::SidebarBulkCounters,
    ) {
        if self.sidebar_lifecycle_records >= MAX_SIDEBAR_ACTION_RECORDS
            || !routine_logging_enabled()
        {
            return;
        }
        self.sidebar_lifecycle_records += 1;
        record(
            "gxStore.sidebarBulk",
            json!({
                "action": log_text(request.action.as_str()),
                "rows": request.messages.len() as u64,
                "intervalMs": request.interval_ms,
                "focusProject": request.focus_project.is_some(),
                "bulkRequests": counters.bulk_requests,
                "bulkMessages": counters.bulk_messages,
                "pacedRequests": counters.paced_requests,
                "emptyRequests": counters.empty_requests,
                "declinedSource": counters.declined_source,
            }),
        );
    }

    /// One line per snooze menu row answered: how many commands it posted, and nothing about the
    /// row. The wake time is NOT recorded; it is a timestamp the user chose for a session of
    /// theirs, and the counters below say everything a support log needs.
    pub(super) fn sidebar_snooze_action_ran(
        &mut self,
        messages: usize,
        counters: super::sidebar_snooze::SidebarSnoozeCounters,
    ) {
        if self.sidebar_lifecycle_records >= MAX_SIDEBAR_ACTION_RECORDS
            || !routine_logging_enabled()
        {
            return;
        }
        self.sidebar_lifecycle_records += 1;
        record(
            "gxStore.sidebarSnoozeAction",
            json!({
                "messages": messages as u64,
                "actions": counters.actions,
                "actionsWithTag": counters.actions_with_tag,
                "actionsEmpty": counters.actions_empty,
                "declinedSource": counters.declined_source,
                "declinedRow": counters.declined_row,
            }),
        );
    }

    /// One line per snooze or unsnooze call: which one, whether the daemon took it, and how long
    /// it took. No session id and no wake time.
    pub(super) fn sidebar_snooze_ran(
        &mut self,
        call: &str,
        accepted: bool,
        round_trip_ms: u64,
        counters: super::sidebar_snooze::SidebarSnoozeCounters,
    ) {
        if self.sidebar_lifecycle_records >= MAX_SIDEBAR_ACTION_RECORDS
            || !routine_logging_enabled()
        {
            return;
        }
        self.sidebar_lifecycle_records += 1;
        record(
            "gxStore.sidebarSnooze",
            json!({
                "call": log_text(call),
                "accepted": accepted,
                "roundTripMs": round_trip_ms,
                "snoozes": counters.snoozes,
                "unsnoozes": counters.unsnoozes,
                "acceptedTotal": counters.accepted,
                "failed": counters.failed,
                "sleeps": counters.sleeps,
                "declinedSource": counters.declined_source,
            }),
        );
    }

    /// One line per dialog the sidebar opened. Which dialog, and whether it was seeded, and
    /// nothing else: the seed IS the user's own title or note.
    pub(super) fn sidebar_modal_opened(
        &mut self,
        is_rename: bool,
        seeded: bool,
        counters: super::sidebar_modals::SidebarModalCounters,
    ) {
        if self.sidebar_lifecycle_records >= MAX_SIDEBAR_ACTION_RECORDS
            || !routine_logging_enabled()
        {
            return;
        }
        self.sidebar_lifecycle_records += 1;
        record(
            "gxStore.sidebarModal",
            json!({
                "modal": log_text(match is_rename {
                    true => "renameSession",
                    false => "sessionNote",
                }),
                "seeded": seeded,
                "renames": counters.renames,
                "notes": counters.notes,
                "declinedSource": counters.declined_source,
                "declinedRow": counters.declined_row,
            }),
        );
    }

    /// One line per flag call: whether the daemon took it, and how long it took.
    ///
    /// No tag id and no session id. A tag is a short fixed word today, but the catalog is the
    /// user's and a custom tag is text they typed, so it is counted and never written.
    pub(super) fn sidebar_flags_ran(
        &mut self,
        accepted: bool,
        round_trip_ms: u64,
        counters: super::sidebar_flags::SidebarFlagsCounters,
    ) {
        if self.sidebar_lifecycle_records >= MAX_SIDEBAR_ACTION_RECORDS
            || !routine_logging_enabled()
        {
            return;
        }
        self.sidebar_lifecycle_records += 1;
        record(
            "gxStore.sidebarFlags",
            json!({
                "accepted": accepted,
                "roundTripMs": round_trip_ms,
                "calls": counters.calls,
                "acceptedTotal": counters.accepted,
                "failed": counters.failed,
                "alreadyAgreed": counters.already_agreed,
                "parksThatSleep": counters.parks_that_sleep,
                "declinedSource": counters.declined_source,
            }),
        );
    }

    /// One line per fork: whether the daemon made one, and how long it took.
    ///
    /// No id and no failure text. The text a fork failure carries is the daemon's or the
    /// transport's and it reaches the user through a toast; it is never written here.
    pub(super) fn sidebar_fork_ran(
        &mut self,
        placed: bool,
        round_trip_ms: u64,
        counters: super::sidebar_lifecycle::SidebarLifecycleCounters,
    ) {
        if self.sidebar_lifecycle_records >= MAX_SIDEBAR_ACTION_RECORDS
            || !routine_logging_enabled()
        {
            return;
        }
        self.sidebar_lifecycle_records += 1;
        record(
            "gxStore.sidebarFork",
            json!({
                "placed": placed,
                "roundTripMs": round_trip_ms,
                "forks": counters.forks,
                "forksPlaced": counters.forks_placed,
                "forksFailed": counters.forks_failed,
            }),
        );
    }

    /// One line per close: what the daemon said, how long it took, and whether the row came back.
    ///
    /// `closesRestored` is the number to watch. Every one of them is a row the TypeScript would
    /// have left missing for the rest of the run, and a run where it is not zero says the daemon
    /// is refusing or dropping closes, which is worth seeing on its own.
    pub(super) fn sidebar_close_ran(
        &mut self,
        answer: &str,
        round_trip_ms: u64,
        counters: super::sidebar_lifecycle::SidebarLifecycleCounters,
    ) {
        if self.sidebar_lifecycle_records >= MAX_SIDEBAR_ACTION_RECORDS
            || !routine_logging_enabled()
        {
            return;
        }
        self.sidebar_lifecycle_records += 1;
        record(
            "gxStore.sidebarClose",
            json!({
                "answer": log_text(answer),
                "roundTripMs": round_trip_ms,
                "closes": counters.closes,
                "closesAccepted": counters.closes_accepted,
                "closesRestored": counters.closes_restored,
                "declinedSource": counters.declined_source,
            }),
        );
    }

    /// One line per sidebar action the store answered: the message type, which calls it made, how
    /// long deciding them took, and the run's totals.
    ///
    /// The record names calls and counts only. The text a copy action carries is a session title,
    /// a project path or a resume command line, and the ids it resolves are the same strings, so
    /// nothing derived from a payload is written beyond its `type`, which is a fixed vocabulary.
    /// `planUs` is the decision, not the call: it is the only part this milestone added to a click
    /// and it is non-zero for the three project-path actions, which resolve the group against the
    /// project facts.
    pub(super) fn sidebar_action_ran(
        &mut self,
        kind: &str,
        plan: &ghostex_gx_core::SidebarActionPlan,
        plan_us: u64,
        counters: super::sidebar_actions::SidebarActionCounters,
    ) {
        if self.sidebar_action_records >= MAX_SIDEBAR_ACTION_RECORDS || !routine_logging_enabled() {
            return;
        }
        self.sidebar_action_records += 1;
        let calls: Vec<serde_json::Value> = plan
            .effects
            .iter()
            .take(SANITIZER_MAX_ENTRIES)
            .map(|effect| {
                serde_json::Value::String(log_text(match effect {
                    ghostex_gx_core::ActionEffect::CopyText { .. } => "copyText".to_string(),
                    // The action name, never the project id the payload carries.
                    ghostex_gx_core::ActionEffect::NativeProjectPathAction { payload } => format!(
                        "nativeProjectPathAction={}",
                        payload
                            .get("action")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("?")
                    ),
                    ghostex_gx_core::ActionEffect::Toast { level, .. } => {
                        format!("toast={}", level.as_str())
                    }
                }))
            })
            .collect();
        record(
            "gxStore.sidebarAction",
            json!({
                "type": log_text(kind.to_string()),
                "calls": calls,
                "planUs": plan_us,
                "handled": counters.handled,
                "nothing": counters.nothing,
                "copyText": counters.copy_text,
                "nativeProjectPath": counters.native_project_path,
                "toast": counters.toast,
                "declinedSource": counters.declined_source,
            }),
        );
    }
}

/// Appends a record after checking it against every rule the log's sanitizer applies.
///
/// CDXC:Sidebar 2026-09-20 WHY:
/// `sanitize_json_value` (support_logs.rs:463) has THREE rules, and this milestone shipped a
/// diagnostic that failed each of them in turn: values below depth 4 become "[depth-capped]",
/// strings over 120 characters or holding a slash become "[redacted]", and an object or array
/// keeps only its first 32 entries and drops the rest in silence. The third is the worst of them
/// because nothing in the output says anything was lost: this summary reached 41 keys over several
/// rounds and quietly stopped reporting the nine timings at the end of it. Checking a record by
/// eye is how all three got through, so it is checked here instead, on the way out, in every debug
/// build.
fn record(event: &'static str, details: serde_json::Value) {
    debug_assert_loggable(&details, 0, event);
    append(event, details);
}

#[cfg(debug_assertions)]
fn debug_assert_loggable(value: &serde_json::Value, depth: usize, event: &str) {
    match value {
        serde_json::Value::String(text) => {
            debug_assert!(
                depth <= SANITIZER_MAX_DEPTH,
                "{event}: a string at depth {depth} is capped"
            );
            debug_assert!(
                text.chars().count() <= SANITIZER_MAX_CHARS
                    && !text.contains('/')
                    && !text.contains('\\')
                    && !text.chars().any(char::is_control),
                "{event}: a string would be redacted"
            );
        }
        serde_json::Value::Array(items) => {
            debug_assert!(
                items.len() <= SANITIZER_MAX_ENTRIES,
                "{event}: an array of {} drops entries past {SANITIZER_MAX_ENTRIES}",
                items.len()
            );
            for item in items {
                debug_assert_loggable(item, depth + 1, event);
            }
        }
        serde_json::Value::Object(entries) => {
            debug_assert!(
                entries.len() <= SANITIZER_MAX_ENTRIES,
                "{event}: an object of {} keys drops the ones past {SANITIZER_MAX_ENTRIES}",
                entries.len()
            );
            for (key, item) in entries {
                debug_assert!(
                    key.chars().count() <= SANITIZER_MAX_CHARS,
                    "{event}: a key would be redacted"
                );
                debug_assert_loggable(item, depth + 1, event);
            }
        }
        other => debug_assert!(
            depth <= SANITIZER_MAX_DEPTH,
            "{event}: {other} sits at depth {depth}, which is capped"
        ),
    }
}

#[cfg(not(debug_assertions))]
fn debug_assert_loggable(_value: &serde_json::Value, _depth: usize, _event: &str) {}

/// A string the log's sanitizer will print rather than replace.
///
/// CDXC:Sidebar 2026-09-20 WHY:
/// `sanitize_string_value` (support_logs.rs:492) replaces a value with `[redacted]` when it runs
/// past 120 characters or contains a slash, a backslash or a control character, and it does that
/// to object KEYS as well as values. Three diagnostics in this milestone have failed at the moment
/// they were needed, twice for the depth cap and once here, so every string these records emit
/// goes through this: over-long values are cut with a marker instead of vanishing, and a slash is
/// replaced rather than taking the whole value with it. Nothing this writes is private: these
/// records carry ids, field names and counts, and the length rule is about paths, which none of
/// them are.
///
/// The depth rule is the other half and is not something a helper can enforce: a value must sit at
/// depth 4 or less, counting `details` as 0. An array of short strings under `details` is depth 2,
/// and `details.<key>[i].<k>[j]` is depth 4, which is the deepest shape any record here uses.
fn log_text(value: impl Into<String>) -> String {
    let value: String = value.into();
    let value = value.replace(['/', '\\'], "|");
    let value: String = value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect();
    let value = if value.chars().count() <= LOG_TEXT_MAX_CHARS {
        value
    } else {
        let kept: String = value.chars().take(LOG_TEXT_MAX_CHARS - 3).collect();
        format!("{kept}...")
    };
    // The rules this has to satisfy are `sanitize_string_value`'s, and a value that fails them is
    // not logged, it is replaced by a word that says nothing. A debug build says so at the point
    // the value is built rather than leaving it to be discovered in a support log months later.
    debug_assert!(
        value.chars().count() <= SANITIZER_MAX_CHARS
            && !value.contains('/')
            && !value.contains('\\')
            && !value.chars().any(char::is_control),
        "a log value must survive the sanitizer"
    );
    value
}

/// `[{id, fields:[...]}]`: the id at depth 3 and each field name at depth 4, both inside the
/// sanitizer's length rule because a field name is one short word.
fn named_fields(entries: &[(String, Vec<String>)]) -> Vec<serde_json::Value> {
    entries
        .iter()
        .map(|(id, fields)| json!({ "id": log_text(id.as_str()), "fields": log_texts(fields) }))
        .collect()
}

fn log_texts<'a>(values: impl IntoIterator<Item = &'a String>) -> Vec<serde_json::Value> {
    values
        .into_iter()
        .map(|value| serde_json::Value::String(log_text(value.as_str())))
        .collect()
}

/// One short string per differing field: the name alone when both sides carry a value, and
/// `name=old` or `name=store` when only one of them does.
fn encode_field(field: &FieldDiff) -> String {
    if let Some((old, store)) = &field.values {
        return format!("{}: old={old} store={store}", field.name);
    }
    match (field.old_has_value, field.store_has_value) {
        (true, false) => format!("{}=old", field.name),
        (false, true) => format!("{}=store", field.name),
        _ => field.name.to_string(),
    }
}

/// Every differing field of a whole record, once.
fn distinct_fields(mismatch: &SidebarMismatch) -> Vec<String> {
    let mut distinct: Vec<String> = Vec::new();
    for field in mismatch
        .top_level
        .iter()
        .chain(mismatch.groups.iter().flat_map(|(_, fields)| fields))
        .chain(mismatch.sessions.iter().flat_map(|(_, fields)| fields))
    {
        let encoded = encode_field(field);
        if !distinct.contains(&encoded) {
            distinct.push(encoded);
        }
    }
    distinct.truncate(MAX_IDS_PER_RECORD);
    distinct
}

fn store_revision(core: &Core) -> Option<i64> {
    core.presentation()
        .loaded(&MachineId::Local)
        .map(|loaded| loaded.revision)
}
