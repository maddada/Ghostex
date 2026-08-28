import type { SidebarProjectCollectionsState } from '../project-collections';
import type { SidebarSpace, SidebarSpacesState } from '../spaces';
import { createProjectCollectionIdByProjectId, type SidebarProjectGroupLookup } from './drag-drop-geometry';

/*
CDXC:SidebarSpaces 2026-08-27:
A Space is scoped to the gxserver that owns its projects, so every Space-aware
value in the sidebar is keyed by SECTION rather than by machine alone: the local
Projects section and each remote machine section each have their own Space set,
their own Space row, and their own selection. These two builders are the single
place those keys are minted, so the persisted selection map
(`selectedSpaceIdBySectionKey`), the dnd sortable ids, and the Space-editor
modal payload cannot drift apart.

The built-in "All Projects" view is the ABSENCE of a selection for a section, so
it never needs a reserved id and can never collide with a real Space id.
*/

export const LOCAL_SIDEBAR_SPACE_SECTION_KEY = 'local';

export function createRemoteSidebarSpaceSectionKey(machineId: string): string {
  return `remote:${machineId}`;
}

/*
CDXC:SidebarSpaces 2026-08-27:
Membership is stored on collections ("groups") and ungrouped projects only.
Everything else is derived here:

- a collection is in the Space when its id is a member collection id;
- an ungrouped project is in the Space when its project id is a member id;
- a project inside a member collection is in the Space (inheritance, with no
  per-project exclusions);
- a worktree follows its PARENT project's effective visibility and can never be
  assigned on its own.

Member ids that resolve to nothing are tolerated on purpose: gxserver prunes
deleted collections and re-grouped projects from its own copy asynchronously, so
a client that treated an unknown id as an error would flicker rows out of a
Space the moment a project moved.
*/
export function createSidebarSpaceGroupVisibility({
  collectionState,
  groupIds,
  groupsById,
  resolveProjectId,
  space,
}: {
  collectionState: SidebarProjectCollectionsState;
  groupIds: readonly string[];
  groupsById: SidebarProjectGroupLookup;
  resolveProjectId: (groupId: string) => string | undefined;
  space: SidebarSpace;
}): (groupId: string) => boolean {
  const collectionIdByProjectId = createProjectCollectionIdByProjectId(
    collectionState,
    groupIds,
    groupsById,
    resolveProjectId
  );
  const memberCollectionIds = new Set(space.memberCollectionIds);
  const memberProjectIds = new Set(space.memberProjectIds);

  return (groupId: string) => {
    const projectId = resolveProjectId(groupId);
    if (!projectId) {
      return false;
    }
    const collectionId = collectionIdByProjectId.get(projectId);
    if (collectionId && memberCollectionIds.has(collectionId)) {
      return true;
    }
    if (memberProjectIds.has(projectId)) {
      return true;
    }
    const parentProjectId = groupsById[groupId]?.projectContext?.worktree?.parentProjectId;
    return Boolean(parentProjectId && memberProjectIds.has(parentProjectId));
  };
}

/**
 * The Space a section is actually filtered by. A selection whose Space no longer
 * exists — deleted from another client, or from a daemon that has since replaced
 * its whole Space document — resolves to All Projects instead of leaving the
 * section filtered by a ghost.
 */
export function resolveSelectedSidebarSpace(
  spacesState: SidebarSpacesState | undefined,
  selectedSpaceId: string | undefined
): SidebarSpace | undefined {
  if (!spacesState || !selectedSpaceId) {
    return undefined;
  }
  return spacesState.spaces[selectedSpaceId];
}
