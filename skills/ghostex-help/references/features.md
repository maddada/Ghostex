# Ghostex features

Hand-written companion to `settings.md` (generated) and `hotkeys.md`
(generated). When you add or rename a titlebar view, a sidebar surface, a
session capability, or a CLI verb that users interact with, update the matching
section here in the same change.

Each section ends with "Related settings" so the helper can turn an
explanation into a change with `ghostex settings set`.

Context menus share the sidebar's rounded appearance and follow the current
light or dark theme. Click a submenu to open it, and click the same row again to
close it; it stays open as you move the pointer across other rows. Long menus
scroll vertically to keep every action reachable, without a horizontal
scrollbar.

## Views (titlebar tabs)

Every project has the same six built-in views, switched from the titlebar tabs
or with Option/Alt+1 through 9 in the visible order. Direct built-in view
shortcuts can be assigned in Settings > Hotkeys. Views other than Agents are extensions:
they load on demand, sleep when idle (Auto Sleep), and can be hidden or
reordered in Settings > Extensions > Titlebar views.
The full view tabs stay centered in the titlebar. When space is tight, they
become a dropdown on the left after the Notifications bell that follows Next
(Forward), before the project name. Hovering a view shows its positional shortcut.
**Hide sidebar** toggles the sidebar. The matching **Hide companion** / **Show companion**
button sits immediately beside it in every view (greyed out in Agents, which has no
companion pane), so Back and Forward stay in the same place, and it uses the same
outlined chat bubble with text lines whether the companion is visible or hidden.
When an update is available, a download button appears just before the project name.

Right-click Code, Browser, Kanban, Automate, Docs, or another web-based view's
titlebar button for **Reload** and **Sleep** (or **Wake** when sleeping), followed by **Extensions**. Reload
refreshes the clicked view (the focused tab in Browser); a sleeping view opens
again. Sleep unloads the view while keeping its place, and Code also stops its
editor server. Choose Wake or select the view again to wake it. Resources can stop Code too,
without closing Ghostex. Custom project views also offer **Command output** and
**Configure view** before **Extensions**. Configure view opens that view's editor
in Settings > Extensions and focuses its name field.

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
  in the system browser depending on Open links in. Annotate the current page
  with Agentation in the Browser toolbar; GitHub pages disallow that tool.
  When a page shows its content inside a frame, such as a Storybook story,
  the Annotate toolbar opens inside that frame so the content itself can be
  selected. Copying or sending annotations clears them afterwards by default;
  the toolbar's own settings panel (Clear on copy/send) turns that off.
  HTML files in Docs use the same Agentation overlay via Annotate. Markdown
  files use Docs selection comments instead (see Docs below).
- **Kanban**: the project board backed by the Beads `bd` CLI (see Project
  board).
