//! The workspace's focus state and the active project context, derived from the store.
//!
//! CDXC:FocusRouting 2026-09-25 WHY:
//! The app runtime used to own the workspace tab list and the active project: it published both
//! (`postGxserverPresentationFocusState`, `postActiveProjectContext`) and the store only mirrored
//! them, with a stamp tell, echo markers and a shadow comparison keeping the two apart. The store's
//! focus is the only owner now. Every change it makes is published here through the same two
//! receivers the runtime's posts reached (`set_sidebar_gxserver_presentation_focus_state`,
//! `receive_sidebar_project_context_payload`), so the project switch coalescer, the tab reconcile
//! and the empty tab list guard stay the one implementation they were. What the runtime's publish
//! did that the store cannot express is carried here too: a selection whose row has not arrived
//! yet (a session created or forked a moment ago), and the All Automations overview.
//!
//! **The empty tab list guard stays.** A tab list is sent only when the store has the group
//! `Loaded`; `NotLoaded` and `Missing` send none, and an empty `Loaded` list still has to pass
//! `gx_store_allows_tab_reconcile` (CDXC:Workarea 2026-09-19), exactly as the runtime's did. Before
//! this computer's first snapshot nothing is published at all unless a remote project owns the
//! selection, which was the runtime's rule (CDXC:Workarea 2026-09-04 in the old `sidebar-groups.ts`).
//!
//! SEE-ALSO: packages/gx-core/src/active_project_context.rs, packages/gx-core/src/focus.rs,
//! apps/desktop/src/app/gx_store/local_focus.rs, apps/desktop/src/app/gx_store/burst.rs,
//! apps/desktop/src/app/workspace_terminals.rs (`set_sidebar_gxserver_presentation_focus_state`).

use std::time::{Duration, Instant};

use ghostex_gx_core::protocol::LifecycleState;
use ghostex_gx_core::{
    ActiveGroup, Event, IgnoredReason, Intent, MachineId, SessionKey, TabSession,
    active_project_context_payload, default_group_for_project,
};
use serde_json::Value;

use super::host::now_ms;
use crate::GhostexGpuiApp;
use crate::app::helpers::{
    GPUI_QUICK_AUTOMATIONS_PROJECT_ID, GpuiRemoteAttachSessionKey, GpuiWorkspaceTerminalSessionKey,
    gpui_sidebar_agent_icon,
};
use crate::app::model::{
    AgentTerminalActivity, AgentsWorkspaceSessionKind, GpuiGxserverPresentationFocusState,
    GpuiLocalWorkspaceSessionKey, GpuiPreferredAgentInterface, GpuiSidebarWorkspaceTabSession,
    GpuiSidebarWorkspaceTerminalFocusMessage, GpuiSwitchableSessionAgent,
    GpuiWorkspaceTerminalFocusPlacement, TerminalSessionPresentationState,
};

/// The id the All Automations overview's focus names (`GPUI_QUICK_AUTOMATIONS_SIDEBAR_SESSION_ID`
/// in the old runtime's constants).
const QUICK_AUTOMATIONS_SESSION_ID: &str = "__quick-automations__";

/// How long a selection the store could not place is held for the workspace. A created or forked
/// session's row arrives well inside a second; a remote machine that never lists the row lets the
/// hold lapse instead of pinning the workspace to it for good.
const UNPLACED_HOLD: Duration = Duration::from_secs(30);

/// A selection the store refused because its row is not in the store yet.
#[derive(Clone, Debug)]
pub(super) struct UnplacedSelection {
    pub(super) session: SessionKey,
    pub(super) visible: Vec<SessionKey>,
    /// The store's focus stamp when it was refused. A newer local intent supersedes it.
    pub(super) stamp: u64,
    pub(super) at: Instant,
}

/// What this app run published. Memory only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FocusPublishCounters {
    pub(crate) publishes: u64,
    pub(crate) contexts: u64,
    pub(crate) unplaced_holds: u64,
    pub(crate) unplaced_placed: u64,
    pub(crate) startup_restores: u64,
}

