import { useEffect, useMemo, useState } from 'react';
import { DEFAULT_COMPLETION_SOUND, getCompletionSoundLabel } from '../shared/completion-sound';
import { createDefaultSidebarAgentButtons } from '../shared/sidebar-agents';
import { createDefaultSidebarCommandButtons } from '../shared/sidebar-commands';
import { createDefaultSidebarGitState } from '../shared/sidebar-git';
import type {
  SessionGridSnapshot,
  SidebarHydrateMessage,
  SidebarRecentProject,
  SidebarSessionGroup,
  SidebarSessionItem,
  SidebarTheme,
  TerminalViewMode,
  VisibleSessionCount,
} from '../shared/session-grid-contract';
import { clampVisibleSessionCount } from '../shared/session-grid-contract';
import { createSidebarSessionItems } from '../shared/session-grid-contract-ui';
import { createDefaultSidebarProjectDiffStats } from '../shared/project-diff-stats';
import { SidebarStoryHarness } from './sidebar-story-harness';
import type { SidebarStoryArgs, SidebarStoryCurrentSettings } from './sidebar-story-fixtures';

const COMBINED_CHATS_GROUP_ID = 'combined-chats';
const CURRENT_SETTINGS_ENDPOINT = '/__ghostex-current-sidebar-settings';
const CURRENT_PROJECTS_ENDPOINT = '/__ghostex-current-sidebar-projects';

type NativeProjectsSnapshot = {
  activeProjectId?: string;
  projects?: NativeProject[];
};

type NativeProject = {
  isChat?: boolean;
  isRecentProject?: boolean;
  name?: string;
  path?: string;
  projectId?: string;
  recentClosedAt?: string;
  theme?: SidebarTheme;
  themeColor?: string;
  workspace?: {
    activeGroupId?: string;
    groups?: NativeWorkspaceGroup[];
  };
};

type NativeProjectWithId = NativeProject & {
  projectId: string;
};

type NativeWorkspaceGroup = {
  groupId?: string;
  snapshot?: Partial<SessionGridSnapshot>;
  title?: string;
};

export function CurrentProjectsSidebarStory({
  args,
  currentSettings,
}: {
  args: SidebarStoryArgs;
  currentSettings?: SidebarStoryCurrentSettings;
}) {
  const [snapshot, setSnapshot] = useState<NativeProjectsSnapshot>();
  const [fetchedSettings, setFetchedSettings] = useState<SidebarStoryCurrentSettings>();

  useEffect(() => {
    let isMounted = true;
    void fetch(CURRENT_PROJECTS_ENDPOINT, { cache: 'no-store' })
      .then((response) => response.json())
      .then((payload: unknown) => {
        if (isMounted && isNativeProjectsSnapshot(payload)) {
          setSnapshot(payload);
        }
      })
      .catch(() => {
        if (isMounted) {
          setSnapshot(undefined);
        }
      });

    return () => {
      isMounted = false;
    };
  }, []);

  useEffect(() => {
    if (currentSettings) {
      return;
    }

    let isMounted = true;
    void fetch(CURRENT_SETTINGS_ENDPOINT, { cache: 'no-store' })
      .then((response) => response.json())
      .then((payload: unknown) => {
        if (isMounted && payload && typeof payload === 'object' && !Array.isArray(payload)) {
          setFetchedSettings(payload as SidebarStoryCurrentSettings);
        }
      })
      .catch(() => {
        if (isMounted) {
          setFetchedSettings(undefined);
        }
      });

    return () => {
      isMounted = false;
    };
  }, [currentSettings]);

  const effectiveSettings = currentSettings ?? fetchedSettings;
  const message = useMemo(
    () => createCurrentProjectsSidebarMessage(snapshot, args, effectiveSettings),
    [args, effectiveSettings, snapshot]
  );

  return <SidebarStoryHarness message={message} />;
}

