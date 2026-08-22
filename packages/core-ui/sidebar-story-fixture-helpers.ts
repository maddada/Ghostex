import type { SidebarAgentIcon } from "../shared/sidebar-agents";
import type {
  SidebarPreviousSessionItem,
  SidebarSessionGroup,
  SidebarSessionItem,
} from "../shared/session-grid-contract";

export type SidebarStoryGroup = Omit<
  SidebarSessionGroup,
  "isFocusModeActive" | "layoutVisibleCount" | "viewMode" | "visibleCount"
>;

export function createStorySession({
  activity = "idle",
  activityLabel,
  alias,
  agentIcon,
  detail,
  isFocused = false,
  isRunning = true,
  isVisible = false,
  lastInteractionAt,
  primaryTitle,
  sessionId,
  shortcutLabel,
  terminalTitle,
}: {
  activity?: SidebarSessionItem["activity"];
  activityLabel?: string;
  alias: string;
  agentIcon?: SidebarAgentIcon;
  detail?: string;
  isFocused?: boolean;
  isRunning?: boolean;
  isVisible?: boolean;
  lastInteractionAt?: string;
  primaryTitle?: string;
  sessionId: string;
  shortcutLabel: string;
  terminalTitle?: string;
}): SidebarSessionItem {
  return {
    activity,
    activityLabel,
    agentIcon,
    alias,
    column: 0,
    detail,
    isFocused,
    isRunning,
    isVisible,
    lastInteractionAt,
    primaryTitle,
    row: 0,
    sessionId,
    shortcutLabel,
    terminalTitle,
  };
}

export function cloneGroups(groups: readonly SidebarStoryGroup[]): SidebarStoryGroup[] {
  return groups.map((group) => ({
    ...group,
    sessions: group.sessions.map((session) => ({ ...session })),
  }));
}

export function createStoryPreviousSession({
  activity = "idle",
  alias,
  closedAt = new Date().toISOString(),
  detail,
  historyId,
  isRestorable = true,
  primaryTitle,
  sessionId,
  shortcutLabel,
  terminalTitle,
}: {
  activity?: SidebarSessionItem["activity"];
  alias: string;
  closedAt?: string;
  detail?: string;
  historyId: string;
  isRestorable?: boolean;
  primaryTitle?: string;
  sessionId: string;
  shortcutLabel: string;
  terminalTitle?: string;
}): SidebarPreviousSessionItem {
  return {
    activity,
    alias,
    closedAt,
    column: 0,
    detail,
    historyId,
    isFocused: false,
    isGeneratedName: false,
    isRestorable,
    isRunning: false,
    isVisible: false,
    primaryTitle,
    row: 0,
    sessionId,
    shortcutLabel,
    terminalTitle,
  };
}

export function getFocusedSessionTitle(groups: readonly SidebarSessionGroup[]): string | undefined {
  const focusedSession = groups
    .flatMap((group) => group.sessions)
    .find((session) => session.isFocused);

  return focusedSession
    ? (focusedSession.alias ??
        focusedSession.terminalTitle ??
        focusedSession.primaryTitle ??
        focusedSession.detail)
    : undefined;
}

export function getVisibleSlotLabels(groups: readonly SidebarSessionGroup[]): string[] {
  return groups
    .flatMap((group) => group.sessions)
    .filter((session) => session.isVisible)
    .map((session) => session.shortcutLabel);
}
