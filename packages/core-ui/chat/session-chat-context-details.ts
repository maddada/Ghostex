/*
CDXC:SessionChatDetectedOptions 2026-09-04 DECISION:
User: the context meter popover gets a "More details" section under the Compact
button, with rows grouped under "Usage & cost", "Context & cache" and "Session".
A pen icon opens a dialog to show/hide rows and reorder them within their group
only (rows never cross a group). Any row, shown or not, can be starred, and the
starred values render as one wrapping text line under the chat box
(`9.0% • $49.28 • …/Ghostex`), each with its title on hover. A group label is
never rendered without at least one row under it. The catalog of rows and the
localStorage-backed preferences live here; the popover, the dialog and the
status line only render what `resolveSessionChatContextDetailGroups` returns.
*/

import { useEffect, useState, useSyncExternalStore } from 'react';
import type { SessionChatClaudeStatus } from '../../shared/session-chat';
import { formatSessionChatContextTokens } from './session-chat-context-meter';

export type SessionChatContextDetailGroupId = 'usage' | 'context' | 'session';

export const SESSION_CHAT_CONTEXT_DETAIL_GROUPS: ReadonlyArray<{ id: SessionChatContextDetailGroupId; label: string }> =
  [
    { id: 'usage', label: 'Usage & cost' },
    { id: 'context', label: 'Context & cache' },
    { id: 'session', label: 'Session' },
  ];

export type SessionChatContextDetailRowId =
  | 'cost'
  | 'rateLimits'
  | 'lines'
  | 'promptCache'
  | 'lastRequest'
  | 'totalOutputTokens'
  | 'remaining'
  | 'cacheMisses'
  | 'thinking'
  | 'version'
  | 'outputStyle'
  | 'sessionName'
  | 'repo'
  | 'folder'
  | 'pr';

/**
 * Ghostex's own view of the session for the session row. User: the title and
 * id come from Ghostex data (the sidebar title, the agent session id on the
 * chat read state), not from Claude's payload.
 */
export interface SessionChatContextDetailSession {
  /** The sidebar title, null while the session has none. */
  title: string | null;
  /** Claude's conversation id (`claude --resume` takes it), null until it resolves. */
  agentSessionId: string | null;
  /** No prompt has reached the agent yet, so the id is not one worth copying. */
  draft: boolean;
}

export interface SessionChatContextDetailRowInput {
  status: SessionChatClaudeStatus;
  /** Milliseconds since the epoch, for the reset and expiry countdowns. */
  now: number;
  /** Null when the host did not describe the session; the session row is skipped. */
  session: SessionChatContextDetailSession | null;
}

export interface SessionChatContextDetailRowDefinition {
  id: SessionChatContextDetailRowId;
  group: SessionChatContextDetailGroupId;
  label: string;
  description: string;
  /** Shown in the popover on a fresh install. Starred is never a default. */
  recommended: boolean;
  /** Null when Claude did not report what the row needs; the row is skipped. */
  value: (input: SessionChatContextDetailRowInput) => string | null;
  /**
   * Text a click on the status line item copies, with the toast title. User:
   * clicking the session name in the status line copies the session id.
   */
  copy?: (input: SessionChatContextDetailRowInput) => { text: string; label: string } | null;
}

const SEPARATOR = ' · ';

function isFinite(value: number | undefined): value is number {
  return typeof value === 'number' && Number.isFinite(value);
}

function formatUsd(value: number): string {
  return `$${value.toFixed(2)}`;
}

