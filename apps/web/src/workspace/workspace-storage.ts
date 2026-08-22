import {
  createWorkspaceModel,
  reconcileWorkspaceSessions,
  WORKSPACE_LAYOUT_STORAGE_KEY,
  type WorkspaceModel,
  type WorkspaceNode,
  type WorkspaceSession,
} from "./workspace-model";

interface StoredWorkspaceLayouts {
  version: 1;
  layouts: Record<string, WorkspaceModel>;
}

function isNode(value: unknown): value is WorkspaceNode {
  if (!value || typeof value !== "object") {
    return false;
  }
  const node = value as Partial<WorkspaceNode>;
  if (node.type === "leaf") {
    return (
      typeof node.paneId === "string" &&
      !!node.tabGroup &&
      Array.isArray(node.tabGroup.tabs) &&
      node.tabGroup.tabs.every((tab) => typeof tab?.sessionId === "string")
    );
  }
  return (
    node.type === "split" &&
    typeof node.splitId === "string" &&
    (node.axis === "horizontal" || node.axis === "vertical") &&
    typeof node.ratio === "number" &&
    typeof node.defaultRatio === "number" &&
    isNode(node.first) &&
    isNode(node.second)
  );
}

function isWorkspaceModel(value: unknown): value is WorkspaceModel {
  if (!value || typeof value !== "object") {
    return false;
  }
  const model = value as Partial<WorkspaceModel>;
  return (
    Array.isArray(model.sessions) &&
    isNode(model.root) &&
    typeof model.focusedPane === "string" &&
    (model.focusModePane === null || typeof model.focusModePane === "string") &&
    typeof model.nextPaneId === "number" &&
    typeof model.nextSplitId === "number" &&
    typeof model.nextSessionId === "number"
  );
}

function readLayouts(): StoredWorkspaceLayouts {
  try {
    const parsed = JSON.parse(window.localStorage.getItem(WORKSPACE_LAYOUT_STORAGE_KEY) ?? "null") as
      | Partial<StoredWorkspaceLayouts>
      | null;
    if (parsed?.version === 1 && parsed.layouts && typeof parsed.layouts === "object") {
      return { version: 1, layouts: parsed.layouts as Record<string, WorkspaceModel> };
    }
  } catch {
    // Invalid persisted UI state is ignored.
  }
  return { version: 1, layouts: {} };
}

export function loadWorkspaceLayout(
  machineId: string,
  sessions: WorkspaceSession[],
): WorkspaceModel {
  const stored = readLayouts().layouts[machineId];
  return isWorkspaceModel(stored)
    ? reconcileWorkspaceSessions(stored, sessions)
    : createWorkspaceModel(sessions);
}

export function loadPersistedWorkspaceLayout(machineId: string): WorkspaceModel | undefined {
  const stored = readLayouts().layouts[machineId];
  return isWorkspaceModel(stored) ? stored : undefined;
}

export function saveWorkspaceLayout(machineId: string, model: WorkspaceModel): void {
  const stored = readLayouts();
  stored.layouts[machineId] = model;
  window.localStorage.setItem(WORKSPACE_LAYOUT_STORAGE_KEY, JSON.stringify(stored));
}
