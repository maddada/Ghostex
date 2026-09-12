import { useCallback, useLayoutEffect, useRef, useState } from 'react';

/** Live height of the composer overlay, for chrome that hugs the box (scroll-to-end button, minimap, welcome state). */
export const SESSION_CHAT_COMPOSER_OVERLAY_VAR = '--ghostex-chat-composer-overlay';
/** Space the transcript keeps clear above its end; held at the expanded height while the composer is collapsed. */
export const SESSION_CHAT_COMPOSER_INSET_VAR = '--ghostex-chat-composer-inset';

/**
 * CDXC:SessionChat 2026-09-08 WHY:
 * The composer overlays the transcript instead of sharing its flex column, so collapsing and expanding never resize the scroll viewport.
 * While the box sat in flow, every collapse grew the viewport and every expansion shrank it; momentum wheel events landing mid-animation saw "not at bottom", collapsed the box again, and it bounced open and shut.
 * The transcript keeps its end clear with a bottom inset that tracks the overlay while expanded and holds the expanded height while collapsed, so the resting box never uncovers rows its expansion will cover again.
 * SEE-ALSO: use-session-chat-composer-transition.ts pins the overlay at its destination height for the tween, so the resize observer here sees one change per transition, not one per frame.
 * The observer is the only reader: measuring the overlay in a layout effect read a stale pin during quick reversals and let the inset drop mid-transition.
 */
export function useSessionChatComposerInset(collapsed: boolean) {
  const [host, setHost] = useState<HTMLDivElement | null>(null);
  const [overlay, setOverlay] = useState<HTMLDivElement | null>(null);
  const collapsedRef = useRef(collapsed);
  const insetRef = useRef(0);
  const overlayHeightRef = useRef(0);
  collapsedRef.current = collapsed;

  const publish = useCallback(
    (height: number) => {
      if (!host) return;
      const next = Math.ceil(height);
      if (next <= 0) return;
      if (overlayHeightRef.current !== next) {
        overlayHeightRef.current = next;
        host.style.setProperty(SESSION_CHAT_COMPOSER_OVERLAY_VAR, `${next}px`);
      }
      const inset = collapsedRef.current ? Math.max(insetRef.current, next) : next;
      if (insetRef.current !== inset) {
        insetRef.current = inset;
        host.style.setProperty(SESSION_CHAT_COMPOSER_INSET_VAR, `${inset}px`);
      }
    },
    [host]
  );

  useLayoutEffect(() => {
    if (!host || !overlay) return;
    const update = () => {
      host.style.setProperty('--ghostex-chat-pane-height', `${host.getBoundingClientRect().height}px`);
      publish(overlay.getBoundingClientRect().height);
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(overlay);
    observer.observe(host);
    return () => {
      observer.disconnect();
      host.style.removeProperty(SESSION_CHAT_COMPOSER_OVERLAY_VAR);
      host.style.removeProperty(SESSION_CHAT_COMPOSER_INSET_VAR);
      host.style.removeProperty('--ghostex-chat-pane-height');
      overlayHeightRef.current = 0;
      insetRef.current = 0;
    };
  }, [host, overlay, publish]);

  return { hostRef: setHost, overlayRef: setOverlay };
}
