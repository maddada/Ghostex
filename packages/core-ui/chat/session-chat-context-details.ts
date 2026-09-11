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
import { formatResetCountdown } from '@/packages/shared/reset-countdown';
import { formatSessionChatDuration } from './session-chat-duration';
export { formatSessionChatDuration } from './session-chat-duration';

import {
  CODEX_CONTEXT_DETAIL_ROWS,
  SHARED_CONTEXT_DETAIL_ROWS,
  USAGE_WINDOW_ROWS,
  type AdditionalContextDetailRowId,
  type ContextDetailStatus,
  type ContextDetailsAgent,
} from './session-chat-context-details-agents';
import { formatSessionChatContextTokens } from './session-chat-context-meter';

export type SessionChatContextDetailGroupId = 'usage' | 'context' | 'session';

export const SESSION_CHAT_CONTEXT_DETAIL_GROUPS: ReadonlyArray<{ id: SessionChatContextDetailGroupId; label: string }> =
  [
    { id: 'usage', label: 'Usage & cost' },
    { id: 'context', label: 'Context & cache' },
    { id: 'session', label: 'Session' },
  ];

export type SessionChatContextDetailRowId =
  | AdditionalContextDetailRowId
  | 'cost'
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
  status: ContextDetailStatus;
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
  /** Null when the agent has not reported a value; popovers omit it and starred items show unavailable. */
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

/** Time left until an epoch-seconds instant, or null once it has passed. */
function formatCountdown(epochSeconds: number, now: number): string | null {
  const remainingMs = epochSeconds * 1000 - now;
  if (remainingMs <= 0) {
    return null;
  }
  return formatResetCountdown(remainingMs);
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
  ...USAGE_WINDOW_ROWS,
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
  ...SHARED_CONTEXT_DETAIL_ROWS,
];

const CODEX_SHARED_ROWS = new Set<SessionChatContextDetailRowId>([
  'lastRequest',
  'totalOutputTokens',
  'remaining',
  'version',
  'sessionName',
  'folder',
  'thinking',
  ...USAGE_WINDOW_ROWS.map((row) => row.id),
  ...SHARED_CONTEXT_DETAIL_ROWS.map((row) => row.id),
]);
const CODEX_ROWS: readonly SessionChatContextDetailRowDefinition[] = [
  ...SESSION_CHAT_CONTEXT_DETAIL_ROWS.filter((row) => CODEX_SHARED_ROWS.has(row.id)).map(
    (row): SessionChatContextDetailRowDefinition => {
      if (row.id === 'thinking')
        return {
          ...row,
          label: 'Reasoning effort',
          description: 'The session’s reasoning effort',
          value: ({ status }) => status.effortName ?? null,
        };
      if (row.id === 'version') return { ...row, label: 'Codex version' };
      if (row.id === 'folder') return { ...row, description: "Codex's current working folder" };
      if (row.id === 'totalOutputTokens')
        return { ...row, description: 'Cumulative Codex output, including reasoning tokens' };
      return row;
    }
  ),
  ...CODEX_CONTEXT_DETAIL_ROWS,
];

export function sessionChatContextDetailRows(
  agent: ContextDetailsAgent = 'claude'
): readonly SessionChatContextDetailRowDefinition[] {
  return agent === 'codex' ? CODEX_ROWS : SESSION_CHAT_CONTEXT_DETAIL_ROWS;
}
const ROWS_BY_AGENT = {
  claude: new Map(SESSION_CHAT_CONTEXT_DETAIL_ROWS.map((row) => [row.id, row])),
  codex: new Map(CODEX_ROWS.map((row) => [row.id, row])),
};
function isRowId(value: unknown, agent: ContextDetailsAgent = 'claude'): value is SessionChatContextDetailRowId {
  return typeof value === 'string' && ROWS_BY_AGENT[agent].has(value as SessionChatContextDetailRowId);
}

/**
 * Rows retired on 2026-09-11 for the one-value usage rows (see the decision on
 * `USAGE_WINDOW_ROWS`). A saved preference for a retired row carries over to
 * the rows that now show its values, so a starred "Rate limits" keeps its place
 * in the status line after the update; the next save stores only current ids.
 */
const RETIRED_ROW_REPLACEMENTS: ReadonlyMap<string, readonly SessionChatContextDetailRowId[]> = new Map<
  string,
  readonly SessionChatContextDetailRowId[]