/** `1h 05m`, `22m`, `45s`. */
export function formatSessionChatDuration(ms: number): string {
  const totalSeconds = Math.max(0, Math.round(ms / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) {
    return `${hours}h ${String(minutes).padStart(2, '0')}m`;
  }
  if (minutes > 0) {
    return `${minutes}m`;
  }
  return `${seconds}s`;
}

/** Time left until an epoch-seconds instant, or null once it has passed. */
function formatCountdown(epochSeconds: number, now: number): string | null {
  const remainingMs = epochSeconds * 1000 - now;
  if (remainingMs <= 0) {
    return null;
  }
  return formatSessionChatDuration(remainingMs);
}

function formatPercentage(value: number): string {
  return value < 10 ? `${value.toFixed(1).replace(/\.0$/, '')}%` : `${Math.round(value)}%`;
}

function joinParts(parts: ReadonlyArray<string | null | undefined>): string | null {
  const present = parts.filter((part): part is string => typeof part === 'string' && part.length > 0);
  return present.length > 0 ? present.join(SEPARATOR) : null;
}

function baseName(path: string): string {
  const trimmed = path.replace(/[\\/]+$/u, '');
  const name = trimmed.split(/[\\/]/u).pop();
  return name && name.length > 0 ? name : trimmed;
}

export const SESSION_CHAT_CONTEXT_DETAIL_ROWS: readonly SessionChatContextDetailRowDefinition[] = [
  {
    id: 'cost',
    group: 'usage',
    label: 'Cost',
    description: 'Total spend, session time, API time',
    recommended: true,
    value: ({ status }) =>
      joinParts([
        isFinite(status.cost?.totalUsd) ? formatUsd(status.cost.totalUsd) : null,
        isFinite(status.cost?.durationMs) ? formatSessionChatDuration(status.cost.durationMs) : null,
        isFinite(status.cost?.apiDurationMs) ? `API ${formatSessionChatDuration(status.cost.apiDurationMs)}` : null,
      ]),
  },
  {
    id: 'rateLimits',
    group: 'usage',
    label: 'Rate limits',
    description: '5h and 7d usage, 5h reset countdown',
    recommended: true,
    value: ({ status, now }) => {
      const fiveHour = status.rateLimits?.fiveHour;
      const sevenDay = status.rateLimits?.sevenDay;
      const reset = isFinite(fiveHour?.resetsAt) ? formatCountdown(fiveHour.resetsAt, now) : null;
      return joinParts([
        isFinite(fiveHour?.usedPercentage) ? `5h ${formatPercentage(fiveHour.usedPercentage)}` : null,
        isFinite(sevenDay?.usedPercentage) ? `7d ${formatPercentage(sevenDay.usedPercentage)}` : null,
        reset ? `resets ${reset}` : null,
      ]);
    },
  },
  {
    id: 'lines',
    group: 'usage',
    label: 'Lines changed',
    description: 'Added and removed this session',
    recommended: true,
    value: ({ status }) =>
      isFinite(status.cost?.linesAdded) || isFinite(status.cost?.linesRemoved)
        ? `+${status.cost?.linesAdded ?? 0} / −${status.cost?.linesRemoved ?? 0}`
        : null,
  },
  {
    id: 'promptCache',
    group: 'context',
    label: 'Prompt cache',
    description: 'Warm state, TTL left, hit ratio',
    recommended: true,
    value: ({ status, now }) => {
      const cache = status.promptCache;
      if (!cache || (cache.warm === undefined && !isFinite(cache.hitRatio))) {
        return null;
      }
      const left = cache.warm && isFinite(cache.expiresAt) ? formatCountdown(cache.expiresAt, now) : null;
      return joinParts([
        cache.warm === undefined ? null : cache.warm ? 'warm' : 'cold',
        left ? `${left} left` : null,
        isFinite(cache.hitRatio) ? `${Math.round(cache.hitRatio * 100)}% hits` : null,
      ]);
    },
  },
  {
    id: 'lastRequest',
    group: 'context',
    label: 'Last request',
    description: 'Input, output and cached tokens',
    recommended: true,
    value: ({ status }) => {
      const request = status.lastRequest;
      return joinParts([
        isFinite(request?.inputTokens) ? `${formatSessionChatContextTokens(request.inputTokens)} in` : null,
        isFinite(request?.outputTokens) ? `${formatSessionChatContextTokens(request.outputTokens)} out` : null,
        isFinite(request?.cacheReadTokens) ? `${formatSessionChatContextTokens(request.cacheReadTokens)} cached` : null,
      ]);
    },
  },
  {
    id: 'totalOutputTokens',
    group: 'context',
    label: 'Total output tokens',
    description: 'Everything Claude wrote this session',
    recommended: false,
    value: ({ status }) =>
      isFinite(status.totalOutputTokens) ? formatSessionChatContextTokens(status.totalOutputTokens) : null,
  },
  {
    id: 'remaining',
    group: 'context',
    label: 'Remaining context',
    description: 'Free share of the window before it compacts',
    recommended: false,
    value: ({ status }) => (isFinite(status.remainingPercentage) ? formatPercentage(status.remainingPercentage) : null),
  },
  {
    id: 'cacheMisses',
    group: 'context',
    label: 'Cache misses',
    description: 'Count and the last miss cause',
    recommended: false,
    value: ({ status }) => {
      const cache = status.promptCache;
      return isFinite(cache?.misses)
        ? joinParts([`${cache.misses}`, cache.lastMissCause ? `last: ${cache.lastMissCause}` : null])
        : null;
    },
  },
  {
    id: 'thinking',
    group: 'session',
    label: 'Thinking',
    description: 'Whether extended thinking is on',
    recommended: true,
    value: ({ status }) => (status.thinkingEnabled === undefined ? null : status.thinkingEnabled ? 'on' : 'off'),
  },
  {
    id: 'version',
    group: 'session',
    label: 'Claude Code version',
    description: 'The CLI build running this session',
    recommended: true,
    value: ({ status }) => status.version ?? null,
  },
  {
    id: 'outputStyle',
    group: 'session',
    label: 'Output style',
    description: "Claude's active output style",
    recommended: false,
    value: ({ status }) => status.outputStyle ?? null,
  },
  {
    id: 'sessionName',
    group: 'session',
    label: 'Session title',
    description: 'The sidebar title, or the session id until there is one',
    recommended: false,
    // User: the id stands in until the session has a title, and a draft
    // (nothing sent yet) says so instead of showing an id that will not be resumed.
    value: ({ session }) =>
      session === null ? null : session.draft ? 'Draft session' : (session.title ?? session.agentSessionId),
    copy: ({ session }) =>
      session !== null && !session.draft && session.agentSessionId !== null
        ? { text: session.agentSessionId, label: 'Session id copied' }
        : null,
  },
  {
    id: 'repo',
    group: 'session',
    label: 'Repository',
    description: 'Owner and name of the git repository',
    recommended: false,
    value: ({ status }) => {
      const repo = status.repo;
      if (!repo?.name) {
        return null;
      }
      return repo.owner ? `${repo.owner}/${repo.name}` : repo.name;
    },
  },
  {
    id: 'folder',
    group: 'session',
    label: 'Folder',
    description: "Claude's current working folder",
    recommended: false,
    value: ({ status }) => {
      const dir = status.currentDir ?? status.projectDir;
      return dir ? `…/${baseName(dir)}` : null;
    },
  },
  {
    id: 'pr',
    group: 'session',
    label: 'Pull request',
    description: 'Number and review state, when one exists',
    recommended: false,
    value: ({ status }) => {
      const pr = status.pr;
      return isFinite(pr?.number)
        ? joinParts([`#${pr.number}`, pr.reviewState ? pr.reviewState.replace(/_/gu, ' ').toLowerCase() : null])
        : null;
    },
  },
];

const ROW_BY_ID = new Map(SESSION_CHAT_CONTEXT_DETAIL_ROWS.map((row) => [row.id, row]));

function isRowId(value: unknown): value is SessionChatContextDetailRowId {
  return typeof value === 'string' && ROW_BY_ID.has(value as SessionChatContextDetailRowId);
}

// ---------------------------------------------------------------------------
// Preferences

export interface SessionChatContextDetailsPreferences {
  /** Row shown in the popover. Absent means the row's `recommended` flag. */
  shown: Partial<Record<SessionChatContextDetailRowId, boolean>>;
  /** Row rendered in the status line under the chat box. Absent means off. */
  starred: Partial<Record<SessionChatContextDetailRowId, boolean>>;
  /** Per-group row order; rows missing here follow in catalog order. */
  order: Partial<Record<SessionChatContextDetailGroupId, SessionChatContextDetailRowId[]>>;
  /**
   * The status line's own order, independent of the groups: starred rows
   * missing here follow in group order. User: the status line items must be
   * freely rearrangeable, so the dialog lists them in their own section.
   */
  starredOrder: SessionChatContextDetailRowId[];
}

export const SESSION_CHAT_CONTEXT_DETAILS_STORAGE_KEY = 'ghostex.chat.context-details.v1';
const CHANGED_EVENT = 'ghostex-chat-context-details-changed';

export const DEFAULT_SESSION_CHAT_CONTEXT_DETAILS_PREFERENCES: SessionChatContextDetailsPreferences = {
  shown: {},
  starred: {},
  order: {},
  starredOrder: [],
};

function normalizeFlags(candidate: unknown): Partial<Record<SessionChatContextDetailRowId, boolean>> {
  const flags: Partial<Record<SessionChatContextDetailRowId, boolean>> = {};
  if (candidate && typeof candidate === 'object' && !Array.isArray(candidate)) {
    for (const [id, flag] of Object.entries(candidate)) {
      if (isRowId(id) && typeof flag === 'boolean') {
        flags[id] = flag;
      }
    }
  }
  return flags;
}

function normalizeOrder(
  candidate: unknown
): Partial<Record<SessionChatContextDetailGroupId, SessionChatContextDetailRowId[]>> {
  const order: Partial<Record<SessionChatContextDetailGroupId, SessionChatContextDetailRowId[]>> = {};
  if (candidate && typeof candidate === 'object' && !Array.isArray(candidate)) {
    for (const group of SESSION_CHAT_CONTEXT_DETAIL_GROUPS) {
      const ids = (candidate as Record<string, unknown>)[group.id];
      if (Array.isArray(ids)) {
        const kept = ids.filter(
          (id): id is SessionChatContextDetailRowId => isRowId(id) && ROW_BY_ID.get(id)?.group === group.id
        );
        order[group.id] = [...new Set(kept)];
      }
    }
  }
  return order;
}

export function normalizeSessionChatContextDetailsPreferences(
  candidate: unknown
): SessionChatContextDetailsPreferences {
  if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) {
    return DEFAULT_SESSION_CHAT_CONTEXT_DETAILS_PREFERENCES;
  }
  const record = candidate as Record<string, unknown>;
  return {
    shown: normalizeFlags(record.shown),
    starred: normalizeFlags(record.starred),
    order: normalizeOrder(record.order),
    starredOrder: Array.isArray(record.starredOrder)
      ? [...new Set(record.starredOrder.filter((id): id is SessionChatContextDetailRowId => isRowId(id)))]
      : [],
  };
}