#[derive(Default)]
pub(crate) struct FocusPublish {
    pub(super) unplaced: Option<UnplacedSelection>,
    /// The All Automations overview is open, since the store's focus stamp this holds: any newer
    /// local focus intent leaves it, as any focus left it in the runtime.
    pub(super) quick_automations_since: Option<u64>,
    /// The newest context payload handed to the main window, applied or about to be.
    scheduled_context: Option<String>,
    /// The store's focus as the last publish found it, so a focus intent made anywhere is
    /// published once (`gx_store_publish_if_focus_moved`).
    published_focus: Option<PublishedFocus>,
    /// A publish is running: a selection it causes is published by it, not inside it.
    publishing: bool,
    /// Something asked for a publish while one was running.
    publish_again: bool,
    startup_restore_done: bool,
    pub(crate) counters: FocusPublishCounters,
}

/// What a publish was made from.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PublishedFocus {
    stamp: u64,
    active_project: Option<ghostex_gx_core::ProjectKey>,
    active_group: Option<ActiveGroup>,
    focused: Option<SessionKey>,
    visible: Vec<SessionKey>,
    overview: bool,
}

impl FocusPublish {
    fn unplaced_for(&self, stamp: u64) -> Option<&UnplacedSelection> {
        self.unplaced
            .as_ref()
            .filter(|held| held.stamp == stamp && held.at.elapsed() < UNPLACED_HOLD)
    }

    fn quick_automations_open(&self, stamp: u64) -> bool {
        self.quick_automations_since == Some(stamp)
    }
}

impl GhostexGpuiApp {
    /// The focus state the workspace follows, built from the store. `None` while it cannot be
    /// judged: this computer has no snapshot yet and no remote project owns the selection.
    fn gx_store_workspace_focus_state(&self) -> Option<GpuiGxserverPresentationFocusState> {
        let core = &self.gx_store.core;
        let store = core.presentation();
        let focus = core.focus();
        let publish = &self.gx_store.focus_publish;
        if publish.quick_automations_open(focus.local_stamp) {
            store.loaded(&MachineId::Local)?;
            // `openQuickAutomationsPage`: the overview owns the Chats group and adds its synthetic
            // row to the visible set.
            let tabs = store
                .tab_sessions(&ActiveGroup::Chats(MachineId::Local))
                .loaded()
                .map(|tabs| workspace_tab_sessions(core, &tabs));
            let mut visible = focus
                .visible_sessions
                .iter()
                .map(SessionKey::to_focus_state_session_id)
                .collect::<Vec<_>>();
            if !visible.iter().any(|id| id == QUICK_AUTOMATIONS_SESSION_ID) {
                visible.push(QUICK_AUTOMATIONS_SESSION_ID.to_string());
            }
            return Some(GpuiGxserverPresentationFocusState {
                active_project_id: Some(GPUI_QUICK_AUTOMATIONS_PROJECT_ID.to_string()),
                active_project_tab_sessions: tabs,
                focused_session_id: Some(QUICK_AUTOMATIONS_SESSION_ID.to_string()),
                visible_session_ids: visible,
            });
        }
        let (project, group, focused, visible) = match publish.unplaced_for(focus.local_stamp) {
            Some(held) => {
                let project = held.session.project_key();
                let group = default_group_for_project(store, &project);
                (
                    Some(project),
                    Some(group),
                    Some(held.session.clone()),
                    held.visible.clone(),
                )
            }
            None => (
                focus.active_project.clone(),
                focus.active_group.clone(),
                focus.focused_session.clone(),
                focus.visible_sessions.clone(),
            ),
        };
        let remote_selection = project
            .as_ref()
            .is_some_and(|project| !project.machine.is_local());
        if store.loaded(&MachineId::Local).is_none() && !remote_selection {
            return None;
        }
        let tabs = group
            .as_ref()
            .and_then(|group| store.tab_sessions(group).loaded())
            .map(|tabs| workspace_tab_sessions(core, &tabs));
        Some(GpuiGxserverPresentationFocusState {
            active_project_id: project.map(|project| project.to_workspace_project_id()),
            active_project_tab_sessions: tabs,
            focused_session_id: focused.map(|session| session.to_focus_state_session_id()),
            visible_session_ids: visible
                .iter()
                .map(SessionKey::to_focus_state_session_id)
                .collect(),
        })
    }

