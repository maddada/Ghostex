import {
  createSidebarHudState,
  createSidebarSessionItems,
  getSessionGridLayoutVisibleCount,
  isSessionGridFocusModeActive,
  type GroupedSessionWorkspaceSnapshot,
  type SessionGroupRecord,
  type SessionRecord,
  type SidebarHydrateMessage,
  type SidebarCommandSessionIndicator,
  type SidebarSessionItem,
  type SidebarSessionStateMessage,
  type SidebarToExtensionMessage,
} from "../shared/session-grid-contract";
import type {
  SidebarV2SessionOverrides,
  SidebarV2SessionSource,
} from "../shared/sidebar-v2-session";
import type { SidebarAgentButton } from "../shared/sidebar-agents";
import type { SidebarCommandButton } from "../shared/sidebar-commands";
import { normalizeghostexSettings, type ghostexSettings } from "../shared/ghostex-settings";
import {
  createGroupInWorkspace,
  createGroupFromSessionInWorkspace,
  focusGroupInWorkspace,
  focusSessionInWorkspace,
  moveSessionToGroupInWorkspace,
  normalizeGroupedSessionWorkspaceSnapshot,
  setViewModeInWorkspace,
  setVisibleCountInWorkspace,
  syncGroupOrderInWorkspace,
  syncSessionOrderInWorkspace,
  toggleFullscreenSessionInWorkspace,
} from "../shared/grouped-session-workspace-state";

type SidebarStoryWorkspaceOptions = {
  activeSessionsSortMode: SidebarHydrateMessage["hud"]["activeSessionsSortMode"];
  agentManagerZoomPercent: number;
  agents: SidebarAgentButton[];
  commands: SidebarCommandButton[];
  commandSessionIndicators: SidebarCommandSessionIndicator[];
  completionBellEnabled: boolean;
  completionSound: SidebarHydrateMessage["hud"]["completionSound"];
  debuggingMode: boolean;
  /*
   * CDXC:SidebarV2Lifecycle 2026-07-29:
   * The story workspace rebuilds the HUD from scratch on every round trip, so
   * anything not carried here is silently dropped. Capability has to survive:
   * without it the V2 shelves classify nothing and the lifecycle stories would
   * pass against an empty inbox.
   */
  lifecycleCapabilities?: SidebarHydrateMessage["hud"]["lifecycleCapabilities"];
  /*
   * CDXC:SidebarV2LogicalProjects 2026-07-29:
   * Per-remote-machine capability and per-daemon auto-settle window. Same
   * reason as the local capability above: the HUD is rebuilt from scratch every
   * round trip, so a multi-machine story would lose its remote machine's
   * lifecycle answers and its own settle window on the first re-render.
   */
  autoSettleAfterDays?: SidebarHydrateMessage["hud"]["autoSettleAfterDays"];
  autoSettleAfterDaysByMachineId?: SidebarHydrateMessage["hud"]["autoSettleAfterDaysByMachineId"];
  lifecycleCapabilitiesByMachineId?: SidebarHydrateMessage["hud"]["lifecycleCapabilitiesByMachineId"];
  recentProjects: SidebarHydrateMessage["hud"]["recentProjects"];
  scratchPadContent: string;
  showCloseButtonOnSessionCards: boolean;
  settings?: ghostexSettings;
  theme: SidebarHydrateMessage["hud"]["theme"];
};

