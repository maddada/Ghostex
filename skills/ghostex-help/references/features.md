# Ghostex features

Hand-written companion to `settings.md` (generated) and `hotkeys.md`
(generated). When you add or rename a view, a sidebar surface, a
session capability, or a CLI verb that users interact with, update the matching
section here in the same change.

Each section ends with "Related settings" so the helper can turn an
explanation into a change with `ghostex settings set`.

Context menus share the sidebar's rounded appearance and follow the current
light or dark theme. Click a submenu to open it, and click the same row again to
close it; it stays open as you move the pointer across other rows. Long menus
scroll vertically to keep every action reachable, without a horizontal
scrollbar.

## Views (tabs in the view panel)

Views open beside your sessions, in a panel with its own tab strip, and a
project can keep several of them open at once. The strip is the top row of the
panel and shares that row with the work area header, so the tabs sit over the
view and the header's breadcrumb and buttons over your sessions. Expand a view
over the Agents Panel and the strip moves to its own row under the header. The tabs belong to the project,
so switching sessions leaves them alone and coming back to a project brings the
same tabs back. Option/Alt+1 through 9 jump to the tabs in the order they appear
in the strip; direct built-in view shortcuts can be assigned in Settings >
Hotkeys, and hovering a tab shows its shortcut. Views other than Agents are
extensions: they load on demand, sleep when idle (Auto Sleep), and can be hidden
or reordered in Settings > Extensions.

The **+** button right after the last tab opens another view. Its top row,
**Browser Tab**, always opens a new browser tab. Below it is every other view:
clicking one opens it, or switches to its tab if it is already open. Then comes
**Hidden here**, which lists the views you hid in this project and brings one
back in a click, then **Customize**. Close a tab with its **x**, with a middle
click, or from its menu. Browser has no view tab of its own: the same row
carries its page tabs, one per open page, so a page you have open reads as a tab
beside the views. The row is one order: drag any tab, a view or a page, anywhere
in it, and it scrolls sideways once the tabs run out of room. Right-click a tab
and choose **Pin tab** to shrink it to its icon and keep it at the left of the
row; a pinned tab has no **x** and ignores a middle click, so it closes only from
its menu, and **Unpin tab** puts it back.
Only the view you are looking at, and the two you came from most recently, stay
loaded; the rest keep their tab and wake when you click them.

The buttons at the far end of the strip **pop the view out** into its own
window, for a second monitor, and **expand** it over the Agents Panel so it
has the whole work area (Cmd+Ctrl+E, `expandViewPanel`). The **+** joined to the
expand button **expands it fully**, hiding the sidebar as well
(Cmd+Ctrl+Shift+E, `expandViewPanelFully`). The same buttons bring everything
back.

The **view panel toggle** in the work area header (Cmd+Option+B,
`toggleViewPanel`) opens and closes the whole panel; opening it comes back to
the view this project last had open, and shows **Open a view** when the project
has no tabs yet. Closing it leaves your sessions at full width. The header itself
carries the project breadcrumb, Start, Open and Commit, the **⋯** button (Ask
Ghostex, Tips & Tricks, Resources, Dev servers, Extensions and Customize), and the
command terminal toggle. **Hide sidebar** and the chat-icon **Toggle Agents Panel**
button beside it (it hides or shows the Agents Panel while the side panel is open,
even on the "Open a view" picker, the same thing Expand side panel does, and leaves the
sidebar alone) sit at the top left of the
sidebar, and move to the start of the header while the sidebar is hidden, so they
stay in the same spot. The sidebar's top row also moves the window when you drag
it. When an update is available, a download button
appears just before the project name. On Linux the button opens the release
notes with an **Open download page** button instead of installing the update;
install the new package the same way you installed Ghostex.

**Open a view** is the picker the panel shows when nothing is open in it. It
lists every view you can open here: the built-in views first, then your own views
and extensions. Press a view's letter to open it (C for Code, B for Browser, K for
Kanban, U for Automate, D for Docs, T for Terminal) while the picker is in front. Views you hid
for this project are not in the list; **Manage views and where they appear…** at
the bottom opens their settings, and **Hidden here** on the `+` menu brings one
back.

Right-click a view tab to choose where that view appears and what happens to it.
**Reload** refreshes the clicked view, **Sleep** unloads it while keeping its tab (Code also stops its editor
server; choose **Wake** or click the tab to bring it back, and Resources can stop
Code too without closing Ghostex), and **Open externally** opens its page in its
own window. Lower down, **Show in this Project** and **Show in this Space** are
ticks: unticking one hides the view there and leaves it everywhere else (the
space row is absent when the project is not in a space), and **Choose where
it's shown…** opens that view's full scope editor in Settings > Extensions.
**Close tab** removes it from the strip, and **Hidden here** is on
this menu as well. Custom project views also offer **Command output** and
**Configure view**, which opens that view's editor in Settings > Extensions and
focuses its name field.

- **Agents**: the terminal grid. Panes run agent CLIs or plain shells, split
  horizontally or vertically. There is no tab bar above a pane: the sidebar is
  the list of sessions, selecting one shows it in the focused pane in place of
  the session that was there, and a session already on screen in another pane
  is focused there instead. To split, drag a session row from the sidebar onto
  the left, right, top or bottom edge of a pane (terminal or Session Chat); drop
  it in the middle of a pane to show it there instead. You can also
  use Advanced > Split Right in the session's menu. While the screen is split,
  the focused pane has a small bar along its top: click it for Close Pane (the
  sessions keep running) and Merge All Panes, or drag it to move that session
  onto another pane's edge (a new split) or its middle (it takes that pane's
  place), and the pane it left closes. Each pane can show the raw terminal or
  Session Chat. Cmd+Shift+O starts a new session with the agent you used last,
  in your default view (Chat or Terminal), like ChatGPT's New Chat. Cmd+N opens
  the New Thread picker instead: type to filter the configured agents (last used
  first), Browser, or Terminal, press Enter to start it in the active project,
  and press Tab on Claude or Codex to pick an account. Cmd+Shift+T opens a new
  terminal, Cmd+T always opens a new browser tab, Cmd+Ctrl+Shift+F forks the
  focused session, and Cmd+D splits. Cmd+R renames the focused session,
  Cmd+Shift+A (or Option+Shift+S) sleeps it, and Cmd+Shift+Backspace (or Cmd+W)
  closes it. On Windows and Linux use Ctrl for Cmd, except that fork is
  Ctrl+Alt+Shift+F and rename is Ctrl+Shift+R, because Ctrl+R belongs to the
  terminal. While the Code editor itself has keyboard focus, Cmd+N and
  Cmd+Shift+O go to VS Code instead (New File, Go to Symbol).
  A new chat that you leave without typing anything closes on its own, so empty
  sessions do not pile up in the sidebar, and pressing Cmd+Shift+O again while
  one is open takes you back to it. Once you type or send something it stays
  like any other session.
  Cmd+Option+Arrow moves focus between the session panes and the Commands pane;
  it skips the view panel.
  Hotkeys: `createAgentSession`, `openNewThreadPalette`, `createSession`,
  `openBrowserPane`, `forkSession`, `renameActiveSession`, `sleepFocusedSession`,
  `closeFocusedSession`.
