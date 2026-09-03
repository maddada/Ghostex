import { lazy, Suspense, useEffect, useState } from 'react';
import type { RemoteSetupRpc } from '@/packages/core-ui/remote-setup-modal';
import { getMachineConnection, rpcForMachine } from '../connections/connection-registry';

/*
 * CDXC:RemoteSetup 2026-09-03:
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
      />
    </Suspense>
  );
}
