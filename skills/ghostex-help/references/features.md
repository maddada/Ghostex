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
  back to the agent. Folders appear as they load, and search fills in while
  Updating files is shown. Expand a folder to load it sooner; a loading or error
  marker means its contents have not been confirmed yet. Use Refresh in the Docs
  sidebar menu to check for changes immediately. The button at the sidebar's
  window edge hides the files list; the same button in the corner brings it
  back, and hovering it, or the last few pixels along that edge, peeks the list
  without pinning it. Hidden or pinned is remembered. When the Docs view is narrower than 800px the list opens as a
  temporary drawer over the document and closes when you open a file, press
  Escape, or click outside it. Cmd+F, or Ctrl+F on Windows and Linux, opens the
  search: inside a Markdown document it shows Find and Replace with the caret
  ready, and anywhere else it reveals the files list and focuses its search.
  Escape closes the document search.

Related settings: `terminalViewWidthMode`, `webLinkOpenTarget`,
`markdownFileOpenView`, `htmlFileOpenView`, the Auto Sleep rows
(`autoSleep*IdleMinutes`), and Settings > Extensions.

## Sidebar

The sidebar lists projects and their sessions. Project headers carry the git
branch and diff stats, an agent launcher, Add Worktree, and project actions.
Session rows show the agent icon, title, status, tags, and last-active time.
Top chrome holds the Quick section (projectless Quick chats and terminals),
tag filters, Spaces, and More Options: Settings, Search by
Prompt, Previous Sessions, Mobile & Remote, Extensions, Tips.
Spaces group projects or groups together; they are not saved filters, and a
filter cannot be saved as a Space. Create one with the "Create space" button
that fills the Space row while you have none, by right-clicking the Other
button or a Space icon and choosing New Space, or from the More menu when
Spaces overflow.
Space icons keep their normal glyph and show amber working-session and blue
attention-session counts in extra-bold text near the bottom of each icon, including
the selected Space.
Switching to a Space brings back what you last had open there: the session you
last used in that Space, in the view its project was in (Agents, Code, Browser,
Kanban, Automate, or Docs). If that session was closed, the one before it is
used; a Space you have never used opens its first project. Choose "Don't switch
projects" to make a Space switch change only the sidebar filter
(`sidebarSpaceSwitchBehavior`). "Follow the active session's Space"
(`sidebarSpaceFollowActiveSession`, off by default) switches the selected Space
to the one that owns a session you open from outside it, for example through
Back/Forward, Search by Prompt, a notification, or Previous Sessions; otherwise
the Space row only marks that Space with a dot.
Switching projects by any route keeps the project's last view; only clicking a
session inside the project you are already in switches to Agents.

- Side and width: the sidebar sits left or right (`sidebarSide`, or
  `ghostex move-sidebar`); drag the divider to resize, double-click it to
  restore `sidebarDefaultWidthPx`. Cmd+B collapses it.
- Pane memory: the companion and Commands panes are remembered for Agents and,
  separately, for the wide views (Browser, Code, Docs, Kanban, Automate), the
  same for every project, so switching projects never moves them. The sidebar
  keeps one state everywhere by default; "Sidebar visibility memory"
  (`sidebarVisibilityMemory`, Advanced) can remember it per view instead, and
  then a project switch that hides it leaves it floating while you hover it.
  While collapsed, hovering the 10px edge on the sidebar's side reveals it as
  a floating panel; in a wide view with the companion hidden, the lower half
  of that edge reveals the companion instead.
- Presets: Settings > General > Sidebar > Preset switches groups of card
  details at once; the individual rows below it are marked Advanced.
- Session cards: agent icon, favicon, last-active time, git stats, colored
  icons, and rename-on-double-click are all toggles.
