/*
 * Dev-only reverse proxy for the tutorial-video window.
 *
 * Why this exists: `GpuiAppModalKind::WatchGhostexVideo` is the ONE app-modal
 * kind that does not use the React modal host. gpui points that native child
 * window straight at `GHOSTEX_TUTORIAL_VIDEO_URL`
 * (`https://www.youtube.com/watch?v=APdP-j5n4Mw`, gpui/src/main.rs:1130) as the
 * top-level CEF document, because YouTube rejects the embed player when it is
 * framed from the `file://` modal-host document (CDXC:GPUITutorialVideo,
 * gpui/src/main.rs ~:1124).
 *
 * The sandbox draws that native window as an iframe, so it must show the same
 * *watch page* — not the embed. YouTube sends `x-frame-options: SAMEORIGIN` on
 * `/watch`, so the page can only be framed if it is served from our own origin:
 * this middleware fetches it server-side, strips the framing/CSP headers, and
 * re-serves it under `/yt/...`.
 *
 * Same-origin also matters for the simulated `f` key press: the parent page
 * needs DOM access into the iframe (see src/bridge/tutorial-video-window.ts).
 *
 * Scope: dev server only. Nothing here ships.
 */
import type { Plugin } from "vite";
import type { IncomingMessage, ServerResponse } from "node:http";
import { Readable } from "node:stream";

const YOUTUBE_ORIGIN = "https://www.youtube.com";
const PROXY_PREFIX = "/yt";
/** `/gv/<googlevideo host>/<path>` — media segments (see REWRITE_SCRIPT). */
const MEDIA_PROXY_PREFIX = "/gv";
const MEDIA_HOST_PATTERN = /^[a-z0-9._-]+\.googlevideo\.com$/i;

/** The watch URL gpui hands to CEF, re-pointed at this proxy. */
export const SANDBOX_TUTORIAL_VIDEO_PROXY_URL = "/yt/watch?v=APdP-j5n4Mw";

/*
 * The watch document requests most of its own machinery with root-absolute
 * paths ("/s/player/…/base.js", "/youtubei/v1/player"). Those resolve against
 * OUR origin once the document is served from it, so the same middleware has to
 * answer them. Kept to YouTube-owned prefixes the sandbox itself never serves.
 */
const YOUTUBE_ROOT_PREFIXES = [
  /*
   * The watch page rewrites its own URL with history.replaceState("/watch?v=…"),
   * dropping the /yt prefix, so a reload (or an in-page navigation to another
   * video) lands here instead. The sandbox never serves /watch itself.
   */
  "/watch",
  "/s/",
  "/youtubei/",
  "/yts/",
  "/iframe_api",
  "/generate_204",
  "/player_204",
  "/ptracking",
  "/csi_204",
  "/error_204",
  "/api/stats/",
  "/youtubei",
];

/** Hop-by-hop + identity headers that must not be forwarded upstream. */
const DROPPED_REQUEST_HEADERS = new Set([
  "accept-encoding",
  "connection",
  "content-length",
  "cookie",
  "host",
  "if-modified-since",
  "if-none-match",
  "keep-alive",
  "origin",
  "proxy-connection",
  "referer",
  "sec-fetch-dest",
  "sec-fetch-mode",
  "sec-fetch-site",
  "sec-fetch-user",
  "transfer-encoding",
  "upgrade-insecure-requests",
]);

/*
 * Response headers that would either re-break framing (the whole point of the
 * proxy) or describe a body we have already decoded/rewritten.
 */
const DROPPED_RESPONSE_HEADERS = new Set([
  /*
   * Verified 2026-08-18: forwarding YouTube's `alt-svc: h3=":443"` makes Chrome
   * try HTTP/3 against the dev server, and every following media request dies
   * with net::ERR_ALPN_NEGOTIATION_FAILED (player spins forever). The reporting
   * headers are dropped for the same "describes the real origin, not ours"
   * reason.
   */
  "alt-svc",
  /*
   * Connection-specific headers: HTTP/2 refuses to send them
   * (ERR_HTTP2_INVALID_CONNECTION_HEADERS), and the dev server speaks h2.
   */
  "connection",
  "keep-alive",
  "proxy-connection",
  "upgrade",
  "content-encoding",
  "nel",
  "report-to",
  "reporting-endpoints",
  "content-length",
  "content-security-policy",
  "content-security-policy-report-only",
  "cross-origin-embedder-policy",
  "cross-origin-opener-policy",
  "cross-origin-resource-policy",
  "set-cookie",
  "transfer-encoding",
  "x-frame-options",
]);

const REDIRECT_ORIGINS = [
  "https://www.youtube.com",
  "https://m.youtube.com",
  "https://youtube.com",
];

