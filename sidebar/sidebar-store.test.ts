import { beforeEach, describe, expect, test } from "vitest";
import type {
  SidebarHydrateMessage,
  SidebarSessionGroup,
  SidebarSessionItem,
  SidebarSessionStateMessage,
} from "../shared/session-grid-contract";
import {
  createInitialSidebarStoreDataState,
  resetSidebarStore,
  useSidebarStore,
} from "./sidebar-store";

describe("sidebar store", () => {
  beforeEach(() => {
    resetSidebarStore();
  });

  test("should track the latest order sync result for the matching sidebar section", () => {
    useSidebarStore.getState().applyOrderSyncResultMessage({
      itemIds: ["claude", "codex"],
      kind: "agent",
      requestId: "req-agent",
      status: "success",
      type: "sidebarOrderSyncResult",
    });

    let state = useSidebarStore.getState();
    expect(state.latestAgentOrderSyncResult).toEqual({
      itemIds: ["claude", "codex"],
      kind: "agent",
      requestId: "req-agent",
      status: "success",
      type: "sidebarOrderSyncResult",
    });
    expect(state.latestCommandOrderSyncResult).toBeUndefined();

    useSidebarStore.getState().applyOrderSyncResultMessage({
      itemIds: ["test", "build"],
      kind: "command",
      requestId: "req-command",
      status: "error",
      type: "sidebarOrderSyncResult",
    });

    state = useSidebarStore.getState();
    expect(state.latestAgentOrderSyncResult).toBeUndefined();
    expect(state.latestCommandOrderSyncResult).toEqual({
      itemIds: ["test", "build"],
      kind: "command",
      requestId: "req-command",
      status: "error",
      type: "sidebarOrderSyncResult",
    });
  });

  test("should track and clear sidebar command run feedback", () => {
    useSidebarStore.getState().applyCommandRunStateMessage({
      commandId: "build",
      runId: "run-build",
      state: "running",
      type: "sidebarCommandRunStateChanged",
    });

    let state = useSidebarStore.getState();
    expect(state.commandRunStates.build).toEqual({
      activeRunIds: ["run-build"],
      status: "running",
    });

    useSidebarStore.getState().applyCommandRunStateMessage({
      commandId: "build",
      runId: "run-build",
      state: "success",
      type: "sidebarCommandRunStateChanged",
    });

    state = useSidebarStore.getState();
    expect(state.commandRunStates.build).toEqual({
      activeRunIds: [],
      status: "success",
    });

    useSidebarStore.getState().clearCommandRunState("build");

    expect(useSidebarStore.getState().commandRunStates.build).toBeUndefined();
  });

  test("should clear sidebar command run feedback from extension messages", () => {
    useSidebarStore.getState().applyCommandRunStateMessage({
      commandId: "build",
      runId: "run-build",
      state: "running",
      type: "sidebarCommandRunStateChanged",
    });

    useSidebarStore.getState().applyCommandRunStateClearedMessage({
      commandId: "build",
      type: "sidebarCommandRunStateCleared",
    });

    expect(useSidebarStore.getState().commandRunStates.build).toBeUndefined();
  });

  test("should consume focused-session scroll suppression once while it is active", () => {
    useSidebarStore.getState().suppressNextFocusedSessionScroll("sessionClose", 1_000);

    expect(useSidebarStore.getState().focusedSessionScrollSuppression).toEqual({
      expiresAtMs: 6_000,
      reason: "sessionClose",
    });
    expect(useSidebarStore.getState().consumeFocusedSessionScrollSuppression(5_999)).toEqual({
      expiresAtMs: 6_000,
      reason: "sessionClose",
    });
    expect(useSidebarStore.getState().focusedSessionScrollSuppression).toBeUndefined();
    expect(useSidebarStore.getState().consumeFocusedSessionScrollSuppression(5_999)).toBeUndefined();
  });

  test("should clear expired focused-session scroll suppression without consuming it", () => {
    useSidebarStore.getState().suppressNextFocusedSessionScroll("sessionClose", 1_000);

    expect(useSidebarStore.getState().consumeFocusedSessionScrollSuppression(6_001)).toBeUndefined();
    expect(useSidebarStore.getState().focusedSessionScrollSuppression).toBeUndefined();
  });

  test("should preserve the synthetic chats group marker during hydration", () => {
    useSidebarStore.getState().applySidebarMessage(
      createHydrateMessage([
        {
          ...createGroup("combined-chats", [createSession("chat-session-1", "Chat")]),
          isChatCollection: true,
          title: "Chats",
        },
      ]),
    );

    expect(useSidebarStore.getState().groupsById["combined-chats"]?.isChatCollection).toBe(true);
  });

  test("should update only the targeted session record on sessionPresentationChanged", () => {
    useSidebarStore
      .getState()
      .applySidebarMessage(
        createHydrateMessage([
          createGroup("group-1", [
            createSession("session-1", "groups"),
            createSession("session-2", "notes"),
          ]),
          createGroup("group-2", [createSession("session-3", "logs")]),
        ]),
      );

    const before = useSidebarStore.getState();
    const previousGroupsById = before.groupsById;
    const previousSessionIdsByGroup = before.sessionIdsByGroup;
    const previousSession = before.sessionsById["session-1"];
    const previousSiblingSession = before.sessionsById["session-2"];

    useSidebarStore.getState().applySessionPresentationMessage({
      session: {
        ...previousSession,
        lifecycleState: "done",
        primaryTitle: "updated groups",
      },
      type: "sessionPresentationChanged",
    });

    const after = useSidebarStore.getState();
    expect(after.groupsById).toBe(previousGroupsById);
    expect(after.sessionIdsByGroup).toBe(previousSessionIdsByGroup);
    expect(after.sessionsById["session-1"]).not.toBe(previousSession);
    expect(after.sessionsById["session-1"]?.lifecycleState).toBe("done");
    expect(after.sessionsById["session-1"]?.primaryTitle).toBe("updated groups");
    expect(after.sessionsById["session-2"]).toBe(previousSiblingSession);
  });

  test("should patch changed groups and order without replacing unchanged groups", () => {
    useSidebarStore
      .getState()
      .applySidebarMessage(
        createHydrateMessage([
          createGroup("group-1", [
            createSession("session-1", "groups"),
            createSession("session-2", "notes"),
          ]),
          createGroup("group-2", [createSession("session-3", "logs")]),
        ]),
      );

    const before = useSidebarStore.getState();
    const unchangedGroup = before.groupsById["group-2"];
    const unchangedSession = before.sessionsById["session-3"];

    useSidebarStore.getState().applyGroupsChangedMessage({
      groupOrder: ["group-2", "group-1", "group-3"],
      groups: [
        createGroup("group-1", [
          createSession("session-1", "groups"),
          createSession("session-4", "scratch"),
        ]),
        createGroup("group-3", [createSession("session-5", "new project")]),
      ],
      removedSessionIds: ["session-2"],
      revision: 2,
      type: "sidebarGroupsChanged",
    });

    const after = useSidebarStore.getState();
    expect(after.revision).toBe(2);
    expect(after.groupOrder).toEqual(["group-2", "group-1", "group-3"]);
    expect(after.workspaceGroupIds).toEqual(["group-2", "group-1", "group-3"]);
    expect(after.groupsById["group-2"]).toBe(unchangedGroup);
    expect(after.sessionsById["session-3"]).toBe(unchangedSession);
    expect(after.sessionIdsByGroup["group-1"]).toEqual(["session-1", "session-4"]);
    expect(after.sessionsById["session-2"]).toBeUndefined();
    expect(after.sessionIdsByGroup["group-3"]).toEqual(["session-5"]);
  });

  test("should apply group removals from sidebar group patches", () => {
    useSidebarStore
      .getState()
      .applySidebarMessage(
        createHydrateMessage([
          createGroup("group-1", [createSession("session-1", "groups")]),
          createGroup("group-2", [createSession("session-2", "notes")]),
        ]),
      );

    useSidebarStore.getState().applyGroupsChangedMessage({
      groupOrder: ["group-1"],
      groups: [],
      removedGroupIds: ["group-2"],
      revision: 2,
      type: "sidebarGroupsChanged",
    });

    const after = useSidebarStore.getState();
    expect(after.groupOrder).toEqual(["group-1"]);
    expect(after.groupsById["group-2"]).toBeUndefined();
    expect(after.sessionIdsByGroup["group-2"]).toBeUndefined();
    expect(after.sessionsById["session-2"]).toBeUndefined();
  });

  test("should keep local overlays during partial group patches until the host confirms them", () => {
    useSidebarStore
      .getState()
      .applySidebarMessage(
        createHydrateMessage([
          createGroup("group-1", [
            createSession("session-1", "groups"),
            createSession("session-2", "notes"),
          ]),
          createGroup("group-2", [createSession("session-3", "logs")]),
        ]),
      );

    useSidebarStore.getState().hideSessionLocally("session-2");
    useSidebarStore.getState().setSessionSleepingLocally("session-3", true);

    useSidebarStore.getState().applyGroupsChangedMessage({
      groupOrder: ["group-1", "group-2"],
      groups: [
        createGroup("group-1", [
          createSession("session-1", "groups"),
          createSession("session-2", "notes"),
        ]),
        createGroup("group-2", [createSession("session-3", "logs")]),
      ],
      revision: 2,
      type: "sidebarGroupsChanged",
    });

    expect(useSidebarStore.getState().sessionsById["session-2"]).toBeUndefined();
    expect(useSidebarStore.getState().localHiddenSessionIds).toEqual({
      "session-2": true,
    });
    expect(useSidebarStore.getState().sessionsById["session-3"]?.isSleeping).toBe(true);
    expect(useSidebarStore.getState().localSessionSleepingOverrides).toEqual({
      "session-3": true,
    });

    useSidebarStore.getState().applyGroupsChangedMessage({
      groupOrder: ["group-1", "group-2"],
      groups: [
        createGroup("group-1", [createSession("session-1", "groups")]),
        createGroup("group-2", [
          {
            ...createSession("session-3", "logs"),
            isRunning: false,
            isSleeping: true,
            lifecycleState: "sleeping",
          },
        ]),
      ],
      removedSessionIds: ["session-2"],
      revision: 3,
      type: "sidebarGroupsChanged",
    });

    expect(useSidebarStore.getState().localHiddenSessionIds).toEqual({});
    expect(useSidebarStore.getState().localSessionSleepingOverrides).toEqual({});
    expect(useSidebarStore.getState().sessionsById["session-3"]?.isSleeping).toBe(true);
  });

  test("should keep locally closed sessions hidden until the host snapshot drops them", () => {
    useSidebarStore
      .getState()
      .applySidebarMessage(
        createHydrateMessage([
          createGroup("group-1", [
            createSession("session-1", "groups"),
            createSession("session-2", "notes"),
          ]),
        ]),
      );

    useSidebarStore.getState().hideSessionLocally("session-2");
    expect(useSidebarStore.getState().sessionsById["session-2"]).toBeUndefined();
    expect(useSidebarStore.getState().sessionIdsByGroup["group-1"]).toEqual(["session-1"]);

    useSidebarStore
      .getState()
      .applySidebarMessage(
        createHydrateMessage(
          [
            createGroup("group-1", [
              createSession("session-1", "groups"),
              createSession("session-2", "notes"),
            ]),
          ],
          { revision: 2 },
        ),
      );

    expect(useSidebarStore.getState().sessionsById["session-2"]).toBeUndefined();
    expect(useSidebarStore.getState().sessionIdsByGroup["group-1"]).toEqual(["session-1"]);

    useSidebarStore
      .getState()
      .applySidebarMessage(
        createHydrateMessage(
          [createGroup("group-1", [createSession("session-1", "groups")])],
          { revision: 3 },
        ),
      );

    expect(useSidebarStore.getState().localHiddenSessionIds).toEqual({});
  });

  test("should keep local sleep state until the host snapshot confirms it", () => {
    useSidebarStore
      .getState()
      .applySidebarMessage(
        createHydrateMessage([
          createGroup("group-1", [createSession("session-1", "groups")]),
        ]),
      );

    useSidebarStore.getState().setSessionSleepingLocally("session-1", true);
    expect(useSidebarStore.getState().sessionsById["session-1"]?.isSleeping).toBe(true);
    expect(useSidebarStore.getState().sessionsById["session-1"]?.lifecycleState).toBe("sleeping");

    useSidebarStore
      .getState()
      .applySidebarMessage(
        createHydrateMessage(
          [createGroup("group-1", [createSession("session-1", "groups")])],
          { revision: 2 },
        ),
      );

    expect(useSidebarStore.getState().sessionsById["session-1"]?.isSleeping).toBe(true);
    expect(useSidebarStore.getState().localSessionSleepingOverrides).toEqual({
      "session-1": true,
    });

    useSidebarStore
      .getState()
      .applySidebarMessage(
        createHydrateMessage(
          [
            createGroup("group-1", [
              {
                ...createSession("session-1", "groups"),
                isRunning: false,
                isSleeping: true,
                lifecycleState: "sleeping",
              },
            ]),
          ],
          { revision: 3 },
        ),
      );

    expect(useSidebarStore.getState().localSessionSleepingOverrides).toEqual({});
  });

  test("should hide multiple sessions locally with one store update", () => {
    useSidebarStore
      .getState()
      .applySidebarMessage(
        createHydrateMessage([
          createGroup("group-1", [
            createSession("session-1", "groups"),
            createSession("session-2", "notes"),
            createSession("session-3", "logs"),
          ]),
        ]),
      );

    useSidebarStore.getState().hideSessionsLocally(["session-2", "session-3", "session-2"]);

    expect(useSidebarStore.getState().sessionsById["session-2"]).toBeUndefined();
    expect(useSidebarStore.getState().sessionsById["session-3"]).toBeUndefined();
    expect(useSidebarStore.getState().sessionIdsByGroup["group-1"]).toEqual(["session-1"]);
    expect(useSidebarStore.getState().localHiddenSessionIds).toEqual({
      "session-2": true,
      "session-3": true,
    });
  });

  test("should preserve unchanged HUD slice references across session snapshots", () => {
    useSidebarStore
      .getState()
      .applySidebarMessage(
        createHydrateMessage([
          createGroup("group-1", [
            createSession("session-1", "groups"),
            createSession("session-2", "notes"),
          ]),
        ]),
      );

    const before = useSidebarStore.getState();
    const sessionState: SidebarSessionStateMessage = {
      groups: [
        createGroup("group-1", [
          {
            ...createSession("session-1", "groups"),
            activity: "attention",
            activityLabel: "Needs attention",
          },
          createSession("session-2", "notes"),
        ]),
      ],
      hud: {
        ...before.hud,
        agents: before.hud.agents.map((agent) => ({ ...agent })),
        commands: before.hud.commands.map((command) => ({ ...command })),
        git: {
          ...before.hud.git,
          files: before.hud.git.files.map((file) => ({ ...file })),
          pr: before.hud.git.pr ? { ...before.hud.git.pr } : null,
        },
        pendingAgentIds: [...before.hud.pendingAgentIds],
        projectSettingsProjects: before.hud.projectSettingsProjects?.map((project) => ({
          ...project,
        })),
        recentProjects: before.hud.recentProjects.map((project) => ({ ...project })),
        settings: before.hud.settings ? { ...before.hud.settings } : undefined,
        visibleSlotLabels: [...before.hud.visibleSlotLabels],
      },
      pinnedPrompts: [],
      previousSessions: [],
      revision: 2,
      scratchPadContent: "",
      type: "sessionState",
    };

    /**
     * CDXC:AppModals 2026-05-29-19:44:
     * Attention/activity snapshots may rebuild HUD objects without changing
     * agents or settings. Open modals subscribe to those slices, so preserving
     * unchanged references keeps unrelated session status updates from
     * reinitializing modal drafts.
     */
    useSidebarStore.getState().applySidebarMessage(sessionState);

    const after = useSidebarStore.getState();
    expect(after.sessionsById["session-1"]?.activity).toBe("attention");
    expect(after.hud.agents).toBe(before.hud.agents);
    expect(after.hud.commands).toBe(before.hud.commands);
    expect(after.hud.git).toBe(before.hud.git);
    expect(after.hud.pendingAgentIds).toBe(before.hud.pendingAgentIds);
    expect(after.hud.projectSettingsProjects).toBe(before.hud.projectSettingsProjects);
    expect(after.hud.recentProjects).toBe(before.hud.recentProjects);
    expect(after.hud.settings).toBe(before.hud.settings);
    expect(after.hud.visibleSlotLabels).toBe(before.hud.visibleSlotLabels);
  });

  /**
   * CDXC:SidebarV2Lifecycle 2026-07-29:
   * Settling or snoozing a quiet session changes nothing else on the row, and
   * hydrate has no revision escape hatch, so the session equality check is the
   * only thing that can hand React a new object for it. Without these fields the
   * V2 settled/snoozed shelves keep rendering the pre-settle row forever.
   */
  test("should replace the session record when only settle state changes", () => {
    useSidebarStore.getState().applySidebarMessage(
      createHydrateMessage([
        createGroup("group-1", [
          createSession("session-1", "groups"),
          createSession("session-2", "notes"),
        ]),
      ]),
    );

    const before = useSidebarStore.getState();
    const previousSession = before.sessionsById["session-1"];
    const previousSiblingSession = before.sessionsById["session-2"];

    useSidebarStore.getState().applySidebarMessage(
      createHydrateMessage(
        [
          createGroup("group-1", [
            {
              ...createSession("session-1", "groups"),
              settledAt: "2026-07-29T10:00:00.000Z",
              settledOverride: "settled",
            },
            createSession("session-2", "notes"),
          ]),
        ],
        { revision: 2 },
      ),
    );

    const afterSettle = useSidebarStore.getState();
    expect(afterSettle.sessionsById["session-1"]).not.toBe(previousSession);
    expect(afterSettle.sessionsById["session-1"]?.settledAt).toBe("2026-07-29T10:00:00.000Z");
    expect(afterSettle.sessionsById["session-1"]?.settledOverride).toBe("settled");
    expect(afterSettle.sessionsById["session-2"]).toBe(previousSiblingSession);

    const settledSession = afterSettle.sessionsById["session-1"];

    useSidebarStore.getState().applySidebarMessage(
      createHydrateMessage(
        [
          createGroup("group-1", [
            {
              ...createSession("session-1", "groups"),
              settledAt: "2026-07-29T10:00:00.000Z",
              settledOverride: "settled",
            },
            createSession("session-2", "notes"),
          ]),
        ],
        { revision: 3 },
      ),
    );

    expect(useSidebarStore.getState().sessionsById["session-1"]).toBe(settledSession);

    useSidebarStore.getState().applySidebarMessage(
      createHydrateMessage(
        [
          createGroup("group-1", [
            {
              ...createSession("session-1", "groups"),
              settledOverride: "active",
            },
            createSession("session-2", "notes"),
          ]),
        ],
        { revision: 4 },
      ),
    );

    const afterUnsettle = useSidebarStore.getState();
    expect(afterUnsettle.sessionsById["session-1"]).not.toBe(settledSession);
    expect(afterUnsettle.sessionsById["session-1"]?.settledAt).toBeUndefined();
    expect(afterUnsettle.sessionsById["session-1"]?.settledOverride).toBe("active");
  });

  test("should replace the session record when only snooze state changes", () => {
    useSidebarStore.getState().applySidebarMessage(
      createHydrateMessage([
        createGroup("group-1", [
          createSession("session-1", "groups"),
          createSession("session-2", "notes"),
        ]),
      ]),
    );

    const before = useSidebarStore.getState();
    const previousSession = before.sessionsById["session-1"];
    const previousSiblingSession = before.sessionsById["session-2"];

    useSidebarStore.getState().applySidebarMessage(
      createHydrateMessage(
        [
          createGroup("group-1", [
            {
              ...createSession("session-1", "groups"),
              snoozedAt: "2026-07-29T10:00:00.000Z",
              snoozedUntil: "2026-07-29T11:00:00.000Z",
            },
            createSession("session-2", "notes"),
          ]),
        ],
        { revision: 2 },
      ),
    );

    const afterSnooze = useSidebarStore.getState();
    expect(afterSnooze.sessionsById["session-1"]).not.toBe(previousSession);
    expect(afterSnooze.sessionsById["session-1"]?.snoozedAt).toBe("2026-07-29T10:00:00.000Z");
    expect(afterSnooze.sessionsById["session-1"]?.snoozedUntil).toBe("2026-07-29T11:00:00.000Z");
    expect(afterSnooze.sessionsById["session-2"]).toBe(previousSiblingSession);

    const snoozedSession = afterSnooze.sessionsById["session-1"];

    useSidebarStore.getState().applySidebarMessage(
      createHydrateMessage(
        [
          createGroup("group-1", [
            {
              ...createSession("session-1", "groups"),
              snoozedAt: "2026-07-29T10:00:00.000Z",
              snoozedUntil: "2026-07-29T11:00:00.000Z",
            },
            createSession("session-2", "notes"),
          ]),
        ],
        { revision: 3 },
      ),
    );

    expect(useSidebarStore.getState().sessionsById["session-1"]).toBe(snoozedSession);

    useSidebarStore.getState().applySidebarMessage(
      createHydrateMessage(
        [
          createGroup("group-1", [
            {
              ...createSession("session-1", "groups"),
              snoozedAt: "2026-07-29T10:00:00.000Z",
              snoozedUntil: "2026-07-30T09:00:00.000Z",
            },
            createSession("session-2", "notes"),
          ]),
        ],
        { revision: 4 },
      ),
    );

    const afterReschedule = useSidebarStore.getState();
    expect(afterReschedule.sessionsById["session-1"]).not.toBe(snoozedSession);
    expect(afterReschedule.sessionsById["session-1"]?.snoozedUntil).toBe(
      "2026-07-30T09:00:00.000Z",
    );
  });

  /*
  CDXC:SettingsModalBlankUnnormalizedHydrate 2026-07-29:
  The GPUI hydrate carries the shared settings file verbatim, so a file only ever
  written by titlebar code paths arrives without whole nested schema objects. The
  app-modal Settings render read `settings.workspaceOpenTargetAvailability
  .availableTargetIds` directly and threw, blanking the window permanently.
  */
  test("should normalize a partial hydrated settings object into the full schema shape", () => {
    const partialSettingsFromDisk = {
      gpuiTitlebarOpenTargetByProject: { P70t8: "finder" },
      gpuiTitlebarTipsReadIds: ["command-palette-all-actions"],
    };
    const message = createHydrateMessage([]);

    useSidebarStore.getState().applySidebarMessage({
      ...message,
      hud: {
        ...message.hud,
        settings: partialSettingsFromDisk as unknown as typeof message.hud.settings,
      },
    });

    const settings = useSidebarStore.getState().hud.settings;
    expect(settings).toBeDefined();

    // The exact read that threw: a missing nested object must arrive complete.
    expect(settings?.workspaceOpenTargetAvailability.availableTargetIds).toBeInstanceOf(Array);
    expect(settings?.hotkeys).toBeDefined();
    expect(settings?.diagnosticLogging).toBeDefined();

    // Values the user already had must survive normalization.
    expect(settings?.gpuiTitlebarOpenTargetByProject).toEqual({ P70t8: "finder" });

    // Keys the schema does not model must survive so a later full save cannot
    // silently drop them.
    expect(
      (settings as unknown as { gpuiTitlebarTipsReadIds?: string[] }).gpuiTitlebarTipsReadIds,
    ).toEqual(["command-palette-all-actions"]);
  });
});

