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
Kanban, U for Automate, D for Files, T for Terminal, W for Work) while the picker is in front. Views you hid
for this project are not in the list; **Manage views and where they appear…** at
the bottom opens their settings, and **Hidden here** on the `+` menu brings one
back. Closing the panel's last tab closes the whole panel; to bring the picker
back instead, turn off **Close side panel with its last tab** in Settings >
Sidebar with Show Advanced on (`closeSidePanelWithLastTab`, on by default).

**Work** lists the tickets and pull requests of every work-mode project in the
window (Linear tickets or GitHub issues, whichever the workspace's primary
tracker is), Personal ones too (see Work mode under Git and
worktrees). Like work mode, it is part of Workspaces, which you turn on in
Settings > Extensions (Features). Open it with the briefcase at the top of the sidebar, which shows
while the window has a work-mode project, or from **Open a view**. It is one list,
newest change first, and it opens with **Assigned to me** on (**Mine** on a narrow
panel). Under the search box, **All | Linear issues (or GitHub issues) | PRs**
picks what the list shows. **Filter** on the right (a number shows how many are
set) holds the state, the repo, the project (a Linear project, or a GitHub
Project in a GitHub workspace) and **Only in my sidebar** (work one of your
sessions is linked to). In a GitHub workspace where `gh` cannot read
GitHub Projects yet, a notice shows the command to run (`gh auth refresh -s
read:project`) with **Copy**; close it and it stays closed. A PR that
belongs to a ticket shows on the ticket's row; a PR with no ticket gets its own
row marked **No ticket**. A green dot means one of your sessions is working on it
right now. **View**, next to Filter, holds the sort order and **Group by**, which sorts the list into groups by Status, Repo, Project,
Assignee, Type (tickets, issues, PRs without a ticket), Pull request (no PR,
draft, checks failing or pending, ready, merged) or Updated (today, yesterday,
this week, older); click a group's header to fold it, and the list remembers
your choice (None keeps one list). Each row's second line starts with the
owner's picture, then the repo, the PR, and the Linear project (or GitHub
Project) last. Click a row's ticket ID, PR number, repo name or project name to
open that page (the ticket, the PR, the repo on GitHub, the project) in the
app's browser. Click anywhere else on a row, or a ticket or PR chip on a session card, to see its
details: the ticket and its latest comments, the PR with each check by name and
its reviews, videos (Loom, YouTube and video files play right there), the
sessions linked to it, and where it is in your team's flow (Ticket, Working
thread, Session, PR, Review comments, CI, Video, QC package, Validation). Steps
Ghostex cannot see, such as the video OK, show a question mark instead of a tick.
In a Work workspace connected to its team (see the team backend under Git and
worktrees), each row shows how many Slack threads the ticket has, and the details
show every thread: its channel, the first message and the latest replies with who
wrote them and when, files, links, and Loom or YouTube videos that play right
there, with **Open in Slack**; the ticket's working thread is marked as such.
Working thread is ticked once the ticket has one (click the step to open it), and
Validation once the request shows up in a watch-only channel or a session posted
the final result. **Conversations** also lists the sessions your teammates (and
you, in the cloud or on another computer) run on the ticket, with **Open in
Claude** and **Open in terminal** for a cloud session; watching a teammate's conversation is not available
yet. Without a team, as in a Personal workspace, these stay as they are. To
change the steps, open Settings > Workspaces and use
the workspace's **Team-flow steps**: reorder, rename or remove steps, add one with
the rule that marks it done (for example "The pull request has a label" with
`READY-FOR-QC`), then **Save steps**; **Reset to default** goes back to the
default flow. In a workspace connected to a team the steps are the team's, shared
by every teammate, and only the team's owners can change them. **Open chat** shows the session linked to the ticket;
**Start chat** starts one in a new worktree on the ticket's branch, linked to it,
and sends nothing. Its arrow opens a menu: Start chat (where you also pick the
agent and the project), **Link to current session**, which adds the ticket to the
links of the session you have selected in the sidebar without removing its other
links (a toast offers **Undo**; it is greyed out with the reason when no session is
selected, the session is on another computer, or, for a pull request, the session
already has a different PR), and **Start in cloud**. **Start in cloud** (also its
own button next to Start chat) lists the clouds Ghostex can start in, Claude Code
for now: pick one and a box shows the task Ghostex wrote from the ticket (its ID,
title, link, a short part of its description, the branch to work on with a pull
request linked to the ticket, and your team's instructions in a team workspace).
Edit it and click **Start**; the session runs on the ticket's branch (or creates
it when GitHub does not have it yet), opens in the app's browser and shows under
**Conversations** as yours, with **Open in Claude** and **Open in terminal**
(which attaches to it in a new terminal session with `claude --cloud <link>`). It
uses your own `claude` login, so sign in to Claude Code first, and the project
needs a GitHub remote. From a terminal: `ghostex work-mode start SPX-1245 --cloud
[--prompt-file task.md]`. **New ticket** at
the top creates a Linear ticket or a GitHub issue (see Create Linear Ticket
under Git and worktrees) and the list picks it up right away. The Work view is
part of the desktop app; the web version opens a chip's link instead.

Right-click a view tab to choose where that view appears and what happens to it.
**Reload** refreshes the clicked view, **Sleep** unloads it while keeping its tab (Code also stops its editor
server; choose **Wake** or click the tab to bring it back, and Resources can stop
Code too without closing Ghostex), and **Open externally** opens its page in its
own window. **Configure ▸** holds what a view can change about itself: **Modify
home URL** for a website view, and **Command output** and **Configure view** for
your own project views (Configure view opens that view's editor in Settings >
Extensions and focuses its name field). **Show in ▸** holds **This Project** and
**This Space**, which are ticks: unticking one hides the view there and leaves
it everywhere else (the space row is absent when the project is not in a
space). **Choose where it's shown…** at the bottom of that submenu opens the
view's full scope editor in Settings > Extensions. **Close tab** removes the
view from the strip.

- **Agents**: the terminal grid. Panes run agent CLIs or plain shells, split
  horizontally or vertically. There is no tab bar above a pane: the sidebar is
  the list of sessions, selecting one shows it in the focused pane in place of
  the session that was there, and a session already on screen in another pane
  is focused there instead. To split, drag a session row from the sidebar onto
  the left, right, top or bottom edge of a pane (terminal or Session Chat); drop
  it in the middle of a pane to show it there instead. Only sessions of the
  project on screen can go into its panes: a pane under a session from another
  project dims and says it can't split there. You can also press
  Option+Shift+D to move the focused session into a pane on the right. While the screen is split,
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
  Starting a new session always creates a new one; empty sessions stay in the
  sidebar until you close them. An Advanced setting, Close empty sessions when
  starting a new one (`closeEmptySessionsOnNew`, off by default), changes that:
  with it on, starting a new session in a project (the hotkey, the project's
  agent button or menu, the New Thread picker, or a project's agent on the
  phone) also closes that project's other sessions that are still completely
  empty: nothing sent, no chat draft, nothing queued, and no text in the agent's
  input box. It only applies when Chat is the default view for that agent; with
  Terminal as the default view it never runs. Once you type or send something a
  session stays like any other.
  Cmd+Option+Arrow moves focus between the session panes and the Commands pane;
  it skips the view panel.
  Hotkeys: `createAgentSession`, `openNewThreadPalette`, `createSession`,
  `openBrowserPane`, `forkSession`, `renameActiveSession`, `sleepFocusedSession`,
  `closeFocusedSession`.
- **Code**: the built-in VS Code based editor (code-server). Opens files from
  chat links, `ghostex edit <file>`, and Open In. Optional Use VS Code settings
  reuses the local VS Code configuration. The editor and the web runtime (see
  Browser) are both optional installs; the Code view offers each one the first
  time it opens.
