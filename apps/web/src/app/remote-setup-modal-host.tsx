import { lazy, Suspense, useEffect, useState } from 'react';
import type { RemoteSetupRpc } from '@/packages/core-ui/remote-setup-modal';
import { getMachineConnection, rpcForMachine } from '../connections/connection-registry';
import { readWebSettings, WEB_SETTINGS_CHANGED_EVENT } from './web-settings';

/*
 * CDXC:RemotePairing 2026-09-03:
 * The Remote Setup modal (sidebar menu → Mobile & Remote) configures the
 * daemon serving this page, so its RPC goes to the local machine connection;
 * without one, Connect is disabled inside the modal.
 */
const LOCAL_REMOTE_SETUP_RPC: RemoteSetupRpc = (path, params) => rpcForMachine('local', path, params);

const RemoteSetupModal = lazy(() =>
  import('@/packages/core-ui/remote-setup-modal').then((module) => ({ default: module.RemoteSetupModal }))
);

export function RemoteSetupModalHost() {
  const [isOpen, setIsOpen] = useState(false);
  const [settings, setSettings] = useState(readWebSettings);

  useEffect(() => {
    const handleSettingsChanged = (event: Event) => {
      setSettings((event as CustomEvent<ReturnType<typeof readWebSettings>>).detail);
    };
    window.addEventListener(WEB_SETTINGS_CHANGED_EVENT, handleSettingsChanged);
    return () => window.removeEventListener(WEB_SETTINGS_CHANGED_EVENT, handleSettingsChanged);
  }, []);

  useEffect(() => {
    const open = () => setIsOpen(true);
    const close = () => setIsOpen(false);
    window.addEventListener('ghostex-web:openRemoteSetupModal', open);
    window.addEventListener('ghostex-web:closeAppModal', close);
    return () => {
      window.removeEventListener('ghostex-web:openRemoteSetupModal', open);
      window.removeEventListener('ghostex-web:closeAppModal', close);
    };
  }, []);

  if (!isOpen) {
    return null;
  }

  return (
    <Suspense fallback={null}>
      <RemoteSetupModal
        isOpen
        onClose={() => setIsOpen(false)}
        onOpenExternalUrl={(url) => {
          window.open(url, '_blank', 'noopener,noreferrer');
        }}
        rpc={getMachineConnection('local') ? LOCAL_REMOTE_SETUP_RPC : undefined}
        tailscaleEnabled={settings.remoteTailscaleEnabled}
      />
    </Suspense>
  );
}
