import { Card, CardDescription, CardHeader } from '@/packages/components/ui/card';
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

export type RemoteGxserverInstallModalProps = {
  isOpen: boolean;
  machineName: string;
  onApprove: () => void;
  onCancel: () => void;
};

export function RemoteGxserverInstallModal({
  isOpen,
  machineName,
  onApprove,
  onCancel,
}: RemoteGxserverInstallModalProps) {
  /*
   * CDXC:RemoteMachines 2026-06-02-23:38:
   * Missing remote gxserver installation must be a user-approved React modal.
   * The app explains why gxserver is required before native uploads the bundled
   * package over SSH and starts it on the selected machine.
   *
   * CDXC:RemoteMachines 2026-06-23-09:46:
   * The approved install path must use a package that matches the remote
   * machine's OS and CPU. The package owns gxserver plus the pinned zmx, bd,
   * Ghostex CLI, and Node resources needed for first-run Ubuntu attach, so copy
   * must not imply that only the local macOS daemon is uploaded.
   *
   * CDXC:AppModal 2026-08-26:
   * The prompt composes the shared AppModalShell; the install detail paragraph
   * (with its monospace path/binary chips) is one section card. Keep the
   * `remote-gxserver-install-modal` marker class: apps/desktop/views/modal-host.tsx
   * measures that selector for the one-shot native fit-height pass.
   */
  return (
    <AppModalShell className='remote-gxserver-install-modal' isOpen={isOpen} onClose={onCancel}>
      <AppModalColumn>
        <AppModalHeader>
          <AppModalTitle>Install remote gxserver</AppModalTitle>
          <AppModalDescription>
            Ghostex can connect to {machineName}, but gxserver is not installed there. Ghostex needs gxserver on that
            machine to browse folders, add projects, clone repositories, and manage sessions remotely.
          </AppModalDescription>
        </AppModalHeader>
        <AppModalStack className='remote-gxserver-install-modal-body'>
          <Card size='sm'>
            <CardHeader>
              <CardDescription>
                If you continue, Ghostex will copy its compatible bundled remote package over SSH into{' '}
                <code>{'${XDG_DATA_HOME:-~/.local/share}/ghostex/gxserver'}</code>, expose <code>gxserver</code>,{' '}
                <code>zmx</code>, <code>bd</code>, <code>ghostex</code>, and <code>gx</code> from{' '}
                <code>~/.local/bin</code> when possible, start gxserver, then connect through an SSH tunnel. Windows
                machines use the selected or default WSL2 distribution, and Ghostex installs the Linux package in that
                distribution&apos;s home directory.
              </CardDescription>
            </CardHeader>
          </Card>
        </AppModalStack>
        <AppModalFooter>
          <AppModalButton onClick={onCancel} type='button'>
            Cancel
          </AppModalButton>
          <AppModalButton onClick={onApprove} type='button'>
            Install gxserver
          </AppModalButton>
        </AppModalFooter>
      </AppModalColumn>
    </AppModalShell>
  );
}
