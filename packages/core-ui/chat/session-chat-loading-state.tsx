import { IconLoader2 } from '@tabler/icons-react';
import { Button } from '@/packages/components/ui/button';

export function SessionChatLoadingState({
  stage,
  onRetry,
}: {
  stage: 'blank' | 'indicator' | 'retry';
  onRetry: () => void;
}) {
  return (
    <div
      aria-busy='true'
      aria-label='Conversation messages'
      className='flex min-h-0 flex-1 items-center justify-center overflow-auto text-muted-foreground text-sm'
      style={{ paddingBottom: 'var(--ghostex-chat-composer-inset, 0px)' }}
    >
      {stage === 'blank' ? null : (
        <div className='flex flex-col items-center gap-3' role='status'>
          <div className='flex items-center gap-2'>
            <IconLoader2 aria-hidden='true' className='size-4 animate-spin' stroke={2} />
            <span>Loading conversation…</span>
          </div>
          {stage === 'retry' ? (
            <Button onClick={onRetry} size='sm' variant='outline'>
              Retry
            </Button>
          ) : null}
        </div>
      )}
    </div>
  );
}
