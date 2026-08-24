import {
  partitionSidebarV2Sessions,
  sortSessionsForSidebarV2,
  sortSettledSessionsForSidebarV2,
  sortSnoozedSessionsForSidebarV2,
  type SidebarV2Partition,
} from '../../shared/sidebar-v2-sort';
import {
  SIDEBAR_V2_LIFECYCLE_CAPABILITIES_DISABLED,
  resolveSidebarV2NextWakeAtMs,
  type SidebarV2LifecycleCapabilities,
} from '../../shared/sidebar-v2-lifecycle';
import {
  isSidebarV2BrowserSession,
  toSidebarV2SessionsFromGroup,
  type SidebarV2Session,
} from '../../shared/sidebar-v2-session';
import { firstValidTimestampMs } from '../../shared/sidebar-v2-status';
import {
  DEFAULT_SIDEBAR_V2_PROJECT_GROUPING_SETTINGS,
  deriveSidebarV2ProjectGroupingOverrideKey,
  groupSidebarV2ProjectsByLogicalKey,
  resolveSidebarV2ProjectGroupingMode,
  toSidebarV2Project,
  type SidebarV2Project,
  type SidebarV2ProjectGroupingMode,
  type SidebarV2ProjectGroupingSettings,
} from '../../shared/sidebar-v2-logical-project';
import type { SidebarSessionItem } from '../../shared/session-grid-contract';
import type { WorkspaceProjectIcon } from '../../shared/workspace-project-appearance';
import type { SidebarGroupRecord } from '../sidebar-store';

/*
 * CDXC:SidebarV2 2026-07-29:
 * Derivation layer between the sidebar store's already-filtered display state
 * and the V2 render tree. Everything here is pure so the inbox's ordering and
 * shelf membership can be unit-tested without React, and so the components
 * stay free of data decisions.
 *
 * Hard rule inherited from the mount contract: this module NEVER decides what
 * is visible. `groupIds` / `sessionIdsByGroup` arrive with V1's search and tag
 * filtering already applied; V2 only re-shapes and re-orders them.
 */

/** Inactivity window before an untouched session drops to the Settled shelf.
    Callers pass the user's `sidebarAutoSettleAfterDays`; this is the fallback
    for callers that have no settings yet, and it matches both that setting's
    default and server's `DEFAULT_AUTO_SETTLE_AFTER_DAYS`. */
export const SIDEBAR_V2_DEFAULT_AUTO_SETTLE_DAYS = 3;

export const SIDEBAR_V2_ALL_SCOPE_ID = 'all';

export type SidebarV2ScopeOption = {
  /** Sessions the scope would show, browser rows included. */
  count: number;
  /** CDXC:SidebarV2ProjectIcons 2026-07-29 (discovered icons): the icon the
      project's own repository ships, discovered by gxserver. Ranks below the
      user's chosen icon and above the folder glyph. */
  discoveredIconDataUrl?: string;
  /** null for the "All projects" entry. */
  groupId: string | null;
  /** CDXC:SidebarV2ProjectIcons 2026-07-29: the typed project icon (Tabler
      glyph + color, or an image), which is what most projects actually carry;
      `iconDataUrl` alone would show nearly every project a folder. */
  icon?: WorkspaceProjectIcon;
  iconDataUrl?: string;
  /** Quick/chat collections read as a pseudo-project in the scope menu. */
  isQuick: boolean;
  isWorktree: boolean;
  label: string;
  machineName?: string;
  scopeId: string;
};

