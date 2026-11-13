use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
    time::SystemTime,
};

use serde_json::Value;

use crate::paths::GxserverPaths;

use super::config::{
    pi_extension_paths, resolve_config_directory, resolve_omp_agent_directory, HookPaths,
    AMP_PLUGIN_MARKER, HOOK_DEFINITIONS, OMP_EXTENSION_MARKER, OPENCODE_PLUGIN_MARKER,
    PI_EXTENSION_MARKER,
};
use super::install::read_json_object;
use super::probing::{list_profile_hook_paths, normalize_environment_path, read_file_text};

#[derive(Clone, Debug)]
pub(crate) struct CodexHookSessionIdentity {
    pub agent_session_id: String,
    pub agent_session_path: Option<String>,
}

/// CDXC:AgentHooks 2026-09-05 WHY:
/// Codex subagent tool hooks inherit the parent terminal routing and can report the parent's session id alongside the child's transcript path.
/// A child PreToolUse reopened a completed parent turn as working indefinitely; use the rollout's own source metadata before accepting any status or identity writes.
pub(crate) fn is_codex_subagent_transcript(transcript_path: Option<&str>) -> bool {
    transcript_path
        .map(super::probing::expand_home_path)
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(".jsonl"))
        })
        .and_then(|path| crate::session_chat_successor::read_codex_session_meta(&path))
        .is_some_and(|meta| meta.is_subagent)
}

/// Each zmx surface's newest Codex hook identity from `codex-hook-sessions.json`, read again only
/// when the file's modification time or size changes.
///
/// CDXC:AgentHooks 2026-11-10 WHY: the live zmx identity pass parsed the whole file (505 KB on the
/// profiled Mac) on every run, 124 of 3,605 busy samples of gxserver; it changes only when a
/// Codex hook fires, so its parsed identities are kept under its stamp.
pub(crate) fn read_codex_hook_session_identities(
    paths: &GxserverPaths,
) -> Arc<HashMap<String, CodexHookSessionIdentity>> {
    type Stamp = (Option<SystemTime>, u64);
    static MEMO: OnceLock<
        Mutex<
            Option<(
                PathBuf,
                Stamp,
                Arc<HashMap<String, CodexHookSessionIdentity>>,
            )>,
        >,
    > = OnceLock::new();
    let store_path = paths
        .app_state_dir
        .join("agent-hooks")
        .join("codex-hook-sessions.json");
    let stamp = std::fs::metadata(&store_path)
        .ok()
        .map(|meta| (meta.modified().ok(), meta.len()));
    let memo = MEMO.get_or_init(Default::default);
    if let (Some(stamp), Ok(memo)) = (stamp, memo.lock()) {
        if let Some((_, _, identities)) = memo
            .as_ref()
            .filter(|(path, known, _)| *path == store_path && *known == stamp)
        {
            return identities.clone();
        }
    }
    let identities = Arc::new(parse_codex_hook_session_identities(&store_path));
    if let (Some(stamp), Ok(mut memo)) = (stamp, memo.lock()) {
        *memo = Some((store_path, stamp, identities.clone()));
    }
    identities
}

