import { useId, useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import { IconChevronDown, IconChevronRight } from '@tabler/icons-react';
import { GXSERVER_LOCAL_API_PORT } from '@/packages/shared/gxserver-protocol';
import { SettingsInput, SettingsTextarea } from '../fields';
import { RemoteCopyButton } from './remote-copy-button';
import {
  EASY_CONNECT_MAX_PORT,
  EASY_CONNECT_MIN_PORT,
  formatEasyConnectAllowedClientKeys,
  formatEasyConnectPorts,
  parseEasyConnectAllowedClientKeys,
  parseEasyConnectPortsInput,
} from './remote-easy-connect-model';
import type { RemoteAccessState } from './use-remote-access';

/**
 * CDXC:RemotePairing 2026-09-03:
 * One collapsible at the bottom of Settings → Remote for the controls a
 * regular user never needs: the ports Easy Connect serves, the raw allowed
 * client key list (Paired devices above is its friendly face), the bare
 * pairing address for pasting by hand, the binary, the local gxserver
 * endpoint, and the raw sidecar status for bug reports.
 */
export function RemoteAdvancedSection({ remote, rpcAvailable }: { remote: RemoteAccessState; rpcAvailable: boolean }) {
  const portsFieldId = useId();
  const allowedKeysFieldId = useId();
  const [open, setOpen] = useState(false);
  const [portsDraft, setPortsDraft] = useState<string>();
  const [portsError, setPortsError] = useState<string>();
  const [allowedKeysDraft, setAllowedKeysDraft] = useState<string>();
  const [allowedKeysOpen, setAllowedKeysOpen] = useState(false);
  const [rawStatusOpen, setRawStatusOpen] = useState(false);
  const status = remote.easyConnect;
  const apiPort = remote.pairingCode?.easyConnect?.code.port ?? GXSERVER_LOCAL_API_PORT;

  const commitPorts = () => {
    if (portsDraft === undefined) {
      return;
    }
    const ports = parseEasyConnectPortsInput(portsDraft);
    if (!ports) {
      setPortsError(
        `Ports must be numbers between ${EASY_CONNECT_MIN_PORT} and ${EASY_CONNECT_MAX_PORT}, separated by commas.`
      );
      return;
    }
    setPortsError(undefined);
    setPortsDraft(undefined);
    if (status && formatEasyConnectPorts(status.ports) === formatEasyConnectPorts(ports)) {
      return;
    }
    remote.setEasyConnectState({ kind: 'setPorts', ports });
  };

  const commitAllowedClientKeys = () => {
    if (allowedKeysDraft === undefined) {
      return;
    }
    const allowedClientKeys = parseEasyConnectAllowedClientKeys(allowedKeysDraft);
    setAllowedKeysDraft(undefined);
    if (status && formatEasyConnectAllowedClientKeys(status.allowedClientKeys) === allowedClientKeys.join('\n')) {
      return;
    }
    remote.setEasyConnectState({ allowedClientKeys, kind: 'setAllowedClientKeys' });
  };

  const portsValue = portsDraft ?? formatEasyConnectPorts(status?.ports ?? []);
  const allowedKeysValue = allowedKeysDraft ?? formatEasyConnectAllowedClientKeys(status?.allowedClientKeys ?? []);
  const token = status?.token ?? undefined;

  return (
    <section className='settings-remote-advanced' data-settings-remote-block='advanced'>
      <Button
        aria-expanded={open}
        className='settings-remote-collapsible-toggle settings-remote-advanced-toggle'
        onClick={() => setOpen((current) => !current)}
        type='button'
        variant='ghost'
      >
        {open ? <IconChevronDown aria-hidden='true' /> : <IconChevronRight aria-hidden='true' />}
        Advanced
        <span className='settings-remote-advanced-hint'>Easy Connect ports and keys, gxserver, raw status</span>
      </Button>
      {open ? (
        <div className='settings-remote-rows settings-remote-advanced-rows'>
          <div className='settings-remote-row settings-remote-adv-served-ports'>
            <div className='settings-management-main'>
              <label className='settings-management-title' htmlFor={portsFieldId}>
                Easy Connect served ports
              </label>
              <span className='settings-management-detail'>Comma-separated local ports exposed to paired phones.</span>
            </div>
            <SettingsInput
              aria-label='Easy Connect served ports'
              className='settings-remote-ports-input'
              disabled={!rpcAvailable || !status}
              id={portsFieldId}
              inputMode='numeric'
              onBlur={commitPorts}
              onChange={(event) => {
                setPortsDraft(event.currentTarget.value);
                setPortsError(undefined);
              }}
              onKeyDown={(event) => {
                if (event.key === 'Enter') {
                  event.preventDefault();
                  commitPorts();
                }
              }}
              placeholder={String(apiPort)}
              value={portsValue}
            />
          </div>
          {portsError ? <p className='settings-remote-field-error'>{portsError}</p> : null}

          <div
            className='settings-remote-row settings-remote-adv-allowed-keys'
            data-open={allowedKeysOpen || undefined}
          >
            <div className='settings-management-main'>
              <label className='settings-management-title' htmlFor={allowedKeysFieldId}>
                Allowed client keys
              </label>
              <span className='settings-management-detail'>
                Empty allows any device that scanned the code. Paired devices are listed above.
              </span>
            </div>
            <Button
              aria-expanded={allowedKeysOpen}
              onClick={() => setAllowedKeysOpen((current) => !current)}
              size='xs'
              type='button'
              variant='outline'
            >
              {allowedKeysOpen ? 'Hide list' : 'Edit list'}
            </Button>
          </div>
          {allowedKeysOpen ? (
            <div className='settings-remote-row settings-remote-row-stacked settings-remote-adv-allowed-keys-body'>
              <label className='settings-management-detail' htmlFor={allowedKeysFieldId}>
                One client key per line.
              </label>
              <SettingsTextarea
                aria-label='Easy Connect allowed client keys'
                className='settings-remote-keys-textarea'
                disabled={!rpcAvailable || !status}
                id={allowedKeysFieldId}
                onBlur={commitAllowedClientKeys}
                onChange={(event) => setAllowedKeysDraft(event.currentTarget.value)}
                rows={4}
                value={allowedKeysValue}
              />
            </div>
          ) : null}

          <div className='settings-remote-row settings-remote-adv-pairing-address'>
            <div className='settings-management-main'>
              <span className='settings-management-title'>Pairing address</span>
              <span className='settings-management-detail'>
                The raw address inside the QR, for pasting into the app by hand.
              </span>
            </div>
            {token ? (
              <span className='settings-remote-row-value settings-remote-mono'>
                <code className='settings-remote-token' title={token}>
                  {token}
                </code>
                <RemoteCopyButton copyLabel='Copy the pairing address' value={token} />
              </span>
            ) : (
              <span className='settings-remote-row-value settings-management-detail'>
                {status?.enabled ? 'Not published yet' : 'Turn on Easy Connect'}
              </span>
            )}
          </div>

          <div className='settings-remote-row settings-remote-adv-binary'>
            <div className='settings-management-main'>
              <span className='settings-management-title'>Easy Connect binary</span>
            </div>
            <span className='settings-remote-row-value settings-remote-mono'>
              {status?.binaryFound && status.binaryPath
                ? status.binaryVersion
                  ? `${status.binaryPath} (${status.binaryVersion})`
                  : status.binaryPath
                : 'Not found'}
            </span>
          </div>

          <div className='settings-remote-row settings-remote-adv-gxserver'>
            <div className='settings-management-main'>
              <span className='settings-management-title'>gxserver</span>
              <span className='settings-management-detail'>Local API the app and phones talk to.</span>
            </div>
            <span className='settings-remote-row-value settings-remote-mono'>
              127.0.0.1:{apiPort} · {rpcAvailable && status ? 'running' : 'unreachable'}
            </span>
          </div>

          <div className='settings-remote-row settings-remote-adv-raw-status'>
            <div className='settings-management-main'>
              <span className='settings-management-title'>Raw Easy Connect status</span>
            </div>
            <Button
              aria-expanded={rawStatusOpen}
              disabled={!status}
              onClick={() => setRawStatusOpen((current) => !current)}
              size='xs'
              type='button'
              variant='ghost'
            >
              {rawStatusOpen ? 'Hide JSON' : 'Show JSON'}
            </Button>
          </div>
          {rawStatusOpen && status ? (
            <pre className='settings-remote-raw-status'>{JSON.stringify(status, null, 2)}</pre>
          ) : null}
        </div>
      ) : null}
    </section>
  );
}
