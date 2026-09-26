//! Local focus: a tab selection, a next or previous tab step, and a sidebar row click change the
//! store at once. This file holds what the rest of the app calls: the selection entry, the row
//! highlight and the empty tab list guard. `burst.rs` owns the timers that finish a selection and
//! release deferred work; `focus_publish.rs` hands the store's focus to the workspace.

use std::time::Instant;

use ghostex_gx_core::{
    Event, IgnoredReason, Intent, ProjectKey, SessionKey, default_group_for_project,
};

use super::host::{GxStoreHost, now_ms};
use crate::GhostexGpuiApp;
use crate::app::model::{
    GpuiGxserverPresentationFocusState, GpuiLocalWorkspaceSessionKey, ShellFocusTarget,
    TitlebarMode,
};

/// Sidebar row ids of local sessions start with this (`SessionKey::to_sidebar_session_id`).
const LOCAL_SESSION_ROW_PREFIX: &str = "combined-session:";
/// And those of remote sessions with this (`remote:<machine>:session:<project>:<session>`); no
/// other row the sidebar draws does.
const REMOTE_SESSION_ROW_PREFIX: &str = "remote:";

/// A local selection whose follow-up (the workspace publish, the attention acknowledge, the
/// remembered session) waits for the burst to end.
#[derive(Clone, Debug)]
pub(super) struct PendingFinish {
    pub(super) key: GpuiLocalWorkspaceSessionKey,
    pub(super) local_was_sleeping: bool,
    pub(super) local_runtime_missing: bool,
}

/// Whose marks the sidebar's session rows draw.
///
/// CDXC:FocusRouting 2026-09-25 WHY:
/// The store owns every focus now, so its own focus is the drawn one except for one case: its newest selection named a row it could not place (the clicked remote row is not in its rows yet, or a session created a moment ago has not arrived). Nothing of the store's draws focused then, until the row arrives and the held selection is placed (focus_publish.rs). The third state this enum had, a focus the old runtime accepted and the store could not place, went with the runtime's focus (supersedes the 2026-09-21 note).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum DrawnFocus {
    /// The store's focus is the drawn one: rows compare against its cache.
    #[default]
    Store,
    /// The store's own newest selection named a row it could not place.
    Unplaced,
}

impl DrawnFocus {
    /// No session row of the store draws focused, and its visible set is not the drawn one.
    pub(super) fn store_rows_unfocused(self) -> bool {
        !matches!(self, Self::Store)
    }
}

/// What happened since the app started. Memory only; the `native.terminal.focus` lines quote it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct LocalFocusCounters {
    pub(super) local_selections: u64,
    pub(super) unplaced_selections: u64,
    /// Selections whose follow-up ran at the end of their burst.
    pub(super) finishes: u64,
    pub(super) attention_acknowledges: u64,
    pub(super) settles: u64,
    pub(super) disputed_empty_tab_lists: u64,
}

