# Ghostex overview

Ghostex is a desktop workspace for running AI coding agents (Claude Code, Codex,
Gemini CLI, OpenCode, Pi, and other agent CLIs) side by side across many
projects. Every agent runs in a real, persistent terminal session; Ghostex adds
the sidebar, the chat view, the browser, the editor, the board, and the
automation and remote-access layers around those sessions.

Run `ghostex guide <chapter>` for the details:

features Views, sidebar, sessions, chat, agents and orchestration, browser,
editor, board, docs, automations, remote and mobile, notifications
settings Every setting with its key, type, allowed values, default, and
the page it lives on (use with `ghostex settings`)
hotkeys Every shortcut and its default binding

## The window

- **Work area header**: the window has no separate title bar. The first row of
  the work area shows Hide sidebar, Back/Forward, and the project icon, project
  name and current session title as one breadcrumb. On the right are the
  **Start**, **Open** and **Commit** buttons (Quick Actions, Open In and Git
  actions, each with a caret for its menu), a **⋯** menu holding Ask Ghostex,
  Tips & Tricks, Resources, Dev servers and Extensions, and the command terminal
  and view panel toggles. The header covers your sessions only: when a view panel
  is open, the panel's own tabs take the rest of that same row, so the tabs sit
  over the view and the header's buttons over your sessions. Narrow the sessions
  column and the buttons drop their labels and the breadcrumb drops the project
  name. There is no line under the header: your chat fades out beneath it. Drag
  the header, or the sidebar's Search row, to move the window.
- **Sidebar** (left by default): projects, their sessions, tags and filters,
  remote machines, Quick chats, and the More Options menu (Settings, Search by
  Prompt, Previous Sessions, Mobile & Remote, Extensions). The Notifications
  bell is in its top row; the Commands row at the bottom carries a chart button
  that shows or hides your account usage meters above it, and a Settings gear.
  Drag the sidebar narrow and the Search and Commands rows become icon buttons
  that keep their names and shortcuts in their tooltips.
- **Work area**: your sessions, and the views open beside them. The sessions are
  a grid of terminal panes and tabs; each pane can show the raw terminal or the
  Session Chat rendering of the same agent conversation. Opening Code, Browser,
  Kanban, Automate or Docs puts it in a panel on the right with a divider you can
  drag, and your agents keep running on the left. The panel's tab strip shares
  the header's row: several views can be open at once, the **+** opens another,
  and the two buttons at its end pop the view out into its own window or expand
  it over the sessions column. With nothing open it shows **Open a view**, a picker of
  everything this project can open, including Ghostex's own Ask Ghostex, Tips &
  Tricks and Resources pages. Project views load on demand and sleep when unused.
  The view panel toggle at the right end of the header, or Cmd+Option+B, opens
  and closes the whole panel.
- **Quick Access** (Cmd+Shift+P): search every command, pane action, settings
  shortcut, and recent session. Cmd+P opens it on Recent Sessions.
- **Settings** (Cmd+,): pages for General, Integrations, Extensions, Remote,
  Projects, Agents, Accounts, Actions, Open In, Hotkeys, Debugging, and About, with one
  search box that finds rows on every page.

## Key concepts

- **Project**: a folder (or git worktree) in the sidebar. Projects can be
  grouped, reordered, and given per-project agents, actions, and defaults.
- **Session**: one terminal pane, usually running one agent conversation.
  Sessions persist across app restarts (the terminal host keeps the process alive), can be
  slept to free RAM and woken later, pinned, tagged, renamed, forked, and
  resumed from history.
- **Session Chat**: the GUI rendering of an agent session, with a composer,
  prompt queue, transcript, tool cards, and image paste. Toggle between chat and
  terminal for the same session with one click or hotkey.
- **Agents**: the configured agent buttons per project (built-in CLIs plus
  custom commands), Global Actions, and the Agents Hub catalog.
- **Extensions**: optional views and panels (Code, Browser, Kanban, Automate,
  Docs, and third-party ones) installed from Settings > Extensions.
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
