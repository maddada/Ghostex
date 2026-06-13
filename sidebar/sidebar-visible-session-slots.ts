import {
  PROJECT_SESSION_LIST_COLLAPSED_COUNT,
  getVisibleProjectSessionIds,
  type ProjectSessionListCollapsedState,
} from "./project-session-list-toggle";

export type SidebarVisibleSlotGroup = {
  isChatCollection?: boolean;
  projectContext?: { editor?: { projectId?: string } };
  remoteMachineContext?: { machineId?: string };
};

export type SidebarVisibleSessionSlotOptions = {
  collapsedGroupsById: Record<string, true>;
  displayedWorkspaceGroupIds: readonly string[];
  displayedWorkspaceSessionIdsByGroup: Record<string, readonly string[]>;
  enableProjectSessionListToggle: boolean;
  groupsById: Record<string, SidebarVisibleSlotGroup | undefined>;
  isReferenceChatsCollapsed: boolean;
  isReferenceProjectsCollapsed: boolean;
  projectSessionListCollapsedCount?: number;
  projectSessionListCollapsedState: ProjectSessionListCollapsedState;
  remoteMachineIds: readonly string[];
};

export function createVisibleSidebarSessionSlotIds({
  collapsedGroupsById,
  displayedWorkspaceGroupIds,
  displayedWorkspaceSessionIdsByGroup,
  enableProjectSessionListToggle,
  groupsById,
  isReferenceChatsCollapsed,
  isReferenceProjectsCollapsed,
  projectSessionListCollapsedCount = PROJECT_SESSION_LIST_COLLAPSED_COUNT,
  projectSessionListCollapsedState,
  remoteMachineIds,
}: SidebarVisibleSessionSlotOptions): string[] {
  const visibleSessionIds: string[] = [];

  const appendGroup = (groupId: string, forceExpanded = false) => {
    const group = groupsById[groupId];
    if (!group || (!forceExpanded && collapsedGroupsById[groupId] === true)) {
      return;
    }

    const sessionIds = displayedWorkspaceSessionIdsByGroup[groupId] ?? [];
    const projectSessionListStorageId = group.projectContext?.editor?.projectId ?? groupId;
    visibleSessionIds.push(
      ...getVisibleProjectSessionIds({
        collapsedCount: projectSessionListCollapsedCount,
        isCollapsed: projectSessionListCollapsedState[projectSessionListStorageId] === true,
        isProjectGroup: Boolean(group.projectContext),
        isToggleEnabled: enableProjectSessionListToggle,
        sessionIds,
      }),
    );
  };

  if (!isReferenceChatsCollapsed) {
    for (const groupId of displayedWorkspaceGroupIds) {
      if (groupsById[groupId]?.isChatCollection === true) {
        appendGroup(groupId, true);
      }
    }
  }

  if (!isReferenceProjectsCollapsed) {
    for (const groupId of displayedWorkspaceGroupIds) {
      const group = groupsById[groupId];
      if (
        group &&
        group.isChatCollection !== true &&
        !group.remoteMachineContext
      ) {
        appendGroup(groupId);
      }
    }
  }

  for (const machineId of remoteMachineIds) {
    for (const groupId of displayedWorkspaceGroupIds) {
      if (groupsById[groupId]?.remoteMachineContext?.machineId === machineId) {
        appendGroup(groupId);
      }
    }
  }

  return visibleSessionIds;
}

export function resolveVisibleSidebarSessionSlotId({
  focusedSessionId,
  slotNumber,
  visibleSessionIds,
}: {
  focusedSessionId?: string;
  slotNumber: number;
  visibleSessionIds: readonly string[];
}): string | undefined {
  if (visibleSessionIds.length === 0) {
    return undefined;
  }

  if (slotNumber > 0) {
    return visibleSessionIds[slotNumber - 1];
  }

  const focusedIndex = focusedSessionId ? visibleSessionIds.indexOf(focusedSessionId) : -1;
  if (slotNumber === 0) {
    const currentIndex = focusedIndex >= 0 ? focusedIndex : -1;
    return visibleSessionIds[(currentIndex + 1) % visibleSessionIds.length];
  }

  const currentIndex = focusedIndex >= 0 ? focusedIndex : 0;
  return visibleSessionIds[
    (currentIndex - 1 + visibleSessionIds.length) % visibleSessionIds.length
  ];
}

