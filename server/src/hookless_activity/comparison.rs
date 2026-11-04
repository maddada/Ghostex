//! The shadow comparison: every activity event a hook-free observer reports is paired with the
//! hook event for the same session, and what only one side saw is counted and logged.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::logging::{DiagnosticLogScenario, GxserverLogInput, GxserverLogger, LogLevel};

/// How far apart the two sides may see the same change and still count as one.
const MATCH_WINDOW: Duration = Duration::from_secs(10);
const RECENT_LIMIT: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Side {
    Hook,
    Observer,
}

impl Side {
    fn index(self) -> usize {
        match self {
            Side::Hook => 0,
            Side::Observer => 1,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Side::Hook => "hook",
            Side::Observer => "observer",
        }
    }
}

/// One activity event in the shape the hooks post to `/api/ingestAgentHookEvent`.
#[derive(Clone, Debug)]
pub(crate) struct ActivityEvent {
    pub(crate) project_id: String,
    pub(crate) session_id: String,
    /// `claude`, `codex` or `opencode`.
    pub(crate) agent: &'static str,
    pub(crate) event_name: String,
    pub(crate) agent_session_id: Option<String>,
    /// Extra hook params the activity mapping reads (`trigger`, `notificationKind`, ...).
    pub(crate) payload: Map<String, Value>,
    /// False for an observer event no hook has a name for: only its activity is compared.
    pub(crate) event_signal: bool,
    /// The state an observer found when it first looked (gxserver started, the comparison turned
    /// on): it sets that side's activity and is not compared.
    pub(crate) baseline: bool,
    /// When Ghostex learned of it.
    pub(crate) at: Instant,
    pub(crate) wall_ms: i64,
    /// The conversation's transcript on disk, when an observer knows it (Claude's).
    pub(crate) transcript: Option<PathBuf>,
}

impl ActivityEvent {
    pub(crate) fn new(
        project_id: &str,
        session_id: &str,
        agent: &'static str,
        event_name: &str,
        agent_session_id: Option<String>,
    ) -> Self {
        Self {
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
            agent,
            event_name: event_name.to_string(),
            agent_session_id,
            payload: Map::new(),
            event_signal: true,
            baseline: false,
            at: Instant::now(),
            wall_ms: chrono::Utc::now().timestamp_millis(),
            transcript: None,
        }
    }

    pub(crate) fn baseline(mut self) -> Self {
        self.baseline = true;
        self
    }

    pub(crate) fn activity_only(mut self) -> Self {
        self.event_signal = false;
        self
    }

    pub(crate) fn with(mut self, key: &str, value: Value) -> Self {
        self.payload.insert(key.to_string(), value);
        self
    }

    /// What kind of notification, wait, start or stream event it was, for the disagreement list.
    fn detail(&self) -> Option<String> {
        [
            "notificationType",
            "notificationKind",
            "waitingFor",
            "source",
            "trigger",
            "streamEvent",
            "reason",
            "toolName",
        ]
        .iter()
        .find_map(|key| self.payload.get(*key).and_then(Value::as_str))
        .map(str::to_string)
    }

    /// The activity the hook pipeline derives from this event (working, idle, attention).
    fn activity(&self) -> Option<String> {
        let mut params = self.payload.clone();
        // The notify hook posts the status its own mapping derives; an observer event has none.
        if !params.contains_key("status") {
            if let Some(status) = crate::agent_hooks::event_mapping::activity_for_hook_event(
                self.agent,
                &self.event_name,
                &Value::Object(self.payload.clone()),
            ) {
                params.insert("status".to_string(), json!(status));
            }
        }
        params.insert("agentName".to_string(), json!(self.agent));
        params.insert("eventName".to_string(), json!(self.event_name));
        crate::agents::normalize_agent_hook_event_activity(&params)
    }
}

/// Hook events that only repeat what another event already said (Claude's Notification a few
/// seconds into an approval or question, the SessionStart a compaction fires after its
/// PostCompact, and the Stop OpenCode's plugin reports when its TUI opens an idle conversation):
/// compared only when they change the activity.
fn follow_up_event(event: &ActivityEvent) -> bool {
    match event.agent {
        "claude" => {
            event.event_name == "Notification"
                || (event.event_name == "SessionStart"
                    && event.payload.get("source").and_then(Value::as_str) == Some("compact"))
        }
        "opencode" => event.event_name == "Stop",
        _ => false,
    }
}

