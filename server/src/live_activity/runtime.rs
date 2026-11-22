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
        token_fingerprint,
    },
    storage::open_gxserver_database,
};

use super::{
    content::{collect_candidates, render, shape_of, Candidates, FinishedMemory, Source},
    relay::{RelayClient, RelayOutcome},
    store::{
        forget_live_activity_token, token_label, LiveActivityPhone, LiveActivityStore, TokenKind,
    },
};

/// Whether a computer may start a Live Activity by push (with the phone's push-to-start token).
///
/// CDXC:Notifications 2026-11-11 FIX: Off. An independent review found that a banner started by push never gets
/// an update token the phone could hand back (the app learns it only while it runs, and a background run has no
/// link to register it), so it said "working" until it went stale, and every next working stint started another,
/// each with an alert. Until the phone can register the token of an activity it did not start, a computer only
/// updates and ends an activity the app itself started. The phone stops sending the push-to-start token too.
const SEND_PUSH_TO_START: bool = false;

/// A gap this much longer than a tick between two wakes of the worker means the computer slept.
const SLEEP_GAP_MS: i64 = 5 * 60_000;

/// A connected computer's read gives up after this long, so one that went away delays the banner little.
const REMOTE_READ_TIMEOUT: Duration = Duration::from_secs(5);

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
    /// How often a connected computer's session list is read while some phone has a banner.
    pub(crate) remote_poll_ms: i64,
    /// How long a connected computer that stopped answering keeps its last rows on the banner.
    pub(crate) remote_keep_ms: i64,
    /// A phone's registration counts this long: the phone registers again well within it while it keeps this
    /// computer as its banner computer, so a computer the phone moved away from while it could not reach it
    /// stops pushing on its own.
    pub(crate) confirmation_ttl_ms: i64,
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
            remote_poll_ms: 10_000,
            remote_keep_ms: 2 * 60_000,
            confirmation_ttl_ms: 60 * 60_000,
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
    /// A working stint began while the phone had no activity: a start is owed (until it is sent, the agents
    /// stop, or the token is gone).
    start_wanted: bool,
    /// This computer sent this activity a working or waiting agent since it got the token. Until then it sends
    /// no idle content and never ends the activity, so it cannot blank or end a banner it is not showing.
    pushed_active: bool,
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
    /// Reads of connected computers that finished on their own threads, for the next pass.
    remote_reads: Mutex<Vec<RemoteRead>>,
    connected: ConnectedComputers,
    /// The phones (by computer id) that registered with this gxserver since it started or last slept: only
    /// they get pushes. A computer that restarted or slept may no longer be the phone's banner computer, so it
    /// waits for the phone to confirm it (`registration.rs`) instead of pushing over the one picked meanwhile.
    /// When each phone last confirmed (ms); a confirmation older than `confirmation_ttl_ms` no longer counts.
    confirmed: Mutex<HashMap<String, i64>>,
}

