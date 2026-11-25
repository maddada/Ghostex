//! The per-agent `/` catalogs, as data.
//!
//! Split out of `slash_commands.rs` so the rules stay readable beside a table this long. It is a
//! curated snapshot of what each agent CLI intercepts, not a runtime discovery, and it also feeds
//! the send classification, so a wrong name costs more than a missing one. The instructions for
//! re-verifying a catalog against the binary the user runs were in
//! `packages/core-ui/chat/session-chat-slash-commands.ts`, which left with the React chat; read
//! them in git history.

use crate::composer::slash_commands::SlashCommand;

pub(crate) const CLAUDE_CODE: &[SlashCommand] = &[
    SlashCommand {
        name: "add-dir",
        description: "Add a new working directory",
        insert_text: None,
    },
    SlashCommand {
        name: "advisor",
        description: "Choose a stronger model Claude can consult",
        insert_text: None,
    },
    SlashCommand {
        name: "auto-mode-setup",
        description: "Teach auto mode about your environment",
        insert_text: None,
    },
    SlashCommand {
        name: "autocompact",
        description: "Set how full the context gets before auto-summarizing",
        insert_text: None,
    },
    SlashCommand {
        name: "autofix-pr",
        description: "Monitor and fix issues with the current PR",
        insert_text: None,
    },
    SlashCommand {
        name: "background",
        description: "Send this session to the background",
        insert_text: None,
    },
    SlashCommand {
        name: "batch",
        description: "Plan a large change across parallel worktree agents",
        insert_text: None,
    },
    SlashCommand {
        name: "branch",
        description: "Create a branch of the conversation at this point",
        insert_text: None,
    },
    SlashCommand {
        name: "brief",
        description: "Toggle brief-only mode",
        insert_text: None,
    },
    // CDXC:SessionChat 2026-09-30 DECISION: User: writing /btw and hitting Enter in the chat only switches Side chat on, it is not sent to the agent. `/btw ` is the Side Chat pill (side_chat.rs), so Enter on a bare `/btw`, on `/btw` picked from the list, or on a lone pill completes to the pill instead of sending.
    SlashCommand {
        name: "btw",
        description: "Ask a side question without interrupting the main conversation",
        insert_text: Some(crate::composer::side_chat::SIDE_CHAT_PREFIX),
    },
    SlashCommand {
        name: "bug",
        description: "Report a bug or share your conversation",
        insert_text: None,
    },
    SlashCommand {
        name: "cd",
        description: "Move this session to a new working directory",
        insert_text: None,
    },
    SlashCommand {
        name: "chrome",
        description: "Manage Claude in Chrome settings",
        insert_text: None,
    },
    SlashCommand {
        name: "claude-api",
        description: "Reference the Claude API and Anthropic SDK",
        insert_text: None,
    },
    SlashCommand {
        name: "clear",
        description: "Start a new session with empty context",
        insert_text: None,
    },
    SlashCommand {
        name: "color",
        description: "Set the prompt bar color for this session",
        insert_text: None,
    },
    SlashCommand {
        name: "compact",
        description: "Free up context by summarizing the conversation",
        insert_text: None,
    },
    SlashCommand {
        name: "config",
        description: "Open settings",
        insert_text: None,
    },
    SlashCommand {
        name: "context",
        description: "Show current context usage",
        insert_text: None,
    },
    SlashCommand {
        name: "copy",
        description: "Copy Claude's last response to the clipboard",
        insert_text: None,
    },
    SlashCommand {
        name: "debug",
        description: "Enable session debug logging and diagnose issues",
        insert_text: None,
    },
    SlashCommand {
        name: "design",
        description: "Manage access to Claude Design projects",
        insert_text: None,
    },
    SlashCommand {
        name: "design-login",
        description: "Authorize access for Design sync",
        insert_text: None,
    },
    SlashCommand {
        name: "design-sync",
        description: "Upload a React design system to Claude Design",
        insert_text: None,
    },
    SlashCommand {
        name: "desktop",
        description: "Continue this session in Claude Desktop",
        insert_text: None,
    },
    SlashCommand {
        name: "diff",
        description: "Toggle the terminal diff panel",
        insert_text: None,
    },
    SlashCommand {
        name: "doctor",
        description: "Diagnose installation issues",
        insert_text: None,
    },
    SlashCommand {
        name: "effort",
        description: "Set the effort level for model usage",
        insert_text: None,
    },
    SlashCommand {
        name: "exit",
        description: "Exit the REPL",
        insert_text: None,
    },
    SlashCommand {
        name: "export",
        description: "Export the conversation to a file or clipboard",
        insert_text: None,
    },
    SlashCommand {
        name: "fast",
        description: "Toggle fast mode",
        insert_text: None,
    },
    SlashCommand {
        name: "feedback",
        description: "Send feedback to Anthropic",
        insert_text: None,
    },
    SlashCommand {
        name: "fewer-permission-prompts",
        description: "Generate project permission rules from common tool calls",
        insert_text: None,
    },
    SlashCommand {
        name: "focus",
        description: "Toggle focus view",
        insert_text: None,
    },
    SlashCommand {
        name: "fork",
        description: "Spawn a background agent with the full conversation",
        insert_text: None,
    },
    SlashCommand {
        name: "goal",
        description: "Set a goal Claude checks before stopping",
        insert_text: None,
    },
    SlashCommand {
        name: "help",
        description: "Show help and available commands",
        insert_text: None,
    },
    SlashCommand {
        name: "hooks",
        description: "View hook configurations",
        insert_text: None,
    },
    SlashCommand {
        name: "ide",
        description: "Manage IDE integrations and show status",
        insert_text: None,
    },
    SlashCommand {
        name: "import",
        description: "Import config from another AI coding agent",
        insert_text: None,
    },
    SlashCommand {
        name: "init",
        description: "Create a CLAUDE.md for the project",
        insert_text: None,
    },
    SlashCommand {
        name: "insights",
        description: "Analyze your Claude Code sessions",
        insert_text: None,
    },
    SlashCommand {
        name: "install-github-app",
        description: "Set up Claude GitHub Actions",
        insert_text: None,
    },
    SlashCommand {
        name: "install-slack-app",
        description: "Install the Claude Slack app",
        insert_text: None,
    },
    SlashCommand {
        name: "keybindings",
        description: "Open the keyboard shortcuts file",
        insert_text: None,
    },
    SlashCommand {
        name: "list-agents",
        description: "List sessions you can message",
        insert_text: None,
    },
    SlashCommand {
        name: "login",
        description: "Switch Anthropic accounts",
        insert_text: None,
    },
    SlashCommand {
        name: "logout",
        description: "Sign out from your Anthropic account",
        insert_text: None,
    },
    SlashCommand {
        name: "mcp",
        description: "Manage MCP servers",
        insert_text: None,
    },
    SlashCommand {
        name: "memory",
        description: "Edit CLAUDE.md files and memory settings",
        insert_text: None,
    },
    SlashCommand {
        name: "mobile",
        description: "Show the Claude mobile app download link",
        insert_text: None,
    },
    SlashCommand {
        name: "model",
        description: "Set the AI model for Claude Code",
        insert_text: None,
    },
    SlashCommand {
        name: "passes",
        description: "Manage referral passes and usage credits",
        insert_text: None,
    },
    SlashCommand {
        name: "permissions",
        description: "Manage allow and deny tool permission rules",
        insert_text: None,
    },
    SlashCommand {
        name: "plan",
        description: "Enable plan mode or view the session plan",
        insert_text: None,
    },
    SlashCommand {
        name: "plugin",
        description: "Manage Claude Code plugins",
        insert_text: None,
    },
    SlashCommand {
        name: "powerup",
        description: "Explore interactive Claude Code lessons",
        insert_text: None,
    },
    SlashCommand {
        name: "privacy-settings",
        description: "Review and change data privacy settings",
        insert_text: None,
    },
    SlashCommand {
        name: "radio",
        description: "Play Claude FM radio",
        insert_text: None,
    },
    SlashCommand {
        name: "recap",
        description: "Generate a one-line session recap",
        insert_text: None,
    },
    SlashCommand {
        name: "release-notes",
        description: "View release notes",
        insert_text: None,
    },
    SlashCommand {
        name: "reload-plugins",
        description: "Activate pending plugin changes",
        insert_text: None,
    },
    SlashCommand {
        name: "reload-skills",
        description: "Pick up skills added or changed on disk",
        insert_text: None,
    },
    SlashCommand {
        name: "remote-control",
        description: "Control this session from your phone or Claude on the web",
        insert_text: None,
    },
    SlashCommand {
        name: "remote-env",
        description: "Choose the default cloud agent environment",
        insert_text: None,
    },
    SlashCommand {
        name: "rename",
        description: "Rename the current conversation",
        insert_text: None,
    },
    SlashCommand {
        name: "resume",
        description: "Resume a previous conversation",
        insert_text: None,
    },
    SlashCommand {
        name: "rewind",
        description: "Restore the code and/or conversation to a previous point",
        insert_text: None,
    },
    SlashCommand {
        name: "run",
        description: "Launch and inspect the project app",
        insert_text: None,
    },
    SlashCommand {
        name: "run-skill-generator",
        description: "Create or improve the project run skill",
        insert_text: None,
    },
    SlashCommand {
        name: "sandbox",
        description: "Configure sandbox mode and overrides",
        insert_text: None,
    },
    SlashCommand {
        name: "schedule",
        description: "Manage scheduled cloud agents",
        insert_text: None,
    },
    SlashCommand {
        name: "scroll-speed",
        description: "Adjust terminal mouse wheel scroll speed",
        insert_text: None,
    },
    SlashCommand {
        name: "security-review",
        description: "Review changes for security issues",
        insert_text: None,
    },
    SlashCommand {
        name: "skill-doctor",
        description: "Show which loaded skills are unused",
        insert_text: None,
    },
    SlashCommand {
        name: "skills",
        description: "List available skills",
        insert_text: None,
    },
    SlashCommand {
        name: "status",
        description: "Show version, model, account, and connectivity",
        insert_text: None,
    },
    SlashCommand {
        name: "statusline",
        description: "Set up the status line",
        insert_text: None,
    },
    SlashCommand {
        name: "stickers",
        description: "Open the Claude Code sticker order page",
        insert_text: None,
    },
    SlashCommand {
        name: "stop",
        description: "Stop this background session",
        insert_text: None,
    },
    SlashCommand {
        name: "subtask",
        description: "Send a subagent off with your full context",
        insert_text: None,
    },
    SlashCommand {
        name: "tasks",
        description: "View and manage everything running in the background",
        insert_text: None,
    },
    SlashCommand {
        name: "team-onboarding",
        description: "Create a team onboarding guide from your usage",
        insert_text: None,
    },
    SlashCommand {
        name: "teleport",
        description: "Send this session to the cloud",
        insert_text: None,
    },
    SlashCommand {
        name: "terminal-setup",
        description: "Install the ⇧Enter key binding",
        insert_text: None,
    },
    SlashCommand {
        name: "theme",
        description: "Change the theme",
        insert_text: None,
    },
    SlashCommand {
        name: "tui",
        description: "Set the terminal UI renderer",
        insert_text: None,
    },
    SlashCommand {
        name: "ultrareview",
        description: "Start a paid cloud review to find and verify bugs",
        insert_text: None,
    },
    SlashCommand {
        name: "update-config",
        description: "Configure Claude Code settings",
        insert_text: None,
    },
    SlashCommand {
        name: "upgrade",
        description: "View plan upgrade options",
        insert_text: None,
    },
    SlashCommand {
        name: "usage",
        description: "Show session cost, plan usage, and activity stats",
        insert_text: None,
    },
    SlashCommand {
        name: "usage-credits",
        description: "Configure additional usage credits",
        insert_text: None,
    },
    SlashCommand {
        name: "voice",
        description: "Configure terminal voice input",
        insert_text: None,
    },
    SlashCommand {
        name: "web-setup",
        description: "Connect GitHub to Claude Code on the web",
        insert_text: None,
    },
    SlashCommand {
        name: "workflows",
        description: "Browse running and completed workflows",
        insert_text: None,
    },
];