/// Events a side re-sends while nothing changed: OpenCode's TUI plugin reports SessionBusy again
/// whenever the running conversation's title or update time changes.
fn repeating_event(agent: &str, event_name: &str) -> bool {
    agent == "opencode" && event_name == "SessionBusy"
}

/// The hook event names each observer can report; a hook event outside its list is counted as
/// not observable instead of as a disagreement.
pub(crate) fn observable_events(agent: &str) -> &'static [&'static str] {
    match agent {
        "claude" => super::claude::OBSERVABLE_EVENTS,
        "codex" => super::codex::OBSERVABLE_EVENTS,
        "opencode" => super::opencode::OBSERVABLE_EVENTS,
        _ => &[],
    }
}

#[derive(Clone, Debug)]
struct Signal {
    side: Side,
    kind: String,
    at: Instant,
    wall_ms: i64,
    event_name: String,
    detail: Option<String>,
    /// A side's first known activity: it can settle the other side's change, and is dropped
    /// unmatched without counting.
    settle_only: bool,
}

#[derive(Default, Clone, Debug)]
struct KindStats {
    matched: u64,
    hook_only: u64,
    observer_only: u64,
    not_observable: u64,
    gap_sum_ms: i64,
    gap_min_ms: Option<i64>,
    gap_max_ms: Option<i64>,
}

impl KindStats {
    fn add_gap(&mut self, gap: i64) {
        self.matched += 1;
        self.gap_sum_ms += gap;
        self.gap_min_ms = Some(self.gap_min_ms.map_or(gap, |min| min.min(gap)));
        self.gap_max_ms = Some(self.gap_max_ms.map_or(gap, |max| max.max(gap)));
    }

    fn merge(&mut self, other: &KindStats) {
        self.matched += other.matched;
        self.hook_only += other.hook_only;
        self.observer_only += other.observer_only;
        self.not_observable += other.not_observable;
        self.gap_sum_ms += other.gap_sum_ms;
        for gap in [other.gap_min_ms, other.gap_max_ms].into_iter().flatten() {
            self.gap_min_ms = Some(self.gap_min_ms.map_or(gap, |min| min.min(gap)));
            self.gap_max_ms = Some(self.gap_max_ms.map_or(gap, |max| max.max(gap)));
        }
    }

    fn json(&self) -> Value {
        json!({
            "matched": self.matched,
            "hookOnly": self.hook_only,
            "observerOnly": self.observer_only,
            "notObservable": self.not_observable,
            "gapMs": {
                "avg": (self.matched > 0).then(|| self.gap_sum_ms / self.matched as i64),
                "min": self.gap_min_ms,
                "max": self.gap_max_ms,
            },
        })
    }
}

#[derive(Default)]
struct SessionCompare {
    agent: &'static str,
    last_event: [Option<String>; 2],
    activity: [Option<String>; 2],
    agent_session_id: [Option<String>; 2],
    conversation_mismatches: u64,
    /// Since when the two sides have named different conversations, and whether it was reported.
    conversation_split: Option<(Instant, bool)>,
    pending: Vec<Signal>,
    kinds: BTreeMap<String, KindStats>,
    events: [u64; 2],
}

#[derive(Default)]
struct Store {
    started_at_ms: Option<i64>,
    sessions: HashMap<(String, String), SessionCompare>,
    disagreements: VecDeque<Value>,
    matches: VecDeque<Value>,
}

static STORE: LazyLock<Mutex<Store>> = LazyLock::new(Default::default);

fn store() -> MutexGuard<'static, Store> {
    STORE.lock().unwrap_or_else(|error| error.into_inner())
}

fn push_recent(list: &mut VecDeque<Value>, entry: Value) {
    if list.len() >= RECENT_LIMIT {
        list.pop_front();
    }
    list.push_back(entry);
}

fn log(logger: &GxserverLogger, event: &str, details: Value) {
    let _ = logger.log_routine(
        DiagnosticLogScenario::HooklessActivityComparison,
        GxserverLogInput {
            level: LogLevel::Debug,
            event: event.to_string(),
            server_id: None,
            request_id: None,
            client: None,
            duration_ms: None,
            error: None,
            details: Some(details),
        },
    );
}

