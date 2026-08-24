import { DEFAULT_COMPLETION_SOUND, getCompletionSoundLabel } from '../shared/completion-sound';
import { createDefaultSidebarAgentButtons } from '../shared/sidebar-agents';
import { createDefaultSidebarCommandButtons } from '../shared/sidebar-commands';
import { createDefaultSidebarGitState } from '../shared/sidebar-git';
import {
  DEFAULT_ghostex_SETTINGS,
  normalizeghostexSettings,
  type SidebarProjectGroupingMode,
  type SidebarV2Layout,
  type SidebarVersion,
  type ghostexSettings,
} from '../shared/ghostex-settings';
import type {
  SidebarHydrateMessage,
  SidebarHudState,
  SidebarTheme,
  TerminalViewMode,
  VisibleSessionCount,
} from '../shared/session-grid-contract';
import { clampVisibleSessionCount } from '../shared/session-grid-contract';
import { GROUPS_BY_FIXTURE } from './sidebar-story-fixture-data';
import {
  cloneGroups,
  createStoryPreviousSession,
  getFocusedSessionTitle,
  getVisibleSlotLabels,
} from './sidebar-story-fixture-helpers';

export type SidebarStoryFixture =
  | 'agent-icon-render'
  | 'combined-header-alignment'
  | 'combined-recent-projects'
  | 'combined-sparse-reference'
  | 'command-indicator-active'
  | 'default'
  | 'sort-toggle-demo'
  | 'selector-states'
  | 'overflow-stress'
  | 'scroll-end-retention'
  | 'empty-groups'
  /*
   * CDXC:SidebarV2 2026-07-29:
   * Inbox-sidebar fixtures. They live alongside the V1 fixtures rather than in
   * a parallel harness so V2 stories exercise the SAME SidebarApp, message
   * bridge, and settings pipeline the real sidebar runs on.
   */
  | 'sidebar-v2-empty'
  | 'sidebar-v2-gxserver-unavailable'
  | 'sidebar-v2-inbox'
  /*
   * CDXC:SidebarV2LogicalProjects 2026-07-29:
   * One repository in three physical places plus a non-git project, so the
   * cross-machine merge, the machine badges, and the per-machine auto-settle
   * window all have something real to act on.
   */
  | 'sidebar-v2-multi-machine'
  /*
   * CDXC:SidebarV2LogicalProjects 2026-07-29 (P5 fix round):
   * Two sub-projects of ONE repository checkout, which is the only shape where
   * "Repository" and "Repository + path" disagree.
   */
  | 'sidebar-v2-monorepo'
  /*
   * CDXC:SidebarV2ProjectIcons 2026-07-29 (discovered icons):
   * One project per branch of the icon precedence chain — a user-attached
   * image, a user-chosen Tabler glyph, a repository that ships its own favicon
   * and nothing else, and a project with no icon at all.
   */
  | 'sidebar-v2-project-icons'
  | 'sidebar-v2-row-width'
  | 'three-groups-stress';

