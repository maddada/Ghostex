import './rust-chat-storage';
import { installSessionChatRuntimeBroker } from '@/apps/desktop/sidebar/session-chat-runtime/broker';

// The desktop's chat broker, run in the page as it is (src/app/chat_host.rs).
globalThis.ghostexInstallChatBroker = installSessionChatRuntimeBroker;
globalThis.ghostexChatDebug = new URLSearchParams(location.search).has('chatDebug');

const loading = document.getElementById('loading');

try {
  // The chat controller bundle each chat's iframe evaluates (src/app/native_chat/runtime_worker.rs).
  globalThis.ghostexChatBundle = await (await fetch('/chat-runtime.js')).text();
  const wasm = await import('./wasm/ghostex_gpui_web.js');
  await wasm.default();
  wasm.run();
  loading.remove();
} catch (error) {
  console.error('Ghostex GPUI web failed to start', error);
  loading.classList.add('error');
  loading.textContent = `Failed to start: ${error?.message ?? error}`;
}