>([
  ['rateLimits', ['fiveHourLimit', 'sevenDayLimit', 'fiveHourReset']],
  ['accountLimits', ['fiveHourLimit', 'sevenDayLimit', 'modelLimit', 'fiveHourReset', 'sevenDayReset']],
  ['accountPrimaryLimit', ['fiveHourLimit', 'fiveHourReset']],
  ['accountWeeklyLimit', ['sevenDayLimit', 'sevenDayReset']],
  ['accountModelLimits', ['modelLimit']],
  ['primaryLimit', ['fiveHourLimit', 'fiveHourReset']],
  ['secondaryLimit', ['sevenDayLimit', 'sevenDayReset']],
]);

/** The current row ids a saved id stands for: itself, a retired row's replacements, or nothing. */
function currentRowIds(saved: unknown, agent: ContextDetailsAgent): SessionChatContextDetailRowId[] {
  if (isRowId(saved, agent)) return [saved];
  const replacements = typeof saved === 'string' ? RETIRED_ROW_REPLACEMENTS.get(saved) : undefined;
  return replacements ? replacements.filter((id) => isRowId(id, agent)) : [];
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

function normalizeFlags(
  candidate: unknown,
  agent: ContextDetailsAgent
): Partial<Record<SessionChatContextDetailRowId, boolean>> {
  const flags: Partial<Record<SessionChatContextDetailRowId, boolean>> = {};
  if (candidate && typeof candidate === 'object' && !Array.isArray(candidate)) {
    for (const [id, flag] of Object.entries(candidate)) {
      if (typeof flag !== 'boolean') {
        continue;
      }
      if (isRowId(id, agent)) {
        flags[id] = flag;
        continue;
      }
      // A retired row's flag never overrides one saved for a current row.
      for (const replacement of currentRowIds(id, agent)) {
        if (flags[replacement] === undefined) {
          flags[replacement] = flag;
        }
      }
    }
  }
  return flags;
}

function normalizeOrder(
  candidate: unknown,
  agent: ContextDetailsAgent
): Partial<Record<SessionChatContextDetailGroupId, SessionChatContextDetailRowId[]>> {
  const order: Partial<Record<SessionChatContextDetailGroupId, SessionChatContextDetailRowId[]>> = {};
  if (candidate && typeof candidate === 'object' && !Array.isArray(candidate)) {
    for (const group of SESSION_CHAT_CONTEXT_DETAIL_GROUPS) {
      const ids = (candidate as Record<string, unknown>)[group.id];
      if (Array.isArray(ids)) {
        const kept = ids
          .flatMap((id) => currentRowIds(id, agent))
          .filter((id) => ROWS_BY_AGENT[agent].get(id)?.group === group.id);
        order[group.id] = [...new Set(kept)];
      }
    }
  }
  return order;
}

export function normalizeSessionChatContextDetailsPreferences(
  candidate: unknown,
  agent: ContextDetailsAgent = 'claude'
): SessionChatContextDetailsPreferences {
  if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) {
    return DEFAULT_SESSION_CHAT_CONTEXT_DETAILS_PREFERENCES;
  }
  const record = candidate as Record<string, unknown>;
  return {
    shown: normalizeFlags(record.shown, agent),
    starred: normalizeFlags(record.starred, agent),
    order: normalizeOrder(record.order, agent),
    starredOrder: Array.isArray(record.starredOrder)
      ? [...new Set(record.starredOrder.flatMap((id) => currentRowIds(id, agent)))]
      : [],
  };
}

/** CDXC:AgentProviders 2026-09-08 DECISION:
 * User: keep the same Claude UI and status line, but save popover and status-line settings independently for Claude and Codex.
 * Claude keeps its existing storage key and configuration; copying to the other agent is a one-time action.
 */
const preferenceKeys = {
  claude: SESSION_CHAT_CONTEXT_DETAILS_STORAGE_KEY,
  codex: 'ghostex.chat.context-details.codex.v1',
};
const cachedPreferences: Partial<Record<ContextDetailsAgent, SessionChatContextDetailsPreferences>> = {};

