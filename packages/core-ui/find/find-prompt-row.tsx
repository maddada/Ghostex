/*
CDXC:AgentHistorySearch 2026-08-20:
One result row: the matched prompt on the first line, and the last-active time
plus session title and project underneath — the same two-line shape the terminal
picker draws, wearing the chat surface's type and color tokens.
*/

import { IconStarFilled } from '@tabler/icons-react';
import { memo } from 'react';
import type { FindPromptRow } from '../../shared/agent-prompt-search';
import { cn } from '@/packages/components/utils';
import { flattenPromptLineWithOffsets, splitHighlightedSegments } from './find-prompt-highlight';

export interface FindPromptRowProps {
  onSelect: () => void;
  row: FindPromptRow;
  selected: boolean;
  timeLabel: string;
}

function PromptLine({ row }: { row: FindPromptRow }) {
  const { offsets, text } = flattenPromptLineWithOffsets(row.text, row.highlights);
  const segments = splitHighlightedSegments(text, offsets);
  return (
    <span className='ghostex-find-row-prompt'>
      {segments.map((segment, position) =>
        segment.highlighted ? (
          <mark className='bg-transparent font-semibold text-[var(--ghostex-find-match)]' key={position}>
            {segment.text}
          </mark>
        ) : (
          <span key={position}>{segment.text}</span>
        )
      )}
    </span>
  );
}

export const FindPromptResultRow = memo(function FindPromptResultRow({
  onSelect,
  row,
  selected,
  timeLabel,
}: FindPromptRowProps) {
  return (
    <div
      aria-selected={selected}
      className={cn(
        // NOTE: never add Tailwind's `group` utility here. The sidebar owns a
        // real `.group` class (a project group row) that sets display:grid and
        // justify-self:center, and it silently centered and shrank every result.
        'ghostex-find-row flex cursor-default gap-2 rounded-lg px-2 py-1.5 transition-colors',
        selected ? 'bg-accent/70 ring-1 ring-inset ring-border' : 'hover:bg-accent/30'
      )}
      data-find-row-index={row.index}
      data-selected={selected ? 'true' : undefined}
      onMouseDown={(event) => {
        // Selecting must never steal focus from the query input; the input keeps
        // every hotkey working while the pointer picks a row.
        event.preventDefault();
        onSelect();
      }}
      role='option'
    >
      <span className='flex w-3 shrink-0 justify-center pt-0.5'>
        {row.favorite ? <IconStarFilled aria-label='Favorite' className='size-3 text-amber-400' /> : null}
      </span>
      <span className='flex min-w-0 flex-1 flex-col gap-0.5'>
        <span className='flex min-w-0 items-baseline gap-2'>
          <span className='w-16 shrink-0 truncate text-[11px] font-medium' style={{ color: row.agentColor }}>
            {row.agent}
          </span>
          <span className='min-w-0 flex-1 truncate text-[13px] leading-5 text-foreground'>
            <PromptLine row={row} />
          </span>
        </span>
        <span className='flex min-w-0 items-baseline gap-2 text-[11px] text-muted-foreground'>
          <span className='w-16 shrink-0 truncate tabular-nums'>{timeLabel}</span>
          <span className='min-w-0 flex-1 truncate'>
            {row.title}
            <span className='px-1 opacity-60'>•</span>
            {row.projectName}
          </span>
        </span>
      </span>
    </div>
  );
});
