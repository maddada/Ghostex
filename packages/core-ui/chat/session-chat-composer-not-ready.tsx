/*
CDXC:SessionChat 2026-08-26:
The composer's refusal notice for `composerNotReady` — the one send failure the
user can actually fix, because the agent CLI is sitting on a trust prompt, an
auth screen, or a first-run setup step and never painted an input box.

"Message could not be sent" is useless for that, so this notice says what is
wrong, and then hands over two ways to deal with it:
  * "Show terminal" expands a read-only excerpt of the session's screen,
    fetched from /api/readSessionTerminalTail through the transport. It is
    re-fetched on every expand — the whole point is to see the CURRENT screen,
    and by the time a user re-opens it they have usually just tried something.
  * "Open Terminal" switches to the session's terminal surface, where they can
    answer the prompt themselves. It reuses the host's existing surface switch
    (SessionChatHostActions.onSwitchToTerminal), the same pathway the composer
    footer's Terminal View button and the agent-picker pill already use.

Both are feature-gated: a host with no terminal surface (or no route to the
endpoint) simply does not pass the callback and the affordance is not rendered,
rather than showing a control that does nothing.

User decision: this refusal uses the same notice card as terminal-detected notices above the chat box instead of appearing as an unframed error block.
*/

import { IconChevronRight, IconLoader2, IconTerminal2 } from '@tabler/icons-react';
import { useRef, useState } from 'react';
import type { GxserverReadSessionTerminalTailResult } from '@/packages/shared/gxserver-protocol';
import { cn } from '@/packages/components/utils';
import { Button } from '../../components/ui/button';
import { SessionChatNoticeCard } from './session-chat-notice-card';

const NOT_READY_HEADLINE = "The agent's input box is not ready yet. Your draft was restored.";

export function SessionChatComposerNotReadyNotice({
  onOpenTerminal,
  onReadTerminalTail,
  reason,
}: {
  /** Host surface switch; omitted by hosts without a terminal surface. */
  onOpenTerminal?: () => void;
  /** Transport-bound tail read; omitted by hosts without the endpoint. */
  onReadTerminalTail?: () => Promise<GxserverReadSessionTerminalTailResult>;
  /** The daemon's own sentence for the refusal, when it sent one. */
  reason?: string;
}) {
  const [expanded, setExpanded] = useState(false);
  const [loading, setLoading] = useState(false);
  const [tail, setTail] = useState<GxserverReadSessionTerminalTailResult | null>(null);
  const [tailError, setTailError] = useState<string | null>(null);
  // Monotonic request id: a fast collapse/expand must not let the older read
  // paint over the newer one.
  const requestRef = useRef(0);

  const toggle = (): void => {
    if (expanded) {
      setExpanded(false);
      return;
    }
    setExpanded(true);
    const read = onReadTerminalTail;
    if (!read) {
      return;
    }
    const request = requestRef.current + 1;
    requestRef.current = request;
    setLoading(true);
    setTailError(null);
    void read()
      .then((result) => {
        if (requestRef.current !== request) {
          return;
        }
        setTail(result);
      })
      .catch((error: unknown) => {
        if (requestRef.current !== request) {
          return;
        }
        setTail(null);
        setTailError(error instanceof Error ? error.message : 'The terminal screen could not be read.');
      })
      .finally(() => {
        if (requestRef.current === request) {
          setLoading(false);
        }
      });
  };

  const excerpt = ((): string | null => {
    if (!tail || !tail.captured || tail.lines.length === 0) {
      return null;
    }
    return tail.lines.join('\n');
  })();

  return (
    <SessionChatNoticeCard kind='composerNotReady' role='alert' severity='error'>
      <div className='flex min-w-0 flex-col px-3 py-2.5'>
        <div className='text-sm leading-snug font-medium text-foreground'>{NOT_READY_HEADLINE}</div>
        {reason && reason !== NOT_READY_HEADLINE ? (
          <div className='mt-1 text-xs leading-snug text-muted-foreground'>{reason}</div>
        ) : null}
        {onReadTerminalTail || onOpenTerminal ? (
          <div className='mt-3 flex flex-wrap items-center gap-1.5'>
            {onReadTerminalTail ? (
              <Button aria-expanded={expanded} onClick={toggle} size='xs' type='button' variant='outline'>
                <IconChevronRight
                  aria-hidden='true'
                  className={cn('transition-transform', expanded && 'rotate-90')}
                  stroke={1.8}
                />
                {expanded ? 'Hide terminal' : 'Show terminal'}
              </Button>
            ) : null}
            {onOpenTerminal ? (
              <Button onClick={onOpenTerminal} size='xs' type='button' variant='outline'>
                <IconTerminal2 aria-hidden='true' stroke={1.8} />
                Open Terminal
              </Button>
            ) : null}
          </div>
        ) : null}
        {expanded ? (
          <div className='mt-2 min-w-0 overflow-hidden rounded-lg border border-input bg-muted/40'>
            {loading ? (
              <div className='flex items-center gap-2 px-3 py-2 text-xs text-muted-foreground'>
                <IconLoader2 aria-hidden='true' className='size-3.5 animate-spin' stroke={2} />
                Reading the terminal…
              </div>
            ) : tailError !== null ? (
              <div className='px-3 py-2 text-xs text-muted-foreground'>{tailError}</div>
            ) : excerpt !== null ? (
              <pre className='max-h-48 overflow-auto whitespace-pre-wrap break-words px-3 py-2 font-mono text-[11px] leading-[1.45] text-foreground'>
                {excerpt}
              </pre>
            ) : (
              <div className='px-3 py-2 text-xs text-muted-foreground'>
                {tail && !tail.captured
                  ? 'Ghostex could not read this session’s terminal screen.'
                  : 'The terminal screen is empty.'}
              </div>
            )}
          </div>
        ) : null}
      </div>
    </SessionChatNoticeCard>
  );
}
