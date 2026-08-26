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
   *
   * CDXC:UnifiedAppModal 2026-08-26:
   * Portless setup now composes the shared AppModalShell, so its chrome comes
   * from `.gx-app-modal`. Keep the `portless-setup-modal-shadcn` marker class:
   * apps/desktop/views/modal-host.tsx measures that selector for the one-shot
   * native fit-height pass.
   */
  return (
    <AppModalShell className='portless-setup-modal-shadcn' isOpen={isOpen} onClose={dismiss}>
      <AppModalColumn>
        <AppModalHeader>
          <AppModalTitle>{copy.title}</AppModalTitle>
          <AppModalDescription>{copy.body[0]}</AppModalDescription>
        </AppModalHeader>
        <AppModalStack className='portless-setup-modal-body'>
          <Card size='sm'>
            <CardHeader>
              <CardDescription>{copy.body[1]}</CardDescription>
            </CardHeader>
          </Card>
        </AppModalStack>
        <AppModalFooter>
          <AppModalButton onClick={dismiss} type='button'>
            {copy.dismissLabel}
          </AppModalButton>
          <AppModalButton onClick={onDisable} tone='danger' type='button'>
            Disable
          </AppModalButton>
          <AppModalButton
            onClick={() =>
              onAdminAction(copy.primaryAction, protocol, createPortlessSetupModalRequestId(copy.primaryAction))
            }
            type='button'
          >
            {copy.primaryLabel}
          </AppModalButton>
        </AppModalFooter>
      </AppModalColumn>
    </AppModalShell>
  );
}

function createPortlessSetupModalRequestId(action: PortlessSetupAdminAction): string {
  return `portless-setup-${action}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}
