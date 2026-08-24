import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from 'react';

const SidebarCollapseAnimationDurationContext = createContext(0);

export function SidebarCollapseAnimationProvider({
  children,
  durationMs,
}: {
  children: ReactNode;
  durationMs: number;
}) {
  return (
    <SidebarCollapseAnimationDurationContext.Provider value={durationMs}>
      {children}
    </SidebarCollapseAnimationDurationContext.Provider>
  );
}

export function useSidebarCollapseAnimationDuration(): number {
  return useContext(SidebarCollapseAnimationDurationContext);
}

/**
 * Keep a collapsible body mounted until its closing transition finishes, and
 * mount it in the collapsed state for one frame before opening. This gives CSS
 * a real start and end height while still unmounting expensive session rows
 * once they are hidden.
 */
export function useSidebarCollapsiblePresence(collapsed: boolean, durationOverrideMs?: number) {
  const contextDurationMs = useSidebarCollapseAnimationDuration();
  const durationMs = durationOverrideMs ?? contextDurationMs;
  const [isPresent, setIsPresent] = useState(!collapsed);
  const [isVisuallyCollapsed, setIsVisuallyCollapsed] = useState(collapsed);
  const collapsibleElementRef = useRef<HTMLElement | null>(null);
  const openFrameRef = useRef<number | undefined>(undefined);
  const previousCollapsedRef = useRef(collapsed);
  const setCollapsibleElement = useCallback((element: HTMLElement | null) => {
    collapsibleElementRef.current = element;
  }, []);

  useLayoutEffect(() => {
    window.cancelAnimationFrame(openFrameRef.current ?? 0);

    if (previousCollapsedRef.current === collapsed) {
      if (durationMs === 0) {
        setIsPresent(!collapsed);
        setIsVisuallyCollapsed(collapsed);
      }
      return;
    }
    previousCollapsedRef.current = collapsed;

    if (durationMs === 0) {
      setIsPresent(!collapsed);
      setIsVisuallyCollapsed(collapsed);
      return;
    }

    if (collapsed) {
      setIsVisuallyCollapsed(true);
      return;
    }

    setIsPresent(true);
    setIsVisuallyCollapsed(true);
    openFrameRef.current = window.requestAnimationFrame(() => {
      openFrameRef.current = window.requestAnimationFrame(() => {
        setIsVisuallyCollapsed(false);
      });
    });

    return () => window.cancelAnimationFrame(openFrameRef.current ?? 0);
  }, [collapsed, durationMs]);

  useEffect(() => {
    if (!collapsed || !isPresent || !isVisuallyCollapsed || durationMs === 0) {
      return;
    }

    const element = collapsibleElementRef.current;
    if (!element) {
      return;
    }

    /*
     * Wait for the transitions the browser actually created instead of
     * guessing their finish time with a matching JS timer. A timer starts
     * before the closing styles necessarily reach the compositor, which can
     * remove the last few pixels of the body one frame early and visibly snap
     * every row below it. `getAnimations()` also resolves immediately when
     * reduced motion means no transition was created.
     */
    const closingAnimations = element.getAnimations();
    if (closingAnimations.length === 0) {
      setIsPresent(false);
      return;
    }

    let cancelled = false;
    void Promise.allSettled(closingAnimations.map((animation) => animation.finished)).then(() => {
      if (!cancelled) {
        setIsPresent(false);
      }
    });

    return () => {
      cancelled = true;
    };
  }, [collapsed, durationMs, isPresent, isVisuallyCollapsed]);

  return {
    isPresent,
    isVisuallyCollapsed,
    setCollapsibleElement,
  };
}
