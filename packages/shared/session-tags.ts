export const SIDEBAR_SESSION_TAGS = [
  'favorite',
  'high-priority',
  'low-priority',
  'todo',
  'research',
  'in-progress',
  'testing',
  'blocked',
  'on-hold',
  'done',
  'bug',
  'feature',
  'design',
] as const;

export type SidebarSessionTag = (typeof SIDEBAR_SESSION_TAGS)[number];

/**
 * CDXC:Sessions 2026-08-18-02:49:
 * "No tag" is filter chrome only. Sessions stay untagged by omitting
 * `sessionTag`; this sentinel never becomes a persisted marker.
 */
export const SIDEBAR_SESSION_TAG_FILTER_UNTAGGED = 'untagged' as const;

export type SidebarSessionTagFilter = SidebarSessionTag | typeof SIDEBAR_SESSION_TAG_FILTER_UNTAGGED;

export type SidebarSessionTagOption = {
  label: string;
  value: SidebarSessionTag;
};

export type SidebarSessionTagSection = {
  label: 'Priority' | 'Progress' | 'Type';
  options: readonly SidebarSessionTagOption[];
};

/**
 * CDXC:Sessions 2026-06-05-12:30:
 * Session tags replace the single Favorite affordance with one mutually exclusive session marker. Keep the list centralized so sidebar rows, Previous Sessions, macOS, Electron, and gxserver share the same persisted values and user-facing labels.
 *
 * CDXC:Sessions 2026-06-05-14:45:
 * Testing and Blocked are first-class tags. Todo remains distinct from progress work state and uses a bright neutral icon color in the sidebar/menu palette instead of green.
 *
 * CDXC:Sessions 2026-06-05-15:22:
 * The session tag menu and filters are grouped by meaning: Priority tags first, workflow Progress tags in lifecycle order, and Type tags last. Research is a Type tag, not a Progress tag. In Progress is a user-assigned progress tag, separate from runtime activity state, and must use the persisted value `in-progress`.
 *
 * CDXC:Sessions 2026-06-05-19:12:
 * Type tags are limited to Research, Bug, Feature, and Design so the tag picker, filters, and persisted union expose only the current supported classification set.
 */
export const SIDEBAR_SESSION_TAG_SECTIONS: readonly SidebarSessionTagSection[] = [
  {
    label: 'Priority',
    options: [
      { label: 'Favorite', value: 'favorite' },
      { label: 'High Priority', value: 'high-priority' },
      { label: 'Low Priority', value: 'low-priority' },
    ],
  },
  {
    label: 'Progress',
    options: [
      { label: 'Todo', value: 'todo' },
      { label: 'In Progress', value: 'in-progress' },
      { label: 'Testing', value: 'testing' },
      { label: 'Blocked', value: 'blocked' },
      { label: 'On Hold', value: 'on-hold' },
      { label: 'Done', value: 'done' },
    ],
  },
  {
    label: 'Type',
    options: [
      { label: 'Research', value: 'research' },
      { label: 'Bug', value: 'bug' },
      { label: 'Feature', value: 'feature' },
      { label: 'Design', value: 'design' },
    ],
  },
];

export const SIDEBAR_SESSION_TAG_OPTIONS: readonly SidebarSessionTagOption[] = SIDEBAR_SESSION_TAG_SECTIONS.flatMap(
  (section) => section.options
);

const SIDEBAR_SESSION_TAG_SET = new Set<string>(SIDEBAR_SESSION_TAGS);

export const SIDEBAR_SESSION_TAG_LIST_SEPARATOR_IDS = [
  'separator-priority-progress',
  'separator-progress-type',
  'separator-type-untagged',
] as const;

export const SIDEBAR_SESSION_TAG_LIST_UNTAGGED_ID = SIDEBAR_SESSION_TAG_FILTER_UNTAGGED;

export type SidebarSessionTagListSeparatorId = (typeof SIDEBAR_SESSION_TAG_LIST_SEPARATOR_IDS)[number];

