use super::*;

#[derive(Clone, Copy)]
pub(super) struct DefaultSidebarAgent {
    pub(super) agent_id: &'static str,
    pub(super) command: &'static str,
    pub(super) hidden_by_default: bool,
    pub(super) icon: &'static str,
    pub(super) name: &'static str,
}

#[derive(Clone, Debug)]
pub(super) struct StoredSidebarAgent {
    pub(super) accept_all_mode: Option<String>,
    pub(super) agent_id: String,
    pub(super) command: String,
    pub(super) hidden: bool,
    pub(super) icon: Option<String>,
    pub(super) name: String,
}

#[derive(Clone, Copy)]
pub(super) struct DefaultSidebarCommand {
    pub(super) command_id: &'static str,
    pub(super) name: &'static str,
}

#[derive(Clone, Debug)]
pub(super) struct StoredSidebarCommand {
    pub(super) action_type: &'static str,
    pub(super) close_terminal_on_exit: bool,
    pub(super) command: Option<String>,
    pub(super) command_id: String,
    pub(super) icon: Option<String>,
    pub(super) is_default: bool,
    pub(super) links: Vec<StoredSidebarCommandLink>,
    pub(super) name: String,
    pub(super) play_completion_sound: bool,
    pub(super) show_on_project_row: bool,
    pub(super) url: Option<String>,
}

/// Terminal actions can carry saved links that open alongside the command run,
/// each targeting the integrated browser pane or the user's system browser.
#[derive(Clone, Debug)]
pub(super) struct StoredSidebarCommandLink {
    pub(super) target: &'static str,
    pub(super) url: String,
}

#[derive(Clone, Debug)]
pub struct SidebarHudProjectMutation {
    pub params: Map<String, Value>,
    pub project_id: String,
}

#[derive(Clone, Debug)]
pub struct SidebarHudSettingsMutation {
    /*
    CDXC:AgentLauncher 2026-08-01-16:00:
    Global Actions belong to the daemon, not to a project row, so a settings
    mutation is no longer always a project write. A global mutation carries this
    field and leaves `updates` empty; a project mutation is unchanged and leaves
    this None. The two never both apply in one request because the Settings UI
    edits one list at a time.
    */
    pub global_command_update: Option<GlobalSidebarCommandUpdate>,
    pub hud_active_project_id: Option<String>,
    pub item_ids: Option<Vec<String>>,
    pub updates: Vec<SidebarHudProjectMutation>,
}

/// A single write against the daemon-owned Global Actions list. The repository
/// owns ordering and timestamps; this only describes the intent.
#[derive(Clone, Debug)]
pub enum GlobalSidebarCommandUpdate {
    Delete {
        command_id: String,
    },
    Order {
        command_ids: Vec<String>,
    },
    Save {
        command_id: String,
        definition: Value,
    },
}

#[derive(Clone, Debug)]
pub(super) enum SidebarAgentAcceptAllModeUpdate {
    Preserve,
    Set(Option<String>),
}

