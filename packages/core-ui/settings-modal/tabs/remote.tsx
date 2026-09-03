import { useEffect, useRef, useState, type ReactNode } from 'react';
import { IconAlertTriangle } from '@tabler/icons-react';
import { normalizeRemoteMachineSettings, type RemoteMachineSettings } from '../../../shared/ghostex-settings';
import type { SettingsRemoteSection } from '../../app-modal-host-bridge';
import type { RemoteSetupRpc } from '../../remote-setup-modal/gxserver-rpc';
import { type WebviewApi } from '../../webview-api';
import { SettingsTabSearch, hasVisibleSettingsSearchResult, shouldShowSettingsSection } from '../search';
import { RemoteAdvancedSection } from './remote-advanced';
import { EasyConnectCard } from './remote-easy-connect';
import { RemoteMachineDialog, type RemoteGxserverInstallState } from './remote-machine-dialog';
import { RemoteMachineGrid } from './remote-machine-grid';
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

type RemoteMachineDialogTarget = { kind: 'add' } | { kind: 'edit'; machineId: string };

/**
 * CDXC:RemotePairing 2026-09-03:
 * Settings → Remote reads top to bottom as: this computer from a phone (the
 * Easy Connect and Tailscale path cards), this computer reaching other
 * machines (the compact saved-machine grid), then one Advanced collapsible.
 * The Remote Setup modal deep-links into a path card through
 * `initialRemoteSection`, which expands, scrolls to, and focuses that card;
 * `initialRemoteMachineId` scrolls to a saved machine's tile and opens its
 * edit dialog.
 *
 * CDXC:RemotePairing 2026-09-03 DECISION:
 * User: "make easy connect and tailscale 2 options that are above each other vertically in that section, not next to each other", shown "as expandible cards so the user clicks to expand the one they want to use".
 * This supersedes the same-day side-by-side layout: the two cards stack full width, both start collapsed unless a deep link names one, and opening one collapses the other so a single QR code is visible at a time. The open card is plain UI state and is not persisted.
 */