fn parse_codex_hook_session_identities(
    store_path: &std::path::Path,
) -> HashMap<String, CodexHookSessionIdentity> {
    let data = read_json_object(&read_file_text(store_path));
    let Some(sessions) = data.get("sessions").and_then(Value::as_object) else {
        return HashMap::new();
    };
    let mut latest_by_surface: HashMap<String, (f64, CodexHookSessionIdentity)> = HashMap::new();
    for entry in sessions.values().filter_map(Value::as_object) {
        let Some(surface_id) = entry
            .get("surfaceId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(agent_session_id) = entry
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let updated_at = entry
            .get("updatedAt")
            .and_then(Value::as_f64)
            .unwrap_or_default();
        if latest_by_surface
            .get(surface_id)
            .is_some_and(|(current_updated_at, _)| *current_updated_at > updated_at)
        {
            continue;
        }
        let agent_session_path = entry
            .get("transcriptPath")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        latest_by_surface.insert(
            surface_id.to_string(),
            (
                updated_at,
                CodexHookSessionIdentity {
                    agent_session_id: agent_session_id.to_string(),
                    agent_session_path,
                },
            ),
        );
    }
    latest_by_surface
        .into_iter()
        .map(|(surface_id, (_, identity))| (surface_id, identity))
        .collect()
}

#[derive(Clone, Debug)]
pub(crate) struct ClaudeHookSurfaceRecord {
    pub agent_session_id: String,
    pub agent_session_path: Option<String>,
    pub updated_at: f64,
}

/// Every Claude conversation id the hook store saw in one terminal surface, oldest first.
pub(crate) fn read_claude_hook_surface_records(
    paths: &GxserverPaths,
    surface_id: &str,
) -> Vec<ClaudeHookSurfaceRecord> {
    let store_path = paths
        .app_state_dir
        .join("agent-hooks")
        .join("claude-hook-sessions.json");
    let data = read_json_object(&read_file_text(&store_path));
    let Some(sessions) = data.get("sessions").and_then(Value::as_object) else {
        return Vec::new();
    };
    let text = |entry: &serde_json::Map<String, Value>, key: &str| {
        entry
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let mut records: Vec<ClaudeHookSurfaceRecord> = sessions
        .values()
        .filter_map(Value::as_object)
        .filter(|entry| text(entry, "surfaceId").as_deref() == Some(surface_id))
        .filter_map(|entry| {
            Some(ClaudeHookSurfaceRecord {
                agent_session_id: text(entry, "sessionId")?,
                agent_session_path: text(entry, "transcriptPath"),
                updated_at: entry
                    .get("updatedAt")
                    .and_then(Value::as_f64)
                    .unwrap_or_default(),
            })
        })
        .collect();
    records.sort_by(|left, right| left.updated_at.total_cmp(&right.updated_at));
    records
}

/// The folder Codex uses: CODEX_HOME (when this profile reads the environment), else its default
/// `.codex`, which on Windows sits in the Profile folder whatever HOME says
/// (`ghostex_paths::agent_homes`).
pub(crate) fn codex_home(hook_paths: &HookPaths) -> PathBuf {
    hook_paths
        .respect_config_environment
        .then(|| std::env::var("CODEX_HOME").ok())
        .flatten()
        .and_then(|value| normalize_environment_path(Some(&value), &hook_paths.home_dir))
        .unwrap_or_else(|| ghostex_paths::agent_homes::codex_default_home(&hook_paths.home_dir))
}

pub(crate) fn provider_hook_paths(agent_id: &str, hook_paths: &HookPaths) -> Vec<PathBuf> {
    match agent_id {
        "codex" => {
            let mut paths = vec![codex_home(hook_paths).join("hooks.json")];
            paths.extend(list_profile_hook_paths(
                &hook_paths.home_dir,
                ".codex-profiles",
                "hooks.json",
            ));
            paths
        }
        "claude" => {
            let mut paths =
                vec![
                    ghostex_paths::agent_homes::claude_default_home(&hook_paths.home_dir)
                        .join("settings.json"),
                ];
            paths.extend(list_profile_hook_paths(
                &hook_paths.home_dir,
                ".claude-profiles",
                "settings.json",
            ));
            paths
        }
        "cursor" => vec![hook_paths.home_dir.join(".cursor").join("hooks.json")],
        "zcode" => vec![super::zcode::zcode_hook_config_path(&hook_paths.home_dir)],
        "opencode" => {
            let config_dir = resolve_config_directory(
                &hook_paths.home_dir,
                hook_paths.respect_config_environment,
                "OPENCODE_CONFIG_DIR",
                ".config/opencode",
                None,
            );
            vec![
                config_dir.join("plugins/ghostex-v2/tui.js"),
                config_dir.join("cli.json"),
            ]
        }
        "amp" => vec![hook_paths
            .home_dir
            .join(".config")
            .join("amp")
            .join("plugins")
            .join("ghostex-session.ts")],
        "pi" => pi_extension_paths(&hook_paths.home_dir, hook_paths.respect_config_environment),
        "omp" => vec![resolve_omp_agent_directory(
            &hook_paths.home_dir,
            hook_paths.respect_config_environment,
        )
        .join("extensions")
        .join("ghostex-omp-session.ts")],
        "grok" => {
            vec![resolve_config_directory(
                &hook_paths.home_dir,
                hook_paths.respect_config_environment,
                "GROK_HOME",
                ".grok/hooks",
                None,
            )
            .join("ghostex-session.json")]
        }
        "antigravity" => vec![hook_paths
            .home_dir
            .join(".gemini")
            .join("config")
            .join("hooks.json")],
        "kiro" => vec![resolve_config_directory(
            &hook_paths.home_dir,
            hook_paths.respect_config_environment,
            "KIRO_HOME",
            ".kiro/agents",
            Some("agents"),
        )
        .join("ghostex.json")],
        "copilot" => {
            vec![resolve_config_directory(
                &hook_paths.home_dir,
                hook_paths.respect_config_environment,
                "COPILOT_HOME",
                ".copilot",
                None,
            )
            .join("config.json")]
        }
        "droid" => vec![hook_paths.home_dir.join(".factory").join("settings.json")],
        "rovodev" => vec![hook_paths.home_dir.join(".rovodev").join("config.yml")],
        /*
        CDXC:AgentHooks 2026-09-27 WHY:
        A `hermes -p <name>` session reads only `profiles/<name>/config.yaml`, so hooks in the root config never fire for a bot and its chat stays blank.
        Only profiles that already have a config are listed, so no config is ever created for a folder Hermes does not treat as a profile.
        */
        "hermes-agent" => {
            let hermes_home = resolve_config_directory(
                &hook_paths.home_dir,
                hook_paths.respect_config_environment,
                "HERMES_HOME",
                ".hermes",
                None,
            );
            let mut paths = vec![hermes_home.join("config.yaml")];
            paths.extend(
                list_profile_hook_paths(&hermes_home, "profiles", "config.yaml")
                    .into_iter()
                    .filter(|path| path.is_file()),
            );
            paths
        }
        "codebuddy" => vec![resolve_config_directory(
            &hook_paths.home_dir,
            hook_paths.respect_config_environment,
            "CODEBUDDY_CONFIG_DIR",
            ".codebuddy",
            None,
        )
        .join("settings.json")],
        "qoder" => {
            vec![resolve_config_directory(
                &hook_paths.home_dir,
                hook_paths.respect_config_environment,
                "QODER_CONFIG_DIR",
                ".qoder",
                None,
            )
            .join("settings.json")]
        }
        "empryo" => vec![super::config::empryo_home(hook_paths).join("hooks.json")],
        /*
        Only the primary `config.toml` layout is managed. Kimi Code's legacy
        `~/.kimi` tree is deliberately left alone: writing both would install
        the same hooks twice for anyone mid-migration.
        */
        "kimi" => vec![resolve_config_directory(
            &hook_paths.home_dir,
            hook_paths.respect_config_environment,
            "KIMI_CODE_HOME",
            ".kimi-code",
            None,
        )
        .join("config.toml")],
        "mastra" => vec![hook_paths.home_dir.join(".mastracode/hooks.json")],
        "openclaude" => vec![hook_paths
            .home_dir
            .join(".openclaude")
            .join("settings.json")],
        "command-code" => vec![hook_paths
            .home_dir
            .join(".commandcode")
            .join("settings.json")],
        "devin" => vec![hook_paths
            .home_dir
            .join(".config")
            .join("devin")
            .join("config.json")],
        _ => Vec::new(),
    }
}

pub(crate) fn is_ghostex_owned_hook_command(value: &Value, command: &str) -> bool {
    let Some(command_value) = value.get("command").and_then(Value::as_str) else {
        return false;
    };
    command_value == command || text_contains_ghostex_owned_hook_command(command_value)
}

pub(crate) fn text_contains_ghostex_owned_hook_command(text: &str) -> bool {
    let normalized = text.to_ascii_lowercase();
    normalized.contains("agent-hook-notify-native")
        || normalized.contains("agent-shell-notify")
        || normalized.contains(".ghostex/hooks")
        || normalized.contains(".ghostexterm")
        || normalized.contains("ghostex_notify_hook")
        || normalized.contains("ghostex-agent-notify")
        || normalized.contains(AMP_PLUGIN_MARKER)
        || normalized.contains(OMP_EXTENSION_MARKER)
        || normalized.contains(PI_EXTENSION_MARKER)
        || normalized.contains(OPENCODE_PLUGIN_MARKER)
        || normalized.contains("ghostex-session-plugin-marker")
        || normalized.contains("ghostex-session-extension-marker")
}

pub(crate) fn normalize_agent_ids(value: Option<&Value>) -> Vec<String> {
    let requested = value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(normalize_requested_agent_id)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            HOOK_DEFINITIONS
                .iter()
                .map(|definition| definition.agent_id.to_string())
                .collect()
        });
    let mut output = Vec::new();
    for agent_id in requested {
        if HOOK_DEFINITIONS
            .iter()
            .any(|definition| definition.agent_id == agent_id)
            && !output.contains(&agent_id)
        {
            output.push(agent_id);
        }
    }
    if output.is_empty() {
        HOOK_DEFINITIONS
            .iter()
            .map(|definition| definition.agent_id.to_string())
            .collect()
    } else {
        output
    }
}

fn normalize_requested_agent_id(value: &Value) -> Option<String> {
    let normalized = value
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mapped = match normalized.as_str() {
        "agy" | "antigravity cli" => "antigravity",
        "mastracode" | "mastra code" => "mastra",
        "claude code" => "claude",
        "code buddy" => "codebuddy",
        "codex cli" => "codex",
        "command code" | "commandcode" => "command-code",
        "cursor agent" | "cursor cli" | "cursor-agent" => "cursor",
        "factory" | "factory droid" => "droid",
        "github copilot" => "copilot",
        "kimi code" => "kimi",
        "kiro cli" | "kiro-cli" => "kiro",
        "open claude" | "openclaude cli" => "openclaude",
        "open code" => "opencode",
        "qodercli" => "qoder",
        "rovo" | "rovo dev" => "rovodev",
        other => other,
    };
    (!mapped.is_empty()).then_some(mapped.to_string())
}