type SidebarSessionDecoration = Pick<
  SidebarSessionItem,
  | "activity"
  | "activityLabel"
  | "agentIcon"
  | "createdAt"
  /*
   * CDXC:SidebarV2Worktree 2026-07-29:
   * The session's working directory. V2 identifies a worktree by the PAIR of
   * cwd and branch, so dropping it here would make every worktree story
   * unreachable even with the branch present.
   */
  | "cwd"
  | "detail"
  /*
   * CDXC:SidebarV2ContextMenuParity 2026-07-30:
   * The four fields the session context menu gates items on. Without them the
   * round trip silently strips the row's captured 1st user message (View 1st
   * message, Generate Title), its stored provider/name pair (Copy attach
   * command), and its assigned marker (the checked option in Tag as), so those
   * items could never be exercised in Storybook — the items would simply be
   * missing and a story asserting them would look like a real product bug.
   */
  | "firstUserMessage"
  | "sessionPersistenceName"
  | "sessionPersistenceProvider"
  | "sessionTag"
  /*
   * CDXC:SidebarV2Git 2026-07-29:
   * gxserver's per-session git/PR probe. It has to survive this round trip or
   * no story can ever show the card's branch line — the snapshot rebuild below
   * only keeps fields named here.
   */
  | "gitStatus"
  | "isPinned"
  | "lifecycleState"
  | "isRunning"
  | "lastInteractionAt"
  | "terminalTitle"
  | "workingStartedAt"
> &
  /*
   * CDXC:SidebarV2 2026-07-29:
   * The Storybook workspace round trip rebuilds sidebar sessions from a
   * SessionRecord snapshot, so anything not listed here is dropped. The V2
   * lifecycle fields are carried as overrides because gxserver does not
   * publish them until P2 — declaring them here lets shelf stories exercise
   * settled/snoozed rows now without inventing a client-side lifecycle.
   */
  SidebarV2SessionOverrides;

export type SidebarStoryWorkspace = {
  groupMetadataById: Readonly<
    Record<
      string,
      Pick<
        SidebarHydrateMessage["groups"][number],
        /*
         * CDXC:SidebarV2ContextMenuParity 2026-07-30:
         * `canFocusMode` is per-group host state ("this group has split panes to
         * zoom"), and both sidebars' Focus item is gated on it. It has to survive
         * the round trip or Focus can never appear in any story.
         */
        "canFocusMode" | "isChatCollection" | "kind" | "projectContext" | "remoteMachineContext"
      >
    >
  >;
  options: SidebarStoryWorkspaceOptions;
  pinnedPrompts: SidebarHydrateMessage["pinnedPrompts"];
  previousSessions: SidebarHydrateMessage["previousSessions"];
  sessionDecorationsById: Readonly<Record<string, SidebarSessionDecoration>>;
  snapshot: GroupedSessionWorkspaceSnapshot;
};

