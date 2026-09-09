import { useCallback, useEffect, useRef, useState, type MouseEvent } from 'react';
import { sessionChatImageTargetForHref, useSessionChatImageViewer } from './session-chat-image-viewer';
import './session-chat-reference-interactions.css';

export function sessionChatReferenceElement(target: EventTarget | null): HTMLElement | null {
  return target instanceof Element ? target.closest<HTMLElement>('[data-ghostex-reference-path]') : null;
}

/**
 * CDXC:SessionChat 2026-09-09 DECISION:
 * User: hovering an image reference outlines its thumbnail; a single click opens the image and a double click still expands the editable reference source.
 * Delay image opening until the double-click window passes so the viewer cannot intercept the second click.
 */
export function useSessionChatReferenceInteractions(draft: string) {
  const viewer = useSessionChatImageViewer();
  const [hoveredImagePath, setHoveredImagePath] = useState<string | null>(null);
  const [contextReference, setContextReference] = useState<string | null>(null);
  const openTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const cancelImageOpen = useCallback(() => {
    if (openTimer.current !== null) clearTimeout(openTimer.current);
    openTimer.current = null;
  }, []);
  useEffect(() => {
    setHoveredImagePath(null);
    return cancelImageOpen;
  }, [draft, cancelImageOpen]);

  const clickReference = (event: MouseEvent): void => {
    cancelImageOpen();
    const pill = sessionChatReferenceElement(event.target);
    const path = pill?.dataset.ghostexReferencePath;
    if (event.detail > 1 || !path || !pill.classList.contains('ghostex-chat-reference-pill--image')) return;
    openTimer.current = setTimeout(() => {
      openTimer.current = null;
      if (pill.isConnected) viewer?.open(sessionChatImageTargetForHref(path));
    }, 500);
  };
  const captureReferenceContext = (target: EventTarget | null): void => {
    cancelImageOpen();
    setContextReference(sessionChatReferenceElement(target)?.dataset.ghostexReferencePath ?? null);
  };
  const hoverReference = (target: EventTarget | null): void => {
    const pill = sessionChatReferenceElement(target);
    setHoveredImagePath(
      pill?.classList.contains('ghostex-chat-reference-pill--image')
        ? (pill.dataset.ghostexReferencePath ?? null)
        : null
    );
  };
  return {
    hoveredImagePath,
    contextReference,
    clickReference,
    captureReferenceContext,
    hoverReference,
    cancelImageOpen,
  };
}
