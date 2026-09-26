# Ghostex GPUI in the browser (experiment)

The desktop app's native GPUI UI, compiled to wasm32 and drawn into a `<canvas>` by `gpui_web` (WebGPU, WebGL2 fallback) from the Zed fork in `.dependencies/zed`. It talks to gxserver directly from the page. It is the only web app: the React one (`apps/web`) was deleted on 2026-09-24, and `ghostex web` serves this build.

Status on 2026-09-22: the sidebar, the chat view and the terminal are the desktop's own source files running in Chrome against live gxserver data.

| Surface | What runs | Reused from the desktop |
| --- | --- | --- |
| Sidebar | Rows, sections, headers, icons, hover actions, tooltips, collapse (saved to localStorage in the desktop's format), row focus, the row context menu | `app/native_sidebar/*` (all but `actions.rs`), `gx_store/sidebar_snapshot.rs`, `gx-core` for the list, menus, UI state and action plans |
| Chat | Transcript, markdown, tool rows, composer with typing, image paste (thumbnail and `Image #n` reference), model pill and model picker, More actions menu, the composer's Terminal View button, tooltips, scrollbar, paging of older turns | All 87 files of `app/native_chat/` except five (`binding`, `runtime_worker`, `rpc`, `focus`, `replay_recording`), the TypeScript controller bundle (`native-host.ts`) and the desktop's chat broker (`sidebar/session-chat-runtime/broker.ts`), both unchanged |
| Terminal | Output, colours, Nerd Font glyphs, cursor, keyboard input through Ghostty's key encoder, resize, and an action bar with the desktop's Chat View button | `terminal_element.rs`, `ghostty_vt.rs`, `terminal_wheel.rs`, `terminal_scrollbar_reveal.rs`, the state half of `terminal_model.rs`, and libghostty-vt itself as a static wasm32 archive |
| Shell | Work area header with breadcrumb and sidebar toggle; chat and terminal switch through their own buttons, as on the desktop. Start, Open, Commit, the more menu and the panel toggles are drawn disabled. Code, Browser, Kanban, Automate and Docs tabs are not drawn | The desktop's constants, palette and icons; the header itself is written here |

Live Windows verification (2026-09-25) covers sending messages, asynchronous question choices and multiline Unicode answers, Codex text dialogs and rewind history, and terminal Tab/Shift+Tab/Ctrl+Enter in disposable sessions. Sleep and Wake have not been exercised in that browser audit.

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

`bun apps/gpui-web/build-wasm.mjs` without `--release` gives a debug build that works but is 100 MB and slow. The Bash entry point delegates to the same builder.

On Windows, run `bun run web:build` from PowerShell with Zig 0.16, Rust 1.95.0's `wasm32-unknown-unknown` target and wasm-bindgen-cli 0.2.125 installed. The builder restores this crate's tracked symlinks when Git checked them out as text; Windows Developer Mode or symlink privileges are required. It invokes Rust directly on Windows because the web-sys feature list exceeds the command-line limit through sccache. Serve the result against the running Windows gxserver with `ghostex web --dist-dir C:/dev/Ghostex/apps/gpui-web/www/dist --no-open` (adjust the checkout path).

- `http://localhost:4174/?session=<projectId>:<sessionId>&surface=terminal` opens a session directly (ids as in its zmx name, `S90-<projectId>-<sessionId>`).
- `?chatDebug` logs every chat runtime output to the console.
- `node shot.mjs out.png --wait 5000 --click 150,240 --type "text" --key Enter --key Control+a --pasteimage <base64 png> --eval "js" --rightclick 150,300 --move x,y --pause ms --url ...` drives the page in headless Chrome over the DevTools protocol, prints the page console and saves a screenshot. It waits in real time; Chrome's `--virtual-time-budget` never delivers the gxserver WebSocket frames.

## How it is put together

| Piece | What it is |
| --- | --- |
| `src/**` symlinks | The desktop's own source files, compiled unchanged. A symlink, not `#[path]`: rustc resolves the nested `mod` lines of a `#[path]`-loaded file as if it were a `mod.rs`, which breaks every file that has a sibling directory. Where one file of a folder has to differ (`native_sidebar/actions.rs`, five chat files), the folder is real and its other entries are per-file symlinks. |
| `build.rs` + `extracted-items.txt` | Lifts named items (functions, consts, types, `impl=Type` blocks, single `Type::method`s) byte for byte out of desktop files that mix portable code with native code: `helpers/titlebar.rs`, `helpers/browser.rs`, and the whole state half of `terminal_model.rs`. The list is the inventory of what a shared crate has to own. |
| `src/lib.rs` prelude | The desktop crate root doubles as a prelude (`use crate::*`), so the same names are re-exported here. |
| `src/app/web_app.rs` | This build's `GhostexGpuiApp`: the fields and methods the shared code reads, with the browser's answer behind each. |
| `src/app/gx_store/` | The web store host: `fetch` + one `WebSocket` in place of `packages/gx-client`, and the `gx-core` `Core`, `SidebarViewModel`, `SidebarUiStore`, `SidebarMenus` and action planners. |
| `src/app/native_chat/runtime_worker.rs` | The chat controller bundle in a hidden same-origin iframe per chat, behind the API of the desktop's QuickJS worker thread. An iframe because the bundle replaces `setTimeout` with virtual timers, which would break the page. It drains in a macrotask, after the page's microtasks, which is what the desktop's "run every pending job, then drain" means in a browser. |
| `src/app/chat_host.rs` | Runs the desktop's `broker.ts` in the page and relays between it and the chat views, with the payload the desktop's `session_chat_runtime.rs` builds. |
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

## Findings

- **The toolchain is not the hard part.** The pinned 1.95.0 compiler works; `RUSTC_BOOTSTRAP=1` is only needed because `gpui_platform` pulls `gpui_web` with its default `multithreaded` feature. Single-threaded, no COOP/COEP headers, no nightly.
- **libghostty-vt links statically into a Rust wasm module.** `zig build -Demit-lib-vt -Dtarget=wasm32-freestanding` gives a 1.2 MB archive, and `ghostty_vt.rs` compiles against it unchanged. No second wasm module and no JavaScript glue.
- **The chat's TypeScript is already host-neutral.** The controller bundle needs nothing but `crypto.randomUUID`, and the broker was written for a browser page. Both run here byte for byte.
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
- Sidebar commands not handled yet log `sidebar command not handled on web yet` (close, fork, pin, tags, snooze, drag and drop, rename, project actions). `gx-core` already plans most of them.
- Settings are empty (`shared_settings::install` is never called), so the chat uses its defaults for theme, zoom and width.

## What is next

1. The rest of the sidebar commands through the core's planners, and settings from gxserver.
2. The desktop's real work area header and view tab strip (`render/workarea_header/`, `view_tab_strip.rs`) in place of the one written here.
3. Reconnect handling for the terminal socket, and parked / chat visibility states when the surface is switched away.
4. Promote the symlinks and `extracted-items.txt` into a shared crate once the set is stable, so the two builds cannot drift and the build-time extraction can go.