/// The computers connected to this one: the registry in gxserver; a test passes its own.
pub(crate) type ConnectedComputers =
    Arc<dyn Fn() -> Vec<crate::connected_computers::ConnectedComputer> + Send + Sync>;

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
            Arc::new(crate::connected_computers::list),
        )
    }

    pub(crate) fn start_with(
        paths: GxserverPaths,
        server_id: String,
        logger: Arc<GxserverLogger>,
        clock: Arc<dyn PushClock>,
        client: RelayClient,
        timings: Timings,
        connected: ConnectedComputers,
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
            remote_reads: Mutex::new(Vec::new()),
            connected,
            confirmed: Mutex::new(HashMap::new()),
        });
        let (done_tx, done_rx) = mpsc::channel();
        let worker_shared = shared.clone();
        let worker = std::thread::Builder::new()
            .name("gxserver-live-activity".into())
            .spawn(move || {
                run(worker_shared);
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

    /// The phone `machine_id` registered (`true`) or cleared (`false`) its tokens here: it is (or is no longer)
    /// sent to, and the worker looks again at once.
    pub(crate) fn confirm(&self, machine_id: &str, registered: bool) {
        {
            let mut confirmed = self
                .shared
                .confirmed
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if registered {
                confirmed.insert(machine_id.to_string(), self.shared.clock.now_ms());
            } else {
                confirmed.remove(machine_id);
            }
        }
        self.poke();
    }

    /// Whether the phone `machine_id` confirmed its registration since this gxserver started or last slept, within
    /// the last `confirmation_ttl_ms`.
    pub(crate) fn is_confirmed(&self, machine_id: &str) -> bool {
        let now = self.shared.clock.now_ms();
        self.shared
            .confirmed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(machine_id)
            .is_some_and(|at| now - at < self.shared.timings.confirmation_ttl_ms)
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

fn run(shared: Arc<Shared>) {
    let shared = &shared;
    let mut phones: HashMap<String, PhoneState> = HashMap::new();
    let mut memory = FinishedMemory::new();
    let mut remotes = Remotes::default();
    let mut next_due: Option<i64> = None;
    let mut awake_at = shared.clock.now_ms();
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
            // The worker never waits longer than a tick, so a longer gap means the computer slept: the phone may
            // have picked another banner computer meanwhile, so wait for it to confirm this one again.
            let woke_at = shared.clock.now_ms();
            if woke_at - awake_at > SLEEP_GAP_MS + shared.timings.tick_ms as i64 {
                let forgotten = std::mem::take(
                    &mut *shared
                        .confirmed
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner),
                );
                if !forgotten.is_empty() {
                    shared.log(
                        LogLevel::Warn,
                        "liveActivity.sleptWaitingForPhone",
                        json!({ "phones": forgotten.len() }),
                    );
                }
            }
            awake_at = woke_at;
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
        next_due = pass(shared, &mut phones, &mut memory, &mut remotes);
        let mut queue = shared.lock();
        queue.busy = false;
        queue.passes += 1;
        shared.idle.notify_all();
    }
}

/// The phones to look at: Live Activity entries whose phone is still paired (or that a caller with the main
/// token registered) and that the phone confirmed since this gxserver started or last slept, each with its
/// "Show session names" switch.
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
    let now = shared.clock.now_ms();
    let confirmed = shared
        .confirmed
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    // An expired confirmation stops the pushes and ends nothing: the phone may have moved to another computer
    // while it could not reach this one.
    let phones: Vec<LiveActivityPhone> = phones
        .into_iter()
        .filter(|phone| {
            confirmed
                .get(&phone.machine_id)
                .is_some_and(|at| now - at < shared.timings.confirmation_ttl_ms)
        })
        .collect();
    if phones.is_empty() {
        return Some(Vec::new());
    }
    let paired: Option<HashSet<String>> = open_gxserver_database(&shared.paths)
        .ok()
        .and_then(|db| {
            crate::remote_access::RemotePairingRepository::new(&db)
                .list_devices()
                .ok()
        })
        .map(|devices| devices.into_iter().map(|device| device.id).collect());
    let Some(paired) = paired else {
        shared.log(LogLevel::Warn, "liveActivity.pairingsUnreadable", json!({}));
        return None;
    };
    Some(
        phones
            .into_iter()
            .filter(|phone| {
                phone
                    .device_id
                    .as_ref()
                    .is_none_or(|device_id| paired.contains(device_id))
            })
            .map(|phone| {
                let names = phone.session_names;
                (phone, names)
            })
            .collect(),
    )
}

/// This computer's rows as the phone's session list would hold them now, and its launchers.
fn read_rows(shared: &Shared, now: i64) -> Result<(Vec<Value>, Vec<Value>), String> {
    let db = open_gxserver_database(&shared.paths).map_err(|error| error.to_string())?;
    let repository = DomainRepository::new(&db, shared.server_id.as_str());
    let projects = repository.list_projects().map_err(|error| error.message)?;
    let sessions = repository
        .list_sessions_excluding_stopped(None)
        .map_err(|error| error.message)?;
    Ok(rows_of(&projects, &sessions, now))
}

/// One computer's mobile summary rows (running sessions of active projects, zmx only, as the phone keeps
/// them) and the launchers that name its custom agents, from its projects and sessions.
fn rows_of(projects: &[Value], sessions: &[Value], now: i64) -> (Vec<Value>, Vec<Value>) {
    let launchers = crate::sidebar_hud::sidebar_agent_buttons_from_projects(projects)
        .as_array()
        .cloned()
        .unwrap_or_default();
    let generated_at = chrono::DateTime::from_timestamp_millis(now)
        .unwrap_or_default()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let rows = sessions
        .iter()
        .filter(|session| session.get("lifecycleState").and_then(Value::as_str) != Some("stopped"))
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
            (row.get("provider").and_then(Value::as_str) == Some("zmx")).then_some(row)
        })
        .collect();
    (rows, launchers)
}

/// A read of a connected computer, finished on its own thread.
struct RemoteRead {
    server_id: String,
    name: String,
    started_at: i64,
    result: Option<(Vec<Value>, Vec<Value>)>,
}

/// What the worker knows of the connected computers between passes.
#[derive(Default)]
struct Remotes {
    lists: HashMap<String, RemoteList>,
    in_flight: HashSet<String>,
    /// When to try again a computer that never answered yet.
    retry_at: HashMap<String, i64>,
}

