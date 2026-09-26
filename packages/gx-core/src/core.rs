//! The top-level state machine: events in, state plus effects out.

use ghostex_gx_protocol::{
    EventParseError, PresentationSnapshot, ServerEvent, GXSERVER_PROTOCOL_VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::change::{ChangeSummary, IgnoredReason};
use crate::connection::ConnectionUpdate;
use crate::focus::{ExternalFocusUpdate, FocusOutcome, FocusState};
use crate::keys::{MachineId, ProjectKey, SessionKey};
use crate::overlay::SessionPatch;
use crate::presentation_store::{PresentationStore, SideStateUpdate, SnapshotOrigin};
use crate::selectors::{Loadable, TabDirection, TabSession};

/// Something the user or the host did. Applied synchronously; never waits on the daemon.
///
/// New variants are added by later milestones; match with a wildcard arm.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum Intent {
    /// Focus a session. `visible` is the host's exact rendered set when the selection came from
    /// the workspace (tab click, next or previous tab); leave it `None` for a sidebar click. The
    /// active project and group follow from where the session lives.
    FocusSession {
        session: SessionKey,
        visible: Option<Vec<SessionKey>>,
    },
    /// Make a project active without choosing a session.
    FocusProject {
        project: ProjectKey,
    },
    /// Make one of a project's USER-MADE session groups active without choosing a session, which
    /// is what creating a group from a session does.
    FocusSubgroup {
        project: ProjectKey,
        group_id: String,
    },
    /// A client-owned document, after a LOCAL edit or after an echo its guard let through: the
    /// workspace session groups document, the project collections document, or the Spaces
    /// document. The list draws all three from the store's side state, so the only way an edit
    /// reaches the screen is to put it there; it carries no revision, because the client owns these
    /// documents and the daemon keeps a copy.
    ///
    /// ONE variant for all three rather than one each: the three differ only in which field of the
    /// side state they land in, and `SideStateUpdate` already says that.
    SetSideState {
        machine: MachineId,
        update: Box<SideStateUpdate>,
    },
    /// The host reports the exact set of sessions that own a pane.
    SetVisibleSessions {
        sessions: Vec<SessionKey>,
    },
    /// The host reports which sessions are on screen (Auto Sleep safety).
    SetDisplayedSessions {
        sessions: Vec<SessionKey>,
    },
    /// A focus update that did not start here; dropped when older than the newest local intent,
    /// and anything it names that the store does not hold is dropped too.
    ExternalFocus(ExternalFocusUpdate),
    /// Local-first close: hide the row now, let the daemon catch up.
    HideSession {
        session: SessionKey,
    },
    /// The close request failed: show the row again.
    UnhideSession {
        session: SessionKey,
    },
    /// Local-first close to recent.
    HideProject {
        project: ProjectKey,
    },
    UnhideProject {
        project: ProjectKey,
    },
    /// Show an optimistic lifecycle or activity value until the daemon reports it, reports
    /// something newer, or the patch expires.
    PatchSession {
        session: SessionKey,
        patch: SessionPatch,
    },
    /// The request a patch anticipated failed.
    ClearSessionPatch {
        session: SessionKey,
    },
    /// The user saw the session: acknowledge its attention now, or once it has been visible for
    /// the minimum time (`attention.rs`).
    AcknowledgeAttention {
        session: SessionKey,
    },
    /// The timer an [`Effect::ArmAttentionAcknowledge`] asked for fired.
    AttentionAcknowledgeDue {
        session: SessionKey,
        entered_at_ms: u64,
    },
    /// Escape was pressed in the session's terminal (`attention.rs`).
    TerminalEscape {
        session: SessionKey,
    },
    /// The manual order of a project's sessions, written locally before `/api/updateSessionOrder`
    /// is awaited so the rows move under the user's finger.
    ReorderProjectSessions {
        project: ProjectKey,
        session_ids: Vec<String>,
    },
}

