# Ghostex GPUI in the browser (experiment)

The desktop app's native GPUI UI, compiled to wasm32 and drawn into a `<canvas>` by `gpui_web` (WebGPU, WebGL2 fallback) from the Zed fork in `.dependencies/zed`. It talks to gxserver directly from the page. It is the only web app: the React one (`apps/web`) was deleted on 2026-09-24, and `ghostex web` serves this build.

Status on 2026-09-25: the sidebar, the chat view, the terminal and the sidebar's actions (the desktop's own `gx_store/` executor files, since the app runtime port's step 4) run in Chrome against live gxserver data.

| Surface | What runs | Reused from the desktop |
| --- | --- | --- |
| Sidebar | Rows, sections, headers, icons, hover actions, tooltips, collapse (saved to localStorage in the desktop's format), row focus, the row context menu, the HUD (agents, Saved Actions), the Git +/- numbers, countdown labels that tick | `app/native_sidebar/*` (all but `actions.rs`), `gx_store/sidebar_snapshot.rs`, `gx_store/hud/`, `gx-core` for the list, menus, UI state and action plans |
| Sidebar actions | Sleep, Wake, Close, Fork, Full Reload, pin and the other flags, Snooze, Park, Rename, Note, Delayed Send and Close After Done (schedule, postpone, cancel), tags, drag order, New Group, group Rename, Close Group and Sleep, project moves and collections, a project's Remove and Close, New Terminal, New Agent (the launcher and its hook check), Quick Terminal; a browser tab opens in the page's own browser | The desktop's `gx_store/sidebar_*.rs`, `terminal_lifecycle/`, `workspace_groups*`, `client_document.rs`, `project_docs.rs`, `create/`, symlinked; `web_host/` answers what they hand to the app (toasts, dialogs, the work area) |
| Dialogs and Quick Access | Rename Session, Session Note, Delayed Send (Session Automations), Agent Hooks Required, Add, Delete and Rename Worktree, and Quick Access (Sessions, Projects, Saved Prompts, Commands) open as overlay windows; toasts draw at the bottom centre | The desktop's `window/*_modal.rs`, their `*_lifecycle.rs`, `window/quick_access/`, `quick_access/`, `window/toast.rs` |
| Chat | Transcript, markdown, tool rows, sending and streaming, question cards, composer with typing, drafts that survive a reload, image paste (thumbnail and `Image #n` reference), model pill and model picker, More actions menu, the composer's Terminal View button, tooltips, scrollbar, paging of older turns, switching chats | All of `app/native_chat/` except four (`binding`, `rpc`, `focus`, `launch`), and the desktop's Rust chat host `app/gx_chat/` on `gx-chat-core` and `packages/gx-chat-client` (all but three files: see below). No TypeScript runs in the chat. |
| Terminal | Output, colours, Nerd Font glyphs, cursor, keyboard input through Ghostty's key encoder, resize, and an action bar with the desktop's Chat View button | `terminal_element.rs`, `ghostty_vt.rs`, `terminal_wheel.rs`, `terminal_scrollbar_reveal.rs`, the state half of `terminal_model.rs`, and libghostty-vt itself as a static wasm32 archive |
| Shell | Work area header with breadcrumb and sidebar toggle; chat and terminal switch through their own buttons, as on the desktop. Commit opens the Git menu (branch, +/-, ahead and behind, the Git actions) from `gx_store/git/`. Start, Open, the more menu and the panel toggles are drawn disabled. Code, Browser, Kanban, Automate and Docs tabs are not drawn | The desktop's constants, palette and icons; the header and its Git dropdown are written here |

Chat sending, model picks and question answers were run against throwaway sessions on 2026-09-25, and so were row focus, New Terminal, New Agent, Rename, Sleep, Wake, Close, Delayed Send schedule and cancel, New Group and group Rename, the Git menu, Add Worktree and Quick Access (app runtime port, step 4).

What a page cannot do, and answers with a toast or a disabled control instead of a fake: anything on a remote machine (its tunnel is an SSH port forward the desktop app owns, so the page draws only this computer's tab); the commit review, Settings, Configure Agents and every other dialog that exists only as a CEF page (Commit, Push and Create PR open the review, so they toast); Finder, Open In, a terminal app and a native folder or file picker (Add Project, a missing folder's Locate, worktree images); Handoff / Export (it writes a file); the pet, Keep Awake and the status item; Split Right and the pane workspace; Settings writes (gxserver has no settings endpoint, so the page composes with the defaults).

## Run it

```bash
# once
cargo install wasm-bindgen-cli --version 0.2.125 --locked   # must match the wasm-bindgen in Cargo.lock
# needs Zig 0.16 (the repo's Zig) for the first build, and bun

# gxserver must be running. From the repository root:
bun run start:web        # builds the wasm (release, ~2 min cold) and the page into www/dist, then serves it with `ghostex web` on http://127.0.0.1:4173

# for iterating, keep `ghostex web --no-open` running (the page's bootstrap hands it the daemon URL and token), then:
bun run web:dev          # rebuilds the wasm and starts Vite on http://localhost:4174, which proxies the bootstrap to :4173
```

`./build-wasm.sh` without `--release` gives a debug build that works but is 100 MB and slow.

- `http://localhost:4174/?session=<projectId>:<sessionId>&surface=terminal` opens a session directly (ids as in its zmx name, `S90-<projectId>-<sessionId>`).
- `node shot.mjs out.png --wait 5000 --click 150,240 --type "text" --key Enter --key Control+a --pasteimage <base64 png> --eval "js" --rightclick 150,300 --move x,y --wheel x,y,deltaY --pause ms --timeout ms --url ...` drives the page in headless Chrome over the DevTools protocol, prints the page console and saves a screenshot. It waits in real time; Chrome's `--virtual-time-budget` never delivers the gxserver WebSocket frames. A run that stalls stops its own Chrome after `--timeout` (90 s). When Chrome for Testing never commits a navigation (seen on a machine short of memory), point `SHOT_CHROME` at Playwright's `chrome-headless-shell`.

## How it is put together

| Piece | What it is |
| --- | --- |
| `src/**` symlinks | The desktop's own source files, compiled unchanged. A symlink, not `#[path]`: rustc resolves the nested `mod` lines of a `#[path]`-loaded file as if it were a `mod.rs`, which breaks every file that has a sibling directory. Where one file of a folder has to differ (`native_sidebar/actions.rs`, five chat files), the folder is real and its other entries are per-file symlinks. |
| `build.rs` + `extracted-items.txt` | Lifts named items (functions, consts, types, `impl=Type` blocks, single `Type::method`s) byte for byte out of desktop files that mix portable code with native code: `helpers/titlebar.rs`, `helpers/browser.rs`, and the whole state half of `terminal_model.rs`. The list is the inventory of what a shared crate has to own. |
| `src/lib.rs` prelude | The desktop crate root doubles as a prelude (`use crate::*`), so the same names are re-exported here. |
| `src/app/web_app.rs` | This build's `GhostexGpuiApp`: the fields and methods the shared code reads, with the browser's answer behind each. |
| `src/app/gx_store/` | The web store host: `fetch` + one `WebSocket` in place of `packages/gx-client`, and the `gx-core` `Core`, `SidebarViewModel`, `SidebarUiStore`, `SidebarMenus` and action planners. Most of the folder is the desktop's own executor files, symlinked: they call gxserver only through `gx_rpc` (`rpc.rs`, `fetch` here) or `gxserver_rpc_result_task` (the older action files' call, same seam). `host.rs`, `sidebar_list.rs`, `runtime_facts.rs`, `sidebar_ui_storage.rs` (localStorage), `diagnostics.rs` (no-op records) and `sidebar_clock.rs` give them the desktop's names. |
| `src/app/web_host/` | What those files hand to the app, answered by the page: toasts, the native dialogs as overlay windows (`modals.rs`), the work area a created or focused session opens in, the Git dropdown, the app windows' `sidebarCommand` messages, and a project's path actions (Copy Path, open a pull request in a new tab). |
| `src/app/remote_conn/` | The names the shared files call for remote machines, with a refusal: the page has no tunnels. |
| `src/app/gx_chat/` | The desktop's chat host, symlinked file by file, with three browser twins: `worker.rs` runs the host on the page's thread (one `setTimeout` for the core's timers, and each view woken in a later macrotask so a view and the host never call each other in one task), `storage_backend.rs` reads and writes the page's `packages/client-storage` (hydrated by `www/src/main.js` before the app starts and exposed as `globalThis.ghostexChatStorage`), and `platform.rs` is the page's clock and `crypto`. The chat socket is `packages/gx-chat-client`'s browser `WebSocket`, the token in its query string. |
| `src/app/chat_host.rs` | What the app does for a chat view: its presentation cache and the host actions a shell performs. |
| `src/terminal_model.rs` | The desktop model with its PTY half replaced by gxserver's `/api/terminal` WebSocket. |
| `src/cef.rs`, `ghostty_kit.rs`, `support_logs.rs`, `shared_settings.rs`, `app/helpers/web.rs`, ... | Same-named stand-ins for native modules the shared files mention. |
| `component-assets/` | A stand-in `gpui-component-assets` that embeds the icon folder. Upstream's crate becomes an HTTP icon downloader on wasm, which changes its type and fails the first paint of every icon. |

## Accessibility tree (for driving and debugging)

The desktop's sidebar, chat and terminal carry roles, labels and state, and the web build mirrors gpui's AccessKit tree into the DOM. What a test or an agent gets, on both:

- **Sidebar** (`navigation "Sidebar"`): machine `tab`s (selected), Space `button`s (selected), a `treeitem` per project and collection (expanded) and per session (label = title, description = state words such as `sleeping, idle, pinned, needs an answer, 18d`, selected = focused), `button`s for section headers (expanded), chevrons, hover actions, Search, Commands, Settings and the sidebar menu (expanded).
- **Chat** (`group "Session chat"`): `document "Conversation"`, one `article` per message labelled `<role> message: <text>` (first 2000 chars), `button`s for tool rows (`Tool <name>: <preview>`, expanded, description `failed`), disclosures, message actions, the composer (`group "Message composer"` with its `textbox`) and every toolbar button, `menuitem`s in menus, `checkbox` cards in the model picker.
- **Terminal** (`log "<title>"`): one `text` node per screen row, in order.

On the desktop, gpui builds the tree only while an assistive client is connected, so nothing is paid until `cua-driver` (or VoiceOver) attaches; `cua-driver` then acts on element tokens instead of pixels. On the web, `#gpui-a11y` holds one element per node with the ARIA role, `aria-label`, `aria-description`, `aria-expanded`, `aria-selected`, `aria-checked`, `data-gpui-actions` and the node's page bounds, brought up to date at most ten times a second; `?a11y=off` turns it off. `window.gpuiA11y.snapshot()` returns the tree as plain objects, `window.gpuiA11y.act(id, 'Click')` dispatches an AccessKit action to a node (gpui clicks the node's centre), and Playwright's `getByRole` finds the mirrored elements: a pointer click on one lands on the canvas underneath, because the mirror lets pointer events through. `node shot.mjs out.png --print "window.gpuiA11y.snapshot()"` dumps it from the command line.

