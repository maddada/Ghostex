import type { Dispatch, SetStateAction } from 'react';
import { hashSidebarCollapseDebugId, summarizeSidebarCollapseDebugGroupIds } from '../sidebar-collapse-state-debug';
import type { ProjectSessionListCollapsedState } from '../project-session-list-toggle';

export type SidebarCollapseStateLogger = (
  event: string,
  details: Record<string, unknown>,
  options?: { enabled?: boolean }
) => void;

export type SidebarCollapseActionsOptions = {
  collapsedGroupsById: Record<string, true>;
  groupOrder: readonly string[];
  postSidebarCollapseStateLog: SidebarCollapseStateLogger;
  setCollapsedGroupsById: Dispatch<SetStateAction<Record<string, true>>>;
  setCollapsedProjectCollectionsByKey: Dispatch<SetStateAction<Record<string, true>>>;
  setCollapsedProjectSessionListsById: Dispatch<SetStateAction<ProjectSessionListCollapsedState>>;
};

/*
 * CDXC:SidebarHookDecomposition 2026-08-22:
 * The collapse setters SidebarApp hands to project rows and collection
 * panels. They hold no React state of their own —
 * they only close over the collapse state and the collapse diagnostic logger —
 * so this hook adds no hook calls and cannot move SidebarApp's hook order.
 */
export function useSidebarCollapseActions({
  collapsedGroupsById,
  groupOrder,
  postSidebarCollapseStateLog,
  setCollapsedGroupsById,
  setCollapsedProjectCollectionsByKey,
  setCollapsedProjectSessionListsById,
}: SidebarCollapseActionsOptions) {
  const setGroupCollapsed = (groupId: string, collapsed: boolean) => {
    const wasCollapsed = collapsedGroupsById[groupId] === true;
    const collapsedGroupCountBefore = Object.keys(collapsedGroupsById).length;
    postSidebarCollapseStateLog('groupToggle', {
      changed: wasCollapsed !== collapsed,
      collapsed,
      collapsedGroupCountBefore,
      collapsedGroupCountExpectedAfter:
        collapsedGroupCountBefore + (wasCollapsed === collapsed ? 0 : collapsed ? 1 : -1),
      groupHash: hashSidebarCollapseDebugId(groupId),
      groupIndex: groupOrder.indexOf(groupId),
      wasCollapsed,
    });
    setCollapsedGroupsById((previous) => {
      if (collapsed) {
        if (previous[groupId]) {
          return previous;
        }

        return {
          ...previous,
          [groupId]: true,
        };
      }

      if (!previous[groupId]) {
        return previous;
      }

      const next = { ...previous };
      delete next[groupId];
      return next;
    });
  };

  const setGroupsCollapsed = (groupIds: readonly string[], collapsed: boolean) => {
    const targetGroupSet = new Set(groupIds);
    const collapsedGroupCountBefore = Object.keys(collapsedGroupsById).length;
    const changedGroupCount = groupIds.filter(
      (groupId) => collapsedGroupsById[groupId] !== (collapsed ? true : undefined)
    ).length;
    postSidebarCollapseStateLog('groupsBulkToggle', {
      changedGroupCount,
      collapsed,
      collapsedGroupCountBefore,
      collapsedGroupCountExpectedAfter:
        collapsedGroupCountBefore + (collapsed ? changedGroupCount : -changedGroupCount),
      groupHashes: summarizeSidebarCollapseDebugGroupIds(groupIds),
      targetGroupCount: targetGroupSet.size,
    });
    setCollapsedGroupsById((previous) => {
      if (collapsed) {
        const next = { ...previous };
        let changed = false;
        for (const groupId of groupIds) {
          if (!next[groupId]) {
            next[groupId] = true;
            changed = true;
          }
        }
        return changed ? next : previous;
      }

      let next: Record<string, true> | undefined;
      for (const groupId of groupIds) {
        if (previous[groupId]) {
          next ??= { ...previous };
          delete next[groupId];
        }
      }
      return next ?? previous;
    });
  };

  const setProjectCollectionCollapsed = (collectionKey: string, collapsed: boolean) => {
    setCollapsedProjectCollectionsByKey((previous) => {
      if (collapsed) {
        return previous[collectionKey] ? previous : { ...previous, [collectionKey]: true };
      }
      if (!previous[collectionKey]) {
        return previous;
      }
      const next = { ...previous };
      delete next[collectionKey];
      return next;
    });
  };

  const setProjectSessionListCollapsed = (projectId: string, collapsed: boolean) => {
    setCollapsedProjectSessionListsById((previous) => {
      if (collapsed) {
        return previous[projectId] ? previous : { ...previous, [projectId]: true };
      }
      if (!previous[projectId]) {
        return previous;
      }
      const next = { ...previous };
      delete next[projectId];
      return next;
    });
  };

  return {
    setGroupCollapsed,
    setGroupsCollapsed,
    setProjectCollectionCollapsed,
    setProjectSessionListCollapsed,
  };
}