- Session hover buttons (click to toggle, drag to reorder), under General >
  Session Cards, is a strip
  of icons: Rename, Pin, Note, Snooze, Close After Done, Tag, Park, Sleep,
  Close, and a chevron. Click an icon to turn that hover button on or off;
  drag icons to reorder them. Buttons to the right of the chevron always show
  at the right end of a hovered session card; buttons to its left stay hidden
  until the chevron is clicked, which reveals them on every card in that
  project until the chevron (now pointing right) is clicked again. Each
  project remembers its choice across restarts. Turning the chevron off shows
  every enabled button at once. By default the strip is Tag, Park, Sleep,
  chevron, Close, so a hovered card shows a chevron and Close until you open
  it. Hover an icon on a card to see its name; buttons flip
  to the reverse action on an active row (Unpin, Wake, Unsnooze, Unpark,
  Cancel Close After Done). An enabled button is left out of the session's
  right-click menu (and its Advanced submenu), so turning Close off puts
  Close back in the menu. Browser tabs ignore the strip and always show Sleep
  and Close. Setting: `sessionCardHoverButtons` (a list of `{ id, enabled }`
  with ids `rename`, `pin`, `note`, `snooze`, `closeAfterDone`, `tag`,
  `park`, `sleep`, `close`, `chevron`).
- Long projects: a project with more sessions than Compact Session Rows (13
  by default, up to 50) starts in Compact mode and shows only that many rows
  plus a "Show all N sessions" row. Rows inside a collapsed Pinned, Browser,
  Parked, or Snoozed section do not count. Click that row, or the chevron on
  the project header, to switch the project to Full mode, which shows every
  row; the chevron switches it back to Compact. Each project remembers its
  mode. The sidebar is the only scroller, and a project's header stays pinned
  at the top while you scroll through its rows. Setting:
  `projectSessionListCollapsedCount`.
- Parking is enabled by default. Right-click a session and choose Park, or
  select several sessions and choose Park selected, to move them into the
  collapsible Parked section at the bottom. Use Unpark or Unpark selected to
  bring them back. Parking keeps sessions running unless Sleep session when
  parking is enabled (off by default). Park & Snooze with tags (on by
  default) makes Park and Snooze open the Tag as menu: pick a tag to tag and
  park in one step, or the No tag change row at the top to park as is.
  Closing that menu without choosing does not park. Unpark after sending a
  message (on by default) moves a parked or snoozed session back out of its
  section as soon as you send it a message from chat or type a prompt into
  its terminal; Codex only notices chat sends. All four settings are in
  General > Sidebar without Show Advanced: `enableSessionParking`,
  `sleepSessionWhenParking`, `showTagMenuWhenParking`,
  `unparkAfterSendingMessage`.
- Snooze puts a session away until a chosen time: right-click it and choose
  Snooze (or use the Snooze hover button), then pick 1 hour, 3 hours,
  Tomorrow (9:00) or Next week (Monday 9:00). The session moves into a
  collapsible Snoozed section below Parked and is put to sleep. When the time
  passes it returns to its usual place, still asleep until you open it.
  Unsnooze brings it back early. Snooze needs no setting; it is always
  available.
- Remote machines appear as their own sidebar sections when connected.

Related settings: everything under General > Sidebar, `agentManagerZoomPercent`
(sidebar interface size), `sidebarProjectGroupStyle`, `sidebarSpacesEnabled`,
`sidebarSpaceSwitchBehavior`, `sidebarSpaceFollowActiveSession`.

## Commands pane

The Commands pane holds command terminals below the workspace, or on its right
when Command Pane Side is set to Right. Open it with F12. Auto-minimize Commands
pane is on by default: after you move focus elsewhere and leave the pointer
outside the pane for 1 minute, it minimizes while commands keep running.
Focusing, hovering, selecting text, scrolling, or resizing keeps it open and
restarts the countdown. Background command output does not restart it.
In Settings > General > Sidebar, turn auto-minimize off or set Minimize after
to 15 seconds, 30 seconds, 1 minute, 2 minutes, or 5 minutes. When auto-minimize
is enabled, the Keep open button appears beside the minimize chevron. It shows
an open lock when off and a highlighted closed lock when on. Keep open pauses
auto-minimize for the current project, useful for watching logs. Click
it again to allow auto-minimize, or manually minimize to clear Keep open.
Keep open survives project switches but resets when the app restarts.
Related settings: `commandsPanelAutoMinimize`,
`commandsPanelAutoMinimizeDelaySeconds`, `commandsPanelSide`,
`commandsPanelDefaultHeightPx`.

