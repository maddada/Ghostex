use std::path::{Path, PathBuf};

use crate::paths::GxserverPaths;

use super::probing::{normalize_environment_path, unique_path_bufs};

pub(crate) const NOTIFY_HOOK_MARKER: &str = "ghostex-gxserver-agent-notify-hook-marker";
pub(crate) const NOTIFY_HOOK_VERSION: usize = 9;
pub(crate) const STATUSLINE_HOOK_MARKER: &str = "ghostex-gxserver-agent-statusline-hook-marker";
pub(crate) const STATUSLINE_HOOK_VERSION: usize = 1;
pub(crate) const OPENCODE_PLUGIN_MARKER: &str = "ghostex-opencode-session-plugin-marker";
pub(crate) const OPENCODE_PLUGIN_SPEC: &str = "./plugins/ghostex-session.js";
pub(crate) const AMP_PLUGIN_MARKER: &str = "ghostex-amp-session-extension-marker";
pub(crate) const PI_EXTENSION_MARKER: &str = "ghostex-pi-session-extension-marker";
pub(crate) const OMP_EXTENSION_MARKER: &str = "ghostex-omp-session-extension-marker";
pub(crate) const CAMPFIRE_EXTENSION_MARKER: &str = "ghostex-campfire-session-extension-marker";
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
    /// Event arrays at the JSON root, as used by Mastra Code.
    RootFlatJson,
    KiroJson,
    MarkedYaml,
    NestedJson,
    Opencode,
    PluginFile,
    /// A Ghostex-owned `# ghostex hooks <agent> begin/end` block inside a TOML
    /// config. TOML comments use the same `#` syntax as YAML, so this shares
    /// the marked-block install/uninstall machinery with [`HookFormat::MarkedYaml`]
    /// and differs only in the body it writes between the markers.
    TomlMarked,
}

pub(crate) const HOOK_DEFINITIONS: &[HookDefinition] = &[
    HookDefinition {
        agent_id: "mastra",
        cli_command: "mastracode",
    },
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
    HookDefinition {
        agent_id: "kimi",
        cli_command: "kimi",
    },
    HookDefinition {
        agent_id: "campfire",
        cli_command: "campfire",
    },
    HookDefinition {
        agent_id: "openclaude",
        cli_command: "openclaude",
    },
    HookDefinition {
        agent_id: "command-code",
        cli_command: "commandcode",
    },
    HookDefinition {
        agent_id: "devin",
        cli_command: "devin",
    },
];

pub(crate) struct HookPaths {
    pub(crate) home_dir: PathBuf,
    pub(crate) hook_state_directory: PathBuf,
    pub(crate) notify_hook_path: PathBuf,
    /// The Claude Code `statusLine` command Ghostex installs beside the notify
    /// hook (CDXC:AgentHooks 2026-09-03, `statusline.rs`).
    pub(crate) statusline_hook_path: PathBuf,
    pub(crate) respect_config_environment: bool,
}

