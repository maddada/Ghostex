use std::time::{Duration, Instant};

use ghostex_gx_client::{ClientDiagnostic, StartError, redact_quoted_values};
use ghostex_gx_core::{ConnectionUpdate, Core, Loadable, MachineId, ProjectKey, ResubscribeReason};
use serde_json::{Value, json};

use super::host::GxStoreCounters;
use super::focus_perform::FocusPerformCounters;
use super::focus_publish::FocusPublishCounters;
use super::sidebar_list::{LastUpdate, SidebarListCounters};
use super::sidebar_scratch_compare::ScratchDifference;
use super::sidebar_self_check::SidebarSelfCheckCounters;
use super::sidebar_ui::SidebarUiCounters;
use crate::{shared_settings, support_logs};

/// Records of the kept list disagreeing with a fresh one. Each one is a bug, so a handful is
/// plenty to name it and the counter carries the rate.
const MAX_SCRATCH_RECORDS: u32 = 4;
/// Records of a sidebar action. One per click is the whole rate, and the totals ride in each one,
/// so the cap only stops a renderer that repeats a command from filling the log.
pub(super) const MAX_SIDEBAR_ACTION_RECORDS: u32 = 200;
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
pub(super) const PERIODIC_SUMMARY_INTERVAL: Duration = Duration::from_secs(60);

/// Log lines of the store, all in the `native.sidebar.refresh` support log.
///
/// Routine lines (`gxStore.loaded`, `gxStore.connection`, `gxStore.focus.summary`) are written only
/// while "Show debug UI controls" and that scenario are on, which the support log enforces.
/// Warnings (a frame that does not parse, snapshot rows that were skipped) are written always,
/// capped per run. Every line holds ids, counts, enum names, and field names: never a title, a
/// path, or frame content.
#[derive(Default)]
pub(crate) struct GxStoreDiagnostics {
    warning_lines: u32,
    /// When the summary was last considered, so a run with logging off reads the settings at
    /// most once per interval, and the totals it last wrote.
    focus_summary_considered_at: Option<Instant>,
    focus_summary_written: (FocusPublishCounters, FocusPerformCounters),
    sidebar_summary_considered_at: Option<Instant>,
    sidebar_summary_written: SidebarSelfCheckCounters,
    sidebar_ui_summary_considered_at: Option<Instant>,
    sidebar_ui_summary_written: SidebarUiCounters,
    /// The runtime facts channel's periodic line (`diagnostics_runtime_facts.rs`).
    pub(super) runtime_facts_summary_at: Option<Instant>,
    #[allow(clippy::type_complexity)]
    pub(super) runtime_facts_summary_written: Option<(
        super::runtime_facts::RuntimeFactsCounters,
        super::sidebar_runtime_route::SidebarRuntimeRouteCounters,
    )>,
    /// The budget of `gxStore.sidebarCommandUnroutable`, which repeats for as long as the command
    /// that has no owner keeps being posted.
    pub(super) unroutable_command_warnings: u32,
    sidebar_refusal_warnings: u32,
    workspace_groups_records: u32,
    workspace_groups_summary_at: Option<Instant>,
    client_document_records: u32,
    /// The PERIODIC line's own budget.
    ///
    /// CDXC:Projects 2026-09-21 WHY:
    /// The summary emits three records per interval (both documents and the moves) and used to
    /// spend `client_document_records`, so a quiet run of an hour exhausted the two hundred and
    /// then a real push or a real move had no line left to write. The periodic path exists to say
    /// "nothing has happened"; it must not be what silences the path that says something did.
    client_document_summary_records: u32,
    client_document_read_warnings: u32,
    client_document_write_warnings: u32,
    client_document_refusal_warnings: u32,
    client_document_unparsable_warnings: u32,
    client_document_summary_at: Option<Instant>,
    #[allow(clippy::type_complexity)]
    client_document_summary_written: Option<(
        super::client_document::ClientDocumentCounters,
        super::client_document::ClientDocumentCounters,
        super::project_docs::ProjectMoveCounters,
    )>,
    workspace_groups_summary_written:
        Option<(super::workspace_groups::WorkspaceGroupsCounters, bool)>,
    workspace_groups_read_warnings: u32,
    workspace_groups_write_warnings: u32,
    workspace_groups_refusal_warnings: u32,
    /// The budget of the lines in `diagnostics_project_docs.rs`, apart from
    /// `client_document_records` so a busy launch cannot silence the proof that a Project Group
    /// menu item, a Space editor result or a Space switch ran at all.
    pub(super) project_doc_edit_records: u32,
    sidebar_scratch_records: u32,
    sidebar_slow_update_records: u32,
    sidebar_storage_warnings: u32,
    sidebar_action_records: u32,
    pub(super) sidebar_lifecycle_records: u32,
    sidebar_drag_records: u32,
    pub(super) remote_last_seen_records: u64,
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

