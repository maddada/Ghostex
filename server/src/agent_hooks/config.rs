use std::path::{Path, PathBuf};

use crate::paths::GxserverPaths;

use super::probing::{normalize_environment_path, unique_path_bufs};

pub(crate) const NOTIFY_HOOK_MARKER: &str = "ghostex-gxserver-agent-notify-hook-marker";
pub(crate) const NOTIFY_HOOK_VERSION: usize = 7;
pub(crate) const OPENCODE_PLUGIN_MARKER: &str = "ghostex-opencode-session-plugin-marker";
pub(crate) const OPENCODE_PLUGIN_SPEC: &str = "./plugins/ghostex-session.js";
pub(crate) const AMP_PLUGIN_MARKER: &str = "ghostex-amp-session-extension-marker";
pub(crate) const PI_EXTENSION_MARKER: &str = "ghostex-pi-session-extension-marker";
pub(crate) const OMP_EXTENSION_MARKER: &str = "ghostex-omp-session-extension-marker";
pub(crate) const SHELL_PATH_SENTINEL: &str = "__GHOSTEX_GXSERVER_SHELL_PATH__";
pub(crate) const GXSERVER_AGENT_HOOK_COLOR_DISABLING_ENVIRONMENT_KEYS: &[&str] =
    &["ANSI_COLORS_DISABLED", "NO_COLOR", "NODE_DISABLE_COLORS"];

pub(crate) struct HookDefinition {
    pub(crate) agent_id: &'static str,
    pub(crate) cli_command: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HookFormat {
    Antigravity,
    FlatJson,
    KiroJson,
    MarkedYaml,
    NestedJson,
    Opencode,
    PluginFile,
}

pub(crate) const HOOK_DEFINITIONS: &[HookDefinition] = &[
    HookDefinition {
        agent_id: "codex",
        cli_command: "codex",
    },
    HookDefinition {
        agent_id: "claude",
        cli_command: "claude",
    },
    HookDefinition {
        agent_id: "cursor",
        cli_command: "cursor-agent",
    },
    HookDefinition {
        agent_id: "gemini",
        cli_command: "gemini",
    },
    HookDefinition {
        agent_id: "kiro",
        cli_command: "kiro-cli",
    },
    HookDefinition {
        agent_id: "copilot",
        cli_command: "copilot",
    },
    HookDefinition {
        agent_id: "droid",
        cli_command: "droid",
    },
    HookDefinition {
        agent_id: "grok",
        cli_command: "grok",
    },
    HookDefinition {
        agent_id: "antigravity",
        cli_command: "agy",
    },
    HookDefinition {
        agent_id: "amp",
        cli_command: "amp",
    },
    HookDefinition {
        agent_id: "omp",
        cli_command: "omp",
    },
    HookDefinition {
        agent_id: "pi",
        cli_command: "pi",
    },
    HookDefinition {
        agent_id: "rovodev",
        cli_command: "acli",
    },
    HookDefinition {
        agent_id: "hermes-agent",
        cli_command: "hermes",
    },
    HookDefinition {
        agent_id: "codebuddy",
        cli_command: "codebuddy",
    },
    HookDefinition {
        agent_id: "qoder",
        cli_command: "qodercli",
    },
    HookDefinition {
        agent_id: "opencode",
        cli_command: "opencode",
    },
];

pub(crate) struct HookPaths {
    pub(crate) home_dir: PathBuf,
    pub(crate) hook_state_directory: PathBuf,
    pub(crate) notify_hook_path: PathBuf,
    pub(crate) respect_config_environment: bool,
}

impl HookPaths {
    pub(crate) fn from_paths(paths: &GxserverPaths) -> Self {
        /*
        CDXC:AgentHookIsolation 2026-08-06-21:35:
        GHOSTEX_HOME and explicit daemon homes are isolated profiles. Their
        hook discovery and writes must stay inside that profile instead of
        following the process HOME or absolute provider-config environment
        variables back into the real user's Claude, Codex, or other agent
        configuration. Production paths continue to honor provider-specific
        environment variables.
        */
        let isolated_home_dir = paths.isolated_agent_home_dir.as_ref();
        Self {
            home_dir: isolated_home_dir
                .cloned()
                .unwrap_or_else(|| paths.home_dir.clone()),
            hook_state_directory: paths.app_state_dir.join("agent-hooks"),
            notify_hook_path: paths.app_data_dir.join("hooks/agent-shell-notify.sh"),
            respect_config_environment: isolated_home_dir.is_none(),
        }
    }

    pub(crate) fn new(home_dir: PathBuf) -> Self {
        Self {
            hook_state_directory: home_dir.join(".ghostexterm"),
            notify_hook_path: home_dir
                .join(".ghostex")
                .join("hooks")
                .join("agent-shell-notify.sh"),
            home_dir,
            respect_config_environment: false,
        }
    }
}

pub(crate) fn hook_format(agent_id: &str) -> HookFormat {
    match agent_id {
        "antigravity" => HookFormat::Antigravity,
        "cursor" => HookFormat::FlatJson,
        "kiro" => HookFormat::KiroJson,
        "rovodev" | "hermes-agent" => HookFormat::MarkedYaml,
        "opencode" => HookFormat::Opencode,
        "amp" | "omp" | "pi" => HookFormat::PluginFile,
        _ => HookFormat::NestedJson,
    }
}

pub(crate) fn hook_marker(agent_id: &str) -> Option<&'static str> {
    match agent_id {
        "amp" => Some(AMP_PLUGIN_MARKER),
        "omp" => Some(OMP_EXTENSION_MARKER),
        "pi" => Some(PI_EXTENSION_MARKER),
        "opencode" => Some(OPENCODE_PLUGIN_MARKER),
        _ => None,
    }
}

