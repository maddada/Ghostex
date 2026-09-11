import { useCallback, useEffect, useRef, type RefObject } from 'react';

const SCROLL_GESTURE_IDLE_MS = 160;

/** CDXC:SessionChat 2026-09-11 DECISION:
 * User: clicking Scroll to bottom or pressing its hotkey cancels upward momentum so chat settles at the bottom.
 * Wheel events keep arriving after a trackpad flick, even after an instant scroll. Consume the remainder of that gesture before the scroller and composer see it; a quiet interval or fresh direct input returns control to the reader.
 */
export function useSessionChatScrollMomentum(viewportRef: RefObject<HTMLDivElement | null>) {
  const settlingRef = useRef(false);
  const idleTimeoutRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  const release = useCallback(() => {
    settlingRef.current = false;
    clearTimeout(idleTimeoutRef.current);
    idleTimeoutRef.current = undefined;
  }, []);

  const waitForGestureEnd = useCallback(() => {
    clearTimeout(idleTimeoutRef.current);
    idleTimeoutRef.current = setTimeout(release, SCROLL_GESTURE_IDLE_MS);
  }, [release]);

  const settleAtBottom = useCallback(() => {
    const viewport = viewportRef.current;
    if (viewport) viewport.scrollTo({ top: viewport.scrollHeight, behavior: 'instant' });
  }, [viewportRef]);

  const cancelMomentum = useCallback(() => {
    settlingRef.current = true;
    waitForGestureEnd();
    // Always issue an instant scroll, even at the bottom, to abort an in-flight smooth scroll.
    settleAtBottom();
  }, [settleAtBottom, waitForGestureEnd]);

  useEffect(() => {
    const targetsViewport = (event: Event) =>
      event.target instanceof Node && viewportRef.current?.contains(event.target);
    const blockRemainingGesture = (event: WheelEvent | TouchEvent) => {
      if (!settlingRef.current || !targetsViewport(event)) return;
      if (event instanceof WheelEvent && event.ctrlKey) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      waitForGestureEnd();
      settleAtBottom();
    };
    const onScroll = (event: Event) => {
      const viewport = viewportRef.current;
      if (!settlingRef.current || !viewport || event.target !== viewport) return;
      if (viewport.scrollHeight - viewport.clientHeight - viewport.scrollTop > 1) {
        // Cover a compositor scroll already queued before the command reached JavaScript.
        event.stopImmediatePropagation();
        waitForGestureEnd();
        settleAtBottom();
      }
    };
    const onDirectInput = (event: Event) => {
      if (
        event instanceof KeyboardEvent &&
        (event.repeat ||
          event.altKey ||
          event.ctrlKey ||
          event.metaKey ||
          !['ArrowUp', 'ArrowDown', 'PageUp', 'PageDown', 'Home', 'End', ' '].includes(event.key))
      )
        return;
      if (targetsViewport(event)) release();
    };
    // Window capture runs before the composer's document listener and React's scroll-intent handlers.
    window.addEventListener('wheel', blockRemainingGesture, { capture: true, passive: false });
    window.addEventListener('touchmove', blockRemainingGesture, { capture: true, passive: false });
    window.addEventListener('scroll', onScroll, true);
    window.addEventListener('pointerdown', onDirectInput, true);
    window.addEventListener('touchstart', onDirectInput, true);
    window.addEventListener('keydown', onDirectInput, true);
    return () => {
      window.removeEventListener('wheel', blockRemainingGesture, true);
      window.removeEventListener('touchmove', blockRemainingGesture, true);
      window.removeEventListener('scroll', onScroll, true);
      window.removeEventListener('pointerdown', onDirectInput, true);
      window.removeEventListener('touchstart', onDirectInput, true);
      window.removeEventListener('keydown', onDirectInput, true);
      release();
    };
  }, [release, settleAtBottom, viewportRef, waitForGestureEnd]);

  return cancelMomentum;
}
