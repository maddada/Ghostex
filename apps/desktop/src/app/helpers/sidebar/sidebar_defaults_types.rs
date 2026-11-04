// C1 wave-1 deferred split: apps/desktop/src/app/helpers/sidebar.rs (~4.1k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the default/stored sidebar agent and command structs, metadata-write kind/scope enums, and the default agent/command constant tables.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use crate::app::helpers::*;

#[derive(Clone, Copy)]
pub(crate) struct GpuiDefaultSidebarAgent {
    pub(crate) agent_id: &'static str,
    pub(crate) icon: &'static str,
    pub(crate) name: &'static str,
}

#[derive(Clone, Debug)]
pub(crate) struct GpuiStoredSidebarCommand {
    pub(crate) action_type: &'static str,
    pub(crate) close_terminal_on_exit: bool,
    pub(crate) command: Option<String>,
    pub(crate) command_id: String,
    pub(crate) icon: Option<String>,
    pub(crate) is_default: bool,
    pub(crate) name: String,
    pub(crate) play_completion_sound: bool,
    pub(crate) show_on_project_row: bool,
    pub(crate) url: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiSidebarMetadataWriteKind {
    Agents,
    Commands,
}

impl GpuiSidebarMetadataWriteKind {
    pub(crate) fn failure_message(self) -> &'static str {
        match self {
            Self::Agents => "GPUI could not save the Agents settings.",
            Self::Commands => "GPUI could not save the Actions settings.",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum GpuiSidebarAgentMetadataWrite {
    Save {
        accept_all_mode: GpuiAgentAcceptAllModeUpdate,
        agent_id: Option<String>,
        command: String,
        icon: Option<String>,
        name: String,
    },
    Delete {
        agent_id: String,
    },
    SyncOrder {
        agent_ids: Vec<String>,
        request_id: String,
    },
    /// Settings › Agents switches: turn agents on or off without deleting them.
    SetEnabled {
        agent_ids: Vec<String>,
        enabled: bool,
    },
}

impl GpuiSidebarAgentMetadataWrite {
    pub(crate) fn order_sync_result(
        &self,
        status: &'static str,
        item_ids: Vec<String>,
    ) -> Option<serde_json::Value> {
        match self {
            Self::SyncOrder { request_id, .. } => Some(gpui_sidebar_order_sync_result_message(
                "agent", request_id, status, item_ids,
            )),
            _ => None,
        }
    }
}

/*
CDXC:AgentLauncher 2026-08-01-19:00:
Settings > Actions writes reach gxserver through this Rust path, not through the
sidebar TypeScript runtime, so Global Actions need their own scope here or the
message is parsed as a project write. Scope only selects the mutation target;
the payload and every validation rule stay identical between the two lists.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiSidebarCommandScope {
    Global,
    Project,
}

impl GpuiSidebarCommandScope {
    pub(crate) fn mutation_target(self) -> &'static str {
        match self {
            Self::Global => "globalCommand",
            Self::Project => "command",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum GpuiSidebarCommandMetadataWrite {
    Save {
        action_type: &'static str,
        active_project_id: String,
        close_terminal_on_exit: bool,
        command: Option<String>,
        command_id: Option<String>,
        icon: Option<String>,
        links: Vec<GpuiTitlebarActionLink>,
        name: String,
        play_completion_sound: bool,
        scope: GpuiSidebarCommandScope,
        show_on_project_row: bool,
        url: Option<String>,
    },
    Delete {
        active_project_id: String,
        command_id: String,
        scope: GpuiSidebarCommandScope,
    },
    SyncOrder {
        active_project_id: String,
        command_ids: Vec<String>,
        request_id: String,
        scope: GpuiSidebarCommandScope,
    },
}

impl GpuiSidebarCommandMetadataWrite {
    pub(crate) fn scope(&self) -> GpuiSidebarCommandScope {
        match self {
            Self::Save { scope, .. }
            | Self::Delete { scope, .. }
            | Self::SyncOrder { scope, .. } => *scope,
        }
    }
}

impl GpuiSidebarCommandMetadataWrite {
    pub(crate) fn order_sync_result(
        &self,
        status: &'static str,
        item_ids: Vec<String>,
    ) -> Option<serde_json::Value> {
        match self {
            Self::SyncOrder { request_id, .. } => Some(gpui_sidebar_order_sync_result_message(
                "command", request_id, status, item_ids,
            )),
            _ => None,
        }
    }

    pub(crate) fn deleted_command_id(&self) -> Option<&str> {
        match self {
            Self::Delete { command_id, .. } => Some(command_id.as_str()),
            _ => None,
        }
    }
}

pub(crate) const GPUI_DEFAULT_SIDEBAR_AGENTS: &[GpuiDefaultSidebarAgent] = &[
    GpuiDefaultSidebarAgent {
        agent_id: "codex",
        icon: "codex",
        name: "Codex",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "claude",
        icon: "claude",
        name: "Claude",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "claude-cloud",
        icon: "claude-cloud",
        name: "Claude (cloud)",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "cursor",
        icon: "cursor-cli",
        name: "Cursor CLI",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "pi",
        icon: "pi",
        name: "Pi Agent",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "opencode",
        icon: "opencode",
        name: "OpenCode",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "gemini",
        icon: "gemini",
        name: "Gemini",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "copilot",
        icon: "copilot",
        name: "Copilot",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "droid",
        icon: "factory-droid",
        name: "Factory Droid",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "grok",
        icon: "grok-build",
        name: "Grok Build",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "antigravity",
        icon: "antigravity-cli",
        name: "Antigravity CLI",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "amp",
        icon: "amp-cli",
        name: "Amp CLI",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "rovodev",
        icon: "rovo-dev",
        name: "Rovo Dev",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "hermes-agent",
        icon: "hermes-agent",
        name: "Hermes Agent",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "codebuddy",
        icon: "codebuddy",
        name: "CodeBuddy",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "qoder",
        icon: "qoder",
        name: "Qoder",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "kiro",
        icon: "kiro",
        name: "Kiro CLI",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "omp",
        icon: "omp",
        name: "OMP",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "kimi",
        icon: "kimi",
        name: "Kimi Code",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "openclaude",
        icon: "openclaude",
        name: "OpenClaude",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "command-code",
        icon: "command-code",
        name: "Command Code",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "devin",
        icon: "devin",
        name: "Devin",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "mastra",
        icon: "mastra",
        name: "Mastra Code",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "zcode",
        icon: "zcode",
        name: "ZCode",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "freebuff",
        icon: "freebuff",
        name: "Freebuff",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "empryo",
        // CDXC:AgentProviders 2026-10-08 WHY: starts off like the server's list, so Empryo shows under More agents until turned on. SEE-ALSO: server/src/sidebar_hud/model.rs.
        icon: "empryo",
        name: "Empryo",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "dsh",
        // Starts off like Empryo: under More agents until turned on. SEE-ALSO: server/src/sidebar_hud/model.rs.
        icon: "dsh",
        name: "DeepSeek Harness",
    },
];

pub(crate) const GPUI_DEFAULT_SIDEBAR_COMMAND_ICON: &str = "playerPlay";
pub(crate) const GPUI_SIDEBAR_COMMAND_ICON_IDS: &[&str] = &[
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