export type SidebarV2GroupModel = {
  /** Browser tabs, kept out of the inbox partition: they have no agent
      lifecycle, so settling or snoozing them would be meaningless. */
  browserSessions: SidebarV2Session[];
  /**
   * CDXC:SidebarV2LogicalProjects 2026-07-29:
   * The "Group across machines" submenu is only offered when merging could
   * actually do something: at least one member checkout has a git `origin` to
   * merge ON. A non-git project would get a menu whose every option produced
   * the same single group, which is a promise the data cannot keep.
   */
  canGroupAcrossMachines: boolean;
  /**
   * The mode every member currently resolves to, or `undefined` when the
   * members disagree (possible when the user overrode one machine's copy and
   * not another's) — the submenu then shows no checkmark rather than claiming
   * one of the answers.
   */
  groupingMode?: SidebarV2ProjectGroupingMode;
  /** Physical-checkout override keys for every member, in member order. A
      grouping choice made here is written to ALL of them, because the user is
      acting on the merged group they can see, not on one hidden member. */
  groupingOverrideKeys: string[];
  /** The REPRESENTATIVE host group id (the local checkout when there is one).
      Collapse state, the create button, and the worktree flow all address this
      id, so a merged group behaves exactly like the local project it contains. */
  groupId: string;
  /** CDXC:SidebarV2ProjectIcons 2026-07-29: see `SidebarV2ScopeOption.icon`. */
  icon?: WorkspaceProjectIcon;
  iconDataUrl?: string;
  /** CDXC:SidebarV2ProjectIcons 2026-07-29 (discovered icons): see
      `SidebarV2ScopeOption.discoveredIconDataUrl`. */
  discoveredIconDataUrl?: string;
  isQuick: boolean;
  isStale: boolean;
  isWorktree: boolean;
  /** True when this row merges more than one physical checkout. */
  isMerged: boolean;
  machineName?: string;
  /** Every host group id merged into this one, representative first. */
  memberGroupIds: string[];
  partition: SidebarV2Partition<SidebarV2Session>;
  /**
   * CDXC:SidebarV2LogicalProjects 2026-07-29 (P5 fix round):
   * The repository every member of this row belongs to, or `undefined` when
   * there is none (non-git) or the members disagree. It is what lets a
   * grouping choice act on the whole repository rather than on the one row the
   * user right-clicked: after "Keep separate" has split a repository into
   * several rows, picking "Repository" on ANY of them has to re-merge the set,
   * or the user would have to visit every split row to undo one click.
   */
  repositoryCanonicalKey?: string;
  sessionCount: number;
  title: string;
};

export type SidebarV2ProjectIdentity = Pick<
  SidebarV2GroupModel,
  | 'discoveredIconDataUrl'
  | 'groupId'
  | 'icon'
  | 'iconDataUrl'
  | 'isQuick'
  | 'isStale'
  | 'isWorktree'
  | 'machineName'
  | 'title'
>;

export type SidebarV2ViewModelInput = {
  /** User's `sidebarAutoSettleAfterDays`; `null` disables inactivity
      auto-settle. Omitted only by callers with no settings (tests). */
  autoSettleAfterDays?: number | null;
  /**
   * CDXC:SidebarV2LogicalProjects 2026-07-29:
   * Per-group inactivity window, because the window is a per-DAEMON fact and
   * one inbox mixes daemons. A group present here uses ITS OWN window (`null`
   * meaning "this machine does not inactivity-settle"); a group absent from the
   * map falls back to `autoSettleAfterDays`. The caller resolves machines to
   * groups — this module never guesses which daemon owns a row.
   */
  autoSettleAfterDaysByGroupId?: Readonly<Record<string, number | null>>;
  /**
   * Settle/snooze capability PER GROUP, because capability is a per-daemon
   * fact and one inbox mixes the local daemon with several remote machines
   * (see `SidebarHudState.lifecycleCapabilities`). A group missing from a
   * SUPPLIED map is treated as incapable; omitting the map entirely leaves the
   * predicates ungated, which is what the pure-logic tests exercise.
   */
  capabilitiesByGroupId?: Readonly<Record<string, SidebarV2LifecycleCapabilities>>;
  /** Newest-first first-seen order carried by the caller across renders; the
      fallback ordering for sessions whose host never published `createdAt`. */
  creationOrder: readonly string[];
  groupIds: readonly string[];
  groupsById: Readonly<Record<string, SidebarGroupRecord>>;
  nowMs: number;
  /**
   * CDXC:SidebarV2LogicalProjects 2026-07-29:
   * Which machine counts as "here". Only the representative choice and the
   * machine badges depend on it; omitted, the module uses the module-level
   * local sentinel, which is what a group with no `remoteMachineContext`
   * already resolves to.
   */
  primaryMachineId?: string | null;
  /**
   * Cross-machine merge rules. Omitted, the default rule applies (merge
   * checkouts that share a normalized git origin, never merge anything
   * without one), which is a no-op for every project whose daemon has not
   * published `gitRemoteOriginUrl`.
   */
  projectGrouping?: SidebarV2ProjectGroupingSettings;
  /** `SIDEBAR_V2_ALL_SCOPE_ID` or a group id. Unknown ids fall back to all. */
  scopeId: string;
  sessionIdsByGroup: Readonly<Record<string, readonly string[]>>;
  sessionsById: Readonly<Record<string, SidebarSessionItem>>;
};

