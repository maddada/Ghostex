import { useEffect, useRef, useState, type ReactNode } from 'react';
import { Button } from '@/packages/components/ui/button';
import { Card, CardContent, CardTitle } from '@/packages/components/ui/card';
import { SegmentedControl, SegmentedControlItem } from '@/packages/components/ui/segmented-control';
import {
  IconAlertTriangle,
  IconDeviceDesktop,
  IconDownload,
  IconPlus,
  IconRefresh,
  IconTrash,
} from '@tabler/icons-react';
import {
  normalizeRemoteMachineSettings,
  type RemoteMachineSettings,
  type RemoteMachineTransport,
} from '../../../shared/ghostex-settings';
import type { SettingsRemoteSection } from '../../app-modal-host-bridge';
import type { RemoteSetupRpc } from '../../remote-setup-modal/gxserver-rpc';
import { type WebviewApi } from '../../webview-api';
import { SettingButton } from '../fields';
import { SettingsTabSearch, hasVisibleSettingsSearchResult, shouldShowSettingsSection } from '../search';
import { RemoteAdvancedSection } from './remote-advanced';
import { EasyConnectCard } from './remote-easy-connect';
import {
  RemoteMachineFields,
  applyRemoteMachineDraftPatch,
  createRemoteMachineDraft,
  createRemoteMachineDraftFromSettings,
  formatRemoteMachineSshTarget,
  normalizeRemoteMachineDraft,
  readEasyConnectCodeInput,
  type RemoteMachineDraft,
} from './remote-machine-fields';
import { TailscaleCard } from './remote-tailscale-card';
import { useRemoteAccess } from './use-remote-access';

export {
  RemoteMachineFields,
  applyRemoteMachineDraftPatch,
  createRemoteMachineDraft,
  createRemoteMachineDraftFromSettings,
  formatRemoteMachineSshTarget,
  normalizeRemoteMachineDraft,
  type RemoteMachineDraft,
} from './remote-machine-fields';

export const REMOTE_GXSERVER_INSTALL_PROBE_DEBOUNCE_MS = 600;

/**
 * CDXC:RemotePairing 2026-09-03:
 * Settings → Remote reads top to bottom as: this computer from a phone (Easy
 * Connect and Tailscale path cards side by side), this computer reaching other
 * machines (the saved-machine grid), then one Advanced collapsible. The Remote
 * Setup modal deep-links into a path card through `initialRemoteSection`,
 * which scrolls to and focuses the card the same way `initialRemoteMachineId`
 * lands on a saved machine.
 */
