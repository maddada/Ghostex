import { describe, expect, it } from 'vitest';
import type { SidebarSessionItem } from '../../shared/session-grid-contract';
import type { SidebarV2ProjectGroupingSettings } from '../../shared/sidebar-v2-logical-project';
import type { SidebarGroupRecord } from '../sidebar-store';
import {
  SIDEBAR_V2_ALL_SCOPE_ID,
  createSidebarV2ViewModel,
  resolveSidebarV2CreationRanks,
} from './sidebar-v2-view-model';

/*
 * CDXC:SidebarV2 2026-07-29:
 * The inbox's promise is that a row holds its position from creation until it
 * parks — activity must never reorder the list. These tests pin the ordering
 * inputs (createdAt vs the first-seen fallback) and the shelf/scope split,
 * because a regression there is invisible in a screenshot but destroys the
 * whole reason V2 exists.
 */

const HOUR_MS = 60 * 60 * 1_000;
const DAY_MS = 24 * HOUR_MS;
const NOW_MS = Date.parse('2026-07-29T12:00:00.000Z');

function iso(offsetMs: number): string {
  return new Date(NOW_MS + offsetMs).toISOString();
}

function session(overrides: Partial<SidebarSessionItem> & { sessionId: string }): SidebarSessionItem {
  return {
    activity: 'idle',
    alias: overrides.sessionId,
    column: 0,
    isFocused: false,
    row: 0,
    shortcutLabel: '1',
    ...overrides,
  };
}

function group(overrides: Partial<SidebarGroupRecord> & { groupId: string }): SidebarGroupRecord {
  return {
    groupId: overrides.groupId,
    isActive: false,
    isFocusModeActive: false,
    layoutVisibleCount: 1,
    title: overrides.groupId,
    viewMode: 'grid',
    visibleCount: 1,
    ...overrides,
  };
}

function buildInput(
  sessions: readonly SidebarSessionItem[],
  options: {
    creationOrder?: readonly string[];
    groups?: readonly SidebarGroupRecord[];
    /* CDXC:SidebarV2LogicalProjects 2026-07-29 (P5 fix round): monorepo cases
       need the grouping settings, which every other case leaves at default. */
    projectGrouping?: SidebarV2ProjectGroupingSettings;
    scopeId?: string;
    sessionIdsByGroup?: Record<string, readonly string[]>;
  } = {}
) {
  const groups = options.groups ?? [group({ groupId: 'project-a' })];
  return {
    creationOrder: options.creationOrder ?? [],
    groupIds: groups.map((entry) => entry.groupId),
    groupsById: Object.fromEntries(groups.map((entry) => [entry.groupId, entry])),
    nowMs: NOW_MS,
    ...(options.projectGrouping ? { projectGrouping: options.projectGrouping } : {}),
    scopeId: options.scopeId ?? SIDEBAR_V2_ALL_SCOPE_ID,
    sessionIdsByGroup:
      options.sessionIdsByGroup ??
      ({ [groups[0]!.groupId]: sessions.map((entry) => entry.sessionId) } as Record<string, readonly string[]>),
    sessionsById: Object.fromEntries(sessions.map((entry) => [entry.sessionId, entry])),
  };
}

describe('resolveSidebarV2CreationRanks', () => {
  it('ranks by createdAt when the host published it', () => {
    const ranks = resolveSidebarV2CreationRanks({
      creationOrder: [],
      sessions: [
        { activity: 'idle', createdAt: iso(-2 * HOUR_MS), sessionId: 'older' },
        { activity: 'idle', createdAt: iso(-1 * HOUR_MS), sessionId: 'newer' },
      ],
    });
    expect(ranks.get('newer')!).toBeGreaterThan(ranks.get('older')!);
  });

  it('sinks sessions without createdAt below every dated row', () => {
    const ranks = resolveSidebarV2CreationRanks({
      creationOrder: ['undated'],
      sessions: [
        { activity: 'idle', createdAt: iso(-10 * DAY_MS), sessionId: 'dated' },
        { activity: 'idle', sessionId: 'undated' },
      ],
    });
    expect(ranks.get('undated')!).toBeLessThan(ranks.get('dated')!);
  });

  it('orders undated sessions newest-first by the first-seen registry', () => {
    const ranks = resolveSidebarV2CreationRanks({
      creationOrder: ['seen-newest', 'seen-oldest'],
      sessions: [
        { activity: 'idle', sessionId: 'seen-oldest' },
        { activity: 'idle', sessionId: 'seen-newest' },
      ],
    });
    expect(ranks.get('seen-newest')!).toBeGreaterThan(ranks.get('seen-oldest')!);
  });
});