/// Records one event from `side` and pairs it with the other side's matching event.
pub(crate) fn record(logger: &GxserverLogger, side: Side, event: ActivityEvent) {
    let activity = event.activity();
    let mut store = store();
    store
        .started_at_ms
        .get_or_insert_with(|| chrono::Utc::now().timestamp_millis());
    let key = (event.project_id.clone(), event.session_id.clone());
    let compare = store.sessions.entry(key).or_default();
    compare.agent = event.agent;
    let index = side.index();
    compare.events[index] += 1;
    if let Some(id) = event.agent_session_id.clone() {
        compare.agent_session_id[index] = Some(id);
    }
    match &compare.agent_session_id {
        [Some(hook), Some(observer)] if hook != observer => {
            compare.conversation_split.get_or_insert((event.at, false));
        }
        _ => compare.conversation_split = None,
    }
    if event.baseline {
        if let Some(activity) = activity {
            compare.activity[index] = Some(activity);
        }
        compare.last_event[index] = Some(event.event_name.clone());
        return;
    }
    // (kind, whether it may wait for the other side)
    let mut signals: Vec<(String, bool)> = Vec::new();
    let repeat = repeating_event(event.agent, &event.event_name)
        && compare.last_event[index].as_deref() == Some(event.event_name.as_str());
    if event.event_signal && !repeat {
        signals.push((format!("event:{}", event.event_name), true));
    }
    if event.event_signal {
        compare.last_event[index] = Some(event.event_name.clone());
    }
    if let Some(activity) = activity.filter(|a| compare.activity[index].as_deref() != Some(a)) {
        // A side's first known activity (after a gxserver restart, say) is where it stands, not
        // a change: it settles a change the other side already reported, and is never pending.
        let first = compare.activity[index].replace(activity.clone()).is_none();
        signals.push((format!("activity:{activity}"), !first));
    }
    let observable = if side == Side::Hook && follow_up_event(&event) {
        signals
            .iter()
            .any(|(kind, wait)| *wait && kind.starts_with("activity:"))
    } else {
        observable_events(event.agent).contains(&event.event_name.as_str())
    };
    let mut matched = Vec::new();
    for (kind, wait) in signals {
        // Two first-known activities settle nothing between them.
        let wait_ok = |pending: &Signal| wait || !pending.settle_only;
        if side == Side::Hook && kind.starts_with("event:") && !observable {
            compare.kinds.entry(kind).or_default().not_observable += 1;
            continue;
        }
        let other = compare.pending.iter().position(|pending| {
            pending.side != side
                && pending.kind == kind
                && wait_ok(pending)
                && event.at.saturating_duration_since(pending.at) <= MATCH_WINDOW
        });
        match other {
            Some(position) => {
                let pending = compare.pending.remove(position);
                // Positive when the observer saw it after the hook.
                let gap = match side {
                    Side::Observer => event.wall_ms - pending.wall_ms,
                    Side::Hook => pending.wall_ms - event.wall_ms,
                };
                compare.kinds.entry(kind.clone()).or_default().add_gap(gap);
                matched.push(json!({
                    "agent": event.agent,
                    "at": event.wall_ms,
                    "gapMs": gap,
                    "kind": kind,
                    "projectId": event.project_id,
                    "sessionId": event.session_id,
                }));
            }
            None => compare.pending.push(Signal {
                settle_only: !wait,
                side,
                kind,
                at: event.at,
                wall_ms: event.wall_ms,
                event_name: event.event_name.clone(),
                detail: event.detail(),
            }),
        }
    }
    drop(store);
    store_matches(logger, matched);
}

fn store_matches(logger: &GxserverLogger, matched: Vec<Value>) {
    if matched.is_empty() {
        return;
    }
    {
        let mut store = store();
        for entry in &matched {
            push_recent(&mut store.matches, entry.clone());
        }
    }
    for entry in matched {
        log(logger, "hooklessActivity.match", entry);
    }
}

