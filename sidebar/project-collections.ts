import type { GxserverSidebarProjectCollectionsState } from "../shared/gxserver-protocol";

export type SidebarProjectCollection = {
  collapsed: boolean;
  color: string;
  collectionId: string;
  projectIds: string[];
  title: string;
};

export type SidebarProjectCollectionsState = {
  collections: SidebarProjectCollection[];
  nextCollectionNumber: number;
};

const STORAGE_KEY = "ghostex.sidebar.projectCollections.v1";

export const SIDEBAR_PROJECT_COLLECTION_COLORS = [
  "transparent",
  "#808080",
  "#7c6df2",
  "#3aa675",
  "#d6873f",
  "#d75b72",
  "#3f8fc7",
  "#b36ad4",
  "#8c9b45",
] as const;

export const SIDEBAR_PROJECT_COLLECTION_COLOR_LABELS: Record<
  (typeof SIDEBAR_PROJECT_COLLECTION_COLORS)[number],
  string
> = {
  transparent: "Transparent",
  "#808080": "Gray",
  "#7c6df2": "Violet",
  "#3aa675": "Green",
  "#d6873f": "Orange",
  "#d75b72": "Pink",
  "#3f8fc7": "Blue",
  "#b36ad4": "Purple",
  "#8c9b45": "Lime",
};

export function readSidebarProjectCollections(): SidebarProjectCollectionsState {
  const empty = { collections: [], nextCollectionNumber: 1 };
  if (typeof window === "undefined") {
    return empty;
  }
  try {
    const parsed = JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? "null") as unknown;
    if (!parsed || typeof parsed !== "object") {
      return empty;
    }
    const record = parsed as Record<string, unknown>;
    return sanitizeSidebarProjectCollections(
      Array.isArray(record.collections) ? record.collections : [],
      record.nextCollectionNumber,
    );
  } catch {
    return empty;
  }
}

