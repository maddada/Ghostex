import { IconAlertTriangle, IconShieldCheck } from '@tabler/icons-react';
import {
  AppModalButton,
  AppModalColumn,
  AppModalDescription,
  AppModalFooter,
  AppModalHeader,
  AppModalShell,
  AppModalTitle,
} from '@/packages/core-ui/app-modal-shell';
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
  const loadsRemoteWebsite = Boolean(entry?.server && 'url' in entry.server);
  const remoteUrl = entry?.server && 'url' in entry.server ? entry.server.url : undefined;
  return (
    <AppModalShell
      className='extensions-consent-modal'
      isOpen={open}
      onClose={() => {
        if (!installing) onCancel();
      }}
      width={520}
    >
      <AppModalColumn>
        <AppModalHeader>
          <AppModalTitle>Install {entry?.title ?? 'extension'}?</AppModalTitle>
          <AppModalDescription>
            Review the access this audited extension declares before installing it.
          </AppModalDescription>
        </AppModalHeader>
        {permissions.length || runsBackgroundProcess || loadsRemoteWebsite ? (
          <ul aria-label='Requested access' className='extensions-consent-list'>
            {permissions.map((permission) => (
              <li className='extensions-consent-item' key={permission}>
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
              <li className='extensions-consent-item'>
                <IconAlertTriangle aria-hidden='true' className='mt-0.5 shrink-0 text-muted-foreground' />
                <div>
                  <div className='text-foreground'>Runs a background process</div>
                  <div className='mt-0.5 text-xs leading-5 text-muted-foreground'>
                    Server extensions run outside a sandbox. Open-source review remains the primary trust boundary.
                  </div>
                </div>
              </li>
            ) : null}
            {loadsRemoteWebsite ? (
              <li className='extensions-consent-item'>
                <IconAlertTriangle aria-hidden='true' className='mt-0.5 shrink-0 text-muted-foreground' />
                <div>
                  <div className='text-foreground'>Loads a remote website</div>
                  <div className='mt-0.5 text-xs leading-5 text-muted-foreground'>
                    This extension opens {remoteUrl} directly. The page runs outside Ghostex and cannot use the
                    extension bridge, but it sees whatever you type into it.
                  </div>
                </div>
              </li>
            ) : null}
          </ul>
        ) : (
          <p className='text-[13px] text-muted-foreground'>This extension does not request additional permissions.</p>
        )}
        <AppModalFooter>
          <AppModalButton disabled={installing} onClick={onCancel} type='button'>
            Cancel
          </AppModalButton>
          <AppModalButton disabled={!entry || installing} onClick={onConfirm} type='button'>
            {installing ? 'Installing…' : 'Install'}
          </AppModalButton>
        </AppModalFooter>
      </AppModalColumn>
    </AppModalShell>
  );
}