export type SidebarV2ViewModel = {
  /** Flat-mode Browser section, newest-first across the active scope. */
  browserSessions: SidebarV2Session[];
  /** Resolved capability per group id, echoed so rows can gate their
      settle/snooze affordances without re-deriving the machine mapping. */
  capabilitiesByGroupId: Readonly<Record<string, SidebarV2LifecycleCapabilities>>;
  /** Flat-mode inbox partition across the active scope. */
  flat: SidebarV2Partition<SidebarV2Session>;
  groups: SidebarV2GroupModel[];
  hasAnySession: boolean;
  /**
   * The soonest moment a snoozed session wakes on its own, across EVERY group
   * (not just the current scope, so switching scope cannot strand a timer).
   * `null` when nothing is snoozed. The root arms one timeout on this instead
   * of polling: the 30s display clock would leave a woken row on the shelf for
   * up to half a minute, and snooze times are second-precise.
   */
  nextWakeAtMs: number | null;
  projectsByGroupId: Readonly<Record<string, SidebarV2ProjectIdentity>>;
  scopeOptions: SidebarV2ScopeOption[];
  /** Group ids the active scope resolves to: every displayed group for "All
      projects", otherwise the one selected project. */
  scopedGroupIds: string[];
};

function resolveProjectIdentity(groupId: string, group: SidebarGroupRecord | undefined): SidebarV2ProjectIdentity {
  return {
    discoveredIconDataUrl: group?.projectContext?.discoveredIconDataUrl,
    groupId,
    icon: group?.projectContext?.icon,
    iconDataUrl: group?.projectContext?.iconDataUrl,
    isQuick: group?.isChatCollection === true,
    isStale: group?.isStale === true,
    isWorktree: Boolean(group?.projectContext?.worktree),
    machineName: group?.remoteMachineContext?.machineName,
    /*
     * CDXC:QuickSessions 2026-05-16-12:55 (V1 precedent):
     * The projectless chat collection is user-facing as "Quick". Keep that
     * label here so the scope menu and card project slot agree with V1.
     */
    title: group?.isChatCollection === true ? 'Quick' : (group?.title ?? groupId),
  };
}

/**
 * Effective creation ordering for one render.
 *
 * `createdAt` wins whenever the host published it, so two clients viewing the
 * same machine agree on the order without sharing client state. Rows without it
 * (older hosts, synthetic rows) fall back to the caller's first-seen registry
 * and sort BELOW every dated row — an unknown creation time is treated as old
 * rather than silently promoted to the top of the inbox.
 */
export function resolveSidebarV2CreationRanks(input: {
  creationOrder: readonly string[];
  sessions: readonly SidebarV2Session[];
}): Map<string, number> {
  const registryIndexById = new Map<string, number>();
  for (const [index, sessionId] of input.creationOrder.entries()) {
    registryIndexById.set(sessionId, index);
  }

  const ranks = new Map<string, number>();
  for (const session of input.sessions) {
    const createdAtMs = firstValidTimestampMs(session.createdAt);
    if (createdAtMs !== null) {
      ranks.set(session.sessionId, createdAtMs);
      continue;
    }
    const registryIndex = registryIndexById.get(session.sessionId);
    ranks.set(session.sessionId, registryIndex === undefined ? Number.NEGATIVE_INFINITY : -1 - registryIndex);
  }
  return ranks;
}

/**
 * Merges per-group partitions into one inbox. Each shelf is re-sorted with its
 * own rule over the merged set rather than concatenated, so the flat list is
 * identical to what a single-pass partition would produce when every group has
 * the same capability.
 */