/// A connected computer's last good read.
struct RemoteList {
    name: String,
    rows: Vec<Value>,
    launchers: Vec<Value>,
    read_at: i64,
    next_read: i64,
    /// Its last read failed (logged once, when it stopped answering).
    unanswered: bool,
}

/// One request to a connected computer over the connection this computer has to it, with the token it gave
/// this one; its gxserver allows only the peer endpoints (`connected_computers::scope`).
fn peer_rpc(
    computer: &crate::connected_computers::ConnectedComputer,
    path: &str,
    params: Value,
) -> Option<Value> {
    // Only a listener of this account gets the token (as coordinator_remote_runtime.rs and relay.rs).
    ghostex_paths::loopback_owner::check_url(&computer.base_url).ok()?;
    let body = json!({
        "params": params,
        "protocolVersion": crate::constants::GXSERVER_PROTOCOL_VERSION,
    });
    let response = crate::http_agent::answering_agent()
        .post(format!("{}{path}", computer.base_url))
        .header("authorization", format!("Bearer {}", computer.token()))
        .header("content-type", "application/json")
        .header(
            crate::constants::GXSERVER_PROTOCOL_HEADER,
            crate::constants::GXSERVER_PROTOCOL_VERSION.to_string(),
        )
        .config()
        .timeout_global(Some(REMOTE_READ_TIMEOUT))
        .build()
        .send(body.to_string())
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let envelope: Value = crate::http_agent::read_json(response).ok()?;
    envelope.get("result").cloned()
}

/// A connected computer's rows, read now; None when it does not answer.
fn read_remote(
    computer: &crate::connected_computers::ConnectedComputer,
    now: i64,
) -> Option<(Vec<Value>, Vec<Value>)> {
    let projects = peer_rpc(computer, "/api/listProjects", json!({}))?
        .get("projects")?
        .as_array()?
        .clone();
    let sessions = peer_rpc(
        computer,
        "/api/listSessions",
        json!({ "includeStopped": false }),
    )?
    .get("sessions")?
    .as_array()?
    .clone();
    Some(rows_of(&projects, &sessions, now))
}

/// CDXC:Notifications 2026-11-11 DECISION: User, on which computer's banner wins while the phone is locked:
/// "u should push from 1 machine. since usually ppl connect their machines together like their 2 pcs for ex.
/// so u can actually push status of both machines from 1 main machine." The computer that holds the phone's
/// tokens covers itself and every computer connected to it (Settings > Remote), read over the connection the
/// computers already have, as the sidebar shows them. A computer whose agents may not reach this one (Agent
/// access off, or the approval still waiting) is not read; one that stops answering keeps its last rows for
/// `remote_keep_ms`, so the banner never says "working" for a computer that went away.
fn refresh_remotes(shared: &Arc<Shared>, remotes: &mut Remotes, now: i64) -> Option<i64> {
    let finished: Vec<RemoteRead> = std::mem::take(
        &mut *shared
            .remote_reads
            .lock()
            .unwrap_or_else(PoisonError::into_inner),
    );
    for read in finished {
        remotes.in_flight.remove(&read.server_id);
        match read.result {
            Some((rows, launchers)) => {
                remotes.lists.insert(
                    read.server_id,
                    RemoteList {
                        name: read.name,
                        rows,
                        launchers,
                        read_at: read.started_at,
                        next_read: read.started_at + shared.timings.remote_poll_ms,
                        unanswered: false,
                    },
                );
            }
            None => {
                let next = now + shared.timings.remote_poll_ms;
                remotes.retry_at.insert(read.server_id.clone(), next);
                if let Some(list) = remotes.lists.get_mut(&read.server_id) {
                    list.next_read = next;
                    if !list.unanswered {
                        list.unanswered = true;
                        shared.log(
                            LogLevel::Warn,
                            "liveActivity.computerUnreadable",
                            json!({ "serverId": read.server_id }),
                        );
                    }
                }
            }
        }
    }
    let connected: Vec<_> = (shared.connected)()
        .into_iter()
        .filter(|computer| {
            computer.agent_access
                && computer.waiting.is_none()
                && !computer.base_url.is_empty()
                && computer.server_id != shared.server_id
        })
        .collect();
    let keep = shared.timings.remote_keep_ms;
    remotes.lists.retain(|server_id, list| {
        connected
            .iter()
            .any(|computer| &computer.server_id == server_id)
            && now - list.read_at < keep
    });
    remotes.retry_at.retain(|server_id, _| {
        connected
            .iter()
            .any(|computer| &computer.server_id == server_id)
    });
    let mut next: Option<i64> = None;
    for computer in connected {
        if remotes.in_flight.contains(&computer.server_id) {
            // Its read wakes the worker when it finishes.
            continue;
        }
        let due = remotes
            .lists
            .get(&computer.server_id)
            .map(|list| list.next_read)
            .or_else(|| remotes.retry_at.get(&computer.server_id).copied())
            .unwrap_or(0);
        if now < due {
            next = Some(next.map_or(due, |current: i64| current.min(due)));
            continue;
        }
        // A computer that is slow or asleep never holds up this computer's updates: each read runs on its own
        // thread and wakes the worker when it is done.
        remotes.in_flight.insert(computer.server_id.clone());
        let reader = shared.clone();
        let spawned = std::thread::Builder::new()
            .name("gxserver-live-activity-read".into())
            .spawn(move || {
                let result = read_remote(&computer, now);
                reader
                    .remote_reads
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(RemoteRead {
                        server_id: computer.server_id.clone(),
                        name: computer.name.clone(),
                        started_at: now,
                        result,
                    });
                let mut queue = reader.lock();
                queue.poked = true;
                reader.wake.notify_all();
            });
        if spawned.is_err() {
            remotes.in_flight.clear();
        }
    }
    next
}

