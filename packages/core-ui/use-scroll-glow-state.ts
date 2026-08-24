import { useEffect, useState, type RefObject } from 'react';

export type ScrollGlowState = {
  hasOverflow: boolean;
  showBottomGlow: boolean;
  showTopGlow: boolean;
};

const SCROLL_GLOW_EPSILON_PX = 2;

function getScrollableContentHeight(element: HTMLElement): number {
  /**
   * CDXC:SidebarScroll 2026-05-08-10:53
   * Bottom-of-list scrolling must preserve the user's offset. Child
   * getBoundingClientRect() values move upward as scrollTop increases, so
   * using them made the list look non-overflowing at the bottom and reset to
   * the top. scrollHeight is stable across scroll positions.
   */
  return element.scrollHeight;
}

export function useScrollGlowState(scrollContainerRef: RefObject<HTMLElement | null>): ScrollGlowState {
  const [scrollGlowState, setScrollGlowState] = useState<ScrollGlowState>({
    hasOverflow: false,
    showBottomGlow: false,
    showTopGlow: false,
  });

  useEffect(() => {
    const element = scrollContainerRef.current;
    if (!element) {
      return;
    }

    let animationFrameId = 0;

    const updateScrollGlowState = () => {
      animationFrameId = 0;

      const contentHeight = getScrollableContentHeight(element);
      const hasOverflow = contentHeight - element.clientHeight > SCROLL_GLOW_EPSILON_PX;
      /**
       * CDXC:SidebarScroll 2026-05-05-05:29
       * Combined-mode sparse project lists must not rubber-band or preserve a
       * stale scroll offset after sessions are collapsed/closed. When the
       * measured content fits, pin the session-list viewport back to the top
       * and let CSS disable wheel scrolling for that non-overflowing state.
       */
      if (!hasOverflow && element.scrollTop !== 0) {
        element.scrollTop = 0;
      }
      /*
       * CDXC:SidebarScroll 2026-06-30-01:59:
       * The main sidebar must prioritize raw scroll throughput over edge-fade polish.
       * Keep the overflow measurement that disables wheel handling for sparse lists, but do not subscribe to scroll frames or update top/bottom glow state now that the main sidebar scroll mask is removed.
       */
      const showTopGlow = false;
      const showBottomGlow = false;

      setScrollGlowState((previous) =>
        previous.hasOverflow === hasOverflow &&
        previous.showTopGlow === showTopGlow &&
        previous.showBottomGlow === showBottomGlow
          ? previous
          : {
              hasOverflow,
              showBottomGlow,
              showTopGlow,
            }
      );
    };

    const scheduleScrollGlowUpdate = () => {
      if (animationFrameId !== 0) {
        return;
      }

      animationFrameId = window.requestAnimationFrame(updateScrollGlowState);
    };

    const resizeObserver = new ResizeObserver(() => {
      scheduleScrollGlowUpdate();
    });

    /*
     * CDXC:SidebarScroll 2026-08-20:
     * The overflow measurement depends on the scroller's *content* height, but
     * the scroller's own box is `height: 100%` and never resizes when content
     * grows, so observing only `element` measured the wrong box. That was
     * survivable while project/collection bodies snapped open, because the DOM
     * mutation that expanded them and the height change landed in the same
     * frame the MutationObserver measured. Now that those bodies animate open
     * over `--sidebar-collapse-duration` (500ms), the attribute mutation fires
     * first and the single scheduled measurement runs on the first transition
     * frame, while the body is still ~0px tall: `hasOverflow` latched false,
     * `data-scrollable-y="false"` put `overflow-y: hidden` on the sidebar, and
     * no further mutation ever arrived to re-measure, so the sidebar stayed
     * unscrollable until an unrelated click mutated the subtree again.
     *
     * Observing the content children instead makes every frame of the
     * expand/collapse transition a resize notification, so the measurement
     * tracks the real content height for the whole animation and settles on
     * its final value.
     */
    const observedContentChildren = new Set<Element>();
    const syncObservedContentChildren = () => {
      for (const child of Array.from(element.children)) {
        if (!observedContentChildren.has(child)) {
          observedContentChildren.add(child);
          resizeObserver.observe(child);
        }
      }

      for (const child of Array.from(observedContentChildren)) {
        if (child.parentElement !== element) {
          observedContentChildren.delete(child);
          resizeObserver.unobserve(child);
        }
      }
    };

    const mutationObserver = new MutationObserver(() => {
      syncObservedContentChildren();
      scheduleScrollGlowUpdate();
    });

    resizeObserver.observe(element);
    syncObservedContentChildren();
    mutationObserver.observe(element, {
      attributes: true,
      childList: true,
      characterData: true,
      subtree: true,
    });
    window.addEventListener('resize', scheduleScrollGlowUpdate);
    scheduleScrollGlowUpdate();

    return () => {
      if (animationFrameId !== 0) {
        window.cancelAnimationFrame(animationFrameId);
      }

      observedContentChildren.clear();
      resizeObserver.disconnect();
      mutationObserver.disconnect();
      window.removeEventListener('resize', scheduleScrollGlowUpdate);
    };
  }, [scrollContainerRef]);

  return scrollGlowState;
}
