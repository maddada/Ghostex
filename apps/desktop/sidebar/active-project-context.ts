import type { SidebarStoryWorkspace } from '@/packages/core-ui/sidebar-story-workspace';
import type { SidebarSessionGroup } from '@/packages/shared/session-grid-contract';
import { normalizeWorkspaceProjectIconDataUrl } from '@/packages/shared/workspace-project-appearance';

type ExplicitSidebarProjectContext = NonNullable<SidebarStoryWorkspace['groupMetadataById'][string]['projectContext']>;
type ExplicitLiveSidebarProjectContext = NonNullable<SidebarSessionGroup['projectContext']>;

const GPUI_QUICK_AUTOMATIONS_PROJECT_ID = 'quick-automations';
const GPUI_QUICK_AUTOMATIONS_DISPLAY_TITLE = 'Automations Overview';

/**
 * CDXC:GPUIProjectSidebarBridge 2026-06-23-19:19:
 * GPUI active-project snapshots deliberately exclude Browser surface identity at the TypeScript payload boundary. Build Source, Kanban, Automate, and Manage ids through this helper-owned shape only; Browser readiness stays in the separate Browser workarea readiness message and must not become `browserWorkareaId` in active-project snapshots.
 */
export type GpuiSidebarActiveProjectSurfaceIds = {
  sourceWorkareaId?: string | null;
  kanbanBoardId?: string | null;
  automateBoardId?: string | null;
  manageWorkspaceId?: string | null;
  browserWorkareaId?: never;
};

export type GpuiSidebarActiveProjectContextPayload = {
  version: 1;
  type: 'ghostex.gpui.sidebar.activeProjectContext';
  activeProject: {
    activeProjectId: string | null;
    displayName: string;
    gitRemoteOriginUrl: string | null;
    projectIconDataUrl: string | null;
    projectPath: string | null;
    selectionOwnerProjectId: string | null;
    isQuickProjectless: boolean;
    workareaAvailability: {
      source: boolean;
      browser: boolean;
      kanban: boolean;
      automate: boolean;
      manage: boolean;
    };
    surfaceIds: GpuiSidebarActiveProjectSurfaceIds;
  };
};

export type GpuiSidebarActiveProjectGroupsInput = {
  groups: readonly SidebarSessionGroup[];
};

export function createGpuiSidebarActiveProjectSurfaceIds(
  surfaceIds: GpuiSidebarActiveProjectSurfaceIds = {}
): GpuiSidebarActiveProjectSurfaceIds {
  const strictSurfaceIds: GpuiSidebarActiveProjectSurfaceIds = {};

  if (surfaceIds.sourceWorkareaId !== undefined) {
    strictSurfaceIds.sourceWorkareaId = surfaceIds.sourceWorkareaId;
  }
  if (surfaceIds.kanbanBoardId !== undefined) {
    strictSurfaceIds.kanbanBoardId = surfaceIds.kanbanBoardId;
  }
  if (surfaceIds.automateBoardId !== undefined) {
    strictSurfaceIds.automateBoardId = surfaceIds.automateBoardId;
  }
  if (surfaceIds.manageWorkspaceId !== undefined) {
    strictSurfaceIds.manageWorkspaceId = surfaceIds.manageWorkspaceId;
  }

  return strictSurfaceIds;
}

/**
 * CDXC:GPUIProjectSidebarBridge 2026-06-22-20:02:
 * GPUI must derive the active-project contract only from explicit sidebar workspace group metadata. A real project requires active-group projectContext and a non-chat collection marker; project titles are display labels only, and only projectContext.path, projectContext.gitRemoteOriginUrl, plus the explicit projectContext.editor.projectId identity may enter the CEF bridge while fixture names, workspace names, .git probing, command text, logs, persistence, and other private user content must not. The origin is transient and becomes a sanitized browser home URL in Rust.
 *
 * CDXC:GPUIProjectSidebarBridge 2026-07-08:
 * Docs/Manage availability mirrors Kanban for real project-scoped contexts. Quick/projectless and synthetic overview payloads stay unavailable, but real projects always receive the manage workarea flag and native project-editor surface id.
 *
 * CDXC:GPUIProjectSidebarBridge 2026-06-23-06:46:
 * The active-project projectPath field is an allowlisted in-memory contract value sourced only from explicit SidebarStoryWorkspace projectContext.path metadata. Keep missing, non-string, and trim-empty paths as null so Rust keeps the project payload instead of rejecting it; pass a valid non-empty explicit string through unchanged, and do not log or persist it.
 *
 * CDXC:GPUIProjectSidebarBridge 2026-06-24-11:00:
 * Production GPUI now derives the same projectPath allowlist from live SidebarSessionGroup projectContext.path after gxserver presentation projection. Storybook workspaces remain only a test/source helper path; production must not infer paths from fixture names, URLs, filesystems, logs, or project labels.
 *
 * CDXC:GPUIProjectSidebarBridge 2026-06-23-12:25:
 * Source workarea identity must come only from the explicit sidebar/native project-editor key at projectContext.editor.projectId. Valid project payloads pass that non-empty string as the active project id and allowlisted sourceWorkareaId; malformed editor identities are not valid GPUI project payloads and must fall back to Quick/projectless instead of synthesizing Browser, Kanban, Automate, Manage, path, title, fixture, filesystem, URL, localhost, or group-id surface identities.
 *
 * CDXC:GPUIProjectSidebarBridge 2026-06-23-12:56:
 * Kanban, Automate, and Manage surface identities may use the same native project-editor id format as macOS, but only from the explicit projectContext.editor.projectId value. Kanban receives the tasks-mode id, Automate receives the automate-mode id, and Manage receives the manage-mode id for real project payloads. This bridge still does not send Browser ids, readiness, paths beyond the explicit in-memory project path, filesystem probes, or fallback localhost state. Its sole Browser URL field is the explicit primary Git origin used transiently for project Home navigation.
 */
