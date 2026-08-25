// @ts-check
/// <reference path="./ghostex-extension-sdk.d.ts" />

/*
Vendored extension SDK runtime. Extension builds copy this file into their own
self-contained bundle. Native extension surfaces already have window.ghostex
at context creation; chat-bar subframes use the typed parent transport below.
*/
(() => {
  if (window.ghostex?.__bridgeVersion === 1 || window.parent === window) {
    return;
  }

  const BRIDGE_VERSION = 1;
  let ready = false;
  let sequence = 0;
  /** @type {Array<() => void>} */
  const queued = [];
  /** @type {Map<string, {resolve: (value: unknown) => void, reject: (reason?: unknown) => void, onChunk?: (chunk: {stream: 'stdout' | 'stderr', text: string}) => void}>} */
  const pending = new Map();
  /** @type {Set<(context: import('./ghostex-extension-sdk').GhostexExtensionContext) => void>} */
  const contextListeners = new Set();

  /**
   * @param {import('./ghostex-extensions').GhostexChatBarBridgeMethod} method
   * @param {Record<string, unknown>} [params]
   * @param {(chunk: {stream: 'stdout' | 'stderr', text: string}) => void} [onChunk]
   */
  const call = (method, params = {}, onChunk) =>
    new Promise((resolve, reject) => {
      const requestId = `${Date.now().toString(36)}-${(++sequence).toString(36)}`;
      pending.set(requestId, { resolve, reject, ...(onChunk ? { onChunk } : {}) });
      const send = () => {
        try {
          // chat.html is a bundled file:// page, so it has an opaque origin and
          // cannot be named as targetOrigin. The receiver still authenticates
          // this exact child Window and its loopback extension origin.
          window.parent.postMessage(
            {
              type: 'ghostexChatBarBridgeRequest',
              bridgeVersion: BRIDGE_VERSION,
              requestId,
              method,
              params,
            },
            '*'
          );
        } catch (error) {
          pending.delete(requestId);
          reject(
            Object.assign(new Error('Ghostex could not send the extension call.'), {
              code: 'operationFailed',
              cause: error,
            })
          );
        }
      };
      if (ready) {
        send();
      } else {
        queued.push(send);
      }
    });

  window.addEventListener('message', (event) => {
    if (event.source !== window.parent || !event.data || typeof event.data !== 'object') {
      return;
    }
    const message = event.data;
    if (message.bridgeVersion !== BRIDGE_VERSION) {
      return;
    }
    if (message.type === 'ghostexChatBarBridgeReady') {
      if (!ready) {
        ready = true;
        for (const send of queued.splice(0)) {
          send();
        }
      }
      return;
    }
    if (message.type === 'ghostexChatBarBridgeContextChanged') {
      for (const listener of contextListeners) {
        listener(message.context);
      }
      return;
    }
    const requestId = typeof message.requestId === 'string' ? message.requestId : '';
    const request = pending.get(requestId);
    if (!request) {
      return;
    }
    if (message.type === 'ghostexChatBarBridgeChunk') {
      request.onChunk?.(message.chunk);
      return;
    }
    if (message.type !== 'ghostexChatBarBridgeResponse') {
      return;
    }
    pending.delete(requestId);
    if (message.ok) {
      request.resolve(message.result);
    } else {
      const error = message.error ?? {};
      request.reject(Object.assign(new Error(error.message || 'Ghostex extension call failed.'), error));
    }
  });

  const api = {
    __bridgeVersion: 1,
    context: () => call('context'),
    onContextChange(callback) {
      contextListeners.add(callback);
      return () => contextListeners.delete(callback);
    },
    cli: (verb, args = []) => call('cli', { verb, args }),
    exec(command, options = {}) {
      const { stream, ...params } = options;
      return call('exec', { command, ...params }, typeof stream === 'function' ? stream : undefined);
    },
    settings: Object.freeze({
      get: () => call('settings.get'),
      set: (values) => call('settings.set', { values }),
    }),
    storage: Object.freeze({
      get: (key) => call('storage.get', { key }),
      set: (key, value) => call('storage.set', { key, value }),
    }),
    ui: Object.freeze({
      toast: (message) => call('ui.toast', { message }),
      close: () => call('ui.close'),
      setBadge: (lines) => call('ui.setBadge', { lines }),
    }),
  };
  Object.defineProperty(window, 'ghostex', {
    configurable: false,
    enumerable: true,
    value: Object.freeze(api),
  });
})();