- **Code**: the built-in VS Code based editor (code-server). Opens files from
  chat links, `ghostex edit <file>`, and Open In. Optional Use VS Code settings
  reuses the local VS Code configuration.
- **Browser**: embedded Chromium tabs with profiles, splits, annotations,
  DevTools, and agent control through the `$ghostex-embedded-browser-use`
  skill. Its tabs sit in the view panel's tab strip, in the same row as the view
  tabs, and stay there while you are in another view, so clicking one comes
  back to Browser with that tab in front. Each tab shows its page icon and
  title, closes with its own x or a middle click, and drags anywhere along the
  strip to reorder. Right-click a tab for Select Tab, Pin Tab, Sleep (that tab),
  Sleep Browser (every browser tab), and Close Tab. Closing the last browser tab closes the
  Browser the way closing any view does: the panel moves to the next open view,
  and shows the Open a view picker when Browser was the last one. **Browser Tab** at the top of the strip's **+** menu opens
  another tab. A tab with no address yet is blank: type or paste an address, or
  pick a running server from the ⋯ menu's Dev servers panel.
  Web links from terminals, chat, and detected dev servers
  open here or in the system browser depending on Open links in. Annotate the current page
  with Agentation in the Browser toolbar; GitHub pages disallow that tool.
  When a page shows its content inside a frame, such as a Storybook story,
  the Annotate toolbar opens inside that frame so the content itself can be
  selected. Copying or sending annotations clears them afterwards by default;
  the toolbar's own settings panel (Clear on copy/send) turns that off.
  HTML files in Docs use the same Agentation overlay via Annotate. Markdown
  files use Docs selection comments instead (see Docs below).
- **Linear and Jira**: open either view from the **+** menu. Paste the workspace,
  project, team, or board URL you want as its home. Ghostex identifies the workspace
  from the address and preserves the full URL, including filters. Previously used
  workspaces are offered when setting up another project. Worktrees follow their
  parent project's home until you choose a different home for that worktree.
  Right-click the Linear or Jira view tab and choose **Modify home URL for…** to
  reopen setup; choose **Follow** to restore the parent home. Middle-click or
  Cmd-click (Ctrl-click on Windows/Linux) a link to open it in a new background
  Browser tab. Settings > Extensions controls visibility (`linearViewTabHidden`,
  `jiraViewTabHidden`).
- **GitHub**: automatically opens the GitHub repository from the project's origin
  remote, including in worktrees. No URL setup is needed. It appears when a GitHub
  repository is available and supports opening links in new Browser tabs just like
  Linear and Jira. Settings > Extensions controls visibility (`githubViewTabHidden`).
- **Sentry, Figma, Vercel, Supabase, GitHub Actions, PostHog, and Custom Website**:
  disabled by default. Enable the views you want in **Settings > Extensions**, then
  open one from the **+** menu and paste its home URL, just like Linear. Use any
  project page, design, dashboard, workflow, or filtered view; Custom Website can
  open any HTTP or HTTPS website. The saved home opens automatically for that
  project. Worktrees follow their parent unless you choose a different home, and
  previously used sites are offered when setting up another project. Right-click
  the view tab and choose **Modify home URL for…** to change it. Middle-click or
  Cmd-click (Ctrl-click on Windows/Linux) links to open background Browser tabs.
  GitHub Actions has its own chosen URL, separate from the automatic GitHub view.
  Visibility settings: `sentryViewTabHidden`, `figmaViewTabHidden`,
  `vercelViewTabHidden`, `supabaseViewTabHidden`, `githubActionsViewTabHidden`,
  `posthogViewTabHidden`, `customWebsiteViewTabHidden`. Homes: `projectWebsiteViews`.
- **Storybook**: the built-in component workshop appears in the view picker and
  **+** menu only when the project has Storybook. Press S in the picker to open it.
  Ghostex runs the project’s `build:storybook`, `build-storybook`, or
  `storybook:build` script, then serves the generated pages without keeping a Node
  server running. Install the project’s dependencies first. **Rebuild Storybook**
  refreshes the workshop after source changes; this static view has no live reload.
  **Annotate with Agentation** selects elements inside the story preview, just as
  in Browser. Build failures offer Retry and Command output. For a workspace with
  several Storybooks, open the desired package as a project or provide a root build
  script. Settings > Extensions controls visibility (`storybookViewTabHidden`).
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
- **Terminal**: a command terminal in the view panel that works like the
  Commands pane, only on the right beside your sessions instead of below them.
  It has its own tab bar with a **+** for new terminals, Cmd+Shift+T for another
  tab and Cmd+D for a split while it has focus, drag to regroup or split, right-click
  a tab for Sleep and Close scopes, and Actions can run in it. A project has one
  Terminal view; opening it creates its first Command Terminal, closing its last
  tab closes the view, and its terminals keep running and come back when you
  reopen it. It does not replace the Commands pane: Cmd+J (Mac), F12 and the header's command
  terminal toggle still open that pane, and both can be open at once.

Related settings: `terminalViewWidthMode`, `webLinkOpenTarget`,
`markdownFileOpenView`, `htmlFileOpenView`, the Auto Sleep rows
(`autoSleep*IdleMinutes`), and Settings > Extensions.

## Sidebar