export type SidebarStoryArgs = {
  createSessionOnSidebarDoubleClick: boolean;
  debuggingMode: boolean;
  fixture: SidebarStoryFixture;
  highlightedVisibleCount: VisibleSessionCount;
  isFocusModeActive: boolean;
  renameSessionOnDoubleClick: boolean;
  showCloseButtonOnSessionCards: boolean;
  showSessionCloseContextMenuAction: boolean;
  showSessionCommandCopyActions: boolean;
  showSessionDetailsCopyAction: boolean;
  /*
   * CDXC:SidebarV2Lifecycle 2026-07-29:
   * Which settle/snooze capability the story's gxserver claims. "absent" is not
   * a cosmetic variant — it is the ONLY way to exercise an un-upgraded daemon,
   * where the shelves must stay empty and every affordance must be missing
   * rather than merely disabled.
   */
  /*
   * CDXC:SidebarV2Git 2026-07-29:
   * "settleAndSnooze" is a daemon that has lifecycle but no git probe, which is
   * a REAL shape (a machine upgraded to P2 but not P3), so it doubles as the
   * "git capability absent" case: its cards must look exactly like cards from a
   * session with no `gitStatus` at all.
   */
  /*
   * CDXC:SidebarV2Worktree 2026-07-29:
   * "settleSnoozeGitAndWorktree" is the fully upgraded daemon. Keeping it as a
   * separate step matters: "settleSnoozeAndGit" is a P3 machine, and V2's split
   * "+" must collapse to the plain button there — that is the capability-absent
   * case the worktree stories assert.
   */
  sidebarLifecycleCapabilities?: 'absent' | 'settleAndSnooze' | 'settleSnoozeAndGit' | 'settleSnoozeGitAndWorktree';
  /*
   * CDXC:SidebarV2LogicalProjects 2026-07-29:
   * Seed grouping overrides so a story can start from an already-separated
   * project instead of having to click its way there first. Stories that
   * exercise the CHANGE still click the menu and assert the settings patch.
   */
  sidebarProjectGroupingOverrides?: Readonly<Record<string, SidebarProjectGroupingMode>>;
  /*
   * CDXC:SidebarV2 2026-07-29:
   * Sidebar V2 is an opt-in setting, so stories select it the same way users
   * do. Leaving these unset keeps every existing story on the classic sidebar.
   */
  sidebarV2Layout?: SidebarV2Layout;
  sidebarVersion?: SidebarVersion;
  theme: SidebarTheme;
  viewMode: TerminalViewMode;
  visibleCount: VisibleSessionCount;
};

export type SidebarStoryCurrentSettings = ghostexSettings & {
  sidebarWidth?: number;
};

const PREVIOUS_SESSIONS_BY_FIXTURE: Partial<Record<SidebarStoryFixture, SidebarHydrateMessage['previousSessions']>> = {
  'sort-toggle-demo': [
    createStoryPreviousSession({
      alias: 'recent retrospective',
      detail: 'OpenAI Codex',
      historyId: 'history-1',
      sessionId: 'history-session-1',
      shortcutLabel: '⌘⌥7',
    }),
    createStoryPreviousSession({
      alias: 'archived follow-up',
      detail: 'Claude Code',
      historyId: 'history-2',
      sessionId: 'history-session-2',
      shortcutLabel: '⌘⌥8',
    }),
  ],
  /*
   * CDXC:SidebarV2 2026-07-29:
   * The Inbox sidebar filters the list exactly as V1 does, which includes
   * offering matching CLOSED sessions. Give the V2 fixture real previous
   * sessions so that path is exercised there too.
   */
  'sidebar-v2-inbox': [
    createStoryPreviousSession({
      alias: 'release retro notes',
      detail: 'OpenAI Codex',
      historyId: 'v2-history-1',
      sessionId: 'v2-history-session-1',
      shortcutLabel: '⌘⌥7',
    }),
    createStoryPreviousSession({
      alias: 'release blockers triage',
      detail: 'Claude Code',
      historyId: 'v2-history-2',
      sessionId: 'v2-history-session-2',
      shortcutLabel: '⌘⌥8',
    }),
  ],
  /**
   * CDXC:SidebarSearch 2026-05-08-12:16
   * Combined-reference Storybook fixtures need real previous-session search
   * hits so spacing between project matches and Previous Sessions can be
   * reproduced without synthetic DOM injection. Keep at least 40 matching
   * previous rows so the long native-sidebar result list from the regression
   * screenshots is represented in Storybook.
   */
  'combined-sparse-reference': [
    createStoryPreviousSession({
      alias: 'Rename Modal Generator',
      detail: 'OpenAI Codex',
      historyId: 'combined-history-1',
      sessionId: 'combined-history-session-1',
      shortcutLabel: '⌘⌥7',
    }),
    createStoryPreviousSession({
      alias: 'Sidebar interactions search',
      detail: 'Browser',
      historyId: 'combined-history-2',
      sessionId: 'combined-history-session-2',
      shortcutLabel: '⌘⌥8',
    }),
    ...Array.from({ length: 40 }, (_, index) =>
      createStoryPreviousSession({
        alias: `nn previous session ${index + 1}`,
        detail: index % 3 === 0 ? 'OpenAI Codex' : index % 3 === 1 ? 'Browser' : 'Terminal',
        historyId: `combined-history-extra-${index + 1}`,
        sessionId: `combined-history-extra-session-${index + 1}`,
        shortcutLabel: `⌘⌥${(index % 9) + 1}`,
      })
    ),
  ],
};

