# voxl

[![crates.io](https://img.shields.io/crates/v/voxl-server.svg)](https://crates.io/crates/voxl-server)

Low-latency desktop streaming for the browser, written in Rust. voxl streams an X11
display, a Mac's screen or a Windows desktop as H.264 over a WebSocket to a small web
client that decodes with WebCodecs, with input, clipboard, audio, microphone, file and
folder transfer and dynamic resolution.

## Quick start

```sh
cargo install voxl-server   # needs x264, libyuv, Opus, PulseAudio and esbuild (see Usage)
DISPLAY=:1 voxl             # streams display :1 and prints a link that signs you in
```

Open the link it prints. On a Mac or a Windows PC, `voxl` streams that machine's own
screen instead; see [macOS](#macos) and [Windows](#windows).

## Benchmarks

https://github.com/user-attachments/assets/acdf3f31-64fc-4acf-b5c5-ff173d7100a8

In every test below voxl averages 60 FPS with a median latency of 13–98 ms; noVNC
averages under 8 FPS.

A 16 vCPU AMD EPYC 9334 VM (no GPU), 2560x1600 Xvfb desktop, same workloads, each
stream decoded by a native client that reads a pixel marker drawn into the content.
Cells are FPS (worst second), then latency p50/p99 from the content being drawn to the
frame being decoded. voxl's column is its best of three runs of each test, and its
numbers (the column and the CPU row) come from a later session than the other
servers', on the same kind of VM, with x264 built with [`patches/x264`](patches/x264).
To reproduce, see [`bench/`](bench/README.md).

| Test                           | voxl                       | [selkies](https://github.com/selkies-project/selkies) | [KasmVNC](https://github.com/kasmtech/KasmVNC) | [Neko](https://github.com/m1k1o/neko) | [Neko, tuned](https://github.com/m1k1o/neko) | [Sunshine](https://github.com/LizardByte/Sunshine) | [Sunshine, tuned](https://github.com/LizardByte/Sunshine) | [noVNC](https://github.com/novnc/noVNC) | [Daytona](https://www.daytona.io) |
| ------------------------------ | -------------------------- | ----------------------------------------------------- | ---------------------------------------------- | ------------------------------------- | -------------------------------------------- | -------------------------------------------------- | --------------------------------------------------------- | --------------------------------------- | --------------------------------- |
| Chromium scroll, LAN           | **60.0 (58.1), 37/50 ms**  | 56.8 (54.0), 85/106 ms                                | 51.7 (39.5), 99/135 ms                         | 25.0 (23.7), 156/217 ms               | 53.9 (49.9), 128/155 ms                      | 28.3 (23.5), 88/115 ms                             | 48.5 (44.0), 94/117 ms                                    | 6.7 (6.3), 198/301 ms                   | 5.2 (2.5), 306/713 ms             |
| Scroll, 10 Mbps / 40 ms RTT    | **60.1 (59.6), 80/90 ms**  | 55.6 (47.7), 152/261 ms                               | 8.3 (7.4), 229/249 ms                          | 24.6 (14.9), 221/737 ms               | 46.3 (40.3), 606/831 ms                      | 27.9 (25.3), 160/179 ms                            | 51.3 (47.5), 147/162 ms                                   | 2.6 (2.4), 426/678 ms                   | 1.9 (1.6), 617/798 ms             |
| Scroll, 5 Mbps / 60 ms RTT     | **60.0 (59.2), 98/115 ms** | 27.9 (19.6), 2.7/3.7 s                                | 4.9 (3.6), 306/415 ms                          | 21.7 (3.8), 1.2/3.6 s                 | stalls                                       | 29.6 (21.9), 178/207 ms                            | 44.5 (35.2), 164/191 ms                                   | 1.6 (1.4), 637/937 ms                   | 1.1 (0.9), 1.1/1.2 s              |
| 1080p animation, LAN           | **60.0 (59.8), 13/18 ms**  | 58.7 (55.9), 31/48 ms                                 | 56.7 (51.6), 51/82 ms                          | 24.8 (23.6), 122/202 ms               | 55.6 (50.0), 54/82 ms                        | 27.1 (22.7), 69/91 ms                              | 45.6 (40.3), 42/57 ms                                     | 7.7 (7.2), 149/210 ms                   | 10.5 (8.8), 138/209 ms            |
| Animation, 10 Mbps / 40 ms RTT | **60.0 (58.1), 45/72 ms**  | 39.3 (31.8), 2.3/3.1 s                                | 52.9 (49.2), 76/115 ms                         | 25.1 (9.1), 295/962 ms                | 33.3 (25.4), 4.0/7.1 s                       | 29.7 (26.0), 108/125 ms                            | 51.1 (44.6), 86/107 ms                                    | 5.8 (5.2), 160/226 ms                   | 7.4 (7.0), 154/213 ms             |
| Animation, 5 Mbps / 60 ms RTT  | **60.0 (59.4), 60/84 ms**  | 18.4 (15.3), 2.2/4.3 s                                | 34.1 (26.4), 104/156 ms                        | 15.2 (8.6), 5.0/9.4 s                 | stalls                                       | 34.7 (25.4), 123/143 ms                            | 45.5 (40.9), 114/131 ms                                   | 5.1 (4.7), 174/240 ms                   | 5.8 (4.4), 189/252 ms             |
| Full-screen animation, LAN     | **60.0 (59.4), 16/23 ms**  | 54.8 (49.8), 54/79 ms                                 | 34.2 (32.5), 83/102 ms                         | 22.7 (18.0), 2.9/3.3 s                | 54.6 (49.7), 54/79 ms                        | 24.9 (21.7), 73/98 ms                              | 47.4 (43.4), 46/65 ms                                     | 5.9 (5.2), 184/279 ms                   | 9.1 (7.2), 154/247 ms             |

| Server CPU, X server + streamer (cores)                   | idle | 1080p animation  | scroll           | full screen      |
| --------------------------------------------------------- | ---- | ---------------- | ---------------- | ---------------- |
| voxl                                                      | 0.00 | 1.54             | 1.44             | 2.00             |
| [selkies](https://github.com/selkies-project/selkies)     | 1.29 | not measured     | 1.54             | 1.80             |
| [KasmVNC](https://github.com/kasmtech/KasmVNC)            | 0.00 | 1.97             | 2.07             | 1.82 (at 34 FPS) |
| [Neko](https://github.com/m1k1o/neko)                     | 1.63 | 2.50 (at 25 FPS) | 2.20 (at 25 FPS) | 2.46 (at 23 FPS) |
| [Neko](https://github.com/m1k1o/neko), tuned              | 1.90 | 2.28             | 2.30             | 2.52             |
| [Sunshine](https://github.com/LizardByte/Sunshine)        | 2.10 | 2.34 (at 27 FPS) | 2.24 (at 28 FPS) | 2.35 (at 25 FPS) |
| [Sunshine](https://github.com/LizardByte/Sunshine), tuned | 2.84 | 3.07 (at 46 FPS) | 3.14 (at 49 FPS) | 3.22 (at 47 FPS) |
| [noVNC](https://github.com/novnc/noVNC)                   | 0.01 | 0.55 (at 8 FPS)  | 0.58 (at 7 FPS)  | 0.64 (at 6 FPS)  |
| [Daytona](https://www.daytona.io) (1 CPU limit)           | 0.02 | 0.82 (at 10 FPS) | 0.53 (at 5 FPS)  | 0.81 (at 9 FPS)  |

- **selkies:** [selkies](https://github.com/selkies-project/selkies) with picom running.
- **KasmVNC:** 1.4.0 with a 60 FPS cap, JPEG/WebP quality 8–9 and video mode, no
  compositor.
- **Neko:** [m1k1o/neko](https://github.com/m1k1o/neko) 3.2 with its defaults (VP8, 25
  FPS). **Neko, tuned** captures at 60 FPS and encodes with x264 `ultrafast`/
  `zerolatency` at up to 30 Mbps, like voxl. Neither adapts its bitrate (Neko's
  estimator is off by default), so on slow links frames queue up; "stalls" means no
  frame arrived in 20 s.
- **Sunshine:** [Sunshine](https://github.com/LizardByte/Sunshine) 2025.628 streaming to
  Moonlight ([moonlight-embedded](https://github.com/moonlight-stream/moonlight-embedded)
  2.7, patched to hand frames to the benchmark client). Sunshine is built around GPU
  capture and encoding; without a GPU it falls back to copying the X11 screen and x264,
  so these numbers show it on a GPU-less VM like the other tests, not on the gaming PC
  it's made for. **Sunshine** runs its defaults (`superfast` on 2 threads) at Moonlight's
  default 40 Mbps; **Sunshine, tuned** uses `ultrafast` on 4 threads at 30 Mbps, like
  voxl (8 or 16 threads did no better). Neither reaches 60 FPS on this VM, and it keeps
  working while the screen is still (its idle CPU). Moonlight doesn't adapt its bitrate,
  so on slow links it's set to 80% of the link. Its CPU was measured in a later session,
  where voxl used 1.74, 1.65 and 1.99 cores on the same workloads.
- **noVNC:** x11vnc and noVNC as [E2B Desktop](https://github.com/e2b-dev/desktop)
  and Daytona ship them (`-forever -wait 50 -shared`, Tight encoding). x11vnc polls the
  screen, which caps it at 6–8 FPS.
- **Daytona:** a default sandbox (1 CPU, x11vnc and noVNC at 2560x1600) streamed
  through Daytona's signed preview proxy, about 12 ms away, which its latency includes.
  The workloads run inside the sandbox, sharing its CPU, and slow links are added on top
  of that internet path.
- **Links:** a bottleneck with a deep queue: a TCP proxy for the WebSocket servers, and
  `tc netem` for Neko's WebRTC media and Sunshine's video (UDP). None of them drop
  packets, where UDP with error correction would have the edge.

## Usage

Build (needs Rust, pkg-config, clang, x264, libyuv, libopus, libpulse and esbuild;
`nix-shell` provides them all):

```sh
nix-shell
cargo build --release
```

On Linux, `--no-default-features` leaves out PulseAudio, and with it desktop audio: a
voxl that can be linked statically (musl) and runs without PulseAudio's libraries.

`nix-shell` and `default.nix` build x264 with the patches in
[`patches/x264`](patches/x264) ([`x264.nix`](x264.nix)), which skip the work x264 does
on macroblocks voxl's damage map marks unchanged: a frame where a small window changed
encodes in about 40% less time, and the stream comes out bit for bit the same. voxl
builds against a stock x264 too, without the savings.

Or install it from [crates.io](https://crates.io/crates/voxl-server) with the same
libraries on your system (the crate is `voxl-server`, the binary is `voxl`):

```sh
cargo install voxl-server
```

Run it against an X display, then open the link it prints (`?stats` adds an FPS
overlay):

```sh
DISPLAY=:1 ./target/release/voxl --port=6901
# prints: open http://localhost:6901/?token=4f1c… to sign in
```

Start Xvfb with `-shmem` for zero-copy capture, and use the patched Xvfb
(`nix-build -E 'with import <nixpkgs> {}; callPackage ./xvfb.nix {}'`) for smooth
scrolling. voxl accepts selkies' command-line flags; other selkies flags are ignored.
`voxl --help` lists every flag with its default, and `voxl --version` prints the version.

| Flag                                                           | Default                            |                                                                                                                   |
| -------------------------------------------------------------- | ---------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| `--addr`, `--port`                                             | `0.0.0.0`, `6901`                  | listen address                                                                                                    |
| `--title`                                                      | `Voxl Computer Stream`             | the page's title: the browser tab, and the app's name if a viewer installs it                                     |
| `--favicon`                                                    | voxl's icon                        | the tab's icon: an `.ico`, `.png`, `.gif`, `.webp` or `.svg` file                                                 |
| `--enable-basic-auth`                                          | `true`                             | ask for a password (see [Login](#login)); `false` serves without one                                              |
| `--basic-auth-password`                                        | `VOXL_PASSWORD`, else made up      | the password                                                                                                      |
| `--basic-auth-user`                                            | any                                | the user name a browser's sign-in has to give                                                                     |
| `--framerate`                                                  | `60`                               | max frames per second (`60-60` style ranges accepted)                                                             |
| `--preset`                                                     | `ultrafast`                        | x264 preset                                                                                                       |
| `--encoder-threads`                                            | `min(4, ncpu/4)`                   | x264 sliced threads                                                                                               |
| `--crf`                                                        | `24`                               | base quality                                                                                                      |
| `--max-kbps` / `--video-bitrate`                               | `30000`                            | adaptive bitrate ceiling                                                                                          |
| `--min-kbps`                                                   | `2000`                             | adaptive bitrate floor                                                                                            |
| `--paintover-crf`, `--paintover-burst`, `--paintover-delay-ms` | `16`, `3`, `150`                   | sharpening when the screen goes still                                                                             |
| `--enable-clipboard`                                           | `true`                             | `true`, `in`, `out` or `false`                                                                                    |
| `--audio-enabled`, `--audio-device-name`, `--audio-bitrate`    | `true`, `output.monitor`, `128000` | desktop audio                                                                                                     |
| `--enable-resize`, `--resize-scale`                            | `false`, `1.5`                     | desktop follows the viewer's window; desktop pixels per CSS pixel                                                 |
| `--microphone-enabled`, `--microphone-fifo`                    | `false`, `/tmp/voxl-mic.fifo`      | browser microphone into the FIFO a PulseAudio pipe source reads                                                   |
| `--round-corners`                                              | `0` (off)                          | corner radius in pixels for top-level windows                                                                     |
| `--round-windows`                                              | none                               | `title:radius,...` rounds windows with that title, docks included                                                 |
| `--adaptive-threads`                                           | `false`                            | encode on one thread, fanning out only under sustained heavy motion                                               |
| `--stripes`                                                    | `1`                                | encode the screen as N independent horizontal streams (experimental)                                              |
| `--scroll-px-per-notch`                                        | `180`                              | desktop pixels one wheel notch scrolls (calibrates smooth scrolling)                                              |
| `--file-transfers`                                             | `upload,download`                  | `upload`, `download`, both, or `none`                                                                             |
| `--read-only`                                                  | `false`                            | viewers only watch: input, clipboard writes, the microphone, uploads and resizing (`--enable-resize`) are refused |
| `--allowed-hosts`                                              | none                               | host names to answer for besides IP addresses and `localhost` (`.example.com` covers subdomains, `*` any)         |
| `--allowed-origins`                                            | none                               | other sites whose pages may connect, such as a page that embeds voxl's client (`*` any site)                      |
| `--tls`                                                        | `false`                            | serve HTTPS with a self-signed certificate, made on first start and reused (see [HTTPS](#https))                  |
| `--tls-cert`, `--tls-key`                                      | none                               | serve HTTPS with this PEM certificate chain and key                                                               |
| `--files-root`                                                 | `$HOME`                            | folder the Files panel can see; nothing outside it is reachable                                                   |
| `--upload-dir`                                                 | `Downloads`                        | folder (under the root) the panel opens and uploads into; created on first upload                                 |
| `--max-upload-mb`                                              | `0` (no limit)                     | largest accepted upload                                                                                           |
| `--max-record-seconds`                                         | `300`                              | longest recording (`/record`)                                                                                     |
| `--convert-threads`, `--coalesce-us`                           | `4`, `0`                           | tuning                                                                                                            |

### Login

voxl asks for a password before it shows the desktop or takes input, files or API
calls. Give it one with `--basic-auth-password` or `VOXL_PASSWORD`; otherwise it makes
one up when it starts and prints a link that signs you in. There are three ways in:

- **The link**: `http://host:6901/?token=<password>`. The page passes the password on to
  its own connections, so the link also works as an iframe's `src`.
- **The browser's sign-in prompt**, which voxl's page brings up: any user name (unless
  `--basic-auth-user` names one) and the password. Password managers offer to save it.
- **For programs**: `Authorization: Bearer <password>`, or `curl -u :<password>`.

The page's scripts and `/healthz` load without it. If voxl sits behind a proxy that signs
people in itself, `--enable-basic-auth=false` turns voxl's login off.

### Hosts and origins

voxl only answers requests for an IP address or `localhost`, and only from its own page.
That keeps out other web pages, which browsers let connect to any address, including
pages whose domain is pointed at voxl's address (DNS rebinding). To reach voxl under a
host name, such as through a proxy or as `my-pc.local`, name it with `--allowed-hosts`.
To let a page on another site connect, name that site with `--allowed-origins`.
Programs that aren't browsers (curl, scripts, agents) send no `Origin` and connect as
usual. A refused request gets a 403 that names the flag to add.

### For agents and scripts

Screenshots and recordings (start voxl with `VOXL_PASSWORD` set, and send it along):

```sh
# the screen as a PNG
curl -u ":$VOXL_PASSWORD" -o shot.png localhost:6901/screenshot
# an MP4 of the next 10 s
curl -u ":$VOXL_PASSWORD" -o clip.mp4 "localhost:6901/record?seconds=10"
# or start...
id=$(curl -u ":$VOXL_PASSWORD" -s -X POST localhost:6901/record/start | jq .id)
# ...and stop when done
curl -u ":$VOXL_PASSWORD" -o clip.mp4 -X POST "localhost:6901/record/stop?id=$id"
```

A recording is the H.264 voxl already sends to viewers, saved as MP4 without encoding
it again, so it costs no extra CPU. It doesn't include the cursor (viewers draw it), and
a resize ends it.

`/wait` returns when the screen settles or changes, so an agent doesn't have to sleep
after every action and hope it was long enough:

```sh
# once nothing changed for 300 ms
curl -u ":$VOXL_PASSWORD" localhost:6901/wait
# still for 1 s, giving up after 30 s
curl -u ":$VOXL_PASSWORD" "localhost:6901/wait?idle=1000&timeout=30000"
# the first change in that rectangle
curl -u ":$VOXL_PASSWORD" "localhost:6901/wait?change&region=0,0,800,600"
# {"changed":true,"timed_out":false,"seq":812,"waited_ms":340}
```

`idle` (ms, default 300) is how long the screen has to stay still, counted from the
request at the earliest so an action just taken has time to show; `change` returns at
the first change instead. `region=x,y,w,h` only watches that rectangle (to the 16-pixel
block) and `timeout` (ms, default 10000) gives up. `changed` says whether anything
changed while waiting. A change can land before the request does, so to wait for what an
action caused, pass `since` the `seq` of an earlier `/wait` or the `X-Voxl-Seq` header of
a screenshot: `/wait?change&since=812` returns at once if anything changed after it. A
blinking text caret redraws about twice a second, so keep `idle` under 500 ms or leave
the caret out of `region`. voxl already knows which parts of the screen change (XDamage,
ScreenCaptureKit and DXGI report them), so waiting needs no screenshots or polling.

### macOS

voxl also streams a Mac's own screen, with every feature of the Linux version
(ScreenCaptureKit capture, CGEvent input, NSPasteboard clipboard, system audio).

With multiple displays, the toolbar's Display picker changes the captured monitor.
All viewers follow the selection and reconnect automatically; read-only viewers
cannot change it. `--display` still selects the initial monitor. The list refreshes
when capture rebuilds; restart voxl to discover newly attached displays.

```sh
brew install x264 opus pkgconf esbuild
cargo build --release
./target/release/voxl --port=6901
```

Open the link it prints. On a Mac voxl listens on `127.0.0.1` unless `--addr` says
otherwise, since the stream is your whole screen. macOS asks voxl (or the terminal that
starts it) for Screen Recording and Accessibility permissions. For the microphone,
install a loopback device (`brew install blackhole-2ch`) and pick it as the input in the
apps that should hear you.

| macOS flag             | Default                             |                                                                                        |
| ---------------------- | ----------------------------------- | -------------------------------------------------------------------------------------- |
| `--display`            | main display                        | display to stream: an index (0 is the main display) or a display ID; logged at startup |
| `--capture-scale`      | native, within the budget           | streamed pixels per point                                                              |
| `--max-capture-pixels` | `4096000` (2560x1600)               | pixel budget for the automatic scale: x264's cost grows with area                      |
| `--microphone-device`  | first loopback device found         | output device (name or prefix) to play the browser microphone into                     |
| `--resize-scale`       | the capture scale                   | so a viewer's window maps to the same number of points on the Mac                      |
| `--show-cursor`        | `false` (`true` with `--read-only`) | include the host cursor in the video stream (for viewers/observers)                    |

The other flags work as on Linux; `--audio-device-name`, `--microphone-fifo` and the
rounded-corner flags don't apply.

### Windows

voxl also streams a Windows desktop (Windows 10 or later), with every feature of the
Linux version (DXGI Desktop Duplication capture, SendInput, clipboard text and images,
WASAPI system audio). Build it with Rust, [vcpkg](https://vcpkg.io), LLVM (for bindgen)
and esbuild:

```powershell
vcpkg install x264 libyuv opus --triplet x64-windows-static-md
$env:VCPKGRS_TRIPLET = 'x64-windows-static-md'
npm install -g esbuild
cargo build --release
.\target\release\voxl.exe --port=6901
```

Open the link it prints. As on a Mac, voxl listens on `127.0.0.1` unless `--addr` says
otherwise. It streams the desktop of the session it runs in, so start it in the user's
session (not as a service); Windows hides the lock screen and UAC prompts from it unless
it runs as SYSTEM. For the microphone, install a virtual audio cable
([VB-CABLE](https://vb-audio.com/Cable/)): voxl plays into "CABLE Input" and apps record
from "CABLE Output".

| Windows flag          | Default                   |                                                                    |
| --------------------- | ------------------------- | ------------------------------------------------------------------ |
| `--display`           | primary display           | display to stream, by index (0 is the primary); logged at startup  |
| `--audio-device-name` | default output device     | output device (name or prefix) whose sound to stream               |
| `--microphone-device` | first virtual cable found | output device (name or prefix) to play the browser microphone into |
| `--resize-scale`      | the display's scaling     | so a viewer's window maps to the same size on the desktop          |

The other flags work as on Linux; `--microphone-fifo` and the rounded-corner flags don't
apply. Dynamic resolution picks the largest display mode that fits the viewer's window,
and Windows restores the original mode when voxl exits.

### HTTPS

`--tls-cert` and `--tls-key` serve HTTPS with a PEM certificate chain and key you
provide. `--tls` on its own makes a self-signed certificate on first start and reuses it
after that, so each browser asks to trust it only once. It's kept in `VOXL_TLS_DIR` if
set, otherwise `%LOCALAPPDATA%\voxl` on Windows or `~/.config/voxl` elsewhere, and names
`localhost`, the machine's host name, the `--addr` address and the `--allowed-hosts`
names; delete it to make a new one. Over HTTPS, open `https://`: a browser that asks for
`http://` on the same port gets an invalid response.

```sh
voxl --addr=0.0.0.0 --tls
```

## FAQ

**Why a WebSocket and not WebRTC?** One WebSocket goes wherever a web page does: through
reverse proxies, tunnels and sandbox preview URLs, with no TURN servers or UDP ports to
open. The browser decodes each frame the moment it arrives with WebCodecs, without
WebRTC's jitter buffer, and voxl runs its own rate control, which keeps latency low on a
clean link. The trade-off is TCP: on a link that drops packets (bad Wi-Fi, a weak mobile
signal) a lost packet holds up the frames behind it, where WebRTC can skip ahead.

**How is it different from Moonlight and Sunshine?** They're for playing games on your
own GPU machine through the Moonlight app, and for that they're the better fit. voxl was
built for things like software factories that use headless VMs and sandboxes (CI,
computer-use agents), usually on Xvfb with no GPU. It streams them to a plain browser tab
over one WebSocket and has APIs for agents.

**How is it different from RustDesk?** RustDesk is an open-source alternative to
TeamViewer and AnyDesk, for reaching your own computers or helping someone with theirs:
you install its app on both ends, desktop or phone, and the two find each other through
an ID server, then connect directly or through a relay. For that it's the better fit,
and it also shares Wayland desktops (experimentally) and Android phones, which voxl
doesn't. voxl is for desktops on servers, VMs and sandboxes: it's one process that
serves its own page, so there's nothing to install on the viewing side and no ID or
relay server to run. The stream goes wherever a URL does, can be embedded in another
page and comes with the APIs agents use. RustDesk has a web client too, but it still
goes through the ID and relay servers, and the self-hosted one comes with RustDesk
Server Pro.

**Why doesn't the stream load at `http://<ip>:6901`?** Browsers only allow WebCodecs on
HTTPS pages and on localhost. To watch from another machine, start voxl with `--tls`
(see [HTTPS](#https)), put it behind a TLS proxy (Caddy, Cloudflare Tunnel,
`tailscale serve`) and name its host with `--allowed-hosts`, or forward the port over
SSH and open `localhost`.

**Is it safe to expose?** voxl asks for a password by default but serves plain HTTP
unless `--tls` or `--tls-cert` is given, so serve it over HTTPS away from localhost. See
[SECURITY.md](SECURITY.md).

**Which browsers and devices work?** Any browser with WebCodecs, on an HTTPS page or
localhost:

| Browser                                                                | Works |
| ---------------------------------------------------------------------- | ----- |
| Chrome, Edge and other Chromium browsers 94+, desktop and Android      | Yes   |
| Firefox 130+, desktop                                                  | Yes   |
| Safari 16.4+ and any browser on iPhone or iPad (sound needs Safari 26) | Yes   |
| Firefox for Android                                                    | No    |

Phones and tablets get touch input (tap to click, drag, long press to right click, pinch
to zoom, two fingers to move around a zoomed-in view and scroll) and an on-screen
keyboard.
The toolbar's trackpad button switches to trackpad mode, which works like a laptop's
trackpad: one finger moves a cursor from where it is (farther the faster it moves), a
tap clicks and a two-finger tap right clicks where the cursor is, and press and hold,
then move, drags. The whole screen is the trackpad, and the browser remembers the mode.

**Wayland?** Not yet; on Linux voxl streams X11 displays, Xvfb included. The virtual
desktops voxl is built for run on Xvfb (E2B's and Daytona's do too), and X11 lets voxl
capture them without a permission prompt, with damage events that keep a still screen at
no CPU and power `/wait`. Wayland support is being worked on.

**Does it use the GPU?** Not yet. voxl encodes H.264 on the CPU with x264: 1.3–1.7 cores
for a busy 2560x1600 desktop, X server included (see the CPU table above). Hardware
encoding on macOS is in progress.

**What's the license?** voxl's code is MIT. It links x264, which is GPL-2.0, so a voxl
binary, and anything that bundles one, is distributed under the GPL.
