//! The worker that keeps each phone's Live Activity in line with this computer's agents, through the relay.
//!
//! CDXC:Notifications 2026-11-11 DECISION: User, on the Lock Screen banner that said "Not current": "yes sure
//! do it now". While the phone is locked only a push to the activity keeps it current, so gxserver builds the
//! banner's content itself (`content.rs`) and sends it through the relay that holds Apple's key:
//! - an update when what the banner shows changed, at most one per token every `min_interval` (the newest
//!   content wins; nothing goes out when it did not change), plus a keep-alive every `keep_alive` while an
//!   agent works or waits, so the stale date (`stale_after`) moves on and a computer that went to sleep leaves
//!   the banner saying when it last heard instead of "working" for hours;
//! - an end `end_after` (15 minutes) after the last agent stopped working or waiting; the activity's token is
//!   then forgotten, since that activity is over;
//! - a start, with the phone's push-to-start token, when it has no activity and an agent's working stint
//!   begins, at most one every `start_interval` (the relay allows 2, then one per 10 minutes).
//! A 429 waits the relay's `retryAfter`; a 410 forgets that token; other failures wait and try again with the
//! newest content. The worker reads the session list only while some phone has a Live Activity token here.
//! Tokens appear in no log: a token is named by the first 8 hex of its SHA-256 (`hash`; the log's sanitizer
//! blanks any field or event whose name says "token").

use std::{
    collections::{HashMap, HashSet},
    sync::{mpsc, Arc, Condvar, Mutex, MutexGuard, PoisonError},
    thread::JoinHandle,
    time::Duration,
};

use serde_json::{json, Value};

use crate::{
    domain::DomainRepository,
    logging::{GxserverLogInput, GxserverLogger, LogLevel},
    paths::GxserverPaths,
    push::{
        runtime::{PushClock, SystemClock},
        token_fingerprint, PushRegistrationStore,
    },
    storage::open_gxserver_database,
};

use super::{
    content::{collect_candidates, render, shape_of, Candidates, FinishedMemory},
    relay::{RelayClient, RelayOutcome},
    store::{
        forget_live_activity_token, token_label, LiveActivityPhone, LiveActivityStore, TokenKind,
    },
};

/// The timings; `Default` is what gxserver uses, tests shorten them.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Timings {
    pub(crate) min_interval_ms: i64,
    pub(crate) end_after_ms: i64,
    pub(crate) keep_alive_ms: i64,
    pub(crate) stale_after_ms: i64,
    pub(crate) start_interval_ms: i64,
    /// How long the worker waits after a change for more to arrive before it reads the sessions.
    pub(crate) settle_ms: u64,
    /// The longest the worker sleeps without a change (finished rows expire, the end comes due).
    pub(crate) tick_ms: u64,
    /// After a failure that may pass (no answer, a server error, the relay without its key).
    pub(crate) failure_backoff_ms: i64,
}

impl Default for Timings {
    fn default() -> Self {
        Self {
            min_interval_ms: 5_000,
            end_after_ms: 15 * 60_000,
            keep_alive_ms: 10 * 60_000,
            stale_after_ms: 15 * 60_000,
            start_interval_ms: 10 * 60_000,
            settle_ms: 500,
            tick_ms: 15_000,
            failure_backoff_ms: 60_000,
        }
    }
}

/// What the worker remembers of one phone (by computer id) between passes. In memory: a restart starts over,
/// which at worst sends the current content once more.
#[derive(Default)]
struct PhoneState {
    /// The activity token this state is for (its fingerprint); a new token starts over.
    activity_fingerprint: Option<String>,
    sent_shape: Option<String>,
    sent_at: i64,
    not_before: i64,
    idle_since: Option<i64>,
    known_stints: HashSet<String>,
    last_start: Option<i64>,
    /// This computer has put an agent on this activity. Until then it neither updates an idle banner nor
    /// ends it: the activity may show another paired computer's agents, or what the phone itself drew.
    showed_agents: bool,
    /// A working stint began while the phone had no activity: a start is owed (until it is sent, the agents
    /// stop, or the token is gone).
    start_wanted: bool,
}

