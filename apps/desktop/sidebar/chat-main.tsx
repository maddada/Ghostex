import '@/packages/core-ui/styles.css';
import { flushSync } from 'react-dom';
import { createRoot } from 'react-dom/client';
import {
  activateSessionChatPage,
  chatBridgeNamespace,
  validatedBootstrap,
  type SessionChatPageActivation,
} from './chat-page-runtime';
import { disposeSessionChatRuntime, updateSessionChatRuntimeEndpoint } from './session-chat-runtime';

const rootElement = document.getElementById('root');
if (!rootElement) throw new Error('Ghostex session chat root element was not found.');
const root = createRoot(rootElement);
const namespace = chatBridgeNamespace();
let active: ReturnType<typeof activateSessionChatPage> | undefined;
let activeActivation: SessionChatPageActivation | undefined;
let bootstrapTimer: number | undefined;
let awaitingBootstrap = false;

function activate(activation: SessionChatPageActivation): void {
  if (!/^\d+$/.test(activation.generation)) return;
  if (activeActivation && BigInt(activation.generation) < BigInt(activeActivation.generation)) return;
  let params: URLSearchParams;
  try {
    params = new URL(activation.url, window.location.href).searchParams;
  } catch {
    return;
  }
  if (!params.get('projectId') || !params.get('sessionId')) return;
  const bootstrap = validatedBootstrap(activation.bootstrap);
  if (activeActivation?.generation === activation.generation) {
    activeActivation.bootstrap = bootstrap;
    updateSessionChatRuntimeEndpoint(params.get('remoteMachineId') || 'local', bootstrap);
    if (active || !awaitingBootstrap) return;
  }
  if (bootstrapTimer !== undefined) {
    window.clearTimeout(bootstrapTimer);
    bootstrapTimer = undefined;
  }
  active?.dispose();
  active = undefined;
  activation.bootstrap = bootstrap;
  activeActivation = activation;
  namespace.sessionChatActivation = activation;
  if (!bootstrap) {
    updateSessionChatRuntimeEndpoint(params.get('remoteMachineId') || 'local');
    awaitingBootstrap = true;
    flushSync(() => root.render(null));
    return;
  }
  awaitingBootstrap = false;
  active = activateSessionChatPage(root, activation);
}

namespace.onSessionChatActivate = activate;
namespace.onSessionChatDeactivate = (generation) => {
  if (activeActivation?.generation !== generation) return;
  flushSync(() => root.render(null));
  active?.dispose();
  active = undefined;
  awaitingBootstrap = false;
};
namespace.onGxserverBootstrapChanged = (candidate) => {
  const bootstrap = validatedBootstrap(candidate);
  namespace.gxserverBootstrap = bootstrap;
  if (!activeActivation) {
    if (bootstrap) initialize();
    return;
  }
  activeActivation.bootstrap = bootstrap;
  const params = new URL(activeActivation.url, window.location.href).searchParams;
  updateSessionChatRuntimeEndpoint(params.get('remoteMachineId') || 'local', bootstrap);
  if (bootstrap && awaitingBootstrap && !active) activate(activeActivation);
};

let bootstrapAttempts = 0;
function initialize(): void {
  if (active || awaitingBootstrap) return;
  if (namespace.sessionChatActivation) {
    activate(namespace.sessionChatActivation);
    if (active || awaitingBootstrap) return;
  }
  const bootstrap = validatedBootstrap(namespace.gxserverBootstrap);
  const generation = new URLSearchParams(window.location.search).get('pageGeneration');
  if (bootstrap && generation) {
    activate({ url: window.location.href, generation, bootstrap });
    if (active) return;
  }
  if (++bootstrapAttempts > 250) {
    root.render(<div className='ghostex-chat-empty-state'>The chat connection could not be initialized.</div>);
    return;
  }
  bootstrapTimer = window.setTimeout(initialize, 120);
}

initialize();
window.addEventListener('pagehide', () => {
  if (bootstrapTimer !== undefined) window.clearTimeout(bootstrapTimer);
  active?.dispose();
  disposeSessionChatRuntime();
});
