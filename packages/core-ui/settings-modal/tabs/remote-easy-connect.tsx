import { useId } from 'react';
import { Button } from '@/packages/components/ui/button';
import { QrCode } from '@/packages/components/ui/qr-code';
import { Switch } from '@/packages/components/ui/switch';
import {
  IconAlertTriangle,
  IconChevronDown,
  IconChevronRight,
  IconDownload,
  IconLoader2,
  IconPower,
  IconQrcode,
} from '@tabler/icons-react';
import { RemoteCopyButton } from './remote-copy-button';
import { getEasyConnectStatusBadge } from './remote-easy-connect-model';
import { PairedDevicesList } from './remote-paired-devices';
import { SshAccessRow } from './remote-ssh-access-row';
import type { RemoteAccessState } from './use-remote-access';

const SSH_REQUIRED_ON = 'Required. Easy Connect carries SSH to this computer.';
const SSH_REQUIRED_OFF =
  'Required. Easy Connect carries SSH to this computer; turning it on asks for an admin password once.';

/**
 * CDXC:RemotePairing 2026-09-03:
 * The Easy Connect path card on Settings → Remote. Header toggle + status
 * badge, the SSH access row (Easy Connect carries SSH, so it is a hard
 * requirement), the pairing QR built from `/api/remotePairingCode` (never from
 * the raw sidecar token, so the code carries the user, the ports, and the
 * one-time secret and rotates with it), and the paired device list. The off
 * state keeps the same header and a blurred placeholder where the QR goes.
 *
 * CDXC:RemotePairing 2026-09-03 DECISION:
 * User: show Easy Connect and Tailscale "as expandible cards so the user clicks to expand the one they want to use. i dont want the user confused by seeing 2 qr codes in front of themselves".
 * The card is collapsed to its header row (icon, title, badges, the enable switch, a chevron) until `expanded`; the QR, SSH row, and paired devices only render inside the open body, and the parent keeps at most one path card open.
 * The switch sits beside the header button, not inside it, so toggling Easy Connect never expands or collapses the card.
 *
 * CDXC:RemotePairing 2026-09-05 DECISION:
 * User: explain why the Tailcat CLI is needed and install it with one click; separate Connect a Phone (compact QR) from Connect a Remote machine (copy button only), and explain the SSH login on the other device.
 */
