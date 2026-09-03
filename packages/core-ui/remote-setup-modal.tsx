import {
  AppModalColumn,
  AppModalDescription,
  AppModalHeader,
  AppModalShell,
  AppModalStack,
  AppModalTitle,
} from './app-modal-shell';
import { ConnectSection } from './remote-setup-modal/connect-section';
import { GetAppSection } from './remote-setup-modal/get-app-section';
import type { RemoteSetupRpc } from './remote-setup-modal/gxserver-rpc';

export type { RemoteSetupRpc } from './remote-setup-modal/gxserver-rpc';
export { gpuiBootstrapRemoteSetupRpc } from './remote-setup-modal/gxserver-rpc';

export type RemoteSetupModalProps = {
  isOpen: boolean;
  onClose: () => void;
  /** Opens a URL in the system browser (Discord invite). */
  onOpenExternalUrl: (url: string) => void;
  /** gxserver RPC for this page's local daemon; undefined disables Connect. */
  rpc: RemoteSetupRpc | undefined;
  /** Settings.remoteTailscaleEnabled; off hides the Tailscale option. */
  tailscaleEnabled: boolean;
};

/**
 * CDXC:RemotePairing 2026-09-03:
 * The sidebar menu's "Mobile & Remote" entry. Two numbered sections at the
 * same level: get the Ghostex app (Android APK, iPhone via TestFlight), then
 * connect it to this computer (Easy Connect, recommended; Tailscale if you
 * already use it). Both connect paths hand off to Settings → Remote. Keep the
 * `remote-setup-modal` marker class: apps/desktop/views/modal-host.tsx measures
 * it for the one-shot native fit-height pass, and `remote-setup-modal-body` is
 * the scroll container once the Android popover grows the content past the
 * fitted window.
 */
export function RemoteSetupModal({ isOpen, onClose, onOpenExternalUrl, rpc, tailscaleEnabled }: RemoteSetupModalProps) {
  return (
    <AppModalShell className='remote-setup-modal' isOpen={isOpen} onClose={onClose} showCloseButton width={560}>
      <AppModalColumn>
        <AppModalHeader>
          <div className='remote-setup-eyebrow'>Mobile &amp; Remote</div>
          <AppModalTitle>Remote Setup</AppModalTitle>
          <AppModalDescription className='remote-setup-intro'>
            Use Ghostex from your phone, or from any other computer, from anywhere. Two steps: get the app, then connect
            it to this computer.
          </AppModalDescription>
        </AppModalHeader>
        <AppModalStack className='remote-setup-modal-body'>
          <GetAppSection onOpenExternalUrl={onOpenExternalUrl} />
          <ConnectSection rpc={rpc} tailscaleEnabled={tailscaleEnabled} />
        </AppModalStack>
      </AppModalColumn>
    </AppModalShell>
  );
}
