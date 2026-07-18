# Plan: ghostex-web — in-browser Agents app (sidebar + agents view + terminals)

Date: 2026-07-15. Orchestrated implementation. Each phase is executed by an
independent agent with ZERO prior context — read this whole header before your
phase.

## Overall goalplspleas

Build `ghostex-web/`, a purely static React SPA (no app server) that replicates
the gpui app's **sidebar** and **Agents view** in the browser:

- Sidebar: the exact same React `SidebarApp` component the gpui app renders in
  CEF (imported from `sidebar/`), with identical session grouping/ordering,
  fed from gxserver. Supports MULTIPLE machines (browser fan-out: one
  connection per machine, merged client-side, machine-scoped ids) like the
  gpui remote-machines feature.
- Agents view: TypeScript reimplementation of gpui's pane/tab workspace
  (split tree, per-pane tab strips, rotate/merge/drag-across-panes, focus
  mode, find-in-terminal, bottom command pane, placeholder cards).
- Terminals: xterm.js attached to zmx sessions through a NEW terminal
  WebSocket endpoint added to gxserver-rs (which spawns `zmx attach` in a
  server-side PTY and pipes bytes). Client wiring follows t3code's pattern
  (attach → replay → live output, coalesced resizes, backoff reconnect).
- Launch story: gxserver serves the built SPA at `http://127.0.0.1:58744/`
  and hands the same-origin page its auth token automatically.

Excluded (do NOT build): Source/Browser/Kanban/Automate/Docs views, t3-chat
panes, browser tabs, tips/resources/git actions/quick actions/open-in-app,
cloud relay, SSH-from-browser.

## Repo rules every worker MUST follow

- Multiple agents share this checkout. NEVER run `git checkout/restore/stash/
  reset/clean` on paths you did not author. Never revert or "clean up" foreign
  uncommitted changes. Do not commit anything.
- NEVER restart the running Ghostex app or gxserver, never run `bun run
  start`, never kill processes on port 58744/58745. For runtime smoke tests
  use an isolated dev instance: `GHOSTEX_GXSERVER_DEV_PORT=58799` plus an
  isolated state dir if supported — and kill only the process YOU started.
- NO tests anywhere in this work (no tests in `ghostex-web/`, `gpui/`, macOS
  app). gxserver-rs: do not add new test files; keep changes to product code.
- NO fallback code where fixing the real behavior is possible. Do the right
  thing from the start; fallbacks hide issues.
- `ghostty/**`, `tui/vendor/**`, `iOS/Vendor/**`, `node_modules/**` are
  imported/vendored — never edit, avoid searching them.
- `gpui/src/**` and `native/**` are READ-ONLY references for this project —
  do not modify them.
- Package manager is bun, run from repo root (`/Users/madda/dev/_active/Ghostex`).
  ALL new npm dependencies are added ONLY in Phase 3; other phases must not
  touch `package.json`/`bun.lock`.
- Keep code comments minimal and only where the code cannot express a
  constraint. Match surrounding style.

## Key reference files (read what your phase needs, they are large)

- `shared/gxserver-protocol.ts` — full TS contract for gxserver HTTP/WS API
  (endpoint path union, request/response types, presentation snapshot/delta
  types `GxserverPresentationSnapshot/Delta`, `GxserverAttachSessionMetadataResult`
  with `attachCommand`/`zmxName`/`cwd`/`startupText`/`startupTextDisposition`).
- `gxserver-rs/src/server.rs` — axum server. Routes registered ~line 312
  (only `/api/events` is a real route; everything else goes through
  `handle_http_request`, dispatch table ~lines 688–1330). `/api/events`
  WebSocket handler ~7228–7437 (auth via `?authToken=` query, subscribe
  protocol). CORS layer ~7574–7624. Protocol-version enforcement ~7499–7529.
- `gxserver-rs/src/zmx.rs` — session lifecycle + interaction dispatch;
  attach-command builder ~1576–1658 (produces the `zmx attach
  --require-existing <zmxName>` shell script), `dispatch_zmx_lifecycle_endpoint`
  ~108, interaction endpoints ~246–368.
- `gxserver-rs/src/auth.rs` — bearer token (file `~/.ghostex/gxserver/auth/token`),
  constant-time compare. `gxserver-rs/src/config.rs` — config + CORS defaults.
  `gxserver-rs/src/constants.rs` — ports (local 127.0.0.1:58744 fixed, remote
  0.0.0.0:58745 off by default, env override `GHOSTEX_GXSERVER_DEV_PORT`).
- `sidebar/sidebar-app.tsx` — the shared SidebarApp React component (host
  agnostic; props `messageSource`, `nativeHostEventSource`, `vscode` sink).
  Message contract: `shared/session-grid-contract.ts`
  (`ExtensionToSidebarMessage` = host→UI, `SidebarToExtensionMessage` = UI→host).
- `gpui/sidebar/main.tsx` — how gpui mounts SidebarApp in CEF.
- `gpui/sidebar/gxserver-runtime.ts` (637KB — search, do not read whole) —
  gpui's runtime adapter: gxserver bootstrap, HTTP rpc helper (~line 13116),
  `/api/events` subscribe (~13164), local message bus → SidebarApp.
- `native/sidebar/gxserver-client.ts` — CLEAN browser-compatible gxserver
  client (fetch + WebSocket + `subscribePresentation`), `DEFAULT_BASE_URL`
  `http://127.0.0.1:58744`. Prefer importing/adapting THIS for web.
- `shared/gxserver-presentation-sidebar-projection.ts` — platform-agnostic
  presentation→sidebar-groups projection (`createGxserverPresentationSidebarGroups`,
  ordering via `orderGxserverPresentationSidebarProjects`).
  `shared/gxserver-presentation-cache.ts` — `reduceGxserverPresentationDelta`.