export function createSidebarStoryWorkspace(message: SidebarHydrateMessage): SidebarStoryWorkspace {
  return {
    options: {
      activeSessionsSortMode: message.hud.activeSessionsSortMode,
      agentManagerZoomPercent: message.hud.agentManagerZoomPercent,
      agents: message.hud.agents,
      commands: message.hud.commands,
      commandSessionIndicators: message.hud.commandSessionIndicators,
      completionBellEnabled: message.hud.completionBellEnabled,
      completionSound: message.hud.completionSound,
      debuggingMode: message.hud.debuggingMode,
      autoSettleAfterDays: message.hud.autoSettleAfterDays,
      autoSettleAfterDaysByMachineId: message.hud.autoSettleAfterDaysByMachineId,
      lifecycleCapabilities: message.hud.lifecycleCapabilities,
      lifecycleCapabilitiesByMachineId: message.hud.lifecycleCapabilitiesByMachineId,
      recentProjects: message.hud.recentProjects,
      scratchPadContent: message.scratchPadContent,
      showCloseButtonOnSessionCards: message.hud.showCloseButtonOnSessionCards,
      settings: message.hud.settings,
      theme: message.hud.theme,
    },
    groupMetadataById: Object.fromEntries(
      message.groups.map((group) => [
        group.groupId,
        {
          canFocusMode: group.canFocusMode,
          /**
           * CDXC:StorybookSettings 2026-05-08-17:01
           * Combined sidebar stories must preserve the synthetic Chats marker
           * through the Storybook workspace round trip. Dropping this flag
           * makes Storybook classify Chats as a project group, which no longer
           * matches the native app's separate Chats and Projects sections.
           */
          isChatCollection: group.isChatCollection,
          kind: group.kind,
          projectContext: group.projectContext,
          /*
           * CDXC:SidebarV2LogicalProjects 2026-07-29:
           * Which machine a group belongs to survives the round trip too:
           * without it every group reads as local, and the cross-machine merge
           * and machine badges could never be exercised in Storybook.
           */
          remoteMachineContext: group.remoteMachineContext,
        },
      ]),
    ),
    pinnedPrompts: message.pinnedPrompts.map((prompt) => ({ ...prompt })),
    previousSessions: message.previousSessions.map((session) => ({ ...session })),
    sessionDecorationsById: Object.fromEntries(
      message.groups.flatMap((group) =>
        group.sessions.map((session) => {
          const v2Session = session as SidebarV2SessionSource;
          return [
            session.sessionId,
            {
              activity: session.activity,
              activityLabel: session.activityLabel,
              /*
               * CDXC:AgentDetection 2026-04-27-06:55
               * Storybook must preserve agent identity across its session-grid
               * round trip so sidebar card icon rendering can be verified there.
               */
              agentIcon: session.agentIcon,
              createdAt: session.createdAt,
              cwd: session.cwd,
              detail: session.detail,
              firstUserMessage: session.firstUserMessage,
              gitStatus: session.gitStatus,
              isPinned: session.isPinned,
              sessionPersistenceName: session.sessionPersistenceName,
              sessionPersistenceProvider: session.sessionPersistenceProvider,
              sessionTag: session.sessionTag,
              lifecycleState: session.lifecycleState,
              isRunning: session.isRunning,
              lastInteractionAt: session.lastInteractionAt,
              settledAt: v2Session.settledAt,
              settledOverride: v2Session.settledOverride,
              snoozedAt: v2Session.snoozedAt,
              snoozedUntil: v2Session.snoozedUntil,
              terminalTitle: session.terminalTitle,
              workingStartedAt: session.workingStartedAt,
            } satisfies SidebarSessionDecoration,
          ];
        }),
      ),
    ),
    snapshot: normalizeGroupedSessionWorkspaceSnapshot({
      activeGroupId:
        message.groups.find((group) => group.isActive)?.groupId ??
        message.groups[0]?.groupId ??
        "group-1",
      groups: message.groups.map((group) => createSessionGroupRecord(group)),
      nextGroupNumber: getNextGroupNumber(message.groups),
      nextSessionDisplayId: Number.NaN,
      nextSessionNumber: getNextSessionNumber(message.groups),
    }),
  };
}

