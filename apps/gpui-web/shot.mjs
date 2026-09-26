#!/usr/bin/env node
// Drives the page in headless Chrome over the DevTools protocol (no dependencies): waits in real time so the gxserver WebSocket can deliver, prints the page's console, and saves a screenshot.
// usage: node shot.mjs <out.png> [--wait ms] [--timeout ms] [--size WxH] [--url url] [--click x,y]... [--wheel x,y,deltaY] [--type text] [--insert text] [--key Enter]
import { spawn } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const args = process.argv.slice(2);
const out = args.shift();
const option = (name, fallback) => {
  const index = args.indexOf(name);
  return index === -1 ? fallback : args[index + 1];
};
const steps = [];
for (let i = 0; i < args.length; i += 2) {
  if (['--click', '--rightclick', '--type', '--insert', '--key', '--pause', '--move', '--eval', '--print', '--pasteimage', '--wheel'].includes(args[i])) {
    steps.push([args[i].slice(2), args[i + 1]]);
  }
}
const wait = Number(option('--wait', 4000));
const [width, height] = option('--size', '1280x800').split('x').map(Number);
const url = option('--url', 'http://localhost:4174/');

import { existsSync, readdirSync } from 'node:fs';
import { homedir } from 'node:os';

// Playwright's Chrome for Testing when it is installed (the system Chrome stopped exposing DevTools in headless mode after its 153 update), else the system Chrome.
function chromeBinary() {
  // An explicit binary wins: Playwright's headless shell keeps working when Chrome for Testing stalls on a loaded machine (its navigations never commit).
  if (process.env.SHOT_CHROME) return process.env.SHOT_CHROME;
  if (process.platform === 'win32') {
    const cache = join(process.env.LOCALAPPDATA ?? join(homedir(), 'AppData/Local'), 'ms-playwright');
    const builds = existsSync(cache) ? readdirSync(cache).filter(name => /^chromium-\d+$/.test(name)).sort().reverse() : [];
    for (const build of builds) {
      for (const directory of ['chrome-win64', 'chrome-win']) {
        const candidate = join(cache, build, directory, 'chrome.exe');
        if (existsSync(candidate)) return candidate;
      }
    }
    for (const root of [process.env.PROGRAMFILES, process.env['PROGRAMFILES(X86)'], process.env.LOCALAPPDATA].filter(Boolean)) {
      const candidate = join(root, 'Google/Chrome/Application/chrome.exe');
      if (existsSync(candidate)) return candidate;
    }
    throw new Error('Install Chrome or Playwright Chromium to run GPUI web screenshots.');
  }
  const cache = join(homedir(), 'Library/Caches/ms-playwright');
  const builds = existsSync(cache) ? readdirSync(cache).filter((name) => /^chromium-\d+$/.test(name)).sort() : [];
  for (const build of builds.reverse()) {
    const candidate = join(cache, build, 'chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing');
    if (existsSync(candidate)) return candidate;
  }
  return '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
}

// CDXC:WebGpui 2026-09-24 WHY: A random port in 9300-9799 could land on Ghostex's own CEF DevTools port (9333-9343); Chrome then failed to bind and this script drove the user's live app instead, loading a Storybook story into the Add Project modal. Chrome picks a free port and reports it in its own profile, so only the Chrome we spawned is reachable.
const profileDir = mkdtempSync(join(tmpdir(), 'gpui-web-shot-'));
const chrome = spawn(
  chromeBinary(),
  [
    '--headless=new',
    '--enable-unsafe-webgpu',
    ...(process.platform === 'darwin' ? ['--use-angle=metal'] : []),
    '--remote-debugging-port=0',
    `--user-data-dir=${profileDir}`,
    `--window-size=${width},${height}`,
    'about:blank',
  ],
  { stdio: ['ignore', 'ignore', process.env.SHOT_CHROME_LOG ? 'inherit' : 'ignore'], windowsHide: true },
);
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
let socket;
// A run that stalls (a page that stops answering, a navigation that never commits) would otherwise be killed from outside, skipping the `finally` below and leaving its Chrome and its ~100 MB profile behind. The watchdog ends it from inside.
const watchdog = setTimeout(() => {
  console.log('[timeout] the run did not finish; Chrome was stopped');
  chrome.kill('SIGKILL');
  rmSync(profileDir, { recursive: true, force: true });
  process.exit(2);
}, Number(option('--timeout', 90000)));

