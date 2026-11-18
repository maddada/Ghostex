use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

#[cfg(not(windows))]
use super::read_zmx_existing_session_names;
use super::{ZmxEndpointError, ZmxEndpointResult, ZmxProcessIdentity};

/*
CDXC:Zmx 2026-09-01:
`listSessions`, `readPresentationSnapshot`, and the presentation subscribe
boundary all run the same zmx existence and process
snapshot probes, and desktop/web clients poll them roughly every two seconds.
Each probe spawns a shell that runs `zmx list` plus `ps -axo`, so a single poll
tick paid for several identical subprocess round trips and delayed every other
RPC behind them.

These two reads only feed presentation freshness, so a result that is at most
one poll interval old is exactly what the pollers already observe. Cache them
for two seconds, keyed by the requested inputs. The authoritative lifecycle
paths (`probe_zmx_session`, provider start/kill, the launchd readiness poll)
deliberately do not go through this module and keep spawning their own probe.
*/
const ZMX_PROBE_CACHE_TTL: Duration = Duration::from_millis(2_000);

/// CDXC:Zmx 2026-11-18 WHY:
/// On Windows every process-identity miss starts `wmx process-snapshot` (a `wmx.exe`, its conhost and an antivirus scan of the image). The loops that only show what runs (the subagent status pass, the Codex rollout follower, the hook-free fallback watch, the memory guard's size attribution) tick every 2 to 5 seconds on their own clocks, so the two-second snapshot was almost always stale when the next one asked: 27 snapshots a minute with six idle agents, and 35 to 60 `wmx.exe` a minute in all with about 40 (GitHub #666). Those loops read through `read_display_zmx_session_process_identities`, which accepts a snapshot up to this old, so one probe serves all of them. Liveness decisions (self-heal, the send heal and start-failure check, the identity repair the session list runs) keep the two-second snapshot, or probe directly, exactly as before: a longer TTL there would read a just-restarted agent as gone. A longer TTL for everyone was rejected for that reason.
const ZMX_PROBE_DISPLAY_TTL: Duration = Duration::from_millis(6_000);

#[cfg(not(windows))]
struct CachedExistingSessionNames {
    fetched_at: Instant,
    names: HashSet<String>,
}

/*
CDXC:Zmx 2026-09-11 WHY:
This cache used to be keyed by the exact requested name set. The fleet-status
pass asks for one session at a time (`current_process`), the sync passes ask
for every running candidate at once, and an open chat asks for its own session,
so none of them ever shared an entry and each caller spawned its own
`zmx list` + `ps -axo` shell: with 85 running sessions that was one snapshot
per session every five seconds and a large share of gxserver's idle CPU.

The cache now remembers every name asked for within the last minute and, on a
miss, probes for that whole set, so any lookup inside the TTL is served from
the one snapshot no matter how the caller sliced its names. The reply is still
filtered to the names the caller asked for.
*/
const ZMX_PROBE_RECENT_NAMES_WINDOW: Duration = Duration::from_secs(60);

struct CachedProcessIdentities {
    fetched_at: Instant,
    home_dir: PathBuf,
    /// The names the snapshot was probed for; a lookup is a hit only when every
    /// requested name is in here.
    names: HashSet<String>,
    identities: HashMap<String, ZmxProcessIdentity>,
}

#[derive(Default)]
struct ProcessIdentitiesCache {
    generation: u64,
    /// Every name requested recently, with the time it was last requested.
    recent_names: HashMap<String, Instant>,
    snapshot: Option<CachedProcessIdentities>,
    /// CDXC:Zmx 2026-09-27 WHY:
    /// A failed probe is remembered for the same TTL as a snapshot. Callers queued on the probe mutex used to each start their own probe after a failure; on Windows the process scan took over 5s and timed out every time, so they ran back to back, and because the presentation passes call this from gxserver's async request threads, every request (chat sends included) stalled for 10s or more at a time.
    failed_at: Option<Instant>,
    shutdown: Option<tokio::sync::broadcast::Receiver<()>>,
}

