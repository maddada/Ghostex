# Ghostex overview

Ghostex is a desktop workspace for running AI coding agents (Claude Code, Codex,
Gemini CLI, OpenCode, Pi, and other agent CLIs) side by side across many
projects. Every agent runs in a real, persistent terminal session; Ghostex adds
the sidebar, the chat view, the browser, the editor, the board, and the
automation and remote-access layers around those sessions.

Run `ghostex guide <chapter>` for the details:

features Views, sidebar, sessions, chat, agents and orchestration, coordinators, browser,
editor, board, docs, automations, remote and mobile, notifications
settings Every setting with its key, type, allowed values, default, and
the page it lives on (use with `ghostex settings`)
hotkeys Every shortcut and its default binding

## The window

- **Work area header**: the window has no separate title bar. The first row of
  the work area shows Hide sidebar, Back/Forward, and the project icon, project
  name and current session title as one breadcrumb. On the right are the
  **Voice** (headset) button that starts and ends a spoken conversation with the voice
  agent, the **Start**, **Open** and **Commit** buttons (Quick Actions, Open In and Git
  actions, each with a caret for its menu; Start and Open show only while the
  Actions and Open In extensions are on, and Actions is off by default), and the command terminal
  and view panel toggles. The header covers your sessions only: when a view panel
  is open, the panel's own tabs take the rest of that same row, so the tabs sit
  over the view and the header's buttons over your sessions. Narrow the Agents
  Panel and the buttons drop their labels and the breadcrumb drops the project
  name. There is no line under the header: your chat fades out beneath it. Drag
  the header, or the sidebar's Search row, to move the window.
- **Sidebar** (left by default): projects, their sessions, tags and filters,
  remote machines, and the More Options menu under the three-lines button at
  the top right (Sort & Filter, Collapse All, Commands, Agents Hub, All Automations,
  Connect a Device, Keep Awake, then the Tools, Help and Ghostex submenus and
  Settings; Ghostex holds New Window, Check for Updates, Restart Ghostex, Quit
  Ghostex, and Quit Ghostex & BG Service, which also stops the background
  service and every session; Help holds Ask Ghostex, Tips & Tricks, Join
  Discord and Send Bug / Feedback; Tools holds Search by Prompt, Import
  Sessions, Extensions, Dev servers and Resources). The
  sidebar's top row holds Search, Add project, the Notifications bell and that
  menu button. Under it, the New session button starts your last agent (its
  arrow opens the New Thread picker). The bottom of the sidebar holds your
  account usage meters, which are the last thing in it; while they are hidden,
  a small handle at the bottom right brings them back (point at it to peek,
  click it to pin them). The workspace button (when Workspaces is on) sits on a
  line of its own above the meters, at the left. There are no other buttons
  down there: Commands and Settings are rows of the More Options menu, with
  Cmd+Shift+P and Cmd+, as their hotkeys. Search is an icon button that keeps
  its name and shortcut in its tooltip.
- **Work area**: your sessions, and the views open beside them. The sessions are
  a grid of terminal panes and tabs; each pane can show the raw terminal or the
  Session Chat rendering of the same agent conversation. Opening Code, Browser,
  Kanban, Automate or Files puts it in a panel on the right with a divider you can
  drag, and your agents keep running on the left. The panel's tab strip shares
  the header's row: several views can be open at once, the **+** opens another,
  and the two buttons at its end pop the view out into its own window or expand
  it over the Agents Panel. With nothing open it shows **Open a view**, a picker of
  everything this project can open. Project views load on demand and sleep when unused.
  The view panel toggle at the right end of the header, or Cmd+Option+B, opens
  and closes the whole panel.
- **More windows**: File > New Window (Cmd+Shift+N) opens another full window
  on the same projects and sessions, for a second monitor or Space; each window
  keeps its own sidebar selection, panes and views, and every open window comes
  back at the next launch.
- **Quick Access** (Cmd+Shift+P): search every command, pane action, settings
  shortcut, and recent session. Cmd+P opens it on Recent Sessions.
- **Settings** (Cmd+,): **Updates** first (with how many updates count) and **Interface** under it,
  then five labelled groups: **Working** (Chat, Sidebar & Sessions, Terminal, Files & Links,
  Projects), **Keyboard & voice** (Hotkeys, Dictation, Voice Agent), **Agents & accounts** (Agents,
  Accounts, Autopilot, Secrets, Integrations), **Connections** (Remote, Sync, Collab) and
  **System** (Notifications, Power & Sleep, Security, About); a group shows only while one of its
  pages does. Then **Extensions** at the bottom (the built-in feature
  switches and the extensions you can install), with a chevron under it that folds the
  add-on pages (Workspaces, Cloud Boxes, Connectors, Actions, Open In, OS
  Integration and Debugging while each is shown), and one search box that finds rows on
  every page. Every page starts with its title and a one-line intro saying what the page is for.
- **Dialogs** such as Quick Access, Settings, Search by Prompt and Add Project open
  over the middle of the window and stay open when you click somewhere else. Close
  one with Escape or the round × in its top-right corner, which appears while the
  pointer is over the dialog.

## Key concepts

- **Project**: a folder (or git worktree) in the sidebar. Projects can be
  grouped, reordered, and given per-project agents, actions, and defaults.
- **Session**: one terminal pane, usually running one agent conversation.
  Sessions persist across app restarts (the terminal host keeps the process alive), can be
  slept to free RAM and woken later, pinned, tagged, renamed, forked, and
  resumed from history.
- **Session Chat**: the GUI rendering of an agent session, with a composer,
  prompt queue, transcript, tool cards, and image paste. Toggle between chat and
  terminal for the same session with one click or hotkey. Agents can draw
  **Ghostex visuals** (gx visuals: charts, stat tiles and tables), ask in a
  **Ghostex form** (gx form: questions with choices) and show a **Ghostex
  page** (gx page: an HTML page such as a mockup) right in the chat.
- **Agents**: the configured agent buttons per project (built-in CLIs plus
  custom commands), Global Actions, and the Agents Hub catalog.
- **Coordinator**: an agent session you talk to that hands the work to
  threads (agent sessions it starts, shown indented under it in the sidebar)
  and reports back when they finish or need you. Start one from a project's
  Select Agent menu with New Coordinator.
- **Extensions**: optional views and panels (Code, Browser, Kanban, Automate,
  Files, and third-party ones) installed from Settings > Extensions.
- **gxserver**: the local daemon that owns sessions, projects, settings sync,
  automations, and the HTTP API the `ghostex` CLI, the web app, and the mobile
  app talk to. A remote machine runs its own gxserver; the desktop app connects
  to it over SSH or Easy Connect.

## The `ghostex` CLI

`ghostex` (alias `gx`) is installed with the app. Agents use it to list and
control sessions, create agent sessions and send them prompts, read output,
manage automations and the board, search prompt history, and now to read this
guide and change settings. `ghostex --help` is the command catalog; the
`$ghostex-cli` skill teaches the help-first workflow.
