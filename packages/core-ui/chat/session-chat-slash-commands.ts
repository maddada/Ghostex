// Per-agent slash-command catalogs for the composer's "/" picker.
// Re-verified 2026-08-23 against the installed binaries: codex-cli 0.149.0 and
// Claude Code 2.1.241. They are a curated snapshot, not discovered at runtime,
// so update them when an agent CLI adds commands. The names also feed
// classifySessionChatSend so verified commands get "Ran /x" markers instead of
// optimistic echoes.
//
// A WRONG NAME COSTS MORE THAN A MISSING ONE. Offering a command the CLI no
// longer has means the picker hands the user a token that gets submitted as
// literal prose to the model (the 2026-08-23 sweep removed Codex's /undo,
// /approvals and /agent, and Claude's /review, /vim, /cost, /bashes,
// /output-style, /pr-comments and /todos on exactly those grounds). Aliases
// are deliberately omitted where the canonical name is listed — Claude's
// /bashes and /cost are now aliases of /tasks and /usage.
//
// KEEPING THE CODEX LIST CURRENT MATTERS MORE THAN THE OTHERS. Claude Code
// records every command it intercepts as a `local_command` transcript row, so
// chat shows a "Slash command" marker for one this list has never heard of —
// drift there costs the picker an entry and nothing else. Codex records
// NOTHING for an intercepted command, so a missing name is the difference
// between a "Ran /x" row and the chat not reacting at all. An unlisted token
// that Codex does NOT intercept is harmless either way: it is submitted as
// literal text and comes back as a normal user turn.
//
// Re-verify against the binary the user actually runs, not a source clone or
// docs. Both CLIs ship their command tables inside the executable, so `strings`
// on it is the ground truth:
//   strings -a "$(readlink -f "$(which codex)" | xargs dirname)"/../node_modules/@openai/\
//     codex-darwin-arm64/vendor/*/bin/codex   # then grep for a known description
//   strings -a "$(readlink -f "$(which claude)")" | grep -o 'name:"[a-z-]*"'
// Codex's enum lives in tui/src/slash_command.rs, but its `is_visible()` filter
// hides platform- and debug-only entries that must stay out of this list (macOS
// release hides sandbox-add-read-dir, rollout and test-approval;
// debug-m-drop/debug-m-update are marked "DO NOT USE"). Claude's table carries
// `isHidden`/`isEnabled:()=>!1` flags that mean the same thing, and mixes the
// user's own plugin and skill commands in with the built-ins — those are
// per-install, so they never belong here.

export interface SessionChatSlashCommand {
  /** Command name without the leading slash. */
  readonly name: string;
  readonly description: string;
}