/// An input to the core.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Event {
    /// A parsed frame from a machine's event stream. Parse off the UI thread with
    /// [`ServerEvent::parse`]; [`Core::handle_raw_frame`] does both for tools.
    Frame {
        machine: MachineId,
        frame: Box<ServerEvent>,
    },
    /// The result of an HTTP `readPresentationSnapshot`.
    SnapshotRead {
        machine: MachineId,
        snapshot: Box<PresentationSnapshot>,
    },
    /// The result of an HTTP `listProjects`: the full domain project rows.
    DomainProjectsRead {
        machine: MachineId,
        projects: Vec<Value>,
    },
    /// What the socket client reports about a machine's event stream.
    Connection {
        machine: MachineId,
        update: ConnectionUpdate,
    },
    /// A machine was removed or its state must be forgotten.
    MachineUnloaded {
        machine: MachineId,
    },
    Intent(Intent),
    /// Time passed. The host decides how often; the core only compares deadlines to `now_ms`.
    Tick,
}

/// Why the host must subscribe to a machine's presentation again.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum ResubscribeReason {
    /// The daemon answered `presentationSnapshotCurrent` but the store holds nothing.
    CurrentWithoutSnapshot,
    /// A frame named another daemon than the loaded snapshot.
    ServerChanged,
}

/// A request the host must perform. The core never performs I/O itself.
///
/// New variants are added by later milestones; match with a wildcard arm.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum Effect {
    /// Send `subscribePresentation` again without `lastRevision`, which forces a full snapshot.
    /// Asked once per cause: further frames that have to be ignored until the snapshot arrives do
    /// not repeat it.
    ResubscribePresentation {
        machine: MachineId,
        reason: ResubscribeReason,
    },
    /// `globalSidebarCommandsChanged` carries no payload: read `/api/readSidebarHud`.
    RefetchSidebarHud { machine: MachineId },
    /// `notificationFeedChanged` carries no payload: read `/api/readNotificationFeed`.
    RefetchNotificationFeed { machine: MachineId },
    /// The machine's stream just went live: a first connect, or back after a loss, when the
    /// payload-less announcements above may have been missed. Read the HUD, the recent projects
    /// and the notification feed again (`refetch.rs`).
    MachineLive { machine: MachineId },
    /// An applied delta carried a project's domain row, or removed a project. Agents and project
    /// Actions are project metadata, so the HUD is read again; the recent projects too when
    /// `removed`, when the row says `is_recent_project`, or when the host lists the project as
    /// recent (`refetch.rs`).
    DomainProjectChanged {
        machine: MachineId,
        project_id: String,
        is_recent_project: bool,
        removed: bool,
    },
    /// Call back with [`Intent::AttentionAcknowledgeDue`] after `delay_ms`: the attention has not
    /// been visible for the minimum time yet.
    ArmAttentionAcknowledge {
        session: SessionKey,
        delay_ms: u64,
        entered_at_ms: u64,
    },
    /// Tell the session's daemon: `/api/updateAgentActivity` with `event` and `agentName` when known.
    ReportAgentActivity {
        session: SessionKey,
        report: crate::attention::AgentActivityReport,
        agent_name: Option<String>,
    },
    /// A live delta moved a session of this computer into unacknowledged attention: play the
    /// completion sound unless the user turned it off.
    SessionAttentionRaised { session: SessionKey },
    /// Persist the user's last selected session of a project (it must survive restarts, and the
    /// project being closed and reopened).
    ///
    /// This fires for every focus change to another session, so a held "next tab" key produces
    /// one per repeat. The host must coalesce: keep only the newest per project and write it after
    /// the burst (a short debounce, and on quit), never once per effect on the UI thread.
    RememberProjectSession {
        project: ProjectKey,
        session: SessionKey,
    },
    /// A snapshot carried rows that did not fit their type and were left out. The machine still
    /// loaded; the host logs this, because it means a damaged daemon row or a moved wire contract.
    ReportSkippedRows {
        machine: MachineId,
        projects: usize,
        groups: usize,
        sessions: usize,
        first_error: String,
    },
}

/// The result of handling one event.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct Output {
    pub effects: Vec<Effect>,
    pub changes: ChangeSummary,
}

/// The one owner of product state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Core {
    presentation: PresentationStore,
    focus: FocusState,
    tabs_generation: u64,
    attention: crate::attention::AttentionTracker,
}

impl Core {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn presentation(&self) -> &PresentationStore {
        &self.presentation
    }

    pub fn focus(&self) -> &FocusState {
        &self.focus
    }

    /// Seeds focus from persisted state before anything else happens (startup restore). Does not
    /// count as a local intent. What it names is checked against each machine's rows once that
    /// machine loads.
    pub fn restore_focus(&mut self, focus: FocusState) {
        self.focus = focus;
    }

