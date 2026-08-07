# gxserver architecture and protocol baseline

> **Status note (2026-07-29).** This document was written while the macOS
> Swift/AppKit app was the desktop client and gxserver was a TypeScript package
> under `gxserver/`. Both have moved on: gxserver is now the Rust implementation
> in `gxserver-rs/`, and the macOS Swift/AppKit app (`native/`, `src/`) is
> deprecated — kept in-tree for reference, not a development target. The active
> Ghostex apps are the gpui desktop app (`gpui/`), the web app
> (`ghostex-web/`), and the React Native/Expo mobile app (`mobile/`). Read
> "the macOS app" below as "the active desktop client", and treat the
> Node/TypeScript packaging details as historical.

This document is the source of truth for the gxserver hard-cutover architecture. Future gxserver beads should update this document and the shared TypeScript protocol types when requirements change.

Operational commands, install paths, server-only packaging, remote setup, and troubleshooting live in [gxserver operations](./gxserver-operations.md).

## Naming and cutover

- The daemon/server is named `gxserver` everywhere: package, binary, docs, API product field, runtime metadata, logs, and protocol types.
- `gxserver` is not the `gx` CLI. The `gx` and `ghostex` CLIs remain user-facing clients that talk to gxserver.
- This branch is a hard cutover. Do not add compatibility mode, a gradual dual backend, or fallback paths that hide incorrect behavior.
- Preserve existing macOS behavior exactly when behavior is derivable from current code. Inspect `native/sidebar` and `native/macos` before changing those flows.
- Closing the macOS app must not stop gxserver. Client actions should restart gxserver when it is missing.

## Process and packaging

- Milestone 1 uses TypeScript on system Node, not Bun.
- gxserver requires Node 22 LTS or newer.
- Ghostex must not bundle, privately install, or auto-install Node. If Node is missing or too old, clients show a clear explanation with install guidance for Node 22 LTS or newer.
- The top-level package is `gxserver/` with its own `package.json`.
- Build output belongs only in `gxserver/dist/`.
- Server-only packaging stages the shared daemon package under `gxserver/dist/server-package`, with compiled JS, package manifests, a system-Node `bin/gxserver` launcher, and bundled pinned `bin/zmx`, `bin/zehn`, and upstream Beads `bin/bd` artifacts.
- Homebrew/server-only helper output lives under `gxserver/dist/homebrew`, declares `node@22`, installs only the headless gxserver package, and must not reference the macOS app bundle, AppKit, WebKit, Xcode, or CEF UI resources.
- The macOS app copies the same staged server package into `Contents/Resources/Web/gxserver` so app-launched gxserver and standalone gxserver execute the same daemon code. The app may start gxserver when missing, but it does not own shutdown.
- Direct `gxserver` runs the server in the foreground.
- `gxserver start` launches the server in the background.
- `gxserver stop` stops only the gxserver control plane. It must not kill zmx, tmux, zellij, shell, or agent sessions.
- `gxserver stop-all` is the explicit destructive shutdown path. It kills gxserver-tracked zmx sessions before stopping the control plane.
- `gxserver status` reports runtime state.
- The local full API listens on fixed port `58744`. The current macOS bridge remains `58743`.
- Runtime metadata lives at `~/.ghostex/gxserver/runtime/server.json` and includes `port`, `pid`, `serverId`, `startedAt`, `version`, and `protocolVersion`.
- On port conflict, gxserver checks `/api/health/server`. A compatible gxserver can be reused; a non-gxserver listener or protocol mismatch fails clearly.

## Ownership split

gxserver owns shared backend state:

- Projects.
- Terminal and agent sessions.
- zmx session identity and lifecycle.
- Sleep/wake policy.
- Agent status and activity.
- CLI and mobile remote control.
- Pinned and favorite state.
- Custom agents, custom commands, and their order.
- Launch and runtime settings.
- Previous-session history and archive metadata.
- Worktree lifecycle and typed Git actions.
- Beads and project-board operations.

Clients own local presentation state:

- Sidebar groups.
- Split and tab layout.
- Visible session count.
- Browser, editor, and code-server panes.
- CEF/browser profiles, toolbars, and devtools.
- Pop-out windows and client chrome.
- Visual-only settings.

Mixed ownership:

- Notification rules are shared; delivery is local.
- Command definitions are shared; panel chrome is local.
- Project icons are shared when they are identity-level metadata, otherwise local.
- Theme is local by default unless intentionally made shared later.

## Storage, auth, and logs

