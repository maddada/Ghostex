# Ghostex features

Hand-written companion to `settings.md` (generated) and `hotkeys.md`
(generated). When you add or rename a titlebar view, a sidebar surface, a
session capability, or a CLI verb that users interact with, update the matching
section here in the same change.

Each section ends with "Related settings" so the helper can turn an
explanation into a change with `ghostex settings set`.

## Views (titlebar tabs)

Every project has the same six built-in views, switched from the titlebar tabs
or with Option/Alt+1 through 9 in the visible order. Direct built-in view
shortcuts can be assigned in Settings > Hotkeys. Views other than Agents are extensions:
they load on demand, sleep when idle (Auto Sleep), and can be hidden or
reordered in Settings > Extensions > Titlebar views.
The full view tabs stay centered in the titlebar. When space is tight, they
become a dropdown on the left immediately after Next (Forward), before the
companion toggle and project name. Hovering a view shows its positional shortcut.

- **Agents**: the terminal grid. Panes and tabs run agent CLIs or plain shells,
  split horizontally or vertically, in one or more groups. Each pane can show
  the raw terminal or Session Chat. Cmd+T creates a session, Cmd+D splits.
  Cmd+Shift+T opens the New Thread picker: type to filter the configured
  agents (last used first), Browser, or Terminal, press Enter to start it in
  the active project, and press Tab on Claude or Codex to pick an account.
- **Code**: the built-in VS Code based editor (code-server). Opens files from
  chat links, `ghostex edit <file>`, and Open In. Optional Use VS Code settings
  reuses the local VS Code configuration.
- **Browser**: embedded Chromium tabs with profiles, splits, annotations,
  DevTools, and agent control through the `$ghostex-embedded-browser-use`
  skill. Web links from terminals, chat, and detected dev servers open here or
  in the system browser depending on Open links in.
- **Kanban**: the project board backed by the Beads `bd` CLI (see Project
  board).
- **Automate**: scheduled and triggered agent runs (see Automations).
- **Docs**: Markdown, HTML, and Excalidraw files from the project's docs
  folders, with a markdown editor and an annotation system that sends notes
  back to the agent.

Related settings: `terminalViewWidthMode`, `webLinkOpenTarget`,
`markdownFileOpenView`, `htmlFileOpenView`, the Auto Sleep rows
(`autoSleep*IdleMinutes`), and Settings > Extensions.

## Sidebar

The sidebar lists projects and their sessions. Project headers carry the git
branch and diff stats, an agent launcher, Add Worktree, and project actions.
Session rows show the agent icon, title, status, tags, and last-active time.
Top chrome holds the Quick section (projectless Quick chats and terminals),
tag filters, Spaces (saved filters), and More Options: Settings, Search by
Prompt, Previous Sessions, Mobile & Remote, Extensions, Tips.
Space icons keep their normal glyph and show amber working-session and blue
attention-session counts in extra-bold text centered inside each icon, including
the selected Space.

- Side and width: the sidebar sits left or right (`sidebarSide`, or
  `ghostex move-sidebar`); drag the divider to resize, double-click it to
  restore `sidebarDefaultWidthPx`. Cmd+B collapses it.
- Presets: Settings > General > Sidebar > Preset switches groups of card
  details at once; the individual rows below it are marked Advanced.
- Session cards: agent icon, favicon, close button, last-active time, git
  stats, colored icons, and rename-on-double-click are all toggles.
- Parking: with `enableSessionParking`, deferred sessions move to a Parked
  section at the bottom (optionally sleeping them).
- Remote machines appear as their own sidebar sections when connected.

Related settings: everything under General > Sidebar, `agentManagerZoomPercent`
(sidebar interface size), `sidebarProjectGroupStyle`, `sidebarSpacesEnabled`.

## Sessions

A session is one terminal pane. Sessions persist across app restarts (zmx keeps
the process alive) and are restored with the agent's resume command. From the
sidebar or `ghostex`, a session can be focused, renamed, pinned, tagged,
slept and woken (`ghostex sleep|wake <selector>`), forked, closed, or moved
between panes and groups. Titles are generated from the first prompt by the
Title Generation Agent, and `/rename` in chat renames manually.