    /// Seeds a remote machine the host has not connected to in this run from its last-seen
    /// snapshot, so the user sees its sessions faded instead of an empty tab. Not a local intent
    /// and not a frame: nothing here came from a daemon, which is the whole point.
    ///
    /// Returns the changes so the caller can feed them to its list exactly as a frame's are fed.
    /// See [`PresentationStore::seed_last_seen`] for the user decision and the revision rules.
    pub fn seed_last_seen_presentation(
        &mut self,
        machine: &MachineId,
        snapshot: PresentationSnapshot,
    ) -> Output {
        let mut output = Output {
            changes: self.presentation.seed_last_seen(machine, snapshot),
            ..Output::default()
        };
        // The same two follow-ups a frame gets, so a seeded machine cannot leave focus or the tab
        // generation behind, and its effects reach the host rather than being dropped here.
        self.settle_after_change(&mut output);
        output
    }

    /// The workspace tabs of the active group. `NotLoaded` until the owning machine's first
    /// snapshot arrived and while no group is active; never `Missing` after an event was handled,
    /// because focus is re-homed when its project goes away.
    ///
    /// `Loaded(vec![])` is a real answer in two cases: a project that has no listed sessions, and
    /// the Chats collection of a machine with no chat sessions, which is where focus is re-homed
    /// when no code project exists (`active_project` is then `None`). A host must never reconcile
    /// a PROJECT's tabs from the Chats list: check `focus().active_group` and `active_project`
    /// first, and treat "Chats with no active project" as "no project workspace", not as "this
    /// project has no sessions".
    pub fn active_tab_sessions(&self) -> Loadable<Vec<TabSession>> {
        match &self.focus.active_group {
            Some(group) => self.presentation.tab_sessions(group),
            None => Loadable::NotLoaded,
        }
    }

    /// The tab next to the focused session in the active group, wrapping. For a held "next tab"
    /// key: follow it with [`Intent::FocusSession`]. It builds no rows and clones no session; see
    /// [`PresentationStore::adjacent_tab_session`] for what it does allocate.
    pub fn adjacent_tab_session(&self, direction: TabDirection) -> Option<SessionKey> {
        self.presentation.adjacent_tab_session(
            self.focus.active_group.as_ref()?,
            self.focus.focused_session.as_ref(),
            direction,
        )
    }

    /// Moves whenever the ordered keys of some group's tab list may have changed (see
    /// [`ChangeSummary::tab_lists_changed`] for the exact rule). A tab strip keeps the list it
    /// built and rebuilds it when this number moves. It never misses a key change; it can move
    /// without one. Row content (title, activity) is reported through `sessions_changed` and does
    /// not move it.
    pub fn tabs_generation(&self) -> u64 {
        self.tabs_generation
    }

    /// Parses and handles one raw frame. For tools; a real client parses on its socket thread and
    /// sends [`Event::Frame`].
    pub fn handle_raw_frame(
        &mut self,
        machine: MachineId,
        frame: &str,
        now_ms: u64,
    ) -> Result<Output, EventParseError> {
        let frame = ServerEvent::parse(frame)?;
        Ok(self.handle(
            Event::Frame {
                machine,
                frame: Box::new(frame),
            },
            now_ms,
        ))
    }

    /// Applies a burst of events and returns one combined output, so the host repaints once.
    pub fn handle_batch(&mut self, events: impl IntoIterator<Item = Event>, now_ms: u64) -> Output {
        let mut combined = Output::default();
        for event in events {
            let output = self.handle(event, now_ms);
            combined.changes.merge(output.changes);
            for effect in output.effects {
                // A later request of the same kind replaces the earlier one.
                combined
                    .effects
                    .retain(|earlier| !supersedes(&effect, earlier));
                combined.effects.push(effect);
            }
        }
        combined
    }