#[derive(Default)]
struct Queue {
    poked: bool,
    stopping: bool,
    busy: bool,
    passes: u64,
}

struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
    idle: Condvar,
    paths: GxserverPaths,
    server_id: String,
    clock: Arc<dyn PushClock>,
    client: RelayClient,
    timings: Timings,
    logger: Arc<GxserverLogger>,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn log(&self, level: LogLevel, event: &str, details: Value) {
        let _ = self.logger.log(GxserverLogInput {
            level,
            event: event.to_string(),
            server_id: Some(self.server_id.clone()),
            request_id: None,
            client: None,
            duration_ms: None,
            error: None,
            details: Some(details),
        });
    }
}

pub(crate) struct LiveActivityRuntime {
    shared: Arc<Shared>,
    worker: Mutex<Option<(JoinHandle<()>, mpsc::Receiver<()>)>>,
}

impl LiveActivityRuntime {
    pub(crate) fn start(
        paths: GxserverPaths,
        server_id: String,
        logger: Arc<GxserverLogger>,
    ) -> Self {
        Self::start_with(
            paths,
            server_id,
            logger,
            Arc::new(SystemClock),
            RelayClient::new(super::relay::relay_url_from_env()),
            Timings::default(),
        )
    }

    pub(crate) fn start_with(
        paths: GxserverPaths,
        server_id: String,
        logger: Arc<GxserverLogger>,
        clock: Arc<dyn PushClock>,
        client: RelayClient,
        timings: Timings,
    ) -> Self {
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue::default()),
            wake: Condvar::new(),
            idle: Condvar::new(),
            paths,
            server_id,
            clock,
            client,
            timings,
            logger,
        });
        let (done_tx, done_rx) = mpsc::channel();
        let worker_shared = shared.clone();
        let worker = std::thread::Builder::new()
            .name("gxserver-live-activity".into())
            .spawn(move || {
                run(&worker_shared);
                let _ = done_tx.send(());
            })
            .ok()
            .map(|handle| (handle, done_rx));
        Self {
            shared,
            worker: Mutex::new(worker),
        }
    }

    /// Something a banner may show changed (a session's state, a registration): look again soon.
    pub(crate) fn poke(&self) {
        let mut queue = self.shared.lock();
        if queue.stopping {
            return;
        }
        queue.poked = true;
        self.shared.wake.notify_all();
    }

    /// Stops the worker; waits up to `wait` for the request in flight.
    pub(crate) fn stop(&self, wait: Duration) -> bool {
        {
            let mut queue = self.shared.lock();
            queue.stopping = true;
            self.shared.wake.notify_all();
        }
        let Some((handle, done)) = self
            .worker
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        else {
            return true;
        };
        match done.recv_timeout(wait) {
            Err(mpsc::RecvTimeoutError::Timeout) => false,
            _ => {
                let _ = handle.join();
                true
            }
        }
    }
}

fn run(shared: &Shared) {
    let mut phones: HashMap<String, PhoneState> = HashMap::new();
    let mut memory = FinishedMemory::new();
    let mut next_due: Option<i64> = None;
    loop {
        {
            let mut queue = shared.lock();
            loop {
                if queue.stopping {
                    return;
                }
                if queue.poked {
                    break;
                }
                let now = shared.clock.now_ms();
                if next_due.is_some_and(|due| due <= now) {
                    break;
                }
                let tick = shared.timings.tick_ms as i64;
                let wait = next_due.map_or(tick, |due| (due - now).clamp(1, tick));
                let (guard, timeout) = shared
                    .wake
                    .wait_timeout(queue, Duration::from_millis(wait as u64))
                    .unwrap_or_else(PoisonError::into_inner);
                queue = guard;
                if timeout.timed_out() {
                    break;
                }
            }
            if queue.poked {
                // Let a burst of changes (one turn ends, the next starts) arrive before reading.
                drop(queue);
                std::thread::sleep(Duration::from_millis(shared.timings.settle_ms));
                queue = shared.lock();
                if queue.stopping {
                    return;
                }
            }
            queue.poked = false;
            queue.busy = true;
        }
        next_due = pass(shared, &mut phones, &mut memory);
        let mut queue = shared.lock();
        queue.busy = false;
        queue.passes += 1;
        shared.idle.notify_all();
    }
}

