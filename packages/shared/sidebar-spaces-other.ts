/*
CDXC:Spaces 2026-09-02:
The built-in "Other" Space. Ghostex has no "All Projects" view any more: the
LAST button of every Space row is Other, and it shows exactly the groups and
ungrouped projects that no user Space claims.

This module is the single definition of that rule. It is deliberately pure and
dependency-free — no React, no DOM, no core-ui types — because the mobile app
mirrors it VERBATIM. Anything that would make the desktop, web, and mobile
sidebars disagree about what "Other" contains is a bug; change this file, then
copy it across, instead of re-deriving the rule per surface.
*/

/**
 * Reserved Space id for the built-in Other view. User Space ids are minted as
 * `space-<position>-<base36 timestamp>` (see `createSidebarSpace`), so a real
 * Space can never claim this id.
 *
 * Other is never a membership target: it is not part of a Space state's `order`
 * or `spaces`, so the group / ungrouped-project "Spaces" submenus — which
 * enumerate that order — can never offer it as a toggle.
 */
export const OTHER_SIDEBAR_SPACE_ID = 'other';

export const OTHER_SIDEBAR_SPACE_LABEL = 'Other';

/**
 * Must be a member of SIDEBAR_COMMAND_ICON_IDS in
 * packages/shared/sidebar-command-icons.ts.
 */
export const OTHER_SIDEBAR_SPACE_ICON = 'layoutDashboard';

/** The only part of a Space this rule reads; `SidebarSpace` satisfies it. */
export type SidebarSpaceMembershipLists = {
  readonly memberCollectionIds: readonly string[];
  readonly memberProjectIds: readonly string[];
};

/**
 * True when no Space claims this sidebar row — the exact complement of the
 * per-Space membership rule:
 *
 * - a project whose collection ("group") is a member collection is claimed
 *   (group inheritance, with no per-project exclusions);
 * - a project whose own id is a member project id is claimed;
 * - a worktree is never claimed on its own; it defers to its PARENT project.
 *
 * `collectionId` is the collection the project resolves to INCLUDING worktree
 * inheritance, exactly as the per-Space rule resolves it, and `spaces` is every
 * Space owned by the row's gxserver section. Member ids that resolve to nothing
 * are tolerated: they simply claim nothing, which is what the daemon's own
 * pruning will converge on.
 */
export function isUnassignedSidebarSpaceProject({
  collectionId,
  parentProjectId,
  projectId,
  spaces,
}: {
  collectionId: string | undefined;
  parentProjectId: string | undefined;
  projectId: string;
  spaces: Iterable<SidebarSpaceMembershipLists>;
}): boolean {
  for (const space of spaces) {
    if (collectionId !== undefined && space.memberCollectionIds.includes(collectionId)) {
      return false;
    }
    if (space.memberProjectIds.includes(projectId)) {
      return false;
    }
    if (parentProjectId !== undefined && space.memberProjectIds.includes(parentProjectId)) {
      return false;
    }
  }
  return true;
}
