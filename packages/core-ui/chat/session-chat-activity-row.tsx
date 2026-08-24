/*
CDXC:SessionChatTerminalActivity 2026-08-22:
The transcript's progress row: what the agent CLI is doing right now when it is
doing something long, on-screen only, and consequential. Claude Code's
compaction is the case this exists for — for a minute or more the chat could say
nothing better than "the agent is working", while the operation quietly running
was the one that REPLACES the conversation the user is reading.

It stands where the typing indicator stands, and replaces it: two live
indicators for one piece of work would just compete.

The elapsed clock ticks LOCALLY from `detectedAt`, which the server holds still
for the whole run. The bar only moves when a probe brings a new percentage
(every few seconds), so a clock that also only moved then would read as a frozen
UI; interpolating between samples costs one interval and is the difference
between "working" and "stuck". Nothing here estimates the PERCENTAGE — that
comes off the screen or is not drawn at all.
*/

import { useEffect, useState } from 'react';
import type { SessionChatTerminalActivity } from '../../shared/session-chat';
import { cn } from '@/packages/components/utils';

/** How often the local clock re-renders between server samples. */
const ACTIVITY_CLOCK_TICK_MS = 1_000;

export function formatSessionChatActivityElapsed(totalSeconds: number): string {
  const seconds = Math.max(0, Math.floor(totalSeconds));
  const hours = Math.floor(seconds / 3_600);
  const minutes = Math.floor((seconds % 3_600) / 60);
  const rest = seconds % 60;
  if (hours > 0) {
    return `${hours}h ${minutes}m ${rest}s`;
  }
  if (minutes > 0) {
    return `${minutes}m ${rest}s`;
  }
  return `${rest}s`;
}

/**
 * Seconds to show now: what the CLI last reported, plus the time since that
 * sample was taken. `detectedAt` anchors the whole run, so this keeps counting
 * smoothly across probes instead of snapping backwards on each one.
 *
 * Takes the two fields rather than the activity so the background-agent strip
 * can share it: its clocks anchor on the FLEET's `detectedAt` while the seconds
 * come off each row.
 */
export function sessionChatActivityElapsedSeconds(
  activity: { elapsedSeconds?: number; detectedAt: string },
  now: number
): number | null {
  if (activity.elapsedSeconds === undefined) {
    return null;
  }
  const anchor = Date.parse(activity.detectedAt);
  if (Number.isNaN(anchor)) {
    return activity.elapsedSeconds;
  }
  return activity.elapsedSeconds + Math.max(0, (now - anchor) / 1_000);
}

export interface SessionChatActivityRowProps {
  activity: SessionChatTerminalActivity;
}

export function SessionChatActivityRow({ activity }: SessionChatActivityRowProps) {
  const [now, setNow] = useState(() => Date.now());

  // Only run a timer when there is a clock to advance.
  const hasClock = activity.elapsedSeconds !== undefined;
  useEffect(() => {
    if (!hasClock) {
      return;
    }
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), ACTIVITY_CLOCK_TICK_MS);
    return () => clearInterval(timer);
  }, [activity.detectedAt, hasClock]);

  const elapsed = sessionChatActivityElapsedSeconds(activity, now);
  const percent = activity.percent === undefined ? null : Math.min(100, Math.max(0, Math.round(activity.percent)));

  return (
    <div
      aria-live='polite'
      className='ghostex-chat-activity-row my-2 grid gap-2 rounded-2xl border border-border/65 bg-muted/20 px-4 py-3'
      data-kind={activity.kind}
      role='status'
    >
      <div className='flex min-w-0 items-center gap-2'>
        <span aria-hidden='true' className='size-1.5 shrink-0 animate-pulse rounded-full bg-primary' />
        <span className='min-w-0 flex-1 truncate text-sm text-foreground/90'>{activity.label}</span>
        {elapsed !== null ? (
          <span className='shrink-0 text-xs text-muted-foreground tabular-nums'>
            {formatSessionChatActivityElapsed(elapsed)}
          </span>
        ) : null}
        {percent !== null ? (
          <span className='shrink-0 text-xs font-medium text-foreground/80 tabular-nums'>{percent}%</span>
        ) : null}
      </div>
      {percent !== null ? (
        <div
          aria-valuemax={100}
          aria-valuemin={0}
          aria-valuenow={percent}
          className='h-1 min-w-0 overflow-hidden rounded-full bg-foreground/10'
          role='progressbar'
        >
          <div
            className={cn('h-full rounded-full bg-primary transition-[width] duration-500')}
            style={{ width: `${percent}%` }}
          />
        </div>
      ) : null}
    </div>
  );
}