- `gpui/src/main.rs` (3.8MB — search by symbol, do not read whole) — Agents
  workspace semantics to replicate: `WorkspaceModel` ~10792, `WorkspaceNode`/
  `WorkspaceSplit` ~10773, `WorkspaceLeaf` ~10077, `WorkspaceTabGroup` ~10071,
  tab bar `render_workspace_tab_bar` ~54558, action cluster ~55046, sidebar
  focus placement `receive_sidebar_workspace_terminal_focus_payload` ~35190,
  attach plan `gpui_prepare_local_workspace_attach_terminal_plan_with_startup_text`
  ~77933, command pane `render_workspace_with_command_pane` ~52118 and
  `gpui_command_terminal_create_session_params` ~78010, placeholder states
  `TerminalSessionPresentationState` ~9398.
- `gpui/vite.config.ts` — vite conventions: alias `"@" -> repo root`.
- t3code reference (read-only): terminal client pattern
  `t3code/apps/web/src/components/ThreadTerminalDrawer.tsx`, reconnect
  supervisor `t3code/packages/client-runtime/src/connection/supervisor.ts`
  (backoff `[1s,2s,4s,8s,16s]`).

## Pinned protocol: terminal attach WebSocket (implemented in Phase 1)

- Endpoint: `GET /api/terminal` upgraded to WebSocket, registered as a real
  axum route next to `/api/events`.
- Query params: `authToken` (required, same validation as `/api/events`),
  `protocolVersion=1` (enforced like other endpoints), `projectId`,
  `sessionId`, `cols`, `rows` (initial size, defaults 120x30).
- Auth failure / bad protocol / unknown session / provider-not-running →
  accept the upgrade, send one text frame
  `{"type":"error","code":"unauthorized"|"protocolMismatch"|"notFound"|"providerNotRunning","message":"..."}`
  then close. (Browsers cannot read HTTP error bodies on WS upgrade.)
- On success the server resolves attach metadata through the SAME internal
  path as `/api/attachSessionMetadata` (no auto-wake; the client must have
  already woken the session over HTTP), spawns a PTY (crate `portable-pty`)
  sized cols x rows running the resolved `attachCommand` via the user's login
  shell: `/bin/zsh -lc <attachCommand>` (use `$SHELL` if set, fall back
  `/bin/zsh`), `TERM=xterm-256color`, cwd = attach metadata `cwd`.
- Framing:
  - Binary WS frames are RAW BYTES both directions: server→client = PTY
    output; client→server = user input written to the PTY.
  - Text WS frames are JSON control messages:
    - server→client: `{"type":"ready","zmxName":"...","cols":N,"rows":N}`
      (sent once after spawn), `{"type":"exit","code":N|null}`,
      `{"type":"error",...}` as above.
    - client→server: `{"type":"resize","cols":N,"rows":N}` → PTY resize.
- Replay/scrollback: none server-side — `zmx attach` itself redraws the full
  screen state on attach. Reconnect = open a new WS (new zmx attach client).
  Multi-client attach to the same session is allowed (zmx supports it).
- Cleanup: when the WS closes, kill the spawned attach child process (ONLY
  the `zmx attach` client — never the underlying zmx session) and reap it.
  When the child exits, send `exit` and close the WS.
- The reader/writer must be non-blocking relative to the axum runtime (use
  `tokio::task::spawn_blocking` for PTY reads or the portable-pty async
  helpers; do not block a tokio worker thread on PTY reads).

## Pinned design: web bootstrap + static serving (implemented in Phase 2)

- `POST /api/webBootstrap` (same envelope conventions as other endpoints,
  but NO auth required) returns
  `{ baseUrl, authToken, machineLabel, protocolVersion }` where `baseUrl` is
  the origin the request arrived on and `machineLabel` is the hostname.
  SECURITY: this endpoint must be same-origin-only — if an `Origin` header is
  present it must exactly match the request's own `Host`-derived origin;
  otherwise respond 403. NEVER emit `Access-Control-Allow-Origin` for this
  endpoint regardless of the CORS allowlist. Only served on the local
  listener (never the remote listener).
- Static SPA serving: on the LOCAL listener only, GET requests that do not
  start with `/api/` serve files from `ghostex-web/dist` resolved at runtime
  (config key `web.distDir` with a sane default: look for `ghostex-web/dist`
  relative to the repo checkout used in dev, plus support an absolute path in
  config). Unknown paths without file extension → `index.html` (SPA
  routing). If the dist dir is missing, serve a small inline HTML page saying
  ghostex-web is not built with the build command.
- CORS default-allowlist extension: allow any origin matching
  `^https?://(127\.0\.0\.1|localhost|\[::1\])(:\d+)?$` (needed so the SPA
  served from one local port can call gxservers reached through SSH
  port-forwards on other local ports). Config-provided origins still merge.
  Bearer-token auth remains the real access control.
- CLI: add `ghostex web` subcommand (in `gxserver-rs/src/ghostex_cli/`,
  follow existing subcommand patterns) that prints and opens
  `http://127.0.0.1:58744/` (macOS `open`, linux `xdg-open`).

## Pinned design: ghostex-web app conventions (set up in Phase 3)

- Folder `ghostex-web/` at repo root. No separate package.json: dependencies
  live in the ROOT `package.json` (repo convention — gpui does the same).
  Phase 3 adds: `@tanstack/react-router`, `@tanstack/router-plugin`,
  `@vitejs/plugin-react`, `@xterm/xterm`, `@xterm/addon-fit`,
  `@xterm/addon-search`, `@xterm/addon-webgl` (root already has react 19.2,
  vite 8, babel-plugin-react-compiler).
- `ghostex-web/vite.config.ts`: alias `"@" -> repo root` (same as gpui),
  `@vitejs/plugin-react` with babel plugin `babel-plugin-react-compiler`,
  TanStack router plugin (file-based routes in `ghostex-web/src/routes/`),
  dev server proxy: `/api` → `http://127.0.0.1:58744` with WS proxying
  enabled and the `Origin` header rewritten/stripped so `/api/webBootstrap`
  sees a same-origin request in dev.
- Root `package.json` scripts: `"web:dev": "vite --config ghostex-web/vite.config.ts"`,
  `"web:build": "vite build --config ghostex-web/vite.config.ts"`,
  `"web:typecheck": "tsc -p ghostex-web/tsconfig.json --noEmit"`.
