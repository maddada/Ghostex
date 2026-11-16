//! Whether Ghostex uses its hooks for Claude, Codex and OpenCode, or follows them from what each
//! agent writes itself (server/src/hookless_activity/), and the agents a fallback put a hook back for.
//!
//! CDXC:AgentHooks 2026-11-12 DECISION:
//! Asked how Ghostex should stop relying on its agent hooks, the user picked "A. Go, with a safety net": "Stop installing the hooks and remove Ghostex's own. Keep the hook code with a 'Use agent hooks' setting. Ghostex switches back to a hook automatically when its watcher can't see a running agent (for example after an agent update changes its files), and says so in the chat." So while Use agent hooks (`useAgentHooks`, off by default) is off, Ghostex installs and repairs no hook for these three agents, removes the ones it wrote (hook_free_cleanup.rs), reports their hook as `notRequired`, and their watchers drive every session; every other agent keeps its hooks. Claude before 2.1.172 writes no session record, so it always keeps its hook. On Codex the user answered earlier (Q935) "Ask to update": Codex 0.162 and older gets no hook either, and its chat asks to update it (hook_free_fallback.rs). Use agent hooks on brings back the hooks for all three, as before.
//! CDXC:AgentHooks 2026-11-12 WHY:
//! It replaces the Debugging switch "Track Claude without its hooks" (`claudeHookFreeActivity`). A saved `true` there asked for what is now the default, so nothing has to move; a saved `false` was that switch's default, which the desktop writes into the settings file for everyone, so it records no wish for hooks and is not carried over.
//! SEE-ALSO: server/src/agent_hooks/api.rs (status and install), server/src/hookless_activity/drive.rs and agent_drive.rs (who drives), server/src/hookless_activity/hook_free_fallback.rs (the safety net), packages/settings-catalog/src/pages/agents.rs (the setting).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::paths::GxserverPaths;

/// The setting: on, Ghostex installs and uses the hooks of the three agents below.
pub(crate) const USE_AGENT_HOOKS: &str = "useAgentHooks";
/// The agents Ghostex follows without hooks while Use agent hooks is off.
pub(crate) const HOOK_FREE_AGENTS: &[&str] = &["claude", "codex", "opencode"];
/// The first Claude Code that writes `<config>/sessions/<pid>.json`.
pub(crate) const CLAUDE_SESSION_RECORDS_SINCE: &str = "2.1.172";
/// The first Codex that reports its state as OSC 7501 (approvals and questions included).
pub(crate) const CODEX_PROGRAM_STATUS_SINCE: &str = "0.163.0";

/// How long one read of the setting is trusted: the hook gate reads it for every hook event.
const SETTING_TTL: Duration = Duration::from_secs(2);
/// How long a CLI version read is trusted before it is read again (agents update themselves).
const VERSION_TTL: Duration = Duration::from_secs(30 * 60);

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

static SETTING: LazyLock<Mutex<HashMap<PathBuf, (Instant, bool)>>> =
    LazyLock::new(Default::default);
/// The installed CLI's version by agent, as last read (`None`: not found or unreadable).
static VERSIONS: LazyLock<Mutex<HashMap<&'static str, (Instant, Option<String>)>>> =
    LazyLock::new(Default::default);

/// Whether Use agent hooks is on. A missing key is the shipped default (off); tests that do not
/// write the key keep the hooks, so the hook tests run as before.
pub(crate) fn use_agent_hooks(paths: &GxserverPaths) -> bool {
    let key = crate::shared_settings_file::shared_settings_path(paths);
    if let Some((at, on)) = lock(&SETTING).get(&key) {
        if at.elapsed() < SETTING_TTL {
            return *on;
        }
    }
    let on = crate::shared_settings_file::read_shared_settings(paths)
        .and_then(|settings| settings.get(USE_AGENT_HOOKS).and_then(Value::as_bool))
        .unwrap_or(cfg!(test));
    lock(&SETTING).insert(key, (Instant::now(), on));
    on
}

/// Forgets the cached setting, so a change made a moment ago is read at once.
pub(crate) fn forget_cached_setting() {
    lock(&SETTING).clear();
}

pub(crate) fn is_hook_free_agent(agent_id: &str) -> bool {
    HOOK_FREE_AGENTS.contains(&agent_id)
}

/// Whether `version` is a release before `since` (an unreadable version is not).
pub(crate) fn version_before(version: &str, since: &str) -> bool {
    crate::agent_cli::latest::version_in(version)
        .is_some_and(|version| crate::agent_cli::latest::is_newer(since, &version))
}

