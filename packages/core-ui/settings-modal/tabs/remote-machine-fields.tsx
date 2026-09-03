import { cn } from '@/packages/components/utils';
import { Field, FieldDescription, FieldGroup, FieldLabel } from '@/packages/components/ui/field';
import { Switch } from '@/packages/components/ui/switch';
import { IconDeviceFloppy } from '@tabler/icons-react';
import { readPairingCode } from '../../../shared/ghostex-remote-pairing';
import {
  normalizeRemoteMachineSettings,
  type RemoteMachineSettings,
  type RemoteMachineTransport,
} from '../../../shared/ghostex-settings';
import { SettingButton, SettingsInput } from '../fields';

export type RemoteMachineDraft = {
  id: string;
  name: string;
  /**
   * CDXC:RemotePairing 2026-09-03:
   * `ssh` machines are typed in as host/user/port. `easyConnect` machines are
   * pasted in as a pairing code: the code fills the name, user, and address,
   * and the desktop dials the address through its loopback forwarder, so the
   * SSH host and port fields do not apply. Credentials (identity file or a
   * saved password) are still the user's own, exactly like an SSH machine.
   */
  transport: RemoteMachineTransport;
  /** The pasted pairing code, kept only so the field can show what was accepted. */
  easyConnectCode: string;
  easyConnectAddress: string;
  sshHost: string;
  sshIdentityFile: string;
  sshPassword: string;
  sshPasswordSaved: boolean;
  sshPort: string;
  sshUser: string;
  wslDistribution: string;
  disabled: boolean;
};

export function createRemoteMachineDraft(): RemoteMachineDraft {
  return {
    id: `remote-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`,
    name: '',
    transport: 'ssh',
    easyConnectCode: '',
    easyConnectAddress: '',
    sshHost: '',
    sshIdentityFile: '',
    sshPassword: '',
    sshPasswordSaved: false,
    sshPort: '',
    sshUser: '',
    wslDistribution: '',
    disabled: false,
  };
}

export function createRemoteMachineDraftFromSettings(
  machine: RemoteMachineSettings,
  sshPassword = ''
): RemoteMachineDraft {
  return {
    id: machine.id,
    name: machine.name,
    transport: machine.transport === 'easyConnect' ? 'easyConnect' : 'ssh',
    easyConnectCode: '',
    easyConnectAddress: machine.easyConnectAddress ?? '',
    sshHost: machine.sshHost,
    sshIdentityFile: machine.sshIdentityFile ?? '',
    sshPassword,
    sshPasswordSaved: machine.sshPasswordSaved === true,
    sshPort: machine.sshPort ? String(machine.sshPort) : '',
    sshUser: machine.sshUser ?? '',
    wslDistribution: machine.wslDistribution ?? '',
    disabled: machine.disabled === true,
  };
}

export function applyRemoteMachineDraftPatch(
  draft: RemoteMachineDraft,
  patch: Partial<RemoteMachineDraft>
): RemoteMachineDraft {
  return {
    ...draft,
    name: patch.name !== undefined ? patch.name : draft.name,
    transport: patch.transport !== undefined ? patch.transport : draft.transport,
    easyConnectCode: patch.easyConnectCode !== undefined ? patch.easyConnectCode : draft.easyConnectCode,
    easyConnectAddress: patch.easyConnectAddress !== undefined ? patch.easyConnectAddress : draft.easyConnectAddress,
    sshHost: patch.sshHost !== undefined ? patch.sshHost : draft.sshHost,
    sshIdentityFile: patch.sshIdentityFile !== undefined ? patch.sshIdentityFile : draft.sshIdentityFile,
    sshPassword: patch.sshPassword !== undefined ? patch.sshPassword : draft.sshPassword,
    sshPasswordSaved: patch.sshPasswordSaved !== undefined ? patch.sshPasswordSaved : draft.sshPasswordSaved,
    sshPort: patch.sshPort !== undefined ? patch.sshPort : draft.sshPort,
    sshUser: patch.sshUser !== undefined ? patch.sshUser : draft.sshUser,
    wslDistribution: patch.wslDistribution !== undefined ? patch.wslDistribution : draft.wslDistribution,
    disabled: patch.disabled !== undefined ? patch.disabled : draft.disabled,
  };
}