describe('createSidebarV2ViewModel', () => {
  it('orders the flat inbox newest-created first with pinned rows floating', () => {
    const model = createSidebarV2ViewModel(
      buildInput([
        session({ createdAt: iso(-3 * HOUR_MS), sessionId: 'oldest' }),
        session({ createdAt: iso(-1 * HOUR_MS), sessionId: 'newest' }),
        session({ createdAt: iso(-5 * HOUR_MS), isPinned: true, sessionId: 'pinned' }),
      ])
    );
    expect(model.flat.active.map((entry) => entry.sessionId)).toEqual(['pinned', 'newest', 'oldest']);
  });

  it('keeps positions stable when only activity changes', () => {
    const before = createSidebarV2ViewModel(
      buildInput([
        session({ createdAt: iso(-3 * HOUR_MS), sessionId: 'a' }),
        session({ createdAt: iso(-1 * HOUR_MS), sessionId: 'b' }),
      ])
    );
    const after = createSidebarV2ViewModel(
      buildInput([
        session({
          activity: 'working',
          createdAt: iso(-3 * HOUR_MS),
          lastInteractionAt: iso(0),
          sessionId: 'a',
        }),
        session({ createdAt: iso(-1 * HOUR_MS), sessionId: 'b' }),
      ])
    );
    expect(after.flat.active.map((entry) => entry.sessionId)).toEqual(
      before.flat.active.map((entry) => entry.sessionId)
    );
  });

  it('moves sessions idle past the auto-settle window onto the settled shelf', () => {
    const model = createSidebarV2ViewModel(
      buildInput([
        session({ createdAt: iso(-9 * DAY_MS), lastInteractionAt: iso(-6 * DAY_MS), sessionId: 'stale' }),
        session({ createdAt: iso(-1 * HOUR_MS), lastInteractionAt: iso(-1 * HOUR_MS), sessionId: 'fresh' }),
      ])
    );
    expect(model.flat.settled.map((entry) => entry.sessionId)).toEqual(['stale']);
    expect(model.flat.active.map((entry) => entry.sessionId)).toEqual(['fresh']);
  });

  it('never settles a session that is blocked on the user', () => {
    const model = createSidebarV2ViewModel(
      buildInput([
        session({
          activity: 'attention',
          createdAt: iso(-30 * DAY_MS),
          lastInteractionAt: iso(-20 * DAY_MS),
          sessionId: 'blocked',
        }),
      ])
    );
    expect(model.flat.settled).toHaveLength(0);
    expect(model.flat.active.map((entry) => entry.sessionId)).toEqual(['blocked']);
  });

  it('hides snoozed sessions from the inbox until their wake time', () => {
    const model = createSidebarV2ViewModel(
      buildInput([
        Object.assign(
          session({ createdAt: iso(-4 * HOUR_MS), lastInteractionAt: iso(-3 * HOUR_MS), sessionId: 'napping' }),
          { snoozedAt: iso(-1 * HOUR_MS), snoozedUntil: iso(2 * HOUR_MS) }
        ),
      ])
    );
    expect(model.flat.snoozed.map((entry) => entry.sessionId)).toEqual(['napping']);
    expect(model.flat.active).toHaveLength(0);
  });

  it('keeps browser rows out of the inbox partition', () => {
    const model = createSidebarV2ViewModel(
      buildInput([
        session({ createdAt: iso(-1 * HOUR_MS), kind: 'browser', sessionId: 'tab', sessionKind: 'browser' }),
        session({ createdAt: iso(-2 * HOUR_MS), sessionId: 'agent' }),
      ])
    );
    expect(model.browserSessions.map((entry) => entry.sessionId)).toEqual(['tab']);
    expect(model.flat.active.map((entry) => entry.sessionId)).toEqual(['agent']);
    expect(model.groups[0]!.browserSessions.map((entry) => entry.sessionId)).toEqual(['tab']);
  });

  it('scopes the flat inbox to one project without changing the grouped model', () => {
    const groups = [group({ groupId: 'project-a' }), group({ groupId: 'project-b' })];
    const model = createSidebarV2ViewModel(
      buildInput([], {
        groups,
        scopeId: 'project-b',
        sessionIdsByGroup: { 'project-a': ['a-1'], 'project-b': ['b-1'] },
      })
    );
    expect(model.flat.active).toHaveLength(0);
    expect(model.groups).toHaveLength(2);

    const scoped = createSidebarV2ViewModel({
      ...buildInput(
        [
          session({ createdAt: iso(-1 * HOUR_MS), sessionId: 'a-1' }),
          session({ createdAt: iso(-2 * HOUR_MS), sessionId: 'b-1' }),
        ],
        {
          groups,
          scopeId: 'project-b',
          sessionIdsByGroup: { 'project-a': ['a-1'], 'project-b': ['b-1'] },
        }
      ),
    });
    expect(scoped.flat.active.map((entry) => entry.sessionId)).toEqual(['b-1']);
    expect(scoped.groups.map((entry) => entry.sessionCount)).toEqual([1, 1]);
  });

  /*
   * CDXC:SidebarV2Lifecycle 2026-07-29:
   * Capability is per daemon and one inbox mixes daemons. These pin the exact
   * failure the capability flags exist to prevent: an un-upgraded remote
   * machine's rows getting settled/snoozed classifications it cannot serve, and
   * therefore parked on a shelf with no way back.
   */
  it('never parks a session whose daemon has no settle capability', () => {
    const model = createSidebarV2ViewModel({
      ...buildInput([
        session({
          createdAt: iso(-30 * DAY_MS),
          lastInteractionAt: iso(-20 * DAY_MS),
          sessionId: 'stale',
        }),
      ]),
      capabilitiesByGroupId: { 'project-a': { settle: false, snooze: false } },
    });
    expect(model.flat.settled).toHaveLength(0);
    expect(model.flat.active.map((entry) => entry.sessionId)).toEqual(['stale']);
  });

  it('never hides a snoozed session whose daemon has no snooze capability', () => {
    const model = createSidebarV2ViewModel({
      ...buildInput([
        Object.assign(session({ createdAt: iso(-4 * HOUR_MS), sessionId: 'napping' }), {
          snoozedAt: iso(-1 * HOUR_MS),
          snoozedUntil: iso(2 * HOUR_MS),
        }),
      ]),
      capabilitiesByGroupId: { 'project-a': { settle: true, snooze: false } },
    });
    expect(model.flat.snoozed).toHaveLength(0);
    expect(model.flat.active.map((entry) => entry.sessionId)).toEqual(['napping']);
  });

  it('treats a group missing from a supplied capability map as incapable', () => {
    const model = createSidebarV2ViewModel({
      ...buildInput([
        session({
          createdAt: iso(-30 * DAY_MS),
          lastInteractionAt: iso(-20 * DAY_MS),
          sessionId: 'stale',
        }),
      ]),
      capabilitiesByGroupId: {},
    });
    expect(model.flat.settled).toHaveLength(0);
    expect(model.capabilitiesByGroupId['project-a']).toEqual({ settle: false, snooze: false });
  });

  it("applies each group's own capability when daemons disagree", () => {
    const groups = [group({ groupId: 'local' }), group({ groupId: 'legacy-remote' })];
    const model = createSidebarV2ViewModel({
      ...buildInput(
        [
          session({
            createdAt: iso(-30 * DAY_MS),
            lastInteractionAt: iso(-20 * DAY_MS),
            sessionId: 'local-stale',
          }),
          session({
            createdAt: iso(-30 * DAY_MS),
            lastInteractionAt: iso(-20 * DAY_MS),
            sessionId: 'remote-stale',
          }),
        ],
        {
          groups,
          sessionIdsByGroup: { 'legacy-remote': ['remote-stale'], local: ['local-stale'] },
        }
      ),
      capabilitiesByGroupId: {
        'legacy-remote': { settle: false, snooze: false },
        local: { settle: true, snooze: true },
      },
    });
    expect(model.flat.settled.map((entry) => entry.sessionId)).toEqual(['local-stale']);
    expect(model.flat.active.map((entry) => entry.sessionId)).toEqual(['remote-stale']);
  });

  it('honors an auto-settle window of null by never settling on inactivity', () => {
    const model = createSidebarV2ViewModel({
      ...buildInput([
        session({
          createdAt: iso(-90 * DAY_MS),
          lastInteractionAt: iso(-60 * DAY_MS),
          sessionId: 'ancient',
        }),
      ]),
      autoSettleAfterDays: null,
    });
    expect(model.flat.settled).toHaveLength(0);
  });

  it('reports the soonest wake boundary across every group, not just the scope', () => {
    const groups = [group({ groupId: 'project-a' }), group({ groupId: 'project-b' })];
    const model = createSidebarV2ViewModel({
      ...buildInput(
        [
          Object.assign(session({ createdAt: iso(-4 * HOUR_MS), sessionId: 'late' }), {
            snoozedAt: iso(-1 * HOUR_MS),
            snoozedUntil: iso(6 * HOUR_MS),
          }),
          Object.assign(session({ createdAt: iso(-4 * HOUR_MS), sessionId: 'soon' }), {
            snoozedAt: iso(-1 * HOUR_MS),
            snoozedUntil: iso(2 * HOUR_MS),
          }),
        ],
        {
          groups,
          scopeId: 'project-a',
          sessionIdsByGroup: { 'project-a': ['late'], 'project-b': ['soon'] },
        }
      ),
    });
    expect(model.nextWakeAtMs).toBe(NOW_MS + 2 * HOUR_MS);
  });

  it('reports no wake boundary when nothing is snoozed', () => {
    const model = createSidebarV2ViewModel(buildInput([session({ createdAt: iso(-1 * HOUR_MS), sessionId: 'awake' })]));
    expect(model.nextWakeAtMs).toBeNull();
  });

  it('sorts the merged snoozed shelf soonest-wake-first across projects', () => {
    const groups = [group({ groupId: 'project-a' }), group({ groupId: 'project-b' })];
    const model = createSidebarV2ViewModel(
      buildInput(
        [
          Object.assign(session({ createdAt: iso(-4 * HOUR_MS), sessionId: 'later' }), {
            snoozedUntil: iso(6 * HOUR_MS),
          }),
          Object.assign(session({ createdAt: iso(-9 * HOUR_MS), sessionId: 'sooner' }), {
            snoozedUntil: iso(1 * HOUR_MS),
          }),
        ],
        {
          groups,
          sessionIdsByGroup: { 'project-a': ['later'], 'project-b': ['sooner'] },
        }
      )
    );
    expect(model.flat.snoozed.map((entry) => entry.sessionId)).toEqual(['sooner', 'later']);
  });

  it('labels the chat collection as the Quick pseudo-project', () => {
    const model = createSidebarV2ViewModel(
      buildInput([session({ createdAt: iso(0), sessionId: 'chat' })], {
        groups: [group({ groupId: 'chats', isChatCollection: true, title: 'Chats' })],
        sessionIdsByGroup: { chats: ['chat'] },
      })
    );
    expect(model.projectsByGroupId['chats']!.title).toBe('Quick');
    expect(model.scopeOptions.map((option) => option.label)).toEqual(['All projects', 'Quick']);
  });
});

