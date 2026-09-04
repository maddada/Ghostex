/*
CDXC:AgentScreenDetection 2026-09-03 WHY:
Context window usage ring for the chat composer, the t3code ContextWindowMeter
shape: a small ring in the footer that fills as the window fills, hover for a
popover with the percentage, tokens over window size, a progress bar and a
Compact button. The numbers come from the statusLine payload Claude Code pipes
to the Ghostex-installed script (see server/src/agent_hooks/statusline.rs), so
the ring exists only for Claude sessions and only once that payload arrived.
*/

import { IconPencil } from '@tabler/icons-react';
import { Button } from '../../components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '../../components/ui/popover';
import type { SessionChatContextUsage } from '../../shared/session-chat';
import { AppTooltip } from '../app-tooltip';
import type { SessionChatContextDetailGroup } from './session-chat-context-details';

export interface SessionChatContextMeterUsage {
  /** 0–100, or null when Claude reported neither a percentage nor tokens over a window. */
  usedPercentage: number | null;
  usedTokens: number | null;
  windowSize: number | null;
}

/**
 * Tokens over window size when Claude reported both (exact), else the
 * rounded percentage it reported. Null when there is nothing to draw.
 */
export function resolveSessionChatContextMeterUsage(
  usage: SessionChatContextUsage | undefined
): SessionChatContextMeterUsage | null {
  if (!usage) {
    return null;
  }
  const usedTokens = isFiniteNonNegative(usage.usedTokens) ? usage.usedTokens : null;
  const windowSize = isFiniteNonNegative(usage.windowSize) && usage.windowSize > 0 ? usage.windowSize : null;
  const usedPercentage =
    usedTokens !== null && windowSize !== null
      ? Math.min(100, (usedTokens / windowSize) * 100)
      : isFiniteNonNegative(usage.usedPercentage)
        ? Math.min(100, usage.usedPercentage)
        : null;
  if (usedPercentage === null && usedTokens === null) {
    return null;
  }
  return { usedPercentage, usedTokens, windowSize };
}

function isFiniteNonNegative(value: number | undefined): value is number {
  return typeof value === 'number' && Number.isFinite(value) && value >= 0;
}

export function formatSessionChatContextTokens(value: number | null): string {
  if (value === null || !Number.isFinite(value)) {
    return '0';
  }
  if (value < 1_000) {
    return `${Math.round(value)}`;
  }
  if (value < 10_000) {
    return `${(value / 1_000).toFixed(1).replace(/\.0$/, '')}k`;
  }
  if (value < 1_000_000) {
    return `${Math.round(value / 1_000)}k`;
  }
  return `${(value / 1_000_000).toFixed(1).replace(/\.0$/, '')}m`;
}

export function formatSessionChatContextPercentage(value: number | null): string | null {
  if (value === null || !Number.isFinite(value)) {
    return null;
  }
  if (value < 10) {
    return `${value.toFixed(1).replace(/\.0$/, '')}%`;
  }
  return `${Math.round(value)}%`;
}

const RING_RADIUS = 9.75;
const RING_CIRCUMFERENCE = 2 * Math.PI * RING_RADIUS;
/** Above this the ring and bar turn to the destructive colour. */
const OVERLOADED_PERCENTAGE = 90;