export function createSidebarStoryMessage(
  workspace: SidebarStoryWorkspace,
  type: SidebarHydrateMessage["type"] | SidebarSessionStateMessage["type"] = "sessionState",
): SidebarHydrateMessage | SidebarSessionStateMessage {
  const activeGroup =
    workspace.snapshot.groups.find((group) => group.groupId === workspace.snapshot.activeGroupId) ??
    workspace.snapshot.groups[0];

  const groups = workspace.snapshot.groups.map((group) => {
    const items = createSidebarSessionItems(group.snapshot, "mac").map((item) => ({
      ...item,
      ...workspace.sessionDecorationsById[item.sessionId],
      activity: workspace.sessionDecorationsById[item.sessionId]?.activity ?? "idle",
      agentIcon: workspace.sessionDecorationsById[item.sessionId]?.agentIcon,
      lifecycleState:
        workspace.sessionDecorationsById[item.sessionId]?.lifecycleState ?? item.lifecycleState,
      isRunning: workspace.sessionDecorationsById[item.sessionId]?.isRunning ?? item.isRunning,
      lastInteractionAt: workspace.sessionDecorationsById[item.sessionId]?.lastInteractionAt,
      terminalTitle: workspace.sessionDecorationsById[item.sessionId]?.terminalTitle,
    }));

    return {
      canFocusMode: workspace.groupMetadataById[group.groupId]?.canFocusMode,
      groupId: group.groupId,
      isChatCollection: workspace.groupMetadataById[group.groupId]?.isChatCollection,
      isActive: workspace.snapshot.activeGroupId === group.groupId,
      isFocusModeActive: isSessionGridFocusModeActive(group.snapshot),
      kind: workspace.groupMetadataById[group.groupId]?.kind,
      layoutVisibleCount: getSessionGridLayoutVisibleCount(group.snapshot),
      projectContext: workspace.groupMetadataById[group.groupId]?.projectContext,
      remoteMachineContext: workspace.groupMetadataById[group.groupId]?.remoteMachineContext,
      sessions: items,
      title: group.title,
      viewMode: group.snapshot.viewMode,
      visibleCount: group.snapshot.visibleCount,
    };
  });

  const hud = createSidebarHudState(
    activeGroup?.snapshot ?? workspace.snapshot.groups[0]?.snapshot,
    workspace.options.theme,
    workspace.options.agentManagerZoomPercent,
    workspace.options.showCloseButtonOnSessionCards,
    workspace.options.debuggingMode,
    workspace.options.completionBellEnabled,
    workspace.options.completionSound,
    workspace.options.agents,
    workspace.options.commands,
    [],
    undefined,
    workspace.options.activeSessionsSortMode,
    false,
    false,
    workspace.options.commandSessionIndicators.map((indicator) => ({
      ...indicator,
      isActive: indicator.sessionId === activeGroup?.snapshot.focusedSessionId,
    })),
  );

  return {
    groups,
    hud: {
      ...hud,
      ...(workspace.options.lifecycleCapabilities
        ? { lifecycleCapabilities: workspace.options.lifecycleCapabilities }
        : {}),
      ...(workspace.options.lifecycleCapabilitiesByMachineId
        ? {
            lifecycleCapabilitiesByMachineId:
              workspace.options.lifecycleCapabilitiesByMachineId,
          }
        : {}),
      ...(workspace.options.autoSettleAfterDays === undefined
        ? {}
        : { autoSettleAfterDays: workspace.options.autoSettleAfterDays }),
      ...(workspace.options.autoSettleAfterDaysByMachineId
        ? { autoSettleAfterDaysByMachineId: workspace.options.autoSettleAfterDaysByMachineId }
        : {}),
      /*
       * CDXC:ProjectActions 2026-08-01:
       * Project rows read their Actions from `commandsByProject`, not the flat
       * active-project list, so the story harness has to publish the same block
       * the daemon does or a flagged Action can never render on a story row.
       * Every story project context sees the same command set because the
       * harness models one Actions store.
       */
      commandsByProject: Object.fromEntries(
        groups
          .map((group) => group.projectContext?.editor.projectId)
          .filter((projectId): projectId is string => Boolean(projectId))
          .map((projectId) => [projectId, workspace.options.commands]),
      ),
      recentProjects: workspace.options.recentProjects,
      settings: workspace.options.settings,
    },
    pinnedPrompts: workspace.pinnedPrompts.map((prompt) => ({ ...prompt })),
    previousSessions: workspace.previousSessions.map((session) => ({ ...session })),
    revision: 1,
    scratchPadContent: workspace.options.scratchPadContent,
    type,
  };
}