pub(crate) const CODEX: &[SlashCommand] = &[
    SlashCommand {
        name: "agents",
        description: "View and switch between agent sessions",
        insert_text: None,
    },
    SlashCommand {
        name: "app",
        description: "Continue in Codex Desktop",
        insert_text: None,
    },
    SlashCommand {
        name: "approve",
        description: "Approve one auto-review retry",
        insert_text: None,
    },
    SlashCommand {
        name: "apps",
        description: "Manage apps",
        insert_text: None,
    },
    SlashCommand {
        name: "archive",
        description: "Archive this session and exit",
        insert_text: None,
    },
    SlashCommand {
        name: "btw",
        description: "Start a side conversation in an ephemeral fork",
        // Enter on a bare `/btw` switches Side chat on, as in Claude's catalog above.
        insert_text: Some(crate::composer::side_chat::SIDE_CHAT_PREFIX),
    },
    SlashCommand {
        name: "cd",
        description: "Change the working directory",
        insert_text: None,
    },
    SlashCommand {
        name: "clear",
        description: "Clear the terminal and start a new chat",
        insert_text: None,
    },
    SlashCommand {
        name: "compact",
        description: "Summarize conversation to save context",
        insert_text: None,
    },
    SlashCommand {
        name: "copy",
        description: "Copy the last response, code block, or quote",
        insert_text: None,
    },
    SlashCommand {
        name: "debug-config",
        description: "Show config layers and requirement sources",
        insert_text: None,
    },
    SlashCommand {
        name: "delete",
        description: "Delete this session and exit",
        insert_text: None,
    },
    SlashCommand {
        name: "diff",
        description: "Show git diff (including untracked files)",
        insert_text: None,
    },
    SlashCommand {
        name: "exit",
        description: "Exit Codex",
        insert_text: None,
    },
    SlashCommand {
        name: "experimental",
        description: "Toggle experimental features",
        insert_text: None,
    },
    SlashCommand {
        name: "export",
        description: "Export the conversation as markdown",
        insert_text: None,
    },
    SlashCommand {
        name: "feedback",
        description: "Send logs to maintainers",
        insert_text: None,
    },
    SlashCommand {
        name: "fork",
        description: "Fork the current chat",
        insert_text: None,
    },
    SlashCommand {
        name: "goal",
        description: "Set or view the goal",
        insert_text: None,
    },
    SlashCommand {
        name: "hooks",
        description: "View lifecycle hooks",
        insert_text: None,
    },
    SlashCommand {
        name: "ide",
        description: "Include IDE context",
        insert_text: None,
    },
    SlashCommand {
        name: "import",
        description: "Import setup from Claude Code or Cursor",
        insert_text: None,
    },
    SlashCommand {
        name: "init",
        description: "Create an AGENTS.md for Codex",
        insert_text: None,
    },
    SlashCommand {
        name: "keymap",
        description: "Remap TUI shortcuts",
        insert_text: None,
    },
    SlashCommand {
        name: "logout",
        description: "Log out of Codex",
        insert_text: None,
    },
    SlashCommand {
        name: "mcp",
        description: "List configured MCP tools",
        insert_text: None,
    },
    SlashCommand {
        name: "memories",
        description: "Configure memory use",
        insert_text: None,
    },
    SlashCommand {
        name: "mention",
        description: "Mention a file",
        insert_text: Some("@"),
    },
    SlashCommand {
        name: "model",
        description: "Choose model and reasoning effort",
        insert_text: None,
    },
    SlashCommand {
        name: "new",
        description: "Start a new chat",
        insert_text: None,
    },
    SlashCommand {
        name: "permissions",
        description: "Review and change tool permissions",
        insert_text: None,
    },
    SlashCommand {
        name: "personality",
        description: "Choose a communication style",
        insert_text: None,
    },
    SlashCommand {
        name: "pets",
        description: "Choose or hide the terminal pet",
        insert_text: None,
    },
    SlashCommand {
        name: "plan",
        description: "Switch to Plan mode",
        insert_text: None,
    },
    SlashCommand {
        name: "plugins",
        description: "Browse plugins",
        insert_text: None,
    },
    SlashCommand {
        name: "ps",
        description: "List background terminals",
        insert_text: None,
    },
    SlashCommand {
        name: "pwd",
        description: "Show the working directory",
        insert_text: None,
    },
    SlashCommand {
        name: "quit",
        description: "Exit Codex",
        insert_text: None,
    },
    SlashCommand {
        name: "raw",
        description: "Toggle raw scrollback mode",
        insert_text: None,
    },
    SlashCommand {
        name: "recap",
        description: "Summarize the current conversation now",
        insert_text: None,
    },
    SlashCommand {
        name: "rename",
        description: "Rename the current session",
        insert_text: None,
    },
    SlashCommand {
        name: "resume",
        description: "Resume a saved chat",
        insert_text: None,
    },
    SlashCommand {
        name: "review",
        description: "Review current changes and find issues",
        insert_text: None,
    },
    SlashCommand {
        name: "setup-default-sandbox",
        description: "Set up the elevated agent sandbox",
        insert_text: None,
    },
    SlashCommand {
        name: "side",
        description: "Start a side conversation",
        insert_text: None,
    },
    SlashCommand {
        name: "skills",
        description: "Manage and use skills",
        insert_text: None,
    },
    SlashCommand {
        name: "status",
        description: "Show session configuration and token usage",
        insert_text: None,
    },
    SlashCommand {
        name: "statusline",
        description: "Configure the status line",
        insert_text: None,
    },
    SlashCommand {
        name: "stop",
        description: "Stop all background terminals",
        insert_text: None,
    },
    SlashCommand {
        name: "subagents",
        description: "Switch between this session's subagents",
        insert_text: None,
    },
    SlashCommand {
        name: "theme",
        description: "Choose a syntax highlighting theme",
        insert_text: None,
    },
    SlashCommand {
        name: "title",
        description: "Configure the terminal title",
        insert_text: None,
    },
    SlashCommand {
        name: "usage",
        description: "View account usage",
        insert_text: None,
    },
    SlashCommand {
        name: "vim",
        description: "Toggle vim editing mode",
        insert_text: None,
    },
];

