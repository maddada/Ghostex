import { IconFileText } from '@tabler/icons-react';
import { Button } from '@/packages/components/ui/button';
import { Popover, PopoverContent, PopoverTitle, PopoverTrigger } from '@/packages/components/ui/popover';
import type { SessionChatDraft } from '@/packages/shared/session-chat-queue';

/**
 * CDXC:Drafts 2026-09-10 DECISION:
 * User: the saved-draft notice needs an icon that previews the message in a popover above it on hover or click.
 */
export function SessionChatDraftConflict({
  draft,
  onUse,
  onDismiss,
}: {
  draft: SessionChatDraft;
  onUse: () => void;
  onDismiss: () => void;
}) {
  return (
    <div className='ghostex-chat-draft-conflict' role='status'>
      <Popover>
        <PopoverTrigger
          openOnHover
          delay={150}
          closeDelay={150}
          render={
            <Button aria-label='Preview saved draft' className='shrink-0 rounded-full' size='icon-xs' variant='ghost' />
          }
        >
          <IconFileText aria-hidden='true' size={15} stroke={1.8} />
        </PopoverTrigger>
        <PopoverContent
          side='top'
          align='start'
          sideOffset={8}
          initialFocus={false}
          className='w-[min(26rem,calc(100vw-2rem))] gap-3 rounded-xl p-3.5 shadow-xl'
        >
          <PopoverTitle className='text-xs font-medium text-muted-foreground'>Saved draft</PopoverTitle>
          <div className='max-h-[min(20rem,45vh)] overflow-y-auto overscroll-contain whitespace-pre-wrap break-words rounded-lg bg-muted/40 p-3 text-sm leading-relaxed select-text'>
            {draft.content}
          </div>
        </PopoverContent>
      </Popover>
      <span className='ghostex-chat-draft-conflict-text'>Another saved draft is available</span>
      <button className='ghostex-chat-draft-conflict-action' onClick={onUse} type='button'>
        Use
      </button>
      <button className='ghostex-chat-draft-conflict-action' onClick={onDismiss} type='button'>
        Dismiss
      </button>
    </div>
  );
}