function createCurrentProjectsSidebarMessage(
  snapshot: NativeProjectsSnapshot | undefined,
  args: SidebarStoryArgs,
  currentSettings: SidebarStoryCurrentSettings | undefined
): SidebarHydrateMessage {
  const projects = snapshot?.projects?.filter(isNativeProjectWithId) ?? [];
  const activeProject = getActiveProject(projects, snapshot?.activeProjectId);
  const groups =
    projects.length > 0
      ? createCurrentProjectGroups(projects, activeProject?.projectId)
      : createEmptyCurrentProjectsGroups();
  const theme = currentSettings?.sidebarTheme ? resolveSettingsSidebarTheme(currentSettings.sidebarTheme) : args.theme;
  const visibleCount = args.visibleCount;
  const highlightedVisibleCount = args.highlightedVisibleCount;

  return {
    groups: groups.map((group) => ({
      ...group,
      isFocusModeActive: group.isActive ? args.isFocusModeActive : false,
      layoutVisibleCount: group.isActive ? highlightedVisibleCount : group.layoutVisibleCount,
      visibleCount: group.isActive ? visibleCount : group.visibleCount,
    })),
    hud: {
      activeSessionsSortMode: 'manual',
      agentManagerZoomPercent: currentSettings?.agentManagerZoomPercent ?? 100,
      agents: createDefaultSidebarAgentButtons(),
      commands: createDefaultSidebarCommandButtons(),
      commandSessionIndicators: [],
      completionBellEnabled: currentSettings?.completionSound !== 'off',
      completionSound:
        currentSettings?.completionSound === 'off'
          ? DEFAULT_COMPLETION_SOUND
          : (currentSettings?.completionSound ?? DEFAULT_COMPLETION_SOUND),
      completionSoundLabel: getCompletionSoundLabel(
        currentSettings?.completionSound === 'off'
          ? DEFAULT_COMPLETION_SOUND
          : (currentSettings?.completionSound ?? DEFAULT_COMPLETION_SOUND)
      ),
      createSessionOnSidebarDoubleClick:
        currentSettings?.createSessionOnSidebarDoubleClick ?? args.createSessionOnSidebarDoubleClick,
      debuggingMode: currentSettings?.debuggingMode ?? args.debuggingMode,
      focusedSessionTitle: getFocusedSessionTitle(groups),
      git: createDefaultSidebarGitState(),
      highlightedVisibleCount,
      isFocusModeActive: args.isFocusModeActive,
      pendingAgentIds: [],
      recentProjects: createCurrentRecentProjects(projects),
      renameSessionOnDoubleClick: currentSettings?.renameSessionOnDoubleClick ?? args.renameSessionOnDoubleClick,
      settings: currentSettings,
      showCloseButtonOnSessionCards:
        currentSettings?.showCloseButtonOnSessionCards ?? args.showCloseButtonOnSessionCards,
      theme,
      viewMode: args.viewMode,
      visibleCount,
      visibleSlotLabels: getVisibleSlotLabels(groups),
    },
    pinnedPrompts: [],
    previousSessions: [],
    revision: 1,
    type: 'hydrate',
  };
}

function createCurrentProjectGroups(
  projects: readonly NativeProjectWithId[],
  activeProjectId: string | undefined
): SidebarSessionGroup[] {
  /**
   * CDXC:SidebarScroll 2026-05-20-08:08:
   * The current-projects regression story must mirror Combined mode: one Quick
   * group for chat sessions followed by one project row per non-recent code
   * project. Keeping zmux expanded with its real session count reproduces the
   * bottom-scroll failure where later projects became unreachable.
   */
  const orderedProjects = orderNativeProjectsForSidebar(projects);
  const chatProjects = orderedProjects.filter((project) => project.isChat === true);
  const projectGroups = orderedProjects
    .filter((project) => project.isChat !== true && project.isRecentProject !== true)
    .map((project) => createCurrentProjectGroup(project, activeProjectId));
  const activeChatProject = chatProjects.find((project) => project.projectId === activeProjectId);
  const activeChatGroup = activeChatProject
    ? createProjectedGroupsForProject(activeChatProject, activeProjectId)[0]
    : undefined;
  const chatSessions = chatProjects.flatMap((project) => {
    const session = createProjectedGroupsForProject(project, activeProjectId).flatMap((group) => group.sessions)[0];
    return session
      ? [
          {
            ...session,
            isFocused: project.projectId === activeProjectId && session.isFocused,
            isVisible: project.projectId === activeProjectId && session.isVisible,
            sessionId: createCombinedProjectSessionId(project.projectId, session.sessionId),
          },
        ]
      : [];
  });

  return [
    {
      groupId: COMBINED_CHATS_GROUP_ID,
      isActive: Boolean(activeChatProject),
      isChatCollection: true,
      isFocusModeActive: activeChatGroup?.isFocusModeActive ?? false,
      kind: 'workspace',
      layoutVisibleCount: activeChatGroup?.layoutVisibleCount ?? 1,
      sessions: chatSessions,
      title: 'Chats',
      viewMode: activeChatGroup?.viewMode ?? 'grid',
      visibleCount: activeChatGroup?.visibleCount ?? 1,
    },
    ...projectGroups,
  ];
}