/// The rule on its own: the agent is one of the three, Use agent hooks is off, and it is not a
/// Claude too old to write session records.
pub(crate) fn hook_free_with(
    use_hooks: bool,
    agent_id: &str,
    claude_version: Option<&str>,
) -> bool {
    is_hook_free_agent(agent_id)
        && !use_hooks
        && !(agent_id == "claude"
            && claude_version
                .is_some_and(|version| version_before(version, CLAUDE_SESSION_RECORDS_SINCE)))
}

/// Whether Ghostex follows `agent_id` without its hook on this computer.
pub(crate) fn hook_free(paths: &GxserverPaths, agent_id: &str) -> bool {
    if !is_hook_free_agent(agent_id) {
        return false;
    }
    let claude_version = (agent_id == "claude")
        .then(|| known_cli_version("claude"))
        .flatten();
    hook_free_with(use_agent_hooks(paths), agent_id, claude_version.as_deref())
}

/// The installed CLI's version as last read, without reading it.
pub(crate) fn known_cli_version(agent_id: &str) -> Option<String> {
    lock(&VERSIONS)
        .get(agent_id)
        .and_then(|(_, version)| version.clone())
}

/// Reads the installed CLI's version again when the last read is older than `VERSION_TTL`
/// (blocking: it runs `<cli> --version`). Returns the version.
pub(crate) fn refresh_cli_version(agent_id: &'static str, home: &Path) -> Option<String> {
    if let Some((at, version)) = lock(&VERSIONS).get(agent_id) {
        if at.elapsed() < VERSION_TTL {
            return version.clone();
        }
    }
    let version = read_cli_version(agent_id, home);
    lock(&VERSIONS).insert(agent_id, (Instant::now(), version.clone()));
    version
}

fn read_cli_version(binary: &str, home: &Path) -> Option<String> {
    let executable = crate::agent_cli::process::resolve(binary, home)?;
    let mut command = crate::platform::process::background_command(&executable);
    command.arg("--version").current_dir(home);
    #[cfg(not(windows))]
    command.env(
        "PATH",
        super::probing::normalize_gxserver_process_path(
            std::env::var("PATH").ok().as_deref(),
            home,
        ),
    );
    let output = super::probing::run_command_stdout_with_timeout(command, Duration::from_secs(10))?;
    crate::agent_cli::latest::version_in(&output)
}

#[cfg(test)]
pub(crate) fn set_cli_version_for_test(agent_id: &'static str, version: Option<&str>) {
    lock(&VERSIONS).insert(agent_id, (Instant::now(), version.map(str::to_string)));
}

/// What the safety net decided, kept across restarts in `<state>/agent-hooks/hook-fallbacks.json`.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Fallbacks {
    /// The agents whose hook a fallback installed, which the start cleanup keeps.
    #[serde(default)]
    pub agents: BTreeMap<String, FallbackRecord>,
    /// The sessions their hook drives (`<projectId>:<sessionId>`), each told so once.
    #[serde(default)]
    pub sessions: BTreeMap<String, FallbackRecord>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FallbackRecord {
    pub agent_id: String,
    pub reason: String,
    pub at: String,
}

/// Sessions kept in the file: older ones are long gone or long settled.
const SESSION_LIMIT: usize = 500;

static FALLBACKS: LazyLock<Mutex<Option<(PathBuf, Fallbacks)>>> = LazyLock::new(Default::default);

fn fallbacks_path(paths: &GxserverPaths) -> PathBuf {
    paths
        .app_state_dir
        .join("agent-hooks")
        .join("hook-fallbacks.json")
}

fn with_fallbacks<T>(paths: &GxserverPaths, read: impl FnOnce(&mut Fallbacks) -> T) -> T {
    let path = fallbacks_path(paths);
    let mut cached = lock(&FALLBACKS);
    if cached.as_ref().is_none_or(|(known, _)| *known != path) {
        let loaded = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Fallbacks>(&text).ok())
            .unwrap_or_default();
        *cached = Some((path, loaded));
    }
    let (_, fallbacks) = cached.as_mut().expect("loaded");
    read(fallbacks)
}

fn session_key(project_id: &str, session_id: &str) -> String {
    format!("{project_id}:{session_id}")
}

/// The agents whose hook a fallback installed.
pub(crate) fn fallback_agents(paths: &GxserverPaths) -> HashSet<String> {
    with_fallbacks(paths, |fallbacks| {
        fallbacks.agents.keys().cloned().collect()
    })
}

/// Whether the session fell back to its agent's hook: its hook drives it, its watcher does not.
pub(crate) fn session_fell_back(paths: &GxserverPaths, project_id: &str, session_id: &str) -> bool {
    with_fallbacks(paths, |fallbacks| {
        fallbacks
            .sessions
            .contains_key(&session_key(project_id, session_id))
    })
}

