//! This computer's own presentation, stored so the next launch draws the settled sidebar on its
//! first frame, and the warm start that reads it back before that frame.
//!
//! CDXC:Startup 2026-11-16 DECISION:
//! User: "my sidebar when i launch ghostex looks like this [the remote computers' rows, every
//! project open, no Space row, the title 'Terminal Session'] then it switches to this (actual
//! settled look it should've had from the start). please fix this i want the start up to be
//! instant and sidebar to show correct details from the start, not this bad loading way we have
//! now". Until gxserver answered (seconds after a rebuild, which restarts it) this computer had no
//! rows at all: the Spaces document, the collections, the pinned sessions and the session the
//! title names all come with them, so the first frames drew only the remote computers' last-seen
//! copies under a Space row that did not exist yet. The rows the last run held are stored the way
//! a remote computer's are (gx_store/remote_last_seen.rs, same catalog store, its own key) and
//! seeded as held-not-live rows (`seed_last_seen`) before the first frame; gxserver's first
//! snapshot replaces them and the list moves only where the two differ.
//!
//! **Read synchronously, on purpose, once.** The warm start runs inside the window's construction,
//! before the first frame, which is the only place the result can be on the first frame. It is a
//! few SQLite reads on a database the app opens anyway, timed into `gpui-startup-timing.log`
//! (`warmStart`), and nothing on the pump path ever reads here.
//!
//! **Written rarely.** This computer's revision moves with every agent's activity, so the copy is
//! rebuilt at most once per `WRITE_DELAY` while live, plus the quit flush, which is the write the
//! next launch reads in the normal case. Seeded rows are never written back: they ARE the copy.
//!
//! SEE-ALSO: packages/gx-core/src/presentation_store/apply.rs (`seed_last_seen`, and the reload a
//! replaced copy counts as), apps/desktop/src/app/gx_store/sidebar_ui.rs (the sidebar's own state,
//! which the warm start also reads before the first frame).

use std::time::{Duration, Instant};

use ghostex_gx_core::{MachineId, snapshot_storage_json};
use serde_json::{Value, json};

use super::records_storage::{RecordRead, RecordWrite, read_record_raw, write_record};
use super::remote_last_seen::STORE;
use crate::GhostexGpuiApp;
use crate::support_logs;

/// The key of this computer's copy, in the same catalog store as the remote computers' copies
/// (`remotePresentations`, prefix-keyed). Outside `remote_last_seen::MACHINE_KEY_PREFIX`, so the
/// prune of removed remote machines never sees it.
const LOCAL_KEY: &str = "ghostex-gpui-remote-last-seen-presentations:local";

/// How long a change waits before the copy is rebuilt. Building it clones every row of this
/// computer on the UI thread (about a megabyte of JSON on a busy computer), so it is rare; the quit
/// flush stores the final state regardless.
const WRITE_DELAY: Duration = Duration::from_secs(60);

/// The write side's state.
#[derive(Default)]
pub(crate) struct LocalLastSeenWriter {
    /// The revision the copy was last built from.
    written_revision: Option<i64>,
    /// A payload built and not stored yet.
    owed: Option<String>,
    booked: bool,
    /// The first live load of this run has been timed into the startup log.
    live_noted: bool,
    pub(crate) counters: LocalLastSeenCounters,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct LocalLastSeenCounters {
    pub(crate) writes: u64,
    pub(crate) unchanged: u64,
    pub(crate) refusals: u64,
    pub(crate) failures: u64,
    pub(crate) largest_bytes: u64,
    pub(crate) build_max_us: u64,
}

/// What the warm start read and how long each part took.
#[derive(Default)]
struct WarmStart {
    sidebar_ui_us: u64,
    sidebar_ui: &'static str,
    local_us: u64,
    local: &'static str,
    local_bytes: usize,
}

impl GhostexGpuiApp {
    /// Reads the sidebar's own state and this computer's stored rows before the first frame, so the
    /// list is drawn from them at once rather than as a skeleton and then as a placeholder list.
    /// Runs once per window; everything it skips (a failed read, a missing copy) falls back to the
    /// asynchronous paths that ran before it existed.
    pub(super) fn gx_store_warm_start(&mut self, cx: &mut gpui::Context<Self>) {
        if self.gx_store.warm_started {
            return;
        }
        self.gx_store.warm_started = true;
        let mut report = WarmStart::default();

        let started = Instant::now();
        report.sidebar_ui = self.gx_store_restore_sidebar_ui_now();
        report.sidebar_ui_us = started.elapsed().as_micros() as u64;

        let started = Instant::now();
        let now_ms = super::host::now_ms() as i64;
        report.local = match read_record_raw(STORE, LOCAL_KEY, now_ms) {
            Err(code) => code,
            Ok(RecordRead::Missing) => "missing",
            Ok(RecordRead::Expired) => "expired",
            Ok(RecordRead::Payload(raw)) => {
                report.local_bytes = raw.len();
                match serde_json::from_str::<ghostex_gx_core::protocol::PresentationSnapshot>(&raw)
                {
                    Err(_) => "parse",
                    Ok(snapshot) => {
                        let output = self
                            .gx_store
                            .core
                            .seed_last_seen_presentation(&MachineId::Local, snapshot);
                        if output.changes.is_empty() {
                            "ignored"
                        } else {
                            self.gx_store.sidebar_list.note_changes(&output.changes);
                            self.gx_store.run_effects(output.effects);
                            "seeded"
                        }
                    }
                }
            }
        };
        report.local_us = started.elapsed().as_micros() as u64;
        if report.local == "seeded" || report.sidebar_ui == "restored" {
            self.gx_store_sidebar_state_changed(cx);
        }
        support_logs::append_startup_timing(
            "warmStart",
            json!({
                "sidebarUi": report.sidebar_ui,
                "sidebarUiMs": report.sidebar_ui_us as f64 / 1000.0,
                "local": report.local,
                "localMs": report.local_us as f64 / 1000.0,
                "localBytes": report.local_bytes,
            }),
        );
    }

