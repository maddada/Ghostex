import { create } from "zustand";
import { createDefaultSidebarAgentButtons } from "../shared/sidebar-agents";
import { createDefaultSidebarCommandButtons } from "../shared/sidebar-commands";
import { DEFAULT_COMPLETION_SOUND, getCompletionSoundLabel } from "../shared/completion-sound";
import { DEFAULT_ghostex_SETTINGS } from "../shared/ghostex-settings";
import { createDefaultSidebarGitState, type SidebarGitFileDiffDraft } from "../shared/sidebar-git";
import type {
  SidebarCommandRunStateClearedMessage,
  SidebarCommandRunStateChangedMessage,
  SidebarDaemonSessionsStateMessage,
  SidebarHydrateMessage,
  SidebarHudState,
  SidebarOrderSyncResultMessage,
  SidebarPinnedPrompt,
  SidebarPreviousSessionItem,
  SidebarPromptGitCommitMessage,
  SidebarSessionGroup,
  SidebarSessionItem,
  SidebarSessionPresentationChangedMessage,
  SidebarSessionStateMessage,
} from "../shared/session-grid-contract";
import {
  applySidebarCommandRunStateChangedMessage,
  reconcileSidebarCommandRunFeedbackStates,
  type SidebarCommandRunFeedbackState,
} from "./command-run-feedback";

export type SidebarGroupRecord = Omit<SidebarSessionGroup, "sessions">;

type SidebarStoreDataState = {
  commandRunStates: Record<string, SidebarCommandRunFeedbackState>;
  daemonSessionsState: SidebarDaemonSessionsStateMessage | undefined;
  gitCommitDraft: SidebarPromptGitCommitMessage | undefined;
  gitFileDiffDraft: SidebarGitFileDiffDraft | undefined;
  groupOrder: string[];
  groupsById: Record<string, SidebarGroupRecord>;
  hud: SidebarHudState;
  latestAgentOrderSyncResult: SidebarOrderSyncResultMessage | undefined;
  latestCommandOrderSyncResult: SidebarOrderSyncResultMessage | undefined;
  localHiddenSessionIds: Record<string, true>;
  localSessionSleepingOverrides: Record<string, boolean>;
  pendingFocusedSessionId: string | undefined;
  pinnedPrompts: SidebarPinnedPrompt[];
  previousSessions: SidebarPreviousSessionItem[];
  revision: number;
  scratchPadContent: string;
  sessionIdsByGroup: Record<string, string[]>;
  sessionsById: Record<string, SidebarSessionItem>;
  workspaceGroupIds: string[];
};

type SidebarStoreActions = {
  applyCommandRunStateClearedMessage: (message: SidebarCommandRunStateClearedMessage) => void;
  applyCommandRunStateMessage: (message: SidebarCommandRunStateChangedMessage) => void;
  applyOrderSyncResultMessage: (message: SidebarOrderSyncResultMessage) => void;
  applyLocalFocus: (groupId: string, sessionId: string) => void;
  hideSessionLocally: (sessionId: string) => void;
  hideSessionsLocally: (sessionIds: readonly string[]) => void;
  setSessionSleepingLocally: (sessionId: string, sleeping: boolean) => void;
  setSessionsSleepingLocally: (sessionIds: readonly string[], sleeping: boolean) => void;
  applySessionPresentationMessage: (message: SidebarSessionPresentationChangedMessage) => void;
  applySidebarMessage: (message: SidebarHydrateMessage | SidebarSessionStateMessage) => void;
  clearCommandRunState: (commandId: string) => void;
  reset: () => void;
  setDaemonSessionsState: (message: SidebarDaemonSessionsStateMessage | undefined) => void;
  setGitCommitDraft: (message: SidebarPromptGitCommitMessage | undefined) => void;
  setGitFileDiffDraft: (draft: SidebarGitFileDiffDraft | undefined) => void;
};

export type SidebarStoreState = SidebarStoreDataState & SidebarStoreActions;

