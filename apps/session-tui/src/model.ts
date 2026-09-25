import { createDisplaySessionLayout } from '@/packages/shared/active-sessions-sort';
import {
  createGxserverPresentationSidebarGroups,
  createGxserverPresentationSidebarGroup,
  createGxserverPresentationSidebarSessionKey,
  parseGxserverPresentationProjectSessionId,
} from '@/packages/shared/gxserver-presentation-sidebar-projection';
import {
  createGpuiPresentationProjectProjectionMetadata,
  resolveGpuiSidebarAgentIcon,
} from '@/apps/desktop/sidebar/gxserver-runtime/helpers/presentation-projection';
import { createGpuiWorkspaceSessionSubgroupId } from '@/packages/shared/workspace-session-subgroup-id';
import { parseSidebarSpacesFromGxserver } from '@/packages/core-ui/spaces';
import { parseSidebarProjectCollectionsFromGxserver } from '@/packages/core-ui/project-collections';
import {
  createSelectedSidebarSpaceVisibility,
  resolveSelectedSidebarSpace,
} from '@/packages/core-ui/sidebar-app/space-filtering';
import { projectSidebarCollections } from '@/packages/core-ui/sidebar-app/project-collection-model';
import { projectSessionSections } from '@/packages/core-ui/sidebar-app/project-session-sections';
import { getSessionCardTitleTooltip } from '@/packages/core-ui/session-card-presentation';
import {
  sessionMatchesSidebarTagFilters,
  type SidebarSessionTagFilter,
} from '@/packages/shared/session-tags';
import type { SidebarSessionGroup, SidebarSessionItem } from '@/packages/shared/session-grid-contract';
import type { ProjectSessionSection } from '@/packages/core-ui/sidebar-app/project-session-section-model';
import type { Catalog } from './gxserver';
import type { Preferences } from './preferences';

export type Row = {
  id: string;
  label: string;
  depth: number;
  type: 'collection' | 'project' | 'section' | 'session' | 'more';
  projectId?: string;
  sessionId?: string;
  zmxName?: string;
  section?: ProjectSessionSection;
  storageId?: string;
  collapsed?: boolean;
  count?: number;
  session?: SidebarSessionItem;
};
export type View = {
  spaces: { id: string; name: string }[];
  spaceId?: string;
  rows: Row[];
  capturedAt: string;
  projects: { id: string; title: string }[];
};
export type ViewOptions = {
  spaceId?: string;
  sortMode: 'manual' | 'lastActivity';
  showHidden: boolean;
  tags: SidebarSessionTagFilter[];
  nowMs?: number;
};