function createCurrentProjectGroup(
  project: NativeProjectWithId,
  activeProjectId: string | undefined
): SidebarSessionGroup {
  const projectedGroups = createProjectedGroupsForProject(project, activeProjectId);
  const activeGroup =
    projectedGroups.find((group) => group.groupId === project.workspace?.activeGroupId) ?? projectedGroups[0];
  const isActiveProject = project.projectId === activeProjectId;

  return {
    groupId: createCombinedProjectGroupId(project.projectId),
    isActive: isActiveProject,
    isFocusModeActive: isActiveProject && activeGroup ? activeGroup.isFocusModeActive : false,
    kind: 'workspace',
    layoutVisibleCount: activeGroup?.layoutVisibleCount ?? 1,
    projectContext: {
      canRemoveProject: true,
      path: project.path ?? '',
      editor: {
        diffStats: createDefaultSidebarProjectDiffStats(),
        isOpen: false,
        isSleeping: false,
        projectId: project.projectId,
        status: 'idle',
      },
      theme: project.theme ?? 'plain-dark',
      themeColor: project.themeColor,
    },
    sessions: projectedGroups.flatMap((group) =>
      group.sessions.map((session) => ({
        ...session,
        isFocused: isActiveProject && session.isFocused,
        isVisible: isActiveProject && session.isVisible,
        sessionId: createCombinedProjectSessionId(project.projectId, session.sessionId),
      }))
    ),
    title: project.name ?? 'Project',
    viewMode: activeGroup?.viewMode ?? 'grid',
    visibleCount: activeGroup?.visibleCount ?? 1,
  };
}

function createProjectedGroupsForProject(
  project: NativeProjectWithId,
  activeProjectId: string | undefined
): SidebarSessionGroup[] {
  const workspace = project.workspace;
  const groups = workspace?.groups?.length ? workspace.groups : [{ groupId: 'group-1' }];

  return groups.map((group, index) => {
    const snapshot = normalizeNativeSessionGridSnapshot(group.snapshot);
    const sessions = createSidebarSessionItems(snapshot, 'mac').map((session) =>
      projectNativeSession(project, group, session)
    );
    const visibleCount = clampVisibleSessionCount(snapshot.visibleCount);

    return {
      groupId: group.groupId ?? `group-${index + 1}`,
      isActive: project.projectId === activeProjectId && group.groupId === workspace?.activeGroupId,
      isFocusModeActive: visibleCount === 1,
      kind: 'workspace',
      layoutVisibleCount: visibleCount,
      sessions,
      title: group.title ?? `Group ${index + 1}`,
      viewMode: normalizeViewMode(snapshot.viewMode),
      visibleCount,
    };
  });
}

function projectNativeSession(
  project: NativeProjectWithId,
  group: NativeWorkspaceGroup,
  session: SidebarSessionItem
): SidebarSessionItem {
  const nativeSession = group.snapshot?.sessions?.find((candidate) => candidate.sessionId === session.sessionId);
  const title = typeof nativeSession?.title === 'string' ? nativeSession.title : undefined;
  const createdAt = typeof nativeSession?.createdAt === 'string' ? nativeSession.createdAt : undefined;
  const lastActivityAt =
    nativeSession && 'lastActivityAt' in nativeSession && typeof nativeSession.lastActivityAt === 'string'
      ? nativeSession.lastActivityAt
      : undefined;
  const agentName = nativeSession && 'agentName' in nativeSession ? nativeSession.agentName : undefined;

  return {
    ...session,
    agentIcon: normalizeAgentIcon(agentName),
    isRunning: nativeSession?.isSleeping === true ? false : session.isRunning,
    lastInteractionAt: lastActivityAt ?? createdAt,
    primaryTitle: title ?? session.primaryTitle,
    terminalTitle: title && title !== session.primaryTitle ? title : session.terminalTitle,
    detail: project.name,
  };
}

function normalizeNativeSessionGridSnapshot(snapshot: Partial<SessionGridSnapshot> | undefined): SessionGridSnapshot {
  const sessions = Array.isArray(snapshot?.sessions) ? snapshot.sessions : [];
  const firstSessionId = sessions[0]?.sessionId;
  const visibleSessionIds = Array.isArray(snapshot?.visibleSessionIds)
    ? snapshot.visibleSessionIds
    : firstSessionId
      ? [firstSessionId]
      : [];

  return {
    focusedSessionId: typeof snapshot?.focusedSessionId === 'string' ? snapshot.focusedSessionId : firstSessionId,
    sessions,
    viewMode: normalizeViewMode(snapshot?.viewMode),
    visibleCount: clampVisibleSessionCount(snapshot?.visibleCount ?? 1),
    visibleSessionIds,
  };
}