export function SessionChatContextMeter({
  usage,
  onCompact,
  compactDisabled,
  compactDisabledReason,
  details,
  onEditDetails,
}: {
  usage: SessionChatContextMeterUsage;
  onCompact?: (() => void) | undefined;
  compactDisabled?: boolean;
  compactDisabledReason?: string | null;
  /**
   * The "More details" groups (session-chat-context-details.ts), already
   * filtered to rows that are shown and have a value. Absent hides the section.
   */
  details?: readonly SessionChatContextDetailGroup[];
  /** Opens the row picker; the pen icon exists only with this. */
  onEditDetails?: (() => void) | undefined;
}) {
  const percentageLabel = formatSessionChatContextPercentage(usage.usedPercentage);
  const normalizedPercentage = Math.max(0, Math.min(100, usage.usedPercentage ?? 0));
  const dashOffset = RING_CIRCUMFERENCE * (1 - normalizedPercentage / 100);
  const isOverloaded = normalizedPercentage > OVERLOADED_PERCENTAGE;
  const usageColor = isOverloaded
    ? 'var(--destructive)'
    : 'color-mix(in oklab, var(--muted-foreground) 72%, transparent)';
  const ariaLabel =
    usage.windowSize !== null && percentageLabel
      ? `Context window ${percentageLabel} used`
      : percentageLabel
        ? `Context window ${percentageLabel} used`
        : `Context window ${formatSessionChatContextTokens(usage.usedTokens)} tokens used`;

  return (
    <Popover>
      <PopoverTrigger
        closeDelay={onCompact ? 150 : 0}
        delay={150}
        openOnHover
        render={
          <Button
            aria-label={ariaLabel}
            className='ghostex-chat-footer-control ghostex-chat-context-meter ml-[6px] rounded-full text-muted-foreground hover:text-muted-foreground'
            size='icon-xs'
            variant='ghost'
          />
        }
      >
        <span className='relative flex size-4 items-center justify-center'>
          <svg aria-hidden='true' className='absolute inset-0 size-full -rotate-90 transform-gpu' viewBox='0 0 24 24'>
            <circle
              cx='12'
              cy='12'
              fill='none'
              r={RING_RADIUS}
              stroke='color-mix(in oklab, var(--muted-foreground) 24%, transparent)'
              strokeWidth='3'
            />
            <circle
              className='transition-[stroke-dashoffset,stroke] duration-500 ease-out motion-reduce:transition-none'
              cx='12'
              cy='12'
              fill='none'
              r={RING_RADIUS}
              stroke={usageColor}
              strokeDasharray={RING_CIRCUMFERENCE}
              strokeDashoffset={dashOffset}
              strokeLinecap='round'
              strokeWidth='3'
            />
          </svg>
        </span>
      </PopoverTrigger>
      <PopoverContent
        align='end'
        className={
          details
            ? 'ghostex-session-chat-popup ghostex-chat-context-meter-popover w-80 gap-2 rounded-xl p-3 text-left whitespace-normal [--radius:0.625rem]'
            : 'ghostex-session-chat-popup ghostex-chat-context-meter-popover w-64 gap-2 rounded-xl p-3 text-left whitespace-normal [--radius:0.625rem]'
        }
        side='top'
        sideOffset={8}
      >
        <div className='flex items-center justify-between gap-3'>
          <div className='text-xs font-medium text-muted-foreground'>Context window</div>
          {usage.windowSize !== null && percentageLabel ? (
            <div className='text-[11px] text-muted-foreground tabular-nums'>
              <span>{percentageLabel}</span>
              <span className='mx-1'>·</span>
              <span>
                {formatSessionChatContextTokens(usage.usedTokens)}/{formatSessionChatContextTokens(usage.windowSize)}
              </span>
            </div>
          ) : (
            <div className='text-[11px] text-muted-foreground tabular-nums'>
              {percentageLabel ?? formatSessionChatContextTokens(usage.usedTokens)}
            </div>
          )}
        </div>
        {usage.usedPercentage !== null ? (
          <div
            aria-label='Context window usage'
            aria-valuemax={100}
            aria-valuemin={0}
            aria-valuenow={Math.round(normalizedPercentage)}
            className='h-1.5 w-full overflow-hidden rounded-full bg-muted/60'
            role='progressbar'
          >
            <div
              className='h-full rounded-full transition-[width,background-color] duration-500 ease-out motion-reduce:transition-none'
              style={{ width: `${normalizedPercentage}%`, backgroundColor: usageColor }}
            />
          </div>
        ) : null}
        <div className='text-[11px] text-muted-foreground'>Compacts automatically as the window fills.</div>
        {onCompact ? (
          <AppTooltip content={compactDisabled ? compactDisabledReason : null} side='top'>
            <span className={compactDisabled ? 'mt-1 block w-full cursor-not-allowed' : 'mt-1 block w-full'}>
              <Button
                className='w-full justify-center rounded-md'
                disabled={compactDisabled}
                onClick={onCompact}
                size='xs'
                variant='outline'
              >
                Compact context
              </Button>
            </span>
          </AppTooltip>
        ) : null}
        {details ? (
          <div className='ghostex-chat-context-details mt-1 border-t border-border/60 pt-2'>
            <div className='flex items-center justify-between'>
              <div className='text-[11px] font-medium text-muted-foreground'>More details</div>
              {onEditDetails ? (
                <AppTooltip content='Choose which details to show' side='top'>
                  <Button
                    aria-label='Choose which details to show'
                    className='ghostex-chat-context-details-edit -mr-1 rounded-md text-muted-foreground'
                    onClick={onEditDetails}
                    size='icon-xs'
                    variant='ghost'
                  >
                    <IconPencil size={12} stroke={1.8} />
                  </Button>
                </AppTooltip>
              ) : null}
            </div>
            {details.length === 0 ? (
              <div className='mt-1 text-[11px] text-muted-foreground'>Nothing selected.</div>
            ) : (
              details.map((group) => (
                <div className='ghostex-chat-context-details-group' key={group.id}>
                  <div className='ghostex-chat-context-details-group-label'>{group.label}</div>
                  {group.items.map((item) => (
                    <div className='ghostex-chat-context-details-row' key={item.id}>
                      <span className='ghostex-chat-context-details-key'>{item.label}</span>
                      <span className='ghostex-chat-context-details-value'>{item.value}</span>
                    </div>
                  ))}
                </div>
              ))
            )}
          </div>
        ) : null}
      </PopoverContent>
    </Popover>
  );
}