/// The phones to look at: Live Activity tokens whose computer id also has a push registration, with that
/// registration's "Show session names" switch.
fn registered_phones(shared: &Shared) -> Option<Vec<(LiveActivityPhone, bool)>> {
    let phones = match LiveActivityStore::open(&shared.paths).read() {
        Ok(phones) => phones,
        Err(error) => {
            shared.log(
                LogLevel::Warn,
                "liveActivity.storeUnreadable",
                json!({ "error": error.to_string() }),
            );
            return None;
        }
    };
    if phones.is_empty() {
        return Some(Vec::new());
    }
    let push = match PushRegistrationStore::open(&shared.paths).read() {
        Ok(push) => push,
        Err(error) => {
            shared.log(
                LogLevel::Warn,
                "liveActivity.pushStoreUnreadable",
                json!({ "error": error.to_string() }),
            );
            return None;
        }
    };
    Some(
        phones
            .into_iter()
            .filter_map(|phone| {
                let names = push.registration(&phone.machine_id)?.session_names;
                Some((phone, names))
            })
            .collect(),
    )
}

/// The rows the phone's session list would hold now: running sessions of active projects, as mobile summary
/// rows, and the launchers that name custom agents.
fn read_rows(shared: &Shared, now: i64) -> Result<(Vec<Value>, Vec<Value>), String> {
    let db = open_gxserver_database(&shared.paths).map_err(|error| error.to_string())?;
    let repository = DomainRepository::new(&db, shared.server_id.as_str());
    let projects = repository.list_projects().map_err(|error| error.message)?;
    let sessions = repository
        .list_sessions_excluding_stopped(None)
        .map_err(|error| error.message)?;
    let launchers = crate::sidebar_hud::sidebar_agent_buttons_from_projects(&projects)
        .as_array()
        .cloned()
        .unwrap_or_default();
    let generated_at = chrono::DateTime::from_timestamp_millis(now)
        .unwrap_or_default()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let rows = sessions
        .iter()
        .filter_map(|session| {
            let project_id = session.get("projectId")?;
            let project = projects
                .iter()
                .find(|project| project.get("projectId") == Some(project_id))?;
            // The phone's list leaves out parked and hidden projects (ghostex_cli inventory.rs,
            // is_active_gxserver_inventory_project).
            if project.get("isRecentProject") == Some(&Value::Bool(true))
                || project.get("visibility").and_then(Value::as_str) == Some("hidden")
                || project.get("systemKind").and_then(Value::as_str) == Some("remoteAttachCarrier")
            {
                return None;
            }
            let presentation = crate::presentation::project_presentation_session(
                project,
                "group",
                session,
                &generated_at,
            );
            let row = crate::ghostex_cli::sessions::mobile_summary_for_session(
                session,
                project,
                &presentation,
            );
            // The phone keeps only zmx sessions.
            (row.get("provider").and_then(Value::as_str) == Some("zmx")).then_some(row)
        })
        .collect();
    Ok((rows, launchers))
}

/// One pass over every phone; returns when the next pass is due without a change.
fn pass(
    shared: &Shared,
    states: &mut HashMap<String, PhoneState>,
    memory: &mut FinishedMemory,
) -> Option<i64> {
    let phones = registered_phones(shared)?;
    states.retain(|machine_id, _| {
        phones
            .iter()
            .any(|(phone, _)| &phone.machine_id == machine_id)
    });
    if phones.is_empty() {
        memory.clear();
        return None;
    }
    let now = shared.clock.now_ms();
    let (rows, launchers) = match read_rows(shared, now) {
        Ok(read) => read,
        Err(error) => {
            shared.log(
                LogLevel::Warn,
                "liveActivity.sessionsUnreadable",
                json!({ "error": error }),
            );
            return Some(now + shared.timings.failure_backoff_ms);
        }
    };
    let candidates = collect_candidates(&rows, &launchers, now, memory);
    let mut next_due: Option<i64> = None;
    let mut due = |at: i64| next_due = Some(next_due.map_or(at, |current: i64| current.min(at)));
    for (phone, names) in phones {
        let state = states.entry(phone.machine_id.clone()).or_default();
        if let Some(at) = phone_pass(shared, &phone, names, state, &candidates, now) {
            due(at);
        }
    }
    // Finished rows leave the banner 2 minutes after their finish.
    if let Some(expiry) = candidates
        .list
        .iter()
        .filter_map(|candidate| candidate.finished_at_ms)
        .map(|finished| finished + super::content::FINISHED_ROW_MS)
        .min()
    {
        due(expiry);
    }
    next_due
}

