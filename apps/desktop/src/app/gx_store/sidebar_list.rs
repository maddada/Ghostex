//! The sidebar list inside the app: built from the store and the sidebar's own state, kept up to
//! date, and the only list the renderer draws.
//!
//! CDXC:Sidebar 2026-09-21 DECISION:
//! User: the desktop app stops running product logic in QuickJS; one Rust state store owns it, and
//! every interaction is a local state change plus one redraw. The list is derived here rather than
//! projected in QuickJS and posted over a bridge. It is rebuilt only when the store, the sidebar's
//! own state, the settings or the clock deadline of a row moved, never per frame, and an update
//! with nothing changed does no work at all. This supersedes the 2026-09-20 note that the
//! `sidebarListSource` setting chose between two lists: M4d part 2 step 6 deleted the setting and
//! the other list. A settings file that still holds the key is ignored, because the normalizer
//! drops what it does not know.

use std::time::{Duration, Instant};

use ghostex_gx_core::{
    ChangeSummary, ConnectionPhase, FocusState, MachineId, SidebarInputs, SidebarMenus,
    SidebarSettings, SidebarView, SidebarViewModel, UnavailableState,
};
use serde_json::Value;

use super::host::now_ms;
use super::sidebar_list_inputs::{DESKTOP_SORT_MODE, InputsCache, refresh_inputs};
use super::sidebar_snapshot::{SnapshotCache, SnapshotInput, snapshot_from_view};
use crate::GhostexGpuiApp;

/// How long the settings verdict is reused. Asking stats the settings file and clones its map
/// under a global lock, and an update can run several times a second.
const SETTINGS_MAX_AGE: Duration = Duration::from_millis(1000);
/// The earliest a clock deadline is allowed to wake the list, so a row whose countdown ends in a
/// millisecond does not book a timer per millisecond.
const MIN_DEADLINE_WAIT: Duration = Duration::from_millis(50);
/// An update at or over this many microseconds is recorded with what it rebuilt. Five milliseconds
/// is well inside a frame at sixty hertz and well above the median, which is tens of microseconds,
/// so this fires for outliers and not for traffic.
const SLOW_UPDATE_US: u64 = 5_000;

/// Everything the installed list carries that neither the view model nor the settings decide.
///
/// CDXC:Sidebar 2026-09-21 WHY:
/// A key of cheap numbers rather than a hash of the values. The old carry fingerprint walked the
/// whole HUD document, which was affordable once per publish and is not affordable on the path an
/// update takes: since step 3 an update runs on every focus move, and holding "next tab" would
/// have hashed a few hundred kilobytes of HUD per keystroke. Every part here moves only when its
/// own source moved, so the comparison is four cheap numbers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SidebarCarryKey {
    /// The runtime facts holder's HUD (gx_store/hud/), which the snapshot and the menus read.
    hud_generation: u64,
    /// The settings content hash the two hotkey labels were formatted at.
    shortcuts_hash: u64,
    rename_request_id: Option<u64>,
    reveal_request_id: Option<u64>,
}

/// What the newest update was handed, in the shape a log line needs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LastUpdate {
    pub(crate) changes_empty: bool,
    pub(crate) sessions_changed: usize,
    pub(crate) sessions_removed: usize,
    pub(crate) projects_changed: usize,
    pub(crate) projects_removed: usize,
    pub(crate) session_order_changed: usize,
    pub(crate) project_order_changed: usize,
    pub(crate) machines_reloaded: usize,
    pub(crate) focus_changed: bool,
    pub(crate) workspace_groups: bool,
    pub(crate) project_collections: bool,
    pub(crate) spaces: bool,
    pub(crate) custom_session_tags: bool,
    /// Something besides the store marked the list.
    pub(crate) dirty: bool,
    /// The sidebar's own state moved.
    pub(crate) ui_generation_moved: bool,
    /// The settings the rows depend on moved.
    pub(crate) settings_moved: bool,
}

/// What happened since the app started. Memory only; the log lines are built from it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SidebarListCounters {
    pub(crate) updates: u64,
    /// Updates that found nothing to do.
    pub(crate) idle: u64,
    pub(crate) view_changes: u64,
    pub(crate) installs: u64,
    /// Wakes that found no drawn time had moved, so the list was left as it was.
    pub(crate) installs_skipped: u64,
    /// Installs made because a value the list carries from outside the view model moved (the HUD,
    /// the two requests, the two hotkey labels), rather than because the list itself did.
    pub(crate) installs_from_carry: u64,
    /// The loading skeleton, drawn while the sidebar's own state or the HUD has not landed. One per
    /// launch is expected; a second one means the list went back to not being ready.
    pub(crate) loading_installs: u64,
    pub(crate) deadline_wakes: u64,
    /// Rows whose drawn time had really moved, summed over every wake, and the most any one wake
    /// moved. A wake per second with one row each is a row in its first minute; a wake per second
    /// with none is a booking that should not have been made.
    pub(crate) wake_rows_moved: u64,
    pub(crate) wake_rows_moved_max: u64,
    /// What servicing a wake costs now that it relabels rows in place instead of rebuilding the
    /// list. The pair to compare against `installUs`.
    pub(crate) last_relabel_us: u64,
    pub(crate) relabel_max_us: u64,
    pub(crate) update_max_us: u64,
    pub(crate) install_max_us: u64,
    pub(crate) last_update_us: u64,
    pub(crate) last_install_us: u64,
}