- **Automate**: scheduled and triggered agent runs (see Automations).
- **Docs**: Markdown, HTML, and Excalidraw files from the project's docs
  folders, with a markdown editor and an annotation system that sends notes
  back to the agent. Select text in a Markdown file to comment on it, mark it
  Looks good, Clarify, or Needs tests, or mark it Remove this (the X button, or
  press D), and add a global comment from the header. Unselect the text to
  close the toolbar. In the comment box, Add (or Cmd+Enter, Ctrl+Enter
  on Windows and Linux) adds the note to the list; the same chord outside the
  box is Send. Send (or Cmd+Enter) delivers the new notes as
  numbered feedback with line numbers to the session last clicked in the
  sidebar for the active project: into its chat composer when the chat is
  showing, or into the agent's terminal when its input box is available. The
  Send button reads Send plus the count, or Copy plus the count when notes
  will go to the clipboard, and is icon-only on a narrow Docs pane; the
  tooltip names the session it will land in. When no agent session is
  selected, the agent's input box is busy, or Ghostex cannot tell for that
  agent, the feedback is copied to the clipboard instead and a toast says so.
  Sending leaves Docs on screen. Sent notes stay visible with a Sent mark;
  Send offers the new notes first and, once everything has been sent, sends
  all of them again, and the Review menu offers Resend all. With notes in
  several files, the Review menu sends the new notes across all files as one
  message. Notes stay until you clear them with Clear. In a chat, the Reply by Annotating button below an
  agent reply (between Copy message and Save to md) opens that reply in Docs so
  it can be annotated the same way, with the feedback going back to that
  session. Folders appear as they load, and search fills in while
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
Right-click a project for Open Folder in the file manager or Add to Group.
Click a project header (or the chevron beside it) or a group header to expand
or collapse it; rename a group from its right-click menu.
Close Project parks the project in Recent Projects; when it held the active
session, Ghostex stays in the current Space and switches to an awake session
of the next project in the list.
Session rows show the agent icon, title, status, tags, and last-active time.
Ctrl+Tab and Ctrl+Shift+Tab (also Cmd+Shift+] and Cmd+Shift+[ on Mac) move to
the next or previous session shown in the sidebar, the same keys Chrome uses
to switch tabs. Sessions inside collapsed projects or sections, or hidden by a
project's Show less, are skipped. Sleeping sessions are included; turn on "Skip
sleeping sessions" (Settings > Hotkeys, under Next Session) to jump over them. To switch tabs inside a split pane instead,
use Cmd+Alt+] and Cmd+Alt+[ (Ctrl+Alt+] and Ctrl+Alt+[ on Windows and Linux).
Shortcuts: `focusNextSession`, `focusPreviousSession`, `focusNextPaneTab`,
`focusPreviousPaneTab`; setting: `sidebarSessionCycleSkipsSleeping`.
Top chrome holds the Quick section (projectless Quick chats and terminals),
tag filters, Spaces, and More Options: Settings, Search by
Prompt, Previous Sessions, Mobile & Remote, Extensions, Tips.
Spaces group projects or groups together; they are not saved filters, and a
filter cannot be saved as a Space. Create one with the "Create space" button
that fills the Space row while you have none, by right-clicking the Other
button or a Space icon and choosing New Space, or from the More menu when
Spaces overflow.
A project added with Add Project (from the More menu, from the "Add Project"
button that an empty project list or empty Space shows, or by right-clicking
the empty sidebar area) joins the Space that is open at the time and appears at
the top of it; add a project while Other is selected to leave it out of every
Space.
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
Switching projects by any route keeps the project's last view. Clicking a session
inside the current project opens it in the visible companion pane, or switches
to Agents if the companion is hidden.
Leaving a project (by switching Spaces or projects) does not close what you had
open there: the terminals, chats, and view page that were on screen stay running
in the background for the "Keep the previous project live for" number of minutes
(`projectSwitchKeepAliveMinutes`, default 10, 0 to 60), so switching back is
instant. Set it to 0 to release them as soon as you leave.
Starting a new agent from the sidebar launcher or New Thread picker keeps your
current view open, including Code, Browser, Kanban, Automate, and Docs. Select
Agents when you want to open the new agent there.
The companion pane holds a second session two ways, chosen with the two buttons
in its title bar: split it vertically to stack the sessions, or split it to the
right to put the second session in its own sidepane beside the first. A fresh
side-by-side pair starts at 440px each where the window is wide enough for it,
and narrower windows give both sidepanes less so the main pane keeps its own
minimum. Drag the divider between the two sidepanes to rebalance them,
double-click it to make them even again, and drag the outer divider to resize the
pair together. Clicking the other arrangement's button rearranges the two
sessions you already have instead of starting a third. The button for the
arrangement you are in reads "Show one companion session": in a side-by-side pair
it keeps the sidepane you click it in, and in a stacked split it keeps the active
session, and the companion goes back to the width it had before it was split.
Click inside either companion pane to make it active. Selecting another session
in the sidebar or creating a new session replaces that active pane's session and
leaves the other pane in place.

- Width: the sidebar sits on the left; drag the divider to resize,
  double-click it to restore `sidebarDefaultWidthPx`. Cmd+B collapses it.
- Reveal active session: the hollow-circle titlebar button expands its section and scrolls
  the active session into view with 50px of space from the top or bottom edge
  (below any pinned headers, where scrolling allows), then blinks its outline
  twice: pale blue in light mode and white in dark mode. Active sessions also have a slightly
  stronger background and border in light mode.
- Pane memory: the companion and Commands panes are remembered for Agents and,
  separately, for the wide views (Browser, Code, Docs, Kanban, Automate), the
  same for every project, so switching projects never moves them. The sidebar
  keeps one state everywhere by default; "Sidebar visibility memory"
  (`sidebarVisibilityMemory`, Advanced) can remember it per view instead, and
  then a project switch that hides it leaves it floating while you hover it.
  While collapsed, hovering the 10px edge on the sidebar's side reveals it as
  a floating panel; in a wide view with the companion hidden, the lower half
  of that edge reveals the companion instead.
- Pane width: agent panes and chat companion sidepanes have a minimum resize
  width of 388px. In the desktop app, the main pane in Code, Browser, Kanban,
  Automate, and Docs has a minimum width of 455px. Two side-by-side companion
  sidepanes share that 388px each where they fit; on a window too narrow for both
  they divide the width they have.
- Presets: Settings > General > Sidebar > Preset switches groups of card
  details at once; the individual rows below it are marked Advanced.
- Timed Delayed Send: open **Delayed Send** from an agent's right-click menu
  under **Advanced**. Session Automations opens with **When all agents finish**
  selected. Choose **After a delay** for hours and minutes, or
  **Specific time** for a future date and time, then **Save changes**.
  Specific time is available in Session Automations on desktop, web, and mobile.
  It uses the local time of the device where you set it, calculates the remaining
  wait when you save, and uses the same timed send.
  For an active timed send, right-click the agent and choose **Postpone By**, then
  **10 Minutes**, **30 Minutes**, **1 Hour**, **2 Hours**, or **5 Hours** to add
  that duration to its existing send time. The same submenu has **Edit Delayed Send**
  to reopen its settings and **Disable Delayed Send** to cancel the pending send.
- Session cards: agent icon, favicon, last-active time, git stats, colored
  icons, and rename-on-double-click are all toggles.
- Session hover buttons (click to toggle, drag to reorder), under General >
  Session Cards (shown with Show Advanced), is a strip
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
  Cancel Close After Done). The enabled buttons also lead the session's
  right-click menu, top to bottom in the card's right-to-left order (Sleep,
  Park, Tag As by default), with the other actions after them. Close is the
  exception: while it is on the card it is never in the menu, and turning it
  off puts Close back as the menu's last row. Hover buttons also in context
  menu (on by default) controls the rest; turn it off and every enabled
  button leaves the menu (and its Advanced submenu).
  Browser tabs ignore the strip and always show Sleep and Close. Both rows
  live under General > Session Cards and need Show Advanced. Settings:
  `sessionCardHoverButtons` (a list of `{ id, enabled }` with ids `rename`,
  `pin`, `note`, `snooze`, `closeAfterDone`, `tag`, `park`, `sleep`,
  `close`, `chevron`) and `showSessionCardHoverButtonsInContextMenu`.
- Long projects: a project with more sessions than Compact Session Rows (13
  by default, up to 50) starts in Compact mode and shows only that many rows
  plus a "Show all N sessions" row. Rows inside a collapsed Pinned, Browser,
  Drafts, Parked, or Snoozed section do not count. Click that row, or the chevron on
  the project header, to switch the project to Full mode, which shows every
  row; the chevron switches it back to Compact. Each project remembers its
  mode. The sidebar is the only scroller, and a project's header stays pinned
  at the top while you scroll through its rows. Setting:
  `projectSessionListCollapsedCount`.