export function projectCatalog(catalog: Catalog, prefs: Preferences, options: ViewOptions): View {
  const snapshot = catalog.snapshot;
  const workspace = snapshot.workspaceGroups;
  const meta = createGpuiPresentationProjectProjectionMetadata({
    domainProjects: catalog.projects,
    presentation: snapshot,
    recentProjects: catalog.recentProjects,
    projectOrder: workspace?.projectOrder,
  });
  const assigned = new Set<string>();
  for (const [projectId, project] of Object.entries(workspace?.projects ?? {}))
    for (const group of project.groups)
      for (const sessionId of group.sessionIds)
        assigned.add(createGxserverPresentationSidebarSessionKey(projectId, sessionId));
  const base = createGxserverPresentationSidebarGroups({
    presentation: snapshot,
    ...meta,
    hiddenSessionKeys: assigned,
    resolveAgentIcon: resolveGpuiSidebarAgentIcon,
  });
  const groups: SidebarSessionGroup[] = [];
  const projectForGroup = new Map<string, string>();
  for (const group of base) {
    if (group.isChatCollection) continue;
    groups.push(group);
    const projectId = group.projectContext?.editor.projectId;
    if (!projectId) continue;
    projectForGroup.set(group.groupId, projectId);
    const project = snapshot.projects.find((p) => p.projectId === projectId);
    if (!project) continue;
    for (const subgroup of workspace?.projects[projectId]?.groups ?? []) {
      const byId = new Map<string, (typeof snapshot.sessions)[number]>(
        snapshot.sessions.filter((s) => s.projectId === projectId).map((s) => [s.sessionId, s])
      );
      const id = createGpuiWorkspaceSessionSubgroupId(projectId, subgroup.groupId);
      const built = createGxserverPresentationSidebarGroup({
        project,
        createProjectGroupId: () => id,
        sessions: subgroup.sessionIds.flatMap((id) => byId.get(id) ?? []),
        resolveAgentIcon: resolveGpuiSidebarAgentIcon,
      });
      groups.push({ ...built, projectContext: undefined, title: subgroup.title });
      projectForGroup.set(id, projectId);
    }
  }
  const groupsById = Object.fromEntries(groups.map((g) => [g.groupId, g]));
  const sessionsById = Object.fromEntries(groups.flatMap((g) => g.sessions.map((s) => [s.sessionId, s])));
  const layout = createDisplaySessionLayout({
    workspaceGroupIds: groups.map((g) => g.groupId),
    sessionIdsByGroup: Object.fromEntries(groups.map((g) => [g.groupId, g.sessions.map((s) => s.sessionId)])),
    sessionsById,
    sortMode: options.sortMode,
    enableSessionParking: prefs.settings.enableSessionParking,
    nowMs: options.nowMs,
  });
  const collections = parseSidebarProjectCollectionsFromGxserver(snapshot.sidebarProjectCollections) ?? {
    collections: [],
    nextCollectionNumber: 1,
  };
  const spaces = prefs.settings.sidebarSpacesEnabled
    ? parseSidebarSpacesFromGxserver(snapshot.sidebarSpaces)
    : undefined;
  const selection = resolveSelectedSidebarSpace(
    spaces,
    options.spaceId ?? prefs.collapse.selectedSpaceIdBySectionKey.local
  );
  const resolveProjectId = (id: string) => groupsById[id]?.projectContext?.editor.projectId;
  const visible = selection
    ? createSelectedSidebarSpaceVisibility({
        collectionState: collections,
        groupIds: layout.groupIds,
        groupsById,
        resolveProjectId,
        selection,
      })
    : () => true;
  const groupIds = layout.groupIds.filter(
    (id) => visible(id) && (options.showHidden || !prefs.hidden.groupIds.includes(id))
  );
  const ordered = projectSidebarCollections({
    sectionGroupIds: groupIds,
    collectionState: collections,
    resolveProjectId,
    groupsById,
    enableProjectCollections: true,
  });
  const rows: Row[] = [];
  const nativeById = new Map(snapshot.sessions.map((s) => [`${s.projectId}:${s.sessionId}`, s]));
  const addProject = (id: string, depth: number) => {
    const group = groupsById[id]!;
    const projectId = projectForGroup.get(id);
    const storageId = group.projectContext?.editor.projectId ?? id;
    const collapsed = !!prefs.collapse.collapsedGroupsById[id];
    const sessions = (layout.sessionIdsByGroup[id] ?? [])
      .map((id) => sessionsById[id]!)
      .filter((s) => sessionMatchesSidebarTagFilters(s, options.tags));
    if (options.tags.length && !sessions.length) return;
    rows.push({
      id,
      type: 'project',
      label: group.title,
      depth,
      projectId,
      storageId,
      collapsed,
      count: sessions.length,
    });
    if (collapsed) return;
    const projected = projectSessionSections(
      { ...group, sessions },
      {
        enableSessionParking: prefs.settings.enableSessionParking,
        compactCount: prefs.settings.projectSessionListCollapsedCount,
        expanded: !!prefs.collapse.expandedProjectSessionListsById[storageId],
        sectionState: prefs.collapse.collapsedProjectSessionSectionsById[storageId],
        nowMs: options.nowMs,
      }
    );
    for (const section of projected.sections) {
      rows.push({
        id: `${id}:${section.id}`,
        type: 'section',
        label: section.id[0]!.toUpperCase() + section.id.slice(1),
        depth: depth + 1,
        projectId,
        storageId,
        section: section.id,
        collapsed: section.collapsed,
        count: section.count,
      });
      for (const sid of section.sessionIds) {
        const session = sessionsById[sid]!;
        const ref = parseGxserverPresentationProjectSessionId(sid);
        const native = ref ? nativeById.get(`${ref.projectId}:${ref.sessionId}`) : undefined;
        const title = getSessionCardTitleTooltip({
          session,
          alwaysShowTitleTooltip: true,
          showDebugSessionNumbers: false,
        });
        rows.push({
          id: sid,
          type: 'session',
          label: session.isGeneratingFirstPromptTitle ? 'Generating title...' : title.headingText,
          depth: depth + 2,
          projectId: ref?.projectId,
          sessionId: ref?.sessionId,
          zmxName: native?.zmxName,
          section: section.id,
          storageId,
          session,
        });
      }
    }
    if (projected.showListToggle)
      rows.push({
        id: `${id}:more`,
        type: 'more',
        label: projected.expanded ? 'Show fewer' : `Show all (${projected.hiddenSessionCount} more)`,
        depth: depth + 1,
        projectId,
        storageId,
      });
  };
  for (const item of ordered) {
    if (item.kind === 'project') {
      addProject(item.groupId, 0);
      continue;
    }
    const key = `local:${item.collection.collectionId}`;
    if (!options.showHidden && prefs.hidden.collectionKeys.includes(key)) continue;
    const collapsed = !!prefs.collapse.collapsedProjectCollectionsByKey[key];
    rows.push({
      id: key,
      type: 'collection',
      label: item.collection.title,
      depth: 0,
      collapsed,
      count: item.groupIds.length,
    });
    if (!collapsed) for (const id of item.groupIds) addProject(id, 1);
  }
  return {
    spaces: spaces
      ? [...spaces.order.map((id) => ({ id, name: spaces.spaces[id]!.name })), { id: 'other', name: 'Other' }]
      : [],
    spaceId: selection?.spaceId,
    rows,
    capturedAt: catalog.capturedAt,
    projects: groups
      .filter((g) => g.projectContext)
      .map((g) => ({ id: g.projectContext!.editor.projectId, title: g.title })),
  };
}
