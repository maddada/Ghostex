// Reload the Ghostex page and print its console/exception output for N seconds (default 20).
import { readFileSync } from 'node:fs'; import { homedir } from 'node:os'; import { join } from 'node:path';
const profile = process.env.WEBDRIVE_PROFILE || join(homedir(), '.local/share/agent-work/Ghostex/smoke/chrome-profile');
const port = readFileSync(join(profile, 'DevToolsActivePort'), 'utf8').split('\n')[0].trim();
const page = (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()).find((t) => t.type === 'page' && t.url.includes(process.env.WEBDRIVE_MATCH || '127.0.0.1:41'));
const ws = new WebSocket(page.webSocketDebuggerUrl); await new Promise((r) => ws.addEventListener('open', r));
let id = 0; const send = (method, params = {}) => ws.send(JSON.stringify({ id: ++id, method, params }));
ws.addEventListener('message', (m) => { const d = JSON.parse(m.data);
  if (d.method === 'Runtime.consoleAPICalled') console.log(`[${d.params.type}]`, d.params.args.map((a) => a.value ?? a.description ?? '').join(' ').slice(0, 300));
  if (d.method === 'Runtime.exceptionThrown') console.log('[exception]', (d.params.exceptionDetails.exception?.description || d.params.exceptionDetails.text).slice(0, 400));
  if (d.method === 'Log.entryAdded') console.log(`[log ${d.params.entry.level}]`, d.params.entry.text.slice(0, 300)); });
send('Runtime.enable'); send('Log.enable'); if (process.argv[3] !== 'noreload') send('Page.reload');
setTimeout(() => process.exit(0), (+process.argv[2] || 20) * 1000);
