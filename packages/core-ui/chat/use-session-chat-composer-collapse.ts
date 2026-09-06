import { useCallback, useEffect, useLayoutEffect, useRef, useState, type RefObject } from 'react';
import {
  createSessionChatComposerScrollGesture,
  recordSessionChatComposerScrollGesture,
  resetSessionChatComposerScrollGesture,
  suppressSessionChatComposerScrollGesture,
} from './session-chat-composer-scroll-gesture';

const SCROLL_GESTURE_THRESHOLD_PX = 24;
const SCROLL_GESTURE_RESET_MS = 120;
const TRANSITION_DURATION_MS = 280;
const BOTTOM_THRESHOLD_PX = 10;

/**
 * CDXC:SessionChat 2026-09-05 DECISION:
 * User: collapse the chat text box when scrolling up through the conversation and expand it again at the bottom.
 */
export function useSessionChatComposerCollapse({
  enabled,
  collapseEligible,
  transcriptRef,
  onCollapsedChange,
}: {
  enabled: boolean;
  collapseEligible: boolean;
  transcriptRef?: RefObject<HTMLDivElement | null>;
  onCollapsedChange?: (collapsed: boolean) => void;
}) {
  const composerRef = useRef<HTMLDivElement>(null);
  const [collapsed, setCollapsed] = useState(false);
  const collapsedRef = useRef(false);
  const previousHeightRef = useRef<number | null>(null);
  const animationRef = useRef<Animation | null>(null);
  const pinBottomRef = useRef(false);
  const gestureRef = useRef(createSessionChatComposerScrollGesture());
  const collapseEligibleRef = useRef(false);
  collapseEligibleRef.current = enabled && collapseEligible && !collapsed;

  const changeCollapsed = useCallback(
    (next: boolean, pinBottom = false) => {
      if (collapsedRef.current === next) return;
      previousHeightRef.current = composerRef.current?.getBoundingClientRect().height ?? null;
      collapsedRef.current = next;
      pinBottomRef.current = pinBottom;
      setCollapsed(next);
      onCollapsedChange?.(next);
    },
    [onCollapsedChange]
  );

  useEffect(() => () => onCollapsedChange?.(false), [onCollapsedChange]);

  const expand = useCallback(() => {
    suppressSessionChatComposerScrollGesture(gestureRef.current, performance.now(), SCROLL_GESTURE_RESET_MS);
    changeCollapsed(false);
  }, [changeCollapsed]);

  useEffect(() => {
    if (!enabled || !collapseEligible) changeCollapsed(false);
  }, [enabled, collapseEligible, changeCollapsed]);

  useEffect(() => {
    if (!enabled) return;
    const getViewport = () =>
      transcriptRef?.current?.querySelector<HTMLDivElement>('[data-slot="message-scroller-viewport"]');
    const atBottom = (viewport: HTMLDivElement) =>
      viewport.scrollHeight - viewport.clientHeight - viewport.scrollTop <= BOTTOM_THRESHOLD_PX;
    let lastScrollTop = getViewport()?.scrollTop ?? 0;
    let gestureTimeout: number | null = null;
    const finishGesture = () => {
      if (gestureTimeout !== null) window.clearTimeout(gestureTimeout);
      gestureTimeout = null;
      resetSessionChatComposerScrollGesture(gestureRef.current);
    };

    const restoreAtBottom = () => {
      changeCollapsed(false, true);
    };
    /*
     * CDXC:SessionChat 2026-09-05 WHY:
     * Capture the gesture before a fast flick reaches the top, including wheel events over nested transcript blocks.
     * Chromium can move the viewport on the compositor thread before delivering even a capture listener's wheel event, so direction checks use the position before that scroll was committed.
     * Reaching the bottom only restores the layout; suppressing the gesture there swallowed immediate upward reversals.
     * Only returning to the editor suppresses the remaining momentum, and the idle timer resets that suppression.
     */
    const onWheel = (event: WheelEvent) => {
      if (event.ctrlKey || !(event.target instanceof Element)) return;
      const viewport = getViewport();
      if (!viewport) return;
      const targetsTranscript = viewport.contains(event.target);
      const gesture = gestureRef.current;
      if (!targetsTranscript && !gesture.collapseSuppressed) return;
      if (gestureTimeout !== null) window.clearTimeout(gestureTimeout);
      gestureTimeout = window.setTimeout(finishGesture, SCROLL_GESTURE_RESET_MS);

      const scrollTopBeforeWheel =
        event.deltaY < 0 ? Math.max(lastScrollTop, viewport.scrollTop) : Math.min(lastScrollTop, viewport.scrollTop);
      const canScrollInGestureDirection =
        targetsTranscript &&
        (event.deltaY < 0
          ? scrollTopBeforeWheel > 0
          : scrollTopBeforeWheel < viewport.scrollHeight - viewport.clientHeight);
      const unit =
        event.deltaMode === WheelEvent.DOM_DELTA_LINE
          ? 16
          : event.deltaMode === WheelEvent.DOM_DELTA_PAGE
            ? viewport.clientHeight
            : 1;
      const scrollsTowardLogicalEnd = event.deltaY > 0 && atBottom(viewport);
      const shouldCollapse = recordSessionChatComposerScrollGesture(gesture, {
        now: performance.now(),
        deltaPx: Math.abs(event.deltaY) * unit,
        collapseThresholdPx: SCROLL_GESTURE_THRESHOLD_PX,
        collapseEligible: targetsTranscript && collapseEligibleRef.current,
        canScrollInGestureDirection,
        scrollsTowardLogicalEnd,
      });
      if (targetsTranscript && scrollsTowardLogicalEnd) {
        restoreAtBottom();
      } else if (shouldCollapse) {
        changeCollapsed(true);
      }
    };
    const onScroll = (event: Event) => {
      const viewport = getViewport();
      if (!viewport || event.target !== viewport) return;
      const scrollingDown = viewport.scrollTop > lastScrollTop;
      lastScrollTop = viewport.scrollTop;
      if (animationRef.current || !collapsedRef.current) return;
      if (scrollingDown && atBottom(viewport)) restoreAtBottom();
    };
    const onClick = (event: Event) => {
      if (
        event.target instanceof Element &&
        transcriptRef?.current?.contains(event.target) &&
        event.target.closest('[data-slot="message-scroller-button"][data-direction="end"]')
      ) {
        restoreAtBottom();
      }
    };
    document.addEventListener('wheel', onWheel, { capture: true, passive: true });
    document.addEventListener('scroll', onScroll, true);
    document.addEventListener('click', onClick);
    return () => {
      document.removeEventListener('wheel', onWheel, true);
      document.removeEventListener('scroll', onScroll, true);
      document.removeEventListener('click', onClick);
      finishGesture();
    };
  }, [enabled, transcriptRef, changeCollapsed]);

  useLayoutEffect(() => {
    const composer = composerRef.current;
    animationRef.current?.cancel();
    animationRef.current = null;
    const previousHeight = previousHeightRef.current;
    previousHeightRef.current = null;
    if (!composer || previousHeight === null) return;
    const viewport = transcriptRef?.current?.querySelector<HTMLDivElement>('[data-slot="message-scroller-viewport"]');
    const pinBottom = () => {
      if (viewport && pinBottomRef.current) viewport.scrollTop = viewport.scrollHeight;
    };
    const height = composer.getBoundingClientRect().height;
    pinBottom();
    if (Math.abs(height - previousHeight) < 1 || matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    const observer = new ResizeObserver(pinBottom);
    observer.observe(composer);
    const animation = composer.animate(
      [
        { height: `${previousHeight}px`, overflow: 'clip' },
        { height: `${height}px`, overflow: 'clip' },
      ],
      { duration: TRANSITION_DURATION_MS, easing: 'cubic-bezier(0.32, 0.72, 0, 1)' }
    );
    animationRef.current = animation;
    animation.onfinish = () => {
      animationRef.current = null;
      observer.disconnect();
      pinBottom();
    };
    return () => {
      observer.disconnect();
      animation.cancel();
      animationRef.current = null;
    };
  }, [collapsed, transcriptRef]);

  return { collapsed: enabled && collapseEligible && collapsed, composerRef, expand };
}
