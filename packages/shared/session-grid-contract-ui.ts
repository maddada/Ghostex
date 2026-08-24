import { DEFAULT_COMPLETION_SOUND, getCompletionSoundLabel, type CompletionSoundSetting } from './completion-sound';
import { createDefaultSidebarAgentButtons, type SidebarAgentButton } from './sidebar-agents';
import { createDefaultSidebarCommandButtons, type SidebarCommandButton } from './sidebar-commands';
import {
  DEFAULT_AGENT_MANAGER_ZOOM_PERCENT,
  type SessionGridSnapshot,
  type SessionRecord,
  type SidebarTheme,
} from './session-grid-contract-core';
import { createDefaultSidebarGitState, type SidebarGitState } from './sidebar-git';
import {
  type SidebarActiveSessionsSortMode,
  type SidebarCommandSessionIndicator,
  type SidebarHudState,
  type SidebarSessionItem,
} from './session-grid-contract-sidebar';
import {
  getOrderedSessions,
  getSessionCardPrimaryTitle,
  getSessionGridLayoutVisibleCount,
  getSessionShortcutLabel,
  getSlotLabel,
  getVisibleSessionNumber,
  isSessionGridFocusModeActive,
} from './session-grid-contract-session';
import { getEffectiveSidebarSessionTag } from './session-tags';

export function createSidebarHudState(
  snapshot: SessionGridSnapshot,
  theme: SidebarTheme = 'dark-blue',
  agentManagerZoomPercent = DEFAULT_AGENT_MANAGER_ZOOM_PERCENT,
  /**
   * CDXC:SidebarSessions 2026-05-09-17:00
   * Fresh sidebar HUD snapshots default close-on-hover to enabled so normal
   * project and chat session cards match the Settings default.
   */
  showCloseButtonOnSessionCards = true,
  debuggingMode = false,
  completionBellEnabled = false,
  completionSound: CompletionSoundSetting = DEFAULT_COMPLETION_SOUND,
  agents: SidebarAgentButton[] = createDefaultSidebarAgentButtons(),
  commands: SidebarCommandButton[] = createDefaultSidebarCommandButtons(),
  pendingAgentIds: string[] = [],
  git: SidebarGitState = createDefaultSidebarGitState(),
  /**
   * CDXC:SidebarSessions 2026-04-28-05:18
   * New sidebar HUD state must default to the reference behavior: active
   * sessions are ordered by last activity unless a caller explicitly requests
   * manual ordering.
   */
  activeSessionsSortMode: SidebarActiveSessionsSortMode = 'lastActivity',
  createSessionOnSidebarDoubleClick = false,
  renameSessionOnDoubleClick = false,
  commandSessionIndicators: SidebarCommandSessionIndicator[] = [],
  buildStamp?: string
): SidebarHudState {
  const sessionById = new Map(snapshot.sessions.map((session) => [session.sessionId, session]));
  const focusedSession = snapshot.focusedSessionId ? sessionById.get(snapshot.focusedSessionId) : undefined;

  return {
    activeSessionsSortMode,
    agentManagerZoomPercent,
    agents,
    buildStamp,
    commands,
    commandSessionIndicators,
    completionBellEnabled,
    completionSound,
    completionSoundLabel: getCompletionSoundLabel(completionSound),
    debuggingMode,
    focusedSessionTitle: focusedSession?.title,
    git,
    highlightedVisibleCount: getSessionGridLayoutVisibleCount(snapshot),
    isFocusModeActive: isSessionGridFocusModeActive(snapshot),
    pendingAgentIds,
    /**
     * CDXC:SidebarLayout 2026-05-22-22:24:
     * The current sidebar no longer renders legacy Actions/Agents/Browsers
     * sections, so HUD snapshots omit section visibility and section collapse
     * state instead of preserving dead chrome controls.
     */
    recentProjects: [],
    createSessionOnSidebarDoubleClick,
    renameSessionOnDoubleClick,
    showCloseButtonOnSessionCards,
    theme,
    viewMode: snapshot.viewMode,
    visibleCount: snapshot.visibleCount,
    visibleSlotLabels: snapshot.visibleSessionIds
      .map((sessionId) => sessionById.get(sessionId))
      .filter((session): session is SessionRecord => session !== undefined)
      .map((session) => getSlotLabel(session.row, session.column)),
  };
}

export function createSidebarSessionItems(
  snapshot: SessionGridSnapshot,
  platform: 'default' | 'mac' = 'default'
): SidebarSessionItem[] {
  const visibleIds = new Set(snapshot.visibleSessionIds);
  return getOrderedSessions(snapshot).map((session) => ({
    activity: 'idle',
    activityLabel: undefined,
    agentIcon: session.kind === 'browser' ? 'browser' : undefined,
    agentSessionId: session.kind === 'terminal' ? session.agentSessionId : undefined,
    alias: session.alias,
    column: session.column,
    detail: undefined,
    faviconDataUrl: session.kind === 'browser' ? session.browser.faviconDataUrl : undefined,
    lifecycleState: session.kind === 'browser' ? 'running' : session.isSleeping === true ? 'sleeping' : 'done',
    firstUserMessage: session.firstUserMessage,
    /**
     * CDXC:SessionFavorites 2026-05-15-12:43
     * Favorite state is stored on the canonical session record but rendered by
     * sidebar cards and Previous Sessions. Project it into SidebarSessionItem so
     * context menus can toggle back to Unfavorite after publish.
     *
     * CDXC:SidebarSessionAgentIcons 2026-06-29-23:58:
     * Favorite projection must not gold-tint the session's agent icon; Favorite
     * remains a session tag/state, while icon color follows the Session Cards
     * monochrome/colored setting.
     */
    isFavorite: getEffectiveSidebarSessionTag(session) === 'favorite',
    sessionTag: getEffectiveSidebarSessionTag(session),
    isParked: session.isParked === true,
    /**
     * CDXC:PinnedSessions 2026-05-28-12:04:
     * Project the canonical pinned flag into sidebar items separately from
     * favorite state so live pinned ordering and favorite history behavior stay
     * independent.
     */
    isPinned: session.isPinned === true,
    isFocused: snapshot.focusedSessionId === session.sessionId,
    isPoppedOut: session.isSleeping === true ? undefined : session.isPoppedOut === true || undefined,
    isSleeping: session.isSleeping === true,
    isRunning: session.kind === 'browser',
    isVisible: visibleIds.has(session.sessionId),
    /**
     * CDXC:BrowserPanes 2026-05-28-05:30:
     * Browser panes must project their browser identity in both the legacy
     * `kind` field and the canonical `sessionKind` field. Some sidebar card
     * paths still check `kind`, so omitting it can let browser rows inherit
     * terminal title/icon handling and render as `∗ Terminal Session`.
     */
    kind: session.kind === 'browser' ? 'browser' : undefined,
    lastInteractionAt: undefined,
    primaryTitle: getSessionCardPrimaryTitle(session),
    row: session.row,
    sessionId: session.sessionId,
    sessionKind: session.kind,
    sessionNumber: getVisibleSessionNumber(session),
    sessionPersistenceName:
      session.kind === 'terminal' ? (session.sessionPersistenceName ?? session.tmuxSessionName) : undefined,
    sessionPersistenceProvider: session.kind === 'terminal' ? session.sessionPersistenceProvider : undefined,
    shortcutLabel: getSessionShortcutLabel(session.slotIndex, platform),
  }));
}
