// Ghostex Collab's page viewer: a guest opens a Ghostex page the shared chat published, or an HTML
// file it links to, over the chat. The guest's browser cannot reach the owner's computer, so the
// page's HTML comes through the share's tunnel (`/api/collabGuestPage`), and the page's answers go
// back the same way (`/api/collabGuestPageAnswers`), reaching the chat as the guest's message.
// The page runs in a sandboxed frame with its own opaque origin: it cannot touch this page or the
// tunnel, only ask for the three things below through postMessage.
//
// CDXC:Collab 2026-11-07 DECISION: User: "Pls make sure htmls and images can be viewed by the guest when clicking on them and even inline htmls can be answered by them."
// SEE-ALSO: server/src/visual_pages/guest.rs (what a guest may open), apps/gpui-web/src/app/chat_host.rs (the clicks).

import { TUNNEL_ORIGIN } from './tunnel.js';

const STYLE = `
.gx-page-viewer { position: fixed; inset: 0; z-index: 2147483000; display: flex; flex-direction: column; background: rgba(22,22,24,0.55); backdrop-filter: blur(10px); -webkit-backdrop-filter: blur(10px); }
.gx-page-bar { flex: none; display: flex; align-items: center; gap: 10px; height: 44px; padding: 0 8px 0 14px; padding-top: env(safe-area-inset-top, 0px); background: rgba(236,236,240,0.92); color: #1c1c1e; font: 600 13px system-ui, -apple-system, sans-serif; border-bottom: 1px solid rgba(0,0,0,0.08); }
.gx-page-title { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.gx-page-note { flex: none; font: 500 12px system-ui, -apple-system, sans-serif; color: rgba(28,28,30,0.7); }
.gx-page-close { flex: none; width: 32px; height: 32px; border-radius: 8px; border: 0; background: rgba(0,0,0,0.06); color: #1c1c1e; font: 18px/1 system-ui, sans-serif; cursor: pointer; }
.gx-page-frame { flex: 1; min-height: 0; width: 100%; border: 0; background: #fff; }
.gx-page-status { margin: auto; padding: 14px 18px; border-radius: 12px; background: rgba(236,236,240,0.92); color: #1c1c1e; font: 13px/1.45 system-ui, -apple-system, sans-serif; max-width: min(420px, 90vw); }
`;

// Runs first in the page: its answers, its pasted pictures, and the routes a shared chat has no use
// for (its draft, scroll position and notice, which stay with the owner's own reading).
const SHIM = `<script>(function(){
var waiting={},next=0;
function ask(kind,payload){return new Promise(function(resolve){var id=++next;waiting[id]=resolve;parent.postMessage({gxGuestPage:true,id:id,kind:kind,payload:payload},"*");});}
addEventListener("message",function(event){var data=event.data;if(event.source!==parent||!data||!data.gxGuestPageReply)return;var resolve=waiting[data.id];if(resolve){delete waiting[data.id];resolve(data);}});
function json(status,body){return new Response(JSON.stringify(body),{status:status,headers:{"content-type":"application/json"}});}
function route(url){var path=String(url).split("?")[0];var match=/\\/(answers|image|draft|scroll|notice)$/.exec(path);return match&&match[1];}
var pageFetch=window.fetch.bind(window);
window.fetch=function(input,init){
  var url=typeof input==="string"?input:(input&&input.url)||"";
  var kind=route(url);
  if(!kind)return pageFetch(input,init);
  if(kind==="answers"){var body=init&&typeof init.body==="string"?init.body:"{}";return ask("answers",body).then(function(reply){return json(reply.status,reply.body);});}
  if(kind==="image"&&init&&init.body instanceof Blob){return new Promise(function(resolve){var reader=new FileReader();reader.onload=function(){resolve(ask("image",reader.result).then(function(reply){return json(reply.status,reply.body);}));};reader.onerror=function(){resolve(json(400,{ok:false,error:"The pasted image could not be read."}));};reader.readAsDataURL(init.body);});}
  return Promise.resolve(json(404,{ok:false,error:"Not kept for a shared chat."}));
};
if(navigator.sendBeacon){var beacon=navigator.sendBeacon.bind(navigator);navigator.sendBeacon=function(url,data){return route(url)?true:beacon(url,data);};}
})();</script>`;

function withShim(html) {
  const head = /<head[^>]*>/i.exec(html);
  if (head) return html.slice(0, head.index + head[0].length) + SHIM + html.slice(head.index + head[0].length);
  const doctype = /<!doctype[^>]*>/i.exec(html);
  if (doctype) return html.slice(0, doctype.index + doctype[0].length) + SHIM + html.slice(doctype.index + doctype[0].length);
  return SHIM + html;
}

