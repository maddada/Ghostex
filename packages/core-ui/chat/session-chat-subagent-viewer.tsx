import { IconArrowLeft, IconLoader2, IconX } from '@tabler/icons-react';
import { useCallback, useMemo, useState, type ReactNode } from 'react';
import { Button } from '@/packages/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/packages/components/ui/dialog';
import { cn } from '@/packages/components/utils';
import type { SessionChatTheme } from '@/packages/shared/session-chat';
import { SessionChatMessageList } from './session-chat-message-list';
import { SessionChatSubagentContext, type SessionChatSubagentTarget } from './session-chat-subagent-link';
import type { SessionChatTransport } from './session-chat-transport';
import { useSessionChatSubagent } from './use-session-chat-subagent';
import { useSessionChatWorkingHold } from './use-session-chat-working-hold';
import './session-chat-subagent.css';

function SubagentTranscript({
  read,
  target,
  open,
  theme,
}: {
  read: NonNullable<SessionChatTransport['readSubagent']>;
  target: SessionChatSubagentTarget;
  open: (target: SessionChatSubagentTarget) => void;
  theme: SessionChatTheme;
}) {
  const { page, error, loadingEarlier, loadEarlier, retry } = useSessionChatSubagent(read, target.selector);
  const isWorking = useSessionChatWorkingHold(page?.lifecycle?.state === 'working');
  const context = useMemo(
    () => ({ open, agentPath: page?.subagent?.name.startsWith('/') ? page.subagent.name : '/root' }),
    [open, page?.subagent?.name]
  );
  return (
    <SessionChatSubagentContext.Provider value={context}>
      {error ? (
        <div className='flex items-center gap-3 px-4 py-2 text-sm text-destructive' role='alert'>
          <span className='flex-1'>{error}</span>
          <Button onClick={retry} size='sm' variant='outline'>
            Retry
          </Button>
        </div>
      ) : null}
      {!page && !error ? (
        <div className='flex flex-1 items-center justify-center gap-2 text-muted-foreground' role='status'>
          <IconLoader2 className='size-4 animate-spin' /> Loading transcript…
        </div>
      ) : page?.messages.length === 0 ? (
        <p className='p-6 text-sm text-muted-foreground'>This subagent has not written any messages yet.</p>
      ) : page ? (
        <div className='flex min-h-0 flex-1 flex-col'>
          <SessionChatMessageList
            messages={page.messages}
            isWorking={isWorking}
            hasMore={page.hasMore && !error}
            loadingEarlier={loadingEarlier}
            onLoadEarlier={loadEarlier}
            theme={theme}
            sessionTitle={target.name}
          />
        </div>
      ) : null}
    </SessionChatSubagentContext.Provider>
  );
}

/**
 * CDXC:SessionChat 2026-09-07 DECISION:
 * User: clicking a subagent's name in the chat transcript shows that subagent's transcript in a popup with a backdrop over the main chat.
 * CDXC:SessionChat 2026-09-09 DECISION:
 * User: subagent transcripts default to the same normal display as main chat, with verbose and summarized modes off.
 */
export function SessionChatSubagentViewer({
  children,
  read,
  theme,
}: {
  children: ReactNode;
  read: SessionChatTransport['readSubagent'];
  theme: SessionChatTheme;
}) {
  const [stack, setStack] = useState<SessionChatSubagentTarget[]>([]);
  const target = stack.at(-1);
  const open = useCallback((target: SessionChatSubagentTarget) => setStack((stack) => [...stack, target]), []);
  const context = useMemo(() => (read ? { open, agentPath: '/root' } : null), [open, read]);
  return (
    <SessionChatSubagentContext.Provider value={context}>
      {children}
      <Dialog
        open={Boolean(target && read)}
        onOpenChange={(open) => {
          if (!open) setStack([]);
        }}
      >
        <DialogContent
          className={cn(
            'ghostex-session-chat-scope ghostex-chat-subagent-dialog [--radius:0.625rem]',
            theme === 'dark' && 'dark'
          )}
          data-chat-theme={theme}
          data-session-chat-typing-redirect-ignore='true'
          onKeyDown={(event) => event.stopPropagation()}
          onPaste={(event) => event.stopPropagation()}
        >
          <div className='flex items-center gap-3 border-b border-border px-4 py-3'>
            {stack.length > 1 ? (
              <Button
                aria-label='Back to previous subagent'
                size='icon-sm'
                variant='ghost'
                onClick={() => setStack((stack) => stack.slice(0, -1))}
              >
                <IconArrowLeft />
              </Button>
            ) : null}
            <div className='min-w-0 flex-1'>
              <DialogTitle className='truncate'>{target?.name}</DialogTitle>
              <DialogDescription className='mt-1 text-xs'>Subagent transcript · Updates while open</DialogDescription>
            </div>
            <Button aria-label='Close subagent transcript' size='icon-sm' variant='ghost' onClick={() => setStack([])}>
              <IconX />
            </Button>
          </div>
          {read && target ? (
            <SubagentTranscript key={target.selector} read={read} target={target} open={open} theme={theme} />
          ) : null}
        </DialogContent>
      </Dialog>
    </SessionChatSubagentContext.Provider>
  );
}
