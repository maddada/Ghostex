# Ghostex overview

Ghostex is a desktop workspace for running AI coding agents (Claude Code, Codex,
Gemini CLI, OpenCode, Pi, and other agent CLIs) side by side across many
projects. Every agent runs in a real, persistent terminal session; Ghostex adds
the sidebar, the chat view, the browser, the editor, the board, and the
automation and remote-access layers around those sessions.

Run `ghostex guide <chapter>` for the details:

  features    Views, sidebar, sessions, chat, agents and orchestration, browser,
              editor, board, docs, automations, remote and mobile, notifications
  settings    Every setting with its key, type, allowed values, default, and
              the page it lives on (use with `ghostex settings`)
  hotkeys     Every shortcut and its default binding

## The window

- **Titlebar**: the project name and icon, Back/Forward, the view tabs
  (Agents, Code, Browser, Kanban, Automate, Docs), and on the right the Help,
  Tips, Resources, Git, Actions, and Open In buttons. Views other than Agents
  are extensions that load on demand and sleep when unused.
  View tabs stay centered; in compact mode their dropdown moves to the left,
  immediately after the Next (Forward) button.
- **Sidebar** (left by default): projects, their sessions, tags and filters,
  remote machines, Quick chats, and the More Options menu (Settings, Search by
  Prompt, Previous Sessions, Mobile & Remote, Extensions).
- **Work area**: the current project's view. In Agents it is a grid of
  terminal panes and tabs; each pane can show the raw terminal or the Session
  Chat rendering of the same agent conversation.
- **Quick Access** (Cmd+Shift+P): search every command, pane action, settings
  shortcut, and recent session. Cmd+P opens it on Recent Sessions.
- **Settings** (Cmd+,): pages for General, Integrations, Extensions, Remote,
  Projects, Agents, Accounts, Actions, Open In, Hotkeys, and About, with one
  search box that finds rows on every page.

## Key concepts

- **Project**: a folder (or git worktree) in the sidebar. Projects can be
  grouped, reordered, and given per-project agents, actions, and defaults.
- **Session**: one terminal pane, usually running one agent conversation.
  Sessions persist across app restarts (zmx keeps the process alive), can be
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