/// CDXC:FocusRouting 2026-09-19 DECISION:
/// User: holding "next tab" must fly through tabs, and sidebar clicks and tab selections must be instant; the old runtime is told about focus, never asked, and never allowed to override a newer local choice.
/// A selection is a store intent plus one repaint. Since 2026-09-25 there is no old runtime to tell: the store owns the focus alone, and the work a selection sets off (the workspace publish, the attention acknowledge, the remembered session) runs once when the burst ends (burst.rs), so a held key costs one intent and one repaint per step. This supersedes the stamp tell, its retells and the echo markers that kept a runtime behind the store from pulling focus back.
/// SEE-ALSO: burst.rs (finish and settle timers), focus_publish.rs (the workspace publish), packages/gx-core/src/focus.rs.
#[derive(Default)]
pub(crate) struct LocalFocus {
    /// The persisted focus was seeded into the core (once, before the first frame).
    pub(super) restored: bool,
    /// Whose marks session rows draw: the store's own focus, or nobody's while its newest
    /// selection could not be placed. Every selection the store places sets it back to `Store`.
    pub(super) drawn_focus: DrawnFocus,
    /// Row id of the store's focused session, and of its visible sessions, so a row compares two
    /// strings per frame instead of decoding its id.
    focused_row_id: Option<String>,
    visible_row_ids: Vec<String>,
    /// `drawn_focus` as of the last cache refresh, so a flip alone also counts as a change.
    cached_drawn_focus: DrawnFocus,
    /// The newest sidebar snapshot marks a browser row of the active group focused; local rows
    /// then draw no focus.
    snapshot_browser_focus: bool,
    pub(super) pending_finish: Option<PendingFinish>,
    pub(super) pending_attention: Vec<GpuiLocalWorkspaceSessionKey>,
    /// Newest remembered session per project, written when the selection finishes.
    pub(super) pending_remembered: Vec<SessionKey>,
    /// The store's focus stamp at the last finish: a selection that repeats it has nothing to do.
    pub(super) finished_stamp: u64,
    pub(super) last_selection_at: Option<Instant>,
    /// Set while heavy per-selection work is held back; `burst.rs` clears it.
    pub(super) settle_due: Option<Instant>,
    pub(super) finish_due: Option<Instant>,
    /// The row a held previous or next session key landed on has no live terminal: it is focused
    /// once the selection settles, and only if it is still the focused row.
    pub(super) walk_landing_focus: Option<String>,
    /// The row to reveal (animated, or flashed when already in view) once the selection settles.
    pub(super) walk_landing_reveal: Option<String>,
    /// The selection being booked comes from a key that is being held (a key repeat), so it is
    /// part of a burst whatever the time since the previous step.
    pub(super) key_held: bool,
    pub(super) burst_task_running: bool,
    /// The instant the burst task sleeps towards, and the channel that wakes it earlier; see
    /// `gx_store_burst_deadline_booked`.
    pub(super) burst_sleeping_until: Option<Instant>,
    pub(super) burst_wake: Option<futures::channel::mpsc::UnboundedSender<()>>,
    pub(super) burst_steps: u32,
    pub(super) chat_reconcile_wanted: bool,
    pub(super) browser_surface_wanted: bool,
    /// What the focus state file holds, so it is written only when one of its fields moved.
    persisted_focus: Option<(Option<String>, Option<String>, Vec<String>)>,
    /// An empty tab list the store does not confirm; judged again when the store's tab lists
    /// change.
    pub(super) disputed_empty_tab_list: Option<String>,
    pub(super) counters: LocalFocusCounters,
}

impl LocalFocus {
    /// Keeps the newest remembered session per project.
    pub(super) fn remember(&mut self, session: SessionKey) {
        let project = session.project_key();
        self.pending_remembered
            .retain(|remembered| remembered.project_key() != project);
        self.pending_remembered.push(session);
    }
}

pub(super) struct LocalSelectionOutcome {
    pub(super) moved: bool,
    /// The store does not hold the session yet (created a moment ago): the caller holds it for
    /// the workspace until its row arrives (focus_publish.rs).
    pub(super) unplaced: bool,
}

impl GxStoreHost {
    /// Applies a local selection to the core. Memory only.
    pub(super) fn apply_local_selection(
        &mut self,
        session: SessionKey,
        visible: Vec<SessionKey>,
    ) -> LocalSelectionOutcome {
        let moved = self.core.focus().focused_session.as_ref() != Some(&session);
        let output = self.core.handle(
            Event::Intent(Intent::FocusSession {
                session,
                visible: Some(visible),
            }),
            now_ms(),
        );
        self.local_focus.counters.local_selections += 1;
        // A session the daemon created a moment ago is not in the store yet, and the stamp did
        // not move: the selection is held until its row arrives.
        let unplaced = output.changes.ignored == Some(IgnoredReason::UnknownTarget);
        if unplaced {
            self.local_focus.counters.unplaced_selections += 1;
            self.local_focus.drawn_focus = DrawnFocus::Unplaced;
        } else {
            self.local_focus.drawn_focus = DrawnFocus::Store;
            self.focus_publish.unplaced = None;
        }
        self.run_effects(output.effects);
        self.refresh_row_focus_cache();
        LocalSelectionOutcome { moved, unplaced }
    }

    /// Whether the store already holds exactly this selection and its follow-up has run.
    fn selection_is_current(&self, session: &SessionKey, visible: &[SessionKey]) -> bool {
        let focus = self.core.focus();
        self.local_focus.pending_finish.is_none()
            && !self.local_focus.drawn_focus.store_rows_unfocused()
            && focus.focused_session.as_ref() == Some(session)
            && focus.visible_sessions == visible
            && self.local_focus.finished_stamp == focus.local_stamp
    }

