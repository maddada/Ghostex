import {
  isSidebarSpaceProject,
  isUnassignedSidebarSpaceProject,
  OTHER_SIDEBAR_SPACE_ID,
} from '../../shared/sidebar-spaces-other';
import type { SidebarSessionItem } from '../../shared/session-grid-contract';
import type { SidebarProjectCollectionsState } from '../project-collections';
import type { SidebarSpace, SidebarSpacesState } from '../spaces';
import { createProjectCollectionIdByProjectId, type SidebarProjectGroupLookup } from './drag-drop-geometry';

/*
CDXC:Spaces 2026-08-27:
A Space is scoped to the gxserver that owns its projects, so every Space-aware
value in the sidebar is keyed by SECTION rather than by machine alone: the local
Projects section and each remote machine section each have their own Space set,
their own Space row, and their own selection. These two builders are the single
place those keys are minted, so the persisted selection map
(`selectedSpaceIdBySectionKey`), the dnd sortable ids, and the Space-editor
modal payload cannot drift apart.

CDXC:Spaces 2026-09-02:
The one built-in view is "Other" (packages/shared/sidebar-spaces-other.ts): the
last button of every Space row, showing exactly what no user Space claims. It
has a reserved id rather than being "no selection", so a section is always
filtered by something and the persisted selection is never ambiguous.
*/

export const LOCAL_SIDEBAR_SPACE_SECTION_KEY = 'local';

export function createRemoteSidebarSpaceSectionKey(machineId: string): string {
  return `remote:${machineId}`;
}

/*
CDXC:Spaces 2026-08-27:
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

  return (groupId: string) => {
    const projectId = resolveProjectId(groupId);
    if (!projectId) {
      return false;
    }
    return isSidebarSpaceProject({
      collectionId: collectionIdByProjectId.get(projectId),
      parentProjectId: groupsById[groupId]?.projectContext?.worktree?.parentProjectId,
      projectId,
      space,
    });
  };
}

/*
CDXC:Spaces 2026-09-02:
The built-in Other view, built on the shared rule the mobile app mirrors. It is
the complement of `createSidebarSpaceGroupVisibility` over EVERY Space the
section's gxserver owns, resolved from the same collection map so a worktree and
a grouped project inherit here exactly as they do there.

A row whose project id cannot be resolved is shown: it belongs to no Space, and
Other is the only view left that can show it.
*/
export function createOtherSidebarSpaceGroupVisibility({
  collectionState,
  groupIds,
  groupsById,
  resolveProjectId,
  spaces,
}: {
  collectionState: SidebarProjectCollectionsState;
  groupIds: readonly string[];
  groupsById: SidebarProjectGroupLookup;
  resolveProjectId: (groupId: string) => string | undefined;
  spaces: readonly SidebarSpace[];
}): (groupId: string) => boolean {
  const collectionIdByProjectId = createProjectCollectionIdByProjectId(
    collectionState,
    groupIds,
    groupsById,
    resolveProjectId
  );

  return (groupId: string) => {
    const projectId = resolveProjectId(groupId);
    if (!projectId) {
      return true;
    }
    return isUnassignedSidebarSpaceProject({
      collectionId: collectionIdByProjectId.get(projectId),
      parentProjectId: groupsById[groupId]?.projectContext?.worktree?.parentProjectId,
      projectId,
      spaces,
    });
  };
}

/**
 * The view a section is filtered by: one of its Spaces, or the built-in Other.
 * The Other variant carries the section's whole Space list, because "claimed by
 * nothing" can only be decided against every Space that section owns.
 */
export type SelectedSidebarSpace =
  | { kind: 'other'; spaceId: typeof OTHER_SIDEBAR_SPACE_ID; spaces: readonly SidebarSpace[] }
  | { kind: 'space'; space: SidebarSpace; spaceId: string };

export type SidebarSpaceSessionSummary = {
  attentionCount: number;
  workingCount: number;
};

/**
 * Aggregate live session activity for every user Space and the built-in Other
 * view from the section's complete project inventory. Each Space uses the same
 * collection, project, and worktree membership predicate as the visible list,
 * so its count cannot disagree with what appears after selecting it.
 */
export function createSidebarSpaceSessionSummaries({
  collectionState,
  groupIds,
  groupsById,
  resolveProjectId,
  sessionIdsByGroup,
  sessionsById,
  spacesState,
}: {
  collectionState: SidebarProjectCollectionsState;
  groupIds: readonly string[];
  groupsById: SidebarProjectGroupLookup;
  resolveProjectId: (groupId: string) => string | undefined;
  sessionIdsByGroup: Readonly<Record<string, readonly string[] | undefined>>;
  sessionsById: Readonly<Record<string, Pick<SidebarSessionItem, 'activity'> | undefined>>;
  spacesState: SidebarSpacesState;
}): Record<string, SidebarSpaceSessionSummary> {
  const orderedSpaces = spacesState.order.flatMap((spaceId) =>
    spacesState.spaces[spaceId] ? [spacesState.spaces[spaceId]] : []
  );
  const selections: SelectedSidebarSpace[] = [
    ...orderedSpaces.map((space) => ({ kind: 'space' as const, space, spaceId: space.spaceId })),
    { kind: 'other', spaceId: OTHER_SIDEBAR_SPACE_ID, spaces: orderedSpaces },
  ];
  const summaries: Record<string, SidebarSpaceSessionSummary> = {};

  for (const selection of selections) {
    const isVisibleInSpace = createSelectedSidebarSpaceVisibility({
      collectionState,
      groupIds,
      groupsById,
      resolveProjectId,
      selection,
    });
    const sessionIds = new Set(
      groupIds.filter(isVisibleInSpace).flatMap((groupId) => sessionIdsByGroup[groupId] ?? [])
    );
    let attentionCount = 0;
    let workingCount = 0;
    for (const sessionId of sessionIds) {
      const activity = sessionsById[sessionId]?.activity;
      if (activity === 'working') {
        workingCount += 1;
      } else if (activity === 'attention') {
        attentionCount += 1;
      }
    }
    summaries[selection.spaceId] = { attentionCount, workingCount };
  }

  return summaries;
}

