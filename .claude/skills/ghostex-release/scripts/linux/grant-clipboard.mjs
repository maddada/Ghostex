// Grant clipboard read/write to the Ghostex web origin in the headless browser (browser-level DevTools).
import { readFileSync } from 'node:fs'; import { homedir } from 'node:os'; import { join } from 'node:path';
const profile = process.env.WEBDRIVE_PROFILE || join(homedir(), '.local/share/agent-work/Ghostex/smoke/chrome-profile');
const [port, path] = readFileSync(join(profile, 'DevToolsActivePort'), 'utf8').split('\n').map((s) => s.trim());
const ws = new WebSocket(`ws://127.0.0.1:${port}${path}`); await new Promise((r) => ws.addEventListener('open', r));
ws.addEventListener('message', (m) => { console.log(m.data.slice(0, 200)); process.exit(0); });
ws.send(JSON.stringify({ id: 1, method: 'Browser.grantPermissions', params: { origin: process.argv[2] || 'http://127.0.0.1:4183', permissions: ['clipboardReadWrite', 'clipboardSanitizedWrite'] } }));
