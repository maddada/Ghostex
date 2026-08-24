/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import {
  GPUI_COMMAND_PANE_SESSION_STRING_MAX_LENGTH,
  GPUI_COMMAND_PANE_SESSION_SUMMARY_LIMIT,
  GPUI_COMMAND_PANE_TIMER_DEADLINE_MAX_LENGTH,
  GPUI_COMMAND_PANE_TIMER_LABEL_MAX_LENGTH,
  GPUI_COMMAND_PANE_TIMER_REMAINING_MS_MAX,
  GPUI_DEFAULT_VISIBLE_COUNT,
  GPUI_GXSERVER_LOCAL_COMMAND_PANE_SESSION_ID_PATTERN,
} from '../constants';
import type {
  GpuiCommandPaneSessionSummary,
  GpuiSidebarCommandSessionIndicatorScope,
  GpuiSidebarRuntimeSettings,
  GpuiWorkspaceSessionDelayedSendSummary,
} from '../types-and-protocol';
import { createGpuiSidebarSettings } from './bootstrap';
import { createGpuiProjectSettingsProjects } from './presentation-projection';
import {
  compareGpuiRecentProjectsByClosedAt,
  createGpuiRecentProjects,
  createGpuiRemoteRecentProjects,
} from './recent-projects';
import { getCompletionSoundLabel } from '@/packages/shared/completion-sound';
import {
  gxserverPresentationSidebarAutoSettleAfterDays,
  gxserverPresentationSidebarLifecycleCapabilities,
  parseGxserverPresentationProjectSessionId,
} from '@/packages/shared/gxserver-presentation-sidebar-projection';
import type {
  GxserverPresentationSnapshot,
  GxserverProjectDomainState,
  GxserverRecentProjectDomainState,
  GxserverSidebarHudResponse,
} from '@/packages/shared/gxserver-protocol';
import type {
  SidebarCommandSessionIndicator,
  SidebarHudState,
  SidebarSessionGroup,
} from '@/packages/shared/session-grid-contract';
import { resolveSidebarTheme } from '@/packages/shared/session-grid-contract';
import type { SidebarAgentButton } from '@/packages/shared/sidebar-agents';
import { createSidebarAgentButtons } from '@/packages/shared/sidebar-agents';
import type { SidebarCommandButton } from '@/packages/shared/sidebar-commands';
import { createSidebarCommandButtons } from '@/packages/shared/sidebar-commands';
import type { SidebarGitState } from '@/packages/shared/sidebar-git';
import { createDefaultSidebarGitState } from '@/packages/shared/sidebar-git';

export function normalizeGpuiWorkspaceSessionDelayedSends(
  sessions: readonly GpuiWorkspaceSessionDelayedSendSummary[] | unknown
): GpuiWorkspaceSessionDelayedSendSummary[] {
  if (!Array.isArray(sessions)) {
    return [];
  }
  return sessions.slice(0, GPUI_COMMAND_PANE_SESSION_SUMMARY_LIMIT).flatMap((session) => {
    if (!session || typeof session !== 'object') {
      return [];
    }
    const record = session as Partial<Record<keyof GpuiWorkspaceSessionDelayedSendSummary, unknown>>;
    const sessionId = normalizeGpuiCommandPaneSessionString(record.sessionId);
    if (!sessionId || !parseGxserverPresentationProjectSessionId(sessionId)) {
      return [];
    }
    const delayedSendDeadlineAt = normalizeGpuiCommandPaneTimerDeadlineAt(record.delayedSendDeadlineAt);
    const delayedSendRemainingLabel = normalizeGpuiWorkspaceDelayedSendRemainingLabel(record.delayedSendRemainingLabel);
    const delayedSendRemainingMs = normalizeGpuiCommandPaneTimerRemainingMs(record.delayedSendRemainingMs);
    const sendWhenAllProjectSessionsStopActive = record.sendWhenAllProjectSessionsStopActive === true;
    const sendWhenAgentStopsActive = record.sendWhenAgentStopsActive === true;
    if (
      !delayedSendDeadlineAt &&
      !delayedSendRemainingLabel &&
      delayedSendRemainingMs === undefined &&
      !sendWhenAllProjectSessionsStopActive &&
      !sendWhenAgentStopsActive
    ) {
      return [];
    }
    return [
      {
        ...(delayedSendDeadlineAt ? { delayedSendDeadlineAt } : {}),
        ...(delayedSendRemainingLabel ? { delayedSendRemainingLabel } : {}),
        ...(delayedSendRemainingMs !== undefined ? { delayedSendRemainingMs } : {}),
        ...(sendWhenAllProjectSessionsStopActive ? { sendWhenAllProjectSessionsStopActive: true } : {}),
        ...(sendWhenAgentStopsActive ? { sendWhenAgentStopsActive: true } : {}),
        sessionId,
      },
    ];
  });
}

