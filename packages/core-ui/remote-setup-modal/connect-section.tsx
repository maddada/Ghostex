import { useEffect, useRef, useState } from 'react';
import { IconLoader2, IconQrcode, IconShield } from '@tabler/icons-react';
import type { GxserverEnableSshAccessResult, GxserverRemoteAccessStatus } from '@/packages/shared/gxserver-protocol';
import { Button } from '@/packages/components/ui/button';
import { openAppModal, type SettingsRemoteSection } from '../app-modal-host-bridge';
import type { RemoteSetupRpc } from './gxserver-rpc';

const EASY_CONNECT_STEPS: readonly string[] = [
  'Click Connect. Easy Connect turns on and a pairing code appears. If SSH access is off, your computer asks for an admin password once to enable it.',
  "Scan it with the Ghostex app. That's it; the pairing stays until you remove it.",
];

const NO_SERVER_MESSAGE = 'The Ghostex server connection is unavailable.';

function openRemoteSettings(section: SettingsRemoteSection): void {
  openAppModal({ initialRemoteSection: section, initialTab: 'remote', modal: 'settings', type: 'open' });
}

/**
 * CDXC:RemoteSetup 2026-09-03:
 * Connect does everything the Easy Connect card promises: it reads SSH access,
 * enables it when it is off (one admin prompt, since Easy Connect carries SSH),
 * turns Easy Connect on, then hands off to Settings → Remote focused on the
 * Easy Connect card. A cancelled admin prompt still opens Settings, where the
 * card shows that SSH access is off together with the per-OS instructions;
 * only a failed request keeps the user here with the error.
 */
async function connectEasyConnect(rpc: RemoteSetupRpc): Promise<void> {
  const status = (await rpc('/api/remoteAccessStatus', {})) as GxserverRemoteAccessStatus;
  if (status.ssh.enabled === false) {
    const enabled = (await rpc('/api/enableSshAccess', {})) as GxserverEnableSshAccessResult;
    if (enabled.outcome === 'failed') {
      throw new Error(enabled.message ?? 'SSH access could not be turned on.');
    }
  }
  await rpc('/api/updateTailcatState', { kind: 'setEnabled', enabled: true });
}

export function ConnectSection({ rpc }: { rpc: RemoteSetupRpc | undefined }) {
  const [isConnecting, setIsConnecting] = useState(false);
  const [connectError, setConnectError] = useState<string>();
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const connect = () => {
    if (!rpc || isConnecting) {
      return;
    }
    setIsConnecting(true);
    setConnectError(undefined);
    void connectEasyConnect(rpc)
      .then(() => {
        openRemoteSettings('easyConnect');
      })
      .catch((error: unknown) => {
        if (mountedRef.current) {
          setConnectError(error instanceof Error ? error.message : String(error));
        }
      })
      .finally(() => {
        if (mountedRef.current) {
          setIsConnecting(false);
        }
      });
  };

  return (
    <section className='remote-setup-section remote-setup-connect'>
      <header className='remote-setup-section-head'>
        <span className='remote-setup-section-number'>2</span>
        <h3 className='remote-setup-section-title'>Connect it to this computer</h3>
      </header>
      <div className='remote-setup-option-card remote-setup-option-easy-connect' data-recommended='true'>
        <div className='remote-setup-option-head'>
          <span className='remote-setup-option-icon'>
            <IconQrcode aria-hidden='true' size={18} stroke={1.7} />
          </span>
          <div className='remote-setup-option-text'>
            <div className='remote-setup-option-title'>
              Easy Connect (QR/Token)
              <span className='remote-setup-tag'>Recommended</span>
            </div>
            <div className='remote-setup-option-sub'>
              Built into Ghostex. No VPN, no accounts, nothing to install on the computer.
            </div>
          </div>
        </div>
        <ol className='remote-setup-steps'>
          {EASY_CONNECT_STEPS.map((step, index) => (
            <li className='remote-setup-step' key={step}>
              <span className='remote-setup-step-number'>{index + 1}</span>
              <span>{step}</span>
            </li>
          ))}
        </ol>
        {connectError ? (
          <div className='remote-setup-error' role='alert'>
            {connectError}
          </div>
        ) : null}
        <div className='remote-setup-option-foot'>
          <span className='remote-setup-muted'>About a minute</span>
          <Button
            className='remote-setup-connect-button'
            disabled={!rpc || isConnecting}
            onClick={connect}
            size='sm'
            title={rpc ? undefined : NO_SERVER_MESSAGE}
            type='button'
          >
            {isConnecting ? (
              <IconLoader2 aria-hidden='true' className='remote-setup-spinner' />
            ) : (
              <IconQrcode aria-hidden='true' />
            )}
            {isConnecting ? 'Connecting…' : 'Connect'}
          </Button>
        </div>
      </div>
      <div className='remote-setup-option-card remote-setup-option-tailscale'>
        <div className='remote-setup-option-head'>
          <span className='remote-setup-option-icon'>
            <IconShield aria-hidden='true' size={18} stroke={1.7} />
          </span>
          <div className='remote-setup-option-text'>
            <div className='remote-setup-option-title'>
              Tailscale
              <span className='remote-setup-badge'>If you already use it</span>
            </div>
            <div className='remote-setup-option-sub'>
              Your device joins your tailnet and connects over SSH. Best when Tailscale is already on both devices.
            </div>
          </div>
        </div>
        <div className='remote-setup-option-foot'>
          <span className='remote-setup-muted'>You&apos;ll type the host, user and password on the device</span>
          <Button
            className='remote-setup-tailscale-instructions-button'
            onClick={() => openRemoteSettings('tailscale')}
            size='sm'
            type='button'
            variant='outline'
          >
            Show instructions
          </Button>
        </div>
      </div>
    </section>
  );
}