export function createInitialSidebarStoreDataState(): SidebarStoreDataState {
  return {
    commandRunStates: {},
    daemonSessionsState: undefined,
    gitCommitDraft: undefined,
    gitFileDiffDraft: undefined,
    groupOrder: [],
    groupsById: {},
    hud: {
      /**
       * CDXC:SidebarSessions 2026-04-28-05:18
       * The client store must match the shared/native default so sidebar
       * sessions start sorted by last activity before the first hydrate message.
       */
      activeSessionsSortMode: "lastActivity",
      agentManagerZoomPercent: 100,
      agents: createDefaultSidebarAgentButtons(),
      commands: createDefaultSidebarCommandButtons(),
      commandSessionIndicators: [],
      completionBellEnabled: DEFAULT_ghostex_SETTINGS.completionBellEnabled,
      completionSound: DEFAULT_COMPLETION_SOUND,
      completionSoundLabel: getCompletionSoundLabel(DEFAULT_COMPLETION_SOUND),
      customThemeColor: undefined,
      debuggingMode: false,
      focusedSessionTitle: undefined,
      git: createDefaultSidebarGitState(),
      highlightedVisibleCount: 1,
      isFocusModeActive: false,
      pendingAgentIds: [],
      projectSettingsProjects: [],
      recentProjects: [],
      settings: DEFAULT_ghostex_SETTINGS,
      createSessionOnSidebarDoubleClick: false,
      renameSessionOnDoubleClick: false,
      showCloseButtonOnSessionCards: DEFAULT_ghostex_SETTINGS.showCloseButtonOnSessionCards,
      theme: getInitialSidebarTheme(),
      viewMode: "grid",
      visibleCount: 1,
      visibleSlotLabels: [],
    },
    latestAgentOrderSyncResult: undefined,
    latestCommandOrderSyncResult: undefined,
    localHiddenSessionIds: {},
    localSessionSleepingOverrides: {},
    pendingFocusedSessionId: undefined,
    pinnedPrompts: [],
    previousSessions: [],
    revision: 0,
    scratchPadContent: "",
    sessionIdsByGroup: {},
    sessionsById: {},
    workspaceGroupIds: [],
  };
}

export const useSidebarStore = create<SidebarStoreState>((set) => ({
  ...createInitialSidebarStoreDataState(),
  applyCommandRunStateClearedMessage: (message) => {
    set((state) => {
      if (!(message.commandId in state.commandRunStates)) {
        return state;
      }

      const nextCommandRunStates = { ...state.commandRunStates };
      delete nextCommandRunStates[message.commandId];
      return {
        commandRunStates: nextCommandRunStates,
      };
    });
  },
  applyCommandRunStateMessage: (message) => {
    set((state) => {
      const nextCommandRunState = applySidebarCommandRunStateChangedMessage(
        state.commandRunStates[message.commandId],
        message,
      );
      if (state.commandRunStates[message.commandId] === nextCommandRunState) {
        return state;
      }

      return {
        commandRunStates: {
          ...state.commandRunStates,
          [message.commandId]: nextCommandRunState,
        },
      };
    });
  },
  applyOrderSyncResultMessage: (message) => {
    set({
      latestAgentOrderSyncResult: message.kind === "agent" ? message : undefined,
      latestCommandOrderSyncResult: message.kind === "command" ? message : undefined,
    });
  },
  applyLocalFocus: (groupId, sessionId) => {
    set((state) => applyLocalFocusState(state, groupId, sessionId));
  },
  hideSessionLocally: (sessionId) => {
    set((state) => hideSessionLocallyState(state, sessionId));
  },
  hideSessionsLocally: (sessionIds) => {
    set((state) => hideSessionsLocallyState(state, sessionIds));
  },
  setSessionSleepingLocally: (sessionId, sleeping) => {
    set((state) => setSessionSleepingLocallyState(state, sessionId, sleeping));
  },
  setSessionsSleepingLocally: (sessionIds, sleeping) => {
    set((state) => setSessionsSleepingLocallyState(state, sessionIds, sleeping));
  },
  applySessionPresentationMessage: (message) => {
    set((state) => applySessionPresentationMessageState(state, message));
  },
  applySidebarMessage: (message) => {
    set((state) => applySidebarMessageState(state, message));
  },
  clearCommandRunState: (commandId) => {
    set((state) => {
      if (!(commandId in state.commandRunStates)) {
        return state;
      }

      const nextCommandRunStates = { ...state.commandRunStates };
      delete nextCommandRunStates[commandId];
      return {
        commandRunStates: nextCommandRunStates,
      };
    });
  },
  reset: () => {
    set(createInitialSidebarStoreDataState());
  },
  setDaemonSessionsState: (message) => {
    set({ daemonSessionsState: message });
  },
  setGitCommitDraft: (message) => {
    set({ gitCommitDraft: message, gitFileDiffDraft: undefined });
  },
  setGitFileDiffDraft: (draft) => {
    set({ gitFileDiffDraft: draft });
  },
}));