/// The Rust list and everything the host keeps around it.
pub(crate) struct SidebarList {
    model: SidebarViewModel,
    /// What the store changed since the last update.
    changes: ChangeSummary,
    /// Something besides the store moved (the sidebar's own state, the app's browser tabs, a new
    /// post of the values still mirrored).
    dirty: bool,
    settings: Option<(u64, SidebarSettings)>,
    /// The two hotkey labels the list carries, and the settings content hash they were read at.
    shortcuts: Option<(u64, Option<String>, Option<String>)>,
    settings_read_at: Option<Instant>,
    unavailable: UnavailableState,
    /// Focus as the newest build saw it. The view model compares it too, but only after the whole
    /// input set has been assembled and compared; this is what the cheap gate reads.
    last_focus: Option<FocusState>,
    /// Whether a list has ever been built, so the first update is never skipped.
    built: bool,
    /// The sidebar state generation the newest update read.
    last_ui_generation: u64,
    /// Whether the diagnostic scenario is on, on the same one-second gate and for the same reason:
    /// asking takes the settings lock, and the per-update CPU clock below is only read while
    /// someone is measuring.
    measuring: std::cell::Cell<Option<(Instant, bool)>>,
    /// The clock deadline a timer is already booked for, and what asked for it, so a wake once a
    /// second can be read as a countdown doing its job or as something booking a time nobody draws.
    deadline_booked: Option<u64>,
    pub(super) deadline_kind: &'static str,
    /// What the installed list carries from outside the view model, and the generation of the
    /// client-storage values its menus read. A change to either is installed; an update that moved
    /// neither and did not move the list itself is not.
    installed_carry: Option<(SidebarCarryKey, u64)>,
    /// Whether the once-a-second tick is running (started once, at the store's bootstrap).
    pub(super) clock_started: bool,
    /// Which of the two legs the list waits for were given up on, and when the wait started
    /// (gx_store/sidebar_ready.rs).
    ready_recovery: super::sidebar_ready::SidebarReadyRecovery,
    /// Whether the loading skeleton is what the renderer is drawing, so it is installed once and
    /// not rebuilt on every update of the launch window.
    loading_installed: bool,
    snapshot_cache: SnapshotCache,
    inputs_cache: InputsCache,
    /// The inputs the newest view was built from, so the shadow compares the same moment. Kept
    /// rather than rebuilt: an update that changed one session must not re-parse the browser tabs
    /// or clone the sidebar's whole state.
    pub(super) last_inputs: SidebarInputs,
    /// What the newest update was given, so a difference between the kept list and a fresh one can
    /// say what the cache was reacting to when it went wrong.
    pub(super) last_update: LastUpdate,
    pub(super) last_built_at_ms: u64,
    pub(super) counters: SidebarListCounters,
}

impl Default for SidebarList {
    fn default() -> Self {
        Self {
            model: SidebarViewModel::default(),
            changes: ChangeSummary::default(),
            dirty: false,
            settings: None,
            shortcuts: None,
            settings_read_at: None,
            unavailable: UnavailableState::default(),
            last_focus: None,
            built: false,
            last_ui_generation: 0,
            measuring: std::cell::Cell::new(None),
            deadline_booked: None,
            deadline_kind: "none",
            installed_carry: None,
            clock_started: false,
            ready_recovery: super::sidebar_ready::SidebarReadyRecovery::default(),
            loading_installed: false,
            snapshot_cache: SnapshotCache::default(),
            inputs_cache: InputsCache::default(),
            last_inputs: SidebarInputs::default(),
            last_update: LastUpdate::default(),
            last_built_at_ms: 0,
            counters: SidebarListCounters::default(),
        }
    }
}

impl SidebarList {
    pub(super) fn ready_recovery(&self) -> &super::sidebar_ready::SidebarReadyRecovery {
        &self.ready_recovery
    }

    pub(super) fn ready_recovery_mut(&mut self) -> &mut super::sidebar_ready::SidebarReadyRecovery {
        &mut self.ready_recovery
    }

    pub(crate) fn view(&self) -> &SidebarView {
        self.model.view()
    }

    /// The model itself, for readers of the groups it built (Back/Forward's trail stop).
    pub(crate) fn model(&self) -> &SidebarViewModel {
        &self.model
    }

