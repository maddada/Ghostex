/**
 * CDXC:Navigation 2026-08-19:
 * Titlebar Back/Forward walks ONE chronological trail of everything the user has
 * had active — sessions and projects, across machines — not a per-project stack.
 * gxserver owns that trail (see `server/src/navigation_history`); this file
 * is the wire contract both clients share, and the only place the entry shape,
 * endpoint names, and button labels are defined.
 *
 * Ids here are the SIDEBAR vocabulary, not raw daemon ids: whatever string the
 * host would pass to `focusSession` / `focusGroup`. Remote machines already
 * scope their ids, so a single trail can hold entries from several machines and
 * activation stays a plain host call with no re-resolution step.
 */

export const NAVIGATION_HISTORY_READ_ENDPOINT = '/api/readNavigationHistory';
export const NAVIGATION_HISTORY_VISIT_ENDPOINT = '/api/recordNavigationVisit';
export const NAVIGATION_HISTORY_NAVIGATE_ENDPOINT = '/api/navigateHistory';

/**
 * Trails are per client, so the desktop app and a browser tab talking to the
 * same daemon never share a cursor.
 */
export const NAVIGATION_HISTORY_SCOPE_GPUI = 'gpui-desktop';
export const NAVIGATION_HISTORY_SCOPE_WEB = 'web';

export type NavigationHistoryDirection = 'back' | 'forward';

export type NavigationHistoryEntry = {
  /** Sidebar-vocabulary project id. Required; identifies the trail stop. */
  projectId: string;
  /** Sidebar-vocabulary session id, when a session (not just a project) was active. */
  sessionId?: string;
  /** Sidebar-vocabulary group id, so activation can focus a project with no session. */
  groupId?: string;
  /** Display title only, for the "Back to …" tooltip. */
  projectLabel?: string;
  /** Display title only, for the "Back to …" tooltip. */
  sessionLabel?: string;
};

export type NavigationHistoryState = {
  canGoBack: boolean;
  canGoForward: boolean;
  entryCount: number;
  backEntry?: NavigationHistoryEntry;
  currentEntry?: NavigationHistoryEntry;
  forwardEntry?: NavigationHistoryEntry;
};

/** What a titlebar needs to paint the two buttons, and nothing else. */
export type NavigationHistoryUiState = {
  canGoBack: boolean;
  canGoForward: boolean;
  /** Tooltip for the Back button, disabled or not. */
  backTooltip: string;
  /** Tooltip for the Forward button, disabled or not. */
  forwardTooltip: string;
  /** A navigation is in flight. Buttons stay clickable; extra clicks queue. */
  isNavigating: boolean;
};

export const EMPTY_NAVIGATION_HISTORY_STATE: NavigationHistoryState = {
  canGoBack: false,
  canGoForward: false,
  entryCount: 0,
};

export const EMPTY_NAVIGATION_HISTORY_UI_STATE: NavigationHistoryUiState = {
  backTooltip: 'Back',
  canGoBack: false,
  canGoForward: false,
  forwardTooltip: 'Forward',
  isNavigating: false,
};

/** Must stay byte-identical to KEY_SEPARATOR in the daemon's navigation_history. */
const NAVIGATION_HISTORY_KEY_SEPARATOR = '\u001f';

/**
 * Identity of a trail stop, mirroring the daemon's key exactly so `forgetKeys`
 * lines up on both sides.
 */
export function navigationHistoryEntryKey(entry: NavigationHistoryEntry): string {
  return `${entry.projectId}${NAVIGATION_HISTORY_KEY_SEPARATOR}${entry.sessionId ?? ''}`;
}

/** "Session · Project", or just the project when no session was active. */
export function navigationHistoryEntryLabel(entry: NavigationHistoryEntry | undefined): string {
  if (!entry) {
    return '';
  }
  const project = entry.projectLabel?.trim() ?? '';
  const session = entry.sessionLabel?.trim() ?? '';
  if (session && project && session !== project) {
    return `${session} · ${project}`;
  }
  return session || project;
}

