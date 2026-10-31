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
install the new package the same way you installed Ghostex. Linux downloads come
for x64 and for ARM64 (aarch64) computers, each as a .deb, an .rpm, a portable
.tar.zst and an AppImage on the release page; pick the one that matches your computer's processor
(`uname -m` prints x86_64 or aarch64). The AppImage needs no install: make it
executable and open it, and it copies Ghostex into `~/.local/share/ghostex/appimage`
and runs it from there, so agents keep running after the window closes; to update,
open the newer AppImage and it replaces that copy. Without FUSE it stops with
"Cannot mount AppImage": install the `fuse3` package, or start it with
`--appimage-extract-and-run`. Windows comes as an installer only; the
portable zip is no longer published. A portable copy you already unpacked keeps
updating in place, keeps all its data in a `GhostexData` folder next to itself and
runs beside an installed Ghostex without sharing its sessions, settings or
projects. It uses the agent hooks the installed Ghostex set up (its Settings lists
them as installed by another Ghostex) instead of rewriting them.

**Open a view** is the picker the panel shows when nothing is open in it. It
lists every view you can open here: the built-in views first, then your own views
and extensions. Press a view's letter to open it (C for VS Code, B for Browser, K for
Kanban, U for Automate, D for Files, G for Changes, T for Terminal, W for Issues) while the picker is in front. Views you hid
for this project are not in the list; **Manage views and where they appear…** at
the bottom opens their settings, and **Hidden here** on the `+` menu brings one
back. Closing the panel's last tab closes the whole panel; to bring the picker
back instead, turn off **Close side panel with its last tab** in Settings >
Sidebar & Sessions, under Advanced at the bottom of the page (`closeSidePanelWithLastTab`, on by default).