- Sleeping frees RAM; Auto Sleep does it after idle minutes; Resources in the
  titlebar sleeps many at once and shows CPU and RAM per session.
- Previous Sessions (More Options or Cmd+P) lists every past conversation from
  every agent CLI with resume and fork.
- Search by Prompt (More Options, or `gx f` in a terminal) fuzzy-searches every
  prompt you ever sent to an agent; Enter resumes that session, and starred
  prompts stay on top. Ctrl+G is agents, Ctrl+J is projects inside the picker.
- Delayed Send arms Enter for later or when agents finish; Close After Done
  closes a pane once its command exits.

Related settings: `autoSleep*`, `clickToWakeSleepingSessions`,
`showSessionIdInTerminalPanes`, `sessionTitleGenerationAgent`,
`renameSessionOnDoubleClick`.

## Session Chat

Session Chat renders the same agent session as a chat GUI: composer with
image paste and Ctrl+G rich prompt editor, a prompt queue that sends when the
agent stops, transcript with thinking, tool, and edit cards, subagent
transcripts, question and approval cards, rewind, and a note per session.
Slash commands sent from chat stay in the conversation after a reload, together
with any captured output. Long command output expands when clicked; model, effort,
Fast mode, and compaction results keep their status rows.
Toggle chat and terminal for a session with one click on the pane header or
the pane hotkey. Compatible agents can default to chat. File writes and code
edits appear outside the tool groups while the agent works. When a turn shows
"Worked for", all its file changes are grouped in a collapsed "N files changed"
section directly below it. The count includes each file once, even if it was
edited repeatedly. Expand the section to see the files along an activity rail,
with added lines in green and removed lines in red. Each file defaults to one
collapsed row with its path and green/red change counts. Enable Show file edit
previews in Settings > Chat to show the first seven code lines by default.
Long paths truncate from the start, keeping the filename visible. Click anywhere
on the path or filename to open it in Editor or Docs, just like a file reference
pill. Right-click anywhere on the path for the same Copy Path and Locate File
options as file references. Hosts without an editor copy the path on click.
Click the card background, circle, or change counts to expand or collapse the full diff.
Only clicks directly on the path or filename open the file. The
circle's center turns white on hover. An open code preview and its left rail
also toggle the diff. After expanding or collapsing, the header stays visible;
chat scrolls to it if needed. This covers Claude's Write and Edit tools and Codex's apply_patch
changes.

Unsent chat drafts are saved automatically. Switching between Chat and Terminal
keeps a saved copy while the text moves, and a late transfer preserves anything
you have typed since. Saving and sync retries happen quietly in the background;
the input only warns if it cannot save on this computer. Open Saved Prompts >
Recovered for unsent text and earlier versions, including interrupted transfers.
Unsent drafts do not expire after five days, and short drafts remain available.
Inserting recovered text keeps the text already in your input; confirmed sends
retire the submitted draft without clearing a newer one.
If another saved draft is available, hover over or click its preview icon to
read the full text above the icon before choosing Use or Dismiss.

Related settings: `preferredAgentInterface`, `sessionChatTheme`,
`sessionChatFontFamily`, `sessionChatCustomTranscriptWidthEnabled`,
`sessionChatTranscriptWidthPercent`, `sessionChatVerboseMode`,
`sessionChatFileEditPreviews`,
`terminalViewWidthMode` (`match-chat` makes the terminal body the same width
as the chat transcript), `terminalWidthApplyToCommandPaneTerminals`.

## Terminal

Terminals are embedded Ghostty surfaces. Font, theme, cursor, padding,
scrollback, clipboard, and scrolling are Settings > General > Terminal rows and
are written into a managed Ghostty config; the Ghostty settings actions row
applies the recommended set or opens the raw config. Command-click opens links;
Cmd+V pastes images as previewable links; the Ctrl+G prompt editor opens Monaco
or Code for long prompts. Dev Servers detects localhost URLs from output and
lists them in Resources.

