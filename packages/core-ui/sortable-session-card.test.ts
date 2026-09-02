import { describe, expect, test } from 'vitest';
import type { SidebarSessionItem } from '../shared/session-grid-contract';
import {
  canSleepSidebarSession,
  createSleepBelowDebugDetails,
  getSidebarSessionContextMenuEligibility,
  getSessionCardAccessibleLabel,
  getSessionTagSubmenuSections,
  resolveSessionCardSessionIdsBelow,
  runSidebarBulkContextMenuActionInBackground,
  shouldFocusSidebarSessionOnPointerDown,
  shouldRenameSidebarSessionOnDoubleClick,
} from './sortable-session-card';

describe('getSessionCardAccessibleLabel', () => {
  test('keeps session row labels independent from focused styling', () => {
    expect(
      getSessionCardAccessibleLabel({
        isFocused: false,
        title: 'Fix sidebar session rows',
      })
    ).toBe('Fix sidebar session rows');

    expect(
      getSessionCardAccessibleLabel({
        isFocused: true,
        title: 'Fix sidebar session rows',
      })
    ).toBe('Fix sidebar session rows, current session');
  });

  test('falls back to a stable label when the title is empty', () => {
    expect(
      getSessionCardAccessibleLabel({
        isFocused: false,
        title: ' ',
      })
    ).toBe('Session');
  });
});

describe('getSessionTagSubmenuSections', () => {
  test('uses default enabled and visible tag rows for the Tag as menu', () => {
    /*
     * CDXC:SessionTagFilters 2026-06-15-22:23:
     * Session context-menu tagging should mirror the Settings-controlled
     * sidebar tag list so Reset to Default removes default-off tags from both
     * the filter menu and the Tag as assignment submenu.
     */
    expect(
      getSessionTagSubmenuSections({}).flatMap((section) => section.options.map((option) => option.value))
    ).toEqual(['favorite', 'in-progress', 'testing', 'blocked', 'on-hold', 'done', 'research', 'design']);
  });

  test('keeps the current hidden tag visible so it can be removed', () => {
    expect(
      getSessionTagSubmenuSections({ currentSessionTag: 'bug' }).flatMap((section) =>
        section.options.map((option) => option.value)
      )
    ).toEqual(['favorite', 'in-progress', 'testing', 'blocked', 'on-hold', 'done', 'research', 'bug', 'design']);
  });
});

describe('runSidebarBulkContextMenuActionInBackground', () => {
  test('defers each bulk target onto the scheduler', () => {
    const queuedOperations: Array<() => void> = [];
    const processedSessionIds: string[] = [];

    runSidebarBulkContextMenuActionInBackground(
      ['session-2', 'session-3'],
      (sessionId) => {
        processedSessionIds.push(sessionId);
      },
      (operation) => {
        queuedOperations.push(operation);
      }
    );

    expect(processedSessionIds).toEqual([]);
    expect(queuedOperations).toHaveLength(1);

    queuedOperations.shift()?.();
    expect(processedSessionIds).toEqual(['session-2']);
    expect(queuedOperations).toHaveLength(1);

    queuedOperations.shift()?.();
    expect(processedSessionIds).toEqual(['session-2', 'session-3']);
    expect(queuedOperations).toHaveLength(0);
  });

  test('uses the clicked menu target list even if the caller mutates its array later', () => {
    const queuedOperations: Array<() => void> = [];
    const processedSessionIds: string[] = [];
    const sessionIds = ['session-2'];

    runSidebarBulkContextMenuActionInBackground(
      sessionIds,
      (sessionId) => {
        processedSessionIds.push(sessionId);
      },
      (operation) => {
        queuedOperations.push(operation);
      }
    );
    sessionIds.push('session-3');

    queuedOperations.shift()?.();

    expect(processedSessionIds).toEqual(['session-2']);
    expect(queuedOperations).toHaveLength(0);
  });
});

