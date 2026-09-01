import { useEffect, useRef, useState, type ReactNode } from 'react';
import { cn } from '@/packages/components/utils';
import { Button } from '@/packages/components/ui/button';
import { Card, CardContent, CardTitle } from '@/packages/components/ui/card';
import {
  Popover,
  PopoverContent,
  PopoverDescription,
  PopoverHeader,
  PopoverTitle,
  PopoverTrigger,
} from '@/packages/components/ui/popover';
import { Field, FieldDescription, FieldGroup, FieldLabel } from '@/packages/components/ui/field';
import { Switch } from '@/packages/components/ui/switch';
import {
  IconDeviceDesktop,
  IconDownload,
  IconInfoCircle,
  IconPlus,
  IconRefresh,
  IconDeviceFloppy,
  IconTrash,
} from '@tabler/icons-react';
import { normalizeRemoteMachineSettings, type RemoteMachineSettings } from '../../../shared/ghostex-settings';
import { type WebviewApi } from '../../webview-api';
import { SettingButton, SettingsInput } from '../fields';
import { SettingsTabSearch, hasVisibleSettingsSearchResult, shouldShowSettingsSection } from '../search';
import { TailcatSettingsPanel, type TailcatSettingsRpc } from './remote-tailcat';

export type RemoteMachineDraft = {
  id: string;
  name: string;
  sshHost: string;
  sshIdentityFile: string;
  sshPassword: string;
  sshPasswordSaved: boolean;
  sshPort: string;
  sshUser: string;
  wslDistribution: string;
  disabled: boolean;
};

export const REMOTE_GXSERVER_INSTALL_PROBE_DEBOUNCE_MS = 600;

