//! Attention on this client: when a session's attention may be acknowledged, what an Escape in its
//! terminal does, and when a session newly entering attention plays the completion sound.
//!
//! The rules are the old app runtime's (`attention-tracking.ts`), moved here unchanged:
//!
//! - An acknowledgement (the user focused the session, typed or clicked in its terminal) waits
//!   until the attention has been on screen for [`MIN_ATTENTION_VISIBLE_MS`], counted from the
//!   moment THIS client first saw it, so a Done flash is never swallowed by the focus that caused
//!   it. The wait is one timer per session; a newer attention event restarts the count and makes a
//!   pending timer stale.
//! - Acknowledging clears the attention locally at once (an overlay that the daemon's next row
//!   settles, `overlay.rs`) and reports `acknowledge` to the session's daemon.
//! - Escape in a session's terminal suppresses its completion sound for
//!   [`ESCAPE_DONE_SUPPRESSION_MS`], clears its attention locally if it has any, and reports
//!   `escape` to the daemon.
//! - The completion sound plays only on a LIVE delta of this computer's daemon that moves a session
//!   the client already knew from another activity into unacknowledged attention, once per
//!   attention event. Snapshots never play it: a session may have been in attention before the
//!   client saw it.
//!
//! CDXC:Notifications 2026-09-25 WHY:
//! The app runtime port (docs/2026-09-25/app-runtime-port/PLAN.md, family F2) moves attention
//! timing into gx-core so the desktop, the web build and mobile acknowledge the same way. The one
//! change of mechanism: the runtime kept a set of locally acknowledged event ids and hid them on
//! every later row; the store's activity overlay does that job, and drops itself as soon as the
//! daemon catches up or raises a newer event, or after [`ATTENTION_PATCH_TTL_MS`] if the report
//! was lost.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/attention/ (the desktop performs the effects).

use std::collections::{HashMap, HashSet, VecDeque};

use ghostex_gx_protocol::{PresentationSession, SessionActivity};

use crate::change::ChangeSummary;
use crate::core::{Effect, Output};
use crate::keys::{MachineId, SessionKey};
use crate::overlay::SessionPatch;
use crate::presentation_store::PresentationStore;

/// `GPUI_MIN_ATTENTION_VISIBLE_MS`.
pub const MIN_ATTENTION_VISIBLE_MS: u64 = 1_500;
/// `GPUI_ESCAPE_DONE_SUPPRESSION_MS`.
pub const ESCAPE_DONE_SUPPRESSION_MS: u64 = 5_000;
/// `GPUI_ATTENTION_COMPLETION_SOUND_EVENT_CACHE_LIMIT`.
const COMPLETION_SOUND_EVENT_CACHE_LIMIT: usize = 2_048;
/// How long a local acknowledgement hides the attention if the daemon never answers.
pub const ATTENTION_PATCH_TTL_MS: u64 = 60_000;

/// What the daemon is told (`/api/updateAgentActivity`, `event`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentActivityReport {
    Acknowledge,
    Escape,
}

impl AgentActivityReport {
    pub fn event(self) -> &'static str {
        match self {
            Self::Acknowledge => "acknowledge",
            Self::Escape => "escape",
        }
    }
}

/// Per-session attention bookkeeping. Lives in the [`crate::Core`].
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct AttentionTracker {
    /// The activity each held session was last seen with, for "was it in attention before".
    activity: HashMap<SessionKey, SessionActivity>,
    /// When this client first saw the session's current attention.
    entered_at: HashMap<SessionKey, u64>,
    event_id: HashMap<SessionKey, String>,
    /// The acknowledgement waiting for the minimum visible time, by the `entered_at` it waits on.
    armed: HashMap<SessionKey, u64>,
    sound_event_keys: HashSet<String>,
    sound_event_order: VecDeque<String>,
    sound_suppressed_until: HashMap<SessionKey, u64>,
}

/// `getGpuiPresentationAttentionEventId`: the event id, else the entered-at time for an older
/// daemon, trimmed, and only while the session is in attention.
fn attention_event_id(session: &PresentationSession) -> Option<String> {
    if session.activity != SessionActivity::Attention {
        return None;
    }
    let attention = session.attention.as_ref()?;
    [&attention.event_id, &attention.entered_at]
        .into_iter()
        .flatten()
        .map(|value| value.trim())
        .find(|value| !value.is_empty())
        .map(str::to_string)
}