- `ghostex-web/tsconfig.json` extends repo root tsconfig conventions, path
  alias `@/* -> ../*`, strict, jsx react-jsx, includes `ghostex-web/src` plus
  the imported `sidebar/`/`shared/` sources compile through vite (typecheck
  scope: ghostex-web/src only).
- Styling: plain CSS (the sidebar ships `sidebar/styles.css`); app-level CSS
  variables copied to match the gpui dark look (near-black `#0a0a0a`-ish
  background, subtle borders, SF Pro/system font stack). Dark theme only.
- State: plain React + small module-level stores (follow the sidebar's
  patterns); persistence via `localStorage`. No extra state libraries.

## Multi-machine model (Phases 4/5/8)

- A "machine" = `{ machineId, label, baseUrl, authToken }`. The PRIMARY
  machine is auto-added from `/api/webBootstrap` at startup (machineId
  `"local"`). Additional machines are user-added (name + baseUrl + token) via
  a small web-only UI and persisted in `localStorage` key
  `ghostexWeb.machines.v1` (tokens included — acceptable v1 tradeoff).
- One `GxserverConnection` per machine: HTTP rpc + `/api/events` presentation
  subscription (reuse/adapt `native/sidebar/gxserver-client.ts`), snapshot +
  `reduceGxserverPresentationDelta` cache, reconnect with `[1,2,4,8,16]s`
  capped backoff, on WS reopen re-fetch authoritative snapshot.
- Sidebar merge: primary machine uses plain presentation ids; added machines
  get machine-scoped ids — follow the existing remote parity pattern
  (`createRemotePresentationSessionId(machineId, projectId, sessionId)` in
  `native/sidebar/native-sidebar.tsx`; shared projector guarantees row
  parity). Terminal attach and every RPC route to the owning machine's
  baseUrl+token.

---

## Phase 1: gxserver-rs terminal attach WebSocket

- depends_on: []
- parallel_ok: true (files disjoint from Phase 3)
- goal: Implement the pinned `/api/terminal` WebSocket endpoint in gxserver-rs
  exactly as specified in "Pinned protocol" above, so any authenticated
  browser client can stream a live zmx-attached PTY for an existing session.
- files: `gxserver-rs/src/terminal_ws.rs` (new), `gxserver-rs/src/server.rs`
  (route registration + module wiring only), `gxserver-rs/src/main.rs` or
  `lib.rs` module decl if needed, `gxserver-rs/Cargo.toml` (add
  `portable-pty`), `shared/gxserver-protocol.ts` (add the terminal WS message
  TS types + endpoint constant, additive only).
- do_not_touch: `ghostex-web/**`, `package.json`, `bun.lock`, `sidebar/**`,
  `gpui/**`, everything else in `gxserver-rs/src` beyond minimal wiring.
- approach: Study how `/api/events` does auth + upgrade (`server.rs`
  ~7228–7437) and mirror it. Resolve attach metadata by calling the same
  internal function `/api/attachSessionMetadata` uses (find it via
  `dispatch_zmx_lifecycle_endpoint` in `zmx.rs` ~108–186) WITHOUT waking:
  if the session/provider is not running return the `providerNotRunning`
  error frame. Spawn PTY via `portable-pty` (`native_pty_system()`,
  `openpty(PtySize{rows,cols,..})`, `CommandBuilder` for `$SHELL -lc
  <attachCommand>` with `TERM=xterm-256color` and cwd). Bridge: blocking
  reader thread (or spawn_blocking loop) forwarding chunks to the WS sink via
  an mpsc channel; WS receive loop writing binary frames to the PTY writer
  and handling `resize` control frames via the pty master `resize()`. On WS
  close kill the child; on child exit send `{"type":"exit"}` and close.
  Keep the endpoint OUT of the remote listener's allowed surface for now
  (match how remote gating works in `server.rs`; local listener only).
- acceptance_criteria:
  - `cd gxserver-rs && cargo check` passes with no warnings introduced.
  - `/api/terminal` route registered alongside `/api/events`; rejects a
    missing/bad `authToken` with an `error` text frame then close (verify by
    code reading).
  - Live smoke test WITHOUT touching the running server: build the binary
    (`cargo build`), start it with `GHOSTEX_GXSERVER_DEV_PORT=58799` and an
    isolated `HOME` or state dir so it does not collide with the real
    daemon, then use a small node script (bun, `ws` not available — use raw
    `WebSocket` from bun) to connect with the dev instance's token, and
    confirm: `ready` frame arrives, binary output frames arrive after
    sending `"echo hi\r"` as a binary frame, `resize` control frame does not
    error, closing the socket leaves no orphaned `zmx attach` process
    (`pgrep -f "zmx attach"` count returns to baseline). If creating a
    session on the isolated instance is impractical, an equivalent smoke via
    a scratch zmx session is acceptable; document what you ran in the
    completion summary. Kill only the dev instance you started.
  - `shared/gxserver-protocol.ts` exports the new message union types
    (`GxserverTerminalWsControlMessage` etc.) and `bunx tsc --noEmit -p
    tsconfig.json` still passes for the repo root config if it passed before
    (do not fix unrelated pre-existing errors).

## Phase 2: gxserver-rs static SPA serving + webBootstrap + `ghostex web`

- depends_on: [1]
- parallel_ok: true (may run alongside Phases 4/6/7; never alongside Phase 1)
- goal: Implement the pinned "web bootstrap + static serving" design: serve
  `ghostex-web/dist` from the local listener, same-origin-only
  `/api/webBootstrap`, the localhost CORS-allowlist regex extension, and the
  `ghostex web` CLI subcommand.
- files: `gxserver-rs/src/server.rs` (static file + webBootstrap handlers,
  CORS extension), `gxserver-rs/src/config.rs` (`web.distDir`),
  `gxserver-rs/src/ghostex_cli/` (new `web` subcommand + registration),
  `shared/gxserver-protocol.ts` (additive: `GxserverWebBootstrapResult`,
  endpoint constant).
- do_not_touch: `ghostex-web/**` sources, `package.json`, `bun.lock`,
  `gxserver-rs/src/terminal_ws.rs` (Phase 1 owns it).
