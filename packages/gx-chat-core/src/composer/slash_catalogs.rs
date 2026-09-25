//! The six per-agent `/` catalogs, as data.
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
    SlashCommand {
        name: "btw",
        description: "Ask a side question without interrupting the main conversation",
        insert_text: None,
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
        insert_text: None,
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

pub(crate) const GROK: &[SlashCommand] = &[
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
        name: "editor",
        description: "Edit prompt in external editor",
        insert_text: None,
    },
    SlashCommand {
        name: "exit",
        description: "Exit Grok",
        insert_text: None,
    },
    SlashCommand {
        name: "help",
        description: "Show help and available commands",
        insert_text: None,
    },
    SlashCommand {
        name: "init",
        description: "Initialize project context",
        insert_text: None,
    },
    SlashCommand {
        name: "login",
        description: "Log in",
        insert_text: None,
    },
    SlashCommand {
        name: "logout",
        description: "Log out",
        insert_text: None,
    },
    SlashCommand {
        name: "mcp",
        description: "Manage MCP servers",
        insert_text: None,
    },
    SlashCommand {
        name: "memory",
        description: "Manage memory",
        insert_text: None,
    },
    SlashCommand {
        name: "model",
        description: "Switch model",
        insert_text: None,
    },
    SlashCommand {
        name: "models",
        description: "List available models",
        insert_text: None,
    },
    SlashCommand {
        name: "settings",
        description: "Open settings",
        insert_text: None,
    },
    SlashCommand {
        name: "status",
        description: "Show session status",
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
