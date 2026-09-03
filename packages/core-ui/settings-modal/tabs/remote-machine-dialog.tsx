import { useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/packages/components/ui/dialog';
import { SegmentedControl, SegmentedControlItem } from '@/packages/components/ui/segmented-control';
import { IconDownload, IconRefresh, IconTrash } from '@tabler/icons-react';
import type { RemoteMachineSettings, RemoteMachineTransport } from '../../../shared/ghostex-settings';
import { type WebviewApi } from '../../webview-api';
import { SettingButton } from '../fields';
import {
  RemoteMachineFields,
  createRemoteMachineDraft,
  createRemoteMachineDraftFromSettings,
  normalizeRemoteMachineDraft,
  readEasyConnectCodeInput,
  type RemoteMachineDraft,
} from './remote-machine-fields';

export type RemoteGxserverInstallState = { installed: boolean; version?: string };

/**
 * CDXC:RemotePairing 2026-09-03 DECISION:
 * User: clicking a machine card shows "that machine's details as a pop up in settings so i can edit it".
 * The pop-up is this dialog; it owns the draft while open and writes Settings.remoteMachines only on Save, so a half-typed host can no longer rewrite the saved machine on every keystroke (the old inline cards guarded that with a draft-per-machine map that is gone now).
 * Passwords keep their one-shot Keychain path: the save icon posts immediately, and Save flushes a password still sitting in the field.
 */
export function RemoteMachineDialog({
  gxserverInstall,
  machine,
  onClose,
  onRemove,
  onSave,
  onSavePassword,
  open,
  vscode,
}: {
  gxserverInstall?: RemoteGxserverInstallState;
  /** The saved machine being edited; absent for "Add a machine". */
  machine?: RemoteMachineSettings;
  onClose: () => void;
  onRemove: (machineId: string) => void;
  onSave: (machine: RemoteMachineSettings, password: string) => void;
  onSavePassword: (machineId: string, password: string) => void;
  open: boolean;
  vscode?: WebviewApi;
}) {
  return (
    <Dialog
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          onClose();
        }
      }}
      open={open}
    >
      {open ? (
        <RemoteMachineDialogContent
          gxserverInstall={gxserverInstall}
          key={machine?.id ?? 'add'}
          machine={machine}
          onClose={onClose}
          onRemove={onRemove}
          onSave={onSave}
          onSavePassword={onSavePassword}
          vscode={vscode}
        />
      ) : null}
    </Dialog>
  );
}