    /// The session Close Project focuses before it parks the project
    /// (gx_store/sidebar_close_project.rs).
    pub(crate) fn close_project_successor_session_id(&self, group_id: &str) -> Option<String> {
        self.model.close_project_successor_session_id(group_id)
    }

    /// Folds what one pump changed into what the next update must apply.
    pub(super) fn note_changes(&mut self, changes: &ChangeSummary) {
        self.changes.merge(changes.clone());
    }

    /// Something besides the store moved.
    pub(super) fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// The settings the list depends on, re-read at most once a second. The two hotkey labels the
    /// list carries are formatted in the same place, so they cost a settings read only when the
    /// file's content hash really moved.
    fn settings(&mut self) -> SidebarSettings {
        if let Some((_, settings)) = &self.settings {
            if self
                .settings_read_at
                .is_some_and(|read_at| read_at.elapsed() < SETTINGS_MAX_AGE)
            {
                return settings.clone();
            }
        }
        let saved = crate::shared_settings::shared_sidebar_settings_snapshot();
        self.settings_read_at = Some(Instant::now());
        if let Some((hash, settings)) = &self.settings {
            if *hash == saved.content_hash() {
                return settings.clone();
            }
        }
        let settings = SidebarSettings::from_settings_json(
            &Value::Object(saved.object().clone()),
            DESKTOP_SORT_MODE,
        );
        self.settings = Some((saved.content_hash(), settings.clone()));
        self.shortcuts = Some((
            saved.content_hash(),
            crate::app::hotkeys::gpui_configured_hotkey_label("openSessionSearchPalette"),
            crate::app::hotkeys::gpui_configured_hotkey_label("openCommandPalette"),
        ));
        settings
    }

    /// The Session Search and Commands hotkey labels, as the newest settings read formatted them.
    fn shortcuts(&self) -> (Option<String>, Option<String>) {
        match &self.shortcuts {
            Some((_, search, commands)) => (search.clone(), commands.clone()),
            None => (None, None),
        }
    }

    /// The settings content hash the hotkey labels were read at, which is what tells an install
    /// that they moved without hashing the labels themselves.
    fn shortcuts_hash(&self) -> u64 {
        self.shortcuts.as_ref().map_or(0, |(hash, _, _)| *hash)
    }

    /// The next moment one of the drawn times reads differently. The view model does not hold the
    /// labels, so this is the host's own deadline beside the one the list reports.
    fn next_label_deadline(
        &self,
        now_ms: u64,
        show_relative_time: bool,
    ) -> Option<ghostex_gx_core::LabelDeadline> {
        self.model
            .view()
            .groups
            .iter()
            .flat_map(|group| group.core.sessions.iter())
            .filter_map(|session| session.row.next_label_deadline(now_ms, show_relative_time))
            .min_by_key(|deadline| deadline.at_ms())
    }

    /// Whether a card draws the relative time at all (`hideLastActiveTimeOnSessionCards`).
    fn show_relative_time(&self) -> bool {
        self.settings
            .as_ref()
            .map(|(_, settings)| settings.show_last_active_time)
            .unwrap_or(true)
    }

    /// Whether an update would find nothing to do. Only the cheap signals are read here; the view
    /// model still decides what is actually rebuilt.
    fn nothing_moved(&self, settings: &SidebarSettings, focus: &FocusState, now_ms: u64) -> bool {
        self.built
            && !self.dirty
            && self.changes.is_empty()
            && self.last_focus.as_ref() == Some(focus)
            && self.last_inputs.settings == *settings
            && !self
                .model
                .next_deadline_ms()
                .is_some_and(|deadline| now_ms >= deadline)
    }

    /// Whether the per-update CPU clock is worth two syscalls right now: only while the diagnostic
    /// scenario that would record it is on. The verdict is reused for a second, because asking
    /// takes the settings lock this is partly trying to measure around.
    fn measuring(&self) -> bool {
        if let Some((taken_at, measuring)) = self.measuring.get() {
            if taken_at.elapsed() < SETTINGS_MAX_AGE {
                return measuring;
            }
        }
        let measuring = super::diagnostics::routine_logging_enabled();
        self.measuring.set(Some((Instant::now(), measuring)));
        measuring
    }

    /// Tracks how long the local daemon has been unavailable, which the empty-state copy reads.
    fn note_machine_state(&mut self, loaded: bool, now_ms: u64) {
        if loaded {
            self.unavailable.since_ms = None;
            self.unavailable.observed_available = true;
        } else if self.unavailable.since_ms.is_none() {
            self.unavailable.since_ms = Some(now_ms);
        }
    }
}

