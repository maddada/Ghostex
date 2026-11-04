/*
CDXC:Cli 2026-07-14:
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

/// `ghostex <command> --help`: the catalog rows for that one command, or the whole catalog when
/// none name it.
///
/// CDXC:Cli 2026-09-24 WHY:
/// A test agent reading another thread ran `read-session-chat --help` and got the 100-line catalog,
/// then had to grep it for the two rows that mattered.
pub fn command_usage(command_name: &str) -> String {
    let catalog = usage();
    let rows: Vec<&str> = catalog
        .lines()
        .filter(|line| {
            let signature = line.trim_start();
            line.starts_with("  ")
                && (signature == command_name
                    || signature.starts_with(&format!("{command_name} "))
                    || signature.starts_with(&format!("{command_name} |")))
        })
        .collect();
    if rows.is_empty() {
        return catalog;
    }
    let mut text = format!("Usage:\n{}\n", rows.join("\n"));
    if command_name == "read-session-chat" {
        text.push_str(READ_SESSION_CHAT_HELP);
    }
    text.push_str("\nEvery command: ghostex --help");
    text
}

const READ_SESSION_CHAT_HELP: &str = "
Examples:
  ghostex read-session-chat <session> --grep \"mobile|react native\" --context 1 --format text
  ghostex read-session-chat <session> --all --role user --format text
  ghostex read-session-chat <session> --since \"2026-09-21 23:00\" --until \"2026-09-21 23:30\" --format text
  ghostex read-session-chat <session> --grep \"rust only\" --role user --before 0 --after 1 --format text

<session> is a title, session id, global ref (S…:P…:G…), zmx name, agent session id, or the
Session ID or Routing ID from the sidebar's Copy Details. Sleeping sessions read without waking.

Whole-thread reader (any of these flags switches to it):
  --all                 Walk the thread back to its first turn. Older turns come as prompt plus
                        final reply, their tool work collapsed into one note.
  --grep <a|b>          Case-insensitive plain-text search over the whole thread (not a regex;
                        ^ $ . * match themselves); | separates alternatives. Matched rows are
                        marked \"match\" in text output.
  --role <list>         user, assistant, tool, system, reasoning or harness (injected task
                        notifications and reminders). With --grep it limits which rows match.
  --context <n>         Keep n rows before and after each match.
  --before <n>          Keep n rows before each match (overrides --context on that side).
  --after <n>           Keep n rows after each match; --before 0 --after 1 gives a prompt and its reply.
  --since <date>        Keep rows at or after a local date or time (2026-09-21, 2026-09-21T18:00).
  --until <date>        Keep rows before a local date or time; with --since it reads one window.
  --last <n>            Keep only the newest n of the rows left after the other filters (with
                        --grep, the newest n matches).
  --format text|json    text prints a readable conversation in local time; json is the default.
  --history-mode detail Every row, including each tool call, instead of collapsed turns.
";

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
            "account-login claude [--account NUMBER]",
            "Connect a new Claude account, or reconnect a selected cswap account",
        ),
        format_help_command(
            "accounts [--all] [--refresh] [--json]",
            "List every Claude and Codex account with its state, usage and reset times, the default, and the sessions on it",
        ),
        format_help_command(
            "sessions | s | ls [--ungrouped|-u] [--json [--full]] [--mobile-summary]",
            "List running terminal sessions",
        ),
        format_help_command(
            "find | f [zehn args...]",
            "Search agent prompt history (built-in Zehn picker)",
        ),
        format_help_command(
            "history | h [ghostex-history args...]",
            "View local agent transcripts in the alt-screen history TUI",
        ),
        format_help_command(
            "android-check [--json]",
            "Verify this computer is ready for Ghostex Android",
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
        format_help_command(
            "hold-sessions-awake --sessions-json <json> [--ttl-ms <n>] [--holder-id <id>] [--release] [--json]",
            "Keep attached sessions out of a client's Auto Sleep sweep while a remote client is viewing them",
        ),
        format_help_command(
            "client-hello --client mobile --os <android|ios> [--os-version <v>] [--app-version <v>] [--json]",
            "Report a mobile client attach (OS and app version only) to the machine's usage analytics",
        ),
        format_help_command(
            "client-view --client mobile --view <mobile_*> [--json]",
            "Report which phone screen was opened (the screen's fixed name only) to the machine's usage analytics",
        ),
        format_help_command(
            "saved-prompts <action> --payload-json json --json",
            "Read and update the daemon-owned Saved Prompts library",
        ),
        format_help_command(
            "previous-sessions [--query text] [--limit n] [--cursor c] [--project-id id] [--json]",
            "List closed sessions that can be brought back, newest closed first",
        ),
        format_help_command(
            "restore-previous-session --project-id id --session-id id [--json]",
            "Bring back a closed session as a new session (its agent resumes when it is opened) and print it",
        ),
        format_help_command(
            "paired-device-seen --device-id <id> --json",
            "Mark a paired device as connected now (run by the mobile app over SSH)",
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
            "extensions --help",
            "Browse, install, and manage gxserver extensions",
        ),
        format_help_command(
            "tailcat status | enable | disable",
            "Inspect or toggle the tailcat remote-access sidecar",
        ),
        format_help_command(
            "app-update status | check | install [--wait] | auto on|off [--server profile]",
            "Update this or a connected computer's Ghostex app",
        ),
        format_help_command(
            "computers [--json]",
            "List the computers this one is connected to; any agent can then start, message and read sessions there with --computer <name> (sessions, agents create/send/list, coordinator start-thread, read-session-chat, read-text and the other session verbs), or with a session ref from that computer",
        ),
        format_help_command(
            "create-session [title] [--input text] [--start] [--project-id id] [--group-id id] [--cwd path]",
            "Create a terminal session; --start materializes the live terminal immediately; --group-id (with --start) puts it in one of the project's session groups (ghostex session-groups lists them); --cwd starts it in another folder while it stays in the project",
        ),
        format_help_command(
            "create-agent <agentId> --project-id id [--group-id id] [--model m] [--effort e] [--first-input-draft text] [--run-on location] [--defer-start] [--replace-empty-sessions] [--allow-hidden]",
            "Create and start a configured agent session; --group-id puts it in one of the project's session groups; an agent turned off in Settings > Agents is refused unless --allow-hidden is passed (ghostex agents types lists the visible ones); --model/--effort (Claude, Codex, Antigravity CLI, Copilot, Grok Build, Hermes Agent; Pi and OMP take provider/model and a thinking level; Cursor CLI and Gemini take --model only) apply to this session only; Empryo takes provider/model and starts on it without changing Empryo's default; --first-input-draft stages text in its input without sending; --run-on local|docker|hetzner|vercel|daytona|e2b|digitalocean|docker:<host> runs Claude, Codex, OpenCode or Pi in an agentbox box",
        ),
        format_help_command(
            "board start-work <bead-id> [--agent id [--allow-hidden]] [--model m] [--effort e] [--project-path path|--project-id id] [--json]",
            "Dispatch a Project Board bead: reuse its usable linked session or create the worker",
        ),
        format_help_command(
            "board associate <bead-id> [--session-id id]",
            "Link a running session to a Project Board bead; defaults to the calling session",
        ),
        format_help_command(
            "notify --title text [--body text] [--session-id id]",
            "Post a notification to the Ghostex bell for a session; defaults to the calling session",
        ),
        format_help_command(
            "secret request --label text --reason text [--placeholder text] [--timeout-minutes N]",
            "Ask the user for a password, API key or token through a private card in this session's chat, and wait for the answer. The value never enters the chat or your context: when saved, the result names an owner-only file (deleted after 24 hours) to read inside the command that needs it. Use it instead of asking for a secret in chat, and for a sudo password",
        ),
        format_help_command(
            "secret providers [list|add --name text --account text|remove <id>|check --provider id --reference op://…]",
            "List, add, remove or test the 1Password accounts Ghostex reads saved keys from (Settings > Secrets); check says only whether a reference can be read, never its value",
        ),
        format_help_command(
            "feedback send --title text --body-file file --github-user name [--image file]... [--dry-run]",
            "File a Ghostex bug or feedback issue on GitHub through the Ghostex feedback relay; --dry-run prints the request instead",
        ),
        format_help_command(
            "show <file.html> [--title text] [--json] [--no-check]",
            "Publish a self-contained HTML page and print the ```visual block that shows it as a card in the chat; the card opens it where the reader's Open chat pages in setting says (a floating window over the chat by default). Questions written as <gx-questions> (or the page's own ghostex.submit(answers)) send the reader's answers back to you as a message; the block then lists the questions' tags, so the chat counts the page as unanswered until they arrive. When Chrome is installed it then opens the page at a phone's width and says what did not fit (--no-check skips that). Ghostex paints the page's backdrop and glass cards itself; for a page that looks good at a glance (a strong header, options side by side in .gx-options with Pick buttons, one question under them, about one screen) follow $ghostex-visuals and its examples/plan-review.html",
        ),
        format_help_command(
            "visual check <file.json|-|page.html> [--width N] [--light] [--out file.png]",
            "Draw a ```visual block's JSON to a PNG and print its path, or read a ```questions block and list its questions, so an agent can check it before replying. Given a page (.html), opens it in headless Chrome at a phone's width (390px) and a computer's (1280px), saves a screenshot of each and lists what does not fit",
        ),
        format_help_command(
            "run-agent <agentId> [--allow-hidden]",
            "Run a configured agent button; a hidden agent needs --allow-hidden",
        ),
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
            "add-project <path> [--name name] [--create-if-missing] [--workspace name|id]",
            "Add a project to Ghostex (in Personal unless --workspace names another workspace); --create-if-missing creates the folder first",
        ),
        format_help_command(
            "remove-project (--project-id id|--path path|--name name)",
            "Remove a project from Ghostex and delete its session records; the folder on disk is kept. The endpoint only deletes records and does not close running terminals, so close-session them first. A name shared by two projects fails with the matches listed",
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
            "clone-repository <remoteUrl> <destinationPath> [--workspace name|id] --json",
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
            "read-sidebar-spaces --json",
            "Print the durable sidebar spaces state",
        ),
        format_help_command(
            "update-sidebar-spaces --state-json json --json",
            "Replace the spaces state; prints the normalized result",
        ),
        format_help_command(
            "read-custom-session-tags --json",
            "Print the daemon's custom session tag catalog",
        ),
        format_help_command(
            "update-custom-session-tags --state-json json --json",
            "Replace the custom session tag catalog; prints the normalized result",
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
        format_help_command(
            "run-in-terminal --project-id id [--session-id id|--cwd path] [--paste] [--json] (--command text | -- command...)",
            "Run a command in the project's Commands panel terminal (a new terminal in the session's folder without one); --paste types it without pressing Enter",
        ),
    ]
    .join("\n");

    let settings_and_help_commands = [
        format_help_command(
            "settings list [--json] [--writable] [--tab id]",
            "List every app setting with its current value, type, and default",
        ),
        format_help_command(
            "settings get <key> [--json]",
            "Show one setting's value, meaning, options, and where it lives in Settings",
        ),
        format_help_command(
            "settings set <key> <value> [--json]",
            "Change an agent-writable setting through the running desktop app",
        ),
        format_help_command(
            "settings reset <key> [--json]",
            "Restore a setting to its default through the running desktop app",
        ),
        format_help_command(
            "settings open [<key>] [--tab id] [--json]",
            "Open Settings on a tab, with the setting's row searched",
        ),
        format_help_command(
            "settings hotkeys [list|get|set|reset] ...",
            "List hotkeys, or change, unassign or reset one (or --all) through the running desktop app",
        ),
        format_help_command(
            "guide [overview|features|settings|hotkeys]",
            "Print the built-in Ghostex guide, or one chapter of it",
        ),
        format_help_command("guide --help", "Show Ghostex Help skill setup"),
        format_help_command(
            "agent-guide",
            "Print what an agent running in Ghostex can do: pages, questions and charts in the chat, the browser, other agents, secrets, Ghostex skills",
        ),
        format_help_command(
            "help ask \"<question>\" [--new] [--timeout-seconds N] [--json]",
            "Ask a Ghostex Help session (the default prompt agent with the ghostex-help skill) and print its answer once it finishes",
        ),
    ]
    .join("\n");

    let automation_commands = {
        let mut lines = vec![format_help_command(
            "automations --help",
            "Show the complete scheduled automation workflow",
        )];
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
        format_help_command("send-key <selector> <key>", "Send enter, ctrl-c, escape, tab, or arrow keys"),
        format_help_command("send-message <selector> <text>", "Type text and Enter into an existing session"),
        format_help_command("read-text <selector> [--lines n] [--visible] [--json]", "Read terminal text by id or quoted title"),
        format_help_command("search-agent-prompts [--query text] [--agents a,b] [--project path] [--group-by-day] [--limit n] [--offset n] --json", "Search every prompt this machine has sent to an agent (the GUI behind gx f)"),
        format_help_command("read-agent-prompt-text --key <key> --json", "Read one prompt's full text by the key a search row reported"),
        format_help_command("toggle-agent-prompt-favorite --key <key> [--favorite true|false] --json", "Star or unstar a prompt; shares gx f's favorites file"),
        format_help_command("resolve-agent-prompt-launch --key <key> [--action resume|fork] [--fork-agent id] --json", "Resolve whether opening a prompt focuses a live session or runs a command"),
        format_help_command("read-session-chat <selector> [--subagent name-or-id] [--limit n] [--before-offset n] [--history-mode turns|detail] [--preserve-newest] [--wait-ms n --fingerprint f] --json", "Read a session or subagent transcript; --wait-ms long-polls the main chat; without --before-offset, --history-mode turns starts at the newest turn"),
        format_help_command("read-session-chat <selector> --all|--grep <text> [--role user,assistant] [--context n] [--before n] [--after n] [--since date] [--until date] [--last n] [--history-mode detail] [--format text|json]", "Read a whole thread in one call: --all walks it to the first turn, --grep searches it (a|b for either word), --context keeps rows around each match, --since/--until/--last trim it, --format text prints a readable conversation; read-session-chat --help has examples. <selector> can be a title, session id, global ref, zmx name, agent session id, or Copy Details' Session ID or Routing ID"),
        format_help_command("switch-draft-agent <selector> --agent-id <id> --json", "Switch an unprompted draft session to another project agent"),
        format_help_command("send-session-chat-key <selector> --key <key> --json", "Queue Enter or a shifted option key behind this session's pending chat writes"),
        format_help_command("select-session-chat-model <selector> [--model <model> --effort <effort>] [--scope session|default] [--mode <mode>] [--fast-mode on|off] [--permissions ask-for-approval|approve-for-me|full-access|read-only] [--defer] --json", "Change chat model, effort or mode; --permissions changes a Codex session's permission mode; --scope session applies it to this session without changing the agent's default (Claude, Codex 0.157 or newer, OpenCode); --defer queues the choice until the agent is ready"),
        format_help_command("read-session-chat-skills <selector> --json", "List skills available to the session's agent"),
        format_help_command("read-session-chat-files <selector> --json", "List the session project's files for @ mentions"),
        format_help_command("read-session-files-links <selector> --json", "List every attachment, file and link the session mentioned, and for a coordinator its threads too, grouped by session"),
        format_help_command("read-session-details <selector> [--refresh] --json", "Read what the Session details panel shows for a Claude session: where it runs, its context use and Claude's own breakdown of it, connectors, skills, memory files, changes and finished background commands; --refresh asks Claude for a fresh breakdown (needs the Session details panel extension)"),
        format_help_command("send-session-chat-message <selector> <text>", "Send a chat message into an agent session"),
        format_help_command("answer-session-chat-prompt <selector> --answer-json '<json>'", "Answer a pending question/approval prompt or an on-screen picker row"),
        format_help_command("rewind-session-chat <selector> --message-id <uuid> --json", "Drive Claude Code or Codex rewind to the point before a user prompt"),
        format_help_command("interrupt-session-chat <selector>", "Interrupt the session's running agent turn"),
        format_help_command("handoff-session-chat-draft <selector>", "Move the agent CLI's composer draft out of the terminal and print it"),
        format_help_command("read-session-chat-queue <selector> --json", "Read the session's queued chat prompts and synced composer draft"),
        format_help_command("session-chat-rpc <method> --params-base64 <base64 json>", "Perform one session chat request for a remote chat client and print its outcome"),
        format_help_command("queue-session-chat-prompt <selector> <text>", "Queue a prompt for delivery the next time the agent stops"),
        format_help_command("update-session-chat-queued-prompt <selector> --prompt-id <id> [--text '<text>'] [--retry]", "Edit a queued prompt, or move a failed one back to queued"),
        format_help_command("remove-session-chat-queued-prompt <selector> --prompt-id <id> --json", "Delete a queued prompt and print the row that was removed"),
        format_help_command("reorder-session-chat-queue <selector> --prompt-ids <id,id,...>", "Reorder the queue; unlisted rows keep their order after the listed ones"),
        format_help_command("send-session-chat-queued-prompt <selector> --prompt-id <id>", "Deliver a queued prompt right now, regardless of agent state"),
        format_help_command("set-session-chat-draft <selector> --content '<text>' --client-id <id>", "Sync the unsent composer draft; empty content clears it"),
        format_help_command("export-transcript <selector> [--project-id id] [--include-commands true|false] [--include-patches true|false] [--include-reasoning true|false] [--last-turns n] [--json]", "Export the session's agent transcript to a markdown file and print its path"),
        format_help_command("count-transcript-turns <selector> [--project-id id] [--include-commands true|false] [--include-patches true|false] [--include-reasoning true|false] [--json]", "Count the transcript's turns and the export's size for each last N turns, without writing a file"),
        format_help_command("wait-for-text <selector> <regex> [--timeout-seconds n] [--interval-seconds n] [--lines n] [--json]", "Poll a session until a scrollback line matches the regex; exits 1 on timeout or dead session"),
        format_help_command("rename-session <sessionId> <title> [--json]", "Rename a session"),
        format_help_command("rename-session --session-id <id> --title <title> [--json]", "Flag form used by Android SSH actions"),
        format_help_command("rename-command <selector> <title>", "Rename a session through its agent's own rename command, waking a sleeping session to deliver it"),
        format_help_command("session-note read --session-id <id> [--project-id id] [--json]", "Read the note attached to the session's agent conversation (needs Session notes on in Settings > Extensions)"),
        format_help_command("session-note save --session-id <id> [--project-id id] --note '<text>' [--json]", "Attach a note to the session's agent conversation; an empty note clears it"),
    ]
    .join("\n");

    let show_connectors = super::connectors::connectors_enabled();
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
        format_help_command(
            "park-session <selector> [true|false] --json",
            "Park or unpark a session, honoring Sleep session when parking",
        ),
        format_help_command(
            "issues status | linear-key [--project-id id] [--clear]",
            "Issues Workflow (formerly work-mode, which still works as an alias): say what is set up (it is on for every project once a Linear key applies or gh is signed in), or save the Linear API key from stdin",
        ),
        format_help_command(
            "issues create-ticket --title t [--description d] [--team-id id] [--linear-project-id id] [--no-assign] [--start] | start <SPX-1245|#218|PR link> [--pr 412] [--agent id] [--model m] [--effort e] [--cloud [--provider claude-code] [--prompt-file path]]",
            "Create a ticket assigned to you (a Linear ticket, or a GitHub issue when the workspace's tracker is GitHub), or start an agent on a ticket in a worktree on its branch, linked to it; nothing is sent to the agent. --cloud starts a Claude Code cloud session on the ticket's branch instead, with the file's task or one drafted from the ticket, and prints its link",
        ),
        format_help_command(
            "issues source [set <path> [--switch] | check | clear | credential <name> [--clear]] [--workspace name|id]",
            "A workspace's ticket command, a program that describes its own tracker: show it, set or re-check its path (it runs the command's info and keeps the id it answers with; --switch accepts a new id), clear it (its credentials stay), or store one of its credentials from stdin (--clear removes it). Only this computer can set it, and a team workspace can't",
        ),
        format_help_command(
            "issues tracker [linear|github|<command id>] [--workspace name|id | --project-id id | --path path]",
            "Show or set the workspace's primary tracker: Linear tickets & projects, GitHub issues & projects, or the id of the workspace's ticket command (a team workspace's owners set Linear or GitHub for the whole team)",
        ),
        format_help_command(
            "issues cleanup <session> clean-up|keep",
            "Answer the offer a session gets when its linked PR is merged: remove its worktree and park it, or keep both",
        ),
        format_help_command(
            "workspace list | create <name> [--kind work|personal] | rename <workspace> <new name> | move-project <project> <workspace> | delete <workspace>",
            "List this computer's workspaces and their projects, create, rename or delete one, or move a project (by id, name or folder) into one; a workspace is named by id or name",
        ),
        format_help_command(
            "connectors [status] | sign-in | sign-out | refresh | connect <app> | tools | call <tool> [--args <json>]  (each takes --workspace name|id; sign-in and connect take --no-open)",
            "A workspace's connectors: sign it in to Composio in your browser, connect one of its apps (gmail, slack, discordbot to read a Discord server…), list the apps and tools its agents get, and run a tool that only reads",
        ),
        format_help_command(
            "connectors gh|gws [status] | sign-in | sign-out | gws client <client_secret.json>|default  (each takes --workspace name|id; sign-in takes --no-open)",
            "A workspace's own GitHub CLI or Google Workspace CLI sign-in: Claude and Codex agents started in that workspace use it instead of your usual one",
        ),
        format_help_command(
            "connectors rules | rule <tool|all> <ask|allow|never|default>  (each takes --workspace name|id)",
            "What a workspace does when an agent's connector call would send, post, change or delete something: ask on a card in the session (the default), allow it, or never run it, for every call or one tool",
        ),
        format_help_command(
            "connectors executor [status] | enable | disable | catalog [words] | add <app> | add-mcp <name> <url> | connect <app> | remove <app> | tools [app] | call <app> <tool> [--args <json>]  (each takes --workspace name|id)",
            "A workspace's Executor: turn it on (Ghostex downloads Executor the first time), add apps from its catalog or by MCP server address, sign them in, list their tools and run one that only reads",
        ),
        format_help_command(
            "team join <invite link> [--name n] [--slack-user U…] | status | invite | identity [--slack-user U…] [--linear-user id] | threads <ticket> | ping | leave | deploy [--team-name n] [--project slug] [--convex-team slug] [--dev]  (each takes --workspace name|id)",
            "Connect a Work workspace to its team's Convex project: join with an invite link, or deploy Ghostex's functions with your Convex CLI login",
        ),
        format_help_command(
            "team flow [show] | set [--working-channel C…] [--watch-only C1,C2] [--default-run cloud|local] [--linear-team KEY] [--qc-owner U…] [--instructions-file path] | map <channel> --repo owner/name [--project name] [--linear-team KEY] [--linear-project name] | unmap <channel>",
            "Read or change the team's Slack flow: the working channel, watch-only channels, which repo each channel's work goes to, where new work runs, and the team instructions",
        ),
        format_help_command(
            "team slack-manifest [--name n] | slack-connect [--dev] | linear-connect [--remove] | own-linear-key on|off",
            "Print the Slack app manifest for this team; store the Slack bot token and signing secret on the team's Convex project (read from stdin); owners set or remove the team's Linear key (read from stdin); own-linear-key on creates the Slack tickets you request with this workspace's Linear key instead",
        ),
        format_help_command(
            "slack post [--session <session>] \"<text>\" [--final]",
            "Post to the Slack working thread of the session's ticket; --final also posts it once in the threads the request came from",
        ),
        format_help_command(
            "link-session <selector> [--pr n|url|none] [--linear SPX-1,SPX-2|none] [--issue n|none] [--linear-project name|none] [--github-project owner/number|none] [--auto] | --candidates pullRequest|linearIssue|linearProject|githubIssue|githubProject [--query text]",
            "Link a session to a PR, Linear or GitHub issues, a Linear project or a GitHub Project; --auto goes back to what its branch says; --candidates lists what the Link to picker suggests",
        ),
        format_help_command(
            "link add|remove|restore|pin|unpin|refresh|list [<url|#123|owner/repo#123|ABC-123>] [--session <session>] [--issue] [--json]",
            "Add, remove, restore, pin, unpin or list the PRs, GitHub issues and Linear tickets a session is linked to; restore brings back a link you removed, pin keeps one at the top of every list and in the sidebar, refresh reads them now; without --session it is the session the command runs in, so an agent can report the PR or issue it opened",
        ),
        format_help_command(
            "tag-session <id> <tag|none>",
            "Set or clear a session tag (built-in tag, or a custom tag by name or id)",
        ),
        format_help_command(
            "session-groups [--project-id id] [--json]",
            "List the session groups of every project, or of one: group id, title and member session ids. The group verbs below edit them through the running Ghostex app",
        ),
        format_help_command(
            "create-session-group --project-id id [--title title] [--json]",
            "Make a session group in a project (named Group <n> without --title) and print its group id",
        ),
        format_help_command(
            "rename-session-group --project-id id --group-id id --title title [--json]",
            "Rename a session group",
        ),
        format_help_command(
            "delete-session-group --project-id id --group-id id [--json]",
            "Close a session group and its sessions, as the sidebar's Close Group does; the closed sessions head the History list, where they can be restored",
        ),
        format_help_command(
            "move-session-to-group <selector> --group-id id [--json]",
            "Put a session in one of its own project's session groups",
        ),
        format_help_command(
            "remove-session-from-group <selector> [--json]",
            "Take a session out of its session group, back to the project's own list",
        ),
        format_help_command(
            "delayed-send <id> (--delay-ms <n> | --when-agent-finishes | --when-all-agents-finish) | --cancel",
            "Arm or cancel a Session Automations Enter trigger",
        ),
        format_help_command(
            "close-after-done <id>",
            "Toggle Close After Done for a session",
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
            "spaceo --help",
            "Show Ghostex SpaceO skill setup for driving Mac apps on SpaceO's virtual displays (macOS only)",
        ),
        format_help_command(
            "cli --help",
            "Show general Ghostex CLI discovery and agent skill setup",
        ),
        format_help_command(
            "agents --help",
            "Communicate and coordinate with other agents",
        ),
        format_help_command(
            "agents switch-account <session-ref> --account <number|email> [--force] [--json]",
            "Move a Claude or Codex session to another account and resume its conversation there; --force ends a running turn first and sends \".\" after",
        ),
        format_help_command(
            "agents reload <session-ref> [--json]",
            "Stop a stuck session's agent and start it again on the same conversation (its current turn is lost)",
        ),
        format_help_command(
            "coordinator --help",
            "Run a coordinator: one agent you talk to that starts, supervises, and reports on thread sessions",
        ),
        format_help_command(
            "agentbox status | list | url <session> [--screen] [--json]",
            "Check agentbox readiness, list boxes, and print a box session's web app or screen URL",
        ),
        format_help_command(
            "cloud providers | connect | list | create | resume | stop | delete | spend | cap [--json]",
            "Run agent sessions on Cloud Boxes: connect Instabox or Boat, start, wake, stop and delete boxes, and see this month's spend (ghostex cloud --help)",
        ),
        format_help_command(
            "agent-cli status | install <agent> | update <agent> [--json]",
            "Install, update, and check the agent CLIs (claude, codex, cursor, grok, …)",
        ),
        format_help_command(
            "agents-orchestration --help",
            "Show Ghostex Agents skill setup (launch and coordinate other agents)",
        ),
        format_help_command(
            "manage-beads --help",
            "Show Ghostex Manage Beads project-board skill setup",
        ),
        format_help_command(
            "generate-title --help",
            "Show Ghostex Auto Rename Session skill setup",
        ),
        format_help_command(
            "move-codex-session --help",
            "Show Ghostex Move Codex Session skill setup",
        ),
        format_help_command("guide --help", "Show Ghostex Help skill setup"),
        format_help_command(
            "agent-sync status | plan | apply [--agent id] [--json]",
            "Point every agent at the shared ~/.agents skills, instructions, and hooks",
        ),
        format_help_command("toggle-sidebar", "Collapse or expand the sidebar"),
    ]
    .into_iter()
    .filter(|line| show_connectors || !line.starts_with("  connectors "))
    .collect::<Vec<_>>()
    .join("\n");

    // CDXC:Cli 2026-09-06 DECISION:
    // User: keep `ghostex web` working, but hide it from the main --help listing while the web app is not shipped.
    let server_commands = [
        format_help_command(
            "ports [--json]",
            "List the TCP ports listening on this machine",
        ),
        format_help_command(
            "resources [--json]",
            "Print the CPU and RAM rows the desktop Resources panel shows",
        ),
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
            "server endpoint",
            "Print the running daemon's port and auth token as JSON (for SSH clients)",
        ),
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
            "Print the newest log lines, oldest first; --grep searches the whole log (warnings and errors are always logged, info lines only while their Diagnostic logs area is on)",
        ),
        format_help_command(
            "bundle [output-dir] [--lines n]",
            "Save state, logs, and a screenshot",
        ),
        format_help_command(
            "hookless-activity [--reset]",
            "Compare Claude, Codex and OpenCode activity seen by their hooks and by their own state files (needs the Hook-free activity comparison Diagnostic logs area)",
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

Settings and help:
{settings_and_help_commands}

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
  gx find and gx f open the built-in Zehn prompt-history picker; gx history and gx h open the transcript viewer.
  Direct attach stays available through attach/a/resume/r.
  Projects and sessions follow the macOS sidebar order, including the active Last Active sort mode.
  Each project prints its path once as the section header, then compact session rows without field labels.
  --ungrouped/-u prints one flat list and prefixes each row with the project name.
  --json prints each session's agent, title, global ref, agent session id, zmx name, project and status;
  add --full for every field Ghostex keeps about each session and project.

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
  Everyday Ghostex work (sessions, orchestration, automations, quick actions,
  chat queues, prompt history, server, diagnostics) is covered by ghostex --help
  and the focused help pages. Use $ghostex-embedded-browser-use,
  $ghostex-browser-use, $ghostex-computer-use, $ghostex-spaceo,
  $ghostex-manage-beads, $ghostex-agents, $ghostex-auto-rename-session, or
  $ghostex-move-codex-session when their domain applies. Use $ghostex-help to
  explain how a Ghostex feature works or to change an app setting for the user
  (ghostex guide, ghostex settings).
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
  install <id> [--json]                 Install an extension from the catalog
  install-local <path> [--json]         Validate and install a local extension folder
  uninstall <id> [--json]               Remove an installed extension
  state <id> [--set k=v] [--json]       Read or update extension state

State keys:
  enabled, pinned, placement, terminalPlacement,
  preferences.<name>, preferences, grantedPermissions
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

Thread execution:
  Prefer agentSessionId for Codex and other resumable agents. Ghostex sends the prompt to the
  existing pane when it still owns that conversation; if the pane has been closed, Ghostex
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
  Terminal and Browser quick actions are project customCommands, not agent buttons.
  Use run-action to execute them; run-command and click-button are renderer-only legacy paths.

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
            "board start-work <bead-id> [--agent id [--allow-hidden]] [--model m] [--effort e] [--project-path path|--project-id id] [--json]",
            "Dispatch a Project Board bead through gxserver",
        ),
        format_help_command(
            "board associate <bead-id> [--session-id id] [--project-id id] [--json]",
            "Show this session on the bead's card; without --session-id it links the calling session",
        ),
        format_help_command(
            "board install-skill [--agent id]",
            "Install the bundled Project Board beads skill for agents",
        ),
    ]
    .join("\n");

    format!(
        "Ghostex Project Board - dispatch bead work through gxserver

Usage:
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
  --model and --effort start a new Claude, Codex, Pi, OMP, Copilot, Grok Build, Hermes Agent, Empryo or Antigravity CLI worker
  on that model and effort for this session only (a Pi, OMP or Empryo model is <provider>/<model>, a Pi or OMP effort a
  thinking level; Cursor CLI and Gemini take --model only); your default
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
            "server endpoint",
            "Print the running daemon's port and auth token as JSON (for SSH clients)",
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

pub fn guide_usage() -> String {
    "Ghostex Help - the built-in guide and the agent skill that explains and configures Ghostex

Usage:
  gx guide                       Print the overview chapter
  gx guide <chapter>             Print one chapter: overview, features, settings, hotkeys
  gx guide list                  List the chapters
  gx guide --help
  gx guide install-skill [--json] [--agent <id>...]

Agent skill:
  Use $ghostex-help when the user asks how a Ghostex feature works, what a
  setting does, or wants an app setting changed for them.

What the skill teaches:
  Read ghostex guide (and the settings or hotkeys chapter) before answering,
  find the exact key with ghostex settings list or get, confirm the change with
  the user, apply it with ghostex settings set, and verify with settings get.
  Settings agents may not write are opened for the user with settings open.

Related commands:
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
  as \"Fork Session\" works too). Keys are written like cmd+shift+o: modifiers
  cmd, ctrl, alt (option) and shift, then one key (a letter, digit, symbol,
  f1-f24, up, down, left, right, tab, enter, escape, space, backspace, delete,
  home, end, pageup, pagedown). Separate a two-step sequence with a space. On
  Windows and Linux cmd means Ctrl. `none` unassigns a hotkey. set refuses keys
  another hotkey already uses unless --replace moves them (the other hotkey is
  left unassigned). Changes save through the running desktop app like the
  Hotkeys page and bind at once.

Tabs (for --tab):
  settings integrations extensions osIntegration remote projects agents
  accounts actions openTargets hotkeys debugging about
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

pub fn visuals_usage() -> String {
    "Ghostex Visuals - check chat visual blocks and install the agent skill

Usage:
  gx visual --help
  gx visual check <file.json|-> [--width N] [--light] [--out file.png]
  gx visual check <page.html> [--width N] [--light] [--out file.png]
  gx visual install-skill [--json]

Agent skill:
  Use $ghostex-visuals when the user asks for it: charts, stats and tables go
  in the reply as a ```visual block; questions go in a ```questions block,
  which the chat shows as a form; mockups and small tools are HTML pages
  published with `ghostex show`. When a question is about a PR, issue, file,
  page, commit, thread or anything else the user could open, link it in the
  question itself ([#407](https://github.com/owner/repo/pull/407), or a file
  path link) so they can check it before answering.

Pages:
  Ghostex paints every page's backdrop (a soft dusk in dark, dawn in light)
  and its glass cards, so a page never paints html or body. The chat's card
  previews the page's top: give it a strong header (.gx-eyebrow, a specific
  h1, a one-line .gx-lede, .gx-chip facts), put options side by side in a
  .gx-options row of tied sections (each gets a Pick button), one question
  under them, and keep it to about one screen. The skill's
  examples/plan-review.html shows the whole shape.

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
  in headless Chrome (or Chromium, Edge, Brave; GHOSTEX_CHROME picks one) at a
  phone's width, 390px, and a computer's, 1280px (--width N for one width).
  It saves a full-page screenshot of each and lists what reaches past the
  screen and what Ghostex had to fit for the phone (columns stacked, pills
  kept on one line, blocks scrolling sideways), with how to fix the page.
"
    .to_string()
}

pub fn manage_beads_usage() -> String {
    "Ghostex Manage Beads - install the agent skill for the project board workflow

Usage:
  gx manage-beads --help
  gx manage-beads install-skill [--json]

Agent skill:
  Use $ghostex-manage-beads when managing Ghostex project board beads with the
  machine-installed `bd` Beads CLI.

What the skill teaches:
  The project swimlane workflow (backlog, in_progress, test, review, close),
  progress comments humans can follow, session-link comments, external refs,
  and safe examples for review beads.

Boundary:
  Use the machine-installed `bd` CLI directly; do not depend on a bundled
  Ghostex copy of Beads.
"
    .to_string()
}

pub fn agents_orchestration_usage() -> String {
    "Ghostex Agents - install the agent skill for launching and coordinating other agents

Usage:
  gx agents-orchestration --help
  gx agents-orchestration install-skill [--json]

Agent skill:
  Use $ghostex-agents when one agent needs other agents to do
  part of the work: launch them with a specific model and effort, send them
  tasks, read their replies, wait for them to finish, and verify the result.

What the skill teaches:
  Read ghostex agents --help and ghostex --help first, resolve your own
  session and project, hand over self-contained tasks, keep the global
  reference of every created session, confirm delivery, wait on a last-line
  sentinel with wait-for-text, verify independently, and cap fix rounds.

Boundary:
  Use Ghostex CLI commands instead of raw zmx/tmux control when coordinating
  sessions inside Ghostex.
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

pub fn spaceo_usage() -> String {
    "Ghostex SpaceO - install the agent skill for driving Mac apps on SpaceO's virtual displays

Usage:
  gx spaceo --help
  gx spaceo install-skill [--json]

Agent skill:
  Use $ghostex-spaceo when a task needs a native Mac app or a Chromium page
  driven out of the user's way. SpaceO opens the app on a headless virtual
  display, so the agent clicks, types and takes screenshots there while the
  user keeps their own screen, pointer and focus.

Requirements:
  macOS only: an Apple Silicon Mac with macOS 14 or later. On Windows and Linux
  install-skill refuses: SpaceO is only available on macOS. Install SpaceO
  from Settings > Integrations, then grant Accessibility and Screen Recording
  from its rows there. `spaceo doctor` checks the Mac without changing it.

Boundary:
  Use $ghostex-spaceo for Mac apps that should run off-screen.
  Use $ghostex-computer-use to drive apps on the user's own screen.
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
        assert!(!text.contains("tui"));
        assert!(text.contains("no subcommand launches or activates the Ghostex desktop app"));
        assert!(!text.contains("gx 2"));
        assert!(text.contains("automation-mark-run-read --run-id id --path path"));
        assert!(text.ends_with("  -h, --help            Show this help\n"));
    }

    #[test]
    fn focused_automation_help_documents_skill_and_definition_contract() {
        let text = automations_usage();
        assert!(!text.contains("install-skill"));
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
        assert!(!text.contains("$ghostex-manage-automations"));
        assert!(!text.contains("$ghostex-agent-orchestration"));
        assert!(text.contains("$ghostex-manage-beads"));
        assert!(text.contains("$ghostex-embedded-browser-use"));
    }

    #[test]
    fn generate_title_usage_keeps_literal_shell_parameter_expansion() {
        assert!(generate_title_usage().contains(
            "ghostex rename-command --session-id \"${GHOSTEX_GLOBAL_SESSION_REF:-${GHOSTEX_SESSION_ID:-${ZMX_SESSION:-}}}\" --title \"<title>\""
        ));
    }
}

pub fn feedback_usage() -> String {
    r#"Usage:
  ghostex feedback send --title <text> --body-file <file> --github-user <name> [--image <file>]... [--dry-run] [--json]
  ghostex feedback send --title <text> --body <text> --github-user <name>

Files a Ghostex bug or feedback issue on the Ghostex GitHub through the Ghostex feedback relay,
the same way Send Bug / Feedback does. The issue starts with "Reported by @<name>" so GitHub
notifies that user, and Ghostex adds a line with its version and the operating system. Attach
up to five PNG, JPEG or WebP screenshots of 5 MB each with --image. The issue is public.
--dry-run prints the exact request (images shown by size) without posting it."#
        .to_string()
}

pub fn secret_usage() -> String {
    r#"Usage:
  ghostex secret request --label <text> --reason <text> [--placeholder <text>] [--timeout-minutes N]
  ghostex secret providers [list]
  ghostex secret providers add --name <text> --account <1Password account>
  ghostex secret providers remove <id>
  ghostex secret providers check --provider <id> --reference op://vault/item/field
  ghostex secret install-skill

request: shows a private card in the calling session's chat asking the user for a secret
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
Secrets). They need the 1Password CLI (op) signed in on this computer. check reads one
reference (1Password may ask to unlock) and says only whether it worked."#
        .to_string()
}

pub fn notify_usage() -> String {
    r#"Usage:
  ghostex notify --title <text> [--body <text>] [--session-id <alias|id|title>] [--json]
  gx notify --title <text>

Posts a notification to the Ghostex titlebar bell for a session. Without --session-id the
calling session is used, read from the Ghostex session environment every pane exports.
The row shows the title, the session's project, and the body, and counts as unread until
the session is opened or the row is marked read."#
        .to_string()
}