- approach: Static serving hooks into the existing fallback dispatcher in
  `handle_http_request`: non-`/api/` GET on the local listener → resolve file
  under distDir (protect against path traversal: canonicalize and require
  prefix), correct content-types for html/js/css/svg/woff2/png/map, no-cache
  for `index.html`, long cache for hashed assets. distDir default resolution:
  config value if set, else try `<exe-adjacent>/ghostex-web/dist` then the
  dev checkout path — document the lookup in a comment. webBootstrap: follow
  the pinned Origin==Host check, read the token via the same auth module the
  server already uses. CORS: extend the default-allowlist check with the
  localhost regex (keep existing config merge). CLI: mirror an existing
  simple subcommand (e.g. look at how `setup`/`resume-lookup` are wired in
  `ghostex_cli/`).
- acceptance_criteria:
  - `cd gxserver-rs && cargo check` passes.
  - Dev-instance smoke (isolated instance on `GHOSTEX_GXSERVER_DEV_PORT=58799`
    as in Phase 1): with a scratch distDir containing `index.html` +
    `assets/app.js`, `curl` shows `/` returns the html, `/assets/app.js`
    returns JS with correct content-type, `/some/route` returns index.html,
    `/../../etc/passwd` (encoded traversal) is rejected; `curl -X POST
    http://127.0.0.1:58799/api/webBootstrap` with no Origin returns the
    token JSON; same with `Origin: http://evil.example` returns 403 and no
    ACAO header; `curl -H "Origin: http://127.0.0.1:5173" OPTIONS /api/health`
    preflight gets ACAO (regex extension works).
  - `ghostex web` prints the URL and attempts to open the browser (verify by
    code reading + `ghostex web --help` output if flags framework supports).

## Phase 3: ghostex-web scaffold (vite + router + react compiler + app shell)

- depends_on: []
- parallel_ok: true (files disjoint from Phase 1)
- goal: Create the `ghostex-web/` app skeleton per "Pinned design: ghostex-web
  app conventions": vite + TanStack Router (file-based) + React 19 + React
  Compiler + dev proxy, dark app shell with gpui-parity titlebar (center view
  tabs and right-side buttons present in code but hidden), empty sidebar
  region and empty agents workspace region, and the root scripts.
- files: `ghostex-web/**` (new), root `package.json` + `bun.lock` (deps +
  scripts ONLY — this is the only phase allowed to touch them).
- do_not_touch: `gxserver-rs/**`, `sidebar/**`, `shared/**`, `gpui/**`.
- approach: Mirror `gpui/vite.config.ts` for the alias and build hygiene (no
  CEF inlining plugin — normal web build to `ghostex-web/dist`). Routes:
  `src/routes/__root.tsx` (app shell) and `src/routes/index.tsx` (agents
  layout page). Titlebar: replicate the gpui titlebar layout (left: sidebar
  toggle button + "Ghostex" title; center: the six view-tab buttons; right:
  icon cluster) but render center+right with a `hidden` gate (a single
  `WEB_TITLEBAR_HIDDEN_SECTIONS` const) so they can be re-enabled later; copy
  icon SVGs from where gpui/sidebar sources them if conveniently importable
  via `@/`, otherwise inline minimal SVGs. Shell grid: titlebar row, then
  sidebar column (resizable width, collapsed toggle) + main workspace region
  with placeholder. Verify the compiler is active (react-compiler runtime
  import appears in build output).
- acceptance_criteria:
  - `bun install` succeeds; `bun run web:build` produces `ghostex-web/dist`
    with hashed assets; `bun run web:typecheck` passes.
  - `bun run web:dev` serves the shell; `/api` proxy forwards to
    `127.0.0.1:58744` including WebSockets (verify config by code reading;
    live check optional).
  - Titlebar renders only left cluster visually; center/right exist in the
    tree behind the hidden gate.

## Phase 4: machines + connection layer

- depends_on: [3]
- parallel_ok: true (disjoint from Phases 2/6/7: owns `ghostex-web/src/connections/**` and `ghostex-web/src/machines/**`)
- goal: Implement the multi-machine connection layer per "Multi-machine
  model": machine catalog with localStorage persistence, primary-machine
  bootstrap via `/api/webBootstrap`, one gxserver connection per machine
  (rpc + presentation subscription + delta cache + reconnect backoff), a
  connection-status store, and a minimal "Add machine" modal UI (name,
  baseUrl, token) reachable from a small machines button in the titlebar
  left cluster.
- files: `ghostex-web/src/connections/**`, `ghostex-web/src/machines/**`
  (UI), small registration hook in the shell created by Phase 3 (keep the
  diff in shell files minimal and additive).
- do_not_touch: root `package.json`/`bun.lock`, `gxserver-rs/**`,
  `ghostex-web/src/workspace/**`, `ghostex-web/src/terminal/**`,
  `sidebar/**`, `shared/**`.
- approach: Import `native/sidebar/gxserver-client.ts` via `@/` if it works
  browser-side as-is (it should — fetch/WebSocket only); otherwise write a
  thin `ghostex-web/src/connections/gxserver-client.ts` adapted from it,
  preserving request envelope (`{params, protocolVersion:1}`, bearer header,
  `?authToken=` on WS). Presentation state per machine: fetch
  `/api/readPresentationSnapshot`, subscribe `/api/events`
  (`subscribePresentation`), reduce deltas with
  `shared/gxserver-presentation-cache.ts`, expose
  `subscribe(listener)`/`getState()` per machine. Reconnect supervisor per
  machine with `[1,2,4,8,16]s` backoff + online/offline listeners (model on
  t3code supervisor). Bootstrap: on app start POST `/api/webBootstrap`
  (same-origin via proxy/served page) → machine `"local"`; failures show a
  disconnected state, not a crash. Machines UI: list + add + remove +
  per-machine status dot; probe a candidate machine with `/api/health/server`
  using the pasted token before saving.
- acceptance_criteria:
  - `bun run web:typecheck` and `bun run web:build` pass.
  - With gxserver running locally, `bun run web:dev` page console shows a
    presentation snapshot for machine `local` (revision + project count) and
    the WS stays subscribed (verify manually via `console.log` gated behind a
    debug flag, and state in the completion summary what you observed).
  - Add-machine modal stores to `ghostexWeb.machines.v1` and creates a live
    second connection when pointed at a reachable gxserver (code-verifiable
    path; live check only if a second endpoint is available).

