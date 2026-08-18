/*
CDXC:GhostexRustCli 2026-07-14:
The native Rust CLI owns these user-facing help contracts. Keep focused help
discoverable from the top-level output because agents and installed skills use
it to choose supported gxserver paths without inspecting implementation code.
*/

/// Help rows use a two-space indent, 58-column command column with
/// a minimum two-space gap before the description.
pub fn format_help_command(signature: &str, description: &str) -> String {
    let command_column_width: usize = 58;
    let gap_width = command_column_width
        .saturating_sub(signature.chars().count())
        .max(2);
    format!("  {signature}{}{description}", " ".repeat(gap_width))
}

/// The automations module owns the commands; this file owns their help rows.
fn automation_help_commands() -> Vec<String> {
    vec![
        format_help_command(
            "automation-state [--path path|--project-id id]",
            "Print gxserver automations and run history",
        ),
        format_help_command(
            "automation-save --path path --definition-json json",
            "Create or update a gxserver automation",
        ),
        format_help_command(
            "automation-delete <automationId> --path path",
            "Delete a gxserver automation",
        ),
        format_help_command(
            "automation-run-now <automationId> --path path",
            "Queue a gxserver automation immediately",
        ),
        format_help_command(
            "automation-set-enabled <automationId> <true|false> --path path",
            "Pause or resume a gxserver automation",
        ),
        format_help_command(
            "automation-archive-run --run-id id --path path [--remove-worktree true]",
            "Archive a completed gxserver run",
        ),
        format_help_command(
            "automation-mark-run-read --run-id id --path path",
            "Mark a gxserver run as read",
        ),
    ]
}