export function resetSidebarStore() {
  useSidebarStore.getState().reset();
}

function getInitialSidebarTheme(): SidebarHudState["theme"] {
  if (typeof document === "undefined") {
    return "dark-blue";
  }

  return document.body.classList.contains("vscode-light") ||
    document.body.classList.contains("vscode-high-contrast-light")
    ? "light-blue"
    : "dark-blue";
}

function applySidebarMessageState(
  state: SidebarStoreState,
  message: SidebarHydrateMessage | SidebarSessionStateMessage,
): Partial<SidebarStoreState> | SidebarStoreState {
  if (message.revision < state.revision) {
    return state;
  }

  const localFirstFiltered = filterLocallyHiddenSidebarSessions(
    message.groups,
    state.localHiddenSessionIds,
  );
  const localSleepApplied = applyLocalSessionSleepingOverrides(
    localFirstFiltered.groups,
    state.localSessionSleepingOverrides,
  );
  const reconciledGroups = reconcilePendingFocusedSession(
    localSleepApplied.groups,
    state.pendingFocusedSessionId,
  );
  const normalizedGroups = normalizeSidebarGroups(state, reconciledGroups.groups);
  const nextHud = preserveSidebarHudReferences(state.hud, {
    ...message.hud,
    projectSettingsProjects: message.hud.projectSettingsProjects ?? [],
    recentProjects: message.hud.recentProjects ?? [],
  });
  return {
    commandRunStates: reconcileSidebarCommandRunFeedbackStates(
      state.commandRunStates,
      message.hud.commands.map((command) => command.commandId),
    ),
    groupOrder: normalizedGroups.groupOrder,
    groupsById: normalizedGroups.groupsById,
    hud: nextHud,
    localHiddenSessionIds: localFirstFiltered.localHiddenSessionIds,
    localSessionSleepingOverrides: localSleepApplied.localSessionSleepingOverrides,
    pendingFocusedSessionId: reconciledGroups.pendingFocusedSessionId,
    pinnedPrompts: message.pinnedPrompts,
    previousSessions: message.previousSessions,
    revision: message.revision,
    scratchPadContent: message.scratchPadContent,
    sessionIdsByGroup: normalizedGroups.sessionIdsByGroup,
    sessionsById: normalizedGroups.sessionsById,
    workspaceGroupIds: normalizedGroups.workspaceGroupIds,
  };
}

/**
 * CDXC:AppModals 2026-05-29-19:44:
 * App-level modals keep user drafts in local React state while the sidebar
 * store still receives full session snapshots for attention/activity updates.
 * Preserve unchanged HUD slice references so unrelated session changes do not
 * look like fresh modal props and reset open drafts.
 */
function preserveSidebarHudReferences(
  previousHud: SidebarHudState,
  nextHud: SidebarHudState,
): SidebarHudState {
  let mergedHud = nextHud;
  const preserveIfEqual = <Key extends keyof SidebarHudState>(key: Key) => {
    if (!haveSameSerializableValue(previousHud[key], nextHud[key])) {
      return;
    }
    if (mergedHud === nextHud) {
      mergedHud = { ...nextHud };
    }
    mergedHud[key] = previousHud[key];
  };

  preserveIfEqual("agents");
  preserveIfEqual("commands");
  preserveIfEqual("commandSessionIndicators");
  preserveIfEqual("currentLayout");
  preserveIfEqual("git");
  preserveIfEqual("pendingAgentIds");
  preserveIfEqual("projectSettingsProjects");
  preserveIfEqual("projectWorktrees");
  preserveIfEqual("recentProjects");
  preserveIfEqual("settings");
  preserveIfEqual("visibleSlotLabels");

  return haveSameSerializableValue(previousHud, mergedHud) ? previousHud : mergedHud;
}

function haveSameSerializableValue(left: unknown, right: unknown): boolean {
  if (Object.is(left, right)) {
    return true;
  }
  if (typeof left !== typeof right) {
    return false;
  }
  if (typeof left !== "object" || left === null || right === null) {
    return false;
  }
  if (Array.isArray(left) || Array.isArray(right)) {
    if (!Array.isArray(left) || !Array.isArray(right) || left.length !== right.length) {
      return false;
    }
    return left.every((value, index) => haveSameSerializableValue(value, right[index]));
  }

  const leftRecord = left as Record<string, unknown>;
  const rightRecord = right as Record<string, unknown>;
  const leftKeys = Object.keys(leftRecord);
  const rightKeys = Object.keys(rightRecord);
  return (
    leftKeys.length === rightKeys.length &&
    leftKeys.every((key) => haveSameSerializableValue(leftRecord[key], rightRecord[key]))
  );
}