fn effective(store: &PresentationStore, key: &SessionKey) -> Option<PresentationSession> {
    store
        .machine(&key.machine)?
        .effective_session(&key.project_id, &key.session_id)
        .map(|session| session.into_owned())
}

/// `normalizeNonEmptyString` of the session's agent name.
fn agent_name(session: &PresentationSession) -> Option<String> {
    session
        .agent_name
        .clone()
        .filter(|name| !name.trim().is_empty())
}

impl AttentionTracker {
    /// Follows what an applied event changed. `live_delta` names the machine when the event was a
    /// presentation delta, the only kind that may play the completion sound.
    pub(crate) fn observe(
        &mut self,
        store: &PresentationStore,
        changes: &ChangeSummary,
        live_delta: Option<&MachineId>,
        now_ms: u64,
        effects: &mut Vec<Effect>,
    ) {
        if changes.ignored.is_some() {
            return;
        }
        for machine in &changes.machines_reloaded {
            let held: Vec<SessionKey> = self
                .activity
                .keys()
                .filter(|key| key.machine == *machine)
                .cloned()
                .collect();
            let current: Vec<(SessionKey, PresentationSession)> = store
                .loaded(machine)
                .map(|loaded| {
                    loaded
                        .server_sessions()
                        .map(|session| SessionKey {
                            machine: machine.clone(),
                            project_id: session.project_id.clone(),
                            session_id: session.session_id.clone(),
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
                .into_iter()
                .filter_map(|key| effective(store, &key).map(|session| (key, session)))
                .collect();
            for key in held {
                if !current.iter().any(|(held, _)| *held == key) {
                    self.forget(&key);
                }
            }
            for (key, session) in current {
                self.track(&key, &session, now_ms, None);
            }
        }
        let reloaded = |key: &SessionKey| changes.machines_reloaded.contains(&key.machine);
        for key in &changes.sessions_removed {
            if !reloaded(key) {
                self.forget(key);
            }
        }
        for key in &changes.sessions_changed {
            if reloaded(key) {
                continue;
            }
            match effective(store, key) {
                None => self.forget(key),
                Some(session) => {
                    let sound = (live_delta == Some(&key.machine) && key.machine.is_local())
                        .then_some(&mut *effects);
                    self.track(key, &session, now_ms, sound);
                }
            }
        }
    }

    /// `syncPresentationAttentionTracking` for one session, then `detectSessionAttentionCompletionSounds`
    /// when `sound` is given.
    fn track(
        &mut self,
        key: &SessionKey,
        session: &PresentationSession,
        now_ms: u64,
        sound: Option<&mut Vec<Effect>>,
    ) {
        let previous = self.activity.insert(key.clone(), session.activity.clone());
        if session.activity != SessionActivity::Attention {
            self.clear_tracking(key);
            return;
        }
        let next_event = attention_event_id(session);
        let had_event = self.event_id.contains_key(key);
        let event_changed =
            next_event.as_ref() != self.event_id.get(key) && (next_event.is_some() || had_event);
        if !self.entered_at.contains_key(key) || event_changed {
            self.armed.remove(key);
            self.entered_at.insert(key.clone(), now_ms);
        }
        match &next_event {
            Some(event) => {
                self.event_id.insert(key.clone(), event.clone());
            }
            None => {
                self.event_id.remove(key);
            }
        }
        let Some(effects) = sound else {
            return;
        };
        if session
            .attention
            .as_ref()
            .is_some_and(|attention| attention.acknowledged)
        {
            return;
        }
        if self.sound_suppressed(key, now_ms) {
            return;
        }
        match previous {
            None | Some(SessionActivity::Attention) => return,
            Some(_) => {}
        }
        if let Some(event) = &next_event {
            let event_key = format!("{}\u{1f}{event}", key.to_sidebar_session_id());
            if !self.remember_sound_event(event_key) {
                return;
            }
        }
        effects.push(Effect::SessionAttentionRaised {
            session: key.clone(),
        });
    }

    fn forget(&mut self, key: &SessionKey) {
        self.activity.remove(key);
        self.clear_tracking(key);
    }

    fn clear_tracking(&mut self, key: &SessionKey) {
        self.armed.remove(key);
        self.entered_at.remove(key);
        self.event_id.remove(key);
    }

    fn sound_suppressed(&mut self, key: &SessionKey, now_ms: u64) -> bool {
        match self.sound_suppressed_until.get(key) {
            Some(until) if *until > now_ms => true,
            Some(_) => {
                self.sound_suppressed_until.remove(key);
                false
            }
            None => false,
        }
    }

    fn remember_sound_event(&mut self, event_key: String) -> bool {
        if !self.sound_event_keys.insert(event_key.clone()) {
            return false;
        }
        self.sound_event_order.push_back(event_key);
        while self.sound_event_order.len() > COMPLETION_SOUND_EVENT_CACHE_LIMIT {
            if let Some(stale) = self.sound_event_order.pop_front() {
                self.sound_event_keys.remove(&stale);
            }
        }
        true
    }
}

/// `acknowledgePresentationSessionAttention`: acknowledge now, or once the attention has been
/// visible for the minimum time.
pub(crate) fn acknowledge(
    tracker: &mut AttentionTracker,
    store: &mut PresentationStore,
    key: &SessionKey,
    now_ms: u64,
    output: &mut Output,
) {
    let Some(session) = effective(store, key) else {
        return;
    };
    if has_finished_thread(store, key, &session) {
        output.effects.push(Effect::AcknowledgeCoordinatorThreads {
            session: key.clone(),
        });
    }
    if session.activity != SessionActivity::Attention {
        return;
    }
    let entered = tracker.entered_at.get(key).copied();
    if let Some(entered) = entered {
        let remaining = MIN_ATTENTION_VISIBLE_MS.saturating_sub(now_ms.saturating_sub(entered));
        if remaining > 0 {
            if !tracker.armed.contains_key(key) {
                tracker.armed.insert(key.clone(), entered);
                output.effects.push(Effect::ArmAttentionAcknowledge {
                    session: key.clone(),
                    delay_ms: remaining,
                    entered_at_ms: entered,
                });
            }
            return;
        }
    }
    complete(tracker, store, key, entered, now_ms, output);
}

/// Whether the session is an orchestrator with a thread under it (at any depth) that may hold a
/// finished turn the user has not seen (`is_finished_unseen`). gxserver decides
/// which of them really finished (`/api/acknowledgeCoordinatorThreads`), since only it knows why a
/// session is in attention.
///
/// CDXC:Coordinators 2026-10-24 SEE-ALSO: the user's decision on `acknowledge_coordinator_threads` in server/src/coordinators/seen_threads.rs ("when i see the main thread itself then that should clear the blue dot on all the sub threads below it"); the phone asks the same through `ghostex orchestrator acknowledge-threads` (apps/mobile/app/src/terminal/attention.ts).
fn has_finished_thread(
    store: &PresentationStore,
    key: &SessionKey,
    session: &PresentationSession,
) -> bool {
    if session.coordinator_role.as_deref() != Some("coordinator") {
        return false;
    }
    let (Some(machine), Some(loaded)) = (store.machine(&key.machine), store.loaded(&key.machine))
    else {
        return false;
    };
    let mut parents: HashSet<(String, String)> =
        HashSet::from([(key.project_id.clone(), key.session_id.clone())]);
    let mut pending = vec![(key.project_id.clone(), key.session_id.clone())];
    while let Some(parent) = pending.pop() {
        for thread in loaded.server_sessions().filter(|thread| {
            thread.coordinator_project_id.as_deref() == Some(parent.0.as_str())
                && thread.coordinator_session_id.as_deref() == Some(parent.1.as_str())
        }) {
            let thread_key = (thread.project_id.clone(), thread.session_id.clone());
            if !parents.insert(thread_key.clone()) {
                continue;
            }
            let effective = machine.effective_session(&thread.project_id, &thread.session_id);
            if effective.is_some_and(|thread| is_finished_unseen(&thread)) {
                return true;
            }
            pending.push(thread_key);
        }
    }
    false
}

/// A thread that went into attention (judged before `observe` records the change) while its orchestrator (at any depth above it) is the session
/// on screen: the user is looking at the orchestrator, so its machine marks the thread's finished
/// turn seen at once, as a click on the orchestrator would.
///
/// CDXC:Coordinators 2026-10-24 SEE-ALSO: the user's decision on `acknowledge_coordinator_threads` in server/src/coordinators/seen_threads.rs; a click only reaches `acknowledge`, so without this a thread that finished while its orchestrator was already open kept its blue dot.
pub(crate) fn acknowledge_threads_of_focused_coordinator(
    tracker: &AttentionTracker,
    store: &PresentationStore,
    focused: Option<&SessionKey>,
    changes: &ChangeSummary,
    effects: &mut Vec<Effect>,
) {
    let Some(focused) = focused else {
        return;
    };
    let Some(machine) = store.machine(&focused.machine) else {
        return;
    };
    let entered = changes.sessions_changed.iter().any(|key| {
        key.machine == focused.machine
            && tracker.activity.get(key) != Some(&SessionActivity::Attention)
            && effective(store, key).is_some_and(|session| {
                is_finished_unseen(&session) && is_under(machine, &session, focused)
            })
    });
    if entered
        && !effects.iter().any(|effect| {
            matches!(effect, Effect::AcknowledgeCoordinatorThreads { session } if session == focused)
        })
    {
        effects.push(Effect::AcknowledgeCoordinatorThreads {
            session: focused.clone(),
        });
    }
}

/// A thread whose finished turn may be unseen: in attention, no question card, and not what
/// gxserver calls `waiting` (a permission prompt or a bell, which only the user's answer clears).
fn is_finished_unseen(thread: &PresentationSession) -> bool {
    thread.activity == SessionActivity::Attention
        && thread.pending_question_count == 0
        && !thread.awaiting_approval
        && thread.coordinator_thread_state.as_deref() != Some("waiting")
}

/// Whether `session` is a thread of `coordinator`, directly or through its threads.
fn is_under(
    machine: &crate::presentation_store::MachinePresentation,
    session: &PresentationSession,
    coordinator: &SessionKey,
) -> bool {
    let mut parent = (
        session.coordinator_project_id.clone(),
        session.coordinator_session_id.clone(),
    );
    // Bounded, so a cycle (two orchestrators that are each other's thread) ends.
    for _ in 0..16 {
        let (Some(project_id), Some(session_id)) = parent else {
            return false;
        };
        if project_id == coordinator.project_id && session_id == coordinator.session_id {
            return true;
        }
        let Some(next) = machine.effective_session(&project_id, &session_id) else {
            return false;
        };
        parent = (
            next.coordinator_project_id.clone(),
            next.coordinator_session_id.clone(),
        );
    }
    false
}

/// The armed timer fired. A timer whose attention was cleared or replaced since is stale.
pub(crate) fn acknowledge_due(
    tracker: &mut AttentionTracker,
    store: &mut PresentationStore,
    key: &SessionKey,
    entered_at_ms: u64,
    now_ms: u64,
    output: &mut Output,
) {
    if tracker.armed.get(key) != Some(&entered_at_ms) {
        return;
    }
    tracker.armed.remove(key);
    if tracker.entered_at.get(key) != Some(&entered_at_ms) {
        return;
    }
    complete(tracker, store, key, Some(entered_at_ms), now_ms, output);
}

/// `completePresentationSessionAttentionAcknowledgement`.
fn complete(
    tracker: &mut AttentionTracker,
    store: &mut PresentationStore,
    key: &SessionKey,
    entered: Option<u64>,
    now_ms: u64,
    output: &mut Output,
) {
    let Some(session) = effective(store, key) else {
        return;
    };
    if session.activity != SessionActivity::Attention {
        return;
    }
    if let (Some(entered), Some(latest)) = (entered, tracker.entered_at.get(key)) {
        if *latest != entered {
            return;
        }
    }
    clear_locally(tracker, store, key, now_ms, output);
    output.effects.push(Effect::ReportAgentActivity {
        session: key.clone(),
        report: AgentActivityReport::Acknowledge,
        agent_name: agent_name(&session),
    });
}

/// `handleGpuiWorkspaceTerminalEscapePressed`: always reported, and the attention (if any) is
/// cleared locally first.
pub(crate) fn terminal_escape(
    tracker: &mut AttentionTracker,
    store: &mut PresentationStore,
    key: &SessionKey,
    now_ms: u64,
    output: &mut Output,
) {
    tracker.sound_suppressed_until.insert(
        key.clone(),
        now_ms.saturating_add(ESCAPE_DONE_SUPPRESSION_MS),
    );
    let session = effective(store, key);
    if session
        .as_ref()
        .is_some_and(|session| session.activity == SessionActivity::Attention)
    {
        clear_locally(tracker, store, key, now_ms, output);
    }
    output.effects.push(Effect::ReportAgentActivity {
        session: key.clone(),
        report: AgentActivityReport::Escape,
        agent_name: session.as_ref().and_then(agent_name),
    });
}

/// `clearPresentationSessionAttentionLocally`: shows the session idle until the daemon answers,
/// keeping any other optimistic value the row already had.
fn clear_locally(
    tracker: &mut AttentionTracker,
    store: &mut PresentationStore,
    key: &SessionKey,
    now_ms: u64,
    output: &mut Output,
) {
    let expires_at_ms = now_ms.saturating_add(ATTENTION_PATCH_TTL_MS);
    let patch = match store
        .machine(&key.machine)
        .and_then(|machine| machine.session_patch(&key.project_id, &key.session_id))
    {
        Some(existing) => {
            let mut merged = existing.clone().with_activity(SessionActivity::Idle);
            merged.expires_at_ms = merged.expires_at_ms.max(expires_at_ms);
            merged
        }
        None => SessionPatch::activity(SessionActivity::Idle, expires_at_ms),
    };
    let changes = store.patch_session(key, patch);
    tracker.clear_tracking(key);
    if changes.ignored.is_none() {
        tracker.activity.insert(key.clone(), SessionActivity::Idle);
        output.changes.merge(changes);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use crate::{Core, Effect, Event, Intent, MachineId, SessionKey};

    /// One local project: the orchestrator `lead`, its thread `done` and that thread's own thread
    /// `deep`, each in the given activity.
    fn session(session_id: &str, activity: &str, parent: Option<&str>) -> Value {
        let mut session = json!({
            "sessionId": session_id,
            "projectId": "P1",
            "groupId": "P1:active",
            "kind": "agent",
            "surface": "workspace",
            "zmxName": format!("S90-P1-{session_id}"),
            "sortKey": format!("000000000000:0:1:{session_id}"),
            "visibleInSidebarByDefault": true,
            "alias": session_id,
            "activity": activity,
            "lifecycleState": "running",
            "createdAt": "2026-10-24T01:00:00.000Z",
            "updatedAt": "2026-10-24T01:00:00.000Z",
            "coordinatorRole": if parent.is_none() || session_id == "done" { "coordinator" } else { "thread" },
        });
        if let Some(parent) = parent {
            session["coordinatorProjectId"] = json!("P1");
            session["coordinatorSessionId"] = json!(parent);
        }
        session
    }

    fn core(done: &str, deep: &str) -> Core {
        let mut core = Core::new();
        core.handle_raw_frame(MachineId::Local, &core_snapshot(done, deep), 0)
            .expect("the frame parses");
        core
    }

    fn core_snapshot(done: &str, deep: &str) -> String {
        let snapshot = json!({
            "type": "presentationSnapshot",
            "protocolVersion": crate::protocol::GXSERVER_PROTOCOL_VERSION,
            "serverId": "local",
            "revision": 1,
            "snapshot": {
                "revision": 1,
                "generatedAt": "2026-10-24T01:00:00.000Z",
                "projects": [{
                    "projectId": "P1",
                    "title": "P1",
                    "path": "/tmp/P1",
                    "pathState": "available",
                    "groupIds": ["P1:active"],
                    "sortKey": "1:P1",
                    "createdAt": "2026-10-24T01:00:00.000Z",
                    "updatedAt": "2026-10-24T01:00:00.000Z",
                }],
                "groups": [{
                    "groupId": "P1:active",
                    "projectId": "P1",
                    "title": "Active",
                    "sessionIds": ["lead", "done", "deep"],
                    "sortKey": "1:P1:active",
                }],
                "sessions": [
                    session("lead", "idle", None),
                    session("done", done, Some("lead")),
                    session("deep", deep, Some("done")),
                ],
            }
        });
        snapshot.to_string()
    }

    fn change(core: &mut Core, revision: i64, session: Value) -> Vec<Effect> {
        let frame = json!({
            "type": "presentationDelta",
            "protocolVersion": crate::protocol::GXSERVER_PROTOCOL_VERSION,
            "serverId": "local",
            "revision": revision,
            "delta": { "type": "sessionPresentationChanged", "session": session },
        });
        core.handle_raw_frame(MachineId::Local, &frame.to_string(), 10_000)
            .expect("the frame parses")
            .effects
    }

    fn asks_lead(effects: &[Effect]) -> usize {
        effects
            .iter()
            .filter(|effect| {
                matches!(effect, Effect::AcknowledgeCoordinatorThreads { session }
                    if *session == SessionKey::local("P1", "lead"))
            })
            .count()
    }

    fn focus_lead(core: &mut Core) -> Vec<Effect> {
        core.handle(
            Event::Intent(Intent::FocusSession {
                session: SessionKey::local("P1", "lead"),
                visible: None,
            }),
            5_000,
        )
        .effects
    }

    fn acknowledge_lead(core: &mut Core) -> Vec<Effect> {
        core.handle(
            Event::Intent(Intent::AcknowledgeAttention {
                session: SessionKey::local("P1", "lead"),
            }),
            6_000,
        )
        .effects
    }

    /// A click on the orchestrator asks its machine to mark its threads seen, also when it is the
    /// session already selected and only a nested thread is in attention.
    #[test]
    fn clicking_an_already_selected_orchestrator_asks_for_its_finished_threads() {
        let mut core = core("idle", "attention");
        focus_lead(&mut core);
        assert_eq!(asks_lead(&acknowledge_lead(&mut core)), 1);
        assert_eq!(asks_lead(&acknowledge_lead(&mut core)), 1);
    }

    /// A thread that finishes while its orchestrator is the focused session is asked about at
    /// once, once; a thread with a question card, or any thread while another session is focused,
    /// is not.
    #[test]
    fn a_thread_finishing_under_the_focused_orchestrator_is_marked_seen_at_once() {
        let mut core = core("working", "idle");
        focus_lead(&mut core);
        assert_eq!(
            asks_lead(&change(
                &mut core,
                2,
                session("done", "attention", Some("lead"))
            )),
            1
        );
        // Already in attention: a later update of the same thread asks nothing more.
        let mut retitled = session("done", "attention", Some("lead"));
        retitled["alias"] = json!("renamed");
        assert_eq!(asks_lead(&change(&mut core, 3, retitled)), 0);
        let mut asking = session("deep", "attention", Some("done"));
        asking["pendingQuestionCount"] = json!(1);
        assert_eq!(asks_lead(&change(&mut core, 4, asking)), 0);

        let mut approval = session("done", "attention", Some("lead"));
        approval["coordinatorThreadState"] = json!("waiting");
        let mut waiting = self::core("working", "idle");
        focus_lead(&mut waiting);
        assert_eq!(asks_lead(&change(&mut waiting, 2, approval)), 0);

        let mut unfocused = self::core("working", "idle");
        assert_eq!(
            asks_lead(&change(
                &mut unfocused,
                2,
                session("done", "attention", Some("lead"))
            )),
            0
        );
    }

    /// A session on a remote machine (here the orchestrator, itself in attention with a finished
    /// thread under it) is acknowledged on that machine: the activity report and the request to
    /// mark its threads seen both carry the remote key, which the host sends down that machine's
    /// tunnel, and the row clears at once.
    #[test]
    fn acknowledging_a_remote_session_targets_its_own_machine() {
        let machine = MachineId::Remote("remote-mac".to_string());
        let mut snapshot: Value =
            serde_json::from_str(&core_snapshot("attention", "idle")).unwrap();
        snapshot["snapshot"]["sessions"][0]["activity"] = json!("attention");
        snapshot["snapshot"]["sessions"][0]["attention"] =
            json!({ "eventId": "attn_1", "acknowledged": false });
        let mut core = Core::new();
        core.handle_raw_frame(machine.clone(), &snapshot.to_string(), 0)
            .expect("the frame parses");
        let lead = SessionKey::remote("remote-mac", "P1", "lead");
        let output = core.handle(
            Event::Intent(Intent::AcknowledgeAttention {
                session: lead.clone(),
            }),
            6_000,
        );
        assert!(output.effects.iter().any(|effect| matches!(
            effect,
            Effect::ReportAgentActivity { session, report: crate::AgentActivityReport::Acknowledge, .. }
                if *session == lead
        )));
        assert!(output.effects.iter().any(|effect| matches!(
            effect,
            Effect::AcknowledgeCoordinatorThreads { session } if *session == lead
        )));
        let row = core
            .presentation()
            .machine(&machine)
            .and_then(|machine| machine.effective_session("P1", "lead"))
            .expect("the row is held");
        assert_ne!(
            row.activity,
            ghostex_gx_protocol::SessionActivity::Attention
        );
    }
}