export function createRemoteMachineDraft(): RemoteMachineDraft {
  return {
    id: `remote-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`,
    name: '',
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

export function RemoteSettingsTab({
  initialRemoteMachineId,
  isActive,
  onChange,
  remoteMachines,
  search,
  searchEmptyState,
  tailcatRpc,
  vscode,
}: {
  initialRemoteMachineId?: string;
  isActive: boolean;
  onChange: (remoteMachines: RemoteMachineSettings[]) => void;
  remoteMachines: RemoteMachineSettings[];
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
  tailcatRpc?: TailcatSettingsRpc;
  vscode?: WebviewApi;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [isTailscaleHelpOpen, setIsTailscaleHelpOpen] = useState(false);
  const [newMachine, setNewMachine] = useState<RemoteMachineDraft>(() => createRemoteMachineDraft());
  const [remoteMachineDraftsById, setRemoteMachineDraftsById] = useState<Record<string, RemoteMachineDraft>>({});
  const [sshPasswordDrafts, setSshPasswordDrafts] = useState<Record<string, string>>({});
  const lastTargetedRemoteMachineIdRef = useRef<string | undefined>(undefined);
  /*
   * CDXC:RemoteMachines 2026-08-19:
   * The saved-machine action reads as Install for a machine without gxserver
   * and as Update for one that already runs it, with the installed version
   * shown on the opposite edge of the same action row. React never inspects the
   * remote machine itself: it asks native for the state of the saved machine id
   * and renders the version string native reports back.
   */
  const [remoteGxserverInstallsById, setRemoteGxserverInstallsById] = useState<
    Record<string, { installed: boolean; version?: string }>
  >({});
  const probedRemoteGxserverKeysRef = useRef<Set<string>>(new Set());

  useEffect(() => {
    const remoteMachineIds = new Set(remoteMachines.map((machine) => machine.id));
    setRemoteMachineDraftsById((drafts) => {
      let next: Record<string, RemoteMachineDraft> | undefined;
      for (const machineId of Object.keys(drafts)) {
        if (!remoteMachineIds.has(machineId)) {
          next ??= { ...drafts };
          delete next[machineId];
        }
      }
      return next ?? drafts;
    });
  }, [remoteMachines]);

  useEffect(() => {
    const handleHostMessage = (event: Event) => {
      const message = (event as CustomEvent<unknown>).detail;
      if (
        !message ||
        typeof message !== 'object' ||
        !('type' in message) ||
        message.type !== 'remoteGxserverInstallState'
      ) {
        return;
      }
      const machineId =
        'remoteMachineId' in message && typeof message.remoteMachineId === 'string'
          ? message.remoteMachineId.trim()
          : '';
      if (!machineId) {
        return;
      }
      const installed = 'installed' in message && message.installed === true;
      const version = 'version' in message && typeof message.version === 'string' ? message.version.trim() : '';
      setRemoteGxserverInstallsById((current) => ({
        ...current,
        [machineId]: { installed, version: version || undefined },
      }));
    };
    window.addEventListener('ghostex-app-modal-host-message', handleHostMessage);
    return () => {
      window.removeEventListener('ghostex-app-modal-host-message', handleHostMessage);
    };
  }, []);

  useEffect(() => {
    if (!isActive || !vscode) {
      return;
    }
    /*
     * Every accepted keystroke in an SSH field rewrites the saved machine, so
     * wait for typing to settle before asking native to open an SSH connection
     * for the probe.
     */
    const timeout = setTimeout(() => {
      for (const machine of remoteMachines) {
        const sshHost = machine.sshHost.trim();
        if (!sshHost) {
          continue;
        }
        /*
         * Probe once per saved SSH target. Editing the host, user, port, or WSL
         * distribution points the action at a different remote, so that target
         * is probed again and the previous answer is dropped instead of
         * labelling a new host with the old machine's version.
         */
        const probeKey = [
          machine.id,
          sshHost,
          machine.sshUser?.trim() ?? '',
          machine.sshPort ?? '',
          machine.wslDistribution?.trim() ?? '',
        ].join('|');
        if (probedRemoteGxserverKeysRef.current.has(probeKey)) {
          continue;
        }
        probedRemoteGxserverKeysRef.current.add(probeKey);
        setRemoteGxserverInstallsById((current) => {
          if (!(machine.id in current)) {
            return current;
          }
          const next = { ...current };
          delete next[machine.id];
          return next;
        });
        vscode.postMessage({ remoteMachineId: machine.id, type: 'probeRemoteGxserverInstall' });
      }
    }, REMOTE_GXSERVER_INSTALL_PROBE_DEBOUNCE_MS);
    return () => clearTimeout(timeout);
  }, [isActive, remoteMachines, vscode]);

  useEffect(() => {
    if (!isActive || !initialRemoteMachineId) {
      if (!isActive) {
        lastTargetedRemoteMachineIdRef.current = undefined;
      }
      return;
    }
    if (lastTargetedRemoteMachineIdRef.current === initialRemoteMachineId) {
      return;
    }
    const animationFrame = requestAnimationFrame(() => {
      const targetCard = Array.from(
        containerRef.current?.querySelectorAll<HTMLElement>('[data-settings-remote-machine-id]') ?? []
      ).find((candidate) => candidate.dataset.settingsRemoteMachineId === initialRemoteMachineId);
      if (!targetCard) {
        return;
      }
      /*
       * CDXC:RemoteMachines 2026-06-10-09:54:
       * Remote machine header Edit should land on the selected saved machine's
       * editable card, not just the generic Remote settings tab. Focus the name
       * field after scrolling because it is the first user-facing machine field.
       */
      targetCard.scrollIntoView({ behavior: 'smooth', block: 'center' });
      targetCard
        .querySelector<HTMLInputElement>("input[aria-label='Remote machine name']")
        ?.focus({ preventScroll: true });
      lastTargetedRemoteMachineIdRef.current = initialRemoteMachineId;
    });
    return () => cancelAnimationFrame(animationFrame);
  }, [initialRemoteMachineId, isActive, remoteMachines]);

  const getRemoteMachineEditDraft = (machine: RemoteMachineSettings): RemoteMachineDraft => {
    const draft =
      remoteMachineDraftsById[machine.id] ??
      createRemoteMachineDraftFromSettings(machine, sshPasswordDrafts[machine.id] ?? '');
    return {
      ...draft,
      sshPassword: sshPasswordDrafts[machine.id] ?? draft.sshPassword,
      sshPasswordSaved: machine.sshPasswordSaved === true,
    };
  };

  const updateRemoteMachine = (machineId: string, patch: Partial<RemoteMachineDraft>) => {
    const currentMachine = remoteMachines.find((machine) => machine.id === machineId);
    if (!currentMachine) {
      return;
    }
    if (patch.sshPassword !== undefined) {
      setSshPasswordDrafts((drafts) => ({
        ...drafts,
        [machineId]: patch.sshPassword ?? '',
      }));
    }
    const settingsPatch = {
      name: patch.name,
      sshHost: patch.sshHost,
      sshIdentityFile: patch.sshIdentityFile,
      sshPort: patch.sshPort,
      sshUser: patch.sshUser,
      wslDistribution: patch.wslDistribution,
      disabled: patch.disabled,
    };
    if (Object.values(settingsPatch).every((value) => value === undefined)) {
      return;
    }
    const nextDraft = applyRemoteMachineDraftPatch(getRemoteMachineEditDraft(currentMachine), patch);
    setRemoteMachineDraftsById((drafts) => ({
      ...drafts,
      [machineId]: nextDraft,
    }));
    const normalizedMachine = normalizeRemoteMachineDraft(nextDraft);
    /*
     * CDXC:RemoteMachines 2026-07-01-00:45:
     * Saved-machine edit fields can be temporarily invalid while the user types.
     * Keep empty required name/host edits in local React draft state so deleting
     * the last character cannot remove the saved machine; only a valid draft or
     * the explicit trash action may change Settings.remoteMachines.
     */
    if (!normalizedMachine) {
      return;
    }
    const nextMachines = remoteMachines
      .map((machine) => {
        if (machine.id !== machineId) {
          return machine;
        }
        return normalizedMachine;
      })
      .filter((machine): machine is RemoteMachineSettings => Boolean(machine));
    onChange(normalizeRemoteMachineSettings(nextMachines));
  };

  const addRemoteMachine = () => {
    const machineId = `remote-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`;
    const password = newMachine.sshPassword;
    const machine = normalizeRemoteMachineDraft({
      ...newMachine,
      id: machineId,
    });
    if (!machine) {
      return;
    }
    /*
     * CDXC:RemoteMachines 2026-06-24-10:40:
     * The add-machine card must show the same password row as saved-machine
     * cards so a new machine and a created machine keep matching grid height.
     * If a create-time password is present, create the machine with a stable id
     * first and send that password as a one-shot Keychain save for the same id;
     * raw SSH passwords still never enter normalized settings.
     */
    onChange(normalizeRemoteMachineSettings([...remoteMachines, machine]));
    if (password.trim().length > 0) {
      postRemoteMachinePasswordSave(machine.id, password);
    }
    setNewMachine(createRemoteMachineDraft());
  };

  const removeRemoteMachine = (machineId: string) => {
    setRemoteMachineDraftsById((drafts) => {
      if (!(machineId in drafts)) {
        return drafts;
      }
      const next = { ...drafts };
      delete next[machineId];
      return next;
    });
    setSshPasswordDrafts((drafts) => {
      if (!(machineId in drafts)) {
        return drafts;
      }
      const next = { ...drafts };
      delete next[machineId];
      return next;
    });
    onChange(remoteMachines.filter((machine) => machine.id !== machineId));
  };

  const postRemoteMachinePasswordSave = (remoteMachineId: string, password: string) => {
    /*
     * CDXC:RemoteMachines 2026-06-09-18:23:
     * The Remote settings password field is a transient entry box. Send the
     * password only from explicit Add Machine or save-icon actions, then clear
     * the React draft so the settings JSON and modal state never retain the
     * secret.
     */
    vscode?.postMessage({
      password,
      remoteMachineId,
      type: 'saveRemoteMachinePassword',
    });
  };

  const saveRemoteMachinePassword = (machine: RemoteMachineSettings) => {
    const password = sshPasswordDrafts[machine.id] ?? '';
    if (!password && machine.sshPasswordSaved !== true) {
      return;
    }
    postRemoteMachinePasswordSave(machine.id, password);
    setSshPasswordDrafts((drafts) => ({
      ...drafts,
      [machine.id]: '',
    }));
  };

  const canAddMachine = newMachine.name.trim().length > 0 && newMachine.sshHost.trim().length > 0;

  if (search.tab.isSearching && !hasVisibleSettingsSearchResult(search.tab)) {
    return (
      <div className='settings-tab-scroll' ref={containerRef}>
        <div className='settings-management-layout'>{searchEmptyState}</div>
      </div>
    );
  }

  /*
   * CDXC:Tailcat 2026-08-31:
   * The Remote page now carries two sections, so each one answers the global
   * Settings search on its own instead of the whole page appearing for any
   * match on it.
   */
  const showRemoteMachines = shouldShowSettingsSection(search.sections.remoteMachines);
  const showTailcat = shouldShowSettingsSection(search.sections.tailcat);

  return (
    <div className='settings-tab-scroll' ref={containerRef}>
      <div className='settings-management-layout'>
        {showRemoteMachines ? (
          <>
            <header className='settings-management-header'>
              <div className='settings-management-header-text'>
                <h3 className='settings-management-heading'>Remote machines</h3>
                <p className='settings-management-description'>
                  Saved SSH machines appear as separate sidebar sections. Hide a machine from the sidebar without
                  deleting it.
                </p>
              </div>
              <Popover onOpenChange={setIsTailscaleHelpOpen} open={isTailscaleHelpOpen}>
                <PopoverTrigger
                  render={<Button className='settings-management-help-button' type='button' variant='outline' />}
                >
                  <IconInfoCircle aria-hidden='true' data-icon='inline-start' />
                  Tailscale setup
                </PopoverTrigger>
                <PopoverContent
                  align='end'
                  className='w-80 max-w-[calc(100vw-2rem)] gap-3 p-4'
                  onOpenAutoFocus={(event) => event.preventDefault()}
                  side='top'
                  sideOffset={8}
                >
                  {/*
                   * CDXC:RemoteMachines 2026-06-08-18:47:
                   * Tailscale setup help should be a compact popover above Remote Machine settings, not a full modal, because it is contextual guidance for filling the SSH host rather than a blocking workflow.
                   *
                   * CDXC:RemoteMachines 2026-06-12-05:42:
                   * The Remote machines header stacks the title over its muted subtitle on the left and pins Tailscale setup as an outline button on the right edge, so the contextual help reads as a real action opposite the header rather than a faint control wedged beside the subtitle.
                   */}
                  <PopoverHeader>
                    <PopoverTitle className='text-sm'>Tailscale setup</PopoverTitle>
                    <PopoverDescription className='text-xs leading-5'>
                      Use Tailscale when the remote machine is not reachable on your local network.
                    </PopoverDescription>
                  </PopoverHeader>
                  <ol className='flex list-decimal flex-col gap-2 pl-5 text-xs leading-5 text-muted-foreground'>
                    <li>Install Tailscale on this Mac and sign in.</li>
                    <li>Install Tailscale on the remote machine and sign in to the same tailnet.</li>
                    <li>Confirm both machines are connected in Tailscale.</li>
                    <li>Use the remote machine's Tailscale DNS name or Tailscale IP as the SSH host.</li>
                  </ol>
                  <p className='text-xs leading-5 text-muted-foreground'>
                    Ghostex still connects with SSH only; no Tailscale tokens or remote gxserver listener are required.
                  </p>
                </PopoverContent>
              </Popover>
            </header>

            <div className='settings-management-list settings-remote-machine-list'>
              {/*
               * CDXC:RemoteMachines 2026-06-12-05:42:
               * Add remote machine is the fixed first grid item (top-left), saved machines fill the remaining slots and wrap to new rows, and the empty placeholder occupies the slot beside the add card so the Remote tab always reads as a single uniform grid.
               *
               * CDXC:RemoteMachines 2026-06-02-23:47:
               * Remote settings require a human name and SSH host before saving because the sidebar section title comes from this user label and v1 remote connections support SSH only.
               */}
              <Card className='settings-remote-machine-card settings-remote-machine-add-card' size='sm'>
                <div className='settings-remote-machine-summary settings-remote-machine-add-summary settings-management-row'>
                  <span aria-hidden='true' className='settings-management-icon settings-remote-machine-add-icon'>
                    <IconPlus size={16} />
                  </span>
                  <span className='settings-management-main min-w-0 flex-1'>
                    <CardTitle className='settings-management-title'>Add remote machine</CardTitle>
                    <span className='settings-management-detail'>New SSH machine</span>
                  </span>
                </div>
                <CardContent className='settings-remote-machine-body'>
                  <RemoteMachineFields
                    draft={newMachine}
                    identityDescription='Provide either an SSH identity file now or an SSH password below.'
                    onChange={(patch) => setNewMachine((draft) => ({ ...draft, ...patch }))}
                    passwordDescription='Passwords are stored in macOS Keychain. Leave blank to add the machine without a saved password.'
                  />
                  <div className='settings-management-actions settings-remote-machine-add-actions'>
                    <SettingButton
                      disabled={!canAddMachine}
                      disabledReason='Enter a machine name and SSH host first.'
                      onClick={addRemoteMachine}
                      type='button'
                    >
                      <IconPlus aria-hidden='true' />
                      Add Machine
                    </SettingButton>
                  </div>
                </CardContent>
              </Card>
              {remoteMachines.length === 0 ? (
                <div className='settings-remote-machine-empty'>
                  <span aria-hidden='true' className='settings-remote-machine-empty-icon'>
                    <IconDeviceDesktop size={18} />
                  </span>
                  <span className='settings-remote-machine-empty-text'>
                    <span className='settings-remote-machine-empty-title'>No machines yet</span>
                    <span className='settings-remote-machine-empty-hint'>
                      Add one to reach it over SSH from the sidebar.
                    </span>
                  </span>
                </div>
              ) : (
                remoteMachines.map((machine) => {
                  const machineDraft = getRemoteMachineEditDraft(machine);
                  const summaryMachine = normalizeRemoteMachineDraft(machineDraft) ?? machine;
                  const gxserverInstall = remoteGxserverInstallsById[machine.id];
                  const gxserverInstalled = gxserverInstall?.installed === true;
                  return (
                    <Card
                      className='settings-remote-machine-card'
                      data-settings-remote-machine-id={machine.id}
                      key={machine.id}
                      size='sm'
                    >
                      <div className='settings-remote-machine-summary settings-management-row'>
                        <span className='settings-management-icon flex size-9 shrink-0 items-center justify-center bg-muted'>
                          <IconDeviceDesktop aria-hidden='true' />
                        </span>
                        <span className='settings-management-main min-w-0 flex-1'>
                          <span className='settings-management-title'>{summaryMachine.name}</span>
                          <span className='settings-management-detail'>
                            {formatRemoteMachineSshTarget(summaryMachine)}
                          </span>
                        </span>
                        <span className='settings-management-row-actions'>
                          <Button
                            aria-label={`Remove ${machine.name}`}
                            onClick={() => removeRemoteMachine(machine.id)}
                            size='icon-sm'
                            type='button'
                            variant='ghost'
                          >
                            <IconTrash aria-hidden='true' />
                          </Button>
                        </span>
                      </div>
                      <CardContent className='settings-remote-machine-body'>
                        <RemoteMachineFields
                          draft={machineDraft}
                          onChange={(patch) => updateRemoteMachine(machine.id, patch)}
                          onPasswordSave={() => saveRemoteMachinePassword(machine)}
                          passwordSaveDisabled={!vscode}
                          showSidebarVisibility
                        />
                        {/*
                         * CDXC:RemoteMachines 2026-06-23-08:30:
                         * Remote Settings needs a direct gxserver install action for
                         * first-run Ubuntu SSH machines. Reuse the reconnect flow so
                         * native opens the approval modal only after SSH proves
                         * gxserver is missing, and otherwise connects the existing
                         * remote daemon without reinstalling it.
                         */}
                        <div className='settings-management-actions settings-remote-machine-install-actions'>
                          {gxserverInstalled ? (
                            <span className='settings-remote-machine-installed-version'>
                              {gxserverInstall?.version ? `gxserver ${gxserverInstall.version}` : 'gxserver installed'}
                            </span>
                          ) : null}
                          <SettingButton
                            disabled={!vscode || !machineDraft.sshHost.trim()}
                            disabledReason={
                              !machineDraft.sshHost.trim()
                                ? 'Enter an SSH host first.'
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
                            {gxserverInstalled ? (
                              <IconRefresh aria-hidden='true' />
                            ) : (
                              <IconDownload aria-hidden='true' />
                            )}
                            {gxserverInstalled ? 'Update gxserver' : 'Install / Connect gxserver'}
                          </SettingButton>
                        </div>
                      </CardContent>
                    </Card>
                  );
                })
              )}
            </div>
          </>
        ) : null}
        {showTailcat && tailcatRpc ? <TailcatSettingsPanel isActive={isActive} rpc={tailcatRpc} /> : null}
      </div>
    </div>
  );
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
      <div className='settings-remote-machine-user-port'>
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
  return normalizeRemoteMachineSettings([
    {
      id: draft.id,
      name: draft.name,
      sshHost: draft.sshHost,
      sshIdentityFile: draft.sshIdentityFile,
      sshPasswordSaved: draft.sshPasswordSaved,
      sshPort: draft.sshPort ? Number(draft.sshPort) : undefined,
      sshUser: draft.sshUser,
      wslDistribution,
      disabled: draft.disabled,
    },
  ])[0];
}

export function formatRemoteMachineSshTarget(machine: RemoteMachineSettings): string {
  const host = machine.sshUser ? `${machine.sshUser}@${machine.sshHost}` : machine.sshHost;
  return machine.sshPort ? `${host}:${machine.sshPort}` : host;
}