The sidebar lists projects and their sessions. Project headers carry the git
branch and diff stats, an agent launcher, Add Worktree, and project actions.
Right-click a project for Open Folder in the file manager or Add to Group.
A project's Sleep, Wake, Sleep Inactive and Close Inactive act on that
project's sessions only; its browser tabs are slept and closed from the tab
strip above the view, where they live.
Click a project header (or the chevron beside it) or a group header to expand
or collapse it; rename a group from its right-click menu.
Close Project parks the project in Recent Projects (a remote machine's project
goes to that machine's Recent Projects, which Quick Access lists); when it held
the active session, Ghostex stays in the current Space and switches to an awake
session of the next project in the list.
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
In Add Project, select the computer whose folders you want to browse. External
drives and other folders shows that computer's filesystem root, or its drives
on native Windows. You can paste a Windows drive or UNC path when the selected
computer runs native Windows, even from a Linux or macOS client.
Right-click a Space icon for its menu: Manage (Edit Space, New Space) and
Sleep. Sleep Inactive sleeps only the Space's sessions that are awake but
neither working nor waiting on you. Sessions stay where they are, asleep, and
wake when you open them; the row is greyed out when it has nothing to sleep.
The Other button offers the same rows for projects that are in no Space.
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
Switching projects by any route keeps the project's last view, its own width for the
split between sessions and the view, and whether the view panel was open.
Clicking a session opens it in the Agents Panel, which is always on screen, so it
never closes the view you are looking at.
Leaving a project (by switching Spaces or projects) does not close what you had
open there: the terminals, chats, and view page that were on screen stay running
in the background for the "Keep the previous project live for" number of minutes
(`projectSwitchKeepAliveMinutes`, default 10, 0 to 60), so switching back is
instant. Set it to 0 to release them as soon as you leave.
Starting a new agent from the sidebar launcher or New Thread picker keeps your
current view open, including Code, Browser, Kanban, Automate, and Docs. Select
Agents when you want to open the new agent there.
Opening a view puts it beside your sessions rather than over them: the whole grid of
terminal panes and chats stays on the left, the view takes the right half, and a
divider separates them. Drag the divider to change the balance and double-click it to
put it back; each project remembers its own. Your agents keep running and stay exactly
where they are while you open, change and close views.

- Width: the sidebar sits on the left; drag the divider to resize,
  double-click it to restore `sidebarDefaultWidthPx`. Cmd+B collapses it.
- Reveal active session: the hollow-circle header button expands its section and scrolls
  the active session into view with 50px of space from the top or bottom edge
  (below any pinned headers, where scrolling allows), then blinks its outline
  twice: pale blue in light mode and white in dark mode. Active sessions also have a slightly
  stronger background and border in light mode.
- Pane memory: each project remembers whether its Commands pane is open or minimized,
  and opening, closing or switching a view never changes it. The sidebar keeps one
  state everywhere by default; "Sidebar visibility memory" (`sidebarVisibilityMemory`,
  Advanced) can instead remember it once for a window with no view open and once for
  a window with one open, and then a project switch that hides it leaves it
  floating while you hover it. While collapsed, hovering the window's left 10px (an
  invisible hover zone over whatever is there) floats the sidebar back over your work, and it slides away when you
  move off it; the slide runs at the sidebar's Collapse animation speed
  (`sidebarCollapseAnimationDurationMs`, 0 for no animation), whatever the
  system's Reduce Motion setting says. If you have also expanded a view to fill the window, that edge
  splits in two: hovering the top half floats the sidebar and the bottom half
  floats the Agents Panel, so you can glance at your agents without
  leaving the view. With the sidebar open and a view expanded, hover the sidebar's
  left edge, click a session, or start a new agent, and the Agents Panel floats
  to the right of the sidebar. It is always 520px wide and takes your typing right
  away. This works on macOS, Windows and Linux.
- Panel animations: showing or hiding the sidebar, the side panel, the Agents Panel
  and the bottom or right command pane slides it open or closed; the side panel and a
  right-docked pane slide in from the window's right edge, and the command pane's
  terminals fade in as it finishes opening. "Panel animations" sets the speed: Off,
  Slow, Normal (the default) or Fast. With Reduce Motion turned on in your computer's
  settings, panels always open and close instantly (`panelAnimationSpeed`).
- Pane width: agent panes have a minimum resize width of 388px, and so does the
  Agents Panel when a view is open beside it. In the desktop app, an open Code,
  Browser, Kanban, Automate, or Docs view has a minimum width of 455px.
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
- Project icons: Ghostex finds favicons in the project and nested app folders.
  Projects without artwork show a square with the first letter of their name.
  Toggle them in Settings > General > Sidebar (`showProjectIcons`).
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
  Both rows live under General > Session Cards and need Show Advanced. Settings:
  `sessionCardHoverButtons` (a list of `{ id, enabled }` with ids `rename`,
  `pin`, `note`, `snooze`, `closeAfterDone`, `tag`, `park`, `sleep`,
  `close`, `chevron`) and `showSessionCardHoverButtonsInContextMenu`.
- Long projects: a project with more sessions than Compact Session Rows (13
  by default, up to 50) starts in Compact mode and shows only that many rows
  plus a "Show all N sessions" row. Rows inside a collapsed Pinned, Drafts,
  Parked, or Snoozed section do not count. Click that row, or the chevron on
  the project header, to switch the project to Full mode, which shows every
  row; the chevron switches it back to Compact. Each project remembers its
  mode. The sidebar is the only scroller, and a project's header stays pinned
  at the top while you scroll through its rows. Setting:
  `projectSessionListCollapsedCount`.
- Drag a session onto another section of its project (its heading or any row
  in it) to move it there: Pinned pins it, Sessions unpins and unparks it, and
  Parked parks it. While you drag, an empty Pinned, Sessions or Parked section
  shows its heading so you can drop onto it.
