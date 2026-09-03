import { Card, CardContent } from '@/packages/components/ui/card';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import {
  AppModalButton,
  AppModalColumn,
  AppModalDescription,
  AppModalFooter,
  AppModalHeader,
  AppModalShell,
  AppModalStack,
  AppModalTitle,
} from './app-modal-shell';

export type UpdateAvailableModalState = {
  notesMarkdown: string;
  portable: boolean;
  state: 'available' | 'ready';
  version: string;
};

/**
 * CDXC:AppModal 2026-08-26:
 * Restyled onto AppModalShell. The `update-available-modal` class stays on the
 * shell root as a marker: apps/desktop/views/modal-host.tsx measures that
 * selector to fit the native child window's height.
 */
export function UpdateAvailableModal({
  isOpen,
  onCancel,
  onDownload,
  onRestart,
  update,
}: {
  isOpen: boolean;
  onCancel: () => void;
  onDownload: () => void;
  onRestart: () => void;
  update?: UpdateAvailableModalState;
}) {
  const ready = update?.state === 'ready';
  return (
    <AppModalShell className='update-available-modal' isOpen={isOpen} onClose={onCancel} width={520}>
      <AppModalColumn>
        <AppModalHeader>
          <AppModalTitle>{ready ? 'Ghostex is ready to update' : 'A Ghostex update is available'}</AppModalTitle>
          <AppModalDescription>Version {update?.version}</AppModalDescription>
        </AppModalHeader>
        <AppModalStack>
          <Card size='sm'>
            <CardContent className='ghostex-chat-markdown update-available-modal-notes'>
              {update?.notesMarkdown.trim() ? (
                <ReactMarkdown
                  components={{
                    a: ({ children }) => <span>{children}</span>,
                    img: () => null,
                  }}
                  remarkPlugins={[remarkGfm]}
                  skipHtml
                >
                  {update.notesMarkdown}
                </ReactMarkdown>
              ) : (
                <p>This update includes improvements and fixes for Ghostex.</p>
              )}
            </CardContent>
          </Card>
          {update?.portable ? (
            <p className='update-available-modal-portable'>
              This portable copy will be updated in place and remain portable.
            </p>
          ) : null}
        </AppModalStack>
        <AppModalFooter>
          <AppModalButton onClick={onCancel} type='button'>
            {ready ? 'Later' : 'Cancel'}
          </AppModalButton>
          <AppModalButton onClick={ready ? onRestart : onDownload} type='button'>
            {ready ? 'Restart and update' : 'Download update'}
          </AppModalButton>
        </AppModalFooter>
      </AppModalColumn>
    </AppModalShell>
  );
}