    /// Rebuilds the row ids the sidebar compares against. Returns `true` when they changed. A
    /// remote session's row id is its machine-scoped id, so one cache serves every machine's rows.
    pub(super) fn refresh_row_focus_cache(&mut self) -> bool {
        let focus = self.core.focus();
        let focused = focus
            .focused_session
            .as_ref()
            .map(SessionKey::to_sidebar_session_id);
        let visible = focus
            .visible_sessions
            .iter()
            .map(SessionKey::to_sidebar_session_id)
            .collect::<Vec<_>>();
        let changed = self.local_focus.focused_row_id != focused
            || self.local_focus.visible_row_ids != visible
            || self.local_focus.cached_drawn_focus != self.local_focus.drawn_focus;
        self.local_focus.cached_drawn_focus = self.local_focus.drawn_focus;
        self.local_focus.focused_row_id = focused;
        self.local_focus.visible_row_ids = visible;
        changed
    }

    /// Seeds the core's focus from the persisted focus state, once, before any frame is applied.
    /// The file names a local session by its raw id, so the project comes from the file's active
    /// project; a remote session is named by its machine-scoped id, which carries its project.
    /// A session that does not belong to the active project the file names is not seeded: the
    /// file is then not one focus, and the store's reconcile after the first snapshot decides.
    pub(super) fn restore_focus_once(&mut self, persisted: &GpuiGxserverPresentationFocusState) {
        if std::mem::replace(&mut self.local_focus.restored, true) {
            return;
        }
        let Some(project) = persisted
            .active_project_id
            .as_deref()
            .and_then(ProjectKey::parse_workspace_project_id)
        else {
            return;
        };
        let session = match persisted.focused_session_id.as_deref() {
            None => None,
            Some(session_id) => match SessionKey::parse_remote_scoped_session_id(session_id) {
                Some(remote) if remote.project_key() == project => Some(remote),
                Some(_) => return,
                None if project.machine.is_local() => {
                    Some(SessionKey::local(project.project_id.as_str(), session_id))
                }
                None => return,
            },
        };
        let mut focus = self.core.focus().clone();
        focus.active_group = Some(default_group_for_project(
            self.core.presentation(),
            &project,
        ));
        focus.active_project = Some(project);
        focus.visible_sessions = session.iter().cloned().collect();
        if let Some(session) = &session {
            focus
                .last_session_by_project
                .retain(|remembered| remembered.project_key() != session.project_key());
            focus.last_session_by_project.push(session.clone());
        }
        focus.focused_session = session;
        self.core.restore_focus(focus);
        self.refresh_row_focus_cache();
        self.diagnostics.focus_restored(&self.core);
    }

    /// Whether an empty tab list may clear a project's workspace: only when the store is loaded
    /// and lists no tab for that project either. This picks the group to judge by; the rule itself
    /// is `empty_tab_list_confirmed` in gx-core, which for a user-made group also asks the
    /// project's own list (packages/gx-core/examples/empty_tab_list_guard.rs).
    ///
    /// CDXC:Workarea 2026-09-19 WHY:
    /// An empty list makes `reconcile_with_sidebar_tab_sessions` drop every restored tab, split and mapping (the "different session after restart" bug of 2026-09-04). The publish sends no list before its machine's first snapshot (`NotLoaded` and `Missing` are never read as "no tabs"), and an empty list is still judged here against the project's own group, because the last member of a user-made group being closed leaves an empty group of a project that has other tabs. Kept when the store became the list's only source (2026-09-25): the guard is what the whole workspace's tabs depend on, so it stays one gate on the one path in.
    pub(super) fn confirms_empty_tab_list(&self, project_id: &str) -> bool {
        let Some(project) = ProjectKey::parse_workspace_project_id(project_id) else {
            return false;
        };
        let store = self.core.presentation();
        if store.loaded(&project.machine).is_none() {
            // A machine the store does not hold cannot dispute anything: its list keeps the old
            // rule. Before M4d that was every remote machine; now it is only one whose client is
            // not running.
            return true;
        }
        // The store's active group is the one to judge by whenever it belongs to the project the
        // list names; any other group falls back to the project's own default group.
        let group = self
            .core
            .focus()
            .active_group
            .clone()
            .filter(|group| match group {
                ghostex_gx_core::ActiveGroup::Project(owner) => *owner == project,
                ghostex_gx_core::ActiveGroup::Subgroup { project: owner, .. } => *owner == project,
                ghostex_gx_core::ActiveGroup::Chats(machine) => {
                    *machine == project.machine && store.is_chat_project(&project)
                }
            })
            .unwrap_or_else(|| default_group_for_project(store, &project));
        ghostex_gx_core::empty_tab_list_confirmed(store, &group)
    }
}

