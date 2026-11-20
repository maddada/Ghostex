// Pause the stuck page's main thread over DevTools and print the call stack (wasm frames by function name).
import { readFileSync } from 'node:fs'; import { homedir } from 'node:os'; import { join } from 'node:path';
const profile = process.env.WEBDRIVE_PROFILE || join(homedir(), '.local/share/agent-work/Ghostex/smoke/chrome-profile');
const port = readFileSync(join(profile, 'DevToolsActivePort'), 'utf8').split('\n')[0].trim();
const page = (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()).find((t) => t.type === 'page' && t.url.includes('127.0.0.1:41'));
const ws = new WebSocket(page.webSocketDebuggerUrl); await new Promise((r) => ws.addEventListener('open', r));
let id = 0; const send = (method, params = {}) => ws.send(JSON.stringify({ id: ++id, method, params }));
setTimeout(() => { console.log('no pause within 60s'); process.exit(3); }, 60000);
let n = 0;
ws.addEventListener('message', (m) => { const d = JSON.parse(m.data);
  if (d.method === 'Debugger.paused') { n++;
    console.log(`--- sample ${n} reason=${d.params.reason}`);
    for (const f of d.params.callFrames.slice(0, 40)) console.log(`${f.functionName || '(anon)'}  ${f.url.split('/').pop()}:${f.location.lineNumber}:${f.location.columnNumber}`);
    send('Debugger.resume'); if (n >= 3) process.exit(0); setTimeout(() => send('Debugger.pause'), 1500); } });
send('Debugger.enable'); setTimeout(() => send('Debugger.pause'), 2000);