impl GhostexGpuiApp {
    /// What the installed list carries from outside the view model right now.
    ///
    /// CDXC:Sidebar 2026-09-20 WHY:
    /// Until M4c every accepted publish forced an install, because the list carried that publish's
    /// menus, hover buttons and header buttons by id. The menus are the store's now, and so are the
    /// machine tabs since M4d and their failure message since M4d part 2 step 3, so what is left is
    /// what `sidebar_snapshot.rs` names: the HUD, the two requests and the two hotkey labels. The
    /// two client-storage values the menus read (the last-used agent and the armed keep-awake
    /// duration) are NOT here: reading them takes a storage round trip, and they have their own
    /// one-second poll (`gx_store_poll_menu_host`) because nothing else would install for them.
    fn gx_store_sidebar_carry_key(&self) -> SidebarCarryKey {
        let store = &self.gx_store;
        SidebarCarryKey {
            hud_generation: store.runtime_facts.hud_generation,
            shortcuts_hash: store.sidebar_list.shortcuts_hash(),
            rename_request_id: store
                .pending_collection_rename
                .as_ref()
                .map(|(_, request_id)| *request_id),
            reveal_request_id: store
                .runtime_facts
                .newest_reveal
                .as_ref()
                .map(|reveal| reveal.request_id),
        }
    }

    /// The sidebar's own state moved. The list is rebuilt at once, because a click must show in
    /// the same frame.
    pub(crate) fn gx_store_sidebar_state_changed(&mut self, cx: &mut gpui::Context<Self>) {
        self.gx_store.sidebar_list.mark_dirty();
        self.gx_store_update_sidebar_list(cx);
    }

    /// The store's focus moved outside a pump: a local selection, the old runtime's focus payload
    /// mirrored in, or a shadow judgement that mirrored it again. The list is brought up to date at
    /// once; the update compares the focus by value and returns at once when it did not move.
    ///
    /// CDXC:Sidebar 2026-09-21 WHY:
    /// The list used to follow focus only at the next pump or publish, because a focus move is in
    /// no change summary. The row highlight hid it (it reads the store's focus directly), but the
    /// project's active mark, the section's and the Space row's "holds the focused session" marks
    /// kept the previous focus until then, and the live scratch check caught it five times in one
    /// day as `spaces`, `isActive`, `sections`, `isFocused`, `isVisible` on an update with no store
    /// change. Gate: the "focus moves without a frame" section of gx-core `sidebar_replay`.
    pub(crate) fn gx_store_sidebar_focus_moved(&mut self, cx: &mut gpui::Context<Self>) {
        self.gx_store_update_sidebar_list(cx);
    }