export function reduceSidebarStoryWorkspace(
  workspace: SidebarStoryWorkspace,
  message: SidebarToExtensionMessage,
): SidebarStoryWorkspace | undefined {
  switch (message.type) {
    case "sidebarDebugLog":
      return undefined;

    case "moveSessionToGroup": {
      const result = moveSessionToGroupInWorkspace(
        workspace.snapshot,
        message.sessionId,
        message.groupId,
        message.targetIndex,
      );
      return result.changed ? { ...workspace, snapshot: result.snapshot } : undefined;
    }

    case "syncSessionOrder": {
      const result = syncSessionOrderInWorkspace(
        workspace.snapshot,
        message.groupId,
        message.sessionIds,
      );
      return result.changed ? { ...workspace, snapshot: result.snapshot } : undefined;
    }

    case "syncGroupOrder": {
      const result = syncGroupOrderInWorkspace(workspace.snapshot, message.groupIds);
      return result.changed ? { ...workspace, snapshot: result.snapshot } : undefined;
    }

    case "focusSession": {
      const result = focusSessionInWorkspace(workspace.snapshot, message.sessionId);
      return result.changed ? { ...workspace, snapshot: result.snapshot } : undefined;
    }

    case "focusGroup": {
      const result = focusGroupInWorkspace(workspace.snapshot, message.groupId);
      return result.changed ? { ...workspace, snapshot: result.snapshot } : undefined;
    }

    case "setVisibleCount":
      return {
        ...workspace,
        snapshot: setVisibleCountInWorkspace(workspace.snapshot, message.visibleCount),
      };

    case "setViewMode":
      return {
        ...workspace,
        snapshot: setViewModeInWorkspace(workspace.snapshot, message.viewMode),
      };

    case "toggleActiveSessionsSortMode":
      return {
        ...workspace,
        options: {
          ...workspace.options,
          activeSessionsSortMode:
            workspace.options.activeSessionsSortMode === "manual" ? "lastActivity" : "manual",
        },
      };

    case "setActiveSessionsSortMode":
      return {
        ...workspace,
        options: {
          ...workspace.options,
          activeSessionsSortMode: message.sortMode,
        },
      };

    /*
     * CDXC:SidebarV2LogicalProjects 2026-07-29:
     * Only the project-grouping write is echoed back into story settings, and
     * deliberately so. That write's whole point is that the list RE-GROUPS, so
     * a story that only asserted the outgoing message would prove nothing about
     * the feature. Every other settings source stays one-way here: the sidebar
     * version switch in particular is asserted by several V1 stories that expect
     * the classic sidebar to stay mounted after the click.
     */
    case "updateSettingsPatch":
      return message.source === "sidebar:projectGrouping" && workspace.options.settings
        ? {
            ...workspace,
            options: {
              ...workspace.options,
              settings: normalizeghostexSettings({
                ...workspace.options.settings,
                ...message.patch,
              }),
            },
          }
        : undefined;

    case "cycleSessionPersistenceProvider":
      return workspace.options.settings
        ? {
            ...workspace,
            options: {
              ...workspace.options,
              settings: {
                ...workspace.options.settings,
                sessionPersistenceProvider:
                  workspace.options.settings.sessionPersistenceProvider === "off"
                    ? "tmux"
                    : workspace.options.settings.sessionPersistenceProvider === "tmux"
                      ? "zmx"
                      : workspace.options.settings.sessionPersistenceProvider === "zmx"
                        ? "zellij"
                        : "off",
              },
            },
          }
        : undefined;

    case "toggleFullscreenSession":
      return {
        ...workspace,
        snapshot: toggleFullscreenSessionInWorkspace(workspace.snapshot),
      };

    case "saveScratchPad":
      return {
        ...workspace,
        options: {
          ...workspace.options,
          scratchPadContent: message.content,
        },
      };

    case "savePinnedPrompt": {
      const now = new Date().toISOString();
      const promptId = message.promptId ?? `story-prompt-${workspace.pinnedPrompts.length}`;
      const existingPrompt = workspace.pinnedPrompts.find((prompt) => prompt.promptId === promptId);
      const nextPrompt = {
        content: message.content,
        createdAt: existingPrompt?.createdAt ?? now,
        promptId,
        title: message.title.trim(),
        updatedAt: now,
      };
      const nextPinnedPrompts = existingPrompt
        ? workspace.pinnedPrompts.map((prompt) =>
            prompt.promptId === promptId ? nextPrompt : prompt,
          )
        : [nextPrompt, ...workspace.pinnedPrompts];

      return {
        ...workspace,
        pinnedPrompts: nextPinnedPrompts,
      };
    }

    case "deletePreviousSession":
      return {
        ...workspace,
        previousSessions: workspace.previousSessions.filter(
          (session) => session.historyId !== message.historyId,
        ),
      };

    case "restorePreviousSession":
      return {
        ...workspace,
        previousSessions: workspace.previousSessions.filter(
          (session) => session.historyId !== message.historyId,
        ),
      };

    case "saveSidebarCommand": {
      const nextCommandId =
        message.commandId ?? `custom-story-${workspace.options.commands.length}`;
      const nextCommands = [...workspace.options.commands];
      const existingIndex = nextCommands.findIndex(
        (command) => command.commandId === nextCommandId,
      );
      const nextCommand = {
        actionType: message.actionType,
        closeTerminalOnExit:
          message.actionType === "terminal" ? message.closeTerminalOnExit : false,
        command: message.actionType === "terminal" ? message.command : undefined,
        commandId: nextCommandId,
        icon: message.icon,
        isDefault: existingIndex >= 0 ? nextCommands[existingIndex]?.isDefault === true : false,
        name: message.name,
        playCompletionSound:
          message.actionType === "terminal" ? message.playCompletionSound : false,
        showOnProjectRow: message.showOnProjectRow,
        url: message.actionType === "browser" ? message.url : undefined,
      };

      if (existingIndex >= 0) {
        nextCommands[existingIndex] = nextCommand;
      } else {
        nextCommands.push(nextCommand);
      }

      return {
        ...workspace,
        options: {
          ...workspace.options,
          commands: nextCommands,
        },
      };
    }

    case "saveSidebarAgent": {
      const nextAgentId =
        message.agentId ?? `custom-agent-story-${workspace.options.agents.length}`;
      const nextAgents = [...workspace.options.agents];
      const existingIndex = nextAgents.findIndex((agent) => agent.agentId === nextAgentId);
      const nextAgent = {
        agentId: nextAgentId,
        command: message.command,
        hidden: false,
        icon: message.icon ?? (existingIndex >= 0 ? nextAgents[existingIndex]?.icon : undefined),
        isDefault: existingIndex >= 0 ? nextAgents[existingIndex]?.isDefault === true : false,
        name: message.name,
      };

      if (existingIndex >= 0) {
        nextAgents[existingIndex] = nextAgent;
      } else {
        nextAgents.push(nextAgent);
      }

      return {
        ...workspace,
        options: {
          ...workspace.options,
          agents: nextAgents,
        },
      };
    }

    case "deleteSidebarCommand":
      return {
        ...workspace,
        options: {
          ...workspace.options,
          commands: workspace.options.commands.filter(
            (command) => command.commandId !== message.commandId,
          ),
        },
      };

    case "syncSidebarCommandOrder": {
      const commandById = new Map(
        workspace.options.commands.map((command) => [command.commandId, command] as const),
      );
      const nextCommands = message.commandIds
        .map((commandId) => commandById.get(commandId))
        .filter((command): command is SidebarCommandButton => command !== undefined);

      for (const command of workspace.options.commands) {
        if (!nextCommands.some((candidate) => candidate.commandId === command.commandId)) {
          nextCommands.push(command);
        }
      }

      return {
        ...workspace,
        options: {
          ...workspace.options,
          commands: nextCommands,
        },
      };
    }

    case "deleteSidebarAgent":
      return {
        ...workspace,
        options: {
          ...workspace.options,
          agents: workspace.options.agents.map((agent) =>
            agent.agentId === message.agentId && agent.isDefault
              ? {
                  ...agent,
                  hidden: true,
                }
              : agent,
          ),
        },
      };

    case "syncSidebarAgentOrder": {
      const agentById = new Map(
        workspace.options.agents.map((agent) => [agent.agentId, agent] as const),
      );
      const nextAgents = message.agentIds
        .map((agentId) => agentById.get(agentId))
        .filter((agent): agent is SidebarAgentButton => agent !== undefined);

      for (const agent of workspace.options.agents) {
        if (!nextAgents.some((candidate) => candidate.agentId === agent.agentId)) {
          nextAgents.push(agent);
        }
      }

      return {
        ...workspace,
        options: {
          ...workspace.options,
          agents: nextAgents,
        },
      };
    }

    case "createGroupFromSession": {
      const result = createGroupFromSessionInWorkspace(workspace.snapshot, message.sessionId);
      return result.changed ? { ...workspace, snapshot: result.snapshot } : undefined;
    }

    case "createGroup": {
      const result = createGroupInWorkspace(workspace.snapshot);
      return result.changed ? { ...workspace, snapshot: result.snapshot } : undefined;
    }

    default:
      return undefined;
  }
}