export type EasyConnectCodeReading =
  | { kind: 'empty' }
  | { kind: 'accepted'; address: string; name?: string; user?: string; sshPort?: number; summary: string }
  | { kind: 'rejected'; reason: string };

/**
 * Inline validation for the pasted Easy Connect code. A structured
 * `ghostex-ec1:` code carries the computer's name and user; the bare `tc…`
 * address from Advanced → "Pairing address" is accepted too and the user types
 * the rest. A Tailscale code is not an Easy Connect code and is refused.
 */
export function readEasyConnectCodeInput(input: string): EasyConnectCodeReading {
  if (input.trim().length === 0) {
    return { kind: 'empty' };
  }
  const result = readPairingCode(input);
  if (!result) {
    return { kind: 'rejected', reason: 'That is not an Easy Connect code. Copy it from the other computer as text.' };
  }
  if (result.kind === 'tailscale') {
    return {
      kind: 'rejected',
      reason: 'That is a Tailscale code. Use SSH details with its Tailscale name or IP instead.',
    };
  }
  if (result.kind === 'legacyAddress') {
    return {
      kind: 'accepted',
      address: result.address,
      summary:
        "Looks like a pairing address. Enter the name and user of that computer below. You still need that computer's SSH login.",
    };
  }
  const { code } = result;
  return {
    kind: 'accepted',
    address: code.address,
    name: code.name,
    user: code.user,
    sshPort: code.sshPort,
    summary: `Looks like a pairing code for ${code.user} on ${code.name}. You still need that computer's SSH login.`,
  };
}

/** The draft patch a pasted code produces: address plus the name/user/port it carries, when it carries them. */
export function easyConnectCodeDraftPatch(input: string, draft: RemoteMachineDraft): Partial<RemoteMachineDraft> {
  const reading = readEasyConnectCodeInput(input);
  if (reading.kind !== 'accepted') {
    return { easyConnectCode: input, easyConnectAddress: '' };
  }
  return {
    easyConnectCode: input,
    easyConnectAddress: reading.address,
    ...(reading.name && draft.name.trim().length === 0 ? { name: reading.name } : {}),
    ...(reading.user ? { sshUser: reading.user } : {}),
    ...(reading.sshPort !== undefined ? { sshPort: String(reading.sshPort) } : {}),
  };
}