/// CDXC:SessionChat 2026-11-24 WHY:
/// A Native Codex chat talks to Codex's app-server, which has no slash commands: only `/compact` is turned into its own request (`thread/compact/start` in server/src/acp/driver_codex.rs), and the composer completes `/btw` and `/mention` itself. Every other Codex command reached the model as plain text, so `/diff` answered "No changes to report." without looking (found testing Native Codex on Linux). A Native Codex chat offers only these.
pub(crate) const CODEX_NATIVE: &[SlashCommand] = &[
    SlashCommand {
        name: "btw",
        description: "Start a side conversation in an ephemeral fork",
        insert_text: Some(crate::composer::side_chat::SIDE_CHAT_PREFIX),
    },
    SlashCommand {
        name: "compact",
        description: "Summarize conversation to save context",
        insert_text: None,
    },
    SlashCommand {
        name: "mention",
        description: "Mention a file",
        insert_text: Some("@"),
    },
];

/// CDXC:SessionChat 2026-11-24 WHY:
/// A Native Antigravity chat talks to `agy acp`, which runs the built-in commands it advertises over ACP (`available_commands_update`, agy 1.5.15; the rest of that list is its skills, which the `/` list shows from the skills source). The generic list offered `/clear`, `/exit`, `/help` and `/model`, none of which `agy acp` has: they reached the model as plain text, so `/help` got a made-up answer (found testing Native Antigravity on Linux). Descriptions are agy's own.
pub(crate) const ANTIGRAVITY_NATIVE: &[SlashCommand] = &[
    SlashCommand {
        name: "boost",
        description: "Invoke the Boost multi-agent orchestrator for complex tasks",
        insert_text: None,
    },
    SlashCommand {
        name: "browser",
        description: "Invoke a browser agent for web tasks",
        insert_text: None,
    },
    SlashCommand {
        name: "compact",
        description: "Compact the conversation now to free up the context window",
        insert_text: None,
    },
    SlashCommand {
        name: "goal",
        description: "Run until the specified goal is completely finished",
        insert_text: None,
    },
    SlashCommand {
        name: "grill-me",
        description: "Interview me to align on a plan",
        insert_text: None,
    },
    SlashCommand {
        name: "learn",
        description: "Capture reusable skills or rules from recent work",
        insert_text: None,
    },
    SlashCommand {
        name: "plan",
        description: "Plan carefully before executing a task",
        insert_text: None,
    },
    SlashCommand {
        name: "schedule",
        description: "Run an instruction on a schedule or as a one-time timer",
        insert_text: None,
    },
    SlashCommand {
        name: "teamwork",
        description: "Invoke a team of agents to tackle large projects",
        insert_text: None,
    },
];

/// CDXC:SessionChat 2026-11-24 WHY:
/// A Native Claude chat drives `claude` in stream-json mode, which runs only the commands it lists in its start-up `slash_commands` (Claude Code 2.1.331; the rest of that list is skills, which the `/` list shows from the skills source). The full terminal catalog offered `/theme`, `/tui`, `/login`, `/exit` and the like, which reached the model as plain text, so `/theme` got "type it in the Claude Code prompt" (found testing Native Claude on Linux). These are the catalog's entries Claude lists, plus `/btw`, which the composer handles itself.
const CLAUDE_NATIVE_NAMES: &[&str] = &[
    "advisor",
    "auto-mode-setup",
    "autocompact",
    "batch",
    "btw",
    "claude-api",
    "clear",
    "color",
    "compact",
    "config",
    "context",
    "debug",
    "design",
    "design-sync",
    "doctor",
    "effort",
    "fast",
    "fewer-permission-prompts",
    "focus",
    "goal",
    "import",
    "init",
    "insights",
    "list-agents",
    "mcp",
    "model",
    "recap",
    "reload-plugins",
    "reload-skills",
    "rename",
    "run",
    "run-skill-generator",
    "schedule",
    "security-review",
    "skill-doctor",
    "team-onboarding",
    "ultrareview",
    "update-config",
    "usage",
    "usage-credits",
];

/// The Claude catalog's entries a Native Claude chat can run (`CLAUDE_NATIVE_NAMES`), with the
/// catalog's own descriptions.
pub(crate) static CLAUDE_NATIVE: std::sync::LazyLock<Vec<SlashCommand>> =
    std::sync::LazyLock::new(|| {
        CLAUDE_CODE
            .iter()
            .filter(|command| CLAUDE_NATIVE_NAMES.contains(&command.name))
            .copied()
            .collect()
    });