- Sidebar section headings (Pinned, Sessions, Drafts, Browser, Parked, and
  Snoozed) show an orange dot when a session is working, a blue dot when
  a session is done, and a pink dot when an agent is waiting for an answer,
  including rows hidden by collapse or Compact mode. Pending questions use
  pink instead of blue; multiple dots appear when multiple states are present,
  including orange and pink when an agent keeps working after asking. Collapsed
  headings show their session count at the right edge and a muted gray hollow
  circle after the status dots when they contain the active session. The circle
  disappears when expanded without moving the other dots. Hovering gives the
  header a subtle square background and replaces the dots with a chevron.
  Click anywhere across the heading row, including the empty space to its
  right, to animate the section closed or open. Sessions keep their full size
  and spacing while the section reveals or clips the list. The animation follows Sidebar
  Collapse Animation and reduced-motion preferences. Related setting:
  `sidebarCollapseAnimationDurationMs`.
- New sessions appear at the top of Sessions for 10 minutes. After that,
  a session with unsent text that has not received its first message moves
  into Drafts, below Pinned and above Sessions. Drafts starts collapsed;
  expand it to continue a draft. Choose Pin to move a draft into Pinned
  without sending or changing its text. Unpin returns it to Drafts after
  the 10-minute window, or to Sessions while still new. Sending its first
  message returns an unpinned draft to Sessions; pinned drafts stay in
  Pinned. Empty sessions remain in Sessions. No setting is required.
- Parking is enabled by default. Right-click a session and choose Park, or
  select several sessions and choose Park Selected, to move them into the
  collapsible Parked section at the bottom. Use Unpark or Unpark Selected to
  bring them back. On mobile, long-press a session and choose Park or Unpark;
  each project has its own Parked section, which starts collapsed. Parked
  sessions are always ordered from most recently active to oldest on desktop,
  mobile, and web, including when ordinary sessions use manual sorting.
  Mobile parking needs a connected computer with the `ghostex park-session
<selector> true|false --json` command. Parking keeps sessions running unless Sleep session when
  parking is enabled (off by default). Park & Snooze with tags (on by
  default) makes Park and Snooze open the Tag As menu: pick a tag to tag and
  park in one step, or the No Tag Change row at the top to park as is.
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
(sidebar interface size), `sidebarSpacesEnabled`,
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
Tag As menu (right-click the session): the built-in Priority, Progress, and
Type tags, plus any custom tags you define. Custom tags are created in one
place, Settings > General > Sidebar > Sidebar Tags: choose Add tag, then give
it a name, an icon from the shared icon list, and a color from the preset list.
New Tag at the bottom of the Tag As menu opens that same place with the form
ready. Custom tags then appear in the same drag list as the built-in ones: drag
to reorder, use the switch or eye to hide them from menus and filters, and the
trash button to delete one (sessions that carried it become untagged). The
mobile app shows the same tags. `ghostex tag-session <selector> <tag>` accepts a
custom tag by name. Claude and Codex name their own sessions; Ghostex
syncs those names without running a first-prompt title job or blocking terminal
input. Pi and OMP use the Title Generation Agent for first-prompt names.
Manual Generate Name and `/rename` in chat remain available for Claude and Codex.
ZCode sessions rename from the sidebar and `ghostex rename-command` too: the name
is saved in ZCode's own session store once its session row exists (after the
first prompt), and ZCode's automatic naming will not replace it. Before that,
the rename is saved only in Ghostex and ZCode's later automatic naming may
replace it.
Fork starts the new session as `Fork: <original name>` and saves that name
through the agent's own rename command so it survives reopening the conversation.
Once a conversation has forks, a small branch button above the chat transcript
lists every session that shares the earlier history, including the thread you
forked away from, and switches to the one you pick; a stopped branch is resumed
when you open it.

- Sleeping frees RAM; Auto Sleep does it after idle minutes; Resources in the
  titlebar sleeps many at once and shows CPU and RAM per session. Clean RAM
  copies a diagnosis prompt; paste it into an agent session to reduce RAM use.
  Sleeping sidebar sessions keep their normal title color and show a dimmer
  last-active time on the right; awake sessions show a stronger timestamp. Use `ghostex sleep|wake <selector>` to
  sleep or wake a session.
- A sleeping session stays asleep until you ask for it. Selecting its tab,
  clicking its row in the sidebar, opening its project, Split Right, and
  closing the tab next to it all show a black "Press Any Key to Wake" pane;
  click that pane or press a key to wake it. With Click to Wake Sleeping Panes
  turned off, selecting a sleeping session wakes it right away
  (`clickToWakeSleepingSessions`).
- Drag pinned sessions to reorder them within their project. Rows stay in place
  while an icon-and-title ghost follows the pointer; the insertion line marks
  where the session moves when you drop it.
- Recent Sessions (Cmd+P) opens Quick Access to jump between sessions.
  Its four tabs are Commands, Projects, Sessions, and Saved Prompts; Cmd+1
  through Cmd+4 switch between them. Cmd+Shift+P opens Commands directly to
  search app commands, pane actions, and project actions.
  Previous Sessions in More Options lists past conversations from every agent
  CLI with resume and fork. The History icon immediately to the
  right of Add Worktree on a project header opens Quick Access > Sessions with
  that project selected and Closed active, ready to search sessions you closed.
  Quick Access also discovers existing Claude, Codex, and ZCode conversations
  from outside Ghostex. Opening an imported ZCode conversation resumes it in
  a terminal with `zcode --resume <session-id>`; install the ZCode CLI on that
  computer first. Deleted, archived, running, and subagent ZCode conversations
  are excluded from discovery.
- Search by Prompt (More Options, the floating Search by Prompt button at the
  bottom of Quick Access > Sessions, the `openFindPrompts` hotkey, default
  `cmd+shift+f`, or `gx f` in a terminal) fuzzy-searches every prompt you ever
  sent to an agent; Enter resumes that session, and starred prompts stay on
  top. Inside the picker the agent and project filters are dropdowns at the top
  right (Ctrl+G and Ctrl+J open them), Grouping (Ctrl+D) toggles day headers,
  and hovering any control shows its hotkey.