impl GhostexGpuiApp {
    /// A local session was selected in the workspace (tab click, next or previous tab, sidebar
    /// row click, attach completion). The store changes now; the workspace publish and the rest of
    /// the follow-up run once the burst is over. Callers repaint themselves, as they did before.
    pub(crate) fn gx_store_select_local_session(
        &mut self,
        key: &GpuiLocalWorkspaceSessionKey,
        local_was_sleeping: bool,
        local_runtime_missing: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let session = SessionKey::local(key.project_id.as_str(), key.session_id.as_str());
        let visible = self.gx_store_visible_local_session_keys(key);
        let focus_state = &mut self.sidebar_gxserver_presentation_focus_state;
        if focus_state.active_project_id.as_deref() == Some(key.project_id.as_str()) {
            // Thirty-odd readers resolve "the focused session" from the workspace's focus state
            // (the companion pane, attach completions, extensions). It follows the store here, in
            // the selection's frame; the publish at the end of the burst brings the rest.
            let visible_ids = visible
                .iter()
                .map(|session| session.session_id.clone())
                .collect::<Vec<_>>();
            focus_state.focused_session_id = Some(key.session_id.clone());
            focus_state.visible_session_ids = visible_ids;
        }
        if self.gx_store.selection_is_current(&session, &visible)
            && !local_was_sleeping
            && !local_runtime_missing
        {
            // An attach completion or a focus request repeating a selection the store holds and
            // has finished: nothing to change.
            return;
        }
        let outcome = self
            .gx_store
            .apply_local_selection(session.clone(), visible.clone());
        if outcome.unplaced {
            self.gx_store_hold_unplaced_selection(session, visible);
        }
        let local_focus = &mut self.gx_store.local_focus;
        // Flags of an earlier selection of the same session still hold: the tab has not been
        // attached or woken in between, or the later caller would not be selecting it again.
        let (was_sleeping, runtime_missing) = match &local_focus.pending_finish {
            Some(pending) if pending.key == *key => (
                pending.local_was_sleeping || local_was_sleeping,
                pending.local_runtime_missing || local_runtime_missing,
            ),
            _ => (local_was_sleeping, local_runtime_missing),
        };
        local_focus.pending_finish = Some(PendingFinish {
            key: key.clone(),
            local_was_sleeping: was_sleeping,
            local_runtime_missing: runtime_missing,
        });
        self.gx_store_note_local_selection(outcome.moved, cx);
        self.gx_store_sidebar_focus_moved(cx);
        if local_runtime_missing && !self.gx_store_selection_is_settling() {
            self.gx_store_attach_surfaced_terminals(cx);
        }
    }

    /// Marks the selections made until it is cleared as steps of a held key. Set around the
    /// dispatch of a hotkey's key repeat.
    pub(crate) fn gx_store_set_key_held(&mut self, held: bool) {
        self.gx_store.local_focus.key_held = held;
    }

    pub(crate) fn gx_store_key_is_held(&self) -> bool {
        self.gx_store.local_focus.key_held
    }

