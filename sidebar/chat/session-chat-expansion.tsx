import type { ReactNode } from "react";
import { cn } from "../../lib/utils";

/** Center a transcript row after React has committed its expanded content. */
export function centerSessionChatExpansion(target: HTMLElement | null): void {
  if (!target) {
    return;
  }
  window.requestAnimationFrame(() => {
    // MessageScroller reconciles resized content on its own animation frame.
    // Center on the following frame so its bottom-follow correction cannot
    // overwrite this explicit user navigation.
    window.requestAnimationFrame(() => {
      if (target.isConnected) {
        target.scrollIntoView({
          behavior: "smooth",
          block: "center",
          inline: "nearest",
        });
      }
    });
  });
}

export function SessionChatExpansion({
  bodyClassName,
  children,
  className,
  label,
  onCollapse,
}: {
  bodyClassName?: string;
  children: ReactNode;
  className?: string;
  label: string;
  onCollapse: () => void;
}) {
  return (
    <div className={cn("ghostex-chat-expansion", className)}>
      <button
        aria-label={label}
        className="ghostex-chat-expansion-rail"
        onClick={onCollapse}
        type="button"
      />
      <div className={cn("ghostex-chat-expansion-body", bodyClassName)}>
        {children}
      </div>
    </div>
  );
}
