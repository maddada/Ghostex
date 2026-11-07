// Ghostex Collab's guest page: the link `share.ghostex.dev/s#<key>` opens the GPUI web build here.
// Before the app starts, this connects to the owner's computer through the relay (tunnel.js), asks
// the guest's name and the share's code, and hands the app a guest endpoint that reaches only the
// shared chat. It reconnects on its own when the connection drops.
//
// CDXC:Collab 2026-11-07 DECISION: User (Q1612, Q1613): the guest's page is "the GPUI web build in a guest mode that shows only that one chat", and a guest sees "everything the chat shows".

import { CLOSE, CollabTunnel, TUNNEL_ORIGIN, installTunnelTransport, parseKey } from './tunnel.js';

const NAME_KEY = 'ghostex-collab-name';
const DEVICE_KEY = 'ghostex-collab-device';

/** This browser's own random id, so an owner who removes a guest keeps them out (the block list). */
function deviceId() {
  let id = localStorage.getItem(DEVICE_KEY);
  if (!id || !/^[0-9a-f]{32}$/.test(id)) {
    id = Array.from(crypto.getRandomValues(new Uint8Array(16)), (b) => b.toString(16).padStart(2, '0')).join('');
    localStorage.setItem(DEVICE_KEY, id);
  }
  return id;
}

const REMOVED = ['You were removed from this chat', 'The chat\u2019s owner removed you, so this link no longer opens it for you.'];
const LOCKED = ['Too many wrong codes', 'This chat stopped taking codes for a few minutes. Ask for the code and try again later.'];
const RETRY_MS = [1000, 2000, 3000, 5000];

export function isGuestPage() {
  return location.pathname === '/s' || location.pathname === '/s/';
}

const STYLE = `
.gx-collab { position: absolute; inset: 0; display: grid; place-items: center; padding: 24px; background: radial-gradient(1200px 600px at 50% -10%, #2a2a2e 0%, #161616 60%); }
.gx-collab-card { width: min(380px, 100%); padding: 24px; border-radius: 14px; background: rgba(255,255,255,0.06); border: 1px solid rgba(255,255,255,0.10); backdrop-filter: blur(24px); box-shadow: 0 20px 60px rgba(0,0,0,0.35); color: #e8e8ea; font: 13px/1.45 system-ui, -apple-system, sans-serif; }
.gx-collab-brand { font-size: 11px; letter-spacing: .04em; text-transform: uppercase; color: rgba(232,232,234,0.55); margin-bottom: 10px; }
.gx-collab-title { font-size: 17px; font-weight: 600; margin-bottom: 4px; }
.gx-collab-note { color: rgba(232,232,234,0.65); margin-bottom: 18px; }
.gx-collab-field { display: grid; gap: 6px; margin-bottom: 12px; }
.gx-collab-field span { font-size: 12px; color: rgba(232,232,234,0.7); }
.gx-collab-field input { height: 34px; padding: 0 10px; border-radius: 8px; border: 1px solid rgba(255,255,255,0.14); background: rgba(255,255,255,0.07); color: #f2f2f4; font: inherit; outline: none; }
.gx-collab-field input:focus { border-color: rgba(160,190,255,0.7); box-shadow: 0 0 0 3px rgba(160,190,255,0.2); }
.gx-collab-code { letter-spacing: .3em; font-variant-numeric: tabular-nums; }
.gx-collab-error { color: #ff9b94; min-height: 18px; margin-bottom: 8px; font-size: 12px; }
.gx-collab-join { width: 100%; height: 34px; border-radius: 8px; border: 1px solid rgba(255,255,255,0.22); background: rgba(235,235,240,0.9); color: #1b1b1f; font: 600 13px system-ui, sans-serif; cursor: pointer; }
.gx-collab-join:disabled { opacity: .55; cursor: default; }
.gx-collab-foot { margin-top: 14px; font-size: 11.5px; color: rgba(232,232,234,0.5); }
.gx-collab-banner { position: fixed; top: 10px; left: 50%; transform: translateX(-50%); z-index: 10; padding: 6px 12px; border-radius: 999px; background: rgba(60,60,66,0.72); border: 1px solid rgba(255,255,255,0.12); backdrop-filter: blur(18px); color: #ececef; font: 12px system-ui, sans-serif; }
`;

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text != null) node.textContent = text;
  return node;
}