/// Commands handled directly by OpenCode 2's Native server host (`/compact`, `/model`, `/agent`,
/// `/init`, `/review`) plus `/btw`, which the composer handles itself.
pub(crate) const OPENCODE_NATIVE: &[SlashCommand] = &[
    SlashCommand {
        name: "agent",
        description: "Switch OpenCode agent (build or plan)",
        insert_text: None,
    },
    SlashCommand {
        name: "btw",
        description: "Ask a quick side question without interrupting",
        insert_text: None,
    },
    SlashCommand {
        name: "compact",
        description: "Compact conversation history",
        insert_text: None,
    },
    SlashCommand {
        name: "init",
        description: "Initialize project instructions (AGENTS.md)",
        insert_text: None,
    },
    SlashCommand {
        name: "model",
        description: "Switch model (/model provider/model [variant])",
        insert_text: None,
    },
    SlashCommand {
        name: "review",
        description: "Review changes in the repository",
        insert_text: None,
    },
];

/// Built-in slash commands that `grok agent stdio` supports over ACP (omitting `/always-approve`,
/// which bypasses Ghostex's permission mode, and `/context`, which completes silently over ACP).
pub(crate) const GROK_NATIVE: &[SlashCommand] = &[
    SlashCommand {
        name: "compact",
        description: "Compress conversation history to save context window",
        insert_text: None,
    },
    SlashCommand {
        name: "deep-research",
        description:
            "Research with bounded parallel agents, cross-check evidence, and write a cited report",
        insert_text: None,
    },
    SlashCommand {
        name: "goal",
        description: "Set, manage, or check an autonomous goal",
        insert_text: None,
    },
    SlashCommand {
        name: "session-info",
        description: "Show session details (model, turns, context usage)",
        insert_text: None,
    },
    SlashCommand {
        name: "workflow",
        description: "Launch a saved workflow, list runs, or manage a run",
        insert_text: None,
    },
];

/// CDXC:SessionChat 2026-10-17 WHY:
/// Grok Build 1.0.62's own built-in commands (its `slash/commands/` registry, checked against the live `/` menu); skills and workflows reach the chat through the `$` picker. The earlier list named commands Grok never had (`/editor`, `/init`, `/models`, `/status`) or renamed (`/mcp` is `/mcps`).
pub(crate) const GROK: &[SlashCommand] = &[
    SlashCommand {
        name: "always-approve",
        description: "Toggle always-approve mode (skip all permission prompts)",
        insert_text: None,
    },
    SlashCommand {
        name: "auto",
        description: "Toggle Auto-review (classifier approves safe tools)",
        insert_text: None,
    },
    SlashCommand {
        name: "btw",
        description: "Ask a side question without interrupting",
        insert_text: None,
    },
    SlashCommand {
        name: "compact",
        description: "Compact conversation history",
        insert_text: None,
    },
    SlashCommand {
        name: "compact-mode",
        description: "Toggle compact UI (less padding, more content)",
        insert_text: None,
    },
    SlashCommand {
        name: "config-agents",
        description: "Manage agent definitions",
        insert_text: None,
    },
    SlashCommand {
        name: "context",
        description: "View context usage",
        insert_text: None,
    },
    SlashCommand {
        name: "context-window",
        description: "Set the context window for the current model",
        insert_text: None,
    },
    SlashCommand {
        name: "copy",
        description: "Copy last response to clipboard or file (/copy [N] [file])",
        insert_text: None,
    },
    SlashCommand {
        name: "dashboard",
        description: "Open the Agent Dashboard",
        insert_text: None,
    },
    SlashCommand {
        name: "delete",
        description: "Delete this session",
        insert_text: None,
    },
    SlashCommand {
        name: "density",
        description: "Set tool call density, from Detailed to Compact",
        insert_text: None,
    },
    SlashCommand {
        name: "docs",
        description: "Open How-to Guides or online Build docs",
        insert_text: None,
    },
    SlashCommand {
        name: "doctor",
        description: "Check this session and show available fixes",
        insert_text: None,
    },
    SlashCommand {
        name: "dream",
        description: "Consolidate memory now (merge observations into topics)",
        insert_text: None,
    },
    SlashCommand {
        name: "edit-prompt",
        description: "Open an external editor for an empty prompt; use the command palette to preserve a draft",
        insert_text: None,
    },
    SlashCommand {
        name: "effort",
        description: "Set reasoning effort for the current model",
        insert_text: None,
    },
    SlashCommand {
        name: "export",
        description: "Export the current conversation to a file or clipboard",
        insert_text: None,
    },
    SlashCommand {
        name: "feedback",
        description: "Send feedback about the current session",
        insert_text: None,
    },
    SlashCommand {
        name: "find",
        description: "Search the conversation scrollback",
        insert_text: None,
    },
    SlashCommand {
        name: "flush",
        description: "Save this session's memory to disk now",
        insert_text: None,
    },
    SlashCommand {
        name: "fork",
        description: "Branch the current session into a peer agent",
        insert_text: None,
    },
    SlashCommand {
        name: "help",
        description: "Browse commands and keyboard shortcuts",
        insert_text: None,
    },
    SlashCommand {
        name: "history",
        description: "Search prompt history",
        insert_text: None,
    },
    SlashCommand {
        name: "home",
        description: "Return to the welcome screen",
        insert_text: None,
    },
    SlashCommand {
        name: "hooks",
        description: "View hooks",
        insert_text: None,
    },
    SlashCommand {
        name: "imagine",
        description: "Generate an image from a text description",
        insert_text: None,
    },
    SlashCommand {
        name: "imagine-video",
        description: "Generate a video from a text description",
        insert_text: None,
    },
    SlashCommand {
        name: "import-claude",
        description: "Open the Claude settings import modal",
        insert_text: None,
    },
    SlashCommand {
        name: "jump",
        description: "Jump to a turn in the conversation",
        insert_text: None,
    },
    SlashCommand {
        name: "login",
        description: "Log in or re-authenticate with your account",
        insert_text: None,
    },
    SlashCommand {
        name: "logout",
        description: "Log out and return to the login screen",
        insert_text: None,
    },
    SlashCommand {
        name: "loop",
        description: "Run a prompt on a recurring interval",
        insert_text: None,
    },
    SlashCommand {
        name: "marketplace",
        description: "View marketplace",
        insert_text: None,
    },
    SlashCommand {
        name: "mcps",
        description: "Show MCP server status",
        insert_text: None,
    },
    SlashCommand {
        name: "memory",
        description: "Browse, view, and manage your memories",
        insert_text: None,
    },
    SlashCommand {
        name: "minimal",
        description: "Switch this session to minimal (scrollback-native) mode, back with /fullscreen",
        insert_text: None,
    },
    SlashCommand {
        name: "model",
        description: "Switch the active model",
        insert_text: None,
    },
    SlashCommand {
        name: "multiline",
        description: "Toggle multiline input mode (swap Enter and Shift+Enter)",
        insert_text: None,
    },
    SlashCommand {
        name: "new",
        description: "Start a new session",
        insert_text: None,
    },
    SlashCommand {
        name: "personas",
        description: "Manage personas (create, edit, delete)",
        insert_text: None,
    },
    SlashCommand {
        name: "plan",
        description: "Enter plan mode",
        insert_text: None,
    },
    SlashCommand {
        name: "plugins",
        description: "View plugins",
        insert_text: None,
    },
    SlashCommand {
        name: "privacy",
        description: "Open coding data, retention, and training settings",
        insert_text: None,
    },
    SlashCommand {
        name: "queue",
        description: "List the prompts queued behind the running turn",
        insert_text: None,
    },
    SlashCommand {
        name: "quit",
        description: "Quit the application",
        insert_text: None,
    },
    SlashCommand {
        name: "recap",
        description: "Summarize the session so far",
        insert_text: None,
    },
    SlashCommand {
        name: "release-notes",
        description: "View release notes for the current version",
        insert_text: None,
    },
    SlashCommand {
        name: "remember",
        description: "Save a memory note",
        insert_text: None,
    },
    SlashCommand {
        name: "rename",
        description: "Rename the current session",
        insert_text: None,
    },
    SlashCommand {
        name: "resume",
        description: "Resume a previous session",
        insert_text: None,
    },
    SlashCommand {
        name: "resume-claude",
        description: "Continue from a recent Claude Code session",
        insert_text: None,
    },
    SlashCommand {
        name: "resume-codex",
        description: "Continue from a recent Codex session",
        insert_text: None,
    },
    SlashCommand {
        name: "resume-cursor",
        description: "Continue from a recent Cursor session",
        insert_text: None,
    },
    SlashCommand {
        name: "rewind",
        description: "Rewind to a previous turn",
        insert_text: None,
    },
    SlashCommand {
        name: "session-info",
        description: "Show session info",
        insert_text: None,
    },
    SlashCommand {
        name: "settings",
        description: "Open the settings modal",
        insert_text: None,
    },
    SlashCommand {
        name: "skills",
        description: "View skills",
        insert_text: None,
    },
    SlashCommand {
        name: "statusline",
        description: "Configure the Grok Build status line",
        insert_text: None,
    },
    SlashCommand {
        name: "tasks",
        description: "List background tasks, subagents, and scheduled tasks",
        insert_text: None,
    },
    SlashCommand {
        name: "theme",
        description: "Switch the color theme",
        insert_text: None,
    },
    SlashCommand {
        name: "timeline",
        description: "Toggle the timeline sidebar",
        insert_text: None,
    },
    SlashCommand {
        name: "timestamps",
        description: "Toggle message timestamps on/off",
        insert_text: None,
    },
    SlashCommand {
        name: "transcript",
        description: "View the conversation transcript in your pager ($PAGER)",
        insert_text: None,
    },
    SlashCommand {
        name: "tutorial",
        description: "Quick tips to get the most out of Grok Build",
        insert_text: None,
    },
    SlashCommand {
        name: "usage",
        description: "View usage",
        insert_text: None,
    },
    SlashCommand {
        name: "view-plan",
        description: "View the current plan",
        insert_text: None,
    },
    SlashCommand {
        name: "vim-mode",
        description: "Toggle vim-style scrollback keybindings (j/k, h/l, g/G, y/Y, …)",
        insert_text: None,
    },
    SlashCommand {
        name: "workflows",
        description: "Browse installed workflows",
        insert_text: None,
    },
];