    /// Applies one event. Synchronous and free of I/O; `now_ms` is the host's clock.
    pub fn handle(&mut self, event: Event, now_ms: u64) -> Output {
        let mut output = Output::default();
        let live_delta = match &event {
            Event::Frame { machine, frame } => {
                matches!(**frame, ServerEvent::PresentationDelta(_)).then(|| machine.clone())
            }
            _ => None,
        };
        match event {
            Event::Frame { machine, frame } => {
                self.handle_frame(&machine, *frame, now_ms, &mut output)
            }
            Event::SnapshotRead { machine, snapshot } => {
                report_skipped_rows(&machine, &snapshot, &mut output);
                output.changes =
                    self.presentation
                        .apply_snapshot(&machine, "", *snapshot, SnapshotOrigin::Read);
            }
            Event::DomainProjectsRead { machine, projects } => {
                output.changes = self.presentation.set_domain_projects(&machine, projects);
            }
            Event::Connection { machine, update } => {
                let was_live = self.is_live(&machine);
                output.changes = self.presentation.apply_connection(&machine, update, now_ms);
                self.note_went_live(&machine, was_live, &mut output);
            }
            Event::MachineUnloaded { machine } => {
                output.changes = self.presentation.unload_machine(&machine);
                let focus = self.focus.machine_gone(&machine);
                self.note_focus(focus, &mut output);
            }
            Event::Intent(intent) => self.handle_intent(intent, now_ms, &mut output),
            Event::Tick => output.changes = self.presentation.expire_patches(now_ms),
        }
        self.settle_after_change(&mut output);
        self.attention.observe(
            &self.presentation,
            &output.changes,
            live_delta.as_ref(),
            now_ms,
            &mut output.effects,
        );
        output
    }

    /// What every handled event and the last-seen seed both owe once the store has moved: re-home
    /// focus if anything changed, and advance the tab generation if a tab list did. One function
    /// rather than two copies, because a seed that skipped either would leave focus naming a row
    /// nobody holds and a tab strip drawing a list that moved.
    fn settle_after_change(&mut self, output: &mut Output) {
        // Focus can only fall out of step with the store when something changed.
        if !output.changes.is_empty() {
            let focus = self.focus.reconcile(&self.presentation);
            self.note_focus(focus, output);
        }
        if output.changes.tab_lists_changed() {
            self.tabs_generation += 1;
        }
    }

    fn handle_frame(
        &mut self,
        machine: &MachineId,
        frame: ServerEvent,
        now_ms: u64,
        output: &mut Output,
    ) {
        if let Some(received) = frame.protocol_version() {
            if received != GXSERVER_PROTOCOL_VERSION {
                output.changes =
                    ChangeSummary::ignored(IgnoredReason::ProtocolMismatch { received });
                return;
            }
        }
        match frame {
            ServerEvent::PresentationSnapshot(frame) => {
                report_skipped_rows(machine, &frame.snapshot, output);
                output.changes = self.presentation.apply_snapshot(
                    machine,
                    &frame.header.server_id,
                    *frame.snapshot,
                    SnapshotOrigin::Stream,
                );
                self.note_stream_acknowledged(machine, now_ms, output);
            }
            ServerEvent::PresentationSnapshotCurrent(frame) => {
                output.changes = self.presentation.apply_snapshot_current(
                    machine,
                    &frame.header.server_id,
                    frame.revision,
                );
                match output.changes.ignored {
                    Some(IgnoredReason::NotLoaded) => self.request_resubscribe(
                        machine,
                        ResubscribeReason::CurrentWithoutSnapshot,
                        output,
                    ),
                    Some(_) => self.resubscribe_if_server_changed(machine, output),
                    None => self.note_stream_acknowledged(machine, now_ms, output),
                }
            }
            ServerEvent::PresentationDelta(frame) => {
                let refetch = crate::refetch::delta_refetch(machine, &frame.delta);
                output.changes = self.presentation.apply_delta(
                    machine,
                    &frame.header.server_id,
                    frame.revision,
                    frame.delta,
                );
                self.resubscribe_if_server_changed(machine, output);
                if output.changes.ignored.is_none() {
                    output.effects.extend(refetch);
                }
            }
            ServerEvent::WorkspaceGroupsChanged(frame) => self.handle_side_state(
                machine,
                &frame.header.server_id,
                frame.revision,
                SideStateUpdate::WorkspaceGroups(frame.groups),
                output,
            ),
            ServerEvent::SidebarProjectCollectionsChanged(frame) => self.handle_side_state(
                machine,
                &frame.header.server_id,
                frame.revision,
                SideStateUpdate::ProjectCollections(frame.sidebar_project_collections),
                output,
            ),
            ServerEvent::SidebarSpacesChanged(frame) => self.handle_side_state(
                machine,
                &frame.header.server_id,
                frame.revision,
                SideStateUpdate::Spaces(frame.sidebar_spaces),
                output,
            ),
            ServerEvent::CustomSessionTagsChanged(frame) => self.handle_side_state(
                machine,
                &frame.header.server_id,
                frame.revision,
                SideStateUpdate::CustomSessionTags(frame.custom_session_tags),
                output,
            ),
            ServerEvent::GlobalSidebarCommandsChanged(frame) => {
                if let Some(revision) = frame.revision {
                    self.presentation
                        .note_revision(machine, &frame.header.server_id, revision);
                }
                output.effects.push(Effect::RefetchSidebarHud {
                    machine: machine.clone(),
                });
            }
            ServerEvent::NotificationFeedChanged(_) => {
                output.effects.push(Effect::RefetchNotificationFeed {
                    machine: machine.clone(),
                });
            }
            ServerEvent::EventStreamReady(_)
            | ServerEvent::ServerStarted(_)
            | ServerEvent::ServerStopping(_)
            | ServerEvent::ApiRequestHandled(_)
            | ServerEvent::RendererCommand(_)
            | ServerEvent::SessionChatSnapshot(_)
            | ServerEvent::SessionChatReplaced(_)
            | ServerEvent::SessionChatAppended(_)
            | ServerEvent::SessionChatState(_)
            | ServerEvent::Unknown { .. } => {
                output.changes = ChangeSummary::ignored(IgnoredReason::NotOwnedYet);
            }
        }
    }