- Sidebar section headings (Pinned, Sessions, Drafts, Parked, and
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
when Command Pane Side is set to Right. Open it with Cmd+J on Mac or F12 anywhere. The **Terminal** view
(see Views) is the same kind of terminal opened as a tab of the view panel
instead; it has its own tabs and does not affect the Commands pane. Auto-minimize Commands
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
Middle-click a tab to close it, or middle-click an empty spot in the tab bar to
close all of that bar's terminals at once.
Related settings: `commandsPanelAutoMinimize`,
`commandsPanelAutoMinimizeDelaySeconds`, `commandsPanelSide`,
`commandsPanelDefaultHeightPx`.

## Sessions

A session is one terminal pane. Sessions persist across app restarts (zmx keeps
the process alive) and are restored with the agent's resume command. From the
sidebar or `ghostex`, a session can be focused, renamed, pinned, tagged,
slept and woken (`ghostex sleep|wake <selector>`), forked, closed, or moved
between panes and groups. Selecting a session shows it in the focused pane;
dragging its row onto a pane's edge opens it in a new split beside that pane. A session carries one tag at a time, chosen from the
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
Fork (a session's right-click menu, or More actions in its chat) opens the new
session beside the original and switches to it. It starts as
`Fork: <original name>` and saves that name through the agent's own rename
command so it survives reopening the conversation.
Once a conversation has forks, a small branch button in the chat's top right
lists every session that shares the earlier history, including the thread you
forked away from, and switches to the one you pick; a stopped branch is resumed
when you open it.

- Sleeping frees RAM; Auto Sleep does it after idle minutes. Auto Sleep runs
  on the computer that hosts the sessions, so it keeps working while the app
  window is closed, and it never sleeps a session a Ghostex window or the phone
  app is showing. Resources in the
  header's ⋯ menu sleeps many at once and shows CPU and RAM per session. Clean RAM
  copies a diagnosis prompt; paste it into an agent session to reduce RAM use.
  Sleeping sidebar sessions keep their normal title color and show a dimmer
  last-active time on the right; awake sessions show a stronger timestamp. Use `ghostex sleep|wake <selector>` to
  sleep or wake a session.
- A sleeping session wakes when you ask for it. Clicking its row in the
  sidebar or Split Right wakes it. Selecting its tab, opening its project, or
  coming back to a project after restarting Ghostex shows a small bar with
  its name and a Resume button instead; click anywhere in the pane or press a key to
  wake the session. With
  Click to Wake Sleeping Panes turned off, those wake right away
  (`clickToWakeSleepingSessions`).
- Drag pinned sessions to reorder them within their project. Rows stay in place
  while an icon-and-title ghost follows the pointer; the insertion line marks
  where the session moves when you drop it.
- Recent Sessions (Cmd+P) opens Quick Access to jump between sessions, and
  Cmd+Option+Shift+O opens it on recent projects.
  Its four tabs are Commands, Projects, Sessions, and Saved Prompts; they sit
  at the bottom left of the window and Cmd+1 through Cmd+4 switch between
  them. Filters (Saved/Recovered/Sent, All/Closed/External, project, tags) are
  dropdowns at the right of the search line. Enter runs the selected row;
  Cmd+K, the Actions button at the bottom right, or a right-click lists
  everything else the row can do (star, tag, copy, edit, delete, remove, New
  Prompt) with each action's hotkey. Cmd+Shift+P opens Commands directly to
  search app commands, pane actions, and project actions. The Commands row at
  the bottom of the sidebar opens the same thing, and the gear beside it opens
  Settings in one click.
  Previous Sessions in More Options lists past conversations from every agent
  CLI with resume and fork. The History icon immediately to the
  right of Add Worktree on a project header opens Quick Access > Sessions with
  that project selected and Closed active, ready to search sessions you closed.
  Quick Access also discovers existing Claude, Codex, and ZCode conversations
  from outside Ghostex. Opening an imported ZCode conversation resumes it in
  a terminal with `zcode --resume <session-id>`; install the ZCode CLI on that
  computer first. Deleted, archived, running, and subagent ZCode conversations
  are excluded from discovery.
- Search by Prompt (More Options, Actions (Cmd+K) in Quick Access > Sessions,
  the `openFindPrompts` hotkey, default
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

Session Chat renders the same agent session as a chat GUI: composer with
image paste and Ctrl+G rich prompt editor, a prompt queue that sends when the
agent stops, transcript with thinking, tool, and edit cards, subagent
transcripts, question and approval cards, rewind, and a note per session.
Use the paperclip to attach images, files, or folders. On Linux, choose
**Images or files…** or **Folders…** before selecting items in the system picker;
the terminal's attachment action offers the same choices.
Hover a message to show its actions and the time it was sent in a row below
it: Copy message, Reply by Annotating, and Save to md under an agent's final
reply; Rewind to here, Save prompt, and Copy message under your own messages.
Hover the time to see the full date. While a chat has focus, Shift+Esc moves
the keyboard to its chat box, Cmd+Shift+; copies the last code block an agent
wrote, and Cmd+Shift+C copies the agent's last reply (Mac only; on Windows and
Linux Ctrl+Shift+C stays terminal copy).
The chat box edits like VS Code: Up on the first line jumps to the start and
Down on the last line to the end, Option+Up/Down moves the current line,
Option+Shift+Up/Down duplicates it, Cmd+Shift+K deletes it, Cmd+L selects it,
and with nothing selected Cmd+X cuts the whole line and Cmd+C copies it (Alt and
Ctrl on Windows and Linux).
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
Settings > Chat > Keep chat box expanded while scrolling leaves the desktop
chat box at full size instead (`sessionChatKeepComposerExpanded`, off by default).
In a short pane, such as one half of a stacked split, the chat box stays
collapsed even at the bottom of the conversation until you click it, and
collapses again when you click elsewhere.
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
box; hover the status line and click the pen after its last item to open
Context details. Claude
Code starts with Account, Model limit, 5h limit, 7d limit, and Repository
starred; Codex starts with Account email, 7d limit, 7d reset, and Account
resets; Cursor starts with Context used, Branch, and Lines changed. Reset to
recommended returns to these. The status line and More details are available
for Claude Code, Codex, and Cursor chats.
Items without a value are hidden until their data is available again;
your starred selections stay saved. Wrapped rows are centered and balanced where
space allows, with separators only between items on the same row.

Codex can ask questions while it keeps working. These appear above the composer,
so you can keep writing your next message. Choose a suggested answer or write
your own, then press Enter or Send answer; Shift+Enter adds a new line, and
selecting an option alone sends nothing. An orange dot followed by a pink dot on
the session, in the sidebar and in the phone's session list, means the agent is
working and has an unanswered question. The dot
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
including before sending the first message in a new draft. Model, effort, and mode choices wait until the agent can apply
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
Option-click Send (Alt-click on Windows and Linux), or right-click Send and choose Compact & Send. Ghostex sends `/compact` first,
then puts your written prompt in the queue above the input to send after compaction.
In narrow chats, notice cards hide Show terminal output; Open terminal remains available.
When an agent asks whether to trust the project folder (Claude Code, Codex,
Cursor, Grok Build, Pi, and Antigravity all do on a new folder), chat shows a
folder-trust card with the agent's own Yes/No choices. Its **Trust and Remember**
button trusts the folder now and remembers it: from then on Ghostex answers the
trust prompt for that project on its own, whichever agent asks, without showing
a card. Worktrees count as part of their parent project, wherever they live on
disk. The remembered folders are kept per computer in `workspace-trust.json`
inside Ghostex's gxserver state folder; delete a folder's entry there to be asked
again. One exception: older Claude Code builds' "Do you trust the files in this
folder?" screen has no safe automatic answer, so that screen still shows the card
and needs a click in the terminal even for a remembered folder.
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
Chat follows the app theme by default. In Settings > Theme > Advanced, set Chat theme to Light, Dark, or System for a separate appearance, or choose Follow app to use the main Appearance (`sessionChatTheme`, `sidebarTheme`).

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
edits appear outside the tool groups while the agent works. As soon as the agent
finishes a turn, its tool work folds away under "Worked for Xs" above the final
reply; click that line to open or close it. When a turn shows
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
pill. Local folder links in desktop chat open your system file explorer; remote
folder links open in this computer's Code view so you can browse the remote files.
File reference pills in the composer also open with one click using the same
Code/Docs preferences as transcript links. Double-click a composer pill to edit
its reference text. Right-click a file reference or file-change path for Open in
Code, Open in Docs (Markdown, HTML, and Excalidraw), Copy Path, or the location row.
In chat the location row names what the path is (Open File Location, Open Folder
Location, Open Image Location, Open Video Location); elsewhere it reads Open File/Folder
Location. It appears directly below the path-copy actions
in chat, Git changed files, and Docs menus, and opens
the location in the machine’s file manager. It requires a local desktop path.
Videos, audio files, and PDFs added to a chat are labelled Video #1, Audio #1, or PDF #1,
and clicking one (or choosing Open Video from its menu) opens it in the system's default
app on macOS, Windows, and Linux instead of the code editor.
Right-click an opened chat image preview to close it. Click the picture itself to step
through three zoom levels, the last one showing it pixel for pixel, and once more to return
it to the fitted size; the cursor shows whether the next click still zooms.
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
If the context ring still does not fit beside the model, it moves into Model
settings at the top of More actions; the model pill shortens instead of moving.
Controls return as space opens up; More actions and Send or Stop stay visible.
Click the model pill or the context meter to open it; hovering does not open either control.
Hover the model pill to see the configured Model & Effort Picker shortcut
(Option+P by default on macOS). Hover the context circle to read the agent's
terminal status line.

The chat input row has one model pill. It shows the agent's logo, the model, and
after it the reasoning level and the context window, for example
"Fable 5.1 High · 200K". Click it to open the model picker: a row of agent tabs
with a starred Favorites tab first, the models of the chosen tab (starred ones
first, in their usual order),
and along the bottom a button each for the reasoning level (brain), the context
window (chart bars) and Fast mode (bolt). Clicking the context window or Fast
mode button switches it; the reasoning button opens a short list to the side. A
button the current model has no choice for is dimmed: reasoning and context
window read Default, and Fast mode reads Off. Grok Build's fast model (Grok 4.7
Fast) is not its own row: pick Grok 4.7 and switch Fast mode on or off. Auto, where an agent offers it,
always sits at the top of that agent's list. Click a row's star to keep that
model on the Favorites tab; hover the info icon that appears on a row to read
what that model is for. In a session that has started, another agent's tab
shows a small handoff badge: picking one of its models hands the conversation
off to that agent instead of changing this session's model. With more than one signed-in Claude or Codex account, an Account
button beside Fast mode shows the account in use and opens the list to switch.

The Model & Effort Picker shortcut (Option+P by default on macOS) opens the same
picker from the keyboard, and pressing it again closes it. Up and Down move through the models and then the bottom buttons, stopping
at the top and bottom; Left and
Right change the highlighted model's reasoning level, which the reasoning button
shows; the letter on each bottom button's icon uses it (R Reasoning, C Context,
F Fast mode, A Account); Tab and Shift+Tab move through the Favorites and agent tabs. Enter uses the highlighted model and level in
this session and closes the picker, Shift+Enter saves them as the agent's default,
and Cmd+1 to Cmd+9
jump the highlight to one of the first nine rows without applying it. Escape closes it
without changing anything. The key reminder along the bottom lists these.