/**
 * CDXC:Spaces 2026-09-02:
 * The default selection rule, in one place: a section with nothing stored — and
 * a section whose stored Space no longer exists, deleted from another client or
 * from a daemon that has since replaced its whole Space document — shows the
 * FIRST Space in the section's order, or Other when the section has no Spaces.
 * Never a ghost Space, and never an "unfiltered" view.
 */
export function resolveSelectedSidebarSpaceId(
  spacesState: SidebarSpacesState,
  selectedSpaceId: string | undefined
): string {
  if (selectedSpaceId === OTHER_SIDEBAR_SPACE_ID) {
    return OTHER_SIDEBAR_SPACE_ID;
  }
  if (selectedSpaceId && spacesState.spaces[selectedSpaceId]) {
    return selectedSpaceId;
  }
  return spacesState.order.find((spaceId) => spacesState.spaces[spaceId] !== undefined) ?? OTHER_SIDEBAR_SPACE_ID;
}

/**
 * The resolved view itself. `undefined` only for a Space-incapable daemon, whose
 * section has no Space row and is not filtered at all.
 */
export function resolveSelectedSidebarSpace(
  spacesState: SidebarSpacesState | undefined,
  selectedSpaceId: string | undefined
): SelectedSidebarSpace | undefined {
  if (!spacesState) {
    return undefined;
  }
  const resolvedSpaceId = resolveSelectedSidebarSpaceId(spacesState, selectedSpaceId);
  const space = spacesState.spaces[resolvedSpaceId];
  return space
    ? { kind: 'space', space, spaceId: resolvedSpaceId }
    : {
        kind: 'other',
        spaceId: OTHER_SIDEBAR_SPACE_ID,
        spaces: spacesState.order.flatMap((spaceId) =>
          spacesState.spaces[spaceId] ? [spacesState.spaces[spaceId]] : []
        ),
      };
}

/** The visibility predicate for whichever view a section resolved to. */
export function createSelectedSidebarSpaceVisibility({
  collectionState,
  groupIds,
  groupsById,
  resolveProjectId,
  selection,
}: {
  collectionState: SidebarProjectCollectionsState;
  groupIds: readonly string[];
  groupsById: SidebarProjectGroupLookup;
  resolveProjectId: (groupId: string) => string | undefined;
  selection: SelectedSidebarSpace;
}): (groupId: string) => boolean {
  if (selection.kind === 'space') {
    return createSidebarSpaceGroupVisibility({
      collectionState,
      groupIds,
      groupsById,
      resolveProjectId,
      space: selection.space,
    });
  }
  return createOtherSidebarSpaceGroupVisibility({
    collectionState,
    groupIds,
    groupsById,
    resolveProjectId,
    spaces: selection.spaces,
  });
}

/** CDXC:Projects 2026-09-05 DECISION: User: opening a project from Quick Access switches to its Space, including membership inherited from its group or parent project. */
export function resolveSidebarSpaceForRevealedGroup({
  targetGroupId,
  spacesState,
  selectedSpaceId,
  ...visibility
}: {
  targetGroupId: string;
  spacesState: SidebarSpacesState;
  selectedSpaceId: string | undefined;
  collectionState: SidebarProjectCollectionsState;
  groupIds: readonly string[];
  groupsById: SidebarProjectGroupLookup;
  resolveProjectId: (groupId: string) => string | undefined;
}): string {
  const current = resolveSelectedSidebarSpace(spacesState, selectedSpaceId)!;
  if (createSelectedSidebarSpaceVisibility({ ...visibility, selection: current })(targetGroupId)) {
    return current.spaceId;
  }
  return (
    spacesState.order.find((spaceId) => {
      const space = spacesState.spaces[spaceId];
      return space && createSidebarSpaceGroupVisibility({ ...visibility, space })(targetGroupId);
    }) ?? OTHER_SIDEBAR_SPACE_ID
  );
}

/**
 * CDXC:Sessions 2026-03-09:
 * User: an unselected Space that still owns the focused session must show the
 * composer chrome. Resolve the Space for the first matching group in this
 * section, or undefined when the focused session is not in this section.
 */
export function resolveSidebarSpaceIdContainingActiveSession({
  groupIdsContainingActiveSession,
  sectionGroupIds,
  spacesState,
  selectedSpaceId,
  ...visibility
}: {
  groupIdsContainingActiveSession: ReadonlySet<string>;
  sectionGroupIds: readonly string[];
  spacesState: SidebarSpacesState | undefined;
  selectedSpaceId: string | undefined;
  collectionState: SidebarProjectCollectionsState;
  groupsById: SidebarProjectGroupLookup;
  resolveProjectId: (groupId: string) => string | undefined;
}): string | undefined {
  if (!spacesState) {
    return undefined;
  }
  const targetGroupId = sectionGroupIds.find((groupId) => groupIdsContainingActiveSession.has(groupId));
  if (!targetGroupId) {
    return undefined;
  }
  return resolveSidebarSpaceForRevealedGroup({
    targetGroupId,
    spacesState,
    selectedSpaceId,
    ...visibility,
    groupIds: sectionGroupIds,
  });
}