export function normalizeGpuiWorkspaceDelayedSendRemainingLabel(value: unknown): string | undefined {
  if (value === 'Waiting for agent' || value === 'Waiting for agents') {
    return value;
  }
  return normalizeGpuiCommandPaneTimerRemainingLabel(value);
}

export function normalizeGpuiCommandPaneSessions(
  sessions: readonly GpuiCommandPaneSessionSummary[] | unknown
): GpuiCommandPaneSessionSummary[] {
  if (!Array.isArray(sessions)) {
    return [];
  }
  return sessions.slice(0, GPUI_COMMAND_PANE_SESSION_SUMMARY_LIMIT).flatMap((session) => {
    if (!session || typeof session !== 'object') {
      return [];
    }
    const record = session as Partial<Record<keyof GpuiCommandPaneSessionSummary, unknown>>;
    const sessionId = normalizeGpuiCommandPaneSessionString(record.sessionId);
    const status = normalizeGpuiCommandPaneSessionStatus(record.status);
    if (!sessionId || !status || !isGpuiGxserverLocalCommandPaneSessionId(sessionId)) {
      return [];
    }
    const commandId = normalizeGpuiCommandPaneSessionString(record.commandId);
    const title = normalizeGpuiCommandPaneSessionString(record.title);
    const delayedSendDeadlineAt = normalizeGpuiCommandPaneTimerDeadlineAt(record.delayedSendDeadlineAt);
    const delayedSendRemainingLabel = normalizeGpuiCommandPaneTimerRemainingLabel(record.delayedSendRemainingLabel);
    const delayedSendRemainingMs = normalizeGpuiCommandPaneTimerRemainingMs(record.delayedSendRemainingMs);
    const closeAfterDoneDeadlineAt = normalizeGpuiCommandPaneTimerDeadlineAt(record.closeAfterDoneDeadlineAt);
    const closeAfterDoneRemainingLabel = normalizeGpuiCommandPaneTimerRemainingLabel(
      record.closeAfterDoneRemainingLabel
    );
    const closeAfterDoneRemainingMs = normalizeGpuiCommandPaneTimerRemainingMs(record.closeAfterDoneRemainingMs);
    return [
      {
        ...(commandId ? { commandId } : {}),
        /*
        CDXC:GPUICommandPaneTimers 2026-06-27-02:05:
        Native Rust emits command-pane timer summaries with only Delayed Send and Close After Done display fields. Keep the TypeScript bridge at the same privacy boundary by normalizing and forwarding just bounded timer strings, non-negative remaining milliseconds, and a true-only Close After Done flag; never pass command text, cwd/env, URLs, paths, output, run ids, status-file paths, tokens, or unknown native fields into the Sidebar HUD.
        */
        ...(record.closeAfterDone === true ? { closeAfterDone: true } : {}),
        ...(closeAfterDoneDeadlineAt ? { closeAfterDoneDeadlineAt } : {}),
        ...(closeAfterDoneRemainingLabel ? { closeAfterDoneRemainingLabel } : {}),
        ...(closeAfterDoneRemainingMs !== undefined ? { closeAfterDoneRemainingMs } : {}),
        ...(delayedSendDeadlineAt ? { delayedSendDeadlineAt } : {}),
        ...(delayedSendRemainingLabel ? { delayedSendRemainingLabel } : {}),
        ...(delayedSendRemainingMs !== undefined ? { delayedSendRemainingMs } : {}),
        ...(record.isActive === true ? { isActive: true } : {}),
        ...(record.isPaneOwner === true ? { isPaneOwner: true } : {}),
        sessionId,
        status,
        ...(title ? { title } : {}),
      },
    ];
  });
}

export function normalizeGpuiCommandPaneSessionString(value: unknown): string | undefined {
  if (typeof value !== 'string') {
    return undefined;
  }
  const normalized = value.trim().replace(/\s+/g, ' ');
  if (
    !normalized ||
    normalized.length > GPUI_COMMAND_PANE_SESSION_STRING_MAX_LENGTH ||
    /[\u0000-\u001F\u007F]/.test(normalized)
  ) {
    return undefined;
  }
  return normalized;
}

