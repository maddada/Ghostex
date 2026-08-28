import type { GxserverSidebarSpacesState } from '@/packages/shared/gxserver-protocol';
import { SIDEBAR_PROJECT_COLLECTION_COLORS } from './project-collections';

/*
CDXC:SidebarSpaces 2026-08-27:
A Space is a server-owned saved sidebar filter: a name, an icon id, a color, a
manual position, and the sidebar members it shows. Members are sidebar project
collections ("groups") and ungrouped projects, and either may belong to any
number of Spaces. Unlike project collections there is no localStorage overlay —
gxserver owns the whole document, one Space set per daemon — so this module is
pure state logic over the wire shape: the sidebar edits a Space state, hands it
back to the host, and the host write-through-syncs it to
/api/updateSidebarSpaces.

`sanitizeSidebarSpacesState` mirrors gxserver's normalization in
server/src/sidebar_spaces.rs so an optimistic local edit renders exactly what
the daemon will echo back, with one deliberate difference: the server also
strips member project ids that its collections document currently groups, and
drops member collection ids that no longer exist. Those cross-document
invariants need the collections state, which only the server holds
authoritatively, so client-side sanitization keeps member ids as given and lets
the server's echo remove them. Clients must therefore tolerate member ids they
cannot resolve — a member project id for a deleted project also lingers as a
soft reference until the daemon prunes it.

The built-in "All Projects" view and worktree inheritance are pure client
concerns and never appear in this state.
*/

export type SidebarSpace = {
  color: string;
  icon: string;
  memberCollectionIds: string[];
  memberProjectIds: string[];
  name: string;
  spaceId: string;
};

export type SidebarSpacesState = {
  order: string[];
  spaces: Record<string, SidebarSpace>;
};

/** Bounds mirror MAX_* in server/src/sidebar_spaces.rs. */
const MAX_SPACES = 256;
const MAX_MEMBER_IDS_PER_LIST = 512;
const MAX_ID_CHARS = 256;
const MAX_NAME_CHARS = 256;
const MAX_ICON_CHARS = 256;

/**
 * Must be a member of SIDEBAR_COMMAND_ICON_IDS in
 * packages/shared/sidebar-command-icons.ts, and matches
 * DEFAULT_SIDEBAR_SPACE_ICON in server/src/sidebar_spaces.rs.
 */
export const DEFAULT_SIDEBAR_SPACE_ICON = 'stack';

export const EMPTY_SIDEBAR_SPACES_STATE: SidebarSpacesState = { order: [], spaces: {} };

export function sanitizeSidebarSpacesState(state: unknown): SidebarSpacesState {
  if (!state || typeof state !== 'object' || Array.isArray(state)) {
    return { order: [], spaces: {} };
  }
  const record = state as Record<string, unknown>;
  const rawSpaces = record.spaces;
  if (!rawSpaces || typeof rawSpaces !== 'object' || Array.isArray(rawSpaces)) {
    return { order: [], spaces: {} };
  }
  const rawSpaceById = rawSpaces as Record<string, unknown>;

  // Candidate ids keyed by trimmed space id; first occurrence wins.
  const candidateById = new Map<string, Record<string, unknown>>();
  for (const [rawSpaceId, rawSpace] of Object.entries(rawSpaceById)) {
    const spaceId = boundedText(rawSpaceId, MAX_ID_CHARS);
    if (!spaceId || candidateById.has(spaceId)) {
      continue;
    }
    if (!rawSpace || typeof rawSpace !== 'object' || Array.isArray(rawSpace)) {
      continue;
    }
    candidateById.set(spaceId, rawSpace as Record<string, unknown>);
  }

  // The explicit order array is authoritative; ids missing from it append in
  // stored map order so every kept Space always has a position.
  const orderedIds: string[] = [];
  const seenOrderIds = new Set<string>();
  for (const entry of Array.isArray(record.order) ? record.order : []) {
    const spaceId = typeof entry === 'string' ? boundedText(entry, MAX_ID_CHARS) : '';
    if (spaceId && candidateById.has(spaceId) && !seenOrderIds.has(spaceId)) {
      seenOrderIds.add(spaceId);
      orderedIds.push(spaceId);
    }
  }
  for (const spaceId of candidateById.keys()) {
    if (!seenOrderIds.has(spaceId)) {
      seenOrderIds.add(spaceId);
      orderedIds.push(spaceId);
    }
  }

  const order: string[] = [];
  const spaces: Record<string, SidebarSpace> = {};
  for (const spaceId of orderedIds) {
    if (order.length >= MAX_SPACES) {
      break;
    }
    const candidate = candidateById.get(spaceId);
    if (!candidate) {
      continue;
    }
    spaces[spaceId] = {
      color: sanitizeSpaceColor(candidate.color, order.length),
      icon: boundedText(candidate.icon, MAX_ICON_CHARS) || DEFAULT_SIDEBAR_SPACE_ICON,
      memberCollectionIds: sanitizeMemberIds(candidate.memberCollectionIds),
      memberProjectIds: sanitizeMemberIds(candidate.memberProjectIds),
      name: boundedText(candidate.name, MAX_NAME_CHARS) || spaceId,
      spaceId,
    };
    order.push(spaceId);
  }
  return { order, spaces };
}

