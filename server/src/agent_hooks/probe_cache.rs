use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use super::probing::{probe_login_shell_path_entries, resolve_command_path};

/*
CDXC:GxserverAgentHookProbeCache 2026-09-01:
`readAgentHookStatus` is awaited before every agent-session launch, and each
call ran two subprocess round trips per provider CLI: an interactive login
shell (`zsh -lic`) to derive the user's PATH, then a `command -v` probe in the
platform shell. The login-shell spawn alone sources the whole user profile and
measured 200ms+ on a normal machine, which is where the endpoint's latency
went. Cache both probes instead of re-running them per request.

The derived login-shell PATH is a property of the daemon's own environment and
the selected HOME: gxserver cannot observe a profile edit without a restart
anyway, so a successful probe is cached for the process lifetime. A *failed*
probe (missing shell, 2s timeout) is only held for the command TTL so a
transient failure cannot pin an empty PATH for the daemon's whole life.

Per-CLI resolution is cached for 60 seconds, keyed by command plus HOME.
Whether `claude`/`codex`/… is on PATH changes rarely, and the only visible
effect of the window is that a fresh install/removal takes up to a minute to
flip the `cliMissing` status. Negative results are cached for the same 60
seconds on purpose: `resolve_command_path` cannot distinguish "not installed"
from "the probe shell failed", so both share one short window rather than
letting a broken shell be hammered once per launch. The explicit install flow
calls `refresh_resolved_command_path`, which bypasses and refreshes the entry,
so installing hooks right after installing a CLI never sees a stale miss.
*/
const COMMAND_PATH_CACHE_TTL: Duration = Duration::from_secs(60);

struct CachedLoginShellPathEntries {
    fetched_at: Instant,
    entries: Vec<String>,
}

#[derive(PartialEq, Eq, Hash)]
struct CommandPathCacheKey {
    command: String,
    home_dir: PathBuf,
}

struct CachedCommandPath {
    fetched_at: Instant,
    resolved: Option<String>,
}

static LOGIN_SHELL_PATH_ENTRIES_CACHE: OnceLock<
    Mutex<HashMap<PathBuf, CachedLoginShellPathEntries>>,
> = OnceLock::new();
static COMMAND_PATH_CACHE: OnceLock<Mutex<HashMap<CommandPathCacheKey, CachedCommandPath>>> =
    OnceLock::new();

/// Login-shell PATH entries for `home_dir`, probed at most once per process
/// while the probe succeeds. An empty (failed) probe is retried after
/// `COMMAND_PATH_CACHE_TTL`.
pub(super) fn login_shell_path_entries(home_dir: &Path) -> Vec<String> {
    let cache = LOGIN_SHELL_PATH_ENTRIES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(guard) = cache.lock() {
        if let Some(entry) = guard.get(home_dir).filter(|entry| {
            !entry.entries.is_empty() || entry.fetched_at.elapsed() < COMMAND_PATH_CACHE_TTL
        }) {
            return entry.entries.clone();
        }
    }
    let entries = probe_login_shell_path_entries(home_dir);
    if let Ok(mut guard) = cache.lock() {
        guard.insert(
            home_dir.to_path_buf(),
            CachedLoginShellPathEntries {
                fetched_at: Instant::now(),
                entries: entries.clone(),
            },
        );
    }
    entries
}

/// Status-freshness resolution of a provider CLI, served from a 60 second
/// cache. Use `refresh_resolved_command_path` where the caller acts on the
/// result instead of only reporting it.
pub(super) fn cached_resolve_command_path(command: &str, home_dir: &Path) -> Option<String> {
    let key = CommandPathCacheKey {
        command: command.to_string(),
        home_dir: home_dir.to_path_buf(),
    };
    let cache = COMMAND_PATH_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut guard) = cache.lock() {
        guard.retain(|_, entry| entry.fetched_at.elapsed() < COMMAND_PATH_CACHE_TTL);
        if let Some(entry) = guard.get(&key) {
            return entry.resolved.clone();
        }
    }
    probe_and_store_command_path(key, command, home_dir)
}

/// Uncached resolution that also refreshes the cached entry, for callers whose
/// behaviour depends on the CLI being present right now.
pub(super) fn refresh_resolved_command_path(command: &str, home_dir: &Path) -> Option<String> {
    let key = CommandPathCacheKey {
        command: command.to_string(),
        home_dir: home_dir.to_path_buf(),
    };
    probe_and_store_command_path(key, command, home_dir)
}

fn probe_and_store_command_path(
    key: CommandPathCacheKey,
    command: &str,
    home_dir: &Path,
) -> Option<String> {
    let resolved = resolve_command_path(command, home_dir);
    let cache = COMMAND_PATH_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut guard) = cache.lock() {
        guard.insert(
            key,
            CachedCommandPath {
                fetched_at: Instant::now(),
                resolved: resolved.clone(),
            },
        );
    }
    resolved
}