- **Browser**: embedded Chromium tabs with profiles, splits, annotations,
  DevTools, and agent control through the `$ghostex-embedded-browser-use`
  skill. Its tabs sit in the view panel's tab strip, in the same row as the view
  tabs, and stay there while you are in another view, so clicking one comes
  back to Browser with that tab in front. Each tab shows its page icon and
  title, closes with its own x or a middle click, and drags anywhere along the
  strip to reorder. Right-click a tab for Select Tab, Pin Tab, Sleep (that tab),
  Sleep Browser (every browser tab), and Close Tab. Closing the last browser tab closes the
  Browser the way closing any view does: the panel moves to the next open view,
  and shows the Open a view picker (or closes the panel, with Close side panel
  with its last tab on) when Browser was the last one. **Browser Tab** at the top of the strip's **+** menu opens
  another tab. A tab with no address yet is blank: type or paste an address, or
  pick a running server from the ⋯ menu's Dev servers panel.
  Right-click a link or image in a page to open it in a new tab, save it
  (Save link as / Save image as ask where to save), copy its address, or copy
  the image itself; any page download also asks where to save.
  Web links from terminals, chat, and detected dev servers
  open here or in the system browser depending on Open links in.
  The Browser runs on the web runtime (Chromium), an optional one-time install
  that Ghostex does not need for anything else: until it is installed the
  Browser, the Code view, website and extension views, and HTML, drawing and
  media files in Files show an Install button instead of the page (or Hide tab
  to remove the view), and web links and saved Browser actions open in the
  system browser. Nothing is downloaded until you press Install, and the runtime
  starts only when one of those views is shown. Settings > Extensions > Built-in
  > Chromium runtime (CEF) installs, reinstalls or uninstalls it (uninstall
  before opening a web view, right after starting Ghostex). Annotate the current page
  with Agentation in the Browser toolbar; GitHub pages disallow that tool.
  When a page shows its content inside a frame, such as a Storybook story,
  the Annotate toolbar opens inside that frame so the content itself can be
  selected. Copying or sending annotations clears them afterwards by default;
  the toolbar's own settings panel (Clear on copy/send) turns that off.
  HTML files in the Files view use the same Agentation overlay via Annotate.
  Markdown files use Files selection comments instead (see Files below).
  A page in the Browser or in a website or extension view (Linear, Jira) can open
  another app through its link, such as Okta Verify, Zoom, Teams or an email link:
  Ghostex asks first ("… wants to open Okta Verify.") and opens the app only when you
  choose Open. A link no installed app can open does nothing. The page stays as it was, and
  a Browser tab opened only for that link closes once you answer. Linear sends its
  links to the Linear desktop app when its "Open in desktop app" preference is on;
  turn it off in Linear's Settings > Account > Preferences inside the Ghostex
  Browser to keep Linear pages there. A sign-in page that checks
  for an app on this computer (Okta FastPass looks for Okta Verify this way) asks
  "Allow … to connect to apps on this computer?"; Allow and Don't Allow are both
  remembered for that site in that workspace's Browser (Don't Allow on linear.app keeps
  Linear pages in the Browser instead of sending them to the Linear desktop app). These
  questions show as a bar at the top of the page that asked, under the Browser's address
  bar, one at a time; they never block the window, so you can keep working or close it.
  The × (or Escape) means "not now" and remembers nothing, and a question goes away by
  itself when its page moves on or its tab closes. Settings > Workspaces > Site
  permissions > Forget all answers lets every site ask again. If the sign-in gave up
  while the question was open, try it again.
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
- **Files**: Markdown, HTML, and Excalidraw files from the project's docs
  folders, with a markdown editor and an annotation system that sends notes
  back to the agent. Select text in a Markdown file to comment on it, mark it
  Looks good, Clarify, or Needs tests, or mark it Remove this (the X button),
  and add a global comment from the header; typing over a selection replaces it,
  as in any editor. Unselect the text (or press Escape) to close the toolbar.
  Hover a table to open it in a window or copy it as Markdown or CSV; a table
  wider than the pane wraps its cells to fit, and scrolls sideways with its
  scrollbar when it still cannot. In the comment box, Add (or Cmd+Enter, Ctrl+Enter
  on Windows and Linux) adds the note to the list; the same chord outside the
  box is Send. Send (or Cmd+Enter) delivers the new notes as
  numbered feedback with line numbers to the session last clicked in the
  sidebar for the active project: into its chat composer when the chat is
  showing, or into the agent's terminal when its input box is available. The
  Send button shows how many new notes are waiting next to its icon and looks
  disabled when there are no notes; its tooltip names the session they will
  land in. When no agent session is
  selected, the agent's input box is busy, or Ghostex cannot tell for that
  agent, the feedback is copied to the clipboard instead and a toast says so.
  Sending leaves Files on screen. Sent notes stay visible with a Sent mark;
  Send offers the new notes first and, once everything has been sent, sends
  all of them again. Send covers the open file's notes. Notes stay until you clear them with Clear. In a chat, the Reply by Annotating button below an
  agent reply (between Copy message and Save to md) opens that reply in Files so
  it can be annotated the same way, with the feedback going back to that
  session. Folders appear as they load, and search fills in while
  Updating files is shown. Expand a folder to load it sooner; a loading or error
  marker means its contents have not been confirmed yet. Use Refresh in the Files
  sidebar menu to check for changes immediately. The button at the sidebar's
  window edge hides the files list; the same button in the corner brings it
  back, and hovering that button peeks the list without pinning it. Hidden or pinned is remembered. When the Files view is narrower than 800px the list opens as a
  temporary drawer over the document and closes when you open a file, press
  Escape, or click outside it. Cmd+F, or Ctrl+F on Windows and Linux, opens the
  search: inside a Markdown document it shows Find and Replace with the caret
  ready, and anywhere else it reveals the files list and focuses its search.
  Escape closes the document search. To open an HTML, Markdown, or Excalidraw
  file in your web browser, use the open-in-browser button in the file's header
  or Open Externally at the top right of the view: HTML pages open as they are,
  Markdown opens as a formatted page, and drawings open in an Excalidraw editor
  that saves back to the file. The arrow beside the button, and Open With in a file's right-click
  menu, list your other browsers and the apps that open that kind of file (on
  Windows, also Choose another app for the system's Open with dialog).
  Browser links stop working when Ghostex restarts; open the file again for a new one.
- **Terminal**: a command terminal in the view panel that works like the
  Commands pane, only on the right beside your sessions instead of below them.
  It has its own tab bar with a **+** for new terminals, Cmd+Shift+T for another
  tab and Cmd+D for a split while it has focus, drag to regroup or split, right-click
  a tab for Sleep and Close scopes, and Actions can run in it. A project has one
  Terminal view; opening it creates its first Command Terminal, closing its last
  tab closes the view, and its terminals keep running and come back when you
  reopen it. It does not replace the Commands pane: Cmd+J (Mac), Shift+Esc, F12 and the header's command
  terminal toggle still open that pane, and both can be open at once.

Related settings: `terminalViewWidthMode`, `webLinkOpenTarget`,
`markdownFileOpenView`, `htmlFileOpenView`, the Auto Sleep rows
(`autoSleep*IdleMinutes`), and Settings > Extensions.

## Sidebar

The sidebar lists projects and their sessions. Project headers carry the git
branch and diff stats and, on hover, the agent launcher (New agent and the
agent picker) and a ⋯ button. The ⋯ button, or a right-click on the project,
opens the project menu: Compact/Full list, Add Worktree (or Create PR), History,
New Browser Tab, Create Terminal, pinned Actions, Open Folder in the file
manager and Add to Group.
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
In Settings > Sidebar, enable Show Advanced to find **Highlight unanswered
questions**. It adds a soft pink background to sessions with a detected unanswered
question, even while the agent keeps working. It is off by default and currently
supports Codex asynchronous questions (`highlightPendingQuestions`).
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
**Group working sessions** (in the More menu's Sort & Filter page, and in
Settings > Sidebar; both switch the same setting, off by default) moves each
project's sessions into a Working section under that project while their agent
works. The section is collapsed by default (click its heading to open it) and
its heading shows the count. A session goes back to Sessions on its own when
its agent stops. The session you have open, and a working session with an
unanswered question (pink), stay in Sessions. Pinned, draft, parked and snoozed
sessions keep their own sections (`groupWorkingSessions`).
Workspaces keep different parts of your work apart, for example one per company
you work for and a Personal one. Workspaces is a built-in extension, off by
default: turn it on with its switch in Settings > Extensions (Features). It also
brings work mode, the Work view and the team backend (see Git and worktrees).
While it is off there is no workspace button, every window shows every project,
the project menu (on the computer and on the phone) has no Work Mode row, and the
`ghostex workspace`, `work-mode`, `link-session`, `team` and `slack`
commands say it is turned off; your workspaces, links, Linear keys and team
connections are kept and come back when you turn it on (`workspacesHidden`).
Each workspace has its own projects, Spaces,
Linear API key, Claude account and Browser sign-ins (cookies), and a window
shows one workspace at a time. Every install starts with one workspace,
Personal, holding all your projects and Spaces. The workspace button sits at
the left end of the Space row (with Spaces off, it shows in that row once you
have a second workspace): the workspace's letter with a small arrow beside it.
Click the letter to switch this window to your other workspace in one click
(with more than two, it goes back to the one this window showed last; hover it
to see where it goes). If another window already shows that workspace, that
window comes forward instead. Click the arrow for the workspace menu: pick any
workspace, open Workspace settings, or make a New workspace. In the menu, a
workspace that no window shows yet has a new-window button at the right end of
its row, which opens it in a new window. Switching reopens the session this window
last had open in that workspace (or its first project with no session, or
nothing if it is empty), and puts to sleep the open views of projects outside
it; their tabs stay and reload when you come back. To move a project,
right-click it and choose Move to workspace; its sessions and worktrees go with
it, and it leaves its group and Spaces and shows under Other in the new
workspace. A project you add with Add Project (or clone from it) joins the
workspace of the window you are in; a worktree joins its project's workspace;
a project added from the command line (`ghostex add-project`) goes to Personal
unless you pass `--workspace <name>`. Opening a folder in Ghostex from your
computer (or a terminal there) adds a new folder to the window's workspace, but
a folder that is already a project stays in its own workspace and opens there,
in a window already showing that workspace or by switching the window to it.
The Ghostex project that holds Help chats shows in every workspace. A Space you
create joins the window's workspace too. A remote machine's tab shows in one
workspace of this computer, Personal until you right-click the tab and choose
Move to workspace; its projects keep that machine's own workspaces. On the
phone, the Sessions list shows the same letter tile at the left of the Space
row for each computer: tap it to choose which of that computer's workspaces the
list shows. The phone remembers the choice, a session opened from a
notification, the session search or Search Prompts switches the list to that
session's workspace, and a project added from the phone joins the workspace it
shows. Settings >
Workspaces names each workspace, sets its color, makes it Work or Personal
(Work turns work mode on by default for its projects), picks its **Primary
tracker** (Linear tickets & projects or GitHub issues & projects), and sets its
Linear API key and the Claude account its agents use (an account you pick when you start a
session still wins, and the agent launcher and New Thread picker mark the
workspace's account as Default); Sign out of all sites clears that workspace's Browser sign-ins, and
Delete moves its projects and Spaces to Personal. From the command line:
`ghostex workspace list`, `ghostex workspace create <name> [--kind work|personal]`,
`ghostex workspace rename <workspace> <new name>`,
`ghostex workspace move-project <project> <workspace>` and
`ghostex workspace delete <workspace>` (a workspace or project can be named by
its name or id; a project also by its folder).
Spaces group projects or groups together; they are not saved filters, and a
filter cannot be saved as a Space. Spaces is a built-in extension, off by
default: turn it on with its switch in Settings > Extensions (Features). While
it is off the sidebar has no Space row or Space menus, the Space settings are
hidden, and a view limited to some Spaces shows everywhere; your Spaces are
kept and come back when you turn it on (`sidebarSpacesEnabled`). Spaces follow the
machine: a remote machine shows its own Spaces when Spaces is turned on on that
machine, and the switch in Settings turns Spaces on or off for this computer
only. Create one with the "Create space" button
that fills the Space row while you have none, by right-clicking the Other
button or a Space icon and choosing New Space, or from the More menu when
Spaces overflow. To switch to the next or previous Space, swipe sideways over
the sidebar list with the trackpad, or drag sideways with the mouse on an
empty part of the list (below the last project or between projects).
To jump straight to a Space, press Cmd+Option+Shift+1 to 9 (Ctrl+Alt+Shift+1 to 9
on Windows and Linux) for the first to ninth Space in the order the Space row
shows them; rebind them under Go to Space 1-9 in Settings > Hotkeys.
A project added with Add Project (from the More menu, from the "Add Project"
button that an empty project list or empty Space shows, or by right-clicking
the empty sidebar area) joins the Space that is open at the time and appears at
the top of it; add a project while Other is selected to leave it out of every
Space.
Grouping never moves a project out of its Space: a group made with Add to Group
> New Project Group joins the Space its project is in, and a project taken out
of a group (Remove from Group, Ungroup, or dragging it out) stays in that
group's Space. Adding a project to an existing group in another Space moves it
to that group's Space, and the sidebar switches there with the project focused.
A session group made with a session's Move to New Group shows in the project's
Space.
In Add Project, select the computer whose folders you want to browse. Local
folder starts in your home folder, and External drives and other folders shows
that computer's filesystem root. On a computer running native Windows
(PowerShell), the two are a single Local folder row that opens the list of
drives, with your home folder at the top. You can paste a Windows drive or UNC
path when the selected computer runs native Windows, even from a Linux or macOS
client.
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
Kanban, Automate, or Files). If that session was closed, the one before it is
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
instant. Set it to 0 to release them as soon as you leave. The page each other
Space opens on is the exception: it stays loaded for as long as that Space opens
on it, so swiping to a Space always shows its web page instantly.
Starting a new agent from the sidebar launcher or New Thread picker keeps your
current view open, including Code, Browser, Kanban, Automate, and Files. Select
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
  Browser, Kanban, Automate, or Files view has a minimum width of 455px.
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
  Cancel Close After Done). The enabled buttons also stay in the
  session's right-click menu, which always runs Rename, Pin, Snooze, Park,
  Sleep, then Tag As after a line, like ChatGPT's menu; Note is under the
  menu's Advanced submenu unless its hover button is on. Close is the
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
  Parked parks it. Dropped between two pinned sessions, it is pinned right
  there; dropped on the Pinned heading, it goes to the end of Pinned. Sessions
  and Parked keep their own order, so the drop line shows where the session
  will sit in them. Only sections already shown can be dropped onto;
  dragging never adds a heading or moves the list.
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
- A red dot on a session means a model, effort or mode change you picked in the
  chat could not be applied: the agent refused it, or it kept failing for 30
  seconds. Messages you send after picking it wait instead of reaching the agent
  on the wrong model. Hover the session to read why, then pick a model again in
  the chat's model menu (the same one, or another); once it applies, the dot
  clears and the waiting messages are sent.
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
  `unparkAfterSendingMessage`. Right-click the Parked header to Sleep All or
  Close All of its sessions.
- Snooze puts a session away until a chosen time: right-click it and choose
  Snooze (or use the Snooze hover button), then pick 1 hour, 3 hours,
  Tomorrow (9:00) or Next week (Monday 9:00). The session moves into a
  collapsible Snoozed section below Parked and is put to sleep. When the time
  passes it returns to its usual place, still asleep until you open it.
  Unsnooze brings it back early. Snooze needs no setting; it is always
  available.
- Remote machines appear as their own sidebar tabs; a red cloud means the
  machine could not connect, and its tab explains why.

Related settings: everything under General > Sidebar, `agentManagerZoomPercent`
(sidebar interface size), `sidebarSpacesEnabled`,
`sidebarSpaceSwitchBehavior`, `sidebarSpaceFollowActiveSession`.

## Commands pane