    /// Rebuilds the list if anything it reads moved, installs it when the renderer is on it, and
    /// books the next clock deadline. Cheap to call: an update with nothing changed returns at
    /// once.
    pub(crate) fn gx_store_update_sidebar_list(&mut self, cx: &mut gpui::Context<Self>) {
        self.gx_store_publish_if_focus_moved(cx);
        let now_ms = now_ms();
        let machine = self.gx_store.core.presentation().machine(&MachineId::Local);
        let loaded = machine.is_some_and(|machine| machine.loaded().is_some());
        let live =
            machine.is_some_and(|machine| machine.connection().phase == ConnectionPhase::Live);
        let _ = live;
        self.gx_store
            .sidebar_list
            .note_machine_state(loaded, now_ms);

        let settings = self.gx_store.sidebar_list.settings();
        // Nothing the list reads moved: the burst carried no change it draws, no click or channel
        // post marked it, focus stands where it did, no row's own clock has run out, and the settings
        // read the same. Returning here is what keeps a pump that changed nothing free, rather
        // than re-assembling the inputs and comparing them field by field inside the view model.
        if self
            .gx_store
            .sidebar_list
            .nothing_moved(&settings, self.gx_store.core.focus(), now_ms)
        {
            self.gx_store.sidebar_list.counters.idle += 1;
            return;
        }
        self.gx_store.sidebar_list.last_focus = Some(self.gx_store.core.focus().clone());
        // Before the generation is read: pruning is a change to the sidebar's own state and the
        // inputs have to carry it.
        self.gx_store_prune_sidebar_tag_filters(&settings);
        let ui_generation = self.gx_store.sidebar_ui.generation();
        let changes = std::mem::take(&mut self.gx_store.sidebar_list.changes);
        let dirty = std::mem::take(&mut self.gx_store.sidebar_list.dirty);
        self.gx_store_hud_store_changed(&changes, cx);
        self.gx_store_quick_access_store_changed(true, cx);
        let mut inputs = std::mem::take(&mut self.gx_store.sidebar_list.last_inputs);
        let unavailable = self.gx_store.sidebar_list.unavailable;
        let store = &mut self.gx_store;
        refresh_inputs(
            &mut inputs,
            &mut store.sidebar_list.inputs_cache,
            store.sidebar_ui.state(),
            ui_generation,
            settings,
            &store.runtime_facts,
            &store.sidebar_ui.stored_project_collections,
            unavailable,
            store.remote.tabs(),
        );
        self.gx_store_indicators_changed(&changes, &inputs, cx);
        let settings_moved = self.gx_store.sidebar_list.last_inputs.settings != inputs.settings;
        let last_update = LastUpdate {
            changes_empty: changes.is_empty(),
            sessions_changed: changes.sessions_changed.len(),
            sessions_removed: changes.sessions_removed.len(),
            projects_changed: changes.projects_changed.len(),
            projects_removed: changes.projects_removed.len(),
            session_order_changed: changes.session_order_changed.len(),
            project_order_changed: changes.project_order_changed.len(),
            machines_reloaded: changes.machines_reloaded.len(),
            focus_changed: changes.focus_changed,
            workspace_groups: changes.side_state.workspace_groups,
            project_collections: changes.side_state.project_collections,
            spaces: changes.side_state.spaces,
            custom_session_tags: changes.side_state.custom_session_tags,
            dirty,
            ui_generation_moved: self.gx_store.sidebar_list.last_ui_generation != ui_generation,
            settings_moved,
        };
        self.gx_store.sidebar_list.last_ui_generation = ui_generation;
        // CDXC:Sidebar 2026-09-20 WHY:
        // The wall clock alone cannot tell a slow update from a stalled thread, and the first two
        // slow ones this recorded rebuilt one row and one group of a hundred and thirteen and still
        // took ten milliseconds. Nothing inside `update` takes a lock or touches the file system
        // (gx-core has neither the dependency nor the permission), so the question is whether the
        // thread was running at all: CPU time near the wall time is work this code did, and CPU
        // time far below it is the thread descheduled or in the kernel. Two syscalls, and only
        // while the scenario that records them is on.
        let measuring = self.gx_store.sidebar_list.measuring();
        let cpu_started = measuring.then(thread_cpu_time_us).flatten();
        let started = Instant::now();
        let changed =
            self.gx_store
                .sidebar_list
                .model
                .update(&self.gx_store.core, &inputs, &changes, now_ms);
        let update_us = started.elapsed().as_micros() as u64;
        let cpu_us =
            cpu_started.and_then(|start| thread_cpu_time_us().map(|end| end.saturating_sub(start)));
        // An update this slow is a dropped frame on the thread that draws, and the only thing that
        // tells one apart from the next is which caches it had to drop. The view model reports
        // that; the host has the change summary that caused it.
        if update_us >= SLOW_UPDATE_US {
            let work = self.gx_store.sidebar_list.model.last_work();
            self.gx_store
                .diagnostics
                .sidebar_slow_update(update_us, cpu_us, &last_update, &work);
        }
        {
            let list = &mut self.gx_store.sidebar_list;
            list.last_update = last_update;
            list.counters.updates += 1;
            list.counters.last_update_us = update_us;
            list.counters.update_max_us = list.counters.update_max_us.max(update_us);
            if changed {
                list.counters.view_changes += 1;
            }
            list.last_inputs = inputs;
            list.last_built_at_ms = now_ms;
            list.built = true;
        }
        // A row the list stopped drawing leaves the multi-selection, which is itself one of the
        // list's inputs, so the build runs once more when it did.
        let mut changed = changed;
        if self.gx_store_prune_sidebar_selection() {
            let mut inputs = std::mem::take(&mut self.gx_store.sidebar_list.last_inputs);
            let settings = inputs.settings.clone();
            let ui_generation = self.gx_store.sidebar_ui.generation();
            let store = &mut self.gx_store;
            refresh_inputs(
                &mut inputs,
                &mut store.sidebar_list.inputs_cache,
                store.sidebar_ui.state(),
                ui_generation,
                settings,
                &store.runtime_facts,
                &store.sidebar_ui.stored_project_collections,
                unavailable,
                store.remote.tabs(),
            );
            changed |= self.gx_store.sidebar_list.model.update(
                &self.gx_store.core,
                &inputs,
                &ghostex_gx_core::ChangeSummary::default(),
                now_ms,
            );
            self.gx_store.sidebar_list.last_inputs = inputs;
            self.gx_store.sidebar_list.dirty = false;
            cx.notify();
        }
        self.gx_store_book_sidebar_deadline(cx);
        // Everything the session changed while this app was not the writer is still owed, so a
        // state restored mid-run writes it rather than waiting for the next click.
        self.gx_store_write_owed_sidebar_ui_state(cx);
        // The focused row's Space memory and the section follow, which ran on every publish until
        // step 6. Here instead, and after the rebuild for the same reason: whether the focused row
        // is DRAWN is the question that decides whether it builds anything, and the answer must be
        // this update's list. Nothing happens unless the focused row really changed
        // (`take_followed_session`), so every other path through here pays one comparison.
        self.gx_store_follow_active_session_space(cx);
        self.navigation_history_sidebar_changed(cx);
        // Every so often the same inputs are also built from scratch and the two lists compared:
        // this port's own cache invalidation, which has no second list to lean on since the page
        // was deleted (gx_store/sidebar_self_check.rs).
        self.gx_store_sidebar_scratch_check();
        // The list itself moved, or a value it carries from outside the view model did (the HUD
        // gx_store/hud/ composes, which the runtime posted until 2026-09-25, the rename or reveal request, the two hotkey labels). Before step 3
        // the second half was a publish's job; now every one of those arrives on a path that ends
        // here, so this is the one gate.
        if !self.gx_store_sidebar_list_ready() {
            self.gx_store_install_loading_sidebar_list(cx);
            return;
        }
        // A reveal the launch window held, now that the state it has to read is known. It re-enters
        // here once through `gx_store_sidebar_state_changed` when it changes something, and that
        // pass finds the request already taken and does nothing.
        self.gx_store_replay_held_sidebar_reveal(cx);
        let carry_moved = self
            .gx_store
            .sidebar_list
            .installed_carry
            .map(|(key, _)| key)
            != Some(self.gx_store_sidebar_carry_key());
        if !changed && !carry_moved {
            return;
        }
        if !changed {
            self.gx_store.sidebar_list.counters.installs_from_carry += 1;
        }
        self.gx_store_install_sidebar_list(cx);
    }