## Phase 5: sidebar integration (SidebarApp + web runtime adapter)

- depends_on: [4]
- parallel_ok: true (disjoint from Phases 2/6/7)
- goal: Render the REAL `SidebarApp` in the shell's sidebar region, driven by
  a new web runtime adapter that merges all machines' presentation state into
  `ExtensionToSidebarMessage`s with grouping/ordering identical to gpui, and
  routes `SidebarToExtensionMessage` UI actions back to the owning machine's
  rpc. Web-gated no-ops for native-only actions.
- files: `ghostex-web/src/sidebar-runtime/**`, shell wiring, `sidebar/styles.css`
  import; small WEB-GATED edits inside `sidebar/**` are allowed ONLY if
  unavoidable and must be gated (new optional prop or
  `data-sidebar-host="web"` checks) so gpui/macOS behavior is untouched.
- do_not_touch: root `package.json`/`bun.lock`, `gxserver-rs/**`,
  `ghostex-web/src/workspace/**`, `ghostex-web/src/terminal/**`, `native/**`,
  `gpui/**`.
- approach: Model the adapter on `gpui/sidebar/main.tsx` +
  `createGpuiSidebarRuntime` (search `gpui/sidebar/gxserver-runtime.ts` for
  the message-bus pattern: an EventTarget-backed `messageSource` dispatching
  hydrate/patch messages, and a `vscode.postMessage` sink receiving UI
  actions). Reuse `shared/gxserver-presentation-sidebar-projection.ts` for
  projection and ordering; machine-scoped ids for non-primary machines per
  the remote parity pattern. Handle at minimum these UI→host actions:
  session focus/click (emit an app-level `ghostex-web:focusSession` event the
  workspace will consume in Phase 8, carrying machineId/projectId/sessionId +
  placement info), wake/sleep/kill/close, fork, create session / new
  terminal, acknowledge attention, project collections (localStorage, reuse
  `sidebar/project-collections.ts` behavior as gpui does), rename if the
  sidebar exposes it via rpc (`/api/*` — check the endpoint union). Actions
  with no web meaning (open-in-app/native app shots/pet overlay/hotkeys) are
  explicit no-ops in the adapter (not in SidebarApp). Theme: `document.body`
  classes/dataset the sidebar expects (`vscode-dark native-sidebar-body`,
  `data-sidebar-theme`) copied from `gpui/sidebar/main.tsx`.
- acceptance_criteria:
  - `bun run web:typecheck` + `bun run web:build` pass.
  - Against the local gxserver, the sidebar shows the SAME projects, groups,
    session order, titles, agent icons, and activity dots as the gpui app
    (spot-verify against `ghostex sessions --json` ordering and state in the
    summary; the projection code reuse is the guarantee).
  - Clicking a session dispatches `ghostex-web:focusSession` with correct
    machine/project/session ids (observable via debug log).
  - `git diff sidebar/` is empty OR contains only web-gated additive changes.

## Phase 6: agents workspace model + pane/tab UI

- depends_on: [3]
- parallel_ok: true (disjoint: owns `ghostex-web/src/workspace/**`)
- goal: TypeScript port of gpui's Agents workspace with full pane parity:
  binary split tree with draggable ratio dividers, per-pane tab strips
  (agent icon, title, status dot, lifecycle badge, close), active-tab
  selection, new-terminal (+) button, pane-actions menu (Split Sideways,
  Split Downwards, Rotate Panes Clockwise, Merge All Tabs), tab context menu
  (Focus, Close), drag tabs to reorder within a pane AND across panes, focus
  mode (one pane maximized toggle), placeholder body cards for non-running
  states (Sleeping→Wake, Mounting→Pending startup, StartupFailed→Retry,
  RestoredUnmounted→Materialize), a find-in-terminal bar shell (UI +
  events; actual search wiring lands with the terminal in Phase 8), and
  localStorage layout persistence. The terminal body itself is a pluggable
  slot: render `props.renderTerminalBody(tab)` so Phase 8 injects the real
  terminal.
- files: `ghostex-web/src/workspace/**`, shell wiring for the main region.
- do_not_touch: root `package.json`/`bun.lock`, `ghostex-web/src/connections/**`,
  `ghostex-web/src/sidebar-runtime/**`, `ghostex-web/src/terminal/**`,
  `sidebar/**`, `shared/**`, `gxserver-rs/**`.
- approach: Mirror the Rust model shapes (see reference list: `WorkspaceModel`,
  `WorkspaceNode`, `WorkspaceSplit{axis,ratio,first,second}`,
  `WorkspaceLeaf{paneId,tabGroup}`, `WorkspaceTab{sessionId}`) as plain TS
  types + pure reducer functions (split, close-pane-collapse, rotate,
  merge-all, move-tab, select-tab, focus-pane) in
  `workspace/workspace-model.ts`; React components render from the model.
  Read the gpui functions listed in the header for exact semantics (e.g.
  closing the last tab of a pane collapses the split; rotate is clockwise
  rotation of leaves; merge-all moves every tab into the focused pane).
  Sessions carry `{machineId, projectId, sessionId, title, agentIcon,
  presentationState, activity}`. Tab visuals: dark tab strip like the
  screenshot (active tab lighter), status dot colors Working=orange
  Attention=yellow-dot Idle=none (match gpui: check `AgentTerminalTabStatus`
  usage). DnD: use native HTML drag events or pointer-based drag (dnd-kit is
  available at root if needed — but prefer pointer events for tabs to avoid
  the known dnd-kit click-swallow pitfall). Persist serialized layout per
  primary machine to `localStorage` `ghostexWeb.workspace.v1`, rehydrate on
  load, drop tabs whose sessions no longer exist (reconciliation hook point
  for Phase 8).
- acceptance_criteria:
  - `bun run web:typecheck` + `bun run web:build` pass.
  - With mock sessions (a dev-only seeding path behind a debug flag), the UI
    supports: split sideways/downwards, divider drag persists ratio, rotate,
    merge-all, tab reorder within and across panes, focus mode toggle, close
    tab (last tab collapses pane), placeholder cards per state — each
    demonstrated and listed in the completion summary.
  - The workspace reducer is pure (no React/DOM imports in
    `workspace-model.ts`).