export function normalizeGpuiCommandPaneTimerDeadlineAt(value: unknown): string | undefined {
  if (typeof value !== 'string') {
    return undefined;
  }
  const normalized = value.trim();
  if (
    !normalized ||
    normalized.length > GPUI_COMMAND_PANE_TIMER_DEADLINE_MAX_LENGTH ||
    /[\u0000-\u001F\u007F]/.test(normalized) ||
    !/^\d{4}-\d{2}-\d{2}T/u.test(normalized) ||
    Number.isNaN(Date.parse(normalized))
  ) {
    return undefined;
  }
  return normalized;
}

export function normalizeGpuiCommandPaneTimerRemainingLabel(value: unknown): string | undefined {
  if (typeof value !== 'string') {
    return undefined;
  }
  const normalized = value.trim().replace(/\s+/g, ' ');
  if (
    !normalized ||
    normalized.length > GPUI_COMMAND_PANE_TIMER_LABEL_MAX_LENGTH ||
    /[\u0000-\u001F\u007F]/.test(normalized) ||
    !/^[0-9dhms: .+-]+$/iu.test(normalized)
  ) {
    return undefined;
  }
  return normalized;
}

export function normalizeGpuiCommandPaneTimerRemainingMs(value: unknown): number | undefined {
  if (
    typeof value !== 'number' ||
    !Number.isFinite(value) ||
    value < 0 ||
    value > GPUI_COMMAND_PANE_TIMER_REMAINING_MS_MAX
  ) {
    return undefined;
  }
  return Math.ceil(value);
}

export function normalizeGpuiCommandPaneSessionStatus(
  value: unknown
): SidebarCommandSessionIndicator['status'] | undefined {
  return isValidGpuiCommandPaneSessionStatus(value) ? value : undefined;
}

export function isValidGpuiCommandPaneSessionStatus(value: unknown): value is SidebarCommandSessionIndicator['status'] {
  return value === 'idle' || value === 'running' || value === 'error';
}

export function hasSameGpuiCommandPaneSessions(
  current: readonly GpuiCommandPaneSessionSummary[],
  next: readonly GpuiCommandPaneSessionSummary[]
): boolean {
  if (current.length !== next.length) {
    return false;
  }
  return current.every((session, index) => {
    const candidate = next[index];
    return (
      session.commandId === candidate?.commandId &&
      session.closeAfterDone === candidate?.closeAfterDone &&
      session.closeAfterDoneDeadlineAt === candidate?.closeAfterDoneDeadlineAt &&
      session.closeAfterDoneRemainingLabel === candidate?.closeAfterDoneRemainingLabel &&
      session.closeAfterDoneRemainingMs === candidate?.closeAfterDoneRemainingMs &&
      session.delayedSendDeadlineAt === candidate?.delayedSendDeadlineAt &&
      session.delayedSendRemainingLabel === candidate?.delayedSendRemainingLabel &&
      session.delayedSendRemainingMs === candidate?.delayedSendRemainingMs &&
      session.isActive === candidate?.isActive &&
      session.isPaneOwner === candidate?.isPaneOwner &&
      session.sessionId === candidate?.sessionId &&
      session.status === candidate?.status &&
      session.title === candidate?.title
    );
  });
}

export function isGpuiGxserverLocalCommandPaneSessionId(sessionId: unknown): sessionId is string {
  /*
  CDXC:GPUICommandPane 2026-06-27-01:37:
  GPUI command-pane summaries are live local tab state for gxserver-backed native-shaped `G...` command sessions only. Rust shell internals may still carry numeric ids, so drop raw numeric strings, lowercase `g...`, malformed strings, and non-string rows at the bridge boundary before stale native-local command tabs can drive HUD indicators, active-tab state, timer projection, or auto-sleep protection.
  */
  return typeof sessionId === 'string' && GPUI_GXSERVER_LOCAL_COMMAND_PANE_SESSION_ID_PATTERN.test(sessionId);
}