export function RemoteMachineFields({
  draft,
  identityDescription,
  onChange,
  onPasswordSave,
  passwordSaveDisabled = false,
  passwordDescription,
  showSidebarVisibility = false,
}: {
  draft: RemoteMachineDraft;
  identityDescription?: string;
  onChange: (patch: Partial<RemoteMachineDraft>) => void;
  onPasswordSave?: () => void;
  passwordSaveDisabled?: boolean;
  passwordDescription?: string;
  showSidebarVisibility?: boolean;
}) {
  const showPasswordSaveButton = typeof onPasswordSave === 'function';
  const canSavePassword =
    !passwordSaveDisabled && showPasswordSaveButton && (draft.sshPassword.trim().length > 0 || draft.sshPasswordSaved);
  const easyConnect = draft.transport === 'easyConnect';
  const codeReading = easyConnect
    ? draft.easyConnectCode.trim().length > 0
      ? readEasyConnectCodeInput(draft.easyConnectCode)
      : draft.easyConnectAddress
        ? ({ kind: 'accepted', address: draft.easyConnectAddress, summary: 'Paired through Easy Connect.' } as const)
        : ({ kind: 'empty' } as const)
    : undefined;
  return (
    <FieldGroup className='settings-remote-machine-fields'>
      {showSidebarVisibility ? (
        <Field
          className='settings-remote-machine-field settings-remote-machine-sidebar-visibility'
          orientation='horizontal'
        >
          <div className='min-w-0 flex-1'>
            <FieldLabel className='settings-remote-machine-field-label'>Show in sidebar</FieldLabel>
            <FieldDescription className='settings-remote-machine-field-description'>
              Turn off to hide this machine from the sidebar without deleting it.
            </FieldDescription>
          </div>
          <Switch
            aria-label='Show remote machine in the sidebar'
            checked={!draft.disabled}
            onCheckedChange={(checked) => onChange({ disabled: !checked })}
          />
        </Field>
      ) : null}
      {easyConnect ? (
        <Field className='settings-remote-machine-field settings-remote-machine-easy-connect-code'>
          <FieldLabel className='settings-remote-machine-field-label'>Easy Connect code</FieldLabel>
          <SettingsInput
            aria-label='Remote machine Easy Connect code'
            autoComplete='off'
            className='settings-remote-machine-input'
            maxLength={4000}
            onChange={(event) => onChange(easyConnectCodeDraftPatch(event.currentTarget.value, draft))}
            placeholder='Paste the code copied from the other computer'
            spellCheck={false}
            value={draft.easyConnectCode}
          />
          <FieldDescription
            className={cn(
              'settings-remote-machine-field-description',
              codeReading?.kind === 'rejected' && 'settings-remote-machine-code-rejected',
              codeReading?.kind === 'accepted' && 'settings-remote-machine-code-accepted'
            )}
            role={codeReading?.kind === 'rejected' ? 'alert' : undefined}
          >
            {codeReading?.kind === 'accepted'
              ? codeReading.summary
              : codeReading?.kind === 'rejected'
                ? codeReading.reason
                : 'On the other computer, open Settings → Remote → Easy Connect and use "Copy as text".'}
          </FieldDescription>
        </Field>
      ) : null}
      <Field className='settings-remote-machine-field'>
        <FieldLabel className='settings-remote-machine-field-label'>Name</FieldLabel>
        <SettingsInput
          aria-label='Remote machine name'
          className='settings-remote-machine-input'
          maxLength={80}
          onChange={(event) => onChange({ name: event.currentTarget.value })}
          placeholder='Machine one'
          value={draft.name}
        />
      </Field>
      {easyConnect ? null : (
        <Field className='settings-remote-machine-field'>
          <FieldLabel className='settings-remote-machine-field-label'>SSH host</FieldLabel>
          <SettingsInput
            aria-label='Remote machine SSH host'
            className='settings-remote-machine-input'
            maxLength={200}
            onChange={(event) => onChange({ sshHost: event.currentTarget.value })}
            placeholder='100.77.81.4'
            value={draft.sshHost}
          />
        </Field>
      )}
      <div className={cn('settings-remote-machine-user-port', easyConnect && 'settings-remote-machine-user-only')}>
        <Field className='settings-remote-machine-field'>
          <FieldLabel className='settings-remote-machine-field-label'>SSH user</FieldLabel>
          <SettingsInput
            aria-label='Remote machine SSH user'
            className='settings-remote-machine-input'
            maxLength={120}
            onChange={(event) => onChange({ sshUser: event.currentTarget.value })}
            placeholder='machine username'
            value={draft.sshUser}
          />
        </Field>
        {easyConnect ? null : (
          <Field className='settings-remote-machine-field'>
            <FieldLabel className='settings-remote-machine-field-label'>SSH port</FieldLabel>
            <SettingsInput
              aria-label='Remote machine SSH port'
              className='settings-remote-machine-input'
              inputMode='numeric'
              maxLength={5}
              onChange={(event) => onChange({ sshPort: event.currentTarget.value.replace(/[^0-9]/gu, '') })}
              placeholder='22'
              value={draft.sshPort}
            />
          </Field>
        )}
      </div>
      <Field className='settings-remote-machine-field'>
        <FieldLabel className='settings-remote-machine-field-label'>Identity file</FieldLabel>
        <SettingsInput
          aria-label='Remote machine SSH identity file'
          className='settings-remote-machine-input'
          maxLength={500}
          onChange={(event) => onChange({ sshIdentityFile: event.currentTarget.value })}
          placeholder='~/.ssh/id_ed25519'
          value={draft.sshIdentityFile}
        />
        <FieldDescription className='settings-remote-machine-field-description'>
          {identityDescription ?? 'Provide either an SSH identity file or save an SSH password below.'}
        </FieldDescription>
      </Field>
      <Field className='settings-remote-machine-field'>
        <FieldLabel className='settings-remote-machine-field-label'>Windows WSL distribution</FieldLabel>
        <SettingsInput
          aria-label='Remote machine WSL distribution'
          className='settings-remote-machine-input'
          maxLength={120}
          onChange={(event) => onChange({ wslDistribution: event.currentTarget.value })}
          placeholder='Ubuntu-24.04'
          value={draft.wslDistribution}
        />
        <FieldDescription className='settings-remote-machine-field-description'>
          Optional. Windows remotes run gxserver inside this WSL2 distribution; leave blank to use the default
          distribution.
        </FieldDescription>
      </Field>
      <Field className='settings-remote-machine-field'>
        <FieldLabel className='settings-remote-machine-field-label'>Password</FieldLabel>
        <div
          className={cn(
            'settings-remote-machine-password-row',
            !showPasswordSaveButton && 'settings-remote-machine-password-row-single'
          )}
        >
          <SettingsInput
            aria-label='Remote machine SSH password'
            autoComplete='off'
            className='settings-remote-machine-input'
            maxLength={500}
            onChange={(event) => onChange({ sshPassword: event.currentTarget.value })}
            placeholder={draft.sshPasswordSaved ? 'Saved in Keychain' : 'SSH password'}
            type='password'
            value={draft.sshPassword}
          />
          {showPasswordSaveButton ? (
            <SettingButton
              aria-label='Save SSH password'
              disabled={!canSavePassword}
              disabledReason={
                passwordSaveDisabled
                  ? 'Password saving needs the Ghostex app connection.'
                  : 'Enter a password to save first.'
              }
              onClick={onPasswordSave}
              size='icon-sm'
              type='button'
              variant='secondary'
            >
              <IconDeviceFloppy aria-hidden='true' />
            </SettingButton>
          ) : null}
        </div>
        <FieldDescription className='settings-remote-machine-field-description'>
          {passwordDescription ??
            'Passwords are stored in macOS Keychain. Leave blank and press Save to remove a saved password.'}
        </FieldDescription>
      </Field>
    </FieldGroup>
  );
}

