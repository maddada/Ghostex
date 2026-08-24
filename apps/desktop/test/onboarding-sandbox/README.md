# Onboarding Sandbox

A dev-server-only Vite app that simulates the gpui macOS app's first-run onboarding so the
flow can be watched and iterated on without launching (or restarting) the real app.
`SPEC.md` in this folder is the architecture contract — read it before changing anything here.

Nothing in this folder ships: there is no production build target, no tests, and no
network access. Everything (gxserver health, agent CLIs, hooks, skills, the persisted
first-run state file) is faked in-page.

## Quickstart

```bash
bun run sandbox:onboarding      # → https://127.0.0.1:5199
```

The dev server is **HTTPS/HTTP-2** (see "Tutorial video window" below for why). On first
start it generates a self-signed certificate into `dev-cert/` with `openssl` (gitignored,
dev-only); the browser shows the usual one-time "not private" warning — accept it and the
sandbox loads normally. If `openssl` is unavailable the server falls back to plain HTTP,
and the tutorial video's playback stops working (everything else is unaffected).

or directly:

```bash
bunx vite --config apps/desktop/test/onboarding-sandbox/vite.config.ts
```

Typecheck:

```bash
bunx tsc -p apps/desktop/test/onboarding-sandbox/tsconfig.json --noEmit
```

Click the Ghostex icon in the fake dock to "launch" the app; use the right-hand control
panel to change the simulated environment, apply scenario presets, edit the fake state
file, force-open any modal, and read the annotated event log.

## How the real modals render

The onboarding modals are the _real_ production React components: each fake NSPanel is an
iframe pointing at `/modal-window.html?windowId=…`, whose entry
(`src/modal-window/modal-window-main.ts`) sets up the host contract and then dynamically
imports `apps/desktop/views/modal-host.tsx` unchanged.

Order matters, and the entry is written around it:

1. set `window.__ghostex_APP_MODAL_HOST_ID__ = "gpui"` and
   `window.__ghostex_APP_MODAL_HOST_SURFACE__ = "nativeWindow"` (the modal host reads both
   at module scope),
2. install `window.webkit.messageHandlers.ghostexAppModalHost.postMessage`
   (`packages/core-ui/app-modal-host-bridge.ts` throws when it is missing, and the host posts
   `{type:"ready"}` the moment it mounts),
3. install the `message` listener that re-dispatches inbound details as the
   `ghostex-app-modal-host-message` CustomEvent,
4. only then `import("@/apps/desktop/views/modal-host")`, which self-mounts into `#root`.

Transport between the sandbox page and each iframe is same-origin `window.postMessage`
with an `__onboardingSandbox` marker (the modal host re-emits some transient results over
plain `postMessage`, so unmarked messages are ignored):

| direction       | envelope                                                          |
| --------------- | ----------------------------------------------------------------- |
| parent → iframe | `{__onboardingSandbox: "deliver", windowId, detail}`              |
| iframe → parent | `{__onboardingSandbox: "outbound", windowId, message}`            |
| iframe → parent | `{__onboardingSandbox: "iframeReady", windowId}` (shim installed) |

`src/bridge/modal-connections.ts` owns the parent side:

- `sendToModalWindow(windowId, detail)` — queues until that window's modal host has posted
  `{type:"ready"}`, so nothing sent between "window opened" and "iframe mounted" is lost.
- `setModalOutboundHandler(handler)` — the engine registers one global handler; messages
  that arrive before it is registered are queued too.
- `registerModalIframe` / `unregisterModalIframe` — called by `ModalWindowFrame`.
- `subscribeModalWindowOutbound(windowId, listener)` — passive per-window observer, used by
  the frame for the one-shot `{type:"contentHeightMeasured"}` resize.

`src/bridge/modal-window-frame.tsx` draws one panel: macOS-style title bar (drag to move,
red close button → `closeModalWindow`), a spinner until the store marks the window
`presented` (the real app keeps the NSPanel hidden until then), fixed `width`/`height` from
the store record, and fit-height windows resized by the measured content height.

## Tutorial video window

`watchGhostexVideo` is the one modal kind the real app does **not** render through
the React modal host (`GpuiAppModalKind::uses_react_modal_host` is false only for it).
Its native child window loads `GHOSTEX_TUTORIAL_VIDEO_URL` —
`https://www.youtube.com/watch?v=APdP-j5n4Mw`, the actual watch page, not the embed
player, which YouTube rejects when it is framed from the `file://` modal host — and the
host starts with `is_ready = true`, so there is no hydrate, no `{type:"open"}` and no
`presented` handshake.

The sandbox mirrors that:

- `engine/modal-chrome.ts` marks the kind with `nonReactHostUrl`; the engine creates the
  window already `presented` and skips all message delivery for it.
- `bridge/modal-window-frame.tsx` points the iframe at that URL instead of
  `modal-window.html` and does not bind the modal bridge.
- The dev server reverse-proxies `/yt/<path>` (and the root-absolute YouTube paths the
  watch document requests, plus `/gv/<host>/…` for media, which also lifts the CORS block
  on googlevideo) to `www.youtube.com`,
  stripping `x-frame-options`/CSP so the page can be framed, and injecting a small
  script that rewrites absolute `https://www.youtube.com/…` fetch/XHR/beacon URLs back
  onto the proxy. Same-origin is required both for framing and for the `f` simulation.
- `bridge/tutorial-video-window.ts` reproduces the real app's trusted `f` key injection
  ~1.5s after load: it dispatches a real `f` keydown/keyup pair into the watch document
  **and** injects a stylesheet that makes the player fill the window. The second half is
  the deliberate simulation of the press's outcome, not a fallback: browsers refuse
  `requestFullscreen()` from untrusted events, a restriction the real CEF host key press
  does not have. The event log records it as
  "Simulated f press (host key injection in the real app)".

**Why the dev server needs HTTPS/HTTP-2** (measured 2026-08-18): YouTube's media protocol
POSTs segment requests with a _streaming_ request body, and Chrome only sends those over
HTTP/2+. Over the original HTTP/1.1 dev server every segment failed with
`net::ERR_ALPN_NEGOTIATION_FAILED` and the player spun forever; left unproxied instead,
the same segments are CORS-blocked (googlevideo sends no `access-control-allow-origin`
for this origin). With TLS + h2 enabled the video really plays: verified with
`currentTime` advancing 8.9s → 18.9s, `readyState` 4, ~2.7 MB of segments served through
`/gv/`, and a decoded frame read back from a canvas.

Remaining caveats: the page's own ad/telemetry pings to `googleads.g.doubleclick.net`
fail (cross-origin, deliberately not proxied) and log `ERR_FAILED` _inside the iframe_;
headless screenshots show the video area black because headless Chrome does not composite
video frames into captures, even though the frames decode.

## Ownership

`SPEC.md` lists which agent owns which folder (`engine/`, `desktop/`, `controls/`,
`bridge/` + `modal-window/`). `src/state/types.ts` is shared and may only be extended
additively. The only production file this sandbox touches is the root `package.json`
script above.
