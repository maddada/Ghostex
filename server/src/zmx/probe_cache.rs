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

#[derive(PartialEq, Eq, Hash)]
struct ProcessIdentitiesCacheKey {
    home_dir: PathBuf,
    session_names: Vec<String>,
}

struct CachedProcessIdentities {
    fetched_at: Instant,
    identities: HashMap<String, ZmxProcessIdentity>,
}

static EXISTING_SESSION_NAMES_CACHE: OnceLock<Mutex<Option<CachedExistingSessionNames>>> =
    OnceLock::new();
static PROCESS_IDENTITIES_CACHE: OnceLock<
    Mutex<HashMap<ProcessIdentitiesCacheKey, CachedProcessIdentities>>,
> = OnceLock::new();

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
/// a two-second cache keyed by the exact requested session-name set and home
/// directory.
pub fn read_cached_zmx_session_process_identities(
    session_names: &[String],
    home_dir: &Path,
) -> ZmxEndpointResult<HashMap<String, ZmxProcessIdentity>> {
    if session_names.is_empty() {
        return Ok(HashMap::new());
    }
    let key = ProcessIdentitiesCacheKey {
        home_dir: home_dir.to_path_buf(),
        session_names: {
            let mut names = session_names.to_vec();
            names.sort();
            names.dedup();
            names
        },
    };
    let cache = PROCESS_IDENTITIES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut guard) = cache.lock() {
        guard.retain(|_, entry| entry.fetched_at.elapsed() < ZMX_PROBE_CACHE_TTL);
        if let Some(entry) = guard.get(&key) {
            return Ok(entry.identities.clone());
        }
    }
    let identities = read_zmx_session_process_identities(session_names, home_dir)?;
    if let Ok(mut guard) = cache.lock() {
        guard.insert(
            key,
            CachedProcessIdentities {
                fetched_at: Instant::now(),
                identities: identities.clone(),
            },
        );
    }
    Ok(identities)
}
