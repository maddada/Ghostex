import type { ReactNode } from 'react';
import { cn } from '@/packages/components/utils';
import type { SessionChatTerminalNotice } from '@/packages/shared/session-chat';

export type SessionChatNoticeSeverity = SessionChatTerminalNotice['severity'];

const SEVERITY_SHELLS: Record<SessionChatNoticeSeverity, string> = {
  error: 'border-destructive/40 bg-destructive/10',
  info: 'border-input bg-muted/20',
  // Deliberately neutral (not amber): the warning card sits directly above the
  // composer, and a yellow slab there shouted over the whole chat surface.
  warning: 'border-foreground/25 bg-muted/25',
};

/*
CDXC:Copy 2026-09-03:
User decision: Ghostex-owned user-facing copy in the desktop, web, and mobile apps uses no em dashes; use punctuation that preserves the sentence's natural reading instead.
*/
/** Shared severity-tinted shell for notices directly above the chat composer. */
export function SessionChatNoticeCard({
  children,
  className,
  kind,
  role = 'status',
  severity,
}: {
  children: ReactNode;
  className?: string;
  kind: string;
  role?: 'alert' | 'status';
  severity: SessionChatNoticeSeverity;
}) {
  /*
  Severity is a closed set in this build's type but an open one on the wire. A
  newer daemon can send a level this build has never heard of, so resolve it at
  runtime and keep the notice on the muted info surface instead of leaving it
  unstyled.
  */
  const severityShells: Record<string, string | undefined> = SEVERITY_SHELLS;
  const severityShell = severityShells[severity] ?? SEVERITY_SHELLS.info;

  return (
    <div
      className={cn('min-w-0 overflow-hidden rounded-2xl border', severityShell, className)}
      data-kind={kind}
      data-severity={severity}
      role={role}
    >
      {children}
    </div>
  );
}