/*
 * CDXC:SidebarV2LogicalProjects 2026-07-29:
 * Cross-machine logical projects and the per-machine auto-settle window. Both
 * are invisible in a single-machine screenshot and both change what the user
 * sees the moment a second machine connects, so they are pinned here rather
 * than only in stories.
 */
function projectContext(path: string, gitRemoteOriginUrl?: string | null, gitRepositoryRootPath?: string) {
  return {
    canRemoveProject: true,
    editor: {
      diffStats: { additions: 0, deletions: 0, files: 0, isLoading: false, isRepo: true },
      isOpen: false,
      isSleeping: false,
      projectId: path,
      status: 'idle' as const,
    },
    ...(gitRemoteOriginUrl === undefined ? {} : { gitRemoteOriginUrl }),
    ...(gitRepositoryRootPath === undefined ? {} : { gitRepositoryRootPath }),
    path,
  };
}

const GHOSTEX_ORIGIN = 'git@github.com:ghostex/ghostex.git';

function twoMachineInput(
  options: {
    autoSettleAfterDaysByGroupId?: Record<string, number | null>;
    overrides?: Record<string, 'repository' | 'repositoryPath' | 'separate'>;
    remoteOrigin?: string | null;
    scopeId?: string;
  } = {}
) {
  const localGroup = group({
    groupId: 'local-ghostex',
    projectContext: projectContext('/Users/madda/dev/Ghostex', GHOSTEX_ORIGIN),
    title: 'Ghostex',
  });
  const remoteGroup = group({
    groupId: 'remote-ghostex',
    projectContext: projectContext(
      '/home/build/ghostex',
      options.remoteOrigin === undefined ? GHOSTEX_ORIGIN : options.remoteOrigin
    ),
    remoteMachineContext: { machineId: 'build-box', machineName: 'Build Box' },
    title: 'ghostex',
  });
  const sessions = [
    session({ createdAt: iso(-HOUR_MS), sessionId: 'local-1' }),
    session({ createdAt: iso(-2 * HOUR_MS), sessionId: 'remote-1' }),
  ];
  return {
    ...(options.autoSettleAfterDaysByGroupId
      ? { autoSettleAfterDaysByGroupId: options.autoSettleAfterDaysByGroupId }
      : {}),
    creationOrder: [],
    groupIds: ['local-ghostex', 'remote-ghostex'],
    groupsById: { 'local-ghostex': localGroup, 'remote-ghostex': remoteGroup },
    nowMs: NOW_MS,
    projectGrouping: {
      sidebarV2ProjectGroupingMode: 'repository' as const,
      sidebarV2ProjectGroupingOverrides: options.overrides ?? {},
    },
    scopeId: options.scopeId ?? SIDEBAR_V2_ALL_SCOPE_ID,
    sessionIdsByGroup: { 'local-ghostex': ['local-1'], 'remote-ghostex': ['remote-1'] },
    sessionsById: Object.fromEntries(sessions.map((entry) => [entry.sessionId, entry])),
  };
}

