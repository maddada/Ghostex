// Per-agent slash-command catalogs for the composer's "/" picker.
// Re-verified 2026-09-05 against Codex 0.153.4 and Claude Code 2.1.260. They are a curated snapshot, not discovered at runtime,
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
  /** Complete a composer-native action without sending a CLI command. */
  readonly insertText?: string;
}

const CLAUDE_CODE_SLASH_COMMANDS: readonly SessionChatSlashCommand[] = [
  { name: 'add-dir', description: 'Add a new working directory' },
  { name: 'advisor', description: 'Choose a stronger model Claude can consult' },
  { name: 'auto-mode-setup', description: 'Teach auto mode about your environment' },
  { name: 'autocompact', description: 'Set how full the context gets before auto-summarizing' },
  { name: 'autofix-pr', description: 'Monitor and fix issues with the current PR' },
  { name: 'background', description: 'Send this session to the background' },
  { name: 'batch', description: 'Plan a large change across parallel worktree agents' },
  { name: 'branch', description: 'Create a branch of the conversation at this point' },
  { name: 'brief', description: 'Toggle brief-only mode' },
  { name: 'btw', description: 'Ask a side question without interrupting the main conversation' },
  { name: 'bug', description: 'Report a bug or share your conversation' },
  { name: 'cd', description: 'Move this session to a new working directory' },
  { name: 'chrome', description: 'Manage Claude in Chrome settings' },
  { name: 'claude-api', description: 'Reference the Claude API and Anthropic SDK' },
  { name: 'clear', description: 'Start a new session with empty context' },
  { name: 'color', description: 'Set the prompt bar color for this session' },
  { name: 'compact', description: 'Free up context by summarizing the conversation' },
  { name: 'config', description: 'Open settings' },
  { name: 'context', description: 'Show current context usage' },
  { name: 'copy', description: "Copy Claude's last response to the clipboard" },
  { name: 'debug', description: 'Enable session debug logging and diagnose issues' },
  { name: 'design', description: 'Manage access to Claude Design projects' },
  { name: 'design-login', description: 'Authorize access for Design sync' },
  { name: 'design-sync', description: 'Upload a React design system to Claude Design' },
  { name: 'desktop', description: 'Continue this session in Claude Desktop' },
  { name: 'diff', description: 'Toggle the terminal diff panel' },
  { name: 'doctor', description: 'Diagnose installation issues' },
  { name: 'effort', description: 'Set the effort level for model usage' },
  { name: 'exit', description: 'Exit the REPL' },
  { name: 'export', description: 'Export the conversation to a file or clipboard' },
  { name: 'fast', description: 'Toggle fast mode' },
  { name: 'feedback', description: 'Send feedback to Anthropic' },
  { name: 'fewer-permission-prompts', description: 'Generate project permission rules from common tool calls' },
  { name: 'focus', description: 'Toggle focus view' },
  { name: 'fork', description: 'Spawn a background agent with the full conversation' },
  { name: 'goal', description: 'Set a goal Claude checks before stopping' },
  { name: 'help', description: 'Show help and available commands' },
  { name: 'hooks', description: 'View hook configurations' },
  { name: 'ide', description: 'Manage IDE integrations and show status' },
  { name: 'import', description: 'Import config from another AI coding agent' },
  { name: 'init', description: 'Create a CLAUDE.md for the project' },
  { name: 'insights', description: 'Analyze your Claude Code sessions' },
  { name: 'install-github-app', description: 'Set up Claude GitHub Actions' },
  { name: 'install-slack-app', description: 'Install the Claude Slack app' },
  { name: 'keybindings', description: 'Open the keyboard shortcuts file' },
  { name: 'list-agents', description: 'List sessions you can message' },
  { name: 'login', description: 'Switch Anthropic accounts' },
  { name: 'logout', description: 'Sign out from your Anthropic account' },
  { name: 'mcp', description: 'Manage MCP servers' },
  { name: 'memory', description: 'Edit CLAUDE.md files and memory settings' },
  { name: 'mobile', description: 'Show the Claude mobile app download link' },
  { name: 'model', description: 'Set the AI model for Claude Code' },
  { name: 'passes', description: 'Manage referral passes and usage credits' },
  { name: 'permissions', description: 'Manage allow and deny tool permission rules' },
  { name: 'plan', description: 'Enable plan mode or view the session plan' },
  { name: 'plugin', description: 'Manage Claude Code plugins' },
  { name: 'powerup', description: 'Explore interactive Claude Code lessons' },
  { name: 'privacy-settings', description: 'Review and change data privacy settings' },
  { name: 'radio', description: 'Play Claude FM radio' },
  { name: 'recap', description: 'Generate a one-line session recap' },
  { name: 'release-notes', description: 'View release notes' },
  { name: 'reload-plugins', description: 'Activate pending plugin changes' },
  { name: 'reload-skills', description: 'Pick up skills added or changed on disk' },
  { name: 'remote-control', description: 'Control this session from your phone or Claude on the web' },
  { name: 'remote-env', description: 'Choose the default cloud agent environment' },
  { name: 'rename', description: 'Rename the current conversation' },
  { name: 'resume', description: 'Resume a previous conversation' },
  { name: 'rewind', description: 'Restore the code and/or conversation to a previous point' },
  { name: 'run', description: 'Launch and inspect the project app' },
  { name: 'run-skill-generator', description: 'Create or improve the project run skill' },
  { name: 'sandbox', description: 'Configure sandbox mode and overrides' },
  { name: 'schedule', description: 'Manage scheduled cloud agents' },
  { name: 'scroll-speed', description: 'Adjust terminal mouse wheel scroll speed' },
  { name: 'security-review', description: 'Review changes for security issues' },
  { name: 'skill-doctor', description: 'Show which loaded skills are unused' },
  { name: 'skills', description: 'List available skills' },
  { name: 'status', description: 'Show version, model, account, and connectivity' },
  { name: 'statusline', description: 'Set up the status line' },
  { name: 'stickers', description: 'Open the Claude Code sticker order page' },
  { name: 'stop', description: 'Stop this background session' },
  { name: 'subtask', description: 'Send a subagent off with your full context' },
  { name: 'tasks', description: 'View and manage everything running in the background' },
  { name: 'team-onboarding', description: 'Create a team onboarding guide from your usage' },
  { name: 'teleport', description: 'Send this session to the cloud' },
  { name: 'terminal-setup', description: 'Install the Shift+Enter key binding' },
  { name: 'theme', description: 'Change the theme' },
  { name: 'tui', description: 'Set the terminal UI renderer' },
  { name: 'ultrareview', description: 'Start a paid cloud review to find and verify bugs' },
  { name: 'update-config', description: 'Configure Claude Code settings' },
  { name: 'upgrade', description: 'View plan upgrade options' },
  { name: 'usage', description: 'Show session cost, plan usage, and activity stats' },
  { name: 'usage-credits', description: 'Configure additional usage credits' },
  { name: 'voice', description: 'Configure terminal voice input' },
  { name: 'web-setup', description: 'Connect GitHub to Claude Code on the web' },
  { name: 'workflows', description: 'Browse running and completed workflows' },
];