function createCurrentRecentProjects(projects: readonly NativeProjectWithId[]): SidebarRecentProject[] {
  return projects
    .filter((project) => project.isChat !== true && project.isRecentProject === true)
    .sort(compareRecentProjectsByClosedAt)
    .map((project) => ({
      path: project.path ?? '',
      projectId: project.projectId,
      recentClosedAt: project.recentClosedAt,
      sessionCount: countProjectSessions(project),
      theme: project.theme,
      themeColor: project.themeColor,
      title: project.name ?? 'Project',
    }));
}

function createEmptyCurrentProjectsGroups(): SidebarSessionGroup[] {
  return [
    {
      groupId: COMBINED_CHATS_GROUP_ID,
      isActive: false,
      isChatCollection: true,
      isFocusModeActive: false,
      kind: 'workspace',
      layoutVisibleCount: 1,
      sessions: [],
      title: 'Chats',
      viewMode: 'grid',
      visibleCount: 1,
    },
  ];
}

function orderNativeProjectsForSidebar(projects: readonly NativeProjectWithId[]): NativeProjectWithId[] {
  return [
    ...projects.filter((project) => project.isChat === true),
    ...projects.filter((project) => project.isChat !== true),
  ];
}

function getFocusedSessionTitle(groups: readonly SidebarSessionGroup[]): string | undefined {
  const focusedSession = groups.flatMap((group) => group.sessions).find((session) => session.isFocused);
  return focusedSession?.primaryTitle ?? focusedSession?.terminalTitle ?? focusedSession?.alias;
}

function getVisibleSlotLabels(groups: readonly SidebarSessionGroup[]): string[] {
  return groups
    .flatMap((group) => group.sessions)
    .filter((session) => session.isVisible)
    .map((session) => session.shortcutLabel);
}

function getActiveProject(
  projects: readonly NativeProjectWithId[],
  activeProjectId: string | undefined
): NativeProjectWithId | undefined {
  return (
    projects.find((project) => project.projectId === activeProjectId) ??
    projects.find((project) => project.isRecentProject !== true) ??
    projects[0]
  );
}

function countProjectSessions(project: NativeProjectWithId): number {
  return project.workspace?.groups?.reduce((total, group) => total + (group.snapshot?.sessions?.length ?? 0), 0) ?? 0;
}

function compareRecentProjectsByClosedAt(left: NativeProjectWithId, right: NativeProjectWithId): number {
  return recentProjectClosedAtTime(right) - recentProjectClosedAtTime(left);
}

function recentProjectClosedAtTime(project: NativeProjectWithId): number {
  const time = project.recentClosedAt ? new Date(project.recentClosedAt).getTime() : 0;
  return Number.isFinite(time) ? time : 0;
}

function createCombinedProjectGroupId(projectId: string): string {
  return `combined-project:${encodeURIComponent(projectId)}`;
}

function createCombinedProjectSessionId(projectId: string, sessionId: string): string {
  return `combined-session:${encodeURIComponent(projectId)}:${encodeURIComponent(sessionId)}`;
}

function normalizeAgentIcon(agentName: unknown): SidebarSessionItem['agentIcon'] {
  const normalizedAgentName = typeof agentName === 'string' ? agentName.trim().toLowerCase() : '';
  switch (normalizedAgentName) {
    case 'antigravity':
      return 'antigravity-cli';
    case 'cursor':
      return 'cursor-cli';
    case 'droid':
      return 'factory-droid';
    case 'grok':
      return 'grok-build';
    case 'amp':
      return 'amp-cli';
    case 'codebuddy':
      return 'codebuddy';
    case 'hermes':
    case 'hermes-agent':
      return 'hermes-agent';
    case 'qoder':
    case 'qodercli':
      return 'qoder';
    case 'rovo':
    case 'rovodev':
      return 'rovo-dev';
    case 'browser':
    case 'claude':
    case 'codex':
    case 'copilot':
    case 'gemini':
    case 'opencode':
    case 'pi':
      return normalizedAgentName;
    default:
      return undefined;
  }
}

function normalizeViewMode(viewMode: unknown): TerminalViewMode {
  return viewMode === 'horizontal' || viewMode === 'vertical' || viewMode === 'grid' ? viewMode : 'grid';
}

function resolveSettingsSidebarTheme(sidebarTheme: unknown): SidebarTheme {
  if (sidebarTheme === 'plain') {
    return 'dark-2';
  }
  return sidebarTheme === 'dark-1' || sidebarTheme === 'dark-2' || sidebarTheme === 'plain-light'
    ? sidebarTheme
    : 'dark-1';
}

function isNativeProjectsSnapshot(payload: unknown): payload is NativeProjectsSnapshot {
  return Boolean(payload && typeof payload === 'object' && !Array.isArray(payload));
}

function isNativeProjectWithId(project: NativeProject): project is NativeProjectWithId {
  return typeof project.projectId === 'string' && project.projectId.length > 0;
}
