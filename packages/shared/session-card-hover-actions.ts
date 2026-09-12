/**
 * CDXC:Sessions 2026-09-12 DECISION:
 * User: Settings gets a "Buttons to show on hover for sessions" strip with Rename, Pin, Note, Snooze, Close After Done, Tag, Park, Sleep and Close as icon toggles, plus the chevron as one more draggable item.
 * User: the strip is reordered by dragging; whatever sits to the right of the chevron is always shown on hover, whatever sits to its left is hidden until the chevron is clicked. Turning the chevron off shows every enabled button at once.
 * User: the default strip is Tag, Park, Sleep, chevron, Close: only Close shows at rest, and the chevron reveals the other three. Fork, Split Right and Delayed Send are deliberately not offered (too rare to earn a slot).
 * User: a button that is enabled here is hidden from the session's main context menu, whichever side of the chevron it is on, so each action lives in exactly one of the two places.
 * SEE-ALSO: packages/core-ui/session-card-content.tsx, packages/core-ui/sortable-session-card.tsx, packages/core-ui/settings-modal/session-card-hover-actions-field.tsx.
 */
export const SESSION_CARD_HOVER_ACTIONS = [
  'rename',
  'pin',
  'note',
  'snooze',
  'closeAfterDone',
  'tag',
  'park',
  'sleep',
  'close',
] as const;

export type SessionCardHoverAction = (typeof SESSION_CARD_HOVER_ACTIONS)[number];

export const SESSION_CARD_HOVER_CHEVRON_ID = 'chevron';

export type SessionCardHoverButtonId = SessionCardHoverAction | typeof SESSION_CARD_HOVER_CHEVRON_ID;

export type SessionCardHoverButtonItem = {
  enabled: boolean;
  id: SessionCardHoverButtonId;
};

export const SESSION_CARD_HOVER_BUTTON_IDS: readonly SessionCardHoverButtonId[] = [
  ...SESSION_CARD_HOVER_ACTIONS,
  SESSION_CARD_HOVER_CHEVRON_ID,
];

export const DEFAULT_SESSION_CARD_HOVER_BUTTONS: readonly SessionCardHoverButtonItem[] = [
  { enabled: false, id: 'rename' },
  { enabled: false, id: 'pin' },
  { enabled: false, id: 'note' },
  { enabled: false, id: 'snooze' },
  { enabled: false, id: 'closeAfterDone' },
  { enabled: true, id: 'tag' },
  { enabled: true, id: 'park' },
  { enabled: true, id: 'sleep' },
  { enabled: true, id: 'chevron' },
  { enabled: true, id: 'close' },
];

export const SESSION_CARD_HOVER_BUTTON_LABELS: Record<SessionCardHoverButtonId, string> = {
  chevron: 'Chevron (hides the buttons to its left)',
  close: 'Close',
  closeAfterDone: 'Close After Done',
  note: 'Note',
  park: 'Park',
  pin: 'Pin',
  rename: 'Rename',
  sleep: 'Sleep',
  snooze: 'Snooze',
  tag: 'Tag',
};

export function isSessionCardHoverAction(candidate: unknown): candidate is SessionCardHoverAction {
  return typeof candidate === 'string' && (SESSION_CARD_HOVER_ACTIONS as readonly string[]).includes(candidate);
}

export function isSessionCardHoverButtonId(candidate: unknown): candidate is SessionCardHoverButtonId {
  return typeof candidate === 'string' && (SESSION_CARD_HOVER_BUTTON_IDS as readonly string[]).includes(candidate);
}

/**
 * Accepts the stored list (`{ id, enabled }` entries in display order) and the earlier plain
 * list of enabled action ids. Unknown ids and duplicates are dropped; ids missing from the
 * stored list are appended in default order, disabled, so a newly added button never appears
 * on cards until the user turns it on. A plain-id list keeps the default order and marks the
 * listed ids enabled.
 */
export function normalizeSessionCardHoverButtons(candidate: unknown): readonly SessionCardHoverButtonItem[] {
  if (!Array.isArray(candidate)) {
    return DEFAULT_SESSION_CARD_HOVER_BUTTONS;
  }
  if (candidate.every((entry) => typeof entry === 'string')) {
    const enabled = new Set(candidate.filter(isSessionCardHoverAction));
    return DEFAULT_SESSION_CARD_HOVER_BUTTONS.map((item) =>
      item.id === SESSION_CARD_HOVER_CHEVRON_ID ? item : { enabled: enabled.has(item.id), id: item.id }
    );
  }
  const items: SessionCardHoverButtonItem[] = [];
  const seen = new Set<SessionCardHoverButtonId>();
  for (const entry of candidate) {
    if (!entry || typeof entry !== 'object') {
      continue;
    }
    const { enabled, id } = entry as { enabled?: unknown; id?: unknown };
    if (!isSessionCardHoverButtonId(id) || seen.has(id)) {
      continue;
    }
    seen.add(id);
    items.push({ enabled: enabled === true, id });
  }
  for (const item of DEFAULT_SESSION_CARD_HOVER_BUTTONS) {
    if (!seen.has(item.id)) {
      items.push({ enabled: false, id: item.id });
    }
  }
  return items;
}

export function setSessionCardHoverButtonEnabled(
  items: readonly SessionCardHoverButtonItem[],
  id: SessionCardHoverButtonId,
  enabled: boolean
): readonly SessionCardHoverButtonItem[] {
  return items.map((item) => (item.id === id ? { ...item, enabled } : item));
}

export function moveSessionCardHoverButton(
  items: readonly SessionCardHoverButtonItem[],
  fromIndex: number,
  toIndex: number
): readonly SessionCardHoverButtonItem[] {
  const next = [...items];
  const [moved] = next.splice(fromIndex, 1);
  if (moved === undefined) {
    return items;
  }
  next.splice(toIndex, 0, moved);
  return next;
}

export function areSessionCardHoverButtonsEqual(
  left: readonly SessionCardHoverButtonItem[],
  right: readonly SessionCardHoverButtonItem[]
): boolean {
  return (
    left.length === right.length &&
    left.every((item, index) => item.id === right[index]?.id && item.enabled === right[index]?.enabled)
  );
}

/** Every enabled action, in strip order, regardless of the chevron. */
export function getEnabledSessionCardHoverActions(
  items: readonly SessionCardHoverButtonItem[]
): readonly SessionCardHoverAction[] {
  return items.flatMap((item) => (item.enabled && item.id !== SESSION_CARD_HOVER_CHEVRON_ID ? [item.id] : []));
}

/**
 * The strip split at the chevron: `before` is hidden while collapsed, `after` is always shown.
 * With the chevron disabled everything lands in `after`.
 */
export function splitSessionCardHoverButtons(items: readonly SessionCardHoverButtonItem[]): {
  after: readonly SessionCardHoverAction[];
  before: readonly SessionCardHoverAction[];
  chevron: boolean;
} {
  const chevronIndex = items.findIndex((item) => item.id === SESSION_CARD_HOVER_CHEVRON_ID);
  const chevron = chevronIndex >= 0 && items[chevronIndex]!.enabled;
  if (!chevron) {
    return { after: getEnabledSessionCardHoverActions(items), before: [], chevron: false };
  }
  return {
    after: getEnabledSessionCardHoverActions(items.slice(chevronIndex + 1)),
    before: getEnabledSessionCardHoverActions(items.slice(0, chevronIndex)),
    chevron: true,
  };
}
