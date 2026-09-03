export type SidebarProjectInventoryGroupsById = Record<
  string,
  | {
      isChatCollection?: boolean;
    }
  | undefined
>;

/**
 * CDXC:Projects 2026-06-30-03:25:
 * Sidebar search must not flash first-project onboarding after any project is
 * known. Search filtering and transient group display updates can temporarily
 * remove all visible Projects rows, so decide first-run copy from authoritative
 * project inventory and parked Recent Projects instead of displayed group arrays.
 */
export function hasKnownSidebarProjectInventory({
  groupsById,
  projectSettingsProjectCount,
  recentProjectCount,
  unavailableProjectGroupId,
  workspaceGroupIds,
}: {
  groupsById: SidebarProjectInventoryGroupsById;
  projectSettingsProjectCount: number;
  recentProjectCount: number;
  unavailableProjectGroupId: string;
  workspaceGroupIds: readonly string[];
}): boolean {
  if (projectSettingsProjectCount > 0 || recentProjectCount > 0) {
    return true;
  }

  return workspaceGroupIds.some((groupId) => {
    if (groupId === unavailableProjectGroupId) {
      return false;
    }

    const group = groupsById[groupId];
    return group !== undefined && group.isChatCollection !== true;
  });
}
