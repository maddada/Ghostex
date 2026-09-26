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
