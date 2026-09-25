//! The sidebar's own state inside the app: seeded from client storage once, moved by intents, and
//! written back on a debounce.
//!
//! CDXC:Sidebar 2026-09-21 WHY:
//! This app is the ONLY writer of the three keys. The write used to be gated on the store's list
//! being the drawn one, to keep exactly one writer while the TypeScript sidebar was still a blind
//! whole-object writer of the same keys; M5 piece 7c deleted those writes and M4d part 2 step 6
//! deleted the switch, so the only precondition left is that the READ has landed. Writing before
//! it would measure the difference against an empty state and erase what the user has. Supersedes
//! the 2026-09-20 note.

use std::time::{Duration, Instant};

use ghostex_gx_core::{
    SidebarCollapseDiff, SidebarCollapseState, SidebarPersistSet, SidebarUiIntent, SidebarUiStore,
    hidden_items_into_storage,
};
use serde_json::Value;

use super::sidebar_ui_storage::{
    SidebarUiWrite, SidebarWriteReport, StoredSidebarUi, read_sidebar_ui_state,
    write_sidebar_ui_state,
};
use crate::GhostexGpuiApp;

/// How long intents are folded together before one write runs. A hold on a collapse hotkey, or a
/// Collapse All over thirty projects, must cost one write and not one per step.
const WRITE_DEBOUNCE: Duration = Duration::from_millis(400);
/// How often a write that failed books its own retry before it waits for the next click instead.
/// Nothing is lost when it stops: the change stays owed and the next intent carries it.
const MAX_WRITE_RETRIES: u32 = 3;
/// How long a failed read waits before it is tried again.
///
/// CDXC:Sidebar 2026-09-21 WHY:
/// The note here used to say nothing the user does is lost while the read fails. That was true
/// while the sidebar page was the other writer of these keys and is false since M5 piece 7c: the
/// state still moves and the clicks still land, but nothing reaches storage until a read does, so
/// a run that never read loses every collapse, Space and hidden item at the next restart. Which is
/// why the ladder below does not end in giving up. Supersedes the 2026-09-20 note.
const READ_RETRY: Duration = Duration::from_secs(5);
/// How many fast attempts before the standing slow one takes over. It never gives up.
const MAX_READ_RETRIES: u32 = 6;
/// The standing retry after those, so a database that was locked for a minute still ends up read.
const READ_RETRY_SLOW: Duration = Duration::from_secs(30);
/// Most clicks held while the first read is on its way. A burst larger than this is a user holding
/// a key through a database that will not open, and the list they end up with is the stored one.
const MAX_QUEUED_INTENTS: usize = 64;

/// What happened since the app started. Memory only; the log lines are built from it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SidebarUiCounters {
    pub(crate) intents: u64,
    pub(crate) writes: u64,
    pub(crate) write_failures: u64,
    pub(crate) read_failures: u64,
    /// Values a storage bound refused. The change stays in memory and the next one carries it
    /// again, which is what the TypeScript writer did with the same refusal.
    pub(crate) write_refusals: u64,
    /// Intents that were applied before the stored state landed and were applied again on top of
    /// it, so a click in the first moments is not thrown away.
    pub(crate) replayed_intents: u64,
    /// Clicks dropped because the queue was full while the read kept failing.
    pub(crate) dropped_intents: u64,
    /// Rows put at the front of a Space's memory, which is what a Space switch restores the focus
    /// to. A run with focus changes and Spaces on and a zero here means nothing is being
    /// remembered and every Space switch will land on the first row it finds.
    pub(crate) space_memory_writes: u64,
    /// Sections whose chosen Space was dropped because that Space was deleted.
    pub(crate) space_forgets: u64,
    /// Project slot hotkeys (cmd+ctrl+1..9 by default) this state answered.
    pub(crate) slot_jumps: u64,
    pub(crate) write_max_us: u64,
    /// Where the slowest write's time went, so a slow one says which step was slow rather than
    /// leaving the whole write as the suspect.
    pub(crate) write_open_max_us: u64,
    pub(crate) write_totals_max_us: u64,
    pub(crate) write_begin_max_us: u64,
    pub(crate) write_stored_max_us: u64,
    pub(crate) write_commit_max_us: u64,
    /// The whole storage call, measured on the background thread. The difference between this and
    /// `write_max_us` is the hop onto that thread and back, which is not storage work at all and
    /// was the missing four milliseconds the first instrumentation could not account for.
    pub(crate) write_call_max_us: u64,
}