pub(crate) const CURSOR: &[SlashCommand] = &[
    SlashCommand {
        name: "model",
        description: "Select the model for this chat",
        insert_text: None,
    },
    SlashCommand {
        name: "goal",
        description: "Create or show the current goal",
        insert_text: None,
    },
    SlashCommand {
        name: "add-dir",
        description: "Add a workspace directory",
        insert_text: None,
    },
    SlashCommand {
        name: "save-workspace",
        description: "Save the current workspace directories",
        insert_text: None,
    },
    SlashCommand {
        name: "load-workspace",
        description: "Load a saved workspace",
        insert_text: None,
    },
    SlashCommand {
        name: "run-everything",
        description: "Toggle Run Everything",
        insert_text: None,
    },
    SlashCommand {
        name: "auto-review",
        description: "Toggle automatic review",
        insert_text: None,
    },
    SlashCommand {
        name: "plan",
        description: "Create or show a plan",
        insert_text: None,
    },
    SlashCommand {
        name: "ask",
        description: "Toggle read-only ask mode",
        insert_text: None,
    },
    SlashCommand {
        name: "debug",
        description: "Toggle debug mode or submit a debug prompt",
        insert_text: None,
    },
    SlashCommand {
        name: "logs",
        description: "Show the current debug log path",
        insert_text: None,
    },
    SlashCommand {
        name: "update",
        description: "Update Cursor Agent",
        insert_text: None,
    },
    SlashCommand {
        name: "max-mode",
        description: "Toggle max mode",
        insert_text: None,
    },
    SlashCommand {
        name: "fast",
        description: "Toggle fast mode",
        insert_text: None,
    },
    SlashCommand {
        name: "rename",
        description: "Rename this chat session",
        insert_text: None,
    },
    SlashCommand {
        name: "clear",
        description: "Start a new chat session",
        insert_text: None,
    },
    SlashCommand {
        name: "resume",
        description: "Resume a previous chat",
        insert_text: None,
    },
    SlashCommand {
        name: "fork",
        description: "Fork this chat into a new session",
        insert_text: None,
    },
    // CDXC:SessionChat 2026-09-15 DECISION: User: offer Cursor's /compact in chat; Cursor already handles it as an alias for /summarize.
    SlashCommand {
        name: "compact",
        description: "Summarize the conversation to reduce context (alias for /summarize)",
        insert_text: None,
    },
    SlashCommand {
        name: "summarize",
        description: "Summarize the conversation to reduce context",
        insert_text: None,
    },
    SlashCommand {
        name: "rewind",
        description: "Jump back to a previous message",
        insert_text: None,
    },
    SlashCommand {
        name: "vim",
        description: "Toggle Vim keys",
        insert_text: None,
    },
    SlashCommand {
        name: "line-numbers",
        description: "Toggle line numbers in code blocks",
        insert_text: None,
    },
    SlashCommand {
        name: "show-thinking",
        description: "Toggle thinking block display",
        insert_text: None,
    },
    SlashCommand {
        name: "status-indicators",
        description: "Toggle terminal title status indicators",
        insert_text: None,
    },
    SlashCommand {
        name: "shell",
        description: "Enter Shell Mode or run a shell command",
        insert_text: None,
    },
    SlashCommand {
        name: "about",
        description: "Show CLI, system, and account information",
        insert_text: None,
    },
    SlashCommand {
        name: "help",
        description: "Show help for a command",
        insert_text: None,
    },
    SlashCommand {
        name: "feedback",
        description: "Share feedback with Cursor",
        insert_text: None,
    },
    SlashCommand {
        name: "open",
        description: "Open the repository in Cursor",
        insert_text: None,
    },
    SlashCommand {
        name: "copy-request-id",
        description: "Copy the last request ID",
        insert_text: None,
    },
    SlashCommand {
        name: "copy-conversation-id",
        description: "Copy this conversation ID",
        insert_text: None,
    },
    SlashCommand {
        name: "logout",
        description: "Sign out from Cursor",
        insert_text: None,
    },
    SlashCommand {
        name: "quit",
        description: "Exit Cursor Agent",
        insert_text: None,
    },
    SlashCommand {
        name: "mcp",
        description: "Manage MCP servers",
        insert_text: None,
    },
    SlashCommand {
        name: "plugin",
        description: "Manage Cursor plugins",
        insert_text: None,
    },
    SlashCommand {
        name: "config",
        description: "Configure CLI settings",
        insert_text: None,
    },
    SlashCommand {
        name: "copy",
        description: "Copy a previous message",
        insert_text: None,
    },
    SlashCommand {
        name: "sandbox",
        description: "Configure the sandbox",
        insert_text: None,
    },
    SlashCommand {
        name: "bedrock",
        description: "Configure Amazon Bedrock",
        insert_text: None,
    },
    SlashCommand {
        name: "changes",
        description: "Review conversation and working-tree changes",
        insert_text: None,
    },
    SlashCommand {
        name: "commit",
        description: "Ask the agent to stage and commit changes",
        insert_text: None,
    },
    SlashCommand {
        name: "jobs",
        description: "Open the active task list",
        insert_text: None,
    },
    SlashCommand {
        name: "rule",
        description: "Manage Cursor rules",
        insert_text: None,
    },
    SlashCommand {
        name: "command",
        description: "Manage custom commands",
        insert_text: None,
    },
    SlashCommand {
        name: "usage",
        description: "Show plan and on-demand usage",
        insert_text: None,
    },
    SlashCommand {
        name: "skills",
        description: "Open the skills menu",
        insert_text: None,
    },
    SlashCommand {
        name: "btw",
        description: "Ask a side question without disrupting the main chat",
        insert_text: None,
    },
    SlashCommand {
        name: "full-conversation",
        description: "Toggle full conversation rendering",
        insert_text: None,
    },
    SlashCommand {
        name: "sync-theme",
        description: "Re-detect the terminal theme",
        insert_text: None,
    },
    SlashCommand {
        name: "context",
        description: "Show context usage details",
        insert_text: None,
    },
];