pub(crate) fn command_agent(agent_id: &str) -> Option<&'static str> {
    match agent_id {
        "claude" => Some("claude"),
        "cursor" => Some("cursor"),
        "gemini" => Some("gemini"),
        "kiro" => Some("kiro"),
        "copilot" => Some("copilot"),
        "droid" => Some("factory"),
        "grok" => Some("grok"),
        "antigravity" => Some("antigravity"),
        "amp" => Some("amp"),
        "omp" => Some("omp"),
        "pi" => Some("pi"),
        "rovodev" => Some("rovodev"),
        "hermes-agent" => Some("hermes-agent"),
        "codebuddy" => Some("codebuddy"),
        "qoder" => Some("qoder"),
        "opencode" => Some("opencode"),
        _ => None,
    }
}

pub(crate) fn nested_timeout(agent_id: &str) -> Option<i64> {
    match agent_id {
        "codex" | "grok" => Some(5),
        "gemini" => Some(10000),
        _ => None,
    }
}

pub(crate) fn all_hook_events(agent_id: &str) -> Vec<&'static str> {
    let events: &[&str] = match agent_id {
        "codex" => &[
            "SessionStart",
            "UserPromptSubmit",
            "Stop",
            "PreToolUse",
            "PermissionRequest",
        ],
        "claude" => &[
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "Stop",
            "Notification",
            "SessionEnd",
        ],
        "cursor" => &[
            "beforeSubmitPrompt",
            "stop",
            "afterAgentResponse",
            "beforeShellExecution",
            "afterShellExecution",
        ],
        "gemini" => &[
            "SessionStart",
            "BeforeAgent",
            "AfterAgent",
            "SessionEnd",
            "PreToolUse",
        ],
        "kiro" => &[
            "agentSpawn",
            "userPromptSubmit",
            "stop",
            "preToolUse",
            "postToolUse",
        ],
        "copilot" | "droid" | "codebuddy" => &[
            "SessionStart",
            "Stop",
            "Notification",
            "SessionEnd",
            "PreToolUse",
        ],
        "grok" => &[
            "SessionStart",
            "UserPromptSubmit",
            "Stop",
            "Notification",
            "SessionEnd",
            "PreToolUse",
        ],
        "antigravity" => &[
            "SessionStart",
            "PreInvocation",
            "Stop",
            "turn-completion",
            "Notification",
            "SessionEnd",
            "PreToolUse",
            "PostToolUse",
        ],
        "qoder" => &["SessionStart", "Stop", "SessionEnd", "PreToolUse"],
        _ => &[],
    };
    let mut output = Vec::new();
    for event in events {
        if !output.contains(event) {
            output.push(*event);
        }
    }
    output
}

pub(crate) fn resolve_config_directory(
    home_dir: &Path,
    respect_environment: bool,
    env_key: &str,
    fallback_relative_path: &str,
    env_subpath: Option<&str>,
) -> PathBuf {
    let configured_path = respect_environment
        .then(|| std::env::var(env_key).ok())
        .flatten();
    match normalize_environment_path(configured_path.as_deref(), home_dir) {
        Some(path) => env_subpath
            .map(|subpath| path.join(subpath))
            .unwrap_or(path),
        None => home_dir.join(fallback_relative_path),
    }
}

pub(crate) fn resolve_omp_agent_directory(home_dir: &Path, respect_environment: bool) -> PathBuf {
    let pi_agent_root = respect_environment
        .then(|| std::env::var("PI_CODING_AGENT_DIR").ok())
        .flatten();
    if let Some(pi_agent_root) = normalize_environment_path(pi_agent_root.as_deref(), home_dir) {
        return pi_agent_root;
    }
    let configured_path = respect_environment
        .then(|| std::env::var("PI_CONFIG_DIR").ok())
        .flatten();
    let config_dir = normalize_environment_path(configured_path.as_deref(), home_dir)
        .unwrap_or_else(|| home_dir.join(".omp"));
    config_dir.join("agent")
}

pub(crate) fn pi_extension_paths(home_dir: &Path, respect_environment: bool) -> Vec<PathBuf> {
    /*
    CDXC:AgentHooks 2026-06-23-05:09:
    Pi's active extension loader uses the Pi root extensions directory, while
    older Ghostex installs and local customization experiments can leave hooks
    under the agent directory. Treat the root extension file as canonical for
    new installs, but keep inspecting the previous agent-scoped locations so
    existing current hooks do not warn and stale hooks report updateRequired.
    */
    let agent_dir = resolve_config_directory(
        home_dir,
        respect_environment,
        "PI_CODING_AGENT_DIR",
        ".pi/agent",
        None,
    );
    let root_dir = agent_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| home_dir.join(".pi"));
    unique_path_bufs(vec![
        root_dir.join("extensions").join("ghostex-session.ts"),
        agent_dir.join("extensions").join("ghostex-session.ts"),
        agent_dir
            .join("extensions")
            .join("ghostex-session")
            .join("index.ts"),
    ])
}