export function createGpuiSidebarActiveProjectContextPayload(
  workspace: SidebarStoryWorkspace
): GpuiSidebarActiveProjectContextPayload {
  const activeGroup = workspace.snapshot.groups.find((group) => group.groupId === workspace.snapshot.activeGroupId);
  const activeGroupMetadata = activeGroup ? workspace.groupMetadataById[activeGroup.groupId] : undefined;
  const projectContext = activeGroupMetadata?.projectContext;

  if (activeGroup && projectContext && activeGroupMetadata?.isChatCollection !== true) {
    return createGpuiProjectPayloadFromActiveGroup({
      activeGroupTitle: activeGroup.title,
      projectContext,
    });
  }

  return createGpuiQuickProjectlessPayload();
}

export function createGpuiSidebarActiveProjectContextPayloadFromGroups({
  groups,
}: GpuiSidebarActiveProjectGroupsInput): GpuiSidebarActiveProjectContextPayload {
  const activeGroup = groups.find((group) => group.isActive);
  const projectContext = activeGroup?.projectContext;

  /*
  CDXC:GPUIProjectSidebarBridge 2026-06-24-11:00:
  Production GPUI sidebar context is derived from the live SidebarApp group projection, not Storybook workspaces or fixture labels. Only an active non-chat group with explicit projectContext.editor.projectId can publish project workareas; Chats, missing gxserver, malformed project ids, and projectless states publish the strict Quick payload.
  */
  if (activeGroup && projectContext && isQuickAutomationsProjectContext(projectContext)) {
    return createGpuiQuickAutomationsOverviewPayload();
  }

  if (activeGroup && projectContext && activeGroup.isChatCollection !== true) {
    return createGpuiProjectPayloadFromActiveGroup({
      activeGroupTitle: activeGroup.title,
      projectContext,
    });
  }

  return createGpuiQuickProjectlessPayload();
}

function createGpuiProjectPayloadFromActiveGroup({
  activeGroupTitle,
  projectContext,
}: {
  activeGroupTitle: string;
  projectContext: ExplicitSidebarProjectContext | ExplicitLiveSidebarProjectContext;
}): GpuiSidebarActiveProjectContextPayload {
  const editorProjectId = explicitEditorProjectId(projectContext);

  if (editorProjectId === null) {
    return createGpuiQuickProjectlessPayload();
  }

  if (editorProjectId === GPUI_QUICK_AUTOMATIONS_PROJECT_ID) {
    return createGpuiQuickAutomationsOverviewPayload();
  }

  return {
    version: 1,
    type: 'ghostex.gpui.sidebar.activeProjectContext',
    activeProject: {
      activeProjectId: editorProjectId,
      displayName: activeGroupTitle,
      gitRemoteOriginUrl: explicitGitRemoteOriginUrl(projectContext),
      projectIconDataUrl: explicitProjectIconDataUrl(projectContext),
      projectPath: explicitInMemoryProjectPath(projectContext),
      selectionOwnerProjectId: explicitSelectionOwnerProjectId(projectContext, editorProjectId),
      isQuickProjectless: false,
      workareaAvailability: {
        source: true,
        browser: true,
        kanban: true,
        automate: true,
        manage: true,
      },
      surfaceIds: explicitProjectSurfaceIds(editorProjectId),
    },
  };
}

function isQuickAutomationsProjectContext(
  projectContext: ExplicitSidebarProjectContext | ExplicitLiveSidebarProjectContext
): boolean {
  return explicitEditorProjectId(projectContext) === GPUI_QUICK_AUTOMATIONS_PROJECT_ID;
}