describe('resolveSessionCardSessionIdsBelow', () => {
  test('materializes below actions from the shared group-provided session list', () => {
    expect(
      resolveSessionCardSessionIdsBelow({
        sessionIdsBelowSource: ['same-project-1', 'same-project-2', 'same-project-3'],
        sessionIdsBelowStartIndex: 1,
      })
    ).toEqual(['same-project-2', 'same-project-3']);
  });

  test('returns no below actions when the row has no later visible sessions', () => {
    expect(
      resolveSessionCardSessionIdsBelow({
        sessionIdsBelowSource: ['same-project-1'],
        sessionIdsBelowStartIndex: 1,
      })
    ).toEqual([]);
  });

  test('preserves remote scoped ids for native bulk below handling', () => {
    /*
     * CDXC:RemoteContextMenu 2026-06-30-15:22:
     * Sleep below and Close below must pass scoped remote session ids through
     * unchanged. Native owns splitting local ids from remote gxserver ids, so
     * the sidebar should not normalize or strip the machine/project namespace.
     */
    expect(
      resolveSessionCardSessionIdsBelow({
        sessionIdsBelowSource: [
          'local-session-1',
          'remote:machine-1:session:project-1:remote-session-2',
          'local-session-3',
        ],
        sessionIdsBelowStartIndex: 1,
      })
    ).toEqual(['remote:machine-1:session:project-1:remote-session-2', 'local-session-3']);
  });
});

describe('shouldFocusSidebarSessionOnPointerDown', () => {
  const baseInput = {
    altKey: false,
    button: 0,
    ctrlKey: false,
    isPrimary: true,
    isProjectSessionListMoreRow: false,
    isProjectSessionListOverflowRow: false,
    metaKey: false,
    renameSessionOnDoubleClick: false,
    shiftKey: false,
  };

  test('uses immediate pointer-down focus when double-click rename is disabled', () => {
    expect(shouldFocusSidebarSessionOnPointerDown(baseInput)).toBe(true);
  });

  test('waits for click semantics when the session row can start a drag', () => {
    /*
     * CDXC:PinnedSessions 2026-07-01-00:47:
     * Last Active sorting keeps pinned project sessions draggable. Those rows
     * must not focus on pointer-down because native focus can take the WebKit
     * pointer stream before dnd-kit activates the delayed drag.
     */
    expect(
      shouldFocusSidebarSessionOnPointerDown({
        ...baseInput,
        isSessionDragActivationEnabled: true,
      })
    ).toBe(false);
  });

  test('waits for click semantics when double-click rename is enabled', () => {
    expect(
      shouldFocusSidebarSessionOnPointerDown({
        ...baseInput,
        renameSessionOnDoubleClick: true,
      })
    ).toBe(false);
  });

  test('keeps modified and non-primary pointer actions out of immediate focus', () => {
    expect(shouldFocusSidebarSessionOnPointerDown({ ...baseInput, metaKey: true })).toBe(false);
    expect(shouldFocusSidebarSessionOnPointerDown({ ...baseInput, ctrlKey: true })).toBe(false);
    expect(shouldFocusSidebarSessionOnPointerDown({ ...baseInput, altKey: true })).toBe(false);
    expect(shouldFocusSidebarSessionOnPointerDown({ ...baseInput, shiftKey: true })).toBe(false);
    expect(shouldFocusSidebarSessionOnPointerDown({ ...baseInput, button: 1 })).toBe(false);
    expect(shouldFocusSidebarSessionOnPointerDown({ ...baseInput, isPrimary: false })).toBe(false);
    expect(
      shouldFocusSidebarSessionOnPointerDown({
        ...baseInput,
        isInteractiveDescendant: true,
      })
    ).toBe(false);
  });

  test('does not immediate-focus placeholder rows', () => {
    expect(
      shouldFocusSidebarSessionOnPointerDown({
        ...baseInput,
        isProjectSessionListMoreRow: true,
      })
    ).toBe(false);
    expect(
      shouldFocusSidebarSessionOnPointerDown({
        ...baseInput,
        isProjectSessionListOverflowRow: true,
      })
    ).toBe(false);
  });
});

