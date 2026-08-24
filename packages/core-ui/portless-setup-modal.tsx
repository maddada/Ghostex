import { Button } from '@/packages/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/packages/components/ui/dialog';
import type { NativePortlessAdminInstallAction, NativePortlessProtocol } from '../shared/native-ghostty-host-protocol';

export type PortlessSetupModalMode = 'firstSetup' | 'standaloneReconfigure';
type PortlessSetupAdminAction = Extract<NativePortlessAdminInstallAction, 'install' | 'reconfigure'>;

type PortlessSetupModalCopy = {
  body: readonly [string, string];
  dismissLabel: 'Postpone' | 'Cancel';
  primaryAction: PortlessSetupAdminAction;
  primaryLabel: 'Install' | 'Reconfigure';
  title: string;
};

export const PORTLESS_SETUP_MODAL_COPY: Record<PortlessSetupModalMode, PortlessSetupModalCopy> = {
  firstSetup: {
    body: [
      'Ghostex found a running dev server. Portless gives it a stable local domain like https://ghostex.localhost, so you can run multiple apps and worktrees of the same project without conflicting ports.',
      'Installing the Portless background proxy requires admin permission once so it can listen on standard local web ports. You can disable Portless if you do not want Ghostex to show this again.',
    ],
    dismissLabel: 'Postpone',
    primaryAction: 'install',
    primaryLabel: 'Install',
    title: 'Set up Portless domains?',
  },
  standaloneReconfigure: {
    body: [
      'Portless is already installed on this Mac. Ghostex needs to manage the Portless background proxy so it can create stable domains for your projects and worktrees.',
      "Reconfiguring will point Portless at Ghostex's state directory. You can cancel, or disable Portless in Settings if you do not want Ghostex to show this again.",
    ],
    dismissLabel: 'Cancel',
    primaryAction: 'reconfigure',
    primaryLabel: 'Reconfigure',
    title: 'Reconfigure Portless for Ghostex?',
  },
};

export type PortlessSetupModalProps = {
  isOpen: boolean;
  mode: PortlessSetupModalMode;
  onAdminAction: (action: PortlessSetupAdminAction, protocol: NativePortlessProtocol, requestId: string) => void;
  onCancel: () => void;
  onDisable: () => void;
  onPostpone: () => void;
  protocol: NativePortlessProtocol;
};

export function PortlessSetupModal({
  isOpen,
  mode,
  onAdminAction,
  onCancel,
  onDisable,
  onPostpone,
  protocol,
}: PortlessSetupModalProps) {
  const copy = PORTLESS_SETUP_MODAL_COPY[mode];
  const dismiss = mode === 'firstSetup' ? onPostpone : onCancel;

  /*
   * CDXC:PortlessSetupModal 2026-06-23-13:42:
   * The Portless setup prompt is an app-modal React dialog, not an AppKit
   * alert. Buttons send only action/protocol/request-id enums or booleans to
   * native-sidebar so the logged modal command boundary never receives the
   * user's full settings object, paths, domains, URLs, or project metadata.
   */
  return (
    <Dialog
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          dismiss();
        }
      }}
      open={isOpen}
    >
      <DialogContent className='command-config-modal-shadcn portless-setup-modal-shadcn font-sans'>
        <div className='portless-setup-modal-content'>
          <DialogHeader>
            <DialogTitle className='text-xl'>{copy.title}</DialogTitle>
            <DialogDescription className='sr-only'>{copy.title}</DialogDescription>
          </DialogHeader>
          <div className='portless-setup-modal-body'>
            <p>{copy.body[0]}</p>
            <p>{copy.body[1]}</p>
          </div>
          <DialogFooter className='portless-setup-modal-actions'>
            <Button
              onClick={() =>
                onAdminAction(copy.primaryAction, protocol, createPortlessSetupModalRequestId(copy.primaryAction))
              }
              type='button'
            >
              {copy.primaryLabel}
            </Button>
            <Button onClick={dismiss} type='button' variant='outline'>
              {copy.dismissLabel}
            </Button>
            <Button onClick={onDisable} type='button' variant='destructive'>
              Disable
            </Button>
          </DialogFooter>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function createPortlessSetupModalRequestId(action: PortlessSetupAdminAction): string {
  return `portless-setup-${action}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}