describe('createSidebarV2ViewModel — cross-machine logical projects', () => {
  it('merges the same repository on two machines into one group', () => {
    const model = createSidebarV2ViewModel(twoMachineInput());
    expect(model.groups).toHaveLength(1);
    const [merged] = model.groups;
    expect(merged?.groupId).toBe('local-ghostex');
    expect(merged?.memberGroupIds).toEqual(['local-ghostex', 'remote-ghostex']);
    expect(merged?.isMerged).toBe(true);
    expect(merged?.title).toBe('ghostex/ghostex');
    expect(merged?.sessionCount).toBe(2);
    expect(merged?.partition.active.map((entry) => entry.sessionId)).toEqual(['local-1', 'remote-1']);
  });

  it('offers one merged entry in the scope filter, counting both machines', () => {
    const model = createSidebarV2ViewModel(twoMachineInput());
    expect(model.scopeOptions.map((option) => option.scopeId)).toEqual([SIDEBAR_V2_ALL_SCOPE_ID, 'local-ghostex']);
    expect(model.scopeOptions[1]?.count).toBe(2);
    expect(model.scopeOptions[1]?.label).toBe('ghostex/ghostex');
  });

  it('scopes to every member of the merged project, not just the representative', () => {
    const model = createSidebarV2ViewModel(twoMachineInput({ scopeId: 'local-ghostex' }));
    expect(model.scopedGroupIds).toEqual(['local-ghostex', 'remote-ghostex']);
    expect(model.flat.active.map((entry) => entry.sessionId)).toEqual(['local-1', 'remote-1']);
  });

  it("keeps each row's OWN machine name so merged rows stay distinguishable", () => {
    const model = createSidebarV2ViewModel(twoMachineInput());
    expect(model.projectsByGroupId['local-ghostex']?.machineName).toBeUndefined();
    expect(model.projectsByGroupId['remote-ghostex']?.machineName).toBe('Build Box');
    // ...while both report the shared repository name on the project line.
    expect(model.projectsByGroupId['local-ghostex']?.title).toBe('ghostex/ghostex');
    expect(model.projectsByGroupId['remote-ghostex']?.title).toBe('ghostex/ghostex');
  });

  it('splits the group again when both members are overridden to separate', () => {
    const model = createSidebarV2ViewModel(
      twoMachineInput({
        overrides: {
          'build-box:/home/build/ghostex': 'separate',
          'local:/Users/madda/dev/Ghostex': 'separate',
        },
      })
    );
    expect(model.groups.map((entry) => entry.groupId)).toEqual(['local-ghostex', 'remote-ghostex']);
    expect(model.groups.every((entry) => entry.isMerged === false)).toBe(true);
    expect(model.groups.map((entry) => entry.groupingMode)).toEqual(['separate', 'separate']);
    expect(model.groups.map((entry) => entry.title)).toEqual(['Ghostex', 'ghostex']);
  });

  it('reports no shared grouping mode when the members disagree', () => {
    const model = createSidebarV2ViewModel(
      twoMachineInput({ overrides: { 'local:/Users/madda/dev/Ghostex': 'separate' } })
    );
    // The local copy opted out, so the two no longer share a key at all.
    expect(model.groups).toHaveLength(2);
    expect(model.groups.map((entry) => entry.groupingMode)).toEqual(['separate', 'repository']);
  });

  it('never merges projects without a git origin', () => {
    const model = createSidebarV2ViewModel(twoMachineInput({ remoteOrigin: null }));
    expect(model.groups).toHaveLength(2);
    expect(model.groups.map((entry) => entry.canGroupAcrossMachines)).toEqual([true, false]);
  });

  it('keeps the grouping submenu off a project with no repository at all', () => {
    const model = createSidebarV2ViewModel(
      buildInput([session({ sessionId: 's1' })], {
        groups: [
          group({
            groupId: 'plain',
            projectContext: projectContext('/Users/madda/dev/plain'),
          }),
        ],
      })
    );
    expect(model.groups[0]?.canGroupAcrossMachines).toBe(false);
    expect(model.groups[0]?.groupingOverrideKeys).toEqual(['local:/Users/madda/dev/plain']);
    expect(model.groups[0]?.repositoryCanonicalKey).toBeUndefined();
  });

  /*
   * CDXC:SidebarV2ProjectIcons 2026-07-29:
   * The project's own icon has to reach every surface that names the project —
   * the card's project line, the group header, and the scope menu — or those
   * surfaces fall back to a folder for a project the user gave an identity.
   */
  it("carries the project's icon into rows, groups, and scope options", () => {
    const icon = { color: '#d6e0f3', icon: 'archive', kind: 'tabler' } as const;
    const model = createSidebarV2ViewModel(
      buildInput([session({ sessionId: 's1' })], {
        groups: [
          group({
            groupId: 'plain',
            projectContext: { ...projectContext('/Users/madda/dev/plain'), icon },
          }),
        ],
      })
    );
    expect(model.projectsByGroupId.plain?.icon).toEqual(icon);
    expect(model.groups[0]?.icon).toEqual(icon);
    expect(model.scopeOptions.find((option) => option.groupId === 'plain')?.icon).toEqual(icon);
  });

  /*
   * CDXC:SidebarV2ProjectIcons 2026-07-29 (discovered icons):
   * The icon gxserver discovered inside the checkout has to reach the same three
   * surfaces, and it has to arrive SEPARATELY from the user's icon: the renderer
   * ranks them (user IMAGE, then discovered, then typed glyph, then folder), and
   * a view model that collapsed them into one field would make that ordering
   * impossible to express.
   */
  it('carries the discovered repository icon into rows, groups, and scope options', () => {
    const discoveredIconDataUrl = 'data:image/png;base64,ZGlzY292ZXJlZA==';
    const icon = { color: '#d6e0f3', icon: 'archive', kind: 'tabler' } as const;
    const model = createSidebarV2ViewModel(
      buildInput([session({ sessionId: 's1' })], {
        groups: [
          group({
            groupId: 'plain',
            projectContext: {
              ...projectContext('/Users/madda/dev/plain'),
              discoveredIconDataUrl,
              icon,
            },
          }),
        ],
      })
    );
    expect(model.projectsByGroupId.plain?.discoveredIconDataUrl).toBe(discoveredIconDataUrl);
    expect(model.groups[0]?.discoveredIconDataUrl).toBe(discoveredIconDataUrl);
    expect(model.scopeOptions.find((option) => option.groupId === 'plain')?.discoveredIconDataUrl).toBe(
      discoveredIconDataUrl
    );
    // Both channels survive independently; the renderer, not the view model,
    // decides which one wins.
    expect(model.groups[0]?.icon).toEqual(icon);
  });

  /*
   * CDXC:SidebarV2LogicalProjects 2026-07-29 (P5 fix round):
   * The repository identity a row belongs to, which is what lets the root apply
   * a merging choice to every row of the same repository instead of only to the
   * one the user right-clicked.
   */
  it('reports the shared repository key on every row of that repository', () => {
    const merged = createSidebarV2ViewModel(twoMachineInput());
    expect(merged.groups[0]?.repositoryCanonicalKey).toBe('github.com/ghostex/ghostex');

    const split = createSidebarV2ViewModel(
      twoMachineInput({
        overrides: {
          'build-box:/home/build/ghostex': 'separate',
          'local:/Users/madda/dev/Ghostex': 'separate',
        },
      })
    );
    expect(split.groups.map((entry) => entry.repositoryCanonicalKey)).toEqual([
      'github.com/ghostex/ghostex',
      'github.com/ghostex/ghostex',
    ]);
  });
});