The Commands pane holds command terminals below the workspace, or on its right
when Command Pane Side is set to Right. Open it with Cmd+J (Mac), Shift+Esc or F12; Settings > Hotkeys can
change the first two (Open Commands Panel and its Second Key). The **Terminal** view
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
In a Hermes chat, `/rename <name>` is sent as Hermes' own `/title <name>`.
Renaming an Empryo session from Ghostex (the Rename dialog or `ghostex rename-command`)
types Empryo's own `/tab rename <name>`, so Empryo keeps the name too, and the
Rename dialog's Generate Name can name an Empryo session from its recent messages.
ZCode sessions rename from the sidebar and `ghostex rename-command` too: the name
is saved in ZCode's own session store once its session row exists (after the
first prompt), and ZCode's automatic naming will not replace it. Before that,
the rename is saved only in Ghostex and ZCode's later automatic naming may
replace it.
Fork (a session's right-click menu, or More actions in its chat) opens the new
session beside the original and switches to it. It starts as
`Fork: <original name>` and saves that name through the agent's own rename
command so it survives reopening the conversation.
Fork works for Claude, Codex, Pi and Empryo sessions. Empryo has no command-line
fork, so Ghostex copies the Empryo conversation up to its last finished
reply under a new id and opens that copy; fork an Empryo session once a turn has
finished. The copy gets its own copies of Empryo's checkpoints, so undo works in
the fork, and deleting or closing either session leaves the other's checkpoints
alone.
Once a conversation has forks, a small branch button in the chat's top right
lists every session that shares the earlier history, including the thread you
forked away from, and switches to the one you pick; a stopped branch is resumed
when you open it.

- Sleeping frees RAM; Auto Sleep does it after idle minutes. Auto Sleep runs
  on the computer that hosts the sessions, so it keeps working while the app
  window is closed, and it never sleeps a session a Ghostex window or the phone
  app is showing. Web pages sleep the same way: a browser tab in
  any project, or a Files, website or extension view, that has been off screen
  for its Browser or Project Auto Sleep time closes to free its memory and
  reloads when you select it again. Tabs playing sound, tabs where you typed
  text you have not sent, and the page another Space opens on stay awake. Resources in the
  header's ⋯ menu sleeps many at once and shows CPU and RAM per session. Clean RAM
  copies a diagnosis prompt; paste it into an agent session to reduce RAM use.
  Sleeping sidebar sessions keep their normal title color and show a dimmer
  last-active time on the right; turn on Settings > Advanced > Dim sleeping
  sessions to fade the whole row instead (`dimSleepingSessions`). Use `ghostex sleep|wake <selector>` to
  sleep or wake a session.
- A sleeping session wakes when you ask for it. Clicking its row in the
  sidebar wakes it. Selecting its tab, opening its project, or
  coming back to a project after restarting Ghostex shows a small bar with
  its name and a Resume button instead; click anywhere in the pane or press a key to
  wake the session. With
  Click to Wake Sleeping Panes turned off, those wake right away
  (`clickToWakeSleepingSessions`). To stop a sidebar click from waking a
  sleeping session, turn off Settings > Advanced > Wake sleeping sessions when
  selected: the click then opens the session with its Resume button, and it
  wakes only when you click the pane or press a key
  (`wakeSleepingSessionsOnSelect`).
- Drag pinned sessions to reorder them within their project. Rows stay in place
  while an icon-and-title ghost follows just below and right of the pointer; the
  insertion line marks where the session moves when you drop it, and no line
  means the drop would change nothing. Projects and project groups show the same
  line where they will land, after an open project's whole list of sessions.
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
  are excluded from discovery. With Bots on, Hermes conversations started
  outside Ghostex (in a terminal, the Hermes app, Discord, or Telegram) are
  discovered from every Hermes profile and listed under their bot; opening one
  runs `hermes -p <profile> --resume <session-id>` (`-p default` for the default
  profile). A Discord or Telegram chat continues in that terminal,
  and replies there do not reach the chat app. Other chat apps, cron, empty
  sessions, Discord threads only bots wrote in, and scripted one-shot runs
  (`hermes chat -q`) are left out.
- Search by Prompt (More Options, Actions (Cmd+K) in Quick Access > Sessions,
  the `openFindPrompts` hotkey, default
  `cmd+shift+f`, or `gx f` in a terminal) fuzzy-searches every prompt you ever
  sent to an agent; Enter resumes that session, and starred prompts stay on
  top. Inside the picker the agent and project filters are dropdowns at the top
  right (Ctrl+G and Ctrl+J open them), Grouping (Ctrl+D) toggles day headers,
  and hovering any control shows its hotkey. On the phone, Search Prompts in
  the menu at the top right searches the selected computer the same way: the agent,
  project and Days buttons under the search field filter and group the results,
  the star on a result stars it, pull down rebuilds the index, and tapping a
  result shows the whole prompt with Resume, Fork, Copy and Star.
  Empryo prompts are listed too, from every repository Empryo has run in, and
  resuming one reopens that Empryo session. A prompt can be forked into Claude,
  Codex, Pi, OpenCode, Cursor or Grok, never into Empryo, which cannot start a
  session from a prompt.
- Delayed Actions opens Session Automations. Send Enter defaults to **When all
  agents finish**. It can also run after a delay, when this agent finishes, or
  **When a specific agent finishes**. Choose the specific agent from the Agent sessions
  on the same computer; sleeping sessions are excluded. Ghostex waits until the
  selected agent has remained idle for 10 seconds and restarts that wait if it
  resumes work. Close After Done closes a pane once its command exits.

Related settings: `autoSleep*`, `clickToWakeSleepingSessions`,
`dimSleepingSessions`, `wakeSleepingSessionsOnSelect`,
`showSessionIdInTerminalPanes`, `sessionTitleGenerationAgent`,
`renameSessionOnDoubleClick`.

## Session Chat

Session Chat renders the same agent session as a chat GUI: composer with
image paste and Ctrl+G rich prompt editor, a prompt queue that sends when the
agent stops, transcript with thinking, tool, and edit cards, subagent
transcripts, question and approval cards, rewind, and a note per session.
Type `@` in the chat box to mention a project file and `$` to pick one of the
agent's skills. A picked skill shows as a pill in the way that agent invokes
skills: `/skill-name` for Claude Code, which Claude receives as its slash
command, and `$skill-name` for Codex.
Skills, second from the bottom of the chat's More actions menu and of the ⋯ menu
under a terminal view (on the computer and the phone), lists the Ghostex skills
the session's agent has installed: in the chat a skill goes into the chat box as
that pill, and in the terminal it is typed into the agent's input; neither sends
it. Its last row, Configure / Install more, opens Settings > Integrations > Agent
skills (the phone says to install them there on the computer).
Use the paperclip to attach images, files, or folders. On Linux, choose
**Images or files…** or **Folders…** before selecting items in the system picker;
the terminal's attachment action offers the same choices.
Dictation (a microphone button in the chat box and on the terminal bar) is turned
off in this version on every platform while it is being fixed, so there is no
microphone button yet.
A long code block shows its first lines and scrolls inside the block; the arrows
button in the block's header shows the whole block, and again collapses it.
A long message you sent (more than 20 lines or 2,000 characters, such as a
pasted log) shows its first lines with Show more under them; Show less folds it
again. Copy still copies the whole message.
Hover a message to show its actions and the time it was sent in a row below
it: Copy message, Reply by Annotating, and Save to md under an agent's final
reply; Rewind to here, Save prompt, and Copy message under your own messages.
Hover the time to see the full date. When the session you are in shows its
chat, Cmd+Shift+; copies the last code block the agent wrote and Cmd+Shift+C
copies the agent's last reply (Mac only; on Windows and Linux Ctrl+Shift+C stays
terminal copy). Focus Chat Box, which moves the keyboard to the chat box from
anywhere in the window, has no default key; set one in Settings > Hotkeys.
The chat box edits like VS Code: Up on the first line jumps to the start and
Down on the last line to the end, Option+Up/Down moves the current line,
Option+Shift+Up/Down duplicates it, Cmd+Shift+K deletes it, Cmd+L selects it,
and with nothing selected Cmd+X cuts the whole line and Cmd+C copies it (Alt and
Ctrl on Windows and Linux).
Type `/` in the chat box to browse the agent's built-in commands. In Cursor
chats, `/compact` summarizes the conversation to reduce context, just like
`/summarize`.
In Claude Code, Codex, and OpenCode v2 chats, start a message with `!` to run a shell command
in that agent's session, for example `! pwd`. The command and its output appear
in the chat.
In Claude Code chats, `/btw <question>` asks a side question without stopping
the agent's work. Side chat in the chat box's More actions does the same: it puts
a Side Chat pill in the chat box, and sending the message turns it off again.
Typing `/btw` on its own and pressing Enter also turns Side chat on; nothing is sent.
The answer opens in a card above the chat box, with Copy, Fork
(continue the side question as a background agent) and Close; a long answer
shows Show all. You can't reply to a side question: close its card to message
the main agent again. After Close the side question stays in the chat, folded where
you asked it, and opens again on click. It is not added to the conversation.
Codex chats have Side chat and `/btw` too, but Codex answers in a side conversation
in its terminal, so sending one switches to Terminal View. Your next chat message,
or switching back to Chat View on the computer, closes the side conversation first.
Claude panels such as `/status` and `/usage` show as clickable tabs, tables and
usage meters instead of terminal text.
A new Claude Code install's first-run setup is answered in the chat too: the
text style, the login method, and Sign in to Claude, where you paste the code the
browser sign-in page shows and press Sign in (Retry appears if the code was wrong).
`/login` in a running Claude chat opens the same Sign in to Claude card. Codex's
Sign in with Device Code shows its link and the one-time code to enter in any
browser, handy on a remote computer. When Codex quits to the terminal (after
Update now on its update prompt, or `/logout`), the chat says so and offers
Restart Codex, which starts it again on the same conversation.
When Cursor isn't signed in, its chat shows Sign in to Cursor: Sign in opens
Cursor's sign-in page in the browser, and the card then shows the sign-in link,
which also works from your phone or another computer. The chat continues by
itself once you're signed in. If Cursor closes without signing in, the card's
Sign in starts it again. Cursor's plan approval (Ready to build?: build here,
build in the cloud, or propose changes), its requests to switch mode, and its
questions are answered from the chat too, and the plan shows in the
conversation.
OpenCode v2 supports streamed replies, reasoning, tool results, image attachments,
questions, permissions, queued prompts, and conversation rewind in Chat. Install
its hooks in Settings > Agents, then open a new OpenCode session to connect it.
The model picker shows the models available from your OpenCode providers and
their reasoning levels; the mode control selects Build or Plan. Left-click a
model to save the default, or right-click to apply it only to this session.
Use `/compact` to summarize the conversation. Forms with conditional fields or
external sign-in steps offer an Open terminal action. Commands:
`ghostex send-session-chat-message`, `ghostex answer-session-chat-prompt`.
Antigravity CLI chats show its replies, its thinking, the commands it runs, the
files it reads and searches, and its file edits with their changes. When
Antigravity asks questions, they appear as a card in the chat: pick options,
write your own answer, or skip a question, and the answer goes to Antigravity
as if you had answered in its terminal. The mode button next to the model pill
switches Antigravity between Default, Accept edits and Plan (Shift+Tab in its
terminal). Commands: `ghostex send-session-chat-message`,
`ghostex answer-session-chat-prompt`.
Sending a message to a sleeping session wakes it. While the agent is still
starting, the message shows in the chat right away and is typed in as soon as
the agent's input box appears. If Claude Code has its settings, a plugin
recommendation, or its background-agents list over the input box, sending closes
it first (a recommendation is declined for now). A question or approval the
agent is waiting on still has to be answered in its card.
ZCode supports chat messages, thinking, tool results, attachments, and imported
conversation history. Install its hooks in Settings > Agents to connect new
conversations and keep activity in sync. ZCode runs in the same terminal, so
you can switch to Terminal for its setup, model menus, and permission prompts.
When ZCode exits, its chat says so and offers Restart ZCode and Open terminal;
Chat View opens for such a session even if ZCode never connected its hooks.
Freebuff supports chat messages, its replies and thinking, tool results, and its
questions: when Freebuff asks a question, it appears as a card in the chat, one
question at a time, and the answer goes to Freebuff as if you had picked it in
its terminal. Freebuff has no hooks, so a new session's chat appears after its
first message; the sidebar still shows it working and done. Its sessions are
named after your first message. Switch to Terminal for sign-in, `/model`, and
Freebuff's other commands and settings. For agents whose models the chat cannot
list, such as Freebuff, the model menu offers Switch model in CLI, which opens
the session's terminal.
Empryo chats show your messages as you typed them (without the repository map
Empryo adds for its model), its replies and thinking, and the tools it runs with
their results, and they can be exported like any other chat. Empryo saves a
reply when the turn ends, and on longer turns sometimes a progress snapshot while
it works, so the chat fills in step by step rather than word by word, with the
working strip shown until the turn ends. One Ghostex session follows one Empryo
tab, the tab of your latest message; Empryo's other tabs show only in its
terminal.
Messages sent from the chat (or with `ghostex send-message`) go into Empryo's
input box. While Empryo is working, a message is queued as its own turn after
the current one, the way Alt+Q queues in Empryo, instead of steering the
running turn. Multi-line messages arrive whole, typed line by line, so Empryo
never picks up an image from your clipboard with them. When Empryo asks a
question, it appears as a card in the chat; pick an option or type your own
answer for its Other row. When Empryo asks for
permission (a web page, files outside the project, a risky command), the card
offers its Allow and Deny choices. A repository whose `.empryo/config.json`
Empryo ignores until you trust it shows a card with a Trust button, which runs
`/trust`; Empryo applies that config after a restart. Cards also explain when
Empryo has no model to use and when its provider rate-limits a turn. Slash
commands sent from the chat show in it with what they printed. Commands that
open an Empryo panel (`/router`, `/models`, `/settings` and the like) show a card
while the panel is open, with Close panel and Terminal View; a message sent from
the chat closes the panel first. The Ghostex phone app opens Empryo sessions in
chat too, with the same cards, and offers Fork and Make Orchestrator on them.
Scrolling up collapses the composer; returning to the bottom expands it.
Settings > Chat > Keep chat box expanded while scrolling leaves the desktop
chat box at full size instead (`sessionChatKeepComposerExpanded`, off by default).
In a short pane, such as one half of a stacked split, the chat box stays
collapsed even at the bottom of the conversation until you click it, and
collapses again when you click elsewhere.
An empty collapsed composer shows only the first placeholder line, and scrolling
keeps the same toolbar buttons visible.
Hex colors (#RRGGBB, #RRGGBBAA, and the short #RGB and #RGBA when they contain a letter, such as #fa0) in messages, inline code, and tables have a small rounded color swatch
beside the value on desktop and web; an all-digit number such as #1234 is not a color and stays plain text (or links to the project's GitHub issue). Copying keeps the original text.
Mermaid diagrams an agent writes (a ```mermaid block) are drawn as diagrams in
the desktop and mobile chats once the block is complete. Source switches to the
diagram's text and Copy copies it. On desktop, the expand button opens a larger
view you can zoom and pan. The web chat shows the diagram's source.

Agents can show charts, tables, stat tiles, and short text layouts right in the
chat, drawn in the chat's own colors on desktop, web, and mobile. For things a
chart can't show, such as a UI mockup or a small interactive tool, the agent
writes an HTML page and the chat shows a card for it. On desktop the card opens
the page in a floating window over the chat, which closes when you click away
or press Escape; its header can open the page in your browser instead. A page
the agent publishes for the browser, and pages on web and mobile, open in the
browser. Agents do this when you ask for `$ghostex-visuals` in your
prompt; they never use it on their own. Install the skill from Settings >
Integrations (Ghostex Visuals) or with `ghostex visual install-skill`. Pages
open in a sandbox and can't reach your computer's files or Ghostex. Commands:
`ghostex show <file.html>`
publishes a page and prints the block that shows its card (`--browser` opens it
in the browser instead of floating), and
`ghostex visual check <file.json>` draws a chart block to an image so the agent
can check it before replying.

Use Cmd+P (Recent Sessions) to jump between chats across projects, or
Cmd+[ and Cmd+] to go back and forward through visited sessions, the same keys
Chrome uses (Ctrl+Alt+Shift+[ and Ctrl+Alt+Shift+] on Windows and Linux).
On Windows and macOS, the mouse Back/Forward buttons follow the same visited
sessions and projects as the header arrows. Inside Settings, they move through
the pages you visited since opening Settings and restore each page's scroll
position. Choosing a new page after going back replaces the forward history.
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
resets; Cursor starts with Context used, Branch, and Lines changed; Hermes
starts with Context used, Cost, Tokens, and Session time; Pi starts with Model,
Context used, Cost, and Tokens (read from Pi's own session record) and can also
show Context tokens, Thinking level, Session time, Output tokens, Repository,
Folder, Branch, and Session title; every other chat
agent (Freebuff, OMP, Grok Build, Antigravity, OpenCode, ZCode) starts with
Repository and Branch, and can also show Folder, Model, and Session title. Reset
to recommended returns to these.
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
typing. On desktop and web, hover the Scroll to bottom button to see your current
shortcut; mobile shows the button without a keyboard shortcut. Both stop any
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
Escape interrupts the agent. While the agent is working, the first Escape shows
"Press Escape again to interrupt" at the bottom of the chat, and a second Escape
within 2 seconds interrupts; Settings > Chat > Press Escape twice to interrupt
turns this off so the first Escape interrupts (`sessionChatConfirmEscapeInterrupt`,
on by default). After an interrupt, a red "Agent was interrupted" notice shows
in the same place for 2 seconds. If a Claude message is cancelled before it is
accepted, its text returns to the chat composer for editing. Rewind to here
also returns the selected text when the message was never accepted, keeping
anything already in the composer (`ghostex interrupt-session-chat <session>`).
Codex rewind continues in a new conversation before the selected prompt and
returns that prompt for editing. If the chat cannot reconnect after the rewind,
choose Retry synchronization in the dialog to reconnect without rewinding again
(`ghostex rewind-session-chat <session> --message-id <message-id>` retries the same pending target).
Sending in a new chat shows your message immediately in the conversation while
Ghostex waits for the agent to be ready. It appears once, with a spinner beside the
bubble's bottom-left corner while it waits; you can retry or remove it if delivery fails.
A message sent while the agent is still working shows a play button there instead:
click it to interrupt the agent (one Escape) so it takes that message right away. Prompts you explicitly
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
Cursor's `/summarize` and Hermes's `/compress` use the same flow. Claude shows its reported progress;
Codex, Cursor, Grok Build, and Hermes show a looping bar. A Hermes `/compress` sent from chat that
finds nothing to compact shows Nothing to compress. Messages sent or queued during compaction
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

To search a chat, press Cmd+F (Ctrl+F on Windows and Linux) with the chat focused, including while typing in the chat box. Every occurrence in the messages is highlighted, the current one more strongly, and the counter shows which match you are on; Enter or the down arrow goes to the next match, Shift+Enter or the up arrow to the previous one, and the chat scrolls to it. Escape or the close button ends the search and returns you to the chat box. A match inside a folded group or a collapsed card marks its row instead. On the phone, Search Conversation in the session's menu opens the same search.

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
on the path or filename to open it in Editor or Files, just like a file reference
pill. Local folder links in desktop chat open your system file explorer; remote
folder links open in this computer's Code view so you can browse the remote files.
File reference pills in the composer also open with one click using the same
Code/Files preferences as transcript links. Double-click a composer pill to edit
its reference text. Right-click a file reference or file-change path for Open in
Code, Open in Files (Markdown, HTML, and Excalidraw), Copy Path, or the location row.
In chat the location row names what the path is (Open File Location, Open Folder
Location, Open Image Location, Open Video Location); elsewhere it reads Open File/Folder
Location. It appears directly below the path-copy actions
in chat, Git changed files, and Files menus, and opens
the location in the machine’s file manager. It requires a local desktop path.
Videos, audio files, and PDFs added to a chat are labelled Video #1, Audio #1, or PDF #1,
and clicking one (or choosing Open Video from its menu) opens it in the system's default
app on macOS, Windows, and Linux instead of the code editor.
Right-click the picture in an opened chat image preview for Copy Image, Copy Path, Save Image,
Reset Zoom (while zoomed in) and Dismiss; right-click around it to close the preview. Click the picture itself to step
through three zoom levels, the last one showing it pixel for pixel, and once more to return
it to the fitted size; the cursor shows whether the next click still zooms. Each step keeps the
spot you clicked under the pointer, and dragging a zoomed picture moves it around.
Disabled Code and Files views are omitted from the menu.
Hosts without an editor copy the path on click.
Click the card background, circle, or change counts to expand or collapse the full diff.
Only clicks directly on the path or filename open the file. The
circle's center turns white on hover. An open code preview and its left rail
also toggle the diff. After expanding or collapsing, the header stays visible;
chat scrolls to it if needed. This covers Claude's Write and Edit tools and Codex's apply_patch
changes.

More actions > View holds the chat's display modes: Simple, Verbose and Summary.
Each one is its own switch, and the View row names the ones that are on.

Simple mode (More actions > View, or Settings > Chat) applies to every chat. Tool groups
without a message above them collapse to a tool-call count, and tool rows hide
command previews; expand a tool to inspect its full input and result. File edits
collapse under "Edited 1 file" or "Edited X files", counting each path once; expand
the row to see the usual file and diff cards. The menu and Settings use the same
toggle (`sessionChatSimpleMode`, on by default). The phone app has Simple mode too,
under More actions > View in its chat box or in its Settings > Chat; the phone keeps
its own switch, also on by default.

Summary mode folds each turn down to your prompt and an "Agent reply" row; the
newest reply stays open, and older ones open with a click. The row holds every
reply the agent gave to that prompt, including the ones it wrote after a
background task finished.
Summary mode has its own button between More actions and Session note when the
chat toolbar has room, and is always under More actions > View.
The button highlights when Summary mode is on; its tooltip shows the Toggle Summary
Mode shortcut (Option+Ctrl+S on macOS, Ctrl+Alt+Shift+S on Windows and Linux), which
switches it from anywhere in that chat and can be changed in Settings > Hotkeys.
As space gets tighter, toolbar buttons move into More actions one at a time:
Summary mode (under View), Session note, Stash prompt, Attach, Maximize, then
Terminal View.
If the context ring still does not fit beside the model, it moves into Model
settings at the top of More actions; the model pill shortens instead of moving.
Controls return as space opens up; More actions and Send or Stop stay visible.
On the phone, Stash prompt with an empty chat box (or a long press on it) opens
Saved prompts for this project: tap one to put it in the chat box, or delete it
with its trash button. Tags and editing stay on the computer.
Click the model pill or the context meter to open it; hovering does not open either control.
Hover the model pill to see the configured Model & Effort Picker shortcut
(Option+P by default on macOS). Hover the context circle to read the agent's
terminal status line.

The chat input row has one model pill. It shows the agent's logo, the model, and
after it the reasoning level, for example "Opus 5.5 High". Claude's Opus 5.5
comes with a 1M or a 200K context window: picking it from another model always
starts on 1M, and the pill only names the window when it is 200K
("Opus 5.5 High · 200K"). Click it to open the model picker: a row of agent tabs
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
F Fast mode, A Account); Tab and Shift+Tab move through the Favorites and agent tabs. Enter applies the highlighted model and level and
saves them as the agent's default, Option+Enter uses them in this session only and
leaves the default alone (either one closes the picker), and Option+1 to Option+9
jump the highlight to one of the first nine rows without applying it. Escape closes it
without changing anything. The key reminder along the bottom lists these.

New Claude Code and Codex sessions start on the model and reasoning level you
last saved as that agent's default, so the pill names it as soon as the chat
opens; a choice for this session only never becomes the default. Cursor CLI has
no session-only choice, so a new Cursor session starts on the model you used
last. Each conversation remembers its own model and reasoning level: one you
wake or resume comes back on what it was last using, including a choice made
for that session only or with `/model` in its terminal, and its pill shows it
as soon as you open it.
If you change an agent's default outside Ghostex (for example with `/model` in
a plain terminal), the next new session starts on that agent's own default and
Ghostex remembers it from then on.

Clicking a model or a reasoning level applies it to this session and saves it as
the agent's default for new sessions. Right-clicking applies it to this session
only and leaves the saved default alone, so new sessions still start where they
did before; waking the session later brings it back on the model you chose.
Right-click session-only picks work for Claude, Codex (version 0.157 or newer)
and OpenCode: other agents' own model pickers always save the choice as the
default, except Hermes, Pi, and OMP, whose picks never change a default (below).

In a Hermes chat the picker has one tab, named after the bot (for example Harry):
the bot's default model first, then the other models its sessions have used, most
used first. Picking a model or a reasoning level (Low, Medium, or High) types
Hermes' own `/model <model> --reasoning <level>` into the session, with
`--provider` added for a model from another provider. Every Hermes
pick applies to that session only and never changes the bot's config. If Hermes
refuses the model, the picker shows its reason. A new Hermes chat gets the
picker once its first message is sent; until then the pill shows the model with
Change it in the CLI, and you switch by typing `/model` in the terminal.

In a Pi or OMP chat the picker lists every model that agent can use right now,
from all the providers you are logged in to, with each model's provider and id
under its name and only the reasoning levels that model supports. Log in to a
provider inside Pi or OMP with `/login` (a ChatGPT Plus or Pro subscription shows
up as `openai-codex`); new models appear in the picker within a couple of minutes.
Picking a model or a level types Pi's own `/model <provider>/<id>` and
`/thinking <level>`, or OMP's `/switch <provider>/<id>:<level>`, into the
session. Every Pi and OMP pick applies to that session only and leaves the
agent's default alone. If the agent refuses a level, the picker shows its reason.
Until Ghostex has read the agent's model list, the pill shows the model with
Change it in the CLI.

In an Empryo chat the picker lists the models `empryo --list-models` shows for
every provider Empryo has ready, grouped by provider and keyed `provider/model`
(for example `subscriptions/gpt-6-luna`). Picking a model opens Empryo's own
`/models` panel in the session and selects that model there, which also makes it
Empryo's default model, as picking it in Empryo would. Picking a level types
`/effort <level>`, which applies to that Empryo tab. Every model offers off, low,
medium, high, xhigh and max; if the model takes fewer, the picker shows the
levels Empryo offers for it. To start an Empryo session on a chosen model, pick
it on Empryo's tab in a new chat, or run
`ghostex create-agent empryo --project-id <id> --model <provider/model> --effort <level>`:
that session starts on the model without changing Empryo's default, the level is
set as soon as Empryo is up, and your first message waits until it is.

On the phone, tapping the model pill opens the same picker as a sheet, without
keyboard shortcuts. Tap a model to highlight it; its reasoning levels appear under
it, and tapping one sets the level. Then tap Use in this session, or Save as
default to also make it the agent's default for new sessions (agents other than
Claude show a single Apply button, which saves the default; in a Hermes, Pi, or
OMP chat it applies to this session only; in an Empryo chat it saves the model as
Empryo's default and sets the level for that tab).
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
A custom agent that brings its own login keeps it: when its command sets
`CLAUDE_CONFIG_DIR` (or `CODEX_HOME` for Codex), or runs your own wrapper
script instead of `claude` or `codex`, Ghostex runs the command as-is and picks
no account for it. The launcher, the New Thread picker and the chat list no
accounts for that agent, Switch Account in the sidebar and terminal bar menus
says "Uses its own login", and Ghostex never moves its sessions to an account,
not even at a usage limit. Keep such a profile in `~/.claude-profiles/<name>`
(or `~/.codex-profiles/<name>`) so Ghostex finds its conversations, then choose
Install hook for that agent in Settings > Agents. Ghostex's skills live in
`~/.claude/skills` (or `~/.codex/skills`); link the profile's `skills` folder
there to use them.
Each provider on Settings > Accounts also lists its account helper, Claude Swap
(cswap) or Codex Swap (xswap), with three icon buttons: Update (shown when a
newer release is out; otherwise a check mark that checks again when clicked),
Reinstall, and Uninstall, which asks first and keeps saved logins and shared
conversations. Hover a button to see the installed and latest versions. Ghostex
runs the update with the tool that installed the helper (uv or pipx for cswap;
Homebrew, Cargo or the Windows installer for xswap); for a helper installed some
other way the buttons are off, so update it the way you installed it.
Before sending a session's first message, use the model menu's Switch Agent CLI
to change between Claude and Codex. The new agent uses its Account for new
sessions rule, just like the sidebar agent button, while the terminal and your
unsent chat text stay in place.
Sign-in and usage-limit notices in Claude and Codex chats offer Switch account
beside Open terminal, so you can choose another account directly from those notices.
In the chat's More actions menu, and in the ⋯ menu of the bar under a Claude or
Codex terminal view, click Switch Account to open its submenu of saved accounts
with their usage; hovering over it does not open it. Open submenus stay open when you move the
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
briefly confirms success, or shows a failure with Retry switch and a close (X)
button in its top-right corner that dismisses it everywhere. On phones and
narrow chat panes the card uses a compact layout with one row of usage pills per
account and a short vertical step list.
A manual switch waits for your next
message without sending anything. An automatic switch sends a "." to continue
the interrupted work once the new account is ready. Configured recovery after
errors can also continue work on the same account.
Whether a session keeps going at a limit comes from Continue automatically and
When the account runs out under the provider's New session defaults in Settings >
Accounts. Both start on: Continue automatically is on and When the account runs
out is Use another account, so a session moves to another account set to
Automatic when it hits a limit; turn Continue automatically off there to stop
that. Every Claude and Codex session follows those settings as they are now,
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
A newly added account starts with Available for automatic switching on; turn it
off in the account's editor in Settings > Accounts to keep that account out of
automatic switching.
When Claude says "Your organization has disabled Claude subscription access",
that account cannot be used any more: Ghostex turns its Available for automatic
switching off, and the next message you send to a session on it first moves the
session to the best account set to Automatic, resuming the same conversation.
With Continue automatically on, Ghostex moves the session by itself and
continues the message that failed. Turn the option back on, reconnect the
account, or pick it for a session when its access works again.
Reconnecting a Codex account works while that account's sessions keep running.
When Codex still has to stop first, Settings lists the sessions in the way and
offers Sleep sessions and continue; sleeping keeps them in the sidebar and they
resume when you open them.

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
Hermes chats offer Context used, Cost, Tokens, Session time, and Model, read
from Hermes itself; Hermes reports no 5h or 7d limits. A Hermes chat also names
the bot it talks to (for example Harry) before the model, and a chat with the
default Hermes profile reads Hermes.
Claude Code, Codex, Cursor, and Hermes keep separate choices. Copying settings
between them is temporarily hidden in this dialog.

Related settings: `hideAccountEmails`, `preferredAgentInterface`, `sessionChatTheme`,
`sessionChatFontFamily`, `sessionChatCustomTranscriptWidthEnabled`,
`sessionChatTranscriptWidthPercent`, `sessionChatVerboseMode`,
`sessionChatFileEditPreviews`,
`sessionChatKeepComposerExpanded`, `sessionChatConfirmEscapeInterrupt`,
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
editor. Sessions and agents on Windows always run with standard user rights, even
when an administrator account connects over SSH, because Codex refuses to run as
administrator. For a single command that needs administrator
rights, turn on `sudo` in Windows Settings > System > For developers and run
`sudo <command>`, then approve the prompt on the Windows desktop.
Native Windows terminals use PowerShell 7 when it is installed (in Program Files,
your user folder, from the Microsoft Store, or anywhere on PATH; a stable release
is preferred over a preview) and Windows PowerShell 5.1 until then, so nothing
breaks on a computer that only has 5.1. When only 5.1 is present, Settings >
General > Terminal shows a **PowerShell 7** row with an **Install PowerShell 7**
button that runs `winget install --id Microsoft.PowerShell --source winget`
(Windows may ask you to allow the installer; without winget the row links to
Microsoft's download page). New terminals and agents use PowerShell 7 right after
it is installed; terminals that are already open keep their shell.
Keys: `windowsTerminalBackend`, `windowsWslDistribution`.

Terminals are embedded Ghostty surfaces. Font, theme, cursor, padding,
scrollback, clipboard, and scrolling are Settings > General > Terminal rows and
are written into a managed Ghostty config; the Ghostty settings actions row
applies the recommended set or opens the raw config. Command-click opens links;
Cmd+V pastes images as previewable links. On Windows and Linux, Ctrl+V or
Ctrl+Shift+V pastes from the client computer's clipboard into the focused
terminal, including a remote terminal. Shift+Insert also pastes, in terminals
and in text fields such as the chat composer and dialogs. A configured hotkey
using the same chord takes precedence. In an agent's terminal prompt, use
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

Codex 0.157 and later draw the conversation full screen by default, so its
terminal keeps no scrollback of its own. The mouse wheel, PageUp and PageDown
scroll Codex's own view, dragging selects text inside Codex (Codex copies it when
you let go), Shift+drag makes a normal terminal selection of what is on screen,
and Command-click still opens links in Ghostex. Stop in the chat works even while
you are scrolled up or have text selected. To go back to the terminal's own
scrollback, type `/tui` in Codex, choose Scrollback, then restart the session
(Sleep and Wake it, or Full Reload).

Terminals follow the app theme by default. The Theme page in Settings holds
Appearance, and its Advanced part holds Chat theme and Terminal theme.
Terminal theme can override the app with Light, Dark, or System. The palette
selectors show your existing Ghostty theme names, including separate light and
dark selections when configured. A single Ghostty theme is used for both appearances
unless you select a separate light palette. Without a configured theme, the defaults
are GitHub Light and GitHub Dark. All open terminals refresh automatically when their
app, system, or terminal theme changes, including idle terminals and terminals in
inactive projects. The appearance override and light palette apply to Ghostex only.

Terminal background (Settings, Terminal) is Black / white by default: pure black
behind dark terminals and pure white behind light ones. Choose Follow theme to use
the theme's background color instead, or Custom color to pick your own for dark mode.

On macOS, Custom shaders (experimental) runs the `custom-shader` files from your
Ghostty config over Ghostex's terminal panes, in the order the config lists them.
Turn on Enable Experimental Features (Settings, Advanced) to see it at the bottom of
the Terminal section. While it is on, terminals use the background color and
opacity from your Ghostty config, so Terminal background and its color are greyed
out. Shader animation follows Ghostty's `custom-shader-animation`. Turn the switch
off to go back to normal rendering; running sessions keep going. If a shader file
does not compile, terminals keep their normal look. Windows and Linux are not
supported yet.

Terminal links (`ghostex://terminal`) without a folder open in the active local
project. A folder supplied in the link takes precedence.

Related settings: `terminalFontFamily`, `terminalFontSize`,
`terminalGhosttyTheme`, `terminalColorScheme`, `terminalGhosttyLightTheme`,
`terminalBackgroundMode`, `workspaceBackgroundColor`, `terminalCursorStyle`, `terminalPane*PaddingPx`,
`terminalScrollbackLimitMb`, `terminalCopyOnSelect`, `promptEditorBackend`,
`terminalDevServerDetectionEnabled`, `terminalShadersEnabled`.

## Agents, actions, and orchestration

Agents are the launch buttons per project: Claude Code, Codex, Gemini CLI,
OpenCode, Pi, and more are built in. In Settings > Agents every agent has a
switch: turn on the agents you use and they appear in the New session menu, the
sidebar's Select Agent and on your phone; drag the rows to set their order.
Turning an agent off keeps its settings and its place, so turning it back on
restores it. Agents you turned off but used before stay dimmed in the list;
agents you never used wait under More agents, where one click turns one on.
Turning on an agent whose CLI is missing offers its install right in its row;
when the CLI is already there, Ghostex asks once whether to turn on its session
resume hook (Install the hook when I turn on an agent, in the Session resume
hooks card, skips the question). An agent that is on shows on its own row when
its CLI is not installed or its resume hook is off, with the button that fixes
it; for the agents you have used, a line above the list sums these up, with Fix
all. An available CLI update shows as Update available on its row. The first time
you open the page after using Ghostex for a while, it offers once to turn off
the built-in agents you never used; nothing turns off by itself. Add custom
agent, at the end of the list, adds your own command or a variant of a built-in
agent: Works like gives it that agent's logo, chat view, resume hook and
permission handling. Custom agents can be turned off, or deleted from their
expanded row; built-in agents are only turned off. Expand an agent row to edit
its name, command, permission mode and default view, duplicate it as a custom
agent, install or update its CLI, see its installed version (and the newer one
when available) and command output, or open its Install docs link. Claude Code, Codex, Cursor Agent
and Grok Build install through their official installers (PowerShell on
Windows). When an installer leaves its folder off PATH (Claude Code's does), Ghostex
adds it to your user PATH so new terminals find the command; Add to PATH
does the same for a CLI installed earlier. Installs run one at a time and wait
their turn. Ghostex selects an updater for recognized installations; choose the
original installation method when it cannot be detected. mise is offered for supported CLIs and is the
default install choice when it can install that CLI on your computer; otherwise
(Cursor on Windows, or a CLI mise installs with npm when npm is missing) the
official installer or npm is. When Chat is your default view, installing a CLI
here also installs the Ghostex hooks its chat needs. Existing mise tools, including custom
backends, update through mise with their version pins bumped to the latest
release; older versions remain available for running sessions. For example,
ZCode can also be installed with `mise use --global 'npm:zcode-app-cli[prerelease=true]@latest'`.
Install and update commands run on the
selected computer and keep running if Settings closes. From a terminal,
`ghostex agent-cli status [agent]`, `ghostex agent-cli install <agent>`,
`ghostex agent-cli update <agent>` and `ghostex agent-cli add-to-path <agent>`
do the same. Start a new session to use the installed version. ZCode launches with `zcode`; install
and update it with `npm install -g zcode-app-cli@latest`, as documented at
[the ZCode installation docs](https://github.com/kingsword09/zcode-cli).
Freebuff launches with `freebuff` and installs with `npm install -g freebuff`;
sign in once in its terminal the first time it starts.
Empryo launches with `empryo` (`em` works too) and installs with
`curl -fsSL https://empryo.com/install.sh | bash`, which puts it in
`~/.empryo/bin` (on Windows, `irm https://empryo.com/install.ps1 | iex` in
PowerShell). Ghostex installs its hooks into Empryo's `hooks.json` (in `~/.empryo`,
or `%LOCALAPPDATA%\Empryo` on Windows) and never touches Empryo's `config.json`,
so an Empryo session shows working and idle and comes back after a restart with
`empryo --session <id>`. On Windows without WSL, Empryo's chat, fork, picker and
orchestrator work, but its sidebar status does not follow its hooks yet. Empryo has
no launch option to accept every approval, so Agent approvals does not apply to it;
Empryo's own `/yolo` does that instead.
Agent Hooks let gxserver watch agent status, questions, and
completions for chat and notifications. Installing the Claude Code hooks also
sets Claude Code's transcript retention (`cleanupPeriodDays`) so past
conversations stay on disk instead of being deleted after 30 days; a value you
set yourself is left unchanged. Installing the Claude Code or Cursor hooks also
registers a Ghostex status line command for that agent, which still runs your
own status line script so the terminal footer looks the same; it is what feeds
the chat's status line and More details (for Cursor: context use, output tokens,
version, Max Mode, auto-run, worktree, plus the branch, lines changed, and pull
request). Removing the hooks restores your own command. The Hermes Agent hooks
cover every Hermes profile as well as the default one, so each bot's sessions
report their status and fill their chat; a profile you add later gets them the
next time Ghostex starts.
Agent approvals ("accept all") is a
per-machine default with per-project overrides. Actions (Settings > Actions)
are saved terminal commands or browser URLs shown on project headers and in
the header’s Quick Actions button, which shows the name and icon of the Action
you used last and runs it again on click (its caret opens the full list; it
reads Start until an Action exists); Global Actions apply to every project.
Actions is a built-in extension, off by default: turn it on with its switch in
Settings > Extensions (Features). While it is off the Start button, the Actions
page in Settings, Actions pinned to project rows, the Start Action hotkeys and
Quick Access's Action rows are gone, and `ghostex run-action`, `run-command`,
`save-command` and `click-button command` say Actions is turned off. Your saved
Actions are kept and come back when you turn it on (`actionsHidden`).

Agents Hub lets you browse and edit agent files in Skills, MDs, Hooks,
Configs & MCPs, and Agent Sync. In MDs, expand Shared agent markdown to see the
files in your shared agent folder, then select a filename to read or edit it.
Expand the profile instruction groups the same way. Use Refresh to reload files
from disk and Save (Cmd+S) to write your edits. Cmd+1 to Cmd+5 switch tabs.

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
and reply reference automatically. A normal send reaches a busy agent at its
next input boundary and wakes a sleeping one, so it is right for almost every
message. Use `--body-file` for multiline messages, `--interrupt` for an urgent
correction, or `--queue` to leave the message waiting until the current turn
finishes. A queued message waits as long as that turn does, so send normally
unless the point is to have the next task ready for an agent whose final
message you have already read. A send reports `delivered` once the recipient's
transcript shows the message, `accepted` when the agent took it but has not
recorded it yet (usually because it is busy), `pending` while a waking agent is
still starting, and fails with the reason when the agent did not take it. If Ghostex cannot deliver
a queued message, the row stays in the recipient's queue marked Not delivered
with Retry and Delete, and the sending agent gets a note saying so. `agents close
<session-ref>` ends that session, including any unfinished work. `ghostex read-session-chat` and `ghostex read-text` read replies.
In the chat, a message another agent sent shows as a "Message from" card with
its first two lines; click it to read the rest. A message the session's own
agent sent shows as a "Message to" card, closed until you click it.
An agent can also read or search any other thread, including a sleeping one:
`ghostex read-session-chat <session> --all --format text` prints the whole
conversation, and `--grep "<words>" --context 1` finds where a topic came up.
`<session>` can be any id from the sidebar's Copy Details (right-click a session,
Copy > Copy Details; its Global Ref is the most precise), or the title. The same
Copy menu also copies the session's branch, its Linear issue ID and link, and its
pull request or GitHub issue link when it has them.
Copy Details lists the session's agent, title, Global Ref, agent session id,
zmx name and project, and ends with a pointer to `$ghostex-agents`: paste it
into another agent's chat and mention that skill, and the agent can message or
read that session. `ghostex sessions --json` prints the same short facts for
every session, plus its project id and status; add `--full` for everything
Ghostex knows about each one.
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
   to `create-agent` or `board start-work` (Claude, Codex and Pi; for Pi the
   model is `<provider>/<model>`, as in Pi's `/model`, and the effort is its
   thinking level, such as `high`). The choice
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

Related settings: Settings > Agents (agent switches, Add custom agent, Default
view per agent; Defaults: Default Prompt Agent, Title Generation Agent, Agent
approvals; Session resume hooks), `agentAcceptAllEnabled`,
`agentHooksAutoInstall`, `showQuickModelPickerInTerminal` (Option+P model
picker).

## Orchestrators

An orchestrator is one agent you talk to about a stream of work in a project. You
tell it what needs doing; it answers quick questions itself and hands every real
task to a thread, which is an ordinary agent session it starts and briefs, so it
stays free to talk to you while the threads work in parallel. It is Ghostex's
version of the Projects features in Cursor and Claude Code.

- **Start one**: hover a project in the sidebar, open the Select Agent menu (the
  arrow beside its agent button) and choose **New Orchestrator…**, the first
  item of that menu. On the phone, open the same menu from the project's agent
  button and pick **New Orchestrator…** at its top. Name it, pick
  Claude, Codex, ZCode or Empryo and its model (Opus 5.5 on Claude, GLM 5.3 Flash on
  ZCode; without a choice each agent starts on its own default) and the effort
  the model takes — medium by default, which is plenty for routing work — and
  optionally give it a one-line goal and a first request;
  it opens in chat. The name is what its sidebar row shows, and the
  orchestrator keeps it: unlike other sessions it is not renamed from your
  first message (Rename in the sidebar still changes it; left blank, it is
  named once from its first conversation). A project can have several
  orchestrators, one per stream of work.
- **Turn a session into an orchestrator**: right-click a Claude, Codex, ZCode or
  Empryo session in the sidebar, open **Advanced** and choose **Make Orchestrator**
  (optionally with a goal); on the phone it is in the session's menu too.
  The session keeps its conversation and is never restarted or interrupted:
  it gets the crown right away, and the orchestrator playbook waits in its
  chat queue until its current turn is over. The next time the session
  starts again on its own (waking from sleep, a Full Reload, an app
  restart) it runs with the orchestrator role built in, like one made with
  New Orchestrator. Sessions it started before are not its threads yet; ask
  it to adopt them. A thread of another orchestrator, a session in a box, or
  a draft cannot be made an orchestrator.
- **Threads in the sidebar**: an orchestrator's row shows a crown in
  place of its agent's logo, and a crew icon
  with one number: how many of its threads are working; when none are
  working, how many are waiting on you; when neither, how many threads it
  lists. The icon and number are orange when the number counts
  working threads, light blue when it counts threads waiting on you, and grey
  otherwise. Its
  threads sit indented right under it with their own status dots: the ones
  working, waiting, or active in the last 2 hours. Older threads wait behind an
  "N older threads" row at the end, which lists them all (and "Hide older
  threads" tucks them away again; both remembered across restarts). The chevron
  that replaces the crown when you hover the orchestrator folds its threads away
  and back, and a folded orchestrator keeps its number and colour. Opening a
  orchestrator's chat leaves both as you set them; opening one of its threads
  unfolds the orchestrator so the thread shows. Click a thread to watch it or
  talk to it directly; answer its questions and approvals there. A thread keeps
  the name its orchestrator gave it, like the orchestrator keeps its own (Rename in
  the sidebar still changes it). Pinning the
  orchestrator takes its threads along. The phone's session list shows the same
  tree: crown, crew count, threads indented under their orchestrator, and the
  chevron to fold them.
- **Finished threads are closed**: once the orchestrator has checked and
  committed a thread's work and expects nothing more from it, it marks the
  thread done and closes its session, so the sidebar keeps only work in flight
  (it never closes a thread that is still working or waiting on you). A closed
  thread is not lost: when a follow-up comes, the orchestrator reopens it or
  messages it, and the same conversation resumes under the orchestrator with
  everything it knew. Ask the orchestrator to keep a thread open if you want to
  look at it.
- **Threads panel in the chat**: above the orchestrator's message box, the
  Threads panel lists the working threads first, then the ones active in the
  last 2 hours, each with one line (what it is doing or how its last report
  began) and its branch; "N more" lists every other thread, closed ones
  included, and "Show fewer" folds them again. Tap or click any thread to open
  it and talk to it. A thread stuck on something only you can allow (a
  permission prompt or a folder-trust question) carries an amber "Needs your
  approval" tag. Fold the panel with its header and it stays folded the next
  time you open an orchestrator's chat. It shows on the phone and in the browser
  too.
- **Reports come back by themselves**: when a thread finishes a turn, Ghostex
  sends its final message to the orchestrator (a "Message from" card in its
  chat); when a thread waits on a question, an approval, or a screen such as
  folder trust or an expired login, the orchestrator is told what it is asking.
  The orchestrator then checks the work, commits it (only that thread's files,
  and it never pushes unless you ask), starts the next step, and tells you
  what needs you. Nobody has to poll.
- **Work that never arrives is caught**: when the orchestrator starts a thread,
  it waits until the thread has actually taken its brief before saying it is
  under way, and says "pending" with the reason when it has not yet. Ghostex
  keeps watching every brief and follow-up the orchestrator sends: if a thread
  sits idle without it, the orchestrator is told the message did not reach it
  and sends it again, so work handed out is never silently dropped.
- **Thread models**: the orchestrator picks each Claude thread's model when it
  starts it: Opus 5.5 at high effort for substantial work, Opus 5.5 at medium
  for hard but small changes, Sonnet 5.5 at high for small contained fixes. It
  never switches a running thread's model (that throws away its prompt cache);
  a follow-up that needs a stronger model gets a new thread.
- **Worktrees**: threads that need the same files run one after another; when
  running them in parallel matters, the orchestrator gives a thread its own git
  worktree and branch, so parallel threads never edit the same checkout. An orchestrator's
  threads trust the project's own folder and the worktrees Ghostex makes for
  them, so they start without stopping at the agent's folder-trust question.
- **Goal, standing instructions and memory**: every thread's brief carries the
  orchestrator's goal, its standing instructions (rules such as which branch to
  target or how to verify work) and its memory notes. When you state a lasting
  preference, the orchestrator proposes the exact wording and saves it as a
  note every later thread receives once you confirm; ask it
  to change the goal or the instructions the same way.
- **Just ask**: "run these three as separate threads", "use worktrees", "give me
  a status of every thread", "use a cheaper model for threads", "don't merge
  anything without asking" all work in plain words.

Orchestrators run on Claude, Codex, ZCode or Empryo; threads can be any configured
agent. An Empryo orchestrator runs as the Empryo agent `ghostex-coordinator`: Ghostex
keeps its playbook in `~/.empryo/agents/ghostex-coordinator.md` and switches the
session to it with `/agent ghostex-coordinator` before anything else, so leave that
file in place. From a terminal or another agent: `ghostex orchestrator create --title <name>
[--model <m>] [--effort <e>] [--goal <text>] [--task <first request>]`, `ghostex orchestrator
promote <session> [--goal <text>]` (make an existing session an orchestrator), `ghostex orchestrator status`,
`ghostex orchestrator options` (the agents, models and efforts an orchestrator can use),
`ghostex orchestrator start-thread --title <title> --task <brief> [--worktree]
[--agent <id>] [--model <m>] [--effort <e>]`, `ghostex orchestrator resolve
<thread> [--keep-open]` (mark done and close its session), `ghostex orchestrator
reopen <thread>` (resume a closed thread), `ghostex orchestrator remember <note>`, `ghostex
orchestrator set-goal|set-instructions`, and `ghostex orchestrator guide` (the
orchestrator's own playbook). `ghostex orchestrator --help` lists every flag. Older
Ghostex releases call the orchestrator a coordinator and name the same verbs
`ghostex coordinator …`; that name still works.

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

## Bots (Hermes agents)

Bots switches the sidebar to your Hermes agents, one row per Hermes profile.
It is off by default: turn it on in Settings > Extensions, under Planning and
automation. The Bots card only appears on a computer where the Hermes CLI
(`hermes`) is installed, and with Bots off the sidebar looks and works as
before.

- **Switching lists**: a Hermes button sits in the last slot of the Space row
  when Spaces are on, or in the sidebar's top row when they are off. Click it
  to show your bots; while they are showing it turns into a back arrow (Show
  projects) that returns to your projects. Ghostex reopens whichever list you
  left open.
- **One row per profile**: the default profile shows as Hermes and every
  folder in `~/.hermes/profiles` under its own name. They are found
  automatically, and a profile you add shows up the next time you open Bots.
  A profile folder you already added as an ordinary project stays a project
  and gets no bot row. Bots never appear in the Projects list, belong to no
  Space, and can be dragged to reorder them among themselves.
- **The bot row**: a yellow letter tile and a dot for the bot's Hermes gateway,
  green while it runs and grey when it is stopped (it catches up within a
  minute). A bot is not a repository, so git stats and the worktree, pull
  request, history, browser, and terminal buttons are left off.
- **New sessions**: the row's **+** starts a new session with that bot, running
  `hermes -p <profile>` in the profile folder (`hermes -p default` for the
  default profile), in chat or terminal as your agent interface setting says. There is
  no agent, model, or terminal picker. A bot with no sessions shows a New
  Session row that does the same. A new session is titled after the bot until
  Hermes names it.
- **Sessions**: a bot's sessions sit under it in the usual Sessions and Parked
  sections, and take the usual session menu: park, rename, tag, sleep, and
  close. They open in chat like any Hermes session.
- **Edit SOUL and Edit config**: two buttons on every bot row that open that
  bot's own `SOUL.md` and `config.yaml` in the Code view. They always point at
  that bot's files and cannot be edited or removed. A bot on another computer
  has neither, and the browser build shows a message instead, since it has no
  Code view.
- **Actions and Open in**: Actions you pin to project rows (Settings >
  Actions) show on bot rows too. Right-click a bot for Open in, which opens the
  profile folder in any of your Open In targets (VS Code, Cursor, Finder, and
  so on).
- Hermes conversations you started outside Ghostex are listed under their bot
  in Quick Access > Sessions (see Sessions), and the Hermes Agent hooks cover
  every profile (see Agents, actions, and orchestration).

Setting: `botsHidden` (on by default, which keeps Bots hidden).

**Bot automations** is a second switch in Settings > Extensions, off by default
and available only while Bots is on. It adds an Automations row at the top of
the Bots sidebar that shows how many runs landed today; click it to open a feed
of every Hermes cron run in the view panel, and click it again to close the
feed. The feed has one `#channel` per cron job across every profile, plus
`#all` for every run at once; a job with no delivered run yet (paused, never
run, or silent every time) has no channel. To read several jobs together the
way a Discord channel does, click + New group under the groups and type a name:
the group shows every run of its jobs in time order. Right-click a job's
channel and choose Move to group to put it in one (a job sits in one group at
a time, listed under it), or Remove from group to take it out; right-click a
group to rename or delete it (its jobs go back to the list), and click its
arrow to collapse it. A group shows how many jobs it holds, and each group and
channel shows how many runs arrived since you last opened it; `#all` never
clears those counts. Job names come from Hermes, so a job's channel can't be
renamed.
Right-click `#all` to hide it, and right-click the Feeds title to show it
again; with it hidden the feed opens on the first channel. The grouping lives
in Ghostex only and changes nothing in Hermes or Discord. Each run is a message
showing the bot, the time and the job's output, newest at the bottom; runs that
stayed silent are left out, a run that failed is marked failed, and new runs
appear while the feed is open. The bot menu at the top narrows the channels and
`#all` to one bot. Newest activity lists the channels that ran most recently
first; Manual lets you drag channels into your own order within their section,
and new jobs land at the bottom. The feed remembers the bot, the sort, your
order, your groups, which groups are collapsed, whether `#all` is hidden and
what you have read. It is read-only and needs the Ghostex app, not the browser
build.

Setting: `botAutomationsHidden` (on by default, which keeps the feed hidden).

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
  To find a session on the phone, tap the search button at the top right of the
  Sessions list and type part of its name or tag; it searches the open sessions
  of every connected computer, newest first, and tapping a result opens it.
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
  and HTML files from the same folders the desktop Files view shows, with search
  and the most recently changed files on top. Files open in a reader on the
  phone, and Reload picks up an agent's latest edit. HTML pages include the
  Agentation annotation tool (the pen button hides it); its copy button puts
  your notes on the phone's clipboard, ready to paste into a session.
  Tapping a link to a file on the computer in a chat or terminal opens it in
  the same reader: Markdown, HTML, text and code files, and pictures (other
  files show their path, copied, since the phone cannot preview them).
- **From another computer**: Settings > Remote > Remote machines > Add a
  machine with SSH details or an Easy Connect code, then Install / Connect
  gxserver on it. The machine appears as a sidebar section with its own
  projects and sessions; its terminals stream into the desktop app. Windows,
  Linux, and macOS clients use the connected computer's folders and shell.
  Files and folders you drop onto a remote terminal or add with its attach
  (paperclip) button are uploaded to that computer first, so the terminal gets
  references the agent there can open.
  Open Code from the view panel's + menu to edit the remote project; if prompted,
  install the editor component first. Folder links in remote chats also browse
  the remote folder in Code.
  Remote localhost links open in the built-in Browser through that computer,
  even when ordinary web links are set to open in your external browser.
  When a machine cannot connect, its tab's cloud turns red and its list says
  why, with Reconnect and Remote Settings buttons. Ghostex keeps retrying a
  dropped connection, but a rejected username or password is not retried (so
  the account is not locked out); saving a new SSH password reconnects it.
- **Web app**: the desktop's sidebar (with its session, group and project
  actions, the Git menu and Quick Access), chat and terminal running in a
  browser and talking to gxserver; remote machines, Settings and the commit
  review stay in the desktop app. It is built from a Ghostex source checkout
  with `cargo xtask start-web` and is not part of the installed app.
- **CLI**: `ghostex attach <selector>` attaches to a session from any terminal,
  including over SSH.

Windows computers accept Android, macOS, and Linux desktop connections over SSH.
Install Ghostex on Windows, enable SSH, and add the Windows address with your
Windows username and the Windows account password (a Microsoft account's
password when you sign in with one; SSH never accepts the Windows Hello PIN).
Turning SSH on in Windows Ghostex installs and starts Windows' OpenSSH Server
feature after one administrator prompt; if Windows needs a restart to finish,
or another SSH server is already installed, the message under the button says
what to do. On a Windows administrator account, pairing a phone with Easy
Connect (and removing it from Paired devices) shows one more administrator
prompt on the computer, because Windows' SSH reads administrators' keys from
`C:\ProgramData\ssh\administrators_authorized_keys`; choose Yes. If the prompt
is declined the phone says so: scan the new code and choose Yes. When a connection needs Ghostex's background service on Windows
and you are signed in to that computer's desktop, the service starts in your
desktop session, so agents there work as if you had opened Ghostex yourself.
When nobody is signed in, it starts in the background as before, and opening
Ghostex on the desktop later replaces it with one started from the desktop. The administrator prompt can only appear when the background service
runs in your desktop session; if Ghostex says it cannot show it, quit Ghostex
together with its background service and open it again from the Start menu.
Leave Advanced > Windows WSL distribution blank to use the
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

## Cloud Boxes (agents in a box, here or in the cloud)

A box is an isolated copy of your project where an agent works without touching
this computer. Boxes run on this computer with Docker (free), in the cloud on
Hetzner, Vercel, Daytona, E2B or DigitalOcean, or on your own server over SSH.
Ghostex drives agentbox, a free open-source command line tool
(https://github.com/madarco/agentbox); Claude, Codex, OpenCode and Pi can run
in a box. Your agent's settings, skills and Codex sign-in go with it. Claude
needs its own one-time sign-in for boxes, so Claude on this computer stays
signed in.

Cloud Boxes is a built-in extension, off by default and only on macOS and
Linux: turn it on with its switch in Settings > Extensions (Features). While it
is off the Cloud Boxes page in Settings, the Run on choice in New Thread and in
a new chat thread, Run in a Box in the Select Agent menu and the
`ghostex agentbox` commands are gone (they say Cloud Boxes is turned off), and
new threads run on this computer. Your boxes, provider logins and default
location are kept and come back when you turn it on (`cloudBoxesHidden`).

- **Set up**: Settings > Cloud Boxes. "Set It Up for Me" starts an agent that
  installs agentbox, asks which clouds you want before anything that costs
  money, creates the provider API token in your browser, and signs Claude and
  Codex in for boxes. To do it by hand: Install agentbox, then Set Up for
  Docker, or Log In (paste an API token from the provider's console) and
  Prepare (once, builds the base image) for a cloud. Add Server registers your
  own server by a name and its SSH address; Check tests it. Claude in boxes >
  Sign In opens the sign-in page. Each step runs in a terminal tab; the page
  updates while it runs. Run Check shows agentbox's own health check.
- **Use**: pick a box under Run on in New Thread (Cmd+Left and Cmd+Right switch
  location), or choose Run in a Box in a project's Select Agent menu. A new
  chat thread also shows a Run on row above its message box until you send the
  first message: pick This computer or a box there, and the box starts with
  that first message. Box sessions open in the terminal view and show a badge with where they run. The
  first Claude box asks you to sign in right in its terminal: approve in the
  browser page that opens and paste the code. Right-click a box session for Open
  Box Web App (the app it serves, opened on this computer), Open Box Screen (the
  box's own browser), Stop Box and Destroy Box. Add an `agentbox.yaml` with
  `services.web.expose.port` to your project to start your dev server in the box
  automatically. Sleeping or waking a session, or restarting Ghostex, reconnects
  to the same box and conversation.
- **Cost**: cloud boxes bill while they exist. Closing or deleting a session
  stops its box and keeps its work; Settings > Cloud Boxes > Your boxes lists
  every box with Open Web App, Stop and Destroy (deletes the box for good).
- Boxes run on macOS and Linux; on Windows, use Ghostex inside WSL.

Related settings: `cloudBoxesHidden` (turns the feature off; default on, so
Cloud Boxes starts off), `agentboxDefaultLocation` (where new threads run unless you
pick another location; default `local`, this computer). CLI:
`ghostex agentbox status`, `ghostex agentbox list`,
`ghostex create-agent <agent> --project-id <id> --run-on docker`.

## Notifications and status

Ghostex tells you when an agent needs you: a completion sound when a session
finishes, an attention state on the session card, system notifications
(macOS banners, Windows toasts, and Linux desktop notifications; clicking one on
macOS or Windows opens the session), menu bar badges with running and done counts (click one to jump to the
session), terminal bell detection, and push notifications on the mobile app.
The optional status pet in the sidebar mirrors session state.
Claude progress updates do not trigger completion notifications while Claude
reports background work still running. Completion notifications arrive when
Claude finishes after that work completes; requests for your input or permission
still get your attention.
Pi, OMP and Amp sessions ring the same way: a finished turn marks the session
done (for Pi and OMP after any automatic retry or queued follow-up), stopping it
with Esc or a provider error does not, and a question or confirmation a Pi or OMP
extension asks marks the session as waiting for you.
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
you previously moved to the back of the unread queue. It works the other way
too: marking a notification read, or using Mark all read, clears the
finished or needs-input mark on its session in the sidebar.
Scripts and agent hooks can post their own rows with
`ghostex notify --title <text> [--body <text>]`.
A banked Claude or Codex usage reset that is about to expire shows as a red
row; it opens that account's usage dropdown instead of a session.

Related settings: `completionSound`, `actionCompletionSound`, `copySound`,
`showMacOSAttentionNotifications`, `resetExpirySystemNotifications`,
`showNotificationOnTerminalBell`,
`hideMenuBarSessionStatusIndicators`, `petOverlayEnabled`,
`notificationsTitlebarButtonHidden`.

### Floating Capture (floating button, screenshots, quick prompts)

Floating Capture puts a small Ghostex button over every app, so you can check
on your agents and send them a prompt without switching to Ghostex. Turn it on
at the top of Settings > Integrations > Floating Capture, or during setup
(off by default). Beside the icon, a narrow dark column shows how many
sessions are waiting for you (blue), asking a question (pink) and working
(amber), stacked in that order, across this computer and your remote
machines; only non-zero numbers are shown. Drag the button anywhere; drop it
against the left or right screen edge and it tucks away as a thin tab with the
numbers stacked, sliding out when you hover it. Its spot is remembered per
screen.

Click the button, or press Cmd+Ctrl+Shift+S (Alt+Ctrl+Shift+S on Windows and
Linux), to open its panel: Capture Area (A), Capture App (Space), Capture Screen (F)
and Write Prompt (T), over the same
Running Agents list as the menu bar dropdown, with Open Ghostex, Hide button,
Restart and Quit. Projects in the list start collapsed: click one to see its
sessions (click a session to open it in Ghostex), or type in the filter above
the list to find a project or session by name. Drag the empty part of the
Running Agents heading to move the panel; it opens there from then on. Use the same
modifiers with A, Space, F or T instead of S to run an action straight away,
from any app. "Hide button" keeps it hidden until you turn the setting on
again or press the S hotkey.

Screenshots hide the Floating Capture windows first. An area capture dims every
screen and starts with a resizable box where your last area capture was; press
Enter or A to capture it, or drag outside it to select another area on any
screen; the current app captures what is on screen inside the window of the app you
were using; full screen takes the screen under the mouse. Every capture opens a
small editor with Crop selected: drag a box to crop and Enter to apply (Enter
with no box moves on to marking up). With the pointer (V), a dot in the middle of
each side of the picture crops it from that side when you drag it. Then add
arrows (A), text (T) and rectangles (R), move and resize them with the pointer,
undo with Cmd+Z, and copy the picture with Cmd+C (Ctrl+C). The toolbar's
icons name their tool and key when you hover them; a text label edits like any
text box, and the background button puts a dark box behind it. Scroll the
mouse wheel or pinch the trackpad to zoom, pan with two fingers or by holding
Space and dragging, and press Cmd+0 (Ctrl+0) to fit the picture again. Drag the empty
part of the editor's toolbar or the prompt box's header to move them, and drag
the editor's edges to resize it; each opens where and at the size you last left
it. Enter adds the picture to the floating
prompt box as `[Image #N]`. Each original and edited picture is saved to
~/Documents/Screenshots on macOS and the Screenshots folder in Pictures on
Windows and Linux.

The prompt box sends to a new session in the project you last sent to (or the
most recently active one), or to any project or running session you pick from
its list, which follows the sidebar's order. It opens where you last left it, or
in the middle of the screen under the mouse, and stays there while you take
screenshots. Add more screenshots with its Area, App and Screen icons or the
hotkeys, and click a picture's thumbnail to edit it again, then press Enter.
Its Send to list starts with every project collapsed: click a project to see
its sessions, or type to filter, and Enter picks the first match. A note under the button says where
the prompt went, with Open. The Ghostex window also switches to that session, without
coming in front of the app you are in; turn this off with "Switch to the session
after sending" under Floating Capture in Settings > Integrations. Closing the prompt box without sending keeps it as
a draft session in that project, visible in the sidebar; "Write Prompt"
starts a new prompt, and the small continue button on it brings the draft
back. On macOS, screenshots need Screen Recording
permission for Ghostex. Linux support is X11 only.

Related settings: `ghostexCaptureEnabled`, `ghostexCaptureSwitchToSession`.

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
choose a destination (on a Windows PowerShell machine the folder list starts at
your drives), and review before Clone & Add. While it clones, the dialog shows
Git's progress and a Cancel clone link.

Project headers show the branch and diff stats; the header’s Commit (Git) menu offers
commit, sync with main, PR review by a prompt agent, and related actions with
persistent running toasts. Add Worktree on a project header creates a git
worktree as its own project so a second agent works on a branch without
touching the main checkout; worktrees can be renamed, merged back, and
deleted from the sidebar.

Work mode links a project's sessions to the work they belong to. It comes with
Workspaces: turn Workspaces on in Settings > Extensions (Features) first; while
it is off no project is in work mode and sessions show no work links. It is on by
default for projects in a Work workspace and off in a Personal one; right-click
a project and choose Work Mode, turn on **Work mode** for the project in Settings >
Projects, or run `ghostex work-mode on` in its folder, to set it yourself, and a project keeps a choice you made when it moves to another
workspace (otherwise it takes the new workspace's default). In a
work-mode project, a session linked to a GitHub pull request, a GitHub issue, a
Linear issue or a Linear project gets a second line on its card: the PR with its
state and checks, the issue with its status, and the Linear project. Click a PR or
issue chip to open its details in the Work view (see Views), or the Linear project
chip to open it in the browser. Links come from the session's branch on their own (a branch named like
`yahia/spx-1245-copy-link` or `yahia/218-arabic-plan-cards`), from the PR Linear
attached to the issue, or by hand: right-click the session, choose Link to, then
Pull request…, Linear issue…, Linear project… or GitHub issue…, and pick from the
list (the session's own repo comes first; type to search, Enter links it). A
session can link several Linear issues shipped in one PR; tick them in the list.
Something already linked shows its ID in the menu, Unlink removes it, and Back to
automatic lets the branch decide again. Each workspace picks one **Primary
tracker** in Settings > Workspaces: Linear tickets & projects, or GitHub issues &
projects (`ghostex work-mode tracker linear|github`; a workspace that never
picked uses Linear when it has a Linear key, otherwise GitHub, and a team's
owners pick it for the whole team). With GitHub, Link to offers GitHub issue…
and GitHub project… instead of the Linear ones, and the card shows the GitHub
Project the issue or PR is on (with its Status) instead of a Linear project;
`ghostex link-session <session> --github-project <owner>/<number>` (or `none`)
sets it by hand. Reading GitHub Projects needs one more `gh` permission: run
`gh auth refresh -s read:project` (Settings > Workspaces shows it while it is
missing). From a terminal, `ghostex link-session
<session> --pr 6538 --linear SPX-1245 --issue 218` does the same and `--auto`
goes back to what the branch says; `--candidates linearIssue --query text` (or
`pullRequest`, `linearProject`, `githubIssue`, `githubProject`) lists what the Link
to list would suggest. The phone's session menu has the same Link to, with the
GitHub rows in a workspace whose primary tracker is GitHub. When a linked PR is merged, its card offers
Clean up (remove the session's worktree and park the session; a worktree with
uncommitted changes is kept) or Keep, once per PR; `ghostex work-mode cleanup
<session> clean-up|keep` answers it from a terminal. The phone's session list
shows the same second line (the GitHub Project chip included): tap a chip to open it in the browser, touch and hold
it to copy its link, and tap Clean up or Keep to answer. A
session on a branch other than main is titled by that branch, without your name
and the ticket ID, until you rename it. Linear status needs a Linear API key: set
one per workspace in Settings > Workspaces; a project that needs a different key
gets its own under **Linear API key** in Settings > Projects once its Work mode is
on (the line under it says whether the project uses its own key, its workspace's
or the shared one, and Remove goes back to the workspace's). Or run `ghostex work-mode linear-key`
and paste it (that sets the Personal workspace's key, which a workspace without
its own uses; `--project-id <id>` sets one for a single project, `--clear`
removes it). GitHub status comes from `gh`, so sign in
with `gh auth login`. `ghostex work-mode status` says what is set up.

To start a piece of work from a new ticket, open the project's "…" menu and choose
Create Linear Ticket… (it shows once the project is in work mode and has a Linear
key; in a workspace whose primary tracker is GitHub it is Create GitHub Issue…,
which makes the issue in the project's repo and works on
`<your GitHub name>/<number>-<title>`), or click **New ticket** at the top of the
Work view, which opens the same
dialog for the repo the list is filtered to and otherwise lets you pick the
project. Give it a title and, if you like, a description, a team and a Linear
project; it is assigned to you unless you turn that off. With Start work now on,
Ghostex creates the ticket and starts the agent you pick in a new worktree on the
branch Linear suggests for it, linked to the ticket. Nothing is sent to the agent:
you type the first message. To start on a ticket that already exists, run
`ghostex work-mode start SPX-1245` (or `#218` for a GitHub issue, which works on
`<your GitHub name>/218-<title>`, or a PR link or `--pr 412` for a pull request,
which works on the PR's own branch and links the session to the PR and the issues
it closes); a second session on the same ticket joins the first one's worktree and
branch. Add `--cloud` to start a Claude Code cloud session on the ticket's branch
instead (with the task from `--prompt-file`, or the one Ghostex writes from the
ticket); it prints the session's link. `ghostex work-mode create-ticket --title "…"
--start` does both from a terminal (a GitHub issue in a GitHub workspace). New session in a work-mode project still
starts on main with no worktree.

A Work workspace can share a team backend (part of Workspaces, so turn that on
in Settings > Extensions first): your team's own Convex project (not
one Ghostex runs), which receives Slack and Linear events while your computer is
off and hands requests to the right person's Ghostex as soon as it is online. One
person sets it up with `ghostex team deploy --workspace <name>`, which uses their
Convex CLI login (`npx convex login`) to create the project, deploy Ghostex's
functions and make them the team's owner; it prints the Slack and Linear webhook
URLs, and the Slack signing secret and Linear webhook secret go into the Convex
project's environment variables (`SLACK_SIGNING_SECRET`, `LINEAR_WEBHOOK_SECRET`).
`ghostex team invite` makes a one-time invite link (valid for a week), and each
teammate runs `ghostex team join <link>`. So that a Slack request reaches you, link
your Slack member ID once with `ghostex team identity --slack-user U012ABC`.
`ghostex team status` shows the connection, `ghostex team ping` checks that your
Ghostex receives requests, and `ghostex team leave` disconnects. The same is in
Settings > Workspaces under each Work workspace: **Team (Convex)** joins with a
pasted invite link, shows who you are in the team, and has **Copy invite link**
(owners) and **Leave**; **Your Slack user** links your Slack member ID; and **Set
up a new team** shows the `ghostex team deploy` command to run in a terminal,
because deploying needs your Convex CLI login.

With the team backend in place, anyone can start work from Slack. In a thread,
type `@Ghostex cloud <what to do>` or `@Ghostex local <what to do>` (`/ghostex
cloud|local …` works at the top of a channel, where there is no thread to read).
Ghostex reads the whole thread, finds the Linear ticket (or GitHub issue or PR)
it mentions, asks which one when there are several, and creates a Linear ticket
from the thread when there is none, in the Linear project (release) the thread
links or the channel is mapped to. In a team whose primary tracker is GitHub it
looks only for GitHub issue and PR links, and when there is none your Ghostex
creates a GitHub issue in the channel's repo with `gh` (assigned to you) before
it starts. A thread that names only a PR works on that
PR's branch. Each ticket gets one working thread in the team's working channel,
tagging only you and the dev/QC owner, whose first post lists the requirements
your Ghostex summarised from the thread with a quick Claude call; and one working
session: a later request for the same ticket goes to that session instead of
starting another. `cloud` (the default) starts Claude Code on the web for the
channel's repo; `local` starts a session on your computer in a worktree on the
ticket's branch. A later request for a cloud session's ticket is sent to that
cloud session from the Ghostex that started it; a cloud session cannot post
milestones to Slack, so its working thread shows its link instead. If your Ghostex
is off, the request waits and starts when it is back. The thread you asked in gets
the working thread's link once and, at the end, only the final result (every
ticket's, when you asked for several). In a watch-only channel Ghostex only reacts 👀 and
forwards the request to the ticket's working thread. Sessions started from Slack
post milestones with `ghostex slack post --session <session> "<text>"` (`--final`
for the result). Setup: create the Slack app from `ghostex team slack-manifest`
(an app made from an older manifest needs the `channels:read` and `groups:read`
scopes added and the app reinstalled before the Work view can show channel names),
store its token and signing secret with `ghostex team slack-connect` (read from
stdin, on the computer that deployed the team), then set the flow with `ghostex team flow set
--working-channel <channel ID> --watch-only <IDs> --default-run cloud|local
--linear-team <KEY> --qc-owner <Slack member ID> --instructions-file <path>` and
map each channel to its repo with `ghostex team flow map <channel ID> --repo
owner/name` (add `--linear-project <name>` to put the tickets Ghostex creates from
that channel into a Linear project). The team instructions file is added to every session started from
Slack. Settings > Workspaces has all of this too: the **Slack** row's **Copy app
manifest**, whether the bot token and signing secret are stored (with the
`slack-connect` command to copy), and the workspace's **Team flow** section for the
working channel, watch-only channels, repos for new work, where new work runs, the
default Linear team and the team instructions. **Never work without a ticket** is
always on. Only the team's owners can change the team flow; members see it
read-only.

Slack commands find and create Linear tickets with the team's Linear key. Only the
team's owners set it: paste it in **Linear for the team** under the Work workspace
in Settings > Workspaces (members see whether it is set and who set it), or run
`ghostex team linear-connect` and paste it (`--remove` removes it). Tickets created
with it show the key's owner as their creator in Linear. To be the creator of the
tickets you request from Slack yourself, turn on **Create my Slack tickets with my
own Linear key** (or `ghostex team own-linear-key on`): Ghostex stores the
workspace's own Linear key (its **Linear API key** row) in the team's Convex project
for you, keeps it up to date when you change that key, and removes it when you turn
the switch off or leave the team; everything else still uses the team's key. The
team's Convex admins can technically read stored keys. Ghostex also fills in your
Linear user from your workspace's Linear key, so tickets created from your Slack
requests are assigned to you.

Related settings: Settings > Projects > Global Defaults (worktree command,
docs directory), Settings > Projects > Work mode and Linear API key, `hideProjectHeaderDiffStats`,
`showProjectEditorDiffFileCount`,
`showUntrackedProjectDiffWhenNoTrackedChanges`. Work mode: `ghostex work-mode
on|off|status|linear-key|create-ticket|start`, `ghostex link-session`. Team
backend: `ghostex team deploy|join|invite|identity|status|ping|leave`. Slack:
`ghostex team flow|slack-manifest|slack-connect|linear-connect|own-linear-key`,
`ghostex slack post`.

## Extensions, Open In, and integrations

- Settings > Extensions shows every extension as a card, three to a row: the
  built-in ones, grouped by category (Features, Project websites, Code and
  files, Planning and automation, Header buttons, Menus and panels, Shared
  runtime),
  then the Extensions Store (installed extensions first, then the audited
  third-party ones you can install), then Your views (custom URLs, Linear,
  GitHub Issues, dev server commands, HTML reports). One filter bar above them
  searches all of them at once and filters by source, type, and category; the
  count beside it says how many are shown. Each card's switch turns it on or
  off, and its actions (Edit, Details, Remove, Reinstall) appear when you hover
  it. Extension commands use the active local project's folder unless the
  extension supplies a folder; relative folders are resolved inside the active
  project.
  Features are whole parts of Ghostex you can switch off to keep the app
  simple: Actions (off by default), Open In (on by default), Spaces (off by
  default), Cloud Boxes (off by default; macOS and Linux only, not listed on
  Windows) and Workspaces (off by default; it brings workspaces, work mode, the
  Work view and the team's Slack flow). Turning one off removes it everywhere at once (its header button,
  Settings pages and rows, hotkeys, Quick Access rows and menus) and turning it
  back on restores everything you had set up. Settings: `actionsHidden`,
  `openInTitlebarButtonHidden`, `sidebarSpacesEnabled`, `cloudBoxesHidden`,
  `workspacesHidden`.
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
  doesn't need a reset yet, nothing is used. When a saved account's banked
  reset expires within 3 days, and again within 24 hours, a red notification
  appears in the Notifications bell (once per reset, also after a restart) and
  as a system notification; click it to open that account's usage dropdown,
  or Settings > Accounts when the account has no usage button. Turn the
  system notification off under Settings > Notifications > Sounds (Reset
  Expiry Notifications); the bell row stays. Each provider on Settings >
  Accounts has Auto-redeem expiring resets (off by default): Ghostex then
  automatically uses a banked reset 5 minutes before it expires, so it
  isn't lost, as long as Ghostex is running then (a Claude reset that only works at a limit is used then only at
  a limit). If that fails, Ghostex tries again until it expires and shows the
  failure in the bell. Under it, Also use it
  when I hit a limit (off by default) also uses a reset right away when the
  account hits a usage limit in the reset's last 24 hours, unless the limit
  resets on its own within 30 minutes. Each automatic use appears in the
  bell (`claudeAutoRedeemExpiringResets`, `codexAutoRedeemExpiringResets`,
  `claudeAutoRedeemResetsAtLimit`, `codexAutoRedeemResetsAtLimit`,
  `resetExpirySystemNotifications`). Shared history stays visible
  below: today's, yesterday's, and the last 30 days' token totals with a daily
  trend. History combines conversations across accounts of the same provider
  on that computer, counts shared copies once, and includes cached tokens.
  Claude and Codex histories stay separate. Switching account buttons changes
  the live limits; the shared history remains the same. Totals come from saved
  conversation logs, so they may omit usage whose logs are missing.
- Settings > Open In chooses which apps appear on session and project Open In
  menus and adds custom open targets. Open In is a built-in extension, on by
  default: switching it off in Settings > Extensions (Features) removes the
  header's Open button, the Open In page and the Open In rows in Quick Access
  and menus, and keeps your apps and custom targets for when you turn it back
  on (`openInTitlebarButtonHidden`).
- Settings > Integrations installs the bundled agent skills (Ghostex CLI,
  Ghostex Help, Computer Use and Browser Use through Fast Computer & Browser
  Use, SpaceO through SpaceO, Embedded Browser Use, Project Board Beads,
  Ghostex Visuals) and
  shows their install status; an installed skill's row shows the command you
  type to use it, such as `$ghostex-computer-use`. Skills are copied
  into the global skill folders every agent CLI reads. When the computer is
  online they are downloaded from the Ghostex GitHub repository, so skill fixes
  arrive between releases, and installed skills are refreshed automatically
  each time Ghostex starts. Offline installs use the copy inside the app.
  Its Desktop control section installs Fast Computer & Browser Use, Trycua's
  open-source driver (the `trycua/cua` link next to its name opens the project
  on GitHub). Installing it, Ghostex Computer Use or Ghostex Browser Use also
  installs Trycua's cua-driver skill (`cua-driver skills install`), which
  teaches agents the driver's own commands and which both Ghostex skills read
  first; if that skill is missing, the row shows an Install skill button.
  Once it is installed, its
  row shows an update button when a newer release is out (on a Mac), or a
  check mark when it is up to date (click it to check again), a reinstall
  button that runs the official installer again, and an uninstall button
  that removes Fast Computer & Browser Use but keeps its Accessibility and
  Screen Recording permissions. Hover them to see the installed and latest
  versions.
  On an Apple Silicon Mac with macOS 14 or later, the same section also
  installs SpaceO, which gives agents their own hidden screen: apps open on a
  virtual display, so agents click, type and take screenshots there while you
  keep your own screen, pointer and focus. Install SpaceO runs its official
  installer, keeps SpaceO running in the background and installs the Ghostex
  SpaceO skill; it does not add SpaceO to your agents' MCP settings. Its row
  gets the same update, reinstall and uninstall buttons (an update waits for
  agents using SpaceO to finish; uninstalling ends their SpaceO sessions and
  keeps the permissions). Its permissions row says which app needs
  Accessibility and Screen Recording turned on and opens those settings. Ask
  agents to use $ghostex-spaceo (or /ghostex-spaceo in Claude). Commands:
  `ghostex spaceo install-skill`, `spaceo doctor`.
- Settings > Integrations > Tools lists the tools Ghostex can install for you
  when something you set up needs them: Node.js and npm, uv, Homebrew (Mac),
  System tools (curl, certificates, unzip and git on Linux), Beads, and the
  GitHub and GitLab CLIs. Each Install button installs with one click and its
  tooltip says exactly how. When you already have a tool (for example Node.js
  from nvm or Homebrew), Ghostex uses yours and shows it as installed by you;
  otherwise it downloads the official release, checks its checksum and keeps
  it in its own tools folder, added to the end of your PATH. Homebrew asks for
  your Mac password once; Linux system tools ask for your password once, or
  open a terminal that asks for it where there is no password dialog (WSL).
  Tools Ghostex installed get Update (or a check mark to check again),
  Reinstall and Uninstall buttons, and installing an agent that needs npm
  installs Node.js first on its own.
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
  Install buttons for Claude Code, Codex, Cursor Agent and Grok Build (an Add
  to PATH button when an installed CLI is not on PATH), an Install guide
  that installs any other supported agent, the Ghostex helper (agent hooks)
  and Computer Use; which views to show (Browser and Files are on by default
  on a first run) and the browser skill; phone pairing and notifications;
  and the first project folder with the default agent and session view,
  next to the look: Appearance, the theme colour squares (Dark and Light tabs),
  Colourfulness and one Transparency row (the simple choices from Settings >
  Theme), with a "More theme options in Settings > Theme" link that opens the
  Theme page. Turning transparency on there also switches Appearance to Dark. The very first run opens on a short
  intro video before the panels: it plays from YouTube (on Linux it opens in
  the browser), shows once, and Continue goes on to setup at any time; the
  video is also at https://youtu.be/QzjFB4J6-8E. Reopen the setup any
  time from Tips > Setup or Quick Access > Commands > Setup.

## More than one window

File > New Window (Cmd+Shift+N, Ctrl+Shift+N on Windows and Linux) opens another
full Ghostex window, so you can put one on each monitor or macOS Space, for
example an agent testing something in one window while you work in another.
New Window is also in Quick Access's Commands tab and at the end of the
sidebar's More Options menu. A new window opens on the project and workspace of the window
you opened it from, with no session open, a little down and to the right of it.
The new-window button on a workspace's row in the workspace button's menu opens
that workspace in a new window, which starts the way switching to it does and
never shows a session or project from another workspace; drag
it to another screen or Space. Each window has its own sidebar selection,
sessions on screen, panes, view panel and views, Commands panel and Browser
tabs. Projects, sessions, settings, themes and hotkeys are shared: a session
started in one window shows up in every window's sidebar, and a setting changed
in one window applies to all of them. The same session can be open in two
windows at once: its chat stays live in both, and its terminal takes the size of
the window you last typed in or showed it in. Clicking a notification or a
session in the menu bar status menu goes to the window already showing that
session. Opening a session or project of another workspace from anywhere (Search,
Quick Access, a notification, the menu bar, a chat link, the Work page,
`ghostex focus`) brings forward the window showing that workspace, or switches
this window to it when no window does. The Code view works in every window at once, all on the same
editor. The menu bar status icon, notifications, completion sounds and Keep
Awake are the app's, not each window's: Keep Awake started from any window keeps
the computer awake and shows as on in every window. The Window menu lists the
open windows (each named after its project) and Cmd+` cycles through them.
Every window open when you quit, restart or update comes back where it was at
the next launch. Closing a window while others stay open removes it for good:
its Commands panel terminals close (a remote project's Action terminal closes on
its machine too) and its Delayed Sends are cancelled (it asks first when it has
either), while agent sessions keep running. Closing the last window quits
Ghostex, and Cmd+Q quits it with all its windows.

Related settings: none; window positions and layouts are saved automatically.

## Appearance and app

Theme colours, colourfulness, window glass, and active pane outline live on their
own Settings page, Theme, right below General (`ghostex settings open --tab theme`).
It has three groups, each with its own More options button for the finer
controls, plus links to related settings on General; a search for a row inside
More options opens it.
Colours: Appearance (System, Light, or Dark; System is the default and follows the
operating system appearance); Theme colour, a row of sixteen small gradient
squares with Dark mode and Light mode tabs (Graphite, the default, Black in dark
mode or White in light mode, Slate, Midnight, a deep navy, Blue, Indigo, Teal,
Green, Forest, Olive, Amber, Orange, Red, Rose, Pink and Purple; picking a colour
gives the other mode the same colour until you pick one there yourself); and
Colourfulness, a fine-grained slider from Subtle to Vivid (Soft is the default;
the name of the nearest of Subtle, Soft, Balanced, Rich and Vivid shows beside it)
that sets how much of the colour shows in the sidebar and work area at once, with a small
sidebar and work area preview. More colour options can set the sidebar and work
area colourfulness separately, turn on a custom colour for dark or light mode
(its tint and depth, 85 to 100 for dark and 60 to 100 for light), and show the
active pane outline and its colour. A preset never overwrites the custom values,
so turning the custom colour back on restores them. The chosen theme colors the
sidebar and window chrome, the sidebar's dropdown menus, and the chat view
background (chat keeps following its own Chat theme setting, so a light chat in a
dark app uses the light theme's color). Chat and terminal: Chat theme and
Terminal theme default to Follow app, with optional Light, Dark, or System
overrides; More chat and terminal options holds the terminal palettes. Existing
saved themes are preserved, and a saved dark contrast or tint that differs from
the default starts on the custom colour. The accent color (status highlights,
accent text, advanced-setting markers) has no setting of its own: it follows the
dark theme's tint hue, and a neutral tint keeps the sky-blue accent.
Window glass lets the blurred desktop show through the sidebar, the work area,
terminals, and chat on macOS, Windows and Linux. On macOS and Windows, menus and most dialogs (Rename
Session, Quick Access and the like) turn frosted to match. The Transparency group's Enable
transparency switch turns it on (Dark only, the default on macOS and Linux; Never, so
the window is fast and solid, on Windows) or off, and
Strength sets how see-through it is, from 0 (fully solid) to 100 (fully clear); the default
is 10. Blur sets how soft what shows behind the window looks, from 0 (sharp) to 100 points;
the default is 60. On Windows, Desktop and windows uses the system's own blur, which has no
setting, so Blur only shows there once What shows behind the glass is Wallpaper only, Custom
image or Live, where it softens the wallpaper, picture and video (`windowGlassBlurRadius`).
On macOS, Menu blur under More transparency options does the same for menus and tooltips
(default 20; menus opened after the change use it; macOS only) (`windowGlassMenuBlurRadius`). More transparency options goes
in the order you decide: 1 what shows behind the glass, 2 the pictures or videos,
3 their position, then Fine-tune the tints and Use transparency (Dark only,
Always, or Never). With Dark only, light mode stays opaque, so the light-mode
picture, video and tints are hidden until Always is picked.
Files, Kanban, the browser, and the code editor stay opaque. Turning on Reduce
transparency in the macOS accessibility settings, or turning off Transparency effects
in Windows Settings > Personalization > Colors, always makes the window opaque. On
Windows, turning glass on takes effect the next time Ghostex starts, and the corners of
menus and pop-ups follow Windows' own rounding.
What shows behind the glass (on macOS, Windows and Linux) is four cards: Desktop and windows (the default) shows everything behind Ghostex. Wallpaper only shows just your desktop wallpaper, so other windows never show through; built-in wallpapers such as Sequoia show as a still picture of that wallpaper, and a solid color wallpaper shows everything behind the window. Picture shows a picture you choose instead, one for dark mode and one for light mode, side by side with Choose and Clear buttons; a mode with no picture shows everything behind the window. Live shows something moving behind the glass, chosen for dark mode and for light mode (only dark mode with Use transparency set to Dark only): one of eight calm animations drawn in your theme's colours (Aurora, Ink, Drift, Nebula, Silk, Bokeh, Waves, Mesh), so switching themes or Colourfulness recolours it at once, or, on macOS and Linux, Your video, a .mov, .mp4 or .m4v file you choose, which plays muted, looping and blurred (a mode set to Your video with no file shows everything behind the window). On Windows, Live pauses when Show animations is off in Windows Settings > Accessibility > Visual effects (showing a still frame) and in battery saver. Speed sets how fast the animation moves (a quarter of its pace to twice as fast) and Brightness how bright it glows (60% by default, a soft glow that shows through the tints; every animation is about as bright as the others at the same setting); your own video plays as it is. An animation loops every two minutes without a seam, and changing the animation, theme or brightness, or switching between an animation and your video, fades rather than jumping. Nothing is downloaded. Live pauses whenever Ghostex is in the background, hidden or minimized, while the display sleeps and in Low Power Mode; Reduce Motion shows a still frame; and Play only when plugged in (on by default) pauses it on battery. For Wallpaper, Picture and Your video, Picture position picks Moves with the window (the default: the picture covers the window and moves with it) or Stays with the desktop (the picture stays put while the window moves over it, and can trail the window while you drag it) (`windowGlassSource`, `windowGlassImagePlacement`, `windowGlassImageDark`, `windowGlassImageLight`, `windowGlassLiveStyleDark`, `windowGlassLiveStyleLight`, `windowGlassVideoDark`, `windowGlassVideoLight`, `windowGlassLiveSpeed`, `windowGlassLiveBrightness`, `windowGlassVideoOnlyOnPower`).
On Linux, transparency works on Xorg and Wayland desktops (the desktop app currently uses XWayland for its embedded pages). Desktop and windows uses your compositor's blur: on Hyprland, keep background blur enabled; on Xorg, run a compositor with transparency and blur support. The compositor controls the blur's strength. Picture and Live animations work without compositor blur. Wallpaper only reads the picture reported by Hyprpaper, swww/awww, GNOME, Cinnamon, MATE or a single-screen Plasma desktop; when no readable picture is available it shows Desktop and windows. Stays with the desktop works on X11 and Hyprland; other native Wayland compositors cannot report the position needed for this mode, so use Moves with the window. Your video requires FFmpeg (included as a Linux package dependency). Live respects battery power, the power-saver profile, and disabled desktop animations (`windowGlass`, `windowGlassSource`, `windowGlassImagePlacement`, `windowGlassVideoOnlyOnPower`).
While glass is on, four sliders tune it, each in dark mode and in light mode: Sidebar tint and Work area tint set how much of the desktop each area hides, independently, so either can be the darker one; lower shows more of your desktop.
Keep Awake (Power)
prevents sleep while agents work.
Advanced holds Enable Experimental Features and Faster rendering (off by default), which
redraws only the parts of a window that changed so Ghostex uses less CPU while agents stream
and you scroll; it applies at once, and turning it off brings back the usual drawing if part
of a window ever stops updating (`fasterRendering`). The separate Debugging page sits
above About and appears in the Settings sidebar only while Show Advanced is on
(a Settings search still finds it). It starts with Show debug UI controls.
Enable that switch to see Diagnostic logs. Diagnostic logs has one switch per area (terminals,
sidebar, chat, modals, board, remote machines, agent activity, prompt editor,
app lifecycle, server requests) and one Turn logs off after choice (15 min,
1 hour, or Never) shared by all of them; warnings, errors, and crashes are
always captured. Open the page with `ghostex settings open --tab debugging`.
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

## Sending feedback

The chat-bubble button at the top of the sidebar, after Search, opens Send
Feedback (when the sidebar is too narrow the button moves into the sidebar
menu; Search and Notifications always stay in the top row). Write what is broken, confusing or missing and paste
screenshots into the text box with Cmd+V (Ctrl+V on Windows and Linux): up to
five PNG, JPEG or WebP images of 5 MB each; a larger PNG screenshot is scaled
down to fit. Review then shows the exact GitHub issue, title and description,
and you can edit both before Send; Ghostex adds your screenshots and a line with
the app, its version and your operating system. The issue is public on the
Ghostex GitHub, and after it is sent Open Issue takes you to it. Cmd+Enter
(Ctrl+Enter) moves to the next step and Escape closes the pop-up. It works in the
desktop app and in the web app. The "Collect more data with an agent" switch is
not available yet.

## Answering the common questions

- "Make Claude control Codex": see Agents, actions, and orchestration.
- "Match the terminal width to the chat": `ghostex settings set
terminalViewWidthMode match-chat`.
- "How do I annotate a Browser page or Markdown file": Browser pages use
  Agentation in the Browser toolbar (see Views, Browser). Markdown files use
  Files: select text to comment or mark Looks good, Clarify, or Needs tests,
  then Send to the last-clicked session (see Views, Files).
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
