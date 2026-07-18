# Ghostex Web

Ghostex Web is the static browser version of Ghostex's sidebar and Agents workspace. It connects directly to gxserver, opens zmx-backed terminals over the authenticated terminal WebSocket, and can merge sessions from multiple machines in one sidebar.

## Build and launch

From the repository root:

```bash
bun run web:build
ghostex web
```

The build writes `ghostex-web/dist`. `ghostex web` opens the SPA served by the local gxserver at `http://127.0.0.1:58744/`; the same-origin bootstrap supplies the local connection token to the page.

## Development

Run the Vite development server from the repository root:

```bash
bun run web:dev
```

Vite proxies HTTP and WebSocket `/api` traffic to the local gxserver on port 58744. Use `bun run web:typecheck` and `bun run web:build` before handing off changes.

## Additional machines

Use the Machines button beside the Ghostex title, then enter a label, gxserver origin, and auth token. Added machines are persisted in browser local storage under `ghostexWeb.machines.v1`, including their tokens. Each machine gets its own presentation subscription and terminal/RPC routing.

Loopback origins on `localhost`, `127.0.0.1`, and `[::1]` are accepted on any port, which covers local development and SSH port forwards. A page hosted on a Tailscale hostname or IP needs that exact origin added to `cors.allowedOrigins` in `~/.ghostex/gxserver/config.json`; the bearer token remains required. Prefer serving the page locally and adding a machine through a loopback port forward when possible.
