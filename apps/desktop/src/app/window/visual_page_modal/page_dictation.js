// What the floating page window adds so a page's text boxes can dictate (page_dictation.rs puts
// this into every document the window loads, after the page bridge's queue). The page's question
// kit (server/src/visual_pages/kit.rs) draws a microphone on each note and text box when it finds
// `window.__ghostexHostDictate`, and calls it with "toggle" (start, or stop and transcribe) or
// "cancel". The app answers with a `gx-dictation` event on the window: `{phase}` as the recording
// moves on, `{text}` with the words, `{error, message}` when something went wrong.
(() => {
  if (window.top !== window || window.__ghostexHostDictate) return;
  Object.defineProperty(window, '__ghostexHostDictate', {
    value: (action) => {
      if (action !== 'toggle' && action !== 'cancel') return;
      try { window.__ghostexPageBridge(JSON.stringify({ type: 'dictate', action })); } catch (_) {}
    },
  });
})();
