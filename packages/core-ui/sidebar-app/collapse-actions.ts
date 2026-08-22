import type { Dispatch, SetStateAction } from "react";
import {
  hashSidebarCollapseDebugId,
  summarizeSidebarCollapseDebugGroupIds,
} from "../sidebar-collapse-state-debug";
import type { ProjectSessionListCollapsedState } from "../project-session-list-toggle";

export type SidebarCollapseStateLogger = (
  event: string,
  details: Record<string, unknown>,
  options?: { enabled?: boolean; },
) => void;

export type SidebarCollapseActionsOptions = {
  collapsedGroupsById: Record<string, true>;
  collapsedRemoteMachineSectionsById: Record<string, true>;
  groupOrder: readonly string[];
  postSidebarCollapseStateLog: SidebarCollapseStateLogger;
  setCollapsedGroupsById: Dispatch<SetStateAction<Record<string, true>>>;
  setCollapsedProjectCollectionsByKey: Dispatch<SetStateAction<Record<string, true>>>;
  setCollapsedProjectSessionListsById: Dispatch<SetStateAction<ProjectSessionListCollapsedState>>;
  setCollapsedRemoteMachineSectionsById: Dispatch<SetStateAction<Record<string, true>>>;
};

/*
 * CDXC:SidebarHookDecomposition 2026-08-22:
 * The five collapse setters SidebarApp hands to project rows, collection
 * panels, and remote machine headers. They hold no React state of their own —
 * they only close over the collapse state and the collapse diagnostic logger —
 * so this hook adds no hook calls and cannot move SidebarApp's hook order.
 */
export function useSidebarCollapseActions({
  collapsedGroupsById,
  collapsedRemoteMachineSectionsById,
  groupOrder,
  postSidebarCollapseStateLog,
  setCollapsedGroupsById,
  setCollapsedProjectCollectionsByKey,
  setCollapsedProjectSessionListsById,
  setCollapsedRemoteMachineSectionsById,
}: SidebarCollapseActionsOptions) {
  const setGroupCollapsed = (groupId: string, collapsed: boolean) => {
    const wasCollapsed = collapsedGroupsById[ groupId ] === true;
    const collapsedGroupCountBefore = Object.keys(collapsedGroupsById).length;
    postSidebarCollapseStateLog("groupToggle", {
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
        if (previous[ groupId ]) {
          return previous;
        }

        return {
          ...previous,
          [ groupId ]: true,
        };
      }

      if (!previous[ groupId ]) {
        return previous;
      }

      const next = { ...previous };
      delete next[ groupId ];
      return next;
    });
  };

  const setGroupsCollapsed = (groupIds: readonly string[], collapsed: boolean) => {
    const targetGroupSet = new Set(groupIds);
    const collapsedGroupCountBefore = Object.keys(collapsedGroupsById).length;
    const changedGroupCount = groupIds.filter(
      (groupId) => collapsedGroupsById[ groupId ] !== (collapsed ? true : undefined),
    ).length;
    postSidebarCollapseStateLog("groupsBulkToggle", {
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
          if (!next[ groupId ]) {
            next[ groupId ] = true;
            changed = true;
          }
        }
        return changed ? next : previous;
      }

      let next: Record<string, true> | undefined;
      for (const groupId of groupIds) {
        if (previous[ groupId ]) {
          next ??= { ...previous };
          delete next[ groupId ];
        }
      }
      return next ?? previous;
    });
  };

  const setProjectCollectionCollapsed = (collectionKey: string, collapsed: boolean) => {
    setCollapsedProjectCollectionsByKey((previous) => {
      if (collapsed) {
        return previous[ collectionKey ] ? previous : { ...previous, [ collectionKey ]: true };
      }
      if (!previous[ collectionKey ]) {
        return previous;
      }
      const next = { ...previous };
      delete next[ collectionKey ];
      return next;
    });
  };

  const setProjectSessionListCollapsed = (projectId: string, collapsed: boolean) => {
    setCollapsedProjectSessionListsById((previous) => {
      if (collapsed) {
        return previous[ projectId ] ? previous : { ...previous, [ projectId ]: true };
      }
      if (!previous[ projectId ]) {
        return previous;
      }
      const next = { ...previous };
      delete next[ projectId ];
      return next;
    });
  };

  const setRemoteMachineSectionCollapsed = (machineId: string, collapsed: boolean) => {
    const wasCollapsed = collapsedRemoteMachineSectionsById[ machineId ] === true;
    postSidebarCollapseStateLog("remoteMachineSectionToggle", {
      changed: wasCollapsed !== collapsed,
      collapsed,
      machineHash: hashSidebarCollapseDebugId(machineId),
      wasCollapsed,
    });
    /*
     * CDXC:RemoteMachines 2026-06-09-19:02:
     * Remote machine sections are peers of Quick and Projects in the reference
     * sidebar. Persist their collapsed state by saved machine id so each machine
     * can collapse independently without affecting local project groups.
     */
    setCollapsedRemoteMachineSectionsById((previous) => {
      if (collapsed) {
        if (previous[ machineId ]) {
          return previous;
        }

        return {
          ...previous,
          [ machineId ]: true,
        };
      }

      if (!previous[ machineId ]) {
        return previous;
      }

      const next = { ...previous };
      delete next[ machineId ];
      return next;
    });
  };

  return {
    setGroupCollapsed,
    setGroupsCollapsed,
    setProjectCollectionCollapsed,
    setProjectSessionListCollapsed,
    setRemoteMachineSectionCollapsed,
  };
}
