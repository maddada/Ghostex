/*
 * Parent side of the sandbox modal transport (SPEC.md "The modal pipeline").
 *
 * One connection per fake NSPanel (`SimModalWindow.windowId`). Messages travel
 * over same-origin `window.postMessage` with the `__onboardingSandbox` marker:
 *   parent → iframe  {__onboardingSandbox:"deliver",  windowId, detail}
 *   iframe → parent  {__onboardingSandbox:"outbound", windowId, message}
 *   iframe → parent  {__onboardingSandbox:"iframeReady", windowId}   (shim installed)
 * The marker is mandatory: `apps/desktop/views/modal-host.tsx` re-emits some
 * transient results through plain `window.postMessage`, and those must be
 * ignored here.
 *
 * Delivery is queued. The real gpui host only sends `{type:"open"}` after the
 * modal host posted `{type:"ready"}`; callers here don't have to care, because
 * anything sent between "window opened" and "modal host mounted" is buffered
 * and flushed on that `ready`.
 */
import type { ModalHostInboundMessage, ModalHostOutboundMessage } from '../state/types';

export type ModalOutboundHandler = (windowId: string, message: ModalHostOutboundMessage) => void;

/** Per-window outbound observer (used by ModalWindowFrame for fit-height). */
export type ModalWindowOutboundListener = (message: ModalHostOutboundMessage) => void;

const SANDBOX_MARKER = '__onboardingSandbox';

interface ModalWindowConnection {
  iframe: HTMLIFrameElement | null;
  /** True once the modal host inside the iframe posted `{type:"ready"}`. */
  hostReady: boolean;
  pendingInbound: ModalHostInboundMessage[];
  listeners: Set<ModalWindowOutboundListener>;
}

const connections = new Map<string, ModalWindowConnection>();
const pendingOutbound: Array<{ message: ModalHostOutboundMessage; windowId: string }> = [];

let outboundHandler: ModalOutboundHandler | null = null;
let transportInstalled = false;

function connectionFor(windowId: string): ModalWindowConnection {
  let connection = connections.get(windowId);
  if (!connection) {
    connection = { hostReady: false, iframe: null, listeners: new Set(), pendingInbound: [] };
    connections.set(windowId, connection);
  }
  return connection;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value && typeof value === 'object' && !Array.isArray(value));
}

function deliverNow(connection: ModalWindowConnection, windowId: string, detail: ModalHostInboundMessage): boolean {
  const target = connection.iframe?.contentWindow;
  if (!target) {
    return false;
  }
  target.postMessage({ [SANDBOX_MARKER]: 'deliver', detail, windowId }, window.location.origin);
  return true;
}

function flushInbound(windowId: string, connection: ModalWindowConnection): void {
  if (!connection.hostReady || connection.pendingInbound.length === 0) {
    return;
  }
  if (!connection.iframe?.contentWindow) {
    // Iframe not mounted yet: keep the queue intact and ordered.
    return;
  }
  const queued = connection.pendingInbound;
  connection.pendingInbound = [];
  for (const detail of queued) {
    deliverNow(connection, windowId, detail);
  }
}

function dispatchOutbound(windowId: string, message: ModalHostOutboundMessage): void {
  const connection = connections.get(windowId);
  if (connection) {
    for (const listener of [...connection.listeners]) {
      listener(message);
    }
  }
  if (!outboundHandler) {
    // The engine registers its handler at module init; if a window somehow
    // reports before that, keep the message instead of dropping `ready`.
    pendingOutbound.push({ message, windowId });
    return;
  }
  outboundHandler(windowId, message);
}

function handleTransportMessage(event: MessageEvent): void {
  const envelope = event.data;
  if (!isRecord(envelope)) {
    return;
  }
  const marker = envelope[SANDBOX_MARKER];
  if (marker !== 'outbound' && marker !== 'iframeReady') {
    return;
  }
  const windowId = envelope.windowId;
  if (typeof windowId !== 'string') {
    return;
  }
  const connection = connectionFor(windowId);

  if (marker === 'iframeReady') {
    // Fresh document (first load or vite reload): the previous host listener is
    // gone, so hold delivery until the remounted host says `ready` again.
    connection.hostReady = false;
    return;
  }

  const message = envelope.message;
  if (!isRecord(message) || typeof message.type !== 'string') {
    return;
  }
  if (message.type === 'ready') {
    connection.hostReady = true;
    flushInbound(windowId, connection);
  }
  dispatchOutbound(windowId, message as ModalHostOutboundMessage);
}

function ensureTransportInstalled(): void {
  if (transportInstalled) {
    return;
  }
  transportInstalled = true;
  window.addEventListener('message', handleTransportMessage);
}

ensureTransportInstalled();

/** Deliver an inbound host message to one modal window (queued until ready). */
export function sendToModalWindow(windowId: string, detail: ModalHostInboundMessage): void {
  ensureTransportInstalled();
  const connection = connectionFor(windowId);
  const mustQueue =
    !connection.hostReady || connection.pendingInbound.length > 0 || !deliverNow(connection, windowId, detail);
  if (mustQueue) {
    connection.pendingInbound.push(detail);
  }
}

/** The engine registers exactly one handler for every modal window's outbound traffic. */
export function setModalOutboundHandler(handler: ModalOutboundHandler): void {
  ensureTransportInstalled();
  outboundHandler = handler;
  if (pendingOutbound.length === 0) {
    return;
  }
  const queued = pendingOutbound.splice(0, pendingOutbound.length);
  for (const entry of queued) {
    handler(entry.windowId, entry.message);
  }
}

/** ModalWindowFrame binds its iframe element here once it is in the DOM. */
export function registerModalIframe(windowId: string, el: HTMLIFrameElement): void {
  ensureTransportInstalled();
  const connection = connectionFor(windowId);
  connection.iframe = el;
  flushInbound(windowId, connection);
}

export function unregisterModalIframe(windowId: string): void {
  const connection = connections.get(windowId);
  if (!connection) {
    return;
  }
  connection.iframe = null;
  connection.hostReady = false;
  connection.pendingInbound = [];
  if (connection.listeners.size === 0) {
    connections.delete(windowId);
  }
}

/**
 * Observe one window's outbound messages without stealing them from the engine
 * handler. `ModalWindowFrame` uses this for the one-shot
 * `{type:"contentHeightMeasured"}` resize.
 */
export function subscribeModalWindowOutbound(windowId: string, listener: ModalWindowOutboundListener): () => void {
  ensureTransportInstalled();
  const connection = connectionFor(windowId);
  connection.listeners.add(listener);
  return () => {
    const current = connections.get(windowId);
    if (!current) {
      return;
    }
    current.listeners.delete(listener);
    if (!current.iframe && current.listeners.size === 0) {
      connections.delete(windowId);
    }
  };
}