/*
CDXC:AgentLauncher 2026-06-24-20:34:
GPUI sidebar and app-modal clients consume normalized launcher/action HUD rows from gxserver instead of hand-mirroring the shared TypeScript read projection in each Rust host. Keep this platform-neutral and project-metadata-only: default rows, hidden built-ins, custom validation, icon allowlists, display order, deleted default actions, and active-project command ownership are resolved here without logging paths, names, commands, URLs, prompts, tokens, stdout/stderr, or daemon bodies.

CDXC:AgentLauncher 2026-06-24-20:54:
Settings save/delete/order mutations for custom agents and actions are gxserver-owned. Accept only narrow semantic mutation payloads, resolve hidden built-in agents, deleted default actions, display order, icon allowlists, active-project command scoping, and worktree parent ownership here, then persist only normalized project metadata fields through the existing project store.

CDXC:Projects 2026-06-25-21:36:
Sidebar HUD action commands must resolve from normal project rows only: explicit boolean isRecentProject true rows are parked Recent Projects metadata and cannot hydrate command buttons or own action mutations. False, missing, or non-boolean flags stay normal so older metadata remains eligible.

CDXC:Projects 2026-06-25-21:36:
When an explicit active project ID resolves only to a parked recent row, the HUD must show default actions and action mutations must fail as no-normal-project behavior instead of borrowing parked commands. Worktree parent ownership also skips parked rows and falls back to the active normal project.
*/
pub(super) const DEFAULT_SIDEBAR_AGENTS: &[DefaultSidebarAgent] = &[
    DefaultSidebarAgent {
        agent_id: "codex",
        command: "codex",
        hidden_by_default: false,
        icon: "codex",
        name: "Codex",
    },
    DefaultSidebarAgent {
        agent_id: "claude",
        command: "claude",
        hidden_by_default: false,
        icon: "claude",
        name: "Claude",
    },
    // CDXC:ClaudeCloud 2026-10-23 SEE-ALSO: the user's decision on the "Claude (cloud)" agent is in packages/gx-core/src/claude_cloud.rs; its sessions are server/src/claude_cloud/.
    DefaultSidebarAgent {
        agent_id: "claude-cloud",
        command: crate::claude_cloud::CLAUDE_CLOUD_COMMAND,
        hidden_by_default: false,
        icon: "claude-cloud",
        name: "Claude (cloud)",
    },
    DefaultSidebarAgent {
        agent_id: "cursor",
        command: "cursor-agent",
        hidden_by_default: false,
        icon: "cursor-cli",
        name: "Cursor CLI",
    },
    DefaultSidebarAgent {
        agent_id: "pi",
        command: "pi",
        hidden_by_default: false,
        icon: "pi",
        name: "Pi Agent",
    },
    DefaultSidebarAgent {
        agent_id: "opencode",
        command: "opencode",
        hidden_by_default: false,
        icon: "opencode",
        name: "OpenCode",
    },
    DefaultSidebarAgent {
        agent_id: "gemini",
        command: "gemini",
        hidden_by_default: false,
        icon: "gemini",
        name: "Gemini",
    },
    DefaultSidebarAgent {
        agent_id: "copilot",
        command: "copilot",
        hidden_by_default: false,
        icon: "copilot",
        name: "Copilot",
    },
    DefaultSidebarAgent {
        agent_id: "droid",
        command: "droid",
        hidden_by_default: false,
        icon: "factory-droid",
        name: "Factory Droid",
    },
    DefaultSidebarAgent {
        agent_id: "grok",
        command: "grok",
        hidden_by_default: false,
        icon: "grok-build",
        name: "Grok Build",
    },
    DefaultSidebarAgent {
        agent_id: "antigravity",
        command: "agy",
        hidden_by_default: false,
        icon: "antigravity-cli",
        name: "Antigravity CLI",
    },
    DefaultSidebarAgent {
        agent_id: "amp",
        command: "amp",
        hidden_by_default: false,
        icon: "amp-cli",
        name: "Amp CLI",
    },
    DefaultSidebarAgent {
        agent_id: "hermes-agent",
        command: "hermes",
        hidden_by_default: false,
        icon: "hermes-agent",
        name: "Hermes Agent",
    },
    DefaultSidebarAgent {
        agent_id: "rovodev",
        command: "acli rovodev run",
        hidden_by_default: true,
        icon: "rovo-dev",
        name: "Rovo Dev",
    },
    DefaultSidebarAgent {
        agent_id: "codebuddy",
        command: "codebuddy",
        hidden_by_default: true,
        icon: "codebuddy",
        name: "CodeBuddy",
    },
    DefaultSidebarAgent {
        agent_id: "qoder",
        command: "qodercli",
        hidden_by_default: true,
        icon: "qoder",
        name: "Qoder",
    },
    DefaultSidebarAgent {
        agent_id: "kiro",
        command: "kiro-cli chat --agent ghostex",
        hidden_by_default: true,
        icon: "kiro",
        name: "Kiro CLI",
    },
    DefaultSidebarAgent {
        agent_id: "omp",
        command: "omp",
        hidden_by_default: true,
        icon: "omp",
        name: "OMP",
    },
    DefaultSidebarAgent {
        agent_id: "kimi",
        command: "kimi",
        hidden_by_default: true,
        icon: "kimi",
        name: "Kimi Code",
    },
    DefaultSidebarAgent {
        agent_id: "openclaude",
        command: "openclaude",
        hidden_by_default: true,
        icon: "openclaude",
        name: "OpenClaude",
    },
    DefaultSidebarAgent {
        agent_id: "command-code",
        command: "commandcode",
        hidden_by_default: true,
        icon: "command-code",
        name: "Command Code",
    },
    DefaultSidebarAgent {
        agent_id: "devin",
        command: "devin",
        hidden_by_default: true,
        icon: "devin",
        name: "Devin",
    },
    DefaultSidebarAgent {
        agent_id: "mastra",
        command: "mastracode",
        hidden_by_default: false,
        icon: "mastra",
        name: "Mastra Code",
    },
    DefaultSidebarAgent {
        agent_id: "zcode",
        command: "zcode",
        hidden_by_default: false,
        icon: "zcode",
        name: "ZCode",
    },
    DefaultSidebarAgent {
        agent_id: "freebuff",
        command: "freebuff",
        hidden_by_default: false,
        icon: "freebuff",
        name: "Freebuff",
    },
    DefaultSidebarAgent {
        agent_id: "empryo",
        command: "empryo",
        // CDXC:AgentProviders 2026-10-08 WHY: Empryo arrived after most users' agent lists were saved, so an agent with no stored row reads as on and showed an enabled switch for a CLI nobody installed. It starts off and shows under More agents until turned on; a stored `hidden: false` row (someone who already turned it on) stays on. SEE-ALSO: apps/desktop/src/app/helpers/sidebar/sidebar_defaults_types.rs, packages/settings-catalog/src/data/sidebar_agents.rs.
        hidden_by_default: true,
        icon: "empryo",
        name: "Empryo",
    },
    DefaultSidebarAgent {
        agent_id: "dsh",
        command: "dsh-tui",
        // Starts off for the same reason as Empryo above.
        hidden_by_default: true,
        icon: "dsh",
        name: "DeepSeek Harness",
    },
];