function RemoteMachineDialogContent({
  gxserverInstall,
  machine,
  onClose,
  onRemove,
  onSave,
  onSavePassword,
  vscode,
}: {
  gxserverInstall?: RemoteGxserverInstallState;
  machine?: RemoteMachineSettings;
  onClose: () => void;
  onRemove: (machineId: string) => void;
  onSave: (machine: RemoteMachineSettings, password: string) => void;
  onSavePassword: (machineId: string, password: string) => void;
  vscode?: WebviewApi;
}) {
  const isNew = machine === undefined;
  const [draft, setDraft] = useState<RemoteMachineDraft>(() =>
    machine ? createRemoteMachineDraftFromSettings(machine) : createRemoteMachineDraft()
  );

  /*
   * CDXC:RemotePairing 2026-09-03:
   * The add form has two entry modes. SSH details need a name and host; an
   * Easy Connect code needs a name and an accepted code (the code usually
   * brings the name with it). Switching modes keeps the shared fields (name,
   * user, identity file, password, WSL) and drops only the other mode's
   * address. A saved machine keeps its transport.
   */
  const setTransport = (transport: RemoteMachineTransport) => {
    setDraft((current) => ({
      ...current,
      transport,
      ...(transport === 'easyConnect' ? { sshHost: '', sshPort: '' } : { easyConnectCode: '', easyConnectAddress: '' }),
    }));
  };

  const codeReading = draft.transport === 'easyConnect' ? readEasyConnectCodeInput(draft.easyConnectCode) : undefined;
  const normalizedMachine = normalizeRemoteMachineDraft(draft);
  const canSave = normalizedMachine !== undefined;
  const saveDisabledReason =
    draft.transport === 'easyConnect'
      ? codeReading?.kind === 'accepted' || draft.easyConnectAddress.length > 0
        ? 'Enter a machine name first.'
        : 'Paste a valid Easy Connect code first.'
      : draft.name.trim().length > 0 && draft.sshHost.trim().length > 0
        ? 'Check the WSL distribution name.'
        : 'Enter a machine name and SSH host first.';
  const machineHasEndpoint =
    draft.transport === 'easyConnect' ? Boolean(draft.easyConnectAddress) : draft.sshHost.trim().length > 0;
  const gxserverInstalled = gxserverInstall?.installed === true;

  const savePassword = () => {
    if (!machine) {
      return;
    }
    const password = draft.sshPassword;
    if (!password && machine.sshPasswordSaved !== true) {
      return;
    }
    onSavePassword(machine.id, password);
    setDraft((current) => ({ ...current, sshPassword: '', sshPasswordSaved: password.trim().length > 0 }));
  };

  const save = () => {
    if (!normalizedMachine) {
      return;
    }
    onSave(normalizedMachine, draft.sshPassword);
  };

  /*
   * The dialog portals to <body>, outside the settings modal's `.ghostex-settings-shadcn` scope, so it re-declares the class to pick up the settings surface, input, and radius tokens.
   *
   * CDXC:RemotePairing 2026-09-03 DECISION:
   * User: "please add a dark overlay on the settings main area and sidebar (fully) and make clicking on this overlay close the machine editing pop up".
   * `nested` gives this dialog its own full-viewport backdrop (Base UI skips the backdrop of a dialog opened inside the Settings dialog), which in the native Settings window covers the sidebar, search, and main area alike.
   * Outside-click and Escape go through the Dialog's onOpenChange above, which calls onClose.
   */
  return (
    <DialogContent className='ghostex-settings-shadcn settings-remote-machine-dialog' nested showCloseButton={false}>
      <DialogHeader>
        <DialogTitle>{isNew ? 'Add a machine' : machine.name}</DialogTitle>
        <DialogDescription>
          {isNew
            ? 'Another computer this one connects to. Its projects show up as a separate sidebar section.'
            : 'Connection details for this machine. Changes apply when you save.'}
        </DialogDescription>
      </DialogHeader>
      <div className='settings-remote-machine-dialog-body'>
        {isNew ? (
          <SegmentedControl
            aria-label='How to add the machine'
            className='settings-remote-machine-add-mode'
            onValueChange={(value) => setTransport(value === 'easyConnect' ? 'easyConnect' : 'ssh')}
            size='sm'
            stretch
            value={draft.transport}
          >
            <SegmentedControlItem value='ssh'>SSH details</SegmentedControlItem>
            <SegmentedControlItem value='easyConnect'>Easy Connect code</SegmentedControlItem>
          </SegmentedControl>
        ) : null}
        <RemoteMachineFields
          draft={draft}
          identityDescription={isNew ? 'Provide either an SSH identity file now or an SSH password below.' : undefined}
          onChange={(patch) => setDraft((current) => ({ ...current, ...patch }))}
          onPasswordSave={isNew ? undefined : savePassword}
          passwordDescription={
            isNew
              ? 'Passwords are stored in macOS Keychain. Leave blank to add the machine without a saved password.'
              : undefined
          }
          passwordSaveDisabled={!vscode}
          showSidebarVisibility={!isNew}
        />
        {isNew ? null : (
          <div className='settings-management-actions settings-remote-machine-install-actions'>
            {/*
             * CDXC:RemoteMachines 2026-06-23-08:30:
             * Remote Settings needs a direct gxserver install action for
             * first-run Ubuntu SSH machines. Reuse the reconnect flow so
             * native opens the approval modal only after SSH proves
             * gxserver is missing, and otherwise connects the existing
             * remote daemon without reinstalling it.
             */}
            {gxserverInstalled ? (
              <span className='settings-remote-machine-installed-version'>
                {gxserverInstall?.version ? `gxserver ${gxserverInstall.version}` : 'gxserver installed'}
              </span>
            ) : null}
            <SettingButton
              disabled={!vscode || !machineHasEndpoint}
              disabledReason={
                !machineHasEndpoint
                  ? draft.transport === 'easyConnect'
                    ? 'Paste a valid Easy Connect code first.'
                    : 'Enter an SSH host first.'
                  : 'This action needs the Ghostex app connection.'
              }
              onClick={() => {
                vscode?.postMessage({
                  remoteMachineId: machine.id,
                  type: 'reconnectRemoteMachine',
                });
              }}
              type='button'
              variant='secondary'
            >
              {gxserverInstalled ? <IconRefresh aria-hidden='true' /> : <IconDownload aria-hidden='true' />}
              {gxserverInstalled ? 'Update gxserver' : 'Install / Connect gxserver'}
            </SettingButton>
          </div>
        )}
      </div>
      <DialogFooter className='settings-remote-machine-dialog-footer'>
        {isNew ? null : (
          <Button
            className='settings-remote-machine-dialog-remove'
            onClick={() => onRemove(machine.id)}
            type='button'
            variant='destructive'
          >
            <IconTrash aria-hidden='true' />
            Remove
          </Button>
        )}
        <Button onClick={onClose} type='button' variant='outline'>
          Cancel
        </Button>
        <SettingButton disabled={!canSave} disabledReason={saveDisabledReason} onClick={save} type='button'>
          {isNew ? 'Add machine' : 'Save'}
        </SettingButton>
      </DialogFooter>
    </DialogContent>
  );
}