function modeNote(intro) {
  switch (intro.mode) {
    case 'send':
      return `${intro.owner}’s computer runs this chat. Your messages go straight to the agent.`;
    case 'view':
      return `${intro.owner}’s computer runs this chat. You can watch it as it works.`;
    default:
      return `${intro.owner}’s computer runs this chat. ${intro.owner} approves each message you send.`;
  }
}

function card(root) {
  root.replaceChildren();
  const wrap = el('div', 'gx-collab');
  const box = el('div', 'gx-collab-card');
  box.append(el('div', 'gx-collab-brand', 'Ghostex Collab'));
  wrap.append(box);
  root.append(wrap);
  return box;
}

function showMessage(root, title, note) {
  const box = card(root);
  box.append(el('div', 'gx-collab-title', title), el('div', 'gx-collab-note', note));
}

function endedText(code) {
  if (code === CLOSE.ended) return ['This link has ended', 'The chat’s owner stopped sharing it.'];
  if (code === CLOSE.full) return ['This chat is full', 'Too many people have it open. Try again later.'];
  return ['Can’t reach this chat', 'The link has ended, or the computer that runs the chat is offline. This page keeps trying.'];
}

/** The live tunnel and its reconnects, as `installTunnelTransport` uses it. */
class GuestSession {
  constructor(key) {
    this.key = key;
    this.tunnel = null;
    this.messageListeners = new Set();
    this.disconnectListeners = new Set();
    this.waiters = [];
  }

  onMessage(listener) {
    this.messageListeners.add(listener);
  }

  onDisconnect(listener) {
    this.disconnectListeners.add(listener);
  }

  current() {
    return this.tunnel;
  }

  ready() {
    if (this.tunnel) return Promise.resolve(this.tunnel);
    return new Promise((resolve, reject) => this.waiters.push({ resolve, reject }));
  }

  async open() {
    const tunnel = new CollabTunnel(location.origin, this.key);
    await tunnel.connect();
    tunnel.onMessage((message) => {
      if (this.tunnel === tunnel) for (const listener of this.messageListeners) listener(message);
    });
    return tunnel;
  }

  adopt(tunnel, onLost) {
    this.tunnel = tunnel;
    for (const waiter of this.waiters.splice(0)) waiter.resolve(tunnel);
    tunnel.onClose((code) => {
      if (this.tunnel !== tunnel) return;
      this.tunnel = null;
      for (const listener of this.disconnectListeners) listener();
      onLost(code);
    });
  }

  failWaiters(error) {
    for (const waiter of this.waiters.splice(0)) waiter.reject(error);
  }
}

async function join(tunnel, name, code) {
  return tunnel.ask({ t: 'join', name, code, device: deviceId() }, (m) => m.t === 'welcome' || m.t === 'denied');
}

/**
 * Connects, asks the guest to join, and resolves with the guest endpoint once they are in.
 * Before it resolves, the page shows its own join card in `root`.
 */
