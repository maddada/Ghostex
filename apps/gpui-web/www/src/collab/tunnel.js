// Ghostex Collab: the guest page's half of the end-to-end lock, and the transport that carries the
// GPUI web build's own gxserver calls (fetch and WebSocket) through it.
//
// The link's key (after `#`) never leaves this page: it is hashed into the relay's route id and
// derives one AES-256-GCM key per direction for this connection, so the relay only copies frames it
// cannot read. Same derivation as server/src/collab/tunnel.rs:
// - route = hex(HKDF-SHA256(K, salt = "", info = "ghostex-collab/route/v1"))
// - first frame (clear): 0x01 || salt (32 random bytes)
// - guest → host key: HKDF(K, salt, "ghostex-collab/g2h/v1"); host → guest: "ghostex-collab/h2g/v1"
// - every later frame: AES-GCM with nonce 0u32 || counter (u64, big-endian), one counter per direction

const encoder = new TextEncoder();
const decoder = new TextDecoder();
const NativeWebSocket = globalThis.WebSocket;
const nativeFetch = globalThis.fetch.bind(globalThis);

/** The fake origin the web build is handed in guest mode; every call to it goes through the lock. */
export const TUNNEL_ORIGIN = 'http://collab.invalid';
const TUNNEL_HOST = 'collab.invalid';

/** Close codes the relay sends (server/share-relay/src/main.rs). */
export const CLOSE = { ended: 4401, offline: 4404, hostLeft: 4410, full: 4429 };

export function parseKey(text) {
  if (!/^[A-Za-z0-9_-]{43}$/.test(text ?? '')) return null;
  const base64 = text.replace(/-/g, '+').replace(/_/g, '/') + '=';
  const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
  return bytes.length === 32 ? bytes : null;
}

async function hkdf(key, salt, info) {
  const base = await crypto.subtle.importKey('raw', key, 'HKDF', false, ['deriveBits']);
  const bits = await crypto.subtle.deriveBits(
    { name: 'HKDF', hash: 'SHA-256', salt, info: encoder.encode(info) },
    base,
    256,
  );
  return new Uint8Array(bits);
}

export async function routeId(key) {
  const bytes = await hkdf(key, new Uint8Array(0), 'ghostex-collab/route/v1');
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
}

function nonce(counter) {
  const iv = new Uint8Array(12);
  new DataView(iv.buffer).setBigUint64(4, BigInt(counter));
  return iv;
}

/** One encrypted connection to the owner's gxserver through the relay. */
export class CollabTunnel {
  constructor(relayOrigin, key) {
    this.relayOrigin = relayOrigin;
    this.key = key;
    this.listeners = new Set();
    this.closeListeners = new Set();
    this.ws = null;
  }

  onMessage(listener) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  onClose(listener) {
    this.closeListeners.add(listener);
    return () => this.closeListeners.delete(listener);
  }

  /** Opens the socket and sends the hello; resolves once frames can be sealed. */
  async connect() {
    const route = await routeId(this.key);
    const salt = crypto.getRandomValues(new Uint8Array(32));
    const [sealKey, openKey] = await Promise.all(
      ['ghostex-collab/g2h/v1', 'ghostex-collab/h2g/v1'].map(async (info) =>
        crypto.subtle.importKey('raw', await hkdf(this.key, salt, info), 'AES-GCM', false, [
          'encrypt',
          'decrypt',
        ]),
      ),
    );
    this.sealKey = sealKey;
    this.openKey = openKey;
    this.sealed = 0;
    this.opened = 0;
    this.sendChain = Promise.resolve();
    this.recvChain = Promise.resolve();
    const url = `${this.relayOrigin.replace(/^http/, 'ws')}/g/${route}`;
    const ws = new NativeWebSocket(url);
    ws.binaryType = 'arraybuffer';
    this.ws = ws;
    await new Promise((resolve, reject) => {
      ws.onopen = resolve;
      ws.onclose = (event) => reject(Object.assign(new Error(event.reason || 'closed'), { code: event.code }));
    });
    const hello = new Uint8Array(33);
    hello[0] = 1;
    hello.set(salt, 1);
    ws.send(hello);
    // Reached only when the owner's computer answers with a frame this key opens; a relay that
    // knows no such link closes the socket instead (CLOSE.offline / CLOSE.ended).
    const ready = new Promise((resolve, reject) => {
      const stop = this.onMessage((message) => {
        if (message.t !== 'ready') return;
        stop();
        resolve();
      });
      ws.onclose = (event) => {
        stop();
        reject(Object.assign(new Error(event.reason || 'closed'), { code: event.code }));
      };
    });
    ws.onmessage = (event) => {
      const frame = new Uint8Array(event.data);
      this.recvChain = this.recvChain.then(async () => {
        try {
          const counter = this.opened++;
          const plain = await crypto.subtle.decrypt({ name: 'AES-GCM', iv: nonce(counter) }, this.openKey, frame);
          const message = JSON.parse(decoder.decode(plain));
          for (const listener of this.listeners) listener(message);
        } catch (error) {
          console.error('Ghostex Collab: a frame did not open', error);
          ws.close();
        }
      });
    };
    await ready;
    ws.onclose = (event) => {
      if (this.ws !== ws) return;
      this.ws = null;
      for (const listener of this.closeListeners) listener(event.code, event.reason);
    };
  }

