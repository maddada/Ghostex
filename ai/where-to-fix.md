# Where a fix or feature belongs

Put the change where the wrong decision is made, not where the symptom shows up. Ghostex runs on several clients at once (the desktop app, the GPUI web build, the Android app, the `ghostex` CLI, remote computers), and they all go through the same daemon. A fix in one client's UI leaves every other client broken. A fix in the daemon or in a shared core fixes all of them at once.

## The first question

Before editing, ask: **would the web build, the mobile app, the CLI, or a remote client hit the same problem?**

- **Yes, or not sure:** the fix goes in gxserver or a shared core (layers 1 to 3 below).
- **No, it really is one client's layout, input, or platform I/O:** the fix goes in that client (layers 4 and 5).

If you still put a fix in a renderer or in one app when a lower layer could own it, say why in your report.

## The layers, lowest first

Work down the list and stop at the first layer that can own the change.

### 1. gxserver (`server/src/`)

gxserver owns everything about an agent session, so almost every agent or terminal bug belongs here:

- what gets typed into a session's terminal, and when (sends, interrupts, clears, slash commands, dialog answers)
- reading an agent's screen and TUI state (composer detection, dialogs, pickers, the rewind flow)
- agent hooks, activity (working, idle, needs input), transcripts and how they are decoded
- session lifecycle: create, fork, resume, sleep, wake, rename
- git and worktree operations, persisted settings, accounts
- every `ghostex` CLI verb

gxserver is Rust. The old TypeScript gxserver is gone; never add code for it or look to it as a reference.

**Example (2026-09-24).** Pressing Escape a few times in chat to stop a Claude turn sent Claude a double Escape, which Claude reads as "rewind" and answers by opening its own `/rewind` picker. That picker hid the input box, and the chat's Restore conversation then failed. The fix limits interrupt Escapes to one per second per session, inside gxserver (`server/src/session_chat_claude_interrupt.rs`). It covers desktop, web and mobile chat with one change, and no chat UI had to know about it.

**Example (2026-09-24).** The chat rewind counted a prompt Claude had handed back to its input box, so it landed one prompt too far up. The fix is also in gxserver (`resolve_rewind_target` in `server/src/session_chat_rewind.rs`), reusing the returned-prompt record the chat readers already use to hide that bubble. One source of truth, no second copy in a client.

### 2. Session daemons: zmx and wmx

How a terminal session is held (attach and detach, resize, the 200-column resting grid, history and refresh, visibility OSCs, persistence, client leadership) lives in the session daemons:

- **zmx** (`.dependencies/zmx/`) on macOS, Linux, and Windows under WSL.
- **wmx** (`.dependencies/wmx/`) on native Windows (PowerShell, ConPTY).

Change both, or say why the other is unaffected. Ghostex-specific startup and paths stay in gxserver (`server/src/zmx/`, `server/src/zmx/scripts_windows.rs`), never in the daemons. Read the zmx wire-generation rules in AGENTS.md before touching zmx IPC.

### 3. Platform-neutral Rust cores (`packages/`)

Client-side state and rules that the daemon does not own live in Rust crates with no I/O, no threads and no clock. Desktop and the web build use them today; mobile will use them through UniFFI.

- `packages/gx-core`: the sidebar store: rows, focus, menus, drag, action plans.
- `packages/gx-chat-core`: the chat brain: the document the chat draws, drafts, the queue, questions, pickers, transcript decisions.
- `packages/gx-protocol`: the typed gxserver wire protocol shared by every Rust client.

### 4. Renderers

Layout, input and pixels only, no product rules:

- `apps/desktop/src/app/native_chat/` (chat), `native_sidebar/` (sidebar), `native_kanban/` and `native_automate/` (board and automations), `render/workarea_header/` (work area header).
- `apps/gpui-web` compiles the desktop's own renderer files through symlinks. Never fork a renderer file for the web; add a web stand-in only where the README says to.

### 5. Host glue

Sockets, storage and timers for one platform, and nothing else:

- `apps/desktop/src/app/gx_chat/` (feeds `gx-chat-core`), `apps/desktop/src/app/gx_store/` (feeds `gx-core`), `apps/gpui-web/src/app/gx_store/`.
- `packages/client-storage/` owns storage ownership, validation, budgets and recovery rules for every host.

## Chat

User decision 2026-09-24: chat-rule fixes and features go **only** into `packages/gx-chat-core`, plus `native_chat/` when drawing changes. Since 2026-09-25 the core is the only chat brain on every client: the desktop and the GPUI web build run it through `apps/desktop/src/app/gx_chat/`, and the phone runs it through UniFFI (`packages/gx-chat-mobile`) under native React Native views (`apps/mobile/app/src/chat/`; decisions 2026-09-21, 1b, and 2026-09-25, native only). The TypeScript chat brain, the React chat and the phone's WebView chat were deleted that day. Phone drawing changes go in `apps/mobile/app/src/chat/native/`, with `native_chat/` as the reference for what each part of the document means.

A chat bug that is really about the session (what reaches the terminal, what the agent's screen shows, what the transcript says) is a gxserver fix under layer 1, not a chat-core fix. It then works for every client immediately.

## Where not to start new work

- **React chat, the TypeScript chat brain and the phone's WebView chat**: deleted on 2026-09-25; do not restore them. What remains in `packages/core-ui/chat/` is the Markdown renderer, the GhostexEditor's Lexical prompt input and the Stashed Prompts draft storage, not a chat.
- **React Kanban and Automate pages** (`apps/desktop/views/tasks-placeholder.tsx`, `apps/desktop/views/project-board/`): retired; the native views are `native_kanban/` and `native_automate/`.
- **The old TypeScript gxserver**: gone; gxserver is Rust only.
- **The desktop QuickJS app runtime**: deleted on 2026-09-25. Its behaviour lives in gxserver, `gx-core` and `apps/desktop/src/app/gx_store/`; do not add a JavaScript engine or service back to the desktop.