/*
 * Injected at the very start of <head>, before any YouTube script runs.
 *
 * InnerTube calls are issued with absolute `https://www.youtube.com/...` URLs;
 * from our origin those are cross-origin and blocked, so rewrite them onto the
 * proxy.
 *
 * Media (googlevideo.com) is routed through `/gv/<host>/…` because the player
 * streams through MSE (fetch/XHR, not a plain `<video src>`) and googlevideo
 * sends no `access-control-allow-origin` for this origin, so direct segment
 * requests are CORS-blocked.
 *
 * This route only works because the dev server speaks HTTP/2 (see dev-cert.ts):
 * YouTube's media protocol POSTs segments with a STREAMING request body, and
 * Chrome refuses to send those over HTTP/1.1 — measured 2026-08-18, every
 * segment failed with `net::ERR_ALPN_NEGOTIATION_FAILED` until TLS+h2 was
 * enabled, after which playback runs (verified: currentTime advancing,
 * readyState 4, ~2.7 MB of segments through this route).
 */
const REWRITE_SCRIPT = `<script>(function(){
  var ORIGIN = ${JSON.stringify(YOUTUBE_ORIGIN)};
  var PREFIX = ${JSON.stringify(PROXY_PREFIX)};
  var MEDIA_PREFIX = ${JSON.stringify(MEDIA_PROXY_PREFIX)};
  var MEDIA_HOST = /^https:\\/\\/([a-z0-9._-]+\\.googlevideo\\.com)(\\/.*)$/i;
  function rewrite(url) {
    if (typeof url !== "string") { return url; }
    if (url.lastIndexOf(ORIGIN, 0) === 0) { return PREFIX + url.slice(ORIGIN.length); }
    var media = MEDIA_HOST.exec(url);
    if (media) { return MEDIA_PREFIX + "/" + media[1] + media[2]; }
    return url;
  }
  window.__sandboxYtProxy = { origin: ORIGIN, prefix: PREFIX, rewrites: 0 };
  function counted(url) {
    var next = rewrite(url);
    if (next !== url) { window.__sandboxYtProxy.rewrites += 1; }
    return next;
  }
  var nativeFetch = window.fetch;
  if (nativeFetch) {
    window.fetch = function (input, init) {
      if (typeof input === "string") {
        input = counted(input);
      } else if (input && typeof input === "object" && typeof input.url === "string") {
        var rewritten = counted(input.url);
        if (rewritten !== input.url) { input = new Request(rewritten, input); }
      }
      return nativeFetch.call(this, input, init);
    };
  }
  var nativeOpen = XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open = function (method, url) {
    var args = Array.prototype.slice.call(arguments);
    args[1] = counted(url);
    return nativeOpen.apply(this, args);
  };
  if (navigator.sendBeacon) {
    var nativeBeacon = navigator.sendBeacon.bind(navigator);
    navigator.sendBeacon = function (url, data) { return nativeBeacon(counted(url), data); };
  }
})();</script>`;

/** Maps an incoming sandbox URL to the absolute upstream URL, or null. */
function resolveUpstreamUrl(rawUrl: string): string | null {
  if (rawUrl.startsWith(`${MEDIA_PROXY_PREFIX}/`)) {
    const rest = rawUrl.slice(MEDIA_PROXY_PREFIX.length + 1);
    const slash = rest.indexOf("/");
    const host = slash === -1 ? rest : rest.slice(0, slash);
    if (!MEDIA_HOST_PATTERN.test(host)) {
      return null;
    }
    return `https://${host}${slash === -1 ? "/" : rest.slice(slash)}`;
  }
  if (rawUrl === PROXY_PREFIX) {
    return `${YOUTUBE_ORIGIN}/`;
  }
  if (rawUrl.startsWith(`${PROXY_PREFIX}/`) || rawUrl.startsWith(`${PROXY_PREFIX}?`)) {
    return `${YOUTUBE_ORIGIN}${rawUrl.slice(PROXY_PREFIX.length) || "/"}`;
  }
  const path = rawUrl.split("?", 1)[0];
  return YOUTUBE_ROOT_PREFIXES.some((prefix) => path === prefix || path.startsWith(prefix))
    ? `${YOUTUBE_ORIGIN}${rawUrl}`
    : null;
}

