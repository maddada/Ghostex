import { lazy, Suspense, useEffect, useState } from 'react';
import type { ghostexSettings } from '@/packages/shared/ghostex-settings';
import type { TailcatSettingsRpc } from '@/packages/core-ui/settings-modal/tabs/remote-tailcat';
import { getMachineConnection, rpcForMachine } from '../connections/connection-registry';
import type { WebSidebarRuntime } from '../sidebar-runtime/sidebar-runtime';
import { readWebSettings, writeWebSettings } from './web-settings';

/*
 * CDXC:Tailcat 2026-08-31:
 * The tailcat sidecar is supervised by the daemon serving this page, so the
 * Remote page's Tailcat section talks to the local machine connection. The
 * identity is stable so the panel's status poll is not restarted per render.
 */
const LOCAL_TAILCAT_RPC: TailcatSettingsRpc = (path, params) => rpcForMachine('local', path, params);

const SettingsModal = lazy(() =>
  import('@/packages/core-ui/settings-modal').then((module) => ({ default: module.SettingsModal }))
);

export function SettingsModalHost({ runtime }: { runtime: WebSidebarRuntime }) {
  const [isOpen, setIsOpen] = useState(false);
  const [settings, setSettings] = useState(readWebSettings);

  useEffect(() => {
    const open = () => setIsOpen(true);
    const close = () => setIsOpen(false);
    window.addEventListener('ghostex-web:openSettingsModal', open);
    window.addEventListener('ghostex-web:closeAppModal', close);
    return () => {
      window.removeEventListener('ghostex-web:openSettingsModal', open);
      window.removeEventListener('ghostex-web:closeAppModal', close);
    };
  }, []);

  const save = (nextSettings: ghostexSettings) => {
    const normalized = writeWebSettings(nextSettings);
    setSettings(normalized);
    runtime.updateSettings(normalized);
  };

  if (!isOpen) {
    return null;
  }

  return (
    <Suspense fallback={null}>
      <SettingsModal
        appIconPickerUnavailable
        isOpen
        onChange={save}
        onClose={() => setIsOpen(false)}
        settings={settings}
        tailcatRpc={getMachineConnection('local') ? LOCAL_TAILCAT_RPC : undefined}
        theme='dark-blue'
      />
    </Suspense>
  );
}