fn phone_pass(
    shared: &Shared,
    phone: &LiveActivityPhone,
    names: bool,
    state: &mut PhoneState,
    candidates: &Candidates,
    now: i64,
) -> Option<i64> {
    let timings = shared.timings;
    let stint_began = candidates
        .working_stints
        .iter()
        .any(|stint| !state.known_stints.contains(stint));
    state.known_stints = candidates.working_stints.clone();
    let content = render(candidates, &phone.machine_id, names, now);
    let shape = shape_of(&content);
    let active = candidates.active();

    let Some(token) = phone.activity_token.as_deref() else {
        state.activity_fingerprint = None;
        state.sent_shape = None;
        state.idle_since = None;
        // No activity: a start when a working stint begins, if the phone gave a push-to-start token.
        let start_token = phone.push_to_start_token.as_deref()?;
        if !active {
            state.start_wanted = false;
            return None;
        }
        state.start_wanted |= stint_began;
        if !state.start_wanted {
            return None;
        }
        let allowed_at = state
            .last_start
            .map_or(0, |last| last + timings.start_interval_ms)
            .max(state.not_before);
        if now < allowed_at {
            return Some(allowed_at);
        }
        let body = request(
            start_token,
            phone,
            "start",
            now,
            Some(&content),
            Some(now + timings.stale_after_ms),
        );
        let outcome = shared.client.send(&body);
        if outcome == RelayOutcome::Sent {
            state.last_start = Some(now);
            state.start_wanted = false;
        }
        settle(
            shared,
            phone,
            TokenKind::PushToStart,
            start_token,
            "start",
            state,
            now,
            outcome,
        );
        return state.start_wanted.then_some(state.not_before.max(now + 1));
    };

    let fingerprint = token_fingerprint(token);
    if state.activity_fingerprint.as_deref() != Some(fingerprint.as_str()) {
        // An activity this computer started by push shows its agents from the start.
        let started_here = state
            .last_start
            .is_some_and(|last| now - last < timings.start_interval_ms);
        *state = PhoneState {
            activity_fingerprint: Some(fingerprint),
            known_stints: state.known_stints.clone(),
            last_start: state.last_start,
            showed_agents: started_here,
            ..PhoneState::default()
        };
    }
    if active {
        state.idle_since = None;
    } else if !state.showed_agents {
        return None;
    } else {
        let since = *state.idle_since.get_or_insert(now);
        if now - since >= timings.end_after_ms {
            if now < state.not_before {
                return Some(state.not_before);
            }
            let body = request(token, phone, "end", now, Some(&content), None);
            let mut body = body;
            body["dismissalDate"] = json!(now / 1000);
            settle(
                shared,
                phone,
                TokenKind::Activity,
                token,
                "end",
                state,
                now,
                shared.client.send(&body),
            );
            return Some(state.not_before.max(now + timings.tick_ms as i64));
        }
    }
    let changed = state.sent_shape.as_deref() != Some(shape.as_str());
    let keep_alive = active && now - state.sent_at >= timings.keep_alive_ms;
    let end_due = state.idle_since.map(|since| since + timings.end_after_ms);
    if !changed && !keep_alive {
        let alive_due = active.then(|| state.sent_at + timings.keep_alive_ms);
        return [end_due, alive_due].into_iter().flatten().min();
    }
    let ready_at = (state.sent_at + timings.min_interval_ms).max(state.not_before);
    if now < ready_at {
        return Some(ready_at);
    }
    let stale = active.then_some(now + timings.stale_after_ms);
    let body = request(token, phone, "update", now, Some(&content), stale);
    let outcome = shared.client.send(&body);
    if outcome == RelayOutcome::Sent {
        state.sent_shape = Some(shape);
        // From the answer, not the decision: a slow request must not bring the next one closer.
        state.sent_at = shared.clock.now_ms();
        state.showed_agents |= active;
    }
    settle(
        shared,
        phone,
        TokenKind::Activity,
        token,
        "update",
        state,
        now,
        outcome,
    );
    let next = (state.sent_at + timings.min_interval_ms).max(state.not_before);
    [Some(next.max(now + 1)), end_due]
        .into_iter()
        .flatten()
        .min()
}