/// The display name of a built-in agent (`Codex`, `Factory Droid`), for clients that list agents
/// by id, such as the phone's Agent CLIs of a machine.
pub(crate) fn default_agent_name(agent_id: &str) -> Option<&'static str> {
    DEFAULT_SIDEBAR_AGENTS
        .iter()
        .find(|agent| agent.agent_id == agent_id)
        .map(|agent| agent.name)
}

pub(super) const DEFAULT_SIDEBAR_COMMANDS: &[DefaultSidebarCommand] = &[
    DefaultSidebarCommand {
        command_id: "dev",
        name: "Dev",
    },
    DefaultSidebarCommand {
        command_id: "build",
        name: "Build",
    },
    DefaultSidebarCommand {
        command_id: "test",
        name: "Test",
    },
    DefaultSidebarCommand {
        command_id: "setup",
        name: "Setup",
    },
];

pub(super) const SIDEBAR_COMMAND_ICON_IDS: &[&str] = &[
    "playerPlay",
    "api",
    "archive",
    "bell",
    "bolt",
    "book",
    "brain",
    "braces",
    "brandDocker",
    "brandGithub",
    "brandPython",
    "brandReact",
    "brandVscode",
    "bug",
    "chartBar",
    "cloud",
    "checklist",
    "clock",
    "code",
    "command",
    "cpu",
    "database",
    "deviceDesktop",
    "deviceLaptop",
    "download",
    "fileCode",
    "fileDiff",
    "fileSearch",
    "fileText",
    "flask",
    "folder",
    "folderOpen",
    "gitBranch",
    "gitCommit",
    "gitMerge",
    "gitPullRequest",
    "key",
    "layoutDashboard",
    "link",
    "lock",
    "messageCircle",
    "package",
    "pencilCode",
    "refresh",
    "robot",
    "route",
    "rocket",
    "search",
    "server",
    "settings",
    "shieldSearch",
    "sparkles",
    "stack",
    "terminal",
    "testPipe",
    "tool",
    "upload",
    "wand",
    "world",
];