function applySessionPresentationMessageState(
  state: SidebarStoreState,
  message: SidebarSessionPresentationChangedMessage,
): Partial<SidebarStoreState> | SidebarStoreState {
  const currentSession = state.sessionsById[message.session.sessionId];
  if (!currentSession || haveSameSidebarSessionItem(currentSession, message.session)) {
    return state;
  }

  return {
    sessionsById: {
      ...state.sessionsById,
      [message.session.sessionId]: message.session,
    },
  };
}

function hideSessionLocallyState(
  state: SidebarStoreState,
  sessionId: string,
): Partial<SidebarStoreState> | SidebarStoreState {
  if (!state.sessionsById[sessionId] && state.localHiddenSessionIds[sessionId]) {
    return state;
  }

  /*
  CDXC:LocalFirstSidebar 2026-06-01-19:34:
  Closing a session must remove the card in the same React event as the click. Native still owns the durable close, but the sidebar store keeps a local hidden-id overlay so no hydrate can briefly reinsert the card while gxserver catches up.
  */
  const nextSessionsById = { ...state.sessionsById };
  delete nextSessionsById[sessionId];

  let didChangeSessionOrder = false;
  const nextSessionIdsByGroup: Record<string, string[]> = {};
  for (const [groupId, sessionIds] of Object.entries(state.sessionIdsByGroup)) {
    const nextSessionIds = sessionIds.filter((candidate) => candidate !== sessionId);
    nextSessionIdsByGroup[groupId] = nextSessionIds;
    if (nextSessionIds.length !== sessionIds.length) {
      didChangeSessionOrder = true;
    }
  }

  return {
    localHiddenSessionIds: {
      ...state.localHiddenSessionIds,
      [sessionId]: true,
    },
    pendingFocusedSessionId:
      state.pendingFocusedSessionId === sessionId ? undefined : state.pendingFocusedSessionId,
    sessionIdsByGroup: didChangeSessionOrder ? nextSessionIdsByGroup : state.sessionIdsByGroup,
    sessionsById: nextSessionsById,
  };
}

function hideSessionsLocallyState(
  state: SidebarStoreState,
  sessionIds: readonly string[],
): Partial<SidebarStoreState> | SidebarStoreState {
  const hiddenSessionIds = Array.from(new Set(sessionIds));
  if (hiddenSessionIds.length === 0) {
    return state;
  }

  const existingTargetIds = hiddenSessionIds.filter(
    (sessionId) => state.sessionsById[sessionId] || !state.localHiddenSessionIds[sessionId],
  );
  if (existingTargetIds.length === 0) {
    return state;
  }

  /*
  CDXC:SidebarContextMenu 2026-06-07-13:34:
  Bulk context-menu actions such as Close below must update sidebar chrome once before native fan-out starts. Avoid one Zustand write per target because each write can re-render the session list and keep the menu/sidebar busy until every native close has been queued.
  */
  const targetIdSet = new Set(existingTargetIds);
  const nextSessionsById = { ...state.sessionsById };
  for (const sessionId of targetIdSet) {
    delete nextSessionsById[sessionId];
  }

  let didChangeSessionOrder = false;
  const nextSessionIdsByGroup: Record<string, string[]> = {};
  for (const [groupId, groupSessionIds] of Object.entries(state.sessionIdsByGroup)) {
    const nextGroupSessionIds = groupSessionIds.filter((sessionId) => !targetIdSet.has(sessionId));
    nextSessionIdsByGroup[groupId] = nextGroupSessionIds;
    if (nextGroupSessionIds.length !== groupSessionIds.length) {
      didChangeSessionOrder = true;
    }
  }

  return {
    localHiddenSessionIds: {
      ...state.localHiddenSessionIds,
      ...Object.fromEntries(existingTargetIds.map((sessionId) => [sessionId, true] as const)),
    },
    pendingFocusedSessionId:
      state.pendingFocusedSessionId && targetIdSet.has(state.pendingFocusedSessionId)
        ? undefined
        : state.pendingFocusedSessionId,
    sessionIdsByGroup: didChangeSessionOrder ? nextSessionIdsByGroup : state.sessionIdsByGroup,
    sessionsById: nextSessionsById,
  };
}

