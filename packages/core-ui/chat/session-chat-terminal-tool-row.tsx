/*
CDXC:SessionChatTerminalActivity 2026-09-04 DECISION:
User: the pending tool card carries the painted row's text as its header and
opens to show the actual tool call text the TUI shows under it, in a mono
code area like the tool rows' Command block; the card remembers whether it
was left open or closed, so the next card comes up the same way instead of
the chat looking frozen while tools run. The header shows only the dot on the
left, with the expand chevron on the right, and draws no outline of its own.
*/

import { useState } from 'react';
import type { KeyboardEvent } from 'react';
import { IconChevronRight } from '@tabler/icons-react';
import type { SessionChatTerminalActivity } from '../../shared/session-chat';
import { cn } from '@/packages/components/utils';

const TERMINAL_TOOL_EXPANDED_STORAGE_KEY = 'ghostex.sessionChat.terminalToolExpanded';

function readTerminalToolExpanded(): boolean {
  try {
    return window.localStorage.getItem(TERMINAL_TOOL_EXPANDED_STORAGE_KEY) === 'true';
  } catch {
    // localStorage can be unavailable in isolated story contexts.
    return false;
  }
}

function writeTerminalToolExpanded(expanded: boolean): void {
  try {
    window.localStorage.setItem(TERMINAL_TOOL_EXPANDED_STORAGE_KEY, expanded ? 'true' : 'false');
  } catch {
    // Same as above: the preference simply does not persist.
  }
}

export function SessionChatTerminalToolRow({ activity }: { activity: SessionChatTerminalActivity }) {
  const [expanded, setExpanded] = useState(readTerminalToolExpanded);
  const detail = activity.detail?.trim() ?? '';
  const expandable = detail.length > 0;
  const open = expanded && expandable;
  const toggle = (): void => {
    if (!expandable) {
      return;
    }
    const next = !expanded;
    setExpanded(next);
    writeTerminalToolExpanded(next);
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
      className='ghostex-chat-activity-row my-1 grid gap-2 rounded-2xl border border-border/65 bg-muted/20 px-4 py-3'
      data-kind={activity.kind}
      role='status'
    >
      {/* Not a <button>: the chat scope's button rules would give the header
          their pill outline and hover fill, and it is a disclosure on a card
          that already has its own border. */}
      <div
        aria-expanded={open}
        className={cn('flex min-w-0 items-start gap-2 text-left outline-none', expandable && 'cursor-pointer')}
        onClick={toggle}
        onKeyDown={onKeyDown}
        role='button'
        tabIndex={expandable ? 0 : -1}
      >
        <span aria-hidden='true' className='mt-[7px] size-1.5 shrink-0 animate-pulse rounded-full bg-primary' />
        <span className='min-w-0 flex-1 whitespace-pre-wrap break-words text-sm leading-5 text-foreground/90'>
          {activity.label}
        </span>
        {expandable ? (
          <IconChevronRight
            aria-hidden='true'
            className={cn(
              'ghostex-chat-disclosure-chevron mt-[3px] size-3.5 shrink-0 text-muted-foreground',
              open && 'is-open'
            )}
          />
        ) : null}
      </div>
      {open ? <pre className='ghostex-chat-tool-body'>{detail}</pre> : null}
    </div>
  );
}