export function parseSidebarSpacesFromGxserver(serverState: unknown): SidebarSpacesState | undefined {
  if (!serverState || typeof serverState !== 'object' || Array.isArray(serverState)) {
    return undefined;
  }
  const spaces = (serverState as Record<string, unknown>).spaces;
  if (!spaces || typeof spaces !== 'object' || Array.isArray(spaces)) {
    return undefined;
  }
  return sanitizeSidebarSpacesState(serverState);
}

export function serializeSidebarSpacesForGxserver(state: SidebarSpacesState): GxserverSidebarSpacesState {
  return {
    order: [...state.order],
    spaces: Object.fromEntries(
      state.order.flatMap((spaceId) => {
        const space = state.spaces[spaceId];
        return space
          ? [
              [
                spaceId,
                {
                  color: space.color,
                  icon: space.icon,
                  memberCollectionIds: [...space.memberCollectionIds],
                  memberProjectIds: [...space.memberProjectIds],
                  name: space.name,
                  spaceId: space.spaceId,
                },
              ] as const,
            ]
          : [];
      })
    ),
  };
}

export function areSidebarSpacesStatesEqual(left: SidebarSpacesState, right: SidebarSpacesState): boolean {
  if (left.order.length !== right.order.length) {
    return false;
  }
  return left.order.every((spaceId, index) => {
    if (right.order[index] !== spaceId) {
      return false;
    }
    const leftSpace = left.spaces[spaceId];
    const rightSpace = right.spaces[spaceId];
    if (!leftSpace || !rightSpace) {
      return leftSpace === rightSpace;
    }
    return (
      leftSpace.color === rightSpace.color &&
      leftSpace.icon === rightSpace.icon &&
      leftSpace.name === rightSpace.name &&
      areIdListsEqual(leftSpace.memberCollectionIds, rightSpace.memberCollectionIds) &&
      areIdListsEqual(leftSpace.memberProjectIds, rightSpace.memberProjectIds)
    );
  });
}

/**
 * New Spaces start last, per the Spaces ordering decision. The id is minted the
 * way project-collection ids are: a positional number plus a base-36 timestamp.
 */
export function createSidebarSpace(
  state: SidebarSpacesState,
  space: { color?: string; icon?: string; name: string }
): { spaceId: string; state: SidebarSpacesState } {
  const position = state.order.length + 1;
  const spaceId = `space-${position}-${Date.now().toString(36)}`;
  return {
    spaceId,
    state: sanitizeSidebarSpacesState({
      order: [...state.order, spaceId],
      spaces: {
        ...state.spaces,
        [spaceId]: {
          color:
            space.color ??
            SIDEBAR_PROJECT_COLLECTION_COLORS[state.order.length % SIDEBAR_PROJECT_COLLECTION_COLORS.length],
          icon: space.icon ?? DEFAULT_SIDEBAR_SPACE_ICON,
          memberCollectionIds: [],
          memberProjectIds: [],
          name: space.name,
          spaceId,
        },
      },
    }),
  };
}

export function updateSidebarSpace(
  state: SidebarSpacesState,
  spaceId: string,
  patch: { color?: string; icon?: string; name?: string }
): SidebarSpacesState {
  const space = state.spaces[spaceId];
  if (!space) {
    return state;
  }
  return sanitizeSidebarSpacesState({
    order: [...state.order],
    spaces: {
      ...state.spaces,
      [spaceId]: {
        ...space,
        color: patch.color ?? space.color,
        icon: patch.icon ?? space.icon,
        name: patch.name ?? space.name,
      },
    },
  });
}

export function deleteSidebarSpace(state: SidebarSpacesState, spaceId: string): SidebarSpacesState {
  if (!(spaceId in state.spaces)) {
    return state;
  }
  const spaces = { ...state.spaces };
  delete spaces[spaceId];
  return {
    order: state.order.filter((candidate) => candidate !== spaceId),
    spaces,
  };
}

/**
 * CDXC:SidebarSpaces 2026-08-27:
 * The New/Edit Space dialog reports field values only, so this is where those
 * values become a state change — against whatever state the sidebar holds when
 * the result arrives, never against the snapshot the dialog opened on. A result
 * naming a Space that has since been deleted is a no-op, because
 * `updateSidebarSpace`/`deleteSidebarSpace` already return the state unchanged
 * for an unknown id.
 */