Clicking a model or a reasoning level applies it to this session and saves it as
the agent's default for new sessions. Right-clicking applies it to this session
only and leaves the saved default alone, so new sessions still start where they
did before; waking the session later brings it back on the model you chose.
Session-only picks work for Claude only: other agents' own model pickers always
save the choice as the default.

On the phone, tapping the model pill opens the same picker as a sheet, without
keyboard shortcuts. Tap a model to highlight it; its reasoning levels appear under
it, and tapping one sets the level. Then tap Use in this session, or Save as
default to also make it the agent's default for new sessions (agents other than
Claude show a single Apply button, since their pickers always save the default).
Tap the info icon on the highlighted model to read what it is for. The bottom
buttons work as on the computer; long-press one (Claude only) to apply the change
to this session alone. In a session that has started, the phone's picker shows
only that agent's models.

Picking a model from another agent's tab does not change the running session,
which cannot switch agents. It opens Handoff / Export on Handoff to an agent with
that agent already selected; confirm it and the new session starts on the model
and reasoning level you picked (Claude and Codex; for other agents choose the
model in the new session). You can still choose a different agent in the dialog.

In terminal view, an agent terminal's bottom bar shows the same model pill after
the session id; click it or press Option+P to open the same picker there, and
its choices apply to the terminal session the same way. Turn this off with Model
picker in terminal view (`showQuickModelPickerInTerminal`) to leave the shortcut
to the terminal.

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
A chat draft you type on one device (your computer or the Ghostex phone app)
shows up in the same chat on your other devices as "Another saved draft is
available". Choose Use to load it into your input, or Dismiss to keep what you
have; hover over or click its preview icon to read the full text above the icon
first.

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
so the terminal tab and the chat stay open. Switching dismisses open CLI dialogs
and interrupts the current work until the agent exits, then resumes your saved
conversation on the selected account. For a Claude background session,
switching stops that conversation and its remaining jobs before resuming its
saved history on the selected account. A card in the middle of Session Chat,
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
Whether a session keeps going at a limit comes from Continue automatically and
When the account runs out under the provider's New session defaults in Settings >
Accounts. Every Claude and Codex session follows those settings as they are now,
including sessions that are already open, forks, and restored sessions, so a
change there applies everywhere at once. To make one session behave differently,
open More actions > Switch Account and click Customize under Keep going at a
limit; Use session defaults returns it to the Settings values.
Account sign-in terminals open in the active local project's folder and appear
under that project. Before a first project is chosen, sign-in uses the home folder.
When a newer usage reading cannot be fetched for a Claude account, for example
while the usage service rate limits checks for an hour at a time, Settings >
Accounts, the account usage popup, and the chat's Switch account rows show the
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
errors and recovery messages in Settings, launchers, and the account usage popup.

