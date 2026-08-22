/*
 * The tutorial-video window's "press f" simulation.
 *
 * Real app: `GpuiAppModalKind::WatchGhostexVideo` opens a native child window
 * whose top-level document is the YouTube watch page, and the CEF host injects
 * a TRUSTED `f` key press ~1.5s after load so the player goes fullscreen inside
 * that window. A host-injected key press carries user-activation, so YouTube's
 * own handler can call `requestFullscreen()`.
 *
 * Sandbox: the fake native window is an iframe, and the page can only dispatch
 * UNTRUSTED KeyboardEvents — browsers refuse `requestFullscreen()` from those,
 * a restriction the real CEF host key press does not have. So the simulation
 * does both halves deliberately:
 *   1. dispatch the real `f` keydown/keyup pair into the (same-origin, proxied)
 *      watch document, exactly where the host injection lands, and
 *   2. enforce the OUTCOME of that press by injecting a stylesheet that makes
 *      the player fill the window and hides the rest of the watch page.
 * Step 2 is the documented simulation of the real trusted injection — not a
 * fallback for a broken step 1. See README.md ("Tutorial video window").
 */

/** Same delay the real host waits after the page loads before pressing `f`. */
export const TUTORIAL_VIDEO_FULLSCREEN_KEY_DELAY_MS = 1_500;

const FULLSCREEN_STYLE_ID = "sandbox-tutorial-video-fullscreen";

/*
 * What the player itself does on `f`: the video surface takes over the whole
 * window and the rest of the watch page is gone. `!important` beats the
 * player's inline sizing.
 */
const FULLSCREEN_STYLE = `
  html, body {
    background: #000 !important;
    overflow: hidden !important;
  }
  ytd-masthead,
  #masthead-container,
  #secondary,
  #below,
  #comments,
  ytd-watch-metadata,
  #chips-wrapper,
  tp-yt-app-drawer,
  ytd-mini-guide-renderer {
    display: none !important;
  }
  #movie_player,
  .html5-video-player {
    background: #000 !important;
    height: 100vh !important;
    inset: 0 !important;
    left: 0 !important;
    position: fixed !important;
    top: 0 !important;
    width: 100vw !important;
    z-index: 2147483647 !important;
  }
  #movie_player video,
  .html5-video-player video,
  .html5-video-container video {
    height: 100% !important;
    left: 0 !important;
    object-fit: contain !important;
    top: 0 !important;
    width: 100% !important;
  }
`;

/** One simulation per opened window, like the real one-shot host injection. */
const simulatedWindowIds = new Set<string>();

export interface TutorialVideoSimulationEvent {
  detail: string;
  label: string;
}

/** The iframe's own realm, so the events YouTube receives are its own types. */
type FrameGlobal = Window & typeof globalThis;

function dispatchFKey(frameWindow: FrameGlobal, frameDocument: Document): void {
  const target =
    frameDocument.querySelector<HTMLElement>("#movie_player") ??
    frameDocument.body ??
    frameDocument.documentElement;
  for (const type of ["keydown", "keyup"] as const) {
    target.dispatchEvent(
      new frameWindow.KeyboardEvent(type, {
        bubbles: true,
        cancelable: true,
        code: "KeyF",
        composed: true,
        key: "f",
        keyCode: 70,
        which: 70,
      } as KeyboardEventInit),
    );
  }
}

function applyFullscreenLayout(frameDocument: Document): boolean {
  if (frameDocument.getElementById(FULLSCREEN_STYLE_ID)) {
    return true;
  }
  const head = frameDocument.head ?? frameDocument.documentElement;
  if (!head) {
    return false;
  }
  const style = frameDocument.createElement("style");
  style.id = FULLSCREEN_STYLE_ID;
  style.textContent = FULLSCREEN_STYLE;
  head.append(style);
  return true;
}

/**
 * Runs the `f` simulation once for `windowId`, ~1.5s after the watch page
 * loaded. `onEvent` is called with the sandbox event to log.
 */
export function simulateTutorialVideoFullscreenKey(
  windowId: string,
  iframe: HTMLIFrameElement,
  onEvent: (event: TutorialVideoSimulationEvent) => void,
): void {
  if (simulatedWindowIds.has(windowId)) {
    return;
  }
  simulatedWindowIds.add(windowId);
  window.setTimeout(() => {
    const frameWindow = iframe.contentWindow as FrameGlobal | null;
    const frameDocument = iframe.contentDocument;
    if (!frameWindow || !frameDocument) {
      onEvent({
        label: "Simulated f press skipped — the video document is unreachable",
        detail:
          "The proxied watch page must stay same-origin for the sandbox to reach into it; a cross-origin navigation (consent redirect) would break this.",
      });
      return;
    }
    dispatchFKey(frameWindow, frameDocument);
    const playerFound = Boolean(frameDocument.querySelector("#movie_player"));
    const layoutApplied = applyFullscreenLayout(frameDocument);
    frameWindow.dispatchEvent(new frameWindow.Event("resize"));
    onEvent({
      label: "Simulated f press (host key injection in the real app)",
      detail: `Dispatched f keydown/keyup into the watch document ${TUTORIAL_VIDEO_FULLSCREEN_KEY_DELAY_MS}ms after load, then enforced the press OUTCOME (player fills the window) because browsers refuse fullscreen from untrusted events — the real app's trusted CEF key press does not have that restriction. Player element found: ${playerFound}; fullscreen layout applied: ${layoutApplied}.`,
    });
  }, TUTORIAL_VIDEO_FULLSCREEN_KEY_DELAY_MS);
}

/** Lets a closed/reopened window run the one-shot simulation again. */
export function forgetTutorialVideoSimulation(windowId: string): void {
  simulatedWindowIds.delete(windowId);
}
