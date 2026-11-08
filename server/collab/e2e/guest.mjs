// A Collab guest for the end-to-end test (server/src/server/tests/collab_component_e2e.rs): the
// guest page's own tunnel (apps/gpui-web/www/src/collab/tunnel.js) run in Node, through a local
// share relay, to the Collab component gxserver started.
//
// Usage: node guest.mjs <relay origin> <link> <code> <projectId> <sessionId>
// Prints one JSON line with what each step answered.
import { CollabTunnel, parseKey } from '../../../apps/gpui-web/www/src/collab/tunnel.js';

const [relay, link, code, projectId, sessionId] = process.argv.slice(2);
const key = parseKey(link.split('#')[1]);
const tunnel = new CollabTunnel(relay, key);
await tunnel.connect();
const intro = await tunnel.ask({ t: 'hello' }, (m) => m.t === 'intro');
const wrong = await tunnel.ask({ t: 'join', name: 'Sam', code: '00000000', device: 'e2edevice' }, (m) => m.t === 'denied' || m.t === 'welcome');
const welcome = await tunnel.ask({ t: 'join', name: 'Sam', code, device: 'e2edevice' }, (m) => m.t === 'welcome' || m.t === 'denied');
let next = 1;
const http = async (path, params) => {
  const id = next++;
  const reply = await tunnel.ask(
    { t: 'http', id, path, body: JSON.stringify({ params }) },
    (m) => m.t === 'http' && m.id === id,
  );
  let body = null;
  try {
    body = JSON.parse(reply.body);
  } catch {}
  return { status: reply.status, body };
};
const read = await http('/api/readSessionChat', { projectId, sessionId });
const other = await http('/api/readSessionChat', { projectId, sessionId: 'another-chat' });
const owner = await http('/api/collab', { op: 'read' });
const sent = await http('/api/sendSessionChatMessage', { projectId, sessionId, text: 'Hello from the e2e guest' });
const secret = await http('/api/readSessionChatImage', { projectId, sessionId, path: '/etc/hosts' });
console.log(
  JSON.stringify({
    intro,
    wrong,
    welcome,
    readStatus: read.status,
    readCollab: read.body?.result?.collab ?? null,
    otherStatus: other.status,
    ownerStatus: owner.status,
    sent: sent.body,
    secretStatus: secret.status,
  }),
);
tunnel.close();
process.exit(0);
