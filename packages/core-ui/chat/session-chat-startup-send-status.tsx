import { useState } from 'react';
import type { SessionChatMessage } from '../../shared/session-chat';
import { Button } from '../../components/ui/button';

export interface SessionChatStartupSendActions {
  onRetryStartupSend?: (promptId: string) => Promise<unknown>;
  onRemoveStartupSend?: (promptId: string) => Promise<unknown>;
}

export function SessionChatStartupSendStatus({
  delivery,
  onRetryStartupSend,
  onRemoveStartupSend,
}: SessionChatStartupSendActions & { delivery: NonNullable<SessionChatMessage['startupDelivery']> }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const run = async (action: (promptId: string) => Promise<unknown>): Promise<void> => {
    setBusy(true);
    setError(null);
    try {
      await action(delivery.promptId);
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };
  const failed = delivery.state === 'failed';
  return (
    <div className='flex max-w-full flex-wrap items-center justify-end gap-1 self-end text-muted-foreground text-xs' role='status'>
      <span>{error ?? (failed ? delivery.errorMessage ?? 'Message could not be delivered.' : 'Waiting for agent…')}</span>
      {failed && onRetryStartupSend ? (
        <Button disabled={busy} onClick={() => void run(onRetryStartupSend)} size='sm' variant='ghost'>Retry</Button>
      ) : null}
      {failed && onRemoveStartupSend ? (
        <Button disabled={busy} onClick={() => void run(onRemoveStartupSend)} size='sm' variant='ghost'>
          Remove
        </Button>
      ) : null}
    </div>
  );
}