    fn handle_side_state(
        &mut self,
        machine: &MachineId,
        server_id: &str,
        revision: Option<i64>,
        update: SideStateUpdate,
        output: &mut Output,
    ) {
        output.changes = self
            .presentation
            .apply_side_state(machine, server_id, revision, update);
        self.resubscribe_if_server_changed(machine, output);
    }

    /// A stream snapshot or a snapshot-current answer means the stream is live.
    fn note_stream_acknowledged(&mut self, machine: &MachineId, now_ms: u64, output: &mut Output) {
        let was_live = self.is_live(machine);
        let connection =
            self.presentation
                .apply_connection(machine, ConnectionUpdate::Live, now_ms);
        output.changes.merge(connection);
        self.note_went_live(machine, was_live, output);
    }

    fn is_live(&self, machine: &MachineId) -> bool {
        self.presentation
            .machine(machine)
            .is_some_and(|entry| entry.connection().phase == crate::ConnectionPhase::Live)
    }

    /// Asks the host to read again what the stream only announces, once per transition to live.
    fn note_went_live(&self, machine: &MachineId, was_live: bool, output: &mut Output) {
        if !was_live && self.is_live(machine) {
            output.effects.push(Effect::MachineLive {
                machine: machine.clone(),
            });
        }
    }

    fn resubscribe_if_server_changed(&mut self, machine: &MachineId, output: &mut Output) {
        if output.changes.ignored == Some(IgnoredReason::ServerChanged) {
            self.request_resubscribe(machine, ResubscribeReason::ServerChanged, output);
        }
    }

    fn request_resubscribe(
        &mut self,
        machine: &MachineId,
        reason: ResubscribeReason,
        output: &mut Output,
    ) {
        if self.presentation.begin_resubscribe(machine) {
            output.effects.push(Effect::ResubscribePresentation {
                machine: machine.clone(),
                reason,
            });
        }
    }