export function readSessionChatContextDetailsPreferences(
  agent: ContextDetailsAgent = 'claude'
): SessionChatContextDetailsPreferences {
  if (!cachedPreferences[agent]) {
    try {
      cachedPreferences[agent] = normalizeSessionChatContextDetailsPreferences(
        JSON.parse(window.localStorage.getItem(preferenceKeys[agent]) ?? 'null'),
        agent
      );
    } catch {
      cachedPreferences[agent] = DEFAULT_SESSION_CHAT_CONTEXT_DETAILS_PREFERENCES;
    }
  }
  return cachedPreferences[agent];
}

export function writeSessionChatContextDetailsPreferences(
  next: SessionChatContextDetailsPreferences,
  agent: ContextDetailsAgent = 'claude'
): void {
  const normalized = normalizeSessionChatContextDetailsPreferences(next, agent);
  window.localStorage.setItem(preferenceKeys[agent], JSON.stringify(normalized));
  cachedPreferences[agent] = normalized;
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
  const reread = () => {
    delete cachedPreferences.claude;
    delete cachedPreferences.codex;
    listener();
  };
  const onStorage = (event: StorageEvent) => {
    if (event.key === null || Object.values(preferenceKeys).includes(event.key)) reread();
  };
  window.addEventListener(CHANGED_EVENT, reread);
  window.addEventListener('storage', onStorage);
  window.addEventListener('focus', reread);
  return () => {
    window.removeEventListener(CHANGED_EVENT, reread);
    window.removeEventListener('storage', onStorage);
    window.removeEventListener('focus', reread);
  };
}

export function useSessionChatContextDetailsPreferences(
  agent: ContextDetailsAgent = 'claude'
): SessionChatContextDetailsPreferences {
  return useSyncExternalStore(
    subscribe,
    () => readSessionChatContextDetailsPreferences(agent),
    () => readSessionChatContextDetailsPreferences(agent)
  );
}

/**
 * These map display preferences, not metric values. The destination keeps its own label and units.
 * Aggregate rows can receive several narrower selections: any selected source keeps the destination selected.
 */
const SIMILAR_CONTEXT_DETAIL_ROWS: Record<
  ContextDetailsAgent,
  Partial<Record<SessionChatContextDetailRowId, SessionChatContextDetailRowId>>
> = {
  claude: {
    promptCache: 'cacheRatio',
    cost: 'lastTurnDuration',
  },
  codex: {
    cacheRatio: 'promptCache',
    totalInputTokens: 'lastRequest',
    totalTokens: 'lastRequest',
    cachedTokens: 'lastRequest',
    cacheWriteTokens: 'lastRequest',
    turnTokens: 'lastRequest',
    reasoningTokens: 'totalOutputTokens',
    credits: 'accountSpending',
    lastTurnDuration: 'cost',
    firstTokenTime: 'cost',
  },
};

/** CDXC:AgentProviders 2026-09-09 DECISION:
 * User: provide Copy to and Copy from buttons and map to the most similar fields, extending the original export-only behavior.
 * Copy visibility, stars, and order; preserve destination-only settings. Import edits the open draft, while export saves to the other agent.
 */