impl SidebarUiCounters {
    fn note_write(&mut self, report: &SidebarWriteReport) {
        self.write_open_max_us = self.write_open_max_us.max(report.open_us);
        self.write_totals_max_us = self.write_totals_max_us.max(report.totals_us);
        self.write_begin_max_us = self.write_begin_max_us.max(report.begin_us);
        self.write_stored_max_us = self.write_stored_max_us.max(report.stored_us);
        self.write_commit_max_us = self.write_commit_max_us.max(report.commit_us);
        self.write_call_max_us = self.write_call_max_us.max(report.call_us);
    }
}

/// The sidebar's own state and everything the host needs around it.
pub(crate) struct SidebarUiHost {
    pub(super) store: SidebarUiStore,
    /// The state is only written once it is known: writing before the read lands would store an
    /// empty collapse state over the user's own.
    restored: bool,
    restoring: bool,
    read_retries: u32,
    retry_scheduled: bool,
    /// Intents applied before the read landed, in the order they were applied, so they can be
    /// applied again on top of the stored state rather than being lost under it.
    queued_intents: Vec<SidebarUiIntent>,
    /// What the last read or write left in storage, so the next write carries only the difference.
    base_collapse: SidebarCollapseState,
    write_scheduled: bool,
    write_retries: u32,
    /// Bumped by every change, so the list can tell whether this state moved without comparing it.
    generation: u64,
    /// The reveal request this state has already answered.
    handled_reveal: Option<u64>,
    /// The focused row the Space was last followed into. `rememberNativeSidebarFocus` runs on a
    /// CHANGE of focused session, as `rememberNativeSidebarFocus` did; running it on every update
    /// instead would drag the selection back every time the user picked another Space.
    followed_session: Option<String>,
    /// The sidebar's own copy of the project collections, read with the rest of the state.
    pub(super) stored_project_collections: Option<Value>,
    pub(super) counters: SidebarUiCounters,
    pub(super) last_error: Option<&'static str>,
}

impl Default for SidebarUiHost {
    fn default() -> Self {
        Self {
            store: SidebarUiStore::new(),
            restored: false,
            restoring: false,
            read_retries: 0,
            retry_scheduled: false,
            queued_intents: Vec::new(),
            base_collapse: SidebarCollapseState::default(),
            write_scheduled: false,
            write_retries: 0,
            generation: 0,
            handled_reveal: None,
            followed_session: None,
            stored_project_collections: None,
            counters: SidebarUiCounters::default(),
            last_error: None,
        }
    }
}

impl SidebarUiHost {
    /// The state the list is built from. Empty defaults until the first read lands.
    pub(crate) fn state(&self) -> &ghostex_gx_core::SidebarUiState {
        self.store.state()
    }

    /// Whether client storage has been read. The Rust list is not drawn before it has, because a
    /// list built from empty collapse state would show every project expanded for a moment.
    pub(crate) fn restored(&self) -> bool {
        self.restored
    }