export function RemoteSettingsTab({
  initialRemoteMachineId,
  initialRemoteSection,
  isActive,
  onChange,
  onTailscaleEnabledChange,
  remoteMachines,
  search,
  searchEmptyState,
  tailcatRpc,
  tailscaleEnabled,
  vscode,
}: {
  initialRemoteMachineId?: string;
  initialRemoteSection?: SettingsRemoteSection;
  isActive: boolean;
  onChange: (remoteMachines: RemoteMachineSettings[]) => void;
  onTailscaleEnabledChange: (enabled: boolean) => void;
  remoteMachines: RemoteMachineSettings[];
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
  /** RPC to the gxserver that owns Easy Connect; absent where the host has no daemon connection. */
  tailcatRpc?: RemoteSetupRpc;
  /** Settings.remoteTailscaleEnabled; off keeps the Tailscale card collapsed. */
  tailscaleEnabled: boolean;
  vscode?: WebviewApi;
}) {
  const remote = useRemoteAccess(tailcatRpc, isActive);
  const rpcAvailable = tailcatRpc !== undefined;
  const lastTargetedRemoteSectionRef = useRef<SettingsRemoteSection | undefined>(undefined);
  const containerRef = useRef<HTMLDivElement>(null);
  const [expandedPathCard, setExpandedPathCard] = useState<SettingsRemoteSection | undefined>(undefined);
  const [machineDialog, setMachineDialog] = useState<RemoteMachineDialogTarget | undefined>(undefined);
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
    Record<string, RemoteGxserverInstallState>
  >({});
  const probedRemoteGxserverKeysRef = useRef<Set<string>>(new Set());

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
     * Saving a machine rewrites the saved list, so wait for the list to settle
     * before asking native to open an SSH connection for the probe.
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
    if (!remoteMachines.some((machine) => machine.id === initialRemoteMachineId)) {
      return;
    }
    /*
     * CDXC:RemoteMachines 2026-06-10-09:54:
     * Remote machine header Edit should land on the selected saved machine, not
     * just the generic Remote settings tab. The tile scrolls into view and its
     * edit dialog opens, since the fields now live in the dialog.
     */
    lastTargetedRemoteMachineIdRef.current = initialRemoteMachineId;
    setMachineDialog({ kind: 'edit', machineId: initialRemoteMachineId });
    const animationFrame = requestAnimationFrame(() => {
      Array.from(containerRef.current?.querySelectorAll<HTMLElement>('[data-settings-remote-machine-id]') ?? [])
        .find((candidate) => candidate.dataset.settingsRemoteMachineId === initialRemoteMachineId)
        ?.scrollIntoView({ behavior: 'smooth', block: 'center' });
    });
    return () => cancelAnimationFrame(animationFrame);
  }, [initialRemoteMachineId, isActive, remoteMachines]);

  useEffect(() => {
    if (!isActive) {
      lastTargetedRemoteSectionRef.current = undefined;
      return;
    }
    if (!initialRemoteSection || lastTargetedRemoteSectionRef.current === initialRemoteSection) {
      return;
    }
    lastTargetedRemoteSectionRef.current = initialRemoteSection;
    setExpandedPathCard(initialRemoteSection);
    const animationFrame = requestAnimationFrame(() => {
      const targetCard = Array.from(
        containerRef.current?.querySelectorAll<HTMLElement>('[data-settings-remote-section]') ?? []
      ).find((candidate) => candidate.dataset.settingsRemoteSection === initialRemoteSection);
      if (!targetCard) {
        return;
      }
      targetCard.scrollIntoView({ behavior: 'smooth', block: 'start' });
      targetCard.focus({ preventScroll: true });
    });
    return () => cancelAnimationFrame(animationFrame);
  }, [initialRemoteSection, isActive]);

  const togglePathCard = (section: SettingsRemoteSection) => {
    setExpandedPathCard((current) => (current === section ? undefined : section));
  };

  const postRemoteMachinePasswordSave = (remoteMachineId: string, password: string) => {
    /*
     * CDXC:RemoteMachines 2026-06-09-18:23:
     * The Remote settings password field is a transient entry box. Send the
     * password only from the explicit save actions, and let the dialog clear
     * its draft so the settings JSON never retains the secret.
     */
    vscode?.postMessage({
      password,
      remoteMachineId,
      type: 'saveRemoteMachinePassword',
    });
  };

  const saveMachine = (machine: RemoteMachineSettings, password: string) => {
    const exists = remoteMachines.some((candidate) => candidate.id === machine.id);
    /*
     * CDXC:RemoteMachines 2026-06-24-10:40:
     * A create-time password is sent as a one-shot Keychain save for the new
     * machine's stable id right after the machine is created; raw SSH
     * passwords still never enter normalized settings.
     */
    onChange(
      normalizeRemoteMachineSettings(
        exists
          ? remoteMachines.map((candidate) => (candidate.id === machine.id ? machine : candidate))
          : [...remoteMachines, machine]
      )
    );
    if (password.trim().length > 0) {
      postRemoteMachinePasswordSave(machine.id, password);
    }
    setMachineDialog(undefined);
  };

  const removeMachine = (machineId: string) => {
    onChange(remoteMachines.filter((machine) => machine.id !== machineId));
    setMachineDialog(undefined);
  };

  const setMachineVisible = (machineId: string, visible: boolean) => {
    onChange(
      normalizeRemoteMachineSettings(
        remoteMachines.map((machine) => (machine.id === machineId ? { ...machine, disabled: !visible } : machine))
      )
    );
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
  const dialogMachine =
    machineDialog?.kind === 'edit'
      ? remoteMachines.find((machine) => machine.id === machineDialog.machineId)
      : undefined;
  const dialogOpen = machineDialog?.kind === 'add' || dialogMachine !== undefined;

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
                  <EasyConnectCard
                    expanded={expandedPathCard === 'easyConnect'}
                    onToggleExpanded={() => togglePathCard('easyConnect')}
                    remote={remote}
                    rpcAvailable={rpcAvailable}
                  />
                ) : null}
                {shouldShowSettingsSection(search.sections.tailscale) ? (
                  <TailscaleCard
                    enabled={tailscaleEnabled}
                    expanded={expandedPathCard === 'tailscale'}
                    onEnabledChange={onTailscaleEnabledChange}
                    onToggleExpanded={() => togglePathCard('tailscale')}
                    remote={remote}
                    rpcAvailable={rpcAvailable}
                  />
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
                  separate sidebar sections; the switch hides a machine from the sidebar without deleting it.
                </p>
              </div>
            </header>
            <RemoteMachineGrid
              machines={remoteMachines}
              onAdd={() => setMachineDialog({ kind: 'add' })}
              onOpen={(machineId) => setMachineDialog({ kind: 'edit', machineId })}
              onSetVisible={setMachineVisible}
            />
            <RemoteMachineDialog
              gxserverInstall={dialogMachine ? remoteGxserverInstallsById[dialogMachine.id] : undefined}
              machine={dialogMachine}
              onClose={() => setMachineDialog(undefined)}
              onRemove={removeMachine}
              onSave={saveMachine}
              onSavePassword={postRemoteMachinePasswordSave}
              open={dialogOpen}
              vscode={vscode}
            />
          </section>
        ) : null}

        {showAdvanced && rpcAvailable ? <RemoteAdvancedSection remote={remote} rpcAvailable={rpcAvailable} /> : null}
      </div>
    </div>
  );
}
