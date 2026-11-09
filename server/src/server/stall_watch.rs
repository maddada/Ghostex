//! Notes, from a thread of its own, when gxserver stops answering: the async runtime no longer
//! runs a trivial task, or a request has been running for minutes. Each such moment is one line in
//! `<logs>/stall.log`, written with plain file appends so a stuck logger or lock can't stop it.
//!
//! CDXC:ServerDaemon 2026-11-09 WHY:
//! GitHub #520 follow-up: a Windows gxserver stopped answering on both its ports for 50 minutes and
//! logged nothing in between, until the installer killed it; nothing said what it was stuck on,
//! and a dump would show no names (builds carry no debug info). This watch records what was
//! running when it stuck, so the next hang names its request and how long the runtime was stalled.

use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};

/// How often the watch looks.
const CHECK_EVERY: Duration = Duration::from_secs(15);
/// The runtime counts as stalled when its heartbeat is older than this.
const RUNTIME_STALLED_AFTER: Duration = Duration::from_secs(30);
/// A request running longer than this is named in the next line.
const REQUEST_SLOW_AFTER: Duration = Duration::from_secs(60);
/// While the same trouble lasts, it is written again at most this often.
const REPEAT_EVERY: Duration = Duration::from_secs(300);
/// Past this size the file moves to `stall.log.1` (replacing the one before) and starts again, so a
/// hang that lasts days can't fill the disk and a later one is still written.
const MAX_LOG_BYTES: u64 = 4 * 1024 * 1024;

/*
CDXC:ServerDaemon 2026-11-09 DECISION:
The coordinator, for the user: `slowRequests` names only real trouble. These routes run past a minute
by design, so they never count as slow (each with the client limit or wait that says so):
- the owner at another device or prompt: `/api/pairComputer` and `/api/computerPairingStatus` (the
  other computer's approval, 190 s), `/api/removePairedDevice` and `/api/enableSshAccess` (a
  Windows administrator prompt, 300 s), `/api/callConnectorAgentTool` (a connector's approval card,
  up to 30 min);
- long polls: `/api/waitConnectorAgentTools` (4 min);
- installs and downloads: `/api/installTailcat` (900 s), `/api/installExtension` and
  `/api/uninstallExtension` (600 s), `/api/updates` (90 s);
- other services doing the work: `/api/agentbox`, `/api/cloudBoxes`, `/api/claudeCloud`,
  `/api/startCloudWork`, `/api/startWorkOnTicket`, `/api/sendFeedback`, `/api/draftFeedback`,
  `/api/startFeedbackAgent`, `/api/transcribeDictation`, `/api/setVoiceAgentKey`,
  `/api/agentAccounts`, and the connectors' sign-ins and catalog (Composio, Executor, connector CLIs);
- a call relayed to another computer (`/api/computers/<id>/relay/...`), which lasts as long as that
  computer's route.
Event streams and websockets end their request when the stream starts, and `/api/waitSecretRequest`
(25 s) and `/api/agentChatChannel` (20 s) stay well under a minute, so they need no entry.
*/
/// Routes, by prefix, that run past a minute by design (the note above).
const LONG_BY_DESIGN: &[&str] = &[
    "/api/pairComputer",
    "/api/computerPairingStatus",
    "/api/removePairedDevice",
    "/api/enableSshAccess",
    "/api/callConnectorAgentTool",
    "/api/waitConnectorAgentTools",
    "/api/installTailcat",
    "/api/installExtension",
    "/api/uninstallExtension",
    "/api/updates",
    "/api/agentbox",
    "/api/cloudBoxes",
    "/api/claudeCloud",
    "/api/startCloudWork",
    "/api/startWorkOnTicket",
    "/api/sendFeedback",
    "/api/draftFeedback",
    "/api/startFeedbackAgent",
    "/api/transcribeDictation",
    "/api/setVoiceAgentKey",
    "/api/agentAccounts",
    "/api/connectComposioApp",
    "/api/signInComposio",
    "/api/refreshComposio",
    "/api/signInConnectorCli",
    "/api/connectExecutorApp",
    "/api/addExecutorApp",
    "/api/addExecutorMcpServer",
    "/api/enableExecutor",
    "/api/searchExecutorCatalog",
    "/api/computers/",
];