    /// The sidebar row that counts as current for the previous and next session walk: the row of
    /// the store's focused session, the same source the highlight reads. While a browser tab or a
    /// row the store does not hold owns focus, it is the row the snapshot marks focused, which is
    /// then also the highlighted one.
    pub(super) fn gx_store_focused_sidebar_row_id(
        &self,
        snapshot: &crate::app::native_sidebar::model::NativeSidebarSnapshot,
    ) -> Option<String> {
        let local_focus = &self.gx_store.local_focus;
        let snapshot_focused_row = |browser: bool| {
            snapshot
                .groups
                .iter()
                .flat_map(|group| group.sessions.iter())
                .find(|session| session.is_focused && session.is_browser() == browser)
                .map(|session| session.session_id.clone())
        };
        let browser_holds_shell_focus = self.active_mode == TitlebarMode::Browser
            && matches!(
                self.shell_focus,
                ShellFocusTarget::BrowserSurface | ShellFocusTarget::BrowserPane(_)
            );
        if local_focus.snapshot_browser_focus && browser_holds_shell_focus {
            return snapshot_focused_row(true);
        }
        if local_focus.drawn_focus.store_rows_unfocused() {
            // The store's cache is not the drawn focus; the walk still needs the row it steps
            // from, and the snapshot's is the only one either side holds.
            return snapshot_focused_row(false);
        }
        local_focus.focused_row_id.clone()
    }

    /// The row a held key landed on has a staged tab and no terminal: it is focused (woken or
    /// attached) once the key is released, and only if it is still the focused row.
    pub(super) fn gx_store_focus_landing_row_at_settle(&mut self, row_id: &str) {
        self.gx_store.local_focus.walk_landing_focus = Some(row_id.to_string());
    }

    /// A walk step revealed a row; when the selection is still moving, the landing row is
    /// revealed again at the settle.
    pub(super) fn gx_store_note_walk_reveal(&mut self, row_id: &str) {
        self.gx_store.local_focus.walk_landing_reveal = self
            .gx_store_selection_is_settling()
            .then(|| row_id.to_string());
    }

    /// The selection settled: what the walk left for the row it landed on. Returns the row to
    /// focus, when it is still the focused one.
    pub(super) fn gx_store_take_walk_landing(&mut self) -> (Option<String>, Option<String>) {
        let local_focus = &mut self.gx_store.local_focus;
        let reveal = local_focus.walk_landing_reveal.take();
        let ask = local_focus
            .walk_landing_focus
            .take()
            .filter(|row_id| local_focus.focused_row_id.as_deref() == Some(row_id.as_str()));
        (reveal, ask)
    }

    /// A remote session was selected in the workspace: the store's own open of a remote row, a
    /// click on a remote tab, or a remote attach that lands. The core's focus takes it here, in
    /// the frame of the selection, so the row highlights at once, and the workspace is published
    /// from it. `false` for a pair that does not name one remote session of one remote project.
    ///
    /// CDXC:FocusRouting 2026-09-25 WHY:
    /// Remote focus part 2 step 2 (2026-09-21) made the store's core focus own the remote row's highlight; the runtime still moved its own marks from the tab selection and the store mirrored them. The runtime's focus is gone, so this is the whole selection: a row the machine does not list yet is held for the workspace until it arrives (focus_publish.rs), and nothing of the store's draws focused until then.
    pub(crate) fn gx_store_select_remote_session(
        &mut self,
        scoped_project_id: &str,
        scoped_session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(session) = SessionKey::parse_remote_scoped_session_id(scoped_session_id) else {
            return false;
        };
        if ProjectKey::parse_workspace_project_id(scoped_project_id) != Some(session.project_key())
        {
            return false;
        }
        let output = self.gx_store.core.handle(
            Event::Intent(Intent::FocusSession {
                session: session.clone(),
                // A remote focus shows the session alone (`setRemotePresentationSessionFocus`),
                // which is the core's rule for a remote session without a reported set.
                visible: None,
            }),
            now_ms(),
        );
        let refused = output.changes.ignored == Some(IgnoredReason::UnknownTarget);
        if refused {
            self.gx_store_hold_unplaced_selection(session.clone(), vec![session.clone()]);
        } else {
            self.gx_store.focus_publish.unplaced = None;
        }
        // Placed means the store HOLDS the row, not merely that the intent was not refused: the
        // core applies a focus on a machine whose first snapshot has not arrived without checking
        // it (startup restore), and such a row is one the store cannot draw either.
        let placed = !refused && self.gx_store.core.presentation().session(&session).is_some();
        self.gx_store.local_focus.drawn_focus = match placed {
            true => DrawnFocus::Store,
            false => DrawnFocus::Unplaced,
        };
        self.gx_store.run_effects(output.effects);
        self.gx_store
            .sidebar_remote_focus
            .note_core_selection(placed);
        if self.gx_store.refresh_row_focus_cache() {
            cx.notify();
        }
        self.gx_store_sidebar_focus_moved(cx);
        self.gx_store_persist_remembered_sessions(cx);
        self.gx_store_publish_workspace_focus(cx);
        true
    }

