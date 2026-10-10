import {
  IconCircleCheck,
  IconCircleDot,
  IconCircleX,
  IconGitMerge,
  IconGitPullRequest,
  IconGitPullRequestClosed,
  IconGitPullRequestDraft,
  IconLoader2,
} from "@tabler/icons-react";
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { initials, personHue } from "./format";
import type {
  WorkChecksState,
  WorkItem,
  WorkItemPullRequest,
  WorkStatusGroup,
} from "./types";

/**
 * The page's small building blocks, in the shadcn shapes the app's native views use (Button,
 * Toggle, Card, DropdownMenu). The shadcn primitives in packages/components/ui were deleted on
 * 2026-10-01 with the React modal host, so these are drawn from work.css here.
 */

export function cx(...names: (string | false | null | undefined)[]): string {
  return names.filter(Boolean).join(" ");
}

export function Button({
  children,
  variant = "outline",
  size = "md",
  className,
  disabled,
  title,
  onClick,
}: {
  children: ReactNode;
  variant?: "outline" | "primary" | "ghost";
  size?: "sm" | "md" | "icon";
  className?: string;
  disabled?: boolean;
  title?: string;
  onClick?: (event: React.MouseEvent<HTMLButtonElement>) => void;
}) {
  return (
    <button
      type="button"
      className={cx("w-btn", `w-btn--${variant}`, `w-btn--${size}`, className)}
      disabled={disabled}
      title={title}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

export function Toggle({
  pressed,
  children,
  className,
  onPressedChange,
}: {
  pressed: boolean;
  children: ReactNode;
  className?: string;
  onPressedChange: (pressed: boolean) => void;
}) {
  return (
    <button
      type="button"
      aria-pressed={pressed}
      className={cx("w-toggle", pressed && "is-on", className)}
      onClick={() => onPressedChange(!pressed)}
    >
      {children}
    </button>
  );
}

export function Card({
  icon,
  title,
  sub,
  action,
  children,
  className,
}: {
  icon?: ReactNode;
  title: ReactNode;
  sub?: ReactNode;
  action?: ReactNode;
  children?: ReactNode;
  className?: string;
}) {
  return (
    <section className={cx("w-card", className)}>
      <header className="w-card-head">
        {icon}
        <span className="w-card-title">{title}</span>
        {sub ? <span className="w-card-sub">{sub}</span> : null}
        <span className="w-spacer" />
        {action}
      </header>
      {children}
    </section>
  );
}

export interface MenuOption<T extends string> {
  value: T;
  label: ReactNode;
  hint?: ReactNode;
}

/** A dropdown that closes on an outside click or Escape. */
export function Dropdown({
  trigger,
  children,
  align = "start",
  className,
}: {
  trigger: (open: boolean, toggle: () => void) => ReactNode;
  children: (close: () => void) => ReactNode;
  align?: "start" | "end";
  className?: string;
}) {
  const [open, setOpen] = useState(false);
  const [shift, setShift] = useState(0);
  const root = useRef<HTMLDivElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  // A menu wider than the room right of its trigger (the last filter on a narrow panel)
  // slides left until it fits inside the page, keeping an 8px gutter.
  useLayoutEffect(() => {
    if (!open || !menu.current) {
      setShift(0);
      return;
    }
    const rect = menu.current.getBoundingClientRect();
    const overflow = rect.right - (document.documentElement.clientWidth - 8);
    setShift(overflow > 0 ? Math.min(overflow, Math.max(0, rect.left - 8)) : 0);
  }, [open]);
  useEffect(() => {
    if (!open) return;
    const onPointer = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      setOpen(false);
      // Back to the trigger, so the keyboard carries on from where the menu opened.
      root.current?.querySelector<HTMLButtonElement>("button")?.focus();
    };
    window.addEventListener("pointerdown", onPointer);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointerdown", onPointer);
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);
  return (
    <div className={cx("w-dropdown", className)} ref={root}>
      {trigger(open, () => setOpen((value) => !value))}
      {open ? (
        <div
          ref={menu}
          className={cx("w-menu", align === "end" && "w-menu--end")}
          style={shift ? { transform: `translateX(-${shift}px)` } : undefined}
          role="menu"
        >
          {children(() => setOpen(false))}
        </div>
      ) : null}
    </div>
  );
}

export function MenuItem({
  checked,
  children,
  onSelect,
}: {
  checked?: boolean;
  children: ReactNode;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      role="menuitemradio"
      aria-checked={checked}
      className="w-menu-item"
      onClick={onSelect}
    >
      <span className="w-menu-check">{checked ? "✓" : ""}</span>
      {children}
    </button>
  );
}

export function Avatar({
  name,
  url,
  size = 16,
}: {
  name?: string | null;
  url?: string | null;
  size?: number;
}) {
  if (url)
    return (
      <img className="w-avatar" src={url} alt="" width={size} height={size} />
    );
  return (
    <span
      className="w-avatar"
      style={{
        width: size,
        height: size,
        fontSize: size * 0.48,
        background: `hsl(${personHue(name)} 45% 38%)`,
      }}
      aria-hidden
    >
      {initials(name)}
    </span>
  );
}

/** Linear's workflow ring for a ticket, and a GitHub glyph for an issue or a PR. */
export function StatusGlyph({
  item,
}: {
  item: Pick<WorkItem, "kind" | "status" | "pullRequest">;
}) {
  if (item.kind === "pullRequest")
    return <PullRequestIcon state={item.pullRequest?.state ?? "open"} />;
  if (item.kind === "githubIssue") {
    return (
      <IconCircleDot
        size={15}
        className={item.status.group === "closed" ? "c-merged" : "c-open"}
      />
    );
  }
  return <LinearGlyph group={item.status.group} />;
}

export function LinearGlyph({ group }: { group: WorkStatusGroup }) {
  return <span className={cx("w-glyph", `is-${group}`)} aria-label={group} />;
}

export function PullRequestIcon({
  state,
  size = 15,
}: {
  state: WorkItemPullRequest["state"];
  size?: number;
}) {
  switch (state) {
    case "draft":
      return <IconGitPullRequestDraft size={size} className="c-draft" />;
    case "merged":
      return <IconGitMerge size={size} className="c-merged" />;
    case "closed":
      return <IconGitPullRequestClosed size={size} className="c-closed" />;
    default:
      return <IconGitPullRequest size={size} className="c-open" />;
  }
}

export function ChecksIcon({
  checks,
  size = 13,
}: {
  checks?: WorkChecksState;
  size?: number;
}) {
  if (checks === "passing")
    return (
      <IconCircleCheck
        size={size}
        className="c-open"
        aria-label="checks passing"
      />
    );
  if (checks === "failing")
    return (
      <IconCircleX
        size={size}
        className="c-closed"
        aria-label="checks failing"
      />
    );
  if (checks === "pending")
    return (
      <IconLoader2
        size={size}
        className="c-pending"
        aria-label="checks running"
      />
    );
  return null;
}

/** `#6538` with its state and checks, as on a session card. `title: null` leaves the tooltip to a wrapping link. */
export function PullRequestChip({
  pullRequest,
  title,
}: {
  pullRequest: WorkItemPullRequest;
  title?: string | null;
}) {
  return (
    <span
      className="w-chip"
      title={
        title === null
          ? undefined
          : (title ?? `PR #${pullRequest.number} · ${pullRequest.state}`)
      }
    >
      <PullRequestIcon state={pullRequest.state} size={13} />#
      {pullRequest.number}
      <ChecksIcon checks={pullRequest.checks} />
    </span>
  );
}

export function Spinner({ size = 14 }: { size?: number }) {
  return <IconLoader2 size={size} className="w-spin" aria-label="Loading" />;
}

export function LiveDot({ title }: { title?: string }) {
  return <span className="w-live-dot" title={title} />;
}