/// Whether a request to `route` may run past a minute without anything being wrong.
fn long_by_design(route: &str) -> bool {
    LONG_BY_DESIGN
        .iter()
        .any(|prefix| route.starts_with(prefix))
}

struct InFlight {
    route: String,
    started: Instant,
}

fn in_flight() -> &'static Mutex<BTreeMap<u64, InFlight>> {
    static IN_FLIGHT: OnceLock<Mutex<BTreeMap<u64, InFlight>>> = OnceLock::new();
    IN_FLIGHT.get_or_init(Default::default)
}

static NEXT_REQUEST: AtomicU64 = AtomicU64::new(0);
/// Milliseconds since `EPOCH` at the runtime's last heartbeat.
static HEARTBEAT_MS: AtomicU64 = AtomicU64::new(0);

fn epoch() -> Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

fn now_ms() -> u64 {
    epoch().elapsed().as_millis() as u64
}

/// Held for the life of one HTTP request; dropping it (answered, or the connection gone) ends it.
pub(crate) struct RequestGuard(u64);

impl Drop for RequestGuard {
    fn drop(&mut self) {
        if let Ok(mut requests) = in_flight().lock() {
            requests.remove(&self.0);
        }
    }
}

/// Notes a request to `path` (the URL path only: API routes, never a query or a body).
pub(crate) fn request_started(path: &str) -> RequestGuard {
    let id = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed);
    let route: String = path.chars().take(120).collect();
    if let Ok(mut requests) = in_flight().lock() {
        requests.insert(
            id,
            InFlight {
                route,
                started: Instant::now(),
            },
        );
    }
    RequestGuard(id)
}

/// Starts the watch: a runtime task that beats every few seconds, and a thread that checks it.
pub(crate) fn start(logs_dir: PathBuf) {
    let _ = epoch();
    HEARTBEAT_MS.store(now_ms(), Ordering::Relaxed);
    tokio::spawn(async {
        loop {
            HEARTBEAT_MS.store(now_ms(), Ordering::Relaxed);
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
    let _ = thread::Builder::new()
        .name("gxserver-stall-watch".into())
        .spawn(move || watch(logs_dir.join("stall.log")));
}

fn watch(log: PathBuf) {
    let mut last_written: Option<(String, Instant)> = None;
    loop {
        thread::sleep(CHECK_EVERY);
        let stalled_ms = now_ms().saturating_sub(HEARTBEAT_MS.load(Ordering::Relaxed));
        let runtime_stalled = stalled_ms >= RUNTIME_STALLED_AFTER.as_millis() as u64;
        let slow: Vec<(String, u64)> = match in_flight().try_lock() {
            Ok(requests) => requests
                .values()
                .filter(|request| request.started.elapsed() >= REQUEST_SLOW_AFTER)
                .filter(|request| !long_by_design(&request.route))
                .map(|request| (request.route.clone(), request.started.elapsed().as_secs()))
                .collect(),
            // The registry itself is held: say so rather than wait on it.
            Err(_) => vec![("(request list locked)".to_string(), 0)],
        };
        if !runtime_stalled && slow.is_empty() {
            last_written = None;
            continue;
        }
        let mut routes: Vec<String> = slow
            .iter()
            .map(|(route, seconds)| format!("{route} {seconds}s"))
            .collect();
        routes.sort();
        let key = format!(
            "{runtime_stalled}|{}",
            slow.iter()
                .map(|(route, _)| route.as_str())
                .collect::<Vec<_>>()
                .join(",")
        );
        if last_written
            .as_ref()
            .is_some_and(|(written, at)| *written == key && at.elapsed() < REPEAT_EVERY)
        {
            continue;
        }
        let line = serde_json::json!({
            "ts": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            "event": "gxserver.stall",
            "runtimeStalled": runtime_stalled,
            "runtimeHeartbeatAgeMs": stalled_ms,
            "slowRequests": routes,
        });
        if append(&log, &line.to_string()) {
            last_written = Some((key, Instant::now()));
        }
    }
}

fn append(log: &PathBuf, line: &str) -> bool {
    if std::fs::metadata(log).is_ok_and(|metadata| metadata.len() >= MAX_LOG_BYTES) {
        let _ = std::fs::rename(log, log.with_extension("log.1"));
    }
    if let Some(folder) = log.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .and_then(|mut file| writeln!(file, "{line}"))
        .is_ok()
}
