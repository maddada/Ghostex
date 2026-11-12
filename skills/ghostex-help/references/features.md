# Ghostex features

Hand-written companion to `settings.md` (generated) and `hotkeys.md`
(generated). When you add or rename a view, a sidebar surface, a
session capability, or a CLI verb that users interact with, update the matching
section here in the same change.

Each section ends with "Related settings" so the helper can turn an
explanation into a change with `ghostex settings set`. It saves through the
running Ghostex app; on a computer without the app (one that runs only
Ghostex's server) the server saves it itself.

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
beside the views (with Show pages in their own row on, see Browser below, it has
one tab instead). The row is one order: drag any tab, a view or a page, anywhere
in it, and it scrolls sideways once the tabs run out of room. Right-click a tab
and choose **Pin tab** to shrink it to its icon and keep it at the left of the
row; a pinned tab has no **x** and ignores a middle click, so it closes only from
its menu, and **Unpin tab** puts it back. Closing the tab you are looking at
shows the tab next to it in the row, the one to its right or else the one to its
left, whether that is a view or a page. A page that another page opened (a link
or an image that opens in a new tab) goes back to that page instead, if it is
still open and you have not switched tabs since.
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
carries the project breadcrumb, Start, Open and Commit, and the
command terminal toggle. **Hide sidebar** sits at the top left of the
sidebar, and moves to the start of the header while the sidebar is hidden. On a
Mac, hiding the sidebar also hides the window's red, yellow and green buttons, so
this button moves to the header's left edge; the sidebar that slides in when
you hover the window's left edge shows the window buttons in its top-left corner.
In full screen they sit at the left edge too, with no gap for the window buttons.
The sidebar's top row also moves the window when you drag it. When an update is available, a download button
appears just before the project name. On Linux the button opens the release
notes with an **Open download page** button instead of installing the update;
install the new package the same way you installed Ghostex. Linux downloads come
for x64 and for ARM64 (aarch64) computers, each as a .deb, an .rpm, a portable
.tar.zst and an AppImage on the release page; pick the one that matches your computer's processor
(`uname -m` prints x86_64 or aarch64). The AppImage needs no install: make it
executable and open it, and it copies Ghostex into `~/.local/share/ghostex/appimage`
and runs it from there, so agents keep running after the window closes; to update,
open the newer AppImage and it replaces that copy. Without FUSE it stops with
"Cannot mount AppImage": install the `fuse3` package, or start it with
`--appimage-extract-and-run`. Windows comes as an installer only, for x64 or
ARM64. Download it from the website, or install from PowerShell with
`irm https://ghostex.dev/install.ps1 | iex`: the script picks the installer for
your PC, downloads it from GitHub, checks it against the SHA-256 digest GitHub
publishes, refuses to run it if they differ, and runs it. It isn't a browser
download, so Windows shows no SmartScreen prompt, and it changes no security
settings. From 11.3.0 the installer and the app are signed; Windows names the
publisher "Open Source Developer Mohamad Yahia Youssef". The portable zip is no longer published. A portable copy you already unpacked keeps
updating in place, keeps all its data in a `GhostexData` folder next to itself and
runs beside an installed Ghostex without sharing its sessions, settings or
projects. It uses the agent hooks the installed Ghostex set up (its Settings lists
them as installed by another Ghostex) instead of rewriting them.

**Open a view** is the picker the panel shows when nothing is open in it. It
lists every view you can open here: the built-in views first, then your own views
and extensions. Press a view's letter to open it (C for VS Code, B for Browser, K for
Beads, U for Automate, D for Files, G for Changes, T for Terminal, W for Issues) while the picker is in front. Views you hid
for this project are not in the list; **Manage views and where they appear…** at
the bottom opens their settings, and **Hidden here** on the `+` menu brings one
back. Closing the panel's last tab closes the whole panel; to bring the picker
back instead, turn off **Close side panel with its last tab** in Settings >
Sidebar & Sessions, under Advanced at the bottom of the page (`closeSidePanelWithLastTab`, on by default).

While a view in the side panel has focus, the session shortcuts act on that view
instead of the session beside it: Cmd+W or Cmd+Shift+Backspace closes its tab,
Previous/Next Tab in Pane (Cmd+Option+[ and ]) steps through the panel's tabs,
Sleep and Reload sleep or reload the view, Cmd+F searches in Files or Beads, and
Cmd+R renames the file open in Files.

**Issues** (called Work before) lists the tickets and pull requests of the
window's projects (Linear tickets or GitHub issues, whichever the workspace's
primary tracker is; see Issues Workflow under Git and worktrees). Open it from
**Open a view** (it is listed once Issues Workflow is set up: a Linear API key,
or `gh` signed in), where the Alt+number view hotkeys reach it by its place in the list, or by clicking a ticket or PR chip on a
session's card. It is one list,
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
details: the ticket and its latest comments, the PR with each check by name,
its reviews and its description, videos (Loom, YouTube and video files play right there), the
sessions linked to it, and where it is in your team's flow (Ticket, Slack
thread, Session, PR, Video, Review comments, CI, QC package, Validation).
Descriptions, comments and Slack messages show formatted the way Linear, GitHub
and Slack show them (headings, bold, lists, checklists, tables, code and links).
Steps Ghostex cannot see, such as the video OK, show a question mark instead of a tick,
and so does an optional step that isn't done yet (Slack thread is optional, since
not every ticket has one); the next step never waits for an optional one.
In a Work workspace connected to its team (see the team backend under Git and
worktrees), each row shows how many Slack threads the ticket has, and the details
show every thread: its channel, the first message and the latest replies with who
wrote them and when, files, links, and Loom or YouTube videos that play right
there, with **Open in Slack**; the ticket's working thread is marked as such.
Slack thread is ticked once the ticket has a working thread (click the step to open it), and
Validation once the request shows up in a watch-only channel or a session posted
the final result. **Conversations** also lists the sessions your teammates (and
you, in the cloud or on another computer) run on the ticket, with **Open in
Claude** and **Open in terminal** for a cloud session; watching a teammate's conversation is not available
yet. Without a team, as in a Personal workspace, these stay as they are. To
change the steps, open Settings > Workspaces and use
the workspace's **Team-flow steps**: reorder, rename or remove steps, tick
**Optional** on a step the team can skip, add one with
the rule that marks it done (for example "The pull request has a label" with
`READY-FOR-QC`), then **Save steps**; **Reset to default** goes back to the
default flow. In a workspace connected to a team the steps are the team's, shared
by every teammate, and only the team's owners can change them. **Open chat** shows the session linked to the ticket;
**Start chat** starts one in a new worktree on the ticket's branch, linked to it,
and sends nothing. Its arrow opens a menu: Start chat (where you also pick the
agent and the project), **Link to current session**, which adds the ticket to the
links of the session you have selected in the sidebar without removing its other
links (a notice in the middle of the view offers **Undo**; it is greyed out with the reason when no session is
selected, the session is on another computer, or, for a pull request, the session
already has a different PR), and **Start in cloud**. **Start in cloud** (also its
own button next to Start chat) lists the clouds Ghostex can start in, Claude Code
for now: pick one and a box shows the task Ghostex wrote from the ticket (its ID,
title, link, a short part of its description, the branch to work on with a pull
request linked to the ticket, and your team's instructions in a team workspace).
Edit it and click **Start**; the session runs on the ticket's branch (or creates
it when GitHub does not have it yet) on your Claude account (the one your
project's last cloud session used, otherwise your account for new sessions),
gets a sidebar row like any Claude cloud session (see Claude cloud sessions)
with the ticket's chip, and shows under **Conversations** as yours, with **Open
in Claude** and **Open in terminal**. The project needs a GitHub remote. From a
terminal: `ghostex issues start SPX-1245 --cloud [--prompt-file task.md]`. **New ticket** at
the top creates a Linear ticket or a GitHub issue (see Create Linear Ticket
under Git and worktrees) and the list picks it up right away. The Issues view is
part of the desktop app; the web version opens a chip's link instead.

**Changes** (on by default, listed right after Files) shows what changed in the
project as one diff you scroll through.
The scope button on the right of the toolbar picks what it shows: **Whole branch**
(the default) shows everything the branch changed, committed or not, compared with
where it left its base branch (main, or develop when the branch came from there);
**Uncommitted only** shows only what is not committed yet. New files are included.
The file count in the toolbar opens a list of the files; picking one unfolds it and
scrolls its header to the middle of the view. Each file's header stays pinned at
the top while you read it; click a header to fold the file, and tick **Viewed** to
fold it until it changes again. The toolbar
switches between Unified and Split, wraps long lines and hides whitespace-only
changes. The flask button (**Hide test files**) leaves test files out, so you can
review an agent's change without its tests: files named like `foo_test.go`,
`button.test.ts`, `user.spec.rb`, `test_parse.py` or `FooTests.swift`, and files in
`test`, `tests`, `__tests__`, `spec`, `testdata` or `fixtures` folders. A line at
the top says how many are hidden, with **Show** to bring them back; a commit still
includes them. The choice is remembered (Settings > Files & Links > File opening >
**Hide test files in Changes**, `changesHideTestFiles`, off by default) and the
phone's Changes screen starts the same way. To comment for the agent, hover a line and click **+**, or drag across
lines and click **Comment**; write the comment and press Cmd+Enter (**Add to
chat**). Each comment joins the chat box of the session you last clicked in the
sidebar as a pill, after whatever is already there; when you send, the agent gets
the file, the lines and your comment, and the sent message shows the same pills.
A session showing its terminal gets the comment pasted as text instead. The
comment stays on its lines as **Sent** until those lines change
or you click **Resolve** (**Show resolved comments** brings old ones back). Drag
across lines and click **Copy** (or Cmd+C) to copy them without line numbers.
Hover a hunk for **Revert**, or right-click a file's header for Copy Path, Open in
Files, Open File/Folder Location and **Revert file…**; both ask first. When the
project has uncommitted work, the commit bar at the bottom takes a message (leave
it empty and the agent picked beside it writes one) and the files ticked in the
list; its main button runs **Commit**, **Commit & Push** or **Commit, Push &
Create PR** (on the default branch that first moves the work to a new branch named
from the message), and its arrow also offers **Multiple Commits** (the agent splits
the work into commits) and **Merge into main…** on any branch other than main.
Keys while the view has focus: J and K step through hunks, ] and [ through files,
C comments on the current hunk, V marks the file viewed, O opens it in Files and
Esc clears the selection. The header's Commit button, the Git menu's Commit, Push
and Create PR open this view (Commit and Push on Uncommitted only, a pull request on
Whole branch). A chat's **Review all** and Session details' Changes card follow
**Open file changes in** (Settings > Files & Links > File opening,
`fileChangesOpenTarget`): **Changes view** (the default) opens Changes scrolled to
the card's first file, on Uncommitted only when that file has uncommitted edits and
Whole branch otherwise (Session details opens Whole branch); **VS Code** opens the
VS Code view. When the chosen view is turned off the other one opens, and when both
are off a notice says so, with a button to the views' settings. Opening a file to
edit it keeps following the other File opening settings. In
Ghostex Web the right-panel button in the header shows Changes beside the chat.
Hide it with `changesViewTabHidden`; give it a key with the **Switch to Changes**
hotkey (`switchChangesView`).
On the phone, **Changes** in a session's ⋯ menu or its long-press menu opens the
same diff for that session's project, read-only: Whole branch or Uncommitted only
(the same scope button, at the top right), the file list with its counts, headers that stay pinned and fold when tapped, and pull
to refresh; hold a line to copy it.

Right-click a view tab to choose where that view appears and what happens to it.
**Reload** refreshes the clicked view, **Sleep** unloads it while keeping its tab (Code also stops its editor
server; choose **Wake** or click the tab to bring it back, and Resources can stop
Code too without closing Ghostex), and **Open externally** opens its page in its
own window (for Beads it reads **Open in new window** and moves the view to a
Ghostex window of its own; see Project board). **Configure ▸** holds what a view can change about itself: **Modify
home URL** for a website view, and **Command output** and **Configure view** for
your own project views (Configure view opens that view's editor in Settings >
Extensions and focuses its name field). **Show in ▸** holds **This Project** and
**This Space**, which are ticks: unticking one hides the view there and leaves
it everywhere else (the space row is absent when the project is not in a
space). When the view on screen is hidden like this or turned off in Settings,
the panel moves to the tab beside it, as closing its tab would.
**Choose where it's shown…** at the bottom of that submenu opens the
view's full scope editor in Settings > Extensions. **Close tab** removes the
view from the strip.

- **Agents**: the terminal grid. Panes run agent CLIs or plain shells, split
  horizontally or vertically. There is no tab bar above a pane: the sidebar is
  the list of sessions, selecting one shows it in the focused pane in place of
  the session that was there, and a session already on screen in another pane
  is focused there instead. To split, drag a session row from the sidebar onto
  the left, right, top or bottom edge of a pane (terminal or Session Chat); drop
  it in the middle of a pane to show it there instead. Dropped on a chat's
  chat box, the row is typed there as a link to the session instead of
  splitting. Only sessions of the project on screen can go into its panes: a
  pane under a session from another project dims and says it can't split there. You can also press
  Option+Shift+D to move the focused session into a pane on the right. While the screen is split,
  the focused pane has a small bar along its top: click it for Close Pane (the
  sessions keep running) and Merge All Panes, or drag it to move that session
  onto another pane's edge (a new split) or its middle (it takes that pane's
  place), and the pane it left closes. Each pane can show the raw terminal or
  Session Chat. Cmd+Shift+O starts a new session with the active project's agent
  (the one its agent button shows, see Sidebar),
  in your default view (Chat or Terminal), like ChatGPT's New Chat. Cmd+N opens
  the New Thread picker instead: type to filter the configured agents (that
  project's agent first), Coordinator, Browser, or Terminal, press Enter to start it,
  and press Tab on Claude or Codex to pick an account. Coordinator opens the
  same New Coordinator dialog as the project's agent launcher. The picker's top
  row names the project it starts in (the one you have open, or the Unified
  filter's one project) and, when there is a choice, its computer: click it, or
  press Cmd+N again, to pick any project on any computer from a list with your
  recent projects first (the ones whose sessions you used last) and the rest A to
  Z, including projects with no session yet ("No sessions"). A project on more
  than one computer lists them as small chips under its name, the one you last
  used for it filled; Enter or a click takes that one, Left and Right or a click
  on a chip pick another, and a computer that isn't connected is dimmed. The same
  repository on several computers is one row even when a computer has no origin
  remote or spells it differently; two different repositories with the same name
  say which is which (owner/repo, or "no remote") under the name. Cloud Box and
  your boxes are chips there too for a project on this computer, once you've
  connected a Cloud Box provider or made a box. The first Down highlights the
  top project; Escape goes back. You can also just type in the search: under the
  agents it lists "Start <agent> in" with the projects it finds, using the agent
  you used last, and a computer's name in the search picks that computer
  ("ghostex windows"), so a search and Enter start a session. Cmd+Shift+T opens a new
  terminal, Cmd+T always opens a new browser tab, Cmd+Ctrl+Shift+F forks the
  focused session, and Cmd+D splits (from a session in a sidebar folder, the new
  session joins that folder). Cmd+R renames the focused session,
  Cmd+Shift+A (or Option+Shift+S) sleeps it, and Cmd+Shift+Backspace (or Cmd+W)
  closes it. On Windows and Linux use Ctrl for Cmd, except that fork is
  Ctrl+Alt+Shift+F and rename is Ctrl+Shift+R, because Ctrl+R belongs to the
  terminal. While a terminal has the keyboard, the Terminal focused hotkeys
  apply instead (Settings > Hotkeys > Terminal focused). On macOS they are the
  same keys. On Windows and Linux they follow Ghostty and leave the plain Ctrl
  keys to the program in the terminal (Ctrl+W deletes a word, Ctrl+D ends
  input): Ctrl+Shift+C and Ctrl+Shift+V copy and paste, Ctrl+Shift+A selects
  everything, Ctrl+Shift+F finds, Ctrl+Shift+W closes the tab, Ctrl+Shift+O
  and Ctrl+Shift+E split right and down, and Ghostex's own keys there are
  Ctrl+Shift+D for a new agent session, Ctrl+Alt+Shift+N for the New Thread
  picker, Ctrl+Shift+L for a browser tab, Ctrl+Shift+B for the sidebar,
  Ctrl+Shift+I for notifications, Ctrl+Alt+Shift+P to search sessions,
  Ctrl+Shift+Y for files, Ctrl+Alt+H for Find Prompts and Ctrl+Alt+/ for
  Hotkeys. A key you changed (or cleared) in the normal sections also applies
  in a terminal until you set that action's Terminal focused row. Every binding
  in either section can be changed or cleared on its own, and the same keys may
  be set in both; `ghostex settings hotkeys` takes `--terminal` for the
  Terminal focused ones. Cmd+Shift+M (Ctrl+Shift+M on
  Windows and Linux) takes the keyboard out of the terminal (Unfocus
  Terminal), so the other hotkeys apply, and pressed again puts it back in the
  terminal that had it last (Focus Last Terminal). Cmd+Shift+J (Ctrl+Shift+J on
  Windows and Linux) moves the keyboard to the sidebar's session list (Focus
  Sidebar), showing the sidebar first if it is hidden: the arrow keys move
  through the sessions and Enter opens one, and Escape or the same key gives the
  keyboard back to where it was. While the VS Code view itself has keyboard focus, Cmd+N and
  Cmd+Shift+O go to VS Code instead (New File, Go to Symbol).
  In the sidebar, Tab and Shift+Tab move through its buttons, headers,
  filters and session rows in the order they are drawn, and the one with the
  keyboard shows a ring: Enter or Space clicks it, Up and Down move between
  session rows, Home and End jump to the first and last row, and Escape gives
  the keyboard back. Tab from an empty chat box moves on to the sidebar's
  Search. Buttons that show only while you point at a row, and right-click
  menus, can't be reached this way yet.
  Closing a session that is busy (an agent in the middle of a turn, waiting on
  your answer to a question or approval, or still running background work) or
  a coordinator asks first, naming what closes; an idle session closes at once.
  Every way to close asks the same way: the sidebar's Close button and menu, a
  tab's x, Cmd+W, the tab menu's Close Others, Left and Right, Running Sessions,
  and the phone's Close. A closed session's conversation stays in Previous
  Sessions, where you can restore it.
  Starting a new session always creates a new one; empty sessions stay in the
  sidebar until you close them. An Advanced setting on Settings > Sidebar &
  Sessions, Close empty sessions when starting a new one
  (`closeEmptySessionsOnNew`, off by default), changes that:
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
- **VS Code**: the built-in VS Code based editor (code-server). Opens files from
  chat links, `ghostex edit <file>`, and Open In. `ghostex edit` opens a file in
  its project's VS Code view, and a file that is in no project in the VS Code
  view of the project on screen, without adding a project. Optional Use VS Code settings
  reuses the local VS Code configuration. VS Code shows one window per project;
  turn on Open VS Code windows as tabs (Settings > Files & Links > Editor,
  advanced) and a window VS Code opens (File > New Window, a folder or workspace
  opened in a new window, or the + after the window tabs) becomes another window
  tab in a row above the VS Code page, so several projects or workspaces stay
  open at once. A folder that already has a window brings that window's tab to
  the front. The windows belong to the project, close with its VS Code tab, and
  are not kept after quitting Ghostex. Middle-click or Cmd/Ctrl-click on a web
  link in VS Code opens it where Open links in says (a Browser tab or the system
  browser) without replacing the editor; a remote project's localhost link opens
  on that computer. The editor and the web runtime (see Browser) are both
  optional installs; the VS Code view offers each one the first time it opens.
  Once installed, an app update that brings a new version of the editor
  updates it by itself the next time the VS Code view opens (it shows
  Updating VS Code IDE component with its progress), without asking again.
  Each time the editor starts it gets a new private key that only Ghostex
  holds, so its address (127.0.0.1:3777) opened in another browser, or by
  another account on the computer or on a remote host, answers Forbidden.
  Settings: `codeServerLinkVscodeUserConfig`, `codeServerNewWindowsAsTabs`,
  `webLinkOpenTarget`.
- **Browser**: embedded Chromium tabs with profiles, splits, annotations,
  DevTools, and agent control through the `$ghostex-embedded-browser-use`
  skill, which reaches the pages through gxserver with its token. Chromium's
  remote DevTools port stays closed, so other programs on the computer cannot
  drive Ghostex's pages, unless Settings > Debugging > Show debug UI controls
  was on when Ghostex started. Its tabs sit in the view panel's tab strip, in
  the same row as the view tabs, and stay there while you are in another view, so clicking one comes
  back to Browser with that tab in front. Each tab shows its page icon and
  title, closes with its own x or a middle click, and drags anywhere along the
  strip to reorder. Right-click a tab for Select Tab, Pin Tab, Sleep (that tab),
  Sleep Browser (every browser tab), and Close Tab. Closing the last browser tab closes the
  Browser the way closing any view does: the panel moves to the next open view,
  and shows the Open a view picker (or closes the panel, with Close side panel
  with its last tab on) when Browser was the last one. **Browser Tab** at the top of the strip's **+** menu opens
  another tab. A tab with no address yet is blank: type or paste an address, or
  pick a running server from the Dev servers panel (Tools in the sidebar's menu).
  Turn on Show pages in their own row (Settings > Files & Links > Browser) to
  show one Browser tab in the strip instead, with the pages as a row of tabs
  under it, like VS Code's window tabs and Files' tabs: click a tab to show its
  page, close it with its × or a middle click, right-click it for the same menu,
  drag it along the row (or onto a page to split it), and use the + after the
  tabs for a new tab. After clicking a tab, the arrow keys step between the
  pages, Delete closes the shown one and Enter goes back to the page. Closing the
  Browser tab hides the Browser and puts its pages to sleep; they come back the
  next time it opens.
  Right-click a link or image in a page to open it in a new tab, save it
  (Save link as / Save image as ask where to save), copy its address, or copy
  the image itself; any page download also asks where to save.
  To see how a site looks on a phone or tablet, click the phone button in the
  Browser toolbar and pick a size (iPhone 15 / 16, iPhone 15 / 16 Pro Max,
  iPhone SE, Android, Small Android, iPad mini, iPad) or Full width; the page
  shows at that size in the middle of the pane. In the same menu, Landscape
  turns the device sideways and Mobile user agent makes the page load as that
  phone's browser would (it reloads). Each tab keeps its own choice.
  Saved form values (addresses, things you typed before) are offered as you
  type, but the Browser does not save passwords or sign you in automatically.
  Web links from terminals, chat, and detected dev servers
  open here or in the system browser depending on Open links in; Shift+click a
  link in the chat to open it in the system browser either way.
  A page an agent opens for a project that is not on screen (`ghostex browser open`
  uses the agent's own project) goes to that project's Browser: the project you are
  looking at stays as it is, a notice names the project, and the page is in front
  when you switch to it. The agent's browser tools then work in that page: they
  only use pages of the agent's own project (unless the agent names another page),
  and switching tabs by hand never moves the agent onto your tab.
  The Browser runs on the web runtime (Chromium), an optional one-time install
  that Ghostex does not need for anything else: until it is installed the
  Browser, the VS Code view, website and extension views, and HTML, drawing and
  media files in Files show an Install button instead of the page (or Hide tab
  to remove the view), and web links and saved Browser actions open in the
  system browser. Nothing is downloaded until you press Install; once installed,
  the runtime starts quietly a few seconds after Ghostex opens, so pages open
  fast. When an app update brings a new version of the runtime, an installed
  runtime updates by itself in the background after Ghostex opens (web views
  show Updating web runtime until it is done). Settings > Integrations >
  Embedded browser (Chromium runtime (CEF))
  installs, reinstalls or uninstalls it; the Install button on the first view
  that needs it does the same. Uninstall while the runtime is running removes
  it the next time Ghostex starts (Keep installed undoes that until then); on
  Windows a Reinstall while it runs is done at the next start too.
  Annotate the current page
  with Agentation in the Browser toolbar; GitHub pages disallow that tool.
  Each element, text or area you pick is captured as a picture along with
  its note, shown in the note box while you write. Send (the paper plane in
  the Annotate toolbar) puts the notes and their pictures into the agent
  session you last clicked in the sidebar: its chat box with the pictures
  attached, or its terminal with their file paths, or the clipboard when no
  session can take them. Nothing is sent to the agent until you send it
  there. Copy still copies the notes as text only. Pictures of images from
  other sites and of frames from other sites can come out blank.
  When a page shows its content inside a frame, such as a Storybook story,
  the Annotate toolbar opens inside that frame so the content itself can be
  selected. Copying or sending annotations clears them afterwards by default;
  the toolbar's own settings panel (Clear on copy/send) turns that off.
  HTML files in the Files view use the same Agentation overlay via Annotate,
  with the same pictures and Send.
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
  Linear pages in the Browser instead of sending them to the Linear desktop app). Any
  other permission a site asks for (show notifications, know your location, use the
  clipboard, camera or microphone, and the like) asks "… wants to show notifications."
  with Allow and Block, remembered for that site in that workspace's Browser; a site you
  allow to notify shows real system notifications, and clicking one brings its page back
  in its own project, switching to that project when another one is on screen.
  A sign-in window a site opens (Continue with Google, Apple or GitHub) opens as its own
  small window and closes itself when you finish. These
  questions show as a bar at the top of the page that asked, under the Browser's address
  bar, one at a time; they never block the window, so you can keep working or close it.
  The × (or Escape) means "not now" and remembers nothing, and a question goes away by
  itself when its page moves on or its tab closes. Settings > Workspaces > Site
  permissions > Forget all answers lets every site ask again (if that workspace's Browser
  hasn't opened a page yet, it applies when it does). If the sign-in gave up
  while the question was open, try it again.
  Agents open pages in (Settings > Files & Links > Browser, default Ask) is
  where an agent opens a page it built for you, such as a website preview: the
  Ghostex Browser or your default browser. While it is Ask, the agent asks you
  the first time and offers to save your answer here; once set, agents use it
  without asking. Setting: `agentPreviewBrowser` (`ask`, `internal-browser`,
  `system-default-browser`).
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
- **Beads** (was Kanban): the project's Beads issues as a tree under their
  epics or as a board, backed by the `bd` CLI (see Beads view).
- **Automate**: scheduled and triggered agent runs (see Automations).
- **Files**: Markdown, HTML, and Excalidraw files from the project's docs
  folders, with a markdown editor and an annotation system that sends notes
  back to the agent. Select text in a Markdown file to comment on it, mark it
  Looks good, Clarify, or Needs tests, or mark it Remove this (the X button),
  and add a global comment from the header; typing over a selection replaces it,
  as in any editor. Unselect the text (or press Escape) to close the toolbar.
  Click a link in a Markdown file to follow it (a click on the link you are
  editing, with its `[text](target)` showing, places the caret; Cmd-click, or
  Ctrl-click on Windows and Linux, follows it there too). A web link opens where
  chat links open (the "Open links in embedded browser" setting), an email link
  in your mail app, and a link to a file (a path, a path relative to the open
  file, or a `file://` link, with or without a `#heading` or `:line`) opens that
  file in a new Files tab beside the current one, or shows its tab if it is
  already open; a path that is not there says so. A link that opens another app
  (such as `vscode://` or `ghostex://`) shows where it goes and asks "Open a
  link in another app?" first, with Cancel as the default.
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
  all of them again. Send covers the open file's notes. Notes stay until you clear them with Clear. In a chat, Reply by Annotating in the ⋯ menu under an
  agent reply opens that reply in Files so
  it can be annotated the same way, with the feedback going back to that
  session. Folders appear as they load, and search fills in while
  Updating files is shown. Expand a folder to load it sooner; a loading or error
  marker means its contents have not been confirmed yet. Use Refresh in the Files
  sidebar menu to check for changes immediately. An open file follows changes
  made to it on disk: without unsaved edits it updates in place, with unsaved
  edits a notice offers Compare (the file's area then shows the diff from the
  copy on disk to your edits; Back to editing returns), Keep mine, or Take theirs
  (Save waits until you pick), and a file deleted on disk is marked Deleted until
  you save it again.
  Text and code files have a wrap button in the header that turns line wrapping
  on or off for every file, and the choice is remembered. CSV and TSV files
  open as a read-only table: click a column header to sort by it (click again to
  change the order), drag a column's edge to resize it, and click a cell and press
  Cmd+C (Ctrl+C on Windows and Linux) to copy it. The header's Raw button shows the
  file's text, which you can edit and save; a file with more than 50,000 rows shows
  its first 50,000 in the table. JSON files have Format and Minify buttons that
  re-space the file without changing its key order (an unsaved edit you can undo;
  .jsonc files don't get them, because their comments would be lost), and a Tree
  button that shows the file as a tree: fold objects and arrays, search keys and
  values, and hover a row to copy its path (such as $.scripts.build) or its value.
  Dockerfiles, PowerShell scripts and Terraform files are coloured, and so are
  .env, .gitignore, .ini and .jsonl files. A Jupyter notebook (.ipynb) opened from
  Quick Access or a link opens in the VS Code view, which shows its cells; opened from
  the files list it shows its JSON, with an Open in VS Code view button. The button at the sidebar's
  window edge hides the files list; the same button in the corner brings it
  back, and hovering that button peeks the list without pinning it. Hidden or pinned is remembered.
  Every file you open stays open, with its own unsaved changes and notes, and is
  shown as a row of tabs above the document (Show open files as tabs, on by
  default; turn it off in Settings > Files & Links > File opening to list them
  under Open Files, above the Project Docs tree instead): click a tab to show its file, close it with
  its × (a dot marks unsaved changes) or a middle click, right-click a tab for
  Close All, and use the + after the tabs to open another file. After clicking a
  tab, the arrow keys step between the files, Delete closes the shown one and
  Enter goes back to the document. When the Files
  view is narrower than 800px the list opens as a
  temporary drawer over the document and closes when you open a file, press
  Escape, or click outside it. Cmd+F, or Ctrl+F on Windows and Linux, opens the
  search: inside a Markdown, code or text file it shows Find and Replace with the
  caret ready (starting from the selected text), marking every match and moving
  through them with Enter and Shift+Enter, and anywhere else it reveals the files
  list and focuses its search. Escape closes the document search. In a Markdown,
  code or text file, Option+Up and Option+Down (Alt+Up and Alt+Down on Windows and
  Linux) move the current line, or every line the selection touches, up or down by
  one line, like VS Code; each press is one undo step, and a read-only file stays
  as it is. The filter
  button at the start of the files list's search box shows only HTML, Markdown,
  Excalidraw, image or video files: pick one or several types (All files shows
  everything again) and typing still narrows them by name. While a type is
  picked or a name is typed, the list groups the matches by folder: each folder
  is one row with its path and how many matches it holds, its files sit right
  under it however deep the folder is, and the folder they all share is named
  once above the list. Click a folder row to fold or unfold its files; it still
  takes drops and has the folder's right-click menu. To search the text of every file in the
  project, press Cmd+Shift+F (Ctrl+Shift+F on Windows and Linux) while Files has
  focus, or click the search button at the top of the files list: the sidebar
  turns into a Search view like VS Code's. Results appear as you type, grouped by
  file with a count of matches; the toggles in the box match case, whole words or
  a regular expression, and the … button adds "files to include" and "files to
  exclude" boxes (comma-separated patterns such as `*.ts, src`). Search skips
  what `.gitignore` ignores, dependency and build folders (node_modules, target,
  build, dist and the like), folders of copied-in outside code (`.dependencies`,
  `vendor`, `third_party` and similar), nested repositories inside dot-folders,
  binary files and files over 2 MB; turn off "Use Exclude Settings and Ignore
  Files" beside the exclude box to search those too, or name one of those folders
  in "files to include" to search just it. At most 10,000 results are shown. Click a
  result to open the file with the match highlighted (double-click to also move
  the cursor there), press Enter or F4 for the next result and Shift+F4 for the
  previous, hover a row and click × to drop it from the list, and press Escape to
  go back to the editor. The header switches between a list and a folder tree,
  folds or unfolds every file, runs the search again, clears it, or goes back to
  the files list; your search stays there for next time. With text selected in
  the editor, Cmd+Shift+F searches for it. Outside Files, Cmd+Shift+F still opens
  Find Prompts. To open an HTML, Markdown, or Excalidraw
  file in your web browser, use the open-in-browser button in the file's header
  or Open Externally at the top right of the view: HTML pages open as they are,
  Markdown opens as a formatted page, and drawings open in an Excalidraw editor
  that saves back to the file. The arrow beside the button, and Open With in a file's right-click
  menu, list your other browsers and the apps that open that kind of file (on
  Windows, also Choose another app for the system's Open with dialog). For a
  file on your own computer the first row is Show in Folder, which shows the
  file selected in File Explorer (Finder on a Mac).
  Browser links stop working when Ghostex restarts; open the file again for a new one.
  Settings > File opening decides where a file opens from Quick Access, a chat
  link, a terminal link or `ghostex open <file>`: one dropdown for HTML,
  Markdown and images, one for every other file (code and text). Both offer Files (the default), the built-in
  VS Code view, System App (the app your computer opens that file type with, on
  Windows, macOS and Linux), and the editor apps Ghostex knows (VS Code, Cursor, Zed, the
  JetBrains IDEs and the rest); an editor that is not installed on this computer
  is listed but cannot be picked. A remote session's file cannot reach this
  computer's apps, so System App opens it in VS Code instead. A chat file link can be a
  plain path, a `file://` URL or, on Windows, `C:/...`, `/C:/...` or
  `file:///C:/...`; all of them open the same file. If you had changed the old Markdown, HTML or
  Images setting, the first dropdown reads As before and keeps those choices
  until you pick something else. Videos and audio keep their own Files or system
  app choice; AVIF pictures follow the first dropdown, and PDFs, fonts and
  SQLite databases the second. HEIC and JPEG XL pictures, and videos or audio
  Files cannot play, open in the system app. A
  file on this computer that Files cannot show as text (a binary file such as a
  zip or an Office file) opens in the system app instead; a remote
  project's binary file shows Files' "not previewed" notice.
  Files shows a PDF in the built-in browser's PDF viewer (zoom, search, page
  thumbnails, print) and an AVIF picture like any other picture. A font file
  (.ttf, .otf, .woff, .woff2) opens as a preview: its name, version and
  weights, sample text you can edit at several sizes, and a grid of its
  characters; nothing is installed. A SQLite database (.db, .sqlite, .sqlite3)
  opens read-only: pick a table or view on the left to page through its rows,
  or type a SELECT query and press Enter or Run; Back to table leaves the query.
  Files never writes to the database, and a query that runs longer than a few
  seconds is stopped. In a remote project these work too, with PDFs, pictures
  and fonts up to 12 MB.
  A picture taller than the view opens at the view's width (never larger than
  its real size) and scrolls down like a page; a wide or small picture fits
  whole. To zoom, pinch on a trackpad, hold Cmd (Ctrl on Windows and Linux)
  and turn the mouse wheel, or press Cmd+= and Cmd+- (Ctrl on Windows and
  Linux); Cmd+0 goes back to how the picture opened. Zooming keeps the spot
  under the pointer in place. Drag or scroll to move around, and double-click
  to switch between the opening size and the actual size. The buttons in the
  file's header do the same: zoom out, the current percentage, zoom in, Fit to
  width (tall pictures), Fit or Fit all for the whole picture, and 1:1 for the
  actual size. Clicking the picture, or the expand button at the end of those
  buttons, opens it in the chat's full-window picture viewer: click to zoom in
  steps, drag to pan, and Escape or a click beside the picture closes it. Every
  picture opens at its opening size; the zoom isn't kept for the next one.
  To make the text of a Markdown, text or code file bigger, press Cmd+= and
  Cmd+- (Ctrl on Windows and Linux) with the file open in Files; Cmd+0 goes
  back to 100%. Normally this steps UI Scale's Scale, so the sidebar,
  chats and terminals grow with it (see Appearance and app). With Scale areas
  separately on, it steps only Files zoom (%) (70% to 200%, 5% steps): every
  open file and the Changes diff change size together, the line numbers grow
  with the text, and the level is saved (`filesZoomPercent`). Browser pages
  keep their own zoom.
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
`documentFileOpenTarget`, `otherFileOpenTarget`, `videoFileOpenTarget`,
`audioFileOpenTarget`, `docsOpenFilesAsTabs`, `filesZoomPercent`, the Auto Sleep rows
(`autoSleep*IdleMinutes`, `keepViewsLoaded`), the Search in Files hotkey (`searchProjectFiles`), and
Settings > Extensions.

## Sidebar

There are two sidebars, picked in Settings > Sidebar & Sessions > Sidebar
(`sidebarView`: `project`, the default, or `unified`) or with Switch to Unified
View / Switch to Project View in the sidebar menu's Sort & Filter. The Project sidebar,
described below, shows one Space at a time, project by project, with every
connected computer's projects in one list (see One list for every computer below).
The Unified sidebar shows every session from every machine, Space and project
in one list (only the active Space's while Spaces is on, see below): each session is a card with its project,
its status on the right (Working with a timer,
Waiting, Question, Approval, Failed or Done), its title, and its branch (followed
by the computer's name when it runs on another computer, as in "main · Windows"),
pull request and agent. Under a project header the project line is left out, so
every card there is two lines. A project is named after its repository there, so the same
repository on this computer and a remote one is one project. Sessions are
ordered by when they last came back to you (started, sent a message, or a
finished turn), never by output while an agent works; pinned sessions come
first, in the order you give them (drag a pinned card above or below another
pinned card of the same list or project group; the Project sidebar's Pinned
keeps the same order; see Pinned order below), and above them a Drafts section holds every session nobody has sent a
message to yet (the pencil), newest first, until its first message is sent;
it opens on its own whenever a new session lands in it.
A coordinator's threads sit folded under its card (see Threads in the Unified
sidebar below).
Moving, Snoozed and Parked fold into shelves at the bottom. Moving holds
sessions whose agent works or still runs something in the background, while the
Moving section switch (`unifiedWorkingSection`, on by default, under the
Sidebar switch) is on; a session leaves it for the top of the list when it needs
you. Parked shows only while session parking is on. A snoozed or parked row
offers Unsnooze or Unpark when you hover it. The phone follows the same
Sidebar organization as your computers (Settings > Sync on a computer): Project
or Unified, Group by project, the Moving section, the project order and the rest
change on the phone when you change them on a computer, and its Settings >
Sidebar & Sessions (or Sort & Filter in its ⋯ menu) changes them on your
computers too. On the phone, hold a project group's header in the Unified list
for Move Up and Move Down. A computer without Sidebar organization sync keeps
what the phone had before. On the phone, Unified hides the computer tabs: Computers in the menu at the top
of the Sessions list picks a computer, adds one or opens Manage computers. A
computer that can't connect shows its name and the problem over the list, with
Reconnect and Remote Settings. Add project asks which computer when you have
several. On the phone, the pencil at the top of the Sessions list (New session,
next to the bell, in both the Unified and the Project layout) lists every
project on every computer, recent ones first, with a search field and a chip for
each computer, starting on the one you used last for that project. Tap a project
and its remembered agent starts there (the computer's Settings > Agents >
Remember the agent you pick decides whether that is the agent picked last in the
project, in its Space or anywhere). It opens in the view the phone's Settings >
Chat > Default agent view picks, as a project's agent button does: in Chat, a new
chat where you can still change the agent, model, worktree, project or computer
before you send the first message; an agent without chat always opens its
terminal. If that computer isn't connected, it says so. A project's "+" and the
"+" on a project in the filter open the agent menu instead, which shows its
project on top (tap it to pick any project) and the same computer chips.

On the computer, both sidebars have a wide New session button in a row of its
own under the top row: a plus and New session, with its shortcut shown while
you point at it or it has the keyboard. The top row holds Search, then
Add project, the bell and the menu; on a very narrow sidebar Add project leaves
that row and is only in the menu (Add Project is always the menu's first row).
The button is there with several computers, while bots are showing and
in the sidebar that slides in while the sidebar is hidden. Click it to start the
agent you used last in the project of the session you have open, as Cmd+Shift+O
does. The chevron pointing right at its right end opens the New Thread picker
(the list of agents), as Cmd+N does, to
pick another agent, a Coordinator, a Browser or a Terminal, or another project.
The new chat's bar above its message box names its project: click the project
to move the chat to another one before you send the first message (it starts
again there, with what you typed). Once a message is sent the bar is gone and
the chat stays in its project.

The Unified sidebar has its own hover strip (`unifiedSessionCardHoverButtons`):
by default Snooze, Park and Close always show on a hovered card and the chevron
brings out Tag and Sleep, and opening the chevron opens it on every card. Settings
shows the strip of the sidebar you are using, and the button beside it copies the
other sidebar's. Group by Project (`unifiedGroupByProject`, in Settings and Sort &
Filter) puts every machine's sessions of one repository under one header, with
that project's Working, Snoozed and Parked inside it; Drafts stay above the groups.
Drag a header to arrange the groups; the order is saved per repository across
machines, and a new project lands after the arranged ones, A to Z. The "+" on a
header opens the New Thread picker on that project, on the computer you last
used for it. A project's worktrees sit inside its header: right-click the header
(or use the ⋯ on the project in the filter) for Delete Worktree, which lists them,
or right-click a session in a worktree for its own Delete Worktree. Both open the
same Delete worktree dialog as the Project sidebar.
A project you hid in the Project sidebar leaves this
list and its filter too, until you turn on Show Hidden. In the More menu, Sort &
Filter offers Show Hidden, Group by Project and Moving Section here, and Collapse
All folds the project groups while they are on.

The row at the top is a filter bar that shows each pick as a chip with its own X.
Chips that do not fit become a count that opens the menu; with nothing picked it
reads All sessions. The filter button narrows the list to projects, machines or
Spaces in a dropdown that scrolls when long. Type in it to find a Space or project
on any machine, or to search sessions: its first row, Search sessions for, makes
what you typed a chip that narrows the list by title, project, branch or machine.
The filter, open shelves and folded groups come back after a restart; search text
does not. A project row shows an X on its right while you hover it: click once
and it turns red, then click again to remove that project from the list on every
machine that has it; the "+" beside it opens the New Thread picker on that
project without changing the filter. The project goes to Recent Projects, where Quick Access >
Projects reopens it; its sessions keep running out of sight until then. Picking
a project in Quick Access > Projects also filters to it. Above the filter, the
New session button's chevron opens the New Thread picker in the filtered project
when the filter is one project (else in the one you have open), without making
it your open project; its top row changes the project and computer first. A
click on the button itself starts your last agent in the project you have open. When the new session's project is one the filter hides, that
project joins the filter as a chip, so you see the session and can drop the chip.
The session starts as a draft, and the bar above its message box has a project
chip and a computer chip to move it.

With Spaces turned on, the Unified sidebar shows the Project sidebar's Space row
under its header, and both sidebars share one active Space: switching Space in
one is the same Space in the other. The list then holds only the sessions of
the active Space's projects, Drafts and the Working, Snoozed and Parked shelves
included, and a Space with no sessions says so. Spaces are shared by all your
connected computers, so every computer's sessions follow the active Space; with
Separate by machine on, sessions on your other computers stay in the list in
every Space under their computer's name. Switch Space by clicking it, swiping sideways over
the list, dragging sideways on an empty part of the list, or with
Cmd+Option+Shift+1 to 9, with the same slide as the Project sidebar. The filter
narrows inside the active Space: its Projects list holds that Space's projects,
and, with Separate by machine on, its Spaces list offers only other computers'
Spaces. A session you start, or a draft you move, in a project of another Space
switches to that Space so you see it. Turn Spaces off and the Unified list shows
every Space again.

**One list for every computer.** Unless Separate by machine is on (Settings >
Sidebar & Sessions > Separate by machine, `sidebarSeparateByMachine`, off by
default), the Project sidebar has no machine tabs: every connected computer's
projects are in one list. A project on several computers (the same git remote,
else the same folder name), or cloned twice on one computer, is one entry with
the sessions of every copy mixed under it; a session on another computer shows
that computer's name, and the project's header names the computers it is on.
To keep two copies on one computer apart (say a folder copied to a new name),
turn off Merge copies of the same repository (Settings > Sidebar & Sessions,
`sidebarMergeRepositoryCopies`, on by default): each folder is then its own
project with only its own sessions, and a copy on another computer still joins
the one with its folder's name. The phone's project list follows the same
switch.
To put a project on a computer that doesn't have it yet, right-click it and
pick Clone on… and the computer: Ghostex clones its git remote into that
computer's usual projects folder (Settings > Projects > Global settings > Default
projects folder on this computer, the home folder on another one), using `<name>-2` and so on
when the folder name is taken, shows the clone in the header's jobs button, and
adds the project there. Computers that aren't connected are listed greyed, and
a project with no git remote can't be cloned this way.
Different folders of one repository (the repository itself and its subfolders,
like `monorepo` and `monorepo/trading`) are always separate projects, each in
its own Space, on one computer and across computers.
New Terminal, Create Agent and the project's ⋯ actions run on the computer a
session of that project last started on. Spaces and project groups are shared
by every connected computer: each keeps the same Spaces and groups, a project is
in the same Space on all of them, and a change made on any of them, on the
phone too, reaches the others; Ungroup removes a group from every computer. The
first time, Spaces with the same name on two computers become one and a message
says so, so name a Space the same on each computer to keep them together.
A computer that joins later takes the Spaces the others already share: its own
arrangement only fills in projects no other computer has filed. Taking a project
out of every Space applies on that computer only; the others keep it where they
have it.
Ghostex saves a copy of every computer's Spaces and groups first, in the
`backups` folder of its state folder. If Ghostex 11.0.0's sync moved every
folder of one repository into a single Space, the update puts them back once,
from that copy, unless you have rearranged them since. When one sync moves
three or more projects between Spaces at once (or puts them back like this),
Settings > Sync lists it under Projects moved by sync, saying which projects
went where and when. Turn Separate by machine on for a tab per
computer, each with its own list; the Spaces stay shared, each tab showing its
computer's projects of them.
Cards show the project's own icon when it has one, and right-clicking
any card opens that session's menu, as in the Project sidebar.

The sidebar lists projects and their sessions. Project headers carry the git
branch and diff stats and, on hover, a New Terminal button, the agent launcher
(New agent and the agent picker) and a ⋯ button.
The agent launcher's button shows the logo of the agent that project starts.
Each project remembers the agent you last picked in it, and a project with no
pick yet starts the one you picked last anywhere. To share one pick across a
Space, or use one pick for every project, change Settings > Agents > Remember
the agent you pick (`newSessionAgentScope`: `project`, `space` or `everywhere`).
The phone app remembers its own picks for each project's agent button the same
way and follows this setting of the computer it shows.
The ⋯ button, or a right-click on the project,
opens the project menu: Add Worktree (or Create PR), History,
New Browser Tab, Create Terminal, pinned Actions, Open Folder in the file
manager and Add to Group.
A project's Sleep, Wake, Sleep Inactive and Close Inactive act on that
project's sessions only; its browser tabs are slept and closed from the tab
strip above the view, where they live.
Sleep Inactive and Close Inactive leave sessions with unanswered question cards
awake, for every agent, as well as sessions that are working, waiting for
approval or running a background shell. A session's Advanced > Sleep Below
also skips unanswered question cards, approvals and background shells.
Click a project header (or the chevron beside it) or a group header to expand
or collapse it; rename a group from its right-click menu.
Sessions groups gather some of a project's sessions under a header of their
own, with a folder icon. To make one, right-click a session and choose
Advanced > Move to New Sessions Group. Once the project has a sessions group, the
session menu's Advanced > Move to Group page lists the project's groups: pick
one to move the session there, or Remove from Group to put it back in the
project's own list. Dragging a session onto another of its project's groups,
or onto the project's list, does the same (its header, its rows, or a heading
such as the project's Sessions heading, even while that heading is collapsed;
a pinned session stays pinned), and dropping it in the empty space
at the bottom of the sidebar puts it in the last session group when that group
belongs to the session's project; the group that would take it lights up while
you drag. A session stays within its own project's groups. A project's session
groups sit indented under it with a guide line and hide while the project is
collapsed; the collapsed project still shows when one of their agents is
working or needs you. Collapse All in the More menu folds the projects and
leaves each session group open or closed as it was.
The session group commands need Ghostex 10.36 or later, and their edits are
made by the Ghostex app, so it has to be running on that computer.
From the command line, `ghostex session-groups` lists each project's session
groups with their ids; `create-session-group --project-id <id> [--title <name>]`,
`rename-session-group` and `delete-session-group` (like Close Group in a
group's menu, it closes the group's sessions, which then head History so you can
restore them) manage them, and `move-session-to-group <session> --group-id
<id>` and `remove-session-from-group <session>` move a session. `create-agent`
and `create-session --start` take `--group-id <id>` to start the new session in
a group, and `create-session --cwd <folder>` starts a terminal in another folder
while it stays in its project.
Close Project parks the project in Recent Projects (a remote machine's project
goes to that machine's Recent Projects, which Quick Access lists); when it held
the active session, Ghostex stays in the current Space and switches to an awake
session of the next project in the list.
To remove a project for good (the folder on your computer stays), use Remove in
Settings > Projects or, from the command line, `ghostex remove-project` with
`--project-id`, `--path` or `--name`; a name two projects share fails and lists
them. Removing a project deletes its session records, so close its sessions first
(`ghostex close-session`) if they are still running.
Session rows show the agent icon, title, status, tags, and last-active time.
In Settings > Sidebar & Sessions, open Advanced at the bottom of the page to find **Highlight unanswered
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
Top chrome holds tag filters, Spaces, and the sidebar's menu (More Options, the
three-lines button at the top of the sidebar): Sort & Filter and Collapse All;
then Commands, Agents Hub, All Automations, Connect a Device and Keep Awake
(while it is switched on); then, between lines, Ask Ghostex AI, Join Discord and
Send Bug / Feedback; then two submenus: Tools (Search by Prompt, Import Sessions,
Extensions, Dev servers, Resources) and, in the desktop app, Ghostex (New Window, Check
for Updates, Restart Ghostex and the Quit rows); and Settings as the last row
(there is no Settings gear at the bottom of the sidebar). Add Project is
the folder button right of Search; it is the menu's first row only while the
sidebar is too narrow for that button. On another computer's tab the menu also
has Edit Machine.
**Group working sessions** (in the More menu's Sort & Filter page, and in
Settings > Sidebar; both switch the same setting, off by default) moves each
project's sessions into a Working section under that project while their agent
works. The section is collapsed by default (click its heading to open it) and
its heading shows the count. A session goes back to Sessions on its own when
its agent stops. The session you have open, and a working session with an
unanswered question (pink), stay in Sessions. Pinned, draft, parked and snoozed
sessions keep their own sections (`groupWorkingSessions`).

**Hide projects without sessions** (same two places, off by default,
`sidebarHideProjectsWithoutSessions`) leaves projects with no session in any state
out of the Project sidebar. The open project always stays, and a Show all projects
row at the end of the list brings the rest back in their places until you press it
again. A project whose sessions run in its worktrees (or in a merged copy of the
repository) counts as having sessions. The phone follows the same switch, synced
with your computers: Hide Projects Without Sessions in its computer menu (hold
the title), in Sort & Filter in its ⋯ menu and in its Settings > Sidebar &
Sessions changes it on your computers too. On the same Sort & Filter page, the tag rows sit under the heading "Show only
sessions tagged"; when nothing matches, the list says "No sessions tagged X.
Right-click a session > Tag As" with a Clear filter button.

Workspaces keep different parts of your work apart, for example one per company
you work for and a Personal one. Workspaces is a built-in extension, off by
default: turn it on with its switch in Settings > Extensions > Built-in. It also
brings the team backend (see Git and worktrees); Issues Workflow works without it.
While it is off there is no workspace button, every window shows every project,
the project menu has no Move to workspace, and the `ghostex workspace`, `team`
and `slack` commands say it is turned off; your workspaces, their Linear keys
and team connections are kept and come back when you turn it on (`workspacesHidden`).
Each workspace has its own projects, Spaces,
Linear API key, Claude account and Browser sign-ins (cookies), and a window
shows one workspace at a time. Every install starts with one workspace,
Personal, holding all your projects and Spaces. The workspace button sits at
the bottom left of the sidebar, on a line of its own above the account usage
meters: the workspace's letter with a small arrow beside it.
Click the letter to switch this window to your other workspace in one click
(with more than two, it goes back to the one this window showed last; hover it
to see where it goes). If another window already shows that workspace, that
window comes forward instead. Click the arrow for the workspace menu: pick any
workspace, open Workspace settings, or make a New workspace. In the menu, a
workspace that no window shows yet has a new-window button at the right end of
its row, which opens it in a new window. Switching reopens the session this window
last had open in that workspace (or its first project with no session, or
nothing if it is empty), and puts to sleep the open views of projects outside
it; their tabs stay and reload when you come back (with Keep views loaded on,
nothing is put to sleep). To move a project,
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
(a Work workspace can share a team backend), picks its **Primary
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
filter cannot be saved as a Space. Spaces is off by default: turn it on with
its switch in Settings > Sidebar & Sessions > Spaces, where its other settings
sit too. While it is off the sidebar has no Space row or Space menus, the Space settings are
hidden, and a view limited to some Spaces shows everywhere; your Spaces are
kept and come back when you turn it on (`sidebarSpacesEnabled`). Spaces follow the
machine: a remote machine shows its own Spaces when Spaces is turned on on that
machine, and the switch in Settings turns Spaces on or off for this computer
only. The Unified sidebar shows the same Space row and follows the same active
Space as the Project sidebar (see Sidebar). Create one with the "Create space" button
that fills the Space row while you have none, by right-clicking the Other
button or a Space icon and choosing New Space, or from the More menu when
Spaces overflow. To switch to the next or previous Space, swipe sideways over
the sidebar list with the trackpad, or drag sideways with the mouse on an
empty part of the list (below the last project or between projects).
To jump straight to a Space, press Cmd+Option+Shift+1 to 9 (Ctrl+Alt+Shift+1 to 9
on Windows and Linux) for the first to ninth Space in the order the Space row
shows them; rebind them under Go to Space 1-9 in Settings > Hotkeys.
**Active projects** is the star at the start of the Space row, before your
Spaces. Click it to see, in one list, the projects you are working on from
every Space and every connected computer: any project with a session working,
waiting on you or still running something, or that you used in the last 8
hours. The list keeps your Spaces' order and each Space's own project order,
and each project shows all its sessions as in its own Space. A project leaves
the list as soon as neither is true; the project of the session open now always
stays. Click a session to open it without switching Spaces. In the Unified
sidebar the star shows only those projects' sessions. Its status dots count
what works and waits there. It is on by default: right-click the star and
choose Remove Active projects to hide it, and turn it back on with Active
projects in Settings > Sidebar & Sessions > Spaces (`sidebarActiveProjects`).
The Go to Space hotkeys count your Spaces, not the star.
On the phone, a star chip leads the Space row above the Sessions list and lists
the active projects of every connected computer; long-press it to remove it.
In the phone's Unified list, pick Active projects in the filter.
A project added with Add Project (from its button right of Search at the top of
the sidebar, from the sidebar's menu while the sidebar is too narrow for that
button, from the "Add Project"
button that an empty project list or empty Space shows, or by right-clicking
the empty sidebar area) joins the Space that is open at the time and appears at
the top of it; add a project while Other is selected to leave it out of every
Space. A project made with New project folder goes where its form says (see
below).
Grouping never moves a project out of its Space: a group made with Add to Group
> New Group joins the Space its project is in, and a project taken out
of a group (Remove from Group, Ungroup, or dragging it out) stays in that
group's Space. Adding a project to an existing group in another Space moves it
to that group's Space, and the sidebar switches there with the project focused.
A sessions group made with a session's Move to New Sessions Group shows in the
project's Space.
In Add Project, select the computer whose folders you want to browse. Local
folder starts in your home folder, and External drives and other folders shows
that computer's filesystem root. On a computer running native Windows
(PowerShell), the two are a single Local folder row that opens the list of
drives, with your home folder at the top. You can paste a Windows drive or UNC
path when the selected computer runs native Windows, even from a Linux or macOS
client. Local folder is the first row. On a Mac, a folder in Documents, Desktop
or Downloads makes macOS ask once to let "Ghostex Server" read it: that is
Ghostex's own background service, and Add Project says so before macOS asks.
Allow it, or your agents can't work in that folder (System Settings > Privacy
& Security > Files and Folders changes it later).
To start a brand-new project, open Add Project and choose New project folder.
Type the project's name; the rows under it show what happens, and each can be
changed before you press Enter (or Create): Folder is where the project's
folder is made, Space and Group are where it goes in the sidebar, and Start is
what opens in it (your default agent, another agent, Terminal or Nothing).
Space and Group start where you are: the Space open in the sidebar and the
project group of the selected project. A project put in a group joins that
group's Spaces. Start remembers what you picked last. If the name is already a
project there, Create opens that project instead.
The folder is made in Settings > Projects > Global settings > Default projects folder
(`defaultProjectsFolder`, for example `~/dev`). With that setting blank, Add
Project suggests the folder that holds the selected project's folder (else
`~/dev`), lets you pick any other, and saves the folder your first new project
is made in as the setting. Local folder and a clone's destination on this
computer start in that folder too. A sessions group (Move to New Sessions
Group) only sorts sessions inside a project and never makes a folder; use New
project folder when the work needs its own folder, and Add to Group > New Group
to group projects together.
Right-click a Space icon for its menu: Manage (Edit Space, New Space) and
Sleep. Sleep Inactive sleeps only the Space's sessions that are awake but
neither working nor waiting on you; unanswered question cards, approvals and
background shells keep a session awake. Sessions stay where they are, asleep,
and wake when you open them; the row is greyed out when it has nothing to sleep.
The Other button offers the same rows for projects that are in no Space.
Space icons keep their normal glyph and show amber working-session and blue
attention-session counts in extra-bold text near the bottom of each icon, including
the selected Space.
Switching to a Space brings back what you last had open there: the session you
last used in that Space, in the view its project was in (Agents, Code, Browser,
Beads, Automate, or Files). If that session was closed, the one before it is
used; a Space you have never used opens its first project. Choose "Don't switch
projects" to make a Space switch change only the sidebar filter
(`sidebarSpaceSwitchBehavior`). "Switch Space when opening a session from another Space"
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
in the background for the "Keep the last project running for" number of minutes
(Settings > Power & Sleep > Auto Sleep, under Advanced at the bottom of the page;
`projectSwitchKeepAliveMinutes`, default 10, 0 to 60), so switching back is
instant. Set it to 0 to release them as soon as you leave. The page each other
Space opens on is the exception: it stays loaded for as long as that Space opens
on it, so swiping to a Space always shows its web page instantly. To keep every
side panel view you opened in a project loaded, not only the one on screen, until
its Auto Sleep time instead of these minutes (Auto Sleep Off keeps them), turn on
Settings > Power & Sleep > Auto Sleep > Keep views loaded (`keepViewsLoaded`).
Sleep on the project or its Space still releases them, it uses more memory,
and terminals and chats keep the minutes above. VS Code still reloads when you
switch VS Code between computers, including when you switch back.
Starting a new agent from the sidebar launcher or New Thread picker keeps your
current view open, including Code, Browser, Beads, Automate, and Files. Select
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
  state everywhere by default; "Remember sidebar per view" (`sidebarVisibilityMemory`,
  Advanced; its values stay `shared` and `perView`) can instead remember it once for a window with no view open and once for
  a window with one open, and then a project switch that hides it leaves it
  floating while you hover it. While collapsed, hovering the window's left 10px (an
  invisible hover zone over whatever is there) floats the sidebar back over your work, and it slides away when you
  move off it; the slide runs at the sidebar's Animations speed (Settings >
  Sidebar & Sessions > Animations, which also sets `sidebarCollapseAnimationDurationMs`; Off for no animation), whatever the
  system's Reduce Motion setting says. If you have also expanded a view to fill the window, that edge
  splits in two: hovering the top half floats the sidebar and the bottom half
  floats the Agents Panel, so you can glance at your agents without
  leaving the view. With the sidebar open and a view expanded, hover the sidebar's
  left edge, click a session, or start a new agent, and the Agents Panel floats
  to the right of the sidebar. It is always 520px wide and takes your typing right
  away. This works on macOS, Windows and Linux. On a Mac with **Show the window
  buttons when the sidebar is collapsed** off (Settings > Interface > Advanced),
  and in full screen, the floating sidebar runs the window's full height, and with
  Frame the work area on it floats as a card inside the frame.
- Panel animations: showing or hiding the sidebar, the side panel, the Agents Panel
  and the bottom or right command pane slides it open or closed; the side panel and a
  right-docked pane slide in from the window's right edge, and the command pane's
  terminals fade in as it finishes opening. "Panel animations" sets the speed: Off,
  Slow, Normal (the default) or Fast. With Reduce Motion turned on in your computer's
  settings, panels always open and close instantly (`panelAnimationSpeed`).
- Pane width: agent panes have a minimum resize width of 388px, and so does the
  Agents Panel when a view is open beside it. In the desktop app, an open Code,
  Browser, Beads, Automate, or Files view has a minimum width of 455px.
- Presets: Settings > Sidebar & Sessions > Sidebar > Preset switches groups of card
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
  Toggle them in Settings > Sidebar & Sessions > Sidebar (`showProjectIcons`).
- Session cards: agent icon, favicon, last-active time, git stats, colored
  icons, and rename-on-double-click are all toggles.
- Session hover buttons (click to toggle, drag to reorder), under Sidebar & Sessions >
  Session Cards (under the page's Advanced fold), is a strip
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
  menu's Advanced submenu unless its hover button is on (Note and its hover
  button show only while Session notes is on in Settings > Extensions). Close is the
  exception: while it is on the card it is never in the menu, and turning it
  off puts Close back as the menu's last row. Hover buttons also in context
  menu (on by default) controls the rest; turn it off and every enabled
  button leaves the menu (and its Advanced submenu).
  Both rows live under Sidebar & Sessions > Session Cards, in the page's Advanced fold. Settings:
  `sessionCardHoverButtons` (a list of `{ id, enabled }` with ids `rename`,
  `pin`, `note`, `snooze`, `closeAfterDone`, `tag`, `park`, `sleep`,
  `close`, `chevron`) and `showSessionCardHoverButtonsInContextMenu`.
  Session tooltips (Sidebar & Sessions > Session Cards, in the page's Advanced fold, on by default)
  controls the tooltip that shows a session's full name when you rest the pointer
  on its row in the sidebar; turn it off to show none. Other sidebar tooltips stay.
  Setting: `showSidebarSessionTooltips`.
- Long projects: every project always lists all of its sessions; there is
  no Show all row or Compact mode. Collapse a section heading (Pinned, Drafts,
  Working, Sessions, Parked, Snoozed) to tuck its rows away. The sidebar is the only
  scroller, and a project's header stays pinned at the top while you scroll
  through its rows.
- Drag a session onto another section of its project (its heading or any row
  in it) to move it there: Pinned pins it, Sessions unpins and unparks it, and
  Parked parks it. Dropped between two pinned sessions, it is pinned right
  there; dropped on the Pinned heading, it goes to the end of Pinned. Sessions
  and Parked keep their own order, so the drop line shows where the session
  will sit in them. Only sections already shown can be dropped onto;
  dragging never adds a heading or moves the list.
- Pinned order (`pinnedSessionOrder`, Settings > Sidebar & Sessions >
  Advanced, and in both sidebars' Sort & Filter menu under Pinned order):
  Manual (drag to rearrange), the default, keeps pinned sessions
  in the order you drag them into. Drag a pinned session above or below
  another one in Pinned, also a worktree's session listed in its project or
  another computer's in a project on several computers; the Unified
  sidebar's pinned cards keep the same order. Most recently updated puts the
  pinned session that came back to you last (started, sent a message, or
  finished a turn) first and turns dragging them off. The phone follows the
  computer's choice.
- Sidebar section headings (Pinned, Sessions, Drafts, Parked, and
  Snoozed) show an orange dot when a session is working, a blue dot when
  a session is done, a pink dot when an agent is waiting for an answer, and a
  red dot when an agent is stuck waiting for your approval, including rows
  hidden by a collapsed heading. A collapsed project, a collapsed collection, a
  Space button and a computer's tile show the same red for those sessions. Unanswered question
  cards count as questions for every agent, alongside Ghostex forms and pages with questions.
  They use pink instead of blue; multiple dots appear when multiple states are
  present, including orange and pink when an agent keeps working after asking. Collapsed
  headings show their session count at the right edge and a muted gray hollow
  circle after the status dots when they contain the active session. The circle
  disappears when expanded without moving the other dots. Hovering gives the
  header a subtle square background and replaces the dots with a chevron.
  Click anywhere across the heading row, including the empty space to its
  right, to animate the section closed or open. Sessions keep their full size
  and spacing while the section reveals or clips the list. The animation follows the
  Animations row (Settings > Sidebar & Sessions) and reduced-motion preferences. Related setting:
  `sidebarCollapseAnimationDurationMs`.
- A red dot on a session usually means its agent is stuck waiting for your
  approval (a permission prompt such as "Run shell command? Yes / No"). It wins
  over every other dot, hovering the session says "Waiting for your approval",
  the notifications list names what waits, and its coordinator's crew icon
  turns red. Answer the prompt in the session's chat or terminal and the dot
  goes away. `ghostex sessions` lists such a session as "waiting for approval".
  An answer from the chat goes only to the prompt its card showed: if the agent
  moved on to another prompt before your answer arrived (or while Ghostex was
  reconnecting), the card says the prompt changed and shows the new one instead.
- If an agent's terminal ends unexpectedly while the agent is running (a crash, or
  something on the computer ending it), Ghostex starts the agent again within seconds on
  the same conversation and account, up to 3 times in 10 minutes. A coordinator's thread
  is told to carry on and its coordinator hears about it; any other session that was
  working gets a notification that its last task was cut off. Sessions you or Auto Sleep
  closed or put to sleep, and everything during a quit, are left alone. When the restarts
  don't hold, the session gets a red dot and a notification: open it to start it again.
- A red dot on a session can also mean a model, effort or mode change you picked in the
  chat could not be applied: the agent refused it, or it kept failing for 30
  seconds. Messages you send after picking it wait instead of reaching the agent
  on the wrong model. Hover the session to read why, then pick a model again in
  the chat's model menu (the same one, or another); once it applies, the dot
  clears and the waiting messages are sent. Once Ghostex has stopped retrying the change,
  the session's chat also shows a card above the chat box with how many messages wait
  and which change failed. Open Details for the agent's diagnostic, including the
  model and thinking level you picked; Pick model opens the model menu.
- A red dot on a working session can also mean it is not moving: it has shown
  no new output (no new chat row and no change on its screen) for 8 minutes, so
  it may be stuck on a hung command. Hovering it says "Not moving", Ghostex adds
  a notification and plays its own not-moving sound (silent when Agent finished
  sound is Off), and the dot clears as soon as the session shows output again.
  A long build or test whose output is still coming does not count. A
  coordinator's thread is nudged first ("Still working?") and its coordinator is
  told so it can heal it, so a thread gets the dot only 8 minutes after that
  nudge; its board card shows a red "Not moving" chip too. While this computer
  is offline (or the agent's API does not answer) the 8 minutes stop: nothing is
  nudged, reported or marked not moving, and the clock starts again once the
  network is back. A "not moving" or "idle" report that waited in the
  coordinator's queue (for example while its input box could not be read) is
  dropped instead of delivered late when the thread has since moved on,
  finished, or been closed or marked done.
- A new session goes straight into Drafts, below Pinned and above Sessions,
  and stays there until its first message is sent or its agent starts
  working, whether it is empty or holds unsent text. Drafts starts collapsed,
  but it opens on its own whenever a new session lands there (New Agent
  Session, the New Thread picker, a project's agent button, or one made
  elsewhere), and a session you start in this window is selected and scrolled
  into view. If you collapse Drafts again it stays collapsed until the next new session.
  Choose Pin to move a draft into Pinned without sending or changing its
  text; Unpin returns it to Drafts. Sending its first message moves an
  unpinned draft to Sessions; pinned drafts stay in Pinned. No setting is
  required.
- Parking is enabled by default. Right-click a session and choose Park, or
  select several sessions and choose Park Selected, to move them into the
  collapsible Parked section at the bottom. Use Unpark or Unpark Selected to
  bring them back. On mobile, long-press a session and choose Park or Unpark;
  each project has its own Parked section, which starts collapsed. Parked
  sessions are always ordered from most recently active to oldest on desktop,
  mobile, and web, including when ordinary sessions use manual sorting.
  Mobile parking needs a connected computer with the `ghostex park-session
<selector> true|false --json` command. Parking keeps sessions running unless Sleep session when
  parking is enabled (off by default). Ask for a tag when parking or snoozing (on by
  default) makes Park and Snooze open the Tag As menu: pick a tag to tag and
  park in one step, or the No Tag Change row at the top to park as is.
  Closing that menu without choosing does not park. Unpark after sending a
  message (on by default) moves a parked or snoozed session back out of its
  section as soon as you send it a message from chat or type a prompt into
  its terminal; Codex only notices chat sends. All four settings are in
  Sidebar & Sessions > Sidebar, outside the Advanced fold: `enableSessionParking`,
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
- With Separate by machine on, each computer is an icon tile at the start of
  the row under New session, before the Spaces, showing that computer's icon,
  with a spinner while it connects and its status dots on the tile. The
  picked computer is filled like a picked Space. Hover a
  tile for its name and state, with the reason when it couldn't connect;
  right-click it for its menu, and click a computer that isn't connected to
  connect it. When the row is too narrow, the computers fold into one button
  that lists them.
- Every computer has an icon. While more than one computer is in the
  sidebar, every session row ends with its computer's icon, this computer's
  sessions included (a Unified card shows it after the branch), and a
  project's header shows the icons of the computers that have it. Hover a row
  or a project's name for the computer's full name. Pick an icon (a laptop, a
  desktop, a server, a cloud, a house, a rocket and more) in Settings > Remote
  > This computer for this computer, and in a saved computer's dialog under
  Computers this machine connects to for that one (it must be connected).
  The icon is kept on that computer, so your phone and your other computers
  show the same one: the phone draws it on the computer's tab and, while more
  than one computer is connected, on every session. Until you pick one, a laptop shows a laptop and other
  computers a desktop (a computer whose kind Ghostex can't tell shows its
  Windows, Apple or Ubuntu mark).

Related settings: everything under Sidebar & Sessions > Sidebar, `interfaceScalePercent`
and `agentManagerZoomPercent` (the sidebar's size, Settings > Interface > UI Scale), `sidebarSpacesEnabled`,
`sidebarSpaceSwitchBehavior`, `sidebarSpaceFollowActiveSession`.

## Commands pane

The Commands pane holds command terminals below the workspace, or on its right
when Commands pane side (Settings > Terminal) is set to Right. Open it with Cmd+J (Mac), Shift+Esc or F12; Settings > Hotkeys can
change the first two (Open Commands Panel and its Second Key). The **Terminal** view
(see Views) is the same kind of terminal opened as a tab of the view panel
instead; it has its own tabs and does not affect the Commands pane. Auto-minimize Commands
pane is on by default: after you move focus elsewhere and leave the pointer
outside the pane for 1 minute, it minimizes while commands keep running.
Focusing, hovering, selecting text, scrolling, or resizing keeps it open and
restarts the countdown. Background command output does not restart it.
In Settings > Terminal, under Advanced at the bottom of the page, Auto-minimize
Commands pane is one select: Off, 15 seconds, 30 seconds, 1 minute, 2 minutes, or 5 minutes. When auto-minimize
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
place, Settings > Sidebar & Sessions > Session Tags: choose Add tag, then give
it a name, an icon from the shared icon list, and a color from the preset list.
New Tag at the bottom of the Tag As menu opens that same place with the form
ready. Custom tags then appear in the same drag list as the built-in ones: drag
to reorder, use the switch or eye to hide them from menus and filters, and the
trash button to delete one (sessions that carried it become untagged). The
mobile app shows the same tags. `ghostex tag-session <selector> <tag>` accepts a
custom tag by name. Claude, Codex and OpenCode name their own sessions; Ghostex
syncs those names without running a first-prompt title job or blocking terminal
input. If Codex has not named a session about a minute after a prompt, Ghostex
names it in the background with the Title Generation Agent, without blocking
typing, and Codex's own name replaces it if Codex names the session later.
Pi uses the Title Generation Agent for first-prompt names. OMP names its own
sessions once a prompt says enough to name, and Ghostex syncs that name; renaming
an OMP session from Ghostex sends OMP's own `/rename <name>`, which OMP keeps.
An OpenCode session takes the name OpenCode holds for it, including a name it
gained after the session opened, such as one written through OpenCode's API.
Manual Generate Name and `/rename` in chat remain available for Claude and Codex.
In the Rename dialog, a name of up to 70 characters renames directly. Paste
longer text, or leave the box empty, and Generate name turns that text (or,
when empty, the session's recent prompts) into a short name in the box, which
you can still edit before pressing Rename. It uses the Title Generation Agent
(`sessionTitleGenerationAgent`); the dialog's "Set it in settings" link opens it.
In a Hermes chat, `/rename <name>` is sent as Hermes' own `/title <name>`.
A renamed agent session shows its new name right away, on the computer and the
phone, while the agent saves it. If the agent has not taken the name within 8
seconds, the old name comes back with a short shake; open Rename again and the
name you tried is already filled in, so you can send it again.
Renaming an Empryo session from Ghostex (the Rename dialog or `ghostex rename-command`)
types Empryo's own `/tab rename <name>`, so Empryo keeps the name too, and the
Rename dialog's Generate Name can name an Empryo session from its recent messages.
ZCode sessions rename from the sidebar and `ghostex rename-command` too: the name
is saved in ZCode's own session store once its session row exists (after the
first prompt), and ZCode's automatic naming will not replace it. Before that,
the rename is saved only in Ghostex and ZCode's later automatic naming may
replace it.
Fork (a session's right-click menu, or More actions > Agent in its chat) opens the new
session beside the original and switches to it. It starts as
`Fork: <original name>` and saves that name through the agent's own rename
command so it survives reopening the conversation.
Fork works for Claude, Codex, Pi, Empryo, GitHub Copilot and Grok Build sessions. OMP has
no fork of its own outside a running session, so Fork is not offered for OMP. Empryo has
no command-line fork, so Ghostex copies the Empryo conversation up to its last
finished reply under a new id and opens that copy; fork an Empryo session once a
turn has finished. The copy gets its own copies of Empryo's checkpoints, so undo
works in the fork, and deleting or closing either session leaves the other's
checkpoints alone. Copilot has no command-line fork either, so Ghostex copies the
Copilot conversation under a new id and resumes the copy.
Fork from here, in the ⋯ menu under one of your messages or under the agent's
last reply of a turn (a long press on the message on the phone), makes the
same kind of fork, but its conversation ends at that message: everything after
it is left out, and the original session carries on untouched. It works for
Claude Code sessions; other agents fork only the whole conversation. From a
terminal: `ghostex fork-session <selector> --message-id <id>`, with the id
`ghostex read-session-chat <selector> --json` shows for the message.
Once a conversation has forks, a small branch button in the chat's top right
lists every session that shares the earlier history, including the thread you
forked away from, and switches to the one you pick; a stopped branch is resumed
when you open it.

- Sleeping frees RAM; Auto Sleep does it after idle minutes. Auto Sleep runs
  on the computer that hosts the sessions, so it keeps working while the app
  window is closed, and it never sleeps a session a Ghostex window or the phone
  app is showing, or one with an unanswered question card, form or page.
  Web pages sleep the same way: a browser tab in
  any project, or a Files, website or extension view, that has been off screen
  for its Browser sleeps after or Project sleeps after time closes to free its memory and
  reloads when you select it again. Tabs playing sound, tabs where you typed
  text you have not sent, and the page another Space opens on stay awake. The side panel
  also keeps only the view on screen and the two you used last awake; with more views open,
  the oldest one sleeps, unless it is VS Code, Browser, Files, Beads or Automate with its
  Auto Sleep Off: Off keeps that view's state (its page, or Code's terminals) in exchange for
  more memory. Keep views
  loaded (`keepViewsLoaded`) lifts that limit for every view, website and extension views
  included, and keeps the views of projects you leave loaded too. When the
  Code view sleeps, its editor restarts: open files and unsaved changes come back, but its
  terminals start fresh. Resources (Tools in
  the sidebar's menu) sleeps many at once and shows CPU and RAM per session. Clean RAM
  copies a diagnosis prompt; paste it into an agent session to reduce RAM use.
  Sleeping sidebar sessions keep their normal title color and show a dimmer
  last-active time on the right; turn on Settings > Sidebar & Sessions > Sleeping Sessions > Dim sleeping
  sessions to fade the whole row instead (`dimSleepingSessions`). Dim inactive projects
  (`dimInactiveProjects`, next to it, off by default) fades the header of a project whose
  sessions are all asleep or closed, none working and none waiting on you (a project with no
  sessions counts too) on the computer's sidebar; hovering it brings it back, and the project
  holding the focused session never dims. The phone does not dim projects. Use `ghostex sleep|wake <selector>` to
  sleep or wake a session.
- A sleeping session wakes when you ask for it. Clicking its row in the
  sidebar wakes it. Selecting its tab, opening its project, or
  coming back to a project after restarting Ghostex shows a small bar with
  its name and a Resume button instead; click anywhere in the pane or press a key to
  wake the session. With
  Keep sleeping tabs asleep until clicked turned off, those wake right away
  (`clickToWakeSleepingSessions`). To stop a sidebar click from waking a
  sleeping session, turn off Settings > Sidebar & Sessions > Sleeping Sessions > Wake sleeping sessions when
  selected: the click then opens the session with its Resume button, and it
  wakes only when you click the pane or press a key
  (`wakeSleepingSessionsOnSelect`).
- Drag pinned sessions to reorder them within their project. Rows stay in place
  while an icon-and-title ghost follows just below and right of the pointer; the
  insertion line marks where the session moves when you drop it, and no line
  means the drop would change nothing. Projects and project groups show the same
  line where they will land, after an open project's whole list of sessions.
- Recent Sessions (Cmd+P) opens Quick Access to jump between sessions,
  Cmd+Option+Shift+O opens it on recent projects, and Cmd+O on Files.
  Its five tabs are Commands, Projects, Sessions, Saved Prompts and Files; they
  sit at the bottom left of the window and Cmd+1 through Cmd+5 switch between
  them (Cmd+5 means Files only while Quick Access is open; elsewhere it still
  focuses session 5). Files searches the files of every project in the sidebar,
  the current project first, with a few of the best matches from each other
  project: type part of a name or path, and letters in order are enough
  (`qawin` finds `quick_access/window.rs`). It lists what git would, so files
  your `.gitignore` skips stay out, and leaves out build and dependency folders
  such as `node_modules`, `target` and `dist`; an empty search shows the files
  at the top of the current project. Enter opens the file where Settings > File
  opening says (the Files view unless you picked otherwise), switching to its
  project first; Copy Path (Cmd+Shift+C) copies its full path. It searches a
  remote computer's projects on that computer, and Ghostex web searches too,
  though opening a file there needs the app. Hotkey: `openQuickAccessFiles`. Filters (Saved/Recovered/Sent, All/Closed/External, project, tags) are
  dropdowns at the right of the search line. Enter runs the selected row;
  Cmd+K, the Actions button at the bottom right, or a right-click lists
  everything else the row can do (star, tag, copy, edit, delete, remove, New
  Prompt) with each action's hotkey. Cmd+Shift+P opens Commands directly to
  search app commands, pane actions, and project actions. Commands in the
  sidebar's menu opens the same thing, and Settings is that menu's last row
  (Cmd+, opens it too).
  The Search button at the top of the sidebar opens Quick Access > Sessions,
  which lists past conversations from every agent CLI with resume and fork;
  Import Sessions (Tools in the sidebar's menu) opens it on External. History in a project's menu (its ⋯ button or a
  right-click on the project) opens Quick Access > Sessions with that project
  selected and Closed active, ready to search sessions you closed. Enter or a
  click on a closed session brings it back and keeps Quick Access open, so you
  can bring back several in a row; each one leaves the list.
  From a terminal, `ghostex previous-sessions [--query text] --json` lists closed
  sessions and `ghostex restore-previous-session --project-id <id> --session-id <id>`
  brings one back.
  Quick Access also discovers existing Claude, Codex, OpenCode, and ZCode
  conversations from outside Ghostex. OpenCode 2 conversations are read from
  OpenCode's background service (or a private one that exits right after, when
  the service is not running); empty, archived, and subagent ones are skipped,
  and opening one resumes it with `opencode --session <session-id>`.
  Opening an imported ZCode conversation resumes it in
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
- Search by Prompt (Tools in the sidebar's menu, Actions (Cmd+K) in Quick Access > Sessions,
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
  It lists prompts from Claude, Codex, Pi, OpenCode, Cursor, Grok, Empryo
  (from every repository Empryo has run in), Antigravity CLI, Hermes Agent
  (every profile), Copilot CLI, Factory Droid, OMP and ZCode, and resuming
  one reopens that agent's session, or focuses it when it is already open in
  the sidebar. A prompt can
  be forked into Claude, Codex, Pi, OpenCode, Cursor or Grok.
- Delayed Actions opens Session Automations. Send Enter defaults to **When all
  agents finish**. It can also run after a delay, when this agent finishes, or
  **When a specific agent finishes**. Choose the specific agent from the Agent sessions
  on the same computer; sleeping sessions are excluded. Ghostex waits until the
  selected agent has remained idle for 10 seconds and restarts that wait if it
  resumes work. Close After Done closes a pane once its command exits.
- Handoff / Export (a session's right-click menu, or picking
  another agent's model in the model picker) writes the conversation to a
  Markdown file and offers two choices. **Handoff to an agent** starts a new
  session. Its chat box starts with a line saying which session and project it
  continues from, then a link to that file, with the cursor two lines below the
  link; nothing is sent, so you write the first message, and the old session keeps
  running. Pick the agent under Continue with. Destination says where the session
  goes: this project on the same computer until you choose **Change…**, which opens
  Computer and Project. The session's own computer comes first, starting on this
  project, with every other connected computer after it. On another computer the
  project with the same Git repository is picked for you, and every other project
  there is listed too. If that computer doesn't have the repository yet, Clone it
  there opens Add Project on that computer with the repository already filled in,
  so you only pick the folder; once it is cloned, Handoff / Export comes back with
  the new project picked. When two checkouts of the repository are on different
  branches, the dialog says so: only the conversation moves, the branch over there
  is not switched. Files the conversation points at outside the project (pasted
  images and attachments, and files the old agent saved in its own folders or in
  temporary folders) are copied next to the transcript, and on another computer
  carried there with it, so the new session can open them; the file lists them
  under Files from this conversation. Files in the project, folders, and anything
  that looks like a key or password are never copied. It works in every direction, from this computer to a remote one,
  back, and between two remote ones. The button says where it goes: Handoff to the
  agent's name for this project, or Handoff to the project's name, with the computer
  when it's another one. **Use this project** goes back. **Handoff to MD** only
  saves the file and copies its path. Include, closed at first and with everything
  off, chooses what else the file holds: Commands & output, File changes and
  Reasoning. Handoff / Export remembers the card, Include, the agent you picked and
  the destination for next time. A remembered destination opens showing where it
  goes; one that's gone, or is the project you're handing off from, goes back to
  this project. Picking another agent's model in the model picker always starts
  from this project on that model's agent. Handing off a coordinator starts the
  new session as a coordinator with the same Goal and Standing instructions when
  the chosen agent supports coordinators.
  Include also holds History. At first the file keeps the last 20 turns, a turn
  being your message and everything up to your next one, and the slider picks 1,
  2, 3, 5, 10, 20, 50 or 100 of them. The top of the file then lists what you
  asked in the older turns, one line each. Turn on **Whole conversation** to keep
  all of it. Under the slider the dialog says what that comes to, like "10 of 120
  turns · about 9k tokens (the whole is 87k)", and the Include heading sums it up,
  like "Last 10 turns · Messages only". History is remembered with the rest, and
  dragging a session onto another project uses it too. On a computer running a
  Ghostex from before History, it is hidden and the whole conversation goes.
- Drag a session onto another project in the Project sidebar to hand it over
  there: a session can't change project, so this starts a new session in that
  project with the conversation attached, the same way Handoff / Export does. The
  project's header lights up while you drag (or the session group's, and the new
  session joins that group), and letting go opens a message, **Handoff to**
  followed by the project's name. Pick the agent under Continue with. **Close
  source session**, on by default, closes the old session once the new one has
  started; turn it off to keep it. If the agent is still working, the message says
  so and handing off stops its turn first; if it can't be stopped, nothing is
  handed over and a message says why. A draft, a terminal, a browser tab, a
  coordinator and its threads can't be handed over, so they light nothing. Only
  the row you drag hands over, also when several are selected. In the Unified
  sidebar, or for a project on another computer or in another Space, use Handoff /
  Export from the session's menu.

Related settings: `autoSleep*`, `keepViewsLoaded`, `clickToWakeSleepingSessions`,
`dimSleepingSessions`, `dimInactiveProjects`, `wakeSleepingSessionsOnSelect`,
`showSessionIdInTerminalPanes`, `sessionTitleGenerationAgent`,
`renameSessionOnDoubleClick`.

## Session Chat

Session Chat renders the same agent session as a chat GUI: composer with
image paste and Ctrl+G rich prompt editor, a prompt queue that sends when the
agent stops, transcript with thinking, tool, and edit cards, subagent
transcripts, question and approval cards, rewind, and a note per session (with
the Session notes extension on).
**Tips on new chats**: a new chat shows a card of tips under "What should we
build with …?", one tip at a time: first how to report a bug (Send Bug /
Feedback with Collect more data with an agent), split the view by dragging a
session from the sidebar, start a coordinator and use Ghostex from your phone,
then the rest of the app in a new order each day. Previous and Next walk the
tips, and each new chat opens on the next one. Got it hides that tip for good,
on every chat; once every tip is read the card stops showing. Hide tips (the
card's ⋯ button) or Settings > Chat > Tips on new chats turns the card off,
and Show tips again on the same page brings back every tip you marked Got it.
The card steps aside in a short window, so the chat box never moves. The Tips &
Tricks panel is gone; tips you had marked read there stay read. Setting:
`newChatTips`.
On the phone, OMP's **Advisor shared notes** card starts as one row with counts
by severity. Tap it to read the notes with their severity labels, then tap again
to fold it. An OMP background job stopped before it finishes has a muted cross
and name rather than the red failure mark.
A pasted or attached image shows in the chat box as one chip, such as Image #1,
on the computer and the phone alike: the cursor and arrow keys step over it, and
Backspace right after it removes it. What is sent is the image reference itself.
On the computer, Ghostex saves pasted images at up to 32 MiB; it converts BMP
and TIFF to PNG and reduces very large pictures when needed. OpenCode reduces large
images further when sending them to its service. If an image still cannot fit,
the chat box shows its size and the limit.
Press Enter while a pasted, dropped or attached picture is still being saved and
the message is sent at once: it moves into the chat with a loading tile where the
picture will be, and goes to the agent the moment the picture is in. When saving
takes more than a moment, the message gets a spinner and the Back to composer
arrow beside it; the arrow (or Up in an empty chat box) puts the text back in the
chat box until the message leaves, and the picture lands there instead. If the
picture can't be saved, the message comes back to the chat box and its error card
says why. The phone's Send works while a picture uploads, the same way.
A message you send is the chat's last message the moment you press Enter, with
a spinner beside it while it is on its way (or the play button of a message
waiting behind a working agent), and it reaches the agent once. If Ghostex's
service is restarting or not answering, the message stays where it is, says
"Couldn't reach Ghostex. It may be restarting. It sends by itself when it
answers." under it, and goes by itself; you do not send it again. A message held
because a dialog on the agent's terminal has its input says so the same way and
goes when the dialog is gone. Only a message that cannot be sent at all (the
session is gone, the text was refused) stops: it stays in the chat with the
reason under it and Retry, Edit and Remove. Edit is the only thing that moves
its words back to the chat box (Escape still takes back a message that has not
left). An answer given in a coordinator's list or Work panel waits out a
restarting service the same way.
Type `/` in the chat box to pick one of the agent's skills (`$` opens the same
list), `@` to mention a project file, and `+` (at the start or after a space) to mention another agent
session of the same project: the list shows each session's logo, title and
whether it is working, newest first, and narrows as you type. A picked session
goes in as a mention, `[<title>](<Global Ref>)`, which the agent can pass to
`ghostex agents send`. The skills list holds the skills that agent's own CLI loads: its
home skills folders (Codex, Pi, Cursor, Gemini CLI and GitHub Copilot also read
`~/.agents/skills`, and Cursor lists its own built-in skills too) and the skills folders inside the session's project. For Claude Code it also holds the skills and commands of its enabled plugins, named `plugin:name`, under Custom commands. A picked skill shows as a `/skill-name` pill with every agent, and each agent receives it the way it runs skills itself:
`/skill-name` for Claude Code, Copilot, Cursor and Gemini CLI, `$skill-name` for
Codex, `/skill:skill-name` for Pi and `@skill-name` for OpenCode. A skill name
you type yourself (`/skill-name` or `$skill-name`, without picking it) becomes
the same pill when you send, as long as the list has that skill.
Skills, under More in the chat's More actions menu and in the ⋯ menu
under a terminal view (on the computer and the phone), lists the Ghostex skills
the session's agent has installed: in the chat a skill goes into the chat box as
that pill, and in the terminal it is typed into the agent's input; neither sends
it. Its last row, Configure / Install more, opens Settings > Integrations > Agent
skills (the phone says to install them there on the computer).
Use the paperclip to attach images, files, or folders. On Linux, choose
**Images or files…** or **Folders…** before selecting items in the system picker;
the terminal's attachment action offers the same choices.
Click a picture in the chat (a thumbnail, or a picture in a reply) to open it
full size. The viewer goes through every picture in the chat, in order: on the
computer press the Left and Right arrow keys, or move the pointer near the left
or right edge for a round previous or next button; on the phone swipe sideways
(when not zoomed in) or tap the buttons at the sides. A counter such as 3 / 6
shows where you are, each picture opens fitted to the window, and Escape (or a
click beside the picture) closes it.
When the agent opens a picture file (Claude's Read, Codex's view image,
OpenCode's read, or any agent reading an image), its row in the chat shows a
small thumbnail of it at the row's right, beside its duration. When the turn folds into "Worked for …" or "Previous
turn", the pictures it read stay in view inside that card, under its title,
and the card counts them ("3 images") beside its tool calls. Other closed folds
(summary mode's "Active work", a step's folded tool calls) keep theirs under
the fold. The thumbnails sit on one row; when there are more than fit, scroll
it sideways with the mouse wheel or a sideways trackpad swipe (swipe on the
phone). Clicking one opens the picture viewer, and the arrows step through them
with the chat's other pictures. The picture is read from the computer the
session runs on, so it works for remote sessions and on the phone; a file that
has since been deleted shows a small missing-picture square. A picture you
already pasted or attached in your own message stays out of a closed fold's
thumbnails and its "N images" count; open the fold and the row that read it
still shows it, so you can see which picture "Read" means.
Dictate is the microphone button in the chat box (always shown, right next to
Send) and on the terminal bar, on a Mac, Windows and Linux: press it, talk, and press it
again to stop. In the chat session you are in, the Dictate hotkey, Option+Ctrl+Space
(Ctrl+Alt+Space on Windows and Linux), does the same as the chat box's microphone in
Auto mode: tap it to start and tap again to stop, or hold it for 300 ms or more
and it stops when you let go (Settings > Dictation > Hotkey mode also offers Toggle
and Hold). When macOS already uses Option+Ctrl+Space to switch input sources,
Settings > Hotkeys says so under Dictate and offers Option+Ctrl+D with one
click; like every hotkey it can be changed there. What you said goes into the chat box (or is typed into that
session's terminal) and is never sent by itself. On the computer and the phone, in
versions newer than 11.3.0, a dictation you start while the caret is in a text box
of a question form in the chat (a question's note, the note under "Other / none of
these", a text answer, or "Anything else?") is typed into that box instead, at its
caret. The box shows a small microphone while it waits for your words, touching
another box while you talk does not move them, and if the form is sent or answered
before they arrive they go into the chat box. Transcribe again and Transcribe
previous always type into the chat box, and so does every dictation in 11.3.0
and earlier. Dictate records first and
transcribes second: your voice is saved as a recording while you talk, and only
after you stop is it turned into text by the engine you picked in Settings >
Dictation: Hex, the recommended engine, or OpenAI's or xAI's speech-to-text
(both need that provider's API key, which stays on this computer). Hex 3 is the
free local dictation app from anomalyco/hex (macOS on Apple silicon, Windows and
Linux): it transcribes with its own models while it runs on that computer, with
no key and no internet, and the recording never leaves it. Ghostex no longer has
a speech model of its own; Hex replaced it. To set Hex up, pick Hex as the
engine and follow its row: Install Hex asks you to confirm (the dialog shows the exact
command from Hex's own installer: Homebrew's cask on a Mac, Hex's install script on
Linux and Windows), then your computer's own prompt (Touch ID, Windows Hello or
your password), and installs Hex 3 for you; a link beside it opens Hex's
website (anoma.ly/hex) to download it yourself, and a Mac without Homebrew is
sent there too. If the install fails the row shows the installer's last lines
and the command to run by hand. An older Hex without Hex 3's local API counts
as not installed. Then open Hex (Open Hex starts it and checks again by itself),
allow the microphone and download a model in Hex's
settings (Parakeet v3 for English and 24 European languages, Whisper for the
rest); Ghostex finds it by itself and the row becomes a picker of the models
Hex has ready. Turn on Open at login in Hex so dictation keeps working after a
restart; when Hex isn't running, the row says "Hex isn't running" with Open Hex
and Check again buttons.
A computer that used Ghostex's old built-in model moves to Hex (never to a paid
engine), and Ghostex deletes the speech models that model downloaded by itself.
A new install
picks the xAI or OpenAI key when one is saved, and Hex otherwise, then keeps
that choice until you change it; Dictate never switches engine by itself. Each
computer has its own engine, and the phone changes it for each computer in its
Settings > Voice. The phone has no engine of its own: it records and the
computer transcribes, so with Hex picked there, the phone's dictation uses Hex
on that computer. The text goes into the chat box as the engine wrote it:
Ghostex no longer polishes it with AI, removes filler words or keeps a
dictionary of its own. For punctuation fixes, filler removal or words it should
spell your way, use Hex's own settings (with OpenAI or xAI, names are misheard
more often). Hover the microphone to see what will transcribe:
"Dictate · Hex, Parakeet v3" (or "OpenAI gpt-4o-transcribe", "xAI speech to
text") over the hotkey, or what is missing, such as Hex not running or no key.

Settings > Dictation, under App in the Settings sidebar with Voice Agent right
below it, holds Dictate. Speech to text comes first: Engine (Hex, where Hex 3
runs on that computer, OpenAI or xAI), right under it the key the picked cloud
engine needs (OpenAI API key or xAI API key; the Voice Agent uses the same key,
and the row says so), the Hex row while Hex is the engine, Language (Auto-detect
or one language), Hotkey (with Change… to Settings > Hotkeys), Hotkey mode (Auto,
Toggle or Hold) and
Microphone (System default or one input device; a device that is unplugged
falls back to the default) and Insert with spacing (adds a space before or after
the text when the characters next to the caret need one, so text dictated
mid-sentence is spaced on both sides but never before a comma), then
Past recordings. With a remote machine saved, a Machine picker at the top
switches Dictate's engine and language to that computer, which transcribes its
sessions; it lists this computer and the remote machines connected right now
("Only connected machines shown" at the bottom of its list), and goes back to this
computer when the picked one disconnects (its API keys are added on that computer itself); the
hotkey mode, the microphone and Insert with spacing belong to the computer you
are at. Settings > Voice Agent keeps the voice agent's own provider (xAI Grok
Voice or OpenAI GPT Realtime), so Dictate can stay on Hex while calls use xAI
or OpenAI; the key that provider needs sits right under it, and it is the same
key Dictation uses. A problem with the provider or the connection never loses
what you said: when the transcription fails, a small card above the microphone
says what went wrong and what to do (no key, a refused key, Hex not installed or
not running, no internet, too many requests, credit or plan usage, audio the
provider couldn't read), with Add
key or Check key, which opens that key's field in Settings > Dictation, Set up
Hex, which opens Settings > Dictation at the engine, and Transcribe again; hover its sentence for the provider's own words when it gave any. In
the chat box, a small pill right above the microphone shows what Dictate is
doing: the recording time while you talk, then Transcribing. Its ✕ cancels, and
so does Esc. A cancelled recording is still
kept: rest the pointer on the microphone (or right-click it) and choose
Transcribe previous to type the newest recording that has no text yet into the
chat box. On the terminal bar, press the microphone again while it transcribes
to cancel, and right-click it for Transcribe previous. Every recording is listed
in Settings > Dictation > Past recordings. On the phone (Android or iPhone), Dictate is the microphone right next to Send in
the chat box, which stays there even when other buttons move into More actions; it
uses the engine (and key) of the computer the chat lives on, keeps the recording on the phone
until that computer has it, shows Recording, Transcribing and Transcribe again
(or Set up Hex) in a row above the chat box, and a long press on the microphone
offers Transcribe previous.
Settings: `dictateHotkeyMode`, `dictateMicrophone`, `dictateInsertSpacing` (this computer's own); Dictate's engine and language are saved by each computer's gxserver and changed in Settings > Dictation (`ghostex settings open --tab dictation`). Hotkey: Dictate (`toggleDictation`).
Read aloud speaks the agent's replies, so you can follow a session while you
drive or walk. It is experimental and off by default: switch on Read aloud in
Settings > Extensions > Experimental. On the computer, hover an
agent's final reply and press Read aloud (the speaker, last before the time); on
the reply being read it becomes Stop reading. Turn on Auto-read in the chat's
More actions (⋯) menu, under View, to hear each finished reply of the chat you are looking
at; while something is read, the same menu has Stop reading. Only the agent's
final reply of each turn is read, never your messages, tool calls or thinking.
Code blocks and tables are skipped with a short note, links are read by their
name and a file path by its file name, and a turn that ends with a question says
"The agent is asking you a question." Switching to another chat or sending a
message stops the reading, and Auto-read stays quiet while you talk with the
voice agent or dictate. Auto-read starts off each time Ghostex opens. Pick the
voice and the speed on Settings > Chat (Read aloud voice, Read aloud speed);
Ghostex in a browser uses the same speed. Two hotkeys, Read Last Reply Aloud and
Stop Reading, have no key until you set one in Settings > Hotkeys. Read aloud
uses the device's own voices: it speaks in the Mac app and in Ghostex in a
browser; the Windows app does not speak yet and the Linux app has no voice, so
use the phone or a browser there. On the phone, Read aloud works in the chats of
a computer that has it switched on: hold a reply and choose Read aloud (Stop
reading on the reply being read), and Auto-read and Stop reading are in the
chat's ⋯ menu. The phone has its own voice and speed under Read aloud in its
Settings > Chat, shown while one of your computers has it on. Auto-read keeps
reading with the phone locked or Ghostex in the background: a "Voice mode on"
notification shows it, with Pause (Resume) and Stop. On an iPhone there is no
such notification: a reply being read keeps playing with the phone locked, and
Stop reading is in the chat's ⋯ menu. If the phone cannot reach
the computer for 5 minutes, Auto-read turns itself off and says so; if Ghostex
on that computer is too old to stay connected in the background, the phone asks
you to update it there (`readAloudHidden`, `readAloudVoice`,
`readAloudRate`).
A long code block shows its first lines and scrolls inside the block; the arrows
button in the block's header shows the whole block, and again collapses it.
Run in terminal: when an agent's reply shows a shell command in a code block (a
bash, sh, zsh, shell or console block, or a block that starts with `!`), the
block's header has a Run button beside wrap and Copy. Run types the command into
a terminal of the Commands pane and presses Enter: the pane's current terminal
tab when it is free (not running something and not an Action's tab), otherwise
a new tab. The pane opens on that tab so you can answer login prompts or menus.
A command written as inline code (such as `npm install` or
`cd my-app && npx vercel login`) shows a small bar when you hover it, above the
command or below it when there is no room above: Copy, and Paste in terminal,
which types the command into the same kind of Commands pane terminal without
pressing Enter. Text you had half-typed at that terminal's prompt is cleared
first. Neither ever types into the agent's terminal. A leading `!` (the agent's
way of asking you to run it) is dropped; a console block runs only its `$ `
lines. A session on another computer runs it on that computer. On the phone, a
code block's Run sits beside its copy button, and holding a reply with one
inline command offers Copy command and Paste in terminal; the phone types into
the project's free Commands pane terminal on the computer (or a new terminal
when there is none) and opens it as a tab. From a shell:
`ghostex run-in-terminal --project-id <id> --session-id <id> --command
"<command>"` (add `--paste` to type it without running it).
A long message you sent (more than 20 lines or 2,000 characters, such as a
pasted log) shows its first lines with Show more under them; Show less folds it
again. Copy still copies the whole message.
Under your own messages and an agent's final reply, a quiet line shows the time
(hover it for the full date; under your own messages it sits at the bubble's edge).
Point at the message, its time or that line and a ⋯ button appears beside the
time. Click ⋯ for the message's actions:
Copy message, Reply by Annotating, Save to Markdown and Read aloud for
an agent's final reply; Copy message, Save prompt and Rewind to here for your own
messages, plus Edit message, Back to composer and Delete message while one still
waits for the agent.
Select words in the chat and right-click them for Copy, Add to Chat (quotes
them into the chat box) and Reply, which opens a small box for your answer:
Add to prompt (Enter) adds the quote with your reply under it to the end of the
chat box, and Send reply (Cmd+Enter, Ctrl+Enter on Windows and Linux) also
sends it, as Send would. Shift+Enter starts a new line and Escape closes it.
On the phone there is no hover. Hold a message's words to select them (drag
the handles to change the selection); the menu that appears offers Copy, Add to
Chat, which quotes the selected words into the chat box, and More, which opens
the message's menu with Copy and Add to Chat for the whole message, Reply for
the words you selected, and the same actions. A selection stays inside one paragraph, list item, table cell or code
block, so use More > Copy for a whole message. Holding a message beside its
words opens that menu straight away, and holding a link opens the link's own
menu. Nothing shows under a message until you do.
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
Type `/` at the start of the chat box to browse the agent's skills, its custom
and plugin commands, and its built-in commands, each under its own heading;
later in a message, `/` lists only the skills (a `/` word that names no skill,
such as a path, stays plain text). In Cursor
chats, `/compact` summarizes the conversation to reduce context, just like
`/summarize`.
In Claude Code, Codex, and OpenCode chats, start a message with `!` to run a shell command
in that agent's session, for example `! pwd`. The command and its output appear
in the chat.
A side chat is a separate conversation beside the main one: ask something on the
side without interrupting or derailing the agent's work. Side chat in the chat
box's More actions puts a Side Chat pill in the chat box (typing `/btw` on its own
and pressing Enter does the same); sending a message with the pill, or typing
`/btw <question>`, forks the session into a side chat that starts with that
question. The main agent never sees it. A side chat knows the conversation up to
that point, works in the same folder, and starts in ask-before-edits (Manual for
Claude, Ask for approval for Codex) whatever mode the main session runs in. It
opens as a tab in the side panel (its own chat screen on the phone), and you can
keep talking in it. Side chats are not sessions in the sidebar: the main chat's
**Side chats** pill, in the strip at the top of the chat box, lists them, and its
dot shows when one is waiting for an answer or an approval (the main session's
sidebar card shows that too). In the pill's menu, or the bar at the top of a side
chat, **Bring back** sends what was said in the side chat to the main session as
one message (a busy agent reads it at its next pause, and a sleeping one is
woken for it), and **Delete**
stops and removes the side chat. A side chat can't start another side chat.
Side chats work with every agent Ghostex can fork (Claude Code, Codex, Pi,
Empryo, Copilot and Grok Build). From the CLI: `ghostex side-chat start
"<question>" --session <session>`, `ghostex side-chat list`, `ghostex side-chat
bring-back --session <side chat>` and `ghostex side-chat delete --session <side chat>`.
Claude panels such as `/status` and `/usage` show as clickable tabs, tables and
usage meters instead of terminal text.
Claude Code's permission mode (Bypass permissions, Auto, Manual, Accept edits or
Plan) is the Mode button beside the model pill. Claude can also put itself into
Plan mode on its own, for example when a message asks it to plan something;
Ghostex then switches the session straight back to the mode you had, so only you
change it. Plan mode you pick yourself (the Mode button, `/plan`, or Shift+Tab in
the terminal) stays on. From the CLI:
`ghostex select-session-chat-model <session> --mode bypass|auto|manual|accept-edits|plan`.
Codex chats have a Mode button too, for what Codex is allowed to do: Ask for
approval (it asks before editing outside the project or using the
internet), Approve for me (an automatic reviewer approves safe actions and only
unsafe ones come to you), Full Access (no approvals at all) or Read Only. The
check follows the mode Codex is in, also when you change it with `/permissions`
in the terminal, and a pick applies to the running session right away, even
while Codex is working. A session started with Agent approvals set to accept all
starts in Full Access. Plan mode stays in the options menu. From the CLI:
`ghostex select-session-chat-model <session> --permissions ask-for-approval|approve-for-me|full-access|read-only`.
`/resume` works from the chat. In Claude Code and Codex chats it opens a Resume a
conversation card listing the agent's own conversations: type to search, Up and
Down move, Enter resumes the highlighted one, Escape cancels, or click a row (tap it
on the phone). The card follows the agent's list as you go, and once you pick, the
chat switches to that conversation. Other agents' pickers and menus that take over
their terminal (and Codex screens the chat can't read) show that part of the terminal
in a card instead. While that card is up it has the chat's keyboard: letters, numbers,
the arrow keys, Enter and Escape go straight to the agent, even if you were typing a
message. Click Return keyboard to chat on the card to type in the chat box again: the
card hides until the agent closes that screen, and nothing is sent to the agent. On
the phone, the card's key buttons send the keys.
A new Claude Code install's first-run setup is answered in the chat too: the
text style, the login method, and Sign in to Claude, where you paste the code the
browser sign-in page shows and press Sign in (Retry appears if the code was wrong).
`/login` in a running Claude chat opens the same Sign in to Claude card. Codex's
Sign in with Device Code shows its link and the one-time code to enter in any
browser, handy on a remote computer. Update now on Codex's update prompt has
Ghostex update the copy of Codex your sessions actually run, with the tool that
installed it (mise, npm, pnpm, bun, Homebrew or the official installer, the same
update as Settings > Agents), and restart Codex on the same conversation once it
is idle; the chat shows the update's progress. When Ghostex can't tell which tool
installed that copy, the card says where it is and how to update it. When Codex
quits to the terminal (after `/logout`, or an update Codex ran itself), the chat
says so and offers Restart Codex, which starts it again on the same conversation.
When Cursor isn't signed in, its chat shows Sign in to Cursor: Sign in opens
Cursor's sign-in page in the browser, and the card then shows the sign-in link,
which also works from your phone or another computer. The chat continues by
itself once you're signed in. If Cursor closes without signing in, the card's
Sign in starts it again. Cursor's command approvals (Run this command?: run it
once, skip it, add it to the allowlist, or run everything), its plan approval
(Ready to build?: build here, build in the cloud, or propose changes), its
requests to switch mode, and its questions are answered from the chat too, and
the plan shows in the conversation. Stop while Cursor waits on a command
approval skips the command and ends the turn. Skip skips the command straight
away, and your next chat message is a new message. The mode button switches Cursor between Agent, Plan,
Debug and Ask; it waits while a command approval is open.
Ghostex supports OpenCode 2 (version 2.0 or newer); OpenCode 1 is not supported.
OpenCode chats read and send through OpenCode's own background service instead
of its terminal screen, and support streamed replies, reasoning, tool results,
image attachments, questions, permissions, queued prompts, and conversation
rewind. It needs no Ghostex plugin (see Agent Hooks). An OpenCode chat works without
opening its terminal: the first message starts OpenCode's background service
when it is not running and creates the conversation, and the chat keeps reading
and sending after the terminal sleeps or exits. Opening the terminal later shows
the same conversation.
Installing the OpenCode plugin (Use agent hooks) also adds a "ghostex" OpenCode theme with a see-through
background, so OpenCode's terminal shows the window's glass like Claude Code and
Codex, in colours that match Ghostex's light and dark terminal. Ghostex turns it
on only when you have not picked an OpenCode theme yourself; to go back, pick
another theme in OpenCode (Ctrl+P, then Switch theme).
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
switches Antigravity between Default, Accept edits, Auto (edits and safe
commands run without asking) and Plan, and the button always shows the mode
Antigravity is really in. The context meter shows how many tokens Antigravity's
latest step filled; Antigravity does not report its window size, so the meter
shows no percentage. Commands: `ghostex send-session-chat-message`,
`ghostex answer-session-chat-prompt`,
`ghostex select-session-chat-model <session> --mode default|accept-edits|auto|plan`.
Sending a message to a sleeping session wakes it. While the agent is still
starting, the message shows in the chat right away and is typed in as soon as
the agent's input box appears. It keeps waiting for as long as the session is
open, even if you close Ghostex, and until it goes out you can edit or delete it
from the buttons under the message. If Claude Code has its settings, a plugin
recommendation, or its background-agents list over the input box, sending closes
it first (a recommendation is declined for now). When Claude Code shows a bug
report or feedback it drafted by itself ("Bug report drafted: …" over its input
box), Ghostex dismisses it as soon as Claude's turn ends and answers "keep" if
Claude then asks whether to turn those drafts off; the draft stays saved for
Claude's own `/feedback`. A question or approval the agent is waiting on still
has to be answered in its card.
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
DeepSeek Harness chats (its terminal app, `dsh-tui`) show your messages, its
replies and thinking, and the tools it runs with their results, and they can be
exported like any other chat. DeepSeek Harness saves each step when it finishes,
so the chat fills in step by step rather than word by word. It has no hooks, so
a new session's chat appears after its first message; the sidebar still shows it
working and done. The model menu lists DeepSeek's models for both ways of
signing in (your API key, or your DeepSeek account under DeepSeek account) with
the reasoning levels Off, Low, High and Max; picking one types its `/model` and
`/effort` into the session. Sign-in (`DEEPSEEK_API_KEY`, or `/auth login
deepseek-account` in its terminal), its Claude and Codex kernels, and its other
commands and settings stay in the terminal; a session running the Claude or
Codex kernel shows no chat. Until you finish DeepSeek Harness's first-run guide
once (in its terminal), each new session opens on that guide, and a chat message
waits until you finish or skip it.
GitHub Copilot chats show its replies, the commands it runs, the files it reads,
creates and edits with their changes, and its thinking when the model shares it.
When Copilot asks for approval (in Manual Approval mode), the request appears as
a card in the chat with Copilot's own choices; Yes and "don't ask again" answer
from the card, and No stops the turn so you can tell Copilot what to do instead.
Its `ask_user` questions appear as a question card: pick an option or write your
own answer. While Copilot waits on either, the sidebar shows the session needs
you, and a finished turn shows done. Stop really ends the turn, including a
shell command it is running. The mode button switches Copilot between
Interactive, Plan and Autopilot; the model pill shows the model Copilot reports
(Auto and the model it routed to), and the model is changed in the CLI with
`/model`, since which models you can pick depends on your Copilot plan. The `/`
menu lists your Copilot skills and Copilot's own commands; skills run as
`/skill-name`. Sleeping and waking a Copilot session, Full Reload and Fork
keep the same conversation.
Grok Build chats show the same cards. When Grok asks for permission (any mode
but Always approve, which Grok starts in while Agent approvals is set to accept
all), the request appears as a card naming the
command, with Grok's own choices: Yes, proceed (Cmd+Enter), always approve, No
(Esc) and never allow. Its questions appear as a question card: pick one or more
options or write your own answer. Stop and Interrupt & Send really end the
running turn, including a shell command. The mode button switches Grok between
Default, Plan, Auto review and Always approve, the context meter shows how full
Grok's context window is, and Fork starts a copy of the conversation in a new
session. Picking a model in the chat also makes it Grok's default for new
sessions, because Grok saves every model switch.
When OMP asks to approve a tool call, the request appears as a card with OMP's
own choices (Approve, Approve for this session when OMP offers it, Deny); click
one to answer. Once you start typing in OMP's terminal, OMP turns the request
into "Deny and send message", and the chat shows that as a terminal card that
passes your keys to OMP.
Empryo chats show your messages as you typed them (without the repository map
Empryo adds for its model), its replies and thinking, and the tools it runs with
their results, and they can be exported like any other chat. Empryo saves a
reply when the turn ends, and on longer turns sometimes a progress snapshot while
it works, so the chat fills in step by step rather than word by word, with the
working card shown until the turn ends. One Ghostex session follows one Empryo
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
chat too, with the same cards, and offers Fork and Make Coordinator on them.
Scrolling up collapses the composer; returning to the bottom expands it.
Settings > Chat > Keep chat box expanded while scrolling leaves the desktop
chat box at full size instead (`sessionChatKeepComposerExpanded`, off by default).
In a short pane, such as one half of a stacked split, the chat box stays
collapsed even at the bottom of the conversation until you click it, and
collapses again when you click elsewhere.
An empty collapsed composer shows only the first placeholder line, and scrolling
keeps the same toolbar buttons visible.
Right-to-left languages (Arabic, Hebrew, Persian and the like) work in the chat
box and the transcript: a line that starts in one of them is right-aligned and
reads right to left, like a web text box, and the arrow keys follow its reading
direction; lines in left-to-right languages stay as they are.
Hex colors (#RRGGBB, #RRGGBBAA, and the short #RGB and #RGBA when they contain a letter, such as #fa0) in messages, inline code, and tables have a small rounded color swatch
beside the value on desktop, web and the phone; an all-digit number such as #1234 is not a color and stays plain text (or links to the project's GitHub issue). Copying keeps the original text.
Mermaid diagrams an agent writes (a ```mermaid block) are drawn as diagrams in
the desktop and mobile chats once the block is complete. Source switches to the
diagram's text and Copy copies it. On desktop, the expand button opens a larger
view you can zoom and pan. The web chat shows the diagram's source.

Ghostex has three names for what agents show in the chat. **Ghostex visuals**
(gx visuals) are charts, tables, stat tiles, and short text layouts drawn right
in the chat, in the chat's own colors on desktop, web, and mobile. A **Ghostex
form** (gx form) is questions asked as a form in the chat. A **Ghostex page**
(gx page) is an HTML page the agent publishes, for things a chart can't show,
such as a UI mockup or a small interactive tool. Ask for them by name ("give me
a gx page for this", "ask me in a gx form", "show it as gx visuals"); agents
still understand the older words inline HTML, inline questions and inline
graphs. For a Ghostex page the chat shows a card with a small preview
picture of the page, the same on desktop, web, and mobile. Pages share one
look: a soft dusk backdrop in dark mode (a soft dawn in light) with frosted
glass cards and choices. The picture is taken
the first time you open the page in the Ghostex desktop app (and again after you
answer it); until then the card shows the page's file name. On desktop the card opens
the page where Settings > Chat > Open Ghostex pages in says: Popup (the default), a
floating window over the chat; Files, a page tab in the Files view, whose header
can open it in your browser or close it; or System browser, your default web
browser. The popup hides, still open, when you click outside it or press its Hide
button: a pill in the row above the chat box (the page's title and how many of its
questions you answered, for example 3/16) brings it back as you left it, and the x that
shows when you point at the pill closes it. Your answers reach the agent the same way
from all three, and the picture viewer and the remembered scroll position below work in all three. A
page with no questions (a report or a mockup to look at) waits for you too: it
ends with a "Reply about this page" box and a Mark as seen button, and until you
press it the page sits in the list above the chat box of things that need you
("Page, not seen yet") and counts in its "need you". Pressing it tells the agent
you saw the page, with your reply if you wrote one, and the page's card then
says Seen. Pages an agent showed before this came in don't wait. A page with
questions shows "N of M answered" in a small pill at its bottom right (green
once every question has an answer). In the
floating window, clicking outside it, its Hide button beside the close button,
a link in it that opens in the app's browser, or opening another page puts the
page aside instead of closing it. A hidden page keeps running: a tab after the
chat's title in that chat's header names it (with a dot while a question on it
is still unanswered), and clicking the tab brings the page back where it was, at
its size, scrolled where you left it, with your picks and notes as they were.
Pointing at the tab shows a small x that closes the page for good; what you
picked stays saved with the page, so its card opens it again with your answers.
Escape and the close button close it for good too, and so does sending its
answers. Its header can open the page in your browser instead. Clicking a picture in the page opens it in a
viewer inside the page, styled like the chat's image viewer: click or scroll to
zoom, drag to pan, use the arrows near the edges (or the Left and Right keys) to
step through the page's pictures, and Escape, the close button or a click beside
the picture to close the viewer (the window stays open). Its buttons zoom, copy
the picture, and open a picture from the web in your browser; a picture inside
the page itself can't be saved from there. A page you reopen comes back where
you left it scrolled, even after Ghostex restarts, and when the agent changes
the page's file while the window is open, a Refresh button at the top right
reloads it. On mobile the card opens
the page full screen over the chat. Tapping a picture in it opens the chat's
image viewer over the page (swipe or use the arrows to step through the page's
pictures), the Close button at the top closes it, and Android's back closes it
only when pressed twice within two seconds. It also comes back where you left
it scrolled, and shows Refresh when the agent changes the page's file. A page
the agent publishes for the browser opens in the browser on desktop, and pages
on web open in the browser. Every page is fitted to a phone's width: side-by-side
columns stack, small pills stay on one line, and a mockup wider than the screen
scrolls sideways inside its own box instead of shrinking the whole page.
Agents do this when you ask for `$ghostex-visuals` in your
prompt; they never use it on their own. Install the skill from Settings >
Integrations (Ghostex Visuals) or with `ghostex visual install-skill`. Pages
open in a sandbox and can't reach your computer's files or Ghostex.
When the agent needs your decisions, it asks them in a Ghostex form right in
the chat on desktop, web, and mobile: every question at once, with choices you click
and a note box for each. Nothing is picked for you (the agent may mark the
option it recommends), every choice question has an Other / none of these
choice, and you can leave any question blank or click a choice again to clear
it. A question you leave blank is sent to the agent as skipped, so it moves on
without asking again, and Skip all (or Ctrl+Shift+Enter, Cmd+Shift+Enter on
macOS) sends right away with every question marked skipped, even ones you already picked. A
form with more than one question also ends with an optional notes box
for anything else you want the agent to know, and every Ghostex page with questions ends
with an "Anything else?" box just above Send, whatever the agent wrote. You
can paste a picture (a screenshot or a copied image) into any note or text
box, in a Ghostex form or page: it shows as an Image #1 pill (as a
thumbnail you can remove in a page's "Anything else?" box) and goes to the
agent with your answers, so it can open it. Send (or Ctrl+Enter, Cmd+Enter
on macOS, from anywhere in the form or page) delivers your answers to the agent
as a message; the chat shows them as a card, and the form then reads Answered.
Answers from Ghostex forms and Ghostex pages send like normal chat messages; a busy
agent receives them after its current tool call when supported. Turn on **Wait
for turn end before sending form answers** to queue them until the turn ends.
A queued card says what it waits for, and Remove takes your answers back so the
form can be answered again. If a card or message left the queue but the
conversation never shows it, it says so instead of spinning, and Remove clears
it. When an agent writes its questions slightly off (a bare list of questions, a
single question on its own, trailing commas or curly quotes), the chat still shows
them as a form. A block it truly can't read shows "These questions could not be
shown" with the reason, and once that turn ends Ghostex asks the agent, once, to
send those questions again in the right shape, so you never have to paste the
error back; the broken card goes away when the new form arrives. Questions that need a picture,
such as choosing between two mockups, come as a Ghostex page instead, where you can
also pick an option right at its mockup with the Pick button at the right end of its title. A
page works the same way: its card reads Answered once you send, and opening it
again shows what you sent. The desktop's floating page window, a form's popup (the work dock's Open, the Resources
list) and the phone's page and form sheet fade out and close once your answers are
sent, and stay open on your answers with the error if they could not be; in the Files view or your browser the page stays open. Until you send,
what you picked and typed is kept, in a form and on a page alike, even when
you close the page or the chat, switch sessions, or restart. A small "?"
button sits on the working card just above the chat box, left of the other
pills (after Scroll to bottom when that shows), while a Ghostex form or
page in the chat is still waiting for your answers (desktop, web and
mobile): each
click goes to the next older unanswered one, round again after the oldest, and
briefly outlines it. A question stops counting once the agent asks the same tag
again in a later form or page, or withdraws it. Question tags the agent writes
in its messages, such as Q3, are always links to the form or page that asked
them, answered or not, drawn as small outlined tags that show where each
question stands: amber with a ? while it still waits for your answer, green
with a check once you answered it, grey once you skipped it, and grey and
struck through once the agent withdrew it. You can also answer by typing it in
the chat after the question was asked: a line that starts with the tag and
then the answer ("Q12: yes", "Q13 = B", "Q14B", "Q15 skip"; several on one line
between commas) answers that question everywhere, so it leaves the ? pill, the
sidebar's question dot and the coordinator board, and its tag turns green. A
form is answered once you have typed an answer for each of its questions.
Quoted lines and a tag followed by other words ("Q12 what does it mean?")
answer nothing. When you answer a question some other way, by doing what it
asked, the agent withdraws it so it stops counting as open. Settings: `chatPageOpenTarget` (`popup`,
`files` or `system-browser`) and
(under the page's Advanced fold) `chatSendOnPick`, which makes picking an answer send a
card that asks a single choice question right away, without Send answers
(off by default; cards with several questions, multiple choice, Other or text
answers still wait for Send). Commands:
`ghostex show <file.html>`
publishes a page and prints the block that shows its card, which opens where
Open Ghostex pages in says. Opening a page's HTML file itself (from the Files
view, a file link in the chat, Open in browser, or the phone) shows it styled
the same way: it answers to the chat that published that file, or, when no
agent sent it, opens read-only with a note saying so. Other HTML files open as
written. And
`ghostex visual check <file.json>` draws a Ghostex visual to an image, or lists a
Ghostex form's questions (and names anything to write the standard way), so the
agent can check it before replying;
`ghostex visual check <file.html>` opens a page in Chrome at a phone's width
and a computer's, saves a screenshot of each and lists what does not fit.

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

Nothing is pinned to the status line under the chat box until you pin
something, so a new install shows no line there. To show one, star items in
Context details; hover the status line and click the pen after its last item to
open Context details again. Everyone who already used Ghostex keeps the line
they had, and your own changes are never overwritten. The phone shows the line
the chat's computer shows: what you pinned in the Ghostex app on that computer,
for each agent, picked up when a chat opens or catches up. The phone's own pins
are used only with a computer that has no Ghostex app or runs an older Ghostex.
Reset to recommended
pins the setup Ghostex suggests: Claude Code gets Account, Model limit, 5h
limit, 7d limit, and Repository; Codex gets Account email, 7d limit, 7d reset,
and Account resets; Cursor gets Context used, Branch, and Lines changed; Hermes
gets Context used, Cost, Tokens, and Session time; Pi gets Model, Context used,
Cost, and Tokens (read from Pi's own session record) and can also show Context
tokens, Thinking level, Session time, Output tokens, Repository, Folder,
Branch, and Session title; every other chat agent (Freebuff, OMP, Grok Build,
Antigravity, OpenCode, ZCode) gets Repository and Branch, and can also show
Folder, Model, and Session title.
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
typing. When you scroll up, a small down-arrow button (Scroll to bottom) appears on
the working card just above the chat box, left of everything else, so it never
moves the other pills (with no card showing it floats on its own at the bottom of the
conversation). On desktop
and web, hover it to see its name and your current shortcut; mobile shows the button
without a keyboard shortcut. Both stop any
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
To stop the agent and send a new message at once, press Ctrl+Enter in the chat box
(on every platform, the same key Claude Code uses in the terminal), or right-click
Send and choose Interrupt & Send; on the phone, hold Send and choose Interrupt & Send
(the same menu has Compact & Send). It is offered while the agent is working; Claude Code stops its turn and
takes the message the way its own Ctrl+Enter does, and other agents are stopped as
with Escape before the message is sent. While the agent is idle, Ctrl+Enter just sends.
Codex rewind continues in a new conversation before the selected prompt and
returns that prompt for editing. If the chat cannot reconnect after the rewind,
choose Retry synchronization in the dialog to reconnect without rewinding again
(`ghostex rewind-session-chat <session> --message-id <message-id>` retries the same pending target).
Sending in a new chat shows your message immediately in the conversation while
Ghostex waits for the agent to be ready. It appears once, with a spinner beside the
bubble's bottom-left corner while it waits; you can retry or remove it if delivery fails.
A message sent while the agent is still working shows a play button there instead:
click it (tap it on the phone) to interrupt the agent so it takes that message right away. For Claude
Code and Grok Build, Back to composer (the arrow left of the play button, also in the message's ⋯
menu and in the menu you get by holding the message on the phone) takes it out of the agent's queue
and adds its text and pictures to your chat draft instead, unsent; pressing Up in an empty chat box does
the same for the newest queued message, like Up in the agent's own input. Prompts you explicitly
queue stay in the list above the input. This also applies when reopening the chat
or continuing on another device. A message that has waited about a minute says why under it, for
example that the agent is still working, is waiting for your answer, or that its input box can't
be read yet; in that last case Ghostex keeps trying on its own and, if it still can't deliver after
a few minutes, marks the message Not sent with Retry. When the agent is idle and its input box still
hasn't come back after about four minutes, Ghostex reloads the agent on its own conversation, as
Full Reload does, and the message goes in once the new input box is ready. It does not reload an
agent that is in the middle of a turn, is waiting for an answer, shows background tasks, or was
already restarted automatically in the last ten minutes.
When a chat message cannot be sent, it stays in the thread with a card saying why.
Retry sends it again, Fix it restarts the agent on its conversation and then
sends it, and Terminal view shows what the agent is showing. The session gets a
red dot in the sidebar and a Not sent row in Notifications until you deal with
it. A mode the session cannot use is greyed out in the Mode menu, with how to
turn it on.
While the agent works, a one-line card just above the chat box says what it is
doing: the running tool's description (or its command) while a tool runs, and a
working word such as "Considering…" between tools. The conversation ends above the
card, so nothing in the chat ever scrolls under it; at the bottom of the chat it
reads as the last thing in it. When the turn ends the card reads "Worked for 2m 13s"
for a minute and then goes, and that turn's fold reads "Previous turn" beside its
counts, so the time appears once. On the computer, a coordinator's chat with its
list above the chat box has no card: the same line sits at the left of the list's
header. The agent's subagents, its task list,
a coordinator's threads and the commands still running in the background are
small pills on that card (Workers, Tasks, Subagents, Background), each with a
count, also while the agent is idle; the Scroll to bottom and "?" buttons and an
armed Delayed Send sit to their left. A long label is cut with "…" so the pills
keep their full size; hover it to read all of it (on the phone, press and hold the
card). The card itself does not open: click or tap a pill to open its list in a
menu (a sheet on the phone) that opens upward from the chat box's top edge, lined
up with the pill you clicked: its right edge on the pill's, or its left edge for a
pill in the left half of the box, shifted to stay inside the box. On the computer, in a chat that has the Session
details panel, Tasks opens that panel instead (it lists the tasks), and the Tasks
pill hides while the panel shows. The Background list has each running command with its
own timer; hover a row, or press and hold it on the phone, to see the command
itself.
Claude children stay in the Subagents list while the terminal lists them, including
between monitor events. Idle children are labelled Idle and their clocks pause;
click a child's name or task to open its transcript.
For Codex and Claude, the Subagents list and popup title show the child's latest
model and effort in compact form, such as Opus 5 High or Astra xHigh. Codex rows
show the child's name/path beside the model and effort, after a ‣ separator.
For Claude, hover to see the agent type, such as Explore or general-purpose.
Unrecorded model values are labelled Model not recorded.
When Claude runs a workflow (Claude Code's Workflow tool, a script that starts
several agents in phases), the chat shows it as one Workflow card instead of a
plain tool row: the workflow's name and description, how many agents are working
and done, each phase with its progress (such as Map 2/2), and one row per agent
with a short preview of what it is doing or what it answered and how long it took.
The card keeps updating while the workflow runs. Click or tap an agent to read
its whole transcript, and use Show full script under the card to see the script
Claude ran. The card stays in the chat after the workflow finishes, even when the
turn that started it is folded.
A slash command shows as your own message, with the command in monospace, whether
you sent it from chat or typed it in the terminal, and it stays in the conversation
after a reload. What the command printed shows as a short note under it; click or
tap a note with more than one line to see the whole output. Model, effort, Fast
mode, and compaction results keep their status rows.
During `/compact`, Claude, Codex, Cursor, and Grok Build show a compaction card above the input.
Cursor's `/summarize` and Hermes's `/compress` use the same flow. Claude shows its reported progress;
Codex, Cursor, Grok Build, and Hermes show a looping bar. A Hermes `/compress` sent from chat that
finds nothing to compact shows Nothing to compress. Messages sent or queued during compaction
wait until it finishes without a delivery warning.
To compact before sending a new prompt, press `⌥Enter` on macOS or `Alt+Enter` on Windows and Linux in the chat box,
Option-click Send (Alt-click on Windows and Linux), or right-click Send and choose Compact & Send. Ghostex sends `/compact` first,
then puts your written prompt in the queue above the input to send after compaction.
Compact context in Session details (the context ring) sends `/compact` the same way a typed
`/compact` goes, and says why in the chat box when it can't run.
Approval and dialog cards (Claude Code, Codex and the other agents) list the
agent's choices as numbered rows: press a row's number or click it. Cmd+Enter
(Ctrl+Enter on Windows and Linux) picks the row highlighted in the terminal and
Esc picks No, or closes a dialog that has no No row. Show terminal output opens
what the agent printed above its question. In narrow chats, notice cards hide
Show terminal output; Open terminal remains available.
If a pick on a dialog card doesn't go through (the card says the agent's dialog
changed), the card gets a **Dismiss** button: close it and answer the agent in
the terminal with Open terminal. Messages you send while that dialog is still
open on the agent's screen wait until it's answered.
Ghostex trusts each project and worktree folder for you before it starts an
agent there, so sessions never stop at the agent's own "Do you trust this
folder?" question. It marks the folder trusted the way the agent would if you
answered yes (Claude Code, Codex, Cursor, Gemini, Copilot, Grok Build,
Antigravity, Pi and Factory Droid have such a question; OMP, OpenCode and Hermes
don't). If an agent asks anyway (one that never ran on this computer has no
record to mark yet), Ghostex answers yes for you when the session runs in a
project you added, a folder inside it or a worktree Ghostex made for it. Only
the folder the session runs in is trusted, never your home folder or a folder
above it, and the agent's own approvals of commands and edits still ask you. On
a connected computer, that computer's Ghostex does the same with its own
setting. To answer the question yourself, turn off **Trust project
folders automatically** in Settings > Agents > Defaults
(`trustProjectFoldersAutomatically`).
With it off, when an agent asks whether to trust the project folder, chat shows a
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
requests for approval keep their own status cards. For Claude Code, the
background card lists each command still running on its own row, named the way
the agent described it, with how long it has been running; hover a row (press
and hold on the phone) to see the command itself. When one finishes, a row in
the chat says so, with its exit code only when it did not succeed.
In an OMP chat, a finished background job is such a row too (click it to read the result),
a message from another OMP agent is a "Message from" card, and the advisor's notes are one
"Advisor shared notes" card whose severity pills (such as "1 concern") show what it holds; click it
to read the notes. On the phone the card shows the most serious note's severity and how many
other notes there are.
Chat follows the app theme by default. In Settings > Interface > Advanced, set Chat theme to Light, Dark, or System for a separate appearance, or choose Follow app to use the main Appearance (`sessionChatTheme`, `sidebarTheme`).

The chat's size follows UI Scale in Settings > Interface (see Appearance and app): Scale sizes the chat together with the sidebar, Files and the terminal text, and with Scale areas separately on, Chat zoom (%) sizes it alone, including messages, controls, and the prompt composer, from 70% to 200% in 5% steps (`sessionChatZoomPercent`). With a chat focused, Cmd+= and Cmd+- (Ctrl on Windows and Linux) step whichever of the two applies, so every chat changes size together and the new level is saved; Cmd+0 sets it back to 100%.

To search a chat, press Cmd+F (Ctrl+F on Windows and Linux) with the chat focused, including while typing in the chat box. Only what you can see is searched: text inside a folded turn, tool group, thinking or collapsed card, or the hidden part of a long message behind Show more, is skipped until you open it. Every matching word is highlighted, the current one more strongly, and the counter shows which match you are on. The search starts at the newest match, the one nearest the bottom of the chat, and counts from there (1/42 is the newest; a + after the total means older messages aren't loaded yet): Shift+Enter or the up arrow goes to the previous, older match (loading older messages when it needs them), Enter or the down arrow back toward the newest, and the chat scrolls to it. Escape or the close button ends the search and returns you to the chat box. On the phone, Search Conversation in the session's menu opens the same search.

Toggle chat and terminal for a session with one click on the pane header or
the pane hotkey. Compatible agents can default to chat. On macOS and Linux,
Ghostex can release unused terminal viewers for persistent sessions and load them
again when needed. The agent keeps running while you use Chat or another project;
returning to Terminal reconnects to the same running session. This does not sleep
the agent.
When an assistant message
has tool calls, click its text or the chevron beside it to expand the tools directly
under that message. Its full text and formatting stay visible when collapsed;
links and code controls keep their own actions. A chat in Detailed view
(More actions > View) opens these tools by default, along with
each finished turn's "Worked for" log; close
one with a click and it stays closed. A tool call the agent described (for
example "Search for config files") is named by that description in every mode,
in the reply's own font and a slightly dimmer colour, instead of the tool's name
and its command; click the row to see the full command and its result. File writes and code
edits appear outside the tool groups while the agent works. As soon as the agent
finishes a turn, its tool work folds away under "Worked for Xs" above the final
reply (open by default in Detailed view); click that
line to open or close it. When a turn shows
"Worked for", all its file changes are grouped in a collapsed "N files changed"
section directly below it. Older history loads with completed turns already
collapsed, so you can scroll through prompts and answers without passing through
their tool logs first. Expanding older work or its files loads those details on
demand; the original history remains available. The count includes each file once, even if it was
edited repeatedly. File changes show as one card per message (or per turn in
that section): its header says how many files changed, the folder they share,
and the total added and removed lines with a green/red ratio bar, and on the
desktop Review all opens the project's Changes view to review them. Each
file is a row with its name, its folder, an Edited, New or Deleted tag, its
green/red counts and a five-block change bar. Click a row to open its diff,
with the file's own line numbers and the function each change sits in (for
Claude sessions); under the first seven lines, Show all N changed lines shows
the rest and Open file opens it in Editor or Files, just like a file reference
pill; Copy path and Open file location sit beside it. Enable Show file edit previews in Settings > Chat to open every row to its
first seven lines by default. Local folder links in desktop chat open your system file explorer; remote
folder links open in this computer's VS Code view so you can browse the remote files.
A link to an app shows it in the file explorer instead of starting it, and a link
to a program, script or installer asks before opening it (Show in Finder is the
default; Open Anyway opens it).
File reference pills in the composer also open with one click using the same
VS Code/Files preferences as transcript links. Double-click a composer pill to edit
its reference text. Every file the chat shows has Copy Path and the location row:
right-click a file reference, a file edit row, a tool row that names a file, a
picture or an attachment for Open in VS Code, Open in Files (every file except
folders and media), Copy Path, or the location row, and each file in Files and links has
copy and location buttons. The location row is left out for a remote session's
files (they are on the other computer) and in Ghostex web; on the phone, hold the
file for Copy Path.
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

More actions > View picks one view for the chat: Summary, Normal or Detailed.
Picking one turns the others off, and the View row names the one in use.

Normal is the view new chats start in. Tool groups
without a message above them collapse to a tool-call count, tool rows hide
command previews, and the work under each reply stays folded until you open it;
expand a tool to inspect its full input and result. File edits
collapse under "Edited 1 file" or "Edited X files", counting each path once; expand
the row to see the usual file and diff cards. With Quiet View on (the default),
Normal folds the work between two replies into quiet lines. Detailed is Quiet View
with every line open, so the tool calls and file edits between replies all show and
switching between Normal and Detailed only opens or shuts lines; you can still shut a
line by clicking it. Summary is described below. The view is kept
per chat, so a chat you set to Detailed stays Detailed, and it follows you: the
computer that runs the session keeps the view you picked last on any device (the
View level and the Quiet View choice), so the phone, the desktop and Ghostex in a
browser open that chat the same way. A device takes it when it opens the chat, and
a chat already open elsewhere follows within a few seconds; one that could not reach the
computer keeps its own pick and hands it over later, and the newest pick wins. A
computer on an older Ghostex keeps no view, so each device keeps its own. Turning Summary off, by
any route, always goes back to Normal. To have new chats start in Detailed, turn
on Settings > Chat > Advanced > Default to Detailed view (`sessionChatDetailedView`).

While a chat is in Summary or Detailed view, a "Summary view" or "Detailed view"
chip sits at the left of the strip along the top of the chat box; hover it and
click its X (or click the chip) to go back to Normal. A chip alone is enough to
show the strip. The phone app has the same View choice in its chat box's More
actions, the same chip (its X always shows; tap the chip), and the same Default
to Detailed view switch in its Settings > Advanced.

While the agent is working, its thinking shows as a closed "Thinking…" card with the newest finished line beside it, so it never pushes the rest of the chat away; click (or tap) the card to read the whole thought, which keeps the newest lines in view as they arrive, and click again to close it. The commands the thought runs stay in view below the card, and when the turn ends everything folds into its "Previous turn" card as before. This works the same for every agent, on the computer, the web and the phone.

Show thinking in grey (Settings > Interface > Text, off by default) shows the agent's thinking in grey, so it reads apart from the agent's replies, which Text strength sets how bright (`sessionChatMutedThinking`); the phone has the same switch in its Settings > Chat.

Tool call lines (Settings > Extensions > Experimental, off by default; on the phone, Settings > Chat > Tool call lines) draw each run of an agent's tool calls as one quiet line with no box: its tools' icons, the running call's own words while it runs (with "+2 more" for the run's other calls), then the call's words for a run of one or "Used 3 tools" for a longer run, with a red "1 failed" count and the time. Click (or tap) the line to see its calls as rows, each in its tense with its result and time and the reason for a failure; Detailed view opens every line and Simple keeps them shut, and a call waiting for your approval opens its line. Opening a call shows its output's first 12 lines and a Show all button for the rest. A run that starts a background task, or reports one ending, stays as rows (`chatToolCallLines`).

A call that is still running shows a light sweeping across its words in every look: the tool rows and Tool call lines.

Chat look (Settings > Chat > Advanced) has five opt-in settings for a quieter, Slack or Claude Code style chat. With all five at Default the chat looks as it always has. Message layout set to Left aligned starts every message at the same left edge instead of putting yours on the right. Your messages set to Band puts yours on a soft full-width strip and also left-aligns them. Status line in the chat shows what the agent is doing as the last line of the chat instead of above the chat box. Chat box matches transcript width makes the chat box as wide as the transcript when Transcript width is set to a percentage (computer only). Message spacing set to Compact puts messages closer together and keeps a small row under each message for its time and ⋯ menu; on the phone that row shows the time. The phone follows the computer's choices for all but the chat box width, and picks up a changed look the next time a chat opens or catches up after a lost connection (`chatMessageLayout`, `chatUserMessageStyle`, `chatStatusInTranscript`, `chatComposerMatchesTranscriptWidth`, `chatMessageSpacing`).

Quiet View makes a chat read as the conversation: your messages, the agent's replies, Ghostex pages and forms, questions and your answers, approvals waiting on you, errors and messages that were not sent stay in view, and everything between two of them (reports from threads, messages sent to them, tool calls, the line that says which work items changed, turns that ended without a reply) folds into one quiet line that counts it, for example "3 reports · 1 message sent · 3 tool calls · 1 item updated". Click the line to see all of it in order, as the full chat shows it; an open line drops its counts and says only "Working", how long the agent worked, or "Reports", since what it holds is in view. Click it again to shut it. While the agent works the line counts up instead of the cards piling up. Every chat opens in Quiet View; More actions > View > Quiet View turns it on or off for any chat, and the chat remembers that choice. When a chat's view differs from its default, a small Quiet view or Full view chip at the left of the chat box's pill row switches it back in one click. Summary view, which folds more, wins while it is on, and Detailed view is Quiet View with every line open. Agent work Reduced and Quiet and the Tool call cards preview are gone: Quiet View replaced them, and a chat that used one of them opens in Quiet View. Settings > Chat > Advanced has the defaults: Coordinators open in Quiet View (on), Other chats open in Quiet View (on; turn it off to have other chats open in the full Normal view), and Expand the last turn in Quiet View (on), which shows the work after the newest reply, or the running turn, open until a newer reply comes. Quiet View is on the computer, in Ghostex in a browser and on the phone, which follows the computer's three settings and has the same Quiet View row in its chat box's More actions > View; with a computer on an older Ghostex the phone shows the full chat (`chatQuietViewCoordinators`, `chatQuietViewChats`, `chatQuietViewExpandLast`).

Marked handovers: a coordinator's threads begin each part of the report
they hand back with a marker (a coordinator itself replies with a few sentences of news
until you turn the setting below on; its work items show above the chat box and in the Work panel either way, see Coordinators). What happened comes first: ✅ Done, 🚀 Shipped,
📣 Comms (replies sent outside, such as GitHub or Discord), 🛫 Kicked off (work started this turn), 🔄 In progress (running now),
📦 Held for release and 🧪 Untested. What needs your attention comes last, so it is the
last thing you read: ⚠️ Read this, 🛑 Blocked, 💡 Suggestions and, at the very end, ❓ Needs you. A section shows only
when it has something, and every bullet under a marker starts with a capital letter; a thread's report to its coordinator is marked too. Turn on
Marked handovers in every session (Settings > Chat, off by default) to have
your coordinators and every other agent session do the same. A coordinator then
writes each reply as these sections with bullets under them, with no Needs you
section for a question that already waits above the chat box. The agent otherwise writes as usual. It
reaches Claude Code, Codex and ZCode (and every coordinator, through its guide). A change applies
to sessions started or resumed afterwards, and to a coordinator from the next message it gets; a running agent keeps the
instructions it started with until it is resumed. The chat colours each marked section by
its kind, in the style picked under Marked sections style right below the
switch (shown while it is on): coloured labels (the default), a thin left rule, or a coloured
badge naming the kind; the eye button next to it previews all three side by
side. Show marker emojis, under it and off by default (an advanced setting, not recommended), adds the marker emoji
to every style; while it is off the colour and the label show the kind.
Copying a message still gives the agent's own text. The phone app sets both in
its own Settings > Chat > Marked sections, which also offers Plain text.
Heading colour strength (Settings > Chat > Marked handovers, under the style, an advanced setting, 0% to 100%, default 100%) sets how strongly the coloured headings stand out: lower values fade them toward the normal text colour, and 0% leaves plain bold text; the phone follows the computer's value.
Coordinators answer without markers (Settings > Chat, an advanced setting, off by default and not recommended) keeps coordinators answering in plain prose while Marked handovers in every session is on, while their threads still mark the reports they send them.
Settings: `markedHandovers`, `markedHandoversStyle` (`labels`, `rule`,
`badge`), `markedHandoversEmoji`, `markedHandoversHeadingStrength`, `orchestratorPlainReplies`.

Tell agents about Ghostex (Settings > Agents > Defaults, on by default) gives every agent
session Ghostex starts a short note that it runs in Ghostex: Ghostex can show
Ghostex pages, Ghostex forms and Ghostex visuals in the chat, open pages the agent
builds in its own browser, run other agents and install Ghostex skills. The
agent uses it only when you ask for one of those things, and then reads the
details from `ghostex agent-guide`. It reaches Claude Code, Codex, ZCode, Pi
and Factory Droid at launch; other agents (OpenCode, Gemini CLI, Cursor and
the rest) get the same note from the Ghostex Agent Guide skill (`ghostex
agent-guide install-skill`). A change applies to sessions started or resumed
afterwards. Agents can also ask Ghostex Help themselves with `ghostex help ask
"<question>"`, which starts (or reuses) a Ghostex Help chat in the Ghostex
project and prints its answer. Asking again while that chat is still answering
waits for its answer instead of starting another chat, and a Ghostex Help chat
never asks Ghostex Help itself. Setting: `agentGhostexNote`; CLI: `ghostex
agent-guide`, `ghostex help ask "<question>" [--new]`.

Catch me up: turn on Catch me up button (Settings > Chat, off by default) and
the chat box shows a Catch me up button at its top right while it is empty, on
the computer and on the phone; it fades out as you start typing and back in
when the box is empty again. Clicking it has your Title Generation Agent
(Settings > Agents, a small, cheap model such as Claude Haiku or Codex's mini
model) read the session's last 30 agent messages (and your messages between
them) and opens a card above the chat box that says what
the session is working on, then what needs you, what is blocked, in progress,
done, untested, kicked off or shipped, coloured like marked handovers. It takes
a few seconds; Refresh writes a fresh one, Copy copies it, and Close or the
button again closes it. Nothing is sent to the session's own agent. If the
Title Generation Agent is not installed, the card says so; pick another one in
Settings > Agents. Setting: `chatCatchMeUp`.

Session details panel (beta): a built-in extension, on by default; turn it off
in Settings > Extensions. While it is on, a Claude chat shows a small
context ring with its percentage at the top right of the window, next to the ⋯
menu; click it to show or hide the panel of the chat you are in (one switch for
the whole app, remembered across restarts). When the chat is wide enough the
panel sits beside it as its own column and the conversation moves over to make
room; when it is too narrow the panel opens as a floating card under the ring,
and a click on the chat closes it. On the phone the same ring sits in the chat's
header and opens the cards as a sheet (Changes opens the phone's Changes view).
The panel has two tabs, Overview and Limits, the same on the computer and on the
phone. The chat box's context ring opens it too, on Overview at the Context
card, so there is no separate Context window popover any more (a chat without
the panel keeps it).

Overview holds Environment (this computer or the
box the session runs in, its folder, branch and account, the agent's version, and how much memory
the session's processes use while it runs on this computer); Changes (lines changed on the
branch against main, uncommitted edits included, with Commit, Create PR or View
PR); Context (how full the context window is, with Claude's own breakdown into
messages, memory files, skills, tools and free space; it is always open, and
Compact context sits under the breakdown; Ghostex asks a silent read-only copy
of the session, so nothing is typed into it and no tokens are spent, and it
counts again after a turn ends at most every ten minutes, or when you press the
refresh button); Tasks (Claude's task list; clicking the chat's Tasks pill opens
the panel, and the pill hides while the panel shows); Connectors (your workspace's
connected apps and every MCP server the session loaded, green when ready, amber
while connecting or needing a sign-in; Manage opens Settings > Connectors);
Skills (the ones the session used, then all of them; click one to open it in
Agents Hub); Memory files (the CLAUDE.md and AGENTS.md files Claude loaded;
click to open); Background tasks (each running command with its time and a
stop button, then the finished ones); and Agent (the agent's other settings).
Limits holds Plan limits (each limit with when it resets), Cost and time and
Cache, and Choose details, which picks the rows to show. A card's title folds
it, except Context. Claude task list (Settings > Agents > Defaults, shown while
the extension is on, on by default) starts Claude with its task list tools, which
recent Claude models only get when Ghostex asks; it applies to Claude sessions
started or woken afterwards. Claude only for now. Settings:
`sessionDetailsPanelHidden`, `sessionDetailsClaudeTaskTools`. Command:
`ghostex read-session-details <selector> [--refresh] --json`.

Files and links: More actions > Files and links (or its shortcut, Option+Ctrl+L
on macOS and Ctrl+Alt+Shift+L on Windows and Linux, changeable in Settings >
Hotkeys) opens a menu that floats above the chat box, without pushing the
conversation up, listing everything the session mentioned, on the computer and
on the phone (where it opens as a sheet). Click outside it or press Escape to
close it. Attachments
holds every picture (pasted, attached, or shown by the agent) and every file
you attached; Files holds every other file or folder you or the agent mentioned
and every file the agent created, edited or deleted (tagged New, Edited or
Deleted); Links holds every web address, with its link text. Each item is
listed once, newest first, with how often and when it was first and last
mentioned, and a filter box appears once the list is long. Click an item to
open it: a picture in the image preview, a file the way a file link in the chat
opens, a link in the browser. First and Last (or Show) scroll the chat to the
message that mentioned it; on the phone, hold a row to see them. In an
coordinator's chat the list also covers every one of its threads, done ones
included, grouped by thread title, and Open thread opens the thread that
mentioned the item. Agents can read the same list with `ghostex
read-session-files-links <session> --json`.

Summary mode folds each turn down to your prompt and a closed "Agent reply" row;
older ones open with a click and hold every reply the agent gave to that prompt.
Under your newest prompt only the last reply is open, with the earlier replies and
reports behind one closed line above it, such as "2 earlier replies · 1 report".
While the agent works, that turn's Active work shows open the way Quiet View shows
a running turn: the agent's words, pages, questions and approvals stay in view,
and each run of tool calls, thinking and reports is one line that counts it
("Working · 3 tool calls so far"); Expand the last turn in Quiet View opens the
newest of those lines. Work the agent does after a reply in the same turn, when a
thread's report or a background task wakes it, shows there too. Ghostex pages and
Ghostex visuals stay in full under their prompt, never folded, and so does a
Ghostex form or question until you answer it; a strip of the pictures the agent
opened sits under the closed reply, and "Interrupted" and command output lines
stay in view. Work item cards and forms you already answered fold away. Messages
to and from other agents (a coordinator's thread reports, messages from other
sessions) open no turn of their own: they fold into the "Agent reply" of your own
prompt, so the summary reads as your conversation.
The phone's Summary view looks the same, on a computer on an up-to-date Ghostex.
Pick Summary under More actions > View, where its row shows the
Toggle Summary Mode shortcut (Option+Ctrl+S on macOS, Ctrl+Alt+Shift+S on Windows
and Linux), which switches it from anywhere in that chat and can be changed in
Settings > Hotkeys.
Stash prompt is in More actions > More: it stashes what is in
the chat box, or, with an empty chat box, opens your stashed prompts; the row
shows how many are stashed. While this chat has stashed prompts, a Stashed pill
with their number sits in the pill strip along the chat box's top edge, after
Resources, Workers and the other pills (before the Links pill); click or tap it
to open the stashed prompts (Saved Prompts on the computer, Saved prompts on the
phone). Alt+S (Option+S on macOS) is the Stash Prompt shortcut in a chat too.
The Maximize button's tooltip shows the Maximize Chat Box shortcut (Option+Ctrl+M
on macOS, Ctrl+Alt+Shift+M on Windows and Linux), which maximizes the chat box of
the chat you are in, or restores it. The Prompt Editor shortcut (Ctrl+G on macOS,
Ctrl+Shift+G on Windows and Linux) does the same while you are in a chat: it
maximizes the chat box with your text in it, and pressing it again restores it.
As space gets tighter, toolbar buttons move into More actions one at a time:
Session note, Attach, Maximize, then Terminal View. The microphone never moves:
it always stays right next to Send.
If the context ring still does not fit beside the model, it moves into More
actions > More as Session details (or Context window in a chat without the
panel); the model pill shortens instead of moving.
Controls return as space opens up; More actions, Dictate and Send or Stop stay visible.
On the phone, More actions > More > Stash prompt with an empty chat box opens
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
first, in alphabetical order, then the rest in their usual order),
and along the bottom a button each for the reasoning level (brain), the context
window (chart bars) and Fast mode (bolt). Clicking the context window or Fast
mode button switches it; the reasoning button opens a short list to the side. A
button the current model has no choice for is dimmed: reasoning and context
window read Default, and Fast mode reads Off. Grok Build's fast model (Grok 4.7
Fast) is not its own row: pick Grok 4.7 and switch Fast mode on or off. Auto, where an agent offers it,
sits at the top of that agent's list, below any starred models. A tab with more
than six models (OpenCode, Pi and Cursor, for example) has a search field above
the list: type part of a model's name or id ("sonnet", "gpt 6") to narrow it,
with matching starred models still first. Older versions of a model (GPT 6 Sol
once GPT 6.1 Sol is out, for example) sit under Legacy models at the bottom of
the tab; click it (or reach it with the arrow keys and press Enter or Right) to show them. A starred legacy model also stands at the top of the list and stays in this list with its star lit. Codex,
Cursor, Grok Build, Antigravity and OpenCode's lists come from the agent installed on that computer, so
new models show up without a Ghostex update. The OpenCode tab is there in every
thread and lists the models of the providers you set up in OpenCode; an OpenCode
thread lists the ones its own folder can use. Click a row's star to keep that
model on the Favorites tab; hover the info icon that appears on a row to read
what that model is for. In a session that has started, another agent's tab
shows a small handoff badge: picking one of its models hands the conversation
off to that agent instead of changing this session's model. In a new thread
you have not sent anything in yet, it switches the thread to that agent instead,
and an OpenCode model becomes the thread's model as soon as OpenCode is up. With more than one signed-in Claude or Codex account, an Account
button beside Fast mode shows the account in use and opens the list to switch.

The Model & Effort Picker shortcut (Option+P by default on macOS) opens the same
picker from the keyboard, and pressing it again closes it. Up and Down move through the models and then the bottom buttons, stopping
at the top and bottom; Left and
Right change the highlighted model's reasoning level, which the reasoning button
shows; the letter on each bottom button's icon uses it (R Reasoning, C Context,
F Fast mode, A Account); Tab and Shift+Tab move through the Favorites and agent tabs. Enter applies the highlighted model and level and
saves them as the agent's default, Option+Enter uses them in this session only and
leaves the default alone (either one closes the picker, as does clicking a model), and Option+1 to Option+9
jump the highlight to one of the first nine rows without applying it. On a tab
with a search field, typing goes into the field (so the bottom buttons' letters
type instead) and the arrows, Tab and Enter still work on the filtered list.
Escape clears the search, then closes the picker without changing anything. The key reminder along the bottom lists these.

New Claude Code and Codex sessions start on the model and reasoning level you
last saved as that agent's default, so the pill names it as soon as the chat
opens; a choice for this session only never becomes the default. Cursor CLI has
no session-only choice, so a new Cursor session starts on the model you used
last; when Cursor shows that model only by a name its `--model` doesn't take
(Auto with the Balance setting shows "Auto Balance"), Ghostex leaves the model
to Cursor, which starts on the same one. Each conversation remembers its own
model and reasoning level: one you wake or resume comes back on what it was
last using, including a choice made for that session only or with `/model` in
its terminal, and its pill shows it as soon as you open it.
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
refuses the model, the picker shows its reason under Not applied; hover a long
reason to read all of it. A new Hermes chat gets the
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
agent's default alone. If the agent refuses a level, the picker shows its reason
under Not applied; hover a long reason to read all of it.
Until Ghostex has read the agent's model list, the pill shows the model with
Change it in the CLI.

A Pi or OMP chat learns which conversation is its own from Ghostex's Pi or OMP
extension, which Install hooks adds in Settings > Agents. If the agent runs in
its terminal but the chat stays empty, after about 20 seconds the chat says why
and offers Install and restart (adds the extension and starts the agent again in
a new conversation), or Restart when the extension is installed but the running
agent didn't load it, plus Open terminal, where your messages did arrive.
Ghostex finds a `pi`, `omp` or other agent installed with npm even when only
your shell's startup files put it on PATH: in nvm's default Node, fnm's default,
the npm `prefix` set in `~/.npmrc`, `~/.npm-global`, bun and pnpm.

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
When a model change holds messages, the phone shows a card above the chat box
with how many are waiting, why, and what releases them. **Pick model** opens the
same model sheet; **Details** shows the diagnostic behind the plain explanation.

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
shows up in the same chat on your other devices as a Saved draft pill at the
start of the pill strip above the chat box (it shortens to Draft, or just its
icon, when the chat box is narrow). Click (or tap) it to preview the draft, then
choose Use draft to load it into your input, or Dismiss to keep what you have.

Settings > Accounts saves Claude and Codex logins, and shows the usage of
Cursor, Gemini and Copilot accounts. The left side has one **Add account**
button over each provider's accounts, each with a usage bar (or "Sign in
needed" when a login stopped working: its Reconnect is on the right), and the
Claude and Codex rules; click one
to see it on the right: its usage bars with reset times (labelled 5 hours, 7
days and Fable), Name, **Use this account** (Automatically: Ghostex may start sessions on
it and move sessions to it at a limit; Only when I pick it: automatic switching
skips it; Paused), Show usage in the sidebar, and under Advanced the Session
badge (up to two letters or numbers on its session icons) and its Account
number, which you can swap with another account's. A Claude account also shows
**claude.ai in Ghostex**: whether its own Browser profile is signed in to
claude.ai for its cloud sessions and the Claude view, with **Sign in to
claude.ai** or **Sign out** (see Claude cloud sessions). Changes save as you make
them; the account's ⋯ menu holds Sign in again and Remove…, which asks first and
says how many sessions use the account.
A newly added account starts with Show usage in the sidebar on. The accounts'
usage sits pinned at the bottom of the sidebar by default, as the last thing in
it. To unpin it, point at the meters and click the small button that shows
above them, or right-click a meter and choose Hide account usage. It then shows
only while you point at the usage icon, which appears at the bottom right when
the pointer is at the bottom of the sidebar; click that icon to pin it back.
Ghostex remembers your choice, and
adding an account pins it again so you can see where the new account went.
Set Use this account to Paused to keep an account you no longer use (for example
one whose subscription ended) saved but out of use: it shows dimmed with a
Paused tag, Ghostex never starts new sessions on it, switches to it at a limit or
offers it in Switch Account menus, stops reading its usage and sign-in state (so
it never asks you to reconnect it), and drops its usage from the sidebar.
Sessions already on it keep running, and the account says how many still use
it. A sleeping or restored session on it moves to another account when it wakes
(your account for new sessions when it works, otherwise an account set to
Automatically with the most room) and carries on the same conversation; with no
account to move to, the wake says the account is turned off. Set it back to
Automatically or Only when I pick it to use it again. This is separate from
"Subscription access disabled", which Ghostex sets on its own when Claude
refuses an account.
Reconnect is one click: your browser opens on the provider's sign-in page with
the account's email filled in, Settings shows "Waiting for you to sign in…" (a
box for a sign-in code appears only if the browser shows one, and the sign-in
log stays under Details), and the account shows Connected with fresh usage when
you finish. If the browser signed in to another account, Settings names it and
offers Try again or adding it as a new account. To add an account, click **Add
account** and pick a provider: your browser opens on its sign-in page right
away, and Settings waits with Copy link and Open browser until you finish, then
shows the new account. Nothing needs typing first: pick the account in the
browser (Codex needs Codex Swap 0.3.6 or later for this; Settings offers the
update). Claude and Codex accounts always share chat history between a
provider's accounts, so a conversation carries on with another account when one
runs out. Logins that Claude Swap or Codex Swap already keep on the computer are
listed under the providers with an Add button.
**Cursor, Gemini and Copilot** accounts only show usage: Ghostex never runs or
switches a session on them. Cursor and Gemini sign in in the browser; Copilot
shows a code to type on GitHub's page. Cursor and Copilot show their plan's use
of the billing month, labelled Mo (with Cursor's Auto and API split and extra
usage spend, and Copilot's chat and premium requests, on the account's page);
Gemini shows the 5h and 7d limits of the Google AI plan as Antigravity (agy's
`/usage`) counts them: the Gemini models' limits, and Claude/GPT 5h and 7d for
the Claude and GPT models agy runs; the sidebar shows whichever is more used.
Ghostex shows what is used, agy's `/usage` what is left (74% used is agy's
26.00%). Their usage shows in
the sidebar, on the account's page, and under Other usage in a chat's Accounts &
limits panel on the computer and the phone; clicking one in the sidebar opens
Settings > Accounts. Their ⋯ menu holds Sign in again, Turn off and Remove….
Each Codex login is its own account with its own name and settings, also when
two people sign in to the same ChatGPT Business workspace or one person has
several workspaces; this needs Codex Swap 0.3.2 or later (its Update is in the ⋯
menu at the top of Settings > Accounts), and accounts saved earlier keep
their names, settings and sessions.
Quick launch, the main launcher row, and any session started without
picking an account use New sessions start on in each provider's rules (Claude
rules or Codex rules in the list). The automatic rules only consider accounts
set to Use this account Automatically: Auto (the default) weighs remaining limit against time to reset and
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
`~/.agents/skills`, which Codex profiles read on their own; for a Claude
profile, link its `skills` folder to `~/.claude/skills` to use them.
A custom agent that starts Claude Code or Codex through Wally
(`wally claude-code -m <model>` or `wally codex -m <model>`) takes no account
either: Wally runs it on its own key and model.
The ⋯ menu at the top of Settings > Accounts holds Refresh accounts, How
accounts work, and each account tool, Claude Swap (cswap) and Codex Swap (xswap),
with its version, Update (when a newer release is out, otherwise Check for
updates), Reinstall and Uninstall…, which asks first and keeps saved logins and
shared conversations. Ghostex runs the update with the tool that installed it
(uv or pipx for cswap; Homebrew, Cargo or the installer script for xswap); for
one installed some other way those items are off, so update it the way you
installed it.
For now Ghostex installs its own build of Claude Swap on every computer: the
released Claude Swap 1.0.0 plus two fixes that are waiting to be released (sessions
on an account whose sign-in on the computer was lost start on that account's own
saved login instead of stopping at "Login expired", and on Windows no stuck
`git.exe` processes are left behind). Its version ends in `+ghostex`. Install and
Update in Settings > Accounts use it, and Ghostex moves any other Claude Swap on
the computer to it on its own. Once those fixes are released, Ghostex goes back to
the regular Claude Swap the same way.
On Windows, running Claude sessions keep the installed Claude Swap in use, so an
update, reinstall or uninstall is prepared at once and applied on its own as soon
as they have all closed (after a restart of the computer at the latest); until
then the Claude Swap line says so, offers no buttons, and new sessions keep working.
Before sending a session's first message, use the model menu's Switch Agent CLI
to change between Claude and Codex. The new agent uses its New sessions start
on rule, just like the sidebar agent button, while the terminal and your
unsent chat text stay in place.
Sign-in and usage-limit notices in Claude and Codex chats offer Switch account
beside Open terminal, so you can choose another account directly from those notices.
When a Claude session can't start because its account has no saved login on this
computer, its chat shows a card naming the account with Switch account, Sign in to
that account (it runs `ghostex account-login claude --account N` in the session's
terminal and starts Claude again when the sign-in finishes) and Try again. A message
you sent waits in the chat and goes out on its own once Claude is up. Waking a
session on such an account (or on one Claude refused) moves it to an account
that works first, on the same conversation, unless you stopped automatic
recovery for that session; the session shows the switch like any other.
When any agent's start command ends before the agent comes up (the command
printed an error and the terminal is back at a shell prompt), its chat shows "The
agent couldn't start" with the error it printed (for example Cursor CLI's
"Cannot use this model"), Try again and Open terminal,
instead of waiting at starting. A message you sent waits in the chat and goes out
once the agent is up.
The same works from the command line, for you or an agent: `ghostex accounts`
lists every Claude and Codex account with its state (ready, limit reached, needs
sign-in), its usage per window with reset times, the default for new sessions
and the sessions on it; `ghostex agents switch-account <session> --account
<number or email>` moves a session to another account and resumes its
conversation there, also when it could not start, and refuses an account that
needs signing in (and a session in the middle of a turn, unless `--force` ends
that turn and sends "." afterwards so it carries on); `ghostex create-agent <claude or codex> --project-id <id> --account <number or email>`, in versions after 11.3.0, starts a new session on that saved account instead of the New sessions start on rule, and refuses one that needs signing in; `ghostex agents reload <session>` stops a stuck session's
agent and starts it again on the same conversation, losing what its current turn
was doing. A coordinator uses them when a thread can't start or gets stuck,
and tells you what it did.
In the chat's More actions menu (under Agent), and in the ⋯ menu of the bar under a Claude or
Codex terminal view, click Switch Account to open its submenu of saved accounts
with their usage; hovering over it does not open it. Open submenus stay open when you move the
pointer across other menu items, so you can move into them without rushing.
In the chat's Accounts & limits panel the account the session is on carries an Active tag,
and a problem with it (a login that expired or needs signing in, a usage reading that
failed, a limit reached, the account turned off) shows on that row, with Reconnect
when signing in again fixes it.
Switching a running Claude or Codex session to another account, from More
actions > Switch Account, a terminal notice, or automatically when its account hits a usage limit,
exits the CLI inside its own terminal and resumes the same conversation there,
so the terminal tab and the chat stay open. Switching dismisses open CLI dialogs
and interrupts the current work until the agent exits, then resumes your saved
conversation on the selected account. For a Claude background session,
switching stops that conversation and its remaining jobs before resuming its
saved history on the selected account. A card in the middle of Session Chat,
over a dimmed conversation, shows the current and selected accounts, their usage
percentages (including Claude's Fable limit), and the switch progress. Renamed
accounts appear as email - name; accounts without a custom name show just their
email. Hide emails also applies to these labels. The card stays
until the new account is confirmed and the conversation is ready on it, then
briefly confirms success, or shows a failure with Retry switch and a close (X)
button in its top-right corner that dismisses it everywhere. On phones and
narrow chat panes the card uses a compact layout with one row of usage pills per
account and a short vertical step list.
A manual switch waits for your next
message without sending anything. An automatic switch sends a "." to continue
the interrupted work once the new account is ready. Configured recovery after
errors can also continue work on the same account.
Whether a session keeps going at a limit comes from When an account hits its
limit in the provider's rules in Settings > Accounts: Switch accounts (the
default) moves the session to another account set to Automatically, Wait for
reset picks up on the same account when its usage resets, and Do nothing leaves
the session stopped. With Switch accounts, Try first picks which account Ghostex
tries first. Every Claude and Codex session follows those settings as they are now,
including sessions that are already open, forks, and restored sessions, so a
change there applies everywhere at once. To make one session behave differently,
open More actions > Agent > Switch Account and click Customize under Keep going at a
limit; Use session defaults returns it to the Settings values.
When a Claude or Codex turn ends on a lost connection or a server error (such as
"API Error: Can't reach the API server"), the chat shows it as a card and
Ghostex resumes the turn by itself once the network is back, after 5s, 15s, 1m,
2m and 5m, then on the slower schedule; the card's Retry now and Stop retrying
buttons override it, and Retry after temporary errors in Settings > Accounts
turns it off. If all five quick retries fail, Ghostex stops retrying without
getting past the error, or retrying is switched off while the error stands, the
bell gets one Failed notification for that error, and only one even if the agent
reported the same error itself, until you send the agent a new message (from the
chat, or by typing it in the agent's terminal) or it works normally for a minute;
Ghostex's own retry messages do not count as new. When Ghostex moves the session
to another account by itself, that is still the same error; picking another
account yourself starts over. A usage limit and Stop retrying never send one.
A sleeping session whose account is already at its limit wakes on another
account that has room. When every account set to Automatically is full, the
session's chat shows "7d limit reached on every account" (or whichever limit
it hit) with when it continues on its own, and the bell gets one notification;
both go away once the session switches or resumes.
Account sign-in terminals open in the active local project's folder and appear
under that project. Before a first project is chosen, sign-in uses the home folder.
When a newer usage reading cannot be fetched for a Claude account, for example
while the usage service rate limits checks for an hour at a time, Settings >
Accounts, the account usage popup, and the chat's Switch account rows show the
last reading with its age ("Usage is from 3 hours ago") and Ghostex keeps
retrying on its own. Automatic switching and the New sessions start on rule
skip that account until its usage refreshes. A login problem shows what to do
instead, such as "This account needs to sign in again. Reconnect it." A login
whose access is only waiting to renew (a session running on that account renews
it the next time it is used) is not a login problem: the account stays usable,
its row says so without a Reconnect button, and a usage reading from the last
ten minutes still counts for automatic switching.
A newly added account starts set to Use this account Automatically; set it to
Only when I pick it in Settings > Accounts to keep that account out of automatic
switching.
When Claude says "Your organization has disabled Claude subscription access",
that account cannot be used any more: Ghostex sets it to Only when I pick it (the
account shows "Subscription access disabled"), and the next message you send to
a session on it first moves the session to the best account set to Automatically,
resuming the same conversation. When the rules say Switch accounts or Wait for
reset, Ghostex moves the session by itself and continues the message that
failed. Set it back to Automatically, reconnect the account, or pick it for a
session when its access works again.
When a session's login stops working mid-conversation ("Login expired · Please
run /login"), the chat's sign-in card offers Reconnect beside Switch account.
For a session on a saved account it opens the same browser sign-in as that
account's Reconnect in Settings (or skips it when the account's saved login
has already worked again since the error), and when you finish, the session (and any
other session on that account showing the same card) restarts on its own
conversation, so you don't retype anything. The account shows Sign in needed in
Settings > Accounts and `ghostex accounts` from the moment a session reports
the error, and new sessions and automatic switches skip it until it signs in
again or one of its sessions answers again. A Claude session on its own login
(not a saved account) gets Reconnect as `/login` typed into Claude, whose
sign-in steps then show in the chat.
Reconnecting a Codex account works while that account's sessions keep running.
When Codex still has to stop first, Settings lists the sessions in the way and
offers Pause sessions and continue; paused sessions stay in the sidebar and
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
`sessionChatTranscriptWidthPercent` (Settings > Chat shows them as one Transcript width
slider that reads Auto until dragged; a percentage is the widest the transcript gets, taken of
the window, so a side panel you open takes the empty space beside it first), `sessionChatDetailedView`,
`sessionChatFileEditPreviews`, `sessionChatMutedThinking`,
`sessionChatKeepComposerExpanded`, `sessionChatConfirmEscapeInterrupt`,
`terminalViewWidthMode` (`match-chat` makes the terminal body the same width
as the chat transcript), `terminalWidthApplyToCommandPaneTerminals`.

## Terminal

On Windows, Settings > Terminal > Windows environment selects native
PowerShell projects and agents or Linux projects in WSL. PowerShell is the default;
an explicitly saved WSL choice is preserved. PowerShell uses Windows folders and
installed Windows agent CLIs without requiring WSL. Switching environments opens a
restart dialog: choose **Restart now** to apply the change or **Later** to keep
working until the next app restart. Projects and running sessions remain in their
original environment. Native sessions stay alive when the app closes or gxserver
restarts. The VS Code view uses the same environment: native Windows folders in
PowerShell mode and Linux folders in WSL mode. The matching editor downloads the
first time you open the VS Code view. Sessions and agents on Windows always run with
standard user rights, even
when an administrator account connects over SSH, because Codex refuses to run as
administrator. For a single command that needs administrator
rights, turn on `sudo` in Windows Settings > System > For developers and run
`sudo <command>`, then approve the prompt on the Windows desktop.
Native Windows terminals use PowerShell 7 when it is installed (in Program Files,
your user folder, from the Microsoft Store, or anywhere on PATH; a stable release
is preferred over a preview) and Windows PowerShell 5.1 until then, so nothing
breaks on a computer that only has 5.1. When only 5.1 is present, Settings >
Settings > Terminal shows a **PowerShell 7** row with an **Install PowerShell 7**
button that runs `winget install --id Microsoft.PowerShell --source winget`
(Windows may ask you to allow the installer; without winget the row links to
Microsoft's download page). New terminals and agents use PowerShell 7 right after
it is installed; terminals that are already open keep their shell.
Keys: `windowsTerminalBackend`, `windowsWslDistribution`.

Terminals are embedded Ghostty surfaces with their own config file, separate
from the Ghostty app: Ghostex never reads or changes Ghostty's own config. Font,
theme, cursor, padding, scrollback, clipboard, and scrolling are Settings >
Settings > Terminal rows and are written into Ghostex's terminal config
(`~/.config/ghostex/ghostty/config.ghostty` on macOS and Linux,
`%APPDATA%\Ghostex\ghostty\config.ghostty` on Windows). The Ghostty settings
actions row applies the recommended set, resets the defaults, opens the Ghostty
app's config or Ghostex's terminal config, and has **Import from Ghostty**, which
copies your Ghostty fonts (family and fallbacks, size, weight, bold and italic
families, font features), line height, letter spacing and cursor style into
Ghostex; it never imports the theme or colors. Bold text is drawn in a heavier
font while **Bold text uses a heavier font** is on (the default); turn it off and
bold text keeps your normal font weight, so it stands out by color only.
Ghostty's own bold lines in Ghostex's terminal config work too, and the two
weight lines take priority over that switch: `font-style-bold = false` keeps bold
at the normal weight, `font-variation-bold = wght=500` picks the bold weight, and
`bold-color = bright`
draws bold text in the bright version of its color (`bold-color = #ffaa00` uses
that color instead). When you first updated to this
version, a terminal look Ghostex had been taking from your Ghostty config was
copied into Ghostex's terminal config once, so nothing changed; the old "Ghostex
managed terminal settings" section in Ghostty's config was left in place and you
can delete it. Command-click opens links (Ctrl-click on Windows and Linux).
A program can show any text over a link, so a link that opens another app
rather than a web page, an email or a file (such as `vscode://`, `slack://` or
`ghostex://`) shows where it goes and asks "Open a link in another app?" first;
Cancel is the default, and Open hands it over. A link printed in a terminal on
another computer still opens on this computer, and the question says so.

Terminals on the computer draw pictures that programs show with the Kitty
graphics protocol, as Ghostty does: agent CLIs that print diagrams or charts,
image viewers, and TUIs that place pictures with Unicode placeholders. Pictures
scroll with the text and go away when the program deletes them or clears the
screen. Programs must send the picture's data in the terminal output (the usual
way); pictures a program asks the terminal to read from a file are refused, and
programs then send the data instead. The web terminal doesn't draw pictures yet,
and a picture already on screen is gone after Ghostex reattaches to the session.
Cmd+V pastes images as previewable links. On Windows and Linux, Ctrl+V pastes
outside a terminal; in a terminal Paste is Ctrl+Shift+V, as in Ghostty, and
Ctrl+V goes to the program there (an image on the clipboard still pastes as a
link). Ctrl+Shift+V pastes from the client computer's clipboard, including into
a remote terminal. Change or clear either Paste key in Settings > Hotkeys. Shift+Insert also pastes, in terminals
and in text fields such as the chat composer and dialogs. A configured hotkey
using the same chord takes precedence. In an agent's terminal prompt, use
Prompt Editor in the terminal toolbar, Ctrl+G on macOS, or Ctrl+Shift+G on
Windows and Linux (in chat view the same shortcut maximizes the chat box) to open the Ghostex prompt editor or your machine default
editor for long prompts. Remote sessions open it in that computer's VS Code view.
The agent must be at its prompt and support an external editor; an idle shell
does not provide that agent prompt-editor action. The Ghostex editor uses the
same text editing controls as the chat composer, with F1 commands, find/replace,
undo/redo, and image previews. Cmd+S/Ctrl+S or Ctrl+G saves and closes it; Cancel
leaves the original prompt unchanged. In remote Code, auto-save is off for prompt
files. Save your edits, then close the prompt file to return to the agent. To
cancel, close it without saving and choose Don't Save if asked. Dev Servers detects
localhost URLs from output and lists them in the Dev servers panel (Tools in the sidebar's menu).

Codex 0.157 and later draw the conversation full screen by default, so its
terminal keeps no scrollback of its own. The mouse wheel, PageUp and PageDown
scroll Codex's own view, dragging selects text inside Codex (Codex copies it when
you let go), Shift+drag makes a normal terminal selection of what is on screen,
and Command-click still opens links in Ghostex. Stop in the chat works even while
you are scrolled up or have text selected. To go back to the terminal's own
scrollback, type `/tui` in Codex, choose Scrollback, then restart the session
(Sleep and Wake it, or Full Reload).

Terminals follow the app theme by default. The Interface page in Settings holds
Appearance, and its Advanced part holds Chat theme and Terminal theme.
Terminal theme can override the app with Light, Dark, or System. The palette
selectors show the theme set in Ghostex's terminal config, including separate light
and dark selections when configured. A single theme is used for both appearances
unless you select a separate light palette. Without a configured theme, the defaults
are GitHub Light and GitHub Dark. All open terminals refresh automatically when their
app, system, or terminal theme changes, including idle terminals and terminals in
inactive projects. The appearance override and light palette apply to Ghostex only.

Terminal background (Settings, Terminal) is Black / white by default: pure black
behind dark terminals and pure white behind light ones. Choose Follow theme to use
the theme's background color instead, or Custom color to pick your own for both
dark and light mode. A theme preset (Settings > Interface > Presets) sets Follow theme.

Terminals are see-through only while the window is: set Transparency to Dark only
or Always (Settings, Interface, Transparency; it starts Off on Windows and Dark only on
macOS and Linux). Background opacity (Settings, Terminal) then sets how much of the
work area's tint terminals keep: 100% matches the rest of the work area, lower
shows more of your desktop through them. While transparency is off the row offers
Turn on transparency instead. The window's Blur (on Windows with Desktop and windows,
the Acrylic, Plain blur or Off choice) is set on the Interface page only (Settings > Interface >
Transparency). Ghostty's own `background-opacity`
and `background-blur` lines in the terminal config do not change Ghostex's panes.

On macOS, Terminal shaders runs the `custom-shader` files from
Ghostex's terminal config over its terminal panes, in the order the config lists them.
Turn it on in Settings > Extensions > Experimental (`terminalShadersEnabled`). While it is on, terminals use the background color and
opacity from Ghostex's terminal config, so Terminal background and its color are greyed
out. Shader animation follows Ghostty's `custom-shader-animation`. Turn the switch
off to go back to normal rendering; running sessions keep going. If a shader file
does not compile, terminals keep their normal look. Windows and Linux are not
supported yet.

The Monospace font (Settings > Interface > Text, `terminalFontFamily`, default
JetBrains Mono) sets one font for terminals, code in the Files view, diffs, code
blocks, inline code and tool output in chat. Its list shows the monospace fonts
installed on this computer; type any other name or a Ghostty list in its search
field. The App font and the Monospace font each take effect when you press Apply:
every window redraws at once, nothing restarts, and the preview under them shows
your pick first, with a sample of your message, thinking and a reply that also
follows Text strength and Show thinking in grey. Terminals on remote machines
follow it, because this computer draws them. The phone keeps its own fonts.

Related settings: `terminalFontFamily`, `terminalFontSize` (with `interfaceScalePercent`,
Settings > Interface > UI Scale), `terminalGhosttyTheme`, `terminalColorScheme`, `terminalGhosttyLightTheme`,
`terminalBackgroundMode`, `workspaceBackgroundColor`, `terminalBackgroundOpacity`,
`windowGlass`, `windowGlassBlurRadius`, `windowGlassWindowsBlur`, `terminalCursorStyle`, `terminalPane*PaddingPx`,
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
permission handling. In a remote computer's project, a custom agent starts as
the built-in agent it works like (or whose command it runs), with that
computer's own command; a custom command that is no built-in agent runs only on
this computer. Custom agents can be turned off, or deleted from their
expanded row; built-in agents are only turned off. Expand an agent row to edit
its name, command, permission mode and default view, duplicate it as a custom
agent, install or update its CLI, see its installed version (and the newer one
when available) and command output, or open its Install docs link. Claude Code, Codex, Cursor Agent
and Grok Build install through their official installers (PowerShell on
Windows; Claude Code doesn't need Git for Windows to install). Pi installs with
its official installer too, which needs Node.js 22.19 or newer and npm (Ghostex
installs its own Node.js first when npm is missing), and updates with
`pi update`; npm stays as another choice. OpenCode's official installer on Windows runs its install script with
Git for Windows' Bash, so it needs Git for Windows; without Git, install
OpenCode with npm instead. OpenCode's Homebrew choice installs
`anomalyco/tap/opencode-v2`, the OpenCode 2 formula its docs name. When an installer leaves its folder off PATH (Claude Code's does), Ghostex
adds it to your user PATH so new terminals find the command; Add to PATH
does the same for a CLI installed earlier. Installs run one at a time and wait
their turn. Ghostex updates a CLI the way it was installed; when it cannot tell,
it uses the CLI's own updater (such as `copilot update`, `pi update` or
`omp update`), and only a CLI without one asks you to choose the original
installation method. Antigravity updates itself in the background while it
runs, so its Update says so instead of running anything; start a new session
to use the newest version. Hermes Agent is checked with its own `hermes update --check`,
so Update available is shown when Hermes is behind even if its version number has
not changed. When mise's `minimum_release_age` setting holds a newer release back,
the CLI shows that release as held back by mise and the date it becomes eligible
(when an older release is already eligible, the update installs that one);
Ghostex leaves that setting alone. If a CLI needs a newer tool first (OMP's Bun
install needs Bun 1.3.14 or newer), the method says what to update; the official
installer downloads OMP's own binary and needs no Bun. Ghostex installs a CLI
with the method its makers document: their official install script where they
have one (OpenCode, Antigravity and Hermes Agent too), otherwise npm, Homebrew
or WinGet. It doesn't install agents with mise, even when your computer has
mise, and never downloads mise for an agent; a CLI mise already manages on your
computer keeps updating through mise. A CLI installed through mise starts in its sessions even when your shell
doesn't set mise up (no `mise activate` needed): Ghostex runs it through mise.
If a session's agent command still isn't found, its chat says which command is
missing and offers Install. When Chat is your default view, installing a CLI
here also installs the Ghostex hooks its chat needs. Existing mise tools, including custom
backends, update through mise with their version pins bumped to the latest
release; older versions remain available for running sessions.
Install and update commands run on the
selected computer and keep running if Settings closes. From a terminal,
`ghostex agent-cli status [agent]`, `ghostex agent-cli install <agent>`,
`ghostex agent-cli update <agent>` and `ghostex agent-cli add-to-path <agent>`
do the same. Start a new session to use the installed version. ZCode launches with `zcode`; install
and update it with `npm install -g zcode-app-cli@latest`, as documented at
[the ZCode installation docs](https://github.com/kingsword09/zcode-cli).
Freebuff launches with `freebuff` and installs with `npm install -g freebuff`;
sign in once in its terminal the first time it starts.
DeepSeek Harness (under More agents until you turn it on) launches with
`dsh-tui` and installs and updates with its documented
`npm install -g @deepseek-ai/dsh @deepseek-harness-tui/dsh-tui` (plus pnpm when it
is missing), then `dsh-tui update`, which sets up its profile, so the terminal app
and its profile stay on the same release. A copy mise already manages keeps
updating through mise; a copy installed another way updates with `dsh-tui update`. It needs a DeepSeek API key (`DEEPSEEK_API_KEY`)
or a DeepSeek account sign-in in its terminal.
Empryo launches with `empryo` (`em` works too) and installs with
`curl -fsSL https://empryo.com/install.sh | bash`, which puts it in
`~/.empryo/bin` (on Windows, `irm https://empryo.com/install.ps1 | iex` in
PowerShell). Ghostex installs its hooks into Empryo's `hooks.json` (in `~/.empryo`,
or `%LOCALAPPDATA%\Empryo` on Windows) and never touches Empryo's `config.json`,
so an Empryo session shows working and idle and comes back after a restart with
`empryo --session <id>`. On Windows without WSL, Empryo's chat, fork, picker and
coordinator work, but its sidebar status does not follow its hooks yet. Empryo has
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
next time Ghostex starts. Claude Code, Codex and OpenCode need no hooks:
Ghostex follows them from the session files and events they write themselves
(working, finished, questions and approvals, the conversation to resume), on
every computer, so it installs no hooks for them and removed the ones it had
installed (your own hooks stay). Their rows in Settings > Agents say "Not
needed". If Ghostex can't follow one of them in a session (for example after an
update of the agent changes its files), it turns that agent's hook back on by
itself and says so in the session's chat; the hook takes over the next time the
agent starts there. Claude Code older than 2.1.172 keeps its hook, and a Codex
older than 0.163 can't show its approvals without one, so its chat asks you to
update it (Update Codex). To use the hooks for these three again, turn on Use
agent hooks in the Session resume hooks card (`useAgentHooks`). Every other
agent still uses its hooks. To install hooks on another computer, use Settings >
Agents with that computer picked. From a terminal, `ghostex agent-hooks status`,
`ghostex agent-hooks install <agent>` (or `--all`) and `ghostex agent-hooks
uninstall <agent>` (or `--all`) do what the Session resume hooks card does; add
`--computer <name>` to run them on a connected computer.
Agent approvals ("accept all") is a
per-machine default with per-project overrides. Actions (Settings > Actions)
are saved terminal commands or browser URLs shown on project headers and in
the header’s Quick Actions button, which shows the name and icon of the Action
you used last and runs it again on click (its caret opens the full list; it
reads Start until an Action exists); Global Actions apply to every project.
The caret's list shows the project's own Actions first and then, under a
**Global** heading, every Global Action (the heading is left out when there are
none); a click on a Global Action runs it in the project you are in, a Terminal
one in that project's folder. Only the project's own Actions become the
button's one-click Action and the Start Action 1-5 hotkeys. To show a Global
Action on every project's row too, turn on its **Show on the project's sidebar
row** in Settings > Actions.
On the phone, a project's Actions menu lists that project's Actions and then
the Global Actions. A Global Terminal Action runs as a normal session in the
project whose menu you opened, in that project's folder, and you open its
output like any session; a Global Browser Action opens its address like a
project one. The phone lists Global Actions only from a computer whose Ghostex
offers them (update Ghostex on an older computer) and only while Actions is
on. From a terminal: `ghostex run-action <id> --project-id <id> --scope global`.
Actions is a built-in extension, off by default: turn it on with its switch in
Settings > Extensions > Built-in. While it is off the Start button, the Actions
page in Settings, Actions pinned to project rows, the Start Action hotkeys and
Quick Access's Action rows are gone, and `ghostex run-action`, `run-command`,
`save-command` and `click-button command` say Actions is turned off. Your saved
Actions are kept and come back when you turn it on (`actionsHidden`).

Agents Hub lets you browse and edit agent files in Skills, MDs, Hooks,
Configs & MCPs, and Agent Sync. Skills lists each skill once, however many
agents have it: expand it to see the agents that use it and every folder it is
in. When the copies in different agents' folders are not the same, the row says
how many versions there are and lists each version's files. Backup folders
(names with `.pre-sync-` or ending in `.bak`) are never listed. In MDs, expand Shared agent markdown to see the
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
instruction file (a file with other content is backed up first), and the hooks
folder and lock file linked into Claude Code and Codex (when `~/.agents` has no
`hooks` folder or lock file, the Hook scripts card says "Nothing shared" and
there is nothing to fix there). Nothing is deleted:
anything in the way is moved to
`~/.local/state/ghostex/agent-sync-backups/<stamp>/`, at the same path it had
under your home folder, so agents never mistake a backup for a skill. Tidying
leftover lock file entries is opt-in (the Optional cleanup switch, or the plan's
last group). Agents that read `~/.agents/skills` directly (Codex, Gemini CLI,
GitHub Copilot, Pi, Amp, Cursor, OpenCode) get no links, and links already in
their folders are removed so they do not load each skill twice; Claude Code and
the others get one link per skill. Old `<name>.pre-sync-<stamp>.bak` backup
folders left inside skills folders are moved to the same backup folder. The same scan, plan, and apply run from the CLI:
`ghostex agent-sync status`, `ghostex agent-sync plan [--agent <id>]`, and
`ghostex agent-sync apply --yes [--agent <id>] [--group <group>...]
[--prune-lock]`; `ghostex agent-sync agents` lists the ids.

Use `ghostex agents --help` to create, message, and close other agent sessions.
`ghostex agents whoami --json` identifies the caller; `agents types` lists
configured agent IDs; `agents create <agent-id> --task "<task>"` starts one in
the caller's project (`--project-id` selects another). An agent turned off in
Settings > Agents is not listed, and `agents create`, `create-agent`,
`run-agent`, `coordinator create`/`start-thread --agent` and `board start-work
--agent` refuse it, naming the agents to use instead, unless `--allow-hidden` is
passed. `agents list` finds
sessions, and `agents send <session-ref> "<text>"` attaches the sender's identity
and reply reference automatically; an agent that sends with
`send-session-chat-message`, `queue-session-chat-prompt` or `send-message`
instead gets the same identity attached, so its message never shows as yours. A normal send reaches a busy agent at its
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
In the chat, a message between agents is one line with that agent's logo and
the other session's title: Message from <session title> for one another agent
sent (thread reports included), Message to <session title> for one this
session's agent sent with `agents send`, `agents create --task` or
`coordinator start-thread`. Click the title to switch to that session; click the
rest of the line to read the whole message. A mention of a session in a message
(from Copy ID or the `+` list) shows as a pill with the agent's logo and the
session's current title. A pill whose session is gone is greyed out and opens nothing. One command that sends the same
message to several sessions is a single "Message to 2 sessions" line that lists
each recipient when opened. A session Ghostex cannot find any more reads
"Message from another agent".
An agent can also read or search any other thread, including a sleeping one:
`ghostex read-session-chat <session> --all --format text` prints the whole
conversation, and `--grep "<words>" --context 1` finds where a topic came up.
`<session>` can be any id from the sidebar's Copy Details (right-click a session,
Copy > Copy Details; its Global Ref is the most precise), or the title. The
session's menu also has Copy ID, right above Tag As, which copies a mention of
the session, `[<session title>](<Global Ref>)`, to paste into another chat (it
shows there as the session's pill); dragging the session's row from the sidebar
onto a chat box types the same mention where the cursor is (one per session
when several selected rows are dragged together). The Copy menu copies the
session's branch, its Linear issue ID and link, and its pull request or GitHub
issue link when it has them.
A session's Copy menu also has Copy Link, which copies an https link to that
session. Opening it from a browser, GitHub or a note brings Ghostex forward and
focuses the session, on this computer or a connected machine. Ghostex asks
first if that would start the session's agent, and offers to restore a closed
session from Previous Sessions. On a computer that does not have the session,
it says so and does nothing else.
Copy Details lists the session's agent, title, Global Ref, agent session id,
zmx name and project, and ends by telling the agent to run `ghostex agents`,
which explains how to message other agents and check their status: paste it
into another agent's chat and that agent can message or read that session. `ghostex sessions --json` prints the same short facts for
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
   to `create-agent` or `board start-work` (Claude, Codex, Pi, OMP, Copilot,
   Grok Build, Hermes Agent, Empryo and Antigravity CLI, or the model alone for Cursor CLI and Gemini; for Pi and Empryo the model is `<provider>/<model>`, as in
   Pi's `/model`, and Pi's effort is its thinking level, such as `high`). The choice
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

## Coordinators

A coordinator is one agent you talk to about a stream of work in a project. You
tell it what needs doing; it answers quick questions itself and hands every real
task to a thread, which is an ordinary agent session it starts and briefs, so it
stays free to talk to you while the threads work in parallel. It is Ghostex's
version of the Projects features in Cursor and Claude Code.

- **Start one**: hover a project in the sidebar, open the Select Agent menu (the
  arrow beside its agent button) and choose **New Coordinator…**, the first
  item of that menu; hover the (i) beside it for why it is worth it. On the
  phone, open the same menu from the project's agent button and pick **New
  Coordinator…** at its top (tap its (i) for the same). When the sidebar shows a
  project that is on more than one computer (or cloned twice on one) as one
  entry, the desktop dialog has a **Computer** field that picks where it runs:
  it starts on the copy you picked in the New Thread picker, else the one you
  last used, greys a computer that isn't connected, and lists that computer's
  own agents and models. A project with one copy, or one Separate by machine
  keeps apart, has no such field. On the phone, a project whose clones its
  list shows as one gets a **Folder** row that picks between them. Name it, pick its agent
  from the Agent dropdown (Claude, Codex, ZCode, Cursor, Grok, Copilot,
  Antigravity, Pi, OMP, OpenCode or DeepSeek Harness, whichever you have set up), its model where the agent
  lets Ghostex choose one (Opus 5.5 on Claude; otherwise each agent starts on its
  own default) and its effort: Low, Medium, High or XHigh, High by default (an agent without
  XHigh offers its levels from Low up to its highest). Cursor and ZCode coordinators take no
  effort choice (Cursor's effort is part of its model). An OpenCode coordinator lists every model
  your OpenCode has (search the list), starting on OpenCode's default, and its effort is that
  model's variant; Ghostex switches OpenCode to them before its first message. A DeepSeek Harness
  coordinator takes no model or effort choice: it starts on the model your DeepSeek Harness is set
  to. Use a very good
  model: the coordinator
  is the agent you talk to all day, and the main way Ghostex itself is built.
  Its first message is always "Read and adhere to this skill:
  /ghostex-coordinator" (`$ghostex-coordinator` on agents other than Claude
  Code and Copilot), shown locked in the dialog: Ghostex installs the
  ghostex-coordinator skill for that agent, and the skill is the coordinator's
  playbook. A Claude or Codex coordinator keeps its role after `/clear` or a
  compaction: Ghostex hands it that line again before its next message. Then it opens in chat, where you write your first request. The name
  is what its sidebar row shows, and the coordinator keeps it: unlike other
  sessions it is not renamed from your first message (Rename in the sidebar
  still changes it; left blank, it is named once from its first conversation).
  A project can have several coordinators, one per stream of work. In an
  coordinator's chat the effort picker offers the same Low, Medium, High and XHigh only.
  To pick from every level the agent has (Max, Ultra and the like), turn on
  **Every effort level in coordinator pickers** (an advanced setting in Settings > Chat);
  a coordinator works best on High or XHigh. From a terminal: `ghostex coordinator
  create [--agent <id>] [--goal <text>] [--task <first request>]`.
- **Turn a session into a coordinator**: right-click a session of one of
  those agents in the sidebar, open **Advanced** and choose **Make
  Coordinator** (optionally with a goal); on the phone it is in the session's
  menu too. The session keeps its conversation and is never restarted or
  interrupted: it gets the coordinator crown right away, Ghostex installs the
  ghostex-coordinator skill for its agent, and a message telling it to read
  the skill waits in its chat queue until its current turn is over. Sessions
  it started before are not its threads yet; ask it to adopt them. A thread of
  another coordinator, a session in a box, or a draft cannot be made an
  coordinator.
- **Threads in the Project sidebar**: a coordinator's row shows the coordinator crown (a three-point outline) in
  place of its agent's logo (Chief of Staff and department leads keep their filled crown), and a crew icon
  with one number: how many sessions under it (its threads' threads too) are
  stuck waiting for your approval; when none, how many of its threads are
  waiting on you; when none are waiting, how many are working; when neither,
  how many threads it lists. The icon and number are red when the number counts
  sessions waiting for approval, light blue when it counts threads waiting on
  you, orange when it counts working threads, and grey otherwise. Its
  threads sit indented right under it with their own status dots: the ones
  working, waiting, or active in the last 2 hours. Older threads wait behind an
  "N older threads" row at the end, which lists them all (and "Hide older
  threads" tucks them away again; both remembered across restarts). The chevron
  that replaces the crown when you hover the coordinator folds its threads away
  and back, and a folded coordinator keeps its number and colour. While its
  threads are folded, the coordinator's row shows their status dots (with its
  own) overlapping in place of its time, like a computer tile's dots, so a thread
  that is working, finished or asking something is never hidden (a thread
  waiting for your approval is the red dot, one waiting on you for anything
  else is the pink dot). Opening an
  coordinator marks its finished threads as seen, so their blue dots clear
  (a thread waiting on a question or approval keeps its dot); otherwise opening
  its chat leaves the fold and the older threads as you set them; opening one of its threads
  unfolds the coordinator so the thread shows. Click a thread to watch it or
  talk to it directly; answer its questions and approvals there. A thread is
  named after its latest task and keeps that name, like the coordinator keeps
  its own (the agent's automatic naming never replaces it); when its
  coordinator starts it on a new task the name does not describe, the thread
  renames itself to match, so the names stay current. Your own Rename in the
  sidebar always wins: a thread you renamed keeps your name. Pinning the
  coordinator takes its threads along. Right-click the coordinator and choose
  **Sleep Inactive Threads** to put its idle threads to sleep in one go: threads
  that are working, waiting on a question or approval, or running a background
  shell stay awake, and so does the coordinator (the item is greyed out when no
  thread can be slept). On the phone, long-press the coordinator for the same
  item; `ghostex coordinator sleep-inactive-threads` does it from a terminal.
- **Main coordinator**: each project has one main coordinator on each
  computer, and its coordinator icon is gold (in the Project and Unified
  sidebars, the chat's `+` picker and the phone's sessions list). It is the one other
  agents and Chief of Staff contact when they want something from that
  project. The first coordinator you create in a project gets it; to give it to
  another coordinator there, right-click that coordinator and choose **Make
  main coordinator** (long-press on the phone). Closing or deleting the main
  one passes the tag to the oldest coordinator left in the project. From a
  terminal: `ghostex coordinator make-main <session-ref>`; `ghostex coordinator
  list` marks the main one, and an agent reaches it with `ghostex agents send
  --main-coordinator --project-id <id> "<message>"` (add `--computer <name>` for
  another computer). When the project has no coordinator, the agent picks
  `--if-none create` (start the project's coordinator) or `--if-none session`
  (start a session there that does the work and reports back).
- **Closing a coordinator** closes its threads that are open in the sidebar
  too (working, waiting, finished or asleep; done threads are left alone), and
  Ghostex asks first: "Close Main Coordinator and its 3 open threads?". Every
  thread keeps its conversation. Restoring the coordinator from Previous
  Sessions brings those threads back under it, each on its own conversation.
  `ghostex kill` on a coordinator closes its open threads the same way.
  The phone's session list shows the same
  tree: the crown, crew count, threads indented under their coordinator, and the
  chevron to fold them.
- **Threads in the Unified sidebar** (from 11.3.0): a
  coordinator's threads sit under its card, folded at first, on the computer and
  on the phone. The card starts with the coordinator crown, which turns into a
  chevron when you point at it: click it to show or hide the threads (on the
  phone, tap it). The card's crew count (the crew icon and a number, coloured as
  in the Project sidebar and grey while no thread works or waits) only informs.
  A coordinator among the threads folds on its own, and each card remembers how
  you left it. Unfolded, a card lists the threads that are working, waiting on
  you or active in the last 2 hours with the same rows and tree lines as the
  Project sidebar (from 11.5.0; with the computer, branch or pull request when they differ
  from the card's), and the rest wait behind an "N older threads" row. A folded card shows its threads'
  status dots overlapping in place of its own, and a card whose thread needs you
  (an approval, a question, stuck or failed) leaves the Moving shelf for the top
  of the list. A parked or snoozed thread sits on its own shelf instead, and a
  thread whose coordinator is outside the active Space or in a hidden project
  shows as its own card (a search then finds it there, not under the
  coordinator). A
  search that matches a thread shows the way to it under its coordinator's card
  even while folded. A computer, project or Space pick keeps the card when a
  thread matches; it shows the way to that thread only when the coordinator
  itself is left out by the pick, and otherwise the card stays as you folded it.
  Shelves and project groups you folded stay folded. Opening a thread from anywhere
  unfolds its coordinator. Threads count as sessions on the computer tiles and
  in Quick Access, not in the list's shelf and project counts. On the phone, a
  thread on another computer nests under its coordinator when both the
  coordinator's computer and the thread's computer run 11.3.0 or later;
  otherwise the thread shows as its own card.
- **Finished threads are closed**: once the coordinator has committed a
  thread's work and expects nothing more from it, it marks the
  thread done and closes its session, so the sidebar keeps only work in flight
  (it never closes a thread that is still working or waiting on you). A closed
  thread is not lost: when a follow-up comes, the coordinator reopens it or
  messages it, and the same conversation resumes under the coordinator with
  everything it knew. A message you send it in its chat reopens it the same
  way (typing in its terminal does not). Ask the coordinator to keep a thread open if you want to
  look at it. You can do it yourself too: right-click a thread and choose
  **Mark Thread Done** (greyed out while it is working or waiting on you) to
  mark it done and close its session, or **Reopen Thread** on a done thread to
  resume it under its coordinator. On the phone, long-press the thread.
- **Workers pill in the chat**: on the working card just above the
  coordinator's chat box, a small "Workers" pill counts the threads working
  or waiting on you. Click or tap it to open the workers menu (a sheet on the phone):
  the working threads first, then "Show recent" for the ones
  active in the last 2 hours, each with one line (what it is doing or how its
  last report began) and its branch; "N more" lists every other thread, closed
  ones included, and "Show fewer" folds them again. Tap or click any thread to
  open it and talk to it. A thread stuck on something only you can allow (a
  permission prompt or a folder-trust question) carries an amber "Needs your
  approval" tag, and the pill shows an amber dot. Click the pill again, or outside
  the menu, to close it. It shows on the phone and in the browser too.
- **Answer a thread's approvals from the coordinator's chat**: when a thread
  stops on a Connectors card ("Allow this agent to run … through Composio?"),
  a Claude Code or Codex permission prompt, or a folder-trust question, the
  same card shows in the coordinator's chat, so you don't have to open the
  thread. It sits under the thread's "waiting for an answer" report, in a red
  **Approvals** pill at the left of the pills above the chat box (its menu
  lists every waiting card), and as buttons on the thread's card on the board.
  Each shows the thread's name, what it asks, the command or arguments (four
  lines, "Show N more lines" for the rest) and the thread's own buttons, such
  as Approve / Always allow / Deny or Yes / Yes, and don't ask again / No.
  Hover a lasting choice (Always allow, "don't ask again", Trust and Remember)
  to see what it changes: Always allow on a Connectors card allows that tool
  in the thread's workspace (Settings > Connectors). Your click answers the
  thread's real card with exactly that choice, and the thread's chat shows
  "Approved from <coordinator>" (or "Denied from …") with the time. The
  coordinator itself never answers these cards: only your click does. After an
  answer the card folds to one line that says how it ended: answered here,
  answered in the thread (when you answered it there instead), expired (a
  Connectors card waits 30 minutes) or moved on (the thread stopped asking).
  A click on a card that changed in the meantime is refused and the card shows
  the current one. Threads on another computer are answered in that thread.
- **Work items**: a coordinator keeps each line of work as one work item (W7),
  named the way you would name it, with one line saying where it stands, who has
  the ball (you, the coordinator or a thread) and one of six statuses: **Needs
  you**, **Blocked** (stopped until you act), **In progress**, **Waiting** (on
  another item, a question or something outside), **Later** and **Finished**.
  Threads, steps and what it asks you belong to the item, and a wait on another
  item or question clears itself once that's done. A coordinator's reply is a
  couple of sentences of news, or marked sections with bullets while Marked
  handovers in every session is on. On the computer, each work item the turn
  changed gets a small card under the reply, like a "Message from" card: "Work
  item updated" (or added, finished, dropped, needs you, asked you, now
  waiting), the item's title, the line that changed, the time and the item's
  state; a turn that changed more than three is one "N work items changed" card
  listing them. Click a card to open the Work panel at that item. On the phone
  the same news is one quiet line under the reply ("Pairing page started · 1
  ask"). For the latest turn, the **Worked for** line says it too ("Worked for
  18s · Pairing page started · 1 ask").
- **Above the chat box**: what needs you sits right on top of the chat box as
  one compact list, one row each: the question or task, its answer buttons (the
  recommended one has a small star) or **Done**, and how long ago it was asked;
  below them, blocked work and what the coordinator's latest turn finished (its
  × takes a finished row away). At most four rows show (three on the phone),
  then **N more need you**: on the computer it opens the rest right there, under
  the first rows, and **Show fewer** folds them back (a long list scrolls inside
  the dock); on the phone it opens the Work panel at Needs you. Something
  waiting more than a day moves below newer ones. Click a row to open it in
  place: its full text, its links (thread, PR or issue, file), and **Reply** (your
  own words, in the same reply bubble as a message's Reply; they reach the
  coordinator as "W7 Q12: your words" and settle the question), **Open** for a
  form or page, **Mention** (puts the item in the chat box to talk about it),
  **Show in chat** (scrolls to the reply of the turn that asked it), **Dismiss**
  (takes it away for good and tells the agent) and **Snooze** (1 hour, 6 hours
  or 24 hours: it leaves the list and the counts and comes back by itself, also
  after a restart). Click the row again to close it. On the computer, Mention,
  Show in chat, Dismiss and Snooze are in the row's ⋯ menu, which takes the
  place of the row's time while the pointer is over the row, open or closed. A
  row's answers and Reply always stay on one line: when they don't fit, the
  answers fold into one **Choose an answer** dropdown beside Reply (the same on
  the Work panel's cards). Words you type in a Reply and don't send stay with
  that row, also when you click away, press Escape, open another row's Reply or
  quit and reopen Ghostex: Reply on it again shows them, and its Reply says
  **Draft** meanwhile. Sending them, or the question being answered or
  dismissed, clears them. On the computer the list
  has a header: the counts as marks and numbers (need you, blocked, in
  progress; point at one to see what it counts), a button that opens the Work
  panel, and a chevron. While the agent works, its status ("Running 1 shell
  command…", then "Worked for 28s" and what changed, for a minute after the turn)
  sits at the header's left, on the same bar whether the list is open or not; the
  counts drop their names first when the bar is short. Click anywhere on the header to collapse the list to
  that one bar, and again to open it; it remembers this per chat and opens by
  itself when something new needs you. While nothing needs you but work is in
  progress, the header stays as one quiet line. On the phone the × at the
  list's corner hides it until something new needs you or you tap the Work
  pill. In any
  agent's chat, a question from a reply that you haven't answered shows here
  too: a one-click question as a row with its answers on it, a form or page as
  "N questions open" with Open. The ? pill counts only questions too old to be
  shown here. On the computer, the list steps aside while the Work panel is open
  beside the chat and while you type a long message.
- **Nothing you click moves the chat**: the ? pill, a Q12 tag in a message, a
  row of the Resources list and Open on a waiting form open the form in a popup
  over the chat (a page opens as the page), and a coordinator's question or work
  item opens in the Work panel. **Show in chat** is the one exception, and only
  when you press it.
- **The Work panel**: on the computer, the button in the header above the chat
  box opens it (the header carries the counts); on the phone the **Work** pill
  does, reading "Work · 2 need you · 4 in progress" while the row has room. The
  panel lists
  every line of work by who has the ball: **Needs you** first (each naming the
  work it belongs to, with its answers or Done), then **Blocked**, **In
  progress** (only work a thread or the coordinator is on right now), **Not
  being worked on** (still open, but nobody is on it; it stays there until the
  coordinator finishes it, parks it, asks you about it or works on it again),
  **Waiting**, **Later** and **Finished today**, the last four folded; on the
  phone, while anything is blocked, a **Blocked** pill beside the Work pill shows
  how many.
  A thread the coordinator hasn't put on an item keeps a row of its own. Click a
  card to open it in place: one that needs you offers **Reply**, **Show in
  chat**, **Dismiss** and **Snooze** beside **Mention**, as a row above the chat
  box does. Right-click a card, or click its ⋯ (long-press on the phone), for
  **Mention in chat**, **Tags**, **Mark done** and **Drop** (**Restore** brings
  it back for 7 days). Drag a card onto another group to move it. The panel
  opens beside the chat (a floating card when the chat is narrow) or as columns
  over the chat (Settings > Chat > **Coordinator board opens as**); in columns,
  click a column's header to fold it to a strip, and the row scrolls sideways
  when the open columns don't fit. The phone shows the panel as a sheet. From a
  terminal: `ghostex coordinator item list` prints the items, and `item add`,
  `item now`, `item step`, `item ask` (with `--answers "A,B"` or `--answer`
  repeated, so an answer may hold a comma), `item wait`, `item done` and `item
  drop` change them; the older `ghostex coordinator board` commands still work.
- **Reports come back by themselves**: when a thread finishes a turn, Ghostex
  sends its final message to the coordinator (a "Message from" card in its
  chat); when a thread waits on a question, an approval, or a screen such as
  folder trust or an expired login, the coordinator is told what it is asking.
  The coordinator then reads the report, commits the work (only that thread's
  files, and it never pushes unless you ask), starts the next step, and tells
  you what needs you. Nobody has to poll. A thread reports only this way: Ghostex
  refuses a thread's messages to its own coordinator in the middle of a turn
  and tells it to end its turn with its report.
- **Threads on another computer**: a coordinator can start a thread on a
  computer this one is connected to (Settings > Remote > Computers this machine connects to): ask
  it to, for example "run this on Omarchy". The thread uses that computer's
  project with the same name (or the one you name), its agents and its
  folders, sits under the coordinator in the sidebar with the computer's name
  after its title in reports, and reports back like any thread. A thread there
  that shows no new output for 8 minutes is nudged and reported to its
  coordinator like a thread on this computer, and one whose turn ended without
  that computer saying so is reported finished from its transcript. While the
  computer is not connected, its reports wait; you can still mark it done. From a terminal: `ghostex
  coordinator start-thread --computer <name> --title "<title>" --task "<task>"
  [--project-id <id>]`.
- **Work that never arrives is caught**: when the coordinator starts a thread,
  it waits until the thread has actually taken its brief before saying it is
  under way, and says "pending" with the reason when it has not yet. Ghostex
  keeps watching every brief and follow-up the coordinator sends: if a thread
  sits idle without it, the coordinator is told the message did not reach it
  and sends it again, so work handed out is never silently dropped.
- **Thread models**: the coordinator picks each Claude thread's model when it
  starts it: Opus 5.5 at high effort for substantial work, Opus 5.5 at medium
  for hard but small changes, Sonnet 5.5 at high for small contained fixes. A
  coordinator on another agent starts its threads on its own agent and model,
  at high effort for substantial work and medium otherwise (Cursor and Gemini
  threads take the model but keep their own effort; OpenCode, ZCode and DeepSeek
  Harness threads start on the agent's configured model).
  It never switches a running thread's model (that throws away its prompt cache);
  a follow-up that needs a stronger model gets a new thread.
- **Worktrees**: threads that need the same files run one after another; when
  running them in parallel matters, the coordinator gives a thread its own git
  worktree and branch, so parallel threads never edit the same checkout. A coordinator's
  threads trust the project's own folder and the worktrees Ghostex makes for
  them, so they start without stopping at the agent's folder-trust question. A
  thread started on a connected computer with its own worktree is trusted there
  the same way.
- **Goal, standing instructions and memory**: every thread's brief carries the
  coordinator's goal and its standing instructions (rules such as which branch
  to target or how to verify work). Memory notes are the coordinator's own:
  when you state a lasting preference, it proposes the exact wording and saves
  it once you confirm, and when it briefs a thread it passes on only the notes
  that matter for that task, worded for the thread. Rules every thread in a
  project must follow belong in the project's AGENTS.md. Ask the coordinator
  to change the goal or the instructions the same way. To edit them yourself,
  right-click the coordinator and choose **Goal & Instructions…** (the one-line
  goal and the standing instructions) or **Memory…** (every note, with a button
  to remove one and a field to add one); on the phone, long-press it. Save
  applies to threads it starts from then on; Cancel leaves everything as it was.
- **Playbook overrides**: every coordinator follows Ghostex's playbook (how it
  talks to you, starts and briefs threads, and reports back), which Ghostex
  keeps up to date with each release. To change how your coordinators work,
  write **playbook overrides**: your own instructions for every coordinator,
  one per line, such as "Never push without asking" or "Ignore the board
  rules". Coordinators read them after Ghostex's playbook, and they take
  precedence over it: the playbook is written for everyone, the overrides are
  yours. Ghostex's playbook itself is never edited, so its updates keep
  arriving. Edit them in **Settings > Chat > Coordinator playbook** (Edit
  overrides…), from the **Playbook overrides** row of the New Coordinator
  dialog (Edit), or by right-clicking a coordinator and choosing **Playbook
  overrides…**. To read the playbook a coordinator follows, right-click it in
  the sidebar (long-press on the phone) and choose **View playbook**, or choose
  **View playbook** in its chat's More actions menu (…), under Agent; it is also in
  **Settings > Chat > Coordinator playbook**, in the New Coordinator dialog and
  in the overrides editor. It shows Ghostex's playbook with the reply format
  you chose, read-only, then your overrides in their own box marked as yours,
  with **Edit overrides** to change them (on the phone it is read-only; edit
  the overrides on the computer). It is the same text `ghostex coordinator
  guide` prints. The
  overrides are one text for every coordinator on that computer; there is no
  per-project playbook. Every coordinator, existing ones too, follows a change
  from its next message, with nothing to do on your side: when a Ghostex update
  or your overrides change its playbook, the next message it gets (yours, a
  thread's report, a queued one) carries one line under it telling it to run
  `ghostex coordinator guide` and follow it from then on. That line is for the
  agent only: the chat shows your message as you typed it. That line comes once
  per change and not at all once it has read the new guide, and an idle
  coordinator is not woken for it. `ghostex coordinator status` says it too.
  If you edited a full copy of the playbook in Ghostex 11.0 or earlier, it
  stays in use (without Ghostex's updates) until you move off it: the overrides
  editor says so, and **Use Ghostex's Playbook** (click it twice to confirm)
  saves your old copy as a Markdown file and moves every coordinator to
  Ghostex's playbook; copy what you still want from it into the overrides.
  Each coordinator's own goal and standing instructions (Goal &
  Instructions…) stay separate. From a terminal: `ghostex coordinator playbook
  --overrides` prints the overrides, `ghostex coordinator playbook
  --set-overrides "<text>"` (or `--body-file <path>`) saves them and
  `--clear-overrides` removes them; one rule at a time, `--list-overrides`
  numbers them, `--add-override "<rule>"` adds one, and `--replace-override
  <n> "<rule>"` and `--remove-override <n>` change or remove one. A coordinator
  can save one too: give it a lasting rule for every coordinator and it proposes
  the wording, then saves it once you confirm. `ghostex coordinator set-playbook
  --use-ghostex` moves off an old copy.
- **How to use coordinators**: every coordinator's chat has a **How to use** pill
  (a light bulb) in the pills above its chat box, on new coordinators, older ones
  and sessions made coordinators alike. Click or tap it for the tips, each in its
  own card with a line you can type; clicking that line puts it in the chat box
  (nothing is sent). The tips: keep one coordinator per project and give it
  everything; ask for a gx page per topic and pick (gx visuals for numbers); ask
  it to dig deeper on a point before you pick; paste a screenshot instead of
  describing; let it start threads, on any agent or computer; answer with one
  click (a gx form or the answer links), or later from the ? pill, or by tag
  ("Q12: yes"); run several lines of work at once on the Board; say a rule once
  and it becomes a note it passes on to the threads that need it; read the end of each reply, where
  Needs you comes last and threads' approvals show; and on a Mac, let threads
  test in real windows with SpaceO and cua-driver. Tick **Don't show again** in
  the tips to remove the pill from every coordinator on that computer's Ghostex
  (the phone and every window follow); this Help keeps the tips. Only you see
  them: the coordinator's own first message is unchanged.
- **Just ask**: "run these three as separate threads", "use worktrees", "give me
  a status of every thread", "use a cheaper model for threads", "don't merge
  anything without asking" all work in plain words.
- **A session that is not a thread**: ask the coordinator for "a separate session I
  can chat to directly" and it creates one that sits in the sidebar on its own and
  reports to nobody; "unlink that thread" turns an existing thread into one.
- **Telling a thread apart at launch**: a thread's agent starts with
  `GHOSTEX_COORDINATOR_ROLE=thread` and `GHOSTEX_COORDINATOR_SESSION=<its coordinator's
  ref>` (such as `S1:P1abc:G1abc`) in its environment, and a coordinator's own agent with
  `GHOSTEX_COORDINATOR_ROLE=coordinator`; a session you start yourself has neither. A launch
  wrapper (an agent command in Settings > Agents) can read them, for example to start
  threads with fewer MCP servers. They are set each time the session's terminal starts: a
  restarted thread keeps them, an unlinked one loses them at its next start. A thread on
  another computer (`start-thread --computer`) does not get them yet.

Coordinators run on Claude, Codex, ZCode, Empryo, Cursor, Grok, Copilot, Antigravity, Pi,
OMP, OpenCode or DeepSeek Harness; threads can be any configured
agent. An Empryo coordinator runs as the Empryo agent `ghostex-coordinator`: Ghostex
keeps its playbook in `~/.empryo/agents/ghostex-coordinator.md` and switches the
session to it with `/agent ghostex-coordinator` before anything else, so leave that
file in place. From a terminal or another agent: `ghostex coordinator create --title <name>
[--path <folder> | --project-id <id>] [--model <m>] [--effort <e>] [--goal <text>] [--task <first request>]`, `ghostex coordinator
promote <session> [--goal <text>] [--computer <name>]` (make an existing session a coordinator; in versions after 10.49.0 this works on a connected computer's session too, and so does the sidebar's Advanced > Make Coordinator on its rows: that computer installs the coordinator skill and keeps the coordinator), `ghostex coordinator status`,
`ghostex coordinator options` (the agents, models and efforts a coordinator can use),
`ghostex coordinator start-thread --title <title> --task <brief> [--worktree]
[--agent <id>] [--model <m>] [--effort <e>] [--account <number|email>]` (`--path <folder>`, here and on `coordinator create` and `ghostex agents create`, adds that folder as a project, creating it if needed, and starts there; `--account`, in versions after 10.45.0, starts a Claude or Codex thread on that saved account instead of the Account for new sessions rule; one that needs signing in is refused), `ghostex coordinator resolve
<thread> [--keep-open]` (mark done and close its session), `ghostex coordinator
reopen <thread>` (resume a closed thread), `ghostex coordinator unlink <thread>` (take a
thread out of the coordinator's tree: it stays running as an ordinary session of its
project, shown as its own sidebar row with no more reports to the coordinator; 10.24.0 or
later), `ghostex agents create <agent> --standalone` (run by a coordinator, create a session
for the user to chat to directly instead of one of its threads; 10.24.0 or later), `ghostex coordinator remember <note>`, `ghostex
coordinator set-goal|set-instructions`, `ghostex coordinator edit [--goal=<text>]
[--instructions=<text>] [--notes-json=<json list>]` (the goal, instructions and whole
note list in one call, as the editors save), and `ghostex coordinator guide` (the
coordinator's own playbook, ending with your playbook overrides). `ghostex coordinator --help` lists every flag. `ghostex orchestrator …`
is an alias that runs the same commands.

## Main Hub (beta extension, off by default)

Main Hub is a beta extension, off until you turn it on in **Settings >
Extensions > Main Hub**. Only you can switch it, not an agent. While it is off,
your home folder's project (if you have one) is an ordinary project and nothing
here applies.

Turned on, the project at your home folder (`~`, `C:\Users\<name>` on
Windows) becomes **Main Hub**, with the Ghostex flower on its header: the home
for chats that don't belong to a project. If you already had a project there, it
becomes Main Hub as it is, with its sessions, history and settings. Rest the
pointer on its header and a card beside the sidebar says what it is.

**Chief of Staff**, pinned first under Main Hub, is one agent per computer you
can ask about anything: any session, agent, project or computer, and Ghostex
itself ("what's stuck?", "how do I pair my phone?").

- **Main Hub's header**: click it to fold or unfold, like any project. Right-click
  for Main Hub's settings (Show on All Spaces, Ask Before New Chat, the folders
  agents can't read) above the project's own menu (Move to Space, New Terminal
  and the rest). It sits at the top of the Space it is in, or of every Space with
  **Show Main Hub on all Spaces** (Settings > Agents > Home folder).
- **First open**: before Chief of Staff runs, its row says "Pick an agent"; click
  it and pick the agent it runs on.
- **What Chief of Staff does**: it reads every session's chat and screen,
  messages, interrupts and types into any session, and answers other agents'
  questions and permission prompts, without asking you each time. Work in a
  project goes to that project's coordinator, and work on another computer to
  that computer's Chief of Staff. Collab, pairing, Easy Connect, connecting
  computers, Autopilot's limits, turning GBots on, security settings and
  installing things stay yours.
- **New Chat keeps your old chat**: right-click the Chief of Staff row for New
  Chat (Ghostex asks first; tick Don't ask again to stop asking). The old chat
  stays in Main Hub, asleep and named after the day it started. Change Agent
  moves it to another agent the same way. On the phone, a long press on the row
  offers New Chat and Change Agent.
- **Your secret folders stay out of reach**: agents Ghostex starts in your home
  folder can't read the folders listed in **Folders agents can't read in your
  home folder** (Settings > Agents > Home folder): `.ssh`, `.aws`, `.gnupg`,
  `.config/gh`, `.netrc`, keychains, password stores, browser profiles, agent
  logins and the like. Claude Code's file tools are held to it, but its shell
  commands are not; Codex is held to it for everything it runs; other agents are
  not. Chief of Staff on Codex runs without asking, because Codex's sandbox would
  otherwise stop it reaching Ghostex; Codex still keeps it out of these folders.
- **Several computers**: with computers combined, every computer's home is one
  Main Hub, each computer's Chief of Staff pinned at the top and other
  computers' chats marked with their computer's name.
- **Turning it off again** hides Main Hub and Chief of Staff, puts Chief of
  Staff's agent to sleep and takes away its control over your other agents; its
  chat and memory are kept for when you turn it back on.

`ghostex chief status`, `ghostex chief guide`, `ghostex chief new-chat`,
`ghostex chief set-agent <agent-id>`, `ghostex chief send [--computer <name>]
"<text>"` (they say Main Hub is turned off while it is). Settings:
`mainHubHidden`, `chiefShowOnAllSpaces`, `chiefConfirmNewChat`,
`homeSecretFolders`.

## Autopilot (keep sessions going while you are away)

Autopilot keeps your agent sessions moving while you are away, until you turn it
off. Turn it on in **Settings > Autopilot** (on the phone too: Settings ›
Autopilot, per computer), from the **Autopilot** pill in any coordinator's chat
(above its chat box, beside Board), or with `ghostex autopilot on`. While it is
on, Ghostex looks at every session it covers as soon as an agent finishes a
turn, and sweeps all of them every 2 minutes (1 to 10 minutes on the page). It
then:

- **Carries on** a session that stopped while its background job finished, that
  stopped mid-task right after saying what it does next, or whose turn was cut
  off (it was put to sleep or restarted mid-turn).
- **Interrupts and carries on** a session stuck on a hung command (no output for
  8 minutes; a coordinator's thread after its coordinator's nudge), telling the
  agent not to run it the same way again.
- **Reloads** a session that still shows nothing a few minutes later, on its own
  conversation, and leaves it for you if that does not help either.
- **Moves a session at a usage limit** to the ready account with the most room
  when its own account policy would wait, and sends "." so it carries on.
- **Restarts a session whose terminal ended** when Ghostex's own restarts gave
  up, and **skips Codex's update prompt** when it holds a session.
- **Answers questions for you** when **Answer questions for me** is on (off by
  default): after you leave a question unanswered for 5, 10 or 15 minutes, it
  picks each question's recommended choice, for an agent's own question and for
  a question form in its chat alike.
- **Answers approval cards** when **Answer approval cards** is on (off by
  default; while it is off only your click answers them): permission prompts,
  Connectors cards and folder trust, within its tier. **Heal** answers none,
  **Heal and decide** approves only what is on its allow list (reads, builds,
  tests, edits and local commits inside the project), and **Everything** (the
  default) approves anything that crosses no hard limit.

**Hard limits it may cross** are each a switch, off until you turn it on: push
a branch, finish a release you asked for, push main, merge pull requests,
delete outside the project, post publicly. A prompt that would cross one you
left off, or that asks for a sign-in, 2FA, an identity check or a payment
(never done), is declined: the agent is told to carry on without it, and the
log notes it for you.

**The supervisor** is an agent that wakes only for judgment calls: a question
with no recommended choice, or a session the rules could not keep moving. It
starts the first time it is needed, in an **Autopilot** project in your
sidebar, on the **Supervisor model** you pick (Claude Opus 5.5 at medium or
Codex GPT 6.1 Sol at high; the other one when that agent has no account with
room), and you can read its chat to see why it did each thing. It keeps notes
(one running file, and one per session) that every wake reads first, so it
never starts from scratch, and it acts only through Ghostex: it answers a
question, tells a session how to carry on, or leaves it for you with a note. It
never approves a prompt itself.

Which sessions: **Every session** (coordinators and threads included), only
**Projects I pick** (their switches show on the page, worktrees included), or
only **Sessions I pick**. Each session's chat shows an **Autopilot** pill while
Autopilot is on (on the phone too): tap it to leave that session out (it reads
**Autopilot off**) or put it back. Right-click a project in the sidebar and use
**Advanced > Autopilot** to take the project and its worktrees out, or put them
back. A session you stop yourself is left alone for 30 minutes. Each kind of
action has an hourly limit per session, so nothing loops.

**Offline**: Ghostex checks that this computer can reach the internet and each
agent's API (Claude's, Codex's, or the proxy your environment sets), not with a
ping. While it can't, Autopilot sends nothing and wakes no supervisor, since
nothing could reach the agent: its pill reads **Offline: waiting for the
network**, and the log gets one line when the network goes and one when it is
back. It carries on within a few seconds of the network returning. Coordinators
work the same way: Ghostex holds its nudges and reports to them while offline
(`ghostex coordinator status` says so), and each coordinator whose threads were
working gets one line when it is back. Messages you or a coordinator send on
purpose still go out.

At the bottom of the page, **Summary** counts what it did since it went on and
**Log** lists every decision, newest first: the session, what it saw, what it
did, and when (kept 30 days).

CLI: `ghostex autopilot` (status and summary), `ghostex autopilot on|off`,
`ghostex autopilot log [--last <n>]`, `ghostex autopilot project <project-id>
on|off`, `ghostex autopilot session <project-id> <session-id> on|off`, and
`ghostex autopilot decide <project-id> <session-id> --answer <pick> | --message
<text> | --leave <note>` (what the supervisor runs).

## Beads view (was Kanban)

The Beads view (the view panel tab that used to be called Kanban) shows the
project's Beads issue tracker: every row or card is a bead in the project's
`.beads` workspace, read and changed through the `bd` CLI installed on the
computer that runs the project (Ghostex does not bundle it; the view's
**Install Beads** button installs it). A project without a Beads workspace
shows a card with a fix prompt to hand an agent, which asks you before it
creates any Beads files. The header has a **Tree | Board** switch, search,
the sync status with **Pull** and **Push**, Refresh, and **New bead**.

- **Tree** (the default): every bead sits under its epic, with the epic's
  progress ("2 of 5 done · 1 in progress · 1 blocked · 1 ready") and its done
  beads folded into one "N done" row; beads not in an epic come last. Beads
  waiting on a person (bd's `human` flag, `bd human`) are pinned on top under
  **Needs a decision**, with an **Answer** button. Each row shows the status,
  type, what it is waiting on, its defer or due date, labels, comments, the
  agent on it (a green dot while it works), the priority and when it last
  changed; hovering shows the bead's id, **Claim** and a ⋯ menu.
- **Filters**: All, Needs a decision, Ready, In progress, Blocked and Deferred,
  with counts from bd's own answers (`bd ready`); a filter keeps the tree's
  shape. **Show done** lists done beads too; the sort is Priority or Last changed.
- **Details**: click a bead to open its details beside the tree (✕ or Escape
  closes them): Close or Reopen, **Defer** (Tomorrow, Next week, Pick a date…,
  or On ice, which waits until you bring it back) and **Bring back**, the
  priority, labels, its epic, who claimed it, who's on it with **Go to agent**,
  what blocks it and what it blocks, the description and comments, and a box to
  add a comment. Claim really claims the bead (`bd update <id> --claim`, which
  sets you as its assignee).
- **New bead**: title (leave it empty and Ghostex writes one from the
  description), description, type (Task, Bug, Feature, Chore, Epic), priority
  P0 to P4, an epic, labels (a child starts with its epic's labels; remove any
  you don't want) and **Blocked by**, the beads it waits on. An epic's **Add
  child** opens it with that epic chosen.
- **Board**: the board you know from the Kanban, drawn in the same quiet style:
  lanes follow the statuses (backlog, open, in progress, test, review, done),
  you drag cards between lanes, and **Start work** on a card dispatches an agent
  to it (`ghostex board start-work <bead-id>`); the agent is linked to the card
  and the card shows who is working it. An agent started by hand links itself
  with `gx board associate <bead-id>`. On the Board, New bead also has the
  estimate and **Create & Start**. Opening a card shows **Blocked by** and
  **Blocks**; pick one under Add, search by title or ID and click to link; the x
  removes a link, and a link that would make cards wait on each other in a loop
  is refused. When the lanes do not fit, the board scrolls sideways (swipe,
  Shift+wheel, or drag the bar under the lanes). **Columns** adds the project's
  own lanes and has a **Show** switch for Backlog, Test and Review (Todo, In
  Progress and Done always show). A hidden lane still appears while a bead is in
  it, and the choice is saved in the project's Beads workspace, so every computer
  and clone sees the same Board (`bd config set custom.ghostex.board_hidden_columns
  test,review`; `bd config unset custom.ghostex.board_hidden_columns` shows them
  all again).
- **Sync**: the header says when the beads last synced from this computer
  ("Synced 4 min ago"); click it for the remote and the last push and pull.
  **Pull** brings in changes from your other computers (`bd dolt pull`), **Push**
  sends this computer's (`bd dolt push`). When a push doesn't go through because
  the remote has changes this computer doesn't have, a card says so with **Pull
  now**; then push again. A workspace with no remote shows no Pull or Push.
- Everything an agent changes with `bd` shows up in the view on its own within
  a few seconds, and an open view that nothing changes costs nothing: Ghostex
  re-reads Beads only when the workspace changes.
- A project on another computer is read with that computer's `bd`. If that
  computer runs an older Ghostex, the view shows a card asking to update it
  and the Board still works.
- The `$ghostex-manage-beads` skill teaches agents the same workflow from the
  command line. Ask a coordinator to "work the ready beads with high priority"
  to have it pick beads and start workers.
- **Open in New Window** (the pop-out control at the top right of the view, or
  the tab's right-click menu) moves the board to a window of its own, for a
  second monitor; the panel shows a card with **Show Window** and **Bring
  Back**, and closing the window brings it back too.

Related settings: `beadsViewLayout` (Tree or Board; if you had the Kanban tab
open before it became Beads, the view starts on Board), Settings > Projects (beads directory and display key),
`globalBeadsDirectory`, `globalBeadsDisplayKey`. Hotkey: Switch to Beads
(`switchKanbanView`).

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
  automatically, and a profile you add in a terminal shows up the next time
  you open Bots.
- **New bot**: the New bot row at the end of the list makes a bot for you on
  the computer the list shows (a connected computer included). It asks for a
  name (lowercase letters and digits, plus `-` and `_` after the first, which
  is also the name `hermes -p` takes), an optional one-line description, and
  what to start from: a copy of the bot you have open (Hermes if none), which
  copies its settings, `SOUL.md` and skills but not its Discord or Telegram
  tokens, or Blank. Create runs `hermes profile create` there; if Hermes
  refuses (the name is taken or reserved) the dialog says why and stays open.
  The new bot appears in the list at once and is selected, so its Edit SOUL
  button is right there to give it a personality.
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
  bot's own `SOUL.md` and `config.yaml` in the VS Code view. They always point at
  that bot's files and cannot be edited or removed. A bot on another computer
  has neither, and the browser build shows a message instead, since it has no
  VS Code view.
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

## GBots (beta, off by default)

GBots (beta) are your own team of bots, led by Chief of Staff in Main Hub. Each GBot
has a name, a role, standing instructions, an icon and a colour, and a memory
it keeps in every chat; its chats run on your own Claude Code or Codex. Turn
them on in Settings > Extensions, on the Experimental tab: they are off by
default, need Main Hub on, and turning them on asks for your approval (Touch
ID), so an agent can only ask. While GBots or Main Hub are off, nothing of
GBots shows, on your computer or the phone: no GBots group in Main Hub, no GBot
menu items, cards or Settings rows beyond the switch. Their chats sleep and
every `ghostex gbots` command stops; the bots and their memory stay for when
you turn them back on.

- **In the sidebar**: your GBots sit in Main Hub under Chief of Staff, in a
  GBots group above Main Hub's other chats. Each GBot's row shows its icon in
  its colour, its name and a dot while one of its chats works or waits for
  you; click it to fold its chats under it (a chat Chief of Staff opened says
  "Chief"). Right-click a GBot for New Chat, GBot Settings… and Remove GBot…,
  and right-click the GBots heading (or press its +) or Main Hub for New
  GBot…. On the phone, GBots show the same way in Main Hub; long-press a GBot
  for the same choices, and the GBots heading or Main Hub for New GBot. A
  GBot's folder never shows as a project.
- **GBot Settings**: its own window, opened from the GBot's row (GBot
  Settings…): Profile (name, role, description, icon and colour, the agent and
  model it runs on, how much it may do without asking), Instructions,
  Department, Memory, Routines, Computer and Bot API, each described below. A
  new GBot starts at Ask; a Codex GBot starts at Full access, because Codex's
  sandbox below Full can't reach Ghostex, and Codex still keeps it out of your
  secret folders. Only you can let a GBot do more.
- **Adding a GBot**: press + on the GBots heading, ask Chief of Staff for one, or run
  `ghostex gbots create --name <name> --title <role>` (add `--soul-file
  <file>` for its instructions, `--agent <id>` for what it runs on, `--icon`
  and `--color` for its look). From a terminal, Ghostex asks for your
  approval first (Chief of Staff adds one on its own, see Handing it work).
  Only you and Chief of Staff can add, change or remove GBots; a department's
  lead may change only its members' instructions, and other agents are refused.
- **Chatting with it**: `ghostex gbots new-chat <bot>` opens a chat (add
  `--task "<text>"` to hand it something to do). A GBot can have several chats
  at once, all sharing its instructions and memory; `ghostex gbots chats <bot>`
  lists them and who opened them, and `ghostex gbots close-chat <chat>` closes
  one (Chief of Staff can close only the chats it opened).
- **Its folder**: each GBot has a folder holding its instructions (`SOUL.md`),
  its memory (`MEMORY.md` and `memory/`) and its working files (`files/`);
  `ghostex gbots show <bot>` says where. A new chat starts with its
  instructions and memory loaded, and `ghostex gbots brief --bot <bot>` shows
  what it loads. Change its instructions with `ghostex gbots edit <bot>
  --soul-file <file>`: editing `SOUL.md` itself changes nothing, and `show`
  points such an edit out.
- **Its memory**: a GBot keeps dated notes in `MEMORY.md` (`ghostex gbots
  memory add "<note>"`, with `--until <date>` for something that stops being
  true), longer detail in topic notes it reads when needed (`--topic <name>`),
  and Ghostex writes a daily log of what it did. Up to 200 lines and 24 KB of
  `MEMORY.md` load at the start of every chat; when it fills up, the oldest
  notes move to `memory/archive.md`. Right-click the GBot and choose Memory…
  (or open the Memory tab of GBot Settings) to see what loads with a gauge
  (amber when nearly full), edit `MEMORY.md` (the save is refused if the GBot
  wrote to it meanwhile), show a topic in Finder, read the daily log and undo
  the newest change. On the phone the tab is read-only with Undo. From a
  terminal: `ghostex gbots memory show|add|forget|edit|undo|journal|topics|topic|log
  --bot <bot>`. Keys and passwords in a note are replaced with «redacted».
- **Preferences every GBot reads**: `ghostex gbots prefs set --text "<how you
  like things>"` (you and Chief of Staff only) puts up to 100 lines in every
  GBot's brief; open chats are told to read it again.
- **Routines**: ask a GBot in its chat for something on a schedule ("every
  weekday at 8, check my inbox folder") and it sets a routine: each run is a
  chat of that GBot, and a "Routine set" card in its chat shows the schedule in
  words and the next run, with Pause, Edit and Undo. A routine can check first,
  with no model (new Gmail, a calendar event coming up, a web page, a folder, a
  command), and runs only when that check sees something new, so a quiet day
  costs nothing. Runs come at least 15 minutes apart, 24 a day at most; a run
  missed while Ghostex was closed is caught up only if it is less than 12 hours
  late, and a failing check backs off up to 4 hours. A routine can also tell
  Chief of Staff when it finds something, or hand what it finds to Chief of
  Staff to tell you with a notification (`--deliver brief`). The Routines tab
  of GBot Settings (on the phone too) lists them with Pause, Resume, Run now and Remove; Edit on the
  card opens the Automate view. A command check a GBot sets waits paused until
  you turn it on. From a terminal: `ghostex gbots routine
  list|add|edit|pause|resume|remove|run-now --bot <bot>`.
- **Keep an eye on my day**: with Gmail or Google Calendar connected in
  Settings > Connectors, ask Chief of Staff to keep an eye on your day (it
  also offers once, when they are connected). It adds a GBot, Day Watcher,
  that only reads your mail and calendar (Ghostex refuses anything it tries to
  send, reply to, accept, create or change) with two routines: a daily brief at
  08:00 (Needs you, Your day, Important emails, Routines) and a watch every 15
  minutes from 07:00 to 21:45 that tells you when a meeting is about to start
  or important mail comes in. Both land in Chief of Staff's chat with a
  notification and a phone push; a quiet watch costs nothing. Ask Chief of
  Staff for another time or to stop, pause a routine or Day Watcher in its GBot
  Settings, or remove it. From a terminal: `ghostex gbots day-watch
  [status]|on [--brief-time HH:MM]|off`.
- **Starting work in a folder**: ask a GBot to make a project in a folder and
  set up a coordinator for it, and it adds the folder as a project (creating
  the folder if needed) and starts the coordinator there, so it shows in your
  sidebar and in `ghostex coordinator list`. A GBot's own folder holds only its
  chats: a coordinator, thread or session can never start there, so a GBot (or
  you) passes `--path <folder>` or `--project-id <id>` to `ghostex coordinator
  create`, `coordinator start-thread` or `agents create`.
- **Its computer**: give a GBot a computer of its own in the Computer tab of
  GBot Settings (right-click the GBot, Computer…): one of your connected
  computers (Settings > Remote, with Agent access on) or one of your Cloud
  Boxes. The chats you, Chief of Staff and other GBots open with it then run
  there (chats already open stay where they are), and its row shows that
  computer's name. On a connected computer its chats work in `~/GBots/<name>`
  there, still read its instructions and keep its memory on this computer, and
  show under its row while that computer is connected. On a Cloud Box each chat
  gets its own worktree of the box's project and carries its instructions with
  it; a chat Chief of Staff or another GBot starts there asks for your approval
  first, since the box gets your agent logins and the GBot's instructions and
  memory, and runs and bills while it works. Use the screen and apps
  lets it drive apps on that computer with SpaceO or cua-driver (macOS); off,
  its chats start without those tools (a Codex GBot at Full access can still run
  their command-line tools from a shell). Its folder, memory and routine runs
  stay on this computer. Only you give a GBot a computer, a new folder there or
  computer use, from Ghostex on your computer; the phone's
  Computer tab shows where it works and can give it this computer back. From a
  terminal: `ghostex gbots computer <bot>` shows where it works and what it
  could have, and `ghostex gbots computer <bot> --use this` brings it back.
- **Calling it**: right-click a GBot and choose Call (on the phone, long-press
  it) to talk to it out loud through the Voice agent; on one of its chats, the
  arrow beside the work area header's headset offers Call <GBot> (on the phone,
  the chat's ⋯ menu). It answers as itself in its own voice and passes what you
  say to its chat, so the call's messages are in that chat (your standing chat
  with it, or the chat you called from) and the call is in its daily log. While
  it works it says what it is doing; when its chat waits on a question or a
  permission prompt it asks you out loud and answers with what you choose. A
  Ghostex approval card it can only ask for: approve it on your computer or
  phone, since a spoken yes never approves it. Pick its voice in
  the Profile tab of GBot Settings (or `ghostex gbots voice <bot> <voice>`);
  without one it uses the Voice agent's voice. A call with a GBot stays with
  it (no All sessions); calls need the Voice agent on with its key, and reach
  GBots whose chats run on this computer. While GBots are off there is no Call
  anywhere.
- **Removing it**: `ghostex gbots remove <bot>` closes its chats, removes it
  and moves its folder, memory included, to the Trash.
- **Departments**: group GBots under a name, like Research or Writing. Every
  GBot in a department follows its shared instructions under its own, and a
  department can have one lead, marked with a small crown, who hands the
  others work and may change their instructions (nothing else). In Main Hub a
  department is a small heading under GBots that folds its bots; right-click
  it for Department Settings… or Remove Department… (its GBots stay, in no
  department). Set a GBot's department, whether it leads, and the department's
  name and shared instructions in the Department tab of GBot Settings (on the
  phone too), where you can also add a new department. Only you and Chief of
  Staff change departments. From a terminal: `ghostex gbots department
  list|show|create|edit|set-lead|remove`, and `ghostex gbots edit <bot>
  --department <name>|none [--lead]` to move a GBot.
- **Handing it work**: Chief of Staff hands work to the right GBot on its own,
  and adds one when no GBot fits, telling you in a "GBot added" card in its
  chat with Undo (Undo removes the bot and closes its chats; it is refused once
  the bot was changed since). To have it ask you before each add instead, turn
  off Settings > Agents > Chief of Staff may add GBots on its own. Chief of
  Staff and a department's lead hand a GBot work with `ghostex gbots ask <bot>
  --task "<brief>"` (Chief of Staff gives a department with a lead its work
  through the lead); any other GBot or coordinator may only ask it a question
  (`--consult`). The answer comes back to the sender's chat when the GBot
  finishes, headed "Researcher reports on A12"; a report that falls short goes
  back with `ghostex gbots ask --rework A12 --task "<what is missing>"`, twice
  at most. `ghostex gbots assignments` lists the work handed out, `ghostex gbots
  changes` what was added or changed, and `ghostex gbots undo <change>` undoes
  an add or a change.
- **When a GBot gets stuck**: Ghostex watches the GBot chats nobody is
  following (ones you opened, routine runs, ones Chief of Staff opened) and
  tells Chief of Staff in its chat when one can't start (Ghostex tries a plain
  start failure once more first), stops and can't be restarted, shows no new
  output for 20 minutes, or a routine run fails. Chief of Staff can restart it
  where it stopped (twice at most) or send it a new message, and tells you in
  two lines when only you can fix it, such as a sign-in or a missing key. A
  GBot opened by anyone but you that waits on a question or an approval goes
  to Chief of Staff too: Chief of Staff may answer it for the GBots it added
  (never a push, a deletion, a publish or spending money you didn't ask for),
  and otherwise tells you where to answer; a sign-in always comes to you. A
  GBot can also raise a flag itself (`ghostex gbots flag "<what it needs>"`).
  A chat that keeps hitting problems (more than 5 in an hour, or 3 waits) stops
  being reported for an hour and you get one notification. From a terminal:
  `ghostex gbots problem list|retry|rebrief|close`.
- **Team files**: share a team of GBots with another Ghostex, or keep a copy,
  as one JSON file. Right-click the GBots heading in Main Hub and choose Export
  Team… (the whole team, a department, or one GBot as a preset; tick Include
  starter notes to add each GBot's memory and topic notes), then Save… or Copy;
  on the phone, long-press the GBots heading and choose Export Team to share the
  file. A team file holds each GBot's name, role, description, look,
  instructions and department, the departments' shared instructions and leads,
  and the GBots' routines. It never holds chat history, the agent or model a
  GBot runs on, approval levels, computers, keys or sign-ins, and anything that
  looks like a password or key is replaced with «redacted» (the export says
  how many). Import Team… (or Import Team on the phone) shows what a file adds
  before anything is added: names already taken get " 2", routines arrive
  paused until you resume them, and imported GBots start at the approval level
  every new GBot starts at (Ask; Codex GBots at Full access). Importing asks for
  your approval, like adding a GBot; afterwards Undo Import removes the whole
  import, unless one of its GBots was changed since. From a terminal: `ghostex
  gbots team export [--department <name> | --preset <bot>] [--notes] [--out
  <file>]` and `ghostex gbots team import <file> [--dry-run]`.
- **Presets**: New GBot can start from a preset (Researcher, Writer, Planner,
  Code reviewer, Note keeper, and the ones you keep): pick one under Start from
  to fill the name, role and instructions, then change what you like. Keep a
  GBot as a preset from Export Team… > One GBot > Keep as Preset, or open a
  `.gbots-preset.json` file with Import Team…. From a terminal: `ghostex gbots
  preset list|show|import|save|export|remove` and `ghostex gbots create
  --preset <preset> --name <name>`.
- **Bot API**: let an app or script on your computer message a GBot and get
  its reply. In the GBot's settings (its row's GBot Settings…), open Bot API and
  turn on "Apps may message <name>": it then shows the
  GBot's key once, inside a `curl` command to paste (Copy command, Copy key).
  Ghostex keeps only the key's fingerprint, so copy it then; New key makes
  another and the old one stops working, and turning it off stops the key at
  once. Only you can turn it on, make a key or turn it off (agents, Chief of
  Staff included, can't), and each change goes into the security log. An app
  sends `POST /hooks/gbots/bots/<id>/messages` with `{"message": "…"}` and
  `Authorization: Bearer <key>`, and gets the GBot's reply (`"wait"` up to 300
  seconds; a longer answer comes back as "working" with an address to ask
  again; add `"chat": "new"` for a fresh chat). `POST
  /hooks/gbots/bots/<id>/routines/<routine id>/run` starts one of the GBot's
  routines. It is reachable only from your computer itself and from devices
  that already reach it over SSH (a paired phone, another computer of yours):
  Ghostex opens no port for it. The Bot API is for GBots on Claude Code or
  Codex. Messages from apps land in the GBot's chats titled "API · …", marked
  as coming from outside. Those chats start in their own folder, are kept out
  of your secret folders and Ghostex's own files (Codex can read the rest of
  the disk but write only its own folder, and never asks; Claude asks first as
  at Ask), never save to the GBot's memory, and read its memory and your shared
  preferences only when you turn on "Apps may read <name>'s memory" (it asks
  for your approval). Only you, from Ghostex on your computer or phone, answer
  what they wait on or send them anything. A key makes up to 10 calls a minute,
  200 messages and 20 new chats a day, keeps at most 3 chats open (a new one
  closes the oldest that isn't working) and opens at most one a minute; a
  message is at most 48,000 characters. On the phone: GBot Settings > API. From
  a terminal: `ghostex gbots api [status] <bot>`, `api on <bot> [--label <who
  uses it>]`, `api new-key <bot>`, `api off <bot>`, `api share-memory <bot>
  on|off`.

The previous GBots, which downloaded a separate runtime and showed its bots in
the Bots space, is gone, and its bots and their chats were not carried over.
On the first start after the update, each computer stops its background
server, removes those bots and their chats from the sidebar, deletes its
download (about 50 MB) and moves its old folder, with each bot's instructions
and notes, to the Trash as "GBots (previous)". The Bots space now shows Hermes
bots only.

Settings: `gbotsEnabled` (off by default), `gbotsChiefAddsOnItsOwn` (on by
default). CLI: `ghostex gbots list`, `show`, `create`, `edit`, `remove`,
`chats`, `new-chat`, `close-chat`, `brief`, `memory`, `prefs`, `routine`,
`day-watch`, `ask`, `assignments`, `changes`, `undo`, `flag`, `problem`,
`department`, `team`, `preset`, `computer`, `api`.

## Voice agent

Talk to your agents out loud. On a Mac, Windows or Linux, the headset button in
the work area header (left of the Start, Open and Commit buttons) starts a spoken conversation with Ghostex's voice
agent, and pressing it again hangs up; Option+Ctrl+V (Ctrl+Alt+Shift+V on Windows
and Linux) does the same from anywhere in the window. Ask what your sessions are doing or
which ones are waiting on you, have it read you the last messages of a session,
tell it to send a prompt to a session (it sends right away and says what it
sent, but for anything risky or destructive, like deleting files or removing a
project, it reads the prompt back and sends only after you say yes), answer a question card or approve or deny a permission prompt, start
a new agent in a project you name, or start a coordinator there ("start a coordinator in
Ghostex to work through the Discord requests": what you said becomes its first message).
Say "stop the Discord one" and it stops that session's running turn at once, like the
chat's Stop button (the session stays open); when your words could mean another session
too, it asks first and shows a card with Stop and Cancel. It cannot close a session. Say "draft it" or "let me check it first"
and it puts the message in that session's chat box without sending it: edit it
and press Enter yourself, or say "send it" and it sends the draft as it is now,
your edits included (if you already had unsent text there, it asks whether to
add to it or replace it). To send an agent a screenshot, copy the image to the
clipboard on your computer and ask the voice agent to include it: it tells the
agent to read the image from the clipboard (this works only for agents on that
computer, not on a remote machine). Name sessions and projects the way you
would say them ("the sidebar one"), or say "this one" for the session you are
looking at; it asks when two match. When it reads messages it can tell you
when they were sent ("he replied five minutes ago"). Whatever session
it talks about (reading its messages, a question it asks, a draft) is shown in
the app: the sidebar selects it and the work area opens it, switching project
or Space when needed, but not while you are typing, so it never pulls you away
mid-sentence. Say "show me the sidebar one" to switch to a session yourself,
even while typing. It sees a coordinator's threads as part of that
coordinator: asked what is going on, it reads the coordinator (like Main
Coordinator) and reads a thread only when you ask for it ("what's the Windows
popup thread doing"). It also sees the sessions on the other computers you are
connected to (Settings > Remote > Computers this machine connects to), named with their computer
("Sidebar fix on Omarchy"): it can read them, send them prompts, answer their
question cards and approvals, stop them, show them and start agents and
coordinators in that computer's projects. Drafting into a chat on another
computer and answering Ghostex forms there are not available; it says so. A
computer whose Agent access is off (in Settings > Remote > Computers this machine connects to, on
either computer) stays out of reach, and the agent tells you why. While the
conversation is open it tells you when a session finishes its turn or starts
waiting on a question or an approval. The conversation keeps going while you
switch projects and chats. A small panel under the header shows what it is
doing, live captions of both sides and the tools it runs, with a microphone
button (mutes you), a speaker button (mutes the agent's voice for this call: its
captions keep coming, and unmuting speaks from its next reply), a small talk
button and Hang up; hover a button to see what a click does. While the agent
speaks, a Stop talking pill
shows above the buttons (talking over it also stops it in a Regular call). Drag the panel by its top
bar to put it anywhere in the window (it stays there until you quit Ghostex),
and drag any of its edges or corners to make it bigger or smaller (Ghostex
remembers the size). The captions keep scrolling to the newest line; scroll up
to read back the conversation, and the down-arrow button at the bottom right
takes you back to the newest line. You can select and copy the captions, and
type to the agent in the text box under them: Enter sends what you typed as
your turn (it stops a reply being spoken, and Mute stays as it is), Shift+Enter
starts a new line. Click the voice name ("Grok ·
Eve") to pick another voice: the list opens on the voice you use now. Grok switches right away when it can, and GPT
Realtime keeps a call's voice once it has spoken, so the new voice starts with
your next conversation (the panel says which). The small speed button right of
the voice name ("1×") opens a menu of speeds (1×, 1.2×, 1.4×, 1.6×, 1.8×, 2×)
that make it talk faster without changing its pitch, at once, even
mid-sentence. Calls come in two modes, chosen in Settings > Voice Agent > Call
mode or with the gear beside the speed button. Regular (the default) is
hands-free: the agent listens all call and you can talk over it; on Grok you
pay for the whole time the call is connected, quiet and muted time included
(about $0.08 a minute). Push-to-talk listens only while you hold the talk
button (the waveform right of Mute): pressing it while the agent speaks stops
it, and letting go sends what you said; a quick tap sends nothing and says
"Hold to talk, then let go to send". You can also hold Space while you aren't
typing anywhere (the panel shows a Space key when it works), or hold the
"Hold to Talk" hotkey, which works even while you type in a chat or a terminal
(no key is set; give it one in Settings > Hotkeys). If the window loses focus
while you hold Space, what you said so far is sent. On Grok push-to-talk
calls you pay only for the audio you send and the agent's replies. GPT
Realtime bills only audio in both modes. A change applies to your next call;
Regular stays the default. Answer when I say "reply" (Settings > Voice Agent,
right under Call mode, on by default, shown only while the call mode is Regular)
makes a Regular call wait for you: the agent answers only once what you said ends
with "reply", "over" or "go ahead", and that word is taken off the caption. Anything
it would have started to say by itself is dropped. Each call opens with a short spoken
introduction that also asks you to say "reply" while the setting is on. Push-to-talk,
typed turns and recorded turns are unchanged. Turn it off to have the agent answer
as soon as you stop talking. Mute turns your microphone off; in a push-to-talk
call it also locks the talk button ("Unmute to talk"), and in a muted Regular
call holding the talk button, Space or the hotkey unmutes you for as long as
you hold. It hangs up by itself after 5 minutes with nobody talking.

A call can also be about just one session. Click the small arrow on the
headset button and pick "This session: <title>" (the session you are looking
at), or press the "Voice Call About This Session" hotkey (no key is set; give it
one in Settings > Hotkeys). The agent starts by saying where that session
stands, tells you when it finishes its turn with the gist of its reply in a
sentence or two, and mentions no other sessions; asked about another one, it
says "This call is about <title> only. Say "all sessions" or press All sessions
to talk about the others." It can send that session prompts, draft in its chat
box and answer its questions and approvals; in a call about a coordinator it
can also read its threads and answer their questions, but sends prompts only to
the coordinator. A strip under the panel's top bar names the session, its
project and its coordinator; click the name to show the session. All sessions
on the strip (or saying "all sessions") widens the same call without hanging
up, and picking the other row of the arrow's menu switches an open call either
way; the menu ticks what the call is about now. While the call runs, that
session's sidebar row shows a small headset. Reading the session doesn't
switch your window to it; asking to see it does. If the session is closed
during the call, the strip says Closed and the call goes on. A call about one
session doesn't offer or replace the remembered last conversation. For a
session on another computer you are connected to, the call runs on that
computer with its own voice key (add one in Settings > Voice Agent on that
computer); the strip says which computer. A call stays on the computer it
started on, so picking a session of another computer while a call is open asks
whether to keep the call or hang up and call, on the computer and the phone
alike. A computer whose
Ghostex is too old for it says "Update Ghostex on <computer> to have a voice
call about one session." On the phone, "Voice call about this chat" in a chat's
⋯ menu and "Voice Call" when you long-press a session start the same call, and
the voice screen shows "Only about <title> (<project>)" with an All sessions
button.

It remembers your last conversation as a short summary, kept on your computer
until the next conversation replaces it. When you start a new conversation, the
panel shows a "Last conversation" card: press Continue and the agent says in a
sentence where things stood and carries on from there, or press Start fresh (or
just start talking) to begin a new topic. You can also say "pick up where we
left off" at any time. Once it continues, the panel shows "Remembering last
conversation"; click it to read the whole summary. Forget on the card, or the
Forget button in Settings > Voice Agent > Memory, deletes the summary for the
desktop and the phone. The summary is written by a small text model of the same
provider, with the same key, right after a conversation ends.

It needs an API key: pick xAI Grok Voice (the default) or OpenAI GPT Realtime
in Settings > Voice Agent, pick a voice, and paste the provider's key there
(xAI keys come from console.x.ai, OpenAI keys from platform.openai.com). The
key is checked with the provider before it is saved, kept encrypted on this
computer and never shown again. If a saved key can't be read anymore,
Settings > Voice Agent says so on its row: paste it again. The provider bills your account while a
conversation is open. When the provider refuses a conversation (for example
xAI answering "payment required" because an X subscription's included usage
is used up for the moment), the panel shows the provider's own reason.

Settings > Dictation > Past recordings lists every recording you made with
Dictate, on the computer or the phone, newest first and grouped by day, with
its text, "This transcription was dismissed" with Recover for one you
cancelled, or why it failed with Try again. Hover a row to Play it, Download
the audio, Copy its text, Transcribe it (the text is copied to your clipboard
too) or Delete it; the search field finds dictations by their words.
Recordings stay on this computer until you delete them, about 0.3 MB for each
minute of speech.

Ghostex cancels the agent's own voice from the speakers, so it works without
headphones, on a Mac, Windows and Linux alike. It uses the computer's default
microphone and speakers; when you plug in a headset or pick another default
device in the sound settings, it switches to it. If Ghostex can't use the
microphone, allow it: on a Mac under Microphone in System Settings > Privacy &
Security, on Windows with Microphone access and Let desktop apps access your
microphone in Windows Settings > Privacy & security > Microphone.

On the phone (Android or iPhone), the Voice button in the bar at the bottom of the session
list (and the headset at the top of a session) starts the same conversation with the voice agent on the
computer you are looking at. The key stays on that computer; the phone never
stores one. When the agent shows a session, the voice screen offers an
"Open <session>" button that opens its chat while the conversation keeps
going, and the computer's window switches to it too unless someone is typing
there. The conversation keeps going while you move around the app, switch
apps or lock the phone, with a "Voice mode on" notification that has Mute and
Hang up (in a push-to-talk call, Talk instead of Mute: tap it to start talking
and tap it again, now Send, to send), and it reconnects by itself after a
short network drop. On an iPhone the conversation is a call instead of a
notification: the lock screen, the Dynamic Island and the green call pill
show it with Mute and End, and it stays out of the Phone app's Recents. In a
push-to-talk call a Live Activity on the lock screen and in the Dynamic Island
shows the call's state with Talk (Send while you talk), so you can talk with
the phone locked; allow Live Activities for Ghostex when the iPhone asks. It talks
through the phone's speaker, or a Bluetooth headset or car hands-free link
(CarPlay on an iPhone) when one is connected, and pauses while a call or a
navigation prompt plays; on an iPhone a phone call coming in offers to hold
the conversation (it goes on when the call ends) or end it.
The phone's voice screen has the same voice picker (tap the voice name under
the computer's name) and a small speed button beside it that opens the list of
speeds, the same gear for the call mode, and the same talk button between
Mute and Hang up: press and hold it to talk and let go to send, or slide your
finger off it ("Release to cancel") and let go to send nothing; slide back on
to keep talking. Stop talking is a pill above the buttons while the agent
speaks. The call mode is also in the phone's Settings > Voice, with the same Answer when I
say "reply" switch under it while Regular is picked (a computer too old to have it
shows no switch). They save the computer's settings, so the desktop and the phone talk
with the same voice at the same speed and in the same mode. It shows
the same "Last conversation" card with Continue, Start fresh and Forget.

The voice agent is a built-in extension, on by default; its settings are
Settings > Voice Agent, under App in the Settings sidebar right below Dictation.
Turn it off with its switch on Settings > Extensions (the Voice
agent card) and the headset and its arrow leave the top
of the window, the voice agent hotkeys stop working and leave Settings >
Hotkeys, a call that is running ends, and the phone app hides its headset,
Voice call about this chat and Voice Call for that computer, and Settings >
Voice Agent leaves the sidebar. Dictate keeps working: Settings > Dictation
still shows the key its engine needs and Past recordings. Turn it back on and everything returns, with your voice
settings as you left them.

Settings: `voiceAgentHidden` (off by default, which keeps the voice agent on), `voiceAgentProvider`, `voiceAgentXaiVoice`, `voiceAgentOpenaiVoice`, `voiceAgentSpeed`, `voiceAgentCallMode`, `voiceAgentWaitForReply`; the Memory row (`voiceAgentMemory`) forgets the remembered conversation. Hotkeys: Toggle Voice Agent, Voice Call About This Session, Hold to Talk (`holdToTalk`).

## Remote machines, web, and mobile

Ghostex is a client/server system: gxserver runs on each computer and owns its
sessions, so any client can control agents on any machine.

- **From a phone**: sidebar More Options > Connect a Device. One window,
  Connect a device, opens with three ways listed at its left (Phone, Computer,
  Tailscale) and turns Easy Connect on by itself (if SSH access is off, it asks
  for an admin password once), then shows the code under Phone. The code there
  is small: click it to show it large enough to scan, and click it again or
  press Escape to go back. Easy Connect needs its helper
  (Tailcat), which the Ghostex app doesn't carry: on a computer that doesn't
  have it yet, the window says "The Easy Connect helper isn't installed yet"
  with Install the helper. Ghostex then downloads its build tools (about
  65 MB) and builds the helper on this computer once, which takes a few
  minutes; the window shows each step (downloading with a percentage,
  unpacking, building, finishing) and the code appears in the same window when
  it is done. The install keeps going if you close the window. A failed
  install says why in plain words (for example that the computer seems
  offline) with Try again; meanwhile, phones on Tailscale or on your own
  network can still connect. Turning Easy Connect on any other way (its
  switch, `ghostex tailcat enable`) also starts the helper's install when it
  is missing, and Easy Connect starts on it once it is in. On the phone, the first screen lists the steps (get Ghostex
  on the computer, open Connect a Device there, scan its code here) and Scan
  the code opens the scanner: two steps, with the camera in the second (on an
  iPhone, Open the scanner there opens Apple's scanner; on Android the camera
  is right in the step, with a light button). It won't scan gives tips. If the
  phone can't scan it, click Copy the code under the code and tap Paste a code
  instead on the phone (also under Other ways to connect on the first screen,
  beside Use Tailscale). The code works once, for five minutes. "Need the
  app?" at the bottom of the window links to the Android app and to the App
  Store for iPhone. Computer in the window's list shows another computer's two
  steps and the code to copy. Tailscale in the list says what it is for and
  shows its checklist as it stands (Tailscale running on this computer, SSH
  access, Tailscale on the phone, the code), with a button to Settings >
  Remote > Tailscale, where its code and its switch are; while Tailscale is
  off it says so, and the button leads to where it is turned on. Remote, at
  the bottom left of the window, opens Settings > Remote, where the paired
  devices are.
  Settings > Remote shows, on one screen: Phones and computers that can reach
  this machine (Easy Connect's switch, Connect a Device, Connect a Computer and
  every paired or waiting device, whether Easy Connect is on or off; while SSH
  access is off it says so with Turn on, and while the Easy Connect helper
  isn't installed it says so with Install, the install's step while it runs,
  and Try again when it failed), Computers this machine connects to
  (the saved computers and Add a computer), and three rows that open in place:
  This computer (its name, set by the system, its icon and SSH access,
  with the macOS, Windows or Linux steps to turn SSH on by hand), Tailscale
  (another way in for tailnets; its code sits behind a click-to-show QR with
  Copy as text) and Advanced (Relay, ports, allowed keys, the pairing address,
  raw status). The page reopens with the row you left open.
  The phone's own camera can scan the code too: the QR is a ghostex.dev link,
  so with an up-to-date Ghostex app installed the camera opens the app straight
  into pairing, on Android and iPhone.
  Scanning doesn't pair the phone by itself. The phone shows Check your
  computer with three words and how long it still waits (The computer shows
  nothing says where to look), and the Connect a device window turns into "Is
  this your phone?" with the phone's name and the same three words. Approve
  only if they are the same; Not my phone says no. After Approve the window
  says "<phone> is connected"; click Done, or Connect another phone. If the
  window is closed, the same question shows as a notice, and in Settings >
  Remote at the top of the device list; phones already paired can approve too.
  Approve is one click unless Settings > Security's "Require Touch ID or your
  password to approve" is on. When it works, the phone opens your sessions
  with "Connected to" the computer on top. When it doesn't, the phone stays on
  a screen that says why (nobody approved in time, the code was used up, the
  computer said no, the words did not match, the computer can't be reached)
  with Scan again and Paste a code instead. A code lasts five minutes, works
  once, and only the Connect a device window makes one; no agent or command can
  get it. The phone's sign-in key lets it
  open terminals and run Ghostex there, but not forward your SSH agent or
  windows.
  Without the app, the link opens a ghostex.dev page that says how to install
  it. An older phone app can't read the new code with its own scanner: update
  the app first.
  The Android app is the `ghostex-android.apk` on the GitHub release, built
  for 64-bit (arm64) phones. If Android says "App not installed as package
  appears to be invalid", uninstall any other Ghostex build first (one you
  built yourself is signed differently) and install the latest release's APK;
  if it still fails, `adb install ghostex-android.apk` from a computer shows
  the real reason.
  To look around with no computer and no account, open the phone app and tap
  **Try a demo computer**, the small link under Scan the code on its first
  screen (with a computer already added: Machines > Add a computer > Open
  demo). The Sessions list opens for **Demo
  computer**, with sample projects (Atlas API, Side project, Wander). A chat
  answers with a reply saved in the app, the terminal is a script (`ls`,
  `pwd`, `git status`, `help`), and Web Preview shows a page saved in the app.
  Nothing is sent over the network, and voice is off. Remove it in Settings >
  Demo computer > Remove demo computer. Pairing a real computer is separate.
  The phone app keeps itself up to date: most releases download in the
  background and apply the next time the app starts. When an update is ready,
  or a release needs a new app, an Update button shows at the top of the
  Sessions screen: it restarts into the update, or gets the new app (the
  GitHub APK downloads it and opens Android's installer; Google Play,
  TestFlight and the App Store open their page). The phone's Settings >
  Updates shows the app's version and where it came from, the update it runs
  (its date, id and what's new), Restart to update, and Check for updates,
  which checks for a new update and a new app at once.
  With Auto reconnect enabled in the phone's Settings > Connection, the phone
  checks the connection when you return to the app or its network changes and
  reconnects interrupted agent terminals. Choose Reconnect from the computer's
  menu to start a fresh connection. Easy Connect does not
  require the separate Tailscale app; the Tailscale connection option does.
  While everything works, the Sessions screen says nothing about connections:
  no "Connected to" line and no "Refreshing" line. Each computer's own icon
  shows its link instead: green is connected, yellow is slow, red can't
  connect. With Separate by machine off the icons sit in a row at the right;
  with it on, each computer's tab carries its own icon. Tap an icon (or the
  failure line) to open the Connections sheet: every computer, the ones with
  problems first, with the way the phone reaches it and how long a round trip
  takes. The way is Tailscale with its address, or, for Easy Connect, Private
  Relay (Ghostex's own relay, or a relay you run), Global Relay (Tailscale's
  public relays), Direct (phone and computer talking straight to each other) or
  Same network (both on the same local network). For a computer
  that can't connect, the sheet says why in plain words and what to do (for
  example "Tailscale is off on this phone" with Open Tailscale, or the
  failure's reason with Remote Settings), with Retry. Logs and Connection
  settings are at the foot of the sheet. If the connection fails while saved
  sessions are still visible, the failure line says so for every computer ("2
  computers aren't connected"); it says "Connecting to" a computer only after
  five seconds. The old amber "Tailscale isn't connected" card and its reminder
  toggle are gone.
  Slow-connection check: while you use the phone, it times each connected
  computer's round trip every 15 seconds. A computer counts as slow when its
  round trip stays over 400 ms for 30 seconds, or when its link drops twice in
  five minutes, and as fine again after a minute under the limit. A slow
  computer's row in the Connections sheet offers Send logs.
  Settings > Connection lists every computer with its status and its live way
  of connecting (each opens that computer's page), then Auto reconnect and
  Troubleshooting > Logs. Logs is also in the menu at the top of the Sessions
  screen (☰), and Settings search finds Computers and Logs. Logs shows the
  phone's connection and status events; Copy copies them. Send to developer,
  beside Copy, first opens a review step that says what the report holds (the
  phone's connection and status events, the app version and phone system,
  routes and timings), what is replaced (IP addresses, user names, computer and
  host names, keys) and what is never sent (sessions, chats, prompts, terminal
  text). It says plainly that sending files a public issue in the Ghostex
  repository on GitHub, through feedback.ghostex.dev. You can add a note and
  your GitHub username (the one Send Bug / Feedback remembers), and show the
  exact text. Nothing is sent until you press Send. In the demo computer the
  route line reads "Sample, on this phone".
  To find a session on the phone, tap Search in the bar at the bottom of the
  Sessions list (beside Voice, Web and Machines) and type part of its name or tag; it searches the open and
  closed sessions of every connected computer, newest first, like the desktop's
  Sessions search. Tapping an open session opens it; tapping a closed one (shown
  with a hollow dot) brings it back on its computer and opens it. Closed
  sessions appear once that computer runs an up-to-date Ghostex.
  A phone that signs in over Tailscale, the local network or SSH (not Easy
  Connect) needs no pairing: its SSH login is enough, so it reads, replies,
  stops, types and approves from its first sign-in (with the phone app from
  this release; an older phone app still asks once, and Ghostex on the computer
  shows "Approve this phone?" with three words to compare). Computers you reach
  over SSH don't pair either, and a computer you add under Settings > Remote
  lets the agents on both computers reach each other at once. With the
  computer's Agent access off they don't.
  Paired devices are listed and can be removed with one tap, on the computer or
  on any paired phone (on the phone, Machines > the computer lists them under
  Devices that can reach this computer).
  Each phone pairs with its own device key, kept in its Secure Enclave or
  Android Keystore, and gets an access token tied to that key, so removing it
  cuts that phone off at once: its sign-in key, its token and its right to
  approve go together. Pairing a phone again replaces only that same phone's
  earlier pairing (proven by its device key), and an Easy Connect key belongs to
  one device: a pairing that names a key another paired device holds is refused,
  and the message names that device and the day it paired, so you remove the
  right one. Phones paired before Ghostex 11.1.0 no longer connect
  and are listed as "pair it again": scan a new code from the phone, or remove
  them. Only paired devices can connect, always: a removed phone can't even
  reach the computer through Easy Connect, and a new phone gets in only while
  the Connect a device window is open or it waits for approval (paired phones
  reconnect once when the window opens and about a minute after it closes).
  Another computer connects the same way: on this computer click Connect a
  computer (Settings > Remote, or Computer in the Connect a device window's
  list), then Copy the code, and on the other computer paste it in Settings >
  Remote > Add a computer. When you click Add computer, both computers show
  three words; check they match and approve the request here.
  Then the other computer can reach this one through Easy Connect, and still
  signs in with this computer's SSH username and password. To re-pair a
  computer paired before 11.1.0, open it there and paste a new code the same
  way.
  Easy Connect meets the phone on a relay. Relay (Settings > Remote > Advanced) picks
  which: Ghostex relay (the default) has phones try Ghostex's own relay first
  and Tailscale's public relays second; Public relays only uses Tailscale's;
  Custom takes the hostname of a relay you run (Tailscale's derper); hover the
  (i) after an option's name for what it does. Phones
  paired earlier keep working whichever you pick, and an up-to-date phone app
  learns the new address on its next connection. When the chosen relay is
  down, the pairing QR and phones use the public relays. Computers that
  connect with an Easy Connect code keep using the public relays.
  On the phone, open Web Preview
  from the machine menu and enter a website address or a port such as `3000`
  immediately, or choose a listening port from the list. The address bar stays
  editable while browsing. The Desktop site button in its toolbar loads the
  page the way a desktop browser would (like Safari's Request Desktop
  Website), and the phone remembers that choice for each site. The list groups Web pages, Development tools,
  Services, and Other ports into collapsible sections; services and unidentified
  ports start collapsed. Search by page title, process, or port. With a computer
  supporting `ghostex ports --json --web`, responding pages show their title,
  HTTP status, and supported favicons. Common-port labels such as Storybook on
  6006 are marked “likely” until identified by the page or process.
  Localhost links in chat, terminals, and browser
  actions open in Web Preview through the connected computer, including their
  path and query, instead of the phone's external browser.
  To copy from a session's terminal on the phone, press and hold a word: it is
  selected, and moving your finger before you let go selects more. Drag the
  handles to adjust it, then choose Copy in the bar that appears (Select All
  selects everything on screen). On iPhone and iPad, a double tap selects a word
  too. To paste, press and hold the terminal and choose Paste. With a keyboard
  attached to an iPhone or iPad, Cmd+V pastes too. Pasting several lines into
  a program that hasn't turned on safe paste first asks "Paste potentially
  unsafe text?", as the desktop does.
  To browse a project's files on the phone, long-press the project and choose
  **Files**, or choose Files from a session's ⋯ menu. It is a read-only browser of
  the whole project: tap folders to open them, search by name across the project,
  or use **Filter by file type** (All files, Markdown, HTML, Excalidraw, Image,
  Video). Recently changed Markdown and HTML files stay at the top. The Files
  menu offers **Refresh** and **Show Hidden Files**, remembered on the phone.
  Markdown and HTML open as pages; other text up to 2 MB opens as numbered
  source. Pictures, binary files and files over 2 MB opened from Files show their
  name, size and why they cannot be previewed, with Copy Full Path and Share….
  Hold a file or folder for **Copy Full Path**; a file also offers **Share…**,
  which fetches a copy from the computer and opens the iPhone's share sheet.
  If Files was opened from an agent session, **Add to Session Context** puts the
  file's text into that session's chat draft, or into its terminal input without
  pressing Enter: it is not sent until you send it yourself.
  **Reload** in an open file picks up an agent's latest edit. HTML pages include
  the Agentation annotation tool (the pen button hides it); its copy button puts
  your notes on the phone's clipboard, ready to paste into a session.
  Tapping a link to a file on the computer in a chat or terminal opens it in
  the same reader: Markdown, HTML, text and code files, and pictures (other
  files show their path, copied, since the phone cannot preview them).
  A picture opens at the screen's width and scrolls down like a page; pinch to
  zoom, and double-tap to switch between that and its actual size.

  On iPhone, the terminal's four-square **Command pad** button opens nine keys:
  arrows around Enter, plus Esc, Ctrl+C, Tab and Upload. Upload asks first for
  **Photo Library** or **Choose File**, as **Send & Attach File** in the
  terminal menu does; the round upload button, when the pad is off, asks the
  same. Hold an arrow to repeat it. Hold the round pad button and drag to move
  the pad; let go to snap it to
  the nearer side edge at that height. The phone remembers its place, separately
  for laptop mode. The buttons around the keys open **Keys**, **Agent hotkeys**,
  the **Text editor**, or show and hide the keyboard. Keys and Agent hotkeys take
  the keyboard's place; tap their button again to go back. Agent hotkeys is
  dimmed when the session's agent has none; a tap on it then shows "This
  session's agent has no agent hotkeys" beside the pad button for a few seconds
  and opens nothing. If the pad and
  keyboard cannot fit together, opening the pad lowers the keyboard and closing
  it brings the keyboard back; bringing the keyboard up closes the pad instead.
  A layer that leaves no room for the pad has a **Close** button to return to it.
  In **Settings > Keyboard**, Command pad turns the pad button on or off,
  **Pad keys** changes the nine keys, **Extra keys layout** changes the Keys
  layer's two rows and custom actions, and **Agent hotkeys** edits the shortcuts
  for Codex, Claude Code and Pi. **Key vibration** (on by default) turns off the
  short vibration of the pad and layer keys, also when you pick up or drop the
  pad. Tap a modifier for the next key, or hold it to lock it until tapped again;
  closing the pad clears armed modifiers.

  The pad's pen opens a multiline **Text editor** above the keyboard. Return
  adds a new line; the up-arrow puts the draft into the terminal and clears the
  editor, without pressing Enter. With the editor empty, its send button sends
  Enter. Where the open pad has no room beside the editor (a phone held
  sideways), the pad closes and the editor has its own **Close** button, which
  works like the pen. Each session keeps its draft however the editor closes,
  also when the keyboard goes away or you switch sessions, until it is sent;
  drafts are not kept when the app restarts.

  On the iPhone Duo, half folding it like a laptop keeps terminal output above
  the fold, whether or not the keyboard is up. The keyboard, or the Keys or Agent
  layer in its place, sits below; the text editor sits above the fold. The pad
  uses its own saved position in the upper half, reaching below the fold only
  when the upper half cannot fit the open pad. In a chat the transcript and the
  chat box end above the fold as well, the chat box lifting above it while the
  keyboard is up and the lower half staying empty while it is down. The
  Sessions list, Files, the Link to pickers, and in Settings the Licences and
  terminal palette lists run on below the fold: a row that scrolls out of the
  upper half at the fold appears at once below it, and no row sits on the fold.
  The other Settings pages, the Unified list, Notifications and Usage end above
  the fold and leave the lower half empty, as every list does while the
  keyboard is up or its rows fit above the fold; sheets and dialogs keep their
  controls clear of it.

  On the iPhone Duo's inner display, tap **Panes** in a session's header (or in
  its More options) to open the panes stored for how the phone is held
  (upright, across or half folded like a laptop): the session on screen takes
  the first session pane, and Files, Usage and the other pane types come back
  with the divider. A pane that showed another session does not bring it back
  but opens **New pane**, as Panes does when none are stored; panes then open
  once you pick there. Pick another
  session on the same computer (those of the project beside it come first),
  **New terminal in** the project, or **Web
  Preview**, **Files**, **Git Changes**, **Agents**, **Usage** or **Attention**.
  Tap a pane's title to change its type; the last session pane cannot change to
  another type. In a session pane beside another session pane, **Session** in
  that menu opens New pane there: a pick replaces its session, and closing it
  keeps the session. Tap a pane to make it active, drag its title onto the other pane
  to swap them, or drag the divider with the keyboard down to resize them.
  Upright they stack; across and in laptop mode they stand side by side, with
  layouts remembered for each arrangement. A session can show Chat in its pane,
  but only one pane shows Chat at a time; a chat in a pane has a compact chat
  box: the field, then one row with the model, **More actions**, Dictate and
  Send. Session note, Attach and Terminal View are under More actions; the
  permission mode, Copy Report, the pill strip and the status line show only in
  the full chat view. While a chat pane is active and the keyboard is up, the
  terminal's keyboard button stands at its usual place and hides the keyboard.
  Closing a pane, or tapping Panes while two are open, goes
  back to one pane, not closing its session; **New pane** itself closes alone,
  by its close button or Panes, and keeps the panes you had. This holds in every arrangement:
  none opens panes by itself until you tap Panes again, and each keeps its own
  panes for then.
  Files and Git Changes keep the project they were opened for, and Usage keeps
  its computer; Web Preview and Agents follow the selected session's computer.
  Files opens a picked file full screen; Git Changes stays read-only. Agents
  lists sessions and their status; Usage shows the saved, enabled Claude and
  Codex accounts' meters, or says when usage is unavailable rather than showing
  zero. Attention is the same Notifications list: tapping a session row opens it
  beside the list and marks its notifications read on its computer, except its
  failure rows; an **Ended** row opens its sheet with **Remove** and, when
  available, **Restore** instead. A question opens its chat at the question full
  screen, and Back returns to the panes. Folding the phone opens the active pane's
  content full screen; for Agents that is the Sessions list.

  On the same inner display, **Show as tiles** in the Sessions header switches
  from the list to the **session wall** (the side bar offers Show as list and
  Show as tiles when the header is there). Each tile has the session's name,
  agent, status in words and a small preview of its latest terminal lines, or
  **No preview** if they cannot be read. Tap to open the session as from its
  list row, or hold for the same menu. The choice is remembered, but the outer
  display and other phones use the list.

  On the iPhone Duo's outer display, **Waiting for you** above the Sessions list
  brings waiting sessions to the top, longest wait first, within the list's
  filter and search. Tap a card's heading to open its session, or answer an
  approval or a single-choice question on the card. **Open Question** takes
  questions needing several choices, several answers or written text into the
  session; an approval without an answer card says to answer in the session.
  Other waiting cards show the last screen lines and **Short replies**: a tap
  sends the reply to that session immediately. Edit their text and order in
  **Settings > Short Replies**. When nothing waits, the list says so; turning the
  folded phone across with its camera at the bottom hides this section.

  The project's **Actions…** menu on the phone lists **Project Actions**, then
  that computer's **Global Actions**, when available; a project with only
  Global Actions still offers the menu. In the Unified list, hold a session and
  choose Actions… for its computer and project instead. A terminal action
  starts there and opens its session; a browser action opens a localhost address
  in Web Preview and another address in the phone's browser. If the computer
  refuses a start, the list shows the action's name and the computer's reason.

  A bottom sheet in chat or the connection setup can be closed by dragging its
  grabber down and letting go far enough down, or flicking it down; a shorter
  drag returns it to its place. Drag the grabber, not the list inside it, to
  close it.

  The phone's **Settings > Licences** lists the open-source software the iPhone
  app includes, with each component's version and licence. Search by name and
  tap a component to read its licence texts in full. The list is bundled with
  the app, so it needs no connection to your computer; a read failure says why
  and offers **Retry**.
- **From another computer**: Settings > Remote > Computers this machine connects to > Add a
  machine with SSH details or an Easy Connect code, then Install / Connect
  gxserver on it. When the machine has Ghostex installed too (a package, the
  AppImage or the Mac app), connecting starts whichever Ghostex there is
  newest, so updating Ghostex on that machine is enough; an older one still
  running is replaced without closing its sessions. What you type in that dialog (except the password) stays
  there when Settings closes or the app restarts, until you add the machine or
  press Cancel. The machine appears as a sidebar section with its own
  projects and sessions; its terminals stream into the desktop app. Windows,
  Linux, and macOS clients use the connected computer's folders and shell.
  Agents can use a connected machine too: any agent on this computer can
  start, message and read sessions there, and the machine's agents can do the
  same on this computer, while the connection is up (`ghostex computers` lists
  them; agents add `--computer <name>` to `ghostex sessions`, `ghostex agents
  create` and `ghostex coordinator start-thread`). They can start, message,
  read, sleep, wake and close sessions there and install its session resume
  hooks (`ghostex agent-hooks install --all --computer <name>`), nothing else,
  and both computers need Ghostex 10.44 or later. An agent started there from
  another computer gets its missing resume hook installed first, so a
  coordinator's threads on a computer that runs only gxserver report back like
  threads here. Connecting is the permission: an agent that can
  type into a session on the other computer can run commands there, as you
  could. Turn **Agent access** off, under Advanced in the machine's Edit dialog, to stop agents
  on either computer from using the other.
  Files and folders you drop onto a remote terminal or add with its attach
  (paperclip) button are uploaded to that computer first, so the terminal gets
  references the agent there can open.
  Open Code from the view panel's + menu to edit the remote project; if prompted,
  install the editor component first. A remote Mac or Windows computer runs the
  editor its own Ghostex has, so if Code says that computer doesn't have it yet,
  open the VS Code view on that computer once (or update Ghostex there) and retry.
  Folder links in remote chats also browse the remote folder in VS Code.
  A file link in a remote chat opens in Files for any file in the project, not
  only the Docs folders. A link to a text file or a picture outside the project
  (an agent's log, a draft or a diagram in another folder, say) opens in Files
  too, read-only, when that session's own chat links to it: text up to 2 MB,
  pictures up to 12 MB, on Mac, Linux and Windows computers alike. Another
  binary or larger file says so; open it in the VS Code view instead. A link to
  a file on a network share (`\\server\share\…`) doesn't open in Files. When a
  link can't open, Ghostex says why: the file is gone, the computer is offline,
  or it runs an older Ghostex (update Ghostex there). On the phone a chat's
  file link already opens text files and pictures anywhere on the computer in
  the file viewer.
  A remote project's Open Folder (its right-click menu, Quick Access) opens the
  project in the Files view. Open In with VS Code, VS Code Insiders, VSCodium,
  Cursor, Windsurf, Trae or Antigravity opens the remote folder over Remote-SSH,
  and Zed opens it over SSH (not on a native Windows machine); the editor's
  command line tool must be installed. Other apps can only open folders on this
  computer, so they show a warning with Copy Path.
  Remote localhost links open in the built-in Browser through that computer,
  even when ordinary web links are set to open in your external browser.
  When a machine cannot connect, its tab's cloud turns red and its list says
  why in one line (hover it for the rest), with Reconnect and Details buttons.
  Details opens the machine in Settings > Remote, where its tile shows a short
  status with a red dot (Setup failed, Can't reach, Sign-in refused) and its
  sheet starts with the whole reason and Reconnect, Reinstall on <machine> (for
  a Ghostex server Ghostex installed there; it replaces it even at the same
  version) and Copy details, for a bug report. Ghostex keeps retrying a
  dropped connection, but a rejected username or password is not retried (so
  the account is not locked out); saving a new SSH password reconnects it.
  A machine you reach through Easy Connect from before 11.1 that the other
  computer turns away shows Pair this computer again and is not retried: on
  the other computer open Settings > Remote and copy its Easy Connect code,
  then paste it into this machine's Easy Connect code (Settings > Remote,
  Edit) and Save, or use Pair in the pair-again notice.
  A computer that asks for a verification code (2FA) on every sign-in can't be
  signed in to by Ghostex itself. On macOS or Linux, open Advanced in the
  machine's Edit dialog and turn on **Reuse an existing SSH connection**, give
  that host `ControlMaster auto` and a `ControlPath` in a folder only you can
  change (for example `~/.ssh/sockets`, `chmod 700`) in your `~/.ssh/config`,
  and sign in once in a terminal with `ssh <user>@<host>`. Ghostex then runs its
  commands and its tunnel over that signed-in connection and never tries a
  login of its own. After the connection closes, sign in in a terminal again;
  the machine says "No SSH connection to <user>@<host> is open" until you do.
  A chat whose computer is off or asleep says so in a card above the chat box,
  for example "Windows PC is offline · last seen 2h ago", with Retry; its
  sessions' computer badge in the sidebar turns red. A message you send then
  waits on this computer and goes out by itself when that computer is back,
  even if Ghostex was closed in between; it shows "Will send when Windows PC is
  back" with Cancel, which puts it back in the chat box. After a day it comes
  back to the chat box with a note. On the phone the message waits on another
  of your computers that is connected to the one that is off. If a message gets
  no answer for 10 seconds, Ghostex checks the computer at once and the card
  says it "isn't answering · checking…".
  To update Ghostex on a remote machine without going to its screen, look at
  its tile in Settings > Remote or its tab in the sidebar: a connected machine
  shows the Ghostex version it runs, and when a newer one is out its tab shows
  an arrow and the tile and the tab's right-click menu offer **Update to
  <version>**. It downloads the update with that computer's own updater and
  restarts Ghostex there; its sessions and agents keep running, the progress
  shows on the tile, and the connection comes back by itself. Open the
  machine's tile and turn on **Keep this machine up to date** to have it
  install updates on its own while none of its agents is working. Ghostex must
  be open on that computer (macOS or Windows, also with WSL); a Linux machine
  shows the new version, which you install with its package manager. From a
  terminal: `ghostex app-update status`, `check`, `install --wait` and
  `auto on|off`, with `--server <profile>` for another computer. On the
  phone, a computer's card in the Machines list shows an Update button when a
  newer version is out; swipe left on it and tap Edit to see the version it
  runs, **Update to <version>** with its progress, and the same **Keep this
  machine up to date** switch. A computer whose Ghostex is too old for this
  says so: update it once on that computer, and from then on it can be
  updated from another device.
  To install or update agent CLIs, agent skills and tools on a remote machine,
  open Settings > Agents or Settings > Integrations and pick the machine in the
  **Machine** dropdown at the top (shown once a remote machine is saved). The
  list then shows that machine's agent CLIs (installed or not, version, update
  available) or its Ghostex CLI, agent skills and Tools, and Install, Update and
  Uninstall run there through its own Ghostex. The dropdown lists this computer
  and the machines connected right now ("Only connected machines shown" at the bottom of
  its list), and a picked machine that disconnects goes back to this computer;
  one whose Ghostex is too old says to update it first. On a remote Mac, Integrations also installs and updates Fast Computer &
  Browser Use and SpaceO, each in its own section with its skills (never on
  Linux or Windows). Their
  Accessibility and Screen Recording permissions can only be allowed by someone
  at that Mac, so until then the row says so. The Session resume hooks card
  below the list shows that machine's hooks, with Install all and Uninstall all
  running there, so a computer that runs only gxserver gets its hooks from
  here. Turning agents on and their defaults are set on each computer itself. On the phone, a computer's
  page (tap its card in Machines, then Agents, skills and tools) shows the same
  Agent CLIs and Skills and tools with Install and Update. When that machine
  (or this computer) runs Windows, both pages recommend Developer Mode (Windows
  Settings > System > For developers > Developer Mode): without it Windows
  blocks symbolic links, so Ghostex links hooks and skills with junctions and
  file copies instead. Click the notice's X to fold it to a warning icon.
- **Settings sync**: Settings > Sync keeps your computers' settings the same
  whenever two of them are connected to each other: a Remote machine, Easy
  Connect or Tailscale. Nothing goes through a server, and each computer works
  on its own while apart. Sidebar organization syncs by default: your Spaces
  (names, icons, colours, their order, which projects are in each and in which
  order), project groups, Project or Unified sidebar, Separate by machine,
  Merge copies of the same repository, Worktree sessions show in their
  project, the Spaces switch, Show Main Hub on all Spaces, Hide projects
  without sessions, Group working sessions, session parking, Pinned order, the
  Unified sidebar's Moving section, Group by project and project order, and
  your Session Tags (the tags you made and the tag list's order). A change
  shows on your other computers within a minute, without a restart. A project
  one computer doesn't have keeps its place and shows there once you add it.
  The sidebar's width, animations, tooltips, card details and hover buttons,
  and what each window has folded, hidden or filtered stay on each computer.
  The phone follows the same Sidebar organization (while its Settings > Sync >
  Carry settings between computers is on): its sessions list is organized like
  your computers' sidebar, with one list for every computer unless Separate by
  machine is on, the Project list's Working and Snoozed sections, the Tag As
  menu's tags and the tag filter, and a change in its Settings > Sidebar &
  Sessions or Sort & Filter reaches your computers within seconds.
  The first sync between two computers makes Spaces with the same name one
  Space, and a deleted Space or a group you Ungroup goes on every computer.
  Turn Sidebar organization off and each computer keeps what it has. Appearance
  (theme, colours, fonts, the chat's look and the glass; the glass picture or
  video, the interface scale and zoom stay on each computer), Agent defaults
  (the default agent, how new sessions open, the title agent, Accept all, each
  agent's default model and effort), Sounds and notifications, Keybindings
  (between computers of the same system), Coordinator (coordinator settings
  and your playbook overrides), Agents, Skills
  and Accounts are off by default. Agents keeps which agents are on in
  Settings > Agents, their names, commands and flags, and their order; an
  agent a computer doesn't have shows Not installed here with Install CLI, and
  Install all missing installs them all (nothing installs until you click).
  Skills keeps your own skills in `~/.agents/skills` and the Ghostex skills you
  installed; a skill over 512 KB stays on its computer, and a skill removed on
  one computer is put aside in Ghostex's backups folder on the others.
  Accounts keeps the Claude and Codex account list (names, numbers, Use this
  account, the sidebar pins) but never the logins: Claude and Codex sign one
  of two computers out when they share a login, so an account that arrives
  shows Sign in on this computer and Ghostex never uses it there until you do.
  Each switch is this computer's own: something syncs between two
  computers when it is on in both. Turning one on never overrides what another
  computer already synced; this computer's values only fill in what nobody
  synced yet. The Ghostex app saves synced settings, so they reach a computer
  while its app is open. When two computers changed the same thing apart, the
  later change wins on both, and the computer whose change lost shows a
  "Changed on …" notice in the middle of the window with Undo (the Sync page lists recent ones too). Your
  computers lists each one with its last sync: Remove stops syncing with it,
  Add back lets it sync again. The Ghostex phone app keeps a copy too and
  passes changes on to each computer it connects to, so two computers that
  never connect to each other still match (phone Settings > Sync > Carry
  settings between computers, on by default); a computer's Settings > Sync
  lists the phone, and Remove stops it there. Sign-ins and secrets never sync.
  Settings:
  `syncSpaces` (Sidebar organization, on by default), `syncAppearance`, `syncAgentDefaults`,
  `syncSoundsAndNotifications`, `syncKeybindings`, `syncCoordinator`,
  `syncAgents`, `syncSkills`, `syncAccounts`.
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
Windows environment selected in Windows Ghostex: native PowerShell with Windows
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

## Claude cloud sessions (Claude Code on the web, in the sidebar)

A Claude cloud session runs on Claude's own computers (Claude Code on the web)
instead of this computer, so it keeps going when the computer sleeps, and still
has its row in the sidebar. Pick **Claude (cloud)** (the orange cloud) in New
Thread's agent list and choose the Claude account it runs on. It opens in Chat View
(its terminal runs behind it): type the task in the chat box and send it, and the
session starts on claude.ai from the project's GitHub repository on a new
`claude/…` branch. On the computer the chat pane then shows the session's own
claude.ai page, signed in as the session's account, where you read and answer
Claude and type follow-ups in claude.ai's own message box; the web page shows its
terminal instead. On the phone, tapping the row opens the session's claude.ai
page, and **Open Terminal** in its long-press menu shows the conversation in its
terminal. Until its task is sent you can move the session to another saved
Claude account (the header's **Cloud** menu > **Account**, or the row's **Switch
Account**); once it starts on claude.ai the account is final for that session:
its usage, follow-ups and Bring to this computer all stay on it. New Thread
highlights the account of the project's last cloud session first. The row always sits under the
project of the repository's main folder, even when started from a worktree.

- **The Cloud button**: while the work area shows a cloud session, the header
  has a **Cloud** button whose menu holds only cloud actions: **Show Terminal**
  (and back to the chat or claude.ai page), **Open on claude.ai**, **Copy
  claude.ai Link**, **Account** (which saved Claude account it runs on, pickable
  until it starts), **Start a Copy on Another Account…** while its account is out
  of usage, and **Bring to This Computer**.
- **The row**: it starts with the orange cloud of the Claude (cloud) agent; hover
  its title for the account, branch and latest status. Clicking it opens the session, whose chat pane is its
  claude.ai page; the row's menu has **Open on claude.ai** (the page in Ghostex's
  Browser) and **Copy > Copy claude.ai Link**. The row shows working (orange), needs you (blue) and Done
  like any session, read from claude.ai every few seconds whether or not the page
  is open; when Claude needs you, its status and notification say what it asks
  for ("Needs you: paste the ticket's title and description"). What reaches
  GitHub ("Pushed 3 commits to claude/…", "Pull request #12 opened") shows there
  too, with a blue dot and a notification.
- **Its terminal**: shows the conversation (your messages and Claude's replies,
  including a question it asks with options) and what Claude needs (open it with
  **Cloud** > **Show Terminal**). Type a reply
  or follow-up and press Enter to send it. A question with options or a
  permission request can only be answered on the claude.ai page; the terminal
  says so. A message sent to the session from its chat or with `ghostex agents
  send` goes to claude.ai the same way, and one sent before the session has its
  task starts it with that task.
- **Close keeps it, asleep**: Close on a cloud row that is running on claude.ai
  (from the row, its tab, a project's Close Inactive, or the phone) puts it to
  sleep instead, because the session goes on running there; the row stays with
  its cloud icon and still opens the page. Close it again while it is asleep to
  take it off the list. `ghostex kill` (and the phone's Kill Session) still
  closes it straight away.
- **Signing in**: each Claude account has its own claude.ai sign-in in Ghostex,
  kept in its own Browser profile and used for its cloud pages and the Claude
  view (Google sign-in works there too). The first time a row needs it, the page says "Sign in
  to claude.ai as <email> to see this session"; or sign in ahead of time from
  Settings > Accounts > the account > claude.ai in Ghostex: Settings steps aside
  while claude.ai's login is open, a **Back to Settings** button at the bottom
  right brings it back on that account, and a spinner beside the button waits
  until the account shows as signed in. If a page is signed in as someone else,
  it says so with a **Sign in as <account>** button; nothing is signed out until
  you click it.
- **Out of usage**: when the session's account hits a limit, the row and page say
  so with the reset time ("Work hit its 5h limit · resumes 4:10 pm") and offer
  **Start a copy on another account**: a new cloud session carries on from the
  same branch with a summary of the task, and the original stays where it is.
- **Right-click a cloud row**: **Open on claude.ai**, **Bring to This Computer**
  (continues it here in a new worktree with `claude --teleport`; the new local row
  takes over and the cloud row parks) and, while its account is out of usage,
  **Start a Copy on Another Account…**.
- **Needs**: Claude Code signed in with claude.ai (Pro, Max, Team or
  Enterprise; not an API key), the repository on GitHub with Claude's GitHub app,
  and a cloud environment set up on claude.ai for that account. Cloud sessions
  count against your Claude plan's limits. The first time a page needs it, sign
  in to claude.ai as the session's account right in the pane (or ahead of time in
  Settings > Accounts); Ghostex keeps that sign-in for the account.

CLI: `ghostex create-agent claude-cloud --project-id <id> --prompt "<task>"`,
`ghostex claude-cloud status <session>`, `ghostex claude-cloud send <session>
<message>`, `ghostex claude-cloud bring-back <session>`.

## Cloud Boxes (sessions on a machine in the cloud)

A Cloud Box is a Linux machine on your own Instabox (instabox.sh) or Boat
(boat.dev) account that runs a Ghostex session: the agent works there, not on
this computer, and keeps working when this computer sleeps. Ghostex never
bills; your provider account does. Cloud Boxes is a built-in extension, on by
default on every computer (Settings > Extensions, `cloudBoxesEnabled`); turning
it off hides it everywhere and keeps your boxes and settings. The Cloud Box
choice in a new chat and in New Thread shows once you've connected a provider
(or made a box), so it stays out of the way until you use Cloud Boxes.

- **Connect a provider**: Settings > Cloud Boxes > Providers. Instabox's
  Connect signs in in your browser. Boat's Connect asks for your Boat email,
  opens Boat's page and shows a 6-digit code to type there. Paste Key takes an
  API key instead. Free plans and trials work; Settings shows their limits
  (Instabox free: one box at a time, and a paused or stopped one counts, so
  delete it in Your boxes to start another; a box that failed before Instabox
  made it doesn't count. Boat trial: boxes stop after 2 hours). When Instabox
  says its limit is reached, the message names the sandbox holding the slot:
  delete a Ghostex box in Your boxes, or one made outside Ghostex at
  dashboard.instabox.sh or with `instabox rm <name>`.
- **Start one**: in a new chat pick Cloud Box in the bar's where part (or the
  Cloud Box chip on a project's row in New Thread's project list); the choice is
  remembered per project. The bar's last part then picks what the box starts from:
  a snapshot (★ is the project's Default), a Blank box, or a running box to
  reuse (the chat gets its own worktree there), plus the provider and size
  with their price per hour. Nothing starts or bills until you send the first
  message; it waits with a progress card while the box starts, fetches the
  code and starts the agent, then the chat opens on the box. The box chip in
  the session's header shows its state, running time and about-$ cost; its
  menu has Open terminal, Save as snapshot, Update snapshot, Pull branch, Open
  PR, Stop and Delete. Cloud sessions sit in their project in the sidebar with
  a cloud badge whose dot shows the box's state (blue starting, green running,
  amber idle, grey paused or stopped, red failed); the sidebar menu's Sort &
  Filter > Cloud Box Sessions shows only them. Each box session works on its own new branch, cut from the
  latest base branch and named from the first message; pictures attached to
  that message go to the box with it. The agent pushes the branch (a GitHub
  login saved in the project's snapshot lets it), and Pull branch brings it to
  this computer as a worktree of the project (an existing worktree of that
  branch is brought up to date).
- **Set up cloud box**: Settings > Projects > Cloud box opens a new chat on a
  blank box with a setup prompt (install the project, run its tests, write
  `.ghostex/cloud-setup.sh`, sign in to GitHub with you). Check it, press
  Enter, and when it's done use Save as snapshot on the box chip so new boxes
  start ready.
- **Snapshots**: a project's snapshot holds its tools and caches, so a box
  starts ready; the code is always fetched fresh. Settings > Projects > Cloud
  box lists them: the Default (★) is what new boxes start from, Update saves a
  running box of the project as the new Default (the old one stays as Previous
  for 7 days), Make Default and Delete. The same section picks the provider
  and size, and **Send to the box**: tick the project's `.env` files and name
  variables; their values are read from this computer whenever a box starts
  and never saved in a snapshot.
- **Agent logins for boxes**: Settings > Cloud Boxes. Allow on Cloud Boxes for
  a Claude account approves a one-year box token once in your browser, so
  Claude on this computer stays signed in (claude.ai connectors and Remote
  Control don't work on a box). A Codex account is allowed as it is: each box
  gets a fresh copy of its login and sends refreshed sign-ins back. Logins and
  the secrets you send wait on the box in a private folder that only the box's
  user can read and that no snapshot saves; this works on Instabox too, whose
  boxes run Ghostex as an ordinary user rather than root.
- **Cost and stopping**: Settings > Cloud Boxes shows this month's spend per
  provider and in total. The Monthly cap warns at 80% and starts no new box at
  100%; running boxes keep running. A box with no agent working and no
  terminal open pauses (Instabox) or stops (Boat) after 15 minutes; opening its
  session shows the last transcript without waking it, and sending a message
  wakes it. When every session on a box is archived it stops, and is deleted
  after 7 days unless it has unpushed work. Your boxes lists every box with
  Stop and Delete (a box with unpushed work asks before Delete Anyway).
- **From your phone**: cloud sessions sit in their project on the phone too,
  with the same cloud badge, and open in chat without waking the box; sending
  a message wakes it. The box pill under a cloud chat's header shows its
  running time and about-$ cost; long-press it (or the session's row) for
  Stop, Pull branch, Open PR and Delete. A project's agent menu starts with
  Where: This computer or Cloud Box, remembered per project like the new-chat
  bar. Start boxes from your phone keeps what a phone needs on your paired
  phones, so while this computer is off the agent menu offers Start Claude (or
  Codex) on a Cloud Box…, which starts a box from the project's Default
  snapshot (not once this month's spend the computer last saw reached the
  monthly cap), and the phone can chat with, wake, stop and delete boxes.
  Connecting providers, the first snapshot and picking secrets stay on the
  computer.
- **More providers (agentbox)**: under Advanced on Settings > Cloud Boxes,
  boxes run through agentbox, a free open-source command line tool
  (https://github.com/madarco/agentbox), on this computer with Docker, on
  Hetzner, Vercel, Daytona, E2B or DigitalOcean, or on your own server over
  SSH (macOS and Linux; on Windows, use Ghostex inside WSL). "Set It Up for Me"
  starts an agent that installs agentbox and sets up the providers you want,
  asking before anything that costs money; or Install agentbox, then Set Up for
  Docker, or Log In and Prepare for a cloud; Add Server registers your own
  server. Pick a box under More providers in a new chat's Cloud Box pick (the bar's last part),
  on a project's row in New Thread's project list, or Run in a Box in a project's Select Agent menu; right-click a box session for Open Box Web App,
  Open Box Screen, Stop Box and Destroy Box. Cloud agentbox boxes bill while
  they exist.

Related settings: `cloudBoxesEnabled` (on by default),
`cloudBoxesMonthlyCapUsd` (0 = no cap), `cloudBoxesIdleStopMinutes` (default
15), `cloudBoxesDeleteStoppedAfterDays` (default 7),
`cloudBoxesPhoneStartEnabled` (default off), `agentboxDefaultLocation` (where
agentbox New Thread runs unless you pick another place; default `local`). CLI:
`ghostex cloud providers`, `ghostex cloud list`, `ghostex cloud spend`,
`ghostex cloud attach <box-id> <project-id> <session-id>` (a box session's
terminal in any terminal; the ids are the box's own, from `ghostex cloud show`),
`ghostex agentbox status`, `ghostex agentbox list`,
`ghostex create-agent <agent> --project-id <id> --run-on docker`.

## Ghostex Collab (share a chat through a link)

Ghostex Collab shares one chat with someone through a link that opens it in their browser: no app, no account. Collab is an extension: the Ghostex you download doesn't contain it until you install it. Install it in Settings > Extensions (Built-in, Sharing, Collab, Install); installing downloads the Collab component. Until then, Share in a chat's header (and the phone's Share pill) opens a popover that says Collab isn't installed, with Install right there: it shows the download's progress and then Ready to share. Don't show Share in that popover hides Share until you install Collab (Settings > Extensions, Collab, Show Share in chats brings it back). Settings > Collab appears once it is installed. Uninstall, in Settings > Extensions or Settings > Collab, never asks: it ends every share and removes Collab from the computer. When an app update brings a newer Collab, Settings > Updates and the Updates menu offer it. Then press Share in a chat's header (on the phone, the Share pill above the chat box) and Create link: the popover's band turns to Live and shows the link (`https://share.ghostex.dev/s/…/#…`) and an eight-digit code to give with it, each with Copy. Before Create link, the Link lasts chip picks how long this link lives (and becomes the default for new links). The person who opens the link types their name and the code, and then follows the chat live (the agent's replies, tool rows and cards as you see them) and can write in it and paste pictures; everyone in the chat sees who is watching and who is typing. You get a notification when someone joins, and the popover lists who is watching, with Remove.

Your own Create link, Show (to see a link and code again later) and turning on Send for a chat just happen. When an agent asks for one of these, an Approve card shows first, on your computer and your phone, and nothing happens until you click Approve (with Settings > Security's switch on, Touch ID, Windows Hello or your password first). Each guest message in Ask me waits for your Send. Turning Collab off, Stop sharing, Remove and Decline never ask.

What their message does is up to you, per chat, under Guest messages in the Share popover: Ask me (the default) holds it as a "Message from <name>" card above your chat box, with its pictures, and Decline, Edit (puts it in your chat box to change and send yourself) and Send; Send, turned on for one chat, passes their messages straight to the agent; View only turns their chat box off. A guest's message always reaches the agent labelled with their name. A guest can answer the agent's questions (its forms, quick picks and Ghostex pages); their answers go to the chat the same way their messages do, so under Ask me they wait for you too. A guest can never answer permission prompts, stop the agent, change its model, or see any other chat, file, terminal or setting: Ghostex only lets their link reach that one chat, and only the pictures, pages and HTML files that chat shows or that they sent themselves; any other file stays on your computer.

A link ends after an hour by default (Settings > Collab, or the Link lasts chip in the Share popover, sets 1 hour, 4 hours, 24 hours or Until I stop it for new links). Until I stop it makes a link that never runs out, and the popover says it stays live until you stop it. Stop sharing ends a link at once (it asks first and names who gets disconnected), as does deleting the chat; sharing again makes a new link. When a guest's message waits for you, the popover says so and Review next to their name takes you to its card above the chat box. A guest always needs the code. Remove next to a guest's name closes their page and keeps that browser out of the chat. Settings > Collab also sets Your name (what guests see for you), what new shares start with (Ask me or View only, and how long the link lasts), lists every chat shared right now with Stop sharing and Stop all, and keeps the Guest messages log: every message guests sent and what you did with it, and who joined, left or was removed. Uninstalling Collab ends every share. The phone has the same Share sheet, cards and Settings › Collab for each computer. The chat runs on your computer, so a guest can only follow it while your computer is on and Ghostex is running.

The link's traffic goes through Ghostex's share relay, locked end to end with the key after the `#` in the link, so the relay can't read the chat. The relay also serves the page your guest opens, and that page reads the key, so each Ghostex version trusts one exact page: before sharing, Ghostex downloads the page from the relay and checks every file against the version it was built with, and again every hour; if the relay serves anything else, sharing pauses and Settings > Collab says why. The page also checks its own code before it runs. What this can't prove is the page one guest's browser got: a relay that served the right page to your computer and a changed one to a guest would not be caught. The relay's address can't be changed from Ghostex's settings or API.

## Security log (Settings > Security)

Ghostex keeps a security log of every sensitive action on your computer: pairing a phone (and failed pairing attempts), showing a pairing code, Easy Connect changes (on or off, its allow-list, ports and relay), sharing a chat in Collab or changing how a share works, letting a connected computer's agents in, Autopilot answering an approval or being allowed to do more, adding a GBot, turning a GBot's Bot API on or off or giving it a new key, and a playbook, skill or agent command arriving by settings sync. Each entry says what happened, who asked (this computer's app or an agent, naming the session it says it runs in, a paired phone, another computer, Autopilot), when, and how it ended. An approval request also gets a line for how it ended: approved (and whether in the Ghostex app, on a paired phone or with the CLI), expired, replaced by a newer request, or withdrawn because the session that asked was closed; these lines stay in the log and don't ring the bell. Each one also shows in the notification bell and as a notification on your paired phones, so you see it when it happens; clicking it opens Settings > Security.

Settings > Security has three parts. Approvals: "Require Touch ID or your password to approve" (see Approvals below). What still asks: the short list of what asks for an Approve on this computer, with the line that everything else works without asking and your own clicks never ask. Log: "Show the security log" (it was under Settings > Extensions > Experimental as Security logs) with Open log beside it; while it is on, the log shows under it, newest first, and the log is kept and its alerts arrive either way. Every entry is chained to the one before it, so if something edits or removes an entry after Ghostex wrote it, the page says "The security log was changed" and where. `ghostex security log` shows the same list in a terminal (`--last <n>`, `--json`) and exits with code 2 when the log was changed.

Settings that are account or pairing state, or hold a key or token (the Remote and Accounts pages), aren't written through `ghostex settings` or `ghostex actions`: change them in Settings. Easy Connect serves only SSH and Ghostex itself, never other ports. Private file links never open keys (such as `~/.ssh`), Ghostex's own data, or a project's hidden files like `.env`.

After updating to 11.1, Ghostex shows a one-time notice: Ghostex went through a big security audit and every phone and computer now gets its own key, so each one pairs once. The notice lists your devices, one row each, with a status (Needs pairing, Waiting for you, Paired, Nothing to do); click one to pair it. A phone on Easy Connect: its page shows the Easy Connect code (scan it, or Copy Easy Connect code and paste it on the phone), then the phone's three words to approve. A phone that signs in over Tailscale, your network or SSH: open Ghostex on it, then approve its words. A computer on Easy Connect: on that computer open Settings > Remote > Easy Connect > Show QR > Copy Easy Connect code, paste it on the computer's page here, and approve it there when its words match. Computers you reach over SSH need nothing. Remind me later brings the notice back tomorrow (twice at most); Settings > Remote keeps a Pair again line, counting only devices that still need pairing again, until every one is done. An old pairing that needs nothing (never used, an older pairing of the same phone, or a phone that now signs in over Tailscale or SSH) shows Not needed and isn't counted; Forget on its row (then Forget again to confirm) takes it off the notice without unpairing anything. Phones you already removed don't show at all.

## Approvals (only at the door)

Ghostex asks only when something could let someone new in, and then with one Approve / Deny card: a new phone or computer asking to pair (compare the three words it shows), each message a Collab guest sends in Ask me mode, and, when an agent asks, turning on Easy Connect (once), letting a device into it, making a Collab share or showing its link again, and turning on Send for a share. Your own clicks in Ghostex never ask. Everything else just works, for you and your agents: connecting your computers, every setting, GBots, Autopilot, Cloud Boxes, teams, extensions, secret providers and starting an agent with full access. Agents can't approve a card or answer a secret request. The card opens in the middle of the Ghostex window on your computer, saying who asked and on which computer; Enter never approves, and Escape, Close or the corner X put it aside behind an "Approvals waiting" pill in the work area header. A card from a computer you added under Settings > Remote can be approved right where you are. On the phone, open the computer in Machines, then Approvals. Approve is one click; to require Touch ID or your Mac password (Windows Hello or your Windows password, your password on Linux, Face ID or your fingerprint on the phone) first, turn on Settings > Security > "Require Touch ID or your password to approve" (on the phone: "Require Face ID to approve" on the computer's page under Machines); it applies to every device that approves for that computer. A request waits 10 minutes, also across a restart of Ghostex or the computer's gxserver, and an approval is good for that one change only. When the same phone or agent asks again for the same thing, the newer request replaces the waiting one and its card shows instead (check its words, then approve); a card whose request ran out of time says so ("This request expired. Ask again from the phone.").

Older Ghostex apps and phones keep working with a computer on this version: their Touch ID or Face ID approvals count as a plain Approve. A computer still on an older Ghostex keeps asking for Touch ID or Face ID to approve, as before.

## Secrets (passwords, API keys and 1Password)

When an agent needs a password, an API key or a token from you, it asks in a
private card above the chat box instead of in the chat: the card says what it
needs and why, and has a hidden field with Save privately and Decline. What you
type never shows in the chat, the transcript or the agent's context: Ghostex
writes it to a private file that only that agent's commands can read, and
deletes the file after 24 hours. This works for every agent Ghostex runs
(Claude, Codex, OpenCode and the rest), in the chat on the computer and on the
phone, and covers a sudo password too. The card closes on its own when the
agent stops waiting (10 minutes unless it asked for longer). Agents ask with
`ghostex secret request`; the bundled `$ghostex-secrets` skill teaches them
when and how, so tell an agent "ask me for the token privately" if it asks in
the chat instead.

Settings > Secrets lists the 1Password accounts Ghostex reads keys from, adds
one (a name and the 1Password account, such as my.1password.com) and removes
one. It needs the 1Password CLI (`op`, from 1Password or `brew install
1password-cli`) with "Integrate with 1Password CLI" turned on in 1Password's
Developer settings; each account shows whether `op` is signed in to it.
Ghostex keeps only the account name, never a key. Only you add, change or remove an account, here; agents and the CLI are refused. With an account added you
can paste a 1Password reference (`op://vault/item/field`, from an item field's
Copy Secret Reference) instead of a key: in a secret request card, and in the
voice agent's xAI and OpenAI key fields, where the key is then read from
1Password each time it is used. 1Password may ask you to unlock when a key is
read. "Check a 1Password reference", under the 1Password accounts, tries a reference and says only
whether it worked.

Only you answer a secret request, and only you can have Ghostex read a
1Password reference: from Ghostex on your computer or your paired phone. An
agent, a script using the token file, the browser version (`ghostex web`) and
another computer's agents are refused, for the card, for "Check a 1Password reference"
and for a reference pasted into a voice key.

Settings page: Secrets (`ghostex settings open --tab secrets`). CLI:
`ghostex secret request --label text --reason text`,
`ghostex secret providers list`,
`ghostex secret providers add --name Personal --account my.1password.com`,
`ghostex secret providers remove <id>` and
`ghostex secret providers check --provider <id> --reference op://vault/item/field`
(all three refused from the CLI: do them in Settings > Secrets).

## Notifications and status

Ghostex tells you when an agent needs you: a completion sound when a session
finishes, an attention state on the session card, system notifications
(macOS banners, Windows toasts, and Linux desktop notifications; clicking one on
macOS or Windows opens the session), menu bar badges with running and done counts (click one to jump to the
session), and terminal bell detection. The phone app has no push notifications
yet: it plays the attention sound while it is open, and on Android an ongoing
notification lists your sessions, but an iPhone with the app in the background
or closed gets no alert.
Claude progress updates do not trigger completion notifications while Claude
reports background work still running. Completion notifications arrive when
Claude finishes after that work completes; requests for your input or permission
still get your attention.
Pi, OMP and Amp sessions ring the same way: a finished turn marks the session
done (for Pi and OMP after any automatic retry or queued follow-up), stopping it
with Esc does not, a provider error shows as a Failed notification, and a
question or confirmation a Pi or OMP extension asks marks the session as waiting
for you.
While an agent has questions in its chat that you have not answered (Ghostex
forms and Ghostex pages with questions, from any turn), the session shows the pink question dot
in the sidebar, the phone's session list and `ghostex sessions` ("answer
requested"), also while the agent keeps working. On the computer, a turn that
asks new questions sends one notification, labelled Needs input with how many it
asked. The dot stays while you look at the session and goes away once every
question is answered (from the form, the page, Voice mode, or typed in the chat as
"Q12: yes"), or the agent withdraws or asks them again.
Voice mode can read these questions to you and answer them by their tag ("answer
Q335 with the first option").
A Ghostex form or page you want to answer another time can be put off
with For later on its card: it stops counting as unanswered (no pink dot, no count
on the chat's ? pill, no notification) but stays answerable, and Back to
unanswered on the card undoes it. Answering it ends For later by itself. The
choice is kept by Ghostex, so the computer, the web page and the phone agree.
The ? pill on the chat's working row opens Chat Resources, a list of the chat's
generated content: Ghostex forms, Ghostex pages (with or without questions) and Ghostex visuals,
each row marked by its icon. A small dropdown at the top right, on the header's
line, picks what it shows: All, Pages, Forms or Visuals, with their counts. Questions
sit in folding sections (Unanswered, For later, Answered, Withdrawn), pages that
ask nothing under Pages and charts under Charts, newest first, each with its
title, tags and time; click a row to jump to it.
On the computer the list opens when you rest the pointer on the pill, and a click
on the pill still steps to the previous unanswered question; on the phone a tap
opens it. The pill says just Resources and stays while the chat has anything to
list, with a number only while a question is unanswered (none: just its word).
The count and the list cover the whole conversation, including what the chat hasn't
loaded. A coordinator's list also has Worker pages: the pages its Workers posted,
closed Workers included, each with the Worker's name. They open read-only there.
Each group (Unanswered, Earlier, Worker pages) shows its newest 30 rows at first
(Unanswered always in full) with its own "Show all N" under them, which loads the
rest; a folded group shows none.
Every copy shows a small "Copied!" bubble at the pointer for a moment, whether
it came from a terminal, a chat message, a copy button, or a menu. Copy sound is
off by default. Enable it under Settings > Notifications > Sounds to also hear a
short sound when copying from a terminal, a chat message, the chat composer
(including its right-click Copy menu), a copy button, or a menu (`copySound`).

On the computer, the Notifications bell sits in the sidebar's top row, just before the sidebar
menu button, and shows how many notifications are unread; the badge turns red while a
permission or approval request waits for your answer. Click it to open the Notifications
panel: one row per session, newest first, saying whether the agent finished a
turn, needs your input or failed, with the last thing it said. A Not sent row stays
until you deal with it, so a session can also show a newer row next to it (for
example Not sent and Failed). Click a row to jump to
that session and mark it read; hover a row to dismiss it; the header has Mark all
read and Clear all. Hotkeys: Cmd+I opens the panel,
Cmd+Shift+U jumps to the latest unread notification, and Cmd+Ctrl+U pushes the
current session to the back of the unread queue and jumps to the next one.
Clearing a session's attention by selecting it in the sidebar, focusing its
terminal, or pressing Escape also marks its notification read, including one
you previously moved to the back of the unread queue. A permission or approval
request stays unread and red in the Notifications panel, with a red dot on its
sidebar row, until it is answered in the agent. Opening or focusing it, reading
its screen, or using Mark all read or Clear all does not remove it while it waits.
For other notifications, marking one read or using Mark all read clears the
finished or needs-input mark on its session in the sidebar.
Scripts and agent hooks can post their own rows with
`ghostex notify --title <text> [--body <text>]`.
A banked Claude or Codex usage reset that is about to expire shows as a red
row; it opens that account's usage dropdown instead of a session.
A Failed row (in the same red; an unread one turns the bell's badge red) means
the agent's run ended on an error Ghostex can tell apart from a finished or
stopped turn: Claude Code, OpenClaude, Empryo, Grok or Kimi reporting a failed
turn, a Pi, OMP, Amp, Cursor or Mastra run that ended with an error, a Copilot
error it could not recover from, a program reporting a failure, or automatic
retry ending without success. A connection or server error that Ghostex retries
by itself becomes a Failed row only once the quick retries have failed or
retrying ends, and a usage limit, an agent that just exited, and stopping an
agent with Esc or Stop never do. Codex errors that only show on its screen while
Continue automatically is off, ZCode and agents with no failure signal are not
covered, and nothing is sent while Ghostex waits for the network to come back or
for something on the agent's screen to let a retry through.
The bell also covers your remote computers: its count includes unread
notifications from every connected remote computer, and with a remote computer
in the sidebar the panel has All, This computer, and Remote tabs, each with its
own unread count. A remote row names its computer in a small chip next to the
title; clicking it opens that session on its computer, and reading, dismissing
or clearing it updates that computer's own list. Mark all read and Clear all act
on the open tab. While a remote computer is offline its notifications are hidden and left out of the count, and
the panel says so until it reconnects.

In the phone app, tap the bell in the Sessions header (or its side bar) for
**Notifications**: what needs you now on every enabled computer, one row per
session. **Needs you** lists the sessions waiting for an approval, an answer or
a reply, the longest wait first, and the bell's number counts only these; it
reads like **3?** while it includes the last known state of a computer the
phone cannot reach. **Done** lists finished sessions you have not opened since,
**Failed** the ones that ended with an error or hold a message that was not
sent, and account notices come last. A row names the agent, project and
session, not the agent's message. Tap a row to open its session and mark its
notifications read on its computer, except an error: that stays under Failed
until the computer reports it fixed or you hold the row and choose **Mark as
Done**. A question opens the chat at its question card when that session has
Chat. A Done or Failed row reads **Ended** when its computer confirms the
session has closed. Tap it for **Remove**, which takes it off this phone's list
only, or **Restore** when the computer can restore it; Restore brings it back as
a new session. The ⋯ menu has **Clear Done**, which marks the Done rows read on
their computers. A computer that is offline keeps its last known rows;
one that needs an update says so. iPhone and Android show the same list. On
Android the ongoing notification lists your sessions in the same order (Needs
you, then Done and Failed, then working ones), counts the ones that need you as
the bell does, and tapping a row there opens the session as the list does.

The iPhone's **Settings > Notifications** has a master Notifications switch
and, under **What notifies you**, a choice of **Sound**, **Silent** (shown
without a sound) or **Off** for each kind:

- **Needs you** (Sound by default): a question, an approval or permission, or
  an agent waiting for you.
- **Finished** (Sound by default): a session you talk to finished its turn. A
  coordinator's own turn notifies only when it needs you.
- **Errors** (Sound by default): a session stopped on an error, is stuck, or is
  out of usage on every account.
- **Thread reports** (Off by default): a thread under a coordinator finished a
  turn. Threads report to their coordinator, so turn this on only to hear every
  report.

An agent starting or working never notifies, and the session you are looking at
makes no alert or attention sound in the running app; it still shows in the bell
while it needs you. The notices of one session stack together, and several close
together play one sound (a notice within 30 seconds of the last one comes
without it). For quiet hours, use an iOS Focus (iOS Settings > Focus). **Send a
test notification** shows a notice 5 seconds later, so you can leave Ghostex or
lock the phone to see and hear it. Settings from before 11.4.0 keep a kind you
had switched off as Off and take the defaults for the rest. The first answered
iOS permission prompt turns notifications on if allowed and leaves them off if
denied; after a denial, **Open iOS Settings** leads to the system controls.
**Attention notification sound** and **Alert me for** stay under Sound.
An alert names the session, or with **Show session names** off (Settings > Live
Activity) the agent and project, such as **Codex · My project**, with
**Finished**, **Waiting for you**, **Has a question**, **Ended with an error** or
**Reported to its coordinator** under it, never prompt or reply text. Stopping
an agent yourself does not send an error alert. Alerts and the attention sound
share a limit of one per session every 20 seconds and eight per minute. Without
push, notifications arrive only while iOS lets Ghostex run, not once it pauses
the app; opening the app shows in the bell what needs you, what finished and
what failed, without a burst of old alerts.

A computer that supports push and has accepted this iPhone can also notify
while Ghostex is closed or paused, through the Expo Push Service. iOS decides
when to deliver a push, so it can be late or missing. With Ghostex 11.4.0 on the
computer the same choices apply to pushes: a kind that is Off is not sent, a
Silent one comes without a sound, and Thread reports come only if you chose
them; an older computer sends Finished, Waiting for you and Ended with an error
as before. An unreachable computer keeps your previous choices until it accepts
them on a later connection, for example when you open Ghostex again. Tap an
alert to open its session; a question goes to its question card when Chat is
available. Notifications on Android are not available yet.

The iPhone's **Settings > Live Activity > Show Live Activity** controls one
card on the Lock Screen and the counts in the Dynamic Island. It starts with
Ghostex open while agents work or wait, lists waiting agents first, then working
agents and briefly those that just finished, with their agent, project, state
and time, not conversation text. **Show session names** on the same page is on
by default, so each row leads with its session's name, as the Sessions list
shows it, and the icon tells which agent it is, so several sessions of one
agent can be told apart; turn it off and the rows lead with the agent's name.
Phones that had it off by default before this change start with it on once;
turn it off again and it stays off. Prompts and replies are never shown, but a session's
name can come from your messages, and anyone who sees your Lock Screen can read
it. Tap a row to open its session; a question
opens at its question card. It ends 15 minutes after Ghostex first notices that
no agent is working or waiting, or when you turn it off; while iOS keeps the app
suspended, it ends the next time Ghostex runs. If a computer needs a newer
Ghostex, the settings page names it and no Live Activity starts until it is
updated. Updates arrive while the app is open, and can also arrive when iOS
wakes it for a push; background updates are not guaranteed. **Keep it updated
while locked** (new in 11.4.0, off by default, under Show Live Activity) lets
one of your computers keep the Live Activity up to date through Ghostex's push
service while the phone is locked, for itself and the computers connected to
it; it needs Ghostex 11.4.0 on that computer, and iOS can still delay or limit
those updates. Off, it updates only while Ghostex is open, as before. Older content says
**Updated** and its time in grey, with the rows dimmed and their times stopped,
rather than claiming to be live.

Add the **Ghostex** Home Screen widget with the iPhone's widget picker: small
shows working and waiting counts and **Needs you** when an agent waits; medium
adds up to three agents; large adds up to eight and the selected computer's
Claude and Codex usage bars when available. StandBy uses the small widget.
The widget shows the app's last saved update, with **Updated** and its time,
not a continuous connection; opening Ghostex refreshes it, and a background
wake for a push can refresh it too. It contains agent, project, state and time,
never prompt or reply text; with **Settings > Live Activity > Show session
names** on, it lists each agent under its session's name instead. Tap the widget
to open Ghostex.

Ghostex combines two sources to know whether a session is working, waiting for
you, or done, and there is no setting for it. Its own detection reads each
agent's hooks, terminal title and screen. Some programs also report their own
status with the Program Status Protocol (OSC 7501): Claude Code 2.1.295 and
newer does, recent Codex betas do, and so can your own scripts. While such a
program runs, Ghostex adds its reports to its detection: failures, what it is
waiting for, its message and progress, and waiting or finished as soon as the
program says so; when the two disagree the more urgent one wins, and running
subagents still keep a session working. With the program's own status, working
shows the orange dot, waiting for permission, an answer or a sign-in shows the blue
dot, finished shows the blue dot until you look, and a failure shows a red dot and
a Failed notification. OMP's tool approvals and questions come from OMP's hooks the
way Claude Code's do, not from its own status: each approval dialog OMP shows and
each question notifies you once, an approval shows the red approval dot until you
answer it, and the session shows working while the approved tool runs. An OMP
session that was already running when Ghostex updated shows them after OMP restarts.
An agent that goes from working back to idle also counts as finished, unless
you stopped it with Escape or the chat's Stop button. Hover the session to read
the program's one-line message (and its progress when it reports one); on the
phone, long-press the session to read it. `ghostex sessions --json --full` shows each session's
reports under `programStatus`, and for a session waiting on you, under `attention`, whether its
turn finished or needs your input, when that turn started working, and which notification it
belongs to once that notification exists.

With Show debug UI controls on, the Program status mismatches log on the
Debugging page records each time detection and a program's report disagree.

Ghostex plays a sound and shows a notification (on the computer and, with push on,
on the phone) only when something needs you. A coordinator's thread finishing its
work never alerts: it reports to its coordinator. A coordinator alerts only when it
needs you: it asks you something, ends with a 🛑 Blocked or ❓ Needs you section, or
gets a red dot; a coordinator that just finished stays quiet. A red dot always
alerts: an approval only you can grant, a session that is not moving, a session
Ghostex could not restart, and a session that is blocked because every account is
out of usage or it stopped on an API, sign-in or similar error Ghostex couldn't get
past (hover the dot to read why). The exception is a red dot Ghostex is fixing
itself: while Autopilot covers the session, a session that is not moving alerts only
once Autopilot leaves it for you, and an approval it answers (Everything, with
approvals on) never alerts; an automatic switch to an account with room or a quick
retry that gets through never makes the session blocked at all. Other sessions
still alert when they finish, as set by Alert me for. The Notifications bell keeps
a row for every finished session; one that did not alert is already marked read.

To hear and see alerts only when you are needed, set Alert me for under Settings >
Notifications to Needs you only (`attentionAlertsFor`, `all` by default). The
agent finished sound and the system notification then come only when an agent
asks you something, waits for your permission or approval, shows a connector
card, or ends its message with a 🛑 Blocked or ❓ Needs you section. The Notifications
bell and the sidebar dots still show every finished session. The phone app has
its own Alert me for under Settings > Notifications, with the same choices, for
its attention sound and its Finished notifications. `ghostex sessions
--json --full` shows `handoverNeedsYou` under `attention` for a finished turn
that ended with Blocked or Needs you.

A session's pane can also get a coloured outline when its agent finishes work or
needs you, until you look at it: turn on Finished-work border under Settings >
Notifications (off by default) and pick its colour from the swatch beside the
switch (`showFinishedWorkBorder`, `finishedWorkBorderColor`).

Related settings: `completionSound`, `actionCompletionSound`, `copySound`,
`showMacOSAttentionNotifications`, `attentionAlertsFor`, `resetExpirySystemNotifications`,
`showFinishedWorkBorder`, `finishedWorkBorderColor`,
`showNotificationOnTerminalBell` (Settings > Notifications),
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
Restart and Quit. The first three projects in the list start open: click a
project to open or close it (click a session to open it in Ghostex), or type in
the filter above the list to find a project, session or remote machine by name.
Below this computer's projects, each remote machine has its own section with its
projects and running agents; a machine that is not connected shows one row saying
so, with a Connect button. Drag the empty part of the
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

The prompt box sends to the session you last had active in the Ghostex window
(or the most recently active one), or to any project or running session you
pick from its Send to dropdown, which follows the sidebar's order; your pick
stays for that prompt. It opens where you last left it, or
in the middle of the screen under the mouse, and stays there while you take
screenshots. Add more screenshots with its Area, App and Screen icons or the
hotkeys, and click a picture's thumbnail to edit it again, then press Enter.
The Send to dropdown opens with every project collapsed: click a project to see
its sessions, or type to filter, and Enter picks the first match. A note under the button says where
the prompt went, with Open. The Ghostex window also switches to that session, without
coming in front of the app you are in; turn this off with "Switch to the session
after sending" under Floating Capture in Settings > Integrations. Closing the prompt box without sending keeps the text for next time, and a
prompt aimed at a new session is kept as a draft session in that project,
visible in the sidebar; "Write Prompt"
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
your drives), and review before Clone & Add. Clone & Add closes the dialog and
the clone carries on in the background: a button left of the voice button in
the work area header shows it ("Cloning 14%", the full line on hover), and a
click opens the list of running jobs with each one's progress and Cancel. The
project shows in the sidebar when the clone finishes, with a short "Repository
cloned" message; a clone that fails stays in that list with the reason, Retry
and a close button. (A clone started from Handoff / Export still runs in the
dialog, which waits for it.)

Project headers show the branch and diff stats; the header’s Commit (Git) menu offers
commit, sync with main, PR review by a prompt agent, and related actions with
persistent running toasts. Commit, Push and Create PR open the Changes view (see
Views) with its commit bar ready, where you review the diff and confirm. Add Worktree on a project header creates a git
worktree so a second agent works on a branch without touching the main
checkout. Its sessions run in the worktree's folder (Files, Changes, Commit and
the diff counts use the worktree, and several sessions can share one worktree),
and the Project sidebar lists them among the project's own sessions, each with
the worktree's branch after its title. Right-click such a session for Rename
Worktree and Delete Worktree, or use the project's ⋯ menu, whose Delete
Worktree lists the project's worktrees. To give each worktree its own entry
under its project instead, turn off Settings > Sidebar & Sessions > Advanced >
Worktree sessions show in their project (`worktreeSessionsInProject`, on by
default); the phone follows it, and nothing is lost either way. The Unified
sidebar always lists worktree sessions under their repository. Worktrees can be
renamed, merged back, and deleted from the sidebar. When a worktree's folder is deleted, the sessions that
worked in it continue in the project's folder with the same conversation, so a
finished session or thread still opens and can be messaged.

A new chat shows a bar on top of its message box that says where it will run, as
one path: **project › computer › where › branch**. Click any part to change it.
The project opens a search over every project on every computer, one row per
project, recent ones first and "No sessions" on a project without one; picking
one moves the draft there, on the computer you last used for that project. The
computer shows while the project is on more than one computer (or the chat is on
another one) and lists the computers that have it; one that isn't connected is
dimmed. Either pick moves the draft: the same agent starts there with what you
typed, and the empty draft closes. In a Git project the where part picks **This
folder** (the project's own checkout, the default), **New worktree** (from a
base branch you pick in the last part, the project's current branch to start
with; that list has a search field and shows the branch picked first, then the
project's default branch),
**Existing** (a worktree the project already has, picked in the last part) or
**Cloud Box** (once Cloud Boxes is set up). With This folder the last part just
shows the folder's branch. With a worktree picked, sending the first message
creates it (Add Worktree's folder and setup command) and starts the chat there;
the sidebar shows the chat under its project, with the worktree's branch on its row. The
branch is named for you from the message: `spx-123-fix-copy-link` when it names
a ticket, issue or PR (`SPX-123`, `#123`, `owner/repo#123` or their links, using
the title when Ghostex already knows it), else its first words. If the worktree
cannot be made, nothing is sent and the message stays in the box with the
reason and Try again. Each project remembers the last choice.

On a narrow chat the path moves to two lines lined up like a grid (project and
computer over where and branch) and sits in the middle of the bar; a name too
long to fit is cut short, and hovering it (a long press on the phone) shows it
whole. While the message box has focus, **Option+O** opens the project,
**Option+M** the computer, **Option+W** where it runs (then 1 to 4 picks) and
**Option+B** the last part (Alt on Windows and Linux). They take those Option
letters from the message box on a Mac; if your keyboard types a letter with one
of them, change it in Settings > Hotkeys (New Chat: Project, Computer, Where It
Runs, Branch).

Issues Workflow (called Work mode before) ties your sessions to the tickets they
work on. There is no switch: it is on for every project once it is set up. Its
Linear parts (Create Linear Ticket, Linear statuses, Link to Linear issue or
project) appear once a Linear API key applies to the project, and its GitHub parts
(pull requests, GitHub issues and GitHub Projects) once `gh` is signed in on this
computer (`gh auth login`). It does not need Workspaces. A session is linked to
its GitHub pull request, its GitHub or Linear issues and a Linear project; its
card shows them the way every session's links show (the **Session links in the
sidebar** setting, below), and the Issues view (see Views) shows a ticket's
details. Links come from the session's branch on their own (a branch named like
`yahia/spx-1245-copy-link` or `yahia/218-arabic-plan-cards`), from the PR Linear
attached to the issue, or by hand: right-click the session, choose Link to, then
Pull request…, Linear issue…, Linear project… or GitHub issue…, and pick from the
list (the session's own repo comes first; type to search, Enter links it). A
session can link several Linear issues shipped in one PR; tick them in the list.
Something already linked shows its ID in the menu, Unlink removes it, and Back to
automatic lets the branch decide again. Each workspace picks one **Primary
tracker** in Settings > Workspaces: Linear tickets & projects, or GitHub issues &
projects (`ghostex issues tracker linear|github`, or the ID of its ticket command,
below; a workspace that never
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
same rows. When a linked PR is merged, its card offers
Clean up (remove the session's worktree and park the session; a worktree with
uncommitted changes is kept) or Keep, once per PR; `ghostex issues cleanup
<session> clean-up|keep` answers it from a terminal. The offer takes the card's
second line until you answer; on the phone's session list, tap Clean up or Keep.
Turn **Offer Clean up when a PR merges** off in Settings > Session Cards to never
get it (on by default). With **Title sessions by their branch** on (same page, off
by default), a session on a branch other than main is titled by that branch,
without your name and the ticket ID, until you rename it. Linear status needs a
Linear API key: set one per workspace in Settings > Workspaces, or for one
project in Settings > Projects: pick it in the **Project** dropdown, then set
**Issues Workflow > Linear API key** (the line under it says
whether the project uses its own key, its workspace's or the shared one, and
Remove goes back to the workspace's). Or run `ghostex issues linear-key`
and paste it (that sets the Personal workspace's key, which a workspace without
its own uses; `--project-id <id>` sets one for a single project, `--clear`
removes it). GitHub status comes from `gh`, so sign in
with `gh auth login`. `ghostex issues status` says what is set up.
`ghostex work-mode …`, the old name, still works for every one of these commands.

A workspace can also point at a **ticket command**: a program you or your team
wrote for another tracker, which tells Ghostex its name, how its ticket IDs look
and which credentials it needs. Set it from a terminal on the computer that runs
the workspace with `ghostex issues source set <path> --workspace Work`, then
store each credential it asks for with `ghostex issues source credential <name>
--workspace Work`, which reads the value from stdin (`--clear` removes it). Secret
credentials need at least 3 characters and are never shown back. A session whose
branch names one of its tickets shows that ticket on its card and in its links,
with its title and state, or Unavailable when the program can't be asked. To
link a session to its tickets by hand, run `ghostex link-session <session>
--ticket <its ID>:ACM-12` (several separated by commas; `<its ID>:none` links
none, and `--auto` goes back to what the branch says).
`ghostex issues source` shows what is set, `check` runs it again and `clear`
removes it; a workspace connected to a team can't use one. Ghostex also checks
the program again by itself once an hour, and reads a ticket again after two
minutes (sooner while it reports pull requests in review). To make it the
workspace's primary tracker, run `ghostex issues tracker <its ID> --workspace
Work` on that computer: the Issues view then lists its open tickets, which you
can open, start a chat on or start in the cloud, as with a Linear ticket, and
Settings > Workspaces shows neither Linear nor GitHub picked (picking one there
switches back). When its IDs are plain numbers, a number in a branch name
(`218-fix`) then names its ticket instead of GitHub issue #218. **Start chat**
and **Start in cloud** on one of its tickets, and `ghostex issues start <its
ticket ID>`, work in a project of the same workspace the ticket was listed in,
on the branch the ticket names, else its ID and title (`acm-12-fix-login`);
Ghostex reads the ticket once first and, if it can't (the program is not
answering, say), says why and starts nothing. With `ghostex issues start`, a
ticket ID is read as the primary tracker's when it looks like one of its IDs,
else as a GitHub issue number or a Linear ticket; add `--source <its ID>` to
read it as that program's ticket whatever the primary tracker is. To create a
ticket with it, run `ghostex issues create-ticket --title "…"` (with
`--description`, and `--no-assign` to leave it unassigned) in a workspace where
it is the primary tracker, or add `--source <its ID>` anywhere in that
workspace, and `--start` to work on it right after. The program must say it can
create tickets and have all its credentials stored, and it picks where its
tickets go, so `--team-id` and `--linear-project-id` are refused. A create runs
once and is never retried. If it takes longer than a minute or answers something
Ghostex can't read, Ghostex says the ticket may have been created and refuses
another create in that workspace for five minutes (check your tracker first);
`--start` does nothing after a failed create. The New ticket button does not
create one yet.

Every session, in any project, keeps a list of the pull requests, GitHub issues
and Linear tickets it is linked to, and gathers them by itself: the PR of the
branch it is on (and of branches it worked on before, when it has a worktree of
its own), the issue or Linear ticket its branch names, the PRs and issues the
agent opened (`gh pr create`, `gh issue create`), the issues a linked PR closes,
and the Linear tickets you name in the chat yourself (with a Linear API key set;
only IDs of the Linear teams that key sees count). Only your own messages link a
ticket: one only the session's agent named in its reply is offered under Suggested
instead, to link with one click. A ticket ID or link in another agent's message, a
thread report or a coordinator's brief, in a tool's output, a file the agent read,
a quoted (`>`) line or a fenced block counts for nothing, neither link nor
suggestion. A ticket linked that way before 2026-10-27 left the list unless you
pinned it (or comes back under Suggested when the agent's own reply named it). An
agent adds one it works on with `ghostex link add <url|#123|ABC-123>` from its own
terminal. A coordinator links (and unlinks) its threads on another computer the same
way, `ghostex link add <url> --session <thread's ref>`, but only threads it started there
itself; no other session can change links on another computer. A PR or issue link of the
project's own repos that you or the session's
agent only wrote in the chat is offered under Suggested too. A coordinator's own
replies suggest nothing (they mostly repeat its threads' reports); there only the PR
and issue links you write yourself are suggested. Suggested shows the five
newest; when there are more, Show all under them lists the rest, and Show fewer folds
them again. A suggestion's × (click it twice, like a linked link's) dismisses it for
good: it moves under Removed, where Restore puts it back under Suggested, and a Linear
ticket you later type yourself still links. The × on the Suggested header removes
every suggestion the same way (the first click turns it red with Remove all?, the
second removes them; on the phone, tap it twice). The + in the list's header (Add on the
phone) links one by hand: type `SPX-123`, `#123` or `owner/repo#123`, or paste a
GitHub or Linear link, and press Enter; Ghostex asks GitHub whether a bare number is
a PR or an issue, and says so under the field when it can't read it or GitHub doesn't
know it. The strip of pills along the top of the chat box ends with a Links pill
that reads like the sidebar's badge, in the softer colours of the project header's
+/- line counts: a link icon and one link's number or ID, with a + after it when
there are more (`#123`, `SPX-123+`); with no links yet but suggestions it says
`3 suggested`, muted and dashed. Click it for the whole list, which opens above the
chat box, lined up with the pill like the other pills' menus:
each link with its icon (a branch for a pull request, the
GitHub mark for an issue, the Linear logo for a Linear ticket, coloured by its
state; rest the pointer on the icon to see the state in words, with a pull request's
checks and review), number and title, and under it, only when there is something to act
on, an open pull request's checks (passing, failing or running) and review (review
requested, approved or changes requested) or the pull request that closes an issue.
Suggested and Removed links look the same, with a link button (Link) or a restore
button (Restore) where a linked one has its ×. The list
leads with Open, the links that still need something, then Merged / Closed
(folded until you open it), Suggested and Removed, each listing pull requests,
then issues, then Linear tickets. Each group folds with the
chevron on its header and keeps its count; what you fold stays folded in every
session and after a restart (the phone keeps its own). Click a link to open it; **Open
session links in** (Settings > Files & Links > Browser) opens them in the internal
browser (the default) or your default browser. In the internal browser a link opens in
its session's project, switching to that project when another one is on screen. Click
a link's × and then Remove?
within three seconds to remove it; a removed link never comes back by itself. Click
the pin beside the × to pin a link (the main issue or PR of the session, say): pinned
links lead a Pinned group at the top of the list, pinned issues and Linear tickets
first, then pinned pull requests, and the
sidebar always shows them under every Session links choice but Off (with Badge, the
badge names the pinned link). Click the pin again to unpin.
`ghostex link pin <link>` and `ghostex link unpin <link>` do the same. The pin and the
× show when the pointer is on a link (a pinned link always shows its filled pin). Each
link keeps what was last read about it, so its title and state show right away after
Ghostex restarts; a link not read yet says Loading… until its state is known. A Linear
ticket Linear no longer has (deleted, or not visible to your key) shows grey as Not
found under Merged / Closed.
A removed link leaves its group at once and stays under a Removed group, folded until you open it, at the bottom of the list
(on the phone's sheet too) with Restore, which puts it back as it was and lets it
join by itself again. In the sidebar, **Session links in the sidebar**
(Settings > Session Cards) picks how a card shows them: Always (a line of chips
under every card that has links), Selected session (that line only under the
session you have open), Badge (the default: a small badge on the card with one link's
number or ID, the pinned one, else the only one, else the most pressing, with a + after it
when there are more, such as `SPX-123+`, coloured by the most pressing link in the soft tones of the project
header's +/- line counts: green for open work, red for failing checks or changes asked for, amber for a
review asked for or checks still running, purple once merged or closed), Open work only (a line of the ones
still open, none once they are all merged or closed), or Off. A session with no
links yet but links the chat mentioned shows a muted, dashed badge counting those
suggestions on a line under its name, with every choice but Off. Rest the pointer on a
card that has links and, after the tooltip delay, a card opens beside the sidebar
listing them the same way, with the same open, ×, Remove? and Link as the
header's list. It is as tall as what it shows and grows or shrinks when you fold a
group; past half the window's height the list scrolls inside it. It stays while the pointer is on the card or its row (Off shows no
card). A red dot on +N means a link behind it has failing checks; Ghostex sends no
notifications for link changes. Sessions on a connected remote machine show their links the
same way, and pinning, removing and restoring act on that machine. On the phone, the chat's bar above the composer
has the same Links pill: tap it for the whole list (touch and hold a link for Copy link, Pin or Unpin and Remove, which asks
Remove? first; Link adds a suggestion). The
phone's session list follows the computer's Session links in the sidebar, and a
session's long-press menu has a Links row that opens the same list. Open links
are checked again every few minutes while their session is in use and every half hour
once it has been idle for a day; opening a session's links list (or running
`ghostex link refresh`) checks its links at once. Merged or closed ones stop being
checked.
`ghostex link list [--session <session>]` prints a session's links and
`ghostex link remove <link>` removes one and `ghostex link restore <link>` brings it back. Settings: `sessionLinkOpenTarget`
(`embedded` or `external`), `sidebarSessionLinks` (`always`, `selected`, `badge`,
`open` or `off`).

To start a piece of work from a new ticket, open the project's "…" menu and choose
Create Linear Ticket… (it shows once a Linear key applies to the project; in a
workspace whose primary tracker is GitHub it is Create GitHub Issue…, once `gh` is
signed in,
which makes the issue in the project's repo and works on
`<your GitHub name>/<number>-<title>`), or click **New ticket** at the top of the
Issues view, which opens the same
dialog for the repo the list is filtered to and otherwise lets you pick the
project. Give it a title and, if you like, a description, a team and a Linear
project; it is assigned to you unless you turn that off. With Start work now on,
Ghostex creates the ticket and starts the agent you pick in a new worktree on the
branch Linear suggests for it, linked to the ticket. Nothing is sent to the agent:
you type the first message. To start on a ticket that already exists, run
`ghostex issues start SPX-1245` (or `#218` for a GitHub issue, which works on
`<your GitHub name>/218-<title>`, or a PR link or `--pr 412` for a pull request,
which works on the PR's own branch and links the session to the PR and the issues
it closes); a second session on the same ticket joins the first one's worktree and
branch. Add `--cloud` to start a Claude Code cloud session on the ticket's branch
instead (with the task from `--prompt-file`, or the one Ghostex writes from the
ticket); it prints the session's link. `ghostex issues create-ticket --title "…"
--start` does both from a terminal (a GitHub issue in a GitHub workspace, a
ticket of the workspace's ticket command where that is its primary tracker or
with `--source <its ID>`, see Issues Workflow above). New session still
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
Its last step connects this workspace to the new project. Settings > Workspaces makes a one-time
invite link (valid for a week), and each teammate joins with it there; joining,
invites, the team's Linear keys and its Slack flow are in Settings > Workspaces. So that a Slack request reaches you, link
your Slack user once: send @Ghostex any message in Slack, and its private reply
gives the exact command, `ghostex team identity --slack-user U012ABC --slack-code
gxl_…`, with a one-time code that works for 30 minutes (Ghostex needs it to know
the Slack user is really you).
`ghostex team status` shows the connection, `ghostex team ping` checks that your
Ghostex receives requests, and `ghostex team leave` disconnects. The same is in
Settings > Workspaces under each Work workspace: **Team (Convex)** joins with a
pasted invite link, shows who you are in the team, and has **Copy invite link**
(owners) and **Leave**; **Your Slack user** links your Slack member ID with the
code from that private reply; and **Set
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
scopes added and the app reinstalled before the Issues view can show channel names),
store its token and signing secret with `ghostex team slack-connect` (read from
stdin, on the computer that deployed the team), then set the flow in Settings >
Workspaces: the **Slack** row's **Copy app manifest**, whether the bot token and
signing secret are stored (with the `slack-connect` command to copy), and the
workspace's **Team flow** section for the working channel, watch-only channels,
repos for new work (and a Linear project for the tickets each channel's work
creates), where new work runs, the default Linear team and the team instructions.
Only you change the flow there; `ghostex team flow show` reads it, and agents and
the CLI can't change it. The team instructions are added to every session started
from Slack. **Never work without a ticket** is
always on. Only the team's owners can change the team flow; members see it
read-only.

Slack commands find and create Linear tickets with the team's Linear key. Only the
team's owners set it: paste it in **Linear for the team** under the Work workspace
in Settings > Workspaces (members see whether it is set and who set it); agents and
the CLI can't set it. Tickets created
with it show the key's owner as their creator in Linear. To be the creator of the
tickets you request from Slack yourself, turn on **Create my Slack tickets with my
own Linear key** there: Ghostex stores the
workspace's own Linear key (its **Linear API key** row) in the team's Convex project
for you, keeps it up to date when you change that key, and removes it when you turn
the switch off or leave the team; everything else still uses the team's key. The
team's Convex admins can technically read stored keys. Ghostex also fills in your
Linear user from your workspace's Linear key, so tickets created from your Slack
requests are assigned to you.

Related settings: Settings > Projects > Global settings (a fold at the top of the
page, closed by default with a summary of what is set) > Global Defaults (worktree
command, docs directory), Settings > Projects > Issues Workflow > Linear API key (for
the project picked in the page's Project dropdown), `hideProjectHeaderDiffStats`,
`showProjectEditorDiffFileCount`,
`showUntrackedProjectDiffWhenNoTrackedChanges`. Issues Workflow:
`titleSessionsByBranch`, `offerCleanupWhenPrMerges`, `ghostex issues
status|linear-key|source|tracker|create-ticket|start|cleanup` (old name `ghostex
work-mode`; `create-ticket` and `start` take `--source <ticket command ID>`),
`ghostex link-session`. Team
backend: `ghostex team deploy|identity|status|ping|leave`. Slack:
`ghostex team flow show|slack-manifest|slack-connect`,
`ghostex slack post`.

## Extensions, Open In, and integrations

- The **Extensions** row at the bottom of the Settings sidebar, below About,
  opens the Extensions page. Under it, a small chevron row (*N extension
  pages*) folds and unfolds the add-on pages: Voice agent, Workspaces, Cloud
  Boxes, Connectors, Actions, Open In, OS Integration and Debugging, each only
  while it is shown. Debugging is a built-in extension, on by default, with its
  switch in the Extensions page's Built-in tab; Connectors and OS Integration are
  experimental, off by default, with switches in its Experimental tab
  (`connectorsHidden`, `osIntegrationHidden`, `debuggingHidden`). **Show
  Advanced** (its switch is in the Settings sidebar) never hides a page: it only
  shows or hides rows inside pages. The pages start folded and Settings remembers whether you left
  them open; a search opens them and lists only the pages that match, and opening
  one of them from elsewhere (a link, a menu, a turned-off message) opens
  them too. While they are folded over the page you are on, the chevron row is
  highlighted.
- Settings > Extensions is in tabs: **On** (everything that is on, from every
  other tab, grouped under the tab it belongs to with how many are on there;
  something limited to some projects or Spaces says so on its row, and
  something you turn off there stays listed, switched off, until you leave the
  tab, so a slip is one click to undo), **Built-in** (parts of Ghostex you can
  switch off), **Views** (project websites, code and files, planning, the view
  order with Arrange views, and your own views), **Panels & menus** (header
  buttons and ⋯ menu entries), **Installed** (extensions you installed, Install
  from folder…, and Collab), **Community** (the audited third-party extensions
  you can install) and **Experimental**. Each tab shows its count, and Settings
  reopens on the tab you had open. Every extension is a card of its own row:
  every card the same height, with its icon, its name and one line of what it
  does, cut short with "…" when it is longer (point at that line, or at the (i)
  that appears beside the name, to read all of it); a card that is off shows
  its name and icon greyed out. The search box at the top of
  Settings searches every tab at once: while you type, the page lists the matches
  of every tab and each tab shows how many it has. Each tile's switch turns it on
  or off, and its actions (Edit, Remove, Reinstall) appear when you hover it.
  Clicking an installed or Community extension's tile (anywhere but its switch,
  buttons and (i)) opens its page: what it does, its settings, permissions and
  version, with Update, Uninstall or Install; the arrow at the top goes back to
  the same tab. Extension commands use the active local project's folder unless the
  extension supplies a folder; relative folders are resolved inside the active
  project. Installing or updating a third-party extension first lists what it
  asks for (permissions, a background process, a remote website) and grants only
  that; an update always shows the list again, and permissions you removed stay
  removed when it updates.
  Experimental holds features that are still early, each off until you turn it
  on: Main Hub (beta), GBots (beta), Read aloud, Connectors, OS Integration, Keep Awake
  (macOS), All Automations, Terminal shaders (macOS), Faster rendering, Tool call lines and
  Security logs. They replace the single
  Enable Experimental Features switch; anyone who had it on keeps each of these
  as it was.
  Built-in holds whole parts of Ghostex you can switch off to keep the app
  simple: Actions (off by default), Open In (on by default), Spaces (off by
  default), Cloud Boxes (on by default), Workspaces (off by default; it brings workspaces and the team's
  Slack flow) and the Session details panel (beta, on by default). Turning one off removes it everywhere at once (its header button,
  Settings pages and rows, hotkeys, Quick Access rows and menus) and turning it
  back on restores everything you had set up. Settings: `actionsHidden`,
  `openInTitlebarButtonHidden`, `sidebarSpacesEnabled`, `cloudBoxesEnabled`,
  `workspacesHidden`, `sessionDetailsPanelHidden`; Experimental: `mainHubHidden`,
  `gbotsEnabled`, `readAloudHidden`, `connectorsHidden`, `osIntegrationHidden`,
  `keepAwakeEnabled`, `allAutomationsEnabled`, `terminalShadersEnabled`, `fasterRendering`,
  `chatToolCallLines`, `securityLogsEnabled`.
  The Edit (pencil) button on a tile chooses where that view, header button, or
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
  To delete one of your own views, open it with its pencil button and press
  **Delete** at the bottom of its editor (or hover its card and click the
  trash button), then confirm. The view is removed from every project and its
  tab closes. Built-in views and the built-in templates cannot be deleted, only
  switched off; a template you saved yourself has its own remove button in the
  Add view template list.
  Star saved Claude and Codex accounts in Settings > Accounts (the Related
  settings links at the bottom of this page open it) to show their usage at the
  bottom of the desktop sidebar, or unstar them to hide it. The meters are the
  last thing in the sidebar, with nothing under them, four per row. To hide
  them, point at the meters and click the small button that shows above them,
  at the right (its tooltip is Hide account usage), or right-click a meter and
  choose Hide account usage. While they are hidden nothing is drawn at the
  bottom. Move the pointer to the bottom of the sidebar and the usage icon, a
  small chart, shows at the bottom right: point at it and every starred account
  floats over the bottom of the list, without moving the list; the icon turns
  into a pin, and a click on it keeps the meters there. Ghostex remembers
  whether they are pinned. The icon is only there once you have starred an
  account and hidden the meters. Both the button and the icon can be reached
  with Tab.
  Claude meters show the two tightest of the 7d, 5h, and Fable
  limits, so the Fable limit is never hidden when it is running out; launcher
  and picker rows and the Accounts figures use the same two numbers. Each
  meter opens that login's live limits, reset times, and extra usage or rate
  limit resets, with the Fable limit as a main bar for Claude. A Gemini, Cursor
  or Copilot meter opens that provider's own popup: its name and logo, its
  limits and plan, and a cell that opens the account in Settings. Right-click a
  usage meter for Extensions and Accounts. Click the same
  usage meter again to close its dropdown. Click another dropdown's
  button to close the current dropdown and open that one in a single click.
  Clicking outside, including in
  Session Chat, closes usage dropdowns. More model
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
- An extension you install from Settings > Extensions opens where it says on its card. A view
  extension (Canvas, Tower) gets a view tab in the view picker. A terminal-pane
  extension, such as Lazygit, opens its command in a terminal split beside the
  focused pane (or a new tab: Open location on its Settings page), in the active
  project's folder. Every enabled extension that opens on its own (terminal panes,
  views, modals) is an **Open <name>** row in Quick Access's Commands (type its name),
  and is listed under Tools > Extensions in the sidebar's menu; a pinned extension also has its
  icon in the work area header. A newly installed terminal-pane extension starts
  pinned; turn **Pinned** off on its Settings page to remove the icon. A
  terminal-pane extension needs its command on this computer (Lazygit needs
  `lazygit`): when it is missing, opening the extension says so, and on a Mac with
  Homebrew its **Install with Homebrew** button runs `brew install <command>` in a
  new terminal; open the extension again once it finishes. Terminal-pane extensions
  run only for local projects. Only you install extensions, in Settings >
  Extensions, which shows what each one may do first; agents and the CLI can't
  install one, turn one on or give it a permission. To install one you made or downloaded
  yourself, use **Install from folder…** on the Extensions page's Installed tab: pick its folder,
  then tick each permission you give it (nothing is ticked at first; it gets nothing
  you leave unticked) and Install. CLI: `ghostex extensions list`,
  `ghostex extensions state <id> --set pinned=true`.
- **Claude** is a built-in extension, off by default: turn it on with its switch
  in Settings > Extensions > Built-in to use claude.ai inside Ghostex.
  It adds a Claude button to the header, directly left of the Talk to the voice agent button.
  Clicking it turns Claude on: claude.ai takes the whole window, from any
  project, and the sidebar, the side panel and the Commands pane are hidden.
  The header's Claude button stays while Claude is on so you can turn it off
  there, and the header no longer names the project and session you were in.
  Claude never gets a tab in the side panel; you can still open the side panel
  beside it (the Browser and every other view work there as usual), and bring
  the Commands pane back with its usual button and shortcut. Showing the
  sidebar (Toggle sidebar or Cmd+B) turns Claude off and takes you back to
  where you were before it, like switching back to a Space and project: the
  same Space, project and session, with the sidebar, side panel and Commands
  pane as they were. Pointing at the window's left edge only peeks at the
  sidebar and leaves Claude on; clicking the Claude button in the header turns it off.
  Opening any session also turns it off: a sidebar click, Quick Access, a
  notification, a new agent, a session shortcut, or switching to it from the
  phone or `ghostex focus`. Turned off with the Claude button or by opening a
  session, the panels come back as they were and anything you opened while
  Claude was on stays open.
  Claude stays on after a restart. Picking Claude in the side panel's Open a
  view picker turns it on too. Each saved Claude account (Settings > Accounts)
  keeps its own claude.ai sign-in, the same one its cloud sessions use, and the
  bar above the page shows the account you are on (a green dot once it is
  signed in): click it to switch accounts, sign the current one out of
  claude.ai, or open Manage accounts. Ghostex remembers the account you
  picked. When the account is not signed in to claude.ai (the first time, or
  after its sign-in ran out), the page says which email to sign in as; a page
  signed in as a different email than its account offers **Sign in as** the
  right account. With no saved Claude account, the page uses one shared
  sign-in. Setting: `claudeWebHidden`.
- **Grok Bot** is a built-in extension, off by default: turn it on with its switch
  in Settings > Extensions > Built-in to use grok.com inside Ghostex. It adds a
  Grok Bot button to the header, left of the Claude button (both sit left of the voice button), and works like Claude:
  clicking it turns Grok Bot on and grok.com takes the whole window, the same way
  (the hidden panels, the header, showing the sidebar or opening a session
  turns it off). With Claude on, clicking Grok Bot switches to Grok Bot in place, and
  back. Grok keeps several logins apart: each login has its own browser
  profile, so its grok.com sign-in, cookies and history never reach another
  login's. The bar above the page shows the login you are on: click it to
  switch logins, **Add another login** (a new, signed-out login named Grok 2,
  Grok 3 and so on), or **Manage logins…**, which opens Settings > Files &
  Links > Browser, where **Grok Bot logins** renames and removes them (removing one
  signs it out). Ghostex remembers the login you picked. The bar's house button
  goes back to the start page and the arrow reloads the page. The page opens on
  grok.com; to open a page of your own instead, such as one of your Grok bots,
  put its address in **Grok Bot start page** on the same Settings page (one start
  page for every login; left empty, it opens grok.com). Ghostex never reads
  the Grok page, so the bar shows no signed-in dot or email. Settings:
  `grokWebHidden`, `grokWebStartPage`, `grokWebLogins`.
- **Session notes** is a built-in extension, off by default: turn it on with its
  switch in Settings > Extensions > Built-in to attach a note to an agent
  conversation, such as what to do next when you come back to it. Then
  right-click a session and choose **Note** (or use its hover button, or
  Option+Cmd+N on macOS for the focused session), or pick **Session note** in the chat's
  More actions (under Agent) or the terminal's More actions; a session with a note shows a small dot on
  its card, the note in its tooltip and a "1" beside Session note. The note
  belongs to the conversation, so it is still there after you close the
  session and resume it, and the phone app shows and edits the same notes.
  Turning it off hides all of that, here and on the phone, and keeps every
  note you wrote for when you turn it back on. Setting: `sessionNotesHidden`;
  CLI: `ghostex session-note read --session-id <id>` and `ghostex session-note
  save --session-id <id> --note '<text>'` (only while it is on).
- Settings > Open In chooses which apps appear on session and project Open In
  menus and adds custom open targets. Open In is a built-in extension, on by
  default: switching it off in Settings > Extensions > Built-in removes the
  header's Open button, the Open In page and the Open In rows in Quick Access
  and menus, and keeps your apps and custom targets for when you turn it back
  on (`openInTitlebarButtonHidden`).
- Settings > Integrations installs the bundled agent skills and shows their
  install status: Ghostex CLI, Ghostex Help, Ghostex Visuals, Ghostex Agents and
  Project Board Beads under Agent skills, and each computer or browser skill
  right under its tool in that tool's own section (below). Uninstall all under
  Agent skills removes all of them; an installed skill's row shows the command you
  type to use it, such as `$ghostex-computer-use`. Each skill is installed
  once, in `~/.agents/skills`; agents that do not read that folder (Claude
  Code, Kiro, Droid, Hermes and others) get a link to it in their own skills
  folder, so every agent uses the same copy. When the computer is
  online they are downloaded from the Ghostex GitHub repository, so skill fixes
  arrive between releases, and installed skills are refreshed automatically
  each time Ghostex starts; a copy an older Ghostex left in an agent's own
  folder is replaced by a link then. Offline installs use the copy inside the
  app.
  Its Computer & browser use section holds everything for Fast Computer &
  Browser Use, Trycua's open-source driver (the `trycua/cua` link next to its
  name opens the project on GitHub): its install row, the Ghostex Computer Use
  and Ghostex Browser Use skills right under it, then its permissions.
  Installing it, Ghostex Computer Use or Ghostex Browser Use also
  installs Trycua's cua-driver skill (`cua-driver skills install`), which
  teaches agents the driver's own commands and which both Ghostex skills read
  first; if that skill is missing, the row shows an Install skill button.
  If cua.ai does not answer on your network (Mac, Linux or Windows), the installer (and the update and
  uninstaller) is fetched from Trycua's GitHub repository instead, with no
  action needed; the driver itself always downloads from GitHub releases.
  Once it is installed, its
  row shows an update button when a newer release is out (on a Mac), or a
  check mark when it is up to date (click it to check again), a reinstall
  button that runs the official installer again, and an uninstall button
  that removes Fast Computer & Browser Use but keeps its Accessibility and
  Screen Recording permissions. Hover them to see the installed and latest
  versions.
  On an Apple Silicon Mac with macOS 14 or later, the Computer use on a hidden
  screen section holds SpaceO, its Ghostex SpaceO skill and its permissions.
  SpaceO gives agents their own hidden screen: apps open on a
  virtual display, so agents click, type and take screenshots there while you
  keep your own screen, pointer and focus. Install SpaceO runs its official
  installer, keeps SpaceO running in the background and installs the Ghostex
  SpaceO skill; it does not add SpaceO to your agents' MCP settings. Its row
  gets the same update, reinstall and uninstall buttons: Settings checks
  SpaceO's latest GitHub release the way it checks Fast Computer & Browser
  Use's, the update button turns blue when a newer version is out (hover it
  for the versions; click the check mark to check again), and an update
  runs SpaceO's official installer, then restarts SpaceO once the agents
  using it finish. Its Accessibility and Screen Recording permissions carry
  over an update; if one is still missing afterwards, the update says so.
  Uninstalling ends agents' SpaceO sessions and keeps the permissions. On a Mac, both tools get an Accessibility row and a
  Screen Recording row that always show Granted, Not granted or Needs restart
  and update on their own while Settings is open. Grant makes the tool itself
  ask macOS for the permission, so it is listed in System Settings (SpaceO as
  "spaceo", Fast Computer & Browser Use as "Cua Driver"), and opens that
  Privacy & Security pane; turn the switch on there and the row turns to
  Granted. For SpaceO, a small Ghostex tag sits on the right edge of System
  Settings (its left edge when the right has no room), pointing into its app
  list: drag its spaceo tile into the list, which adds SpaceO and switches it
  on. The tag shows Waiting for the drop until macOS has the permission, then
  opens the other permission's pane (Screen Recording first, then
  Accessibility). If SpaceO started before a permission, it restarts by
  itself once no agent is using it (the tag says Restarts when SpaceO is free
  while one is), then shows SpaceO is ready and closes; the rows' Needs
  restart has a Restart SpaceO button that waits the same way. Fast Computer
  & Browser Use has no tag: if it isn't listed, click + and add
  /Applications/CuaDriver.app. During a Grant it restarts by itself once macOS
  has both permissions, and its Needs restart row has a Restart Cua Driver
  button, which ends what agents are doing with it right then. If it is
  already on but the row stays Not granted, the entry is out of date: select
  it, remove it with the minus button, and click Grant again. Close the tag
  any time with its x. SpaceO is macOS only: on Windows and Linux its rows
  are hidden and `ghostex spaceo install-skill` says so. Ask agents
  to use $ghostex-spaceo (or /ghostex-spaceo in Claude).
  The Embedded browser section holds the Chromium runtime (CEF) and the Ghostex
  Embedded Browser Use skill, which lets agents control Ghostex's browser panes.
  Commands: `ghostex spaceo install-skill`, `spaceo doctor`.
- Settings > Integrations > Tools lists the tools Ghostex can install for you
  when something you set up needs them: Node.js and npm, uv, Homebrew (Mac),
  System tools (curl, certificates, unzip and git on Linux), Beads, the
  GitHub and GitLab CLIs, and Hex (the dictation app Dictate's Hex engine uses:
  on a Mac Ghostex downloads Hex.app from Hex's release service, checks its
  checksum and Hex's code signature and puts it in Applications, replacing an
  older Hex there; on Linux and Windows it runs Hex's own installer). Each Install button installs with one click and its
  tooltip says exactly how. When you already have a tool (for example Node.js
  from nvm or Homebrew), Ghostex uses yours and shows it as installed by you;
  otherwise it downloads the official release, checks its checksum and keeps
  it in its own tools folder, added to the end of your PATH. Beads (`bd`) and
  the GitHub and GitLab CLIs are linked into `~/.local/bin` instead when that
  folder is on your PATH (never over a `bd`, `gh` or `glab` of your own), so
  terminals and agents that are already open find them at once. If a tool
  Ghostex installed can't be found by name in a new terminal, its row is not
  shown as installed and Install puts it back on your PATH. Homebrew asks for
  your Mac password once; Linux system tools ask for your password once, or
  open a terminal that asks for it where there is no password dialog (WSL).
  Tools Ghostex installed get Update (or a check mark to check again),
  Reinstall and Uninstall buttons, and installing an agent that needs npm
  installs Node.js first on its own.
- Connectors link a workspace to your apps (Gmail, Google Calendar, Slack,
  Notion, GitHub, Linear, Outlook and many more) through Composio. Connectors
  is experimental and off by default: switch it on in Settings > Extensions >
  Experimental (or run `ghostex settings set connectorsHidden false`). Switching
  it off again (`ghostex settings set connectorsHidden true`) removes its page
  and its apps from agents, and
  `ghostex connectors` refuses until you turn it back on; your sign-ins and
  connected apps are kept. If `ghostex connectors` is still not in `ghostex
  --help` with both on, this version of Ghostex does not have Connectors yet. Sign the
  workspace in to Composio once with `ghostex connectors sign-in` (it opens
  your browser), then connect each app with `ghostex connectors connect
  <app>`, for example `gmail`. For Discord, connect `discordbot`, not
  `discord`: Composio adds its own bot to the server you pick, and agents can
  then list that server's channels and read the messages and threads the bot
  can see (`discord` only signs in as you and can't read messages). Composio's
  bot asks for wide permissions; if agents only read, you can cut its role
  down to View Channels and Read Message History in Discord's Server Settings
  › Roles. Each workspace has its own Composio sign-in
  and its own connected apps, so work apps stay in your Work workspace; while
  Workspaces is off, everything belongs to Personal. The sign-in stays on this
  computer, sealed with a key kept in the system keychain; if it can't be read
  anymore, Settings › Connectors and `ghostex connectors status` say so, and you
  sign in again. `ghostex connectors
  status` shows every workspace's sign-in and apps, `ghostex connectors tools`
  lists the Composio tools, and `ghostex connectors call <tool> --args
  '<json>'` runs a tool that only reads, such as `GMAIL_FETCH_EMAILS`;
  anything that would send, post, create, edit or delete is refused.
  `ghostex connectors sign-out` removes a workspace's sign-in. Every command
  takes `--workspace <name>`, and `ghostex connectors help` lists them all,
  so an agent can set connectors up for you. Settings › Connectors (shown while
  Connectors is on) does the same with buttons, one section per
  workspace: the Composio sign-in and apps, Executor and its apps, the
  Ask/Allow/Never rules and the `gh` and `gws` sign-ins.
- Agents use connectors on their own: every Claude and Codex session that
  starts or resumes while Connectors is on gets its workspace's connector
  tools (Composio's and, when Executor is on, Executor's), so you can just
  ask an agent to read your mail or file a Linear issue. Apps connected
  later reach running sessions within seconds. Reads run at once. Before an
  agent sends, posts, creates, changes or deletes anything, Ghostex shows a
  card in that session's chat: Approve, Always allow (stop asking for that
  tool in this workspace) or Deny, and the session shows waiting on you in
  the sidebar until it is answered; a card nobody answers ends after 30
  minutes. A program an agent runs through Executor asks only for the steps
  whose app asks for approval. `ghostex connectors rules` shows what a
  workspace does with such calls, and `ghostex connectors rule <tool|all>
  <ask|allow|never|default>` changes it, for example `rule all never` so
  agents in that workspace can only read, or `rule GMAIL_SEND_EMAIL allow`.
- A workspace can also have its own GitHub CLI (`gh`) and Google Workspace
  CLI (`gws`) sign-in, so work agents act as your work accounts:
  `ghostex connectors gh sign-in` (or `gws`) opens the sign-in page in your
  browser (for `gh`, enter the code it shows), and Claude and Codex sessions
  that start or resume in that workspace use it instead of your usual
  sign-in; your own commands in the terminal keep your usual one.
  `ghostex connectors gh` shows it and `ghostex connectors gh sign-out`
  removes it. `gws` needs your own Google Cloud OAuth client: by default the
  one your usual `gws` uses (set up once with `gws auth setup`), or one just
  for this workspace with `ghostex connectors gws client
  <client_secret.json>` (or Choose client… in Settings › Connectors; `default`
  or Use usual goes back). An Internal client only signs in accounts of its
  own Google organization, so a personal Gmail needs an External one. This is
  part of Connectors and also experimental.
- Executor is the second way Connectors links apps, next to Composio, and
  runs on this computer. `ghostex connectors executor enable` turns it on for
  a workspace (the first time, Ghostex downloads Executor). Each workspace
  gets its own Executor with its own apps and sign-ins; it starts when needed
  and stops after 15 minutes unused. Find an app in Executor's catalog with
  `ghostex connectors executor catalog <words>` and add it with `ghostex
  connectors executor add <app>`, or add any remote MCP server with `ghostex
  connectors executor add-mcp <name> <url>` (Linear's is
  `https://mcp.linear.app/mcp`). Sign it in with `ghostex connectors executor
  connect <app>`, which opens your browser; some catalog apps, Google's for
  example, first need your own OAuth client from that app's developer
  console. `ghostex connectors executor tools [app]` lists an app's tools and
  `ghostex connectors executor call <app> <tool> --args '<json>'` runs one
  that only reads; anything else is refused for now. `ghostex connectors
  executor remove <app>` removes an app, and `ghostex connectors executor
  disable` turns Executor off for the workspace but keeps its apps and
  sign-ins for next time. `ghostex connectors executor` shows its state.
  Every command takes `--workspace <name>`.
- Ask Ghostex AI is a row of the sidebar's menu (below Keep Awake), and Extensions, Dev
  servers and Resources are under Tools. Each opens a panel under the menu
  button that closes when you click away. Ask Ghostex AI and Resources are
  available in every project. Resources lists what Ghostex is running with
  per-session CPU and RAM, and can put a session or the editor to sleep; Ask
  Ghostex AI lists sample questions, and picking one opens a Ghostex Help chat
  with the question staged so the user can edit it and press Enter. Dev
  servers lists the development servers running on this computer and on your
  remote computers. Extensions lists your installed extensions, and its
  Browse extensions… row opens Settings > Extensions, where each of these
  entries can be switched on or off; an entry switched off there is not listed
  in the menu. Settings at the end of the menu reaches that page too. There is
  no ⋯ button above the chat.
- Welcome to Ghostex is the onboarding that opens the first time Ghostex
  runs. Its five panels cover: Welcome, with three things Ghostex does (agents
  that work together, coordinators, desktop and mobile); the agents found on
  this computer, with Install buttons for Claude Code, Codex, Cursor Agent and
  Grok Build (an Add to PATH button when an installed CLI is not on PATH; a
  command that is only a launcher installing the agent the first time it runs,
  like Omarchy's, shows as not installed yet), an Install guide that installs
  any other supported agent, the Ghostex helper (agent hooks), which setup
  installs for every installed agent when you go on, and a line that checks the
  chosen agent is signed in (Claude Code, Codex and Cursor; a saved account in
  Settings > Accounts counts) with a Sign in button that opens the agent's own
  sign-in in your browser (starting an agent that was installed after setup
  asks once to connect it, and Not now there is remembered for that agent);
  which views
  to show (Browser and Files are on by default on a first run), the browser
  skill, Spaces (on for a first run, the same switch as Settings > Sidebar &
  Sessions > Spaces), the Unified sidebar (the same choice as Settings >
  Sidebar) and Fast Computer & Browser use (Computer Use: off until you turn
  it on, and on a Mac it then asks for Accessibility and Screen Recording),
  with a pointer to the other extensions in Settings > Extensions; how your
  phone and other computers connect (Connect a Device), with one switch that
  opens Connect a Device after your first agent reply; and the default session view and
  Agent approvals (Keep default or Skip permissions, the same setting as
  Settings > Agents, `agentAcceptAllEnabled`), next to the look: Appearance,
  the theme colour squares (Dark and Light tabs), Colourfulness and one
  Transparency row (the simple choices from Settings > Interface), with a
  "More theme options in Settings > Interface" link that opens the Interface
  page. Turning transparency on there also switches Appearance to Dark. The
  last panel also says that Ghostex sends anonymous usage analytics, which
  Settings > About turns off. Setup
  asks for no project folder: Open Ghostex ends it and goes straight to the
  app, and when there is no project yet, Add Project opens. Advanced settings
  later ends it and opens Settings. Closing the window instead (with no project
  yet) leaves setup unfinished and opens Add Project; it comes back on the next
  launch only if there is still no project. The very first run opens on a
  short intro video before the panels: press Play to watch it there (on Linux
  it opens in your web browser, and says so), it shows once, and Continue (or
  Enter) skips it and goes on to setup at any time; the video is also at
  https://youtu.be/QzjFB4J6-8E. Reopen the setup any time from Quick
  Access > Commands > Setup.
- The `ghostex` and `gx` commands are linked into a folder on your PATH, or
  into `~/.local/bin` when no other folder is writable (the usual case on
  Linux; never into a mise, asdf or other version-manager shims folder,
  `~/.cargo/bin` or `~/.bun/bin`). When the command is not usable, Settings >
  Integrations > Ghostex CLI says why under Command line tool. When
  `~/.local/bin` is not on your PATH it names the shell file to edit
  (`~/.zshrc`, `~/.bashrc` or `~/.profile`) and shows the line with a copy
  button: `export PATH="$HOME/.local/bin:$PATH"` (in fish,
  `fish_add_path $HOME/.local/bin`, run once).

## More than one window

File > New Window (Cmd+Shift+N, Ctrl+Shift+N on Windows and Linux) opens another
full Ghostex window, so you can put one on each monitor or macOS Space, for
example an agent testing something in one window while you work in another.
New Window is also in Quick Access's Commands tab and under Ghostex in the
sidebar's menu. A new window opens on the project and workspace of the window
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
Quick Access, a notification, the menu bar, a chat link, the Issues view,
`ghostex focus`) brings forward the window showing that workspace, or switches
this window to it when no window does. The VS Code view works in every window at once, all on the same
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

## Updates (Settings > Updates)

Settings > Updates (`ghostex settings open --tab updates`, first in the Settings
list, with a count beside it) shows everything this computer uses that Ghostex
can update, as a grid of cards: Ghostex itself, your remote machines' Ghostex,
the agent CLIs you have turned on or used in the last week, Claude Swap and
Codex Swap, Fast Computer & Browser Use (cua-driver) and SpaceO, the tools
Ghostex installed (Node, uv and the like), Hex when it is installed or Dictate
is set to use it (with Install when it is missing; Hex installs or updates only
when you click, never by itself), your extensions, the VS Code view's
code-server while that view is on, the Chromium runtime (CEF), and the Easy
Connect helper (Tailcat) that lets phones reach this computer. The helper's
card offers Update when a Ghostex update brings a newer helper than the one
Ghostex installed: Ghostex builds the new helper while Easy Connect keeps
working, then starts Easy Connect again on it once, so a connected phone
drops for a moment and reconnects on its own; pairing, approvals and which
devices are allowed in stay as they were, and if the new helper would change
this computer's Easy Connect address, or doesn't start, Ghostex keeps the
helper you had and says so on the card. It has no Update automatically, and a
helper Ghostex didn't install (your own Go or Homebrew copy, or the one a Linux
package ships, which updates with Ghostex) is shown and left alone. Each card
shows the installed and newest versions, one word for its state (Failed,
Needs you, Updating, Major), what the update does (for example "4 sessions
restart on it when idle") and an Update button; a failed update keeps its error
on the card with Retry and Dismiss, and an update Ghostex can't finish for you (an
extension asking for new permissions) says Needs you with Review…, which opens
the page where you approve it. Everything already current, and the agents
whose version Ghostex can't read (such as Cursor, with Run its updater), sit in
one Up to date card below. The filter at the top shows only Ghostex, Agents,
Tools or Extensions. Check now reads every version again (otherwise every half
hour).

**Pick a machine.** Once you have saved a remote machine, a Machine picker sits
at the top of the page, the same one Settings > Agents, Integrations and
Dictation use (picking a machine on one of those pages picks it on all of them
until Settings closes). The page then shows that machine's cards, Up to date
pills and Not installed part, with its Ghostex at the top; Update, Retry,
Dismiss and Update automatically run on that machine. It lists this computer
and the machines connected right now ("Only connected machines shown" at the bottom of
its list); a picked machine that disconnects goes back to this computer, and
one whose Ghostex is too old to
list its updates shows "Update Ghostex on <name> to see its updates" (update its
Ghostex from Settings > Remote, then its list appears).

**Update all machines**, beside the picker, runs every update on every
connected machine at once, each through that machine's own Ghostex. A line per
machine under the picker then says how it went: Updating 2 of 3…, Updated 3,
which item failed and why, or Skipped: not connected.

**Not installed.** Under the machine's updates, a quieter Not installed part
lists the agent CLIs, Claude Swap or Codex Swap, the tools and the Easy
Connect helper that Ghostex can install there but that aren't installed yet. Click one to install it on that
machine (point at it: "Install <name>"), the same way Settings > Agents,
Accounts or Integrations > Tools would (an agent CLI with the first install
method that works there); the pill shows a spinner while it installs, then
moves to Up to date. Nothing in Not installed ever installs by itself or
through Update all, and it is never counted in the ↑ button. A failed install
stays in the list with its reason and Retry.

**Update all**, at the top of the updates, runs every update the shown machine
lists, each the way its own Settings row does (an agent CLI the way it was
installed, Claude Swap through uv or whatever installed it, tools through
Settings > Integrations' installer, a remote machine with its own updater).
Ghostex itself updates last and restarts once; your agents keep running. Every
agent whose CLI changed restarts into the same chat as soon as it finishes its
turn, so it runs the new version without losing the conversation (a session
you put to sleep stays asleep). Droid's updater stops running Droid sessions,
so Ghostex waits until they are idle.

A version that is really a build id (the VS Code view's code-server) shows no
number on its pill; point at the pill to see it whole.

Each card has an **Update automatically** switch, off by default: Ghostex then
installs that item's updates by itself, only while it isn't in use (an agent
while none of its sessions is working, the other tools while no agent is
working). On Ghostex's own card, and on a remote machine's, it is that
machine's Keep this machine up to date.

The **↑ button** in the chat's top bar, left of the project name, shows how
many updates need you: Ghostex, the agents you use, Claude Swap, Codex Swap,
Fast Computer & Browser Use and SpaceO when installed, the VS Code view when it
is on, the Chromium runtime, and anything whose update failed. Click it for a
short list with **Update all** and **Open Updates…**; other updates (tools,
extensions, remote machines) are counted only on the page. The count includes
every connected computer's updates; an item from another computer names it
("Claude Code · Linux box"). Click an item in the
list to update it (or Retry a failed one); the list stays open and the item
shows a spinner while it updates, and the ↑ button shows a spinner while any
update runs. When an update fails the button turns red and stays red until you
dismiss the failure: the item shows a red icon (point at it to see why it
failed) and an × that dismisses it. Dismiss on the item's card in Settings >
Updates does the same; a dismissed failure stays hidden in every window until
that item's next update. An item only the page can finish (Needs you) opens
Settings > Updates.

## Appearance and app

Theme colours, colourfulness, the work area frame, window glass, and active pane outline live on the
Interface page in Settings, right below General (`ghostex settings open --tab theme`).
It has Text, UI Scale, Presets, Colours and Transparency sections, with Advanced controls and links
to related settings on General; a search for a row inside More opens it.
Text: App font lists the fonts installed on this computer, with a search field and
each name drawn in its own font; monospaced fonts sit at the end of the list. The
pick applies to the whole app (sidebar, chat, composer, menus, dialogs, Settings,
tooltips and notifications), while terminals, code, diffs and other monospace text
keep their own font. App default, at the top of the list, goes back to Ghostex's
own fonts. A saved font that isn't on this computer stays listed as not installed,
and the app uses its own fonts until it is. The phone keeps its own font. To make
chat text stronger, use Settings > Interface > Text strength, from the standard colour
(0%, the default) up to full white in dark mode or full black in light mode, on the
computer and the phone; muted text, links, code blocks and status colours stay as they are.
Settings: `sessionChatFontFamily`, `terminalFontFamily`, `sessionChatTextStrengthPercent`.
UI Scale, under Text, sets how big things are. Scale (70% to 200%, 100% by
default) sizes the sidebar, every chat, files open in Files (with the Changes diff)
and the terminal text together; the header, tabs, menus and dialogs keep their size.
Cmd+= and Cmd+- (Ctrl on Windows and Linux) step Scale from anywhere except a web
page, which keeps its own zoom, and Cmd+0 sets 100%. Turn on Scale areas separately
to give each area its own slider instead: Sidebar zoom (50% to 200%), Chat zoom and
Files zoom (70% to 200%) and Terminal font size (8 to 32 points). Each starts at the
size Scale gave it, Scale no longer applies, and the keys change only the area you
are in (in a terminal, only that terminal until it restarts). If you had changed one
of those sizes before UI Scale existed, the switch starts on, so nothing
changes size. The Terminal page's Font size row links here.
On Linux, the whole app first takes its size from your desktop's display scale
(GNOME, KDE, Hyprland and COSMIC included), so the header, menus, dialogs and web
pages match your other apps, and UI Scale applies on top. Display scale,
under Scale areas separately (Linux only), shows the scale Ghostex found and lets
you pick a fixed one from 100% to 300% if it gets it wrong; it takes effect when
Ghostex restarts. The first time Ghostex finds a display scale above 100% on
COSMIC while UI Scale is raised, it offers once to set UI Scale back
to 100%.
Settings: `interfaceScalePercent`, `interfaceScaleAreasSeparately`, `linuxDisplayScale`, `agentManagerZoomPercent`, `sessionChatZoomPercent`, `filesZoomPercent`, `terminalFontSize`.
Presets, right above Colours, are twenty theme cards in one row that scrolls
sideways (swipe sideways, hold Shift and turn the wheel, or drag with a finger); a
thin line separates families: Catppuccin (Mocha, Macchiato, Frappé, Latte), Dracula
and Alucard, Solarized, Tokyo Night (Night, Storm, Moon, Day), Nord, Kanagawa (Wave,
Dragon, Lotus), Everforest, and Gruvbox (Material); a moon or sun marks dark and
light ones, and the picked card is brought into view when the page opens. Use Tab or Shift+Tab to focus a card, then Enter or Space to
choose it. Clicking one or choosing it with the keyboard makes Ghostex look like
that theme in one step: it switches Appearance to the preset's mode, sets that mode's colour to a custom colour
and Colourfulness to Full so the work area, chat and terminals show the theme's own
background, picks the theme's terminal palette, and sets Terminal background to
Follow theme. The same family's preset for the other mode comes along: a light preset
always brings its dark partner, a dark preset brings its light partner unless you
picked a light colour yourself since opening Settings (Nord has no light partner).
Chat theme, Terminal theme, transparency, fonts and text strength stay as they are,
and every value stays editable afterwards; a card shows as picked while the settings
still match it. Solarized Dark's work area is a shade redder than the theme's
#002b36, the nearest the dark work area can draw. The palettes come from the themes'
own projects (licences in the app's third-party notices).
Colours: Appearance, three preview cards (System, Light, or Dark; System is the
default and follows the operating system appearance); Dark mode colour and Light
mode colour, each a row of sixteen small colour squares (Graphite, the default,
Black in dark mode or White in light mode, Slate, Midnight, a deep navy, Blue,
Indigo, Teal, Green, Forest, Olive, Amber, Orange, Red, Rose, Pink and Purple;
picking a dark colour gives light mode the same colour until you pick one there
yourself, and the light row says when it still follows) and a + square for a
custom colour, which opens a colour picker (with HEX, R, G and B boxes) right at
the square; and Colourfulness, a fine-grained slider from Subtle to Full (Soft is the default;
the name of the nearest of Subtle, Soft, Balanced, Rich, Vivid and Full shows beside it)
that sets how much of the colour shows in the sidebar and work area at once, with a small
sidebar and work area preview. Past Vivid, the colour fades toward the colour you picked
itself and is exactly that colour at Full, so a flat theme such as Monokai Pro Machine
(#273136) can colour the whole window; in dark mode a colour too light for the
sidebar's text is deepened just enough to stay readable, and in light mode a colour too
dark for the dark text is lightened just enough. Colourfulness also sets how deep a custom colour
is; it has no control of its own. More colour options can set the sidebar and
work area colourfulness separately, and show the active pane outline and its
colour.
Picking a colour square never overwrites the custom values, so picking the + square again
restores them (a theme preset does set them).
Frame the work area (macOS; in the page's Advanced section; on by default for a new install, while a computer that already had settings keeps its choice) shows the work area as a card inside the
window: a thin margin all around it, a hairline border and, without window glass, a light
shadow. It stays on with the sidebar hidden and in full screen, web pages in the work area
follow the card's rounded corners, and the sidebar is at least 196pt wide while it is on.
Off keeps the work area edge to edge (`workAreaFrame`).
Under it, Show the window buttons when the sidebar is collapsed (macOS, on by default) keeps the red, yellow and green window buttons in the window's top left while the sidebar is collapsed; off hides them until you open the floating sidebar, which then carries them in its top row (`showWindowButtonsWhenSidebarCollapsed`). Full screen keeps its own title bar either way.
Under them, Close buttons on the right (macOS, off by default) puts the X that closes a dialog or a pop-up message back at its top right. Off, it sits at the top left, where macOS puts a window's close button: the corner X of every dialog and of the dialogs inside Settings, the X in the header of the Markdown table, Mermaid diagram and page windows and of the picture viewer, and the X of the messages that pop up at the bottom of the window (`closeButtonsOnRight`). On Windows and Linux the X stays at the top right.
The chosen theme colors the
sidebar and window chrome, the sidebar's dropdown menus, and the chat view
background (chat keeps following its own Chat theme setting, so a light chat in a
dark app uses the light theme's color). Chat and terminal: Chat theme and
Terminal theme default to Follow app, with optional Light, Dark, or System
overrides; its Terminal palettes row holds the terminal palettes. Existing
saved themes are preserved, and a saved dark contrast or tint that differs from
the default starts on the custom colour. The accent color (status highlights,
accent text, advanced-setting markers) has no setting of its own: it follows the
dark theme's tint hue, and a neutral tint keeps the sky-blue accent.

On the phone, **Settings > Interface > Presets** offers the Catppuccin,
Dracula, Solarized, Tokyo Night, Nord, Kanagawa, Everforest and Gruvbox Material
families. Tapping a preset sets the app colours, colourfulness, terminal
palette and terminal background together; you can change each afterwards.
**Colours** has Appearance (System, Light, Dark), Dark mode colour, Light mode
colour and Colourfulness from Subtle to Full; **Set the sidebar and work area
separately** gives each its own slider, and a custom colour uses a hex field.
**Chat theme** and **Terminal theme** can Follow app, System, Light or Dark
independently. **Terminal palette in dark mode** and **Terminal palette in
light mode** open searchable lists. **Settings > Terminal > Terminal
background** chooses Black / white, Follow theme or Custom color.

On the phone, **Settings > Interface > Import Ghostty theme** opens a local
file to add a private terminal palette, not an app theme. The file needs
background and foreground colours and all sixteen palette entries, using
six-digit RGB hex colours; other Ghostty settings are not imported. After
**Palette imported**, choose it in Terminal palette in dark mode or Terminal
palette in light mode: importing does not select it or replace your current
colours. An unreadable or invalid file says why and leaves the palettes as they
were.

Window glass lets the blurred desktop show through the sidebar, the work area,
terminals, and chat on macOS, Windows and Linux. On macOS and Windows, menus and most dialogs (Rename
Session, Quick Access and the like) turn frosted to match. The Transparency group's Transparency
control picks Off, Dark only or Always (Dark only, the default on macOS and Linux,
keeps light mode solid; Off, so the window is fast and solid, is the default on
Windows), and
Strength sets how see-through it is, from 0 (fully solid) to 100 (fully clear); the default
is 10, and out of the box the sidebar and the work area get the same tint. The four tint settings
under More transparency options set them apart by hand, in dark and light mode separately; moving
Strength afterwards moves both and keeps that difference (it narrows only where a tint reaches 0
or 100). Blur sets how soft what shows behind the window looks, from 0 (sharp) to 100 points;
the default is 60. On Windows, Desktop and windows uses Windows' own blur, which has no amount
to set, so there Blur is a choice instead: Acrylic (the default, Windows' frosted blur, which stays
blurred while Ghostex is in the background), Plain blur (the same blur without Windows' grey
frost, so Strength can go fully clear; it takes more work to draw) or Off (what is behind shows
sharp; the fastest) (`windowGlassWindowsBlur`). The Blur slider shows on Windows once Behind the
glass is Wallpaper only, Custom image or Live, where it softens the wallpaper, picture
and video (`windowGlassBlurRadius`).
On macOS, Menu blur under More transparency options does the same for menus, panels, dialogs
and tooltips: menus, panels and dialogs blur a quarter more than the number (25 at the default
20) and tooltips use it as is (default 20; menus opened after the change use it; macOS only)
(`windowGlassMenuBlurRadius`). Behind the glass sits in
the Transparency card, with the pictures or the Live choices right under the
picked card; More transparency options holds the picture position, the Live
speed and brightness, Play only when plugged in, Fine-tune the tints and Menu
blur. With Dark only, light mode stays opaque, so the light-mode picture, video
and tints are hidden until Always is picked.
Files, Beads, the browser, and the code editor stay opaque. Turning on Reduce
transparency in the macOS accessibility settings, or turning off Transparency effects
in Windows Settings > Personalization > Colors, always makes the window opaque. Turning
transparency on or off, or changing any of its choices, applies to every open Ghostex
window at once, without restarting. On Windows the corners of menus and pop-ups follow
Windows' own rounding.
Behind the glass (on macOS, Windows and Linux) is four cards: Desktop and windows (the default) shows everything behind Ghostex; on macOS, in full screen (where nothing sits behind the window) it shows your desktop wallpaper blurred and tinted the same way, and leaving full screen shows everything behind Ghostex again (a solid color wallpaper stays black in full screen). Wallpaper only shows just your desktop wallpaper, so other windows never show through; built-in wallpapers such as Sequoia show as a still picture of that wallpaper, and a solid color wallpaper shows everything behind the window. Picture shows a picture you choose instead, one for dark mode and one for light mode, side by side with Choose and Clear buttons; a mode with no picture shows everything behind the window. Live shows something moving behind the glass, chosen for dark mode and for light mode (only dark mode with Transparency set to Dark only): one of eight calm animations drawn in your theme's colours (Aurora, Ink, Drift, Nebula, Silk, Bokeh, Waves, Mesh), so switching themes or Colourfulness recolours it at once, or, on macOS and Linux, Your video, a .mov, .mp4 or .m4v file you choose, which plays muted, looping and blurred (a mode set to Your video with no file shows everything behind the window). On Windows, Live pauses when Show animations is off in Windows Settings > Accessibility > Visual effects (showing a still frame) and in battery saver. Speed sets how fast the animation moves (a quarter of its pace to twice as fast) and Brightness how bright it glows (60% by default, a soft glow that shows through the tints; every animation is about as bright as the others at the same setting); your own video plays as it is. An animation loops every two minutes without a seam, and changing the animation, theme or brightness, or switching between an animation and your video, fades rather than jumping. Nothing is downloaded. Live pauses whenever Ghostex is in the background, hidden or minimized, while the display sleeps and in Low Power Mode; Reduce Motion shows a still frame; and Play only when plugged in (on by default) pauses it on battery. For Wallpaper, Picture and Your video, Picture position picks Moves with the window (the default: the picture covers the window and moves with it) or Stays with the desktop (the picture stays put while the window moves over it, and can trail the window while you drag it) (`windowGlassSource`, `windowGlassImagePlacement`, `windowGlassImageDark`, `windowGlassImageLight`, `windowGlassLiveStyleDark`, `windowGlassLiveStyleLight`, `windowGlassVideoDark`, `windowGlassVideoLight`, `windowGlassLiveSpeed`, `windowGlassLiveBrightness`, `windowGlassVideoOnlyOnPower`).
On Linux, transparency works on Xorg and Wayland desktops (the desktop app currently uses XWayland for its embedded pages). Desktop and windows uses your compositor's blur: on Hyprland, keep background blur enabled; on Xorg, run a compositor with transparency and blur support. The compositor controls the blur's strength. Picture and Live animations work without compositor blur. Wallpaper only reads the picture reported by Hyprpaper, swww/awww, GNOME, Cinnamon, MATE or a single-screen Plasma desktop; when no readable picture is available it shows Desktop and windows. Stays with the desktop works on X11 and Hyprland; other native Wayland compositors cannot report the position needed for this mode, so use Moves with the window. Your video requires FFmpeg (included as a Linux package dependency). Live respects battery power, the power-saver profile, and disabled desktop animations (`windowGlass`, `windowGlassSource`, `windowGlassImagePlacement`, `windowGlassVideoOnlyOnPower`).
While glass is on, four sliders tune it, each in dark mode and in light mode: Sidebar tint and Work area tint set how much of the desktop each area hides, independently, so either can be the darker one; lower shows more of your desktop.
Keep Awake (Power, macOS) prevents sleep while agents work; it is experimental:
switch it on in Settings > Extensions > Experimental (`keepAwakeEnabled`).
Faster rendering (Settings > Extensions > Experimental, off by default) redraws only the
parts of a window that changed so Ghostex uses less CPU while agents stream and you scroll; it
applies at once, and turning it off brings back the usual drawing if part of a window ever
stops updating (`fasterRendering`). The separate Debugging page sits
in the Settings sidebar's Extensions group and is listed
while its Debugging extension is on (it is by default;
switch it off in Settings > Extensions, or `debuggingHidden`, to drop the page). It starts with Show debug UI controls.
Enable that switch to see Diagnostic logs. Diagnostic logs has one switch per area (terminals,
sidebar, chat, modals, board, remote machines, agent activity, prompt editor,
app lifecycle, server requests) and one Turn logs off after choice (15 min,
1 hour, or 3 days) shared by all of them, so no area stays on longer than 3 days;
warnings, errors, and crashes are always captured. Open the page with `ghostex settings open --tab debugging`.
Settings that depend on a setting above them have an indented ↳ before their
name. They appear when the parent setting enables them.
In the Settings table of contents, click a page or section title to go there.
Only the small chevron on its right expands or collapses its entries.

Related settings: `sidebarTheme`, `darkThemePreset`, `lightThemePreset`,
`customSidebarTitlebarBackgroundDarknessPercent`, `customSidebarTitlebarBackgroundTintColor`,
`customSidebarTitlebarLightBackgroundLightnessPercent`, `customSidebarTitlebarLightBackgroundTintColor`,
`windowGlass`, `windowGlassSidebarOpacityDark`, `windowGlassWorkAreaTintDark`,
`windowGlassSidebarOpacityLight`, `windowGlassWorkAreaTintLight`, `themeSidebarContrast`, `themeWorkAreaContrast`, `showActivePaneOutline`, `workAreaFrame`, the `keepAwake*` rows,
`debuggingMode`.

### When your computer starts (start at login, resume after a restart)

Settings > Power & Sleep > When your computer starts has three switches. **Start Ghostex's
server when you log in** runs Ghostex's background server from the moment you log in, so your
other computers, your phone and coordinators reach this computer while the app is closed (on
macOS it is listed under Ghostex in System Settings > General > Login Items; on Windows it is a
"Ghostex Server" startup entry; on Linux an autostart entry). The two switches under it only work
while it is on, and turning it off turns them off too. **Also open the Ghostex app** opens the
window after the server starts. **Resume sessions after a restart or power loss** continues the
agent sessions that were running when the computer shut down, restarted or lost power, one after
another, each in its own chat with the agent's own resume command, whether or not the app opens.
Sessions you closed or put to sleep, and sessions that had already ended, stay as they were, and
plain terminal sessions come back when you open them. Restarting only Ghostex's server resumes
nothing, because sessions keep running through it.
Settings: `startServerAtLogin`, `openAppAtLogin`, `resumeSessionsAfterRestart`.

## Data folders

On macOS and Linux, Ghostex keeps its settings, sessions, chat history, pasted
images and logs in `~/.config/ghostex`, `~/.local/state/ghostex`,
`~/.local/share/ghostex` and `~/.cache/ghostex` (one folder when `GHOSTEX_HOME` is
set). These folders are private to your account: each time Ghostex starts it makes
them readable only by you, so other accounts on the computer cannot open them.

Ghostex's app, CLI and agent hooks reach its service on this computer with a private
access token. Before sending it, they check that the program listening on the
service's port (127.0.0.1:58744 by default) belongs to your account. If another
account holds the port, they send nothing and say "Another account on this computer
is listening on 127.0.0.1:…"; stop that program, then start Ghostex again.

## Disk space and automatic cleanup

Ghostex deletes old files it made by itself, once a day, so its data folders do not keep
growing. It removes screenshots agents saved more than 60 days ago (folders named
`release-*` or `youtube-*` are kept, and images you pasted into chats or board cards are
never removed), published Ghostex pages after 90 days, log files nothing has written for
14 days, crash reports after 30 days, backups taken before a data migration after 30 days,
Ghostex's copies of compressed Codex chats nobody opened for 30 days (opening the chat
again rebuilds its copy), and closed sessions idle for 90 days from the closed sessions
list (pinned and favorite sessions stay, and the agent's own conversation files are not
touched). There is no setting for it.

## Sending feedback

Send Bug / Feedback, under Help in the sidebar's menu right under Join Discord,
opens the feedback pop-up. Enter your GitHub username first ("name" or "@name");
it is required, the issue starts with "Reported by @name" so GitHub tells you
when it is fixed, and Ghostex remembers it for next time. Then write what is
broken, confusing or missing and paste screenshots into the text box with Cmd+V
(Ctrl+V on Windows and Linux), or drag image files onto the pop-up (desktop
app only): up to five PNG, JPEG or WebP images of 5 MB each; a larger PNG
screenshot is scaled down to fit. Review then shows the exact
GitHub issue, title and description, and you can edit both before Send; Ghostex
adds your screenshots and a line with the app, its version and your operating
system. The issue is public on the Ghostex GitHub, and after it is sent Open
Issue takes you to it. Cmd+Enter (Ctrl+Enter) moves to the next step and Escape
closes the pop-up. It works in the desktop app and in the web app.

Turn on "Collect more data with an agent" and the button becomes Start Agent: it
opens a new session with your default agent in the Ghostex config folder
(`~/.config/ghostex`, shown in the sidebar as its own project, "Ghostex config"
unless you already had one there), which Ghostex trusts for every agent so no
folder-trust question stops it. Each report keeps its screenshots, notes and
draft in its own `feedback/<time>/` folder there. The agent gets your message,
username and screenshots, reads this computer's Ghostex version,
settings and logs, looks into the problem and tries to reproduce it without
changing anything, keeps tokens, passwords and your files' contents out of the
issue, then shows you the whole draft in the chat and posts it once you say yes.
It files the issue with `ghostex feedback send --title <text> --body-file <file>
--github-user <name> [--image <file>]...`, which you or any agent can also run
by hand (`--dry-run` prints what would be sent without posting). When the
session you are looking at (or, with none focused, the active project) is on a
connected remote machine, the agent starts on that machine instead, in its own
Ghostex config folder, and reads that machine's version, settings and logs; the
switch's description names the machine. If that machine is not connected, Start
Agent says so instead of starting on this computer. Start Agent waits until the
agent has your report, usually a few seconds. If the agent can't start (it isn't
installed or signed in, or it isn't ready after a minute and a half), the pop-up
says why in plain words, closes that session and turns the switch off, so you can
still send your report without the agent.

When a chat message cannot reach the agent because Ghostex cannot see its input
box, the "Message not sent" card has Copy Report, and so does the chat box while
its Terminal View button is red (it clears by itself once the input box reads
again). It copies a report of what Ghostex sees in that session to the
clipboard: the whole terminal screen as Ghostex reads it, whether the input box
is ready and why not, the terminal's size, the session's waiting messages and
recent send problems, and the Ghostex version and operating system. It names
nothing about you: the session's name and id, the project's name, folder paths,
your user name, emails, account names and other ids are left out or masked
(shown as `<path>`, `<email>` and the like), including on the screen. A toast says
it was copied, and Send Bug / Feedback opens with "Collect more data with an
agent" on, so you can paste the report there and send it to us as a GitHub
issue. The phone's chat has the same button.

If Ghostex's background service (gxserver) is not answering on this computer,
Review, Send and Start Agent cannot go through it. The desktop app then offers
"Open a GitHub issue in your browser": it opens a new issue on the Ghostex GitHub
with your message and this computer's startup diagnostics filled in. Your user
name, home folder, computer name, saved remote machines, addresses, emails and
tokens are removed first. Nothing is posted until you press Create on GitHub. A
long report has its diagnostics shortened to fit in the link, and the issue says
so; a message too long even on its own is put on your clipboard for you to paste.

On the phone, Send Bug / Feedback is in the menu at the top of the Sessions
screen, under Settings. It has the same form: your GitHub username (required,
remembered), the message, up to five screenshots from your photo library or
pasted with Paste Image, and the same Review step. The phone posts the issue
itself, so it works even when it cannot reach your computer. While a computer is
connected, "Collect more data with an agent" is there too: Start Agent starts
the agent on that computer and opens its chat on the phone.

If the phone app closes unexpectedly, it saves a crash report on the phone
(what went wrong, the app version, the phone model and the screen you were on);
nothing is sent on its own. The next time you open the app, a card at the
bottom says it closed unexpectedly, once per crash: Dismiss deletes the
report, Send Report opens Send Bug / Feedback with it attached. Names of your
computers, addresses, usernames, emails and keys are removed first; Show Report
displays the cleaned text, and Review shows the exact issue before you send it.
If you started the agent instead, it removes anything personal it still finds.
A saved report you have not sent can also be attached later with Attach Crash
Report in Send Bug / Feedback.

## Answering the common questions

- "Make Claude control Codex": see Agents, actions, and orchestration.
- "Match the terminal width to the chat": `ghostex settings set
terminalViewWidthMode match-chat`.
- "How do I annotate a Browser page or Markdown file": Browser pages use
  Agentation in the Browser toolbar (see Views, Browser); Send attaches a
  picture of each annotated spot. Markdown files use
  Files: select text to comment or mark Looks good, Clarify, or Needs tests,
  then Send to the last-clicked session (see Views, Files).
- "Update everything" or "update my agents": Settings > Updates has Update all
  (`ghostex settings open --tab updates`); see Updates. One agent CLI alone:
  `ghostex agent-cli update <agent-id>`.
- "Use Ghostex from my phone or another computer": see Remote machines, web,
  and mobile, then `ghostex settings open --tab remote`.
- "Have my coordinator run work on my other computer": connect that computer
  (Settings > Remote > Computers this machine connects to, Agent access on), then tell the
  coordinator "run this on <computer name>"; see Coordinators, Threads on
  another computer.
- "Run an agent on a schedule": see Automations; open the Automate view or
  drive `ghostex automation-save`.
- "Play a sound and notify me when an agent finishes": `completionSound`
  (any value except `off`), `showMacOSAttentionNotifications true`, the
  menu bar badges (always on; remove the item from the menu bar itself), and the
  sidebar bell (kept visible with `notificationsTitlebarButtonHidden false`)
  lists every finished turn with what the agent said.
- "What does the Beads view do, and how do I start work on a bead?" (also asked as "What does the Kanban board do"): see Beads view (was Kanban); on the Board, Start work on a card dispatches an agent to it, and from the command line `ghostex board start-work <bead-id>`.
- "Does a coordinator use the Kanban board?": not on its own. A coordinator's Work panel is its own list of work (see Coordinators, Work items and The Work panel); the Beads view shows the Beads issues in the repository (Beads view). Tell the coordinator "use Beads to manage the work" (with the Ghostex Project Board Beads skill installed) or "work the high-priority beads on the board" and it keeps tasks as cards and starts a thread per card. Docs: ghostex.dev/docs/workflows/orchestrators.