- Delayed Actions opens Session Automations. Send Enter defaults to **When all
  agents finish**. It can also run after a delay, when this agent finishes, or
  **When a specific agent finishes**. Choose the specific agent from the Agent sessions
  on the same computer; sleeping sessions are excluded. Ghostex waits until the
  selected agent has remained idle for 10 seconds and restarts that wait if it
  resumes work. Close After Done closes a pane once its command exits.

Related settings: `autoSleep*`, `clickToWakeSleepingSessions`,
`showSessionIdInTerminalPanes`, `sessionTitleGenerationAgent`,
`renameSessionOnDoubleClick`.

## Session Chat

In Settings > Chat, **Use GPUI chat** selects the desktop chat renderer. It is on by default, so desktop uses the native GPUI chat. Turn it off to go back to React chat. Restart the desktop app after changing it. Mobile and web keep their existing chat renderer (`sessionChatUseGpui`).

Session Chat renders the same agent session as a chat GUI: composer with
image paste and Ctrl+G rich prompt editor, a prompt queue that sends when the
agent stops, transcript with thinking, tool, and edit cards, subagent
transcripts, question and approval cards, rewind, and a note per session.
Hover a message to show its actions and the time it was sent in a row below
it: Copy message, Reply by Annotating, and Save to md under an agent's final
reply; Rewind to here, Save prompt, and Copy message under your own messages.
Hover the time to see the full date.
Type `/` in the chat box to browse the agent's built-in commands. In Cursor
chats, `/compact` summarizes the conversation to reduce context, just like
`/summarize`.
In Claude Code and Codex chats, start a message with `!` to run a shell command
in that agent's session, for example `! pwd`. The command and its output appear
in the chat.
ZCode supports chat messages, thinking, tool results, attachments, and imported
conversation history. Install its hooks in Settings > Agents to connect new
conversations and keep activity in sync. ZCode runs in the same terminal, so
you can switch to Terminal for its setup, model menus, and permission prompts.
Scrolling up collapses the composer; returning to the bottom expands it.
An empty collapsed composer shows only the first placeholder line, and scrolling
keeps the same toolbar buttons visible.
Hex colors in messages, inline code, and tables have a small rounded color swatch
beside the value on desktop, mobile, and web. Copying keeps the original text.

Use Cmd+P (Recent Sessions) to jump between chats across projects, or
Cmd+[ and Cmd+] to go back and forward through visited sessions, the same keys
Chrome uses (Ctrl+Alt+Shift+[ and Ctrl+Alt+Shift+] on Windows and Linux).
Recently visited chats show their loaded messages while catching up with the
agent. On desktop, returning to a recently visited chat also restores its account
badge, context usage, and status line while their values refresh. The status
line appears as soon as its values are available, including on the first visit.
Each chat remembers your reading position, expanded tool cards, and
composer cursor. Older messages load as needed when returning to a place in
the conversation's history. Shortcuts: `openSessionSearchPalette`,
`navigateHistoryBack`, `navigateHistoryForward`.

Star items in Context details to show them in the status line under the chat
box. Items without a value are hidden until their data is available again;
your starred selections stay saved. Wrapped rows are centered and balanced where
space allows, with separators only between items on the same row.

Codex can ask questions while it keeps working. These appear above the composer,
so you can keep writing your next message. Choose a suggested answer or write
your own, then press Enter or Send answer; Shift+Enter adds a new line, and
selecting an option alone sends nothing. An orange spinner with a pink dot in
the sidebar means the agent is working and has an unanswered question. The dot
stays visible until you answer or skip, even while that chat is focused; if the
agent finishes first, the pink attention dot remains.
Use the arrows to move between questions, collapse the panel to answer later,
or Skip a question without interrupting the agent.
Unsent answers and selected options in question cards are saved on this computer
as you edit, so switching sessions or reopening Chat keeps them with their question.
Sending an answer or explicitly dismissing its question clears that saved answer;
a failed send keeps it available to retry.
Paste images into an answer to add numbered image references and the same
clickable thumbnails as the composer. The references stay with the saved answer.