async function rpc(path, params) {
  const response = await fetch(`${TUNNEL_ORIGIN}${path}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ protocolVersion: 1, params }),
  });
  const reply = await response.json().catch(() => ({}));
  if (!response.ok || reply.ok === false) {
    const message = reply.message || reply.error?.message || `The chat\u2019s computer answered ${response.status}.`;
    return { ok: false, status: response.status, message: String(message) };
  }
  return { ok: true, status: response.status, result: reply.result ?? {} };
}

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text != null) node.textContent = text;
  return node;
}

/** Lets the app open pages for the guest: `ghostexCollabOpenPage({pageId})` or `({path})`. */
export function installGuestPages(guest) {
  document.head.append(Object.assign(document.createElement('style'), { textContent: STYLE }));
  let open = null;

  const close = () => {
    open?.remove();
    open = null;
  };
  addEventListener('keydown', (event) => {
    if (open && event.key === 'Escape') close();
  });

  globalThis.ghostexCollabOpenPage = async (request) => {
    close();
    const viewer = el('div', 'gx-page-viewer');
    const bar = el('div', 'gx-page-bar');
    const title = el('div', 'gx-page-title', request.title || 'Page');
    const closeButton = Object.assign(el('button', 'gx-page-close', '×'), { type: 'button', title: 'Close' });
    closeButton.setAttribute('aria-label', 'Close the page');
    closeButton.addEventListener('click', close);
    const note = el('div', 'gx-page-note');
    bar.append(title, note, closeButton);
    const status = el('div', 'gx-page-status', 'Opening the page…');
    viewer.append(bar, status);
    document.body.append(viewer);
    open = viewer;

    const params = { projectId: guest.projectId, sessionId: guest.sessionId };
    if (request.pageId) params.pageId = request.pageId;
    else params.path = request.path;
    const reply = await rpc('/api/collabGuestPage', params);
    if (open !== viewer) return;
    if (!reply.ok) {
      status.textContent = reply.message;
      return;
    }
    const page = reply.result;
    title.textContent = page.title || title.textContent;
    const frame = el('iframe', 'gx-page-frame');
    // Its own opaque origin: scripts and forms run, nothing reaches this page.
    frame.setAttribute('sandbox', 'allow-scripts allow-forms allow-popups allow-popups-to-escape-sandbox');
    frame.setAttribute('referrerpolicy', 'no-referrer');
    frame.title = page.title || 'Page';
    frame.srcdoc = withShim(page.html);
    status.replaceWith(frame);

    const onMessage = async (event) => {
      if (event.source !== frame.contentWindow || !event.data?.gxGuestPage) return;
      const { id, kind, payload } = event.data;
      const answer = (status, body) => frame.contentWindow?.postMessage({ gxGuestPageReply: true, id, status, body }, '*');
      if (kind === 'answers') {
        if (!page.pageId || !page.answerable) return answer(409, { ok: false, error: 'This page doesn’t answer to the shared chat.' });
        let answers = null;
        try {
          answers = JSON.parse(payload).answers ?? null;
        } catch {
          return answer(400, { ok: false, error: 'The answers could not be read.' });
        }
        const sent = await rpc('/api/collabGuestPageAnswers', { ...params, pageId: page.pageId, answers });
        if (!sent.ok) return answer(sent.status || 502, { ok: false, error: sent.message });
        const held = Boolean(sent.result?.collab?.held);
        // The page's own note says the agent has the answers; while the owner still has to approve
        // them, the bar says so.
        note.textContent = held ? `Sent. Waiting for ${guest.owner} to approve it.` : 'Sent to the chat.';
        return answer(200, { ok: true, answeredAt: new Date().toISOString(), delivered: !held });
      }
      if (kind === 'image') {
        const saved = await rpc('/api/saveSessionChatImage', { projectId: guest.projectId, sessionId: guest.sessionId, base64Data: payload });
        if (!saved.ok || !saved.result?.path) return answer(saved.status || 502, { ok: false, error: saved.message || 'The pasted image could not be saved.' });
        return answer(200, { ok: true, path: saved.result.path });
      }
    };
    addEventListener('message', onMessage);
    const observer = new MutationObserver(() => {
      if (!viewer.isConnected) {
        removeEventListener('message', onMessage);
        observer.disconnect();
      }
    });
    observer.observe(document.body, { childList: true });
  };
}
