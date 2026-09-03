import { useEffect, useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import { IconDeviceLaptop, IconDeviceMobile, IconLoader2 } from '@tabler/icons-react';
import type { GxserverPairedDevice } from '@/packages/shared/gxserver-protocol';
import { formatPairedDeviceDetail, isPairedDeviceConnectedNow } from './remote-easy-connect-model';

const PHONE_PLATFORMS = new Set(['android', 'ios', 'iphone', 'ipad', 'mobile', 'phone']);

function isPhonePlatform(platform: string): boolean {
  return PHONE_PLATFORMS.has(platform.trim().toLowerCase());
}

/**
 * CDXC:RemotePairing 2026-09-03:
 * The friendly face of the pairing registry: one row per device with its
 * platform glyph, when it paired, and whether it checked in within the last
 * three minutes. Remove asks for confirmation inline, naming the device, and
 * then posts `/api/removePairedDevice`, which also drops the device's SSH key.
 */
export function PairedDevicesList({
  devices,
  onRemove,
  removingDeviceId,
  rpcAvailable,
}: {
  devices: readonly GxserverPairedDevice[] | undefined;
  onRemove: (deviceId: string) => void;
  removingDeviceId: string | undefined;
  rpcAvailable: boolean;
}) {
  const [confirmingDeviceId, setConfirmingDeviceId] = useState<string>();
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    const interval = window.setInterval(() => setNow(Date.now()), 30_000);
    return () => window.clearInterval(interval);
  }, []);

  useEffect(() => {
    if (confirmingDeviceId && !devices?.some((device) => device.id === confirmingDeviceId)) {
      setConfirmingDeviceId(undefined);
    }
  }, [confirmingDeviceId, devices]);

  return (
    <div className='settings-remote-paired-devices'>
      <span className='settings-remote-section-label'>Paired devices</span>
      <div className='settings-remote-rows settings-remote-paired-device-rows'>
        {devices === undefined ? (
          <div className='settings-remote-row settings-remote-paired-device-row' data-state='loading'>
            <span className='settings-management-detail'>Loading paired devices…</span>
          </div>
        ) : devices.length === 0 ? (
          <div className='settings-remote-row settings-remote-paired-device-row' data-state='empty'>
            <span className='settings-management-detail'>
              No devices yet. Scan the code with the Ghostex app and it appears here.
            </span>
          </div>
        ) : (
          devices.map((device) => {
            const connected = isPairedDeviceConnectedNow(device, now);
            const Icon = isPhonePlatform(device.platform) ? IconDeviceMobile : IconDeviceLaptop;
            const isConfirming = confirmingDeviceId === device.id;
            const isRemoving = removingDeviceId === device.id;
            return (
              <div
                className='settings-remote-row settings-remote-paired-device-row'
                data-connected={connected || undefined}
                data-device-id={device.id}
                key={device.id}
              >
                <div className='settings-management-main'>
                  <span className='settings-remote-row-label'>
                    <Icon aria-hidden='true' size={15} />
                    {device.name}
                  </span>
                  <span className='settings-management-detail'>{formatPairedDeviceDetail(device, now)}</span>
                </div>
                <div className='settings-remote-row-value'>
                  {isConfirming ? (
                    <>
                      <span className='settings-management-detail settings-remote-remove-confirm-text'>
                        Remove {device.name}?
                      </span>
                      <Button
                        disabled={isRemoving}
                        onClick={() => {
                          onRemove(device.id);
                          setConfirmingDeviceId(undefined);
                        }}
                        size='xs'
                        type='button'
                        variant='destructive'
                      >
                        {isRemoving ? <IconLoader2 aria-hidden='true' className='settings-remote-spinner' /> : null}
                        Remove
                      </Button>
                      <Button onClick={() => setConfirmingDeviceId(undefined)} size='xs' type='button' variant='ghost'>
                        Cancel
                      </Button>
                    </>
                  ) : (
                    <>
                      <span
                        aria-label={connected ? 'Connected now' : 'Not connected'}
                        className='settings-remote-dot'
                        data-connected={connected || undefined}
                        role='img'
                      />
                      <Button
                        aria-label={`Remove ${device.name}`}
                        disabled={!rpcAvailable || isRemoving}
                        onClick={() => setConfirmingDeviceId(device.id)}
                        size='xs'
                        type='button'
                        variant='ghost'
                      >
                        {isRemoving ? <IconLoader2 aria-hidden='true' className='settings-remote-spinner' /> : null}
                        Remove
                      </Button>
                    </>
                  )}
                </div>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