    /// Moves whenever the state moves.
    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    /// The code of the newest failed read, or `None` while the last read succeeded.
    pub(super) fn last_error(&self) -> Option<&'static str> {
        self.last_error
    }

    /// Whether this reveal request has already been answered, asked without taking it.
    pub(super) fn reveal_handled(&self, request_id: u64) -> bool {
        self.handled_reveal == Some(request_id)
    }

    /// Whether this reveal request is a new one. Answered once, like the renderer's own.
    pub(super) fn take_reveal_request(&mut self, request_id: u64) -> bool {
        if self.handled_reveal == Some(request_id) {
            return false;
        }
        self.handled_reveal = Some(request_id);
        true
    }

    /// Whether this focused row is a new one to follow. Answered once per row, like the
    /// TypeScript's comparison against the previous focused session.
    pub(super) fn take_followed_session(&mut self, sidebar_session_id: Option<&str>) -> bool {
        let next = sidebar_session_id.map(str::to_string);
        if self.followed_session == next {
            return false;
        }
        self.followed_session = next;
        true
    }

    pub(crate) fn selected_machine_id(&self) -> &str {
        self.store.selected_machine_id()
    }

    /// What a write still owes.
    pub(super) fn store_pending(&self) -> ghostex_gx_core::SidebarPersistSet {
        self.store.pending()
    }

    /// Drops rows the list no longer draws from the multi-selection.
    pub(super) fn retain_selected_sessions(&mut self, keep: impl FnMut(&str) -> bool) -> bool {
        let moved = self.store.retain_selected_sessions(keep);
        if moved {
            self.generation += 1;
        }
        moved
    }

    /// Drops ticked tag filters the Sort & Filter menu no longer offers, which is what the old
    /// projection did on every build. A tag turned off in Settings, or a custom tag the daemon
    /// dropped, stops filtering here as well as in the list, and does not start again if it comes
    /// back.
    pub(super) fn retain_tag_filters(&mut self, offered: &[String]) -> bool {
        if self.store.state().selected_tag_filters.is_empty() {
            return false;
        }
        let moved = self.store.retain_tag_filters(offered);
        if moved {
            self.generation += 1;
        }
        moved
    }

    /// Takes the stored state as the truth and applies whatever the user did while it was on its
    /// way on top of it.
    fn adopt(&mut self, stored: StoredSidebarUi) {
        let state = ghostex_gx_core::SidebarUiState {
            selected_machine_id: stored.selected_machine_id,
            collapse: stored.collapse.clone(),
            hidden_items: stored.hidden_items,
            // Show Hidden, the tag filters and the multi-selection start over on every launch,
            // exactly as the TypeScript state did: none of them is persisted.
            ..ghostex_gx_core::SidebarUiState::default()
        };
        self.base_collapse = stored.collapse;
        self.stored_project_collections = stored.project_collections;
        self.store.restore(state);
        self.restored = true;
        self.generation += 1;
        for intent in std::mem::take(&mut self.queued_intents) {
            self.counters.replayed_intents += 1;
            self.store.apply(intent);
        }
    }

    /// The write the pending changes amount to, what it was computed against, and the set to owe
    /// again if it never reaches storage.
    fn take_write(&mut self) -> (SidebarUiWrite, SidebarCollapseState, SidebarPersistSet) {
        let pending = self.store.take_pending();
        let state = self.store.state();
        let mut write = SidebarUiWrite::default();
        if pending.collapse {
            let diff = SidebarCollapseDiff::between(&self.base_collapse, &state.collapse);
            if !diff.is_empty() {
                write.collapse = Some((diff, state.collapse.clone()));
            }
        }
        if pending.hidden_items {
            write.hidden_items = Some(hidden_items_into_storage(&state.hidden_items));
        }
        if pending.machine_tab {
            write.selected_machine_id = Some(state.selected_machine_id.clone());
        }
        (write, state.collapse.clone(), pending)
    }
}

impl GhostexGpuiApp {
    /// Reads the sidebar's own state from client storage. Runs once; a failed read books its own
    /// retry, because the only other caller runs inside the first second and would otherwise be
    /// the last attempt the run ever makes.
    pub(crate) fn gx_store_restore_sidebar_ui(&mut self, cx: &mut gpui::Context<Self>) {
        let ui = &mut self.gx_store.sidebar_ui;
        if ui.restored || ui.restoring || ui.retry_scheduled {
            return;
        }
        ui.restoring = true;
        cx.spawn(async move |this, cx| {
            let stored = cx
                .background_executor()
                .spawn(async move { read_sidebar_ui_state() })
                .await;
            let _ = this.update(cx, |this, cx| {
                let ui = &mut this.gx_store.sidebar_ui;
                ui.restoring = false;
                match stored {
                    Ok(stored) => {
                        ui.last_error = None;
                        ui.adopt(stored);
                        // Everything the list reads moved at once; it is rebuilt from scratch.
                        this.gx_store_sidebar_state_changed(cx);
                    }
                    Err(code) => {
                        ui.counters.read_failures += 1;
                        ui.last_error = Some(code);
                        this.gx_store.diagnostics.sidebar_ui_read_failed(code);
                        this.gx_store_schedule_sidebar_ui_read_retry(cx);
                    }
                }
            });
        })
        .detach();
    }

