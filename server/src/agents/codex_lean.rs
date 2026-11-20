//! Lean Codex: Codex started without its own computer use, browser use, image generation, apps,
//! plugins and skill search, for work that never needs them.
//!
//! CDXC:AgentProviders 2026-11-20 DECISION:
//! The user picked option B of the "Codex dry mode" research (Pietro Schirano's `codex exec`
//! subagent command, measured at about 3.2k fewer prompt tokens per request, about 12%, and about
//! 1.7s faster to start, all of it from the feature flags): Ghostex's own background Codex calls
//! (title generation, Catch me up, commit messages) always pass `--disable` for these features,
//! and "Lean Codex workers" (Settings > Agents, off by default) gives Codex threads a coordinator
//! starts the same flags; sessions the user starts never get them. `plugin_discovery_prefetch` is
//! left out (removed upstream, so it does nothing) and so is the post's `-c 'mcp_servers={}'`:
//! Codex merges `-c` tables key by key, so an empty table turns off no MCP server.
//! WHY: Codex refuses to start on a feature name it does not know ("Unknown feature flag"), so only
//! the names the installed Codex lists in `codex features list` are passed, read once per Codex
//! version (`codex --version`, checked at most once a minute); no list, no flags.
//! SEE-ALSO: server/src/agents/launch_plan.rs and resume_plan.rs (the launch hook),
//! server/src/server/title_generation/title_command.rs and commit_message_generation.rs (the
//! background calls), server/src/ghostex_cli/coordinator/threads.rs (`start-thread` marks the
//! create), apps/desktop/src/app/window/settings_modal/tabs/agents/model.rs (the title command
//! preview names the same features), packages/settings-catalog/src/pages/agents.rs (the row).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{Map, Value};

/// The Codex features a lean Codex runs without.
pub(crate) const LEAN_CODEX_FEATURES: [&str; 11] = [
    "plugins",
    "apps",
    "remote_plugin",
    "plugin_sharing",
    "browser_use",
    "browser_use_external",
    "computer_use",
    "image_generation",
    "in_app_browser",
    "skill_search",
    "skill_mcp_dependency_install",
];

/// Settings > Agents > Lean Codex workers.
pub(crate) const LEAN_CODEX_WORKERS_SETTING: &str = "leanCodexWorkers";

/// The create param `coordinator start-thread` sends, and the runtime setting that keeps it on the
/// session: this session was started as a coordinator's thread.
pub(crate) const COORDINATOR_THREAD_KEY: &str = "coordinatorThread";

const VERSION_CHECK_INTERVAL: Duration = Duration::from_secs(60);
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

struct KnownFeatures {
    checked_at: Instant,
    version: String,
    flags: String,
}

/// ` --disable <feature>` for each lean feature the installed Codex knows, or an empty string when
/// Codex can't be asked.
pub(crate) fn lean_codex_flags() -> String {
    // Unit tests assert exact commands, which must not depend on the Codex installed here.
    if cfg!(test) {
        return String::new();
    }
    static CACHE: Mutex<Option<KnownFeatures>> = Mutex::new(None);
    let mut cache = CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(known) = cache
        .as_ref()
        .filter(|known| known.checked_at.elapsed() < VERSION_CHECK_INTERVAL)
    {
        return known.flags.clone();
    }
    let Some(version) = run_installed_codex(&["--version"])
        .map(|output| output.trim().to_string())
        .filter(|version| !version.is_empty())
    else {
        return String::new();
    };
    if let Some(known) = cache.as_mut().filter(|known| known.version == version) {
        known.checked_at = Instant::now();
        return known.flags.clone();
    }
    let Some(listed) = run_installed_codex(&["features", "list"]) else {
        return String::new();
    };
    let flags = lean_flags_from_feature_list(&listed);
    eprintln!("[codex-lean] {version}: background calls and lean workers run with{flags}");
    *cache = Some(KnownFeatures {
        checked_at: Instant::now(),
        version,
        flags: flags.clone(),
    });
    flags
}

/// The flags for the lean features `codex features list` names (its first column).
fn lean_flags_from_feature_list(listed: &str) -> String {
    let known = listed
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .collect::<Vec<_>>();
    LEAN_CODEX_FEATURES
        .iter()
        .filter(|feature| known.contains(feature))
        .map(|feature| format!(" --disable {feature}"))
        .collect()
}

/// Whether "Lean Codex workers" is on (off by default).
fn lean_codex_workers_enabled() -> bool {
    crate::session_lifecycle::read_sidebar_settings(&crate::paths::get_gxserver_paths(None))
        .and_then(|settings| {
            settings
                .get(LEAN_CODEX_WORKERS_SETTING)
                .and_then(Value::as_bool)
        })
        .unwrap_or(false)
}

/// Whether a session was started as a coordinator's thread (`COORDINATOR_THREAD_KEY`).
pub(crate) fn is_coordinator_thread(runtime_settings: Option<&Map<String, Value>>) -> bool {
    runtime_settings
        .and_then(|settings| settings.get(COORDINATOR_THREAD_KEY))
        .and_then(Value::as_bool)
        == Some(true)
}

/// `command` with the lean flags when it starts Codex for a coordinator's thread while "Lean Codex
/// workers" is on; every other command as it is.
pub(crate) fn with_lean_codex_worker(
    family: Option<&str>,
    runtime_settings: Option<&Map<String, Value>>,
    command: &str,
) -> String {
    if cfg!(test)
        || family != Some("codex")
        || command.trim().is_empty()
        || !is_coordinator_thread(runtime_settings)
        || command.contains("--disable plugins")
        || !lean_codex_workers_enabled()
    {
        return command.to_string();
    }
    format!("{}{}", command.trim_end(), lean_codex_flags())
}

/// The installed `codex`'s stdout for `args`, the way the `--no-daemon` probe finds it
/// (codex_daemon.rs): never through an install-on-demand wrapper.
fn run_installed_codex(args: &[&str]) -> Option<String> {
    #[cfg(windows)]
    {
        let executable = crate::platform::live_path::find("codex", &[]).and_then(|executable| {
            crate::agent_hooks::on_demand::probe_target_path(
                executable,
                "codex",
                &crate::resume_lookup::home_dir(),
            )
        })?;
        let mut command = crate::platform::process::background_command(executable);
        command.args(args);
        crate::agent_hooks::probing::run_command_stdout_with_timeout(command, PROBE_TIMEOUT)
    }
    #[cfg(not(windows))]
    {
        let home = crate::resume_lookup::home_dir();
        let codex = crate::agent_hooks::probing::resolve_probe_command("codex", &home)?;
        let script = std::iter::once(crate::agent_hooks::plugin_sources::shell_quote(&codex))
            .chain(args.iter().map(|arg| arg.to_string()))
            .collect::<Vec<_>>()
            .join(" ");
        crate::agent_hooks::probing::cli_script_stdout(&script, &home, PROBE_TIMEOUT)
    }
}