/*
 * CDXC:SidebarV2LogicalProjects 2026-07-29 (P5 fix round):
 * "Repository + path" only has anything to split on once the daemon publishes
 * `gitRepositoryRootPath`. These pin the whole chain — presentation field →
 * repository identity → logical key — through the real view model.
 */
describe('createSidebarV2ViewModel — monorepo sub-projects', () => {
  const MONO_ORIGIN = 'git@github.com:ghostex/mono.git';
  const MONO_ROOT = '/Users/madda/dev/mono';

  function monorepoInput(overrides: Record<string, 'repository' | 'repositoryPath' | 'separate'> = {}) {
    return buildInput([session({ sessionId: 'web-1' }), session({ sessionId: 'api-1' })], {
      groups: [
        group({
          groupId: 'web',
          projectContext: projectContext(`${MONO_ROOT}/apps/web`, MONO_ORIGIN, MONO_ROOT),
          title: 'web',
        }),
        group({
          groupId: 'api',
          projectContext: projectContext(`${MONO_ROOT}/services/api`, MONO_ORIGIN, MONO_ROOT),
          title: 'api',
        }),
      ],
      projectGrouping: {
        sidebarV2ProjectGroupingMode: 'repository' as const,
        sidebarV2ProjectGroupingOverrides: overrides,
      },
      sessionIdsByGroup: { api: ['api-1'], web: ['web-1'] },
    });
  }

  it('merges two sub-projects of one repository by default', () => {
    const model = createSidebarV2ViewModel(monorepoInput());
    expect(model.groups).toHaveLength(1);
    expect(model.groups[0]?.memberGroupIds).toEqual(['web', 'api']);
  });

  it("splits them once 'repositoryPath' is chosen", () => {
    const model = createSidebarV2ViewModel(
      monorepoInput({
        [`local:${MONO_ROOT}/apps/web`]: 'repositoryPath',
        [`local:${MONO_ROOT}/services/api`]: 'repositoryPath',
      })
    );
    expect(model.groups.map((entry) => entry.groupId)).toEqual(['web', 'api']);
    expect(model.groups.every((entry) => entry.isMerged === false)).toBe(true);
    expect(model.groups.map((entry) => entry.repositoryCanonicalKey)).toEqual([
      'github.com/ghostex/mono',
      'github.com/ghostex/mono',
    ]);
  });

  it('cannot split when the daemon published no repository root', () => {
    const rootless = buildInput([session({ sessionId: 'web-1' }), session({ sessionId: 'api-1' })], {
      groups: [
        group({
          groupId: 'web',
          projectContext: projectContext(`${MONO_ROOT}/apps/web`, MONO_ORIGIN),
          title: 'web',
        }),
        group({
          groupId: 'api',
          projectContext: projectContext(`${MONO_ROOT}/services/api`, MONO_ORIGIN),
          title: 'api',
        }),
      ],
      projectGrouping: {
        sidebarV2ProjectGroupingMode: 'repository' as const,
        sidebarV2ProjectGroupingOverrides: {
          [`local:${MONO_ROOT}/apps/web`]: 'repositoryPath',
          [`local:${MONO_ROOT}/services/api`]: 'repositoryPath',
        },
      },
      sessionIdsByGroup: { api: ['api-1'], web: ['web-1'] },
    });
    expect(createSidebarV2ViewModel(rootless).groups).toHaveLength(1);
  });
});

