import type { OpenAppModalMessage } from "@/sidebar/app-modal-host-bridge";
import type {
  OpenAddProjectModalDetail,
  OpenDelayedActionsModalDetail,
  OpenRecentProjectsModalDetail,
} from "./action-events";

type OpenRecentProjectsModalMessage = Extract<
  OpenAppModalMessage,
  { modal: "recentProjects" }
>;

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
    console.warn("[ghostex-web] Ignoring invalid app-modal host message.");
    return;
  }

  if (message.type === "close") {
    window.dispatchEvent(new CustomEvent("ghostex-web:closeAppModal"));
    return;
  }

  if (message.type === "open" && isAddProjectModal(message.modal)) {
    openAddProjectModal(message);
    return;
  }

  if (message.type === "open" && message.modal === "delayedSend") {
    window.dispatchEvent(
      new CustomEvent("ghostex-web:openDelayedActionsModal", {
        detail: message as OpenDelayedActionsModalDetail,
      }),
    );
    return;
  }

  if (message.type === "open" && message.modal === "settings") {
    window.dispatchEvent(new CustomEvent("ghostex-web:openSettingsModal"));
    return;
  }

  if (message.type !== "open" || message.modal !== "recentProjects") {
    console.warn(
      `[ghostex-web] Ignoring unsupported app modal: ${String(message.modal ?? "unknown")}.`,
    );
    return;
  }

  const openMessage = message as OpenRecentProjectsModalMessage;
  const detail: OpenRecentProjectsModalDetail = {
    ...(typeof openMessage.machineId === "string"
      ? { machineId: openMessage.machineId }
      : {}),
    ...(typeof openMessage.machineName === "string"
      ? { machineName: openMessage.machineName }
      : {}),
  };
  window.dispatchEvent(
    new CustomEvent("ghostex-web:openRecentProjectsModal", { detail }),
  );
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
  return modal === "addProject" || modal === "remoteProjectPicker";
}

function openAddProjectModal(message: Record<string, unknown>): void {
  const machineId = typeof message.machineId === "string"
    ? message.machineId
    : typeof message.remoteMachineId === "string"
      ? message.remoteMachineId
      : undefined;
  const detail: OpenAddProjectModalDetail = {
    ...(machineId ? { machineId } : {}),
  };
  window.dispatchEvent(
    new CustomEvent("ghostex-web:openAddProjectModal", { detail }),
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value && typeof value === "object" && !Array.isArray(value));
}