    /// The sidebar's state as the startup log describes it: whether the list is the real one and
    /// where this computer's rows came from.
    pub(crate) fn gx_store_startup_sidebar_facts(&self) -> Value {
        let local = match self.gx_store.core.presentation().loaded(&MachineId::Local) {
            None => "none",
            Some(loaded) if loaded.last_seen => "lastSeen",
            Some(_) => "live",
        };
        let view = self.gx_store.sidebar_list.view();
        json!({
            "sidebarReady": self.gx_store_sidebar_list_ready(),
            "local": local,
            "groups": view.groups.len(),
            "spaces": view.spaces.len(),
        })
    }

    /// A pump moved this computer's rows: the first live load is timed, and the stored copy owes
    /// a rebuild.
    pub(super) fn gx_store_note_local_last_seen_change(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(revision) = self
            .gx_store
            .core
            .presentation()
            .loaded_live(&MachineId::Local)
            .map(|loaded| loaded.revision)
        else {
            return;
        };
        if !self.gx_store.local_last_seen.live_noted {
            self.gx_store.local_last_seen.live_noted = true;
            support_logs::append_startup_timing("localLive", self.gx_store_startup_sidebar_facts());
        }
        let writer = &mut self.gx_store.local_last_seen;
        if writer.written_revision == Some(revision) || writer.booked {
            return;
        }
        writer.booked = true;
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            background.timer(WRITE_DELAY).await;
            let _ = this.update(cx, |this, cx| {
                this.gx_store.local_last_seen.booked = false;
                this.gx_store_build_local_last_seen();
                this.gx_store_run_local_last_seen_write(cx);
            });
        })
        .detach();
    }

    /// Builds the payload from the live rows, if they moved since the last build.
    fn gx_store_build_local_last_seen(&mut self) {
        let started = Instant::now();
        let built = self
            .gx_store
            .core
            .presentation()
            .machine(&MachineId::Local)
            .filter(|entry| entry.loaded().is_some_and(|loaded| !loaded.last_seen))
            .and_then(|entry| entry.to_snapshot())
            .filter(|snapshot| {
                self.gx_store.local_last_seen.written_revision != Some(snapshot.revision)
            })
            .map(|snapshot| (snapshot.revision, snapshot_storage_json(&snapshot)));
        let Some((revision, raw)) = built else {
            return;
        };
        let writer = &mut self.gx_store.local_last_seen;
        writer.counters.build_max_us = writer
            .counters
            .build_max_us
            .max(started.elapsed().as_micros() as u64);
        writer.counters.largest_bytes = writer.counters.largest_bytes.max(raw.len() as u64);
        writer.written_revision = Some(revision);
        writer.owed = Some(raw);
    }

    fn gx_store_run_local_last_seen_write(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(raw) = self.gx_store.local_last_seen.owed.clone() else {
            return;
        };
        let now_ms = super::host::now_ms() as i64;
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let attempted = raw.clone();
            let result = background
                .spawn(async move { write_record(STORE, LOCAL_KEY, &raw, now_ms) })
                .await;
            let _ = this.update(cx, |this, _| {
                this.gx_store_note_local_last_seen_write(&attempted, result);
            });
        })
        .detach();
    }

    /// A failed write stays owed; the next booking or the quit flush stores it.
    fn gx_store_note_local_last_seen_write(
        &mut self,
        attempted: &str,
        result: Result<RecordWrite, &'static str>,
    ) {
        let writer = &mut self.gx_store.local_last_seen;
        let current = writer.owed.as_deref() == Some(attempted);
        match result {
            Ok(outcome) => {
                if current {
                    writer.owed = None;
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
                self.gx_store.diagnostics.remote_last_seen_failed(code);
            }
        }
    }

    /// Stores the final rows synchronously on the quit path: this is the copy the next launch
    /// draws its first frame from.
    pub(crate) fn gx_store_flush_local_last_seen(&mut self) {
        self.gx_store_build_local_last_seen();
        let Some(raw) = self.gx_store.local_last_seen.owed.take() else {
            return;
        };
        let now_ms = super::host::now_ms() as i64;
        let result = write_record(STORE, LOCAL_KEY, &raw, now_ms);
        self.gx_store_note_local_last_seen_write(&raw, result);
    }
}
