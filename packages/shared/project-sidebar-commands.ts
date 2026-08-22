import {
  normalizeStoredSidebarCommandOrder,
  normalizeStoredSidebarCommands,
  type StoredSidebarCommand,
} from "./sidebar-commands";

/**
 * CDXC:ProjectActions 2026-05-19-12:00:
 * Sidebar Actions (terminal/browser command buttons) are owned per project, not
 * globally. Each project stores its own command definitions, display order, and
 * deleted default-action ids so switching projects swaps the Actions panel,
 * command palette section, action-slot hotkeys, and titlebar Actions list.
 *
 * CDXC:ProjectActions 2026-05-19-17:10:
 * Worktree projects share the parent project's Actions store key so every
 * worktree for the same repo uses one command list.
 */
export type ProjectSidebarCommandsState = {
  commands: StoredSidebarCommand[];
  deletedDefaultCommandIds: string[];
  order: string[];
};

export type ProjectSidebarCommandsStore = Record<string, ProjectSidebarCommandsState>;

export function createDefaultProjectSidebarCommandsState(): ProjectSidebarCommandsState {
  return {
    commands: [],
    deletedDefaultCommandIds: [],
    order: [],
  };
}

export function normalizeProjectSidebarCommandsState(
  candidate: unknown,
): ProjectSidebarCommandsState | undefined {
  if (!candidate || typeof candidate !== "object") {
    return undefined;
  }

  const source = candidate as Partial<ProjectSidebarCommandsState>;
  return {
    commands: normalizeStoredSidebarCommands(source.commands),
    deletedDefaultCommandIds: normalizeStoredSidebarCommandOrder(source.deletedDefaultCommandIds),
    order: normalizeStoredSidebarCommandOrder(source.order),
  };
}

export function normalizeProjectSidebarCommandsStore(
  candidate: unknown,
): ProjectSidebarCommandsStore {
  if (!candidate || typeof candidate !== "object" || Array.isArray(candidate)) {
    return {};
  }

  const store: ProjectSidebarCommandsStore = {};
  for (const [projectId, projectState] of Object.entries(candidate)) {
    const trimmedProjectId = projectId.trim();
    if (!trimmedProjectId) {
      continue;
    }
    const normalizedState = normalizeProjectSidebarCommandsState(projectState);
    if (normalizedState) {
      store[trimmedProjectId] = normalizedState;
    }
  }
  return store;
}

export function getProjectSidebarCommandsState(
  store: ProjectSidebarCommandsStore,
  projectId: string,
): ProjectSidebarCommandsState {
  return store[projectId] ?? createDefaultProjectSidebarCommandsState();
}

export function resolveProjectCommandsOwnerId(
  projectId: string,
  parentProjectId?: string,
): string {
  const normalizedParentProjectId = parentProjectId?.trim();
  return normalizedParentProjectId || projectId;
}