export function filterGpuiGxserverLocalCommandPaneSessions(
  commandPaneSessions: readonly GpuiCommandPaneSessionSummary[],
  scope: GpuiSidebarCommandSessionIndicatorScope = {}
): GpuiCommandPaneSessionSummary[] {
  /*
  CDXC:GPUICommandPane 2026-06-27-08:32:
  Command-pane ownership consumers require both an external native-shaped local `G...` id and a valid Sidebar HUD status. Reuse this filter for HUD indicators and Auto Sleep owner protection so malformed native rows, including `isPaneOwner:true` rows with invalid status, cannot keep sessions awake.

  CDXC:GPUICommandPane 2026-06-27-08:45:
  Native presentation cleanup removes stale command-panel rows after authoritative gxserver snapshots and explicit removal deltas. When the live HUD is built with an active project and presentation, require the command-pane summary id to still exist in that active project so deleted local `G...` tabs cannot keep Action indicators, timers, or active states visible.
  */
  const presentedSessionIds =
    scope.activeProjectId && scope.presentation
      ? new Set<string>(
          scope.presentation.sessions.flatMap((session) =>
            session.projectId === scope.activeProjectId ? [session.sessionId] : []
          )
        )
      : undefined;
  return commandPaneSessions.filter((session) => {
    if (
      !isGpuiGxserverLocalCommandPaneSessionId(session.sessionId) ||
      !isValidGpuiCommandPaneSessionStatus(session.status)
    ) {
      return false;
    }
    return presentedSessionIds ? presentedSessionIds.has(session.sessionId) : true;
  });
}

export function createGpuiSidebarCommandSessionIndicators(
  commands: readonly SidebarCommandButton[],
  commandPaneSessions: readonly GpuiCommandPaneSessionSummary[],
  scope: GpuiSidebarCommandSessionIndicatorScope = {}
): SidebarCommandSessionIndicator[] {
  /*
  CDXC:GPUICommandPane 2026-06-27-06:30:
  Command-session HUD status is owned by Rust's sanitized command-pane summary. The TypeScript bridge may forward only external native-shaped local `G...` command-pane rows whose status is already a Sidebar HUD status; internal Rust numeric shell ids and malformed bridge rows must not match HUD Actions or infer status from renderer activity, command text, paths, URLs, output, logs, titles, status files, or other private fields.

  CDXC:GPUICommandPane 2026-06-27-08:45:
  Keep the exported helper backward-compatible for direct two-argument tests and callers. Live HUD construction passes the optional active-project presentation scope so stale command-pane summaries are pruned against the full current presentation, not against whichever ids happen to appear in a non-removal delta.
  */
  const localCommandPaneSessions = filterGpuiGxserverLocalCommandPaneSessions(commandPaneSessions, scope);
  return commands.flatMap((command) => {
    if (command.actionType !== 'terminal') {
      return [];
    }
    const commandTitleKey = getGpuiSidebarCommandTitleKey(getGpuiSidebarCommandSessionTitle(command));
    if (!commandTitleKey) {
      return [];
    }
    const mappedSession = localCommandPaneSessions.find(
      (session) =>
        session.commandId === command.commandId && getGpuiSidebarCommandTitleKey(session.title) === commandTitleKey
    );
    const session =
      mappedSession ??
      localCommandPaneSessions.find((candidate) => getGpuiSidebarCommandTitleKey(candidate.title) === commandTitleKey);
    if (!session) {
      return [];
    }
    return [
      {
        commandId: command.commandId,
        ...(session.closeAfterDone === true ? { closeAfterDone: true } : {}),
        ...(session.closeAfterDoneDeadlineAt
          ? {
              closeAfterDoneDeadlineAt: session.closeAfterDoneDeadlineAt,
            }
          : {}),
        ...(session.closeAfterDoneRemainingLabel
          ? {
              closeAfterDoneRemainingLabel: session.closeAfterDoneRemainingLabel,
            }
          : {}),
        ...(session.closeAfterDoneRemainingMs !== undefined
          ? {
              closeAfterDoneRemainingMs: session.closeAfterDoneRemainingMs,
            }
          : {}),
        ...(session.delayedSendDeadlineAt
          ? {
              delayedSendDeadlineAt: session.delayedSendDeadlineAt,
            }
          : {}),
        ...(session.delayedSendRemainingLabel
          ? {
              delayedSendRemainingLabel: session.delayedSendRemainingLabel,
            }
          : {}),
        ...(session.delayedSendRemainingMs !== undefined
          ? {
              delayedSendRemainingMs: session.delayedSendRemainingMs,
            }
          : {}),
        isActive: session.isActive === true,
        sessionId: session.sessionId,
        status: session.status,
        ...(session.title ? { title: session.title } : {}),
      },
    ];
  });
}