export function navigationHistoryTooltip(direction: NavigationHistoryDirection, state: NavigationHistoryState): string {
  const verb = direction === 'back' ? 'Back' : 'Forward';
  const entry = direction === 'back' ? state.backEntry : state.forwardEntry;
  const enabled = direction === 'back' ? state.canGoBack : state.canGoForward;
  if (!enabled) {
    return direction === 'back' ? 'Back (nothing earlier)' : 'Forward (nothing later)';
  }
  const label = navigationHistoryEntryLabel(entry);
  return label ? `${verb} to ${label}` : verb;
}

export function createNavigationHistoryUiState(
  state: NavigationHistoryState,
  isNavigating: boolean
): NavigationHistoryUiState {
  return {
    backTooltip: navigationHistoryTooltip('back', state),
    canGoBack: state.canGoBack,
    canGoForward: state.canGoForward,
    forwardTooltip: navigationHistoryTooltip('forward', state),
    isNavigating,
  };
}

export function navigationHistoryUiStatesEqual(
  left: NavigationHistoryUiState,
  right: NavigationHistoryUiState
): boolean {
  return (
    left.canGoBack === right.canGoBack &&
    left.canGoForward === right.canGoForward &&
    left.backTooltip === right.backTooltip &&
    left.forwardTooltip === right.forwardTooltip &&
    left.isNavigating === right.isNavigating
  );
}

function normalizeText(value: unknown, max = 512): string | undefined {
  if (typeof value !== 'string') {
    return undefined;
  }
  const trimmed = value.trim();
  return trimmed ? trimmed.slice(0, max) : undefined;
}

export function normalizeNavigationHistoryEntry(value: unknown): NavigationHistoryEntry | undefined {
  if (!value || typeof value !== 'object') {
    return undefined;
  }
  const raw = value as Record<string, unknown>;
  const projectId = normalizeText(raw.projectId);
  if (!projectId) {
    return undefined;
  }
  const sessionId = normalizeText(raw.sessionId);
  const groupId = normalizeText(raw.groupId);
  const projectLabel = normalizeText(raw.projectLabel, 256);
  const sessionLabel = normalizeText(raw.sessionLabel, 256);
  return {
    projectId,
    ...(sessionId ? { sessionId } : {}),
    ...(groupId ? { groupId } : {}),
    ...(projectLabel ? { projectLabel } : {}),
    ...(sessionLabel ? { sessionLabel } : {}),
  };
}

/** Parse the `navigationHistory` block of any of the three RPC responses. */
export function normalizeNavigationHistoryState(value: unknown): NavigationHistoryState {
  if (!value || typeof value !== 'object') {
    return EMPTY_NAVIGATION_HISTORY_STATE;
  }
  const raw = (value as { navigationHistory?: unknown }).navigationHistory ?? value;
  if (!raw || typeof raw !== 'object') {
    return EMPTY_NAVIGATION_HISTORY_STATE;
  }
  const state = raw as Record<string, unknown>;
  const backEntry = normalizeNavigationHistoryEntry(state.backEntry);
  const currentEntry = normalizeNavigationHistoryEntry(state.currentEntry);
  const forwardEntry = normalizeNavigationHistoryEntry(state.forwardEntry);
  return {
    canGoBack: state.canGoBack === true,
    canGoForward: state.canGoForward === true,
    entryCount: typeof state.entryCount === 'number' ? state.entryCount : 0,
    ...(backEntry ? { backEntry } : {}),
    ...(currentEntry ? { currentEntry } : {}),
    ...(forwardEntry ? { forwardEntry } : {}),
  };
}

export function navigationHistoryEntriesEqual(
  left: NavigationHistoryEntry | undefined,
  right: NavigationHistoryEntry | undefined
): boolean {
  if (!left || !right) {
    return left === right;
  }
  return (
    left.projectId === right.projectId &&
    left.sessionId === right.sessionId &&
    left.groupId === right.groupId &&
    left.projectLabel === right.projectLabel &&
    left.sessionLabel === right.sessionLabel
  );
}