## Phase 7: terminal component (xterm.js + terminal WS client)

- depends_on: [3]
- parallel_ok: true (disjoint: owns `ghostex-web/src/terminal/**`)
- goal: A `SessionTerminal` React component + `TerminalWsClient` implementing
  the pinned Phase 1 protocol: xterm.js rendering, fit-addon sizing with
  coalesced resize messages, raw binary input/output, search addon, WebGL
  addon (progressive enhancement with try/catch → DOM renderer), reconnect
  with `[1,2,4,8,16]s` backoff re-opening the WS (zmx redraws on re-attach),
  and the gpui default Ghostty theme as the xterm theme.
- files: `ghostex-web/src/terminal/**`.
- do_not_touch: root `package.json`/`bun.lock`, everything else in
  `ghostex-web/src`, `gxserver-rs/**`, `sidebar/**`, `shared/**` (READ the
  Phase 1 types from `shared/gxserver-protocol.ts`; if Phase 1 has not
  landed them yet, define local mirrors in `terminal/protocol.ts` matching
  the pinned protocol EXACTLY and leave a `// mirrors shared/gxserver-protocol.ts` note).
- approach: `TerminalWsClient({baseUrl, authToken, projectId, sessionId,
  cols, rows})`: connect `ws(s)://…/api/terminal?...`, binaryType
  `arraybuffer`, expose callbacks `onOutput(Uint8Array)`, `onReady`,
  `onExit`, `onError`, methods `sendInput(bytes|string)`,
  `resize(cols,rows)` (trailing-edge coalesce ~50ms like t3code's
  latest-mode scheduler), `close()`. Reconnect only for abnormal closes and
  only while the component wants to stay attached; reset xterm (`\x1bc`)
  before re-attach so the zmx redraw lands on a clean grid. Component:
  `Terminal` with `scrollback: 5000`, `cursorBlink: true`, `fontSize` ~12.5,
  font stack matching gpui terminals (check what font gpui uses — search
  `font_family` defaults in `gpui/src`; fall back SF Mono/JetBrains Mono
  stack), `theme` = default Ghostty theme extracted from gpui: find the
  default/embedded theme gpui applies (search `gpui/src` for the theme
  constants added by commit "apply embedded Ghostty themes"; extract the
  16 ANSI colors + bg/fg/cursor/selection into
  `terminal/ghostty-default-theme.ts` with a comment naming the source).
  `attachCustomKeyEventHandler` passthrough for app-level hotkeys (leave a
  hook prop). Wire fit-addon on container resize (ResizeObserver) → client
  `resize`. Export also `searchNext/searchPrev` via the search addon for the
  Phase 6 find bar. Include a dev harness route or debug flag rendering one
  terminal against hardcoded ids for manual testing.
- acceptance_criteria:
  - `bun run web:typecheck` + `bun run web:build` pass.
  - Code implements: binary in/out frames, JSON control frames per pinned
    protocol, coalesced resize, backoff reconnect with xterm reset, webgl
    try/catch enhancement, search addon exports, ghostty-default theme file
    with sourced colors (verifier will diff the palette against the gpui
    source you cite).
  - If Phase 1 is live on the dev instance by the time you finish, run one
    manual end-to-end echo check and note it; otherwise note that live check
    was not possible.

## Phase 8: integration — attach flow, command pane, end-to-end

- depends_on: [1, 2, 4, 5, 6, 7]
- parallel_ok: false
- goal: Wire everything into the working product: sidebar session click →
  gpui-parity placement → attach flow → live terminal; wake/retry/
  materialize placeholder actions; find-in-terminal wired to the search
  addon; the bottom command pane; multi-machine terminal routing; layout
  reconciliation against live presentation state; final build + README.
- files: `ghostex-web/src/**` (integration glue, `app/` wiring), may touch
  workspace/terminal/sidebar-runtime/connections modules for integration
  seams; `ghostex-web/README.md`.
- do_not_touch: root `package.json`/`bun.lock` (deps frozen), `gxserver-rs/**`
  except true blocking bugs found in Phases 1–2 code (fix root cause, note it),
  `gpui/**`, `native/**`.