function filterLocallyHiddenSidebarSessions(
  groups: readonly SidebarSessionGroup[],
  localHiddenSessionIds: Record<string, true>,
): {
  groups: SidebarSessionGroup[];
  localHiddenSessionIds: Record<string, true>;
} {
  const hiddenIds = Object.keys(localHiddenSessionIds);
  if (hiddenIds.length === 0) {
    return {
      groups: [...groups],
      localHiddenSessionIds,
    };
  }

  const incomingSessionIds = new Set(
    groups.flatMap((group) => (group.sessions ?? []).map((session) => session.sessionId)),
  );
  const nextLocalHiddenSessionIds: Record<string, true> = {};
  for (const sessionId of hiddenIds) {
    if (incomingSessionIds.has(sessionId)) {
      nextLocalHiddenSessionIds[sessionId] = true;
    }
  }
  if (Object.keys(nextLocalHiddenSessionIds).length === 0) {
    return {
      groups: [...groups],
      localHiddenSessionIds: nextLocalHiddenSessionIds,
    };
  }

  return {
    groups: groups.map((group) => {
      const sessions = group.sessions ?? [];
      if (!sessions.some((session) => nextLocalHiddenSessionIds[session.sessionId])) {
        return group;
      }
      return {
        ...group,
        sessions: sessions.filter((session) => !nextLocalHiddenSessionIds[session.sessionId]),
      };
    }),
    localHiddenSessionIds: nextLocalHiddenSessionIds,
  };
}

function setSessionSleepingLocallyState(
  state: SidebarStoreState,
  sessionId: string,
  sleeping: boolean,
): Partial<SidebarStoreState> | SidebarStoreState {
  const session = state.sessionsById[sessionId];
  if (!session && state.localSessionSleepingOverrides[sessionId] === sleeping) {
    return state;
  }

  /*
  CDXC:LocalFirstSidebar 2026-06-01-19:34:
  Sleep and Wake from the session context menu should dismiss the menu and flip the card state before native disposes or recreates the terminal surface. Keep a local override until the host snapshot confirms the same sleeping state.
  */
  return {
    localSessionSleepingOverrides: {
      ...state.localSessionSleepingOverrides,
      [sessionId]: sleeping,
    },
    sessionsById: session
      ? {
          ...state.sessionsById,
          [sessionId]: applyLocalSessionSleepingOverride(session, sleeping),
        }
      : state.sessionsById,
  };
}

function setSessionsSleepingLocallyState(
  state: SidebarStoreState,
  sessionIds: readonly string[],
  sleeping: boolean,
): Partial<SidebarStoreState> | SidebarStoreState {
  const targetSessionIds = Array.from(new Set(sessionIds));
  if (targetSessionIds.length === 0) {
    return state;
  }

  /*
  CDXC:SidebarContextMenu 2026-06-07-13:34:
  Sleep below can target many rows. Apply the local sleeping overlay to all awake targets in one store write so the context menu closes and the sidebar paints once while native session shutdown proceeds asynchronously.
  */
  let nextSessionSleepingOverrides: Record<string, boolean> | undefined;
  let nextSessionsById: Record<string, SidebarSessionItem> | undefined;

  for (const sessionId of targetSessionIds) {
    const session = (nextSessionsById ?? state.sessionsById)[sessionId];
    if (
      session &&
      session.isSleeping === sleeping &&
      session.lifecycleState === (sleeping ? "sleeping" : "running")
    ) {
      continue;
    }
    if (!session && (nextSessionSleepingOverrides ?? state.localSessionSleepingOverrides)[sessionId] === sleeping) {
      continue;
    }

    nextSessionSleepingOverrides ??= { ...state.localSessionSleepingOverrides };
    nextSessionSleepingOverrides[sessionId] = sleeping;
    if (session) {
      nextSessionsById ??= { ...state.sessionsById };
      nextSessionsById[sessionId] = applyLocalSessionSleepingOverride(session, sleeping);
    }
  }

  if (!nextSessionSleepingOverrides && !nextSessionsById) {
    return state;
  }

  return {
    localSessionSleepingOverrides: nextSessionSleepingOverrides ?? state.localSessionSleepingOverrides,
    sessionsById: nextSessionsById ?? state.sessionsById,
  };
}