describe('shouldRenameSidebarSessionOnDoubleClick', () => {
  const baseInput = {
    isBrowserSession: false,
    isProjectSessionListMoreRow: false,
    isProjectSessionListOverflowRow: false,
    renameSessionOnDoubleClick: true,
  };

  test('reserves session-card double-click for explicit rename', () => {
    expect(shouldRenameSidebarSessionOnDoubleClick(baseInput)).toBe(true);
    expect(
      shouldRenameSidebarSessionOnDoubleClick({
        ...baseInput,
        renameSessionOnDoubleClick: false,
      })
    ).toBe(false);
  });

  test('does not rename browser or placeholder rows on double-click', () => {
    expect(
      shouldRenameSidebarSessionOnDoubleClick({
        ...baseInput,
        isBrowserSession: true,
      })
    ).toBe(false);
    expect(
      shouldRenameSidebarSessionOnDoubleClick({
        ...baseInput,
        isProjectSessionListMoreRow: true,
      })
    ).toBe(false);
    expect(
      shouldRenameSidebarSessionOnDoubleClick({
        ...baseInput,
        isProjectSessionListOverflowRow: true,
      })
    ).toBe(false);
  });
});

function createContextMenuSession(overrides: Partial<SidebarSessionItem> = {}): SidebarSessionItem {
  return {
    activity: 'idle',
    alias: 'Session 1',
    column: 0,
    isFocused: false,
    isLive: true,
    isRunning: true,
    isSleeping: false,
    isVisible: true,
    lifecycleState: 'running',
    nativePaneState: 'mounted',
    providerSessionState: 'exists',
    row: 0,
    sessionId: 'session-1',
    sessionKind: 'terminal',
    shortcutLabel: '1',
    ...overrides,
  };
}

describe('getSidebarSessionContextMenuEligibility', () => {
  test('keeps shared remote terminal affordances visible without host-local capability guesses', () => {
    /*
     * CDXC:RemoteSessionMenus 2026-06-30-15:22:
     * Remote terminal rows should retain the shared gxserver-backed context-menu
     * affordances while Pop Out Pane, Delayed Send, and Close After Done remain
     * hidden until the row explicitly reports support for those host-local flows.
     */
    expect(
      getSidebarSessionContextMenuEligibility({
        isProjectSessionListMoreRow: false,
        isRemoteSession: true,
        session: createContextMenuSession({
          sessionPersistenceName: 'remote-session',
          sessionPersistenceProvider: 'zmx',
        }),
        showSessionCommandCopyActions: true,
        showSessionDetailsCopyAction: true,
      })
    ).toMatchObject({
      canCloseAfterDone: false,
      canCopyAttachCommand: true,
      canCopyResumeCommand: false,
      canCopySessionDetails: true,
      canDelayedSend: false,
      canForkSession: false,
      canFullReloadSession: true,
      canPinSession: true,
      canPopOutPane: false,
      canRenameSession: true,
      canSleepSession: true,
      canTagSession: true,
    });
  });

  test('uses local metadata gates for remote agent resume, attach, and fork', () => {
    const eligibility = getSidebarSessionContextMenuEligibility({
      isProjectSessionListMoreRow: false,
      isRemoteSession: true,
      session: createContextMenuSession({
        agentIcon: 'codex',
        sessionPersistenceName: 'remote-codex',
        sessionPersistenceProvider: 'zmx',
      }),
      showSessionCommandCopyActions: true,
      showSessionDetailsCopyAction: true,
    });

    expect(eligibility.canCopyAttachCommand).toBe(true);
    expect(eligibility.canCopyResumeCommand).toBe(true);
    expect(eligibility.canForkSession).toBe(true);
  });

  test('shows remote timer and pop-out actions only from explicit row capabilities', () => {
    const eligibility = getSidebarSessionContextMenuEligibility({
      isProjectSessionListMoreRow: false,
      isRemoteSession: true,
      session: createContextMenuSession({
        canPopOutPane: true,
        canScheduleDelayedSend: true,
        canToggleCloseAfterDone: true,
      }),
      showSessionCommandCopyActions: true,
      showSessionDetailsCopyAction: true,
    });

    expect(eligibility.canDelayedSend).toBe(true);
    expect(eligibility.canCloseAfterDone).toBe(true);
    expect(eligibility.canPopOutPane).toBe(true);

    expect(
      getSidebarSessionContextMenuEligibility({
        isProjectSessionListMoreRow: false,
        isRemoteSession: true,
        session: createContextMenuSession({
          canPopOutPane: true,
          isSleeping: true,
          lifecycleState: 'sleeping',
        }),
        showSessionCommandCopyActions: true,
        showSessionDetailsCopyAction: true,
      }).canPopOutPane
    ).toBe(false);
  });

  test('keeps local terminal timer and pop-out gates unchanged', () => {
    const eligibility = getSidebarSessionContextMenuEligibility({
      isProjectSessionListMoreRow: false,
      isRemoteSession: false,
      session: createContextMenuSession({ agentIcon: 'codex' }),
      showSessionCommandCopyActions: true,
      showSessionDetailsCopyAction: true,
    });

    expect(eligibility.canDelayedSend).toBe(true);
    expect(eligibility.canCloseAfterDone).toBe(true);
    expect(eligibility.canPopOutPane).toBe(true);
  });
});