Related settings: `terminalFontFamily`, `terminalFontSize`,
`terminalGhosttyTheme`, `terminalCursorStyle`, `terminalPane*PaddingPx`,
`terminalScrollbackLimitMb`, `terminalCopyOnSelect`, `promptEditorBackend`,
`terminalDevServerDetectionEnabled`.

## Agents, actions, and orchestration

Agents are the launch buttons per project: Claude Code, Codex, Gemini CLI,
OpenCode, Pi, and more are built in, and custom commands can be added in
Settings > Agents. Agent Hooks let gxserver watch agent status, questions, and
completions for chat and notifications. Agent approvals ("accept all") is a
per-machine default with per-project overrides. Actions (Settings > Actions)
are saved terminal commands or browser URLs shown on project headers and in
the titlebar Actions menu; Global Actions apply to every project.

Cross-agent orchestration is built in: any agent with the `$ghostex-cli`
skill installed can run `ghostex` to start other agents and steer them. For
"make Claude Code control Codex":

1. Install the Ghostex CLI skill (Settings > Integrations, or
   `ghostex cli install-skill`). It is installed on first launch by default.
2. In a Claude Code session, ask it to use `$ghostex-cli`. It can then run
   `ghostex create-agent codex --project-id <id> --first-input-draft "<task>"`
   or `ghostex create-session --input "<prompt>" --start`, send follow-ups with
   `ghostex send-message <selector> "<text>"`, read output with
   `ghostex read-text` or `ghostex read-session-chat`, and wait with
   `ghostex wait-for-text`.
3. The optional Fable 5.6 Orchestration skill (`$ghostex-fable-56-orchestration`)
   packages a plan-with-Claude, implement-with-Codex, verify-with-Claude
   pipeline.

Related settings: Settings > Agents (Default Prompt Agent, Agent approvals,
Agent Hooks, Default view per agent), `agentAcceptAllEnabled`,
`showQuickModelPickerInTerminal` (Option+P model picker).

## Project board (Kanban)

The Kanban view is a board over the Beads issue tracker: every card is a bead
in the project's `.beads` database, driven by the `bd` CLI installed on the
machine that runs the project (Ghostex does not bundle it; the board's
"Install or Update Beads" action installs it). Columns follow the bead
statuses (backlog, open, in progress, test, review, done); cards have
priority, labels, comments, and linked sessions.

- "Start work" on a card dispatches an agent session to it (`ghostex board
start-work <bead-id>`); the session is linked to the card and the card shows
  who is working it. An agent started by hand links itself with
  `gx board associate <bead-id>`.
- The `$ghostex-manage-beads` skill teaches agents the swimlane workflow:
  claim with `bd update <id> --status in_progress`, comment progress with
  `bd comment`, move to test or review, and `bd close`.
- Ask an orchestrator agent to "work the high-priority beads on the board" to
  have it pick cards and spawn workers.

Related settings: Settings > Projects (beads directory and display key),
`globalBeadsDirectory`, `globalBeadsDisplayKey`.

## Automations

The Automate view schedules agent work per project: a name, an agent, a
prompt, a schedule (timer, once, interval, daily, weekly, or cron with a
timezone), and an execution mode (local checkout, a fresh worktree with an
optional setup command, or an existing agent thread). Runs are listed with
status, transcript, and an archive action; a run must end with an
`AUTOMATION_RESULT: <status>` line. Everything is also scriptable:
`ghostex automations --help` documents `automation-save`,
`automation-run-now`, `automation-set-enabled`, and `automation-state`.

## Remote machines, web, and mobile

Ghostex is a client/server system: gxserver runs on each computer and owns its
sessions, so any client can control agents on any machine.

- **From a phone**: sidebar More Options > Mobile & Remote (Settings >
  Remote). Easy Connect installs the Tailcat helper, turns on SSH access with
  one admin prompt, and shows a pairing QR code; scan it with the Ghostex
  mobile app (Android ships today). A Tailscale path is offered for tailnets.
  Paired devices are listed and can be removed.
- **From another computer**: Settings > Remote > Remote machines > Add a
  machine with SSH details or an Easy Connect code, then Install / Connect
  gxserver on it. The machine appears as a sidebar section with its own
  projects and sessions; its terminals stream into the desktop app.
