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
    if command_name == "page" {
        text.push_str(PAGE_HELP);
    }
    text.push_str("\nEvery command: ghostex --help");
    text
}

const PAGE_HELP: &str = "
Examples:
  ghostex page retire 0123456789abcdef0123456789abcdef
  ghostex page retire http://127.0.0.1:8899/visual/0123456789abcdef0123456789abcdef --json

Use it for a page you replaced with another file, or that is out of date. You do not need it after
publishing a newer version of the same file with ghostex show: the newest card already replaces
the older ones. The page's card then reads Replaced; nothing is sent to you.
";

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
pub(super) fn automation_help_commands() -> Vec<String> {
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
            "List every Claude and Codex account with its state, usage and reset times, the default, and the sessions on it, then the Gemini (Antigravity), Cursor and Copilot usage",
        ),
        format_help_command(
            "accounts hub [list|add|update|remove]",
            "List, add, change or remove CLIProxyAPI hubs; keys are read from a file or standard input only",
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
            "computer-code [--once] [--json] [--copy]",
            "Print this computer's pairing code (the one Connect a Computer shows) to paste into Connect a Computer on another computer; a computer that uses it waits for your Approve in Ghostex on a computer or phone that owns this one, which this command names; on a computer with no owner yet, or whose only owner is its own Ghostex app, approve it here when its three words match; --copy also puts the code on the clipboard; works on a server without the app; run it in your own terminal, not in a Ghostex session",
        ),
        format_help_command(
            "computers [--json]",
            "List the computers this one is connected to; any agent can then start, message and read sessions there with --computer <name> (sessions, agents create/send/list, coordinator start-thread, read-session-chat, read-text and the other session verbs), or with a session ref from that computer",
        ),
        format_help_command(
            "computers add <code> | remove <name> | access <name> on|off",
            "Server links: link this computer with another one's gxserver directly, no desktop app needed (two headless servers, say); paste the code ghostex computer-code prints there; you approve in Ghostex or on your phone once for each computer; remove ends it on both; run it in your own terminal, not in a Ghostex session",
        ),
        format_help_command(
            "autopilot [status|on|off|log [--last n]|project <id> on|off|session <project-id> <session-id> on|off|read-only-plan on|off] [--json]",
            "Autopilot keeps sessions going while you are away (carries on, interrupts a hung command, reloads, switches account at a limit) and logs every decision; Settings > Autopilot has every choice",
        ),
        format_help_command(
            "create-session [title] [--input text] [--start] [--project-id id] [--group-id id] [--cwd path]",
            "Create a terminal session (ghostex projects lists the project ids); --start materializes the live terminal immediately; --group-id (with --start) puts it in one of the project's session groups (ghostex session-groups lists them); --cwd starts it in another folder while it stays in the project",
        ),
        format_help_command(
            "create-agent <agentId> --project-id id [--group-id id] [--model m] [--effort e] [--first-input-draft text] [--run-on location] [--account number|email] [--defer-start] [--replace-empty-sessions] [--allow-hidden]",
            "Create and start a configured agent session (ghostex projects lists the project ids); --group-id puts it in one of the project's session groups; an agent turned off in Settings > Agents is refused unless --allow-hidden is passed (ghostex agents types lists the visible ones); --model/--effort (Claude, Codex, Antigravity CLI, Copilot, Grok Build, Hermes Agent; Pi and OMP take provider/model and a thinking level; Cursor CLI takes --model only) apply to this session only; Empryo takes provider/model and starts on it without changing Empryo's default; --first-input-draft stages text in its input without sending; --run-on local|docker|hetzner|vercel|daytona|e2b|digitalocean|docker:<host> runs Claude, Codex, OpenCode or Pi in an agentbox box; --account starts a Claude or Codex session on that saved account (one that needs signing in is refused)",
        ),
        format_help_command(
            "board guide",
            "How to work a Project Board bead with bd: link this session first, swimlanes, review beads",
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
            "secret guide",
            "How to ask the user for a secret and use it without printing it (a sudo password, 1Password)",
        ),
        format_help_command(
            "secret request --label text --reason text [--placeholder text] [--timeout-minutes N]",
            "Ask the user for a password, API key or token through a private card in this session's chat, and wait for the answer. The value never enters the chat or your context: when saved, the result names an owner-only file (deleted after 24 hours) to read inside the command that needs it. Use it instead of asking for a secret in chat, and for a sudo password",
        ),
        format_help_command(
            "grokbot [status] | connect [<url>] [--key-stdin] | notify [--kind ask|blocked|done|report|info] \"<text>\" | test | disconnect | brief | instructions",
            "Connect the Ghostex bot in Grok Bot (xAI) so the user can drive agents from it and agents can reach them: connect keeps its webhook routine's address and key (asked without showing it, never printed), notify sends the user a note through it, brief is the bot's current command list",
        ),
        format_help_command(
            "needs-you add \"what to do\" [--detail text] [--command text] [--answer label]... | list [--all] | done Q<n> | drop Q<n>",
            "Put a step only the user can do (a sudo command, a tap on their phone, a sign-in, a check by eye) above this session's chat box as a to-do with a Done button; it stays until they press Done or you clear it, and their answer arrives as \"Q61: Done\". Add one without being asked whenever such a step blocks your work",
        ),
        format_help_command(
            "message show <M12|G9jt7.M12> | list [--last N] [--grep words]",
            "Read one of the user's numbered messages whole (M12 in this chat, G9jt7.M12 in another), or list this chat's with their first line. When the user points at a message (a pill, #M12 or G9jt7.M12, not a bare M12), its start is already quoted at the bottom of their prompt",
        ),
        format_help_command(
            "work item add|now|step|ask|wait|done|drop|list … | work topic [\"one line\"] | work next-tag",
            "Keep this session's work items and its Topic in the Work panel (ghostex coordinator item is the same command); an ordinary session keeps its own while the Work items extension is on. ghostex work --help has the rules",
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
            "feedback brief --github-user name [--image file]...",
            "Get a report folder and the steps Send Bug / Feedback's agent follows to file an issue (read only, private things kept out, the draft shown to the user before feedback send posts it); posts nothing",
        ),
        format_help_command(
            "show <file.html> [--title text] [--json] [--no-check]",
            "Publish a Live page (once called a Ghostex page or gx page): a self-contained HTML page, and print the ```visual block that shows it as a card in the chat; the card opens it where the reader's Open Live pages in setting says (a floating window over the chat by default). Questions written as <gx-questions> (or the page's own ghostex.submit(answers)) send the reader's answers back to you as a message; the block then lists the questions' tags, so the chat counts the page as unanswered until they arrive. When Chrome is installed it then opens the page at 360 and 600px and at 900px with large text and says what overflows, overlaps or did not fit (--no-check skips that). Ghostex paints the page's backdrop and glass cards itself; for a page that looks good at a glance (a strong header, options side by side in .gx-options with Pick buttons, one question under them, about one screen) follow `ghostex visual guide pages` and the page `ghostex visual guide example` prints",
        ),
        format_help_command(
            "page retire <page>... [--json]",
            "Retire Live pages you published and then replaced or no longer need: each page (its id or its address, the \"url\" in the block ghostex show printed) and its open questions stop waiting on the reader in every chat that shows it (also one that printed its block after yours), so they leave the dock, the ? pill, the board's Needs you and the sidebar's dot on every computer and phone; the page still opens. A newer ghostex show of the same file already retires its older pages by itself, in every chat that shows them",
        ),
        format_help_command(
            "visual check <file.json|-|page.html> [--width N] [--text-scale P] [--light] [--out file.png]",
            "Draw a Ghostex visual (a ```visual block's JSON) to a PNG and print its path, or read a Ghostex form (a ```questions block) and list its questions, so an agent can check it before replying. Given a page (.html), opens it in headless Chrome at 360, 600, 900 and 1280px and with 130% text at 360 and 900px, saves a screenshot of each and lists, by CSS selector, what overflows its box, is cut off or is drawn over other text; it fails while anything does",
        ),
        format_help_command(
            "run-agent <agentId> [--allow-hidden]",
            "Run a configured agent button; a hidden agent needs --allow-hidden",
        ),
        format_help_command(
            "run-command <commandId>",
            "Trigger a renderer command button; use run-action for project and Global quick actions",
        ),
        format_help_command(
            "click-button <agent|command> <id>",
            "Trigger a renderer sidebar button; use run-action for project and Global quick actions",
        ),
        format_help_command(
            "projects [--all] [--json] [--computer name]",
            "List the projects with their ids, the --project-id that create-session, create-agent and the other project verbs take; --all adds parked projects (Recent Projects); read-only",
        ),
        format_help_command(
            "project status [--computer name | --all-computers] [--project-id id] [--json]",
            "This project's git state here and on every connected computer, as one table: uncommitted files, commits not on origin, stashes, and each submodule and worktree; an offline computer shows when it was last seen. Run it before a release; read-only",
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
            "relocate-project --project-id id --path <folder> [--json]",
            "Point a project whose folder is missing at the folder it is in now; its sessions move with it. Nothing on disk changes",
        ),
        format_help_command(
            "rename-project (--project-id id|--path path|--name name) <new name> [--computer name] [--json]",
            "Change a project's name in Ghostex; its id, folder, sessions, history, groups, board and settings stay. The folder on disk keeps its name. A worktree is renamed with Rename Worktree in the sidebar instead",
        ),
        format_help_command(
            "move-project-folder (--project-id id|--path path|--name name) <new folder path> [--dry-run] [--force-open-files] [--json]",
            "Move a project's folder on disk (same drive) and point Ghostex at it: the project, its sessions (Claude and Codex resume in the new folder), worktrees, board and automations. Refuses, changing nothing, while one of its sessions runs, another program works inside the folder or has files there open, or Files has unsaved changes there; --dry-run lists what would change; --resume or --abandon a move that stopped part-way. This computer only; agents run it only when the user asked for the move",
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
            "run-action <commandId> --project-id id [--scope project|global]",
            "Run a Terminal action in a session or return a Browser action URL (ghostex projects lists the project ids); --scope global runs a Global Action in that project",
        ),
        format_help_command(
            "run-in-terminal --project-id id [--session-id id|--cwd path] [--paste] [--json] (--command text | -- command...)",
            "Run a command in the project's Commands panel terminal (a new terminal in the session's folder without one; ghostex projects lists the project ids); --paste types it without pressing Enter",
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
            "settings export <file> [--json]",
            "Save this computer's settings to a JSON file for another computer (no secrets or sign-ins)",
        ),
        format_help_command(
            "settings import <file> [--dry-run] [--json]",
            "Apply a settings export here through the running desktop app and list what changed",
        ),
        format_help_command(
            "theme [read|set <key>=<value> ...|undo|redo|history] [--json]",
            "Read the look settings, change several as one change, undo or redo it, list the last 20 changes",
        ),
        format_help_command(
            "guide [overview|features|settings|hotkeys]",
            "Print the built-in Ghostex guide, or one chapter of it",
        ),
        format_help_command("guide --help", "Show Ghostex Help: the built-in guide's chapters"),
        format_help_command(
            "agent-guide",
            "Print what an agent running in Ghostex can do: Live pages, Ghostex forms and visuals in the chat, the browser, other agents, secrets, and the guide to read for each job",
        ),
        format_help_command(
            "help guide",
            "Print how Ghostex Help answers questions about Ghostex and changes settings, the theme and hotkeys for the user",
        ),
        format_help_command(
            "help ask \"<question>\" [--new] [--timeout-seconds N] [--json]",
            "Ask a Ghostex Help session (the default prompt agent following `ghostex help guide`) and print its answer once it finishes",
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
        format_help_command("clear-command <selector> [--timeout <seconds>] [--no-wait] [--json]", "Start a Claude Code or Codex session's conversation over (its own /clear) in the same session, and print the new conversation id; refused while it works, waits on you or has queued messages"),
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
            "issues create-ticket --title t [--description d] [--source <ticket command id>] [--team-id id] [--linear-project-id id] [--no-assign] [--start] | start <SPX-1245|#218|PR link> [--source <ticket command id>] [--pr 412] [--agent id] [--model m] [--effort e] [--cloud [--provider claude-code] [--prompt-file path]]",
            "Create a ticket assigned to you (a Linear ticket, a GitHub issue when the workspace's tracker is GitHub, or a ticket of the workspace's ticket command when that is the tracker or --source names it, which runs its create once and refuses Linear's flags), or start an agent on a ticket in a worktree on its branch, linked to it; nothing is sent to the agent. A ticket ID is read as the workspace's tracker's (its ticket command's, when that is the tracker and the ID has one of its keys), else as a GitHub issue number or a Linear ticket; --source names the ticket command to read it as. --cloud starts a Claude Code cloud session on the ticket's branch instead, with the file's task or one drafted from the ticket, and prints its link",
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
            "team join <invite link> [--name n] [--slack-user U… --slack-code gxl_…] | status | invite | identity [--slack-user U… --slack-code gxl_…] [--linear-user id] | threads <ticket> | ping | leave | deploy [--team-name n] [--project slug] [--convex-team slug] [--dev]  (each takes --workspace name|id)",
            "Connect a Work workspace to its team's Convex project: join with an invite link, or deploy Ghostex's functions with your Convex CLI login. Joining and invites are the user's, in Settings > Workspaces; the CLI is refused. deploy's last step (connecting this workspace) waits for the user's approval on their computer or phone. status, threads, ping and leave stay open",
        ),
        format_help_command(
            "team flow [show] | set [--working-channel C…] [--watch-only C1,C2] [--default-run cloud|local] [--linear-team KEY] [--qc-owner U…] [--instructions-file path] | map <channel> --repo owner/name [--project name] [--linear-team KEY] [--linear-project name] | unmap <channel>",
            "Read or change the team's Slack flow: the working channel, watch-only channels, which repo each channel's work goes to, where new work runs, and the team instructions. Only the user changes it, in Settings > Workspaces; set, map and unmap are refused here",
        ),
        format_help_command(
            "team slack-manifest [--name n] | slack-connect [--dev] | linear-connect [--remove] | own-linear-key on|off",
            "Print the Slack app manifest for this team; store the Slack bot token and signing secret on the team's Convex project (read from stdin); owners set or remove the team's Linear key (read from stdin); own-linear-key on creates the Slack tickets you request with this workspace's Linear key instead. linear-connect and own-linear-key are the user's, in Settings > Workspaces; the CLI is refused",
        ),
        format_help_command(
            "slack post [--session <session>] \"<text>\" [--final]",
            "Post to the Slack working thread of the session's ticket; --final also posts it once in the threads the request came from",
        ),
        format_help_command(
            "link-session <selector> [--pr n|url|none] [--linear SPX-1,SPX-2|none] [--issue n|none] [--ticket id:ID,id:ID|id:none] [--linear-project name|none] [--github-project owner/number|none] [--auto] | --candidates pullRequest|linearIssue|linearProject|githubIssue|githubProject [--query text]",
            "Link a session to a PR, Linear or GitHub issues, its ticket command's tickets, a Linear project or a GitHub Project; --auto goes back to what its branch says; --candidates lists what the Link to picker suggests",
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
            "Pages in Chrome, Edge or Electron apps: read the cua-driver skill",
        ),
        format_help_command(
            "computer-use --help",
            "Desktop apps through Cua Driver: read the cua-driver skill",
        ),
        format_help_command(
            "spaceo --help",
            "SpaceO: drive Mac apps on hidden virtual displays (macOS only)",
        ),
        format_help_command(
            "spaceo guide",
            "How to drive a Mac app or a Chromium page on SpaceO's hidden display, out of the user's way (macOS only)",
        ),
        format_help_command(
            "spaceo status [--json]",
            "Say whether SpaceO can give agents screens, or why it is stuck (read-only)",
        ),
        format_help_command(
            "spaceo repair",
            "Unstick SpaceO with its operator recovery; for you at a terminal, never for agents",
        ),
        format_help_command(
            "cli --help",
            "How to work with the Ghostex CLI, and the guide for each job",
        ),
        format_help_command(
            "agents --help",
            "Communicate and coordinate with other agents",
        ),
        format_help_command(
            "agents guide",
            "How to work with other agents: habits, Copy Details, other computers, stuck sessions, reading a thread, renaming",
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
            "agents wait-reply <session-ref> [--contains <text>] [--timeout <seconds>] [--after <message-id>|now] [--json]",
            "Wait until the agent's next reply finishes (optionally one holding the text) and print it; exits 1 with a timeout message",
        ),
        format_help_command(
            "chief --help",
            "Ghostex AI, the Ghostex-wide coordinator pinned at the top of Main Hub: its status, its playbook, a new chat, its agent, and messaging another computer's Ghostex AI",
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
            "agent-hooks status | install <agent>... | --all [--computer <name>] [--json]",
            "Check and install Ghostex's session resume hooks, here or on a connected computer",
        ),
        format_help_command(
            "agents-orchestration --help",
            "Launching and coordinating other agents (ghostex agents guide)",
        ),
        format_help_command(
            "manage-beads --help",
            "Project Board beads (ghostex board guide)",
        ),
        format_help_command(
            "generate-title --help",
            "Naming a session (ghostex agents guide)",
        ),
        format_help_command(
            "move-codex-session --help",
            "Moving a Codex session to another folder",
        ),
        format_help_command("guide --help", "Show Ghostex Help: the built-in guide's chapters"),
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
            "server endpoint [--no-token]",
            "Print the running daemon's port and auth token as JSON (for SSH clients); --no-token leaves the token out",
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
            "debug show-modal <name> | --list",
            "Open a one-time pop-up (rail-intro) on this computer's Ghostex window for checks and screenshots; its once-only marker and your settings stay as they are",
        ),
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
  cli --help            How to work with the Ghostex CLI
  quick-actions --help  Show focused project quick-action help
  server --help         Show server command help
  help                  Show this help
  -h, --help            Show this help
"
    )
}
