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

export type BuiltinSidebarSessionTag = (typeof SIDEBAR_SESSION_TAGS)[number];

/**
 * CDXC:Sessions 2026-09-11 DECISION:
 * User: users can define their own session tags (a name, an icon from the shared icon allowlist, and a color from a preset list) and sort them, without editing the built-in tags.
 * A custom tag is one more value of the same single session marker: its id is `custom-` plus a random token, gxserver owns the catalog per daemon (name, icon, color, order) exactly like Spaces, and the persisted `sessionTag` stores the id.
 * The phone renders custom tags from the hex color and icon id shipped in the catalog, so it needs no palette mirror.
 * SEE-ALSO: server/src/custom_session_tags.rs, packages/core-ui/session-tag-ui.tsx, apps/mobile/app/src/contract/sessionTags.ts.
 */
export const CUSTOM_SESSION_TAG_ID_PREFIX = 'custom-';

export type CustomSessionTagId = `custom-${string}`;

export type SidebarSessionTag = BuiltinSidebarSessionTag | CustomSessionTagId;

const CUSTOM_SESSION_TAG_ID_PATTERN = /^custom-[a-z0-9]{4,40}$/;

export function isCustomSessionTagId(value: unknown): value is CustomSessionTagId {
  return typeof value === 'string' && CUSTOM_SESSION_TAG_ID_PATTERN.test(value);
}

export function createCustomSessionTagId(): CustomSessionTagId {
  const random = Math.random().toString(36).slice(2, 8).padEnd(6, '0');
  return `${CUSTOM_SESSION_TAG_ID_PREFIX}${Date.now().toString(36)}${random}`;
}

export type CustomSessionTag = {
  /** `#rrggbb`, lowercase. */
  color: string;
  /** A SIDEBAR_COMMAND_ICON_IDS id. Unknown ids render as the default tag glyph. */
  icon: string;
  name: string;
  tagId: CustomSessionTagId;
};

export type CustomSessionTagsState = {
  order: readonly CustomSessionTagId[];
  tags: Readonly<Record<string, CustomSessionTag>>;
};

export const EMPTY_CUSTOM_SESSION_TAGS_STATE: CustomSessionTagsState = { order: [], tags: {} };

export const MAX_CUSTOM_SESSION_TAGS = 64;
export const MAX_CUSTOM_SESSION_TAG_NAME_LENGTH = 40;
export const DEFAULT_CUSTOM_SESSION_TAG_ICON = 'sparkles';
export const DEFAULT_CUSTOM_SESSION_TAG_NAME = 'Tag';

/**
 * CDXC:Sessions 2026-09-11 DECISION:
 * User: custom tag colors come from a preset list. The presets are the built-in tag hues, which are already tuned to read as a 15px stroke glyph on the dark sidebar; the collection palette's dim tones vanish at that size.
 * Mirrored in server/src/custom_session_tags.rs for the server-side fallback rotation.
 */
export const SESSION_TAG_COLOR_PRESETS = [
  { label: 'Gold', value: '#f3cc5f' },
  { label: 'Coral', value: '#ff8b6b' },
  { label: 'Amber', value: '#f0c66e' },
  { label: 'Mint', value: '#4ee6b8' },
  { label: 'Sky', value: '#59d9ff' },
  { label: 'Ice', value: '#95d7f6' },
  { label: 'Blue', value: '#8fb8ff' },
  { label: 'Lavender', value: '#d2a7ff' },
  { label: 'Pink', value: '#ff9ee7' },
  { label: 'Rose', value: '#ff5f73' },
  { label: 'Brick', value: '#a54646' },
  { label: 'Silver', value: '#d9dee6' },
  { label: 'Gray', value: '#8e949d' },
] as const;

export type SessionTagColorPreset = (typeof SESSION_TAG_COLOR_PRESETS)[number]['value'];

const HEX_COLOR_PATTERN = /^#[0-9a-f]{6}$/;

export function normalizeSessionTagColor(value: unknown, fallbackIndex: number): string {
  if (typeof value === 'string') {
    const color = value.trim().toLowerCase();
    if (HEX_COLOR_PATTERN.test(color)) {
      return color;
    }
  }
  return SESSION_TAG_COLOR_PRESETS[Math.abs(fallbackIndex) % SESSION_TAG_COLOR_PRESETS.length]!.value;
}

export function normalizeCustomSessionTagName(value: unknown): string {
  const name = typeof value === 'string' ? value.trim().replace(/\s+/g, ' ') : '';
  return (name || DEFAULT_CUSTOM_SESSION_TAG_NAME).slice(0, MAX_CUSTOM_SESSION_TAG_NAME_LENGTH);
}

/**
 * Mirrors `normalize_custom_session_tags_state` in server/src/custom_session_tags.rs: the order array is authoritative, tags missing from it append in map order, and every kept tag has a bounded name, an icon id, and a lowercase `#rrggbb` color.
 */
