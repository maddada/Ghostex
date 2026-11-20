#!/usr/bin/env node
// Drive one persistent headless Chromium (started by webdrive-chrome.sh) over DevTools, one command per call.
//   webdrive.mjs open <url> | eval <js> | shot <file> | click x y | rclick x y | type <text> | key <Key> [mods]
//   webdrive.mjs a11y [filter]   (lists gpuiA11y nodes: id role label bounds, optionally filtered by label/role substring)
//   webdrive.mjs act <id> [Action] | logs
// Profile dir: $WEBDRIVE_PROFILE (default ~/.local/share/agent-work/Ghostex/smoke/chrome-profile).
import { readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
const profile = process.env.WEBDRIVE_PROFILE || join(homedir(), '.local/share/agent-work/Ghostex/smoke/chrome-profile');
const port = readFileSync(join(profile, 'DevToolsActivePort'), 'utf8').split('\n')[0].trim();
const [cmd, ...rest] = process.argv.slice(2);
setTimeout(() => { console.error('webdrive: timed out (page busy?)'); process.exit(3); }, +(process.env.WEBDRIVE_TIMEOUT_MS || 60000)).unref();
const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const match = process.env.WEBDRIVE_MATCH || '127.0.0.1:41';
let page = targets.find((t) => t.type === 'page' && t.url.includes(match)) || targets.find((t) => t.type === 'page');
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r) => ws.addEventListener('open', r));
let id = 0; const pending = new Map(); const events = [];
ws.addEventListener('message', (m) => { const d = JSON.parse(m.data); if (d.id && pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); } else events.push(d); });
const send = (method, params = {}) => new Promise((res, rej) => { const i = ++id; pending.set(i, (d) => d.error ? rej(new Error(method + ': ' + JSON.stringify(d.error))) : res(d.result)); ws.send(JSON.stringify({ id: i, method, params })); });
const evaluate = async (expr) => { const r = await send('Runtime.evaluate', { expression: expr, awaitPromise: true, returnByValue: true }); if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails).slice(0, 500)); return r.result.value; };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const mouse = async (x, y, button = 'left') => { for (const type of ['mouseMoved', 'mousePressed', 'mouseReleased']) { await send('Input.dispatchMouseEvent', { type, x, y, button, clickCount: type === 'mouseMoved' ? 0 : 1 }); await sleep(40); } };
const MOD = { alt: 1, ctrl: 2, meta: 4, shift: 8 };
const KEYS = { Enter: [13, 'Enter', '\r'], Escape: [27, 'Escape'], Backspace: [8, 'Backspace'], Tab: [9, 'Tab'], ArrowDown: [40, 'ArrowDown'], ArrowUp: [38, 'ArrowUp'] };
switch (cmd) {
  case 'open': await send('Page.navigate', { url: rest[0] }); console.log('navigated'); break;
  case 'eval': console.log(JSON.stringify(await evaluate(rest.join(' ')), null, 1)); break;
  case 'shot': { const r = await send('Page.captureScreenshot', { format: 'png' }); (await import('node:fs')).writeFileSync(rest[0], Buffer.from(r.data, 'base64')); console.log(rest[0]); break; }
  case 'click': await mouse(+rest[0], +rest[1]); console.log('clicked'); break;
  case 'rclick': await mouse(+rest[0], +rest[1], 'right'); console.log('rclicked'); break;
  case 'type': for (const ch of rest.join(' ')) { await send('Input.dispatchKeyEvent', { type: 'keyDown', key: ch, text: ch, unmodifiedText: ch }); await send('Input.dispatchKeyEvent', { type: 'keyUp', key: ch }); await sleep(15); } console.log('typed'); break;
  case 'insert': await send('Input.insertText', { text: rest.join(' ') }); console.log('inserted'); break;
  case 'key': { const k = rest[0]; const mods = (rest[1] || '').split('+').filter(Boolean).reduce((a, m) => a | MOD[m], 0); const [code, key, text] = KEYS[k] || [k.toUpperCase().charCodeAt(0), k]; await send('Input.dispatchKeyEvent', { type: 'rawKeyDown', key, windowsVirtualKeyCode: code, modifiers: mods, ...(text && !mods ? { text } : {}) }); if (text && !mods) await send('Input.dispatchKeyEvent', { type: 'char', key, text, modifiers: mods }); await send('Input.dispatchKeyEvent', { type: 'keyUp', key, windowsVirtualKeyCode: code, modifiers: mods }); console.log('key', k, rest[1] || ''); break; }
  case 'a11y': { const f = (rest[0] || '').toLowerCase(); const snap = await evaluate('JSON.stringify(window.gpuiA11y ? window.gpuiA11y.snapshot() : null)'); const out = []; const walk = (n, w) => { if (!n) return; if (Array.isArray(n)) return n.forEach((c) => walk(c, w)); const label = n.label || n.name || ''; const line = `${w}${n.id} ${n.role} "${String(label).slice(0, 80)}"${n.value !== undefined ? ' value=' + JSON.stringify(n.value).slice(0, 80) : ''} ${n.bounds ? JSON.stringify(n.bounds) : ''}${n.actions ? ' [' + n.actions + ']' : ''}`; if (!f || line.toLowerCase().includes(f)) out.push(line); for (const c of n.children || []) walk(c, w); }; const s = JSON.parse(snap); if (!s) { console.log('no gpuiA11y'); break; } if (Array.isArray(s)) s.forEach((win, i) => walk(win.root || win.nodes || win, i ? i + ':' : '')); else walk(s.root || s.nodes || s, ''); console.log(out.join('\n') || '(none)'); break; }
  case 'act': console.log(JSON.stringify(await evaluate(`window.gpuiA11y.act(${JSON.stringify(rest[0])}, ${JSON.stringify(rest[1] || 'Click')})`))); break;
  case 'raw': console.log(JSON.stringify(await evaluate('JSON.stringify(window.gpuiA11y.snapshot()).slice(0, 3000)'))); break;
  default: console.log('usage: see header'); process.exit(2);
}
ws.close(); process.exit(0);