    pub(super) fn warning(&mut self, event: &str, details: serde_json::Value) {
        if self.warning_lines >= MAX_WARNING_LINES {
            return;
        }
        self.warning_lines += 1;
        append(event, details);
    }

    /// The focus the store published and performed, at most once a minute and only when it moved.
    /// Counts only.
    pub(super) fn focus_summary(
        &mut self,
        publish: FocusPublishCounters,
        perform: FocusPerformCounters,
        core: &Core,
    ) {
        if (publish, perform) == self.focus_summary_written
            || self
                .focus_summary_considered_at
                .is_some_and(|at| at.elapsed() < PERIODIC_SUMMARY_INTERVAL)
        {
            return;
        }
        self.focus_summary_considered_at = Some(Instant::now());
        if !routine_logging_enabled() {
            return;
        }
        self.focus_summary_written = (publish, perform);
        let active_tabs = match core.active_tab_sessions() {
            Loadable::Loaded(tabs) => Some(tabs.len()),
            Loadable::NotLoaded | Loadable::Missing => None,
        };
        record(
            "gxStore.focus.summary",
            json!({
                "publishes": publish.publishes,
                "contexts": publish.contexts,
                "unplacedHolds": publish.unplaced_holds,
                "unplacedPlaced": publish.unplaced_placed,
                "startupRestores": publish.startup_restores,
                "sessionFocuses": perform.sessions,
                "remoteSessionFocuses": perform.remote_sessions,
                "groupFocuses": perform.groups,
                "wakes": perform.wakes,
                "refused": perform.refused,
                "storeRevision": store_revision(core),
                "storeActiveTabs": active_tabs,
                "tabsGeneration": core.tabs_generation(),
            }),
        );
    }
}