    fn handle_intent(&mut self, intent: Intent, now_ms: u64, output: &mut Output) {
        match intent {
            Intent::FocusSession { session, visible } => {
                // Before the first snapshot the target cannot be checked (startup restore); once
                // the machine is loaded a session that does not exist is refused.
                let loaded = self.presentation.loaded(&session.machine).is_some();
                if loaded && self.presentation.session(&session).is_none() {
                    output.changes = ChangeSummary::ignored(IgnoredReason::UnknownTarget);
                    return;
                }
                let focus = self
                    .focus
                    .focus_session(&self.presentation, session, visible, now_ms);
                self.note_focus(focus, output);
            }
            Intent::FocusProject { project } => {
                let loaded = self.presentation.loaded(&project.machine).is_some();
                if loaded && self.presentation.project(&project).is_none() {
                    output.changes = ChangeSummary::ignored(IgnoredReason::UnknownTarget);
                    return;
                }
                let focus = self
                    .focus
                    .focus_project(&self.presentation, project, now_ms);
                self.note_focus(focus, output);
            }
            Intent::FocusSubgroup { project, group_id } => {
                let loaded = self.presentation.loaded(&project.machine).is_some();
                if loaded && self.presentation.project(&project).is_none() {
                    output.changes = ChangeSummary::ignored(IgnoredReason::UnknownTarget);
                    return;
                }
                let focus =
                    self.focus
                        .focus_subgroup(&self.presentation, project, group_id, now_ms);
                self.note_focus(focus, output);
            }
            Intent::SetSideState { machine, update } => {
                output.changes = self
                    .presentation
                    .apply_side_state(&machine, "", None, *update);
            }
            Intent::SetVisibleSessions { sessions } => {
                let focus = self.focus.set_visible_sessions(sessions, now_ms);
                self.note_focus(focus, output);
            }
            Intent::SetDisplayedSessions { sessions } => {
                let focus = self.focus.set_displayed_sessions(sessions);
                self.note_focus(focus, output);
            }
            Intent::ExternalFocus(update) => {
                let focus = self.focus.apply_external(update);
                self.note_focus(focus, output);
            }
            Intent::HideSession { session } => {
                output.changes = self.presentation.hide_session(&session);
            }
            Intent::UnhideSession { session } => {
                output.changes = self.presentation.unhide_session(&session);
            }
            Intent::HideProject { project } => {
                output.changes = self.presentation.hide_project(&project);
            }
            Intent::UnhideProject { project } => {
                output.changes = self.presentation.unhide_project(&project);
            }
            Intent::PatchSession { session, patch } => {
                output.changes = self.presentation.patch_session(&session, patch);
            }
            Intent::ClearSessionPatch { session } => {
                output.changes = self.presentation.clear_session_patch(&session);
            }
            Intent::AcknowledgeAttention { session } => crate::attention::acknowledge(
                &mut self.attention,
                &mut self.presentation,
                &session,
                now_ms,
                output,
            ),
            Intent::AttentionAcknowledgeDue {
                session,
                entered_at_ms,
            } => crate::attention::acknowledge_due(
                &mut self.attention,
                &mut self.presentation,
                &session,
                entered_at_ms,
                now_ms,
                output,
            ),
            Intent::TerminalEscape { session } => crate::attention::terminal_escape(
                &mut self.attention,
                &mut self.presentation,
                &session,
                now_ms,
                output,
            ),
            Intent::ReorderProjectSessions {
                project,
                session_ids,
            } => {
                output.changes = self
                    .presentation
                    .reorder_project_sessions(&project, &session_ids);
            }
        }
    }

    fn note_focus(&self, focus: FocusOutcome, output: &mut Output) {
        if let Some((local_stamp, observed_stamp)) = focus.stale_external {
            output.changes.merge(ChangeSummary::ignored(
                IgnoredReason::OlderThanLocalIntent {
                    local_stamp,
                    observed_stamp,
                },
            ));
        }
        if focus.changed || focus.displayed_changed {
            let changes = ChangeSummary {
                focus_changed: focus.changed,
                displayed_changed: focus.displayed_changed,
                ..ChangeSummary::default()
            };
            output.changes.merge(changes);
        }
        if let Some((project, session)) = focus.remembered {
            output
                .effects
                .push(Effect::RememberProjectSession { project, session });
        }
    }
}

/// Whether `later` makes `earlier` pointless inside one batch.
fn supersedes(later: &Effect, earlier: &Effect) -> bool {
    match (later, earlier) {
        (
            Effect::RememberProjectSession { project, .. },
            Effect::RememberProjectSession {
                project: earlier_project,
                ..
            },
        ) => project == earlier_project,
        _ => later == earlier,
    }
}

fn report_skipped_rows(machine: &MachineId, snapshot: &PresentationSnapshot, output: &mut Output) {
    if snapshot.skipped_row_count() == 0 {
        return;
    }
    let first_error = snapshot
        .projects
        .skipped
        .iter()
        .chain(&snapshot.groups.skipped)
        .chain(&snapshot.sessions.skipped)
        .next()
        .map(|row| row.error.clone())
        .unwrap_or_default();
    output.effects.push(Effect::ReportSkippedRows {
        machine: machine.clone(),
        projects: snapshot.projects.skipped.len(),
        groups: snapshot.groups.skipped.len(),
        sessions: snapshot.sessions.skipped.len(),
        first_error,
    });
}
