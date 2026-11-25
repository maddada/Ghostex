use super::catalog::{automation_help_commands, format_help_command};

pub fn cli_usage() -> String {
    "Ghostex CLI - start every Ghostex task from the CLI help

Usage:
  gx cli --help

How to work with Ghostex:
  Read ghostex --help first, follow the focused help it points to, inspect
  state with JSON, target stable ids, perform the requested operation, and
  verify the resulting state.

Guides (each printed by this Ghostex, so it always matches it):
  ghostex agent-guide            What an agent in Ghostex can do, and where to read more
  ghostex agents guide           Work with other agents, read a thread, rename a session
  ghostex coordinator guide      The coordinator playbook
  ghostex visual guide <topic>   Ghostex visuals, forms and Live pages
  ghostex board guide            Project Board beads with bd
  ghostex secret guide           Ask the user for a password, API key or token
  ghostex spaceo guide           Drive a Mac app on a hidden display (macOS)
  ghostex browser --help         Ghostex's own browser panes
  ghostex help guide             Explain Ghostex and change its settings for the user
  Pages in Chrome, Edge or an Electron app, and desktop apps: read the cua-driver skill.
"
    .to_string()
}

pub fn extensions_usage() -> String {
    "Ghostex Extensions - browse, install, and manage gxserver extensions

Usage:
  ghostex extensions <command> [args] [--json]

Commands:
  list [--json]                         List installed extensions and state
  catalog [--json]                      Fetch the configured extension catalog
  install <id> [--json]                 Install an extension from the catalog (the user only)
  install-local <path> [--json]         Install a local extension folder (the user only)
  uninstall <id> [--json]               Remove an installed extension
  state <id> [--set k=v] [--json]       Read or update extension state
  panel get|set|patch|events <id>       Read, write or follow a sidebar extension's panel
                                        (ghostex extensions panel --help)
  message <id> <name> [<json>|-]        Inside an agent: send the extension a message about
                                        this session and print its answer

Only the user installs extensions, from Settings > Extensions, where Ghostex shows what each
one may do before it is installed. The CLI, agents and other computers are refused: tell the
user which extension to install there.

State keys:
  enabled, pinned, placement, terminalPlacement,
  preferences.<name>, preferences, grantedPermissions,
  sidebarModeHidden, sidebarSectionsHidden (a sidebar extension's mode and sections)
Turning an extension on and granting it a permission are the user's too (Settings >
Extensions); turning one off and the other keys stay open.
"
    .to_string()
}

pub fn app_update_usage() -> String {
    "Ghostex app-update - update a computer's Ghostex app, here or from another device

Usage:
  ghostex app-update <command> [--server <profile>]

Commands:
  status                Print the update status JSON: the running version, the available
                        version, the phase and progress of an install, and the auto switch
  check                 Ask the app to check for an update now and print the status
  install [--wait]      Download and install the update with the app's own updater, then
                        restart Ghostex there; sessions and their agents keep running.
                        --wait follows it through the restart and prints how it ended
  auto on|off           Keep this computer up to date: install updates on their own while
                        no agent there is working

Ghostex must be open on that computer (macOS and Windows). Linux packages only report the
new version; install it the way Ghostex was installed. Use --server <profile> for another
computer with a saved connection profile.
"
    .to_string()
}

pub fn tailcat_usage() -> String {
    "Ghostex tailcat - control-plane-free remote access through the tailcat sidecar

Usage:
  ghostex tailcat <command>

Commands:
  status                                Print the tailcat sidecar status JSON
  enable                                Serve the configured ports and print the status
  disable                               Stop the sidecar and print the status
"
    .to_string()
}

pub fn ports_usage() -> String {
    "Ghostex ports - list the TCP ports listening on this machine

Usage:
  ghostex ports [--json] [--web]

Output:
  Without --json, prints a PORT/ADDRESS/PID/COMMAND table.
  With --json, prints { \"ports\": [{ \"port\", \"address\", \"pid\", \"command\" }] },
  one entry per port and bind address, sorted by port and then address.
  Add --web with --json to inspect HTTP/HTTPS page titles, service details,
  and small favicons. Unresponsive ports remain listed without web metadata.

Notes:
  Every listening TCP socket is listed, not only the ones Ghostex started.
  pid and command are null when the socket belongs to another user's process.
  Linux reads the list with ss, which sees every user's sockets (including
  root's and Docker's); macOS reads it with lsof. Missing both is an error.
"
    .to_string()
}