Press Ctrl+Shift+Down to scroll the focused chat to the bottom, including while
typing. The Scroll to bottom button shows your current shortcut on desktop and web;
mobile shows the button without a keyboard shortcut. Both stop any
ongoing scroll momentum so the conversation settles at the bottom. This takes
priority over paragraph selection or adding a cursor in the composer; rebind or
clear Scroll Chat to Bottom in Settings > Hotkeys (`scrollChatToBottom`).
On mobile, choose Codex models and effort directly from the chat box dropdowns,
including before sending the first message in a new draft. Mobile does not offer
the Quick picker. Model, effort, and mode choices wait until the agent can apply
them. The connected computer needs a Ghostex version with
`ghostex select-session-chat-model <selector> --model <model> --effort <effort> --defer --json`;
the same command accepts `--mode <mode>` and `--fast-mode on|off`.
Claude's default-effort pricing notice also appears in chat with its original
explanation and choices, so you can keep the current effort or switch to the
recommended effort there without opening Terminal.
Escape interrupts the agent. If a Claude message is cancelled before it is
accepted, its text returns to the chat composer for editing. Rewind to here
also returns the selected text when the message was never accepted, keeping
anything already in the composer (`ghostex interrupt-session-chat <session>`).
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
Click the Subagents header to minimize the card to its header or expand it again.
It starts minimized in Simple mode and expanded otherwise. Your last choice is
remembered per session and takes precedence over the mode's default.
For Codex and Claude, the Subagents card and popup title show the child's latest
model and effort in compact form, such as Opus 5 High or Astra xHigh. Codex rows
show the child's name/path beside the model and effort, after a ‣ separator.
For Claude, hover to see the agent type, such as Explore or general-purpose.
Unrecorded model values are labelled Model not recorded.
Slash commands sent from chat stay in the conversation after a reload, together
with any captured output. Long command output expands when clicked; model, effort,
Fast mode, and compaction results keep their status rows.
During `/compact`, Claude, Codex, Cursor, and Grok Build show a compaction card above the input.
Cursor's `/summarize` uses the same flow. Claude shows its reported progress;
Codex, Cursor, and Grok Build show a looping bar. Messages sent or queued during compaction
wait until it finishes without a delivery warning.
To compact before sending a new prompt, press `⌥Enter` on macOS or `Alt+Enter` on Windows and Linux in the chat box,
or right-click Send and choose Compact & Send. Ghostex sends `/compact` first,
then puts your written prompt in the queue above the input to send after compaction.
In narrow chats, notice cards hide Show terminal output; Open terminal remains available.
If Codex says **Conversation open elsewhere**, choose **Continue here** or press
Cmd+Enter (Ctrl+Enter on Windows and Linux) to close the other matching Ghostex
sessions and retry here. This stops their running work but keeps conversation history.
**Go to other session**, when available, opens the existing session instead.
If the conversation is open outside Ghostex, close it in that app and choose
**Retry** with the same shortcut. Your draft stays editable and is not sent by recovery.
While Claude Code writes a reply, the chat shows the text as it appears in the
terminal, updated about once a second, and swaps in the saved message the moment
Claude records it; nothing to enable.
The live tool card at the bottom of chat shows current work until the same tool
is available in the conversation. It clears when the turn ends, you stop the
agent, or the agent asks for input. Completed searches and commands stay in
that turn's expandable work details; changing model or effort does not bring
them back. Background commands that are still running, compaction, and
requests for approval keep their own status cards.
Chat follows the app theme by default. In Settings > General > Theme, set Chat theme to Light, Dark, or System for a separate appearance, or choose Follow app to use the main App theme (`sessionChatTheme`, `sidebarTheme`).

Set Default Chat Zoom (%) in Settings > Chat to scale the desktop chat interface, including messages, controls, and the prompt composer. Choose 70% to 200% in 5% steps; the initial default is 100%. The saved level applies to open chats and when chats open again (`sessionChatZoomPercent`). With a chat focused, Cmd+= and Cmd+- (Ctrl on Windows and Linux) resize that chat for as long as it is open, and Cmd+0 returns it to the saved level.

Toggle chat and terminal for a session with one click on the pane header or
the pane hotkey. Compatible agents can default to chat. On macOS and Linux,
Ghostex can release unused terminal viewers for persistent sessions and load them
again when needed. The agent keeps running while you use Chat or another project;
returning to Terminal reconnects to the same running session. This does not sleep
the agent.
When an assistant message
has tool calls, click its text or the chevron beside it to expand the tools directly
under that message. Its full text and formatting stay visible when collapsed;
links and code controls keep their own actions. Verbose mode opens these tools
by default (`sessionChatVerboseMode`). File writes and code
edits appear outside the tool groups while the agent works. When a turn shows
"Worked for", all its file changes are grouped in a collapsed "N files changed"
section directly below it. Older history loads with completed turns already
collapsed, so you can scroll through prompts and answers without passing through
their tool logs first. Expanding older work or its files loads those details on
demand; the original history remains available. The count includes each file once, even if it was
edited repeatedly. Expand the section to see the files along an activity rail,
with added lines in green and removed lines in red. Each file defaults to one
collapsed row with its path and green/red change counts. Enable Show file edit
previews in Settings > Chat to show the first seven code lines by default.
Long paths truncate from the start, keeping the filename visible. Click anywhere
on the path or filename to open it in Editor or Docs, just like a file reference
pill. Folder links in desktop chat open the folder in your system file explorer.
File reference pills in the composer also open with one click using the same
Code/Docs preferences as transcript links. Double-click a composer pill to edit
its reference text. Right-click a file reference or file-change path for Open in
Code, Open in Docs (Markdown, HTML, and Excalidraw), Copy Path, or Open File/Folder
Location. Open File/Folder Location appears directly below the path-copy actions
in chat, image previews, Git changed files, and Docs menus, and opens
the location in the machine’s file manager. It requires a local desktop path.
Disabled Code and Docs views are omitted from the menu.
Hosts without an editor copy the path on click.
Click the card background, circle, or change counts to expand or collapse the full diff.
Only clicks directly on the path or filename open the file. The
circle's center turns white on hover. An open code preview and its left rail
also toggle the diff. After expanding or collapsing, the header stays visible;
chat scrolls to it if needed. This covers Claude's Write and Edit tools and Codex's apply_patch
changes.

Simple mode in More actions or Settings > Chat applies to every chat. Tool groups
without a message above them collapse to a tool-call count, and tool rows hide
command previews; expand a tool to inspect its full input and result. File edits
collapse under "Edited 1 file" or "Edited X files", counting each path once; expand
the row to see the usual file and diff cards. The menu and Settings use the same
toggle (`sessionChatSimpleMode`, off by default).

Summary mode has its own button between More actions and Session note when the
chat toolbar has room. In a narrow chat, find it under More actions instead.
The button highlights when Summary mode is on; its tooltip shows the shortcut.
As space gets tighter, toolbar buttons move into More actions one at a time:
Summary mode, Session note, Stash prompt, Attach, Maximize, then Terminal View.
If the context ring and effort still do not fit beside the model, they move
together into Model settings at the top of More actions. Controls return as
space opens up; More actions and Send or Stop stay visible.
Click effort or the context meter to open it; hovering does not open either control.
Hover the model or effort to see the configured Model & Effort Picker shortcut
(Option+P by default on macOS). Hover the context circle to read the agent's
terminal status line.