pub(crate) const HERMES: &[SlashCommand] = &[
    SlashCommand {
        name: "new",
        description: "Start a new session",
        insert_text: None,
    },
    SlashCommand {
        name: "clear",
        description: "Clear screen and start a new session",
        insert_text: None,
    },
    SlashCommand {
        name: "history",
        description: "Show conversation history",
        insert_text: None,
    },
    SlashCommand {
        name: "save",
        description: "Export the current conversation",
        insert_text: None,
    },
    SlashCommand {
        name: "retry",
        description: "Retry the last message",
        insert_text: None,
    },
    SlashCommand {
        name: "undo",
        description: "Back up N user turns and re-prompt",
        insert_text: None,
    },
    SlashCommand {
        name: "title",
        description: "Set a title for the current session",
        insert_text: None,
    },
    SlashCommand {
        name: "branch",
        description: "Branch the current session",
        insert_text: None,
    },
    SlashCommand {
        name: "compact",
        description: "Compress conversation context",
        insert_text: None,
    },
    SlashCommand {
        name: "model",
        description: "Switch model",
        insert_text: None,
    },
    SlashCommand {
        name: "reasoning",
        description: "Manage reasoning effort and display",
        insert_text: None,
    },
    SlashCommand {
        name: "status",
        description: "Show session, model, token, and context info",
        insert_text: None,
    },
    SlashCommand {
        name: "context",
        description: "Show detailed context window view",
        insert_text: None,
    },
    SlashCommand {
        name: "skills",
        description: "Search, install, inspect, or manage skills",
        insert_text: None,
    },
    SlashCommand {
        name: "tools",
        description: "Manage tools",
        insert_text: None,
    },
    SlashCommand {
        name: "memory",
        description: "Review pending memory writes",
        insert_text: None,
    },
    SlashCommand {
        name: "usage",
        description: "Show token usage and rate limits",
        insert_text: None,
    },
    SlashCommand {
        name: "help",
        description: "Show available commands",
        insert_text: None,
    },
    SlashCommand {
        name: "quit",
        description: "Exit the CLI",
        insert_text: None,
    },
];

/// Pi's `BUILTIN_SLASH_COMMANDS` (`@earendil-works/pi-coding-agent` 1.3.0, `dist/core/slash-commands.js`), in Pi's order.
pub(crate) const PI: &[SlashCommand] = &[
    SlashCommand {
        name: "settings",
        description: "Open settings menu",
        insert_text: None,
    },
    SlashCommand {
        name: "model",
        description: "Select model",
        insert_text: None,
    },
    SlashCommand {
        name: "tree",
        description: "Navigate session tree (switch branches)",
        insert_text: None,
    },
    SlashCommand {
        name: "thinking",
        description: "Set thinking level",
        insert_text: None,
    },
    SlashCommand {
        name: "scoped-models",
        description: "Enable/disable models for Ctrl+P cycling",
        insert_text: None,
    },
    SlashCommand {
        name: "export",
        description: "Export session (HTML default, or specify path: .html/.jsonl)",
        insert_text: None,
    },
    SlashCommand {
        name: "import",
        description: "Import and resume a session from a JSONL file",
        insert_text: None,
    },
    SlashCommand {
        name: "share",
        description: "Share session via a Radius artifact or a GitHub gist",
        insert_text: None,
    },
    SlashCommand {
        name: "bug",
        description: "Report a bug to the Pi developers",
        insert_text: None,
    },
    SlashCommand {
        name: "copy",
        description: "Copy last agent message to clipboard",
        insert_text: None,
    },
    SlashCommand {
        name: "name",
        description: "Set session display name",
        insert_text: None,
    },
    SlashCommand {
        name: "session",
        description: "Show session info and stats",
        insert_text: None,
    },
    SlashCommand {
        name: "changelog",
        description: "Show changelog entries",
        insert_text: None,
    },
    SlashCommand {
        name: "hotkeys",
        description: "Show all keyboard shortcuts",
        insert_text: None,
    },
    SlashCommand {
        name: "fork",
        description: "Create a new fork from a previous user message",
        insert_text: None,
    },
    SlashCommand {
        name: "clone",
        description: "Duplicate the current session at the current position",
        insert_text: None,
    },
    SlashCommand {
        name: "trust",
        description: "Save project trust decision for future sessions",
        insert_text: None,
    },
    SlashCommand {
        name: "login",
        description: "Configure provider authentication",
        insert_text: None,
    },
    SlashCommand {
        name: "logout",
        description: "Remove provider authentication",
        insert_text: None,
    },
    SlashCommand {
        name: "new",
        description: "Start a new session",
        insert_text: None,
    },
    SlashCommand {
        name: "compact",
        description: "Manually compact the session context",
        insert_text: None,
    },
    SlashCommand {
        name: "resume",
        description: "Resume a different session",
        insert_text: None,
    },
    SlashCommand {
        name: "reload",
        description: "Reload keybindings, extensions, skills, prompts, themes, and context files",
        insert_text: None,
    },
    SlashCommand {
        name: "quit",
        description: "Quit Pi",
        insert_text: None,
    },
];