impl HookPaths {
    pub(crate) fn from_paths(paths: &GxserverPaths) -> Self {
        /*
        CDXC:AgentHooks 2026-08-06-21:35:
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
            statusline_hook_path: paths.app_data_dir.join("hooks/agent-statusline.sh"),
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
            statusline_hook_path: home_dir
                .join(".ghostex")
                .join("hooks")
                .join("agent-statusline.sh"),
            home_dir,
            respect_config_environment: false,
        }
    }
}

pub(crate) fn hook_format(agent_id: &str) -> HookFormat {
    match agent_id {
        "antigravity" => HookFormat::Antigravity,
        "cursor" => HookFormat::FlatJson,
        "mastra" => HookFormat::RootFlatJson,
        "kiro" => HookFormat::KiroJson,
        "rovodev" | "hermes-agent" => HookFormat::MarkedYaml,
        "kimi" => HookFormat::TomlMarked,
        "opencode" => HookFormat::Opencode,
        "amp" | "omp" | "pi" | "campfire" => HookFormat::PluginFile,
        _ => HookFormat::NestedJson,
    }
}

pub(crate) fn hook_marker(agent_id: &str) -> Option<&'static str> {
    match agent_id {
        "amp" => Some(AMP_PLUGIN_MARKER),
        "omp" => Some(OMP_EXTENSION_MARKER),
        "pi" => Some(PI_EXTENSION_MARKER),
        "campfire" => Some(CAMPFIRE_EXTENSION_MARKER),
        "opencode" => Some(OPENCODE_PLUGIN_MARKER),
        _ => None,
    }
}

pub(crate) fn command_agent(agent_id: &str) -> Option<&'static str> {
    match agent_id {
        "mastra" => Some("mastra"),
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
        "kimi" => Some("kimi"),
        "campfire" => Some("campfire"),
        "openclaude" => Some("openclaude"),
        "command-code" => Some("command-code"),
        "devin" => Some("devin"),
        _ => None,
    }
}

/*
CDXC:AgentHooks 2026-08-27:
Codex CLI silently clamps the Interrupt hook timeout to 3 seconds and prints
"⚠ clamping Interrupt hook timeout to 3s in ~/.codex/hooks.json" on every run.
Writing the clamped value ourselves keeps the warning off the user's terminal
without changing any other Codex hook's 5s budget.
*/
pub(crate) const CODEX_INTERRUPT_HOOK_TIMEOUT_SECONDS: i64 = 3;

pub(crate) fn nested_timeout(agent_id: &str) -> Option<i64> {
    match agent_id {
        "codex" | "grok" => Some(5),
        "command-code" | "devin" => Some(10),
        "gemini" => Some(10000),
        // `openclaude` is deliberately absent: OpenClaude is a Claude-shaped
        // settings.json, and Claude has no entry either, so both fall into the
        // same 5000 default and stay in lockstep.
        _ => None,
    }
}

/// Per-event override on top of [`nested_timeout`], for providers that cap a
/// single event's timeout below their general budget.
pub(crate) fn nested_event_timeout(agent_id: &str, event_name: &str) -> Option<i64> {
    if agent_id == "codex" && event_name == "Interrupt" {
        return Some(CODEX_INTERRUPT_HOOK_TIMEOUT_SECONDS);
    }
    nested_timeout(agent_id)
}

pub(crate) fn all_hook_events(agent_id: &str) -> Vec<&'static str> {
    let events: &[&str] = match agent_id {
        "mastra" => &[
            "SessionStart",
            "SessionEnd",
            "UserPromptSubmit",
            "AgentStart",
            "AgentEnd",
            "PreToolUse",
            "PostToolUse",
            "PermissionRequest",
            "PermissionResult",
            "Interrupt",
            "Notification",
            "SubagentStart",
            "SubagentEnd",
        ],
        // Codex deliberately has no PreCompact/PostCompact registration: its
        // compaction lifecycle does not carry the trigger metadata needed to
        // tell a mid-turn auto-compact from a manual one.
        "codex" => &[
            "SessionStart",
            "UserPromptSubmit",
            "Stop",
            "Interrupt",
            "PreToolUse",
            "PermissionRequest",
            "PostToolUse",
            "SubagentStart",
            "SubagentStop",
        ],
        /*
        PreCompact is deliberately absent: it fires BEFORE the compact is
        validated, and an aborted compact emits it alone, so mapping it to
        working would strand the pane. PostCompact is registered instead and
        only counts when its trigger is manual.
        */
        // OpenClaude ships Claude's hook contract verbatim (same
        // `~/.openclaude/settings.json` shape, same event names), so the two
        // deliberately share one catalog and must never drift apart.
        "claude" | "openclaude" => &[
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "Stop",
            "Notification",
            "SessionEnd",
            "StopFailure",
            "SubagentStart",
            "SubagentStop",
            "TeammateIdle",
            "PostToolUse",
            "PostToolUseFailure",
            "PermissionRequest",
            "PostCompact",
        ],
        "cursor" => &[
            "beforeSubmitPrompt",
            "stop",
            "afterAgentResponse",
            "beforeShellExecution",
            "afterShellExecution",
            "preToolUse",
            "postToolUse",
            "postToolUseFailure",
            "beforeMCPExecution",
        ],
        // Gemini's real pre-tool event is BeforeTool; the PreToolUse name it
        // used to be registered under was never delivered.
        "gemini" => &[
            "SessionStart",
            "BeforeAgent",
            "AfterAgent",
            "SessionEnd",
            "BeforeTool",
            "AfterTool",
        ],
        "kiro" => &[
            "agentSpawn",
            "userPromptSubmit",
            "stop",
            "preToolUse",
            "postToolUse",
        ],
        // `subagentStart` is camelCase on purpose: GitHub documents the
        // camelCase payload shape for Copilot's subagent-start event.
        "copilot" => &[
            "SessionStart",
            "Stop",
            "Notification",
            "SessionEnd",
            "PreToolUse",
            "UserPromptSubmit",
            "PostToolUse",
            "PostToolUseFailure",
            "subagentStart",
            "SubagentStop",
            "PreCompact",
            "ErrorOccurred",
            "PermissionRequest",
        ],
        "droid" => &[
            "SessionStart",
            "Stop",
            "Notification",
            "SessionEnd",
            "PreToolUse",
            "UserPromptSubmit",
            "SubagentStop",
            "PostToolUse",
            "PermissionRequest",
        ],
        "codebuddy" => &[
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
            "StopFailure",
            "PostToolUse",
            "PostToolUseFailure",
        ],
        "antigravity" => &[
            "SessionStart",
            "PreInvocation",
            "PostInvocation",
            "Stop",
            "turn-completion",
            "Notification",
            "SessionEnd",
            "PreToolUse",
            "PostToolUse",
        ],
        "qoder" => &["SessionStart", "Stop", "SessionEnd", "PreToolUse"],
        // Kimi Code writes one `[[hooks]]` table per event and treats `matcher`
        // as a regex, so Ghostex registers each event without a matcher.
        "kimi" => &[
            "SessionStart",
            "UserPromptSubmit",
            "Notification",
            "Stop",
            "StopFailure",
            "SessionEnd",
            "PreToolUse",
            "PostToolUse",
            "PostToolUseFailure",
            "PermissionRequest",
        ],
        "command-code" => &["PreToolUse", "PostToolUse", "Stop"],
        // `PostCompaction` is Devin's own spelling — not Claude's PostCompact.
        "devin" => &[
            "SessionStart",
            "UserPromptSubmit",
            "Stop",
            "PostCompaction",
            "SessionEnd",
            "PreToolUse",
            "PostToolUse",
            "PermissionRequest",
        ],
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
    CDXC:AgentHooks 2026-08-24:
    Pi's extension loader reads ONLY the agent config directory
    (`getAgentDir()/extensions`, i.e. `~/.pi/agent/extensions/`) plus the
    project-local `.pi/extensions` — verified against pi 0.80.2
    (`dist/core/resource-loader.js`). Ghostex used to install to the Pi ROOT
    extensions directory (`~/.pi/extensions/`), which pi's loader once read and
    no longer does, so the extension silently never loaded, session ids were
    never reported, and pi sessions could not enter chat. The agent-dir file is
    canonical (installs write the FIRST path); the root path stays listed so an
    existing install there is still detected — as present but stale, never
    current — and migrated by the repair pass.
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
        agent_dir.join("extensions").join("ghostex-session.ts"),
        root_dir.join("extensions").join("ghostex-session.ts"),
        agent_dir
            .join("extensions")
            .join("ghostex-session")
            .join("index.ts"),
    ])
}

/// True when pi's extension loader would actually load a file at `path`: pi
/// reads user extensions only from `<agent_dir>/extensions/`. A Ghostex
/// extension anywhere else (the pre-2026-08 root `~/.pi/extensions/` location)
/// exists on disk but never runs, so it must never count as a current install.
pub(crate) fn pi_extension_path_is_loader_visible(
    home_dir: &Path,
    respect_environment: bool,
    path: &Path,
) -> bool {
    let agent_dir = resolve_config_directory(
        home_dir,
        respect_environment,
        "PI_CODING_AGENT_DIR",
        ".pi/agent",
        None,
    );
    path.starts_with(agent_dir.join("extensions"))
}