export async function startGuest(root) {
  document.head.append(Object.assign(document.createElement('style'), { textContent: STYLE }));
  const key = parseKey(location.hash.slice(1));
  if (!key) {
    showMessage(root, 'This link is incomplete', 'Copy the whole link from the person who shared the chat, including the part after #.');
    return new Promise(() => {});
  }
  const session = new GuestSession(key);

  let tunnel;
  for (let attempt = 0; ; attempt += 1) {
    try {
      tunnel = await session.open();
      break;
    } catch (error) {
      const [title, note] = endedText(error.code);
      showMessage(root, title, note);
      if (error.code === CLOSE.ended) return new Promise(() => {});
      await new Promise((r) => setTimeout(r, RETRY_MS[Math.min(attempt, RETRY_MS.length - 1)] * 2));
    }
  }
  const intro = await tunnel.ask({ t: 'hello' }, (m) => m.t === 'intro');
  document.title = `${intro.title} · Ghostex Collab`;

  const welcome = await new Promise((resolve) => {
    const box = card(root);
    box.append(
      el('div', 'gx-collab-title', `${intro.owner} shared “${intro.title}”`),
      el('div', 'gx-collab-note', modeNote(intro)),
    );
    const form = el('form');
    const nameField = el('label', 'gx-collab-field');
    const nameInput = Object.assign(el('input'), { required: true, maxLength: 40, autocomplete: 'nickname', value: localStorage.getItem(NAME_KEY) ?? '' });
    nameInput.setAttribute('aria-label', 'Your name');
    nameField.append(el('span', null, 'Your name'), nameInput);
    form.append(nameField);
    let codeInput = null;
    if (intro.needsCode) {
      const codeField = el('label', 'gx-collab-field');
      codeInput = Object.assign(el('input', 'gx-collab-code'), { inputMode: 'numeric', maxLength: 6, autocomplete: 'one-time-code', pattern: '[0-9]{6}', required: true });
      codeInput.setAttribute('aria-label', 'Code');
      codeField.append(el('span', null, `Code from ${intro.owner}`), codeInput);
      form.append(codeField);
    }
    const error = el('div', 'gx-collab-error');
    const button = Object.assign(el('button', 'gx-collab-join', 'Join chat'), { type: 'submit' });
    form.append(error, button);
    box.append(form, el('div', 'gx-collab-foot', 'No account or app needed. Others in the chat see your name.'));
    (nameInput.value ? codeInput ?? nameInput : nameInput).focus();
    form.addEventListener('submit', async (event) => {
      event.preventDefault();
      button.disabled = true;
      error.textContent = '';
      const name = nameInput.value.trim();
      const code = codeInput?.value.trim() ?? '';
      try {
        const reply = await join(tunnel, name, code);
        if (reply.t === 'welcome') {
          localStorage.setItem(NAME_KEY, name);
          resolve({ ...reply, code });
          return;
        }
        if (reply.reason === 'code') {
          error.textContent = reply.triesLeft > 0 ? `That code isn’t right. ${reply.triesLeft} ${reply.triesLeft === 1 ? 'try' : 'tries'} left.` : 'Too many wrong codes. Open the link again.';
          if (reply.triesLeft > 0) button.disabled = false;
        } else if (reply.reason === 'name') {
          error.textContent = 'Type your name first.';
          button.disabled = false;
        } else if (reply.reason === 'removed') {
          showMessage(root, ...REMOVED);
        } else if (reply.reason === 'locked') {
          showMessage(root, ...LOCKED);
        } else {
          const [title, note] = endedText(reply.reason === 'full' ? CLOSE.full : CLOSE.ended);
          showMessage(root, title, note);
        }
      } catch (failure) {
        error.textContent = failure.message;
        button.disabled = false;
      }
    });
  });

  const guest = {
    baseUrl: TUNNEL_ORIGIN,
    viewerId: welcome.viewerId,
    name: welcome.name,
    projectId: welcome.projectId,
    sessionId: welcome.sessionId,
    title: welcome.title,
    owner: welcome.owner,
    mode: welcome.mode,
  };
  globalThis.ghostexCollabGuest = guest;
  installTunnelTransport(session);

  const banner = el('div', 'gx-collab-banner');
  let removed = false;
  session.onMessage((message) => {
    if (message.t !== 'removed') return;
    removed = true;
    showMessage(document.body.appendChild(el('div')), ...REMOVED);
  });
  const lost = async (code) => {
    if (removed) {
      session.failWaiters(new Error('removed'));
      return;
    }
    if (code === CLOSE.ended) {
      showMessage(document.body.appendChild(el('div')), ...endedText(code));
      session.failWaiters(new Error('ended'));
      return;
    }
    banner.textContent = `Reconnecting to ${guest.owner}’s computer…`;
    document.body.append(banner);
    for (let attempt = 0; ; attempt += 1) {
      await new Promise((r) => setTimeout(r, RETRY_MS[Math.min(attempt, RETRY_MS.length - 1)]));
      try {
        const next = await session.open();
        await next.ask({ t: 'hello' }, (m) => m.t === 'intro');
        const reply = await join(next, guest.name, welcome.code);
        if (reply.t !== 'welcome') {
          next.close();
          if (reply.reason === 'removed') {
            banner.remove();
            showMessage(document.body.appendChild(el('div')), ...REMOVED);
            session.failWaiters(new Error('removed'));
            return;
          }
          if (reply.reason === 'ended') {
            banner.remove();
            showMessage(document.body.appendChild(el('div')), ...endedText(CLOSE.ended));
            session.failWaiters(new Error('ended'));
            return;
          }
          continue;
        }
        guest.viewerId = reply.viewerId;
        banner.remove();
        session.adopt(next, lost);
        return;
      } catch {
        // Offline or not answering yet: keep trying.
      }
    }
  };
  session.adopt(tunnel, lost);
  root.replaceChildren(el('div', null, 'Loading Ghostex…'));
  return guest;
}