export function normalizeCustomSessionTagsState(candidate: unknown): CustomSessionTagsState {
  if (!isRecord(candidate)) {
    return EMPTY_CUSTOM_SESSION_TAGS_STATE;
  }
  const rawTags = isRecord(candidate.tags) ? candidate.tags : {};
  const candidates = new Map<CustomSessionTagId, Record<string, unknown>>();
  for (const [rawId, rawTag] of Object.entries(rawTags)) {
    const tagId = rawId.trim();
    if (!isCustomSessionTagId(tagId) || !isRecord(rawTag) || candidates.has(tagId)) {
      continue;
    }
    candidates.set(tagId, rawTag);
  }
  const orderedIds: CustomSessionTagId[] = [];
  const seen = new Set<string>();
  for (const entry of Array.isArray(candidate.order) ? candidate.order : []) {
    const id = typeof entry === 'string' ? entry.trim() : '';
    if (isCustomSessionTagId(id) && candidates.has(id) && !seen.has(id)) {
      seen.add(id);
      orderedIds.push(id);
    }
  }
  for (const id of candidates.keys()) {
    if (!seen.has(id)) {
      seen.add(id);
      orderedIds.push(id);
    }
  }
  const order: CustomSessionTagId[] = [];
  const tags: Record<string, CustomSessionTag> = {};
  for (const tagId of orderedIds) {
    if (order.length >= MAX_CUSTOM_SESSION_TAGS) {
      break;
    }
    const raw = candidates.get(tagId)!;
    const icon =
      typeof raw.icon === 'string' && raw.icon.trim() ? raw.icon.trim().slice(0, 64) : DEFAULT_CUSTOM_SESSION_TAG_ICON;
    tags[tagId] = {
      color: normalizeSessionTagColor(raw.color, order.length),
      icon,
      name: normalizeCustomSessionTagName(raw.name),
      tagId,
    };
    order.push(tagId);
  }
  return { order, tags };
}

export function listCustomSessionTags(state: CustomSessionTagsState | undefined): CustomSessionTag[] {
  if (!state) {
    return [];
  }
  return state.order.flatMap((tagId) => {
    const tag = state.tags[tagId];
    return tag ? [tag] : [];
  });
}

export function areCustomSessionTagsStatesEqual(
  left: CustomSessionTagsState | undefined,
  right: CustomSessionTagsState | undefined
): boolean {
  if (left === right) {
    return true;
  }
  if (!left || !right || left.order.length !== right.order.length) {
    return false;
  }
  return left.order.every((tagId, index) => {
    const leftTag = left.tags[tagId];
    const rightTag = right.tags[right.order[index]!];
    return (
      tagId === right.order[index] &&
      leftTag !== undefined &&
      rightTag !== undefined &&
      leftTag.color === rightTag.color &&
      leftTag.icon === rightTag.icon &&
      leftTag.name === rightTag.name
    );
  });
}

/**
 * Resolve a custom tag id against one or more daemon catalogs. Ids are random tokens, so looking a remote session's tag up across every connected daemon's catalog cannot collide in practice, and callers need no session-to-machine mapping.
 */
export function findCustomSessionTag(
  tagId: string | undefined,
  states: readonly (CustomSessionTagsState | undefined)[]
): CustomSessionTag | undefined {
  if (!isCustomSessionTagId(tagId)) {
    return undefined;
  }
  for (const state of states) {
    const tag = state?.tags[tagId];
    if (tag) {
      return tag;
    }
  }
  return undefined;
}

export function createCustomSessionTag(
  state: CustomSessionTagsState,
  tag: { color?: string; icon?: string; name: string }
): { state: CustomSessionTagsState; tagId: CustomSessionTagId } {
  const tagId = createCustomSessionTagId();
  return {
    state: normalizeCustomSessionTagsState({
      order: [...state.order, tagId],
      tags: {
        ...state.tags,
        [tagId]: {
          color: normalizeSessionTagColor(tag.color, state.order.length),
          icon: tag.icon ?? DEFAULT_CUSTOM_SESSION_TAG_ICON,
          name: tag.name,
          tagId,
        },
      },
    }),
    tagId,
  };
}

export function deleteCustomSessionTag(state: CustomSessionTagsState, tagId: string): CustomSessionTagsState {
  const { [tagId]: _removed, ...tags } = state.tags;
  return normalizeCustomSessionTagsState({ order: state.order.filter((id) => id !== tagId), tags });
}

export function reorderCustomSessionTags(
  state: CustomSessionTagsState,
  orderedTagIds: readonly string[]
): CustomSessionTagsState {
  return normalizeCustomSessionTagsState({ order: orderedTagIds, tags: state.tags });
}

/** Catalogs a tag-rendering surface can resolve custom ids against: the local daemon's plus every connected remote daemon's. */
export type CustomSessionTagCatalogs = readonly (CustomSessionTagsState | undefined)[];

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
  label: 'Priority' | 'Progress' | 'Type' | 'Custom';
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

export function isBuiltinSidebarSessionTag(value: unknown): value is BuiltinSidebarSessionTag {
  return typeof value === 'string' && SIDEBAR_SESSION_TAG_SET.has(value);
}

export function isSidebarSessionTag(value: unknown): value is SidebarSessionTag {
  return isBuiltinSidebarSessionTag(value) || isCustomSessionTagId(value);
}