function applyLocalSessionSleepingOverrides(
  groups: readonly SidebarSessionGroup[],
  localSessionSleepingOverrides: Record<string, boolean>,
): {
  groups: SidebarSessionGroup[];
  localSessionSleepingOverrides: Record<string, boolean>;
} {
  const overrideEntries = Object.entries(localSessionSleepingOverrides);
  if (overrideEntries.length === 0) {
    return {
      groups: [...groups],
      localSessionSleepingOverrides,
    };
  }

  const incomingSessionIds = new Set(
    groups.flatMap((group) => (group.sessions ?? []).map((session) => session.sessionId)),
  );
  const nextOverrides: Record<string, boolean> = {};
  for (const [sessionId, sleeping] of overrideEntries) {
    if (incomingSessionIds.has(sessionId)) {
      nextOverrides[sessionId] = sleeping;
    }
  }
  if (Object.keys(nextOverrides).length === 0) {
    return {
      groups: [...groups],
      localSessionSleepingOverrides: nextOverrides,
    };
  }

  return {
    groups: groups.map((group) => {
      const sessions = group.sessions ?? [];
      if (!sessions.some((session) => session.sessionId in nextOverrides)) {
        return group;
      }
      return {
        ...group,
        sessions: sessions.map((session) => {
          const override = nextOverrides[session.sessionId];
          if (override === undefined) {
            return session;
          }
          if (session.isSleeping === override) {
            delete nextOverrides[session.sessionId];
            return session;
          }
          return applyLocalSessionSleepingOverride(session, override);
        }),
      };
    }),
    localSessionSleepingOverrides: nextOverrides,
  };
}

function applyLocalSessionSleepingOverride(
  session: SidebarSessionItem,
  sleeping: boolean,
): SidebarSessionItem {
  return {
    ...session,
    isRunning: sleeping ? false : true,
    isSleeping: sleeping,
    lifecycleState: sleeping ? "sleeping" : "running",
  };
}

function applyLocalFocusState(
  state: SidebarStoreState,
  groupId: string,
  sessionId: string,
): Partial<SidebarStoreState> | SidebarStoreState {
  if (!state.groupsById[groupId] || !state.sessionsById[sessionId]) {
    return state;
  }

  let groupsById = state.groupsById;
  let sessionsById = state.sessionsById;

  for (const candidateGroupId of state.groupOrder) {
    const group = state.groupsById[candidateGroupId];
    if (!group) {
      continue;
    }

    const isActiveGroup = candidateGroupId === groupId;
    if (group.isActive !== isActiveGroup) {
      if (groupsById === state.groupsById) {
        groupsById = { ...state.groupsById };
      }
      groupsById[candidateGroupId] = {
        ...group,
        isActive: isActiveGroup,
      };
    }

    for (const candidateSessionId of state.sessionIdsByGroup[candidateGroupId] ?? []) {
      const session = state.sessionsById[candidateSessionId];
      if (!session) {
        continue;
      }

      const isFocused = isActiveGroup && candidateSessionId === sessionId;
      const isVisible =
        group.kind !== "browser" && isActiveGroup && candidateSessionId === sessionId
          ? true
          : session.isVisible;
      if (session.isFocused === isFocused && session.isVisible === isVisible) {
        continue;
      }

      if (sessionsById === state.sessionsById) {
        sessionsById = { ...state.sessionsById };
      }
      sessionsById[candidateSessionId] = {
        ...session,
        isFocused,
        isVisible,
      };
    }
  }

  if (
    groupsById === state.groupsById &&
    sessionsById === state.sessionsById &&
    state.pendingFocusedSessionId === sessionId
  ) {
    return state;
  }

  return {
    groupsById,
    pendingFocusedSessionId: sessionId,
    sessionsById,
  };
}

