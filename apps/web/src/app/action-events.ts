import type { GxserverSidebarHudCommandButton } from '@/packages/shared/gxserver-protocol';
import type { OpenAppModalMessage, SettingsRemoteSection } from '@/packages/core-ui/app-modal-host-bridge';
import type { SettingsModalTab } from '@/packages/core-ui/settings-modal';

export type OpenRecentProjectsModalDetail = Pick<
  Extract<OpenAppModalMessage, { modal: 'recentProjects' }>,
  'machineId' | 'machineName'
>;

export type OpenDelayedActionsModalDetail = Extract<OpenAppModalMessage, { modal: 'delayedSend' }>;

/*
 * CDXC:SessionNotes 2026-08-24:
 * The session-note editor's open payload, forwarded verbatim from the shared
 * sidebar's `openAppModal` call so the web dialog opens on exactly the note the
 * row was rendering.
 */
export type OpenSessionNoteModalDetail = Extract<OpenAppModalMessage, { modal: 'sessionNote' }>;

/*
 * CDXC:Spaces 2026-08-27:
 * The New/Edit Space dialog's open payload, forwarded verbatim from the shared
 * sidebar's `openAppModal` call so the web dialog opens on exactly the Space the
 * row was rendering. `remoteMachineId` is the only routing token it carries.
 */
export type OpenSidebarSpaceEditorModalDetail = Extract<OpenAppModalMessage, { modal: 'sidebarSpaceEditor' }>;

/*
 * CDXC:AddProject 2026-07-30:
 * The add-project dialog opens from two different web entry points — the
 * app-modal shim (gpui posts `openAppModal({ modal: "addProject" })`, and the
 * legacy remote machine header still posts `remoteProjectPicker`) and the
 * sidebar runtime's `pickWorkspaceFolder` message, which has no browser
 * equivalent of a native folder picker. Both converge on this one event so the
 * host component has a single entry contract.
 *
 * `machineId` is the only routing token that crosses this boundary: never a
 * base URL, an auth token, or an SSH host.
 */
export interface OpenAddProjectModalDetail {
  machineId?: string;
}

/*
 * CDXC:TranscriptExport 2026-08-20 / CDXC:TranscriptExport 2026-08-24:
 * The Export Transcript action runs from two mounts of the same host-action
 * cluster (the chat surface and the terminal surface's floating overlay), so
 * its dialog cannot live inside either one. The action only names the session
 * on one window event; the single modal host mounted in the app shell owns
 * the whole flow (include-toggle options, the daemon call, the result).
 *
 * The exported path is absolute ON THE DAEMON'S MACHINE, never the browser's,
 * which is why the dialog offers Copy path instead of a reveal.
 */
export interface ExportTranscriptSessionRef {
  machineId: string;
  projectId: string;
  sessionId: string;
  sessionTitle: string;
  /** gxserver agent id, used to seed the follow-up conversation. */
  agentId?: string;
}

export type ExportTranscriptStatusDetail = ExportTranscriptSessionRef & { status: 'requested' };

export interface RunTitlebarActionDetail {
  action: GxserverSidebarHudCommandButton;
  machineId: string;
  projectId: string;
}

/**
 * CDXC:RemotePairing 2026-09-03:
 * Settings deep-link fields the web shell honours: the tab, and the Remote tab
 * card the Remote Setup modal hands off to.
 */
export interface OpenSettingsModalDetail {
  initialRemoteSection?: SettingsRemoteSection;
  initialTab?: SettingsModalTab;
}

declare global {
  interface WindowEventMap {
    'ghostex-web:closeAppModal': CustomEvent;
    'ghostex-web:exportTranscriptStatus': CustomEvent<ExportTranscriptStatusDetail>;
    'ghostex-web:openRemoteSetupModal': CustomEvent;
    'ghostex-web:openSettingsModal': CustomEvent<OpenSettingsModalDetail | undefined>;
    'ghostex-web:openAddProjectModal': CustomEvent<OpenAddProjectModalDetail>;
    'ghostex-web:openCommandPane': CustomEvent<{ toggle?: boolean } | undefined>;
    'ghostex-web:openDelayedActionsModal': CustomEvent<OpenDelayedActionsModalDetail>;
    'ghostex-web:openRecentProjectsModal': CustomEvent<OpenRecentProjectsModalDetail>;
    'ghostex-web:openSessionNoteModal': CustomEvent<OpenSessionNoteModalDetail>;
    'ghostex-web:openSidebarSpaceEditorModal': CustomEvent<OpenSidebarSpaceEditorModalDetail>;
    'ghostex-web:runTitlebarAction': CustomEvent<RunTitlebarActionDetail>;
  }
}