#[cfg(not(windows))]
static EXISTING_SESSION_NAMES_CACHE: OnceLock<Mutex<Option<CachedExistingSessionNames>>> =
    OnceLock::new();
static PROCESS_IDENTITIES_CACHE: OnceLock<Mutex<ProcessIdentitiesCache>> = OnceLock::new();
static PROCESS_IDENTITIES_PROBE: Mutex<()> = Mutex::new(());

/// CDXC:ServerDaemon 2026-09-23 WHY:
/// Blocking identity readers can already be queued on the probe mutex when their async owners are aborted. Windows shutdown then ran three consecutive five-second process snapshots. Retain the daemon's shutdown receiver here so queued readers stop at the shared probe boundary instead of starting another subprocess.
pub(crate) fn set_zmx_process_identity_shutdown(shutdown: tokio::sync::broadcast::Receiver<()>) {
    let cache =
        PROCESS_IDENTITIES_CACHE.get_or_init(|| Mutex::new(ProcessIdentitiesCache::default()));
    let mut guard = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.shutdown = Some(shutdown);
}

fn ensure_process_identity_probe_active(cache: &ProcessIdentitiesCache) -> ZmxEndpointResult<()> {
    if cache
        .shutdown
        .as_ref()
        .is_some_and(|shutdown| shutdown.is_closed() || !shutdown.is_empty())
    {
        return Err(ZmxEndpointError::DependencyUnavailable(
            "gxserver is shutting down.".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn invalidate_zmx_process_identity_cache() {
    if let Some(cache) = PROCESS_IDENTITIES_CACHE.get() {
        if let Ok(mut cache) = cache.lock() {
            cache.snapshot = None;
            cache.failed_at = None;
            cache.generation = cache.generation.wrapping_add(1);
        }
    }
    #[cfg(windows)]
    {
        let mut shared = SHARED_PROCESS_SNAPSHOT
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        shared.output = None;
        shared.generation = shared.generation.wrapping_add(1);
    }
}

/// Presentation-freshness read of the live zmx session names, served from a
/// two-second cache. Never use this where authoritative provider state is
/// required; call `read_zmx_existing_session_names` directly instead.
pub fn read_cached_zmx_existing_session_names() -> Result<HashSet<String>, ZmxEndpointError> {
    #[cfg(windows)]
    {
        windows_live_session_names()
    }
    #[cfg(not(windows))]
    {
        let cache = EXISTING_SESSION_NAMES_CACHE.get_or_init(|| Mutex::new(None));
        if let Ok(guard) = cache.lock() {
            if let Some(entry) = guard
                .as_ref()
                .filter(|entry| entry.fetched_at.elapsed() < ZMX_PROBE_CACHE_TTL)
            {
                return Ok(entry.names.clone());
            }
        }
        let names = read_zmx_existing_session_names()?;
        if let Ok(mut guard) = cache.lock() {
            *guard = Some(CachedExistingSessionNames {
                fetched_at: Instant::now(),
                names: names.clone(),
            });
        }
        Ok(names)
    }
}

/// Presentation-freshness read of the live zmx process identities, served from
/// a two-second cache shared by every caller: a miss probes for the union of
/// all recently requested names, and the result is filtered to `session_names`.
pub fn read_cached_zmx_session_process_identities(
    session_names: &[String],
    home_dir: &Path,
) -> ZmxEndpointResult<HashMap<String, ZmxProcessIdentity>> {
    read_process_identities_within(session_names, home_dir, ZMX_PROBE_CACHE_TTL)
}

/// The same shared snapshot for loops that only display what runs, accepting it up to
/// `ZMX_PROBE_DISPLAY_TTL` old. Never use it to decide whether an agent or daemon is alive.
pub fn read_display_zmx_session_process_identities(
    session_names: &[String],
    home_dir: &Path,
) -> ZmxEndpointResult<HashMap<String, ZmxProcessIdentity>> {
    read_process_identities_within(session_names, home_dir, ZMX_PROBE_DISPLAY_TTL)
}

fn read_process_identities_within(
    session_names: &[String],
    home_dir: &Path,
    max_age: Duration,
) -> ZmxEndpointResult<HashMap<String, ZmxProcessIdentity>> {
    if session_names.is_empty() {
        return Ok(HashMap::new());
    }
    let cache =
        PROCESS_IDENTITIES_CACHE.get_or_init(|| Mutex::new(ProcessIdentitiesCache::default()));
    {
        let mut guard = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        ensure_process_identity_probe_active(&guard)?;
        let now = Instant::now();
        for name in session_names {
            guard.recent_names.insert(name.clone(), now);
        }
        guard
            .recent_names
            .retain(|_, at| now.duration_since(*at) < ZMX_PROBE_RECENT_NAMES_WINDOW);
        if let Some(snapshot) = fresh_process_snapshot(&guard, session_names, home_dir, max_age) {
            return Ok(select_identities(&snapshot.identities, session_names));
        }
        ensure_no_recent_probe_failure(&guard)?;
    }
    // CDXC:Zmx 2026-09-13 WHY:
    // Concurrent misses used to spawn identical process scans. Serialize only misses and recheck after waiting, while fresh readers keep using the short-held cache mutex.
    let _probe = PROCESS_IDENTITIES_PROBE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (probe_names, generation) = {
        let guard = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        ensure_process_identity_probe_active(&guard)?;
        if let Some(snapshot) = fresh_process_snapshot(&guard, session_names, home_dir, max_age) {
            return Ok(select_identities(&snapshot.identities, session_names));
        }
        ensure_no_recent_probe_failure(&guard)?;
        let mut names = guard.recent_names.keys().cloned().collect::<Vec<_>>();
        // A request can outlive the recent-name window while waiting for a probe.
        names.extend(session_names.iter().cloned());
        names.sort();
        names.dedup();
        (names, guard.generation)
    };
    #[cfg(windows)]
    let probed = shared_process_snapshot_within(max_age).and_then(|output| {
        if !output.whole {
            return Err(super::provider::process_snapshot_unreadable());
        }
        Ok((
            super::provider::zmx_session_process_identities_from_probe(
                &output.stdout,
                &probe_names,
                home_dir,
            ),
            output.read_at,
        ))
    });
    #[cfg(not(windows))]
    let probed = super::read_zmx_session_process_identities(&probe_names, home_dir)
        .map(|identities| (identities, Instant::now()));
    let (identities, fetched_at) = match probed {
        Ok(probed) => probed,
        Err(error) => {
            let mut guard = cache
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if guard.generation == generation {
                guard.failed_at = Some(Instant::now());
            }
            return Err(error);
        }
    };
    let selected = select_identities(&identities, session_names);
    {
        let mut guard = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        ensure_process_identity_probe_active(&guard)?;
        if guard.generation == generation {
            guard.failed_at = None;
            guard.snapshot = Some(CachedProcessIdentities {
                fetched_at,
                home_dir: home_dir.to_path_buf(),
                names: probe_names.into_iter().collect(),
                identities,
            });
        }
    }
    Ok(selected)
}

/// One `wmx process-snapshot` output with the moment its probe started, shared by every reader on Windows.
///
/// CDXC:Zmx 2026-11-18 WHY:
/// `wmx process-snapshot` starts with wmx's `client::list`, the same list `wmx list --short` prints, then prints every process row whichever sessions were asked for. gxserver started one for the identity readers and a separate `wmx list --short` for the live-name readers (self-heal every 3 seconds, the existence repair): about 47 `wmx.exe` a minute with six idle agents (GitHub #666). Both readers now take this one output: a name reader while it is under the two-second TTL, an identity reader under its own TTL, and a miss on either side probes once for both. The probe start is earlier than wmx's own list read, so every reader sees data at least as fresh as the separate two-second caches gave. The process walk costs wmx about 14 ms over `list --short` (74 ms against 60 ms with 690 processes); starting the process is the expensive part. The output lists every process on the computer (310 KB with 700), so it can pass gxserver's 1 MiB cap where `list --short` would not: the list comes first, so name readers still read an output cut there and identity readers refuse it as before. `invalidate_zmx_process_identity_cache` drops it too.
#[cfg(windows)]
struct SharedProcessSnapshot {
    generation: u64,
    output: Option<ProcessSnapshotOutput>,
}

#[cfg(windows)]
#[derive(Clone)]
struct ProcessSnapshotOutput {
    stdout: std::sync::Arc<str>,
    /// When the probe started, before wmx read its session list.
    read_at: Instant,
    /// `false` when the output was cut at the stdout cap: its session list can still be read,
    /// its process rows cannot, so identity readers refuse it as they did before.
    whole: bool,
}

#[cfg(windows)]
static SHARED_PROCESS_SNAPSHOT: Mutex<SharedProcessSnapshot> = Mutex::new(SharedProcessSnapshot {
    generation: 0,
    output: None,
});

#[cfg(windows)]
fn shared_process_snapshot_if_within(max_age: Duration) -> Option<ProcessSnapshotOutput> {
    SHARED_PROCESS_SNAPSHOT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .output
        .clone()
        .filter(|output| output.read_at.elapsed() < max_age)
}

/// The shared output if it is younger than `max_age`, otherwise a new probe stored for every
/// reader. The caller holds `PROCESS_IDENTITIES_PROBE`, so concurrent misses start one probe.
#[cfg(windows)]
fn shared_process_snapshot_within(max_age: Duration) -> ZmxEndpointResult<ProcessSnapshotOutput> {
    if let Some(output) = shared_process_snapshot_if_within(max_age) {
        return Ok(output);
    }
    let generation = SHARED_PROCESS_SNAPSHOT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .generation;
    let read_at = Instant::now();
    let (stdout, whole) = super::provider::run_zmx_process_snapshot_probe()?;
    let output = ProcessSnapshotOutput {
        stdout: stdout.into(),
        read_at,
        whole,
    };
    let mut shared = SHARED_PROCESS_SNAPSHOT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if shared.generation == generation {
        shared.output = Some(output.clone());
    }
    Ok(output)
}

/// The live session names from the shared `wmx process-snapshot` output, two seconds old at most.
#[cfg(windows)]
fn windows_live_session_names() -> Result<HashSet<String>, ZmxEndpointError> {
    if let Some(names) = shared_process_snapshot_if_within(ZMX_PROBE_CACHE_TTL)
        .and_then(|output| super::provider::process_snapshot_live_names(&output.stdout))
    {
        return Ok(names);
    }
    let _probe = PROCESS_IDENTITIES_PROBE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(cache) = PROCESS_IDENTITIES_CACHE.get() {
        ensure_process_identity_probe_active(
            &cache
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        )?;
    }
    let output = shared_process_snapshot_within(ZMX_PROBE_CACHE_TTL)?;
    super::provider::process_snapshot_live_names(&output.stdout).ok_or_else(|| {
        ZmxEndpointError::DependencyUnavailable(
            "Could not read the session list from the process snapshot.".to_string(),
        )
    })
}

fn ensure_no_recent_probe_failure(cache: &ProcessIdentitiesCache) -> ZmxEndpointResult<()> {
    if cache
        .failed_at
        .is_some_and(|failed_at| failed_at.elapsed() < ZMX_PROBE_CACHE_TTL)
    {
        return Err(ZmxEndpointError::DependencyUnavailable(
            "The session process snapshot failed moments ago.".to_string(),
        ));
    }
    Ok(())
}

fn fresh_process_snapshot<'a>(
    cache: &'a ProcessIdentitiesCache,
    session_names: &[String],
    home_dir: &Path,
    max_age: Duration,
) -> Option<&'a CachedProcessIdentities> {
    cache.snapshot.as_ref().filter(|snapshot| {
        snapshot.fetched_at.elapsed() < max_age
            && snapshot.home_dir == home_dir
            && session_names
                .iter()
                .all(|name| snapshot.names.contains(name))
    })
}

fn select_identities(
    identities: &HashMap<String, ZmxProcessIdentity>,
    session_names: &[String],
) -> HashMap<String, ZmxProcessIdentity> {
    session_names
        .iter()
        .filter_map(|name| {
            identities
                .get(name)
                .map(|identity| (name.clone(), identity.clone()))
        })
        .collect()
}