While a view in the side panel has focus, the session shortcuts act on that view
instead of the session beside it: Cmd+W or Cmd+Shift+Backspace closes its tab,
Previous/Next Tab in Pane (Cmd+Option+[ and ]) steps through the panel's tabs,
Sleep and Reload sleep or reload the view, Cmd+F searches in Files or Kanban, and
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
links (a toast offers **Undo**; it is greyed out with the reason when no session is
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
changes. To comment for the agent, hover a line and click **+**, or drag across
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
own window (for Kanban it reads **Open in new window** and moves the board to a
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
  it in the middle of a pane to show it there instead. Only sessions of the
  project on screen can go into its panes: a pane under a session from another
  project dims and says it can't split there. You can also press
  Option+Shift+D to move the focused session into a pane on the right. While the screen is split,
  the focused pane has a small bar along its top: click it for Close Pane (the
  sessions keep running) and Merge All Panes, or drag it to move that session
  onto another pane's edge (a new split) or its middle (it takes that pane's
  place), and the pane it left closes. Each pane can show the raw terminal or
  Session Chat. Cmd+Shift+O starts a new session with the active project's agent
  (the one its agent button shows, see Sidebar),
  in your default view (Chat or Terminal), like ChatGPT's New Chat. Cmd+N opens
  the New Thread picker instead: type to filter the configured agents (that
  project's agent first), Browser, or Terminal, press Enter to start it in the active project,
  and press Tab on Claude or Codex to pick an account. Cmd+Shift+T opens a new
  terminal, Cmd+T always opens a new browser tab, Cmd+Ctrl+Shift+F forks the
  focused session, and Cmd+D splits. Cmd+R renames the focused session,
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
  terminal that had it last (Focus Last Terminal). While the VS Code view itself has keyboard focus, Cmd+N and
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
  Settings: `codeServerLinkVscodeUserConfig`, `codeServerNewWindowsAsTabs`,
  `webLinkOpenTarget`.
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
  when you switch to it.
  The Browser runs on the web runtime (Chromium), an optional one-time install
  that Ghostex does not need for anything else: until it is installed the
  Browser, the VS Code view, website and extension views, and HTML, drawing and
  media files in Files show an Install button instead of the page (or Hide tab
  to remove the view), and web links and saved Browser actions open in the
  system browser. Nothing is downloaded until you press Install; once installed,
  the runtime starts quietly a few seconds after Ghostex opens, so pages open
  fast. Settings > Integrations > Web runtime (Chromium runtime (CEF))
  installs, reinstalls or uninstalls it; the Install button on the first view
  that needs it does the same. Uninstall while the runtime is running removes
  it the next time Ghostex starts (Keep installed undoes that until then); on
  Windows a Reinstall while it runs is done at the next start too.
  Annotate the current page
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
  listed under Open Files, above the Project Docs tree. Turn on Show open files
  as tabs (Settings > Files & Links > File opening) to show them as a row of
  tabs above the document instead: click a tab to show its file, close it with
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
  list and focuses its search. Escape closes the document search. The filter
  button at the start of the files list's search box shows only HTML, Markdown,
  Excalidraw, image or video files: pick one or several types (All files shows
  everything again) and the list keeps just those files under the folders that
  hold them, while typing still narrows them by name. To search the text of every file in the
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
  VS Code view, and the editor apps Ghostex knows (VS Code, Cursor, Zed, the
  JetBrains IDEs and the rest); an editor that is not installed on this computer
  is listed but cannot be picked. If you had changed the old Markdown, HTML or
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
`audioFileOpenTarget`, `docsOpenFilesAsTabs`, the Auto Sleep rows
(`autoSleep*IdleMinutes`, `keepViewsLoaded`), the Search in Files hotkey (`searchProjectFiles`), and
Settings > Extensions.

## Sidebar

There are two sidebars, picked in Settings > Sidebar & Sessions > Sidebar
(`sidebarView`: `project`, the default, or `unified`). The Project sidebar,
described below, shows one machine and one Space at a time, project by project.
The Unified sidebar shows every session from every machine, Space and project
in one list: each session is a card with its project, the machine's name when it
runs on another computer, its status on the right (Working with a timer,
Waiting, Question, Approval, Failed or Done), its title, and its branch, pull
request and agent. A project is named after its repository there, so the same
repository on this computer and a remote one is one project. Sessions are
ordered by when they last came back to you (started, sent a message, or a
finished turn), never by output while an agent works; pinned sessions come
first, and above them a Drafts section holds every session nobody has sent a
message to yet (the pencil), newest first, until its first message is sent.
Working, Snoozed and Parked fold into shelves at the bottom. Working holds
sessions whose agent works or still runs something in the background, while the
Working section switch (`unifiedWorkingSection`, on by default, under the
Sidebar switch) is on; a session leaves it for the top of the list when it needs
you. Parked shows only while session parking is on. Hovering a card swaps its
status for Snooze (1 Hour, 3 Hours, Tomorrow or Next Week) and Park; a snoozed
or parked row offers Unsnooze or Unpark instead. The filter button at the top
narrows the list to projects, machines or Spaces in a dropdown that scrolls when
long; picking a project in Quick Access > Projects also filters to it. Next to
the filter are Add project and New session, which opens the new-session picker
(agents, Browser, Terminal) in the filtered project when the filter is one
project. Cards show the project's own icon when it has one, and right-clicking
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
Click a project header (or the chevron beside it) or a group header to expand
or collapse it; rename a group from its right-click menu.
Session groups gather some of a project's sessions under a header of their
own, with a folder icon. To make one, right-click a session and choose
Advanced > Move to New Group. Once the project has a session group, the
session menu's Advanced > Move to Group page lists the project's groups: pick
one to move the session there, or Remove from Group to put it back in the
project's own list. Dragging a session onto another of its project's groups,
or onto the project's list, does the same, and dropping it in the empty space
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
Top chrome holds tag filters, Spaces, and More Options: Settings, Search by
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
brings the team backend (see Git and worktrees); Issues Workflow works without it.
While it is off there is no workspace button, every window shows every project,
the project menu has no Move to workspace, and the `ghostex workspace`, `team`
and `slack` commands say it is turned off; your workspaces, their Linear keys
and team connections are kept and come back when you turn it on (`workspacesHidden`).
Each workspace has its own projects, Spaces,
Linear API key, Claude account and Browser sign-ins (cookies), and a window
shows one workspace at a time. Every install starts with one workspace,
Personal, holding all your projects and Spaces. The workspace button sits at
the bottom left of the sidebar, in its bottom bar: the workspace's letter with
a small arrow beside it.
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
(`projectSwitchKeepAliveMinutes`, default 10, 0 to 60), so switching back is
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
- Sidebar section headings (Pinned, Sessions, Drafts, Parked, and
  Snoozed) show an orange dot when a session is working, a blue dot when
  a session is done, a pink dot when an agent is waiting for an answer, and a
  red dot when an agent is stuck waiting for your approval, including rows
  hidden by a collapsed heading. A collapsed project, a collapsed collection, a
  Space button and a computer tab show the same red for those sessions. Pending questions use
  pink instead of blue; multiple dots appear when multiple states are present,
  including orange and pink when an agent keeps working after asking. Collapsed
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
- A red dot on a session can also mean a model, effort or mode change you picked in the
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
- Remote machines appear as their own sidebar tabs; a red cloud means the
  machine could not connect, and its tab explains why.

Related settings: everything under Sidebar & Sessions > Sidebar, `agentManagerZoomPercent`
(sidebar zoom), `sidebarSpacesEnabled`,
`sidebarSpaceSwitchBehavior`, `sidebarSpaceFollowActiveSession`.

## Commands pane

The Commands pane holds command terminals below the workspace, or on its right
when Commands pane side is set to Right. Open it with Cmd+J (Mac), Shift+Esc or F12; Settings > Hotkeys can
change the first two (Open Commands Panel and its Second Key). The **Terminal** view
(see Views) is the same kind of terminal opened as a tab of the view panel
instead; it has its own tabs and does not affect the Commands pane. Auto-minimize Commands
pane is on by default: after you move focus elsewhere and leave the pointer
outside the pane for 1 minute, it minimizes while commands keep running.
Focusing, hovering, selecting text, scrolling, or resizing keeps it open and
restarts the countdown. Background command output does not restart it.
In Settings > Sidebar & Sessions > Sidebar, Auto-minimize Commands pane is one
select: Off, 15 seconds, 30 seconds, 1 minute, 2 minutes, or 5 minutes. When auto-minimize
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
place, Settings > Sidebar & Sessions > Sidebar Tags: choose Add tag, then give
it a name, an icon from the shared icon list, and a color from the preset list.
New Tag at the bottom of the Tag As menu opens that same place with the form
ready. Custom tags then appear in the same drag list as the built-in ones: drag
to reorder, use the switch or eye to hide them from menus and filters, and the
trash button to delete one (sessions that carried it become untagged). The
mobile app shows the same tags. `ghostex tag-session <selector> <tag>` accepts a
custom tag by name. Claude and Codex name their own sessions; Ghostex
syncs those names without running a first-prompt title job or blocking terminal
input. If Codex has not named a session about a minute after a prompt, Ghostex
names it in the background with the Title Generation Agent, without blocking
typing, and Codex's own name replaces it if Codex names the session later.
Pi uses the Title Generation Agent for first-prompt names. OMP names its own
sessions once a prompt says enough to name, and Ghostex syncs that name; renaming
an OMP session from Ghostex sends OMP's own `/rename <name>`, which OMP keeps.
Manual Generate Name and `/rename` in chat remain available for Claude and Codex.
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
Fork (a session's right-click menu, or More actions in its chat) opens the new
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
Once a conversation has forks, a small branch button in the chat's top right
lists every session that shares the earlier history, including the thread you
forked away from, and switches to the one you pick; a stopped branch is resumed
when you open it.

- Sleeping frees RAM; Auto Sleep does it after idle minutes. Auto Sleep runs
  on the computer that hosts the sessions, so it keeps working while the app
  window is closed, and it never sleeps a session a Ghostex window or the phone
  app is showing. Web pages sleep the same way: a browser tab in
  any project, or a Files, website or extension view, that has been off screen
  for its Browser sleeps after or Project sleeps after time closes to free its memory and
  reloads when you select it again. Tabs playing sound, tabs where you typed
  text you have not sent, and the page another Space opens on stay awake. The side panel
  also keeps only the view on screen and the two you used last awake; with more views open,
  the oldest one sleeps, unless it is VS Code, Browser, Files, Kanban or Automate with its
  Auto Sleep Off: Off keeps that view's state (its page, or Code's terminals) in exchange for
  more memory. Keep views
  loaded (`keepViewsLoaded`) lifts that limit for every view, website and extension views
  included, and keeps the views of projects you leave loaded too. When the
  Code view sleeps, its editor restarts: open files and unsaved changes come back, but its
  terminals start fresh. Resources in the
  header's ⋯ menu sleeps many at once and shows CPU and RAM per session. Clean RAM
  copies a diagnosis prompt; paste it into an agent session to reduce RAM use.
  Sleeping sidebar sessions keep their normal title color and show a dimmer
  last-active time on the right; turn on Settings > Sidebar & Sessions > Sleeping Sessions > Dim sleeping
  sessions to fade the whole row instead (`dimSleepingSessions`). Use `ghostex sleep|wake <selector>` to
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
  search app commands, pane actions, and project actions. The lightning button
  at the bottom right of the sidebar opens the same thing, and the gear beside
  it opens Settings in one click.
  Previous Sessions in More Options lists past conversations from every agent
  CLI with resume and fork. History in a project's menu (its ⋯ button or a
  right-click on the project) opens Quick Access > Sessions with that project
  selected and Closed active, ready to search sessions you closed. Enter or a
  click on a closed session brings it back and keeps Quick Access open, so you
  can bring back several in a row; each one leaves the list.
  From a terminal, `ghostex previous-sessions [--query text] --json` lists closed
  sessions and `ghostex restore-previous-session --project-id <id> --session-id <id>`
  brings one back.
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
- Handoff / Export (the chat's ⋯ menu, a session's right-click menu, or picking
  another agent's model in the model picker) writes the conversation to a
  Markdown file and offers three choices. **Handoff to an agent** starts a new
  session in the same project with a link to that file typed into its chat box,
  unsent, so you write the first message. **Handoff to Remote** does the same on
  another connected computer: pick the agent, the Computer and the Project there.
  The project with the same Git repository is picked for you, and every other
  project there is listed too. If that computer doesn't have the repository yet,
  Clone it there opens Add Project on that computer with the repository already
  filled in, so you only pick the folder; once it is cloned, Handoff / Export
  comes back with the new project picked. When the two checkouts are on
  different branches, the dialog says so: only the conversation moves, the
  branch over there is not switched. It works in every direction, from this
  computer to a remote one, back, and between two remote ones; the old session
  keeps running. **Export to Markdown** only saves the file and copies its path.
  Commands & output, File changes and Reasoning choose what the file includes.

Related settings: `autoSleep*`, `keepViewsLoaded`, `clickToWakeSleepingSessions`,
`dimSleepingSessions`, `wakeSleepingSessionsOnSelect`,
`showSessionIdInTerminalPanes`, `sessionTitleGenerationAgent`,
`renameSessionOnDoubleClick`.

## Session Chat

Session Chat renders the same agent session as a chat GUI: composer with
image paste and Ctrl+G rich prompt editor, a prompt queue that sends when the
agent stops, transcript with thinking, tool, and edit cards, subagent
transcripts, question and approval cards, rewind, and a note per session (with
the Session notes extension on).
Type `@` in the chat box to mention a project file, `$` to pick one of the
agent's skills, and `+` (at the start or after a space) to mention another agent
session of the same project: the list shows each session's logo, title and
whether it is working, newest first, and narrows as you type. A picked session
goes in as a mention, `[<title>](<Global Ref>)`, which the agent can pass to
`ghostex agents send`. The `$` list holds the skills that agent's own CLI loads: its
home skills folders (Codex, Pi, Cursor, Gemini CLI and GitHub Copilot also read
`~/.agents/skills`, and Cursor lists its own built-in skills too) and the skills folders inside the session's project. A picked skill shows as a pill in the way that agent invokes
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
Click a picture in the chat (a thumbnail, or a picture in a reply) to open it
full size. The viewer goes through every picture in the chat, in order: on the
computer press the Left and Right arrow keys, or move the pointer near the left
or right edge for a round previous or next button; on the phone swipe sideways
(when not zoomed in) or tap the buttons at the sides. A counter such as 3 / 6
shows where you are, each picture opens fitted to the window, and Escape (or a
click beside the picture) closes it.
Dictate is the microphone button in the chat box (always shown, right next to
Send) and on the terminal bar, on a Mac, Windows and Linux: press it, talk, and press it
again to stop. In the chat session you are in, Option+Ctrl+D (Ctrl+Alt+Shift+D on
Windows and Linux) does the same as the chat box's microphone. What you said goes into the chat box (or is typed into that
session's terminal) and is never sent by itself. Dictate records first and
transcribes second: your voice is saved as a recording while you talk, and only
after you stop is it turned into text with xAI's speech-to-text, or OpenAI's
when only an OpenAI key is saved, so it needs an xAI or OpenAI API key in
Settings > Extensions > Voice agent (the key stays on this computer; xAI is used
when both are saved). A problem with the provider or the connection never loses
what you said: when the transcription fails, a small card above the microphone
says what went wrong and what to do (no key, a refused key, no internet, too
many requests, credit or plan usage, audio the provider couldn't read), with Add
key or Check key, which opens that key's field in Settings, and Transcribe
again; hover its sentence for the provider's own words when it gave any. In
the chat box, a small pill right above the microphone shows what Dictate is
doing: the recording time while you talk, then Transcribing. Its ✕ cancels, and
so does Esc. A cancelled recording is still
kept: rest the pointer on the microphone (or right-click it) and choose
Transcribe previous to type the newest recording that has no text yet into the
chat box. On the terminal bar, press the microphone again while it transcribes
to cancel, and right-click it for Transcribe previous. Every recording is listed
in Settings > Extensions > Voice agent > Dictation history. On an Android phone, Dictate is the microphone right next to Send in
the chat box, which stays there even when other buttons move into More actions; it
uses the key on the computer the chat lives on, keeps the recording on the phone
until that computer has it, shows Recording, Transcribing and Transcribe again
in a row above the chat box, and a long press on the microphone
offers Transcribe previous.
Read aloud speaks the agent's replies, so you can follow a session while you
drive or walk. It is experimental and off by default: turn on Settings > About
> Experimental > Enable Experimental Features, then switch on Read
aloud in Settings > Extensions (Features). On the computer, hover an
agent's final reply and press Read aloud (the speaker, last before the time); on
the reply being read it becomes Stop reading. Turn on Auto-read in the chat's
More actions (⋯) menu to hear each finished reply of the chat you are looking
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
notification shows it, with Pause (Resume) and Stop. If the phone cannot reach
the computer for 5 minutes, Auto-read turns itself off and says so; if Ghostex
on that computer is too old to stay connected in the background, the phone asks
you to update it there (`showBetaFeatures`, `readAloudHidden`, `readAloudVoice`,
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
Hover a message to show its actions and the time it was sent in a row below
it: Copy message, Reply by Annotating, and Save to md under an agent's final
reply; Rewind to here, Save prompt, and Copy message under your own messages.
Select words in the chat and right-click them for Copy, Add to Chat (quotes
them into the chat box) and Reply, which opens a small box for your answer:
Add to prompt (Enter) adds the quote with your reply under it to the end of the
chat box, and Send reply (Cmd+Enter, Ctrl+Enter on Windows and Linux) also
sends it, as Send would. Shift+Enter starts a new line and Escape closes it.
On the phone there is no hover. Hold a message's words to select them (drag
the handles to change the selection); the menu that appears offers Copy, Add to
Chat, which quotes the selected words into the chat box, and More, which opens
the message's menu with Copy, Add to Chat and Reply for the whole message and the same
actions. A selection stays inside one paragraph, list item, table cell or code
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
Type `/` in the chat box to browse the agent's built-in commands. In Cursor
chats, `/compact` summarizes the conversation to reduce context, just like
`/summarize`.
In Claude Code, Codex, and OpenCode chats, start a message with `!` to run a shell command
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
message. Click Return keyboard to chat on the card to type in the chat box again. On
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
rewind. Install its hooks in Settings > Agents, then open a new OpenCode session
to connect it.
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
menu lists Copilot's own commands, and `$` lists your Copilot skills, which run
as `/skill-name`. Sleeping and waking a Copilot session, Full Reload and Fork
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
beside the value on desktop and web; an all-digit number such as #1234 is not a color and stays plain text (or links to the project's GitHub issue). Copying keeps the original text.
Mermaid diagrams an agent writes (a ```mermaid block) are drawn as diagrams in
the desktop and mobile chats once the block is complete. Source switches to the
diagram's text and Copy copies it. On desktop, the expand button opens a larger
view you can zoom and pan. The web chat shows the diagram's source.

Agents can show charts, tables, stat tiles, and short text layouts right in the
chat, drawn in the chat's own colors on desktop, web, and mobile. For things a
chart can't show, such as a UI mockup or a small interactive tool, the agent
writes an HTML page and the chat shows a card for it with a small preview
picture of the page, the same on desktop, web, and mobile. The picture is taken
the first time you open the page in the Ghostex desktop app (and again after you
answer it); until then the card shows the page's file name. On desktop the card opens
the page where Settings > Chat > Open chat pages in says: Popup (the default), a
floating window over the chat; Files, a page tab in the Files view, whose header
can open it in your browser or close it; or System browser, your default web
browser. Your answers reach the agent the same way from all three, and the
picture viewer and the remembered scroll position below work in all three. In the
floating window, clicking outside it once only
arms it ("Click outside again to close"); a second click outside within two
seconds closes it, as do Escape and its close button. Its header can open the
page in your browser instead. Clicking a picture in the page opens it in a
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
When the agent needs your decisions, it asks them as a form right in the chat
on desktop, web, and mobile: every question at once, with choices you click
and a note box for each. Nothing is picked for you (the agent may mark the
option it recommends), every choice question has an Other / none of these
choice, and you can leave any question blank or click a choice again to clear
it. A question you leave blank is sent to the agent as skipped, so it moves on
without asking again, and Skip all (or Ctrl+Shift+Enter, Cmd+Shift+Enter on
macOS) sends right away with every question marked skipped, even ones you already picked. A
form with more than one question also ends with an optional notes box
for anything else you want the agent to know. Send (or Ctrl+Enter, Cmd+Enter
on macOS, from anywhere in the form or page) delivers your answers to the agent as a message (when the agent is
busy, they wait in the chat's queue and go as soon as it is free), the chat
shows them as a card, and the form then reads Answered. Questions that need a picture,
such as choosing between two mockups, come as a page instead, where you can
also pick an option right at its mockup with the Pick button at the right end of its title. A
page works the same way: its card reads Answered once you send, and opening it
again shows what you sent. The desktop's floating page window and the mobile page close once your answers are
sent, and stay open with the error if they could not be; in the Files view or your browser the page stays open. Until you send,
what you picked and typed is kept, in a form and on a page alike, even when
you close the page or the chat, switch sessions, or restart. A small "?"
button sits on the working card just above the chat box, left of the other
pills (after Scroll to bottom when that shows), while a form or
question page in the chat is still waiting for your answers (desktop, web and
mobile): each
click goes to the next older unanswered one, round again after the oldest, and
briefly outlines it. A question stops counting once the agent asks the same tag
again in a later form or page, or withdraws it. Question tags the agent writes
in its messages, such as Q3, are always links to the form or page that asked
them, answered or not, drawn as small outlined tags that show where each
question stands: amber with a ? while it still waits for your answer, green
with a check once you answered it, grey once you skipped it, and grey and
struck through once the agent withdrew it. When you answer a question another
way, by telling the agent in the chat or by doing what it asked, the agent
withdraws it so it stops counting as open. Settings: `chatPageOpenTarget` (`popup`,
`files` or `system-browser`) and
(under the page's Advanced fold) `chatSendOnPick`, which makes picking an answer send a
card that asks a single choice question right away, without Send answers
(off by default; cards with several questions, multiple choice, Other or text
answers still wait for Send). Commands:
`ghostex show <file.html>`
publishes a page and prints the block that shows its card, which opens where
Open chat pages in says, and
`ghostex visual check <file.json>` draws a chart block to an image, or lists a
question form's questions, so the agent can check it before replying;
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
Send and choose Interrupt & Send; on the phone, choose Interrupt & Send in More
actions. It is offered while the agent is working; Claude Code stops its turn and
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
Code and Grok Build, Back to composer (the arrow left of the play button, also in the message's hover
actions and in the menu you get by holding the message on the phone) takes it out of the agent's queue
and adds its text and pictures to your chat draft instead, unsent; pressing Up in an empty chat box does
the same for the newest queued message, like Up in the agent's own input. Prompts you explicitly
queue stay in the list above the input. This also applies when reopening the chat
or continuing on another device. A message that has waited about a minute says why under it, for
example that the agent is still working, is waiting for your answer, or that its input box can't
be read yet; in that last case Ghostex keeps trying on its own and, if it still can't deliver after
a few minutes, marks the message Not sent with Retry.
While the agent works, a one-line card just above the chat box says what it is
doing: the running tool's description (or its command) while a tool runs, and a
working word such as "Considering…" between tools. The conversation ends above the
card, so nothing in the chat ever scrolls under it; at the bottom of the chat it
reads as the last thing in it. When the turn ends the card reads "Worked for 2m 13s"
until you send your next message, and that turn's fold then reads "Previous turn"
beside its counts, so the time appears once. The agent's subagents, its task list,
a coordinator's threads and the commands still running in the background are
small pills on that card (Workers, Tasks, Subagents, Background), each with a
count, also while the agent is idle; the Scroll to bottom and "?" buttons and an
armed Delayed Send sit to their left. A long label is cut with "…" so the pills
keep their full size; hover it to read all of it (on the phone, press and hold the
card). The card itself does not open: click or tap a pill to open its list in a
menu (a sheet on the phone) that opens upward from the chat box's top edge, lined
up with the pill you clicked: its right edge on the pill's, or its left edge for a
pill in the left half of the box, shifted to stay inside the box. The Background list has each running command with its
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
Approval and dialog cards (Claude Code, Codex and the other agents) list the
agent's choices as numbered rows: press a row's number or click it. Cmd+Enter
(Ctrl+Enter on Windows and Linux) picks the row highlighted in the terminal and
Esc picks No, or closes a dialog that has no No row. Show terminal output opens
what the agent printed above its question. In narrow chats, notice cards hide
Show terminal output; Open terminal remains available.
Ghostex trusts each project and worktree folder for you before it starts an
agent there, so sessions never stop at the agent's own "Do you trust this
folder?" question. It marks the folder trusted the way the agent would if you
answered yes (Claude Code, Codex, Cursor, Gemini, Copilot, Grok Build,
Antigravity, Pi and Factory Droid have such a question; OMP, OpenCode and Hermes
don't). Only the folder the session runs in is trusted, never your home folder or
a folder above it. To answer the question yourself, turn off **Trust project
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
Chat follows the app theme by default. In Settings > Theme > Advanced, set Chat theme to Light, Dark, or System for a separate appearance, or choose Follow app to use the main Appearance (`sessionChatTheme`, `sidebarTheme`).

Set Default chat zoom (%) in Settings > Chat to scale the desktop chat interface, including messages, controls, and the prompt composer. Choose 70% to 200% in 5% steps; the initial default is 100%. The saved level applies to every open chat and when chats open again (`sessionChatZoomPercent`). With a chat focused, Cmd+= and Cmd+- (Ctrl on Windows and Linux) step this same level, so every chat changes size together and the new level is saved; Cmd+0 sets it back to 100%.

To search a chat, press Cmd+F (Ctrl+F on Windows and Linux) with the chat focused, including while typing in the chat box. Only what you can see is searched: text inside a folded turn, tool group, thinking or collapsed card, or the hidden part of a long message behind Show more, is skipped until you open it. Every matching word is highlighted, the current one more strongly, and the counter shows which match you are on; Enter or the down arrow goes to the next match, Shift+Enter or the up arrow to the previous one, and the chat scrolls to it. Escape or the close button ends the search and returns you to the chat box. On the phone, Search Conversation in the session's menu opens the same search.

Toggle chat and terminal for a session with one click on the pane header or
the pane hotkey. Compatible agents can default to chat. On macOS and Linux,
Ghostex can release unused terminal viewers for persistent sessions and load them
again when needed. The agent keeps running while you use Chat or another project;
returning to Terminal reconnects to the same running session. This does not sleep
the agent.
When an assistant message
has tool calls, click its text or the chevron beside it to expand the tools directly
under that message. Its full text and formatting stay visible when collapsed;
links and code controls keep their own actions. A chat at the Verbose or
Detailed level (More actions > View) opens these tools by default, along with
each finished turn's "Worked for" log; close
one with a click and it stays closed. A tool call the agent described (for
example "Search for config files") is named by that description in every mode,
in the reply's own font and a slightly dimmer colour, instead of the tool's name
and its command; click the row to see the full command and its result. File writes and code
edits appear outside the tool groups while the agent works. As soon as the agent
finishes a turn, its tool work folds away under "Worked for Xs" above the final
reply (open by default at the Detailed and Verbose levels); click that
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

More actions > View picks the chat's detail level, one of Simple, Detailed and
Verbose, and holds the Summary switch. The View row names the level that is on,
and adds "Summary" while Summary mode is on.

Simple is the level new chats start at. Tool groups
without a message above them collapse to a tool-call count, tool rows hide
command previews, and the work under each reply stays folded until you open it;
expand a tool to inspect its full input and result. File edits
collapse under "Edited 1 file" or "Edited X files", counting each path once; expand
the row to see the usual file and diff cards. Detailed shows the tool calls and
file edits under each reply, folded; Verbose opens them. The level is kept per
chat (the `simple` and `verbose` records in the chat's own storage). The settings
keys `sessionChatSimpleMode` (on by default) and `sessionChatVerboseMode` only say
which level a chat starts at and have no Settings row. Summary mode shows the
Simple look while it is on; turning it off gives the chat its picked level back,
and picking a level while Summary is on turns Summary off. The phone app has
Simple mode too, under More actions > View in its chat box or in its Settings >
Chat; the phone keeps its own switch, also on by default.

While the agent is working, its thinking shows as a closed "Thinking…" card with the newest finished line beside it, so it never pushes the rest of the chat away; click (or tap) the card to read the whole thought, which keeps the newest lines in view as they arrive, and click again to close it. The commands the thought runs stay in view below the card, and when the turn ends everything folds into its "Previous turn" card as before. This works the same for every agent, on the computer, the web and the phone.

Show thinking in grey (Settings > Chat, off by default) shows the agent's thinking in grey and its replies in the full text colour, so the two are easy to tell apart (`sessionChatMutedThinking`); the phone has the same switch in its Settings > Chat.

Tool call cards (Settings > Chat, a preview, off by default) show each run of tool calls between two pieces of the agent's text as one card: its header stacks the tools' icons and says "Using 3 tools" while they run and "Used 6 tools" with any failures and the time once done, and each call reads like a sentence that changes tense when it ends ("Reading main.rs", then "Read main.rs · 92 lines · 30ms"), with its result and time on the right. A failed call says what failed with the reason underneath, and calls to apps such as Linear, GitHub or Notion show the app's logo. The card folds to one line as soon as the agent writes again; click (or tap) it to open it again, and click a call to see its command and output (`chatToolCallCards`). The phone has its own switch for it in its Settings > Chat.

Marked handovers: coordinators and their threads begin each part of
the message they hand back with a marker: ❓ Needs you, ✅ Done, ⚠️ Read this,
🛫 Kicked off (work started this turn), 🔄 In progress, 🧪 Untested, 🚀 Shipped or 🛑 Blocked, so a quick skim shows
what needs you, and every bullet under a marker starts with a capital letter; a thread's report to its coordinator is marked too. Turn on
Marked handovers in every session (Settings > Chat, off by default) to have
every other agent session do the same. The agent otherwise writes as usual. It
reaches Claude Code, Codex and ZCode (and Empryo coordinators). A change applies
to sessions started or resumed afterwards; a running agent keeps the
instructions it started with until it is resumed. The chat colours each marked section by
its kind, in the style picked under Marked sections style right below the
switch (shown while it is on): coloured labels (the default), a thin left rule, or a coloured
badge naming the kind; the eye button next to it previews all three side by
side. Show marker emojis, under it and off by default (an advanced setting, not recommended), adds the marker emoji
to every style; while it is off the colour and the label show the kind.
Copying a message still gives the agent's own text. The phone app sets both in
its own Settings > Chat > Marked sections, which also offers Plain text.
Heading colour strength (Settings > Chat > Marked handovers, under the style, an advanced setting, 0% to 100%, default 100%) sets how strongly the coloured headings stand out: lower values fade them toward the normal text colour, and 0% leaves plain bold text; the phone follows the computer's value.
Coordinators answer without markers (Settings > Chat, an advanced setting, off by default and not recommended) makes coordinators started or resumed afterwards answer normally, while their threads still mark the reports they send them.
Settings: `markedHandovers`, `markedHandoversStyle` (`labels`, `rule`,
`badge`), `markedHandoversEmoji`, `markedHandoversHeadingStrength`, `orchestratorPlainReplies`.

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
and a click on the chat closes it. Its cards: Environment (this computer or the
box the session runs in, its folder, branch and account); Context (how full the
context window is, with Claude's own breakdown into messages, memory files,
skills, tools and free space; Ghostex asks a silent read-only copy of the
session, so nothing is typed into it and no tokens are spent, and it counts
again after a turn ends at most every ten minutes, or when you press the
refresh button); Tasks (Claude's task list); Connectors (your workspace's
connected apps and every MCP server the session loaded, green when ready, amber
while connecting or needing a sign-in; Manage opens Settings > Connectors);
Skills (the ones the session used, then all of them; click one to open it in
Agents Hub); Memory files (the CLAUDE.md and AGENTS.md files Claude loaded;
click to open); Changes (lines changed on the branch against main, uncommitted
edits included, with Commit, Create PR or View PR); and Background tasks (each
running command with its time and a stop button, then the finished ones). A
card's title folds it. Claude task list (Settings > Chat, shown while the
extension is on, on by default) starts Claude with its task list tools, which
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

Summary mode folds each turn down to your prompt and an "Agent reply" row; the
newest reply stays open, and older ones open with a click. The row holds every
reply the agent gave to that prompt, including the ones it wrote after a
background task finished. While the agent is still working, that turn's work
shows open under Active work, with the running command visible; close it with a
click, and it folds into the reply once the turn finishes. Any message that holds
an HTML page, a question form or a chart is always shown in full under its prompt,
never folded, so you can scroll up through every page. Messages to and from other
agents (a coordinator's thread reports, messages from other sessions) open no
turn of their own: they fold into the "Agent reply" of your own prompt, so the
summary reads as your conversation.
Turn Summary mode on or off under More actions > View, where its row shows the
Toggle Summary Mode shortcut (Option+Ctrl+S on macOS, Ctrl+Alt+Shift+S on Windows
and Linux), which switches it from anywhere in that chat and can be changed in
Settings > Hotkeys.
Stash prompt is in More actions, second from the bottom: it stashes what is in
the chat box, or, with an empty chat box, opens your stashed prompts; the row
shows how many are stashed.
The Maximize button's tooltip shows the Maximize Chat Box shortcut (Option+Ctrl+M
on macOS, Ctrl+Alt+Shift+M on Windows and Linux), which maximizes the chat box of
the chat you are in, or restores it. The Prompt Editor shortcut (Ctrl+G on macOS,
Ctrl+Shift+G on Windows and Linux) does the same while you are in a chat: it
maximizes the chat box with your text in it, and pressing it again restores it.
As space gets tighter, toolbar buttons move into More actions one at a time:
Session note, Attach, Maximize, then Terminal View. The microphone never moves:
it always stays right next to Send.
If the context ring still does not fit beside the model, it moves into Model
settings at the top of More actions; the model pill shortens instead of moving.
Controls return as space opens up; More actions, Dictate and Send or Stop stay visible.
On the phone, More actions > Stash prompt with an empty chat box opens
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
always sits at the top of that agent's list. Older versions of a model (GPT 6 Sol
once GPT 6.1 Sol is out, for example) sit under Legacy models at the bottom of
the tab; click it to show them, or star one to keep it at the top. Codex,
Cursor, Grok Build and Antigravity's lists come from the CLI installed on that computer, so
new models show up without a Ghostex update. Click a row's star to keep that
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
leaves the default alone (either one closes the picker, as does clicking a model), and Option+1 to Option+9
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

Settings > Accounts saves Claude and Codex logins. The left side lists each
provider's accounts with a usage bar (or a Reconnect button when a login stopped
working) and the provider's rules; click one to see it on the right: its usage
bars with reset times (labelled 5h, 7d and Fable), Enabled, Name, Auto-switch (whether Ghostex may move sessions to
it at a limit, shown as an Auto-switch on or off tag), Show usage in the sidebar,
and under Advanced the Session badge (up to two letters or numbers on its
session icons) and its Account number, which you can swap with another
account's. A Claude account also shows **claude.ai in Ghostex**: whether its own
Browser profile is signed in to claude.ai for its cloud sessions and the Claude
view, with **Sign in to claude.ai** or **Sign out** (see Claude cloud
sessions). Changes save as you
make them; Sign in again and Remove sit at the
bottom, and Remove asks first and says how many sessions use the account.
Turn Enabled off to keep an account you no longer use (for example one whose
subscription ended) saved but out of use: it shows dimmed with a Disabled tag,
Ghostex never starts new sessions on it, switches to it at a limit or offers it
in Switch Account menus, stops reading its usage and sign-in state (so it never
asks you to reconnect it), and drops its usage from the sidebar. Sessions already
on it keep running, and the account says how many still use it. A sleeping or
restored session on it moves to another account when it wakes (your account for
new sessions when it works, otherwise the Auto-switch account with the most
room) and carries on the same conversation; with no account to move to, the wake
says the account is turned off. Turn Enabled on to use it again. This is separate from "Subscription access disabled", which
Ghostex sets on its own when Claude refuses an account.
Reconnect is one click: your browser opens on the provider's sign-in page with
the account's email filled in, Settings shows "Waiting for you to sign in…" (a
box for a sign-in code appears only if the browser shows one, and the sign-in
log stays under Details), and the account shows Connected with fresh usage when
you finish. If the browser signed in to another account, Settings names it and
offers Try again or adding it as a new account. To add an account, click + next
to Claude or Codex and choose Sign in with browser; a Claude account needs no
email (pick it in the browser), while Codex asks for the new account's email
first. The first account of a provider also asks to share chat history between
your accounts, which lets a conversation carry on with another account.
Each Codex login is its own account with its own name and settings, also when
two people sign in to the same ChatGPT Business workspace or one person has
several workspaces; this needs Codex Swap 0.3.2 or later (its Update button sits
under the account list in Settings > Accounts), and accounts saved earlier keep
their names, settings and sessions.
Quick launch, the main launcher row, and any session started without
picking an account use New sessions start on in each provider's rules (Claude
rules or Codex rules in the list). The automatic rules only consider accounts
with Auto-switch on: Auto (the default) weighs remaining limit against time to reset and
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
Under the account list, Settings > Accounts shows one line per account helper,
Claude Swap (cswap) and Codex Swap (xswap), with its version and an Update button
when a newer release is out (otherwise Check for updates). Its ⋯ menu holds
Reinstall and Uninstall, which asks first and keeps saved logins and shared
conversations. Ghostex runs the update with the tool that installed the helper
(uv or pipx for cswap; Homebrew, Cargo or the Windows installer for xswap); for
a helper installed some other way the buttons are off, so update it the way you
installed it.
On Windows, Ghostex installs its own build of Claude Swap, because the released
one can't share conversation history between accounts there yet. Install and
Update in Settings > Accounts use it, and Ghostex moves any other Claude Swap on
the computer to it on its own. Running Claude sessions keep the installed Claude
Swap in use, so an update, reinstall or uninstall is prepared at once and applied
on its own as soon as they have all closed (after a restart of the computer at the
latest); until then the Claude Swap line says so, offers no buttons, and new
sessions keep working.
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
agent couldn't start" with the terminal's last lines, Try again and Open terminal,
instead of waiting at starting. A message you sent waits in the chat and goes out
once the agent is up.
The same works from the command line, for you or an agent: `ghostex accounts`
lists every Claude and Codex account with its state (ready, limit reached, needs
sign-in), its usage per window with reset times, the default for new sessions
and the sessions on it; `ghostex agents switch-account <session> --account
<number or email>` moves a session to another account and resumes its
conversation there, also when it could not start, and refuses an account that
needs signing in (and a session in the middle of a turn, unless `--force` ends
that turn and sends "." afterwards so it carries on); `ghostex agents reload <session>` stops a stuck session's
agent and starts it again on the same conversation, losing what its current turn
was doing. A coordinator uses them when a thread can't start or gets stuck,
and tells you what it did.
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
default) moves the session to another account with Auto-switch on, Wait for
reset picks up on the same account when its usage resets, and Do nothing leaves
the session stopped. With Switch accounts, Try first picks which account Ghostex
tries first. Every Claude and Codex session follows those settings as they are now,
including sessions that are already open, forks, and restored sessions, so a
change there applies everywhere at once. To make one session behave differently,
open More actions > Switch Account and click Customize under Keep going at a
limit; Use session defaults returns it to the Settings values.
When a Claude or Codex turn ends on a lost connection or a server error (such as
"API Error: Can't reach the API server"), the chat shows it as a card and
Ghostex resumes the turn by itself once the network is back, after 5s, 15s, 1m,
2m and 5m, then on the slower schedule; the card's Retry now and Stop retrying
buttons override it, and Retry after temporary errors in Settings > Accounts
turns it off.
A sleeping session whose account is already at its limit wakes on another
account that has room. When every account with Auto-switch on is full, the
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
instead, such as "The saved login expired. Reconnect this account."
A newly added account starts with Auto-switch on; turn it off on the account in
Settings > Accounts to keep that account out of automatic switching.
When Claude says "Your organization has disabled Claude subscription access",
that account cannot be used any more: Ghostex turns its Auto-switch off (the
account shows "Subscription access disabled"), and the next message you send to
a session on it first moves the session to the best account with Auto-switch on,
resuming the same conversation. When the rules say Switch accounts or Wait for
reset, Ghostex moves the session by itself and continues the message that
failed. Turn Auto-switch back on, reconnect the account, or pick it for a
session when its access works again.
When a session's login stops working mid-conversation ("Login expired · Please
run /login"), the chat's sign-in card offers Reconnect beside Switch account.
For a session on a saved account it opens the same browser sign-in as that
account's Reconnect in Settings (or skips it when the account's saved login
has already worked again since the error), and when you finish, the session (and any
other session on that account showing the same card) restarts on its own
conversation, so you don't retype anything. The account shows Needs sign-in in
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
slider that reads Auto until dragged), `sessionChatVerboseMode`,
`sessionChatFileEditPreviews`, `sessionChatMutedThinking`, `chatToolCallCards`,
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
can delete it. Command-click opens links;
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
selectors show the theme set in Ghostex's terminal config, including separate light
and dark selections when configured. A single theme is used for both appearances
unless you select a separate light palette. Without a configured theme, the defaults
are GitHub Light and GitHub Dark. All open terminals refresh automatically when their
app, system, or terminal theme changes, including idle terminals and terminals in
inactive projects. The appearance override and light palette apply to Ghostex only.

Terminal background (Settings, Terminal) is Black / white by default: pure black
behind dark terminals and pure white behind light ones. Choose Follow theme to use
the theme's background color instead, or Custom color to pick your own for dark mode.

Terminals are see-through only while the window is: set Transparency to Dark only
or Always (Settings, Theme, Transparency; it starts Off on Windows and Dark only on
macOS and Linux). Background opacity (Settings, Terminal) then sets how much of the
work area's tint terminals keep: 100% matches the rest of the work area, lower
shows more of your desktop through them. While transparency is off the row offers
Turn on transparency instead. The window's Blur (on Windows with Desktop and windows,
the Acrylic, Plain blur or Off choice) is set on the Theme page only (Settings > Theme >
Transparency). Ghostty's own `background-opacity`
and `background-blur` lines in the terminal config do not change Ghostex's panes.

On macOS, Custom shaders (experimental) runs the `custom-shader` files from
Ghostex's terminal config over its terminal panes, in the order the config lists them.
Turn on Enable Experimental Features (Settings > About > Experimental) to see it at the bottom of
the Terminal section. While it is on, terminals use the background color and
opacity from Ghostex's terminal config, so Terminal background and its color are greyed
out. Shader animation follows Ghostty's `custom-shader-animation`. Turn the switch
off to go back to normal rendering; running sessions keep going. If a shader file
does not compile, terminals keep their normal look. Windows and Linux are not
supported yet.

Terminal links (`ghostex://terminal`) without a folder open in the active local
project. A folder supplied in the link takes precedence.

Related settings: `terminalFontFamily`, `terminalFontSize`,
`terminalGhosttyTheme`, `terminalColorScheme`, `terminalGhosttyLightTheme`,
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
Windows). OpenCode's official installer on Windows runs its install script with
Git for Windows' Bash, so it needs Git for Windows; without Git, install
OpenCode with mise or npm instead. When an installer leaves its folder off PATH (Claude Code's does), Ghostex
adds it to your user PATH so new terminals find the command; Add to PATH
does the same for a CLI installed earlier. Installs run one at a time and wait
their turn. Ghostex updates a CLI the way it was installed; when it cannot tell,
it uses the CLI's own updater (such as `copilot update`, `pi update self` or
`omp update`), and only a CLI without one asks you to choose the original
installation method. Hermes Agent is checked with its own `hermes update --check`,
so Update available is shown when Hermes is behind even if its version number has
not changed. When mise's `minimum_release_age` setting holds a newer release back,
the CLI shows that release as held back by mise and the date it becomes eligible
(when an older release is already eligible, the update installs that one);
Ghostex leaves that setting alone. If a CLI needs a newer tool first (OMP's Bun
install needs Bun 1.3.14 or newer), the method says what to update; the official
installer downloads OMP's own binary and needs no Bun. mise is offered for supported CLIs and is the
default install choice when it can install that CLI on your computer; otherwise
(Cursor on Windows, or a computer without mise) the official installer or npm
is. On a computer without mise the mise choice reads "installs mise first":
Ghostex downloads mise from its official GitHub releases into its own tools
folder (checked against the published checksum) without changing your shell
profile, and when npm is missing it has mise install Node.js along with the
CLI. A CLI installed through mise starts in its sessions even when your shell
doesn't set mise up (no `mise activate` needed): Ghostex runs it through mise.
If a session's agent command still isn't found, its chat says which command is
missing and offers Install. When Chat is your default view, installing a CLI
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
DeepSeek Harness (under More agents until you turn it on) launches with
`dsh-tui` and installs and updates only through mise (which Ghostex downloads
itself when the computer has none): Install and Update run
`mise use --global npm:@deepseek-harness-tui/dsh-tui@latest npm:@deepseek-ai/dsh@<release>`
with the DeepSeek Harness release npm currently marks as latest (plus pnpm when it
is missing), then `dsh-tui update`, which sets up its profile, so the terminal app
and its profile stay on the same release. A copy installed another way
updates with `dsh-tui update`. It needs a DeepSeek API key (`DEEPSEEK_API_KEY`)
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
Copy > Copy Details; its Global Ref is the most precise), or the title. The same
Copy menu also has Copy ID, which copies a mention of the session,
`[<session title>](<Global Ref>)`, to paste into another chat (it shows there
as the session's pill), and copies the session's branch, its Linear issue ID and
link, and its pull request or GitHub issue link when it has them.
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
   to `create-agent` or `board start-work` (Claude, Codex, Pi, Empryo and
   Antigravity CLI; for Pi and Empryo the model is `<provider>/<model>`, as in
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
  Coordinator…** at its top (tap its (i) for the same). Name it, pick its agent
  from the Agent dropdown (Claude, Codex, ZCode, Cursor, Grok, Copilot,
  Antigravity, Pi, OMP or OpenCode, whichever you have set up), its model where the agent
  lets Ghostex choose one (Opus 5.5 on Claude; otherwise each agent starts on its
  own default) and its effort: Medium, High or XHigh, High by default (an agent without
  XHigh offers its levels from Medium up to its highest). Use a very good model: the coordinator
  is the agent you talk to all day, and the main way Ghostex itself is built.
  Its first message is always "Read and adhere to this skill:
  /ghostex-orchestrator" (`$ghostex-orchestrator` on agents other than Claude
  Code and Copilot), shown locked in the dialog: Ghostex installs the
  ghostex-orchestrator skill for that agent, and the skill is the coordinator's
  playbook. Then it opens in chat, where you write your first request. The name
  is what its sidebar row shows, and the coordinator keeps it: unlike other
  sessions it is not renamed from your first message (Rename in the sidebar
  still changes it; left blank, it is named once from its first conversation).
  A project can have several coordinators, one per stream of work. In an
  coordinator's chat the effort picker offers the same Medium, High and XHigh only.
  To pick from every level the agent has, turn on **Every effort level in coordinator pickers**
  (an advanced setting in Settings > Chat); anything below Medium
  isn't recommended for a coordinator. From a terminal: `ghostex coordinator
  create [--agent <id>] [--goal <text>] [--task <first request>]`.
- **Turn a session into a coordinator**: right-click a session of one of
  those agents in the sidebar, open **Advanced** and choose **Make
  Coordinator** (optionally with a goal); on the phone it is in the session's
  menu too. The session keeps its conversation and is never restarted or
  interrupted: it gets the crown right away, Ghostex installs the
  ghostex-orchestrator skill for its agent, and a message telling it to read
  the skill waits in its chat queue until its current turn is over. Sessions
  it started before are not its threads yet; ask it to adopt them. A thread of
  another coordinator, a session in a box, or a draft cannot be made an
  coordinator.
- **Threads in the sidebar**: a coordinator's row shows a crown in
  place of its agent's logo, and a crew icon
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
  own) overlapping in place of its time, like a computer tab's dots, so a thread
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
  The phone's session list shows the same
  tree: crown, crew count, threads indented under their coordinator, and the
  chevron to fold them.
- **Finished threads are closed**: once the coordinator has checked and
  committed a thread's work and expects nothing more from it, it marks the
  thread done and closes its session, so the sidebar keeps only work in flight
  (it never closes a thread that is still working or waiting on you). A closed
  thread is not lost: when a follow-up comes, the coordinator reopens it or
  messages it, and the same conversation resumes under the coordinator with
  everything it knew. Ask the coordinator to keep a thread open if you want to
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
- **Reports come back by themselves**: when a thread finishes a turn, Ghostex
  sends its final message to the coordinator (a "Message from" card in its
  chat); when a thread waits on a question, an approval, or a screen such as
  folder trust or an expired login, the coordinator is told what it is asking.
  The coordinator then checks the work, commits it (only that thread's files,
  and it never pushes unless you ask), starts the next step, and tells you
  what needs you. Nobody has to poll. A thread reports only this way: Ghostex
  refuses a thread's messages to its own coordinator in the middle of a turn
  and tells it to end its turn with its report.
- **Work that never arrives is caught**: when the coordinator starts a thread,
  it waits until the thread has actually taken its brief before saying it is
  under way, and says "pending" with the reason when it has not yet. Ghostex
  keeps watching every brief and follow-up the coordinator sends: if a thread
  sits idle without it, the coordinator is told the message did not reach it
  and sends it again, so work handed out is never silently dropped.
- **Thread models**: the coordinator picks each Claude thread's model when it
  starts it: Opus 5.5 at high effort for substantial work, Opus 5.5 at medium
  for hard but small changes, Sonnet 5.5 at high for small contained fixes. It
  never switches a running thread's model (that throws away its prompt cache);
  a follow-up that needs a stronger model gets a new thread.
- **Worktrees**: threads that need the same files run one after another; when
  running them in parallel matters, the coordinator gives a thread its own git
  worktree and branch, so parallel threads never edit the same checkout. A coordinator's
  threads trust the project's own folder and the worktrees Ghostex makes for
  them, so they start without stopping at the agent's folder-trust question.
- **Goal, standing instructions and memory**: every thread's brief carries the
  coordinator's goal, its standing instructions (rules such as which branch to
  target or how to verify work) and its memory notes. When you state a lasting
  preference, the coordinator proposes the exact wording and saves it as a
  note every later thread receives once you confirm; ask it
  to change the goal or the instructions the same way. To edit them yourself,
  right-click the coordinator and choose **Goal & Instructions…** (the one-line
  goal and the standing instructions) or **Memory…** (every note, with a button
  to remove one and a field to add one); on the phone, long-press it. Save
  applies to threads it starts from then on; Cancel leaves everything as it was.
- **Coordinator playbook**: the playbook every coordinator follows (how it
  talks to you, starts and briefs threads, and reports back) is editable, so
  you can change how your coordinators work instead of telling each one.
  Edit it for all projects in **Settings > Chat > Coordinator playbook**
  (Edit…), and give a project its own in **Settings > Projects** (pick the
  project, then Coordinator playbook, Edit…); a project's playbook also covers
  all of its worktrees. The New Coordinator dialog says which playbook its
  coordinators follow, and its **Edit** button changes either one. The editor
  opens with the playbook in use; **Reset to Default** (bottom left, click it
  twice to confirm) goes back to Ghostex's playbook, or for a project to the
  one for all projects, and saving text that matches the default keeps no
  edited copy. The switch on each row turns an edited playbook off without
  losing it. Every coordinator, existing ones too, follows the change from
  its next guide read; `ghostex coordinator status` tells it when its
  playbook changed. When Ghostex ships a new playbook after you edited yours,
  the editor says so: Reset to Default takes the new one, or ask an agent to
  merge it into your copy. Each coordinator's own goal and standing
  instructions (Goal & Instructions…) stay separate. From a terminal:
  `ghostex coordinator playbook [--this-project | --project-id <id>] [--text |
  --default]` shows it, and `ghostex coordinator set-playbook (--body-file
  <path> | --reset | --off | --on) [--this-project | --project-id <id>]` changes
  it.
- **Just ask**: "run these three as separate threads", "use worktrees", "give me
  a status of every thread", "use a cheaper model for threads", "don't merge
  anything without asking" all work in plain words.
- **A session that is not a thread**: ask the coordinator for "a separate session I
  can chat to directly" and it creates one that sits in the sidebar on its own and
  reports to nobody; "unlink that thread" turns an existing thread into one.

Coordinators run on Claude, Codex, ZCode or Empryo; threads can be any configured
agent. An Empryo coordinator runs as the Empryo agent `ghostex-coordinator`: Ghostex
keeps its playbook in `~/.empryo/agents/ghostex-coordinator.md` and switches the
session to it with `/agent ghostex-coordinator` before anything else, so leave that
file in place. From a terminal or another agent: `ghostex coordinator create --title <name>
[--model <m>] [--effort <e>] [--goal <text>] [--task <first request>]`, `ghostex coordinator
promote <session> [--goal <text>]` (make an existing session a coordinator), `ghostex coordinator status`,
`ghostex coordinator options` (the agents, models and efforts a coordinator can use),
`ghostex coordinator start-thread --title <title> --task <brief> [--worktree]
[--agent <id>] [--model <m>] [--effort <e>]`, `ghostex coordinator resolve
<thread> [--keep-open]` (mark done and close its session), `ghostex coordinator
reopen <thread>` (resume a closed thread), `ghostex coordinator unlink <thread>` (take a
thread out of the coordinator's tree: it stays running as an ordinary session of its
project, shown as its own sidebar row with no more reports to the coordinator; 10.24.0 or
later), `ghostex agents create <agent> --standalone` (run by a coordinator, create a session
for the user to chat to directly instead of one of its threads; 10.24.0 or later), `ghostex coordinator remember <note>`, `ghostex
coordinator set-goal|set-instructions`, `ghostex coordinator edit [--goal=<text>]
[--instructions=<text>] [--notes-json=<json list>]` (the goal, instructions and whole
note list in one call, as the editors save), and `ghostex coordinator guide` (the
coordinator's own playbook). `ghostex coordinator --help` lists every flag. `ghostex orchestrator …`
is an alias that runs the same commands.

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
- Ask a coordinator agent to "work the high-priority beads on the board" to
  have it pick cards and spawn workers.
- When the columns do not fit, the board scrolls sideways: swipe on a trackpad,
  Shift+wheel on a mouse, or drag the bar under the lanes (each lane has its
  own bar for its cards). Dragging a card to the board's left or right edge
  scrolls the board to the lanes off screen.
- **Open in New Window** (the pop-out control at the top right of the view, or
  the tab's right-click menu) moves the board to a window of its own, for a
  second monitor; the panel shows a card with **Show Window** (brings that
  window to the front) and **Bring Back**, and closing the window brings the
  board back too. The window follows the project you switch
  to, and after a restart the board is in the panel again.

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
popup thread doing"). While the
conversation is open it tells you when a session finishes its turn or starts
waiting on a question or an approval. The conversation keeps going while you
switch projects and chats. A small panel under the header shows what it is
doing, live captions of both sides and the tools it runs, with Mute, a small
talk button beside it and Hang up; while the agent speaks, a Stop talking pill
shows above the buttons (talking over it also stops it in a Regular call). Drag the panel by its top
bar to put it anywhere in the window (it stays there until you quit Ghostex),
and scroll it to read back the conversation. Click the voice name ("Grok ·
Eve") to pick another voice: Grok switches right away when it can, and GPT
Realtime keeps a call's voice once it has spoken, so the new voice starts with
your next conversation (the panel says which). The small speed button right of
the voice name ("1×") opens a menu of speeds (1×, 1.2×, 1.4×, 1.6×, 1.8×, 2×)
that make it talk faster without changing its pitch, at once, even
mid-sentence. Calls come in two modes, chosen in Settings > Extensions > Voice agent > Call
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
Regular stays the default. Mute turns your microphone off; in a push-to-talk
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
computer with its own voice key (add one in Settings > Extensions > Voice agent on that
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
Forget button in Settings > Extensions > Voice agent > Memory, deletes the summary for the
desktop and the phone. The summary is written by a small text model of the same
provider, with the same key, right after a conversation ends.

It needs an API key: pick xAI Grok Voice (the default) or OpenAI GPT Realtime
in Settings > Extensions > Voice agent, pick a voice, and paste the provider's key there
(xAI keys come from console.x.ai, OpenAI keys from platform.openai.com). The
key is checked with the provider before it is saved, kept encrypted on this
computer and never shown again. If a saved key can't be read anymore,
Settings > Extensions > Voice agent says so on its row: paste it again. The provider bills your account while a
conversation is open. When the provider refuses a conversation (for example
xAI answering "payment required" because an X subscription's included usage
is used up for the moment), the panel shows the provider's own reason.

Settings > Extensions > Voice agent > Dictation history lists every recording you made with
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

On an Android phone, the Voice button in the bar at the bottom of the session
list (and the headset at the top of a session) starts the same conversation with the voice agent on the
computer you are looking at. The key stays on that computer; the phone never
stores one. When the agent shows a session, the voice screen offers an
"Open <session>" button that opens its chat while the conversation keeps
going, and the computer's window switches to it too unless someone is typing
there. The conversation keeps going while you move around the app, switch
apps or lock the phone, with a "Voice mode on" notification that has Mute and
Hang up (in a push-to-talk call, Talk instead of Mute: tap it to start talking
and tap it again, now Send, to send), and it reconnects by itself after a
short network drop. It talks
through the phone's speaker, or a Bluetooth headset or car hands-free link
when one is connected, and pauses while a call or a navigation prompt plays.
The phone's voice screen has the same voice picker (tap the voice name under
the computer's name) and a small speed button beside it that opens the list of
speeds, the same gear for the call mode, and the same talk button between
Mute and Hang up: press and hold it to talk and let go to send, or slide your
finger off it ("Release to cancel") and let go to send nothing; slide back on
to keep talking. Stop talking is a pill above the buttons while the agent
speaks. The call mode is also in the phone's Settings > Voice agent. They save the computer's settings, so the desktop and the phone talk
with the same voice at the same speed and in the same mode. It shows
the same "Last conversation" card with Continue, Start fresh and Forget.

The voice agent is a built-in extension, on by default, and its settings page,
Voice agent, is the first page folded under Extensions at the bottom of the
Settings sidebar. Turn it off with its switch on Settings > Extensions (the Voice
agent card) and the headset and its arrow leave the top
of the window, the voice agent hotkeys stop working and leave Settings >
Hotkeys, a call that is running ends, and the phone app hides its headset,
Voice call about this chat and Voice Call for that computer. Dictate keeps
working: Settings > Extensions > Voice agent then shows only the xAI and OpenAI keys Dictate uses and
the Dictation history. Turn it back on and everything returns, with your voice
settings as you left them.

Settings: `voiceAgentHidden` (off by default, which keeps the voice agent on), `voiceAgentProvider`, `voiceAgentXaiVoice`, `voiceAgentOpenaiVoice`, `voiceAgentSpeed`, `voiceAgentCallMode`; the Memory row (`voiceAgentMemory`) forgets the remembered conversation. Hotkeys: Toggle Voice Agent, Voice Call About This Session, Hold to Talk (`holdToTalk`).

## Remote machines, web, and mobile

Ghostex is a client/server system: gxserver runs on each computer and owns its
sessions, so any client can control agents on any machine.

- **From a phone**: sidebar More Options > Mobile & Remote (Settings >
  Remote, which has four tabs: Easy Connect, Tailscale, Remote machines (the
  computers this one connects to) and Advanced; each tab shows its status, such
  as Running or how many machines are connected, and the page reopens on the
  last tab). Easy Connect installs the Tailcat helper, turns on SSH access with
  one admin prompt, and gives a pairing QR code: under Connect a phone, click
  Show QR to open it in a popup (Settings never shows the code until you ask)
  and scan it with the Ghostex mobile app (Android ships today). If the phone can't scan it, click Copy as text under Show QR and paste the code in the phone's Scan code screen (Paste a pairing code). A Tailscale path is offered for tailnets; its code sits behind the same Show QR button and Copy as text.
  The phone's own camera can scan the code too: the QR is a ghostex.dev link,
  so with an up-to-date Ghostex app installed the camera opens the app and it
  pairs straight away, on Android and iPhone.
  Without the app, the link opens a ghostex.dev page that says how to install
  it. An older phone app can't read the new code with its own scanner: update
  the app first.
  The Android app is the `ghostex-android.apk` on the GitHub release, built
  for 64-bit (arm64) phones. If Android says "App not installed as package
  appears to be invalid", uninstall any other Ghostex build first (one you
  built yourself is signed differently) and install the latest release's APK;
  if it still fails, `adb install ghostex-android.apk` from a computer shows
  the real reason.
  The phone app keeps itself up to date: most releases download in the
  background and apply the next time the app starts, or right away with
  Restart to update in the phone's Settings > Updates, which also shows the
  running version. A release that needs a new app shows Update available
  there instead, with a Download button on the GitHub APK.
  With Auto reconnect enabled in the phone's Settings > Connection, the phone
  checks the connection when you return to the app or its network changes and
  reconnects interrupted agent terminals. Tap a red cloud or choose Reconnect
  from the computer's menu to start a fresh connection. Easy Connect does not
  require the separate Tailscale app; the Tailscale connection option does.
  If the connection fails while saved sessions are still visible, the Sessions
  list shows a warning with the failure reason and a Retry button. The warning
  clears once the list successfully refreshes.
  To find a session on the phone, tap Search in the bar at the bottom of the
  Sessions list (beside Voice, Web and Machines) and type part of its name or tag; it searches the open and
  closed sessions of every connected computer, newest first, like the desktop's
  Sessions search. Tapping an open session opens it; tapping a closed one (shown
  with a hollow dot) brings it back on its computer and opens it. Closed
  sessions appear once that computer runs an up-to-date Ghostex.
  Paired devices are listed and can be removed. On the phone, open Web Preview
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
  To read a project's docs on the phone, long-press the project and choose
  Files, or choose Files from a session's ⋯ menu. It lists the project's Markdown
  and HTML files from the same folders the desktop Files view shows, with search
  and the most recently changed files on top; very deep folders list down to
  eight levels. Files open in a reader on the
  phone, and Reload picks up an agent's latest edit. HTML pages include the
  Agentation annotation tool (the pen button hides it); its copy button puts
  your notes on the phone's clipboard, ready to paste into a session.
  Tapping a link to a file on the computer in a chat or terminal opens it in
  the same reader: Markdown, HTML, text and code files, and pictures (other
  files show their path, copied, since the phone cannot preview them).
  A picture opens at the screen's width and scrolls down like a page; pinch to
  zoom, and double-tap to switch between that and its actual size.
- **From another computer**: Settings > Remote > Remote machines > Add a
  machine with SSH details or an Easy Connect code, then Install / Connect
  gxserver on it. What you type in that dialog (except the password) stays
  there when Settings closes or the app restarts, until you add the machine or
  press Cancel. The machine appears as a sidebar section with its own
  projects and sessions; its terminals stream into the desktop app. Windows,
  Linux, and macOS clients use the connected computer's folders and shell.
  Files and folders you drop onto a remote terminal or add with its attach
  (paperclip) button are uploaded to that computer first, so the terminal gets
  references the agent there can open.
  Open Code from the view panel's + menu to edit the remote project; if prompted,
  install the editor component first. A remote Mac or Windows computer runs the
  editor its own Ghostex has, so if Code says that computer doesn't have it yet,
  open the VS Code view on that computer once (or update Ghostex there) and retry.
  Folder links in remote chats also browse the remote folder in VS Code.
  A remote project's Open Folder (its right-click menu, Quick Access) opens the
  project in the Files view. Open In with VS Code, VS Code Insiders, VSCodium,
  Cursor, Windsurf, Trae or Antigravity opens the remote folder over Remote-SSH,
  and Zed opens it over SSH (not on a native Windows machine); the editor's
  command line tool must be installed. Other apps can only open folders on this
  computer, so they show a warning with Copy Path.
  Remote localhost links open in the built-in Browser through that computer,
  even when ordinary web links are set to open in your external browser.
  When a machine cannot connect, its tab's cloud turns red and its list says
  why, with Reconnect and Remote Settings buttons. Ghostex keeps retrying a
  dropped connection, but a rejected username or password is not retried (so
  the account is not locked out); saving a new SSH password reconnects it.
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
  Uninstall run there through its own Ghostex. A machine that is not connected
  shows why and can't be picked; one whose Ghostex is too old says to update it
  first. On a remote Mac, Integrations also installs and updates Desktop control
  (Fast Computer & Browser Use and SpaceO; never on Linux or Windows). Their
  Accessibility and Screen Recording permissions can only be allowed by someone
  at that Mac, so until then the row says so. Turning agents on, their defaults
  and resume hooks are set on each computer itself. On the phone, a computer's
  Edit screen (swipe left on its card in Machines, then Edit) shows the same
  Agent CLIs and Skills and tools with Install and Update. When that machine
  (or this computer) runs Windows, both pages recommend Developer Mode (Windows
  Settings > System > For developers > Developer Mode): without it Windows
  blocks symbolic links, so Ghostex links hooks and skills with junctions and
  file copies instead. Click the notice's X to fold it to a warning icon.
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
Claude progress updates do not trigger completion notifications while Claude
reports background work still running. Completion notifications arrive when
Claude finishes after that work completes; requests for your input or permission
still get your attention.
Pi, OMP and Amp sessions ring the same way: a finished turn marks the session
done (for Pi and OMP after any automatic retry or queued follow-up), stopping it
with Esc or a provider error does not, and a question or confirmation a Pi or OMP
extension asks marks the session as waiting for you.
While an agent has questions in its chat that you have not answered (question
forms and question pages, from any turn), the session shows the pink question dot
in the sidebar, the phone's session list and `ghostex sessions` ("answer
requested"), also while the agent keeps working. A turn that asks new questions
sends one notification, labelled Needs input with how many it asked. The dot stays
while you look at the session and goes away once every question is answered (from
the form, the page, or Voice mode), or the agent withdraws or asks them again.
Voice mode can read these questions to you and answer them by their tag ("answer
Q335 with the first option").
A question form or question page you want to answer another time can be put off
with For later on its card: it stops counting as unanswered (no pink dot, no count
on the chat's ? pill, no notification) but stays answerable, and Back to
unanswered on the card undoes it. Answering it ends For later by itself. The
choice is kept by Ghostex, so the computer, the web page and the phone agree.
The ? pill on the chat's working row opens Chat Resources, a list of the chat's
generated content: question forms, HTML pages (with or without questions) and charts,
each row marked by its icon. A small dropdown at the top right, on the header's
line, picks what it shows: All, Html, Q&A or Charts, with their counts. Questions
sit in folding sections (Unanswered, For later, Answered, Withdrawn), pages that
ask nothing under Pages and charts under Charts, newest first, each with its
title, tags and time; click a row to jump to it.
On the computer the list opens when you rest the pointer on the pill, and a click
on the pill still steps to the previous unanswered question; on the phone a tap
opens it. The pill stays while the chat has anything to list, with its dot and
count only while a question is unanswered.
The count and the list cover the whole conversation, including what the chat hasn't
loaded. A coordinator's list also has Worker pages: the pages its Workers posted,
closed Workers included, each with the Worker's name. They open read-only there.
Each group (Unanswered, Earlier, Worker pages) shows 12 rows at first with its own
Show more under them; a folded group shows none.
Every copy shows a small "Copied!" bubble at the pointer for a moment, whether
it came from a terminal, a chat message, a copy button, or a menu. Copy sound is
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
The bell also covers your remote computers: its count includes unread
notifications from every connected remote computer, and with a remote computer
in the sidebar the panel has All, This computer, and Remote tabs, each with its
own unread count. A remote row names its computer in a small chip next to the
title; clicking it opens that session on its computer, and reading, dismissing
or clearing it updates that computer's own list. Mark all read and Clear all act
on the open tab, while Next unread goes through every computer. While a remote
computer is offline its notifications are hidden and left out of the count, and
the panel says so until it reconnects.

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
dot, finished shows the blue dot until you look, and a failure shows a red dot.
An agent that goes from working back to idle also counts as finished, unless
you stopped it with Escape or the chat's Stop button. Hover the session to read
the program's one-line message (and its progress when it reports one); on the
phone, long-press the session to read it. `ghostex sessions --json --full` shows each session's
reports under `programStatus`.

With Show debug UI controls on, the Program status mismatches log on the
Debugging page records each time detection and a program's report disagree.

Related settings: `completionSound`, `actionCompletionSound`, `copySound`,
`showMacOSAttentionNotifications`, `resetExpirySystemNotifications`,
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
your drives), and review before Clone & Add. While it clones, the dialog shows
Git's progress and a Cancel clone link.

Project headers show the branch and diff stats; the header’s Commit (Git) menu offers
commit, sync with main, PR review by a prompt agent, and related actions with
persistent running toasts. Commit, Push and Create PR open the Changes view (see
Views) with its commit bar ready, where you review the diff and confirm. Add Worktree on a project header creates a git
worktree as its own project so a second agent works on a branch without
touching the main checkout; worktrees can be renamed, merged back, and
deleted from the sidebar. When a worktree's folder is deleted, the sessions that
worked in it continue in the project's folder with the same conversation, so a
finished session or thread still opens and can be messaged.

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
projects (`ghostex issues tracker linear|github`; a workspace that never
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
project under **Linear API key** in Settings > Projects (the line under it says
whether the project uses its own key, its workspace's or the shared one, and
Remove goes back to the workspace's). Or run `ghostex issues linear-key`
and paste it (that sets the Personal workspace's key, which a workspace without
its own uses; `--project-id <id>` sets one for a single project, `--clear`
removes it). GitHub status comes from `gh`, so sign in
with `gh auth login`. `ghostex issues status` says what is set up.
`ghostex work-mode …`, the old name, still works for every one of these commands.

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
terminal. A PR or issue link of the project's own repos that you or the session's
agent only wrote in the chat is offered under Suggested too. A coordinator's own
replies suggest nothing (they mostly repeat its threads' reports); there only the PR
and issue links you write yourself are suggested. Suggested shows the five
newest; when there are more, Show all under them lists the rest, and Show fewer folds
them again. A suggestion's × (click it twice, like a linked link's) dismisses it for
good: it moves under Removed, where Restore puts it back under Suggested, and a Linear
ticket you later type yourself still links. The + in the list's header (Add on the
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
--start` does both from a terminal (a GitHub issue in a GitHub workspace). New session still
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
scopes added and the app reinstalled before the Issues view can show channel names),
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
docs directory), Settings > Projects > Linear API key, `hideProjectHeaderDiffStats`,
`showProjectEditorDiffFileCount`,
`showUntrackedProjectDiffWhenNoTrackedChanges`. Issues Workflow:
`titleSessionsByBranch`, `offerCleanupWhenPrMerges`, `ghostex issues
status|linear-key|tracker|create-ticket|start|cleanup` (old name `ghostex
work-mode`), `ghostex link-session`. Team
backend: `ghostex team deploy|join|invite|identity|status|ping|leave`. Slack:
`ghostex team flow|slack-manifest|slack-connect|linear-connect|own-linear-key`,
`ghostex slack post`.

## Extensions, Open In, and integrations

- The **Extensions** row at the bottom of the Settings sidebar, below About,
  opens the Extensions page. Under it, a small chevron row (*N extension
  pages*) folds and unfolds the add-on pages: Voice agent, Workspaces, Cloud
  Boxes, Connectors, Actions, Open In, OS Integration and Debugging, each only
  while it is shown. Connectors, OS Integration and Debugging are built-in
  extensions, on by default, with switches in the Extensions page's Features; the
  Connectors and OS Integration pages and cards also need Enable Experimental
  Features (`connectorsHidden`, `osIntegrationHidden`, `debuggingHidden`). **Show
  Advanced** (its switch is in the Settings sidebar) never hides a page: it only
  shows or hides rows inside pages. The pages start folded and Settings remembers whether you left
  them open; a search opens them and lists only the pages that match, and opening
  one of them from elsewhere (a link, a menu, a turned-off message) opens
  them too. While they are folded over the page you are on, the chevron row is
  highlighted.
- Settings > Extensions shows every extension as a card, three to a row: the
  built-in ones, grouped by category (Features, Project websites, Code and
  files, Planning and automation, Header buttons, Menus and panels, Shared
  runtime),
  then Get more (installed extensions first, then the audited
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
  Windows), Workspaces (off by default; it brings workspaces and the team's
  Slack flow), Read aloud (off by default, and
  listed only while Enable Experimental Features is on) and the Session details
  panel (beta, on by default). Turning one off removes it everywhere at once (its header button,
  Settings pages and rows, hotkeys, Quick Access rows and menus) and turning it
  back on restores everything you had set up. Settings: `actionsHidden`,
  `openInTitlebarButtonHidden`, `sidebarSpacesEnabled`, `cloudBoxesHidden`,
  `workspacesHidden`, `readAloudHidden`, `sessionDetailsPanelHidden`.
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
  To delete one of your own views, open it with its pencil button and press
  **Delete** at the bottom of its editor (or hover its card and click the
  trash button), then confirm. The view is removed from every project and its
  tab closes. Built-in views and the built-in templates cannot be deleted, only
  switched off; a template you saved yourself has its own remove button in the
  Add view template list.
  Star saved Claude and Codex accounts in Settings > Accounts (the Related
  settings links at the bottom of this page open it) to show their usage at the
  bottom of the desktop sidebar, or unstar them to hide it. The meters are hidden until you ask for them: hover the
  chart button in the sidebar's bottom bar, just left of the Settings gear,
  and every starred account floats over the bottom of the list, four per row,
  without moving the list. The button turns into a pin while you hover it:
  click it to keep the meters there above the bottom bar, and click it again
  (the pin shows filled while they are pinned) to unpin them. Ghostex remembers
  whether they are pinned. The button itself is only there once you have
  starred an account.
  Claude meters show the two tightest of the 7d, 5h, and Fable
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
- An extension you install from Settings > Extensions opens where it says on its card. A view
  extension (Canvas, Tower) gets a view tab in the view picker. A terminal-pane
  extension, such as Lazygit, opens its command in a terminal split beside the
  focused pane (or a new tab: Open location on its Settings page), in the active
  project's folder. Every enabled extension that opens on its own (terminal panes,
  views, modals) is an **Open <name>** row in Quick Access's Commands (type its name),
  and is listed in the header's ⋯ menu > Extensions; a pinned extension also has its
  icon in the work area header. A newly installed terminal-pane extension starts
  pinned; turn **Pinned** off on its Settings page to remove the icon. A
  terminal-pane extension needs its command on this computer (Lazygit needs
  `lazygit`): when it is missing, opening the extension says so, and on a Mac with
  Homebrew its **Install with Homebrew** button runs `brew install <command>` in a
  new terminal; open the extension again once it finishes. Terminal-pane extensions
  run only for local projects. CLI: `ghostex extensions list`, `ghostex extensions
  install <id>`, `ghostex extensions state <id> --set pinned=true`.