function upstreamHeaders(request: IncomingMessage): Headers {
  const headers = new Headers();
  for (const [name, value] of Object.entries(request.headers)) {
    /*
     * Over HTTP/2 the compat layer hands us pseudo-headers (":method",
     * ":path", ":scheme", ":authority"); `Headers.set` throws on those names.
     */
    if (value === undefined || name.startsWith(":") || DROPPED_REQUEST_HEADERS.has(name)) {
      continue;
    }
    headers.set(name, Array.isArray(value) ? value.join(", ") : value);
  }
  // Identity YouTube trusts: a real youtube.com navigation, no cookies.
  headers.set("origin", YOUTUBE_ORIGIN);
  headers.set("referer", `${YOUTUBE_ORIGIN}/`);
  if (!headers.has("user-agent")) {
    headers.set("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36");
  }
  headers.set("accept-language", headers.get("accept-language") ?? "en-US,en;q=0.9");
  return headers;
}

function rewriteLocation(location: string): string {
  for (const origin of REDIRECT_ORIGINS) {
    if (location.startsWith(origin)) {
      return `${PROXY_PREFIX}${location.slice(origin.length) || "/"}`;
    }
  }
  return location;
}

function injectRewriteScript(html: string): string {
  const headMatch = /<head[^>]*>/i.exec(html);
  if (!headMatch) {
    return `${REWRITE_SCRIPT}${html}`;
  }
  const insertAt = headMatch.index + headMatch[0].length;
  return `${html.slice(0, insertAt)}${REWRITE_SCRIPT}${html.slice(insertAt)}`;
}

async function readRequestBody(
  request: IncomingMessage,
): Promise<Uint8Array<ArrayBuffer> | undefined> {
  if (request.method === "GET" || request.method === "HEAD") {
    return undefined;
  }
  const chunks: Buffer[] = [];
  for await (const chunk of request) {
    chunks.push(typeof chunk === "string" ? Buffer.from(chunk) : chunk);
  }
  if (chunks.length === 0) {
    return undefined;
  }
  // Copy into a plain ArrayBuffer: Buffer's ArrayBufferLike is not a BodyInit.
  const merged = Buffer.concat(chunks);
  const body = new Uint8Array(new ArrayBuffer(merged.byteLength));
  body.set(merged);
  return body;
}

async function proxyRequest(
  upstreamUrl: string,
  request: IncomingMessage,
  response: ServerResponse,
): Promise<void> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 25_000);
  try {
    const body = await readRequestBody(request);
    const upstream = await fetch(upstreamUrl, {
      body,
      headers: upstreamHeaders(request),
      method: request.method ?? "GET",
      redirect: "manual",
      signal: controller.signal,
    });

    const outgoing = new Headers(upstream.headers);
    for (const name of DROPPED_RESPONSE_HEADERS) {
      outgoing.delete(name);
    }
    const location = upstream.headers.get("location");
    if (location) {
      outgoing.set("location", rewriteLocation(location));
    }

    const contentType = upstream.headers.get("content-type") ?? "";
    const isHtml = contentType.includes("text/html");
    for (const [name, value] of outgoing) {
      response.setHeader(name, value);
    }
    response.statusCode = upstream.status;

    if (!upstream.body) {
      response.end();
      return;
    }
    if (isHtml) {
      const html = await upstream.text();
      const rewritten = injectRewriteScript(html);
      response.setHeader("content-length", Buffer.byteLength(rewritten));
      response.end(rewritten);
      return;
    }
    Readable.fromWeb(upstream.body as Parameters<typeof Readable.fromWeb>[0]).pipe(response);
  } catch (error) {
    response.statusCode = 502;
    response.setHeader("content-type", "text/plain; charset=utf-8");
    response.end(`onboarding-sandbox youtube proxy failed: ${String(error)}`);
  } finally {
    clearTimeout(timeout);
  }
}

/**
 * Serves `https://www.youtube.com/<path>` under `/yt/<path>` (plus the
 * root-absolute YouTube paths the watch document itself requests and the
 * `/gv/<host>/…` media route), with the framing headers stripped so the real
 * watch page can render — and play — inside the fake native window.
 */
export function sandboxYouTubeProxy(): Plugin {
  return {
    name: "onboarding-sandbox-youtube-proxy",
    configureServer(server) {
      server.middlewares.use((request, response, next) => {
        const rawUrl = request.url ?? "";
        /*
         * The watch page registers a service worker at /sw.js. Serving the real
         * one would let YouTube's worker intercept every sandbox request on this
         * origin; letting vite answer it returns index.html and logs an
         * "unsupported MIME type" error. Answer 404 so registration just fails.
         */
        if (rawUrl.split("?", 1)[0] === "/sw.js") {
          response.statusCode = 404;
          response.setHeader("content-type", "text/plain; charset=utf-8");
          response.end("not found");
          return;
        }
        const upstreamUrl = resolveUpstreamUrl(rawUrl);
        if (!upstreamUrl) {
          next();
          return;
        }
        void proxyRequest(upstreamUrl, request, response);
      });
    },
  };
}