    /// Publishes the store's focus to the workspace: the active project context, then the focus
    /// state with its tab list, in the order the runtime's publish posted them. Cheap when nothing
    /// moved: an equal focus state only re-runs the surfaced attach, as the runtime's did.
    pub(crate) fn gx_store_publish_workspace_focus(&mut self, cx: &mut gpui::Context<Self>) {
        if self.gx_store.focus_publish.publishing {
            self.gx_store.focus_publish.publish_again = true;
            return;
        }
        self.gx_store.focus_publish.publishing = true;
        // A publish can select a tab (a reconcile, an attach), which moves the store's focus
        // again; that focus is published right after this one, never inside it. Bounded: a
        // publish of an unchanged focus selects nothing.
        for _ in 0..4 {
            self.gx_store.focus_publish.publish_again = false;
            self.gx_store_publish_workspace_focus_once(cx);
            if !self.gx_store.focus_publish.publish_again {
                break;
            }
        }
        self.gx_store.focus_publish.publishing = false;
    }

    fn gx_store_publish_workspace_focus_once(&mut self, cx: &mut gpui::Context<Self>) {
        let published = self.gx_store_published_focus();
        let Some(state) = self.gx_store_workspace_focus_state() else {
            return;
        };
        self.gx_store.focus_publish.published_focus = Some(published);
        self.gx_store.focus_publish.counters.publishes += 1;
        self.gx_store_publish_project_context(cx);
        self.set_sidebar_gxserver_presentation_focus_state(state, cx);
    }

    /// Publishes when the store's focus moved since the last publish and no selection is waiting
    /// for its burst to end (its finish publishes). This is how a focus intent made anywhere (a
    /// wake of a project, a new group, a fork's group, a Git workflow's project) reaches the
    /// workspace, as the runtime's publish after each of them did.
    pub(crate) fn gx_store_publish_if_focus_moved(&mut self, cx: &mut gpui::Context<Self>) {
        let local_focus = &self.gx_store.local_focus;
        if self.gx_store.focus_publish.publishing
            || local_focus.pending_finish.is_some()
            || local_focus.settle_due.is_some()
        {
            return;
        }
        if self.gx_store.focus_publish.published_focus.as_ref()
            == Some(&self.gx_store_published_focus())
        {
            return;
        }
        self.gx_store_publish_workspace_focus(cx);
    }

    fn gx_store_published_focus(&self) -> PublishedFocus {
        let focus = self.gx_store.core.focus();
        PublishedFocus {
            stamp: focus.local_stamp,
            active_project: focus.active_project.clone(),
            active_group: focus.active_group.clone(),
            focused: focus.focused_session.clone(),
            visible: focus.visible_sessions.clone(),
            overview: self
                .gx_store
                .focus_publish
                .quick_automations_open(focus.local_stamp),
        }
    }

    /// The active project context for the store's focus, applied when it changed. It needs the
    /// main window (the view swaps and the Automate landing do), so it is applied right after the
    /// current update, before anything queued behind it.
    fn gx_store_publish_project_context(&mut self, cx: &mut gpui::Context<Self>) {
        let core = &self.gx_store.core;
        let publish = &self.gx_store.focus_publish;
        let focus = core.focus();
        let overview = publish.quick_automations_open(focus.local_stamp);
        let payload = match publish.unplaced_for(focus.local_stamp) {
            Some(held) => {
                let project = held.session.project_key();
                let group = default_group_for_project(core.presentation(), &project);
                ghostex_gx_core::project_context_payload(core.presentation(), &project, &group)
                    .unwrap_or_else(ghostex_gx_core::quick_projectless_payload)
            }
            None => active_project_context_payload(core.presentation(), focus, overview),
        }
        .to_string();
        if publish.scheduled_context.as_deref() == Some(payload.as_str()) {
            return;
        }
        self.gx_store.focus_publish.scheduled_context = Some(payload.clone());
        self.gx_store.focus_publish.counters.contexts += 1;
        self.gx_store_with_main_window(cx, move |app, window, cx| {
            // The receiver stores the snapshot and does nothing when it did not change.
            app.receive_sidebar_project_context_payload(&payload, window, cx);
        });
    }