function createSessionGroupRecord(
  group: SidebarHydrateMessage["groups"][number],
): SessionGroupRecord {
  return {
    groupId: group.groupId,
    snapshot: {
      focusedSessionId: group.sessions.find((session) => session.isFocused)?.sessionId,
      fullscreenRestoreVisibleCount:
        group.isFocusModeActive && group.visibleCount === 1 ? group.layoutVisibleCount : undefined,
      sessions: group.sessions.map((session) => createSessionRecord(session)),
      viewMode: group.viewMode,
      visibleCount: group.visibleCount,
      visibleSessionIds: group.sessions
        .filter((session) => session.isVisible)
        .map((session) => session.sessionId),
    },
    title: group.title,
  };
}

function createSessionRecord(session: SidebarSessionItem): SessionRecord {
  const baseRecord = {
    alias: session.alias,
    column: session.column,
    createdAt: new Date(0).toISOString(),
    displayId: session.sessionNumber ?? parseDisplayId(session.shortcutLabel),
    row: session.row,
    sessionId: session.sessionId,
    slotIndex: parseShortcutIndex(session.shortcutLabel),
    title: session.primaryTitle ?? session.terminalTitle ?? session.alias,
  };

  if (session.kind === "browser") {
    return {
      ...baseRecord,
      browser: {
        url: session.detail ?? "",
      },
      kind: "browser",
    };
  }

  if (session.agentIcon === "t3") {
    return {
      ...baseRecord,
      kind: "t3",
      t3: {
        projectId: `story-project-${session.sessionId}`,
        serverOrigin: "http://127.0.0.1:3773",
        threadId: "pending-thread",
        workspaceRoot: "/tmp/story-workspace",
      },
    };
  }

  return {
    ...baseRecord,
    kind: "terminal",
    terminalEngine: "ghostty-native",
  };
}