let cachedPreferences: SessionChatContextDetailsPreferences | null = null;
const listeners = new Set<() => void>();

function readStoredPreferences(): SessionChatContextDetailsPreferences {
  try {
    return normalizeSessionChatContextDetailsPreferences(
      JSON.parse(window.localStorage.getItem(SESSION_CHAT_CONTEXT_DETAILS_STORAGE_KEY) ?? 'null')
    );
  } catch {
    return DEFAULT_SESSION_CHAT_CONTEXT_DETAILS_PREFERENCES;
  }
}

export function readSessionChatContextDetailsPreferences(): SessionChatContextDetailsPreferences {
  if (cachedPreferences === null) {
    cachedPreferences = readStoredPreferences();
  }
  return cachedPreferences;
}

export function writeSessionChatContextDetailsPreferences(next: SessionChatContextDetailsPreferences): void {
  cachedPreferences = normalizeSessionChatContextDetailsPreferences(next);
  window.localStorage.setItem(SESSION_CHAT_CONTEXT_DETAILS_STORAGE_KEY, JSON.stringify(cachedPreferences));
  window.dispatchEvent(new Event(CHANGED_EVENT));
}

/*
CDXC:SessionChatDetectedOptions 2026-09-04 DECISION:
User: a change saved in one chat view must reach every other chat view, not
necessarily instantly. Every desktop chat view is its own CEF browser on the
shared app-UI profile, so they read one localStorage; the `storage` event
carries a save to the other views and a focus re-read covers a view that
missed it, so the next time it is looked at it shows the latest picks.
*/
function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  const reread = () => {
    cachedPreferences = null;
    listener();
  };
  const onStorage = (event: StorageEvent) => {
    if (event.key === null || event.key === SESSION_CHAT_CONTEXT_DETAILS_STORAGE_KEY) {
      reread();
    }
  };
  window.addEventListener(CHANGED_EVENT, listener);
  window.addEventListener('storage', onStorage);
  window.addEventListener('focus', reread);
  return () => {
    listeners.delete(listener);
    window.removeEventListener(CHANGED_EVENT, listener);
    window.removeEventListener('storage', onStorage);
    window.removeEventListener('focus', reread);
  };
}

