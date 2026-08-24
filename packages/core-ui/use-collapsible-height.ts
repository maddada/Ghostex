import { useCallback, useLayoutEffect, useRef, useState, type CSSProperties } from 'react';

type CollapsibleStyle = CSSProperties & {
  '--sidebar-collapse-content-height'?: string;
};

export function useCollapsibleHeight<T extends HTMLElement>() {
  const contentRef = useRef<T>(null);
  const [contentElement, setContentElementState] = useState<T | null>(null);
  const [contentHeight, setContentHeight] = useState<number>();

  const setContentElement = useCallback((element: T | null) => {
    /*
     * CDXC:SidebarPerformance 2026-06-28-08:28:
     * Collapsed project bodies are now unmounted so hidden session rows do not
     * keep row observers and dnd hooks alive. Re-measure when the body ref
     * appears again, otherwise expanding a previously collapsed project can
     * keep the old undefined/zero collapse height and hide its sessions.
     */
    contentRef.current = element;
    setContentElementState(element);
  }, []);

  useLayoutEffect(() => {
    const element = contentElement;
    if (!element) {
      setContentHeight(undefined);
      return;
    }

    let animationFrameId = 0;

    const updateHeight = () => {
      const renderedHeight = Math.ceil(element.getBoundingClientRect().height);
      setContentHeight(Math.max(element.scrollHeight, renderedHeight));
    };

    const scheduleUpdate = () => {
      window.cancelAnimationFrame(animationFrameId);
      animationFrameId = window.requestAnimationFrame(updateHeight);
    };

    updateHeight();
    const observer = new ResizeObserver(() => {
      scheduleUpdate();
    });
    observer.observe(element);

    return () => {
      observer.disconnect();
      window.cancelAnimationFrame(animationFrameId);
    };
  }, [contentElement]);

  const collapsibleStyle: CollapsibleStyle | undefined =
    contentHeight === undefined
      ? undefined
      : ({
          '--sidebar-collapse-content-height': `${contentHeight}px`,
        } as CollapsibleStyle);

  return {
    collapsibleStyle,
    contentRef,
    setContentElement,
  };
}
