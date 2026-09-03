/*
CDXC:SessionChat 2026-08-21:
The option rows shared by the two answer pickers: the AskUserQuestion card
(agent-asked questions) and the terminal-notice card (a picker the agent CLI
painted on screen, such as Claude Code's resume-usage chooser).

They are one component because they are one affordance — a user answering
"which model?" and a user answering "resume from summary or in full?" should not
have to learn two row shapes. Everything specific to a surface (the header, the
free-text lane, the submit button) stays with that surface.
*/

import { IconCheck } from '@tabler/icons-react';
import { cn } from '@/packages/components/utils';

export interface SessionChatChoiceRowOption {
  label: string;
  /** Second line; skipped when it only repeats the label. */
  description?: string;
  /** Badge for the row the surface wants marked as its default. */
  badge?: string;
}

export interface SessionChatChoiceRowsProps {
  options: SessionChatChoiceRowOption[];
  /** Selected row indices; multi-select surfaces pass more than one. */
  selected: number[];
  onSelect: (index: number) => void;
  /** Rows render dimmed and inert (input is held elsewhere / answer in flight). */
  readOnly?: boolean;
  /**
   * Show the 1-9 shortcut key on unselected rows. Off when the surface has no
   * matching keyboard handler, so the badge can never promise a dead key.
   */
  showShortcuts?: boolean;
}

export function SessionChatChoiceRows({
  onSelect,
  options,
  readOnly = false,
  selected,
  showShortcuts = false,
}: SessionChatChoiceRowsProps) {
  return (
    <div className='max-h-[45vh] space-y-1.5 overflow-y-auto'>
      {options.map((option, optionIndex) => {
        const isSelected = selected.includes(optionIndex);
        const shortcutKey = showShortcuts && optionIndex < 9 ? optionIndex + 1 : null;
        return (
          <button
            className={cn(
              'group/option flex w-full items-center gap-3 rounded-lg border px-3 py-2 text-left outline-none transition-all duration-150 focus-visible:border-ring focus-visible:ring-1 focus-visible:ring-ring/30',
              isSelected
                ? 'border-primary/30 bg-primary/10 text-foreground'
                : 'border-transparent bg-foreground/[0.045] text-foreground/85 hover:border-border hover:bg-foreground/[0.08]',
              readOnly && 'cursor-default opacity-60'
            )}
            data-chat-answer-control=''
            data-selected={isSelected ? 'true' : undefined}
            // The sidebar's legacy `button:where(:not([data-slot]))` base paints
            // a 1px app border on every bare button; naming the slot opts these
            // rows out so their Tailwind borders/fills are the only ones.
            data-slot='session-chat-question-option'
            disabled={readOnly}
            key={`${optionIndex}:${option.label}`}
            onClick={() => {
              onSelect(optionIndex);
            }}
            type='button'
          >
            <span className='flex min-w-0 flex-1 flex-col gap-0.5'>
              <span className='text-sm leading-snug font-medium'>{option.label}</span>
              {option.description && option.description !== option.label ? (
                <span className='text-xs leading-snug text-muted-foreground'>{option.description}</span>
              ) : null}
            </span>
            {option.badge ? (
              <span className='flex h-5 shrink-0 items-center rounded-md bg-muted/60 px-1.5 text-[10px] font-medium text-muted-foreground'>
                {option.badge}
              </span>
            ) : null}
            {isSelected ? (
              <IconCheck aria-hidden='true' className='ghostex-chat-glyph-semantic text-primary' />
            ) : shortcutKey !== null ? (
              <kbd className='flex size-5 shrink-0 items-center justify-center rounded border border-border/60 bg-background/40 text-[11px] font-medium text-muted-foreground tabular-nums transition-colors duration-150 group-hover/option:text-foreground'>
                {shortcutKey}
              </kbd>
            ) : null}
          </button>
        );
      })}
    </div>
  );
}