/// Records a session's fallback (and its agent's). Returns `false` when the session already had one.
pub(crate) fn record_fallback(
    paths: &GxserverPaths,
    agent_id: &str,
    project_id: &str,
    session_id: &str,
    reason: &str,
) -> bool {
    let record = FallbackRecord {
        agent_id: agent_id.to_string(),
        reason: reason.to_string(),
        at: super::probing::now_iso(),
    };
    let snapshot = with_fallbacks(paths, |fallbacks| {
        let key = session_key(project_id, session_id);
        if fallbacks.sessions.contains_key(&key) {
            return None;
        }
        fallbacks.sessions.insert(key, record.clone());
        fallbacks
            .agents
            .entry(agent_id.to_string())
            .or_insert(record);
        while fallbacks.sessions.len() > SESSION_LIMIT {
            let oldest = fallbacks
                .sessions
                .iter()
                .min_by(|a, b| a.1.at.cmp(&b.1.at))
                .map(|(key, _)| key.clone());
            match oldest {
                Some(key) => fallbacks.sessions.remove(&key),
                None => break,
            };
        }
        Some(fallbacks.clone())
    });
    let Some(snapshot) = snapshot else {
        return false;
    };
    let path = fallbacks_path(paths);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(&snapshot) {
        let temp = super::probing::temp_path_for(&path);
        if std::fs::write(&temp, text).is_ok() {
            let _ = std::fs::rename(&temp, &path);
        }
    }
    true
}

/// Forgets every fallback (Use agent hooks turned on: the hooks drive everything again).
pub(crate) fn clear_fallbacks(paths: &GxserverPaths) {
    with_fallbacks(paths, |fallbacks| *fallbacks = Fallbacks::default());
    let _ = std::fs::remove_file(fallbacks_path(paths));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_claude_codex_and_opencode_go_without_hooks_and_only_while_the_setting_is_off() {
        for agent in ["claude", "codex", "opencode"] {
            assert!(hook_free_with(false, agent, None), "{agent}");
            assert!(!hook_free_with(true, agent, None), "{agent}");
        }
        for agent in ["pi", "cursor", "omp"] {
            assert!(!hook_free_with(false, agent, None), "{agent}");
        }
    }

    #[test]
    fn a_claude_without_session_records_keeps_its_hook() {
        assert!(!hook_free_with(
            false,
            "claude",
            Some("2.1.171 (Claude Code)")
        ));
        assert!(hook_free_with(
            false,
            "claude",
            Some("2.1.172 (Claude Code)")
        ));
        assert!(hook_free_with(
            false,
            "claude",
            Some("2.1.296 (Claude Code)")
        ));
        // An unreadable version is not an old one.
        assert!(hook_free_with(false, "claude", Some("unknown")));
        assert!(version_before(
            "codex-cli 0.162.1",
            CODEX_PROGRAM_STATUS_SINCE
        ));
        assert!(!version_before(
            "codex-cli 0.163.0",
            CODEX_PROGRAM_STATUS_SINCE
        ));
    }

    #[test]
    fn the_setting_is_read_from_the_settings_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
        // No key in a test keeps the hooks (the shipped default is off, see `use_agent_hooks`).
        assert!(use_agent_hooks(&paths));
        std::fs::create_dir_all(&paths.app_config_dir).expect("dir");
        std::fs::write(
            crate::shared_settings_file::shared_settings_path(&paths),
            r#"{"useAgentHooks": false}"#,
        )
        .expect("settings");
        forget_cached_setting();
        assert!(!use_agent_hooks(&paths));
        assert!(hook_free(&paths, "codex"));
        assert!(!hook_free(&paths, "pi"));
    }

    #[test]
    fn a_session_falls_back_once_and_its_agent_is_remembered() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
        assert!(!session_fell_back(&paths, "P1", "G1"));
        assert!(record_fallback(&paths, "claude", "P1", "G1", "no record"));
        assert!(!record_fallback(&paths, "claude", "P1", "G1", "no record"));
        assert!(session_fell_back(&paths, "P1", "G1"));
        assert!(!session_fell_back(&paths, "P1", "G2"));
        assert_eq!(
            fallback_agents(&paths),
            HashSet::from(["claude".to_string()])
        );
        let saved: Fallbacks =
            serde_json::from_str(&std::fs::read_to_string(fallbacks_path(&paths)).expect("file"))
                .expect("json");
        assert_eq!(saved.sessions["P1:G1"].reason, "no record");
        clear_fallbacks(&paths);
        assert!(fallback_agents(&paths).is_empty());
    }
}