## Sessions

A session is one terminal pane. Sessions persist across app restarts (zmx keeps
the process alive) and are restored with the agent's resume command. From the
sidebar or `ghostex`, a session can be focused, renamed, pinned, tagged,
slept and woken (`ghostex sleep|wake <selector>`), forked, closed, or moved
between panes and groups. A session carries one tag at a time, chosen from the
Tag as menu (right-click the session): the built-in Priority, Progress, and
Type tags, plus any custom tags you define. Custom tags are created in one
place, Settings > General > Sidebar > Sidebar Tags: choose Add tag, then give
it a name, an icon from the shared icon list, and a color from the preset list.
New tag at the bottom of the Tag as menu opens that same place with the form
ready. Custom tags then appear in the same drag list as the built-in ones: drag
to reorder, use the switch or eye to hide them from menus and filters, and the
trash button to delete one (sessions that carried it become untagged). The
mobile app shows the same tags. `ghostex tag-session <selector> <tag>` accepts a
custom tag by name. Claude and Codex name their own sessions; Ghostex
syncs those names without running a first-prompt title job or blocking terminal
input. Pi and OMP use the Title Generation Agent for first-prompt names.
Manual Generate Name and `/rename` in chat remain available for Claude and Codex.

- Sleeping frees RAM; Auto Sleep does it after idle minutes; Resources in the
  titlebar sleeps many at once and shows CPU and RAM per session.
- Drag pinned sessions to reorder them within their project. Rows stay in place
  while an icon-and-title ghost follows the pointer; the insertion line marks
  where the session moves when you drop it.
- Previous Sessions (More Options or Cmd+P) lists every past conversation from
  every agent CLI with resume and fork. The History icon immediately to the
  right of Add Worktree on a project header opens Quick Access > Sessions with
  that project selected and Closed active, ready to search sessions you closed.
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

Use Cmd+P (Recent Sessions) to jump between chats across projects, or
Cmd+Ctrl+[ and Cmd+Ctrl+] to go back and forward through visited sessions.
Recently visited chats show their loaded messages while catching up with the
agent. Each chat remembers your reading position, expanded tool cards, and
composer cursor. Older messages load as needed when returning to a place in
the conversation's history. Shortcuts: `openSessionSearchPalette`,
`navigateHistoryBack`, `navigateHistoryForward`.

Star items in Context details to show them in the status line under the chat
box. Items without a value are hidden until their data is available again;
your starred selections stay saved.

Codex can ask questions while it keeps working. These appear above the composer,
so you can keep writing your next message. Choose a suggested answer or write
your own, then press Enter or Send answer; Shift+Enter adds a new line, and
selecting an option alone sends nothing. An orange spinner with a blue dot in
the sidebar means the agent is working and has an unanswered question. The dot
stays visible until you answer or skip, even while that chat is focused; if the
agent finishes first, the blue attention dot remains.
Use the arrows to move between questions, collapse the panel to answer later,
or Skip a question without interrupting the agent.

