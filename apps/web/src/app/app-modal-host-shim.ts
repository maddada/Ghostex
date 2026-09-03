import type { OpenAppModalMessage } from '@/packages/core-ui/app-modal-host-bridge';
import type {
  OpenAddProjectModalDetail,
  OpenDelayedActionsModalDetail,
  OpenRecentProjectsModalDetail,
  OpenSessionNoteModalDetail,
  OpenSettingsModalDetail,
  OpenSidebarSpaceEditorModalDetail,
} from './action-events';

type OpenRecentProjectsModalMessage = Extract<OpenAppModalMessage, { modal: 'recentProjects' }>;
type OpenSettingsModalMessage = Extract<OpenAppModalMessage, { modal: 'settings' }>;

export function installWebAppModalHostShim(): void {
  window.webkit = {
    ...window.webkit,
    messageHandlers: {
      ...window.webkit?.messageHandlers,
      ghostexAppModalHost: {
        postMessage: handleAppModalHostMessage,
      },
    },
  };
}

function handleAppModalHostMessage(message: unknown): void {
  if (!isRecord(message)) {
    console.warn('[ghostex-web] Ignoring invalid app-modal host message.');
    return;
  }

  if (message.type === 'close') {
    window.dispatchEvent(new CustomEvent('ghostex-web:closeAppModal'));
    return;
  }

  if (message.type === 'open' && isAddProjectModal(message.modal)) {
    openAddProjectModal(message);
    return;
  }

  if (message.type === 'open' && message.modal === 'delayedSend') {
    window.dispatchEvent(
      new CustomEvent('ghostex-web:openDelayedActionsModal', {
        detail: message as OpenDelayedActionsModalDetail,
      })
    );
    return;
  }

  /*
   * CDXC:SessionNotes 2026-08-24:
   * The session-note editor is opened by the SHARED sidebar (both versions),
   * so the web shell has to answer the same `openAppModal` call gpui's native
   * host does. The payload is forwarded unchanged to the one mounted note host.
   */
  if (message.type === 'open' && message.modal === 'sessionNote') {
    window.dispatchEvent(
      new CustomEvent('ghostex-web:openSessionNoteModal', {
        detail: message as OpenSessionNoteModalDetail,
      })
    );
    return;
  }

  /*
   * CDXC:Spaces 2026-08-27:
   * The New/Edit Space dialog is opened by the SHARED sidebar's Space row, so
   * the web shell answers the same `openAppModal` call gpui's native host does
   * and forwards the payload unchanged to the one mounted Space editor host.
   */
  if (message.type === 'open' && message.modal === 'sidebarSpaceEditor') {
    window.dispatchEvent(
      new CustomEvent('ghostex-web:openSidebarSpaceEditorModal', {
        detail: message as OpenSidebarSpaceEditorModalDetail,
      })
    );
    return;
  }

  if (message.type === 'open' && message.modal === 'settings') {
    /*
     * The web shell runs one modal host per kind, so a modal that hands off to
     * Settings (Remote Setup → Connect / Show instructions) must be closed
     * first or the two dialogs stack.
     */
    window.dispatchEvent(new CustomEvent('ghostex-web:closeAppModal'));
    const settingsMessage = message as OpenSettingsModalMessage;
    const detail: OpenSettingsModalDetail = {
      ...(settingsMessage.initialTab ? { initialTab: settingsMessage.initialTab } : {}),
      ...(settingsMessage.initialRemoteSection ? { initialRemoteSection: settingsMessage.initialRemoteSection } : {}),
    };
    window.dispatchEvent(new CustomEvent('ghostex-web:openSettingsModal', { detail }));
    return;
  }

  /*
   * CDXC:RemotePairing 2026-09-03:
   * The sidebar menu's Mobile & Remote entry opens the shared Remote Setup
   * modal; the web shell mounts it in its own host like Settings.
   */
  if (message.type === 'open' && message.modal === 'remoteSetup') {
    window.dispatchEvent(new CustomEvent('ghostex-web:openRemoteSetupModal'));
    return;
  }

  if (message.type !== 'open' || message.modal !== 'recentProjects') {
    console.warn(`[ghostex-web] Ignoring unsupported app modal: ${String(message.modal ?? 'unknown')}.`);
    return;
  }

  const openMessage = message as OpenRecentProjectsModalMessage;
  const detail: OpenRecentProjectsModalDetail = {
    ...(typeof openMessage.machineId === 'string' ? { machineId: openMessage.machineId } : {}),
    ...(typeof openMessage.machineName === 'string' ? { machineName: openMessage.machineName } : {}),
  };
  window.dispatchEvent(new CustomEvent('ghostex-web:openRecentProjectsModal', { detail }));
}

/*
 * CDXC:AddProject 2026-07-30:
 * `addProject` is the new dialog's own modal kind. `remoteProjectPicker` is the
 * legacy remote-machine header entry point, which carries the same intent with
 * a different payload name, so the web shim resolves both to the shared
 * add-project dialog preselected to that machine. This message is read
 * structurally rather than through the bridge union so the web shim keeps
 * working while the gpui side of the kind lands.
 */
function isAddProjectModal(modal: unknown): boolean {
  return modal === 'addProject' || modal === 'remoteProjectPicker';
}

function openAddProjectModal(message: Record<string, unknown>): void {
  const machineId =
    typeof message.machineId === 'string'
      ? message.machineId
      : typeof message.remoteMachineId === 'string'
        ? message.remoteMachineId
        : undefined;
  const detail: OpenAddProjectModalDetail = {
    ...(machineId ? { machineId } : {}),
  };
  window.dispatchEvent(new CustomEvent('ghostex-web:openAddProjectModal', { detail }));
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value && typeof value === 'object' && !Array.isArray(value));
}