function createGpuiQuickAutomationsOverviewPayload(): GpuiSidebarActiveProjectContextPayload {
  /*
  CDXC:GPUIAutomationsOverview 2026-07-08:
  Mirror macOS `createQuickAutomationsProjectEditorUrl` and
  `focusQuickAutomationsProject`: the Quick Automations Overview publishes a
  project-scoped Automate surface id for `quick-automations`, but no Source,
  Browser, Kanban, Manage, icon, or project path.
  */
  return {
    version: 1,
    type: 'ghostex.gpui.sidebar.activeProjectContext',
    activeProject: {
      activeProjectId: GPUI_QUICK_AUTOMATIONS_PROJECT_ID,
      displayName: GPUI_QUICK_AUTOMATIONS_DISPLAY_TITLE,
      gitRemoteOriginUrl: null,
      projectIconDataUrl: null,
      projectPath: null,
      selectionOwnerProjectId: GPUI_QUICK_AUTOMATIONS_PROJECT_ID,
      isQuickProjectless: false,
      workareaAvailability: {
        source: false,
        browser: false,
        kanban: false,
        automate: true,
        manage: false,
      },
      surfaceIds: createGpuiSidebarActiveProjectSurfaceIds({
        automateBoardId: nativeProjectEditorSurfaceId(GPUI_QUICK_AUTOMATIONS_PROJECT_ID, 'automate'),
      }),
    },
  };
}

function createGpuiQuickProjectlessPayload(): GpuiSidebarActiveProjectContextPayload {
  return {
    version: 1,
    type: 'ghostex.gpui.sidebar.activeProjectContext',
    activeProject: {
      activeProjectId: null,
      displayName: 'Quick',
      gitRemoteOriginUrl: null,
      projectIconDataUrl: null,
      projectPath: null,
      selectionOwnerProjectId: null,
      isQuickProjectless: true,
      workareaAvailability: {
        source: true,
        browser: false,
        kanban: false,
        automate: false,
        manage: false,
      },
      surfaceIds: createGpuiSidebarActiveProjectSurfaceIds(),
    },
  };
}

function explicitProjectIconDataUrl(
  projectContext: ExplicitSidebarProjectContext | ExplicitLiveSidebarProjectContext
): string | null {
  return normalizeWorkspaceProjectIconDataUrl((projectContext as { iconDataUrl?: unknown }).iconDataUrl) ?? null;
}

function explicitGitRemoteOriginUrl(
  projectContext: ExplicitSidebarProjectContext | ExplicitLiveSidebarProjectContext
): string | null {
  const remoteUrl = (projectContext as { gitRemoteOriginUrl?: unknown }).gitRemoteOriginUrl;

  if (typeof remoteUrl !== 'string' || remoteUrl.trim().length === 0) {
    return null;
  }

  return remoteUrl.trim();
}

function explicitInMemoryProjectPath(
  projectContext: ExplicitSidebarProjectContext | ExplicitLiveSidebarProjectContext
): string | null {
  const projectPath = (projectContext as { path?: unknown }).path;

  if (typeof projectPath !== 'string' || projectPath.trim().length === 0) {
    return null;
  }

  return projectPath;
}

function explicitEditorProjectId(
  projectContext: ExplicitSidebarProjectContext | ExplicitLiveSidebarProjectContext
): string | null {
  const projectId = (projectContext as { editor?: { projectId?: unknown } }).editor?.projectId;

  if (typeof projectId !== 'string' || projectId.trim().length === 0) {
    return null;
  }

  return projectId;
}

function explicitSelectionOwnerProjectId(
  projectContext: ExplicitSidebarProjectContext | ExplicitLiveSidebarProjectContext,
  editorProjectId: string
): string {
  /*
   * Titlebar selection preferences belong to the main project family. The
   * live sidebar contract already owns canonical worktree metadata, so pass
   * its parent id directly and never infer family identity from paths, names,
   * Git state, or group labels.
   */
  const parentProjectId = (projectContext as { worktree?: { parentProjectId?: unknown } }).worktree?.parentProjectId;
  return typeof parentProjectId === 'string' && parentProjectId.trim().length > 0
    ? parentProjectId.trim()
    : editorProjectId;
}

function explicitProjectSurfaceIds(editorProjectId: string): GpuiSidebarActiveProjectSurfaceIds {
  return createGpuiSidebarActiveProjectSurfaceIds({
    sourceWorkareaId: editorProjectId,
    kanbanBoardId: nativeProjectEditorSurfaceId(editorProjectId, 'tasks'),
    automateBoardId: nativeProjectEditorSurfaceId(editorProjectId, 'automate'),
    manageWorkspaceId: nativeProjectEditorSurfaceId(editorProjectId, 'manage'),
  });
}

function nativeProjectEditorSurfaceId(projectId: string, mode: 'tasks' | 'automate' | 'manage'): string {
  return `project-editor:${encodeURIComponent(projectId)}:${mode}`;
}