export function isSidebarSessionTagFilter(value: unknown): value is SidebarSessionTagFilter {
  return isSidebarSessionTag(value) || value === SIDEBAR_SESSION_TAG_FILTER_UNTAGGED;
}

export function normalizeSidebarSessionTag(value: unknown): SidebarSessionTag | undefined {
  return isSidebarSessionTag(value) ? value : undefined;
}

export const UNKNOWN_CUSTOM_SESSION_TAG_LABEL = 'Custom tag';

export function getSidebarSessionTagLabel(
  tag: SidebarSessionTagFilter | undefined,
  customTags: CustomSessionTagCatalogs = []
): string | undefined {
  if (tag === SIDEBAR_SESSION_TAG_FILTER_UNTAGGED) {
    return 'No tag';
  }
  if (isCustomSessionTagId(tag)) {
    return findCustomSessionTag(tag, customTags)?.name ?? UNKNOWN_CUSTOM_SESSION_TAG_LABEL;
  }
  return SIDEBAR_SESSION_TAG_OPTIONS.find((option) => option.value === tag)?.label;
}

export function getSidebarSessionTagListItemLabel(
  item: SidebarSessionTagListItem,
  customTags: CustomSessionTagCatalogs = []
): string {
  if (item.type === 'tag') {
    return getSidebarSessionTagLabel(item.tag, customTags) ?? item.tag;
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

/**
 * CDXC:Sessions 2026-09-11 WHY:
 * The tag filter list is a client setting, so it keeps custom tag ids by shape when it is normalized without a catalog (settings persistence), and only a caller that has the daemon catalog drops ids the daemon no longer knows and inserts newly created tags. New custom tags land just before the No tag separator, enabled and visible, so they show up in menus the moment they are created.
 */
export function normalizeSidebarSessionTagListItems(
  candidate: unknown,
  customTags?: CustomSessionTagsState
): SidebarSessionTagListItem[] {
  const seenIds = new Set<string>();
  const normalized: SidebarSessionTagListItem[] = [];

  for (const item of Array.isArray(candidate) ? candidate : []) {
    const normalizedItem = normalizeSidebarSessionTagListItem(item);
    if (!normalizedItem || seenIds.has(normalizedItem.id)) {
      continue;
    }
    if (customTags && normalizedItem.type === 'tag' && isCustomSessionTagId(normalizedItem.tag)) {
      if (!customTags.tags[normalizedItem.tag]) {
        continue;
      }
    }
    seenIds.add(normalizedItem.id);
    normalized.push(normalizedItem);
  }

  for (const item of DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS) {
    if (!seenIds.has(item.id)) {
      normalized.push(cloneSidebarSessionTagListItem(item));
    }
  }

  if (customTags) {
    const missing = customTags.order.filter((tagId) => !seenIds.has(tagId));
    if (missing.length > 0) {
      const separatorIndex = normalized.findIndex((item) => item.id === 'separator-type-untagged');
      const untaggedIndex = normalized.findIndex((item) => item.type === 'untagged');
      const insertAt = separatorIndex >= 0 ? separatorIndex : untaggedIndex >= 0 ? untaggedIndex : normalized.length;
      normalized.splice(
        insertAt,
        0,
        ...missing.map((tagId): SidebarSessionTagListItem => ({
          enabled: true,
          id: tagId,
          tag: tagId,
          type: 'tag',
          visible: true,
        }))
      );
    }
  }

  return normalized;
}

/** The relative order of the custom tags inside a tag filter list, which is written back to the daemon catalog. */
export function getCustomSessionTagOrderFromListItems(
  items: readonly SidebarSessionTagListItem[]
): CustomSessionTagId[] {
  return items.flatMap((item) => (item.type === 'tag' && isCustomSessionTagId(item.tag) ? [item.tag] : []));
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
  options: { customTags?: CustomSessionTagsState; includeTags?: readonly SidebarSessionTag[] } = {}
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
  const visibleCustomTagIds: CustomSessionTagId[] = [];
  for (const item of normalizeSidebarSessionTagListItems(items, options.customTags)) {
    if (item.type === 'tag' && item.enabled && item.visible) {
      visibleTagSet.add(item.tag);
      if (isCustomSessionTagId(item.tag)) {
        visibleCustomTagIds.push(item.tag);
      }
    }
  }
  for (const tag of options.includeTags ?? []) {
    if (visibleTagSet.has(tag)) {
      continue;
    }
    visibleTagSet.add(tag);
    if (isCustomSessionTagId(tag)) {
      visibleCustomTagIds.push(tag);
    }
  }

  const sections: SidebarSessionTagSection[] = SIDEBAR_SESSION_TAG_SECTIONS.map((section) => ({
    ...section,
    options: section.options.filter((option) => visibleTagSet.has(option.value)),
  }));
  const customOptions: SidebarSessionTagOption[] = visibleCustomTagIds.flatMap((tagId) => {
    const tag = options.customTags?.tags[tagId];
    return tag ? [{ label: tag.name, value: tag.tagId }] : [];
  });
  sections.push({ label: 'Custom', options: customOptions });
  return sections.filter((section) => section.options.length > 0);
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
