import { IconAlertTriangle, IconShieldCheck } from '@tabler/icons-react';
import { Button } from '@/packages/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/packages/components/ui/dialog';
import type { GhostexExtensionCatalogEntry } from '@/packages/shared/ghostex-extensions';

const PERMISSION_DESCRIPTIONS = {
  cli: 'Control Ghostex through its command-line interface.',
  clipboard: 'Read from or write to your clipboard.',
  exec: 'Run system commands on this machine.',
  network: 'Connect to services on your network or the internet.',
  ssh: 'Use configured SSH access for remote machines.',
} as const;

export function InstallConsentDialog({
  entry,
  installing,
  onCancel,
  onConfirm,
  open,
}: {
  entry?: GhostexExtensionCatalogEntry;
  installing?: boolean;
  onCancel: () => void;
  onConfirm: () => void;
  open: boolean;
}) {
  const permissions = entry?.permissions ?? [];
  const runsBackgroundProcess = Boolean(entry?.server && 'command' in entry.server);
  return (
    <Dialog
      onOpenChange={(nextOpen) => {
        if (!nextOpen && !installing) onCancel();
      }}
      open={open}
    >
      <DialogContent className='max-w-lg bg-[#0e0e0e]' showCloseButton={false}>
        <DialogHeader>
          <DialogTitle>Install {entry?.title ?? 'extension'}?</DialogTitle>
          <DialogDescription>Review the access this audited extension declares before installing it.</DialogDescription>
        </DialogHeader>
        <div className='flex flex-col gap-3'>
          {permissions.length || runsBackgroundProcess ? (
            <ul className='flex flex-col gap-2' aria-label='Requested access'>
              {permissions.map((permission) => (
                <li className='flex items-start gap-3 border border-border/70 bg-card/40 p-3' key={permission}>
                  <IconShieldCheck aria-hidden='true' className='mt-0.5 shrink-0 text-muted-foreground' />
                  <div>
                    <div className='capitalize text-foreground'>{permission}</div>
                    <div className='mt-0.5 text-xs leading-5 text-muted-foreground'>
                      {PERMISSION_DESCRIPTIONS[permission]}
                    </div>
                  </div>
                </li>
              ))}
              {runsBackgroundProcess ? (
                <li className='flex items-start gap-3 border border-border/70 bg-card/40 p-3'>
                  <IconAlertTriangle aria-hidden='true' className='mt-0.5 shrink-0 text-muted-foreground' />
                  <div>
                    <div className='text-foreground'>Runs a background process</div>
                    <div className='mt-0.5 text-xs leading-5 text-muted-foreground'>
                      Server extensions run outside a sandbox. Open-source review remains the primary trust boundary.
                    </div>
                  </div>
                </li>
              ) : null}
            </ul>
          ) : (
            <p className='text-sm text-muted-foreground'>This extension does not request additional permissions.</p>
          )}
        </div>
        <DialogFooter>
          <Button disabled={installing} onClick={onCancel} type='button' variant='outline'>
            Cancel
          </Button>
          <Button disabled={!entry || installing} onClick={onConfirm} type='button'>
            {installing ? 'Installing…' : 'Install'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