export type RenderedSidebarSessionSlotElement = {
  closest(selectors: string): Element | null;
  getAttribute(name: string): string | null;
  getClientRects?: () => { length: number };
};

export type RenderedSidebarSessionSlot = {
  isSleeping: boolean;
  sessionId: string;
};

export function createRenderedSidebarSessionSlots(
  elements: readonly RenderedSidebarSessionSlotElement[],
): RenderedSidebarSessionSlot[] {
  const visibleSlots: RenderedSidebarSessionSlot[] = [];

  for (const element of elements) {
    const sessionId = element.getAttribute("data-sidebar-session-id");
    if (!sessionId) {
      continue;
    }

    if (element.closest('[aria-hidden="true"], [data-collapsed="true"]')) {
      continue;
    }

    if (element.getAttribute("data-visible") === "false") {
      continue;
    }

    if (element.getClientRects && element.getClientRects().length === 0) {
      continue;
    }

    visibleSlots.push({
      isSleeping: element.getAttribute("data-sleeping") === "true",
      sessionId,
    });
  }

  return visibleSlots;
}

export function createRenderedSidebarSessionSlotIds(
  elements: readonly RenderedSidebarSessionSlotElement[],
): string[] {
  return createRenderedSidebarSessionSlots(elements).map((slot) => slot.sessionId);
}

export function resolveAdjacentRenderedSidebarSessionSlotId({
  direction,
  focusedSessionId,
  slots,
}: {
  direction: -1 | 1;
  focusedSessionId?: string;
  slots: readonly RenderedSidebarSessionSlot[];
}): string | undefined {
  const awakeSlots = slots.filter((slot) => !slot.isSleeping);
  if (awakeSlots.length === 0) {
    return undefined;
  }

  const focusedIndex = focusedSessionId
    ? slots.findIndex((slot) => slot.sessionId === focusedSessionId)
    : -1;
  if (focusedIndex < 0) {
    return direction > 0 ? awakeSlots[0]?.sessionId : awakeSlots.at(-1)?.sessionId;
  }

  for (let step = 1; step <= slots.length; step += 1) {
    const candidate = slots[(focusedIndex + direction * step + slots.length) % slots.length];
    if (candidate && !candidate.isSleeping) {
      return candidate.sessionId;
    }
  }

  return undefined;
}

export function readRenderedSidebarSessionSlotIds(root: ParentNode = document): string[] {
  /**
   * CDXC:Hotkeys 2026-06-05-21:17:
   * A user repro showed state-derived Cmd+number slots could include a hidden row, making Cmd+5 select the sixth visible session and Cmd+6 jump much lower in the sidebar. Read the rendered session-card rows at key time so slot numbers match the pixels shown in the sidebar and collapsed projects do not reserve indices.
   */
  return createRenderedSidebarSessionSlotIds(
    Array.from(
      root.querySelectorAll<HTMLElement>("[data-sidebar-session-id]"),
    ),
  );
}

export function readRenderedSidebarSessionSlots(
  root: ParentNode = document,
): RenderedSidebarSessionSlot[] {
  /**
   * CDXC:Hotkeys 2026-06-07-14:05:
   * Cmd+Shift+[ / Cmd+Shift+] and Cmd+Shift+Tab / Cmd+Tab traverse sidebar rows exactly as rendered across expanded groups, but skip rows whose session card is sleeping. Read row state from the DOM so collapsed groups and filtered rows do not participate in navigation.
   */
  return createRenderedSidebarSessionSlots(
    Array.from(
      root.querySelectorAll<HTMLElement>("[data-sidebar-session-id]"),
    ),
  );
}