export type SidebarSessionTagListItem =
  | {
      enabled: boolean;
      id: SidebarSessionTag;
      tag: SidebarSessionTag;
      type: 'tag';
      visible: boolean;
    }
  | {
      enabled: boolean;
      id: SidebarSessionTagListSeparatorId;
      type: 'separator';
      visible: boolean;
    }
  | {
      enabled: boolean;
      id: typeof SIDEBAR_SESSION_TAG_LIST_UNTAGGED_ID;
      type: 'untagged';
      visible: boolean;
    };

const SIDEBAR_SESSION_TAG_LIST_SEPARATOR_SET = new Set<string>(SIDEBAR_SESSION_TAG_LIST_SEPARATOR_IDS);

/**
 * CDXC:Sessions 2026-06-13-17:50:
 * Sidebar tag filters need a user-reorderable presentation list with movable
 * separators between the default Priority, Progress, and Type groups. Keep this
 * separate from the durable sessionTag union so changing filter chrome cannot
 * rewrite existing session metadata.
 *
 * CDXC:Sessions 2026-06-15-18:32:
 * First-run sidebar tag filters should show a smaller triage set by default.
 * Keep High Priority, Low Priority, Todo, Bug, and Feature hidden from the
 * filter menu until users opt them back in from Settings, while Testing,
 * Research, and Design remain visible.
 *
 * CDXC:Sessions 2026-06-15-22:10:
 * Tags hidden by the first-run default should also be disabled so Reset to
 * Default does not leave a filter row looking enabled while its eye state is
 * hidden. Treat those defaults as fully off until the user turns them back on.
 *
 * CDXC:Sessions 2026-08-18-02:49:
 * The filter menu ends with a No tag row so users can isolate sessions that
 * have no marker. Keep that row out of the durable sessionTag union.
 */
const DEFAULT_OFF_SIDEBAR_SESSION_TAG_FILTERS = new Set<SidebarSessionTag>([
  'high-priority',
  'low-priority',
  'todo',
  'bug',
  'feature',
]);

export const DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS: readonly SidebarSessionTagListItem[] = [
  ...SIDEBAR_SESSION_TAG_SECTIONS[0]!.options.map((option) => createDefaultSidebarSessionTagListTagItem(option.value)),
  createDefaultSidebarSessionTagListSeparatorItem('separator-priority-progress'),
  ...SIDEBAR_SESSION_TAG_SECTIONS[1]!.options.map((option) => createDefaultSidebarSessionTagListTagItem(option.value)),
  createDefaultSidebarSessionTagListSeparatorItem('separator-progress-type'),
  ...SIDEBAR_SESSION_TAG_SECTIONS[2]!.options.map((option) => createDefaultSidebarSessionTagListTagItem(option.value)),
  createDefaultSidebarSessionTagListSeparatorItem('separator-type-untagged'),
  createDefaultSidebarSessionTagListUntaggedItem(),
];

export function isSidebarSessionTag(value: unknown): value is SidebarSessionTag {
  return typeof value === 'string' && SIDEBAR_SESSION_TAG_SET.has(value);
}

export function isSidebarSessionTagFilter(value: unknown): value is SidebarSessionTagFilter {
  return isSidebarSessionTag(value) || value === SIDEBAR_SESSION_TAG_FILTER_UNTAGGED;
}

export function normalizeSidebarSessionTag(value: unknown): SidebarSessionTag | undefined {
  return isSidebarSessionTag(value) ? value : undefined;
}

export function getSidebarSessionTagLabel(tag: SidebarSessionTagFilter | undefined): string | undefined {
  if (tag === SIDEBAR_SESSION_TAG_FILTER_UNTAGGED) {
    return 'No tag';
  }
  return SIDEBAR_SESSION_TAG_OPTIONS.find((option) => option.value === tag)?.label;
}

export function getSidebarSessionTagListItemLabel(item: SidebarSessionTagListItem): string {
  if (item.type === 'tag') {
    return getSidebarSessionTagLabel(item.tag) ?? item.tag;
  }
  if (item.type === 'untagged') {
    return 'No tag';
  }
  return 'Separator';
}

export function getSidebarSessionTagListItemFilter(
  item: SidebarSessionTagListItem
): SidebarSessionTagFilter | undefined {
  if (item.type === 'tag') {
    return item.tag;
  }
  if (item.type === 'untagged') {
    return SIDEBAR_SESSION_TAG_FILTER_UNTAGGED;
  }
  return undefined;
}