    /// The store keys of the sessions that own a rendered pane, the selected one included: the
    /// exact visible set of `Intent::FocusSession`.
    fn gx_store_visible_local_session_keys(
        &self,
        selected: &GpuiLocalWorkspaceSessionKey,
    ) -> Vec<SessionKey> {
        let shell_session_ids = self
            .agents_workspace
            .rendered_leaf_order()
            .into_iter()
            .filter_map(|pane_id| self.agents_workspace.active_session_in_pane(pane_id))
            .collect::<Vec<_>>();
        let mut keys = Vec::with_capacity(shell_session_ids.len() + 1);
        for shell_session_id in shell_session_ids {
            let Some(key) = self
                .local_workspace_session_mappings
                .iter()
                .find_map(|(key, mapped)| (*mapped == shell_session_id).then_some(key))
            else {
                continue;
            };
            let key = SessionKey::local(key.project_id.as_str(), key.session_id.as_str());
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        let selected =
            SessionKey::local(selected.project_id.as_str(), selected.session_id.as_str());
        if !keys.contains(&selected) {
            keys.push(selected);
        }
        keys
    }

    /// Writes the focus state file when the focus it holds moved. The one writer while the app
    /// runs: a payload of the old runtime and the end of a local burst both come through here, so
    /// a held key costs one write, not one per tab. The quit path writes unconditionally.
    pub(crate) fn gx_store_persist_focus_state_file(&mut self) {
        let focus_state = &self.sidebar_gxserver_presentation_focus_state;
        let current = (
            focus_state.active_project_id.clone(),
            focus_state.focused_session_id.clone(),
            focus_state.visible_session_ids.clone(),
        );
        if self.gx_store.local_focus.persisted_focus.as_ref() == Some(&current) {
            return;
        }
        crate::app::helpers::persist_gpui_gxserver_presentation_focus_state(focus_state);
        self.gx_store.local_focus.persisted_focus = Some(current);
    }

    /// A sidebar snapshot arrived: whether it marks a browser row of an active group focused.
    /// Computed once per snapshot by the receiver, never per row.
    pub(crate) fn gx_store_note_sidebar_snapshot_browser_focus(&mut self, browser_focus: bool) {
        self.gx_store.local_focus.snapshot_browser_focus = browser_focus;
    }

    /// Whether a native sidebar row draws focused and with the visible fill.
    ///
    /// A session row of any machine reads the store, unless a browser tab owns focus or the
    /// store's focus is not the drawn one (`DrawnFocus`). A browser row keeps the
    /// snapshot's flags while the shell's focus is on the browser (browser tabs are host state,
    /// not sessions). Any other row draws no focus of its own.
    pub(crate) fn gx_store_sidebar_row_focus(
        &self,
        row_id: &str,
        is_browser: bool,
        snapshot_focused: bool,
        snapshot_visible: bool,
    ) -> (bool, bool) {
        let local_focus = &self.gx_store.local_focus;
        // Whether a browser tab has focus is a fact of this shell (it is what the old runtime's
        // flag is derived from), so leaving the browser shows at once; the snapshot's flag lags.
        let browser_holds_shell_focus = self.active_mode == TitlebarMode::Browser
            && matches!(
                self.shell_focus,
                ShellFocusTarget::BrowserSurface | ShellFocusTarget::BrowserPane(_)
            );
        if is_browser {
            return (
                snapshot_focused && browser_holds_shell_focus,
                snapshot_visible,
            );
        }
        let local_row = row_id.starts_with(LOCAL_SESSION_ROW_PREFIX);
        let remote_row = row_id.starts_with(REMOTE_SESSION_ROW_PREFIX);
        let session_row = local_row || remote_row;
        if local_focus.snapshot_browser_focus && browser_holds_shell_focus {
            // One focused row: the browser tab's, which the snapshot already draws.
            let visible = if session_row && !local_focus.drawn_focus.store_rows_unfocused() {
                local_focus
                    .visible_row_ids
                    .iter()
                    .any(|visible| visible == row_id)
            } else {
                snapshot_visible
            };
            return (false, visible);
        }
        if !session_row {
            return (false, snapshot_visible);
        }
        if local_focus.drawn_focus.store_rows_unfocused() {
            // The store's newest selection is a row it could not place: no row draws focused, and
            // every row's visible fill is the snapshot's, until the held selection is placed.
            return (false, snapshot_visible);
        }
        let focused = local_focus.focused_row_id.as_deref() == Some(row_id);
        let visible = local_focus
            .visible_row_ids
            .iter()
            .any(|visible| visible == row_id);
        (focused, visible)
    }

    /// The session the store has focused, on any machine.
    pub(crate) fn gx_store_focused_session(&self) -> Option<SessionKey> {
        self.gx_store.core.focus().focused_session.clone()
    }

    /// Whether the tab list of a focus payload may be reconciled into the workspace now. A list
    /// with rows always may; an empty one needs the store's agreement and is judged again when the
    /// store's tab lists change.
    pub(crate) fn gx_store_allows_tab_reconcile(
        &mut self,
        project_id: Option<&str>,
        tab_count: usize,
    ) -> bool {
        let local_focus = &mut self.gx_store.local_focus;
        if tab_count > 0 {
            local_focus.disputed_empty_tab_list = None;
            return true;
        }
        let Some(project_id) = project_id else {
            return true;
        };
        if self.gx_store.confirms_empty_tab_list(project_id) {
            self.gx_store.local_focus.disputed_empty_tab_list = None;
            return true;
        }
        let local_focus = &mut self.gx_store.local_focus;
        if local_focus.disputed_empty_tab_list.as_deref() != Some(project_id) {
            local_focus.disputed_empty_tab_list = Some(project_id.to_string());
            local_focus.counters.disputed_empty_tab_lists += 1;
            self.gx_store.diagnostics.empty_tab_list_disputed(
                &self.gx_store.core,
                self.gx_store.local_focus.counters.disputed_empty_tab_lists,
            );
        }
        false
    }

    /// The store changed (frames applied): finish what was waiting for it. Returns `true` when
    /// something on screen changed.
    pub(super) fn gx_store_after_pump(
        &mut self,
        tab_lists_changed: bool,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        // A selection held for a row that had not arrived is placed once the row is there.
        self.gx_store_place_unplaced_selection(cx);
        // A focus the core applied on a machine it held no rows for (startup restore, a remote
        // machine still connecting) is drawn once the machine lists the row.
        if self.gx_store.local_focus.drawn_focus == DrawnFocus::Unplaced
            && self.gx_store.focus_publish.unplaced.is_none()
            && self
                .gx_store
                .core
                .focus()
                .focused_session
                .as_ref()
                .is_some_and(|session| self.gx_store.core.presentation().session(session).is_some())
        {
            self.gx_store.local_focus.drawn_focus = DrawnFocus::Store;
        }
        // A frame can move focus too: the focused session was closed, or its project went away.
        let mut repaint = self.gx_store.refresh_row_focus_cache();
        // The workspace follows every frame, as it followed every publish of the runtime; a
        // selection that is still moving publishes once it settles instead (burst.rs).
        if self.gx_store_selection_is_settling() {
            self.gx_store_book_selection_finish(cx);
        } else {
            self.gx_store_publish_workspace_focus(cx);
            self.gx_store_restore_startup_focus(cx);
        }
        let store = &mut self.gx_store;
        store.diagnostics.focus_summary(
            store.focus_publish.counters,
            store.focus_perform,
            &store.core,
        );
        if tab_lists_changed
            && let Some(project_id) = self.gx_store.local_focus.disputed_empty_tab_list.clone()
        {
            let focus_state = self.sidebar_gxserver_presentation_focus_state.clone();
            let still_empty = focus_state.active_project_id.as_deref() == Some(project_id.as_str())
                && focus_state
                    .active_project_tab_sessions
                    .as_ref()
                    .is_some_and(Vec::is_empty);
            if !still_empty {
                self.gx_store.local_focus.disputed_empty_tab_list = None;
            } else if self.gx_store.confirms_empty_tab_list(&project_id)
                && self.reconcile_local_workspace_tabs_with_sidebar(&focus_state, cx)
            {
                self.reconcile_agents_chat_surfaces(cx);
                self.persist_shell_layout_state();
                repaint = true;
            }
        }
        repaint
    }
}