pub fn usage() -> String {
    let session_commands = [
        format_help_command(
            "tui [--tui-bin path]",
            "Open the Ghostex terminal TUI",
        ),
        format_help_command(
            "sessions | s | ls [--ungrouped|-u] [--json] [--mobile-summary]",
            "List running terminal sessions",
        ),
        format_help_command(
            "find | f [zehn args...]",
            "Search agent prompt history with bundled zehn",
        ),
        format_help_command(
            "history | h [ghostex-history args...]",
            "View local agent transcripts in the alt-screen history TUI",
        ),
        format_help_command(
            "android-check [--json]",
            "Verify this Mac is ready for Ghostex Android",
        ),
        format_help_command(
            "attach | a [selector]",
            "Attach to a provider session, or open the picker without a selector",
        ),
        format_help_command("resume | r [selector]", "Alias for attach"),
        format_help_command(
            "attach | a --session-id <id> [--project-id id] [--prompt-editor monaco|code-server]",
            "Flag form used by mobile and desktop remote session attach",
        ),
        format_help_command(
            "kill | k <selector|all> [--json]",
            "Close one session or every listed session",
        ),
        format_help_command(
            "sleep <selector|all> [--json]",
            "Sleep one session or every listed session",
        ),
        format_help_command(
            "wake <selector|all> [--json]",
            "Wake one session or every listed session",
        ),
        format_help_command(
            "focus <selector> [--json] | focus --agent-session-id <id> --agent <agent> --if-running",
            "Focus a live Ghostex session, including one owning an agent conversation",
        ),
        format_help_command(
            "(sleep|wake|kill) --session-id <id> [--json]",
            "Flag form used by Android sidebar actions",
        ),
    ]
    .join("\n");

    let workspace_commands = [
        format_help_command("state | dump-state", "Print sidebar state as JSON"),
        format_help_command("open | o <path...>", "Open files or folders in Ghostex"),
        format_help_command(
            "edit | e [--wait] [--goto] <file...>",
            "Open files in embedded Code",
        ),
        format_help_command(
            "terminal | t [--cwd path] [--title title] [-- command...]",
            "Create a Quick terminal",
        ),
        format_help_command(
            "create-session [title] [--input text] [--start] [--project-id id] [--group-id id]",
            "Create a terminal session; --start materializes the live terminal immediately",
        ),
        format_help_command(
            "create-chat [title] [--input text] [--start] --json",
            "Create a Quick chat workspace with its first terminal session",
        ),
        format_help_command(
            "create-agent <agentId> --project-id id [--group-id id]",
            "Create and start a configured agent session",
        ),
        format_help_command(
            "board start-work <bead-id> [--agent id] [--project-id id] [--json]",
            "Dispatch a Project Board bead: reuse its usable linked session or create the worker",
        ),
        format_help_command("run-agent <agentId>", "Run a configured agent button"),
        format_help_command(
            "run-command <commandId>",
            "Trigger a renderer command button; use run-action for project quick actions",
        ),
        format_help_command(
            "click-button <agent|command> <id>",
            "Trigger a renderer sidebar button; use run-action for project quick actions",
        ),
        format_help_command(
            "switch-project (--project-id|--path|--name) <value>",
            "Switch active project",
        ),
        format_help_command(
            "move-project --project-id id --direction up|down",
            "Move a project in the desktop sidebar order",
        ),
        format_help_command(
            "add-project <path> [--name name] [--create-if-missing]",
            "Add a project to Ghostex; --create-if-missing creates the folder first",
        ),
        format_help_command(
            "group-project (--project-id id|--path path|--name name) --group title",
            "Move a project into a named sidebar group, creating the group when needed",
        ),
        format_help_command(
            "browse-directories <partialPath> [--cwd dir] [--limit n] --json",
            "List directory suggestions for an Add Project path input",
        ),
        format_help_command(
            "discover-source-control --json",
            "Report which hosting CLIs (gh/glab) this machine can clone with",
        ),
        format_help_command(
            "lookup-repository <github|gitlab> <owner/repo> --json",
            "Resolve a repository into its clone URLs",
        ),
        format_help_command(
            "clone-repository <remoteUrl> <destinationPath> --json",
            "Clone a repository and register it as a project (waits for the job)",
        ),
        format_help_command(
            "restore-recent-project --project-id id --json",
            "Restore a parked project to the active sidebar",
        ),
        format_help_command(
            "read-sidebar-project-collections --json",
            "Print the durable sidebar project collections state",
        ),
        format_help_command(
            "update-sidebar-project-collections --state-json json --json",
            "Replace the collections state; prints the normalized result",
        ),
        format_help_command(
            "focus-session <id|--index n|--session-number n>",
            "Focus a session by raw selector",
        ),
        format_help_command(
            "acknowledge-session-attention <selector>",
            "Mark a session's shared attention event as seen",
        ),
        format_help_command("focus-group <groupId>", "Focus a project group"),
    ]
    .join("\n");

    let quick_action_commands = [
        format_help_command(
            "quick-actions --help",
            "Show the complete Terminal and Browser quick-action workflow",
        ),
        format_help_command(
            "save-command --command-id id --name name --command command [--path path]",
            "Create or update a Terminal quick action through gxserver",
        ),
        format_help_command(
            "save-command --type browser --command-id id --name name --url url [--path path]",
            "Create or update a Browser quick action through gxserver",
        ),
        format_help_command(
            "run-action <commandId> --project-id id",
            "Run a Terminal action in a session or return a Browser action URL",
        ),
    ]
    .join("\n");

    let automation_commands = {
        let mut lines = vec![
            format_help_command(
                "automations --help",
                "Show the complete scheduled automation workflow and install its skill",
            ),
            format_help_command(
                "save-agent --agent-id id --name name --command command",
                "Unsupported renderer-era agent-button writer; not a project quick action",
            ),
        ];
        lines.extend(automation_help_commands());
        lines.push(format_help_command(
            "bd <args...>",
            "Compatibility passthrough to the machine-installed Beads CLI",
        ));
        lines.join("\n")
    };

    let input_commands = [
        format_help_command("send-text <selector> <text>", "Type text into a session by id or quoted title"),
        format_help_command("send-enter <selector>", "Send Enter to a session by id or quoted title"),
        format_help_command("send-key <selector> <key>", "Send ctrl-c, escape, tab, or arrow keys"),
        format_help_command("send-message <selector> <text>", "Type text and Enter into an existing session"),
        format_help_command("send-message <agentId> <text>", "Unsupported in gxserver cutover until renderer-created visible sessions land"),
        format_help_command("read-text <selector> [--lines n] [--visible] [--json]", "Read terminal text by id or quoted title"),
        format_help_command("read-session-chat <selector> [--limit n] [--before-offset n] [--wait-ms n --fingerprint f] --json", "Read a session's chat transcript; --wait-ms long-polls until the chat changes"),
        format_help_command("read-session-chat-skills <selector> --json", "List skills available to the session's agent"),
        format_help_command("read-session-chat-files <selector> --json", "List the session project's files for @ mentions"),
        format_help_command("send-session-chat-message <selector> <text>", "Send a chat message into an agent session"),
        format_help_command("answer-session-chat-prompt <selector> --answer-json '<json>'", "Answer a pending question/approval prompt"),
        format_help_command("interrupt-session-chat <selector>", "Interrupt the session's running agent turn"),
        format_help_command("wait-for-text <selector> <regex> [--timeout-seconds n] [--interval-seconds n] [--lines n] [--json]", "Poll a session until a scrollback line matches the regex; exits 1 on timeout or dead session"),
        format_help_command("rename-session <sessionId> <title> [--json]", "Rename a session"),
        format_help_command("rename-session --session-id <id> --title <title> [--json]", "Flag form used by Android SSH actions"),
        format_help_command("rename-command <selector> <title>", "Send the agent rename command"),
    ]
    .join("\n");

    let ui_commands = [
        format_help_command(
            "floating-editor | fe -- <editor> [args...]",
            "Open a draggable terminal overlay",
        ),
        format_help_command(
            "floating-monaco-editor | fme <file>",
            "Open the standalone Ghostex Editor app",
        ),
        format_help_command(
            "editor-daemon <ensure|status|warm|shutdown>",
            "Manage the standalone Ghostex Editor daemon",
        ),
        format_help_command(
            "(close|restart|fork|reload)-session <id>",
            "Manage a session lifecycle",
        ),
        format_help_command(
            "sleep-session|pin-session <id> [true|false]",
            "Set raw session flags",
        ),
        format_help_command("tag-session <id> <tag|none>", "Set or clear a session tag"),
        format_help_command(
            "delayed-send <id> (--delay-ms <n> | --when-agent-finishes | --when-all-agents-finish) | --cancel",
            "Arm or cancel a Session Automations Enter trigger",
        ),
        format_help_command(
            "close-after-done <id>",
            "Toggle Close After Done for a session",
        ),
        format_help_command(
            "set-visible-count <1|2|3|4|6|9>",
            "Set visible session count",
        ),
        format_help_command(
            "set-view-mode <grid|horizontal|vertical>",
            "Set session layout mode",
        ),
        format_help_command(
            "browser --help",
            "Show embedded CEF browser control and MCP setup",
        ),
        format_help_command(
            "browser-use --help",
            "Show Cua Driver browser-page skill setup",
        ),
        format_help_command(
            "computer-use --help",
            "Show Ghostex Computer Use skill setup for Cua Driver",
        ),
        format_help_command(
            "cli --help",
            "Show general Ghostex CLI discovery and agent skill setup",
        ),
        format_help_command(
            "agent-orchestration --help",
            "Show Ghostex Agent Orchestration skill setup",
        ),
        format_help_command(
            "fable-5.6-orchestration --help",
            "Show Ghostex Fable 5.6 Orchestration skill setup",
        ),
        format_help_command(
            "find-prev-session --help",
            "Show Ghostex Find Previous Session skill setup",
        ),
        format_help_command(
            "generate-title --help",
            "Show Ghostex Auto Rename Session skill setup",
        ),
        format_help_command(
            "move-codex-session --help",
            "Show Ghostex Move Codex Session skill setup",
        ),
        format_help_command("toggle-sidebar", "Collapse or expand the sidebar"),
        format_help_command("move-sidebar", "Move the sidebar"),
    ]
    .join("\n");

    let server_commands = [
        format_help_command("web", "Open the Ghostex web app in the default browser"),
        format_help_command("server", "Run gxserver in the foreground"),
        format_help_command("server start [--json]", "Start gxserver in the background"),
        format_help_command(
            "server stop [--json]",
            "Stop only the gxserver control plane",
        ),
        format_help_command(
            "server stop-all [--json]",
            "Stop gxserver and kill tracked zmx sessions",
        ),
        format_help_command("server status [--json]", "Print gxserver runtime state"),
        format_help_command(
            "server version | server --version",
            "Print the gxserver package version",
        ),
        format_help_command("server --help", "Show gxserver lifecycle command help"),
    ]
    .join("\n");

    let evidence_commands = [
        format_help_command("screenshot [output.png]", "Capture the Ghostex window"),
        format_help_command(
            "logs [--file name] [--lines n] [--grep text] [--json]",
            "Print recent logs",
        ),
        format_help_command(
            "bundle [output-dir] [--lines n]",
            "Save state, logs, and a screenshot",
        ),
        format_help_command(
            "assert-card <id> [--agent-icon codex] [--visible true]",
            "Assert card projection",
        ),
        format_help_command(
            "wait-for <id> [--agent-icon codex] [--timeout-ms n]",
            "Wait for card projection",
        ),
    ]
    .join("\n");

    format!(
        "Ghostex CLI - manage running Ghostex terminal sessions

Usage:
\t  ghostex
\t  gx
\t  ghostex <path...>
\t  ghostex <command> [args...] [--flags]
\t  gx <command> [args...] [--flags]

Commands:
{session_commands}

Workspace:
{workspace_commands}

Quick actions:
{quick_action_commands}

Automations:
{automation_commands}

Input:
{input_commands}

UI:
{ui_commands}

Server:
{server_commands}

Evidence:
{evidence_commands}

Selectors:
  <selector> can be an alias, session id, provider session name, title, or project:title.
  Numeric aliases come from the last \"ghostex sessions\" or \"gx sessions\" list.
  Titles match exact first, then case-insensitive substring.

Sessions:
  Running ghostex or gx with no subcommand launches or activates the Ghostex desktop app.
  ghostex tui and gx tui open the Ghostex terminal TUI.
  gx find and gx f launch bundled zehn for prompt-history search; gx history and gx h open the transcript viewer.
  The TUI shows the attached session, with a top switch button for project/session switching.
  The switcher lists Ghostex projects and sessions in macOS sidebar order and attaches through the existing zmx path.
  Direct attach stays available through attach/a/resume/r without opening the TUI.
  Projects and sessions follow the macOS sidebar order, including the active Last Active sort mode.
  Each project prints its path once as the section header, then compact session rows without field labels.
  --ungrouped/-u prints one flat list and prefixes each row with the project name.

Attach:
  attach/resume uses the stored tmux, zmx, or zellij provider session when present.
  Without provider metadata, it runs the supported agent resume command in the session project.

Global flags:
  --port <number>       Native bridge port
  --token-stdin         Read a temporary remote gxserver token from stdin
  --token <token>       Bridge token; legacy remote one-shot only because argv can expose secrets
  --timeout <ms>        Bridge request timeout
  automations --help    Show focused scheduled automation help
  cli --help            Show general CLI agent skill setup
  quick-actions --help  Show focused project quick-action help
  server --help         Show server command help
  help                  Show this help
  -h, --help            Show this help
"
    )
}