- **Web app**: a static browser build of the same workspace UI that talks to
  gxserver.
- **CLI**: `ghostex attach <selector>` attaches to a session from any terminal,
  including over SSH.

Related settings: Settings > Remote (all rows are user-only; open them with
`ghostex settings open --tab remote`), `hideKeepAwakeTitlebarControl` and the
Keep Awake rows for machines that must stay reachable.

## Notifications and status

Ghostex tells you when an agent needs you: a completion sound when a session
finishes, an attention state on the session card, OS notifications on macOS,
menu bar badges with running and done counts (click one to jump to the
session), terminal bell detection, and push notifications on the mobile app.
The optional status pet in the sidebar mirrors session state.

Related settings: `completionSound`, `actionCompletionSound`,
`showMacOSAttentionNotifications`, `showNotificationOnTerminalBell`,
`hideMenuBarSessionStatusIndicators`, `petOverlayEnabled`.

## Git and worktrees

Project headers show the branch and diff stats; the titlebar Git menu offers
commit, sync with main, PR review by a prompt agent, and related actions with
persistent running toasts. Add Worktree on a project header creates a git
worktree as its own project so a second agent works on a branch without
touching the main checkout; worktrees can be renamed, merged back, and
deleted from the sidebar.

Related settings: Settings > Projects > Global Defaults (worktree command,
docs directory), `hideProjectHeaderDiffStats`,
`showProjectEditorDiffFileCount`,
`showUntrackedProjectDiffWhenNoTrackedChanges`.

## Extensions, Open In, and integrations

- Settings > Extensions manages the built-in views, the official extensions
  (Code, Browser, Kanban, Automate, Docs, Chromium runtime), the Extension
  store for audited third-party extensions, and Your views (custom URLs,
  Storybook, Linear, GitHub Issues, dev server commands, HTML reports).
- Settings > Open In chooses which apps appear on session and project Open In
  menus and adds custom open targets.
- Settings > Integrations installs the bundled agent skills (Ghostex CLI,
  Ghostex Help, Computer Use and Browser Use through Trycua, Embedded Browser
  Use, Project Board Beads) and shows their install status. Skills are copied
  into the global skill folders every agent CLI reads.
- Tips (titlebar) teaches features one card at a time; Resources lists dev
  servers, ports, docs, project links, and per-session CPU and RAM; Help
  (titlebar question mark) opens sample questions; picking one opens a
  Ghostex Help chat with the question staged so the user can edit it and
  press Enter.

## Appearance and app

Theme, background contrast and tint, accent color, active pane outline, and
the app icon live under Settings > General > Appearance. Keep Awake (Power)
prevents sleep while agents work. Storage shows the Ghostex data folders.
Advanced holds Enable Experimental Features and the Debugging rows (Show debug
UI controls gates diagnostic disk logging; leave these to the user).

Related settings: `sidebarTheme`, `customSidebarTitlebarBackgroundDarknessPercent`,
`customSidebarTitlebarBackgroundTintColor`, `accentColor`,
`showActivePaneOutline`, `appIconSourceId`, the `keepAwake*` rows,
`showBetaFeatures`, `debuggingMode`.

## Answering the common questions

- "Make Claude control Codex": see Agents, actions, and orchestration.
- "Match the terminal width to the chat": `ghostex settings set
terminalViewWidthMode match-chat`.
- "Move the sidebar to the right and make it narrower": `ghostex settings set
sidebarSide right`, then `ghostex settings set sidebarDefaultWidthPx 220`
  and tell the user to double-click the sidebar divider to apply the default
  width (dragging sets the live width).
- "Use Ghostex from my phone or another computer": see Remote machines, web,
  and mobile, then `ghostex settings open --tab remote`.
- "Run an agent on a schedule": see Automations; open the Automate view or
  drive `ghostex automation-save`.
- "Play a sound and notify me when an agent finishes": `completionSound`
  (any value except `off`), `showMacOSAttentionNotifications true`, and the
  menu bar badges via `hideMenuBarSessionStatusIndicators false`.
- "What does the Kanban board do": see Project board (Kanban).