function sanitizeSidebarProjectCollections(
  rawCollections: readonly unknown[],
  rawNextCollectionNumber: unknown,
): SidebarProjectCollectionsState {
  const seenCollectionIds = new Set<string>();
  const seenProjectIds = new Set<string>();
  const collections: SidebarProjectCollection[] = [];
  for (const rawCollection of rawCollections) {
    if (!rawCollection || typeof rawCollection !== "object") {
      continue;
    }
    const candidate = rawCollection as Record<string, unknown>;
    const collectionId =
      typeof candidate.collectionId === "string"
        ? candidate.collectionId.trim().slice(0, 120)
        : "";
    const title = typeof candidate.title === "string" ? candidate.title.trim().slice(0, 80) : "";
    const color =
      typeof candidate.color === "string" &&
      (candidate.color === "transparent" || /^#[0-9a-f]{6}$/iu.test(candidate.color))
        ? candidate.color
        : SIDEBAR_PROJECT_COLLECTION_COLORS[
            collections.length % SIDEBAR_PROJECT_COLLECTION_COLORS.length
          ];
    if (!collectionId || !title || seenCollectionIds.has(collectionId)) {
      continue;
    }
    seenCollectionIds.add(collectionId);
    const projectIds = (Array.isArray(candidate.projectIds) ? candidate.projectIds : [])
      .filter((projectId): projectId is string => typeof projectId === "string")
      .map((projectId) => projectId.trim().slice(0, 300))
      .filter((projectId) => {
        if (!projectId || seenProjectIds.has(projectId)) {
          return false;
        }
        seenProjectIds.add(projectId);
        return true;
      });
    if (projectIds.length === 0) {
      continue;
    }
    collections.push({
      collapsed: candidate.collapsed === true,
      color,
      collectionId,
      projectIds,
      title,
    });
  }
  return {
    collections,
    nextCollectionNumber:
      typeof rawNextCollectionNumber === "number" &&
      Number.isSafeInteger(rawNextCollectionNumber)
        ? Math.max(1, rawNextCollectionNumber)
        : collections.length + 1,
  };
}

export function writeSidebarProjectCollections(state: SidebarProjectCollectionsState): void {
  if (typeof window === "undefined") {
    return;
  }
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch {
    // Persistence can be unavailable while the in-memory grouping remains usable.
  }
}

/*
CDXC:SidebarProjectCollections 2026-07-18-00:00:
Project collections are now server-backed shared metadata: gxserver stores the
normalized wire state ({collections: record, order, nextCollectionNumber}) so
iOS/Android render and edit the same colored "Group N" overlay. localStorage
stays the instant-edit overlay; these converters translate between the local
ordered-array shape and the gxserver wire shape for write-through sync and
server-authoritative reconciliation.
*/
export function serializeSidebarProjectCollectionsForGxserver(
  state: SidebarProjectCollectionsState,
): GxserverSidebarProjectCollectionsState {
  return {
    collections: Object.fromEntries(
      state.collections.map((collection) => [
        collection.collectionId,
        {
          collapsed: collection.collapsed,
          collectionId: collection.collectionId,
          color: collection.color,
          projectIds: [...collection.projectIds],
          title: collection.title,
        },
      ]),
    ),
    nextCollectionNumber: state.nextCollectionNumber,
    order: state.collections.map((collection) => collection.collectionId),
  };
}

export function parseSidebarProjectCollectionsFromGxserver(
  serverState: unknown,
): SidebarProjectCollectionsState | undefined {
  if (!serverState || typeof serverState !== "object" || Array.isArray(serverState)) {
    return undefined;
  }
  const record = serverState as Record<string, unknown>;
  const collectionsById = record.collections;
  if (!collectionsById || typeof collectionsById !== "object" || Array.isArray(collectionsById)) {
    return undefined;
  }
  const collectionRecord = collectionsById as Record<string, unknown>;
  const orderedIds: string[] = [];
  const seenOrderIds = new Set<string>();
  for (const entry of Array.isArray(record.order) ? record.order : []) {
    if (typeof entry === "string" && entry in collectionRecord && !seenOrderIds.has(entry)) {
      seenOrderIds.add(entry);
      orderedIds.push(entry);
    }
  }
  for (const collectionId of Object.keys(collectionRecord)) {
    if (!seenOrderIds.has(collectionId)) {
      seenOrderIds.add(collectionId);
      orderedIds.push(collectionId);
    }
  }
  return sanitizeSidebarProjectCollections(
    orderedIds.map((collectionId) => {
      const collection = collectionRecord[collectionId];
      return collection && typeof collection === "object" && !Array.isArray(collection)
        ? { ...(collection as Record<string, unknown>), collectionId }
        : undefined;
    }),
    record.nextCollectionNumber,
  );
}

export function areSidebarProjectCollectionsStatesEqual(
  left: SidebarProjectCollectionsState,
  right: SidebarProjectCollectionsState,
): boolean {
  if (
    left.nextCollectionNumber !== right.nextCollectionNumber ||
    left.collections.length !== right.collections.length
  ) {
    return false;
  }
  return left.collections.every((collection, index) => {
    const other = right.collections[index];
    return (
      collection.collapsed === other.collapsed &&
      collection.collectionId === other.collectionId &&
      collection.color === other.color &&
      collection.title === other.title &&
      collection.projectIds.length === other.projectIds.length &&
      collection.projectIds.every((projectId, projectIndex) => projectId === other.projectIds[projectIndex])
    );
  });
}

export function createSidebarProjectCollection(
  state: SidebarProjectCollectionsState,
  projectId: string,
): { collectionId: string; state: SidebarProjectCollectionsState } {
  const nextCollectionNumber = state.nextCollectionNumber;
  const collectionId = `project-collection-${nextCollectionNumber}-${Date.now().toString(36)}`;
  const withoutProject = removeProjectFromSidebarCollections(state, projectId);
  return {
    collectionId,
    state: {
      collections: [
        ...withoutProject.collections,
        {
          collapsed: false,
          color:
            SIDEBAR_PROJECT_COLLECTION_COLORS[
              (nextCollectionNumber - 1) % SIDEBAR_PROJECT_COLLECTION_COLORS.length
            ],
          collectionId,
          projectIds: [projectId],
          title: `Group ${nextCollectionNumber}`,
        },
      ],
      nextCollectionNumber: nextCollectionNumber + 1,
    },
  };
}

export function moveProjectToSidebarCollection(
  state: SidebarProjectCollectionsState,
  projectId: string,
  collectionId: string | undefined,
): SidebarProjectCollectionsState {
  return moveProjectsToSidebarCollection(state, [projectId], collectionId);
}

export function moveProjectsToSidebarCollection(
  state: SidebarProjectCollectionsState,
  projectIds: readonly string[],
  collectionId: string | undefined,
): SidebarProjectCollectionsState {
  const uniqueProjectIds = [...new Set(projectIds.map((projectId) => projectId.trim()))].filter(
    Boolean,
  );
  if (uniqueProjectIds.length === 0) {
    return state;
  }
  if (collectionId) {
    const targetCollection = state.collections.find(
      (collection) => collection.collectionId === collectionId,
    );
    if (!targetCollection) {
      return state;
    }
  }
  const movingProjectIds = new Set(uniqueProjectIds);
  return {
    ...state,
    collections: state.collections
      .map((collection) => ({
        ...collection,
        projectIds: collection.projectIds.filter(
          (candidate) => !movingProjectIds.has(candidate),
        ),
      }))
      .map((collection) =>
        collection.collectionId === collectionId
          ? { ...collection, projectIds: [...collection.projectIds, ...uniqueProjectIds] }
          : collection,
      )
      .filter((collection) => collection.projectIds.length > 0),
  };
}

export function reorderSidebarProjectCollections(
  state: SidebarProjectCollectionsState,
  projectIdsInOrder: readonly string[],
): SidebarProjectCollectionsState {
  const orderByProjectId = new Map(
    projectIdsInOrder.map((projectId, index) => [projectId, index]),
  );
  return {
    ...state,
    collections: state.collections.map((collection) => ({
      ...collection,
      projectIds: [...collection.projectIds].sort((left, right) => {
        const leftIndex = orderByProjectId.get(left);
        const rightIndex = orderByProjectId.get(right);
        if (leftIndex === undefined || rightIndex === undefined) {
          return leftIndex === undefined ? (rightIndex === undefined ? 0 : 1) : -1;
        }
        return leftIndex - rightIndex;
      }),
    })),
  };
}

export function reorderSidebarProjectCollectionDefinitions(
  state: SidebarProjectCollectionsState,
  collectionIdsInOrder: readonly string[],
): SidebarProjectCollectionsState {
  const collectionById = new Map(
    state.collections.map((collection) => [collection.collectionId, collection]),
  );
  const orderedCollections = collectionIdsInOrder.flatMap((collectionId) => {
    const collection = collectionById.get(collectionId);
    if (!collection) {
      return [];
    }
    collectionById.delete(collectionId);
    return [collection];
  });

  return {
    ...state,
    collections: [...orderedCollections, ...collectionById.values()],
  };
}

export function removeSidebarProjectCollection(
  state: SidebarProjectCollectionsState,
  collectionId: string,
): SidebarProjectCollectionsState {
  return {
    ...state,
    collections: state.collections.filter((collection) => collection.collectionId !== collectionId),
  };
}

export function updateSidebarProjectCollection(
  state: SidebarProjectCollectionsState,
  collectionId: string,
  update: (collection: SidebarProjectCollection) => SidebarProjectCollection,
): SidebarProjectCollectionsState {
  return {
    ...state,
    collections: state.collections.map((collection) =>
      collection.collectionId === collectionId ? update(collection) : collection,
    ),
  };
}

function removeProjectFromSidebarCollections(
  state: SidebarProjectCollectionsState,
  projectId: string,
): SidebarProjectCollectionsState {
  return {
    ...state,
    collections: state.collections
      .map((collection) => ({
        ...collection,
        projectIds: collection.projectIds.filter((candidate) => candidate !== projectId),
      }))
      .filter((collection) => collection.projectIds.length > 0),
  };
}
