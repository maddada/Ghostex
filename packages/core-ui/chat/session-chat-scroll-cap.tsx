// Height cap for a long reasoning block.
//
// A 200-line reasoning dump otherwise owns the whole scroll view: the answer it
// belongs to becomes unreachable without a long drag, and the transcript's
// scrollbar stops meaning anything. Capping the block and letting it scroll in
// place keeps that scrollbar proportional to the number of turns rather than to
// the longest thought.
//
// This is for SECONDARY content only. Answers are never capped: an answer is
// the thing being read, and hiding its tail behind an inner scrollbar costs
// more than a long turn does.
//
// The cap is never silent. macOS renders overlay scrollbars, so a capped block
// is indistinguishable from one that simply ended — the toggle is the signal,
// and it also releases the cap so the whole thought can be read, selected, and
// copied in one pass. It only renders once the content actually overflows.

import { IconChevronDown } from "@tabler/icons-react";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { cn } from "@/packages/components/utils";

export function SessionChatScrollCap({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  const capRef = useRef<HTMLDivElement>(null);
  const [overflowing, setOverflowing] = useState(false);
  const [expanded, setExpanded] = useState(false);

  const measure = useCallback(() => {
    const cap = capRef.current;
    // Expanded, the element is as tall as its content, so scrollHeight always
    // matches clientHeight. Keep the last collapsed verdict instead: it is
    // what decides whether the toggle stays reachable.
    if (!cap || expanded) {
      return;
    }
    setOverflowing(cap.scrollHeight - cap.clientHeight > 1);
  }, [expanded]);

  useEffect(() => {
    const cap = capRef.current;
    if (!cap) {
      return;
    }
    measure();
    // The cap's own box is pinned by max-height and never resizes, so watch the
    // content instead — that is what grows while a reply streams in.
    const observer = new ResizeObserver(measure);
    for (const child of Array.from(cap.children)) {
      observer.observe(child);
    }
    return () => observer.disconnect();
  }, [children, measure]);

  return (
    <div className={cn("ghostex-chat-scroll-cap-wrap", className)}>
      <div
        className={cn("ghostex-chat-scroll-cap", !expanded && "scroll-mask-y")}
        data-expanded={expanded ? "true" : undefined}
        ref={capRef}
      >
        {children}
      </div>
      {overflowing ? (
        <button
          className="ghostex-chat-scroll-cap-toggle"
          onClick={() => setExpanded((value) => !value)}
          type="button"
        >
          <IconChevronDown
            aria-hidden="true"
            className={cn(
              "size-3.5 shrink-0 transition-transform duration-150",
              expanded && "rotate-180",
            )}
            stroke={2}
          />
          {expanded ? "Show less" : "Show more"}
        </button>
      ) : null}
    </div>
  );
}