pub fn cli_usage() -> String {
    "Ghostex CLI Skill - install the help-first agent workflow for Ghostex commands

Usage:
  gx cli --help
  gx cli install-skill [--json]

Agent skill:
  Use $ghostex-cli for general Ghostex CLI work across projects, sessions,
  quick actions, automations, UI controls, servers, and diagnostics.

What the skill teaches:
  Read ghostex --help first, follow focused help where available, inspect state
  with JSON, target stable ids, perform the requested operation, and verify the
  resulting state.

Specialized workflows:
  Use $ghostex-manage-automations, $ghostex-agent-orchestration,
  $ghostex-embedded-browser-use,
  $ghostex-browser-use, or $ghostex-computer-use when their domain applies.
"
    .to_string()
}

pub fn automations_usage() -> String {
    let commands = automation_help_commands().join("\n");
    format!(
        r#"Ghostex Automations - manage scheduled project agent work through gxserver

Usage:
  gx automations --help
  gx automations install-skill [--json]
  gx automation-state --path <project-path>
  gx automation-save --path <project-path> --definition-json '<json>'

Commands:
{commands}

Recommended workflow:
  1. Inspect the project with ghostex sessions --json or ghostex state.
  2. Read automation-state before creating, updating, deleting, or archiving.
  3. Save new automations disabled unless they should begin scheduling now.
  4. Re-read automation-state after every mutation.
  5. After run-now, follow the newest matching run until it leaves queued or running.

Repeating definition JSON:
  {{"name":"Daily review","agentId":"codex","prompt":"Review the project and report actionable findings.","enabled":false,"schedule":{{"kind":"daily","time":"09:00","timezone":"local"}},"executionMode":{{"kind":"local"}}}}

Timer definition JSON:
  {{"name":"Follow up","agentId":"codex","prompt":"Check whether the task finished.","enabled":true,"schedule":{{"kind":"timer","delayMs":1800000}},"executionMode":{{"kind":"local"}}}}

Schedule shapes:
  timer     {{"kind":"timer","delayMs":1800000}} (one-time convenience input; 1,000 ms through 365 days)
  once      {{"kind":"once","runAt":"2026-08-14T09:30:00.000Z"}} (one-time ISO 8601 date)
  interval  {{"kind":"interval","everyMs":3600000}} (60,000 ms through 365 days)
  daily     {{"kind":"daily","time":"09:00","timezone":"local"}}
  weekly    {{"kind":"weekly","days":[1,3,5],"time":"09:00","timezone":"local"}} (days 0-6)
  cron      {{"kind":"cron","expression":"0 9 * * 1-5","timezone":"local"}}

One-time schedules:
  timer is converted to once when saved, anchoring the deadline so server restarts do not reset it.
  timer and once definitions must be enabled to run automatically. After their due run is queued,
  gxserver disables them. A past once date cannot be enabled; save it with a new future runAt.

Execution modes:
  local     {{"kind":"local"}}
  worktree  {{"kind":"worktree","setupCommand":"optional command"}}
  thread    {{"kind":"thread","sessionId":"exact-session-id"}}

Updates:
  Start from the definition returned by automation-state. Preserve its id,
  createdAt, project selection, and unchanged fields, then pass the complete
  edited object to automation-save.

Run results:
  Automation prompts require a final `AUTOMATION_RESULT: <status>` line.
  Replace <status> with exactly findings, no_findings, or needs_attention.
  gxserver records that marker in run history.

Safety:
  Inspect exact automation and run ids before destructive operations.
  Use --remove-worktree true only when the exact archived run worktree should be removed.
  Project automations are separate from the per-session delayed-send and close-after-done controls.
"#
    )
}