const COMMAND_SESSION_INDICATORS_BY_FIXTURE: Partial<
  Record<SidebarStoryFixture, SidebarHudState['commandSessionIndicators']>
> = {
  'command-indicator-active': [
    {
      commandId: 'dev',
      isActive: true,
      sessionId: 'session-1',
      status: 'running',
      title: 'Dev server',
    },
  ],
};

function isCombinedReferenceFixture(fixture: SidebarStoryFixture): boolean {
  return (
    fixture === 'combined-header-alignment' ||
    fixture === 'combined-recent-projects' ||
    fixture === 'combined-sparse-reference'
  );
}

function createCombinedStorySettings(currentSettings: SidebarStoryCurrentSettings | undefined): ghostexSettings {
  /**
   * CDXC:StorybookSettings 2026-05-08-16:45
   * CDXC:SidebarLayout 2026-05-13-08:11
   * Storybook sidebar scenarios inherit the user's current ghostex settings
   * snapshot when available. Combined/reference sidebar is now the only target
   * surface, so the fixture no longer needs to force a mode setting.
   */
  return normalizeghostexSettings(currentSettings ?? DEFAULT_ghostex_SETTINGS);
}

export function createSidebarStoryMessage(
  args: SidebarStoryArgs,
  currentSettings?: SidebarStoryCurrentSettings
): SidebarHydrateMessage {
  const baseStorySettings = isCombinedReferenceFixture(args.fixture)
    ? createCombinedStorySettings(currentSettings)
    : args.showSessionCloseContextMenuAction || args.showSessionCommandCopyActions || args.showSessionDetailsCopyAction
      ? normalizeghostexSettings({
          ...DEFAULT_ghostex_SETTINGS,
          showSessionCloseContextMenuAction: args.showSessionCloseContextMenuAction,
          showSessionCommandCopyActions: args.showSessionCommandCopyActions,
          showSessionDetailsCopyAction: args.showSessionDetailsCopyAction,
        })
      : undefined;
  /*
   * CDXC:SidebarV2 2026-07-29:
   * Only stories that explicitly ask for the Inbox sidebar carry a settings
   * override, so the hydrate payload of every other story is unchanged.
   */
  const storySettings =
    args.sidebarVersion || args.sidebarV2Layout || args.sidebarProjectGroupingOverrides
      ? normalizeghostexSettings({
          ...(baseStorySettings ?? DEFAULT_ghostex_SETTINGS),
          /*
           * CDXC:SidebarV2LogicalProjects 2026-07-29:
           * Grouping overrides ride the same settings object the sidebar reads
           * at runtime, so a seeded story starts in exactly the state a user
           * who had already chosen "Keep separate" would see.
           */
          sidebarProjectGroupingOverrides:
            args.sidebarProjectGroupingOverrides ?? DEFAULT_ghostex_SETTINGS.sidebarProjectGroupingOverrides,
          sidebarV2Layout: args.sidebarV2Layout ?? DEFAULT_ghostex_SETTINGS.sidebarV2Layout,
          sidebarVersion: args.sidebarVersion ?? DEFAULT_ghostex_SETTINGS.sidebarVersion,
        })
      : baseStorySettings;
  const groups = cloneGroups(GROUPS_BY_FIXTURE[args.fixture]).map((group) => {
    const visibleCount = group.isActive
      ? args.visibleCount
      : clampVisibleSessionCount(Math.max(1, group.sessions.length));

    return {
      ...group,
      isFocusModeActive: group.isActive ? args.isFocusModeActive : false,
      layoutVisibleCount: group.isActive ? args.highlightedVisibleCount : visibleCount,
      viewMode: group.isActive ? args.viewMode : 'grid',
      visibleCount,
    };
  });
  const hud: SidebarHudState = {
    activeSessionsSortMode: 'manual',
    agentManagerZoomPercent: 100,
    agents: createDefaultSidebarAgentButtons(),
    commands: createDefaultSidebarCommandButtons(),
    commandSessionIndicators: COMMAND_SESSION_INDICATORS_BY_FIXTURE[args.fixture] ?? [],
    completionBellEnabled: storySettings?.completionBellEnabled ?? false,
    completionSound: storySettings?.completionSound ?? DEFAULT_COMPLETION_SOUND,
    completionSoundLabel: getCompletionSoundLabel(storySettings?.completionSound ?? DEFAULT_COMPLETION_SOUND),
    debuggingMode: storySettings?.debuggingMode ?? args.debuggingMode,
    focusedSessionTitle: getFocusedSessionTitle(groups),
    git: createDefaultSidebarGitState(),
    highlightedVisibleCount: args.highlightedVisibleCount,
    isFocusModeActive: args.isFocusModeActive,
    /*
     * CDXC:SidebarV2Lifecycle 2026-07-29:
     * Absent means "this daemon has no session lifecycle", which is exactly
     * what an older gxserver looks like on the wire. Spelling it as an omitted
     * key rather than `{sessionSettlement: false}` keeps the story honest about
     * the shape the sidebar actually has to survive.
     */
    ...(args.sidebarLifecycleCapabilities === 'settleAndSnooze' ||
    args.sidebarLifecycleCapabilities === 'settleSnoozeAndGit' ||
    args.sidebarLifecycleCapabilities === 'settleSnoozeGitAndWorktree'
      ? {
          lifecycleCapabilities: {
            sessionGitStatus:
              args.sidebarLifecycleCapabilities === 'settleSnoozeAndGit' ||
              args.sidebarLifecycleCapabilities === 'settleSnoozeGitAndWorktree',
            sessionSettlement: true,
            sessionSnooze: true,
            worktreeSessions: args.sidebarLifecycleCapabilities === 'settleSnoozeGitAndWorktree',
          },
        }
      : {}),
    /*
     * CDXC:SidebarV2LogicalProjects 2026-07-29:
     * The multi-machine fixture's SECOND daemon. It is fully capable, and it
     * states a THIRTY-day auto-settle window against the local default of
     * three, so a five-day-idle session must be parked locally and still active
     * on Build Box. Applying one window to both machines was the recorded P2
     * minor; this pairing is what catches a regression back into it.
     */
    ...(args.fixture === 'sidebar-v2-multi-machine'
      ? {
          autoSettleAfterDays: 3,
          autoSettleAfterDaysByMachineId: { 'build-box': 30 },
          lifecycleCapabilitiesByMachineId: {
            'build-box': {
              sessionGitStatus: true,
              sessionSettlement: true,
              sessionSnooze: true,
              worktreeSessions: true,
            },
          },
        }
      : {}),
    pendingAgentIds: [],
    recentProjects:
      args.fixture === 'combined-recent-projects' || args.fixture === 'combined-sparse-reference'
        ? [
            {
              path: '/Users/story/dev/shortpoint',
              projectId: 'recent-shortpoint',
              recentClosedAt: new Date(Date.now() - 10 * 60 * 1000).toISOString(),
              sessionCount: 3,
              title: 'shortpoint',
            },
            {
              path: '/Users/story/dev/open-design',
              projectId: 'recent-open-design',
              recentClosedAt: new Date(Date.now() - 40 * 60 * 1000).toISOString(),
              sessionCount: 0,
              title: 'open-design',
            },
          ]
        : [],
    settings: storySettings,
    createSessionOnSidebarDoubleClick:
      storySettings?.createSessionOnSidebarDoubleClick ?? args.createSessionOnSidebarDoubleClick,
    renameSessionOnDoubleClick: storySettings?.renameSessionOnDoubleClick ?? args.renameSessionOnDoubleClick,
    showCloseButtonOnSessionCards: storySettings?.showCloseButtonOnSessionCards ?? args.showCloseButtonOnSessionCards,
    theme: args.theme,
    viewMode: args.viewMode,
    visibleCount: args.visibleCount,
    visibleSlotLabels: getVisibleSlotLabels(groups),
  };

  return {
    groups,
    hud,
    pinnedPrompts: [],
    previousSessions: (PREVIOUS_SESSIONS_BY_FIXTURE[args.fixture] ?? []).map((session) => ({
      ...session,
    })),
    revision: 1,
    scratchPadContent: '',
    type: 'hydrate',
  };
}
