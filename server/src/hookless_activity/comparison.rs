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
/// Why a one-sided event is expected (the `reason` of a `recentExpected` entry).
const EXPECTED_GHOSTEX_CLOSE: &str = "ghostexClosedSession";
const EXPECTED_GHOSTEX_SLEEP: &str = "ghostexSleptSession";

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
    /// Compared with the hook but never driven: what Claude's hook reports that changes nothing
    /// (a background task's notice folded into a running turn, claude_turns.rs).
    pub(crate) compare_only: bool,
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
            compare_only: false,
        }
    }

    pub(crate) fn baseline(mut self) -> Self {
        self.baseline = true;
        self
    }

    pub(crate) fn compare_only(mut self) -> Self {
        self.compare_only = true;
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
    pub(super) fn activity(&self) -> Option<String> {
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
    /// One side only, for a reason the comparison knows is right (`EXPECTED_*`): not counted as
    /// a disagreement.
    expected: u64,
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
        self.expected += other.expected;
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
            "expected": self.expected,
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
    /// When Ghostex itself last closed or slept the session, and the `expected` reason it gives
    /// (session_close_log.rs, zmx/endpoint.rs).
    closed_by_ghostex: Option<(Instant, &'static str)>,
}

#[derive(Default)]
struct Store {
    started_at_ms: Option<i64>,
    sessions: HashMap<(String, String), SessionCompare>,
    disagreements: VecDeque<Value>,
    matches: VecDeque<Value>,
    expected: VecDeque<Value>,
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

/// CDXC:AgentHooks 2026-11-12 WHY:
/// A day of comparison found every observer-only Claude SessionEnd was a thread Ghostex closed
/// itself (`server.threadMarkedDone` through `killSession`, logged about 200 ms before the
/// observer saw the record go): the pane is killed, so Claude's SessionEnd hook never posts, while
/// the observer sees the process end. That is the observer being right where a hook cannot speak.
/// Rule: a close Ghostex makes is noted here; an observer-only SessionEnd (and the idle it brings)
/// within the match window of it counts as `expected`, not as a disagreement, and the hook side is
/// settled to idle as its SessionEnd would have, so the next SessionStart compares cleanly.
/// A sleep kills the pane the same way: the Windows PC's 11 observer-only SessionEnds of
/// 2026-11-12, first taken for a watcher that ended live sessions, were Auto Sleep's sweep (on its
/// 60 s grid, two sessions 350 ms apart in one sweep, each pane's wmx host started anew when the
/// session woke), so a sleep is noted too (`note_session_slept`, zmx/endpoint.rs).
pub(crate) fn note_session_closed(project_id: &str, session_id: &str) {
    note_session_ended_by_ghostex(project_id, session_id, EXPECTED_GHOSTEX_CLOSE);
}

/// A sleep Ghostex made (Auto Sleep, the user's Sleep, an account switch's cycle).
pub(crate) fn note_session_slept(project_id: &str, session_id: &str) {
    note_session_ended_by_ghostex(project_id, session_id, EXPECTED_GHOSTEX_SLEEP);
}

fn note_session_ended_by_ghostex(project_id: &str, session_id: &str, reason: &'static str) {
    let mut store = store();
    if let Some(compare) = store
        .sessions
        .get_mut(&(project_id.to_string(), session_id.to_string()))
    {
        compare.closed_by_ghostex = Some((Instant::now(), reason));
    }
}

/// Why a signal only one side reported is expected, if it is.
fn expected_reason(compare: &SessionCompare, signal: &Signal) -> Option<&'static str> {
    let closed_near = |closed: Instant| {
        let gap = if closed > signal.at {
            closed - signal.at
        } else {
            signal.at - closed
        };
        gap <= MATCH_WINDOW
    };
    let (closed, reason) = compare.closed_by_ghostex?;
    (signal.side == Side::Observer && signal.event_name == "SessionEnd" && closed_near(closed))
        .then_some(reason)
}

/// Settles every pending event the other side never matched within the window.
pub(crate) fn expire(logger: &GxserverLogger) {
    let now = Instant::now();
    let mut expired = Vec::new();
    let mut explained = Vec::new();
    {
        let mut store = store();
        let Store {
            sessions,
            disagreements,
            expected,
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
                if let Some(reason) = expected_reason(compare, &signal) {
                    compare
                        .kinds
                        .entry(signal.kind.clone())
                        .or_default()
                        .expected += 1;
                    if signal.kind == "event:SessionEnd" {
                        // What the hook's SessionEnd would have said.
                        compare.activity[Side::Hook.index()] = Some("idle".to_string());
                        compare.last_event[Side::Hook.index()] = Some("SessionEnd".to_string());
                    }
                    let entry = json!({
                        "agent": compare.agent,
                        "at": signal.wall_ms,
                        "event": signal.event_name,
                        "kind": signal.kind,
                        "only": signal.side.name(),
                        "projectId": project_id,
                        "reason": reason,
                        "sessionId": session_id,
                    });
                    push_recent(expected, entry.clone());
                    explained.push(entry);
                    continue;
                }
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
    for entry in explained {
        log(logger, "hooklessActivity.expected", entry);
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
        // One side only, for a known reason (`expected` in the counts): not disagreements.
        "recentExpected": store.expected.iter().rev().take(20).cloned().collect::<Vec<_>>(),
    })
}

/// Forgets everything recorded so far (`ghostex hookless-activity --reset`).
pub(crate) fn reset() {
    *store() = Store::default();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn logger() -> (tempfile::TempDir, GxserverLogger) {
        let temp = tempfile::tempdir().unwrap();
        let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
        (temp, GxserverLogger::new(paths))
    }

    fn event(session: &str, name: &str) -> ActivityEvent {
        ActivityEvent::new("Ptest", session, "claude", name, Some("c".to_string()))
    }

    fn stats(session: &str, kind: &str) -> KindStats {
        store().sessions[&("Ptest".to_string(), session.to_string())]
            .kinds
            .get(kind)
            .cloned()
            .unwrap_or_default()
    }

    /// Moves a session's pending signals and close back past the match window.
    fn age(session: &str) {
        let mut store = store();
        let compare = store
            .sessions
            .get_mut(&("Ptest".to_string(), session.to_string()))
            .unwrap();
        let back = |at: Instant| at.checked_sub(MATCH_WINDOW * 2).unwrap();
        for signal in &mut compare.pending {
            signal.at = back(signal.at);
        }
        compare.closed_by_ghostex = compare
            .closed_by_ghostex
            .map(|(at, reason)| (back(at), reason));
    }

    #[test]
    fn a_folded_task_notice_matches_the_hooks_prompt() {
        let (_temp, logger) = logger();
        let session = "Gfold1";
        record(
            &logger,
            Side::Hook,
            event(session, "UserPromptSubmit").baseline(),
        );
        record(
            &logger,
            Side::Observer,
            event(session, "UserPromptSubmit").baseline(),
        );
        record(&logger, Side::Hook, event(session, "UserPromptSubmit"));
        record(
            &logger,
            Side::Observer,
            event(session, "UserPromptSubmit").compare_only(),
        );
        assert_eq!(stats(session, "event:UserPromptSubmit").matched, 1);
        assert!(
            store().sessions[&("Ptest".to_string(), session.to_string())]
                .pending
                .is_empty()
        );
    }

    #[test]
    fn a_session_end_ghostex_caused_is_expected_not_a_disagreement() {
        let (_temp, logger) = logger();
        let session = "Gclose1";
        record(
            &logger,
            Side::Hook,
            event(session, "UserPromptSubmit").baseline(),
        );
        record(
            &logger,
            Side::Observer,
            event(session, "UserPromptSubmit").baseline(),
        );
        record(&logger, Side::Hook, event(session, "Stop"));
        record(&logger, Side::Observer, event(session, "Stop"));
        assert_eq!(stats(session, "activity:attention").matched, 1);
        note_session_closed("Ptest", session);
        record(&logger, Side::Observer, event(session, "SessionEnd"));
        age(session);
        expire(&logger);
        let end = stats(session, "event:SessionEnd");
        assert_eq!((end.expected, end.observer_only), (1, 0));
        let idle = stats(session, "activity:idle");
        assert_eq!((idle.expected, idle.observer_only), (1, 0));
        let store = store();
        let compare = &store.sessions[&("Ptest".to_string(), session.to_string())];
        // Settled as the hook's SessionEnd would have.
        assert_eq!(compare.activity[0].as_deref(), Some("idle"));
        assert!(store.expected.iter().any(
            |entry| entry["sessionId"] == session && entry["reason"] == EXPECTED_GHOSTEX_CLOSE
        ));
    }

    /// The Windows PC on 2026-11-12: Auto Sleep killed idle Claude panes, Claude's SessionEnd hook
    /// never posted, and the observer's SessionEnd was counted as a disagreement.
    #[test]
    fn a_session_end_from_a_ghostex_sleep_is_expected_not_a_disagreement() {
        let (_temp, logger) = logger();
        let session = "Gsleep1";
        record(
            &logger,
            Side::Hook,
            event(session, "UserPromptSubmit").baseline(),
        );
        record(
            &logger,
            Side::Observer,
            event(session, "UserPromptSubmit").baseline(),
        );
        record(&logger, Side::Hook, event(session, "Stop"));
        record(&logger, Side::Observer, event(session, "Stop"));
        note_session_slept("Ptest", session);
        record(&logger, Side::Observer, event(session, "SessionEnd"));
        age(session);
        expire(&logger);
        let end = stats(session, "event:SessionEnd");
        assert_eq!((end.expected, end.observer_only), (1, 0));
        let store = store();
        assert!(store.expected.iter().any(
            |entry| entry["sessionId"] == session && entry["reason"] == EXPECTED_GHOSTEX_SLEEP
        ));
        assert!(!store
            .disagreements
            .iter()
            .any(|entry| entry["sessionId"] == session));
    }

    #[test]
    fn a_session_end_with_no_ghostex_close_still_counts() {
        let (_temp, logger) = logger();
        let session = "Gclose2";
        record(
            &logger,
            Side::Hook,
            event(session, "UserPromptSubmit").baseline(),
        );
        record(
            &logger,
            Side::Observer,
            event(session, "UserPromptSubmit").baseline(),
        );
        record(&logger, Side::Hook, event(session, "Stop"));
        record(&logger, Side::Observer, event(session, "Stop"));
        record(&logger, Side::Observer, event(session, "SessionEnd"));
        age(session);
        expire(&logger);
        let end = stats(session, "event:SessionEnd");
        assert_eq!((end.expected, end.observer_only), (0, 1));
    }
}