/// Copilot CLI's interactive commands (`copilot help commands`, GitHub Copilot CLI 1.0.98), in Copilot's order.
pub(crate) const COPILOT: &[SlashCommand] = &[
    SlashCommand {
        name: "init",
        description: "Initialize Copilot instructions for this repository",
        insert_text: None,
    },
    SlashCommand {
        name: "agent",
        description: "Browse and select agents: /agent [name]",
        insert_text: None,
    },
    SlashCommand {
        name: "skills",
        description: "Manage skills for enhanced capabilities",
        insert_text: None,
    },
    SlashCommand {
        name: "mcp",
        description: "Manage MCP server configuration",
        insert_text: None,
    },
    SlashCommand {
        name: "plugin",
        description: "Manage plugins and plugin marketplaces",
        insert_text: None,
    },
    SlashCommand {
        name: "model",
        description: "Select the AI model for this session (use 'auto' to let Copilot pick automatically). Use /config model to set the user default, '--repo'/'--local' to set the repo default, or 'plan'/'--plan' to set the plan-mode model.",
        insert_text: None,
    },
    SlashCommand {
        name: "delegate",
        description: "Send this session to GitHub and Copilot will create a PR; use --base to choose the PR target branch",
        insert_text: None,
    },
    SlashCommand {
        name: "fleet",
        description: "Enable fleet mode for parallel subagent execution",
        insert_text: None,
    },
    SlashCommand {
        name: "autopilot",
        description: "Toggle autopilot mode, or set an autopilot objective with an optional AI-credit limit (--max-ai-credits)",
        insert_text: None,
    },
    SlashCommand {
        name: "tasks",
        description: "View and manage tasks (subagents and shell commands)",
        insert_text: None,
    },
    SlashCommand {
        name: "ide",
        description: "Connect to an IDE workspace",
        insert_text: None,
    },
    SlashCommand {
        name: "diff",
        description: "Review the changes made in the current directory",
        insert_text: None,
    },
    SlashCommand {
        name: "pr",
        description: "Operate on pull requests for the current branch",
        insert_text: None,
    },
    SlashCommand {
        name: "review",
        description: "Run code review agent to analyze changes",
        insert_text: None,
    },
    SlashCommand {
        name: "security-review",
        description: "Analyze staged and unstaged changes for security vulnerabilities.",
        insert_text: None,
    },
    SlashCommand {
        name: "rubber-duck",
        description: "Get an independent critique of your current work from the rubber duck agent",
        insert_text: None,
    },
    SlashCommand {
        name: "lsp",
        description: "Manage language server configuration",
        insert_text: None,
    },
    SlashCommand {
        name: "terminal-setup",
        description: "Configure terminal for multiline input support (shift+enter)",
        insert_text: None,
    },
    SlashCommand {
        name: "permissions",
        description: "Switch between permission modes",
        insert_text: None,
    },
    SlashCommand {
        name: "allow-all",
        description: "Enable all permissions (tools, paths, and URLs)",
        insert_text: None,
    },
    SlashCommand {
        name: "sandbox",
        description: "Manage sandbox settings, including file path permissions, network permissions, and more",
        insert_text: None,
    },
    SlashCommand {
        name: "add-dir",
        description: "Allow file access to a directory and load its .github skills and agents as trusted configuration",
        insert_text: None,
    },
    SlashCommand {
        name: "list-dirs",
        description: "Display allowed directories and exact session path grants",
        insert_text: None,
    },
    SlashCommand {
        name: "cwd",
        description: "Change working directory or show current directory",
        insert_text: None,
    },
    SlashCommand {
        name: "reset-allowed-tools",
        description: "Reset session tool and exact-path approvals",
        insert_text: None,
    },
    SlashCommand {
        name: "resume",
        description: "Switch sessions by local or cloud history ID, task ID, or name",
        insert_text: None,
    },
    SlashCommand {
        name: "rename",
        description: "Rename the current session, or auto-generate a name from conversation",
        insert_text: None,
    },
    SlashCommand {
        name: "fork",
        description: "Fork the current session into a new session, optionally with a name. Use `/fork worktree` to fork into a new worktree based on the current branch.",
        insert_text: None,
    },
    SlashCommand {
        name: "worktree",
        description: "Create a new git worktree from your configured base ref and switch into it, leaving your uncommitted changes behind. Use `/new worktree` for an empty session or `/fork worktree` to preserve the current context. The deprecated `/worktree new` command still works for now.",
        insert_text: None,
    },
    SlashCommand {
        name: "move",
        description: "Move your uncommitted changes into a new git worktree and switch into it. Pass a branch name, a task to start, or nothing to auto-name.",
        insert_text: None,
    },
    SlashCommand {
        name: "context",
        description: "Show context window token usage and visualization",
        insert_text: None,
    },
    SlashCommand {
        name: "usage",
        description: "Display session usage metrics and statistics",
        insert_text: None,
    },
    SlashCommand {
        name: "session",
        description: "View and manage sessions. Use subcommands for details.",
        insert_text: None,
    },
    SlashCommand {
        name: "compact",
        description: "Summarize conversation history to reduce context window usage. Optionally provide focus instructions.",
        insert_text: None,
    },
    SlashCommand {
        name: "share",
        description: "Share session or research report to a markdown file, HTML file, GitHub gist, or a shareable GitHub link",
        insert_text: None,
    },
    SlashCommand {
        name: "remote",
        description: "Share this session: remote control from GitHub web and mobile, or directly with another CLI",
        insert_text: None,
    },
    SlashCommand {
        name: "copy",
        description: "Copy the last response to the clipboard",
        insert_text: None,
    },
    SlashCommand {
        name: "rewind",
        description: "Rewind the last turn and revert file changes",
        insert_text: None,
    },
    SlashCommand {
        name: "help",
        description: "Show help for interactive commands",
        insert_text: None,
    },
    SlashCommand {
        name: "changelog",
        description: "Display changelog for CLI versions. Add 'summarize' to get an AI summary.",
        insert_text: None,
    },
    SlashCommand {
        name: "feedback",
        description: "Provide feedback about the CLI",
        insert_text: None,
    },
    SlashCommand {
        name: "diagnose",
        description: "Analyze the current session log, optionally with a custom prompt",
        insert_text: None,
    },
    SlashCommand {
        name: "theme",
        description: "View or set color mode",
        insert_text: None,
    },
    SlashCommand {
        name: "statusline",
        description: "Configure status line items",
        insert_text: None,
    },
    SlashCommand {
        name: "footer",
        description: "Configure status line items",
        insert_text: None,
    },
    SlashCommand {
        name: "update",
        description: "Update the CLI to the latest version",
        insert_text: None,
    },
    SlashCommand {
        name: "version",
        description: "Display version information and check for updates",
        insert_text: None,
    },
    SlashCommand {
        name: "experimental",
        description: "Show available experimental features, or enable/disable experimental mode",
        insert_text: None,
    },
    SlashCommand {
        name: "memory",
        description: "Show memory status, or enable/disable memory across sessions",
        insert_text: None,
    },
    SlashCommand {
        name: "clear",
        description: "Abandon this session and start fresh",
        insert_text: None,
    },
    SlashCommand {
        name: "instructions",
        description: "View and toggle custom instruction files",
        insert_text: None,
    },
    SlashCommand {
        name: "app",
        description: "Prefer a visual workspace? Try out the GitHub Copilot desktop app",
        insert_text: None,
    },
    SlashCommand {
        name: "ask",
        description: "Ask a side question or reopen a recently cached answer without adding to history",
        insert_text: None,
    },
    SlashCommand {
        name: "chronicle",
        description: "Session history tools and insights",
        insert_text: None,
    },
    SlashCommand {
        name: "collect-debug-logs",
        description: "Collect debug logs to a local .tgz file, share with GitHub after consent, or create a secret GitHub gist",
        insert_text: None,
    },
    SlashCommand {
        name: "computer",
        description: "Show or toggle Computer Use",
        insert_text: None,
    },
    SlashCommand {
        name: "env",
        description: "Show loaded environment details (instructions, MCP servers, skills, agents, hooks, plugins, LSPs, extensions)",
        insert_text: None,
    },
    SlashCommand {
        name: "exit",
        description: "Exit the CLI; use 'print' to print the session after exiting alt screen",
        insert_text: None,
    },
    SlashCommand {
        name: "keep-alive",
        description: "Manage keep-alive mode (prevents system sleep).",
        insert_text: None,
    },
    SlashCommand {
        name: "limits",
        description: "View or edit session limits; the AI Credit limit is a soft cap",
        insert_text: None,
    },
    SlashCommand {
        name: "login",
        description: "Log in to Copilot",
        insert_text: None,
    },
    SlashCommand {
        name: "logout",
        description: "Log out of an OAuth login session",
        insert_text: None,
    },
    SlashCommand {
        name: "new",
        description: "Start a new conversation",
        insert_text: None,
    },
    SlashCommand {
        name: "plan",
        description: "Create an implementation plan before coding",
        insert_text: None,
    },
    SlashCommand {
        name: "refine",
        description: "Rewrite a rough, stream-of-consciousness prompt into a clear one for review (Ctrl+X / then /refine cleans up your current input)",
        insert_text: None,
    },
    SlashCommand {
        name: "research",
        description: "Run deep research investigation using GitHub search and web sources",
        insert_text: None,
    },
    SlashCommand {
        name: "restart",
        description: "Restart the CLI, restoring supported live sessions in this process",
        insert_text: None,
    },
    SlashCommand {
        name: "search",
        description: "Search the conversation timeline (experimental)",
        insert_text: None,
    },
    SlashCommand {
        name: "settings",
        description: "Open the settings UI, show, set, or unset a single value ('/settings unset <key>' removes it). Use '--repo'/'--local' to target repo settings.",
        insert_text: None,
    },
    SlashCommand {
        name: "subagents",
        description: "Configure default and per-agent subagent models",
        insert_text: None,
    },
    SlashCommand {
        name: "user",
        description: "Manage signed-in accounts",
        insert_text: None,
    },
    SlashCommand {
        name: "vim",
        description: "Toggle Vim mode for the input (hjkl/w/b/e/ge/0/^/$/gg/G motions, f/F/t/T/;/, char search, i/a/o insert, r/~/J/x/D/C edit, d/c/y operators, y/p/P yank & put, . repeat, u/ctrl+r undo & redo, counts, esc for normal mode)",
        insert_text: None,
    },
    SlashCommand {
        name: "voice",
        description: "Manage voice mode (dictation transcription via Foundry Local)",
        insert_text: None,
    },
];