try {
  let target;
  for (let attempt = 0; attempt < 50 && !target; attempt++) {
    await sleep(200);
    try {
      const port = readFileSync(join(profileDir, 'DevToolsActivePort'), 'utf8').split('\n')[0];
      const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
      target = targets.find((entry) => entry.type === 'page');
    } catch {}
  }
  socket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve) => (socket.onopen = resolve));
  let nextId = 1;
  const pending = new Map();
  socket.onmessage = ({ data }) => {
    const message = JSON.parse(data);
    if (message.id) pending.get(message.id)?.(message.result);
    if (message.method === 'Runtime.consoleAPICalled') {
      const text = message.params.args.map((arg) => arg.value ?? arg.description ?? '').join(' ');
      console.log(`[${message.params.type}] ${text.slice(0, 600)}`);
    }
    if (message.method === 'Inspector.targetCrashed') console.log('[crash] the page crashed');
    if (message.method === 'Runtime.exceptionThrown') {
      console.log('[exception]', message.params.exceptionDetails.exception?.description?.slice(0, 1200));
    }
  };
  const send = (method, params = {}) =>
    new Promise((resolve) => {
      const id = nextId++;
      pending.set(id, resolve);
      socket.send(JSON.stringify({ id, method, params }));
    });
  socket.onclose = () => console.log('[devtools socket closed]');
  await send('Inspector.enable');
  await send('Runtime.enable');
  await send('Page.enable');
  await send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false });
  await send('Page.navigate', { url });
  await sleep(wait);
  const mouse = (type, x, y, button = 'left', extra = {}) =>
    send('Input.dispatchMouseEvent', { type, x, y, button, clickCount: 1, pointerType: 'mouse', ...extra });
  for (const [kind, value] of steps) {
    if (kind === 'click' || kind === 'rightclick' || kind === 'move') {
      const [x, y] = value.split(',').map(Number);
      await mouse('mouseMoved', x, y, 'none');
      await sleep(80);
      if (kind !== 'move') {
        const button = kind === 'click' ? 'left' : 'right';
        await mouse('mousePressed', x, y, button);
        await mouse('mouseReleased', x, y, button);
      }
    } else if (kind === 'wheel') {
      // `x,y,deltaY`: one wheel event over the point, for reaching a sidebar row far down the list.
      const [x, y, deltaY] = value.split(',').map(Number);
      await mouse('mouseMoved', x, y, 'none');
      await send('Input.dispatchMouseEvent', { type: 'mouseWheel', x, y, deltaX: 0, deltaY, pointerType: 'mouse' });
    } else if (kind === 'insert') {
      // Text with no key events, the way browser automation and dictation insert it (`Input.insertText`).
      await send('Input.insertText', { text: value });
    } else if (kind === 'type') {
      // Real key events, one per character: the canvas listens for keydown, not for DOM text input.
      for (const character of value) {
        const key = { key: character, text: character, unmodifiedText: character };
        await send('Input.dispatchKeyEvent', { type: 'keyDown', ...key });
        await send('Input.dispatchKeyEvent', { type: 'keyUp', key: character });
      }
    } else if (kind === 'key') {
      // `Control+a` style chords: DevTools modifier bits are Alt=1, Control=2, Meta=4, Shift=8.
      const parts = value.split('+');
      const key = parts.pop();
      const bits = { Alt: 1, Control: 2, Meta: 4, Shift: 8 };
      const modifiers = parts.reduce((sum, name) => sum | (bits[name] ?? 0), 0);
      await send('Input.dispatchKeyEvent', { type: 'keyDown', key, code: key, modifiers });
      await send('Input.dispatchKeyEvent', { type: 'keyUp', key, code: key, modifiers });
    } else if (kind === 'eval') {
      await send('Runtime.evaluate', { expression: value, awaitPromise: true });
    } else if (kind === 'print') {
      // Evaluate and print the JSON of the result: `--print "window.gpuiA11y.snapshot()"` dumps the accessibility tree.
      const { result } = await send('Runtime.evaluate', { expression: `JSON.stringify(${value})`, awaitPromise: true, returnByValue: true });
      console.log(result?.value ?? result?.description ?? '');
    } else if (kind === 'pasteimage') {
      // A paste event carrying a small PNG file, fired at the focused element, the way a browser delivers a pasted screenshot.
      const expression = `(() => {
        const png = Uint8Array.from(atob('${value}'), (c) => c.charCodeAt(0));
        const data = new DataTransfer();
        data.items.add(new File([png], 'pasted.png', { type: 'image/png' }));
        const event = new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true });
        (document.activeElement ?? document.body).dispatchEvent(event);
      })()`;
      await send('Runtime.evaluate', { expression });
    } else if (kind === 'pause') {
      await sleep(Number(value));
      continue;
    }
    await sleep(600);
  }
  const { data } = await send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(out, Buffer.from(data, 'base64'));
  console.log(`saved ${out}`);
  await send('Browser.close');
} finally {
  socket?.close();
  clearTimeout(watchdog);
  chrome.kill('SIGKILL');
  // The throwaway Chrome profile is ~100 MB; without this every screenshot left one in $TMPDIR.
  setTimeout(() => rmSync(profileDir, { recursive: true, force: true }), 500);
}