pub fn automations_usage() -> String {
    let commands = automation_help_commands().join("\n");
    format!(
        r#"Ghostex Automations - manage scheduled project agent work through gxserver

Usage:
  gx automations --help
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

Definition with a model and effort:
  {{"name":"Weekly check","agentId":"claude","model":"sonnet","effort":"medium","prompt":"Check the open issues and report actionable findings.","enabled":false,"schedule":{{"kind":"weekly","days":[1],"time":"09:00","timezone":"local"}},"executionMode":{{"kind":"local"}}}}

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
  thread    {{"kind":"thread","agentSessionId":"durable-agent-conversation-id","sessionId":"optional-live-pane-hint"}}

Model and effort:
  model and effort are optional and take the same values as ghostex create-agent --model and
  --effort. Each run's new session starts on them, for that session only; leave them out to keep
  the agent's own. They are checked for the agent when saved. Thread mode runs in a session that
  already exists, which keeps its own model and effort, so thread definitions refuse them.

Definition fields:
  name, agentId, prompt, enabled, schedule, executionMode, model, effort, plus id, projectIds,
  createdAt, updatedAt and nextRunAt as automation-state returns them. Any other field is refused
  with an error naming it, and nothing is saved.

Thread execution:
  Prefer agentSessionId for Codex and other resumable agents. Ghostex queues the prompt in the
  existing pane's chat, sent once the agent is free, when it still owns that conversation; if
  the pane has been closed, Ghostex
  creates a new pane, resumes the exact agent conversation, and then sends the prompt.
  sessionId-only definitions remain supported but require that Ghostex pane to still exist.

Updates:
  Start from the definition returned by automation-state. Preserve its id,
  createdAt, project selection, and unchanged fields, then pass the complete
  edited object to automation-save.

Run results:
  Automation prompts require a final `AUTOMATION_RESULT: <status>` line.
  Replace <status> with exactly findings, no_findings, or needs_attention.
  gxserver records that marker in run history.
  A run ends as failed when its prompt never reaches the agent (10 minutes for a new session,
  30 for a thread's busy session), and as needs_attention when the agent finishes its turn
  without the line, or after an hour. While a run is going, due runs are recorded as skipped.

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
            "run-action <commandId> --project-id id [--scope project|global]",
            "Start a Terminal action or return a Browser action URL; --scope global runs a Global Action",
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
  ghostex run-action <commandId> --project-id <id> [--scope project|global]

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
  ghostex run-action docs --project-id P1n8o --scope global

Behavior:
  save-command writes to the live gxserver project store and refreshes normal project state.
  Reusing a command id replaces that action definition in the same ordered position; a new id is appended.
  Do not edit workspace-state.json or the Ghostex state database directly.
  Terminal and Browser quick actions are project customCommands, not agent buttons.
  Use run-action to execute them; run-command and click-button are renderer-only legacy paths.
  save-command saves project actions only; Global Actions are made in Settings > Actions.
  run-action looks the id up among the project's actions; --scope global looks it up among the
  Global Actions instead and never falls back to the project's list. A Global Terminal action runs
  as a session in the --project-id project, in that project's folder. Any other --scope value is an error.

Inspect:
  ghostex state
  ghostex state | jq '.projects[] | {{projectId, name, path, customCommands, customCommandOrder}}'
"
    )
}

/// Render CLI help for Project Board worker dispatch.
pub fn board_usage() -> String {
    let commands = [
        format_help_command(
            "board guide",
            "How to work a bead with bd: link this session first, swimlanes, review beads",
        ),
        format_help_command(
            "board start-work <bead-id> [--agent id [--allow-hidden]] [--model m] [--effort e] [--project-path path|--project-id id] [--json]",
            "Dispatch a Project Board bead through gxserver",
        ),
        format_help_command(
            "board associate <bead-id> [--session-id id] [--project-id id] [--json]",
            "Show this session on the bead's card; without --session-id it links the calling session",
        ),
    ]
    .join("\n");

    format!(
        "Ghostex Project Board - dispatch bead work through gxserver

Usage:
  ghostex board guide
  ghostex board start-work <bead-id> [--agent <agentId> [--allow-hidden]] [--model <model>] [--effort <level>] [--project-path <path>|--project-id <id>] [--json]
  ghostex board associate <bead-id> [--session-id <alias|id|title>] [--project-id <id>] [--json]
  gx board start-work <bead-id>
  gx board associate <bead-id>

Commands:
{commands}

Behavior:
  start-work IS the dispatch: it creates and starts the visible worker session with the bead's
  canonical work prompt and links the conversation to the card. Do not also launch a worker yourself.
  Repeated calls are safe: an existing usable linked conversation (live, sleeping, or restorable)
  is returned as {{ \"projectId\": ..., \"sessionId\": ..., \"created\": false }} instead of creating a second worker.
  Without --agent, the bead assignee is matched case-insensitively against configured agents,
  falling back to the default prompt agent. An --agent turned off in Settings > Agents is refused
  unless --allow-hidden is passed.
  --model and --effort start a new Claude, Codex, Pi, OMP, Copilot, Grok Build, Hermes Agent, Empryo or Antigravity worker
  on that model and effort for this session only (a Pi, OMP or Empryo model is <provider>/<model>, a Pi or OMP effort a
  thinking level; Cursor CLI takes --model only); your default
  model is unchanged, and a resumed worker keeps them. A reused worker keeps
  whatever it already runs.
  Pass --project-path <repo> (or --project-id) to start the worker in the project the card is
  about; the bead is still looked up on that project's board. Without either, the worker starts
  in the project whose own path is the Beads directory - the board itself - never in a sibling
  project that merely mounts the same board.

  associate is for an agent that was already asked to work a bead - by hand, or in a session
  someone else started - rather than dispatched from the card. It creates no session: it links the
  one it runs in, read from the Ghostex session environment, so the card shows who is working it.
  Run it again after a fork or restore to move the card onto the session that is really working.
  Pass --session-id to link another session by alias, id, provider session name, or title.

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
        format_help_command(
            "server endpoint [--no-token]",
            "Print the running daemon's port and auth token as JSON (for SSH clients); --no-token leaves the token out",
        ),
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
  gx server stop-all kills gxserver-tracked zmx sessions, marks killed
  sessions sleeping so they wake on next open, then stops the control plane.

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
  The MCP server reaches Ghostex's embedded CEF pages through the local gxserver, with its token.
  It scans the CEF DevTools ports only for an older Ghostex whose gxserver lacks that route.
  Pass --port, or set GHOSTEX_CEF_REMOTE_DEBUGGING_PORT, to use a DevTools port the app was started with.

Outside Ghostex's browser:
  For pages in Chrome, Chromium, Edge or an Electron app, or for desktop apps, read the cua-driver skill
  (Trycua's Cua Driver, which Ghostex installs with Computer Use) and follow it.

Legacy aliases:
  browser-devtools-mcp and browser-mcp still run the MCP server.
"
    )
}

pub fn generate_title_usage() -> String {
    "Ghostex session titles

Usage:
  gx generate-title --help

A title is under 60 characters and names the work done. Apply it with:
  ghostex rename-session \"$GHOSTEX_GLOBAL_SESSION_REF\" \"<title>\"
A coordinator's thread uses ghostex coordinator rename-self \"<title>\".
ghostex agents guide (Rename a session) has the rules.
"
    .to_string()
}

pub fn guide_usage() -> String {
    "Ghostex Help - the built-in guide that explains and configures Ghostex

Usage:
  gx guide                       Print the overview chapter
  gx guide <chapter>             Print one chapter: overview, features, settings, hotkeys
  gx guide list                  List the chapters
  gx guide --help

Answering for the user:
  An agent asked how Ghostex works, or asked to change a setting, follows
  ghostex help guide: read ghostex guide (and the settings or hotkeys
  chapter) before answering, find the exact key with ghostex settings list or
  get, confirm the change with the user, apply it with ghostex settings set,
  and verify with settings get. Settings agents may not write are opened for
  the user with settings open.

Related commands:
  ghostex help guide
  ghostex settings --help
"
    .to_string()
}

pub fn settings_usage() -> String {
    "Ghostex Settings - read and change app settings from the command line

Usage:
  gx settings list [--json] [--writable] [--tab <id>]
  gx settings get <key> [--json]
  gx settings set <key> <value> [--json]
  gx settings reset <key> [--json]
  gx settings open [<key>] [--tab <id>] [--json]
  gx settings hotkeys [list] [<search>] [--changed] [--terminal] [--json]
  gx settings hotkeys get <id> [--terminal] [--json]
  gx settings hotkeys set <id> <keys> [--replace] [--terminal] [--json]
  gx settings hotkeys reset <id> [--terminal] [--json]
  gx settings hotkeys reset --all [--terminal] [--json]
  gx settings export <file> [--json]
  gx settings import <file> [--dry-run] [--json]
  gx settings --help

Keys:
  Keys are the camelCase settings keys from the built-in catalog (ghostex guide
  settings). list prints every key with its current value; get explains one.

Values:
  boolean   true or false (also on/off, yes/no)
  number    a number; some keys accept only listed values or a min/max/step range
  enum      exactly one of the listed option values (case-sensitive)
  text      any text, passed verbatim

Hotkeys:
  Every hotkey that can act from a terminal has two bindings: the normal one,
  which applies outside a terminal, and the Terminal focused one, which applies
  while a terminal has the keyboard (Settings > Hotkeys > Terminal focused).
  --terminal reads or changes the Terminal focused one. Until it is set, it
  follows the normal key when you changed that, else its own default. reset
  --terminal makes it follow again. Copy, Select All and Unfocus Terminal have
  only a Terminal focused binding.

Writing:
  set and reset send the change to the running Ghostex desktop app, which saves
  it the same way the Settings modal does, then the CLI confirms the saved
  value. The desktop app must be running. Keys marked read-only for agents
  (structured values, account and remote-pairing state, secrets, Settings UI
  actions) cannot be set; use settings open <key> so the user can change them.

Hotkeys:
  Ids are the camelCase action ids from `ghostex guide hotkeys` (a title such
  as \"Session: Fork\" works too). Keys are written like cmd+shift+o: modifiers
  cmd, ctrl, alt (option) and shift, then one key (a letter, digit, symbol,
  f1-f24, up, down, left, right, tab, enter, escape, space, backspace, delete,
  home, end, pageup, pagedown). Separate a two-step sequence with a space. On
  Windows and Linux cmd means Ctrl. `none` unassigns a hotkey. set refuses keys
  another hotkey already uses unless --replace moves them (the other hotkey is
  left unassigned). Changes save through the running desktop app like the
  Hotkeys page and bind at once.

Export and import:
  export saves this computer's settings to a JSON file to bring them to another
  computer (Settings > Sync > Export settings does the same). Secrets, sign-ins
  and settings that belong to one computer (folders, the glass picture,
  interface scale and zoom, the sync switches) are left out. import applies such
  a file here through the running app like set does (or saves it when the app
  isn't running) and lists what changed; --dry-run lists it without saving.
  Settings this Ghostex doesn't know, values of the wrong type, and keyboard
  shortcuts from another system are skipped and counted.

Tabs (for --tab):
  settings integrations extensions osIntegration remote projects agents
  accounts actions openTargets hotkeys debugging about
"
    .to_string()
}

pub fn move_codex_session_usage() -> String {
    "Moving a Codex session to another folder

Usage:
  gx move-codex-session --help

Codex can't move a running session. Run /status in it, copy the session id,
then fork it into the folder: codex fork --yolo -C <folder-path> <SESSION_ID>
(leave out --yolo for approvals and the sandbox). Without the id:
codex fork --all --last --yolo -C <folder-path>. ghostex agents guide has
the same steps.
"
    .to_string()
}

pub fn visuals_usage() -> String {
    "Ghostex Visuals - check Ghostex visuals, forms and pages, and read their guide

Usage:
  gx visual --help
  gx visual check <file.json|-> [--width N] [--light] [--out file.png]
  gx visual check <page.html> [--width N] [--text-scale P] [--light] [--out file.png]
  gx visual export <file.html|page id|page URL> [--out file.html] [--json]
  gx visual guide <visuals|forms|pages|example|all>
  gx visual boosts [list|show <id>|set <id> <prompt|->|reset <id>|on|off] [--json]

Guide:
  `gx visual guide pages` (or visuals, forms) prints the whole guide to that
  kind: what to write, the layout, the look, how to check and publish it.
  Read it once per conversation before you make one; it always matches this
  Ghostex. `gx visual guide example` prints a complete Live page to copy the
  shape of.

Boosts:
  A Boost is a pill the user adds to a chat message (Live page, Chart it,
  Side by side, Map it, Ask me in a form; Computer use, Browser use, Show
  screenshots, Second opinion; Triage requests, Plan with workers, Try it in
  a worktree, Save to Docs, Steps for me, Tell me when done; or their own).
  Ghostex adds its prompt under that message in a <ghostex-system> block; do
  what it asks in your answer, after reading the guide it names.
  `gx visual boosts` lists them and edits or resets their prompts.

Names:
  Ghostex visuals (gx visuals): charts, stats and tables, a ```visual block.
  A Ghostex form (gx form): questions as a form, a ```questions block, or
  one-click answers. A Live page: an HTML page published with `ghostex
  show`. Older words mean the same: \"inline graphs\", \"inline
  questions\" or \"inline q\", and \"Ghostex page\", \"gx page\",
  \"inline HTML\" or \"chat page\" for a Live page. Use the new names
  when you talk to the user (\"I made you a Live page\").

What goes where:
  Charts, stats and tables go
  in the reply as a ```visual block (Ghostex visuals); questions go in a
  ```questions block, which the chat shows as a Ghostex form; mockups and
  small tools are Live pages published with `ghostex show`. When a
  question is about a PR, issue, file, page, commit, thread or anything else
  the user could open, link it in the question itself
  ([#407](https://github.com/owner/repo/pull/407), or a file path link) so
  they can check it before answering.

Questions:
  A ```questions block is one JSON object whose \"questions\" is a list, even
  of one question. Each question has a \"tag\" (Q1, Q2, … counting up across
  the conversation), a \"title\", and either \"choices\" (each its text, or
  {\"label\", \"detail\", \"value\", \"recommended\"}) or \"type\": \"text\":
    ```questions
    {\"title\": \"Settings redesign: layout\",
     \"questions\": [
       {\"tag\": \"Q1\", \"title\": \"Which layout should I build?\",
        \"choices\": [{\"label\": \"Tabs\", \"detail\": \"One page per area.\",
                     \"recommended\": true}, \"One long page\"]},
       {\"tag\": \"Q2\", \"title\": \"What should it be called?\", \"type\": \"text\"}
     ]}
    ```
  Optional: \"detail\" on a question, \"multiple\": true for several picks,
  \"placeholder\" for a text box, \"submit\" to rename the button, and
  \"withdraw\": [\"Q40\"] (see check below). The chat adds \"Other / none of
  these\", the note boxes and Send itself. Write the block to a file and run
  `ghostex visual check <file>` before sending it; send it only when check
  lists your questions with no error and nothing to write the standard way.

One-click answers:
  One quick pick with 2 to 4 short answers and nothing to explain or note
  goes in the reply itself instead of a form: after the question, one
  Markdown link per answer to ghostex-answer: and the question's tag, such as
    PR #414: Merge it? [Yes](ghostex-answer:Q901) [No](ghostex-answer:Q901)
  The question shows alone in the dock and on the phone, so it names its
  subject (\"PR #414: Merge it?\", \"Delete the branch fix/tab-drag?\"), never
  \"it\" or \"this\" alone; the lines just above it show as its description.
  Coordinators never use these: they ask with item ask, a form or a page.
  A title of \"recommended\" marks your pick ([Keep](ghostex-answer:Q902
  \"recommended\")) and =value sends a value other than the label
  (ghostex-answer:Q902=keep). The chat draws the answers as a pill under the
  question; one tap sends them back in a <ghostex-page-answers> message keyed
  by the tag, the \"?\" pill counts the tag until then, and \"withdraw\" in a
  later ```questions block drops it. Anything bigger is a Ghostex form.

Pages (Live pages):
  Ghostex paints every page's backdrop (a soft dusk in dark, dawn in light)
  and its glass cards, so a page never paints html or body. The chat's card
  previews the page's top: give it a strong header (.gx-eyebrow, a specific
  h1, a one-line .gx-lede, .gx-chip facts), put options side by side in a
  .gx-options row of tied sections (each gets a Pick button), one question
  under them, and keep it to about one screen. `gx visual guide example`
  shows the whole shape.
  A page that lists items and asks about them puts the picks on the rows:
  each item a <gx-pick value=\"FB-34\"> with Markdown inside (or data-pick on
  any element), under data-question=\"Q3\", and a <gx-question tag=\"Q3\">
  with no <gx-choice> lines: the kit makes the choices from the rows, gives
  each row a checkbox (a radio for pick one) and a Note, folds long rows,
  and the question shows only what was picked. check refuses a question
  that lists the items above it again.

Exported pages:
  Any Live page can be exported: one HTML file that opens in any browser
  without Ghostex, to send to someone who answers it there. Its Send button
  reads Export answers and gives the answers as JSON (copy or download). The
  user drops or pastes that JSON into the chat, and it reaches you as the
  page's usual <ghostex-page-answers> message, keyed the same way. The page
  card and page window have an Export button; `gx visual export` writes the
  same file (default name: the page's title, in this folder). An exported
  page makes no requests; only what the page itself loads from https (a CDN
  library, a web font) needs the internet, so a self-contained page also
  works offline.

check:
  Draws the block's JSON (or the whole fenced block) exactly as the chat lays
  it out and prints the path of a PNG to look at, or the error that names what
  to fix. --width defaults to 640; use 360 for a phone. A ```questions block
  is read instead: check lists its questions and answer keys, or names the
  problem. A ```questions block may also withdraw earlier questions by tag
  (\"withdraw\": [\"Q40\"]), alone or beside new questions: do that when the
  user already answered one another way (in the chat, in another form, or by
  doing what it asked). A question the reader leaves blank arrives as
  \"skipped\", so move on and don't ask it again unless it blocks the work.

  Given a page (.html), check prepares it as `ghostex show` would and opens it
  in headless Chrome (or Chromium, Edge, Brave; GHOSTEX_CHROME picks one) at
  360px (a phone), 600px, 900px (a foldable or tablet) and 1280px (a
  computer), then at 360px and 900px with text at 130%, the way a phone's
  large font setting draws it (--width N for one width, --text-scale 130 for
  large text). It saves a full-page screenshot of each (in parts, -part2.png
  and on, for a long page) and lists, by CSS
  selector, every box whose content overflows it or is cut off, content a
  sideways scroller can't reach, text drawn over other text, the answered
  pill over text or a control with the page scrolled to its end, and what
  Ghostex had to fit for the phone (columns stacked, pills kept on one line,
  blocks scrolling sideways), with how to fix the page. It exits with an error while anything overflows or overlaps.
"
    .to_string()
}

pub fn manage_beads_usage() -> String {
    "Ghostex Project Board beads

Usage:
  gx manage-beads --help

Read ghostex board guide before working a bead: link this session to it
first, the project swimlane workflow (backlog, in_progress, test, review,
close), progress comments humans can follow, external refs, and safe examples
for review beads. ghostex board --help has the dispatch verbs.

Boundary:
  Use the machine-installed `bd` CLI directly; do not depend on a bundled
  Ghostex copy of Beads.
"
    .to_string()
}

pub fn agents_orchestration_usage() -> String {
    "Ghostex Agents - launching and coordinating other agents

Usage:
  gx agents-orchestration --help

Read ghostex agents --help for the verbs and ghostex agents guide for the
habits: resolve your own session and project, hand over self-contained
tasks, keep the global reference of every created session, confirm
delivery, read replies, verify independently, and cap fix rounds. A bigger
job across several agents: ghostex coordinator --help.

Boundary:
  Use Ghostex CLI commands instead of raw zmx/tmux control when coordinating
  sessions inside Ghostex.
"
    .to_string()
}

pub fn computer_use_usage() -> String {
    "Ghostex Computer Use - native desktop app control through Cua Driver

Usage:
  gx computer-use --help
  gx computer-use install-skill [--json]   Install Trycua's cua-driver skill for every agent

To drive a desktop app, read the cua-driver skill (Trycua's Cua Driver,
installed with it) and follow it.

Desktop Control requirements:
  Install Desktop Control from Ghostex setup or Settings > Integrations.
  Cua Driver must be installed, and macOS Accessibility plus Screen Recording
  permissions must be granted before desktop automation can work.

Boundary:
  Apps on the user's own screen, and pages in Chrome, Edge or an Electron app:
  the cua-driver skill. Mac apps off-screen: ghostex spaceo guide. Ghostex's
  own browser panes: gx browser --help.
"
    .to_string()
}

pub fn spaceo_usage() -> String {
    "Ghostex SpaceO - drive Mac apps on SpaceO's virtual displays

Usage:
  gx spaceo --help
  gx spaceo guide                 How to drive a Mac app on a hidden display
  gx spaceo status [--json]
  gx spaceo repair

Health:
  status says whether SpaceO can give agents screens, read-only. When SpaceO is
  stuck (its display safety stop is on, so every agent gets 'an earlier
  display mutation failed or never completed'), repair stops SpaceO, runs its
  operator recovery and starts it again. repair is for the person at the
  keyboard only: it asks you to type 'repair' and refuses to run for an agent.
  Agents that find SpaceO stuck tell the user to click Repair in Ghostex.

Guide:
  Read ghostex spaceo guide when a task needs a native Mac app or a Chromium
  page driven out of the user's way. SpaceO opens the app on a headless
  virtual display, so the agent clicks, types and takes screenshots there
  while the user keeps their own screen, pointer and focus.

Requirements:
  macOS only: an Apple Silicon Mac with macOS 14 or later. Install SpaceO
  from Settings > Integrations, then grant Accessibility and Screen Recording
  from its rows there. `spaceo doctor` checks the Mac without changing it.

Boundary:
  Mac apps that should run off-screen: ghostex spaceo guide.
  Apps on the user's own screen: the cua-driver skill.
  Ghostex's own browser panes: gx browser --help.
"
    .to_string()
}

pub fn browser_use_usage() -> String {
    "Ghostex Browser Use - pages in other browsers through Cua Driver

Usage:
  gx browser-use --help
  gx browser-use install-skill [--json]    Install Trycua's cua-driver skill for every agent

For Chrome, Chromium, Edge and Electron page content, read the cua-driver
skill (Trycua's Cua Driver) and use its typed browser tools.

Requirements:
  Cua Driver must be installed. Browser preparation and existing-profile access
  require the explicit approvals the cua-driver skill describes.

Boundary:
  Pages in other browsers, native apps, browser chrome and native dialogs:
  the cua-driver skill. Ghostex's own browser panes: gx browser --help.
"
    .to_string()
}

pub fn resources_usage() -> String {
    "Ghostex resources - print the desktop app's Resources panel

Usage:
  ghostex resources [--json] [--timeout-ms n]

Output:
  Without --json, prints the same header totals, sections, and rows the
  Resources panel in the desktop titlebar shows, followed by the per-process
  sample every row was summed from (one line per pid).
  With --json, prints the raw snapshot: header, sections[].rows[], and
  processes[], with memoryMb as floating-point megabytes.

Notes:
  The numbers come from the running desktop app, sampled when you run the
  command, so the app must be open and connected to gxserver.
  memoryMetric names the ledger memoryMb is read from (phys_footprint on macOS,
  which is what Activity Monitor's Memory column shows)."
        .to_string()
}

pub fn feedback_usage() -> String {
    r#"Usage:
  ghostex feedback send --title <text> --body-file <file> --github-user <name> [--image <file>]... [--dry-run] [--json]
  ghostex feedback send --title <text> --body <text> --github-user <name>
  ghostex feedback brief --github-user <name> [--image <file>]... [--json]

Files a Ghostex bug or feedback issue on the Ghostex GitHub through the Ghostex feedback relay,
the same way Send Bug / Feedback does. The issue starts with "Reported by @<name>" so GitHub
notifies that user, and Ghostex adds a line with its version and the operating system. Attach
up to five PNG, JPEG or WebP screenshots of 5 MB each with --image. The issue is public.
--dry-run prints the exact request (images shown by size) without posting it.

brief is for an agent filing a report from its own chat (Ask Ghostex AI): it makes a report
folder in the Ghostex config folder, copies the screenshots into it, and prints the same steps
Send Bug / Feedback's "Collect more data with an agent" gives its agent: check the version, read
logs and settings without changing anything, keep private things out, show the user the whole
draft and post with feedback send only after they say yes. It posts nothing."#
        .to_string()
}

pub fn clear_command_usage() -> String {
    r#"Usage:
  ghostex clear-command <session> [--timeout <seconds>] [--no-wait] [--json]

Starts a session's conversation over with its agent's own /clear (Claude Code and Codex, Native
or Terminal-based), in the same session: its name, place in the sidebar and ref stay, and its old conversation stays on disk.
A sleeping session is woken first and cleared once its agent is up (up to 120 seconds). Then it
waits until the session is on its new conversation and prints its id (up to 30 seconds, 10 for
Codex or a session it woke; --timeout sets both, --no-wait skips the second).

Exit codes: 0 cleared (or sent with --no-wait); 2 sent, not confirmed yet: Codex, and a session
that had to wake, show their new conversation only once they get their next message, so the clear
went in but its new id isn't known yet (JSON "status": "sentNotConfirmed"); 1 not cleared: a
refusal, or a running Claude Code session whose conversation did not change in time.

Refused, with nothing typed, while the session is working, waits on you (a permission prompt, a
question, a Needs you to-do), has queued messages or unsent text in its input box, or runs an
agent other than Claude Code or Codex.

Who can clear: you (from Ghostex, or from your own terminal outside Ghostex) and any agent on this
computer, any session but the agent's own. Each clear is in the security log with who did it.

<session> is an alias, id, ref or title, as for rename-command."#
        .to_string()
}

pub fn needs_you_usage() -> String {
    r#"Usage:
  ghostex needs-you add "<what to do>" [--detail <text>] [--command <command>] [--answer <label>]... [--session <alias|id|title>]
  ghostex needs-you list [--all] [--session <alias|id|title>]
  ghostex needs-you done Q<n> [--session <alias|id|title>]
  ghostex needs-you drop Q<n> [--session <alias|id|title>]

add: when a step can only be done by the user (an owner-only command, a phone tap, a browser
sign-in, a check by eye), put it above this session's chat box as a to-do with a Done button,
without being asked. Never ask the user to run or paste a `ghostex` command you can run yourself:
run setup, sign-in, connect and `ghostex settings open` commands yourself first and ask only for
the part that needs their hands (Touch ID or fingerprint, clicking Approve, signing in in the
browser window that opened). It takes the session's next Q tag and stays until the user presses
Done or you clear it, even if the chat is cleared. The user reads it with nothing else on screen,
often in another chat: its title names what it is about ("Run the signing setup on this computer"),
never "it", "this" or "that" without the thing. --command shows as a chip the user clicks to copy.
--detail says what it is about and why, always, and holds everything the user needs to do it: the
text to paste or post (in a ``` block, which gets a Copy button), the link, the code, the steps.
The row above the chat shows the title only; the detail is in Details. A to-do whose title starts
with paste, post, copy, send, reply, run, type, enter, sign in or open is refused until --detail
holds a ``` block, a link or `code`, or --command is set. --answer (repeated) offers choices such
as "Works" and "Something's off" instead of a single Done. Also name it (its tag and title, never
its content) under Needs you in your reply. The user's answer arrives as a message: "Q61: Done" (or the answer they picked, or their
own words). Inside a coordinator's thread the to-do goes to the coordinator's Needs you list,
and the user's answer still reaches you.
list: the open to-dos (--all adds the ones settled lately and how).
done: you saw it happen yourself (the port answers now), so it leaves the user's list.
drop: it is no longer needed.
--session: the session the to-do belongs to. Without it the calling session is used, read from
the variables every Ghostex pane exports, or, when the agent hides them (a Codex whose
shell_environment_policy is inherit = "core"), from the Ghostex pane the agent runs in."#
        .to_string()
}

pub fn message_usage() -> String {
    r#"Usage:
  ghostex message show <M12 | G9jt7.M12> [--json] [--session <alias|id|title>]
  ghostex message list [--last N] [--grep "words|other words"] [--json] [--session <alias|id|title>]

Every message the user writes in a chat gets an ID that counts up in that chat: M1, M2, M3.
Written with the session in front (G9jt7.M12), it names a message of another chat. When the
user points at one in a message they send (a pill, #M12 or G9jt7.M12; a bare M12 is plain text),
the start of each of the first 5 is quoted in a block at the bottom of the prompt and any more
are named there; this prints a message whole.
show: one message, whole: this chat's M12, or another chat's G9jt7.M12.
list: this chat's numbered messages with their first line, newest first (20 unless --last);
--grep keeps those that contain any of the words.
--session: the chat to read. Without it the calling session is used."#
        .to_string()
}

pub fn secret_usage() -> String {
    r#"Usage:
  ghostex secret guide              How to ask for a secret and use it safely (sudo, 1Password)
  ghostex secret request --label <text> --reason <text> [--placeholder <text>] [--timeout-minutes N] [--session <alias|id|title>]
  ghostex secret providers [list]
  ghostex secret providers add --name <text> --account <1Password account>
  ghostex secret providers remove <id>
  ghostex secret providers check --provider <id> --reference op://vault/item/field

request: shows a private card in the calling session's chat (or --session's) asking the user for a secret
(a password, API key or token) and waits for the answer (10 minutes unless --timeout-minutes,
at most 60). The value never enters the chat, the transcript or your context. Prints JSON:
  {"status":"saved","file":"<owner-only file>","next":"…"}   read it inside the command
  {"status":"declined"|"timedOut"|"cancelled","next":"…"}    exit code 1
The file is deleted after 24 hours. Use it only inside the command that needs it, e.g.
  CLOUDFLARE_API_TOKEN="$(cat '<file>')" wrangler whoami
For a sudo password: once `sudo -n <command>` fails for lack of a password, run
  sudo -k -S -p '' <command> < '<file>'
(the file replaces the command's stdin, so give the command its input as a file argument).
Never print, echo, copy or commit the value. The user may answer with a 1Password reference
(op://vault/item/field) when a 1Password provider is set up; Ghostex reads it for them.

providers: the 1Password accounts Ghostex reads saved keys and tokens from (Settings >
Secrets). They need the 1Password CLI (op) signed in on this computer. Only the user adds,
changes or removes one, in Settings > Secrets; add and remove are refused here. check reads one
reference (1Password may ask to unlock) and says only whether it worked; only the user can
run it, from Settings > Secrets, so the CLI is refused. Only the user answers a secret
request, from Ghostex on their computer or phone."#
        .to_string()
}

pub fn notify_usage() -> String {
    r#"Usage:
  ghostex notify --title <text> [--body <text>] [--session-id <alias|id|title>] [--json]
  gx notify --title <text>

Posts a notification to the Ghostex titlebar bell for a session. Without --session-id the
calling session is used, read from the Ghostex session environment every pane exports, or from
the Ghostex pane the agent runs in when the agent hides that environment.
The row shows the title, the session's project, and the body, and counts as unread until
the session is opened or the row is marked read."#
        .to_string()
}
