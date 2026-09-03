import { useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/packages/components/ui/popover';
import {
  IconAlertTriangle,
  IconBrandApple,
  IconBrandUbuntu,
  IconBrandWindows,
  IconCircleCheckFilled,
  IconLoader2,
  IconX,
} from '@tabler/icons-react';
import type { GxserverRemoteAccessPlatform, GxserverRemoteSshAccessStatus } from '@/packages/shared/gxserver-protocol';
import { cn } from '@/packages/components/utils';
import { SSH_ACCESS_INSTRUCTIONS, SSH_ACCESS_PLATFORMS, SSH_ACCESS_PLATFORM_LABELS } from './ssh-access-instructions';
import type { SshEnableAttempt } from './use-remote-access';

const PLATFORM_ICONS: Record<GxserverRemoteAccessPlatform, typeof IconBrandApple> = {
  linux: IconBrandUbuntu,
  macos: IconBrandApple,
  windows: IconBrandWindows,
};

/**
 * CDXC:RemotePairing 2026-09-03:
 * Three OS buttons, the current OS marked, each opening a small popover with
 * that OS's manual steps. Shown once Ghostex's own enable attempt was
 * cancelled or failed: doing it by hand is the designed second route, so the
 * buttons carry the same steps the mobile app shows in its help sheet.
 */
export function SshAccessInstructionButtons({
  currentPlatform,
  className,
}: {
  currentPlatform: GxserverRemoteAccessPlatform | undefined;
  className?: string;
}) {
  const [openPlatform, setOpenPlatform] = useState<GxserverRemoteAccessPlatform>();
  return (
    <div className={cn('settings-remote-os-buttons', className)} data-current={currentPlatform}>
      {SSH_ACCESS_PLATFORMS.map((platform) => {
        const Icon = PLATFORM_ICONS[platform];
        const instructions = SSH_ACCESS_INSTRUCTIONS[platform];
        const isCurrent = platform === currentPlatform;
        return (
          <Popover
            key={platform}
            onOpenChange={(open) => setOpenPlatform(open ? platform : undefined)}
            open={openPlatform === platform}
          >
            <PopoverTrigger
              render={
                <Button
                  aria-label={instructions.title}
                  className='settings-remote-os-button'
                  data-current={isCurrent || undefined}
                  data-platform={platform}
                  size='xs'
                  type='button'
                  variant={isCurrent ? 'secondary' : 'ghost'}
                />
              }
            >
              <Icon aria-hidden='true' data-icon='inline-start' />
              {SSH_ACCESS_PLATFORM_LABELS[platform]}
            </PopoverTrigger>
            <PopoverContent
              align='start'
              className='settings-remote-os-popover w-80 max-w-[calc(100vw-2rem)] gap-3 p-4'
              onOpenAutoFocus={(event) => event.preventDefault()}
              side='bottom'
              sideOffset={6}
            >
              <div className='settings-remote-os-popover-head'>
                <span className='settings-remote-os-popover-title'>
                  <Icon aria-hidden='true' size={16} />
                  {instructions.title}
                </span>
                <Button
                  aria-label='Close'
                  onClick={() => setOpenPlatform(undefined)}
                  size='icon-xs'
                  type='button'
                  variant='ghost'
                >
                  <IconX aria-hidden='true' />
                </Button>
              </div>
              <ol className='settings-remote-os-popover-steps'>
                {instructions.steps.map((step, index) => (
                  <li className='settings-remote-step' key={step}>
                    <span className='settings-remote-step-number'>{index + 1}</span>
                    <span className='settings-remote-step-title'>{step}</span>
                  </li>
                ))}
              </ol>
              <p className='settings-management-detail'>{instructions.note}</p>
            </PopoverContent>
          </Popover>
        );
      })}
    </div>
  );
}

/**
 * The "SSH access is on / off" row shared by the Easy Connect card and the
 * Tailscale card's step 2. Off state: one button that asks gxserver to enable
 * it (one admin prompt) and the per-OS "by hand" buttons underneath, since
 * doing it by hand is the designed second route, not a fallback. A step that
 * already carries its own title and detail passes `compact` to render only
 * the actions.
 */
export function SshAccessRow({
  attempt,
  className,
  compact = false,
  detailWhenOff,
  detailWhenOn,
  isEnabling,
  onEnable,
  platform,
  rpcAvailable,
  ssh,
}: {
  attempt: SshEnableAttempt | undefined;
  className?: string;
  /** Render only the state line, button, and OS buttons (the parent owns title + detail). */
  compact?: boolean;
  detailWhenOff: string;
  detailWhenOn: string;
  isEnabling: boolean;
  onEnable: () => void;
  platform: GxserverRemoteAccessPlatform | undefined;
  rpcAvailable: boolean;
  ssh: GxserverRemoteSshAccessStatus | undefined;
}) {
  if (!ssh) {
    return (
      <div className={cn('settings-remote-ssh-row', className)} data-state='unknown'>
        <span className='settings-remote-ssh-title'>
          <IconLoader2 aria-hidden='true' className='settings-remote-spinner' size={16} />
          Checking SSH access…
        </span>
      </div>
    );
  }
  if (ssh.enabled) {
    return (
      <div className={cn('settings-remote-ssh-row', className)} data-state='on'>
        <span className='settings-remote-ssh-title'>
          <IconCircleCheckFilled aria-hidden='true' className='settings-remote-ok' size={16} />
          SSH access is on
        </span>
        <span className='settings-management-detail'>{detailWhenOn}</span>
      </div>
    );
  }
  return (
    <div className={cn('settings-remote-ssh-row', className)} data-compact={compact || undefined} data-state='off'>
      {compact ? null : (
        <>
          <span className='settings-remote-ssh-title'>
            <IconAlertTriangle aria-hidden='true' className='settings-remote-warn' size={16} />
            SSH access is off
          </span>
          <span className='settings-management-detail'>{detailWhenOff}</span>
        </>
      )}
      <div className='settings-remote-ssh-actions'>
        <Button
          className='settings-remote-enable-ssh-button'
          disabled={!rpcAvailable || isEnabling}
          onClick={onEnable}
          size='xs'
          title={rpcAvailable ? undefined : 'This action needs the Ghostex server connection.'}
          type='button'
        >
          {isEnabling ? <IconLoader2 aria-hidden='true' className='settings-remote-spinner' /> : null}
          {isEnabling ? 'Waiting for the admin prompt…' : 'Turn on SSH access'}
        </Button>
      </div>
      <span className='settings-management-detail settings-remote-ssh-attempt' data-outcome={attempt?.outcome}>
        {attempt === undefined
          ? 'Or do it by hand:'
          : attempt.outcome === 'cancelled'
            ? 'The admin prompt was cancelled. Or do it by hand:'
            : `${attempt.message ?? 'Ghostex could not turn on SSH access.'} Or do it by hand:`}
      </span>
      <SshAccessInstructionButtons currentPlatform={platform} />
    </div>
  );
}