The context meter above the chat box opens a popover whose More details rows
are grouped under Usage & cost, Context & cache, and Session. Its pen icon
opens the Context details dialog: the filter bar at the top finds a row by its
title, description, or current value; switch rows on or off, drag them to
reorder within their group, and star a row to show its value in the status
line under the chat box. Every row holds one value, for example Cost, Session
time, and API time, or 5h limit, 7d limit, Model limit (such as Fable), 5h
reset, and 7d reset, read from the session's saved account, or from the agent
itself when the session has no account.
Claude Code, Codex, and Cursor keep separate choices. Copying settings between them
is temporarily hidden in this dialog.

Related settings: `hideAccountEmails`, `preferredAgentInterface`, `sessionChatTheme`,
`sessionChatFontFamily`, `sessionChatCustomTranscriptWidthEnabled`,
`sessionChatTranscriptWidthPercent`, `sessionChatVerboseMode`,
`sessionChatFileEditPreviews`,
`sessionChatKeepComposerExpanded`,
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
Cmd+V pastes images as previewable links. On Windows and Linux, Ctrl+V or
Ctrl+Shift+V pastes from the client computer's clipboard into the focused
terminal, including a remote terminal. A configured hotkey using the same chord
takes precedence. In an agent's terminal prompt, use
Prompt Editor in the terminal toolbar, Ctrl+G on macOS, or Ctrl+Shift+G on
Windows and Linux to open the Ghostex prompt editor or your machine default
editor for long prompts. Remote sessions open it in that computer's Code view.
The agent must be at its prompt and support an external editor; an idle shell
does not provide that agent prompt-editor action. The Ghostex editor uses the
same text editing controls as the chat composer, with F1 commands, find/replace,
undo/redo, and image previews. Cmd+S/Ctrl+S or Ctrl+G saves and closes it; Cancel
leaves the original prompt unchanged. In remote Code, auto-save is off for prompt
files. Save your edits, then close the prompt file to return to the agent. To
cancel, close it without saving and choose Don't Save if asked. Dev Servers detects
localhost URLs from output and lists them in the ⋯ menu's Dev servers panel.

Terminals follow the app theme by default. The Theme page in Settings holds
Appearance, and its Advanced part holds Chat theme and Terminal theme.
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
set yourself is left unchanged. Installing the Claude Code or Cursor hooks also
registers a Ghostex status line command for that agent, which still runs your
own status line script so the terminal footer looks the same; it is what feeds
the chat's status line and More details (for Cursor: context use, output tokens,
version, Max Mode, auto-run, worktree, plus the branch, lines changed, and pull
request). Removing the hooks restores your own command. Agent approvals ("accept all") is a
per-machine default with per-project overrides. Actions (Settings > Actions)
are saved terminal commands or browser URLs shown on project headers and in
the header’s Quick Actions button, which shows the name and icon of the Action
you used last and runs it again on click (its caret opens the full list; it
reads Start until an Action exists); Global Actions apply to every project.

Agents Hub lets you browse and edit agent files in Skills, MDs, Hooks,
Configs & MCPs, and Agent Sync. In MDs, expand Shared agent markdown to see the
files in your shared agent folder, then select a filename to read or edit it.
Expand the profile instruction groups the same way. Use Refresh to reload files
from disk and Save to write your edits.

Agent Sync (the fifth Hub tab, Cmd+5) keeps one source of truth in `~/.agents`
(skills, `main.md` and the other rule files, hook scripts, `.skill-lock.json`)
and points every agent on the computer at it. The Overview says how many agents
are out of sync, shows how many use the shared Skills, Instructions, and Hook
scripts, and lists the things to fix in plain words (links to skills that no
longer exist, skills that are copies instead of links, agents that link the whole
skills folder, agents that do not read the shared instructions); open a row to
see which agents it affects and what the fix does. The left list shows each
agent with "to fix" or a check, profiles nested under their agent, and a To fix /
In sync / All filter; select an agent to see its Skills, Instructions, and Hook
scripts cards. Review and fix all, a row's Fix, or an agent's Review and fix
opens a plan first:
one relative symlink per skill in every agent's skills folder, whole-folder links
converted to per-skill links, the one-line pointer written into each agent's
instruction file (a file with other content is backed up as
`<name>.pre-sync-<stamp>.bak` first), and the hooks folder and lock file linked
into Claude Code and Codex. Nothing is deleted; tidying leftover lock file
entries is opt-in (the Optional cleanup switch, or the plan's last group). Agents that read `~/.agents/skills` directly (Amp, Cursor,
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
`--interrupt` for an urgent correction, or `--queue` to leave the message
waiting until the current turn finishes. A queued message waits as long as that
turn does, so send normally unless the point is to have the next task ready for
an agent whose final message you have already read. If Ghostex cannot deliver
a queued message, the row stays in the recipient's queue marked Not delivered
with Retry and Delete, and the sending agent gets a note saying so. `agents close
<session-ref>` ends that session, including any unfinished work. `ghostex read-session-chat` and `ghostex read-text` read replies.
An agent can also read or search any other thread, including a sleeping one:
`ghostex read-session-chat <session> --all --format text` prints the whole
conversation, and `--grep "<words>" --context 1` finds where a topic came up.
`<session>` can be any id from the sidebar's Copy Details, or the title.
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
3. The Ghostex Agents skill (`$ghostex-agents`, installed
   from Settings > Integrations or `ghostex agents-orchestration install-skill`)
   teaches an agent to read `ghostex agents --help` and `ghostex --help`, then
   launch other agents with the model and effort you ask for, message them to
   hand off tasks and coordinate work, read their replies, and verify what they
   did. Agents message each other the same way, so several sessions can split a
   job between them and report back without you relaying every step.

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
  With Auto reconnect enabled in the phone's Settings > Connection, the phone
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
  To read a project's docs on the phone, long-press the project and choose
  Docs, or choose Docs from a session's ⋯ menu. It lists the project's Markdown
  and HTML files from the same folders the desktop Docs view shows, with search
  and the most recently changed files on top. Files open in a reader on the
  phone, and Reload picks up an agent's latest edit. HTML pages include the
  Agentation annotation tool (the pen button hides it); its copy button puts
  your notes on the phone's clipboard, ready to paste into a session.
- **From another computer**: Settings > Remote > Remote machines > Add a
  machine with SSH details or an Easy Connect code, then Install / Connect
  gxserver on it. The machine appears as a sidebar section with its own
  projects and sessions; its terminals stream into the desktop app. Windows,
  Linux, and macOS clients use the connected computer's folders and shell.
  Open Code from the view panel's + menu to edit the remote project; if prompted,
  install the editor component first. Folder links in remote chats also browse
  the remote folder in Code.
  Remote localhost links open in the built-in Browser through that computer,
  even when ordinary web links are set to open in your external browser.
- **Web app**: the desktop's sidebar (with its session, group and project
  actions, the Git menu and Quick Access), chat and terminal running in a
  browser and talking to gxserver; remote machines, Settings and the commit
  review stay in the desktop app. It is built from a Ghostex source checkout
  with `bun run start:web` and is not part of the installed app.
