/*
CDXC:SessionChat 2026-09-06 DECISION:
User: a Codex goal shows as a card in the same shape as the Claude Code status cards (the activity row and the pending tool row): dot on the left, chevron on the right, no outline of its own.
Collapsed it shows the first three lines of the goal's text; expanding shows the full goal text and nothing else.
*/

import { useLayoutEffect, useRef, useState } from 'react';
import type { KeyboardEvent } from 'react';
import { IconChevronRight } from '@tabler/icons-react';
import { cn } from '@/packages/components/utils';

const STATUS_TONES: Record<string, string> = {
  active: 'bg-primary/15 text-primary',
  complete: 'bg-foreground/10 text-foreground/80',
  paused: 'bg-muted text-muted-foreground',
  cleared: 'bg-muted text-muted-foreground',
  stalled: 'bg-destructive/10 text-destructive',
  'usage limited': 'bg-destructive/10 text-destructive',
  'limited by budget': 'bg-destructive/10 text-destructive',
};

export interface SessionChatGoalCardProps {
  status: string;
  objective: string;
  usage?: string;
}

export function SessionChatGoalCard({ objective, status, usage }: SessionChatGoalCardProps) {
  const [expanded, setExpanded] = useState(false);
  const [overflows, setOverflows] = useState(false);
  const objectiveRef = useRef<HTMLParagraphElement>(null);
  const text = objective.trim();
  const active = status === 'active';

  // The chevron only appears when the clamp actually hides text.
  useLayoutEffect(() => {
    const element = objectiveRef.current;
    if (!element || expanded) {
      return;
    }
    const measure = (): void => setOverflows(element.scrollHeight > element.clientHeight + 1);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, [expanded, text]);

  const expandable = expanded || overflows;
  const toggle = (): void => {
    if (expandable) {
      setExpanded((value) => !value);
    }
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>): void => {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      toggle();
    }
  };

  return (
    <div
      aria-live='polite'
      className='ghostex-chat-activity-row ghostex-chat-status-card grid gap-2 rounded-2xl border border-border/65 bg-muted/20 px-4 py-3'
      data-kind='codex-goal'
      data-status={status}
      role='status'
    >
      {/* Not a <button>: the chat scope's button rules would give the header
          their pill outline and hover fill, and it is a disclosure on a card
          that already has its own border. */}
      <div
        aria-expanded={expandable ? expanded : undefined}
        className={cn(
          'flex min-w-0 items-start gap-2 text-left leading-relaxed outline-none',
          expandable && 'cursor-pointer'
        )}
        onClick={toggle}
        onKeyDown={onKeyDown}
        role='button'
        tabIndex={expandable ? 0 : -1}
      >
        {/* One-line-tall (1lh) boxes center the dot and chevron on the first
            text line for any inherited font size; see session-chat-terminal-tool-row.tsx. */}
        <span aria-hidden='true' className='flex h-[1lh] shrink-0 items-center'>
          <span
            className={cn('size-1.5 rounded-full', active ? 'animate-pulse bg-primary' : 'bg-muted-foreground/60')}
          />
        </span>
        <div className='min-w-0 flex-1'>
          <div className='flex min-w-0 items-center gap-2'>
            <span className='font-medium text-foreground'>Goal</span>
            <span
              className={cn(
                'rounded-full px-1.5 py-px text-[11px] leading-4 font-medium',
                STATUS_TONES[status] ?? 'bg-muted text-muted-foreground'
              )}
            >
              {status}
            </span>
            {usage ? (
              <span className='ml-auto min-w-0 truncate text-xs text-muted-foreground tabular-nums'>{usage}</span>
            ) : null}
          </div>
          {text ? (
            <p
              className={cn(
                'mt-1 whitespace-pre-wrap break-words text-foreground/90',
                // Three lines of the objective while collapsed.
                !expanded && 'line-clamp-3'
              )}
              ref={objectiveRef}
            >
              {text}
            </p>
          ) : null}
        </div>
        {expandable ? (
          <span aria-hidden='true' className='flex h-[1lh] shrink-0 items-center'>
            <IconChevronRight
              className={cn('ghostex-chat-disclosure-chevron size-3.5 text-muted-foreground', expanded && 'is-open')}
            />
          </span>
        ) : null}
      </div>
    </div>
  );
}