function normalizeSidebarGroups(
  previousState: Pick<
    SidebarStoreDataState,
    | "groupOrder"
    | "groupsById"
    | "sessionIdsByGroup"
    | "sessionsById"
    | "workspaceGroupIds"
  >,
  groups: readonly SidebarSessionGroup[],
) {
  const nextGroupOrder = groups.map((group) => group.groupId);
  const nextWorkspaceGroupIds: string[] = [];
  const nextGroupsById: Record<string, SidebarGroupRecord> = {};
  const nextSessionIdsByGroup: Record<string, string[]> = {};
  const nextSessionsById: Record<string, SidebarSessionItem> = {};

  for (const group of groups) {
    const groupSessions = group.sessions ?? [];

    if (group.kind !== "browser") {
      nextWorkspaceGroupIds.push(group.groupId);
    }

    const previousGroup = previousState.groupsById[group.groupId];
    const nextGroup = toSidebarGroupRecord(group);
    nextGroupsById[group.groupId] =
      previousGroup && haveSameSidebarGroupRecord(previousGroup, nextGroup)
        ? previousGroup
        : nextGroup;

    const nextSessionIds = groupSessions.map((session) => session.sessionId);
    const previousSessionIds = previousState.sessionIdsByGroup[group.groupId];
    nextSessionIdsByGroup[group.groupId] =
      previousSessionIds && haveSameStringArray(previousSessionIds, nextSessionIds)
        ? previousSessionIds
        : nextSessionIds;

    for (const session of groupSessions) {
      const previousSession = previousState.sessionsById[session.sessionId];
      nextSessionsById[session.sessionId] =
        previousSession && haveSameSidebarSessionItem(previousSession, session)
          ? previousSession
          : session;
    }
  }

  return {
    groupOrder: haveSameStringArray(previousState.groupOrder, nextGroupOrder)
      ? previousState.groupOrder
      : nextGroupOrder,
    groupsById: nextGroupsById,
    sessionIdsByGroup: nextSessionIdsByGroup,
    sessionsById: nextSessionsById,
    workspaceGroupIds: haveSameStringArray(previousState.workspaceGroupIds, nextWorkspaceGroupIds)
      ? previousState.workspaceGroupIds
      : nextWorkspaceGroupIds,
  };
}

function reconcilePendingFocusedSession(
  groups: readonly SidebarSessionGroup[],
  pendingFocusedSessionId: string | undefined,
): {
  groups: SidebarSessionGroup[];
  pendingFocusedSessionId: string | undefined;
} {
  if (!pendingFocusedSessionId) {
    return {
      groups: [...groups],
      pendingFocusedSessionId: undefined,
    };
  }

  const containingGroup = groups.find((group) =>
    (group.sessions ?? []).some((session) => session.sessionId === pendingFocusedSessionId),
  );
  if (!containingGroup) {
    return {
      groups: [...groups],
      pendingFocusedSessionId: undefined,
    };
  }

  const isConfirmed = (containingGroup.sessions ?? []).some(
    (session) => session.sessionId === pendingFocusedSessionId && session.isFocused,
  );
  if (isConfirmed) {
    return {
      groups: [...groups],
      pendingFocusedSessionId: undefined,
    };
  }

  return {
    groups: groups.map((group) => {
      const isActiveGroup = group.groupId === containingGroup.groupId;
      return {
        ...group,
        isActive: isActiveGroup,
        sessions: (group.sessions ?? []).map((session) => ({
          ...session,
          isFocused: isActiveGroup && session.sessionId === pendingFocusedSessionId,
          isVisible:
            group.kind !== "browser" &&
            isActiveGroup &&
            session.sessionId === pendingFocusedSessionId
              ? true
              : session.isVisible,
        })),
      };
    }),
    pendingFocusedSessionId,
  };
}

function toSidebarGroupRecord(group: SidebarSessionGroup): SidebarGroupRecord {
  return {
    groupId: group.groupId,
    isActive: group.isActive,
    /**
     * CDXC:Chats 2026-05-05-18:37
     * The synthetic Chats group must keep its explicit collection marker after
     * store normalization. Without this flag, the header icon falls through to
     * project folder icons when Chats is expanded or collapsed.
     */
    isChatCollection: group.isChatCollection,
    canFocusMode: group.canFocusMode,
    isFocusModeActive: group.isFocusModeActive,
    kind: group.kind,
    layoutVisibleCount: group.layoutVisibleCount,
    projectContext: group.projectContext,
    remoteMachineContext: group.remoteMachineContext,
    title: group.title,
    viewMode: group.viewMode,
    visibleCount: group.visibleCount,
  };
}

function haveSameSidebarGroupRecord(left: SidebarGroupRecord, right: SidebarGroupRecord): boolean {
  return (
    left.groupId === right.groupId &&
    left.isActive === right.isActive &&
    left.isChatCollection === right.isChatCollection &&
    left.canFocusMode === right.canFocusMode &&
    left.isFocusModeActive === right.isFocusModeActive &&
    left.kind === right.kind &&
    left.layoutVisibleCount === right.layoutVisibleCount &&
    haveSameSidebarProjectContext(left.projectContext, right.projectContext) &&
    haveSameSidebarRemoteMachineContext(left.remoteMachineContext, right.remoteMachineContext) &&
    left.title === right.title &&
    left.viewMode === right.viewMode &&
    left.visibleCount === right.visibleCount
  );
}