export function useSessionChatContextDetailsPreferences(): SessionChatContextDetailsPreferences {
  return useSyncExternalStore(
    subscribe,
    readSessionChatContextDetailsPreferences,
    readSessionChatContextDetailsPreferences
  );
}

/** Wall clock that re-renders the countdowns every half minute. */
export function useSessionChatContextDetailsClock(): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 30_000);
    return () => window.clearInterval(timer);
  }, []);
  return now;
}

// ---------------------------------------------------------------------------
// Resolution

export function isSessionChatContextDetailShown(
  preferences: SessionChatContextDetailsPreferences,
  row: SessionChatContextDetailRowDefinition
): boolean {
  return preferences.shown[row.id] ?? row.recommended;
}

export function isSessionChatContextDetailStarred(
  preferences: SessionChatContextDetailsPreferences,
  row: SessionChatContextDetailRowDefinition
): boolean {
  return preferences.starred[row.id] === true;
}

/** The group's rows in the user's order, missing rows appended in catalog order. */
export function orderedSessionChatContextDetailRows(
  preferences: SessionChatContextDetailsPreferences,
  group: SessionChatContextDetailGroupId
): SessionChatContextDetailRowDefinition[] {
  const catalog = SESSION_CHAT_CONTEXT_DETAIL_ROWS.filter((row) => row.group === group);
  const ordered = (preferences.order[group] ?? [])
    .map((id) => ROW_BY_ID.get(id))
    .filter((row): row is SessionChatContextDetailRowDefinition => row !== undefined && row.group === group);
  const seen = new Set(ordered.map((row) => row.id));
  return [...ordered, ...catalog.filter((row) => !seen.has(row.id))];
}

