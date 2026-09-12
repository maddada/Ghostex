import { useLayoutEffect, useRef, type RefObject } from 'react';
import type { SessionChatScrollSnapshot } from './session-chat-interaction-state';
import { FOLLOW_BOTTOM_ATTRIBUTE, STREAM_HOLD_ATTRIBUTE } from './session-chat-scroll-bottom-button';
import { isSessionChatPendingMessageId } from './session-chat-pending';
import { SESSION_CHAT_TERMINAL_TOOL_ID_PREFIX } from './session-chat-terminal-status';
import { SESSION_CHAT_STREAMING_ID } from './session-chat-streaming';

export const SESSION_CHAT_HISTORY_NAVIGATION_EVENT = 'ghostex-session-chat-history-navigation';

export interface SessionChatScrollRestorationControl {
  finished: boolean;
  blocked: boolean;
}

/** CDXC:SessionChat 2026-09-12 WHY:
 * A reused renderer can have the saved reading anchor without its older transcript page.
 * Recover pages in cursor order while following is held; an unchanged cursor after a settled request stops automatic retries, including failures during a live stream.
 */
export function useSessionChatScrollRestoration({
  snapshot,
  viewportRef,
  contentRef,
  restoringRef,
  controlRef,
  earlierPageCursor,
  oldestMessageId,
  messagesRevision,
  hasMore,
  loadingEarlier,
  onLoadEarlier,
  setViewportScrollTop,
}: {
  snapshot: SessionChatScrollSnapshot | undefined;
  viewportRef: RefObject<HTMLDivElement | null>;
  contentRef: RefObject<HTMLDivElement | null>;
  restoringRef: RefObject<boolean>;
  controlRef: RefObject<SessionChatScrollRestorationControl>;
  earlierPageCursor: number | undefined;
  oldestMessageId: string | undefined;
  messagesRevision: unknown;
  hasMore: boolean;
  loadingEarlier: boolean;
  onLoadEarlier: () => void;
  setViewportScrollTop: (top: number) => void;
}) {
  const requestedPageRef = useRef<number | string | null>(null);

  useLayoutEffect(() => {
    if (!snapshot || controlRef.current.finished) return;
    const viewport = viewportRef.current;
    const content = contentRef.current;
    if (!viewport || !content) return;
    const anchor = snapshot.anchorId
      ? (Array.from(content.children).find(
          (child) => child instanceof HTMLElement && child.dataset.messageId === snapshot.anchorId
        ) as HTMLElement | undefined)
      : undefined;
    const followsEnd = snapshot.followBottom && !snapshot.streamOnScreen;
    const historyAnchor =
      snapshot.anchorId &&
      snapshot.anchorId !== SESSION_CHAT_STREAMING_ID &&
      !snapshot.anchorId.startsWith(SESSION_CHAT_TERMINAL_TOOL_ID_PREFIX) &&
      !isSessionChatPendingMessageId(snapshot.anchorId);
    if (!followsEnd && !anchor && historyAnchor && hasMore) {
      viewport.setAttribute(FOLLOW_BOTTOM_ATTRIBUTE, 'false');
      if (loadingEarlier) {
        restoringRef.current = true;
        return;
      }
      const page = earlierPageCursor ?? oldestMessageId ?? '';
      if (requestedPageRef.current === page) {
        controlRef.current.blocked = true;
        restoringRef.current = false;
        return;
      }
      requestedPageRef.current = page;
      controlRef.current.blocked = false;
      restoringRef.current = true;
      onLoadEarlier();
      return;
    }

    // A remounted offscreen row starts with the scroller's 10rem intrinsic
    // estimate. Materialize the saved row before applying an offset inside a
    // long reply, otherwise that offset can skip past the estimated row.
    const measuredAnchor = followsEnd ? undefined : anchor;
    const previousVisibility = measuredAnchor?.style.getPropertyValue('content-visibility') ?? '';
    const previousPriority = measuredAnchor?.style.getPropertyPriority('content-visibility') ?? '';
    measuredAnchor?.style.setProperty('content-visibility', 'visible', previousPriority);
    const restoreVisibility = () => {
      if (!measuredAnchor || measuredAnchor.style.getPropertyValue('content-visibility') !== 'visible') return;
      if (previousVisibility)
        measuredAnchor.style.setProperty('content-visibility', previousVisibility, previousPriority);
      else measuredAnchor.style.removeProperty('content-visibility');
    };
    const applyPosition = () => {
      const anchorRect = measuredAnchor?.getBoundingClientRect();
      const top = followsEnd
        ? viewport.scrollHeight
        : anchorRect
          ? anchorRect.top - viewport.getBoundingClientRect().top + viewport.scrollTop - snapshot.anchorOffset
          : snapshot.top;
      setViewportScrollTop(top);
      viewport.setAttribute(FOLLOW_BOTTOM_ATTRIBUTE, snapshot.followBottom ? 'true' : 'false');
      if (snapshot.streamOnScreen && !snapshot.streamHoldReleased) viewport.setAttribute(STREAM_HOLD_ATTRIBUTE, 'true');
    };
    restoringRef.current = true;
    applyPosition();
    // Mount-time layout can briefly clamp scrollTop to zero before the
    // transcript and composer finish measuring. Keep those scroll events from
    // replacing the saved follow intent, then reapply the real anchor.
    let frame = requestAnimationFrame(() => {
      if (controlRef.current.finished) {
        restoreVisibility();
        return;
      }
      applyPosition();
      frame = requestAnimationFrame(() => {
        if (!controlRef.current.finished) {
          applyPosition();
          controlRef.current.finished = true;
          controlRef.current.blocked = false;
          restoringRef.current = false;
        }
        restoreVisibility();
      });
    });
    return () => {
      cancelAnimationFrame(frame);
      restoreVisibility();
    };
  }, [
    contentRef,
    controlRef,
    earlierPageCursor,
    hasMore,
    loadingEarlier,
    messagesRevision,
    oldestMessageId,
    onLoadEarlier,
    restoringRef,
    setViewportScrollTop,
    snapshot,
    viewportRef,
  ]);
}