    /// Draws the loading skeleton while the list is not ready yet.
    ///
    /// CDXC:Sidebar 2026-09-21 WHY:
    /// Before step 6 this window drew the old projection's list. With that gone the choice is
    /// between the real list built on a default sidebar state (every project open, nothing hidden,
    /// snapping to the user's own state a moment later) and saying "still loading", and the
    /// renderer already draws a skeleton for an empty order with `loading` set
    /// (`native_sidebar/empty.rs`). It is installed once rather than per update, and it is replaced
    /// by the real list the moment both inputs land. A client-storage read that never succeeds
    /// leaves it standing, which is what `gxStore.sidebarUi` `readFailures` and the
    /// `sidebarUiReadUnavailable` record are for.
    fn gx_store_install_loading_sidebar_list(&mut self, cx: &mut gpui::Context<Self>) {
        if self.gx_store.sidebar_list.loading_installed {
            return;
        }
        self.gx_store.sidebar_list.loading_installed = true;
        let view = self.gx_store.sidebar_list.view();
        let snapshot = crate::app::native_sidebar::model::NativeSidebarSnapshot {
            scroll_scope: view.scroll_scope.clone(),
            rename_request: None,
            reveal_request: None,
            empty_state: serde_json::json!({
                "loading": true,
                "error": false,
                "canAddProject": false,
                "copy": Value::Null,
            }),
            hud: std::sync::Arc::new(Value::Null),
            groups: Vec::new(),
            selected_machine_id: view.selected_machine_id.clone(),
            machines: Vec::new(),
            spaces: Vec::new(),
            spaces_enabled: false,
            collections: Vec::new(),
            order: Vec::new(),
            more_menu: Value::Null,
            search_shortcut: None,
            commands_shortcut: None,
        };
        self.gx_store.sidebar_list.counters.loading_installs += 1;
        self.install_native_sidebar_snapshot(std::sync::Arc::new(snapshot), cx);
    }

    /// Drops ticked tag filters the Sort & Filter menu no longer offers, the way the old
    /// projection pruned them before every build.
    fn gx_store_prune_sidebar_tag_filters(&mut self, settings: &SidebarSettings) {
        if self
            .gx_store
            .sidebar_ui
            .state()
            .selected_tag_filters
            .is_empty()
        {
            return;
        }
        let machine = ghostex_gx_core::MachineId::Local;
        let offered = settings.offered_tag_filters(
            self.gx_store
                .core
                .presentation()
                .machine(&machine)
                .and_then(|machine| machine.side_state().custom_session_tags.as_ref()),
        );
        self.gx_store.sidebar_ui.retain_tag_filters(&offered);
    }

    /// Drops rows the list no longer draws from the multi-selection, the way the old projection
    /// prunes its own selection before it builds.
    fn gx_store_prune_sidebar_selection(&mut self) -> bool {
        if self
            .gx_store
            .sidebar_ui
            .state()
            .selected_session_ids
            .is_empty()
        {
            return false;
        }
        let drawn: std::collections::HashSet<&str> = self
            .gx_store
            .sidebar_list
            .model
            .view()
            .groups
            .iter()
            .flat_map(|group| group.core.sessions.iter())
            .map(|session| session.row.sidebar_session_id.as_str())
            .collect();
        self.gx_store
            .sidebar_ui
            .retain_selected_sessions(|session_id| drawn.contains(session_id))
    }

