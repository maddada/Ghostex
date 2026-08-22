import type {
  GxserverExportSessionTranscriptResult,
  GxserverSidebarHudCommandButton,
} from "@/packages/shared/gxserver-protocol";
import type { OpenAppModalMessage } from "@/packages/core-ui/app-modal-host-bridge";

export type OpenRecentProjectsModalDetail = Pick<
  Extract<OpenAppModalMessage, { modal: "recentProjects" }>,
  "machineId" | "machineName"
>;

export type OpenDelayedActionsModalDetail = Extract<OpenAppModalMessage, { modal: "delayedSend" }>;

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
 * CDXC:ExportTranscript 2026-08-20:
 * The Export Transcript action runs from two mounts of the same host-action
 * cluster (the chat surface and the terminal surface's floating overlay), so
 * its result dialog cannot live inside either one. The action reports every
 * phase on one window event and the single modal host mounted in the app shell
 * renders it — the same split the other web modal hosts use.
 *
 * `path` in the result is absolute ON THE DAEMON'S MACHINE, never the
 * browser's, which is why the dialog offers Copy path instead of a reveal.
 */
export interface ExportTranscriptSessionRef {
  machineId: string;
  projectId: string;
  sessionId: string;
  sessionTitle: string;
  /** gxserver agent id, used to seed the follow-up conversation. */
  agentId?: string;
}

export type ExportTranscriptStatusDetail = ExportTranscriptSessionRef & (
  | { status: "exporting" }
  | { status: "exported"; result: GxserverExportSessionTranscriptResult }
  | { status: "failed"; message: string }
);

export interface RunTitlebarActionDetail {
  action: GxserverSidebarHudCommandButton;
  machineId: string;
  projectId: string;
}

declare global {
  interface WindowEventMap {
    "ghostex-web:closeAppModal": CustomEvent;
    "ghostex-web:exportTranscriptStatus": CustomEvent<ExportTranscriptStatusDetail>;
    "ghostex-web:openSettingsModal": CustomEvent;
    "ghostex-web:openAddProjectModal": CustomEvent<OpenAddProjectModalDetail>;
    "ghostex-web:openCommandPane": CustomEvent;
    "ghostex-web:openDelayedActionsModal": CustomEvent<OpenDelayedActionsModalDetail>;
    "ghostex-web:openRecentProjectsModal": CustomEvent<OpenRecentProjectsModalDetail>;
    "ghostex-web:runTitlebarAction": CustomEvent<RunTitlebarActionDetail>;
  }
}