- **Claude** is a built-in extension, off by default: turn it on with its switch
  in Settings > Extensions (Features) to use claude.ai inside Ghostex.
  It adds a Claude button (grey, like its neighbours) to the sidebar's top row, next to the Issues button.
  Clicking it turns Claude on: claude.ai takes the whole window, from any
  project, and the sidebar, the side panel and the Commands pane are hidden.
  The sidebar's top buttons (Toggle sidebar, the Agents Panel toggle, Claude,
  Search, the bell and the menu) stay where they are, so the Claude button is
  in the same spot to turn it off, and the header no longer names the project
  and session you were in.
  Claude never gets a tab in the side panel; you can still open the side panel
  beside it (the Browser and every other view work there as usual), and bring
  the Commands pane back with its usual button and shortcut. Showing the
  sidebar (Toggle sidebar or Cmd+B) turns Claude off and takes you back to
  where you were before it, like switching back to a Space and project: the
  same Space, project and session, with the sidebar, side panel and Commands
  pane as they were. Pointing at the window's left edge only peeks at the
  sidebar and leaves Claude on; clicking the Claude button there turns it off.
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
- **Session notes** is a built-in extension, off by default: turn it on with its
  switch in Settings > Extensions (Features) to attach a note to an agent
  conversation, such as what to do next when you come back to it. Then
  right-click a session and choose **Note** (or use its hover button, or
  Option+Cmd+N on macOS for the focused session), or pick **Session note** in the chat's
  or the terminal's More actions; a session with a note shows a small dot on
  its card, the note in its tooltip and a "1" beside Session note. The note
  belongs to the conversation, so it is still there after you close the
  session and resume it, and the phone app shows and edits the same notes.
  Turning it off hides all of that, here and on the phone, and keeps every
  note you wrote for when you turn it back on. Setting: `sessionNotesHidden`;
  CLI: `ghostex session-note read --session-id <id>` and `ghostex session-note
  save --session-id <id> --note '<text>'` (only while it is on).
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
  type to use it, such as `$ghostex-computer-use`. Each skill is installed
  once, in `~/.agents/skills`; agents that do not read that folder (Claude
  Code, Kiro, Droid, Hermes and others) get a link to it in their own skills
  folder, so every agent uses the same copy. When the computer is
  online they are downloaded from the Ghostex GitHub repository, so skill fixes
  arrive between releases, and installed skills are refreshed automatically
  each time Ghostex starts; a copy an older Ghostex left in an agent's own
  folder is replaced by a link then. Offline installs use the copy inside the
  app.
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
  "spaceo", Fast Computer & Browser Use as "CuaDriver"), and opens that
  Privacy & Security pane; turn the switch on there and the row turns to
  Granted. For SpaceO, a small Ghostex tag sits on the right edge of System
  Settings (its left edge when the right has no room), pointing into its app
  list: drag its spaceo tile into the list, which adds SpaceO and switches it
  on. The tag shows Waiting for the drop until macOS has the permission, then
  opens the other permission's pane (Screen Recording first, then
  Accessibility). If SpaceO started before a permission, it restarts by
  itself once no agent is using it (the tag says Restarts when SpaceO is free
  while one is), then shows SpaceO is ready and closes; the rows' Needs
  restart has a Restart SpaceO button that waits the same way. Close the tag
  any time with its x. SpaceO is macOS only: on Windows and Linux its rows
  are hidden and `ghostex spaceo install-skill` says so. Ask agents
  to use $ghostex-spaceo (or /ghostex-spaceo in Claude).
  Commands: `ghostex spaceo install-skill`, `spaceo doctor`.