/// One pass over every phone; returns when the next pass is due without a change.
fn pass(
    shared: &Arc<Shared>,
    states: &mut HashMap<String, PhoneState>,
    memory: &mut FinishedMemory,
    remotes: &mut Remotes,
) -> Option<i64> {
    let phones = registered_phones(shared)?;
    states.retain(|machine_id, _| {
        phones
            .iter()
            .any(|(phone, _)| &phone.machine_id == machine_id)
    });
    if phones.is_empty() {
        memory.clear();
        remotes.lists.clear();
        remotes.retry_at.clear();
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
    let remote_due = refresh_remotes(shared, remotes, now);
    let this_name = crate::connected_computers::this_computer_name();
    let mut sources = vec![Source {
        server_id: &shared.server_id,
        computer: &this_name,
        rows: &rows,
        launchers: &launchers,
        read_at: now,
    }];
    let mut remote_ids: Vec<&String> = remotes.lists.keys().collect();
    remote_ids.sort();
    for server_id in remote_ids {
        let list = &remotes.lists[server_id];
        sources.push(Source {
            server_id,
            computer: &list.name,
            rows: &list.rows,
            launchers: &list.launchers,
            read_at: list.read_at,
        });
    }
    let candidates = collect_candidates(&sources, now, memory);
    let mut next_due: Option<i64> = remote_due;
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
    let target = |server_id: &str| phone.machine_id_for(server_id, &shared.server_id);
    let content = render(candidates, &target, names, now);
    let shape = shape_of(&content);
    let active = candidates.active();

    let Some(token) = phone.activity_token.as_deref() else {
        state.activity_fingerprint = None;
        state.sent_shape = None;
        state.idle_since = None;
        // No activity: a start when a working stint begins, if the phone gave a push-to-start token (off for now,
        // SEND_PUSH_TO_START).
        if !SEND_PUSH_TO_START {
            return None;
        }
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
        *state = PhoneState {
            activity_fingerprint: Some(fingerprint),
            known_stints: state.known_stints.clone(),
            last_start: state.last_start,
            ..PhoneState::default()
        };
    }
    // The phone confirmed this computer as its one banner computer (registration.rs). Once it has put a working or
    // waiting agent on the activity it owns it: idle content included, and the end. Before that it leaves it alone.
    if active {
        state.idle_since = None;
    } else if !state.pushed_active {
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
    let mut body = request(token, phone, "update", now, Some(&content), stale);
    // A keep-alive (nothing the banner shows changed) goes at Apple's low priority, which iOS does not count
    // against the activity's update budget; a change goes at once (priority 10, the relay's default).
    if !changed {
        body["priority"] = json!(5);
    }
    let outcome = shared.client.send(&body);
    let refused_for_good = matches!(outcome, RelayOutcome::Failed { retry: false, .. });
    if outcome == RelayOutcome::Sent || refused_for_good {
        // A content the relay refused for good is not sent again; the next change is.
        state.sent_shape = Some(shape);
    }
    if outcome == RelayOutcome::Sent {
        // From the answer, not the decision: a slow request must not bring the next one closer.
        state.sent_at = shared.clock.now_ms();
        state.pushed_active |= active;
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
                // The relay refused this content: the next change is sent (phone_pass keeps its shape), not this.
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