function mergeSidebarV2Partitions(
  partitions: readonly SidebarV2Partition<SidebarV2Session>[],
  options: { creationRankById: ReadonlyMap<string, number> }
): SidebarV2Partition<SidebarV2Session> {
  return {
    active: sortSessionsForSidebarV2(
      partitions.flatMap((partition) => partition.active),
      { creationRankById: options.creationRankById }
    ),
    settled: sortSettledSessionsForSidebarV2(partitions.flatMap((partition) => partition.settled)),
    snoozed: sortSnoozedSessionsForSidebarV2(partitions.flatMap((partition) => partition.snoozed)),
  };
}

export function createSidebarV2ViewModel(input: SidebarV2ViewModelInput): SidebarV2ViewModel {
  const projectsByGroupId: Record<string, SidebarV2ProjectIdentity> = {};
  const sessionsByGroupId = new Map<string, SidebarV2Session[]>();
  const allSessions: SidebarV2Session[] = [];

  for (const groupId of input.groupIds) {
    const group = input.groupsById[groupId];
    projectsByGroupId[groupId] = resolveProjectIdentity(groupId, group);
    const sessions = toSidebarV2SessionsFromGroup({
      groupId,
      sessions: (input.sessionIdsByGroup[groupId] ?? [])
        .map((sessionId) => input.sessionsById[sessionId])
        .filter((session): session is SidebarSessionItem => session !== undefined),
    });
    sessionsByGroupId.set(groupId, sessions);
    allSessions.push(...sessions);
  }

  const creationRankById = resolveSidebarV2CreationRanks({
    creationOrder: input.creationOrder,
    sessions: allSessions,
  });
  const fallbackAutoSettleAfterDays =
    input.autoSettleAfterDays === undefined ? SIDEBAR_V2_DEFAULT_AUTO_SETTLE_DAYS : input.autoSettleAfterDays;
  /*
   * CDXC:SidebarV2LogicalProjects 2026-07-29:
   * The window is looked up per group, not applied once to the whole inbox.
   * `null` is a real answer ("this machine never inactivity-settles") and must
   * survive the lookup, so absence is tested with `in` rather than with `??`.
   */
  const resolveAutoSettleAfterDays = (groupId: string): number | null => {
    const windows = input.autoSettleAfterDaysByGroupId;
    if (windows && Object.hasOwn(windows, groupId)) {
      return windows[groupId] ?? null;
    }
    return fallbackAutoSettleAfterDays;
  };
  const partitionOptions = {
    creationRankById,
    nowMs: input.nowMs,
  };
  /*
   * CDXC:SidebarV2Lifecycle 2026-07-29:
   * Capability is resolved per group and the partition runs PER GROUP, then the
   * flat inbox merges the group partitions. Partitioning the flat list in one
   * pass would force one capability answer onto rows from several daemons, and
   * an un-upgraded remote machine would get settle/snooze classifications it
   * cannot serve. Merging is lossless because each shelf's sort is a total
   * order over the merged set, not a stable merge of pre-sorted runs.
   */
  const capabilitiesByGroupId: Record<string, SidebarV2LifecycleCapabilities> = {};
  const resolveGroupCapabilities = (groupId: string): SidebarV2LifecycleCapabilities | undefined => {
    if (input.capabilitiesByGroupId === undefined) {
      return undefined;
    }
    return input.capabilitiesByGroupId[groupId] ?? SIDEBAR_V2_LIFECYCLE_CAPABILITIES_DISABLED;
  };

  const partitionsByGroupId = new Map<string, SidebarV2Partition<SidebarV2Session>>();
  const browserSessionsByGroupId = new Map<string, SidebarV2Session[]>();
  for (const groupId of input.groupIds) {
    const sessions = sessionsByGroupId.get(groupId) ?? [];
    const browserSessions: SidebarV2Session[] = [];
    const inboxSessions: SidebarV2Session[] = [];
    for (const session of sessions) {
      (isSidebarV2BrowserSession(session) ? browserSessions : inboxSessions).push(session);
    }
    const capabilities = resolveGroupCapabilities(groupId);
    capabilitiesByGroupId[groupId] = capabilities ?? { settle: true, snooze: true };
    partitionsByGroupId.set(
      groupId,
      partitionSidebarV2Sessions(inboxSessions, {
        ...partitionOptions,
        autoSettleAfterDays: resolveAutoSettleAfterDays(groupId),
        capabilities,
      })
    );
    browserSessionsByGroupId.set(groupId, browserSessions);
  }

  /*
   * CDXC:SidebarV2LogicalProjects 2026-07-29:
   * ── cross-machine merge ───────────────────────────────────────────────────
   * Checkouts of ONE repository — this Mac's clone, a remote machine's clone,
   * and any duplicate local clone — collapse into one logical project here,
   * AFTER every per-group decision (capability, window, partition) has already
   * been made against the owning daemon. Merging earlier would force one
   * daemon's answers onto another machine's rows, which is exactly the bug the
   * per-group partition exists to avoid.
   *
   * The merged row is addressed by its REPRESENTATIVE group id (the local
   * member when there is one), so collapse state, the create button, and the
   * worktree flow keep operating on a real host group that can actually serve
   * them. `memberGroupIds` carries the rest for anything that needs the whole
   * set.
   */
  const projectGrouping = input.projectGrouping ?? DEFAULT_SIDEBAR_V2_PROJECT_GROUPING_SETTINGS;
  const logicalProjectsByGroupId = new Map<string, SidebarV2Project>();
  for (const groupId of input.groupIds) {
    const group = input.groupsById[groupId];
    logicalProjectsByGroupId.set(
      groupId,
      toSidebarV2Project({
        groupId,
        projectContext: group?.projectContext,
        remoteMachineContext: group?.remoteMachineContext,
        title: projectsByGroupId[groupId]?.title ?? groupId,
      })
    );
  }
  const logicalGroups = groupSidebarV2ProjectsByLogicalKey({
    primaryMachineId: input.primaryMachineId,
    projects: input.groupIds.flatMap((groupId) => {
      const project = logicalProjectsByGroupId.get(groupId);
      return project ? [project] : [];
    }),
    settings: projectGrouping,
  });

  const groups: SidebarV2GroupModel[] = logicalGroups.map((logicalGroup) => {
    const memberGroupIds = [
      logicalGroup.representative.projectId,
      ...logicalGroup.members
        .map((member) => member.projectId)
        .filter((projectId) => projectId !== logicalGroup.representative.projectId),
    ];
    const memberModes = logicalGroup.members.map((member) =>
      resolveSidebarV2ProjectGroupingMode(member, projectGrouping)
    );
    const sharedMode = memberModes.every((mode) => mode === memberModes[0]) ? memberModes[0] : undefined;
    const representativeGroupId = logicalGroup.representative.projectId;
    const memberRepositoryKeys = new Set(
      logicalGroup.members.flatMap((member) => {
        const canonicalKey = member.repository?.canonicalKey?.trim();
        return canonicalKey ? [canonicalKey] : [];
      })
    );
    const [repositoryCanonicalKey] = memberRepositoryKeys.size === 1 ? [...memberRepositoryKeys] : [];
    /*
     * The project line inside a merged group shows the SHARED name, so a flat
     * inbox card never claims a session belongs to "ghostex" on one row and
     * "ghostex-copy" on the next when both are the same repository. The machine
     * badge, resolved per session from its own host group, keeps them apart.
     */
    for (const memberGroupId of memberGroupIds) {
      const identity = projectsByGroupId[memberGroupId];
      if (identity) {
        projectsByGroupId[memberGroupId] = { ...identity, title: logicalGroup.displayName };
      }
    }
    return {
      ...projectsByGroupId[representativeGroupId]!,
      /*
       * CDXC:ProjectBrowserTabs 2026-05-16-12:59 (V1 precedent):
       * Browser rows render above the agent/terminal rows inside a project.
       * Keep the store's incoming order for them instead of re-sorting, so
       * grouped mode reads exactly like the classic sidebar.
       */
      browserSessions: memberGroupIds.flatMap((memberGroupId) => browserSessionsByGroupId.get(memberGroupId) ?? []),
      canGroupAcrossMachines: logicalGroup.members.some((member) => member.repository !== undefined),
      groupId: representativeGroupId,
      ...(sharedMode ? { groupingMode: sharedMode } : {}),
      groupingOverrideKeys: logicalGroup.members.map((member) => deriveSidebarV2ProjectGroupingOverrideKey(member)),
      isMerged: memberGroupIds.length > 1,
      memberGroupIds,
      partition: mergeSidebarV2Partitions(
        memberGroupIds.flatMap((memberGroupId) => {
          const partition = partitionsByGroupId.get(memberGroupId);
          return partition ? [partition] : [];
        }),
        { creationRankById }
      ),
      ...(repositoryCanonicalKey ? { repositoryCanonicalKey } : {}),
      sessionCount: memberGroupIds.reduce(
        (total, memberGroupId) => total + (sessionsByGroupId.get(memberGroupId) ?? []).length,
        0
      ),
      title: logicalGroup.displayName,
    };
  });

  /*
   * A scope selects a LOGICAL project, addressed by its representative id.
   * Resolving through membership rather than by equality also survives a
   * re-group: when an override splits or merges projects under a scope the user
   * already picked, the scope still resolves to whichever logical group now
   * contains it instead of silently falling back to "All projects".
   */
  const scopedLogicalGroup =
    input.scopeId === SIDEBAR_V2_ALL_SCOPE_ID
      ? undefined
      : groups.find((group) => group.memberGroupIds.includes(input.scopeId));
  const scopedGroupIds = scopedLogicalGroup ? [...scopedLogicalGroup.memberGroupIds] : [...input.groupIds];
  const scopedGroupIdSet = new Set(scopedGroupIds);
  const scopedSessions = allSessions.filter(
    (session) => session.projectId !== undefined && scopedGroupIdSet.has(session.projectId)
  );
  const scopedBrowserSessions = scopedSessions.filter((session) => isSidebarV2BrowserSession(session));
  const scopedPartitions = scopedGroupIds.flatMap((groupId) => {
    const partition = partitionsByGroupId.get(groupId);
    return partition ? [partition] : [];
  });

  /*
   * The wake boundary spans every group, not the scope: a snoozed row in a
   * project the user is not currently looking at still has to wake on time,
   * because the scope can flip at any moment and the shelf counts are visible.
   */
  let nextWakeAtMs: number | null = null;
  for (const groupId of input.groupIds) {
    const capabilities = resolveGroupCapabilities(groupId);
    for (const session of partitionsByGroupId.get(groupId)?.snoozed ?? []) {
      const wakeAtMs = resolveSidebarV2NextWakeAtMs(session, {
        capabilities,
        nowMs: input.nowMs,
      });
      if (wakeAtMs !== null && (nextWakeAtMs === null || wakeAtMs < nextWakeAtMs)) {
        nextWakeAtMs = wakeAtMs;
      }
    }
  }

  return {
    browserSessions: sortSessionsForSidebarV2(scopedBrowserSessions, { creationRankById }),
    capabilitiesByGroupId,
    flat: mergeSidebarV2Partitions(scopedPartitions, { creationRankById }),
    groups,
    hasAnySession: allSessions.length > 0,
    nextWakeAtMs,
    projectsByGroupId,
    scopeOptions: [
      {
        count: allSessions.length,
        groupId: null,
        isQuick: false,
        isWorktree: false,
        label: 'All projects',
        scopeId: SIDEBAR_V2_ALL_SCOPE_ID,
      },
      /*
       * CDXC:SidebarV2LogicalProjects 2026-07-29:
       * The scope filter lists LOGICAL projects: one entry per merged group,
       * counting every member's sessions. Listing physical checkouts here
       * would offer the user two entries that the grouped view already shows
       * as one thing.
       */
      ...groups.map((group) => ({
        count: group.sessionCount,
        discoveredIconDataUrl: group.discoveredIconDataUrl,
        groupId: group.groupId,
        icon: group.icon,
        iconDataUrl: group.iconDataUrl,
        isQuick: group.isQuick,
        isWorktree: group.isWorktree,
        label: group.title,
        machineName: group.machineName,
        scopeId: group.groupId,
      })),
    ],
    scopedGroupIds,
  };
}