export function mapSessionChatContextDetailsPreferences(
  source: SessionChatContextDetailsPreferences,
  from: ContextDetailsAgent,
  currentDestination: SessionChatContextDetailsPreferences
): { preferences: SessionChatContextDetailsPreferences; matched: number; skipped: number } {
  const to = from === 'claude' ? 'codex' : 'claude';
  const destination = normalizeSessionChatContextDetailsPreferences(currentDestination, to);
  const sourceRows = sessionChatContextDetailRows(from);
  const mapping = new Map<SessionChatContextDetailRowId, SessionChatContextDetailRowId>();
  for (const row of sourceRows) {
    const target = ROWS_BY_AGENT[to].has(row.id) ? row.id : SIMILAR_CONTEXT_DETAIL_ROWS[from][row.id];
    if (target && ROWS_BY_AGENT[to].has(target)) mapping.set(row.id, target);
  }
  const matchingIds = new Set(mapping.values());
  const previousStarredOrder = orderedSessionChatStarredRows(destination, to).map((row) => row.id);
  for (const target of matchingIds) {
    const rows = sourceRows.filter((row) => mapping.get(row.id) === target);
    destination.shown[target] = rows.some((row) => isSessionChatContextDetailShown(source, row));
    destination.starred[target] = rows.some((row) => isSessionChatContextDetailStarred(source, row));
  }
  const mappedOrder = (rows: readonly SessionChatContextDetailRowDefinition[]) => [
    ...new Set(
      rows.flatMap((row) => {
        const target = mapping.get(row.id);
        return target ? [target] : [];
      })
    ),
  ];
  // Replace matching slots in their existing group, preserving the relative order of unrelated fields.
  const mergeOrder = (existing: SessionChatContextDetailRowId[], incoming: SessionChatContextDetailRowId[]) => {
    let index = 0;
    const merged = existing.flatMap((id) =>
      matchingIds.has(id) ? (incoming[index] ? [incoming[index++]] : []) : [id]
    );
    return [...merged, ...incoming.slice(index)];
  };
  for (const group of SESSION_CHAT_CONTEXT_DETAIL_GROUPS) {
    const incoming = mappedOrder(orderedSessionChatContextDetailRows(source, group.id, from));
    destination.order[group.id] = mergeOrder(
      orderedSessionChatContextDetailRows(destination, group.id, to).map((row) => row.id),
      incoming
    );
  }
  destination.starredOrder = mergeOrder(previousStarredOrder, mappedOrder(orderedSessionChatStarredRows(source, from)));
  return { preferences: destination, matched: mapping.size, skipped: sourceRows.length - mapping.size };
}

export function copySessionChatContextDetailsPreferences(
  source: SessionChatContextDetailsPreferences,
  from: ContextDetailsAgent
): { matched: number; skipped: number } {
  const to = from === 'claude' ? 'codex' : 'claude';
  const result = mapSessionChatContextDetailsPreferences(source, from, readSessionChatContextDetailsPreferences(to));
  writeSessionChatContextDetailsPreferences(result.preferences, to);
  return { matched: result.matched, skipped: result.skipped };
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
  group: SessionChatContextDetailGroupId,
  agent: ContextDetailsAgent = 'claude'
): SessionChatContextDetailRowDefinition[] {
  const catalog = sessionChatContextDetailRows(agent).filter((row) => row.group === group);
  const ordered = (preferences.order[group] ?? [])
    .map((id) => ROWS_BY_AGENT[agent].get(id))
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
  status: ContextDetailStatus | undefined,
  preferences: SessionChatContextDetailsPreferences,
  now: number,
  select: 'shown' | 'starred',
  session: SessionChatContextDetailSession | null,
  agent: ContextDetailsAgent = 'claude'
): SessionChatContextDetailGroup[] {
  if (!status) {
    return [];
  }
  const selected = select === 'shown' ? isSessionChatContextDetailShown : isSessionChatContextDetailStarred;
  const groups: SessionChatContextDetailGroup[] = [];
  for (const group of SESSION_CHAT_CONTEXT_DETAIL_GROUPS) {
    const items: SessionChatContextDetailItem[] = [];
    for (const row of orderedSessionChatContextDetailRows(preferences, group.id, agent)) {
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
  preferences: SessionChatContextDetailsPreferences,
  agent: ContextDetailsAgent = 'claude'
): SessionChatContextDetailRowDefinition[] {
  const starred = SESSION_CHAT_CONTEXT_DETAIL_GROUPS.flatMap((group) =>
    orderedSessionChatContextDetailRows(preferences, group.id, agent).filter((row) =>
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

/** CDXC:AgentProviders 2026-09-09 DECISION:
 * User: items starred in context details must always remain visible in the status line.
 * A missing value is labeled unavailable so refreshes cannot remove the item or imply zero usage.
 */
export function resolveSessionChatStarredContextDetails(
  status: ContextDetailStatus | undefined,
  preferences: SessionChatContextDetailsPreferences,
  now: number,
  session: SessionChatContextDetailSession | null,
  agent: ContextDetailsAgent = 'claude'
): SessionChatContextDetailItem[] {
  const items: SessionChatContextDetailItem[] = [];
  const input = { status: status ?? {}, now, session };
  for (const row of orderedSessionChatStarredRows(preferences, agent)) {
    const value = row.value(input);
    const copy = value == null ? null : (row.copy?.(input) ?? null);
    items.push({
      id: row.id,
      label: row.label,
      value: value ?? `${row.label}: unavailable`,
      ...(copy ? { copy } : {}),
    });
  }
  return items;
}
