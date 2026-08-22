/*
 * Iframe entry for one fake NSPanel (see SPEC.md "The modal pipeline").
 *
 * Everything in this module runs BEFORE the real `apps/desktop/views/modal-host`
 * module is imported, because that module:
 *   - reads `window.__ghostex_APP_MODAL_HOST_SURFACE__` / `__ghostex_APP_MODAL_HOST_ID__`
 *     at module scope to pick its native-window body classes, and
 *   - self-mounts `<AppModalHost/>` into `#root`, which immediately posts
 *     `{type:"ready"}` through
 *     `window.webkit.messageHandlers.ghostexAppModalHost.postMessage` —
 *     `packages/core-ui/app-modal-host-bridge.ts` THROWS when that handler is missing.
 * Same ordering contract as `apps/web/src/main.tsx` +
 * `apps/web/src/app/app-modal-host-shim.ts`, only the transport differs:
 * here outbound messages are forwarded to the sandbox parent page.
 */

const SANDBOX_MARKER = "__onboardingSandbox";
const HOST_MESSAGE_EVENT = "ghostex-app-modal-host-message";

const windowId = new URLSearchParams(window.location.search).get("windowId") ?? "unknown";
const parentOrigin = window.location.origin;

function postToSandboxParent(envelope: Record<string, unknown>): void {
  const parentWindow = window.parent;
  if (!parentWindow || parentWindow === window) {
    // Opened standalone (a smoke check, or a stray tab): nothing to talk to.
    return;
  }
  parentWindow.postMessage(envelope, parentOrigin);
}

function forwardOutboundMessage(message: unknown): void {
  postToSandboxParent({ [SANDBOX_MARKER]: "outbound", message, windowId });
}

window.__ghostex_APP_MODAL_HOST_ID__ = "gpui";
window.__ghostex_APP_MODAL_HOST_SURFACE__ = "nativeWindow";

window.webkit = {
  ...window.webkit,
  messageHandlers: {
    ...window.webkit?.messageHandlers,
    ghostexAppModalHost: { postMessage: forwardOutboundMessage },
  },
};

/*
 * Inbound: the sandbox parent sends `{__onboardingSandbox:"deliver", windowId,
 * detail}` and the detail is re-dispatched verbatim as the CustomEvent the
 * modal host listens for. The marker matters because the modal host re-emits
 * some transient results through `window.postMessage` itself.
 */
window.addEventListener("message", (event: MessageEvent) => {
  const envelope = event.data as Record<string, unknown> | null | undefined;
  if (!envelope || typeof envelope !== "object") {
    return;
  }
  if (envelope[SANDBOX_MARKER] !== "deliver") {
    return;
  }
  if (typeof envelope.windowId === "string" && envelope.windowId !== windowId) {
    return;
  }
  const detail = envelope.detail;
  if (!detail || typeof detail !== "object") {
    return;
  }
  window.dispatchEvent(new CustomEvent(HOST_MESSAGE_EVENT, { detail }));
});

/*
 * Transport handshake: tells the parent the shim is live so it can (re)arm the
 * connection after a reload. The parent still waits for the modal host's own
 * `{type:"ready"}` outbound before flushing queued host messages, because the
 * host only installs its CustomEvent listener when `<AppModalHost/>` mounts.
 */
postToSandboxParent({ [SANDBOX_MARKER]: "iframeReady", windowId });

void import("@/apps/desktop/views/modal-host");