fn request(
    token: &str,
    phone: &LiveActivityPhone,
    event: &str,
    now: i64,
    content: Option<&Value>,
    stale_date_ms: Option<i64>,
) -> Value {
    let mut body = json!({
        "token": token,
        "event": event,
        "environment": phone.environment.wire(),
        "timestamp": now / 1000,
    });
    if let Some(content) = content {
        body["contentState"] = content.clone();
    }
    if let Some(stale) = stale_date_ms {
        body["staleDate"] = json!(stale / 1000);
    }
    body
}

/// Applies the relay's answer: the wait it asks for, the token it says is gone, the log line.
#[allow(clippy::too_many_arguments)]
fn settle(
    shared: &Shared,
    phone: &LiveActivityPhone,
    kind: TokenKind,
    token: &str,
    event: &str,
    state: &mut PhoneState,
    now: i64,
    outcome: RelayOutcome,
) {
    let label = token_label(token);
    let details = |extra: Value| {
        let mut details = json!({ "event": event, "machineId": phone.machine_id, "hash": label });
        if let (Some(map), Some(extra)) = (details.as_object_mut(), extra.as_object()) {
            map.extend(extra.clone());
        }
        details
    };
    match outcome {
        RelayOutcome::Sent => {
            if event != "update" {
                shared.log(LogLevel::Info, "liveActivity.sent", details(json!({})));
            }
            if event == "end" {
                // That activity is over; the phone sends the next one's token when it starts one.
                forget(shared, phone, kind, token, "ended");
            }
        }
        RelayOutcome::RetryAfter(wait) => {
            state.not_before = now + wait.as_millis() as i64;
            shared.log(
                LogLevel::Warn,
                "liveActivity.rateLimited",
                details(json!({ "retryAfterSeconds": wait.as_secs() })),
            );
        }
        RelayOutcome::TokenGone => {
            shared.log(
                LogLevel::Warn,
                "liveActivity.appleDropped",
                details(json!({})),
            );
            forget(shared, phone, kind, token, "gone");
            *state = PhoneState {
                known_stints: state.known_stints.clone(),
                last_start: state.last_start,
                ..PhoneState::default()
            };
        }
        RelayOutcome::NotConfigured => {
            state.not_before = now + shared.timings.failure_backoff_ms;
            shared.log(
                LogLevel::Warn,
                "liveActivity.relayNotConfigured",
                details(json!({})),
            );
        }
        RelayOutcome::Failed { code, retry } => {
            state.not_before = now + shared.timings.failure_backoff_ms;
            if !retry {
                // The relay refused this content; wait for the next change rather than resend it.
                if event == "update" {
                    state.sent_at = now;
                }
                if event == "start" {
                    state.start_wanted = false;
                }
            }
            shared.log(
                LogLevel::Warn,
                "liveActivity.failed",
                details(json!({ "code": code, "willRetry": retry })),
            );
        }
    }
}

fn forget(shared: &Shared, phone: &LiveActivityPhone, kind: TokenKind, token: &str, why: &str) {
    if let Err(error) = forget_live_activity_token(
        &shared.paths,
        &phone.machine_id,
        kind,
        &token_fingerprint(token),
    ) {
        shared.log(
            LogLevel::Warn,
            "liveActivity.forgetFailed",
            json!({ "machineId": phone.machine_id, "why": why, "error": error.to_string() }),
        );
    }
}