    /// Books another read after a failure: `MAX_READ_RETRIES` fast attempts, then a standing slow
    /// one for the life of the run (see `READ_RETRY` for why it never gives up). The moment it
    /// stopped trying would be the moment nothing said so, which is why the last fast attempt
    /// warns.
    fn gx_store_schedule_sidebar_ui_read_retry(&mut self, cx: &mut gpui::Context<Self>) {
        let ui = &mut self.gx_store.sidebar_ui;
        if ui.retry_scheduled {
            return;
        }
        let exhausted = ui.read_retries >= MAX_READ_RETRIES;
        ui.read_retries += 1;
        ui.retry_scheduled = true;
        if exhausted && ui.read_retries == MAX_READ_RETRIES + 1 {
            self.gx_store.diagnostics.sidebar_ui_read_unavailable();
        }
        let delay = match exhausted {
            true => READ_RETRY_SLOW,
            false => READ_RETRY,
        };
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update(cx, |this, cx| {
                this.gx_store.sidebar_ui.retry_scheduled = false;
                this.gx_store_restore_sidebar_ui(cx);
            });
        })
        .detach();
    }

    /// Applies one intent and books the write it owes. Returns whether the list must be rebuilt.
    pub(crate) fn gx_store_apply_sidebar_ui_intent(
        &mut self,
        intent: SidebarUiIntent,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let changed = self.gx_store_apply_sidebar_ui_intent_unbuilt(intent, cx);
        if changed {
            self.gx_store_sidebar_state_changed(cx);
        }
        changed
    }

    /// Applies several intents in order and rebuilds the list ONCE, after the last. Returns how
    /// many of them moved the state. A reveal and a slot jump are several intents that the user
    /// sees as one change, and a rebuild per intent was a full build of the list for each.
    pub(crate) fn gx_store_apply_sidebar_ui_intents(
        &mut self,
        intents: Vec<SidebarUiIntent>,
        cx: &mut gpui::Context<Self>,
    ) -> usize {
        let mut changed = 0;
        for intent in intents {
            if self.gx_store_apply_sidebar_ui_intent_unbuilt(intent, cx) {
                changed += 1;
            }
        }
        if changed > 0 {
            self.gx_store_sidebar_state_changed(cx);
        }
        changed
    }

    /// One intent, its queueing before the stored state lands and its write, without the rebuild.
    pub(super) fn gx_store_apply_sidebar_ui_intent_unbuilt(
        &mut self,
        intent: SidebarUiIntent,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let ui = &mut self.gx_store.sidebar_ui;
        ui.counters.intents += 1;
        if !ui.restored {
            // The stored state is still on its way, and it replaces this one when it lands, so the
            // click is kept to be applied again on top of it.
            if ui.queued_intents.len() < MAX_QUEUED_INTENTS {
                ui.queued_intents.push(intent.clone());
            } else {
                ui.counters.dropped_intents += 1;
            }
        }
        let outcome = ui.store.apply(intent);
        if !outcome.changed {
            return false;
        }
        ui.generation += 1;
        if !ui.store.pending().is_empty() {
            self.gx_store_schedule_sidebar_ui_write(cx);
        }
        true
    }

    /// Reports the values a bound refused and OWES each of them again.
    ///
    /// CDXC:Sidebar 2026-09-21 WHY:
    /// A refusal is not retried on its own, because the payload does not shrink by trying again;
    /// what it must not do is disappear. `take_write` has already cleared the pending set, so a
    /// refused value is only in memory, and the collapse envelope happens to self-heal (its next
    /// diff is measured against a base this write did not advance) while the hidden items and the
    /// machine tab do not: hide a project, have the write refused, never touch hidden items again,
    /// and that project is back after a restart. Each refused key is owed again so the next write
    /// carries it, which is also what makes a refusal that was caused by ANOTHER key's growth
    /// recover on its own once that key shrinks.
    fn gx_store_note_sidebar_write_refusals(&mut self, report: &SidebarWriteReport) {
        let mut owed = SidebarPersistSet::default();
        for refusal in &report.refused {
            self.gx_store.sidebar_ui.counters.write_refusals += 1;
            match refusal.key {
                "collapse" => owed.collapse = true,
                "hiddenItems" => owed.hidden_items = true,
                "machineTab" => owed.machine_tab = true,
                _ => {}
            }
            self.gx_store
                .diagnostics
                .sidebar_ui_write_refused(refusal.key, refusal.bound);
        }
        if !owed.is_empty() {
            self.gx_store.sidebar_ui.store.mark_pending(owed);
        }
    }

    /// Books a write for anything still owed, which is what a session whose read landed after its
    /// first clicks has.
    pub(super) fn gx_store_write_owed_sidebar_ui_state(&mut self, cx: &mut gpui::Context<Self>) {
        if self.gx_store.sidebar_ui.store_pending().is_empty() {
            return;
        }
        self.gx_store_schedule_sidebar_ui_write(cx);
    }

    /// Books one write for everything the intents since the last one changed.
    fn gx_store_schedule_sidebar_ui_write(&mut self, cx: &mut gpui::Context<Self>) {
        let ui = &mut self.gx_store.sidebar_ui;
        // Nothing is written before the read landed: the difference would be measured against an
        // empty state and would erase what the user has.
        if !ui.restored || ui.write_scheduled {
            return;
        }
        ui.write_scheduled = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(WRITE_DEBOUNCE).await;
            let Ok(Some((write, base, owed))) = this.update(cx, |this, _| {
                this.gx_store.sidebar_ui.write_scheduled = false;
                let (write, base, owed) = this.gx_store.sidebar_ui.take_write();
                (!write.is_empty()).then_some((write, base, owed))
            }) else {
                return;
            };
            let started = Instant::now();
            let stored = write.collapse.is_some();
            let result = cx
                .background_executor()
                .spawn(async move {
                    let called = Instant::now();
                    let mut result = write_sidebar_ui_state(&write);
                    if let Ok(report) = &mut result {
                        report.call_us = called.elapsed().as_micros() as u64;
                    }
                    result
                })
                .await;
            let elapsed = started.elapsed().as_micros() as u64;
            let _ = this.update(cx, |this, cx| {
                let ui = &mut this.gx_store.sidebar_ui;
                ui.counters.write_max_us = ui.counters.write_max_us.max(elapsed);
                match result {
                    Ok(report) => {
                        ui.counters.writes += 1;
                        ui.counters.note_write(&report);
                        ui.last_error = None;
                        ui.write_retries = 0;
                        if stored
                            && !report
                                .refused
                                .iter()
                                .any(|refusal| refusal.key == "collapse")
                        {
                            // Stored now; the next write carries what changes from here.
                            ui.base_collapse = base;
                        }
                        this.gx_store_note_sidebar_write_refusals(&report);
                    }
                    Err(code) => {
                        ui.counters.write_failures += 1;
                        ui.last_error = Some(code);
                        // The change is still only in memory, so it is owed again rather than
                        // dropped: the next click, or the retry below, carries it with the rest.
                        ui.store.mark_pending(owed);
                        let retry = ui.write_retries < MAX_WRITE_RETRIES;
                        ui.write_retries += 1;
                        this.gx_store.diagnostics.sidebar_ui_write_failed(code);
                        if retry {
                            this.gx_store_schedule_sidebar_ui_write(cx);
                        }
                    }
                }
            });
        })
        .detach();
    }
}

impl GhostexGpuiApp {
    /// Stores whatever the debounce still owes, synchronously, on the quit path. Without it the
    /// last four hundred milliseconds of clicks never reach the database: the task that would have
    /// written them dies with the app.
    pub(crate) fn gx_store_flush_sidebar_ui_write(&mut self) {
        if !self.gx_store.sidebar_ui.restored() {
            return;
        }
        let (write, base, owed) = self.gx_store.sidebar_ui.take_write();
        if write.is_empty() {
            return;
        }
        let ui = &mut self.gx_store.sidebar_ui;
        match write_sidebar_ui_state(&write) {
            Ok(report) => {
                ui.counters.writes += 1;
                ui.counters.note_write(&report);
                if write.collapse.is_some()
                    && !report
                        .refused
                        .iter()
                        .any(|refusal| refusal.key == "collapse")
                {
                    ui.base_collapse = base;
                }
                self.gx_store_note_sidebar_write_refusals(&report);
            }
            Err(code) => {
                ui.counters.write_failures += 1;
                ui.last_error = Some(code);
                ui.store.mark_pending(owed);
                self.gx_store.diagnostics.sidebar_ui_write_failed(code);
            }
        }
    }
}