- Settings > Integrations > Tools lists the tools Ghostex can install for you
  when something you set up needs them: Node.js and npm, uv, Homebrew (Mac),
  System tools (curl, certificates, unzip and git on Linux), Beads, and the
  GitHub and GitLab CLIs. Each Install button installs with one click and its
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
  is experimental: turn on Settings > About > Experimental >
  Enable Experimental Features (or run `ghostex settings set showBetaFeatures
  true`) to use it. It is also a built-in extension, on by default: switching
  Connectors off in Settings > Extensions (or `ghostex settings set
  connectorsHidden true`) removes its page and its apps from agents, and
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
  so an agent can set connectors up for you. Settings › Connectors (shown while Enable
  Experimental Features and the Connectors extension are on) does the same with buttons, one section per
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
  to PATH button when an installed CLI is not on PATH; a command that is only
  a launcher installing the agent the first time it runs, like Omarchy's, shows
  as not installed yet), an Install guide
  that installs any other supported agent, the Ghostex helper (agent hooks)
  and Computer Use; which views to show (Browser and Files are on by default
  on a first run) and the browser skill; phone pairing and notifications;
  and the first project folder with the default agent, session view and
  Agent approvals (Keep default or Skip permissions, the same setting as
  Settings > Agents, `agentAcceptAllEnabled`), next to the look: Appearance, the theme colour squares (Dark and Light tabs),
  Colourfulness and one Transparency row (the simple choices from Settings >
  Theme), with a "More theme options in Settings > Theme" link that opens the
  Theme page. Turning transparency on there also switches Appearance to Dark. The very first run opens on a short
  intro video before the panels: press Play to watch it there (on Linux it
  opens in your web browser, and says so), it shows once, and Continue (or Enter) goes on to
  setup at any time; the
  video is also at https://youtu.be/QzjFB4J6-8E. Choose folder opens the
  system's folder picker; where there is none (a Linux desktop without the
  xdg-desktop-portal file chooser) Ghostex opens its own: type a path (Tab
  completes it, `~` is your home folder) or click through the folders, then
  Use this folder. Setup counts as done once it opens your first project.
  Skip for now (or closing the window) before any project exists leaves setup
  unfinished and opens Add Project; it comes back on the next launch only if
  there is still no project. With a folder chosen, Advanced settings later
  adds the project and then opens Settings. Reopen the setup any
  time from Tips > Setup or Quick Access > Commands > Setup.
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