// Audited against Codex 0.153.4 and upstream 459a79eb on 2026-09-05.
const CODEX_SLASH_COMMANDS: readonly SessionChatSlashCommand[] = [
  { name: 'agents', description: 'View and switch between agent sessions' },
  { name: 'app', description: 'Continue in Codex Desktop' },
  { name: 'approve', description: 'Approve one auto-review retry' },
  { name: 'apps', description: 'Manage apps' },
  { name: 'archive', description: 'Archive this session and exit' },
  { name: 'btw', description: 'Start a side conversation in an ephemeral fork' },
  { name: 'cd', description: 'Change the working directory' },
  { name: 'clear', description: 'Clear the terminal and start a new chat' },
  { name: 'compact', description: 'Summarize conversation to save context' },
  { name: 'copy', description: 'Copy the last response, code block, or quote' },
  { name: 'debug-config', description: 'Show config layers and requirement sources' },
  { name: 'delete', description: 'Delete this session and exit' },
  { name: 'diff', description: 'Show git diff (including untracked files)' },
  { name: 'exit', description: 'Exit Codex' },
  { name: 'experimental', description: 'Toggle experimental features' },
  { name: 'export', description: 'Export the conversation as markdown' },
  { name: 'feedback', description: 'Send logs to maintainers' },
  { name: 'fork', description: 'Fork the current chat' },
  { name: 'goal', description: 'Set or view the goal' },
  { name: 'hooks', description: 'View lifecycle hooks' },
  { name: 'ide', description: 'Include IDE context' },
  { name: 'import', description: 'Import setup from Claude Code or Cursor' },
  { name: 'init', description: 'Create an AGENTS.md for Codex' },
  { name: 'keymap', description: 'Remap TUI shortcuts' },
  { name: 'logout', description: 'Log out of Codex' },
  { name: 'mcp', description: 'List configured MCP tools' },
  { name: 'memories', description: 'Configure memory use' },
  { name: 'mention', description: 'Mention a file', insertText: '@' },
  { name: 'model', description: 'Choose model and reasoning effort' },
  { name: 'new', description: 'Start a new chat' },
  { name: 'permissions', description: 'Review and change tool permissions' },
  { name: 'personality', description: 'Choose a communication style' },
  { name: 'pets', description: 'Choose or hide the terminal pet' },
  { name: 'plan', description: 'Switch to Plan mode' },
  { name: 'plugins', description: 'Browse plugins' },
  { name: 'ps', description: 'List background terminals' },
  { name: 'pwd', description: 'Show the working directory' },
  { name: 'quit', description: 'Exit Codex' },
  { name: 'raw', description: 'Toggle raw scrollback mode' },
  { name: 'recap', description: 'Summarize the current conversation now' },
  { name: 'rename', description: 'Rename the current session' },
  { name: 'resume', description: 'Resume a saved chat' },
  { name: 'review', description: 'Review current changes and find issues' },
  { name: 'setup-default-sandbox', description: 'Set up the elevated agent sandbox' },
  { name: 'side', description: 'Start a side conversation' },
  { name: 'skills', description: 'Manage and use skills' },
  { name: 'status', description: 'Show session configuration and token usage' },
  { name: 'statusline', description: 'Configure the status line' },
  { name: 'stop', description: 'Stop all background terminals' },
  { name: 'subagents', description: "Switch between this session's subagents" },
  { name: 'theme', description: 'Choose a syntax highlighting theme' },
  { name: 'title', description: 'Configure the terminal title' },
  { name: 'usage', description: 'View account usage' },
  { name: 'vim', description: 'Toggle vim editing mode' },
];