describe('createSidebarV2ViewModel — per-machine auto-settle window', () => {
  function idleSessions() {
    return [
      session({ lastInteractionAt: iso(-5 * DAY_MS), sessionId: 'local-1' }),
      session({ lastInteractionAt: iso(-5 * DAY_MS), sessionId: 'remote-1' }),
    ];
  }

  function windowInput(autoSettleAfterDaysByGroupId: Record<string, number | null>) {
    const input = twoMachineInput({ autoSettleAfterDaysByGroupId });
    const sessions = idleSessions();
    return {
      ...input,
      autoSettleAfterDays: 3,
      capabilitiesByGroupId: {
        'local-ghostex': { settle: true, snooze: true },
        'remote-ghostex': { settle: true, snooze: true },
      },
      // Split them apart so each group's shelf can be inspected on its own.
      projectGrouping: {
        sidebarV2ProjectGroupingMode: 'repository' as const,
        sidebarV2ProjectGroupingOverrides: {
          'build-box:/home/build/ghostex': 'separate' as const,
          'local:/Users/madda/dev/Ghostex': 'separate' as const,
        },
      },
      sessionsById: Object.fromEntries(sessions.map((entry) => [entry.sessionId, entry])),
    };
  }

  it("applies each machine's OWN window instead of the local one", () => {
    const model = createSidebarV2ViewModel(windowInput({ 'local-ghostex': 3, 'remote-ghostex': 14 }));
    const local = model.groups.find((entry) => entry.groupId === 'local-ghostex');
    const remote = model.groups.find((entry) => entry.groupId === 'remote-ghostex');
    // 5 days idle: past the local 3-day window, inside the remote 14-day one.
    expect(local?.partition.settled.map((entry) => entry.sessionId)).toEqual(['local-1']);
    expect(remote?.partition.settled).toEqual([]);
    expect(remote?.partition.active.map((entry) => entry.sessionId)).toEqual(['remote-1']);
  });

  it('never client-settles rows from a machine that states no window', () => {
    const model = createSidebarV2ViewModel(windowInput({ 'local-ghostex': 3, 'remote-ghostex': null }));
    const remote = model.groups.find((entry) => entry.groupId === 'remote-ghostex');
    expect(remote?.partition.settled).toEqual([]);
  });

  it('falls back to the shared window for groups absent from the map', () => {
    const model = createSidebarV2ViewModel(windowInput({ 'remote-ghostex': null }));
    const local = model.groups.find((entry) => entry.groupId === 'local-ghostex');
    expect(local?.partition.settled.map((entry) => entry.sessionId)).toEqual(['local-1']);
  });
});
