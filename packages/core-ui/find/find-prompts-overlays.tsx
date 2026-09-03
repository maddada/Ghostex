/*
CDXC:PromptSearch 2026-08-20:
The three overlays take over the bottom pane exactly where the terminal picker
puts them, so `^g`, `^j`, and `^o` land in the same place they always did.
*/

import { IconCheck } from '@tabler/icons-react';
import { useEffect, useRef, useState } from 'react';
import {
  FIND_PROMPT_AGENTS,
  type FindPromptAgent,
  type FindPromptProjectFacet,
} from '../../shared/agent-prompt-search';
import { cn } from '@/packages/components/utils';

function OverlayShell({ children, hint, title }: { children: React.ReactNode; hint: string; title: string }) {
  return (
    <div className='flex h-full min-h-0 flex-col gap-1 px-3 py-2'>
      <div className='text-[11px] font-medium uppercase tracking-wide text-muted-foreground'>{title}</div>
      <div className='min-h-0 flex-1 overflow-y-auto scrollbar-thin'>{children}</div>
      <div className='text-[11px] text-muted-foreground'>{hint}</div>
    </div>
  );
}

function OverlayRow({
  checked,
  focused,
  label,
  onSelect,
  swatch,
}: {
  checked: boolean;
  focused: boolean;
  label: string;
  onSelect: () => void;
  swatch?: string;
}) {
  const ref = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    if (focused) {
      ref.current?.scrollIntoView({ block: 'nearest' });
    }
  }, [focused]);
  return (
    <div
      className={cn(
        'flex cursor-default items-center gap-2 rounded-md px-2 py-1 text-[13px]',
        focused ? 'bg-accent/70 text-foreground' : 'text-muted-foreground hover:bg-accent/30'
      )}
      onMouseDown={(event) => {
        event.preventDefault();
        onSelect();
      }}
      ref={ref}
      role='option'
      aria-selected={checked}
    >
      {swatch ? (
        <span aria-hidden='true' className='size-2 shrink-0 rounded-full' style={{ backgroundColor: swatch }} />
      ) : null}
      <span className='min-w-0 flex-1 truncate'>{label}</span>
      {checked ? <IconCheck aria-hidden='true' className='size-3.5 text-emerald-400' /> : null}
    </div>
  );
}

export function FindAgentFilterOverlay({
  colors,
  cursor,
  onToggle,
  selected,
}: {
  colors: Readonly<Record<string, string>>;
  cursor: number;
  onToggle: (agent: FindPromptAgent) => void;
  selected: ReadonlySet<FindPromptAgent>;
}) {
  return (
    <OverlayShell
      hint='↑/↓ or ^p/^n move · Enter/Space toggle · 1-6 quick toggle · Esc close · select none to show all'
      title='Filter by agent'
    >
      {FIND_PROMPT_AGENTS.map((agent, position) => (
        <OverlayRow
          checked={selected.has(agent)}
          focused={position === cursor}
          key={agent}
          label={agent}
          onSelect={() => onToggle(agent)}
          swatch={colors[agent]}
        />
      ))}
    </OverlayShell>
  );
}

export function FindProjectFilterOverlay({
  cursor,
  onFilterChange,
  onSelect,
  projects,
  selected,
}: {
  cursor: number;
  filter: string;
  onFilterChange: (next: string) => void;
  onSelect: (path: string | null) => void;
  projects: readonly FindPromptProjectFacet[];
  selected: string | null;
}) {
  return (
    <OverlayShell
      hint='Type to search · ↑/↓ or ^p/^n move · Enter select · Space clears · Esc close'
      title='Filter by project'
    >
      <div className='mb-1'>
        <input
          aria-label='Filter projects'
          autoFocus
          className='w-full rounded-md bg-input/40 px-2 py-1 text-[13px] outline-none placeholder:text-muted-foreground'
          onChange={(event) => onFilterChange(event.target.value)}
          placeholder='project name'
          type='text'
        />
      </div>
      {projects.length === 0 ? (
        <div className='px-2 py-1 text-[13px] text-muted-foreground'>No matching projects</div>
      ) : null}
      {projects.map((facet, position) => (
        <OverlayRow
          checked={selected === facet.path}
          focused={position === cursor}
          key={facet.path}
          label={facet.name}
          onSelect={() => onSelect(selected === facet.path ? null : facet.path)}
        />
      ))}
    </OverlayShell>
  );
}

export function FindForkOverlay({
  colors,
  onPick,
}: {
  colors: Readonly<Record<string, string>>;
  onPick: (agent: FindPromptAgent) => void;
}) {
  return (
    <OverlayShell hint='Press 1-6, or click an agent · Esc cancels' title='Fork prompt into'>
      <div className='flex flex-wrap gap-1.5 py-1'>
        {FIND_PROMPT_AGENTS.map((agent, position) => (
          <button
            className='flex items-center gap-1.5 rounded-md bg-accent/40 px-2 py-1 text-[13px] hover:bg-accent/70'
            key={agent}
            onMouseDown={(event) => {
              event.preventDefault();
              onPick(agent);
            }}
            type='button'
          >
            <span className='font-semibold tabular-nums'>{position + 1}</span>
            <span aria-hidden='true' className='size-2 rounded-full' style={{ backgroundColor: colors[agent] }} />
            <span>{agent}</span>
          </button>
        ))}
      </div>
    </OverlayShell>
  );
}

/** Filters the project list the same way the terminal picker does. */
export function filterProjectFacets(
  projects: readonly FindPromptProjectFacet[],
  filter: string
): FindPromptProjectFacet[] {
  const needle = filter.trim().toLowerCase();
  if (!needle) {
    return [...projects];
  }
  return projects.filter(
    (facet) => facet.name.toLowerCase().includes(needle) || facet.path.toLowerCase().includes(needle)
  );
}

/** Cursor state shared by the overlays; wrapping matches the terminal picker. */
export function useOverlayCursor(count: number) {
  const [cursor, setCursor] = useState(0);
  useEffect(() => {
    setCursor((value) => (count === 0 ? 0 : Math.min(value, count - 1)));
  }, [count]);
  const move = (delta: number) => {
    setCursor((value) => {
      if (count === 0) {
        return 0;
      }
      return (value + delta + count) % count;
    });
  };
  return { cursor, move, setCursor };
}