const GROK_SLASH_COMMANDS: readonly SessionChatSlashCommand[] = [
  { name: 'clear', description: 'Clear conversation history' },
  { name: 'compact', description: 'Compact conversation history' },
  { name: 'editor', description: 'Edit prompt in external editor' },
  { name: 'exit', description: 'Exit Grok' },
  { name: 'help', description: 'Show help and available commands' },
  { name: 'init', description: 'Initialize project context' },
  { name: 'login', description: 'Log in' },
  { name: 'logout', description: 'Log out' },
  { name: 'mcp', description: 'Manage MCP servers' },
  { name: 'memory', description: 'Manage memory' },
  { name: 'model', description: 'Switch model' },
  { name: 'models', description: 'List available models' },
  { name: 'settings', description: 'Open settings' },
  { name: 'status', description: 'Show session status' },
];

const CURSOR_SLASH_COMMANDS: readonly SessionChatSlashCommand[] = [
  { name: 'model', description: 'Select the model for this chat' },
  { name: 'goal', description: 'Create or show the current goal' },
  { name: 'add-dir', description: 'Add a workspace directory' },
  { name: 'save-workspace', description: 'Save the current workspace directories' },
  { name: 'load-workspace', description: 'Load a saved workspace' },
  { name: 'run-everything', description: 'Toggle Run Everything' },
  { name: 'auto-review', description: 'Toggle automatic review' },
  { name: 'plan', description: 'Create or show a plan' },
  { name: 'ask', description: 'Toggle read-only ask mode' },
  { name: 'debug', description: 'Toggle debug mode or submit a debug prompt' },
  { name: 'logs', description: 'Show the current debug log path' },
  { name: 'update', description: 'Update Cursor Agent' },
  { name: 'max-mode', description: 'Toggle max mode' },
  { name: 'fast', description: 'Toggle fast mode' },
  { name: 'rename', description: 'Rename this chat session' },
  { name: 'clear', description: 'Start a new chat session' },
  { name: 'resume', description: 'Resume a previous chat' },
  { name: 'fork', description: 'Fork this chat into a new session' },
  { name: 'summarize', description: 'Summarize the conversation to reduce context' },
  { name: 'rewind', description: 'Jump back to a previous message' },
  { name: 'vim', description: 'Toggle Vim keys' },
  { name: 'line-numbers', description: 'Toggle line numbers in code blocks' },
  { name: 'show-thinking', description: 'Toggle thinking block display' },
  { name: 'status-indicators', description: 'Toggle terminal title status indicators' },
  { name: 'shell', description: 'Enter Shell Mode or run a shell command' },
  { name: 'about', description: 'Show CLI, system, and account information' },
  { name: 'help', description: 'Show help for a command' },
  { name: 'feedback', description: 'Share feedback with Cursor' },
  { name: 'open', description: 'Open the repository in Cursor' },
  { name: 'copy-request-id', description: 'Copy the last request ID' },
  { name: 'copy-conversation-id', description: 'Copy this conversation ID' },
  { name: 'logout', description: 'Sign out from Cursor' },
  { name: 'quit', description: 'Exit Cursor Agent' },
  { name: 'mcp', description: 'Manage MCP servers' },
  { name: 'plugin', description: 'Manage Cursor plugins' },
  { name: 'config', description: 'Configure CLI settings' },
  { name: 'copy', description: 'Copy a previous message' },
  { name: 'sandbox', description: 'Configure the sandbox' },
  { name: 'bedrock', description: 'Configure Amazon Bedrock' },
  { name: 'changes', description: 'Review conversation and working-tree changes' },
  { name: 'commit', description: 'Ask the agent to stage and commit changes' },
  { name: 'jobs', description: 'Open the active task list' },
  { name: 'rule', description: 'Manage Cursor rules' },
  { name: 'command', description: 'Manage custom commands' },
  { name: 'usage', description: 'Show plan and on-demand usage' },
  { name: 'skills', description: 'Open the skills menu' },
  { name: 'btw', description: 'Ask a side question without disrupting the main chat' },
  { name: 'full-conversation', description: 'Toggle full conversation rendering' },
  { name: 'sync-theme', description: 'Re-detect the terminal theme' },
  { name: 'context', description: 'Show context usage details' },
];

