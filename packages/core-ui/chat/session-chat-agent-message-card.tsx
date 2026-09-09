/*
CDXC:SessionChat 2026-09-06 DECISION:
User: a message one Codex agent sends to another shows as a collapsible card titled `Received a message from "<name>"`, in the same shape as the goal and Claude Code status cards.
Collapsed it shows only the first two lines of the message text; expanding shows the full message rendered as markdown.
*/

import { useLayoutEffect, useRef, useState } from 'react';
import { IconChevronRight } from '@tabler/icons-react';
import { cn } from '@/packages/components/utils';
import { SessionChatMarkdown } from './session-chat-markdown';
import { SessionChatSubagentLink } from './session-chat-subagent-link';

/**
 * How gxserver writes a decoded inter-agent message: the sender path on the
 * first line, a blank line, then the readable payload (session_chat_decode_codex.rs).
 */
const AGENT_MESSAGE_HEADER = /^Message from (\S+)\n\n([\s\S]*)$/;

export interface SessionChatAgentMessage {
  sender: string;
  body: string;
}

export function parseSessionChatAgentMessage(text: string): SessionChatAgentMessage | null {
  const match = AGENT_MESSAGE_HEADER.exec(text);
  if (!match) {
    return null;
  }
  return { body: match[2]?.trim() ?? '', sender: match[1] ?? '' };
}

/** `/root/windows_support` is addressed as `windows_support` by the agents themselves. */
function agentDisplayName(sender: string): string {
  const segments = sender.split('/').filter((segment) => segment.length > 0);
  return segments.at(-1) ?? sender;
}

export function SessionChatAgentMessageCard({ body, sender }: SessionChatAgentMessage) {
  const [expanded, setExpanded] = useState(false);
  const [overflows, setOverflows] = useState(false);
  const previewRef = useRef<HTMLParagraphElement>(null);
  const name = agentDisplayName(sender);

  // The chevron only appears when the clamp actually hides text.
  useLayoutEffect(() => {
    const element = previewRef.current;
    if (!element || expanded) {
      return;
    }
    const measure = (): void => setOverflows(element.scrollHeight > element.clientHeight + 1);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, [body, expanded]);

  const expandable = expanded || overflows;
  const toggle = (): void => {
    if (expandable) {
      setExpanded((value) => !value);
    }
  };

  return (
    <div
      className='ghostex-chat-activity-row ghostex-chat-status-card grid gap-2 rounded-2xl border border-border/65 bg-muted/20 px-4 py-3'
      data-kind='agent-message'
      data-sender={sender}
    >
      {/* Not a <button>: the chat scope's button rules would give the header
          their pill outline and hover fill, and it is a disclosure on a card
          that already has its own border. */}
      <div
        className={cn(
          'flex min-w-0 items-start gap-2 text-left leading-relaxed outline-none',
          expandable && 'cursor-pointer'
        )}
        onClick={toggle}
      >
        {/* One-line-tall (1lh) boxes center the dot and chevron on the first
            text line for any inherited font size; see session-chat-terminal-tool-row.tsx. */}
        <span aria-hidden='true' className='flex h-[1lh] shrink-0 items-center'>
          <span className='size-1.5 rounded-full bg-primary' />
        </span>
        <div className='min-w-0 flex-1'>
          <p className='font-medium text-foreground'>
            Received a message from &ldquo;
            <SessionChatSubagentLink name={name} selector={sender} />
            &rdquo; subagent
          </p>
          {!expanded ? (
            <p className='mt-1 line-clamp-2 whitespace-pre-wrap break-words text-foreground/90' ref={previewRef}>
              {body}
            </p>
          ) : null}
        </div>
        {expandable ? (
          <button
            className='ghostex-chat-agent-message-disclosure flex h-[1lh] items-center'
            type='button'
            aria-label={expanded ? 'Collapse agent message' : 'Expand agent message'}
            aria-expanded={expanded}
            onClick={(event) => {
              event.stopPropagation();
              toggle();
            }}
          >
            <IconChevronRight
              aria-hidden='true'
              className={cn('ghostex-chat-disclosure-chevron size-3.5 text-muted-foreground', expanded && 'is-open')}
            />
          </button>
        ) : null}
      </div>
      {expanded ? (
        <div className='ghostex-chat-agent-message min-w-0 pl-3.5'>
          <SessionChatMarkdown markdown={body} />
        </div>
      ) : null}
    </div>
  );
}