export function RemoteSettingsTab({
  initialRemoteMachineId,
  initialRemoteSection,
  isActive,
  onChange,
  remoteMachines,
  search,
  searchEmptyState,
  tailcatRpc,
  vscode,
}: {
  initialRemoteMachineId?: string;
  initialRemoteSection?: SettingsRemoteSection;
  isActive: boolean;
  onChange: (remoteMachines: RemoteMachineSettings[]) => void;
  remoteMachines: RemoteMachineSettings[];
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
  /** RPC to the gxserver that owns Easy Connect; absent where the host has no daemon connection. */
  tailcatRpc?: RemoteSetupRpc;
  vscode?: WebviewApi;
}) {
  const remote = useRemoteAccess(tailcatRpc, isActive);
  const rpcAvailable = tailcatRpc !== undefined;
  const lastTargetedRemoteSectionRef = useRef<SettingsRemoteSection | undefined>(undefined);
  const containerRef = useRef<HTMLDivElement>(null);
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
      easyConnectCode: patch.easyConnectCode,
      easyConnectAddress: patch.easyConnectAddress,
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

  useEffect(() => {
    if (!isActive) {
      lastTargetedRemoteSectionRef.current = undefined;
      return;
    }
    if (!initialRemoteSection || lastTargetedRemoteSectionRef.current === initialRemoteSection) {
      return;
    }
    const animationFrame = requestAnimationFrame(() => {
      const targetCard = Array.from(
        containerRef.current?.querySelectorAll<HTMLElement>('[data-settings-remote-section]') ?? []
      ).find((candidate) => candidate.dataset.settingsRemoteSection === initialRemoteSection);
      if (!targetCard) {
        return;
      }
      targetCard.scrollIntoView({ behavior: 'smooth', block: 'start' });
      targetCard.focus({ preventScroll: true });
      lastTargetedRemoteSectionRef.current = initialRemoteSection;
    });
    return () => cancelAnimationFrame(animationFrame);
  }, [initialRemoteSection, isActive]);

  /*
   * CDXC:RemotePairing 2026-09-03:
   * The add card is one form with two entry modes. SSH details need a name and
   * host; an Easy Connect code needs a name and an accepted code (the code
   * usually brings the name with it). Switching modes keeps the shared fields
   * (name, user, identity file, password, WSL) and drops only the other mode's
   * address.
   */
  const newMachineCodeReading =
    newMachine.transport === 'easyConnect' ? readEasyConnectCodeInput(newMachine.easyConnectCode) : undefined;
  const canAddMachine =
    newMachine.name.trim().length > 0 &&
    (newMachine.transport === 'easyConnect'
      ? newMachineCodeReading?.kind === 'accepted' && newMachine.easyConnectAddress.length > 0
      : newMachine.sshHost.trim().length > 0);
  const addMachineDisabledReason =
    newMachine.transport === 'easyConnect'
      ? newMachineCodeReading?.kind === 'accepted'
        ? 'Enter a machine name first.'
        : 'Paste a valid Easy Connect code first.'
      : 'Enter a machine name and SSH host first.';
  const setNewMachineTransport = (transport: RemoteMachineTransport) => {
    setNewMachine((draft) => ({
      ...draft,
      transport,
      ...(transport === 'easyConnect' ? { sshHost: '', sshPort: '' } : { easyConnectCode: '', easyConnectAddress: '' }),
    }));
  };

  if (search.tab.isSearching && !hasVisibleSettingsSearchResult(search.tab)) {
    return (
      <div className='settings-tab-scroll' ref={containerRef}>
        <div className='settings-management-layout'>{searchEmptyState}</div>
      </div>
    );
  }

  /*
   * Each section answers the global Settings search on its own instead of the
   * whole page appearing for any match on it.
   */
  const showFromPhone =
    shouldShowSettingsSection(search.sections.easyConnect) || shouldShowSettingsSection(search.sections.tailscale);
  const showRemoteMachines = shouldShowSettingsSection(search.sections.remoteMachines);
  const showAdvanced = shouldShowSettingsSection(search.sections.remoteAdvanced);

  return (
    <div className='settings-tab-scroll' ref={containerRef}>
      <div className='settings-management-layout'>
        {showFromPhone ? (
          <section className='settings-remote-from-phone'>
            <header className='settings-management-header'>
              <div className='settings-management-header-text'>
                <h3 className='settings-management-heading'>Use Ghostex from your phone</h3>
                <p className='settings-management-description'>
                  Two ways for the Ghostex app to reach this computer. Most people only need Easy Connect.
                </p>
              </div>
            </header>
            {rpcAvailable ? (
              <div className='settings-remote-path-cards'>
                {shouldShowSettingsSection(search.sections.easyConnect) ? (
                  <EasyConnectCard remote={remote} rpcAvailable={rpcAvailable} />
                ) : null}
                {shouldShowSettingsSection(search.sections.tailscale) ? (
                  <TailscaleCard remote={remote} rpcAvailable={rpcAvailable} />
                ) : null}
              </div>
            ) : (
              <p className='settings-management-description settings-remote-no-server'>
                Pairing needs the Ghostex server on this computer. Open Settings from the Ghostex app to set up your
                phone.
              </p>
            )}
            {remote.requestError ? (
              <div className='settings-remote-error' role='alert'>
                <IconAlertTriangle aria-hidden='true' />
                <span>{remote.requestError}</span>
              </div>
            ) : null}
          </section>
        ) : null}

        {showRemoteMachines ? (
          <section className='settings-remote-machines'>
            <header className='settings-management-header'>
              <div className='settings-management-header-text'>
                <h3 className='settings-management-heading'>Remote machines</h3>
                <p className='settings-management-description'>
                  Other computers this one connects to, over SSH or with an Easy Connect code. Their projects show up as
                  separate sidebar sections; hide a machine from the sidebar without deleting it.
                </p>
              </div>
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
                    <span className='settings-management-detail'>
                      {newMachine.transport === 'easyConnect' ? 'New Easy Connect machine' : 'New SSH machine'}
                    </span>
                  </span>
                </div>
                <CardContent className='settings-remote-machine-body'>
                  <SegmentedControl
                    aria-label='How to add the machine'
                    className='settings-remote-machine-add-mode'
                    onValueChange={(value) => setNewMachineTransport(value === 'easyConnect' ? 'easyConnect' : 'ssh')}
                    size='sm'
                    stretch
                    value={newMachine.transport}
                  >
                    <SegmentedControlItem value='ssh'>SSH details</SegmentedControlItem>
                    <SegmentedControlItem value='easyConnect'>Easy Connect code</SegmentedControlItem>
                  </SegmentedControl>
                  <RemoteMachineFields
                    draft={newMachine}
                    identityDescription='Provide either an SSH identity file now or an SSH password below.'
                    onChange={(patch) => setNewMachine((draft) => ({ ...draft, ...patch }))}
                    passwordDescription='Passwords are stored in macOS Keychain. Leave blank to add the machine without a saved password.'
                  />
                  <div className='settings-management-actions settings-remote-machine-add-actions'>
                    <SettingButton
                      disabled={!canAddMachine}
                      disabledReason={addMachineDisabledReason}
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
                    <span className='settings-remote-machine-empty-hint'>Add one to reach it from the sidebar.</span>
                  </span>
                </div>
              ) : (
                remoteMachines.map((machine) => {
                  const machineDraft = getRemoteMachineEditDraft(machine);
                  const summaryMachine = normalizeRemoteMachineDraft(machineDraft) ?? machine;
                  const gxserverInstall = remoteGxserverInstallsById[machine.id];
                  const gxserverInstalled = gxserverInstall?.installed === true;
                  const machineHasEndpoint =
                    machine.transport === 'easyConnect'
                      ? Boolean(machineDraft.easyConnectAddress)
                      : machineDraft.sshHost.trim().length > 0;
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
                            disabled={!vscode || !machineHasEndpoint}
                            disabledReason={
                              !machineHasEndpoint
                                ? machine.transport === 'easyConnect'
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
          </section>
        ) : null}

        {showAdvanced && rpcAvailable ? <RemoteAdvancedSection remote={remote} rpcAvailable={rpcAvailable} /> : null}
      </div>
    </div>
  );
}