const HERMES_SLASH_COMMANDS: readonly SessionChatSlashCommand[] = [
  { name: 'new', description: 'Start a new session' },
  { name: 'clear', description: 'Clear screen and start a new session' },
  { name: 'history', description: 'Show conversation history' },
  { name: 'save', description: 'Export the current conversation' },
  { name: 'retry', description: 'Retry the last message' },
  { name: 'undo', description: 'Back up N user turns and re-prompt' },
  { name: 'title', description: 'Set a title for the current session' },
  { name: 'branch', description: 'Branch the current session' },
  { name: 'compact', description: 'Compress conversation context' },
  { name: 'model', description: 'Switch model' },
  { name: 'reasoning', description: 'Manage reasoning effort and display' },
  { name: 'status', description: 'Show session, model, token, and context info' },
  { name: 'context', description: 'Show detailed context window view' },
  { name: 'skills', description: 'Search, install, inspect, or manage skills' },
  { name: 'tools', description: 'Manage tools' },
  { name: 'memory', description: 'Review pending memory writes' },
  { name: 'usage', description: 'Show token usage and rate limits' },
  { name: 'help', description: 'Show available commands' },
  { name: 'quit', description: 'Exit the CLI' },
];

/** Conservative fallback for unrecognized agents (mirrors the default catalog). */
const FALLBACK_SLASH_COMMANDS: readonly SessionChatSlashCommand[] = [
  { name: 'clear', description: 'Clear conversation history' },
  { name: 'compact', description: 'Compact conversation history' },
  { name: 'exit', description: 'Exit the agent' },
  { name: 'help', description: 'Show help and available commands' },
  { name: 'model', description: 'Switch model' },
];

const SLASH_COMMANDS_BY_AGENT: Record<string, readonly SessionChatSlashCommand[]> = {
  claude: CLAUDE_CODE_SLASH_COMMANDS,
  openclaude: CLAUDE_CODE_SLASH_COMMANDS,
  codex: CODEX_SLASH_COMMANDS,
  cursor: CURSOR_SLASH_COMMANDS,
  grok: GROK_SLASH_COMMANDS,
  hermes: HERMES_SLASH_COMMANDS,
};

const SLASH_HEADING_BY_AGENT: Record<string, string> = {
  antigravity: 'Antigravity CLI',
  claude: 'Claude Code',
  codex: 'Codex',
  cursor: 'Cursor CLI',
  grok: 'Grok',
  hermes: 'Hermes Agent',
  openclaude: 'OpenClaude',
};

/** Picker section heading (the agent's display name, "Commands" fallback). */
export function sessionChatSlashHeadingForAgent(agent: string | null | undefined): string {
  if (agent === null || agent === undefined) {
    return 'Commands';
  }
  return SLASH_HEADING_BY_AGENT[agent] ?? 'Commands';
}

export function sessionChatSlashCommandsForAgent(agent: string | null | undefined): readonly SessionChatSlashCommand[] {
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
  query: string
): readonly SessionChatSlashCommand[] {
  if (query === '') {
    return commands;
  }
  const lower = query.toLowerCase();
  const prefixed = commands.filter((command) => command.name.startsWith(lower));
  const substring = commands.filter((command) => !command.name.startsWith(lower) && command.name.includes(lower));
  return [...prefixed, ...substring];
}
