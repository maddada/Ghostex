# gxserver operations

<!--
CDXC:GxserverOperations 2026-05-30-16:10:
gxserver must be operable as the app-launched local daemon and as a standalone server-only install. Keep this guide focused on the implemented commands, paths, dependency rules, and troubleshooting states so users and agents do not infer a macOS-app-owned backend or a hidden Node/zmx fallback.
-->

This guide covers running, installing, and troubleshooting `gxserver`. The daemon is the Ghostex backend server. It is separate from the user-facing `gx`/`ghostex` CLI clients.

## Local development

From the repository root:

```sh
cd gxserver
npm install
npm run check
```

Run the daemon in the foreground:

```sh
gxserver
```

Run it in the background:

```sh
gxserver start
gxserver status --json
gxserver stop
gxserver stop-all
```

`gxserver stop` stops only the control plane. It must not kill zmx sessions, shells, or agent processes. Existing zmx sessions remain attachable after a daemon restart.

`gxserver stop-all` is destructive: it kills gxserver-tracked zmx provider sessions, marks killed sessions stopped, and then stops the control plane.

## App-launched mode

The macOS app starts or reuses local `gxserver` on launch. Closing the app does not stop the daemon. The app remains responsible for UI, layout, window chrome, and the native Ghostty renderer; `gxserver` owns shared backend state and lifecycle decisions.

The macOS dev native bridge must not use port `58744`. That port belongs to `gxserver`. Production bridge traffic stays on `58743`; the dev bridge uses `58742`.

## Paths

Server-owned state lives under:

```text
~/.ghostex/gxserver
```

Important paths:

- `auth/token`: local bearer token generated on first launch.
- `identity.json`: stable server identity.
- `state.db`: SQLite state.
- `logs/gxserver.jsonl`: structured JSONL logs.
- `runtime/server.json`: runtime metadata for status/start/reuse.
- `migrations/`: idempotent import markers.
- `zmx/`: daemon-owned zmx runtime metadata.

Client-local connection metadata lives at:

```text
~/.ghostex/clients/connections.json
```

Secrets for remote servers are stored through the OS credential store. There is no plaintext token fallback.

## Auth

All non-minimal APIs require auth. Local clients read `~/.ghostex/gxserver/auth/token` directly. `GET /api/health` is the minimal unauthenticated health endpoint:

```sh
curl -fsS http://127.0.0.1:58744/api/health
```

Authenticated requests must include the local token and the expected protocol version. A protocol mismatch is a hard failure and means Ghostex and `gxserver` need to be updated together.

## Logs

Logs are JSONL with camelCase fields such as `ts`, `level`, `event`, `serverId`, `requestId`, `projectId`, `sessionId`, `client`, `durationMs`, and `error`.

Use the API-backed CLI path when the server is running:

```sh
gx logs
```

If the local server is down, the CLI can read the local JSONL file directly. Remote logs are accessed through API/tunnel flows, not filesystem scraping.

## Server-only package

Build the standalone server package from `gxserver/`:

```sh
npm run build
npm run package:server
npm run package:check
```

The server package is staged under:

```text
gxserver/dist/server-package
```

The tarball is:

```text
gxserver/dist/gxserver-<version>-server.tar.gz
```

The generated Homebrew helper is:

```text
gxserver/dist/homebrew/gxserver.rb
```

The package uses system Node and declares Node 22 or newer. It does not bundle Node or macOS UI resources. It bundles the pinned `zmx`, `zehn`, and upstream Beads `bd` artifacts.

## App package

Build the app package staging directory from `gxserver/`:

```sh
npm run build
npm run package:app
npm run package:check
```

The macOS app build copies the staged package into:

```text
Contents/Resources/Web/gxserver
```

The app-launched and standalone daemon paths use the same compiled daemon code.

## Dependencies

- Node: system Node 22 LTS or newer. Ghostex does not install Node automatically.
- zmx: bundled from the pinned submodule. Ghostex-managed zmx sessions must not use PATH `zmx`.
- zehn: bundled from the pinned submodule.
- Beads: bundled from the checksum-pinned official v1.1.2 platform archive. Ghostex ignores shell-installed `bd`; packaging stages and smoke-tests that artifact, and shell workflows should use `gx bd`.

If bundled `zmx` is missing, zmx-backed attach metadata must fail clearly. Falling back to PATH `zmx` is not allowed because the pinned fork carries Ghostex refresh behavior.

## Remote Servers

Remote/headless servers can be reached by direct trusted-network listener, Tailscale, or SSH helper.

SSH profiles use `ssh://user@host`. The client can plan:

- checking remote `gxserver` status,
- starting remote `gxserver`,
- forwarding the remote local API,
- running remote `zmx attach`.

Remote attach is by running `zmx attach` on the remote host, not by streaming PTY bytes through `gxserver`.

Remote APIs remain typed and limited. They do not expose generic `runProcess`, auth/listener mutation, tool installation, raw filesystem browsing outside known project roots, or destructive admin actions by default.

## Migration

On first launch after update, `gxserver` imports legacy macOS/sidebar state into SQLite. The import is idempotent and records a marker so it does not run again after success. Legacy files are read but left untouched.

Imported data includes projects, active/sleeping sessions, provider metadata, previous-session hidden restore links, runtime settings, agents, custom commands, worktree/Git/project-board config, and relevant backend/debug logs.

## Troubleshooting

Node missing or too old:

- Install Node 22 LTS or newer from `https://nodejs.org/en/download`.
- Re-run `gxserver status --json`.

Port conflict on `58744`:

- `gxserver` checks whether the listener is a compatible daemon.
- If it is not compatible, stop the conflicting process or update Ghostex and `gxserver` together.

Auth failure:

- Check `~/.ghostex/gxserver/auth/token`.
- Restart `gxserver` if the token file was deleted.

Protocol mismatch:

- Update Ghostex and `gxserver` together.
- Do not add compatibility fallback behavior.

Bundled zmx missing:

- Build the pinned zmx submodule before packaging.
- Do not use PATH `zmx` for Ghostex-managed sessions.

Beads missing:

- Rebuild or reinstall Ghostex so the bundled `bd` artifact is staged.
- Source checkouts must run the checksum-verifying artifact stager; do not build an arbitrary checkout or use PATH `bd` as a fallback.

Full native macOS build blocked:

- Ensure `ghostty/macos/GhosttyKit.xcframework` is present.
- Use the Zig version required by `native/macos/ghostexHost/build-ghostex-host.sh`.