const CLAUDE_CODE_SLASH_COMMANDS: readonly SessionChatSlashCommand[] = [
  { name: "add-dir", description: "Add a new working directory" },
  { name: "autocompact", description: "Set how full the context gets before auto-summarizing" },
  { name: "background", description: "Send this session to the background" },
  { name: "branch", description: "Create a branch of the conversation at this point" },
  { name: "brief", description: "Toggle brief-only mode" },
  { name: "bug", description: "Report a bug or share your conversation" },
  { name: "cd", description: "Move this session to a new working directory" },
  { name: "clear", description: "Start a new session with empty context" },
  { name: "compact", description: "Free up context by summarizing the conversation" },
  { name: "config", description: "Open settings" },
  { name: "context", description: "Show current context usage" },
  { name: "copy", description: "Copy Claude's last response to the clipboard" },
  { name: "desktop", description: "Continue this session in Claude Desktop" },
  { name: "doctor", description: "Diagnose installation issues" },
  { name: "effort", description: "Set the effort level for model usage" },
  { name: "exit", description: "Exit the REPL" },
  { name: "export", description: "Export the conversation to a file or clipboard" },
  { name: "fast", description: "Toggle fast mode" },
  { name: "feedback", description: "Send feedback to Anthropic" },
  { name: "focus", description: "Toggle focus view" },
  { name: "fork", description: "Spawn a background agent with the full conversation" },
  { name: "goal", description: "Set a goal Claude checks before stopping" },
  { name: "help", description: "Show help and available commands" },
  { name: "hooks", description: "Manage hook configurations" },
  { name: "ide", description: "Manage IDE integrations and show status" },
  { name: "import", description: "Import config from another AI coding agent" },
  { name: "init", description: "Create a CLAUDE.md for the project" },
  { name: "install-github-app", description: "Set up Claude GitHub Actions" },
  { name: "list-agents", description: "List sessions you can message" },
  { name: "login", description: "Switch Anthropic accounts" },
  { name: "logout", description: "Sign out from your Anthropic account" },
  { name: "mcp", description: "Manage MCP servers" },
  { name: "memory", description: "Edit CLAUDE.md files and memory settings" },
  { name: "model", description: "Set the AI model for Claude Code" },
  { name: "permissions", description: "Manage allow and deny tool permission rules" },
  { name: "plan", description: "Enable plan mode or view the session plan" },
  { name: "plugin", description: "Manage Claude Code plugins" },
  { name: "recap", description: "Generate a one-line session recap" },
  { name: "release-notes", description: "View release notes" },
  { name: "reload-plugins", description: "Activate pending plugin changes" },
  { name: "reload-skills", description: "Pick up skills added or changed on disk" },
  { name: "rename", description: "Rename the current conversation" },
  { name: "resume", description: "Resume a previous conversation" },
  { name: "rewind", description: "Restore the code and/or conversation to a previous point" },
  { name: "security-review", description: "Review changes for security issues" },
  { name: "skill-doctor", description: "Show which loaded skills are unused" },
  { name: "skills", description: "List available skills" },
  { name: "status", description: "Show version, model, account, and connectivity" },
  { name: "statusline", description: "Set up the status line" },
  { name: "stop", description: "Stop this background session" },
  { name: "subtask", description: "Send a subagent off with your full context" },
  { name: "tasks", description: "View and manage everything running in the background" },
  { name: "teleport", description: "Send this session to the cloud" },
  { name: "terminal-setup", description: "Install the Shift+Enter key binding" },
  { name: "theme", description: "Change the theme" },
  { name: "tui", description: "Set the terminal UI renderer" },
  { name: "usage", description: "Show session cost, plan usage, and activity stats" },
  { name: "workflows", description: "Browse running and completed workflows" },
];

const CODEX_SLASH_COMMANDS: readonly SessionChatSlashCommand[] = [
  { name: "agents", description: "View and switch between agent sessions" },
  { name: "app", description: "Continue in Codex Desktop" },
  { name: "approve", description: "Approve one auto-review retry" },
  { name: "apps", description: "Manage apps" },
  { name: "archive", description: "Archive this session and exit" },
  { name: "btw", description: "Start a side conversation in an ephemeral fork" },
  { name: "cd", description: "Change the working directory" },
  { name: "clear", description: "Clear the terminal and start a new chat" },
  { name: "compact", description: "Summarize conversation to save context" },
  { name: "copy", description: "Copy the last response as markdown" },
  { name: "debug-config", description: "Show config layers and requirement sources" },
  { name: "delete", description: "Delete this session and exit" },
  { name: "diff", description: "Show git diff (including untracked files)" },
  { name: "exit", description: "Exit Codex" },
  { name: "experimental", description: "Toggle experimental features" },
  { name: "export", description: "Export the conversation as markdown" },
  { name: "feedback", description: "Send logs to maintainers" },
  { name: "fork", description: "Fork the current chat" },
  { name: "goal", description: "Set or view the goal" },
  { name: "hooks", description: "View lifecycle hooks" },
  { name: "ide", description: "Include IDE context" },
  { name: "import", description: "Import setup from Claude Code" },
  { name: "init", description: "Create an AGENTS.md for Codex" },
  { name: "keymap", description: "Remap TUI shortcuts" },
  { name: "logout", description: "Log out of Codex" },
  { name: "mcp", description: "List configured MCP tools" },
  { name: "memories", description: "Configure memory use" },
  { name: "mention", description: "Mention a file" },
  { name: "model", description: "Choose model and reasoning effort" },
  { name: "new", description: "Start a new chat" },
  { name: "permissions", description: "Review and change tool permissions" },
  { name: "personality", description: "Choose a communication style" },
  { name: "pets", description: "Choose or hide the terminal pet" },
  { name: "plan", description: "Switch to Plan mode" },
  { name: "plugins", description: "Browse plugins" },
  { name: "ps", description: "List background terminals" },
  { name: "pwd", description: "Show the working directory" },
  { name: "quit", description: "Exit Codex" },
  { name: "raw", description: "Toggle raw scrollback mode" },
  { name: "rename", description: "Rename the current session" },
  { name: "resume", description: "Resume a saved chat" },
  { name: "review", description: "Review current changes and find issues" },
  { name: "setup-default-sandbox", description: "Set up the elevated agent sandbox" },
  { name: "side", description: "Start a side conversation" },
  { name: "skills", description: "Manage and use skills" },
  { name: "status", description: "Show session configuration and token usage" },
  { name: "statusline", description: "Configure the status line" },
  { name: "stop", description: "Stop all background terminals" },
  { name: "subagents", description: "Switch between this session's subagents" },
  { name: "theme", description: "Choose a syntax highlighting theme" },
  { name: "title", description: "Configure the terminal title" },
  { name: "usage", description: "View account usage" },
  { name: "vim", description: "Toggle vim editing mode" },
];

