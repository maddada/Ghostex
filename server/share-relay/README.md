# ghostex-share-relay

Ghostex Collab's share relay, `share.ghostex.dev`. An owner's Collab component (`server/collab`, the
sidecar gxserver runs once Collab is installed) keeps one outgoing WebSocket here (`/h`) while a
chat is shared; a guest's browser opens the
link `/s/<page id>/#<key>`, gets the guest page (the GPUI web build) from this relay, and connects to
`/g/<route>`. The relay pairs them and copies frames it cannot read: the key stays in the link's
`#fragment`, and the page and the component lock every frame end to end with keys derived from it
(`server/collab/src/tunnel.rs`, `apps/gpui-web/www/src/collab/tunnel.js`). It keeps nothing on disk.

## The pinned guest page

Each Collab component release trusts one guest page (GitHub #495): `server/collab/src/guest_page.manifest`
lists every file of the page with its SHA-256, and the page hash is the SHA-256 of that manifest.
The relay keeps each page in `--pages <folder>/<page hash>/`, checks at start that the folder's
files hash to its name (a folder that doesn't is not served), and serves it at
`/s/<first 16 hex of the hash>/`. Several releases' pages live side by side; add a page, never
replace one.

Before the component shares a chat through this relay it downloads that page, file by file, and
compares it with its manifest; any difference and it registers no shared chat, so no guest is let
in through a changed page. It checks again every hour while sharing. In the browser, `index.html`
carries the script's integrity hash and the script carries the WebAssembly's SHA-256, so a page
that starts from the right `index.html` runs only the release's code.

What that does not cover: an operator who serves the right page to the owner's checks and a
changed one to a guest's browser. A browser can't prove which page it got. Anyone can check by hand:

```bash
mkdir page && cd page   # then download index.html and every file it names from /s/<page id>/
find . -type f | sed 's|^\./||' | LC_ALL=C sort | while IFS= read -r f; do shasum -a 256 "$f"; done | shasum -a 256
```

The result must equal the page hash in the release's `guest_page.manifest` (`shasum -a 256 server/collab/src/guest_page.manifest`).

## Run it locally

```bash
cd server/share-relay && cargo build
cargo xtask web-build   # from the repository root: the guest page in apps/gpui-web/www/dist
server/share-relay/target/debug/ghostex-share-relay --listen 127.0.0.1:58800 --dev-page "$PWD/apps/gpui-web/www/dist"
```

Start a debug gxserver (`cargo xtask test-server`, which passes `GHOSTEX_COLLAB_*` through) with
`GHOSTEX_COLLAB_RELAY_URL=http://127.0.0.1:58800`, `GHOSTEX_COLLAB_UNPINNED_GUEST_PAGE=1` (debug
builds only) and `GHOSTEX_COLLAB_DEV_BINARY=<target>/debug/ghostex-collab` (`cargo build` in
`server/collab`; Settings > Extensions > Collab then installs that binary): links then read
`http://127.0.0.1:58800/s/dev/#<key>` and skip the page check. To test the pinned path instead,
run `build-linux.sh web`, unpack the page tarball into a folder and pass it as `--pages`.

## Build for Linux

`server/share-relay/build-linux.sh [x64|arm64|all|web]` builds static musl binaries from macOS or
Linux and writes `dist/ghostex-share-relay-<version>-linux-<arch>.tar.gz` (binary, systemd unit,
Caddyfile, this README) with a `.sha256` beside each. When `apps/gpui-web/www/dist` has been built
it also writes `dist/ghostex-share-relay-web-<version>.tar.gz` (one folder named by the page hash),
its `.sha256` and its `.manifest`, and copies the manifest to `server/collab/src/guest_page.manifest`:
commit that file in the release that should use the page, and deploy the page before that release
ships.

## Deploy (one Linux box, beside Caddy)

1. DNS: an `A` (and `AAAA`) record for `share.ghostex.dev` to the box; ports 80 and 443 open.
2. `sudo mkdir -p /opt/ghostex-share-relay/pages /opt/ghostex-share-relay/web`, unpack the binary to
   `/opt/ghostex-share-relay/ghostex-share-relay`, each release's page tarball into
   `/opt/ghostex-share-relay/pages` (it unpacks as `<page hash>/`), and 11.0.0's page (the
   0.1.x web tarball) into `/opt/ghostex-share-relay/web` for links made before 11.0.1.
3. `sudo cp ghostex-share-relay.service /etc/systemd/system/ && sudo systemctl daemon-reload &&
   sudo systemctl enable --now ghostex-share-relay`.
4. Add the `Caddyfile` block to `/etc/caddy/Caddyfile` and `sudo systemctl reload caddy`.
5. Check: `curl -s https://share.ghostex.dev/healthz` answers `{"ok":true,"routes":0,"guests":0}`,
   `journalctl -u ghostex-share-relay` names each page it serves (and any folder it refused), and
   `https://share.ghostex.dev/s/<page id>/` shows "This link is incomplete".

Updating: replace the binary, or add a page folder, and `sudo systemctl restart ghostex-share-relay`;
shared chats reconnect on their own within seconds.

## Flags

- `--listen <addr>` (default `127.0.0.1:58800`).
- `--pages <folder>`: the pinned guest pages, one folder per page hash (checked at start).
- `--web-dist <folder>`: 11.0.0's guest page at `/s`; without it `/s` answers 404.
- `--dev-page <folder>`: a development page at `/s/dev/`, unchecked.
- `--behind-proxy`: take the client's address from `X-Forwarded-For` (Caddy sets it), for the rate
  limits. Leave it off when nothing is in front of the relay.

## Limits

Per client address: 60 guest sockets a minute and 20 open at once; 30 owner sockets a minute and
8 open at once. Per shared chat: 120 guest sockets a minute and 32 guests at once. One owner
socket registers at most 64 chats. A frame is at most 32 MB. The owner's gxserver adds its own:
10 guests per chat, an eight-digit code always, 5 wrong codes per connection and 20 per chat before codes stop for 15 minutes,
6 messages a minute per guest, 20 held messages per chat, 40 requests a second per guest page.