- approach:
  - Attach flow (mirror gpui `receive_sidebar_workspace_terminal_focus_payload`
    ~35190 and the attach-plan fn ~77933): on `ghostex-web:focusSession` —
    if session already open in a pane → select that tab; else target pane =
    focused pane (fork placement: source session's pane), then call the
    owning machine's rpc: lifecycle `sleeping` → `/api/wakeSession` else
    `/api/attachSessionMetadata`; if response says provider must start →
    `/api/startSessionProvider` then re-fetch attach metadata; then insert a
    Running tab and mount `SessionTerminal` with that machine's
    baseUrl/token. `startupText` with disposition `queueAfterTerminalReady`
    and `persistenceSessionCreated`: deliver AFTER the terminal `ready`
    via HTTP `/api/sendSessionText` then `/api/sendSessionEnter` (NEVER type
    it into the WS — known input-visibility pitfall). Handle `restoreBlocked`
    by showing the StartupFailed placeholder with the reason.
  - Placeholder actions: Wake → wakeSession then attach; Retry/Materialize →
    attach path again.
  - Reconciliation: subscribe workspace to presentation state — session
    removed → drop tab; lifecycle changes update tab badges/dots and swap
    terminal↔placeholder bodies (do not gate attention logic on local
    activity; presentation state is authoritative).
  - Command pane (mirror gpui `render_workspace_with_command_pane` ~52118,
    `gpui_command_terminal_create_session_params` ~78010, and close semantics
    — close acts directly, no deferred confirm): bottom collapsible strip
    per ACTIVE PROJECT with its own small tab row; new command terminal =
    `/api/createSession` with `launchSettings.surface:"commands"` +
    `providerState.provider:"zmx"` then the same attach flow; switching
    active project parks the strip's sessions and swaps in the new project's
    (sessions keep running server-side); close kills the session via rpc.
    Active project = sidebar's active project context (adapter exposes it).
  - Find-in-terminal: pane find bar ↔ active tab's search addon
    (next/prev/highlight, Esc closes).
  - README: build (`bun run web:build`), launch (`ghostex web`), dev
    (`bun run web:dev`), add-machine + CORS notes for tailscale origins.
  - Full manual pass against the local gxserver; list what you exercised.
- acceptance_criteria:
  - `bun run web:typecheck` + `bun run web:build` pass; built app served by
    the Phase 2 static handler loads at `http://127.0.0.1:58799/` on an
    isolated dev instance (or via `web:dev` proxy against the real local
    gxserver — read-only actions only against the real one; do NOT
    kill/sleep/create sessions on the user's real daemon beyond one scratch
    terminal session you create and close yourself).
  - Clicking an existing running session in the sidebar opens a tab in the
    focused pane and shows the live terminal (echo test typed + visible).
  - A sleeping session shows the Sleeping card; Wake attaches it.
  - Command pane creates a scratch commands-surface session, shows it live,
    close kills it (verify via `ghostex sessions --json`).
  - Find bar highlights matches in the active terminal.
  - README present and accurate.

## Handoff notes

(appended by the orchestrator as phases complete)

- Phase 3 COMPLETE: ghostex-web scaffold landed — Vite + React 19 + TanStack
  Router (file-based routes) + React Compiler active, deps added to root
  package.json, scripts `web:dev`/`web:build`/`web:typecheck` work. Dark
  gpui-style shell with collapsible/resizable sidebar region (persisted),
  titlebar with center tabs + right controls present but hidden-gated.
  Build outputs hashed assets to `ghostex-web/dist`.

- Phase 1 COMPLETE: `/api/terminal` WebSocket landed in
  `gxserver-rs/src/terminal_ws.rs` + route wiring in `server.rs` (local
  listener only, upgrade-first error frames, no auto-wake). Bidirectional
  PTY streaming with resize/exit/cleanup via `portable-pty`. Additive TS
  contracts (terminal WS control-message types + endpoint constant) exported
  from `shared/gxserver-protocol.ts`. cargo check + isolated port-58799
  smoke passed; zmx attach child count returned to baseline after close.

- Phase 7 COMPLETE: `ghostex-web/src/terminal/` — `SessionTerminal` +
  `TerminalWsClient` per the pinned protocol (binary I/O, JSON control
  frames, ~50ms coalesced resize, [1,2,4,8,16]s reconnect with xterm reset,
  WebGL try/catch enhancement, search addon exports). Theme: gpui JetBrains
  Mono defaults + embedded Ghostty GitHub Dark palette in
  `terminal/ghostty-default-theme.ts`. web:typecheck/web:build pass. NOTE:
  no live end-to-end echo test was run (no dev instance up at the time) —
  Phase 8 must do the live terminal check.

- Phase 6 COMPLETE: `ghostex-web/src/workspace/` — pure split-tree reducer
  (`workspace-model.ts`: split/rotate/merge/move-tab/close-collapse/focus +
  reconciliation hook) and pane/tab UI (tab strips with status dots +
  lifecycle badges, pane-actions + tab context menus, focus mode, find bar
  shell, draggable dividers, cross-pane tab drag, placeholder cards,
  localStorage persistence). Terminal body is pluggable via
  `renderTerminalBody(tab)`. Debug seeding: `?workspaceDebug=1`. All
  interactions demonstrated in Chrome; web:typecheck/web:build pass.

- Phase 2 COMPLETE: local-listener static SPA serving (traversal-safe path
  resolution, SPA fallback to index.html, content types, cache headers,
  missing-build guidance page), same-origin-only `/api/webBootstrap` (no
  ACAO ever), loopback CORS extension (http/https localhost + IPv4/IPv6 any
  port), `web.distDir` config, `GxserverWebBootstrapResult` TS contract, and
  `ghostex web` CLI opener. cargo check + isolated port-58799 curl smokes
  passed (traversal rejected, evil-origin 403, preflight regex works).

- Phase 4 COMPLETE: `ghostex-web/src/connections/` + `src/machines/` —
  persisted machine catalog (`ghostexWeb.machines.v1`) with primary
  bootstrap via `/api/webBootstrap`, machine-scoped ids, authenticated rpc +
  health probe, presentation snapshot + delta cache + `/api/events`
  subscription per machine, connection-status store with online/offline
  handling and capped backoff, titlebar Machines modal (add/remove/validate/
  status dots). Typecheck/build pass; live runtime checks against the real
  local gxserver and a second machine succeeded.

- Phase 5 COMPLETE: `ghostex-web/src/sidebar-runtime/` — real shared
  `SidebarApp` mounted with web theme/styles; multi-machine projection with
  machine-scoped ids, chat grouping, status + HUD; UI→host routing for
  focus (`ghostex-web:focusSession` app event), lifecycle (wake/sleep/kill/
  close), fork, create, rename, attention-acknowledge, project actions;
  native-only actions are explicit adapter no-ops. `git diff sidebar/` is
  EMPTY (no gated edits were needed). Typecheck/build pass; live CLI/browser
  ordering parity verified against the local gxserver.

- Phase 8 COMPLETE: integration glue — sidebar focus → gpui-parity placement
  → wake/attach/startProvider sequencing → live terminal; startupText via
  HTTP sendSessionText/sendSessionEnter after terminal ready; machine-scoped
  terminal routing; lifecycle reconciliation (tab badges, terminal↔placeholder
  swap, removed-session tab drop); wake/retry/materialize actions; find bar
  wired to search addon; per-project command pane (create/park/swap/direct
  close); `ghostex-web/README.md`. Typecheck, production build, static
  serving, echo, find, sleep/wake, and zero-session cleanup all passed on an
  isolated gxserver instance; real daemon untouched.

## Round 2 user-feedback tasks (T1, T2)

User feedback after first live use. Same repo rules as the header. Current
deploy note: the SPA is served by the REAL local gxserver from
`ghostex-web/dist` (config `web.distDir`); rebuilding with `bun run web:build`
is enough to update it — do NOT restart the real daemon.

### T1: web layout fill + command pane UX + titlebar Actions button

- files: `ghostex-web/src/**` EXCEPT `ghostex-web/src/sidebar-runtime/**`
  (read-only for you; if you need data from it, import existing exports or
  build your own fetch in a new module via the connections layer).
- do_not_touch: `ghostex-web/src/sidebar-runtime/**`, `sidebar/**`,
  `native/**`, `gxserver-rs/**`, root `package.json`/`bun.lock`.
- goal, three parts:
  1. FIX: the main workspace does not fill the viewport height — a large
     black band shows under the pane area and above the command strip (see
     the user screenshot description: pane content ~600px tall, black below).
     The app shell column must make the Agents workspace region flex to fill
     all height not used by titlebar/command pane; panes and terminals fill
     their leaf; xterm refits on size change. Beware display:block defaults —
     the repo has a known pitfall where a bare div collapses flex children
     to 0/auto height. Verify by rendering: no black band, terminal rows
     reach the bottom.
  2. Command pane UX rework:
     - Hidden by default: NO strip at the bottom at all unless the pane is
       open. Remove the current always-visible collapsed "Commands" strip.
     - Header must match the gpui command pane: a slim dark header row with
       bold label "Command Terminal", session tabs inline after the label,
       a "+" (new command terminal) button after the tabs, and a
       right-aligned chevron-down button that hides the pane. Reference
       gpui rendering: `render_workspace_with_command_pane` ~52118 in
       `gpui/src/main.rs`.
     - Resizable: dragging the top edge of the pane resizes its height
       (min ~120px, max ~70% viewport), persisted to localStorage.
  3. Titlebar Actions button (like gpui): add a button in the titlebar right
     cluster (un-hide just this one from the hidden gate; play/bolt icon —
     gpui uses `gpui/assets/titlebar/bolt.svg`, copy the SVG shape). Clicking
     opens a dropdown listing the active project's HUD command actions from
     `POST /api/readSidebarHud` `{activeProjectId}` → `commands`
     (`GxserverSidebarHudCommandButton` in `shared/gxserver-protocol.ts`).
     Selecting an action with `actionType:"terminal"`: create a command
     session on the owning machine (study
     `gpui_command_terminal_create_session_params` ~78010 in
     `gpui/src/main.rs` — `/api/createSession` with
     `launchSettings.surface:"commands"` and the action command), attach it,
     and OPEN the command pane showing it. `actionType:"browser"` actions
     open their `url` in a new tab. The command pane only ever appears via
     this flow (or the chevron re-open affordance you add to the Actions
     dropdown when command sessions exist).
- acceptance: `bun run web:typecheck` + `bun run web:build` pass; state in
  your completion summary the manual browser checks you performed (workspace
  fills height with no black band, command pane hidden by default, action
  launch opens styled resizable pane).

### T2: Recent Projects as a bottom section per machine (web + macOS/shared)

- files: `sidebar/**` (shared SidebarApp — targeted edits, this changes ALL
  hosts including macOS and gpui, which is EXPLICITLY wanted),
  `native/sidebar/**` if host plumbing needs it,
  `ghostex-web/src/sidebar-runtime/**` (feed per-machine recent projects),
  `gpui/sidebar/**` only if its runtime needs the same feed.
- do_not_touch: `ghostex-web/src/workspace/**`, `ghostex-web/src/terminal/**`,
  `ghostex-web/src/connections/**` (import, do not edit), `gxserver-rs/**`,
  root `package.json`/`bun.lock`, `gpui/src/**`.
- goal: Replace the current "Recent Projects" UI (the collapsible HUD area
  with its fuzzy search at the bottom of the sidebar, driven by
  `state.hud.recentProjects` — see `sidebar/sidebar-app.tsx` ~826-960 and
  ~2429) with a plain "Recent Projects" SECTION rendered BELOW all other
  project sections, per machine: in the web app each machine's section list
  ends with that machine's Recent Projects; in the single-machine macOS/gpui
  sidebar it is the last section. Rows look like normal project section rows
  (title, open-on-click restoring/opening the recent project via the same
  action the old UI used). Remove the old bottom HUD area cleanly (no dead
  code left). Data: gxserver `/api/listRecentProjects` per machine for web;
  macOS/gpui keep their existing recentProjects state feed.
- Existing tests in `sidebar/`/`native/sidebar/` that fail because of this
  INTENDED behavior change: update them to the new behavior or delete them
  if obsolete; add no new tests.
- acceptance: `bun run web:typecheck` + `bun run web:build` pass; repo
  typecheck for the shared/native side no worse than before; completion
  summary lists what renders where now.

## Verifier findings — round 1 (fix these)

- FINDING 1: phase=8 criterion="Find bar highlights matches in the active
  terminal". Evidence: `ghostex-web/src/terminal/session-terminal.tsx:142-154`
  — Terminal options lack `allowProposedApi`; browser error "You must set the
  allowProposedApi option to true to use proposed API" thrown from
  SearchAddon `_createResultDecorations` → `registerDecoration` whenever the
  query matches. Fix: add `allowProposedApi: true` to the `new Terminal({...})`
  options in `session-terminal.tsx`, then re-verify in a browser that
  searching a matching term renders `.xterm-decoration` highlight elements
  and no window error fires.

- FINDING 2: phase=5 criterion="sidebar shows the SAME projects/groups/
  sessions (multi-machine merge per Multi-machine model)". Evidence: debug
  log shows the second machine's group published (sessionCount 1, title
  secondmachineproj) but document.body contains no such text while both
  machines show machines-status--connected. Root cause:
  `sidebar/sidebar-app.tsx:2358` + `:4782-4791` render remote sections only
  from `settings.remoteMachines`, and `rg remoteMachines ghostex-web/src`
  returns no matches. Fix: in
  `ghostex-web/src/sidebar-runtime/sidebar-runtime.ts`, include a
  `remoteMachines` array (RemoteMachineSettings entries per
  `shared/ghostex-settings.ts:1059`, id=machineId, name=machine label) in
  the settings payload hydrated to SidebarApp, derived from all non-"local"
  machines in the connection registry; re-verify a second machine's projects
  render as a remote section in the sidebar. Do NOT edit `sidebar/**`.