export function normalizeRemoteMachineDraft(
  draft: RemoteMachineDraft & { id: string }
): RemoteMachineSettings | undefined {
  const wslDistribution = draft.wslDistribution.trim();
  if (
    wslDistribution &&
    (wslDistribution.startsWith('-') || !/^[A-Za-z0-9][A-Za-z0-9._+() -]*$/u.test(wslDistribution))
  ) {
    return undefined;
  }
  if (draft.transport === 'easyConnect' && !draft.easyConnectAddress) {
    return undefined;
  }
  return normalizeRemoteMachineSettings([
    {
      id: draft.id,
      name: draft.name,
      transport: draft.transport,
      easyConnectAddress: draft.easyConnectAddress,
      sshHost: draft.transport === 'easyConnect' ? '' : draft.sshHost,
      sshIdentityFile: draft.sshIdentityFile,
      sshPasswordSaved: draft.sshPasswordSaved,
      sshPort: draft.sshPort ? Number(draft.sshPort) : undefined,
      sshUser: draft.sshUser,
      wslDistribution,
      disabled: draft.disabled,
    },
  ])[0];
}

/** The card's how-line: the SSH target for SSH machines, "Easy Connect" for paired ones. */
export function formatRemoteMachineSshTarget(machine: RemoteMachineSettings): string {
  if (machine.transport === 'easyConnect') {
    return machine.sshUser ? `Easy Connect · ${machine.sshUser}` : 'Easy Connect';
  }
  const host = machine.sshUser ? `${machine.sshUser}@${machine.sshHost}` : machine.sshHost;
  return machine.sshPort ? `${host}:${machine.sshPort}` : host;
}