- **CLI**: `ghostex attach <selector>` attaches to a session from any terminal,
  including over SSH.

Windows computers accept Android, macOS, and Linux desktop connections over SSH.
Install Ghostex on Windows, enable SSH, and add the Windows address with your
Windows username. Leave Advanced > Windows WSL distribution blank to use the
Windows Environment selected in Windows Ghostex: native PowerShell with Windows
folders (the default), or WSL with Linux folders. Enter a distribution name to
use that WSL2 distribution instead. Windows agent CLIs must be installed for native
PowerShell projects. After changing the Windows environment and restarting the
Windows app, reconnect the phone or remote desktop machine to use that environment.
The connecting computer keeps its own local environment: a Linux or macOS client
can work with Windows paths and PowerShell sessions on the connected computer.
Linux stores saved SSH passwords in the desktop keyring; Windows uses Windows
Credential Manager.

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
Every copy shows a small "Copied!" bubble at the pointer for a moment, whether
it came from a terminal, a chat message, a copy button, or a menu. Copy Sound is
off by default. Enable it under Settings > Notifications > Sounds to also hear a
short sound when copying from a terminal, a chat message, the chat composer
(including its right-click Copy menu), a copy button, or a menu (`copySound`).

The Notifications bell sits in the sidebar's top row, just before the sidebar
menu button, and shows how many notifications are unread. Click it to open the Notifications
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

Project headers show the branch and diff stats; the header’s Commit (Git) menu offers
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

- Settings > Extensions shows every extension as a card, three to a row: the
  built-in ones, grouped by category (Project websites, Code and files,
  Planning and automation, Header buttons, Menus and panels, Shared runtime),
  then the Extensions Store (installed extensions first, then the audited
  third-party ones you can install), then Your views (custom URLs, Linear,
  GitHub Issues, dev server commands, HTML reports). One filter bar above them
  searches all of them at once and filters by source, type, and category; the
  count beside it says how many are shown. Each card's switch turns it on or
  off, and its actions (Edit, Details, Remove, Reinstall) appear when you hover
  it. Extension commands use the active local project's folder unless the
  extension supplies a folder; relative folders are resolved inside the active
  project.
  The Edit (pencil) button on a card chooses where that view, header button, or
  extension appears. Pick **Everywhere** or **Only in selected places**, then
  choose projects and spaces from the dropdown next to **Except in** (or
  **Show in**), so a view can be hidden in one project without listing every
  other one. Once a space is picked, **But keep in** (or **But not in**) lists
  projects that should ignore their space's choice, because a project's own
  setting wins over its space, and a space wins over the default. A sentence
  under the choices spells out the result. Worktrees follow their parent
  project, and a project inside a group follows the group. A card narrowed this
  way shows its scope under its description, and the view or button is simply
  absent wherever it is hidden, so its hotkeys and command palette entries go
  away with it. Custom views under Your views keep their own Available in
  picker inside their own editor.
  Its Account usage in the sidebar section lets you star saved Claude and Codex
  accounts to show their usage at the bottom of the desktop sidebar, or unstar
  them to hide it. These are the same per-account stars available in
  Settings > Accounts. The meters are hidden until you ask for them: hover the
  chart button in the sidebar's Commands row, just left of the Settings gear,
  and every starred account floats over the bottom of the list, four per row,
  without moving the list. The button turns into a pin while you hover it:
  click it to keep the meters there above the Commands row, and click it again
  (the pin shows filled while they are pinned) to unpin them. Ghostex remembers
  whether they are pinned. The button itself is only there once you have
  starred an account.
  Claude meters show the two tightest of the weekly, five-hour, and Fable
  limits, so the Fable limit is never hidden when it is running out; launcher
  and picker rows and the Accounts figures use the same two numbers. Each
  meter opens that login's live limits, reset times, and extra usage or rate
  limit resets, with the Fable limit as a main bar for Claude. Right-click a
  usage meter for Extensions and Accounts. Click the same
  usage meter again to close its dropdown. Click another dropdown's
  button to close the current dropdown and open that one in a single click.
  Clicking outside, including in
  Session Chat, closes usage dropdowns and Tips. More model
  limits starts collapsed. Codex and Claude meters both show a Rate limit
  resets row when the provider has granted the account free resets (for
  Claude, promotions such as a model-launch reset). Click the count to list
  each reset with its expiry date, click Use beside one, then Reset to confirm:
  Ghostex uses that reset right away, without opening a terminal, and the
  limits refresh in the dropdown. Using a reset can't be undone; if your usage
  doesn't need a reset yet, nothing is used. Shared history stays visible
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
- The header's ⋯ menu holds Ask Ghostex, Tips & Tricks, Resources, Dev
  servers, Extensions and Customize. Each of the first five opens a panel
  under the ⋯ button that closes when you click away. Ask Ghostex, Tips &
  Tricks and Resources are available in every project. Tips & Tricks teaches
  features one card at a time; Resources lists what Ghostex is running with
  per-session CPU and RAM, and can put a session or the editor to sleep; Ask
  Ghostex lists sample questions, and picking one opens a Ghostex Help chat
  with the question staged so the user can edit it and press Enter. Dev
  servers lists the development servers running on this computer and on your
  remote computers; the same list is what a new Browser tab shows. Extensions
  lists your installed extensions. Customize opens Settings > Extensions,
  where each of these entries can be switched on or off; an entry switched off
  there is not listed, and Customize itself is always in the menu.
