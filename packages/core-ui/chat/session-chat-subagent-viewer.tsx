import { IconArrowLeft, IconLoader2, IconX } from '@tabler/icons-react';
import { useCallback, useMemo, useState, type ReactNode } from 'react';
import { Button } from '@/packages/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/packages/components/ui/dialog';
import { cn } from '@/packages/components/utils';
import { AppTooltip } from '../app-tooltip';
import type { SessionChatSubagentInfo, SessionChatTheme } from '@/packages/shared/session-chat';
import { SessionChatMessageList } from './session-chat-message-list';
import { SessionChatSubagentContext, type SessionChatSubagentTarget } from './session-chat-subagent-link';
import { SessionChatSubagentModel } from './session-chat-subagent-model';
import type { SessionChatTransport } from './session-chat-transport';
import { useSessionChatSubagent } from './use-session-chat-subagent';
import { useSessionChatWorkingHold } from './use-session-chat-working-hold';
import './session-chat-subagent.css';

function SubagentTranscript({
  read,
  target,
  open,
  readInfo,
  onBack,
  onClose,
  theme,
}: {
  read: NonNullable<SessionChatTransport['readSubagent']>;
  target: SessionChatSubagentTarget;
  open: (target: SessionChatSubagentTarget) => void;
  readInfo: (selector: string) => Promise<SessionChatSubagentInfo>;
  onBack?: () => void;
  onClose: () => void;
  theme: SessionChatTheme;
}) {
  const { page, error, loadingEarlier, loadEarlier, retry } = useSessionChatSubagent(read, target.selector);
  const isWorking = useSessionChatWorkingHold(page?.lifecycle?.state === 'working');
  const context = useMemo(
    () => ({ open, readInfo, agentPath: page?.subagent?.name.startsWith('/') ? page.subagent.name : '/root' }),
    [open, readInfo, page?.subagent?.name]
  );
  return (
    <SessionChatSubagentContext.Provider value={context}>
      <div className='flex items-center gap-3 border-b border-border px-4 py-3'>
        {onBack ? (
          <Button aria-label='Back to previous subagent' size='icon-sm' variant='ghost' onClick={onBack}>
            <IconArrowLeft />
          </Button>
        ) : null}
        <div className='min-w-0 flex-1'>
          <AppTooltip content={page?.subagent?.agentType ?? target.agentType ?? target.name} side='top'>
            <DialogTitle className='truncate'>
              <SessionChatSubagentModel
                info={page?.subagent ?? target}
                loading={!page && !target.model && !error}
                unavailable={Boolean(error && !page && !target.model)}
              />
            </DialogTitle>
          </AppTooltip>
          <DialogDescription className='mt-1 text-xs'>
            {target.task ?? 'Subagent transcript · Updates while open'}
          </DialogDescription>
        </div>
        <Button aria-label='Close subagent transcript' size='icon-sm' variant='ghost' onClick={onClose}>
          <IconX />
        </Button>
      </div>
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
  const readInfo = useMemo(() => {
    if (!read) return undefined;
    const pending = new Map<string, Promise<SessionChatSubagentInfo>>();
    return (selector: string) => {
      const existing = pending.get(selector);
      if (existing) return existing;
      const request = read({ subagent: selector, limit: 1 })
        .then((page) => {
          if (!page.subagent) throw new Error('Subagent details are unavailable.');
          return page.subagent;
        })
        .finally(() => pending.delete(selector));
      pending.set(selector, request);
      return request;
    };
  }, [read]);
  const context = useMemo(() => (readInfo ? { open, readInfo, agentPath: '/root' } : null), [open, readInfo]);
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
          {read && readInfo && target ? (
            <SubagentTranscript
              key={target.selector}
              read={read}
              target={target}
              open={open}
              readInfo={readInfo}
              onBack={stack.length > 1 ? () => setStack((stack) => stack.slice(0, -1)) : undefined}
              onClose={() => setStack([])}
              theme={theme}
            />
          ) : null}
        </DialogContent>
      </Dialog>
    </SessionChatSubagentContext.Provider>
  );
}