Press Ctrl+Shift+Down to scroll the focused chat to the bottom, including while
typing. The Scroll to bottom button shows your current shortcut. Both stop any
ongoing scroll momentum so the conversation settles at the bottom. This takes
priority over paragraph selection or adding a cursor in the composer; rebind or
clear Scroll Chat to Bottom in Settings > Hotkeys (`scrollChatToBottom`).
Codex rewind continues in a new conversation before the selected prompt and
returns that prompt for editing. If the chat cannot reconnect after the rewind,
choose Retry synchronization in the dialog to reconnect without rewinding again
(`ghostex rewind-session-chat <session> --message-id <message-id>` retries the same pending target).
Sending in a new chat shows your message immediately in the conversation while
Ghostex waits for the agent to be ready. It appears once, with a waiting status;
you can retry or remove it if delivery fails. Prompts you explicitly
queue stay in the list above the input. This also applies when reopening the chat
or continuing on another device.
Claude children stay in the Subagents card while the terminal lists them, including
between monitor events. Idle children are labelled Idle and their clocks pause;
click a child's name or task to open its transcript.
For Codex and Claude, the Subagents card and popup title show the child's latest
model and effort in compact form, such as Opus 5 High or Astra xHigh. Codex rows
show the child's name/path beside the model and effort, after a ‣ separator.
For Claude, hover to see the agent type, such as Explore or general-purpose.
Unrecorded model values are labelled Model not recorded.
Slash commands sent from chat stay in the conversation after a reload, together
with any captured output. Long command output expands when clicked; model, effort,
Fast mode, and compaction results keep their status rows.
While Claude Code writes a reply, the chat shows the text as it appears in the
terminal, updated about once a second, and swaps in the saved message the moment
Claude records it; nothing to enable.
Chat Appearance defaults to System, following your computer’s light or dark appearance as it changes. Choose Light or Dark to keep chat in one palette; the surrounding app stays dark. Set it in Settings > Chat with `sessionChatTheme`.

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
pill. Folder links in desktop chat open the folder in your system file explorer.
Right-click anywhere on a path for Copy Path, just like file references.
Hosts without an editor copy the path on click.
Click the card background, circle, or change counts to expand or collapse the full diff.
Only clicks directly on the path or filename open the file. The
circle's center turns white on hover. An open code preview and its left rail
also toggle the diff. After expanding or collapsing, the header stays visible;
chat scrolls to it if needed. This covers Claude's Write and Edit tools and Codex's apply_patch
changes.

Summary mode has its own button between More actions and Session note when the
chat toolbar has room. In a narrow chat, find it under More actions instead.
The button highlights when Summary mode is on; its tooltip shows the shortcut.

Unsent chat drafts are saved automatically. Switching between Chat and Terminal
keeps a saved copy while the text moves, and a late transfer preserves anything
you have typed since. Saving and sync retries happen quietly in the background;
the input only warns if it cannot save on this computer. Open Saved Prompts >
Recovered for unsent text and earlier versions, including interrupted transfers.
Recovered shows one entry per session. Identical copies are combined, and
Earlier versions lets you read, copy, or insert previous text without filling
the main list with typing edits. Search includes earlier versions too.
Unsent drafts do not expire after five days, and short drafts remain available.
Inserting recovered text keeps the text already in your input; confirmed sends
retire the submitted draft without clearing a newer one.
Sending a chat message, including a delayed chat send, replaces any text still
in the terminal input. Ghostex checks that the agent's input is ready and empty
before inserting the message; spaces and newlines alone count as empty. If it
cannot confirm this, delivery stops and the chat draft or queued message is kept.
If another saved draft is available, hover over or click its preview icon to
read the full text above the icon before choosing Use or Dismiss.