The Model & Effort Picker commits a choice in one of two ways. Set as default
saves the choice as the agent's default for new sessions as well as changing
this session. Use in this session changes the model and effort for this session
only and leaves the agent's saved default alone, so new sessions still start
where they did before; waking this session later brings it back on the model
you chose. Enter sets the default and Shift+Enter applies to this session only.
Turn on Session-only model picks under Settings, Agents, Config to swap them, so
Enter applies to this session only and Shift+Enter sets the default. Use in this
session is available for Claude only: Codex's own model picker always writes the
choice to its configuration file, so on a Codex session that action is greyed
out and Enter sets the default.

The model and effort dropdowns in the chat input row follow the same setting
without asking each time. Each carries an Also set as default switch at the
bottom of its menu. It starts on, so picking a model or effort there saves the
agent's default; with Session-only model picks turned on it starts off, so a
pick changes this session and leaves the saved default alone. Flip the switch
to do the other thing in one session: the menu stays open so you can set the
switch and choose in one go, and the choice is remembered for that session. On
a Codex session the switch is on and greyed out, because its picker cannot
change a model without saving it. Settings key:
`sessionChatModelPicksSessionOnly`.

A choice that cannot be applied says so at the top of the model menu, under Not
applied, with the reason. The usual reason is that the agent's own model list
does not offer that model in this session, which it cannot then change for one
session; picking another model clears the message.

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
Before sending a session's first message, use the model menu's Switch Agent CLI
to change between Claude and Codex. The new agent uses its Account for new
sessions rule, just like the sidebar agent button, while the terminal and your
unsent chat text stay in place.
Sign-in and usage-limit notices in Claude and Codex chats offer Switch account
beside Open terminal, so you can choose another account directly from those notices.
In the chat's More actions menu, click Switch Account to open its submenu;
hovering over it does not open it. Open submenus stay open when you move the
pointer across other menu items, so you can move into them without rushing.
Switching a running Claude or Codex session to another account, from More
actions > Switch Account, a terminal notice, or automatically when its account hits a usage limit,
exits the CLI inside its own terminal and resumes the same conversation there,
so the terminal tab and the chat stay open. A card in the middle of Session Chat,
over a dimmed conversation, shows the current and selected accounts, their usage
percentages (including Claude's Fable limit), and the switch progress. It stays
until the new account is confirmed and the conversation is ready on it, then
briefly confirms success, or shows a failure with Retry switch. On phones and
narrow chat panes the card uses a compact layout with one row of usage pills per
account and a short vertical step list.
A manual switch waits for your next
message without sending anything. An automatic switch sends a "." to continue
the interrupted work once the new account is ready. Configured recovery after
errors can also continue work on the same account.
Account sign-in terminals open in the active local project's folder and appear
under that project. Before a first project is chosen, sign-in uses the home folder.
When a newer usage reading cannot be fetched for a Claude account, for example
while the usage service rate limits checks for an hour at a time, Settings >
Accounts, the titlebar usage popup, and the chat's Switch account rows show the
last reading with its age ("Usage is from 3 hours ago") and Ghostex keeps
retrying on its own. Automatic switching and the Account for new sessions rule
skip that account until its usage refreshes. A login problem shows what to do
instead, such as "The saved login expired. Reconnect this account."

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
Claude Code and Codex keep separate choices. Copying settings between them
is temporarily hidden in this dialog.

Related settings: `hideAccountEmails`, `preferredAgentInterface`, `sessionChatTheme`,
`sessionChatFontFamily`, `sessionChatCustomTranscriptWidthEnabled`,
`sessionChatTranscriptWidthPercent`, `sessionChatVerboseMode`,
`sessionChatFileEditPreviews`,
`terminalViewWidthMode` (`match-chat` makes the terminal body the same width
as the chat transcript), `terminalWidthApplyToCommandPaneTerminals`.

## Terminal

On Windows, Settings > General > Terminal > Windows Environment selects native
PowerShell projects and agents or Linux projects in WSL. PowerShell is the default;
an explicitly saved WSL choice is preserved. PowerShell uses Windows folders and
installed Windows agent CLIs without requiring WSL. Switching environments opens a
restart dialog: choose **Restart now** to apply the change or **Later** to keep
working until the next app restart. Projects and running sessions remain in their
original environment. Native sessions stay alive when the app closes or gxserver
restarts. The Code view uses the same environment: native Windows folders in
PowerShell mode and Linux folders in WSL mode. The Windows app includes the native
editor.
Keys: `windowsTerminalBackend`, `windowsWslDistribution`.

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

Terminals follow the app theme by default. Settings > General > Theme groups
App theme, Chat theme, and Terminal theme together at the top of Settings.
Terminal theme can override the app with Light, Dark, or System. The palette
selectors show your existing Ghostty theme names, including separate light and
dark selections when configured. A single Ghostty theme is used for both appearances
unless you select a separate light palette. Without a configured theme, the defaults
are GitHub Light and GitHub Dark. All open terminals refresh automatically when their
app, system, or terminal theme changes, including idle terminals and terminals in
inactive projects. The appearance override and light palette apply to Ghostex only.

Terminal links (`ghostex://terminal`) without a folder open in the active local
project. A folder supplied in the link takes precedence.

Related settings: `terminalFontFamily`, `terminalFontSize`,
`terminalGhosttyTheme`, `terminalColorScheme`, `terminalGhosttyLightTheme`,
`terminalCursorStyle`, `terminalPane*PaddingPx`,
`terminalScrollbackLimitMb`, `terminalCopyOnSelect`, `promptEditorBackend`,
`terminalDevServerDetectionEnabled`.

## Agents, actions, and orchestration

Agents are the launch buttons per project: Claude Code, Codex, Gemini CLI,
OpenCode, Pi, and more are built in, and custom commands can be added in
Settings > Agents. Expand an agent row to install or update its CLI, see its
installed version and command output, or open its Install docs link. Ghostex
selects an updater for recognized installations; choose the original installation
method when it cannot be detected. mise is offered for supported CLIs and is the
default install choice when available. Existing mise tools, including custom
backends, update through mise with their version pins bumped to the latest
release; older versions remain available for running sessions. For example,
ZCode can also be installed with `mise use --global 'npm:zcode-app-cli[prerelease=true]@latest'`.
Install and update commands run on the
selected computer and keep running if Settings closes. Start a new session to use the installed version. ZCode launches with `zcode`; install
and update it with `npm install -g zcode-app-cli@latest`, as documented at
[the ZCode installation docs](https://github.com/kingsword09/zcode-cli).
Agent Hooks let gxserver watch agent status, questions, and
completions for chat and notifications. Installing the Claude Code hooks also
sets Claude Code's transcript retention (`cleanupPeriodDays`) so past
conversations stay on disk instead of being deleted after 30 days; a value you
set yourself is left unchanged. Agent approvals ("accept all") is a
per-machine default with per-project overrides. Actions (Settings > Actions)
are saved terminal commands or browser URLs shown on project headers and in
the titlebar Actions menu; Global Actions apply to every project.

Agents Hub lets you browse and edit agent files in Skills, MDs, Hooks,
Configs & MCPs, and Agent Sync. In MDs, expand Shared agent markdown to see the
files in your shared agent folder, then select a filename to read or edit it.
Expand the profile instruction groups the same way. Use Refresh to reload files
from disk and Save to write your edits.

Agent Sync (the fifth Hub tab, Cmd+5) keeps one source of truth in `~/.agents`
(skills, `main.md` and the other rule files, hook scripts, `.skill-lock.json`)
and points every agent on the computer at it. The left list shows each detected
agent and profile with three dots for Skills, Instructions, and Hooks; the right
pane shows the problems found (dangling links, copied skill folders, whole-folder
links, instruction files that do not point at `main.md`, stale lock entries) and
one agent's details when selected. Sync all or Sync <agent> opens a plan first:
one relative symlink per skill in every agent's skills folder, whole-folder links
converted to per-skill links, the one-line pointer written into each agent's
instruction file (a file with other content is backed up as
`<name>.pre-sync-<stamp>.bak` first), and the hooks folder and lock file linked
into Claude Code and Codex. Nothing is deleted; pruning stale lock entries is an
opt-in group. Agents that read `~/.agents/skills` directly (Amp, Cursor,
OpenCode) get no links. The same scan, plan, and apply run from the CLI:
`ghostex agent-sync status`, `ghostex agent-sync plan [--agent <id>]`, and
`ghostex agent-sync apply --yes [--agent <id>] [--group <group>...]
[--prune-lock]`; `ghostex agent-sync agents` lists the ids.

Use `ghostex agents --help` to create, message, and close other agent sessions.
`ghostex agents whoami --json` identifies the caller; `agents types` lists
configured agent IDs; `agents create <agent-id> --task "<task>"` starts one in
the caller's project (`--project-id` selects another). `agents list` finds
sessions, and `agents send <session-ref> "<text>"` attaches the sender's identity
and reply reference automatically. Use `--body-file` for multiline messages,
`--interrupt` for an urgent correction, or `--queue` to wait until the current
turn finishes. `agents close <session-ref>` ends that session, including any
unfinished work. `ghostex read-session-chat` and `ghostex read-text` read replies.
On older versions without `agents`, use the existing commands below.

Cross-agent orchestration also works through the `$ghostex-cli` skill. For
"make Claude Code control Codex":

1. Install the Ghostex CLI skill (Settings > Integrations, or
   `ghostex cli install-skill`). It is installed on first launch by default.
2. In a Claude Code session, ask it to use `$ghostex-cli`. It can then run
   `ghostex create-agent codex --project-id <id> --first-input-draft "<task>"`
   or `ghostex create-session --input "<prompt>" --start`, send follow-ups with
   `ghostex send-message <selector> "<text>"`, read output with
   `ghostex read-text` or `ghostex read-session-chat`, and wait with
   `ghostex wait-for-text`.
   To pick the worker's model and effort, add `--model <model> --effort <level>`
   to `create-agent` or `board start-work` (Claude and Codex). The choice
   applies to that session only, survives a resume, and leaves your default
   model unchanged. For `board start-work`, these flags apply only when a new
   worker is created; a reused linked worker keeps its existing model and effort.
   Model and effort overrides require a single agent launch command, without
   shell operators, command substitutions, comments, or line continuations.
3. The Agents Orchestration skill (`$ghostex-agents-orchestration`, installed
   from Settings > Integrations or `ghostex agents-orchestration install-skill`)
   teaches an agent to read `ghostex agents --help` and `ghostex --help`, then
   launch other agents with the model and effort you ask for, send them tasks,
   read their replies, and verify their work.

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

The sidebar All Automations page lists scheduled work across projects. The
Automate view schedules agent work per project: a name, an agent, a
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
  With Auto reconnect enabled in the phone's SSH connection settings, the phone
  checks the connection when you return to the app or its network changes and
  reconnects interrupted agent terminals. Tap a red cloud or choose Reconnect
  from the computer's menu to start a fresh connection. Easy Connect does not
  require the separate Tailscale app; the Tailscale connection option does.
  If the connection fails while saved sessions are still visible, the Sessions
  list shows a warning with the failure reason and a Retry button. The warning
  clears once the list successfully refreshes.
  Paired devices are listed and can be removed. On the phone, open Web Preview
  from the machine menu and enter a website address or a port such as `3000`
  immediately, or choose a listening port from the list. The address bar stays
  editable while browsing. The list groups Web pages, Development tools,
  Services, and Other ports into collapsible sections; services and unidentified
  ports start collapsed. Search by page title, process, or port. With a computer
  supporting `ghostex ports --json --web`, responding pages show their title,
  HTTP status, and supported favicons. Common-port labels such as Storybook on
  6006 are marked “likely” until identified by the page or process.
  Localhost links in chat, terminals, and browser
  actions open in Web Preview through the connected computer, including their
  path and query, instead of the phone's external browser.
- **From another computer**: Settings > Remote > Remote machines > Add a
  machine with SSH details or an Easy Connect code, then Install / Connect
  gxserver on it. The machine appears as a sidebar section with its own
  projects and sessions; its terminals stream into the desktop app.
- **Web app**: a static browser build of the same workspace UI that talks to
  gxserver.
- **CLI**: `ghostex attach <selector>` attaches to a session from any terminal,
  including over SSH.

Windows computers accept both Android and macOS desktop connections over SSH.
Install Ghostex on Windows, enable SSH, and add the Windows address with your
Windows username. The connection uses the Windows Environment selected in
Windows Ghostex: native PowerShell with Windows folders, or the selected WSL
distribution with Linux folders. Windows agent CLIs must be installed for native
PowerShell projects. After changing the Windows environment and restarting the
Windows app, reconnect the phone or remote desktop machine to use that environment.

Related settings: Settings > Remote (all rows are user-only; open them with
`ghostex settings open --tab remote`), `hideKeepAwakeTitlebarControl` and the
Keep Awake rows for machines that must stay reachable.

## Notifications and status

Ghostex tells you when an agent needs you: a completion sound when a session
finishes, an attention state on the session card, OS notifications on macOS,
menu bar badges with running and done counts (click one to jump to the
session), terminal bell detection, and push notifications on the mobile app.
The optional status pet in the sidebar mirrors session state.
Claude progress updates do not trigger completion notifications while Claude
reports background work still running. Completion notifications arrive when
Claude finishes after that work completes; requests for your input or permission
still get your attention.
Copy Sound is off by default. Enable it under Settings > Notifications > Sounds
to hear a short sound when copying from a terminal, a chat message, the chat
composer (including its right-click Copy menu), a copy button, or a menu
(`copySound`).

The Notifications bell sits in the titlebar right after the Next button and
shows how many notifications are unread. Click it to open the Notifications
panel: one row per session, newest first, saying whether the agent finished a
turn or needs your input, with the last thing it said. Click a row to jump to
that session and mark it read; hover a row to dismiss it; the header has Next
unread, Mark all read, and Clear all. Hover a header button to see its configured
hotkey when one is available. Hotkeys: Cmd+I opens the panel,
Cmd+Shift+U jumps to the latest unread notification, and Cmd+Ctrl+U pushes the
current session to the back of the unread queue and jumps to the next one.
Clearing a session's attention by selecting it in the sidebar, focusing its
terminal, or pressing Escape also marks its notification read, including one
you previously moved to the back of the unread queue.
Scripts and agent hooks can post their own rows with
`ghostex notify --title <text> [--body <text>]`.

Related settings: `completionSound`, `actionCompletionSound`, `copySound`,
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
  Every row on this page has an Edit (pencil) button that chooses where that
  view, title bar button, or extension appears: All projects, Selected projects
  (tick the projects), or Selected spaces (tick the spaces). Worktrees follow
  their parent project, and a project inside a group follows the group. A row
  narrowed this way shows its scope under its description, and the view or
  button is simply absent while you work in a project it does not cover, so its
  hotkeys and command palette entries go away with it. Custom views under Your
  views use the same Available in picker inside their own editor.
  Its Titlebar account usage section lets you star saved Claude and Codex
  accounts to show their usage in the desktop titlebar, or unstar them to hide
  it. These are the same per-account stars available in Settings > Accounts.
  Claude buttons show the two tightest of the weekly, five-hour, and Fable
  limits, so the Fable limit is never hidden when it is running out; launcher
  and picker rows and the Accounts figures use the same two numbers.   Each
  button opens that login's live limits, reset times, and extra usage or rate
  limit resets, with the Fable limit as a main bar for Claude. Right-click a
  usage button for Extensions and Accounts. Click the same
  usage button again to close its dropdown. Click another titlebar dropdown's
  button to close the current dropdown and open that one in a single click.
  Clicking outside, including in
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
- Welcome to Ghostex is the onboarding that opens the first time Ghostex
  runs. Its five panels cover: the agents found on this computer, with
  Install buttons for Claude Code, Codex and Cursor Agent, an Install guide
  that installs any other supported agent, the Ghostex helper (agent hooks)
  and Computer Use; which views to show (Browser and Docs are on by default
  on a first run) and the browser skill; phone pairing and notifications;
  and the first project folder with the default agent and session view.
  "I already know Ghostex" on the first panel skips the rest. Reopen it any
  time from Tips > Setup or Quick Access > Commands > Setup.

## Appearance and app

Theme, background contrast and tint, accent color, active pane outline, and
the app icon live under Settings > General > Theme, the first section.
App theme offers Dark Gray, Light, and System. Chat and terminal default to
Follow app, with optional Light, Dark, or System overrides in the same section.
System is the app default and follows the operating system appearance. Existing
saved app themes are preserved; dark contrast and tint return unchanged when switching back from Light.
In light mode, the sidebar and titlebar have solid light-gray (#f4f4f5) backgrounds. Enable Show
Advanced to find Dark theme background contrast, Dark theme background tint, and
Dark theme accent color; these controls do not recolor light-mode chrome.
Keep Awake (Power)
prevents sleep while agents work.
Advanced holds Enable Experimental Features. The separate Debugging page sits
above About and starts with Show debug UI controls. Enable that switch to see
diagnostic logging scenarios, session context-menu debugging controls, Storage
usage, and Ghostex folder storage. Storage usage lists browser space by feature
and can clear disposable caches; unsaved drafts and pending work are protected.
Ghostex folder storage shows on-disk folder sizes with Refresh and Open Folder.
Both storage panels load only while debug controls are enabled on that page.
Open the page with `ghostex settings open --tab debugging`, or the inspector
with `ghostex settings open storageUsage` after enabling debug controls.
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
- "How do I annotate a Browser page or Markdown file": Browser pages use
  Agentation in the Browser toolbar (see Views, Browser). Markdown files use
  Docs: select text to comment or mark Looks good, Clarify, or Needs tests,
  then Send to the last-clicked session (see Views, Docs).
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