- Welcome to Ghostex is the onboarding that opens the first time Ghostex
  runs. Its five panels cover: the agents found on this computer, with
  Install buttons for Claude Code, Codex and Cursor Agent, an Install guide
  that installs any other supported agent, the Ghostex helper (agent hooks)
  and Computer Use; which views to show (Browser and Docs are on by default
  on a first run) and the browser skill; phone pairing and notifications;
  and the first project folder with the default agent and session view,
  next to the look: Appearance, the dark and light theme, Background contrast,
  Enable Transparency and Transparency strength (the same choices as Settings >
  Theme). Turning transparency on there also switches Appearance to Dark. "I already know Ghostex" on the first panel skips the rest. Reopen it any
  time from Tips > Setup or Quick Access > Commands > Setup.

## Appearance and app

Theme, background contrast and tint, window glass, and active pane outline
live on their own Settings page, Theme, right below General
(`ghostex settings open --tab theme`). The page starts simple: Appearance
(System, Light, or Dark; System is the default and follows the operating
system appearance), a row of cards for the dark theme and one for the light theme, each
card a small picture of the window in that theme's colors, Background contrast (five
steps from Lowest to Highest, Normal in the middle; higher makes dark backgrounds
darker and light backgrounds whiter, for both appearances; it sets the Sidebar
contrast and Work area contrast sliders under Advanced together, which can also be
set apart, and it moves the Custom contrast sliders too), an Enable Transparency switch, and Transparency strength (a 0 to 100 slider; higher shows more of the desktop, and it sets the four glass tint sliders under Advanced so the work area stays a little more see-through than the sidebar). Everything else is under Advanced, a button below those that opens the
Colours, Chat and terminal, and Glass groups plus links to related
settings on General; a search for one of those rows opens it. Dark theme offers
Dark Gray (the default), Black, Blue, Green, Red, Purple, or Custom; Light theme
offers Light Gray (the default, #f4f4f5), White, Blue, Green, Pink, Orange, or
Custom. Choosing Custom opens Advanced, where Colours shows that appearance's
Background contrast slider (85 to 100 for dark, 60 to 100 for light; 100 is
black for dark, white for light) and Background tint color picker; the other
appearance's rows stay hidden. A preset never overwrites the custom values, so switching back to
Custom restores them. The chosen theme colors the sidebar and window chrome, the
sidebar's dropdown menus, and the chat view background (chat keeps following its
own Chat theme setting, so a light chat in a dark app uses the light theme's
color). Chat and terminal default to Follow app, with optional Light, Dark, or
System overrides under Advanced > Chat and terminal. Existing saved themes are preserved, and a
saved dark contrast or tint that differs from the default starts on Custom. The
accent color (status highlights, accent text, advanced-setting markers) has no
setting of its own: it follows the dark theme's tint hue, and a neutral tint
keeps the sky-blue accent.
Window glass lets the blurred desktop show through the sidebar, the work area,
terminals, and chat on macOS and Windows. The Enable Transparency switch turns it on as Glass in
dark mode (the default), which uses glass in dark mode and stays opaque in light
mode, or off as Always opaque; Advanced > Glass also offers Always glass, which
forces glass in both.
Docs, Kanban, the browser, and the code editor stay opaque. Turning on Reduce
transparency in the macOS accessibility settings, or turning off Transparency effects
in Windows Settings > Personalization > Colors, always makes the window opaque. On
Windows, turning glass on takes effect the next time Ghostex starts, the corners of
menus and pop-ups follow Windows' own rounding, and notifications keep solid cards.
Glass shows (macOS only) picks what the glass blurs: Desktop and windows (the default) shows everything behind Ghostex. Wallpaper only shows just your desktop wallpaper, so other windows never show through; built-in wallpapers such as Sequoia or the aerials show as a still picture of that wallpaper, and a solid color wallpaper shows everything behind the window. Custom image shows a picture you choose instead, one for dark mode and one for light mode (Glass image for dark mode and Glass image for light mode, each with a Choose image button); a mode with no picture shows everything behind the window. Video plays a muted, looping, blurred video behind the glass, one for dark mode and one for light mode (Glass video for dark mode and Glass video for light mode): pick an aerial wallpaper your computer has already downloaded (download more by choosing them in System Settings > Wallpaper), or Choose a file… for a .mov or .mp4 video. The video pauses whenever Ghostex is in the background, hidden or minimized, while the display sleeps and in Low Power Mode; Reduce Motion shows a still frame; and Play glass video only when plugged in (on by default) pauses it on battery. For Wallpaper only, Custom image and Video, Glass picture position picks Moves with the window (the default: the picture covers the window and moves with it) or Stays with the desktop (the picture stays put while the window moves over it, and can trail the window while you drag it) (`windowGlassSource`, `windowGlassImagePlacement`, `windowGlassImageDark`, `windowGlassImageLight`, `windowGlassVideoDark`, `windowGlassVideoLight`, `windowGlassVideoOnlyOnPower`).
While glass is on, four sliders tune it, each in dark mode and in light mode: Sidebar tint and Work area tint set how much of the desktop each area hides, independently, so either can be the darker one; lower shows more of your desktop.
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

Related settings: `sidebarTheme`, `darkThemePreset`, `lightThemePreset`,
`customSidebarTitlebarBackgroundDarknessPercent`, `customSidebarTitlebarBackgroundTintColor`,
`customSidebarTitlebarLightBackgroundLightnessPercent`, `customSidebarTitlebarLightBackgroundTintColor`,
`windowGlass`, `windowGlassSidebarOpacityDark`, `windowGlassWorkAreaTintDark`,
`windowGlassSidebarOpacityLight`, `windowGlassWorkAreaTintLight`, `themeSidebarContrast`, `themeWorkAreaContrast`, `showActivePaneOutline`, the `keepAwake*` rows,
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
  sidebar bell (kept visible with `notificationsTitlebarButtonHidden false`)
  lists every finished turn with what the agent said.
- "What does the Kanban board do": see Project board (Kanban).