export interface SessionChatContextDetailItem {
  id: SessionChatContextDetailRowId;
  label: string;
  value: string;
  /** Present when a click on the status line item copies something. */
  copy?: { text: string; label: string };
}

export interface SessionChatContextDetailGroup {
  id: SessionChatContextDetailGroupId;
  label: string;
  items: SessionChatContextDetailItem[];
}

/**
 * Groups with at least one row that is selected AND has a value; a group with
 * nothing under it is dropped so its label never renders alone.
 */
export function resolveSessionChatContextDetailGroups(
  status: SessionChatClaudeStatus | undefined,
  preferences: SessionChatContextDetailsPreferences,
  now: number,
  select: 'shown' | 'starred',
  session: SessionChatContextDetailSession | null
): SessionChatContextDetailGroup[] {
  if (!status) {
    return [];
  }
  const selected = select === 'shown' ? isSessionChatContextDetailShown : isSessionChatContextDetailStarred;
  const groups: SessionChatContextDetailGroup[] = [];
  for (const group of SESSION_CHAT_CONTEXT_DETAIL_GROUPS) {
    const items: SessionChatContextDetailItem[] = [];
    for (const row of orderedSessionChatContextDetailRows(preferences, group.id)) {
      if (!selected(preferences, row)) {
        continue;
      }
      const value = row.value({ status, now, session });
      if (value !== null) {
        items.push({ id: row.id, label: row.label, value });
      }
    }
    if (items.length > 0) {
      groups.push({ id: group.id, label: group.label, items });
    }
  }
  return groups;
}

/** The starred rows in the status line's own order, then any others in group order. */
export function orderedSessionChatStarredRows(
  preferences: SessionChatContextDetailsPreferences
): SessionChatContextDetailRowDefinition[] {
  const starred = SESSION_CHAT_CONTEXT_DETAIL_GROUPS.flatMap((group) =>
    orderedSessionChatContextDetailRows(preferences, group.id).filter((row) =>
      isSessionChatContextDetailStarred(preferences, row)
    )
  );
  const byId = new Map(starred.map((row) => [row.id, row]));
  const ordered = preferences.starredOrder
    .map((id) => byId.get(id))
    .filter((row): row is SessionChatContextDetailRowDefinition => row !== undefined);
  const seen = new Set(ordered.map((row) => row.id));
  return [...ordered, ...starred.filter((row) => !seen.has(row.id))];
}

/** The starred rows with a value, in the status line's order. */
export function resolveSessionChatStarredContextDetails(
  status: SessionChatClaudeStatus | undefined,
  preferences: SessionChatContextDetailsPreferences,
  now: number,
  session: SessionChatContextDetailSession | null
): SessionChatContextDetailItem[] {
  if (!status) {
    return [];
  }
  const items: SessionChatContextDetailItem[] = [];
  for (const row of orderedSessionChatStarredRows(preferences)) {
    const value = row.value({ status, now, session });
    if (value === null) {
      continue;
    }
    const copy = row.copy?.({ status, now, session }) ?? null;
    items.push({ id: row.id, label: row.label, value, ...(copy ? { copy } : {}) });
  }
  return items;
}
