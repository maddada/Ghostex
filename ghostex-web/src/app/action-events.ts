import type { GxserverSidebarHudCommandButton } from "@/shared/gxserver-protocol";
import type { OpenAppModalMessage } from "@/sidebar/app-modal-host-bridge";

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

export interface RunTitlebarActionDetail {
  action: GxserverSidebarHudCommandButton;
  machineId: string;
  projectId: string;
}

declare global {
  interface WindowEventMap {
    "ghostex-web:closeAppModal": CustomEvent;
    "ghostex-web:openSettingsModal": CustomEvent;
    "ghostex-web:openAddProjectModal": CustomEvent<OpenAddProjectModalDetail>;
    "ghostex-web:openCommandPane": CustomEvent;
    "ghostex-web:openDelayedActionsModal": CustomEvent<OpenDelayedActionsModalDetail>;
    "ghostex-web:openRecentProjectsModal": CustomEvent<OpenRecentProjectsModalDetail>;
    "ghostex-web:runTitlebarAction": CustomEvent<RunTitlebarActionDetail>;
  }
}