/// Settles every pending event the other side never matched within the window.
pub(crate) fn expire(logger: &GxserverLogger) {
    let now = Instant::now();
    let mut expired = Vec::new();
    {
        let mut store = store();
        let Store {
            sessions,
            disagreements,
            ..
        } = &mut *store;
        for ((project_id, session_id), compare) in sessions.iter_mut() {
            // Each side moves to a new conversation at its own moment; only a lasting split counts.
            if let Some((since, reported)) = compare.conversation_split.as_mut() {
                if !*reported && now.saturating_duration_since(*since) > MATCH_WINDOW {
                    *reported = true;
                    compare.conversation_mismatches += 1;
                    let [hook, observer] = &compare.agent_session_id;
                    let entry = json!({
                        "agent": compare.agent,
                        "at": chrono::Utc::now().timestamp_millis(),
                        "kind": "conversationId",
                        "projectId": project_id,
                        "sessionId": session_id,
                        "hookConversation": crate::server::hash_log_identity(hook.as_deref()),
                        "observerConversation": crate::server::hash_log_identity(observer.as_deref()),
                    });
                    push_recent(disagreements, entry.clone());
                    expired.push(entry);
                }
            }
            let (old, keep): (Vec<_>, Vec<_>) = compare
                .pending
                .drain(..)
                .partition(|signal| now.saturating_duration_since(signal.at) > MATCH_WINDOW);
            compare.pending = keep;
            for signal in old.into_iter().filter(|signal| !signal.settle_only) {
                let stats = compare.kinds.entry(signal.kind.clone()).or_default();
                match signal.side {
                    Side::Hook => stats.hook_only += 1,
                    Side::Observer => stats.observer_only += 1,
                }
                let entry = json!({
                    "agent": compare.agent,
                    "at": signal.wall_ms,
                    "event": signal.event_name,
                    "detail": signal.detail,
                    "hookActivity": compare.activity[0],
                    "kind": signal.kind,
                    "observerActivity": compare.activity[1],
                    "only": signal.side.name(),
                    "projectId": project_id,
                    "sessionId": session_id,
                });
                push_recent(disagreements, entry.clone());
                expired.push(entry);
            }
        }
    }
    for entry in expired {
        log(logger, "hooklessActivity.disagreement", entry);
    }
}

/// The comparison so far, per agent and per session, with the latest disagreements and matches.
pub(crate) fn summary(enabled: bool) -> Value {
    let store = store();
    let mut agents: BTreeMap<&str, BTreeMap<String, KindStats>> = BTreeMap::new();
    let mut sessions = Vec::new();
    for ((project_id, session_id), compare) in &store.sessions {
        let totals = agents.entry(compare.agent).or_default();
        for (kind, stats) in &compare.kinds {
            totals.entry(kind.clone()).or_default().merge(stats);
        }
        sessions.push(json!({
            "agent": compare.agent,
            "projectId": project_id,
            "sessionId": session_id,
            "hookEvents": compare.events[0],
            "observerEvents": compare.events[1],
            "hookActivity": compare.activity[0],
            "observerActivity": compare.activity[1],
            "hookLastEvent": compare.last_event[0],
            "observerLastEvent": compare.last_event[1],
            "conversationIdsAgree": match &compare.agent_session_id {
                [Some(hook), Some(observer)] => Some(hook == observer),
                _ => None,
            },
            "conversationMismatches": compare.conversation_mismatches,
            "pending": compare.pending.len(),
            "kinds": compare.kinds.iter().map(|(kind, stats)| (kind.clone(), stats.json())).collect::<Map<_, _>>(),
        }));
    }
    json!({
        "enabled": enabled,
        "matchWindowMs": MATCH_WINDOW.as_millis() as u64,
        "startedAt": store.started_at_ms,
        "agents": agents.iter().map(|(agent, kinds)| {
            (agent.to_string(), Value::Object(kinds.iter().map(|(kind, stats)| (kind.clone(), stats.json())).collect()))
        }).collect::<Map<_, _>>(),
        "sessions": sessions,
        "recentDisagreements": store.disagreements.iter().rev().take(60).cloned().collect::<Vec<_>>(),
        "recentMatches": store.matches.iter().rev().take(40).cloned().collect::<Vec<_>>(),
    })
}

/// Forgets everything recorded so far (`ghostex hookless-activity --reset`).
pub(crate) fn reset() {
    *store() = Store::default();
}