## Appearance and app

Theme colours, colourfulness, window glass, and active pane outline live on their
own Settings page, Theme, right below General (`ghostex settings open --tab theme`).
It has three groups, Colours, Transparency and Chat and terminal, each one card
whose finer controls sit under its own More row, plus links to related settings
on General; a search for a row inside a More row opens it.
Colours: Appearance, three preview cards (System, Light, or Dark; System is the
default and follows the operating system appearance); Dark mode colour and Light
mode colour, each a row of sixteen small colour squares (Graphite, the default,
Black in dark mode or White in light mode, Slate, Midnight, a deep navy, Blue,
Indigo, Teal, Green, Forest, Olive, Amber, Orange, Red, Rose, Pink and Purple;
picking a dark colour gives light mode the same colour until you pick one there
yourself, and the light row says when it still follows) and a + square for a
custom colour, which opens a colour picker (with HEX, R, G and B boxes) right at
the square; and Colourfulness, a fine-grained slider from Subtle to Vivid (Soft is the default;
the name of the nearest of Subtle, Soft, Balanced, Rich and Vivid shows beside it)
that sets how much of the colour shows in the sidebar and work area at once, with a small
sidebar and work area preview. Colourfulness also sets how deep a custom colour
is; it has no control of its own. More colour options can set the sidebar and
work area colourfulness separately, and show the active pane outline and its
colour.
A preset never overwrites the custom values, so picking the + square again
restores them. The chosen theme colors the
sidebar and window chrome, the sidebar's dropdown menus, and the chat view
background (chat keeps following its own Chat theme setting, so a light chat in a
dark app uses the light theme's color). Chat and terminal: Chat theme and
Terminal theme default to Follow app, with optional Light, Dark, or System
overrides; its Terminal palettes row holds the terminal palettes. Existing
saved themes are preserved, and a saved dark contrast or tint that differs from
the default starts on the custom colour. The accent color (status highlights,
accent text, advanced-setting markers) has no setting of its own: it follows the
dark theme's tint hue, and a neutral tint keeps the sky-blue accent.
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
Files, Kanban, the browser, and the code editor stay opaque. Turning on Reduce
transparency in the macOS accessibility settings, or turning off Transparency effects
in Windows Settings > Personalization > Colors, always makes the window opaque. On
Windows, turning glass on takes effect the next time Ghostex starts, and the corners of
menus and pop-ups follow Windows' own rounding.
Behind the glass (on macOS, Windows and Linux) is four cards: Desktop and windows (the default) shows everything behind Ghostex. Wallpaper only shows just your desktop wallpaper, so other windows never show through; built-in wallpapers such as Sequoia show as a still picture of that wallpaper, and a solid color wallpaper shows everything behind the window. Picture shows a picture you choose instead, one for dark mode and one for light mode, side by side with Choose and Clear buttons; a mode with no picture shows everything behind the window. Live shows something moving behind the glass, chosen for dark mode and for light mode (only dark mode with Transparency set to Dark only): one of eight calm animations drawn in your theme's colours (Aurora, Ink, Drift, Nebula, Silk, Bokeh, Waves, Mesh), so switching themes or Colourfulness recolours it at once, or, on macOS and Linux, Your video, a .mov, .mp4 or .m4v file you choose, which plays muted, looping and blurred (a mode set to Your video with no file shows everything behind the window). On Windows, Live pauses when Show animations is off in Windows Settings > Accessibility > Visual effects (showing a still frame) and in battery saver. Speed sets how fast the animation moves (a quarter of its pace to twice as fast) and Brightness how bright it glows (60% by default, a soft glow that shows through the tints; every animation is about as bright as the others at the same setting); your own video plays as it is. An animation loops every two minutes without a seam, and changing the animation, theme or brightness, or switching between an animation and your video, fades rather than jumping. Nothing is downloaded. Live pauses whenever Ghostex is in the background, hidden or minimized, while the display sleeps and in Low Power Mode; Reduce Motion shows a still frame; and Play only when plugged in (on by default) pauses it on battery. For Wallpaper, Picture and Your video, Picture position picks Moves with the window (the default: the picture covers the window and moves with it) or Stays with the desktop (the picture stays put while the window moves over it, and can trail the window while you drag it) (`windowGlassSource`, `windowGlassImagePlacement`, `windowGlassImageDark`, `windowGlassImageLight`, `windowGlassLiveStyleDark`, `windowGlassLiveStyleLight`, `windowGlassVideoDark`, `windowGlassVideoLight`, `windowGlassLiveSpeed`, `windowGlassLiveBrightness`, `windowGlassVideoOnlyOnPower`).
On Linux, transparency works on Xorg and Wayland desktops (the desktop app currently uses XWayland for its embedded pages). Desktop and windows uses your compositor's blur: on Hyprland, keep background blur enabled; on Xorg, run a compositor with transparency and blur support. The compositor controls the blur's strength. Picture and Live animations work without compositor blur. Wallpaper only reads the picture reported by Hyprpaper, swww/awww, GNOME, Cinnamon, MATE or a single-screen Plasma desktop; when no readable picture is available it shows Desktop and windows. Stays with the desktop works on X11 and Hyprland; other native Wayland compositors cannot report the position needed for this mode, so use Moves with the window. Your video requires FFmpeg (included as a Linux package dependency). Live respects battery power, the power-saver profile, and disabled desktop animations (`windowGlass`, `windowGlassSource`, `windowGlassImagePlacement`, `windowGlassVideoOnlyOnPower`).
While glass is on, four sliders tune it, each in dark mode and in light mode: Sidebar tint and Work area tint set how much of the desktop each area hides, independently, so either can be the darker one; lower shows more of your desktop.
Keep Awake (Power)
prevents sleep while agents work.
Advanced holds Enable Experimental Features and Faster rendering (off by default), which
redraws only the parts of a window that changed so Ghostex uses less CPU while agents stream
and you scroll; it applies at once, and turning it off brings back the usual drawing if part
of a window ever stops updating (`fasterRendering`). The separate Debugging page sits
in the Settings sidebar's Extensions group and is listed
while its Debugging extension is on (it is by default;
switch it off in Settings > Extensions, or `debuggingHidden`, to drop the page). It starts with Show debug UI controls.
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

Send Bug / Feedback, in the sidebar's More Options menu right under Join Discord,
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
  Agentation in the Browser toolbar (see Views, Browser). Markdown files use
  Files: select text to comment or mark Looks good, Clarify, or Needs tests,
  then Send to the last-clicked session (see Views, Files).
- "Use Ghostex from my phone or another computer": see Remote machines, web,
  and mobile, then `ghostex settings open --tab remote`.
- "Run an agent on a schedule": see Automations; open the Automate view or
  drive `ghostex automation-save`.
- "Play a sound and notify me when an agent finishes": `completionSound`
  (any value except `off`), `showMacOSAttentionNotifications true`, the
  menu bar badges (always on; remove the item from the menu bar itself), and the
  sidebar bell (kept visible with `notificationsTitlebarButtonHidden false`)
  lists every finished turn with what the agent said.
- "What does the Kanban board do": see Project board (Kanban).