    /// Replaces the list the renderer draws with the one derived from the store. Needs no publish:
    /// the caller has already asked `gx_store_sidebar_list_ready`, which is the whole precondition.
    pub(crate) fn gx_store_install_sidebar_list(&mut self, cx: &mut gpui::Context<Self>) {
        // An empty object once the HUD leg was given up on: every reader of it indexes and
        // defaults (gx_store/sidebar_ready.rs).
        let Some(hud) = self.gx_store_sidebar_hud() else {
            return;
        };
        self.gx_store.sidebar_list.loading_installed = false;
        // The menu host reads two client-storage values behind a one-second cache, so it is taken
        // before the list is borrowed rather than inside the build.
        //
        // Measured, because it was the last main-thread work on this path that neither `installUs`
        // nor `updateUs` covered: at most once a second it opens client storage and reads two
        // indexed rows, and a slow one would have been invisible in both numbers.
        let host_started = Instant::now();
        let host = self.gx_store_menu_host();
        let host_us = host_started.elapsed().as_micros() as u64;
        let carry = (
            self.gx_store_sidebar_carry_key(),
            self.gx_store.menu_host.generation(),
        );
        let rename_request = self.gx_store_pending_collection_rename();
        let reveal_request = self.gx_store.runtime_facts.newest_reveal.clone();
        let started = Instant::now();
        // The labels are formatted against the clock of the moment they are drawn, not the moment
        // the list was last built: a wake that only ticks a countdown does not rebuild the list.
        let now_ms = now_ms();
        let snapshot = {
            // Borrowed rather than cloned: the model, the inputs and the cache are separate
            // fields, so the whole list does not have to be copied to build the one the renderer
            // draws.
            let core = &self.gx_store.core;
            let list = &mut self.gx_store.sidebar_list;
            let (search_shortcut, commands_shortcut) = list.shortcuts();
            let menus =
                SidebarMenus::new(core, list.model.view(), &list.last_inputs, &host, now_ms);
            snapshot_from_view(
                list.model.view(),
                &SnapshotInput {
                    menus: &menus,
                    hud: &hud,
                    rename_request,
                    reveal_request,
                    search_shortcut,
                    commands_shortcut,
                    settings: &list.last_inputs.settings,
                    hidden_items: &list.last_inputs.ui.hidden_items,
                    host: &host,
                    collapsed_groups: &list.last_inputs.ui.collapse.collapsed_groups,
                    show_hidden: list.last_inputs.ui.show_hidden,
                    selected_tag_filters: &list.last_inputs.ui.selected_tag_filters,
                    now_ms,
                },
                &mut list.snapshot_cache,
            )
        };
        let install_us = started.elapsed().as_micros() as u64;
        let list = &mut self.gx_store.sidebar_list;
        list.snapshot_cache.phases.host_us = host_us;
        list.installed_carry = Some(carry);
        list.counters.installs += 1;
        list.counters.last_install_us = install_us;
        list.counters.install_max_us = list.counters.install_max_us.max(install_us);
        self.install_native_sidebar_snapshot(std::sync::Arc::new(snapshot), cx);
    }

    /// Replaces the rows whose drawn time reads differently in the installed list, in place, and
    /// returns how many moved. Nothing else of the list is rebuilt or resynced.
    ///
    /// CDXC:Sidebar 2026-09-20 WHY:
    /// A wake moves one or two label strings and never the order, the sections, a menu or a group,
    /// so none of what `install_native_sidebar_snapshot` does for a real install (the scroll
    /// scope, the reveal, the disclosure animations, the open menu, the project view scope
    /// options) applies to it. The measured cost of servicing a wake through the full build was
    /// two milliseconds, ninety-three times in four minutes, on a sidebar nobody was touching.
    fn gx_store_relabel_sidebar_rows(&mut self, cx: &mut gpui::Context<Self>) -> usize {
        let started = Instant::now();
        let now = now_ms();
        let moved = {
            let list = &mut self.gx_store.sidebar_list;
            list.snapshot_cache.relabelled_rows(list.model.view(), now)
        };
        if moved.is_empty() {
            return 0;
        }
        let Some(installed) = self.native_sidebar.snapshot.as_mut() else {
            return moved.len();
        };
        // Cheap now that a group's menu and its header buttons are behind their own `Arc`s: this
        // copies the group records and the section lists, not a few hundred kilobytes of menu.
        let installed = std::sync::Arc::make_mut(installed);
        for (session_id, element) in &moved {
            for group in &mut installed.groups {
                if let Some(row) = group
                    .sessions
                    .iter_mut()
                    .find(|row| row.session_id == *session_id)
                {
                    *row = element.clone();
                    break;
                }
            }
        }
        cx.notify();
        let relabel_us = started.elapsed().as_micros() as u64;
        let list = &mut self.gx_store.sidebar_list;
        list.counters.last_relabel_us = relabel_us;
        list.counters.relabel_max_us = list.counters.relabel_max_us.max(relabel_us);
        moved.len()
    }

