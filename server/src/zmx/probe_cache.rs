use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use super::{
    read_zmx_existing_session_names, read_zmx_session_process_identities, ZmxEndpointError,
    ZmxEndpointResult, ZmxProcessIdentity,
};

/*
CDXC:Zmx 2026-09-01:
`listSessions`, `readPresentationSnapshot`, `readProjectStatus`, and the
presentation subscribe boundary all run the same zmx existence and process
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
    /// Every name requested recently, with the time it was last requested.
    recent_names: HashMap<String, Instant>,
    snapshot: Option<CachedProcessIdentities>,
}

static EXISTING_SESSION_NAMES_CACHE: OnceLock<Mutex<Option<CachedExistingSessionNames>>> =
    OnceLock::new();
static PROCESS_IDENTITIES_CACHE: OnceLock<Mutex<ProcessIdentitiesCache>> = OnceLock::new();

pub(crate) fn invalidate_zmx_process_identity_cache() {
    if let Some(cache) = PROCESS_IDENTITIES_CACHE.get() {
        if let Ok(mut cache) = cache.lock() {
            cache.snapshot = None;
        }
    }
}

/// Presentation-freshness read of the live zmx session names, served from a
/// two-second cache. Never use this where authoritative provider state is
/// required; call `read_zmx_existing_session_names` directly instead.
pub fn read_cached_zmx_existing_session_names() -> Result<HashSet<String>, ZmxEndpointError> {
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

/// Presentation-freshness read of the live zmx process identities, served from
/// a two-second cache shared by every caller: a miss probes for the union of
/// all recently requested names, and the result is filtered to `session_names`.
pub fn read_cached_zmx_session_process_identities(
    session_names: &[String],
    home_dir: &Path,
) -> ZmxEndpointResult<HashMap<String, ZmxProcessIdentity>> {
    if session_names.is_empty() {
        return Ok(HashMap::new());
    }
    let cache =
        PROCESS_IDENTITIES_CACHE.get_or_init(|| Mutex::new(ProcessIdentitiesCache::default()));
    let probe_names = {
        let Ok(mut guard) = cache.lock() else {
            return read_zmx_session_process_identities(session_names, home_dir);
        };
        let now = Instant::now();
        for name in session_names {
            guard.recent_names.insert(name.clone(), now);
        }
        guard.recent_names.retain(|_, requested_at| {
            now.duration_since(*requested_at) < ZMX_PROBE_RECENT_NAMES_WINDOW
        });
        if let Some(snapshot) = guard.snapshot.as_ref().filter(|snapshot| {
            snapshot.fetched_at.elapsed() < ZMX_PROBE_CACHE_TTL
                && snapshot.home_dir == home_dir
                && session_names
                    .iter()
                    .all(|name| snapshot.names.contains(name))
        }) {
            return Ok(select_identities(&snapshot.identities, session_names));
        }
        let mut names = guard.recent_names.keys().cloned().collect::<Vec<_>>();
        names.sort();
        names
    };
    let identities = read_zmx_session_process_identities(&probe_names, home_dir)?;
    let selected = select_identities(&identities, session_names);
    if let Ok(mut guard) = cache.lock() {
        guard.snapshot = Some(CachedProcessIdentities {
            fetched_at: Instant::now(),
            home_dir: home_dir.to_path_buf(),
            names: probe_names.into_iter().collect(),
            identities,
        });
    }
    Ok(selected)
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