/// omp 18.11's own built-in commands (`slash-commands/builtin-*.ts`), minus its debugging,
/// recording and collaboration tools.
pub(crate) const OMP: &[SlashCommand] = &[
    SlashCommand {
        name: "model",
        description: "Switch model for this session",
        insert_text: None,
    },
    SlashCommand {
        name: "switch",
        description: "Switch model for this session; accepts provider/id and :level",
        insert_text: None,
    },
    SlashCommand {
        name: "effort",
        description: "Set reasoning effort (thinking level) for this session",
        insert_text: None,
    },
    SlashCommand {
        name: "fast",
        description: "Toggle fast service",
        insert_text: None,
    },
    SlashCommand {
        name: "plan",
        description: "Toggle plan mode (agent plans before executing)",
        insert_text: None,
    },
    SlashCommand {
        name: "goal",
        description: "Toggle goal mode (persistent autonomous objective for this session)",
        insert_text: None,
    },
    SlashCommand {
        name: "loop",
        description: "Toggle loop mode: re-submit the next prompt after every yield",
        insert_text: None,
    },
    SlashCommand {
        name: "queue",
        description: "Queue a message for after the agent yields",
        insert_text: None,
    },
    SlashCommand {
        name: "btw",
        description: "Ask a side question, or browse this session's BTW history",
        insert_text: None,
    },
    SlashCommand {
        name: "new",
        description: "Start a new session",
        insert_text: None,
    },
    SlashCommand {
        name: "clear",
        description: "Clear the conversation context in place, keeping the session",
        insert_text: None,
    },
    SlashCommand {
        name: "compact",
        description: "Manually compact the session context",
        insert_text: None,
    },
    SlashCommand {
        name: "shake",
        description: "Drop heavy content from context (tool results, large blocks)",
        insert_text: None,
    },
    SlashCommand {
        name: "handoff",
        description: "Summarize the session into a handoff document and compact in place",
        insert_text: None,
    },
    SlashCommand {
        name: "retry",
        description: "Retry the last failed agent turn",
        insert_text: None,
    },
    SlashCommand {
        name: "rename",
        description: "Rename the current session (omit title to generate)",
        insert_text: None,
    },
    SlashCommand {
        name: "resume",
        description: "Resume a different session",
        insert_text: None,
    },
    SlashCommand {
        name: "branch",
        description: "Rewind to a previous message, keeping the old path as a branch",
        insert_text: None,
    },
    SlashCommand {
        name: "fork",
        description: "Fork this session, or open it in a multiplexer pane or window",
        insert_text: None,
    },
    SlashCommand {
        name: "tree",
        description: "Navigate session tree (switch branches)",
        insert_text: None,
    },
    SlashCommand {
        name: "todo",
        description: "View or modify the agent's todo list",
        insert_text: None,
    },
    SlashCommand {
        name: "context",
        description: "Show estimated context usage breakdown",
        insert_text: None,
    },
    SlashCommand {
        name: "usage",
        description: "Show provider usage and limits",
        insert_text: None,
    },
    SlashCommand {
        name: "jobs",
        description: "Show async background jobs status",
        insert_text: None,
    },
    SlashCommand {
        name: "tools",
        description: "Show tools currently visible to the agent",
        insert_text: None,
    },
    SlashCommand {
        name: "skills",
        description: "Search, install, and update skills from the skills.omp.sh registry",
        insert_text: None,
    },
    SlashCommand {
        name: "mcp",
        description: "Manage MCP servers (add, list, remove, test)",
        insert_text: None,
    },
    SlashCommand {
        name: "plugins",
        description: "View and manage installed plugins",
        insert_text: None,
    },
    SlashCommand {
        name: "reload-plugins",
        description: "Reload all plugins (skills, commands, hooks, tools, agents, MCP)",
        insert_text: None,
    },
    SlashCommand {
        name: "memory",
        description: "Inspect and operate memory maintenance",
        insert_text: None,
    },
    SlashCommand {
        name: "advisor",
        description: "Toggle the advisor (a second model that reviews each turn)",
        insert_text: None,
    },
    SlashCommand {
        name: "export",
        description: "Export session to HTML file",
        insert_text: None,
    },
    SlashCommand {
        name: "share",
        description: "Share session via an encrypted link",
        insert_text: None,
    },
    SlashCommand {
        name: "copy",
        description: "Pick text or code from the conversation to copy",
        insert_text: None,
    },
    SlashCommand {
        name: "add-dir",
        description: "Add a workspace directory to this session",
        insert_text: None,
    },
    SlashCommand {
        name: "remove-dir",
        description: "Remove a workspace directory from this session",
        insert_text: None,
    },
    SlashCommand {
        name: "dirs",
        description: "List this session's workspace directories",
        insert_text: None,
    },
    SlashCommand {
        name: "wt",
        description: "Move this session into a new worktree, changes included",
        insert_text: None,
    },
    SlashCommand {
        name: "move",
        description: "Move the current session to a different directory",
        insert_text: None,
    },
    SlashCommand {
        name: "settings",
        description: "Open settings menu",
        insert_text: None,
    },
    SlashCommand {
        name: "hotkeys",
        description: "Show all keyboard shortcuts",
        insert_text: None,
    },
    SlashCommand {
        name: "changelog",
        description: "Show changelog entries",
        insert_text: None,
    },
    SlashCommand {
        name: "restart",
        description: "Restart omp with the same launch flags, resuming this session",
        insert_text: None,
    },
    SlashCommand {
        name: "exit",
        description: "Exit the application",
        insert_text: None,
    },
];

pub(crate) const FALLBACK: &[SlashCommand] = &[
    SlashCommand {
        name: "clear",
        description: "Clear conversation history",
        insert_text: None,
    },
    SlashCommand {
        name: "compact",
        description: "Compact conversation history",
        insert_text: None,
    },
    SlashCommand {
        name: "exit",
        description: "Exit the agent",
        insert_text: None,
    },
    SlashCommand {
        name: "help",
        description: "Show help and available commands",
        insert_text: None,
    },
    SlashCommand {
        name: "model",
        description: "Switch model",
        insert_text: None,
    },
];