export function applySidebarSpaceEditorResult(
  state: SidebarSpacesState,
  result: {
    color?: string;
    icon?: string;
    memberCollectionId?: string;
    memberProjectId?: string;
    mode: 'create' | 'delete' | 'edit';
    name?: string;
    spaceId?: string;
  }
): SidebarSpacesState {
  if (result.mode === 'delete') {
    return result.spaceId ? deleteSidebarSpace(state, result.spaceId) : state;
  }
  if (result.mode === 'edit') {
    return result.spaceId
      ? updateSidebarSpace(state, result.spaceId, {
          color: result.color,
          icon: result.icon,
          name: result.name,
        })
      : state;
  }
  const name = result.name?.trim();
  if (!name) {
    return state;
  }
  const created = createSidebarSpace(state, { color: result.color, icon: result.icon, name });
  let nextState = created.state;
  if (result.memberCollectionId) {
    nextState = toggleSpaceCollectionMembership(nextState, created.spaceId, result.memberCollectionId);
  }
  if (result.memberProjectId) {
    nextState = toggleSpaceProjectMembership(nextState, created.spaceId, result.memberProjectId);
  }
  return nextState;
}

/** The ordered ids of the Spaces a project collection ("group") belongs to. */
export function getSidebarSpaceIdsContainingCollection(
  state: SidebarSpacesState,
  collectionId: string
): readonly string[] {
  return state.order.filter((spaceId) => state.spaces[spaceId]?.memberCollectionIds.includes(collectionId));
}

/** The ordered ids of the Spaces an ungrouped project is directly assigned to. */
export function getSidebarSpaceIdsContainingProject(state: SidebarSpacesState, projectId: string): readonly string[] {
  return state.order.filter((spaceId) => state.spaces[spaceId]?.memberProjectIds.includes(projectId));
}

export function reorderSidebarSpaces(state: SidebarSpacesState, orderedIds: readonly string[]): SidebarSpacesState {
  const order: string[] = [];
  const placed = new Set<string>();
  for (const spaceId of orderedIds) {
    if (spaceId in state.spaces && !placed.has(spaceId)) {
      placed.add(spaceId);
      order.push(spaceId);
    }
  }
  for (const spaceId of state.order) {
    if (spaceId in state.spaces && !placed.has(spaceId)) {
      placed.add(spaceId);
      order.push(spaceId);
    }
  }
  return { order, spaces: { ...state.spaces } };
}

export function toggleSpaceCollectionMembership(
  state: SidebarSpacesState,
  spaceId: string,
  collectionId: string
): SidebarSpacesState {
  return withToggledMember(state, spaceId, 'memberCollectionIds', collectionId);
}

export function toggleSpaceProjectMembership(
  state: SidebarSpacesState,
  spaceId: string,
  projectId: string
): SidebarSpacesState {
  return withToggledMember(state, spaceId, 'memberProjectIds', projectId);
}

function withToggledMember(
  state: SidebarSpacesState,
  spaceId: string,
  field: 'memberCollectionIds' | 'memberProjectIds',
  memberId: string
): SidebarSpacesState {
  const space = state.spaces[spaceId];
  const trimmedMemberId = memberId.trim();
  if (!space || !trimmedMemberId) {
    return state;
  }
  const memberIds = space[field].includes(trimmedMemberId)
    ? space[field].filter((candidate) => candidate !== trimmedMemberId)
    : [...space[field], trimmedMemberId];
  return {
    order: [...state.order],
    spaces: {
      ...state.spaces,
      [spaceId]: { ...space, [field]: memberIds },
    },
  };
}

function sanitizeMemberIds(value: unknown): string[] {
  const memberIds: string[] = [];
  const seenMemberIds = new Set<string>();
  for (const entry of Array.isArray(value) ? value : []) {
    if (memberIds.length >= MAX_MEMBER_IDS_PER_LIST) {
      break;
    }
    const memberId = boundedText(entry, MAX_ID_CHARS);
    if (!memberId || seenMemberIds.has(memberId)) {
      continue;
    }
    seenMemberIds.add(memberId);
    memberIds.push(memberId);
  }
  return memberIds;
}

function sanitizeSpaceColor(value: unknown, fallbackIndex: number): string {
  if (typeof value === 'string') {
    const color = value.trim();
    if (/^#[0-9a-f]{6}$/iu.test(color)) {
      return color.toLowerCase();
    }
  }
  return SIDEBAR_PROJECT_COLLECTION_COLORS[fallbackIndex % SIDEBAR_PROJECT_COLLECTION_COLORS.length];
}

function boundedText(value: unknown, maxChars: number): string {
  if (typeof value !== 'string') {
    return '';
  }
  const text = value.trim();
  return text.length === 0 || [...text].length > maxChars ? '' : text;
}

function areIdListsEqual(left: readonly string[], right: readonly string[]): boolean {
  return left.length === right.length && left.every((id, index) => id === right[index]);
}