describe('createSleepBelowDebugDetails', () => {
  test('keeps Sleep below lag diagnostics free of user-owned content', () => {
    const details = createSleepBelowDebugDetails({
      clickedSessionKind: 'terminal',
      debugInstanceId: 42,
      elapsedSinceRequestMs: 12.34,
      event: 'posted',
      flushDurationMs: 4.44,
      frameDelayMs: 17.19,
      postMessageDurationMs: 3.91,
      resolveDurationMs: 2.24,
      skippedCount: 1,
      sourceIndex: 3,
      targetCount: 5,
      visibleBelowCount: 6,
    });

    expect(details).toEqual({
      action: 'sleepBelow',
      clickedSessionKind: 'terminal',
      debugInstanceId: 42,
      elapsedSinceRequestMs: 12.3,
      event: 'posted',
      flushDurationMs: 4.4,
      frameDelayMs: 17.2,
      postMessageDurationMs: 3.9,
      resolveDurationMs: 2.2,
      skippedCount: 1,
      sourceIndex: 3,
      targetCount: 5,
      visibleBelowCount: 6,
    });
    expect(Object.keys(details).sort()).not.toEqual(
      expect.arrayContaining(['command', 'message', 'path', 'sessionId', 'sessionIds', 'text', 'title', 'url'])
    );
  });
});

describe('canSleepSidebarSession', () => {
  const baseSession: SidebarSessionItem = {
    activity: 'idle',
    alias: 'Session 1',
    isRunning: true,
    isSleeping: false,
    kind: 'terminal',
    lifecycleState: 'running',
    primaryTitle: 'Session 1',
    sessionId: 'session-1',
    sessionKind: 'terminal',
  };

  test('skips already sleeping sessions from bulk sleep actions', () => {
    expect(canSleepSidebarSession(baseSession)).toBe(true);
    expect(
      canSleepSidebarSession({ ...baseSession, isRunning: false, isSleeping: true, lifecycleState: 'sleeping' })
    ).toBe(false);
    expect(canSleepSidebarSession({ ...baseSession, lifecycleState: 'sleeping' })).toBe(false);
  });
});