  get open() {
    return this.ws?.readyState === NativeWebSocket.OPEN;
  }

  send(message) {
    const ws = this.ws;
    if (!ws) return;
    const plain = encoder.encode(JSON.stringify(message));
    const counter = this.sealed++;
    this.sendChain = this.sendChain.then(async () => {
      const sealed = await crypto.subtle.encrypt({ name: 'AES-GCM', iv: nonce(counter) }, this.sealKey, plain);
      if (ws.readyState === NativeWebSocket.OPEN) ws.send(sealed);
    });
  }

  /** Sends `message` and resolves with the first reply `match` accepts. */
  ask(message, match, timeoutMs = 30000) {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        stop();
        reject(new Error('The chat’s computer did not answer.'));
      }, timeoutMs);
      const stop = this.onMessage((reply) => {
        if (!match(reply)) return;
        clearTimeout(timer);
        stop();
        resolve(reply);
      });
      this.send(message);
    });
  }

  close() {
    this.ws?.close();
  }
}

function isTunnelUrl(url) {
  try {
    return new URL(url, location.href).hostname === TUNNEL_HOST;
  } catch {
    return false;
  }
}

/**
 * Routes the web build's calls to TUNNEL_ORIGIN through `session`, which hands out the live tunnel
 * (it changes when the page reconnects): `session.ready()` resolves with an open, joined tunnel.
 */
export function installTunnelTransport(session) {
  let nextId = 1;
  const pending = new Map();
  const sockets = new Map();

  session.onMessage((message) => {
    switch (message.t) {
      case 'http': {
        const waiter = pending.get(message.id);
        if (waiter) {
          pending.delete(message.id);
          waiter.resolve(message);
        }
        break;
      }
      case 'ws-open':
        sockets.get(message.id)?._opened();
        break;
      case 'ws':
        sockets.get(message.id)?._message(message.text);
        break;
      case 'ws-close':
        sockets.get(message.id)?._closed(1006, 'closed by the chat’s computer');
        break;
    }
  });
  session.onDisconnect(() => {
    for (const [, waiter] of pending) waiter.reject(new TypeError('Ghostex Collab disconnected'));
    pending.clear();
    for (const [, socket] of sockets) socket._closed(1006, 'disconnected');
  });

  globalThis.fetch = async (input, init) => {
    const request = input instanceof Request ? input : new Request(input, init);
    if (!isTunnelUrl(request.url)) return nativeFetch(input, init);
    const url = new URL(request.url);
    const body = request.method === 'GET' ? '' : await request.text();
    const tunnel = await session.ready();
    const id = nextId++;
    const reply = await new Promise((resolve, reject) => {
      pending.set(id, { resolve, reject });
      tunnel.send({ t: 'http', id, path: url.pathname, body });
    });
    const noBody = [101, 204, 205, 304].includes(reply.status);
    return new Response(noBody ? null : reply.body, {
      status: reply.status || 502,
      headers: { 'content-type': 'application/json' },
    });
  };

  class TunnelSocket extends EventTarget {
    static CONNECTING = 0;
    static OPEN = 1;
    static CLOSING = 2;
    static CLOSED = 3;

    constructor(url) {
      super();
      if (!isTunnelUrl(url)) throw new DOMException('Only the shared chat can be reached here.', 'SecurityError');
      this.CONNECTING = 0;
      this.OPEN = 1;
      this.CLOSING = 2;
      this.CLOSED = 3;
      this.url = String(url);
      this.readyState = 0;
      this.binaryType = 'blob';
      this.protocol = '';
      this.extensions = '';
      this.bufferedAmount = 0;
      this.onopen = null;
      this.onmessage = null;
      this.onerror = null;
      this.onclose = null;
      this.id = nextId++;
      sockets.set(this.id, this);
      const parsed = new URL(this.url);
      session.ready().then(
        (tunnel) => {
          if (this.readyState === 0) tunnel.send({ t: 'ws-open', id: this.id, path: parsed.pathname + parsed.search });
        },
        () => this._closed(1006, 'disconnected'),
      );
    }

    _emit(event) {
      const handler = this['on' + event.type];
      if (typeof handler === 'function') handler.call(this, event);
      this.dispatchEvent(event);
    }

    _opened() {
      if (this.readyState !== 0) return;
      this.readyState = 1;
      this._emit(new Event('open'));
    }

    _message(text) {
      if (this.readyState !== 1) return;
      this._emit(new MessageEvent('message', { data: text }));
    }

    _closed(code, reason) {
      if (this.readyState === 3) return;
      this.readyState = 3;
      sockets.delete(this.id);
      this._emit(new CloseEvent('close', { code, reason, wasClean: code === 1000 }));
    }

    send(data) {
      if (this.readyState !== 1) throw new DOMException('The socket is not open.', 'InvalidStateError');
      session.current()?.send({ t: 'ws', id: this.id, text: String(data) });
    }

    close() {
      if (this.readyState >= 2) return;
      session.current()?.send({ t: 'ws-close', id: this.id });
      this._closed(1000, '');
    }
  }

  globalThis.WebSocket = TunnelSocket;
}