export function EasyConnectCard({
  expanded,
  onToggleExpanded,
  remote,
  rpcAvailable,
}: {
  expanded: boolean;
  onToggleExpanded: () => void;
  remote: RemoteAccessState;
  rpcAvailable: boolean;
}) {
  const titleId = useId();
  const bodyId = useId();
  const status = remote.easyConnect;
  const badge = getEasyConnectStatusBadge(status);
  const binaryFound = status?.binaryFound === true;
  const isOn = status?.enabled === true;
  const easyConnectCode = remote.pairingCode?.easyConnect;
  const platform = remote.access?.platform;

  return (
    <section
      aria-labelledby={titleId}
      className='settings-remote-path-card settings-remote-easy-connect-card'
      data-expanded={expanded || undefined}
      data-settings-remote-section='easyConnect'
      data-state={isOn ? 'on' : 'off'}
      tabIndex={-1}
    >
      <div className='settings-remote-path-head'>
        <button
          aria-controls={expanded ? bodyId : undefined}
          aria-expanded={expanded}
          className='settings-remote-path-toggle'
          onClick={onToggleExpanded}
          type='button'
        >
          <span className='settings-remote-path-icon' data-accent={isOn || undefined}>
            <IconQrcode aria-hidden='true' size={16} />
          </span>
          <span className='settings-remote-path-title' id={titleId}>
            Easy Connect
          </span>
          <span className='settings-remote-badges'>
            <span className='settings-remote-tag'>Recommended</span>
            <span className='settings-remote-status-badge' data-status={badge.tone}>
              {badge.label}
            </span>
          </span>
          <span aria-hidden='true' className='settings-remote-path-chevron'>
            {expanded ? <IconChevronDown size={16} /> : <IconChevronRight size={16} />}
          </span>
        </button>
        <Switch
          aria-label='Turn Easy Connect on or off'
          checked={isOn}
          disabled={!rpcAvailable || !status || !binaryFound}
          onCheckedChange={(checked) => remote.setEasyConnectState({ enabled: checked, kind: 'setEnabled' })}
          onClick={(event) => event.stopPropagation()}
        />
      </div>
      {expanded ? (
        <div className='settings-remote-path-body' id={bodyId}>
          <p className='settings-management-description'>
            {isOn
              ? 'Reach this computer from your phone or another computer. No VPN setup or account needed.'
              : 'Reach this computer from your phone or another computer. Turn on Easy Connect to get a pairing code.'}
          </p>

          {status && (!binaryFound || status.installing) ? (
            <div className='settings-remote-install'>
              <div className='settings-management-main'>
                <span className='settings-management-title'>Install the Easy Connect helper</span>
                <span className='settings-management-detail'>
                  Easy Connect uses the Tailcat CLI to carry your SSH connection through an encrypted tunnel, so your
                  devices can reach this computer across networks. Ghostex installs and manages the helper for you.
                </span>
              </div>
              <Button
                className='settings-remote-install-button'
                disabled={!rpcAvailable || remote.isInstallingEasyConnect}
                onClick={remote.installEasyConnect}
                size='sm'
                type='button'
              >
                {remote.isInstallingEasyConnect ? (
                  <IconLoader2 aria-hidden='true' className='animate-spin' />
                ) : (
                  <IconDownload aria-hidden='true' />
                )}
                {remote.isInstallingEasyConnect ? 'Installing Easy Connect…' : 'Install Easy Connect'}
              </Button>
              <span className='settings-management-detail' role='status'>
                {status.installProgress ??
                  'One-time setup. Any required build tools are downloaded automatically. No terminal commands needed.'}
              </span>
            </div>
          ) : null}
          {remote.easyConnectInstallError ? (
            <div className='settings-remote-error' role='alert'>
              <IconAlertTriangle aria-hidden='true' />
              <span>{remote.easyConnectInstallError}</span>
            </div>
          ) : null}

          <SshAccessRow
            attempt={remote.sshEnableAttempt}
            className='settings-remote-easy-connect-ssh-row'
            detailWhenOff={SSH_REQUIRED_OFF}
            detailWhenOn={SSH_REQUIRED_ON}
            isEnabling={remote.isEnablingSsh}
            onEnable={remote.enableSshAccess}
            platform={platform}
            rpcAvailable={rpcAvailable}
            ssh={remote.access?.ssh}
          />

          {isOn ? (
            <div className='settings-remote-connect-sections'>
              <section
                aria-label='Connect a Phone'
                className='settings-remote-qr-block settings-remote-easy-connect-qr-block'
              >
                {easyConnectCode ? (
                  <QrCode
                    alt='Easy Connect pairing code'
                    className='settings-remote-qr'
                    size={144}
                    value={easyConnectCode.payload}
                  />
                ) : (
                  <span className='settings-remote-qr settings-remote-qr-pending' data-slot='qr-code'>
                    <span className='settings-management-detail'>Waiting for the address…</span>
                  </span>
                )}
                <div className='settings-remote-qr-meta'>
                  <strong>1. Connect a Phone</strong>
                  <span>
                    On your phone, open Ghostex → <em>Connect your computer</em> → <em>Scan code</em> and scan this QR.
                  </span>
                  {easyConnectCode ? (
                    <span>
                      Pairs as <strong>{easyConnectCode.code.user}</strong> on{' '}
                      <strong>{easyConnectCode.code.name}</strong>. Nothing to type on the phone.
                    </span>
                  ) : null}
                  <span>The QR refreshes after pairing. Remove a paired phone below to disconnect it.</span>
                </div>
              </section>
              <section aria-label='Connect a Remote machine' className='settings-remote-computer-connect'>
                <div className='settings-remote-qr-meta'>
                  <strong>2. Connect a Remote machine</strong>
                  <span>To add this computer on another computer, copy its Easy Connect code below.</span>
                  <ol className='settings-remote-connect-instructions'>
                    <li>On the other computer, open Ghostex → Settings → Remote → Add a machine.</li>
                    <li>
                      Choose <strong>Easy Connect code</strong> and paste the copied code.
                    </li>
                    <li>
                      Confirm this computer’s SSH username
                      {easyConnectCode ? (
                        <>
                          {' '}
                          (<strong>{easyConnectCode.code.user}</strong>)
                        </>
                      ) : null}{' '}
                      and enter its SSH password. If you use an SSH key, choose it under Advanced instead.
                    </li>
                    <li>
                      Click <strong>Add machine</strong>, then open it from the sidebar.
                    </li>
                  </ol>
                  {easyConnectCode ? (
                    <div className='settings-remote-qr-actions'>
                      <RemoteCopyButton
                        className='settings-remote-copy-code-button'
                        copyLabel='Copy Easy Connect code for another computer'
                        size='sm'
                        value={easyConnectCode.payload}
                        variant='default'
                      >
                        Copy Easy Connect code
                      </RemoteCopyButton>
                    </div>
                  ) : null}
                </div>
              </section>
            </div>
          ) : (
            <div className='settings-remote-off-block'>
              <span aria-hidden='true' className='settings-remote-qr settings-remote-qr-placeholder' />
              <div className='settings-remote-off-text'>
                <strong>Turn on Easy Connect to get a pairing code</strong>
                <span className='settings-management-detail'>
                  Ghostex keeps it running while the app is open. You can turn it off any time; paired phones and
                  computers stop being able to reach this computer.
                </span>
                <Button
                  className='settings-remote-turn-on-button'
                  disabled={!rpcAvailable || !status || !binaryFound}
                  onClick={() => remote.setEasyConnectState({ enabled: true, kind: 'setEnabled' })}
                  size='sm'
                  type='button'
                >
                  <IconPower aria-hidden='true' data-icon='inline-start' />
                  Turn on Easy Connect
                </Button>
              </div>
            </div>
          )}

          {status?.lastError ? (
            <div className='settings-remote-error' role='alert'>
              <IconAlertTriangle aria-hidden='true' />
              <span>{status.lastError}</span>
            </div>
          ) : null}

          {isOn ? (
            <PairedDevicesList
              devices={remote.pairedDevices}
              onRemove={remote.removePairedDevice}
              removingDeviceId={remote.removingDeviceId}
              rpcAvailable={rpcAvailable}
            />
          ) : null}
        </div>
      ) : null}
    </section>
  );
}