export function sessionMatchesSidebarTagFilters(
  session: {
    isFavorite?: boolean;
    sessionTag?: SidebarSessionTag;
  },
  selectedFilters: readonly SidebarSessionTagFilter[]
): boolean {
  if (selectedFilters.length === 0) {
    return true;
  }
  const sessionTag = getEffectiveSidebarSessionTag(session);
  if (sessionTag === undefined) {
    return selectedFilters.includes(SIDEBAR_SESSION_TAG_FILTER_UNTAGGED);
  }
  return selectedFilters.includes(sessionTag);
}

export function getEffectiveSidebarSessionTag(input: {
  isFavorite?: boolean;
  sessionTag?: SidebarSessionTag;
}): SidebarSessionTag | undefined {
  return input.sessionTag ?? (input.isFavorite === true ? 'favorite' : undefined);
}

export function normalizeSidebarSessionTagListItems(candidate: unknown): SidebarSessionTagListItem[] {
  if (!Array.isArray(candidate)) {
    return cloneSidebarSessionTagListItems(DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS);
  }

  const seenIds = new Set<string>();
  const normalized: SidebarSessionTagListItem[] = [];

  for (const item of candidate) {
    const normalizedItem = normalizeSidebarSessionTagListItem(item);
    if (!normalizedItem || seenIds.has(normalizedItem.id)) {
      continue;
    }
    seenIds.add(normalizedItem.id);
    normalized.push(normalizedItem);
  }

  for (const item of DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS) {
    if (!seenIds.has(item.id)) {
      normalized.push(cloneSidebarSessionTagListItem(item));
    }
  }

  return normalized;
}

export function areSidebarSessionTagListItemsEqual(
  left: readonly SidebarSessionTagListItem[],
  right: readonly SidebarSessionTagListItem[]
): boolean {
  return (
    left.length === right.length &&
    left.every((leftItem, index) => {
      const rightItem = right[index];
      return (
        rightItem !== undefined &&
        leftItem.id === rightItem.id &&
        leftItem.type === rightItem.type &&
        leftItem.enabled === rightItem.enabled &&
        leftItem.visible === rightItem.visible &&
        (leftItem.type !== 'tag' || rightItem.type !== 'tag' || leftItem.tag === rightItem.tag)
      );
    })
  );
}

export function getEnabledVisibleSidebarSessionTagFilters(
  items: readonly SidebarSessionTagListItem[]
): SidebarSessionTagFilter[] {
  return normalizeSidebarSessionTagListItems(items).flatMap((item) => {
    const filter = getSidebarSessionTagListItemFilter(item);
    return item.enabled && item.visible && filter ? [filter] : [];
  });
}

export function getEnabledVisibleSidebarSessionTags(items: readonly SidebarSessionTagListItem[]): SidebarSessionTag[] {
  return getEnabledVisibleSidebarSessionTagFilters(items).filter(isSidebarSessionTag);
}

export function getEnabledVisibleSidebarSessionTagSections(
  items: unknown,
  options: { includeTags?: readonly SidebarSessionTag[] } = {}
): SidebarSessionTagSection[] {
  /*
   * CDXC:Sessions 2026-06-15-22:33:
   * Every tag-selection surface should derive its menu sections from the same
   * enabled-and-visible sidebar tag list. This keeps Previous Sessions filters
   * and session-card Tag as menus aligned with Settings Reset to Default while
   * still allowing a caller to include an already-selected hidden tag for
   * removal.
   */
  const visibleTagSet = new Set<SidebarSessionTag>();
  for (const item of normalizeSidebarSessionTagListItems(items)) {
    if (item.type === 'tag' && item.enabled && item.visible) {
      visibleTagSet.add(item.tag);
    }
  }
  for (const tag of options.includeTags ?? []) {
    visibleTagSet.add(tag);
  }

  return SIDEBAR_SESSION_TAG_SECTIONS.map((section) => ({
    ...section,
    options: section.options.filter((option) => visibleTagSet.has(option.value)),
  })).filter((section) => section.options.length > 0);
}