export function getGpuiSidebarCommandSessionTitle(command: SidebarCommandButton): string {
  const normalizedActionName = command.name.trim();
  return normalizedActionName.length > 0 ? normalizedActionName : (command.command ?? '').trim().slice(0, 20);
}

export function getGpuiSidebarCommandTitleKey(value: string | undefined): string {
  return normalizeGpuiCommandPaneSessionString(value)?.toLocaleLowerCase() ?? '';
}

export function createGpuiSidebarHudState({
  activeProjectId,
  commandPaneSessions = [],
  domainProjects = [],
  focusedSessionId,
  git,
  groups = [],
  presentation,
  recentProjects = [],
  remoteRecentProjectsByMachineId,
  remotePresentationsByMachineId,
  runtimeSettings,
  sidebarHud,
}: {
  activeProjectId?: string;
  commandPaneSessions?: readonly GpuiCommandPaneSessionSummary[];
  domainProjects?: readonly GxserverProjectDomainState[];
  focusedSessionId?: string;
  git?: SidebarGitState;
  groups?: readonly SidebarSessionGroup[];
  presentation?: GxserverPresentationSnapshot;
  recentProjects?: readonly GxserverRecentProjectDomainState[];
  remoteRecentProjectsByMachineId?: ReadonlyMap<string, readonly GxserverRecentProjectDomainState[]>;
  remotePresentationsByMachineId?: ReadonlyMap<string, GxserverPresentationSnapshot>;
  runtimeSettings?: GpuiSidebarRuntimeSettings;
  sidebarHud?: GxserverSidebarHudResponse;
} = {}): SidebarHudState {
  const settings = createGpuiSidebarSettings(runtimeSettings);
  /*
   * CDXC:SidebarHudContract 2026-06-24-20:34:
   * GPUI SidebarApp uses gxserver's `/api/readSidebarHud` projection for read-side agent/action buttons so live sidebar and app-modal Settings share one production contract. The local shared defaults are only for pre-bootstrap or unavailable gxserver state; project metadata is not re-normalized here.
   */
  const agents = sidebarHud ? ([...sidebarHud.agents] as SidebarAgentButton[]) : createSidebarAgentButtons([], []);
  /*
   * CDXC:ProjectActions 2026-08-01:
   * `showOnProjectRow` is optional on the gxserver contract because a daemon
   * older than the app drops fields it does not know, so a legacy response
   * yields `undefined` where SidebarCommandButton promises a boolean. Normalize
   * at the surface boundary instead of casting the gap away, so row rendering
   * and the Settings toggle both see a real boolean.
   */
  const normalizeHudCommands = (
    hudCommands: readonly GxserverSidebarHudResponse['commands'][number][]
  ): ReturnType<typeof createSidebarCommandButtons> =>
    hudCommands.map((command) => ({
      ...command,
      showOnProjectRow: command.showOnProjectRow === true,
    })) as ReturnType<typeof createSidebarCommandButtons>;
  const commands = sidebarHud ? normalizeHudCommands(sidebarHud.commands) : createSidebarCommandButtons([], [], []);
  /*
   * CDXC:GlobalActions 2026-08-01:
   * `globalCommands` is optional on the gxserver contract because a daemon
   * older than the app drops fields it does not know. Normalize the gap to an
   * empty list here, at the surface boundary, so Settings renders an empty
   * Global Actions section instead of failing on undefined.
   */
  const globalCommands = (
    sidebarHud?.globalCommands ? normalizeHudCommands(sidebarHud.globalCommands) : []
  ) as ReturnType<typeof createSidebarCommandButtons>;
  const commandsByProject = sidebarHud?.commandsByProject
    ? Object.fromEntries(
        Object.entries(sidebarHud.commandsByProject).map(([projectId, projectCommands]) => [
          projectId,
          normalizeHudCommands(projectCommands),
        ])
      )
    : undefined;
  const focusedSession = groups
    .flatMap((group) => group.sessions)
    .find(
      (session) =>
        parseGxserverPresentationProjectSessionId(session.sessionId)?.sessionId === focusedSessionId ||
        session.sessionId === focusedSessionId
    );
  const visibleSessions = groups.flatMap((group) => group.sessions.filter((session) => session.isVisible));
  return {
    activeSessionsSortMode: 'lastActivity',
    agentManagerZoomPercent: settings.agentManagerZoomPercent,
    agents,
    commands,
    ...(commandsByProject ? { commandsByProject } : {}),
    commandSessionIndicators: createGpuiSidebarCommandSessionIndicators(commands, commandPaneSessions, {
      activeProjectId,
      presentation,
    }),
    completionBellEnabled: settings.completionBellEnabled,
    completionSound: settings.completionSound,
    completionSoundLabel: getCompletionSoundLabel(settings.completionSound),
    debuggingMode: settings.debuggingMode,
    focusedSessionTitle: focusedSession?.displayTitle ?? focusedSession?.primaryTitle ?? focusedSession?.alias,
    git: git ?? createDefaultSidebarGitState(),
    globalCommands,
    highlightedVisibleCount: GPUI_DEFAULT_VISIBLE_COUNT,
    isFocusModeActive: false,
    /*
    CDXC:SidebarV2Lifecycle 2026-07-29:
    Settle/snooze capability is per daemon, and GPUI holds one presentation
    snapshot per daemon: the local gxserver plus `remotePresentations` keyed by
    machine id. Publish them separately so the sidebar can gate a remote
    machine's rows on that machine's own answer. A snapshot with no
    `capabilities` block (an older daemon) projects to `undefined`, which the
    sidebar reads as "no lifecycle" and hides the affordances — never as
    "assume it works".

    CDXC:SidebarV2Git 2026-07-29:
    The per-session git/PR probe rides the SAME block (`sessionGitStatus`) and
    the same two paths, so a remote machine whose daemon predates the probe
    renders plain cards while the local one shows branch/PR lines. The git data
    itself needs no plumbing here: it lives on the presentation session and
    reaches the sidebar through the existing snapshot/delta projection.
    */
    lifecycleCapabilities: gxserverPresentationSidebarLifecycleCapabilities(presentation),
    lifecycleCapabilitiesByMachineId: Object.fromEntries(
      [...(remotePresentationsByMachineId ?? new Map())].flatMap(([machineId, remotePresentation]) => {
        const capabilities = gxserverPresentationSidebarLifecycleCapabilities(remotePresentation);
        return capabilities ? [[machineId, capabilities] as const] : [];
      })
    ),
    /*
    CDXC:SidebarV2LogicalProjects 2026-07-29:
    The auto-settle WINDOW travels the same two paths as the capability block
    above, and for the same reason: each daemon runs its own sweep against its
    own `sidebarAutoSettleAfterDays`, so the local user's window is not an
    answer for a remote machine. A daemon that omits the field is left OUT of
    the map entirely rather than defaulted, because "absent" and "null" mean
    different things to the sidebar (fall back to the local setting vs. do not
    inactivity-settle at all).
    */
    autoSettleAfterDays: gxserverPresentationSidebarAutoSettleAfterDays(presentation),
    autoSettleAfterDaysByMachineId: Object.fromEntries(
      [...(remotePresentationsByMachineId ?? new Map())].flatMap(([machineId, remotePresentation]) => {
        const autoSettleAfterDays = gxserverPresentationSidebarAutoSettleAfterDays(remotePresentation);
        return autoSettleAfterDays === undefined ? [] : [[machineId, autoSettleAfterDays] as const];
      })
    ),
    pendingAgentIds: [],
    projectSettingsProjects: createGpuiProjectSettingsProjects(domainProjects, presentation),
    /*
    CDXC:GPUIRecentProjects 2026-06-24-12:27:
    GPUI Recent Projects hydrate from `/api/listRecentProjects`, a
    gxserver-owned parked-project contract. Keep an empty drawer when the
    endpoint has no explicit rows; never derive recent projects from labels,
    inactive sessions, presentation titles, command text, or path guessing.
    */
    recentProjects: [
      ...createGpuiRecentProjects(recentProjects, settings),
      ...createGpuiRemoteRecentProjects(remoteRecentProjectsByMachineId, remotePresentationsByMachineId, settings),
    ].sort(compareGpuiRecentProjectsByClosedAt),
    settings,
    createSessionOnSidebarDoubleClick: settings.createSessionOnSidebarDoubleClick,
    renameSessionOnDoubleClick: settings.renameSessionOnDoubleClick,
    showCloseButtonOnSessionCards: settings.showCloseButtonOnSessionCards,
    theme: resolveSidebarTheme(settings.sidebarTheme, 'dark'),
    viewMode: 'grid',
    visibleCount: GPUI_DEFAULT_VISIBLE_COUNT,
    visibleSlotLabels: visibleSessions.map((session) => session.shortcutLabel),
  };
}