function haveSameSidebarRemoteMachineContext(
  left: SidebarGroupRecord["remoteMachineContext"],
  right: SidebarGroupRecord["remoteMachineContext"],
): boolean {
  if (!left || !right) {
    return left === right;
  }
  return left.machineId === right.machineId && left.machineName === right.machineName;
}

function haveSameSidebarProjectContext(
  left: SidebarGroupRecord["projectContext"],
  right: SidebarGroupRecord["projectContext"],
): boolean {
  if (!left || !right) {
    return left === right;
  }

  /**
   * CDXC:EditorPanes 2026-05-06-14:21
   * Project editor buttons update from native diff-stat refreshes and open
   * state changes. Include editor context in group equality so the store does
   * not suppress those project-card updates as duplicate hydration payloads.
   *
   * CDXC:EditorPanes 2026-05-09-17:24
   * Project editor load status and error text are sidebar-visible state. They
   * must participate in equality so opening, timeout, and crash diagnostics are
   * not dropped when the focused workspace switches to a terminal session.
   */
  return (
    left.canRemoveProject === right.canRemoveProject &&
    left.path === right.path &&
    left.theme === right.theme &&
    left.themeColor === right.themeColor &&
    left.worktree?.branch === right.worktree?.branch &&
    left.worktree?.name === right.worktree?.name &&
    left.worktree?.parentProjectId === right.worktree?.parentProjectId &&
    left.editor.projectId === right.editor.projectId &&
    left.editor.errorMessage === right.editor.errorMessage &&
    left.editor.isOpen === right.editor.isOpen &&
    left.editor.isSleeping === right.editor.isSleeping &&
    left.editor.status === right.editor.status &&
    left.editor.diffStats.additions === right.editor.diffStats.additions &&
    left.editor.diffStats.deletions === right.editor.diffStats.deletions &&
    left.editor.diffStats.files === right.editor.diffStats.files &&
    left.editor.diffStats.isLoading === right.editor.diffStats.isLoading &&
    left.editor.diffStats.isRepo === right.editor.diffStats.isRepo
  );
}

function haveSameSidebarSessionItem(left: SidebarSessionItem, right: SidebarSessionItem): boolean {
  return (
    left.activity === right.activity &&
    left.activityLabel === right.activityLabel &&
    left.agentIcon === right.agentIcon &&
    left.alias === right.alias &&
    left.column === right.column &&
    left.delayedSendDeadlineAt === right.delayedSendDeadlineAt &&
    left.delayedSendRemainingLabel === right.delayedSendRemainingLabel &&
    left.delayedSendRemainingMs === right.delayedSendRemainingMs &&
    left.detail === right.detail &&
    left.displayTitle === right.displayTitle &&
    left.displayTitleTooltip === right.displayTitleTooltip &&
    left.faviconDataUrl === right.faviconDataUrl &&
    left.isGeneratingFirstPromptTitle === right.isGeneratingFirstPromptTitle &&
    left.isReloading === right.isReloading &&
    left.lifecycleState === right.lifecycleState &&
    left.isFocused === right.isFocused &&
    left.isFavorite === right.isFavorite &&
    left.isPinned === right.isPinned &&
    left.sessionTag === right.sessionTag &&
    left.isSleeping === right.isSleeping &&
    left.isRunning === right.isRunning &&
    left.isVisible === right.isVisible &&
    left.isPrimaryTitleTerminalTitle === right.isPrimaryTitleTerminalTitle &&
    left.kind === right.kind &&
    left.lastInteractionAt === right.lastInteractionAt &&
    left.primaryTitle === right.primaryTitle &&
    left.row === right.row &&
    left.sessionId === right.sessionId &&
    left.sessionKind === right.sessionKind &&
    left.sessionNumber === right.sessionNumber &&
    left.shortcutLabel === right.shortcutLabel &&
    left.terminalTitle === right.terminalTitle
  );
}

function haveSameStringArray(left: readonly string[], right: readonly string[]): boolean {
  if (left.length !== right.length) {
    return false;
  }

  return left.every((value, index) => value === right[index]);
}