Known gaps: rows inside the chat's virtual list report bounds relative to the list rather than the page, so reach them through `act()` or `getByRole` rather than by coordinate; scrolled-out sidebar rows are in the tree with their off-screen bounds; text runs inside a message are not separate nodes (one label per message, by decision).

## Changes outside this folder

All are no-ops for native builds; `cargo check --bins` of the desktop crate passes.

- **Desktop, portable clock.** `std::time::Instant` became `web_time::Instant` in the shared sidebar, chat and terminal files (`Instant::now()` panics on wasm32; `web_time` re-exports std's on native), and `instant::Instant` for the one value gpui-component's scrollbar takes. `web-time` and `instant` were added to `apps/desktop/Cargo.toml`.
- **Desktop, async chat RPC.** `native_chat/rpc.rs::request` is an `async fn` and its two call sites await it. The desktop body still blocks on the background thread it always ran on; the web body awaits `fetch`.
- **Desktop.** `render/window_drag_region.rs` got a `target_family = "wasm"` arm.
- **Zed fork, `crates/gpui`.** The accessibility setup and action handling are no longer gated off wasm, and the three adapter channels use `try_send` (they are unbounded; `send_blocking` does not exist on wasm).
- **Zed fork, `crates/gpui_web`, accessibility.** New `a11y.rs`: the DOM mirror described above, behind `a11y_init` and `a11y_tree_update` on `WebWindow`.
- **Zed fork, `crates/gpui_web`, pasted images.** A paste event's image files are read, parked where `Platform::read_from_clipboard` finds them, and the paste shortcut is dispatched again, so an app's own synchronous paste handler runs unchanged (the chat's `paste_attachments` does). The replayed shortcut is ctrl-v on every computer, because key bindings are picked by the compile target and wasm is not macOS.
- **Zed fork, `crates/gpui_web`, windows.** Every window after the first is an overlay canvas placed at the bounds it asked for, with a transparent surface, so the desktop's child windows (menus, model picker, image viewer, dialogs) work in a page. A closed window now removes its elements, cancels its animation frame and disconnects its observers. Focus, blur and hover handlers tolerate re-entry, because opening a second window blurs the first from inside its own click handler.
- `ai/AREAS.md` has a new `WebGpui` area.
- **Desktop, portable clock (step 4).** The store's action, Git, create and Quick Access files use `web_time::Instant` / `web_time::SystemTime` too.
- **Desktop, one call seam (step 4).** The older sidebar action files (sleep, wake, close, fork, flags, snooze, order writes, the client documents' pushes) spawned the blocking `gpui_gxserver_rpc_result` inline; that expression moved behind `gx_store/rpc.rs` `gxserver_rpc_result_task`, unchanged on the desktop, with a `fetch` twin here.
- **Zed fork, `crates/gpui_web`, overlay size.** An overlay window starts at the size it asked for instead of zero, so a dialog that fits its window to its content measures itself at its real width on the first frame.

## Findings

- **The toolchain is not the hard part.** The pinned 1.95.0 compiler works; `RUSTC_BOOTSTRAP=1` is only needed because `gpui_platform` pulls `gpui_web` with its default `multithreaded` feature. Single-threaded, no COOP/COEP headers, no nightly.
- **libghostty-vt links statically into a Rust wasm module.** `zig build -Demit-lib-vt -Dtarget=wasm32-freestanding` gives a 1.2 MB archive, and `ghostty_vt.rs` compiles against it unchanged. No second wasm module and no JavaScript glue.
- **The chat host is host-neutral too.** Only its runner, its storage door and its clock differ between the desktop and the page; the rules, the effects, the retention and the socket's routing are the same files.
- **An idle page stops answering after 30 to 40 seconds in headless Chrome** (2026-09-25): `shot.mjs` screenshots hang with no chat open and with the chat host and client storage switched off, so it is not the chat. Keep a headless run under half a minute until it is found.
- **`gx-core` pays off.** The list, the menus, the sidebar's own state, its storage formats and the daemon call plans are host-neutral.
- **What does not port is the migration scaffolding.** Most of `apps/desktop/src/app/gx_store/` keeps the old QuickJS runtime in step. Including the whole folder gives 916 errors; it should stay out.
- **Debug assertions are off in this crate's dev profile**: gpui's `shape_line` asserts on newlines the shared chat code passes it, and a panic in wasm takes the page down.
- **Three clock types meet here**: `web_time` (gpui), `instant` (gpui-component) and std. Shared files have to name the one their callee takes.

## Known gaps

- The model picker opens about 190 px left of its pill, and child windows show a dark pixel at their rounded corners.
- The console logs `RefCell already borrowed` when a child window opens: an event of the new canvas arrives while the app is mid-update and is dropped. Harmless so far.
- The chat's native file pickers, attachments from disk and Save as Markdown have no browser implementation yet (they compile and fail softly). Pasting an image works; pasting a copied FILE does not.
- The header breadcrumb is empty for a session whose row the sidebar is not drawing (a compact list, a deep link).
- Colour emoji draw as a missing glyph; SVGs with `color(display-p3 ...)` fills fall back to black in resvg.
- The terminal has no context menu, and Cmd+C / Cmd+V go through gpui_web's clipboard, which cannot read outside a paste event.
- A terminal attached here is a visible zmx client with its own grid, so a desktop pane showing the same session reflows while both are open.
- Settings are empty (`shared_settings::install` is never called, and gxserver has no settings read), so the chat, the HUD and the menus use their defaults for theme, zoom, width, the default agent view and the Open In targets.
- Remote machines are not drawn: the page reaches only the daemon that served it.
- The commit review and every other CEF-only dialog answer with a toast; Commit, Push and Create PR need the review, so the Git menu's commit flow is the desktop's for now. Sync, Release and the Git workflows that start an agent run.

## What is next

1. Settings from gxserver (a read the page can call), and the commit review as a native dialog, which would give the page the commit, push and pull request flow.
2. The desktop's real work area header and view tab strip (`render/workarea_header/`, `view_tab_strip.rs`) in place of the one written here.
3. Reconnect handling for the terminal socket, and parked / chat visibility states when the surface is switched away.
4. Promote the symlinks and `extracted-items.txt` into a shared crate once the set is stable, so the two builds cannot drift and the build-time extraction can go.