    /// Runs `perform` with the main window once the current update has returned. Nothing runs
    /// before the first frame has recorded the window; the next publish tries again.
    pub(crate) fn gx_store_with_main_window(
        &mut self,
        cx: &mut gpui::Context<Self>,
        perform: impl FnOnce(&mut Self, &mut gpui::Window, &mut gpui::Context<Self>) + 'static,
    ) {
        let Some(main) = self.main_window_handle else {
            // Nothing was handed over, so the next publish hands it over again.
            self.gx_store.focus_publish.scheduled_context = None;
            return;
        };
        let app = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                if let Some(app) = app.upgrade() {
                    app.update(cx, |app, cx| perform(app, window, cx));
                }
            });
        });
    }

    /// A selection the store refused because its row has not arrived: the workspace follows it
    /// anyway, as it followed the runtime's focus, until the row arrives and the store takes it.
    pub(super) fn gx_store_hold_unplaced_selection(
        &mut self,
        session: SessionKey,
        visible: Vec<SessionKey>,
    ) {
        let stamp = self.gx_store.core.focus().local_stamp;
        self.gx_store.focus_publish.counters.unplaced_holds += 1;
        self.gx_store.focus_publish.unplaced = Some(UnplacedSelection {
            session,
            visible,
            stamp,
            at: Instant::now(),
        });
    }

    /// Whether the store's newest selection is this local session: its focus, or the selection it
    /// holds for a row that has not arrived yet. An attach that completes late asks this before it
    /// selects its tab.
    ///
    /// CDXC:FocusRouting 2026-09-25 WHY:
    /// A sidebar attach used to check the workspace's focus state copy, which lags the store while a project switch is coalesced (the publish waits for the trailing flush). A fast Ctrl+Tab walk across projects then let the attach of a row it had already left select that row again, and the next step walked on from there, landing rows short. The store's own selection is the planned target, so a newer step always wins.
    pub(crate) fn gx_store_selection_names_local_session(
        &self,
        key: &GpuiLocalWorkspaceSessionKey,
    ) -> bool {
        let focus = self.gx_store.core.focus();
        let selected = match self.gx_store.focus_publish.unplaced_for(focus.local_stamp) {
            Some(held) => Some(&held.session),
            None => focus.focused_session.as_ref(),
        };
        selected.is_some_and(|session| {
            session.machine.is_local()
                && session.project_id == key.project_id
                && session.session_id == key.session_id
        })
    }

    /// A session this app has just created or forked and is opening in its project's workspace:
    /// the store's focus takes it now when its row is there, and holds it until the row arrives
    /// otherwise, so no publish in between pulls the workspace back to the session it came from.
    pub(crate) fn gx_store_select_opened_session(
        &mut self,
        session: &SessionKey,
        cx: &mut gpui::Context<Self>,
    ) {
        let output = self.gx_store.core.handle(
            Event::Intent(Intent::FocusSession {
                session: session.clone(),
                visible: None,
            }),
            now_ms(),
        );
        if output.changes.ignored == Some(IgnoredReason::UnknownTarget) {
            self.gx_store_hold_unplaced_selection(session.clone(), vec![session.clone()]);
            self.gx_store.local_focus.drawn_focus = super::local_focus::DrawnFocus::Unplaced;
        } else {
            self.gx_store.focus_publish.unplaced = None;
            self.gx_store.local_focus.drawn_focus = super::local_focus::DrawnFocus::Store;
        }
        self.gx_store.run_effects(output.effects);
        if self.gx_store.refresh_row_focus_cache() {
            cx.notify();
        }
        self.gx_store_sidebar_focus_moved(cx);
    }

    /// Frames arrived: a held selection whose row is now in the store becomes the store's focus.
    pub(super) fn gx_store_place_unplaced_selection(&mut self, cx: &mut gpui::Context<Self>) {
        let stamp = self.gx_store.core.focus().local_stamp;
        let Some(held) = self.gx_store.focus_publish.unplaced.clone() else {
            return;
        };
        if held.stamp != stamp || held.at.elapsed() >= UNPLACED_HOLD {
            self.gx_store.focus_publish.unplaced = None;
            return;
        }
        if self
            .gx_store
            .core
            .presentation()
            .session(&held.session)
            .is_none()
        {
            return;
        }
        let output = self.gx_store.core.handle(
            Event::Intent(Intent::FocusSession {
                session: held.session,
                visible: Some(held.visible),
            }),
            now_ms(),
        );
        self.gx_store.focus_publish.unplaced = None;
        self.gx_store.focus_publish.counters.unplaced_placed += 1;
        self.gx_store.local_focus.drawn_focus = super::local_focus::DrawnFocus::Store;
        self.gx_store.run_effects(output.effects);
        if self.gx_store.refresh_row_focus_cache() {
            cx.notify();
        }
        self.gx_store_sidebar_focus_moved(cx);
    }

    /// `openQuickAutomationsPage`: the All Automations overview becomes the active project, which
    /// lands the Automate view through the active project context.
    ///
    /// CDXC:Automations 2026-07-08:
    /// Mirror macOS `focusQuickAutomationsProject`: selecting the synthetic quick-automations project activates the Quick group and focused overview row; Rust receives the Automate workarea through the active-project context post.
    pub(crate) fn gx_store_open_quick_automations(&mut self, cx: &mut gpui::Context<Self>) {
        let stamp = self.gx_store.core.focus().local_stamp;
        self.gx_store.focus_publish.quick_automations_since = Some(stamp);
        self.gx_store_publish_workspace_focus(cx);
    }

    /// The first snapshot of this computer arrived: the session the user quit on is materialized
    /// where the restored layout shows it, once.
    ///
    /// CDXC:Navigation 2026-09-04 DECISION:
    /// User: restart must bring back the last active project, the last active view (Agents, Code, and so on), and the last visible sessions, so the user can continue where they left off.
    /// The focus normally switches the app to Agents and moves keyboard focus to the pane, which is right for a sidebar click but undoes the restored view when the user quit on Code, Browser, Kanban, Automate, or Docs. `startupRestore` says this is the restore replay, not a click, so the restored mode is kept and the session is only materialized where the restored layout already shows it. Only a focused session that is still running and was visible at quit is replayed (the old runtime's `autoMaterializeStartupFocusedSession`); every other surfaced session is restored from the workspace model.
    pub(super) fn gx_store_restore_startup_focus(&mut self, cx: &mut gpui::Context<Self>) {
        if self.gx_store.focus_publish.startup_restore_done {
            return;
        }
        let store = self.gx_store.core.presentation();
        let Some(loaded) = store.loaded(&MachineId::Local) else {
            return;
        };
        self.gx_store.focus_publish.startup_restore_done = true;
        let focus = self.gx_store.core.focus();
        let Some(session) = focus.focused_session.clone() else {
            return;
        };
        if !session.machine.is_local() || !focus.visible_sessions.contains(&session) {
            return;
        }
        let running = loaded
            .server_session(&session.project_id, &session.session_id)
            .is_some_and(|row| row.lifecycle_state == LifecycleState::Running);
        if !running {
            return;
        }
        self.gx_store.focus_publish.counters.startup_restores += 1;
        self.gx_store_request_workspace_focus(
            GpuiSidebarWorkspaceTerminalFocusMessage {
                force_remount: false,
                placement: GpuiWorkspaceTerminalFocusPlacement::Tab,
                placement_target_session_id: None,
                preferred_interface: GpuiPreferredAgentInterface::Terminal,
                project_id: session.project_id,
                session_id: session.session_id,
                startup_restore: true,
                keep_view: false,
                wake_sleeping: false,
                keep_sleeping: false,
            },
            cx,
        );
    }
}