Settings > Accounts saves Claude and Codex logins and marks each one Automatic
or Manual. Quick launch, the main launcher row, and any session started without
picking an account use the Account for new sessions rule under each
provider's New session defaults. The automatic rules only consider Automatic
accounts: Auto (the default) weighs remaining limit against time to reset and
picks the account that can absorb the most usage before any of its limits
resets, Most limit remaining picks the account with the most limit left, Soonest
reset the one whose limit resets first, Most used first keeps draining the
account already in use, and Same as last session reuses the account of the last
session. Pick a specific account instead to always start
with it. When the rule finds no account, new sessions use the current CLI login.
Terminal notices in Claude and Codex chats also offer Switch account beside
Open terminal, so you can choose another account directly from a usage-limit warning.
In the chat's More actions menu, click Switch Account to open its submenu;
hovering over it does not open it.
Switching a running Claude or Codex session to another account, from More
actions > Switch Account, a terminal notice, or automatically when its account hits a usage limit,
exits the CLI inside its own terminal and resumes the same conversation there,
so the terminal tab and the chat stay open. A card in the middle of Session Chat
shows the current and selected accounts, their usage percentages (including
Claude's Fable limit), and the switch progress. It stays until the switch
finishes, briefly confirms success, or shows a failure with Retry switch.
A manual switch waits for your next
message without sending anything. An automatic switch sends a "." to continue
the interrupted work once the new account is ready. Configured recovery after
errors can also continue work on the same account.
Account sign-in terminals open in the active local project's folder and appear
under that project. Before a first project is chosen, sign-in uses the home folder.

Hide emails in Settings > Accounts keeps the first and last characters before
`@` and shows the same `•••••.•••` for every domain, with no blur effect.
It also masks email addresses in the status
line below the chat box, the context meter's More details popover, and the
Context details dialog previews and hover text. It also covers account choices
and selected dropdown values, account setup and reconnect fields, and account
errors and recovery messages in Settings, launchers, and the titlebar usage popup.

The context meter above the chat box opens a popover whose More details rows
are grouped under Usage & cost, Context & cache, and Session. Its pen icon
opens the Context details dialog: the filter bar at the top finds a row by its
title, description, or current value; switch rows on or off, drag them to
reorder within their group, and star a row to show its value in the status
line under the chat box. Every row holds one value, for example Cost, Session
time, and API time, or 5h limit, 7d limit, Model limit (such as Fable), 5h
reset, and 7d reset, read from the session's saved account, or from the agent
itself when the session has no account.
Claude Code and Codex keep separate choices; the copy buttons in the dialog
header transfer them between the two.

Related settings: `hideAccountEmails`, `preferredAgentInterface`, `sessionChatTheme`,
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
Cmd+V pastes images as previewable links. Ctrl+G opens the Ghostex prompt editor
or your machine default editor for long prompts. The Ghostex editor uses the
same text editing controls as the chat composer, with F1 commands, find/replace,
undo/redo, and image previews. Cmd+S/Ctrl+S or Ctrl+G saves and closes it; Cancel
leaves the original prompt unchanged. Dev Servers detects localhost URLs from output and
lists them in Resources.

Terminal links (`ghostex://terminal`) without a folder open in the active local
project. A folder supplied in the link takes precedence.

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

Agents Hub lets you browse and edit agent files in Skills, MDs, Hooks, and
Configs & MCPs. In MDs, expand Shared agent markdown to see the files in your
shared agent folder, then select a filename to read or edit it. Expand the
profile instruction groups the same way. Use Refresh to reload files from
disk and Save to write your edits.

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

The Notifications bell sits in the titlebar right after the Next button and
shows how many notifications are unread. Click it to open the Notifications
panel: one row per session, newest first, saying whether the agent finished a
turn or needs your input, with the last thing it said. Click a row to jump to
that session and mark it read; hover a row to dismiss it; the header has Next
unread, Mark all read, and Clear all. Hover a header button to see its configured
hotkey when one is available. Hotkeys: Cmd+I opens the panel,
Cmd+Shift+U jumps to the latest unread notification, and Cmd+Ctrl+U pushes the
current session to the back of the unread queue and jumps to the next one.
Scripts and agent hooks can post their own rows with
`ghostex notify --title <text> [--body <text>]`.

Related settings: `completionSound`, `actionCompletionSound`,
`showMacOSAttentionNotifications`, `showNotificationOnTerminalBell`,
`hideMenuBarSessionStatusIndicators`, `petOverlayEnabled`,
`notificationsTitlebarButtonHidden`.

## Git and worktrees

In New Project, paste a folder path, `cd ~/dev/my-app`, a quoted path, or a
path with shell-escaped spaces to browse it on the local machine. A selected
machine stays selected; `saved-machine:/path` selects a saved machine by name
or ID. GitHub, GitLab, Bitbucket, Azure DevOps, and other Git URLs open the
clone flow. Bare `owner/repo` offers Local folder and GitHub repository;
the folder path is relative to the current project or the machine's Add
project starts in folder. Copied `git clone`, `gh repo clone`, and
`glab repo clone` commands prefill the repository, destination, branch
(`-b` or `--branch`), and supported options (`--single-branch`, `--depth 1`).
Repository file links identify the repository; unambiguous branch links
also prefill the branch. Registered paths offer Open existing project, and
files inside a Git repository offer its root. Press Enter to continue,
choose a destination, and review before Clone & Add.

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
  Extension commands use the active local project's folder unless the extension
  supplies a folder; relative folders are resolved inside the active project.
  Its Titlebar account usage section lets you star saved Claude and Codex
  accounts to show their usage in the desktop titlebar, or unstar them to hide
  it. These are the same per-account stars available in Settings > Accounts.
  Claude buttons show the two tightest of the weekly, five-hour, and Fable
  limits, so the Fable limit is never hidden when it is running out; launcher
  and picker rows and the Accounts figures use the same two numbers. Each
  button opens that login's live limits, reset times, and extra usage or rate
  limit resets, with the Fable limit as a main bar for Claude. Click the same
  usage button again to close its dropdown. Clicking outside, including in
  Session Chat, closes usage dropdowns and Tips. More model
  limits starts collapsed. Click the Codex reset
  count to see each reset's expiry date. Redeem a reset opens a Codex terminal
  in the active project's folder, shows it under that project in the sidebar,
  and redeems the reset expiring soonest for the selected account. The project
  and account must be on the same computer. If Codex needs attention or the
  reset cannot be confirmed, continue in that chat. Shared history stays visible
  below: today's, yesterday's, and the last 30 days' token totals with a daily
  trend. History combines conversations across accounts of the same provider
  on that computer, counts shared copies once, and includes cached tokens.
  Claude and Codex histories stay separate. Switching account buttons changes
  the live limits; the shared history remains the same. Totals come from saved
  conversation logs, so they may omit usage whose logs are missing.
- Settings > Open In chooses which apps appear on session and project Open In
  menus and adds custom open targets.
- Settings > Integrations installs the bundled agent skills (Ghostex CLI,
  Ghostex Help, Computer Use and Browser Use through Trycua, Embedded Browser
  Use, Project Board Beads) and shows their install status. Skills are copied
  into the global skill folders every agent CLI reads. When the computer is
  online they are downloaded from the Ghostex GitHub repository, so skill fixes
  arrive between releases, and installed skills are refreshed automatically
  each time Ghostex starts. Offline installs use the copy inside the app.
- Tips (titlebar) teaches features one card at a time; Resources lists dev
  servers, ports, docs, project links, and per-session CPU and RAM; Help
  (titlebar question mark) opens sample questions; picking one opens a
  Ghostex Help chat with the question staged so the user can edit it and
  press Enter.

## Appearance and app

Theme, background contrast and tint, accent color, active pane outline, and
the app icon live under Settings > General > Appearance. Keep Awake (Power)
prevents sleep while agents work.
Advanced holds Enable Experimental Features and the Debugging rows (Show debug
UI controls gates diagnostic disk logging; leave these to the user).
Settings that depend on a setting above them have an indented ↳ before their
name. They appear when the parent setting enables them.
In the Settings table of contents, click a page or section title to go there.
Only the small chevron on its right expands or collapses its entries.

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
  (any value except `off`), `showMacOSAttentionNotifications true`, the
  menu bar badges via `hideMenuBarSessionStatusIndicators false`, and the
  titlebar bell (kept visible with `notificationsTitlebarButtonHidden false`)
  lists every finished turn with what the agent said.
- "What does the Kanban board do": see Project board (Kanban).
