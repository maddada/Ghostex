/*
CDXC:AgentHistorySearchModal 2026-08-23:
Search by Prompt is an app-level modal, matching Settings, rather than a mode
that replaces the focused workspace pane. The transport remains scoped to the
active project's machine, while result focus reuses the existing workspace
focus event after the modal closes.
*/

import { IconX } from '@tabler/icons-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/packages/components/ui/dialog';
import { FindPromptsView } from '@/packages/core-ui/find/find-prompts-view';
import '@/packages/core-ui/styles.css';
import { createFindPromptsTransport } from '../find/find-prompts-transport';
import { getActiveSidebarProject } from '../sidebar-runtime/active-project-store';

export function FindPromptsHost({ machineId, onClose }: { machineId: string; onClose(): void }) {
  const transport = useMemo(
    () =>
      createFindPromptsTransport(machineId, {
        focusSession: ({ projectId, sessionId }) => {
          onClose();
          window.dispatchEvent(
            new CustomEvent('ghostex-web:focusSession', {
              detail: {
                machineId,
                placement: 'focusedPane',
                projectId,
                sessionId,
                source: 'sidebar',
              },
            })
          );
        },
        close: onClose,
      }),
    [machineId, onClose]
  );
  return (
    <Dialog onOpenChange={(open) => (!open ? onClose() : undefined)} open>
      <DialogContent
        className='ghostex-settings-shadcn settings-modal-dialog flex flex-col gap-0 overflow-hidden p-0 font-sans'
        data-sidebar-theme='dark-blue'
        showCloseButton={false}
      >
        <DialogTitle className='sr-only'>Search by Prompt</DialogTitle>
        <DialogDescription className='sr-only'>Search prompts from previous agent sessions.</DialogDescription>
        <FindPromptsView
          hostActions={
            <button
              aria-label='Close Search by Prompt'
              className='inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground'
              onClick={onClose}
              type='button'
            >
              <IconX aria-hidden='true' className='size-4' />
            </button>
          }
          transport={transport}
        />
      </DialogContent>
    </Dialog>
  );
}

export function FindPromptsModalHost() {
  const [machineId, setMachineId] = useState<string>();
  const close = useCallback(() => setMachineId(undefined), []);

  useEffect(() => {
    const open = () => setMachineId(getActiveSidebarProject()?.machineId);
    window.addEventListener('ghostex-web:openFindPrompts', open);
    window.addEventListener('ghostex-web:closeAppModal', close);
    return () => {
      window.removeEventListener('ghostex-web:openFindPrompts', open);
      window.removeEventListener('ghostex-web:closeAppModal', close);
    };
  }, [close]);

  return machineId ? <FindPromptsHost machineId={machineId} onClose={close} /> : null;
}