    /// Books one timer for the next moment a row moves on its own (a new session stops leading the
    /// list, a snooze ends, a countdown ticks). Nothing else wakes the list on time.
    fn gx_store_book_sidebar_deadline(&mut self, cx: &mut gpui::Context<Self>) {
        let now = now_ms();
        // Only the drawn list's labels are formatted here; the old projection refreshes its own
        // from the clock rows it publishes. The view model's own deadline is booked either way,
        // because the order and the sections it moves are the list's, not the labels'.
        let show_relative_time = self.gx_store.sidebar_list.show_relative_time();
        let label_deadline = self
            .gx_store_sidebar_list_ready()
            .then(|| {
                self.gx_store
                    .sidebar_list
                    .next_label_deadline(now, show_relative_time)
            })
            .flatten();
        let row_deadline = self.gx_store.sidebar_list.next_deadline_ms();
        let label_kind = match label_deadline {
            Some(ghostex_gx_core::LabelDeadline::Countdown(_)) => "countdown",
            Some(ghostex_gx_core::LabelDeadline::Relative(_)) => "relative",
            None => "none",
        };
        let (deadline, kind) = match (row_deadline, label_deadline.map(|at| at.at_ms())) {
            (Some(row), Some(label)) if row <= label => (row, "row"),
            (_, Some(label)) => (label, label_kind),
            (Some(row), None) => (row, "row"),
            (None, None) => {
                self.gx_store.sidebar_list.deadline_booked = None;
                self.gx_store.sidebar_list.deadline_kind = "none";
                return;
            }
        };
        self.gx_store.sidebar_list.deadline_kind = kind;
        let booked = self.gx_store.sidebar_list.deadline_booked;
        // A timer for an earlier or equal deadline already covers this one.
        if booked.is_some_and(|booked| booked <= deadline) {
            return;
        }
        self.gx_store.sidebar_list.deadline_booked = Some(deadline);
        let wait = Duration::from_millis(deadline.saturating_sub(now_ms())).max(MIN_DEADLINE_WAIT);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let _ = this.update(cx, |this, cx| {
                if this.gx_store.sidebar_list.deadline_booked != Some(deadline) {
                    return;
                }
                this.gx_store.sidebar_list.deadline_booked = None;
                this.gx_store.sidebar_list.counters.deadline_wakes += 1;
                this.gx_store_update_sidebar_list(cx);
                // A label that reads differently is not a change in the list itself, so the update
                // above may have found nothing. Only the rows whose time actually moved need the
                // list rebuilt, and on a long list the earliest deadline usually belongs to one
                // row while every other row reads exactly as it did.
                if this.gx_store_sidebar_list_ready() {
                    // How many rows a wake actually moved is what tells a rate of one a second
                    // apart: one row ticking is a countdown or a fresh row doing its job, and a
                    // wake that moved none is a booking that should not have been made.
                    let moved = this.gx_store_relabel_sidebar_rows(cx);
                    let list = &mut this.gx_store.sidebar_list;
                    list.counters.wake_rows_moved += moved as u64;
                    list.counters.wake_rows_moved_max =
                        list.counters.wake_rows_moved_max.max(moved as u64);
                    if moved == 0 {
                        this.gx_store.sidebar_list.counters.installs_skipped += 1;
                    }
                    this.gx_store_book_sidebar_deadline(cx);
                }
            });
        })
        .detach();
    }
}

impl SidebarList {
    fn next_deadline_ms(&self) -> Option<u64> {
        self.model.next_deadline_ms()
    }

    /// Where the newest install's time went.
    pub(super) fn install_phases(&self) -> super::sidebar_snapshot::InstallPhases {
        self.snapshot_cache.phases
    }

    /// The menu-host generation the installed list was built with, if one is installed.
    pub(super) fn installed_menu_host_generation(&self) -> Option<u64> {
        self.installed_carry.map(|(_, generation)| generation)
    }
}

/// The CPU time this thread has used so far, in microseconds.
///
/// `None` where the platform has no per-thread clock (Windows), which reads in the record as "not
/// measured" rather than as zero. Measured at 256 ns a call on this computer, and read twice per
/// update only while the scenario is on, against a median update of tens of microseconds.
fn thread_cpu_time_us() -> Option<u64> {
    #[cfg(unix)]
    {
        let mut spec = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: `clock_gettime` writes only the `timespec` it is given and returns non-zero on
        // failure, which is the case a platform without this clock takes.
        let read = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut spec) };
        (read == 0)
            .then(|| (spec.tv_sec.max(0) as u64) * 1_000_000 + (spec.tv_nsec.max(0) as u64) / 1_000)
    }
    #[cfg(not(unix))]
    {
        None
    }
}