/// The store's tab rows in the workspace's shape.
fn workspace_tab_sessions(
    core: &ghostex_gx_core::Core,
    tabs: &[TabSession],
) -> Vec<GpuiSidebarWorkspaceTabSession> {
    tabs.iter()
        .map(|tab| workspace_tab_session(core, tab))
        .collect()
}

/// One tab row. The fields are the ones the runtime's `activeWorkspaceTabSessionsFromLatestGroups`
/// published; the lifecycle words are the sidebar's (`presentationLifecycleStateForSidebar`), which
/// the old contract parser mapped to the pane states the same way.
fn workspace_tab_session(
    core: &ghostex_gx_core::Core,
    tab: &TabSession,
) -> GpuiSidebarWorkspaceTabSession {
    let key = match tab.key.machine.remote_id() {
        None => GpuiWorkspaceTerminalSessionKey::Local(GpuiLocalWorkspaceSessionKey {
            project_id: tab.key.project_id.clone(),
            session_id: tab.key.session_id.clone(),
        }),
        Some(machine_id) => GpuiWorkspaceTerminalSessionKey::Remote(GpuiRemoteAttachSessionKey {
            remote_machine_id: machine_id.to_string(),
            project_id: tab.key.project_id.clone(),
            session_id: tab.key.session_id.clone(),
        }),
    };
    let presentation_state = match (&tab.is_sleeping, &tab.lifecycle_state) {
        (true, _) | (false, LifecycleState::Sleeping) => TerminalSessionPresentationState::Sleeping,
        (false, LifecycleState::Running) => TerminalSessionPresentationState::Running,
        (false, LifecycleState::Unknown) => TerminalSessionPresentationState::StartupFailed,
        (false, other) if other.as_str() == "missing" => {
            TerminalSessionPresentationState::StartupFailed
        }
        (false, _) => TerminalSessionPresentationState::RestoredUnmounted,
    };
    let activity = match tab.bridge_activity() {
        "working" => AgentTerminalActivity::Working,
        "attention" => AgentTerminalActivity::Attention,
        _ => AgentTerminalActivity::Idle,
    };
    let switchable_agents = core
        .presentation()
        .session(&tab.key)
        .and_then(|session| session.switchable_agents.clone())
        .and_then(|rows| rows.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .take(64)
        .filter_map(switchable_agent)
        .collect();
    GpuiSidebarWorkspaceTabSession {
        activity,
        working_directory: tab.working_directory.clone(),
        agent_icon: gpui_sidebar_agent_icon(tab.agent_icon.as_deref()),
        agent_name: tab.agent_name.clone(),
        agent_session_id: tab.agent_session_id.clone(),
        key,
        kind: AgentsWorkspaceSessionKind::Terminal,
        is_draft: tab.is_draft,
        is_generating_first_prompt_title: tab.is_generating_first_prompt_title,
        presentation_state,
        has_session_note: tab.has_session_note,
        stashed_prompt_count: tab.stashed_prompt_count.unwrap_or(0).min(200),
        switchable_agents,
        title: tab.title.clone(),
    }
}

/// One daemon-resolved account row (`agentId`, `icon`, `name`); a row missing its id or name is
/// left out.
fn switchable_agent(row: &Value) -> Option<GpuiSwitchableSessionAgent> {
    let text = |key: &str| {
        row.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    Some(GpuiSwitchableSessionAgent {
        agent_id: text("agentId")?,
        icon: gpui_sidebar_agent_icon(row.get("icon").and_then(Value::as_str)),
        name: text("name")?,
    })
}
