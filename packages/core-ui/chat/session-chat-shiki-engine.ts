// Creates the Shiki core that Session Chat highlights fences with.
//
// Isolated in its own module for the same reason as
// session-chat-code-grammars.ts: this is the only place that pulls in
// `shiki/core`, the oniguruma engine, its wasm, and both themes — roughly
// 750 KB that no chat pane should pay for until it actually renders a fenced
// code block. gpui's packaged CEF surfaces REPLACE this module at build time
// (`createCefSingleFileEsbuildPlugin` in apps/desktop/vite.config.ts) with a loader
// over a staged classic script, because a file:// page cannot fetch module
// chunks. Keep the public surface to exactly these two exports so that swap
// stays a one-liner.
//
// The mobile webview (tooling/build-mobile-chat.mjs) has no origin at all — the
// React Native host feeds it one HTML string with no base URL — so it can
// neither import module chunks nor load classic scripts. There, both this
// module and the grammar loader are stubbed and the flag below is built as
// false, so fences render as plain `<pre>` without ever attempting a load that
// cannot succeed.

import type { HighlighterCore } from 'shiki/core';

export const SESSION_CHAT_HIGHLIGHTING_AVAILABLE = true;

export async function createSessionChatHighlighterCore(): Promise<HighlighterCore> {
  const [{ createHighlighterCore }, { createOnigurumaEngine }] = await Promise.all([
    import('shiki/core'),
    import('shiki/engine/oniguruma'),
  ]);
  return createHighlighterCore({
    engine: createOnigurumaEngine(import('shiki/wasm')),
    langs: [],
    themes: [import('@shikijs/themes/github-light-default'), import('@shikijs/themes/github-dark-default')],
  });
}