const GROK_SLASH_COMMANDS: readonly SessionChatSlashCommand[] = [
  { name: "clear", description: "Clear conversation history" },
  { name: "compact", description: "Compact conversation history" },
  { name: "editor", description: "Edit prompt in external editor" },
  { name: "exit", description: "Exit Grok" },
  { name: "help", description: "Show help and available commands" },
  { name: "init", description: "Initialize project context" },
  { name: "login", description: "Log in" },
  { name: "logout", description: "Log out" },
  { name: "mcp", description: "Manage MCP servers" },
  { name: "memory", description: "Manage memory" },
  { name: "model", description: "Switch model" },
  { name: "models", description: "List available models" },
  { name: "settings", description: "Open settings" },
  { name: "status", description: "Show session status" },
];

/** Conservative fallback for unrecognized agents (mirrors the default catalog). */
const FALLBACK_SLASH_COMMANDS: readonly SessionChatSlashCommand[] = [
  { name: "clear", description: "Clear conversation history" },
  { name: "compact", description: "Compact conversation history" },
  { name: "exit", description: "Exit the agent" },
  { name: "help", description: "Show help and available commands" },
  { name: "model", description: "Switch model" },
];

const SLASH_COMMANDS_BY_AGENT: Record<string, readonly SessionChatSlashCommand[]> = {
  claude: CLAUDE_CODE_SLASH_COMMANDS,
  openclaude: CLAUDE_CODE_SLASH_COMMANDS,
  codex: CODEX_SLASH_COMMANDS,
  grok: GROK_SLASH_COMMANDS,
};

const SLASH_HEADING_BY_AGENT: Record<string, string> = {
  claude: "Claude Code",
  codex: "Codex",
  grok: "Grok",
  openclaude: "OpenClaude",
};

/** Picker section heading (the agent's display name, "Commands" fallback). */
export function sessionChatSlashHeadingForAgent(
  agent: string | null | undefined,
): string {
  if (agent === null || agent === undefined) {
    return "Commands";
  }
  return SLASH_HEADING_BY_AGENT[agent] ?? "Commands";
}

export function sessionChatSlashCommandsForAgent(
  agent: string | null | undefined,
): readonly SessionChatSlashCommand[] {
  if (agent === null || agent === undefined) {
    return FALLBACK_SLASH_COMMANDS;
  }
  return SLASH_COMMANDS_BY_AGENT[agent] ?? FALLBACK_SLASH_COMMANDS;
}

/**
 * The token being completed: the whole draft when it is a single line-leading
 * "/word" with no whitespace yet, else null (picker closed).
 */
export function sessionChatSlashQuery(draft: string): string | null {
  return /^\/[^\s/]*$/.test(draft) ? draft.slice(1) : null;
}

export function filterSessionChatSlashCommands(
  commands: readonly SessionChatSlashCommand[],
  query: string,
): readonly SessionChatSlashCommand[] {
  if (query === "") {
    return commands;
  }
  const lower = query.toLowerCase();
  const prefixed = commands.filter((command) => command.name.startsWith(lower));
  const substring = commands.filter(
    (command) => !command.name.startsWith(lower) && command.name.includes(lower),
  );
  return [...prefixed, ...substring];
}