function parseShortcutIndex(shortcutLabel: string): number {
  const matchedIndex = shortcutLabel.match(/(\d+)$/)?.[1];
  const index = matchedIndex ? Number.parseInt(matchedIndex, 10) : Number.NaN;
  return Number.isFinite(index) && index > 0 ? index : 1;
}

function parseDisplayId(shortcutLabel: string): string {
  const matchedIndex = shortcutLabel.match(/(\d+)$/)?.[1];
  return matchedIndex ? matchedIndex.padStart(2, "0") : "00";
}

function getNextGroupNumber(groups: readonly SidebarHydrateMessage["groups"][number][]): number {
  return (
    Math.max(
      1,
      ...groups.map((group) => {
        const matchedNumber = group.groupId.match(/group-(\d+)$/)?.[1];
        return matchedNumber ? Number.parseInt(matchedNumber, 10) : 1;
      }),
    ) + 1
  );
}

function getNextSessionNumber(groups: readonly SidebarHydrateMessage["groups"][number][]): number {
  return (
    Math.max(
      0,
      ...groups.flatMap((group) =>
        group.sessions.map((session) => {
          const matchedNumber = session.sessionId.match(/session-(\d+)$/)?.[1];
          return matchedNumber ? Number.parseInt(matchedNumber, 10) : 0;
        }),
      ),
    ) + 1
  );
}
