import { useState, type ReactNode } from 'react';
import { Button } from '@/packages/components/ui/button';
import { QrCode } from '@/packages/components/ui/qr-code';
import { IconCheck, IconChevronDown, IconChevronRight, IconShield } from '@tabler/icons-react';
import { RemoteCopyButton } from './remote-copy-button';
import { SshAccessRow } from './remote-ssh-access-row';
import type { RemoteAccessState } from './use-remote-access';

function TailscaleStep({
  children,
  detail,
  done,
  number,
  title,
}: {
  children?: ReactNode;
  detail: ReactNode;
  done: boolean;
  number: number;
  title: string;
}) {
  return (
    <li className='settings-remote-step' data-done={done || undefined}>
      <span className='settings-remote-step-number'>{done ? <IconCheck aria-hidden='true' size={12} /> : number}</span>
      <div className='settings-remote-step-body'>
        <span className='settings-remote-step-title'>{title}</span>
        <span className='settings-management-detail'>{detail}</span>
        {children}
      </div>
    </li>
  );
}

function ManualValueRow({ label, value }: { label: string; value: string }) {
  return (
    <div className='settings-remote-kv'>
      <span className='settings-remote-kv-key'>{label}</span>
      <span className='settings-remote-kv-value'>
        <code>{value}</code>
        <RemoteCopyButton copyLabel={`Copy ${label.toLowerCase()}`} value={value} />
      </span>
    </div>
  );
}

/**
 * CDXC:RemotePairing 2026-09-03:
 * The Tailscale path card: four steps, the first two self-checking from
 * `/api/remoteAccessStatus`, the last one a QR built from
 * `/api/remotePairingCode.tailscale` that the app recognises by its prefix.
 * Scanning only fills the form on the phone; the connection stays SSH over
 * the tailnet with host, user, and password, so the typed values stay
 * reachable behind "Or type these in".
 */
export function TailscaleCard({ remote, rpcAvailable }: { remote: RemoteAccessState; rpcAvailable: boolean }) {
  const [manualOpen, setManualOpen] = useState(false);
  const access = remote.access;
  const tailscale = access?.tailscale;
  const tailscaleCode = remote.pairingCode?.tailscale;
  const tailscaleRunning = tailscale?.running === true;
  const sshOn = access?.ssh.enabled === true;
  const detectedBadge = !tailscale
    ? undefined
    : tailscaleRunning
      ? { label: 'Detected', tone: 'active' as const }
      : tailscale.installed
        ? { label: 'Not running', tone: 'needsSetup' as const }
        : { label: 'Not installed', tone: 'disabled' as const };
  const host = tailscaleCode?.code.host ?? tailscale?.magicDnsName ?? undefined;
  const ip = tailscaleCode?.code.ip ?? tailscale?.ip ?? undefined;
  const user = tailscaleCode?.code.user ?? access?.username;

  return (
    <section
      className='settings-remote-path-card settings-remote-tailscale-card'
      data-settings-remote-section='tailscale'
      tabIndex={-1}
    >
      <div className='settings-remote-path-head'>
        <span className='settings-remote-path-title'>
          <span className='settings-remote-path-icon'>
            <IconShield aria-hidden='true' size={16} />
          </span>
          <span>Tailscale</span>
        </span>
      </div>
      <div className='settings-remote-badges'>
        <span className='settings-remote-status-badge' data-status='plain'>
          If you already use it
        </span>
        {detectedBadge ? (
          <span className='settings-remote-status-badge' data-status={detectedBadge.tone}>
            {detectedBadge.label}
          </span>
        ) : null}
      </div>
      <p className='settings-management-description'>
        Your phone joins your tailnet and connects over SSH. Nothing to enable here; follow the checklist, then scan the
        code at the end with the app.
      </p>

      <ol className='settings-remote-steps settings-remote-tailscale-steps'>
        <TailscaleStep
          detail={
            !tailscale
              ? 'Checking…'
              : tailscaleRunning
                ? `Signed in as ${tailscale.account ?? 'your account'}${tailscale.magicDnsName ? ' · MagicDNS on' : ''}`
                : tailscale.installed
                  ? 'Tailscale is installed but not connected. Open it and sign in.'
                  : 'Install Tailscale on this computer and sign in.'
          }
          done={tailscaleRunning}
          number={1}
          title='Tailscale is running on this computer'
        />
        <TailscaleStep
          detail={
            sshOn
              ? 'Ghostex checked it just now.'
              : 'Ghostex can enable it; your computer asks for an admin password once.'
          }
          done={sshOn}
          number={2}
          title='Turn on SSH access'
        >
          {sshOn ? null : (
            <SshAccessRow
              attempt={remote.sshEnableAttempt}
              className='settings-remote-tailscale-ssh-row'
              compact
              detailWhenOff='Your phone signs in over SSH.'
              detailWhenOn='Your phone signs in over SSH.'
              isEnabling={remote.isEnablingSsh}
              onEnable={remote.enableSshAccess}
              platform={access?.platform}
              rpcAvailable={rpcAvailable}
              ssh={access?.ssh}
            />
          )}
        </TailscaleStep>
        <TailscaleStep
          detail='Sign in to the same account, then make sure it shows Connected.'
          done={false}
          number={3}
          title='Install Tailscale on your phone'
        />
        <TailscaleStep
          detail='Fills in the name, address and username. The app then asks for your computer password once and saves it on the phone.'
          done={false}
          number={4}
          title='In the Ghostex app, scan this code'
        >
          <div className='settings-remote-qr-block settings-remote-tailscale-qr-block'>
            {tailscaleCode ? (
              <QrCode
                alt='Tailscale connection code'
                className='settings-remote-qr'
                size={140}
                value={tailscaleCode.payload}
              />
            ) : (
              <span
                className='settings-remote-qr settings-remote-qr-pending'
                data-slot='qr-code'
                style={{ height: 140, width: 140 }}
              >
                <span className='settings-management-detail'>
                  {tailscaleRunning ? 'Waiting for the address…' : 'Appears once Tailscale is running.'}
                </span>
              </span>
            )}
            <div className='settings-remote-qr-meta'>
              <span className='settings-management-detail'>
                Open the app → <em>Connect your computer</em> → <em>Scan code</em>. The app recognizes this as a
                Tailscale code.
              </span>
              <Button
                aria-expanded={manualOpen}
                className='settings-remote-collapsible-toggle settings-remote-manual-values-toggle'
                onClick={() => setManualOpen((open) => !open)}
                size='xs'
                type='button'
                variant='ghost'
              >
                {manualOpen ? <IconChevronDown aria-hidden='true' /> : <IconChevronRight aria-hidden='true' />}
                Or type these in
              </Button>
              {manualOpen ? (
                <div className='settings-remote-manual-values'>
                  {host ? <ManualValueRow label='Host' value={host} /> : null}
                  {ip ? <ManualValueRow label={host ? 'or IP' : 'IP'} value={ip} /> : null}
                  {user ? <ManualValueRow label='Username' value={user} /> : null}
                  <div className='settings-remote-kv'>
                    <span className='settings-remote-kv-key'>Password</span>
                    <span className='settings-remote-kv-value settings-management-detail'>Your login password</span>
                  </div>
                  {!host && !ip ? (
                    <span className='settings-management-detail'>
                      The host and IP appear once Tailscale is running on this computer.
                    </span>
                  ) : null}
                </div>
              ) : null}
            </div>
          </div>
        </TailscaleStep>
      </ol>
    </section>
  );
}