function createHydrateMessage(
  groups: SidebarSessionGroup[],
  options?: {
    revision?: number;
  },
): SidebarHydrateMessage {
  const initialHud = createInitialSidebarStoreDataState().hud;

  return {
    groups,
    hud: {
      ...initialHud,
    },
    pinnedPrompts: [],
    previousSessions: [],
    revision: options?.revision ?? 1,
    scratchPadContent: "",
    type: "hydrate",
  };
}

function createGroup(groupId: string, sessions: SidebarSessionItem[]): SidebarSessionGroup {
  return {
    groupId,
    isActive: groupId === "group-1",
    isFocusModeActive: false,
    layoutVisibleCount: 1,
    sessions,
    title: groupId === "group-1" ? "Main" : "Group 2",
    viewMode: "grid",
    visibleCount: 1,
  };
}

function createSession(sessionId: string, primaryTitle: string): SidebarSessionItem {
  return {
    activity: sessionId === "session-1" ? "working" : "idle",
    activityLabel: sessionId === "session-1" ? "Codex active" : undefined,
    alias: primaryTitle,
    column: 0,
    isFocused: sessionId === "session-1",
    lifecycleState: sessionId === "session-1" ? "running" : "done",
    isRunning: sessionId === "session-1",
    isVisible: sessionId === "session-1",
    primaryTitle,
    row: 0,
    sessionId,
    shortcutLabel: "⌘⌥1",
  };
}
