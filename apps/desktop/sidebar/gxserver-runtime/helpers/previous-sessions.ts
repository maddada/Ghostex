/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import { resolveGpuiSidebarAgentIcon } from './presentation-projection';
import type { GxserverPresentationSearchResult } from '@/packages/shared/gxserver-protocol';
import type { SidebarPreviousSessionItem } from '@/packages/shared/session-grid-contract';
import { DEFAULT_TERMINAL_SESSION_TITLE } from '@/packages/shared/session-grid-contract';

export function gxserverSearchResultToPreviousSessionItem(
  result: GxserverPresentationSearchResult,
  options: { historyIdPrefix?: string; projectNamePrefix?: string } = {}
): SidebarPreviousSessionItem {
  const title = result.displayTitle || result.primaryTitle || result.title || 'Previous Session';
  const closedAt = result.closedAt ?? result.updatedAt ?? result.createdAt;
  const agentName = result.agentName ?? result.agentId;
  const sessionPersistenceProvider = result.sessionPersistenceProvider ?? 'zmx';
  const sessionPersistenceName = result.sessionPersistenceName ?? result.zmxName;
  return {
    activity: 'idle',
    agentIcon: resolveGpuiSidebarAgentIcon(result.agentIcon ?? agentName),
    agentSessionId: result.agentSessionId,
    alias: title,
    closedAt,
    column: 0,
    displayTitle: result.displayTitle,
    displayTitleTooltip: result.displayTitleTooltip,
    historyId: `${options.historyIdPrefix ?? 'gxserver'}:${result.projectId}:${result.sessionId}`,
    isFavorite: result.isFavorite,
    isFocused: false,
    isGeneratedName: false,
    isPinned: result.isPinned,
    isPrimaryTitleTerminalTitle: result.isPrimaryTitleTerminalTitle,
    isRestorable: true,
    isRunning: false,
    isVisible: false,
    lastInteractionAt: result.lastActiveAt,
    lifecycleState: 'done',
    primaryTitle: result.primaryTitle ?? title,
    projectId: result.projectId,
    projectName: options.projectNamePrefix
      ? `${options.projectNamePrefix} / ${result.projectTitle}`
      : result.projectTitle,
    row: 0,
    sessionId: result.sessionId,
    sessionKind: 'terminal',
    sessionPersistenceName,
    sessionPersistenceProvider,
    sessionTag: result.sessionTag,
    shortcutLabel: '',
    terminalTitle: result.terminalTitle,
  };
}

export function comparePreviousSessionItemsByClosedTime(
  left: SidebarPreviousSessionItem,
  right: SidebarPreviousSessionItem
): number {
  return previousSessionClosedTime(right) - previousSessionClosedTime(left);
}

export function previousSessionClosedTime(session: SidebarPreviousSessionItem): number {
  const time = Date.parse(session.closedAt);
  return Number.isFinite(time) ? time : 0;
}

export function parseGpuiGxserverPreviousSessionHistoryId(
  historyId: string
): { projectId: string; sessionId: string } | undefined {
  const match = /^gxserver:([^:]+):([^:]+)$/u.exec(historyId);
  if (!match) {
    return undefined;
  }
  return { projectId: match[1]!, sessionId: match[2]! };
}

export function parseGpuiRemotePreviousSessionHistoryId(
  historyId: string
): { machineId: string; projectId: string; sessionId: string } | undefined {
  const match = /^remote-gxserver:([^:]+):([^:]+):([^:]+)$/u.exec(historyId);
  if (!match) {
    return undefined;
  }
  return { machineId: match[1]!, projectId: match[2]!, sessionId: match[3]! };
}

export function previousSessionTitle(previousSession: SidebarPreviousSessionItem | undefined): string {
  return (
    previousSession?.primaryTitle ||
    previousSession?.terminalTitle ||
    previousSession?.alias ||
    DEFAULT_TERMINAL_SESSION_TITLE
  );
}