function createDefaultSidebarSessionTagListTagItem(tag: SidebarSessionTag): SidebarSessionTagListItem {
  const isDefaultOff = DEFAULT_OFF_SIDEBAR_SESSION_TAG_FILTERS.has(tag);
  return {
    enabled: !isDefaultOff,
    id: tag,
    tag,
    type: 'tag',
    visible: !isDefaultOff,
  };
}

function createDefaultSidebarSessionTagListSeparatorItem(
  id: SidebarSessionTagListSeparatorId
): SidebarSessionTagListItem {
  return {
    enabled: true,
    id,
    type: 'separator',
    visible: true,
  };
}

function createDefaultSidebarSessionTagListUntaggedItem(): SidebarSessionTagListItem {
  return {
    enabled: true,
    id: SIDEBAR_SESSION_TAG_LIST_UNTAGGED_ID,
    type: 'untagged',
    visible: true,
  };
}

function normalizeSidebarSessionTagListItem(candidate: unknown): SidebarSessionTagListItem | undefined {
  if (!isRecord(candidate)) {
    return undefined;
  }

  const id = readLooseString(candidate.id);
  const tag = normalizeSidebarSessionTag(candidate.tag) ?? normalizeSidebarSessionTag(id);
  if (tag) {
    return {
      enabled: readBoolean(candidate.enabled, true),
      id: tag,
      tag,
      type: 'tag',
      visible: readBoolean(candidate.visible, true),
    };
  }

  if (id === SIDEBAR_SESSION_TAG_LIST_UNTAGGED_ID || candidate.type === 'untagged') {
    return {
      enabled: readBoolean(candidate.enabled, true),
      id: SIDEBAR_SESSION_TAG_LIST_UNTAGGED_ID,
      type: 'untagged',
      visible: readBoolean(candidate.visible, true),
    };
  }

  if (SIDEBAR_SESSION_TAG_LIST_SEPARATOR_SET.has(id)) {
    return {
      enabled: readBoolean(candidate.enabled, true),
      id: id as SidebarSessionTagListSeparatorId,
      type: 'separator',
      visible: readBoolean(candidate.visible, true),
    };
  }

  return undefined;
}

function cloneSidebarSessionTagListItems(items: readonly SidebarSessionTagListItem[]): SidebarSessionTagListItem[] {
  return items.map(cloneSidebarSessionTagListItem);
}

function cloneSidebarSessionTagListItem(item: SidebarSessionTagListItem): SidebarSessionTagListItem {
  if (item.type === 'tag') {
    return { enabled: item.enabled, id: item.id, tag: item.tag, type: 'tag', visible: item.visible };
  }
  if (item.type === 'untagged') {
    return { enabled: item.enabled, id: item.id, type: 'untagged', visible: item.visible };
  }
  return { enabled: item.enabled, id: item.id, type: 'separator', visible: item.visible };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function readBoolean(value: unknown, fallback: boolean): boolean {
  return typeof value === 'boolean' ? value : fallback;
}

function readLooseString(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

export function getRestoredPreviousSessionTag(input: {
  isFavorite?: boolean;
  sessionTag?: SidebarSessionTag | null;
}): SidebarSessionTag | undefined {
  /*
  CDXC:Sessions 2026-06-06-05:29:
  Restoring a Previous Sessions row must keep the user's durable tag marker attached to the recreated session. Legacy Favorite-only rows restore as the `favorite` tag so the new tag model does not lose older session intent.
  */
  return normalizeSidebarSessionTag(input.sessionTag) ?? (input.isFavorite === true ? 'favorite' : undefined);
}

export function getRestoredPreviousSessionSidebarOrder(input: { sidebarOrder?: number | null }): number | undefined {
  /*
  CDXC:Sessions 2026-06-06-05:29:
  Previous-session restore should return near the old manual sidebar position only when gxserver has a saved positive sidebarOrder from an explicit manual/pinned order. Rows without a saved manual position keep the normal new-session order at the top.
  */
  return typeof input.sidebarOrder === 'number' && Number.isFinite(input.sidebarOrder) && input.sidebarOrder > 0
    ? input.sidebarOrder
    : undefined;
}