pub fn quick_actions_usage() -> String {
    let commands = [
        format_help_command(
            "save-command --command-id id --name name --command command [project flags]",
            "Create or replace a Terminal quick action",
        ),
        format_help_command(
            "save-command --type browser --command-id id --name name --url url [project flags]",
            "Create or replace a Browser quick action",
        ),
        format_help_command(
            "run-action <commandId> --project-id id",
            "Start a Terminal action or return a Browser action URL",
        ),
        format_help_command(
            "state",
            "Print projects and their customCommands/customCommandOrder",
        ),
    ]
    .join("\n");

    format!(
        "Ghostex Quick Actions - manage project Terminal and Browser actions through gxserver

Usage:
  ghostex quick-actions --help
  gx quick-actions --help
  ghostex save-command --command-id <id> --name <name> --command <shell-command> [project flags]
  ghostex save-command --type browser --command-id <id> --name <name> --url <url> [project flags]

Commands:
{commands}

Project selection:
  With no project flag, save-command targets the Ghostex project whose path is the current directory.
  --path <path>          Target the project with this path; use this from outside the project directory.
  --project-id <id>     Target a known project id from ghostex state or ghostex sessions --json.
  --project-name <name> Target an exact project name; prefer path or id when names may repeat.

Terminal quick actions:
  --command <command>              Required shell command.
  --play-completion-sound <bool>   Play a sound after successful completion; default true.
  --close-terminal-on-exit <bool>  Legacy saved metadata only; GPUI keeps completed Action tabs open.

Browser quick actions:
  Pass --type browser and --url <url>. Browser actions do not accept a shell command.

Examples:
  ghostex save-command --command-id dev --name \"Dev Server\" --command \"npm run dev\" --path /path/to/project
  ghostex save-command --type browser --command-id app --name \"Open App\" --url http://localhost:5173 --path /path/to/project
  ghostex run-action dev --project-id P1n8o

Behavior:
  save-command writes to the live gxserver project store and refreshes normal project state.
  Reusing a command id replaces that action definition in the same ordered position; a new id is appended.
  Do not edit workspace-state.json or the Ghostex state database directly.
  Terminal and Browser quick actions are project customCommands, not agent buttons; do not use save-agent.
  Use run-action to execute them; run-command and click-button are renderer-only legacy paths.

Inspect:
  ghostex state
  ghostex state | jq '.projects[] | {{projectId, name, path, customCommands, customCommandOrder}}'
"
    )
}