- gxserver storage root is `~/.ghostex/gxserver`.
- Planned storage paths are `auth/token`, `config.json`, `state.db`, `logs/gxserver.jsonl`, `runtime/`, `zmx/`, `migrations/`, and `identity.json`.
- SQLite is the durable state store.
- All non-minimal APIs require auth.
- Local desktop and CLI clients read the server token file directly.
- Remote connection tokens are stored by the client in the OS credential store in later work.
- Logs are JSONL with camelCase fields such as `ts`, `level`, `event`, `serverId`, `requestId`, `projectId`, `sessionId`, `client`, `durationMs`, and `error`.

## IDs and refs

- `serverId`: capital `S` plus two body chars; first body char numeric, second lowercase letter or digit, for example `S7k`.
- `projectId`: capital `P` plus four body chars; first body char numeric, rest lowercase letters or digits, for example `P3a91`.
- `sessionId`: capital `G` plus four body chars; first body char numeric, rest lowercase letters or digits, for example `G8v20`.
- Global refs look like `S7k:P3a91:G8v20`.
- zmx session names avoid colons and use `P3a91-G8v20`.
- Session IDs are unique per project and immutable.
- Project IDs are unique per server.
- Server IDs are stable daemon identities.
- Project and server IDs are generated by gxserver and survive move/rename.
- The sidebar shows the full `G...` session ID. Session titles remain separate and user-editable.

## zmx boundary

- gxserver owns Ghostex-managed zmx identity and lifecycle decisions.
- zmx attach is a backend boundary. Clients request attach metadata or control actions from gxserver instead of reconstructing zmx names independently.
- `gxserver stop` stops only gxserver's control plane and must leave zmx-backed terminal sessions alive.
- Bundled zmx resolution and failure behavior are implemented by later packaging/zmx beads. Do not add PATH fallback behavior as a substitute for the required bundled zmx path.

## Server-only install

Build and check the headless package from `gxserver/`:

```sh
npm install
npm run build
npm run package:server
npm run package:check
```

The package artifact is `gxserver/dist/gxserver-<version>-server.tar.gz`, and the generated Homebrew formula helper is `gxserver/dist/homebrew/gxserver.rb`. A server-only install can start and inspect gxserver with:

```sh
gxserver start
gxserver status --json
curl -fsS http://127.0.0.1:58744/api/health
```

Remote/headless hosts still need system Node 22 LTS or newer. Project board features require bundled `bin/bd`; shell-installed `bd` is intentionally ignored so all Ghostex and agent workflows share the same checksum-pinned official Beads v1.1.2 binary. The official Linux artifact uses glibc 2.34 or newer so its CGO-enabled embedded Dolt backend works without an external server.

When installing the tarball directly instead of using the Homebrew helper, run `npm ci --omit=dev --no-audit --no-fund` in the extracted package before invoking `bin/gxserver`. Homebrew performs that production dependency install with its declared `node@22` dependency.

## API and protocol

- Shared TypeScript protocol types live in `gxserver/protocol` and are re-exported for existing repo clients from `shared/gxserver-protocol.ts`.
- JSON fields are camelCase.
- Endpoint path tokens are camelCase, for example `/api/createSession`.
- RPC endpoints are mostly `POST`.
- `GET /api/health` is the minimal unauthenticated health endpoint.
- `GET /api/health/server` is the local identity endpoint used by lifecycle checks.
- `/api/events` is the WebSocket event stream endpoint.
- Keep OpenAPI generation out of milestone 1.
- Protocol mismatch hard fails and asks the user to update Ghostex/gxserver. Do not silently downgrade or retry through a compatibility API.

## Remote access

- Local full API remains on `127.0.0.1:58744`.
- A separate remote/Tailscale listener, such as `58745`, is default off and added by later work.
- Remote APIs are deliberately narrower than local APIs: project/session listing, text/status read, text/message/enter send, terminal/agent creation, focus/sleep/wake, attach metadata, health/capabilities, typed Git/worktree/Beads workflows, and add-project-path are allowed.
- Remote APIs block generic process execution, auth/listener mutation, tool installation, raw filesystem browsing outside known project roots, and destructive admin actions by default.

## Migration

- Migration is a one-way hard cutover into gxserver-owned state.
- Migration preserves current behavior and user-visible identity whenever it is derivable from existing macOS/sidebar state.
- Migration must not keep two authoritative backends running.
- If required migration state is missing or inconsistent, fail clearly and ask for a repair/update path instead of creating compatibility fallbacks.

## Behavior-preservation rule

When moving behavior from the macOS app into gxserver, first identify the current behavior in `native/sidebar` and `native/macos`, then encode the same behavior in protocol, storage, and server code. User-visible behavior changes need an explicit requirement update and a CDXC comment near the code that implements the changed requirement.
