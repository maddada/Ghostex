# ghostex-share-relay

Ghostex Collab's share relay, `share.ghostex.dev`. An owner's gxserver keeps one outgoing
WebSocket here (`/h`) while Ghostex Collab is on and a chat is shared; a guest's browser opens the
link `/s#<key>`, gets the guest page (the GPUI web build) from this relay, and connects to
`/g/<route>`. The relay pairs them and copies frames it cannot read: the key stays in the link's
`#fragment`, and the page and gxserver lock every frame end to end with keys derived from it
(`server/src/collab/tunnel.rs`, `apps/gpui-web/www/src/collab/tunnel.js`). It keeps nothing on disk.

## Run it locally

```bash
cd server/share-relay && cargo build
cargo xtask web-build   # from the repository root: the guest page in apps/gpui-web/www/dist
server/share-relay/target/debug/ghostex-share-relay --listen 127.0.0.1:58800 --web-dist "$PWD/apps/gpui-web/www/dist"
```

Point a gxserver at it with `GHOSTEX_COLLAB_RELAY_URL=http://127.0.0.1:58800`, or
`/api/collab {op: "set", patch: {relayUrl: "http://127.0.0.1:58800"}}` on a test gxserver
(`cargo xtask test-server`). Links then read `http://127.0.0.1:58800/s#<key>`.

## Build for Linux

`server/share-relay/build-linux.sh [x64|arm64|all]` builds static musl binaries from macOS or Linux
and writes `dist/ghostex-share-relay-<version>-linux-<arch>.tar.gz` (binary, systemd unit,
Caddyfile, this README) with a `.sha256` beside each, plus
`dist/ghostex-share-relay-web-<version>.tar.gz`, the guest page, when `apps/gpui-web/www/dist`
has been built.

## Deploy (one Linux box, beside Caddy)

1. DNS: an `A` (and `AAAA`) record for `share.ghostex.dev` to the box; ports 80 and 443 open.
2. `sudo mkdir -p /opt/ghostex-share-relay/web`, unpack the binary to
   `/opt/ghostex-share-relay/ghostex-share-relay` and the guest page into
   `/opt/ghostex-share-relay/web` (its `index.html` at the top).
3. `sudo cp ghostex-share-relay.service /etc/systemd/system/ && sudo systemctl daemon-reload &&
   sudo systemctl enable --now ghostex-share-relay`.
4. Add the `Caddyfile` block to `/etc/caddy/Caddyfile` and `sudo systemctl reload caddy`.
5. Check: `curl -s https://share.ghostex.dev/healthz` answers `{"ok":true,"routes":0,"guests":0}`,
   and `https://share.ghostex.dev/s#x` shows "This link is incomplete".

Updating: replace the binary or the `web` folder and `sudo systemctl restart ghostex-share-relay`;
shared chats reconnect on their own within seconds.

## Flags

- `--listen <addr>` (default `127.0.0.1:58800`).
- `--web-dist <folder>`: the guest page; without it `/s` answers 404.
- `--behind-proxy`: take the client's address from `X-Forwarded-For` (Caddy sets it), for the rate
  limits. Leave it off when nothing is in front of the relay.

## Limits

Per client address: 60 guest sockets a minute and 20 open at once; 30 owner sockets a minute and
8 open at once. Per shared chat: 120 guest sockets a minute and 32 guests at once. One owner
socket registers at most 64 chats. A frame is at most 32 MB. The owner's gxserver adds its own:
10 guests per chat, 5 wrong codes per connection and 20 per chat before codes stop for 15 minutes,
6 messages a minute per guest, 20 held messages per chat, 40 requests a second per guest page.