impl GxStoreDiagnostics {
    /// The running totals of the sidebar list and its self check, at most once a minute and only
    /// when they moved.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn sidebar_summary(
        &mut self,
        counters: &SidebarSelfCheckCounters,
        list: &SidebarListCounters,
        ready: bool,
        // Which of the two legs the list waits for was given up on and drawn without
        // (gx_store/sidebar_ready.rs). Both false on an ordinary run.
        recovered: (bool, bool),
        deadline_kind: &'static str,
        groups: usize,
        rows: usize,
        phases: super::sidebar_snapshot::InstallPhases,
        remote: &super::remote_clients::RemoteClientCounters,
        machines: usize,
    ) {
        if *counters == self.sidebar_summary_written
            || self
                .sidebar_summary_considered_at
                .is_some_and(|at| at.elapsed() < PERIODIC_SUMMARY_INTERVAL)
        {
            return;
        }
        self.sidebar_summary_considered_at = Some(Instant::now());
        if !routine_logging_enabled() {
            return;
        }
        self.sidebar_summary_written = *counters;
        record(
            "gxStore.sidebarList.summary",
            json!({
                // The list is the only one there is; this says whether it is the REAL one or the
                // loading skeleton the launch window draws.
                "ready": ready,
                // Ready only because a leg was declared absent, which the `.error` line names.
                "recoveredHud": recovered.0,
                "recoveredState": recovered.1,
                "storeGroups": groups,
                "storeRows": rows,
                "deadlineKind": deadline_kind,
                // Grouped rather than flat: the sanitizer keeps the first 32 keys of an object and
                // drops the rest without saying so, and this record passed 32 as it grew.
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
                    "installsFromCarry": list.installs_from_carry,
                    "loadingInstalls": list.loading_installs,
                    "deadlineWakes": list.deadline_wakes,
                    "wakeRowsMoved": list.wake_rows_moved,
                    "wakeRowsMovedMax": list.wake_rows_moved_max,
                },
                "timings": {
                    "updateUs": list.last_update_us,
                    "updateMaxUs": list.update_max_us,
                    "installUs": list.last_install_us,
                    "installMaxUs": list.install_max_us,
                    "relabelUs": list.last_relabel_us,
                    "relabelMaxUs": list.relabel_max_us,
                },
                // Where the newest install's time went, and what the caches saved it. A rise in
                // installUs says which part of the build it came from rather than inviting a
                // guess: the shared key, the rows, the group menus, the collection menus, the more
                // menu, or the tail that copies what the view model does not own.
                "install": {
                    "hostUs": phases.host_us,
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

    /// The running totals of the sidebar's own state and its writes, on the same schedule.
    pub(super) fn sidebar_ui_summary(&mut self, counters: &SidebarUiCounters) {
        if *counters == self.sidebar_ui_summary_written
            || self
                .sidebar_ui_summary_considered_at
                .is_some_and(|at| at.elapsed() < PERIODIC_SUMMARY_INTERVAL)
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

    /// The sidebar's own state could not be read after every fast attempt. Said once, as a
    /// warning rather than a routine line, because from here nothing the user collapses, hides or
    /// filters will survive a restart until a read lands.
    pub(super) fn sidebar_ui_read_unavailable(&mut self) {
        self.warning("gxStore.sidebarUi.read.unavailable", json!({}));
    }

    /// A bound refused the workspace session groups document.
    ///
    /// CDXC:Sessions 2026-09-21 WHY:
    /// One budget per condition, not one for all three. They shared `workspace_groups_warnings`
    /// until 2026-09-21, and since the read retries every five seconds a run whose database is
    /// briefly locked burns the whole budget on read failures inside fifteen seconds and then says
    /// nothing at all about a write that is being refused for the rest of the run.
    pub(super) fn workspace_groups_write_refused(&mut self, bound: &'static str) {
        if self.workspace_groups_refusal_warnings >= 3 {
            return;
        }
        self.workspace_groups_refusal_warnings += 1;
        self.warning(
            "gxStore.workspaceGroups.write.refused",
            json!({ "bound": bound }),
        );
    }

    /// A read of the stored document that did not land. Retried; until it does, nothing is adopted
    /// and nothing is edited.
    pub(super) fn workspace_groups_read_failed(&mut self, error: &'static str) {
        if self.workspace_groups_read_warnings >= 3 {
            return;
        }
        self.workspace_groups_read_warnings += 1;
        self.warning(
            "gxStore.workspaceGroups.read.failed",
            json!({ "error": error }),
        );
    }

    /// The stored document could not be read after every fast attempt. Said once, because from here
    /// every edit of the user's groups is refused until a read lands.
    pub(super) fn workspace_groups_read_unavailable(&mut self) {
        self.warning("gxStore.workspaceGroups.read.unavailable", json!({}));
    }

    /// A storage write of that document that did not land. Retried; the warning says it happened.
    pub(super) fn workspace_groups_write_failed(&mut self, error: &'static str) {
        if self.workspace_groups_write_warnings >= 3 {
            return;
        }
        self.workspace_groups_write_warnings += 1;
        self.warning(
            "gxStore.workspaceGroups.write.failed",
            json!({ "error": error }),
        );
    }

    /// A read of a client-owned document's stored key that did not land. Retried; until it does,
    /// nothing is adopted and nothing is edited.
    pub(super) fn client_document_read_failed(&mut self, name: &'static str, error: &'static str) {
        if self.client_document_read_warnings >= 3 {
            return;
        }
        self.client_document_read_warnings += 1;
        self.warning(
            "gxStore.clientDocument.read.failed",
            json!({ "document": name, "error": error }),
        );
    }

    /// The stored key could not be read after every fast attempt. Said once per document.
    pub(super) fn client_document_read_unavailable(&mut self, name: &'static str) {
        self.warning(
            "gxStore.clientDocument.read.unavailable",
            json!({ "document": name }),
        );
    }

    /// A storage write of a client-owned document that did not land. Retried; the warning says so.
    pub(super) fn client_document_write_failed(&mut self, name: &'static str, error: &'static str) {
        if self.client_document_write_warnings >= 3 {
            return;
        }
        self.client_document_write_warnings += 1;
        self.warning(
            "gxStore.clientDocument.write.failed",
            json!({ "document": name, "error": error }),
        );
    }

    /// A storage bound refused a client-owned document. Its OWN warning budget, because a shared
    /// one let three read failures inside fifteen seconds silence every later refusal.
    pub(super) fn client_document_write_refused(
        &mut self,
        name: &'static str,
        bound: &'static str,
    ) {
        if self.client_document_refusal_warnings >= 3 {
            return;
        }
        self.client_document_refusal_warnings += 1;
        self.warning(
            "gxStore.clientDocument.write.refused",
            json!({ "document": name, "bound": bound }),
        );
    }

    /// An echo that was not a document at all.
    ///
    /// CDXC:Projects 2026-09-21 WHY:
    /// LOUD on purpose. The collections and Spaces documents parse an echo through the wire types
    /// so the guard and the store's own side state cannot disagree about what the daemon said, and
    /// the price is that ONE malformed entry takes the whole echo with it where the TypeScript kept
    /// the others. A user whose collections quietly stopped following the daemon would never find
    /// that cause from the outside, so it is warned as well as counted and carried in every record.
    pub(super) fn client_document_echo_unparsable(&mut self, name: &'static str) {
        if self.client_document_unparsable_warnings >= 3 {
            return;
        }
        self.client_document_unparsable_warnings += 1;
        self.warning(
            "gxStore.clientDocument.echo.unparsable",
            json!({ "document": name }),
        );
    }

    /// One line per client-owned document: what it has done this run, and whether the last push
    /// landed. `ok` is `None` on the periodic path.
    pub(super) fn client_document_record(
        &mut self,
        name: &'static str,
        ok: Option<bool>,
        counters: super::client_document::ClientDocumentCounters,
    ) {
        self.client_document_record_inner(name, ok, counters, false);
    }

    /// `periodic` picks which budget this line spends. The two are separate because the periodic
    /// path writes three lines an interval and would otherwise silence the push and move records.
    fn client_document_record_inner(
        &mut self,
        name: &'static str,
        ok: Option<bool>,
        counters: super::client_document::ClientDocumentCounters,
        periodic: bool,
    ) {
        let budget = match periodic {
            true => &mut self.client_document_summary_records,
            false => &mut self.client_document_records,
        };
        if *budget >= MAX_SIDEBAR_ACTION_RECORDS || !routine_logging_enabled() {
            return;
        }
        *budget += 1;
        record(
            "gxStore.clientDocument",
            json!({
                "document": name,
                "ok": ok,
                "edits": counters.edits,
                "storageWrites": counters.storage_writes,
                "storageRemoves": counters.storage_removes,
                "storageAttempts": counters.storage_attempts,
                "storageFailures": counters.storage_failures,
                "storageRefusals": counters.storage_refusals,
                "pushes": counters.pushes,
                "pushFailures": counters.push_failures,
                "echoesRefused": counters.echoes_refused,
                "echoesAdopted": counters.echoes_adopted,
                "echoesEqual": counters.echoes_equal,
                "echoesAbsent": counters.echoes_absent,
                "echoesUnparsable": counters.echoes_unparsable,
                "echoesPushedBack": counters.echoes_pushed_back,
                "echoesDeferred": counters.echoes_deferred,
                "deferredRecovered": counters.deferred_recovered,
                "readFailures": counters.read_failures,
            }),
        );
    }

    /// The same counters on the periodic path, so a run in which the user moved no project still
    /// says whether the two documents reached the app at all. The first line is emitted with every
    /// counter at zero on purpose: "the path never ran" is the answer that was missing twice.
    pub(super) fn client_document_summary(
        &mut self,
        collections: super::client_document::ClientDocumentCounters,
        spaces: super::client_document::ClientDocumentCounters,
        moves: super::project_docs::ProjectMoveCounters,
    ) {
        if self
            .client_document_summary_written
            .is_some_and(|written| written == (collections, spaces, moves))
            || self
                .client_document_summary_at
                .is_some_and(|at| at.elapsed() < PERIODIC_SUMMARY_INTERVAL)
        {
            return;
        }
        self.client_document_summary_at = Some(Instant::now());
        if !routine_logging_enabled() {
            return;
        }
        self.client_document_summary_written = Some((collections, spaces, moves));
        self.client_document_record_inner("collections", None, collections, true);
        self.client_document_record_inner("spaces", None, spaces, true);
        self.project_move_summary(moves);
    }

    /// What the project moves did this run. Separate from the documents' own line because it is
    /// about the GESTURE, and a run with moves but no document edit is the shape that says the
    /// planner refused every one of them.
    fn project_move_summary(&mut self, counters: super::project_docs::ProjectMoveCounters) {
        if self.client_document_summary_records >= MAX_SIDEBAR_ACTION_RECORDS {
            return;
        }
        self.client_document_summary_records += 1;
        record(
            "gxStore.projectMove",
            json!({
                "moves": counters.moves,
                "refusals": counters.refusals,
                "handOffs": counters.hand_offs,
                "declinedSource": counters.declined_source,
                "collectionEdits": counters.collection_edits,
                "spaceEdits": counters.space_edits,
                "groupOrders": counters.group_orders,
                "renameRequests": counters.rename_requests,
                "spaceEditors": counters.space_editors,
            }),
        );
    }

    /// One line per project move the store answered: what it wrote, and the run's totals.
    pub(super) fn project_move_ran(
        &mut self,
        plan: &ghostex_gx_core::ProjectMovePlan,
        counters: super::project_docs::ProjectMoveCounters,
    ) {
        if self.client_document_records >= MAX_SIDEBAR_ACTION_RECORDS || !routine_logging_enabled()
        {
            return;
        }
        self.client_document_records += 1;
        record(
            "gxStore.projectMove",
            json!({
                "writes": plan.writes.len() as u64,
                "refusal": plan.refusal.unwrap_or("none"),
                "moves": counters.moves,
                "refusals": counters.refusals,
                "handOffs": counters.hand_offs,
                "declinedSource": counters.declined_source,
                "collectionEdits": counters.collection_edits,
                "spaceEdits": counters.space_edits,
                "groupOrders": counters.group_orders,
            }),
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
    /// A Full Reload, named by its legs rather than by a label: the record says how many went out
    /// and which row they were for, never what the session is.
    pub(super) fn sidebar_reload_ran(
        &mut self,
        plan: &ghostex_gx_core::ReloadPlan,
        counters: super::sidebar_lifecycle::SidebarLifecycleCounters,
    ) {
        if self.sidebar_lifecycle_records >= MAX_SIDEBAR_ACTION_RECORDS
            || !routine_logging_enabled()
        {
            return;
        }
        self.sidebar_lifecycle_records += 1;
        record(
            "gxStore.sidebarReload",
            json!({
                "legs": plan.legs.len() as u64,
                "reloads": counters.reloads,
                "reloadLegs": counters.reload_legs,
                "reloadsStopped": counters.reloads_stopped,
                "remounts": counters.remounts,
                "declinedSource": counters.declined_source,
            }),
        );
    }

    /// A drag, named by what it posted and never by the row it moved. The refusal reason is a
    /// fixed word from a closed list, so it can say WHY nothing happened without carrying an id.
    ///
    /// CDXC:Sidebar 2026-09-21 WHY:
    /// Every value here is a count or one of a handful of fixed words, which is what keeps it
    /// through `sanitize_json_value`: the sanitizer caps depth at 4, redacts a string over 120
    /// characters or containing a slash, and silently drops an object past 32 entries. An order is
    /// a list of session ids, each one containing a colon and a project path fragment, so it is
    /// reported as a LENGTH and never as itself.
    pub(super) fn sidebar_move_ran(
        &mut self,
        plan: &ghostex_gx_core::SessionMovePlan,
        counters: super::sidebar_drag::SidebarDragCounters,
    ) {
        if self.sidebar_drag_records >= MAX_SIDEBAR_ACTION_RECORDS || !routine_logging_enabled() {
            return;
        }
        self.sidebar_drag_records += 1;
        record(
            "gxStore.sidebarDrag",
            json!({
                "posts": plan.messages.len() as u64,
                "refusal": plan.refusal.unwrap_or("none"),
                "moves": counters.moves,
                "movePosts": counters.move_posts,
                "moveRefusals": counters.move_refusals,
                "declinedSource": counters.declined_source,
            }),
        );
    }

    /// What one order message wrote. `kind` is the message type, which is one of three fixed
    /// strings, and `rows` is how long the order was, never the order itself.
    pub(super) fn sidebar_order_write_ran(
        &mut self,
        message: &Value,
        plan: &ghostex_gx_core::OrderWritePlan,
        counters: super::sidebar_drag::SidebarDragCounters,
    ) {
        if self.sidebar_drag_records >= MAX_SIDEBAR_ACTION_RECORDS || !routine_logging_enabled() {
            return;
        }
        self.sidebar_drag_records += 1;
        record(
            "gxStore.sidebarOrderWrite",
            json!({
                "kind": message.get("type").and_then(Value::as_str).unwrap_or("?"),
                "rows": message
                    .get("sessionIds")
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    .unwrap_or(0) as u64,
                "writes": plan.writes.len() as u64,
                "refusal": plan.refusal.unwrap_or("none"),
                "orderWrites": counters.order_writes,
                "documentEdits": counters.document_edits,
                "sessionOrderCalls": counters.session_order_calls,
                "activations": counters.activations,
                "toasts": counters.toasts,
            }),
        );
    }

    /// A push of the workspace session groups document, and what the guard has seen so far.
    /// `echoesRefused` is the guard doing its job; a run with edits and a zero there means the
    /// window between an edit and its push never opened.
    pub(super) fn workspace_groups_pushed(
        &mut self,
        ok: bool,
        counters: super::workspace_groups::WorkspaceGroupsCounters,
    ) {
        self.workspace_groups_record(Some(ok), counters, None);
    }

    /// The same counters on the periodic path, whether or not anything has happened.
    ///
    /// CDXC:Sessions 2026-09-21 WHY:
    /// **A record that is emitted at ONE instant is a record that is not there when it is read.**
    /// This one was emitted only from a push, so a run with no group edit had no line; it was then
    /// also emitted from the reconcile, which fires once, at about a second into the run, and two
    /// live rounds produced no line either, because `routine_logging_enabled()` reads the shared
    /// settings snapshot and everything through `record()` is silent until that snapshot is warm,
    /// while `gxStore.loaded` is not (it calls `append` directly, which is why it was always there
    /// to mislead). So the counters ride the SAME periodic path as `gxStore.sidebarShadow.summary`,
    /// which is proved to reach the log in a quiet run, and the first line is emitted even when
    /// every counter is zero, because "the path never ran" is the answer that was missing twice.
    pub(super) fn workspace_groups_summary(
        &mut self,
        counters: super::workspace_groups::WorkspaceGroupsCounters,
        side_state_held: bool,
    ) {
        if self
            .workspace_groups_summary_written
            .is_some_and(|written| written == (counters, side_state_held))
            || self
                .workspace_groups_summary_at
                .is_some_and(|at| at.elapsed() < PERIODIC_SUMMARY_INTERVAL)
        {
            return;
        }
        self.workspace_groups_summary_at = Some(Instant::now());
        if !routine_logging_enabled() {
            return;
        }
        self.workspace_groups_summary_written = Some((counters, side_state_held));
        self.workspace_groups_record(None, counters, Some(side_state_held));
    }

    fn workspace_groups_record(
        &mut self,
        ok: Option<bool>,
        counters: super::workspace_groups::WorkspaceGroupsCounters,
        side_state_held: Option<bool>,
    ) {
        if self.workspace_groups_records >= MAX_SIDEBAR_ACTION_RECORDS || !routine_logging_enabled()
        {
            return;
        }
        self.workspace_groups_records += 1;
        record(
            "gxStore.workspaceGroups",
            json!({
                "ok": ok,
                "edits": counters.edits,
                "storageWrites": counters.storage_writes,
                "storageRemoves": counters.storage_removes,
                "storageAttempts": counters.storage_attempts,
                "storageFailures": counters.storage_failures,
                "pushes": counters.pushes,
                "pushFailures": counters.push_failures,
                "echoesRefused": counters.echoes_refused,
                "echoesAdopted": counters.echoes_adopted,
                "echoesEqual": counters.echoes_equal,
                "echoesAbsent": counters.echoes_absent,
                // Its own key, not folded into `echoesAbsent`: an outcome standing for two is what
                // made `echoesRefused` count the guard never being asked. Expected to stay at zero
                // for this document, which is what makes a non-zero value worth reading.
                "echoesUnparsable": counters.echoes_unparsable,
                "echoesPushedBack": counters.echoes_pushed_back,
                "hostMessagesDropped": super::workspace_groups::native_host_messages_dropped(),
                "prunes": counters.prunes,
                "storageRefusals": counters.storage_refusals,
                "readFailures": counters.read_failures,
                "echoesDeferred": counters.echoes_deferred,
                "deferredRecovered": counters.deferred_recovered,
                "reconcileSeen": counters.reconcile_seen,
                "reconcileEntered": counters.reconcile_entered,
                // Whether the store holds the daemon's copy at all. With `reconcileSeen` this
                // separates the three answers the last two rounds could not tell apart: absent
                // means the document never reached the side state, held with `reconcileSeen` zero
                // means it arrived without ever being reported as a CHANGE, and held with
                // `reconcileSeen` non-zero means the host saw it.
                "sideStateHeld": side_state_held,
            }),
        );
    }

    /// A Split Right, named by which of the two branches the row took.
    pub(super) fn sidebar_split_ran(
        &mut self,
        plan: &ghostex_gx_core::SplitPlan,
        counters: super::sidebar_lifecycle::SidebarLifecycleCounters,
    ) {
        if self.sidebar_lifecycle_records >= MAX_SIDEBAR_ACTION_RECORDS
            || !routine_logging_enabled()
        {
            return;
        }
        self.sidebar_lifecycle_records += 1;
        record(
            "gxStore.sidebarSplit",
            json!({
                "action": log_text(match plan.action {
                    ghostex_gx_core::SplitAction::Nothing => "nothing",
                    ghostex_gx_core::SplitAction::Wake(_) => "wake",
                    ghostex_gx_core::SplitAction::Focus => "focus",
                }),
                "splits": counters.splits,
                "splitsWoken": counters.splits_woken,
                "splitsPlaced": counters.splits_placed,
                "declinedSource": counters.declined_source,
            }),
        );
    }

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
                "remoteProject": request.is_remote_project(),
                "bulkRequests": counters.bulk_requests,
                "bulkMessages": counters.bulk_messages,
                "pacedRequests": counters.paced_requests,
                "remoteRequests": counters.remote_requests,
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
                    // The open family never reaches this line, which is the read-only one's, but
                    // naming the effects here keeps a planner that started emitting one from
                    // being dropped silently. The modal NAME is a fixed word this store builds;
                    // the payload beside it carries project paths and is never named.
                    ghostex_gx_core::ActionEffect::CloseAppModal => "closeAppModal".to_string(),
                    ghostex_gx_core::ActionEffect::OpenAppModal { payload } => format!(
                        "openAppModal={}",
                        payload
                            .get("modal")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("?")
                    ),
                    // Every other effect by its NAME only (a settings patch or a launch message is
                    // the user's). Line-neutral: this file is over its ceiling.
                    other => other.call_name().to_string(),
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
pub(super) fn record(event: &'static str, details: serde_json::Value) {
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
pub(super) fn log_text(value: impl Into<String>) -> String {
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

fn store_revision(core: &Core) -> Option<i64> {
    core.presentation()
        .loaded(&MachineId::Local)
        .map(|loaded| loaded.revision)
}