/// Render CLI help for Project Board worker dispatch.
pub fn board_usage() -> String {
    let commands = [format_help_command(
        "board start-work <bead-id> [--agent id] [--project-id id] [--json]",
        "Dispatch a Project Board bead through gxserver",
    )]
    .join("\n");

    format!(
        "Ghostex Project Board - dispatch bead work through gxserver

Usage:
  ghostex board start-work <bead-id> [--agent <agentId>] [--project-id <id>] [--json]
  gx board start-work <bead-id>

Commands:
{commands}

Behavior:
  start-work IS the dispatch: it creates and starts the visible worker session with the bead's
  canonical work prompt and links the conversation to the card. Do not also launch a worker yourself.
  Repeated calls are safe: an existing usable linked conversation (live, sleeping, or restorable)
  is returned as {{ \"projectId\": ..., \"sessionId\": ..., \"created\": false }} instead of creating a second worker.
  Without --agent, the bead assignee is matched case-insensitively against configured agents,
  falling back to the default prompt agent.
  Without --project-id, the bead is located across the registered project boards; pass --project-id
  when the same bead id exists on more than one board.
"
    )
}

pub fn server_usage() -> String {
    let commands = [
        format_help_command("server", "Run gxserver in the foreground"),
        format_help_command("server start [--json]", "Start gxserver in the background"),
        format_help_command(
            "server stop [--json]",
            "Stop only the gxserver control plane",
        ),
        format_help_command(
            "server stop-all [--json]",
            "Stop gxserver and kill tracked zmx sessions",
        ),
        format_help_command("server status [--json]", "Print gxserver runtime state"),
        format_help_command("server version", "Print the gxserver package version"),
        format_help_command("server --version", "Alias for server version"),
        format_help_command("server help | server --help", "Show this help"),
    ]
    .join("\n");

    format!(
        "Ghostex Server - manage the gxserver background process

Usage:
  gx server
  gx server <command> [args...] [--flags]
  ghostex server <command> [args...] [--flags]

Commands:
{commands}

Lifecycle:
  gxserver is the Ghostex background control plane for projects, sessions,
  zmx lifecycle, auth, local APIs, logs, and remote/headless access.
  Closing the macOS app does not stop gxserver.
  gx server stop stops only the control plane; it does not kill zmx, tmux,
  zellij, shell, or agent sessions.
  gx server stop-all is destructive: it kills gxserver-tracked zmx sessions,
  marks killed sessions stopped, then stops the control plane.

Compatibility:
  The gxserver command remains available for server-only/headless installs.
  These gx server commands forward to the same gxserver implementation.
"
    )
}

pub fn browser_usage() -> String {
    let setup_commands = [
        format_help_command(
            "browser mcp [--port n] [--target id|--page id]",
            "Run the stdio MCP server for CEF DevTools control",
        ),
        format_help_command(
            "browser install-skill [--json]",
            "Install the $ghostex-embedded-browser-use skill with the external skills CLI",
        ),
        format_help_command(
            "browser open [url] [project/reuse flags]",
            "Open or reuse an embedded browser pane",
        ),
        format_help_command(
            "browser open-pane [url] [project/reuse flags]",
            "Alias for browser open",
        ),
    ]
    .join("\n");

    let mcp_tools = [
        format_help_command(
            "ghostex_list_pages",
            "List CEF DevTools targets and current page ids",
        ),
        format_help_command(
            "ghostex_select_page",
            "Choose the target page for later tool calls",
        ),
        format_help_command("ghostex_navigate", "Navigate the selected CEF page"),
        format_help_command(
            "ghostex_console_logs",
            "Read console messages, Log entries, and exceptions captured after attach",
        ),
        format_help_command(
            "ghostex_snapshot",
            "Get an accessibility-like DOM snapshot with @e element refs",
        ),
        format_help_command(
            "ghostex_click / ghostex_fill",
            "Interact with @e refs or CSS selectors",
        ),
        format_help_command(
            "ghostex_press_key",
            "Send Enter, Tab, Escape, arrows, or printable keys",
        ),
        format_help_command(
            "ghostex_evaluate",
            "Run JavaScript in the selected page for inspection",
        ),
        format_help_command(
            "ghostex_screenshot",
            "Capture a PNG screenshot as base64 MCP image content",
        ),
    ]
    .join("\n");

    format!(
        "Ghostex Embedded Browser Use - control embedded CEF panes from agents

Usage:
  gx browser --help
  gx browser mcp [--port n] [--target id|--page id] [--timeout ms]
  gx browser install-skill [--json]
  gx browser open [url] [--project-path path|--project-id id] [--reuse similar|exact|none]
  gx browser open-pane [url] [--project-path path|--project-id id] [--reuse similar|exact|none]
Agent MCP config:
  [mcp_servers.ghostex-browser]
  command = \"ghostex\"
  args = [\"browser\", \"mcp\"]

Commands:
{setup_commands}

Project scoping:
  browser open/open-pane default to the CLI process cwd as --project-path.
  Agents running in a worktree should keep that default, or pass --project-path \"$PWD\".
  Use --project-id when you already know the Ghostex project id from ghostex sessions --json.
  Use --group-id to place the browser in a specific project group.
  Use --active-project only for intentional manual control of the currently focused Ghostex project.

Tab reuse:
  browser open/open-pane default to --reuse similar, so an existing browser pane in the same project with the same origin is reused instead of creating a duplicate tab.
  Use --reuse exact when only the exact same URL should be reused.
  Use --reuse none or --new only when a separate browser pane is required.
  When a pane is reused for a different URL on the same origin, Ghostex focuses that pane and navigates it instead of creating another tab.
  After creating or selecting a page, keep the returned session id and the MCP page id from ghostex_list_pages; pass --target <pageId> to gx browser mcp or call ghostex_select_page before follow-up actions.

MCP tools exposed to the agent:
{mcp_tools}

Recommended agent workflow:
  1. Run ghostex_list_pages to find browser targets.
  2. Run ghostex_select_page when more than one page is open.
  3. Run ghostex_console_logs before reproducing a bug, then again after the action.
  4. Run ghostex_snapshot and use @e refs with ghostex_click or ghostex_fill.
  5. Use ghostex_screenshot for visual proof and ghostex_evaluate for focused inspection.

Connection details:
  The MCP server talks directly to Ghostex's embedded CEF Chrome DevTools Protocol endpoint.
  It scans the default Ghostex CEF ports automatically. Pass --port or set
  GHOSTEX_CEF_REMOTE_DEBUGGING_PORT only when the app is using a non-default port.

Legacy aliases:
  browser-devtools-mcp and browser-mcp still run the MCP server.
  install-browser-skill still installs the skill, but new docs should use browser install-skill.
"
    )
}

pub fn generate_title_usage() -> String {
    "Ghostex Auto Rename Session - install the agent skill for naming Ghostex sessions

Usage:
  gx generate-title --help
  gx generate-title install-skill [--json]

Agent skill:
  Use $ghostex-auto-rename-session when a task needs a concise Ghostex session title.

What the skill does:
  Generate one title shorter than 60 characters.
  Then submit /rename <title> in the current Ghostex session with rename-command.

Self-session command:
  ghostex rename-command --session-id \"${GHOSTEX_GLOBAL_SESSION_REF:-${GHOSTEX_SESSION_ID:-${ZMX_SESSION:-}}}\" --title \"<title>\"
"
    .to_string()
}

pub fn move_codex_session_usage() -> String {
    "Ghostex Move Codex Session - install the agent skill for moving Codex sessions between folders

Usage:
  gx move-codex-session --help
  gx move-codex-session install-skill [--json]

Agent skill:
  Use $ghostex-move-codex-session when a user asks how to move or fork the
  current Codex CLI conversation into another folder.

What the skill teaches:
  Run /status in the current Codex session, copy the session id, then create a
  separate fork in the target folder with codex fork --yolo -C <folder-path> <SESSION_ID>.

Fallback:
  Use codex fork --all --last --yolo -C <folder-path> only when the user does
  not want to copy the session id.
"
    .to_string()
}

pub fn agent_orchestration_usage() -> String {
    "Ghostex Agent Orchestration - install the agent skill for Ghostex CLI coordination

Usage:
  gx agent-orchestration --help
  gx agent-orchestration install-skill [--json]

Agent skill:
  Use $ghostex-agent-orchestration when a task needs Ghostex session or agent
  coordination from the CLI.

What the skill teaches:
  Read ghostex --help first, then use commands such as sessions --json,
  create-session, create-agent, send-message, read-text --lines, focus, sleep,
  wake, kill, wait-for, and assert-card.

Boundary:
  Use Ghostex CLI commands instead of raw zmx/tmux control when coordinating
  panes inside Ghostex.
"
    .to_string()
}

pub fn fable56_orchestration_usage() -> String {
    "Ghostex Fable 5.6 Orchestration - install the agent skill for the Fable plan / Codex implement / Fable verify pipeline

Usage:
  gx fable-5.6-orchestration --help
  gx fable-5.6-orchestration install-skill [--json]

Agent skill:
  Use $ghostex-fable-5.6-orchestration to run a multi-phase coding task as a
  pipeline over Ghostex panes: plan inline with Fable, launch one Codex
  gpt-5.6 worker pane per phase, then verify with a Fable pane and spawn
  fixer panes until verification passes.

What the skill teaches:
  Ask for Fable and Codex effort levels, write a self-contained phase plan
  file, launch workers with create-session, monitor sentinels with read-text,
  verify acceptance criteria with a Fable pane, and cap the fix loop.

Boundary:
  Use Ghostex CLI commands instead of raw zmx/tmux control when coordinating
  panes inside Ghostex.
"
    .to_string()
}

pub fn find_prev_session_usage() -> String {
    "Ghostex Find Previous Session - install the agent skill for Zehn history search

Usage:
  gx find-prev-session --help
  gx find-prev-session install-skill [--json]

Agent skill:
  Use $ghostex-find-prev-session when a user wants to find, inspect, resume, or
  fork a previous agent session with Ghostex's bundled Zehn search.

Supported agents:
  Claude Code, Codex, Pi, OpenCode, Cursor Agent, and Grok.

What the skill teaches:
  Read ghostex find --help, narrow by a distinctive prompt phrase and optional
  --agent filter, then use Zehn's interactive picker to inspect, resume, copy,
  or fork the selected session without guessing session ids.
"
    .to_string()
}

pub fn computer_use_usage() -> String {
    "Ghostex Computer Use - install the agent skill for native macOS app control

Usage:
  gx computer-use --help
  gx computer-use install-skill [--json]

Agent skill:
  Use $ghostex-computer-use when a task needs native macOS app automation.
  The skill is a Ghostex-named wrapper around $cua-driver, so agents get the
  Cua Driver workflow without requiring the user to remember the lower-level name.

Desktop Control requirements:
  Install Desktop Control from Ghostex setup or Settings > Integrations.
  Cua Driver must be installed, and macOS Accessibility plus Screen Recording
  permissions must be granted before desktop automation can work.

Boundary:
  Use $ghostex-computer-use for native macOS apps.
  Use $ghostex-browser-use for supported external browser page content.
  Use $ghostex-embedded-browser-use and gx browser --help for embedded Ghostex browser panes.
"
    .to_string()
}

pub fn browser_use_usage() -> String {
    "Ghostex Browser Use - install the Cua Driver browser-page agent skill

Usage:
  gx browser-use --help
  gx browser-use install-skill [--json]

Agent skill:
  Use $ghostex-browser-use for supported Chrome, Chromium, Edge, and Electron
  page content through Cua Driver's typed browser tools.

Requirements:
  Cua Driver must be installed. Browser preparation and existing-profile access
  require the explicit approvals described by the installed Cua Driver skill.

Boundary:
  Use $ghostex-browser-use for supported external browser page content.
  Use $ghostex-embedded-browser-use and gx browser --help for browser panes built into Ghostex.
  Use $ghostex-computer-use for native apps, browser chrome, and native dialogs.
"
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_help_command_pads_to_58_columns_with_min_gap() {
        let line = format_help_command("state | dump-state", "Print sidebar state as JSON");
        assert_eq!(
            line,
            format!(
                "  state | dump-state{}Print sidebar state as JSON",
                " ".repeat(58 - "state | dump-state".len())
            )
        );
        // Signatures longer than the column keep a minimum two-space gap.
        let long = "wait-for-text <selector> <regex> [--timeout-seconds n] [--interval-seconds n] [--lines n] [--json]";
        let long_line = format_help_command(long, "desc");
        assert_eq!(long_line, format!("  {long}  desc"));
    }

    #[test]
    fn usage_contains_expected_sections_and_tab_indented_usage_lines() {
        let text = usage();
        assert!(text.starts_with("Ghostex CLI - manage running Ghostex terminal sessions\n"));
        assert!(text.contains("\n\t  ghostex\n\t  gx\n\t  ghostex <path...>\n"));
        for section in [
            "\nCommands:\n",
            "\nWorkspace:\n",
            "\nAutomations:\n",
            "\nInput:\n",
            "\nUI:\n",
            "\nServer:\n",
            "\nEvidence:\n",
            "\nSelectors:\n",
            "\nSessions:\n",
            "\nAttach:\n",
            "\nGlobal flags:\n",
        ] {
            assert!(text.contains(section), "missing section {section:?}");
        }
        assert!(text.contains("automations --help"));
        assert!(text.contains("cli --help"));
        assert!(text.contains("tui [--tui-bin path]"));
        assert!(text.contains("no subcommand launches or activates the Ghostex desktop app"));
        assert!(!text.contains("gx 2"));
        assert!(text.contains("automation-mark-run-read --run-id id --path path"));
        assert!(text.ends_with("  -h, --help            Show this help\n"));
    }

    #[test]
    fn focused_automation_help_documents_skill_and_definition_contract() {
        let text = automations_usage();
        assert!(text.contains("gx automations install-skill [--json]"));
        assert!(text.contains("automation-save --path path --definition-json json"));
        assert!(text.contains("Repeating definition JSON:"));
        assert!(text.contains(r#"{"kind":"timer","delayMs":1800000}"#));
        assert!(text.contains(r#"{"kind":"once","runAt":"2026-08-14T09:30:00.000Z"}"#));
        assert!(text.contains("AUTOMATION_RESULT"));
        assert!(text.contains("delayed-send"));
    }

    #[test]
    fn cli_skill_help_routes_to_specialized_workflows() {
        let text = cli_usage();
        assert!(text.contains("gx cli install-skill [--json]"));
        assert!(text.contains("$ghostex-manage-automations"));
        assert!(text.contains("$ghostex-agent-orchestration"));
    }

    #[test]
    fn generate_title_usage_keeps_literal_shell_parameter_expansion() {
        assert!(generate_title_usage().contains(
            "ghostex rename-command --session-id \"${GHOSTEX_GLOBAL_SESSION_REF:-${GHOSTEX_SESSION_ID:-${ZMX_SESSION:-}}}\" --title \"<title>\""
        ));
    }
}
