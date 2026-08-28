/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import { GPUI_AUTO_SLEEP_MINUTE_MS } from '../constants';
import type { GpuiCommandPaneSessionSummary } from '../types-and-protocol';
import { filterGpuiGxserverLocalCommandPaneSessions } from './command-pane';
import { normalizeNonEmptyString } from './records';
import { parseGpuiRemotePresentationSessionId } from './remote-presentation';
import type { ghostexSettings } from '@/packages/shared/ghostex-settings';
import {
  createGxserverPresentationProjectSessionId,
  parseGxserverPresentationProjectSessionId,
} from '@/packages/shared/gxserver-presentation-sidebar-projection';
import type {
  GxserverPresentationSession,
  GxserverPresentationSnapshot,
  GxserverSleepSessionResult,
} from '@/packages/shared/gxserver-protocol';
import type { SidebarSessionGroup } from '@/packages/shared/session-grid-contract';

/*
CDXC:MobileKeepAwake 2026-08-19:
gxserver answers a declined automatic sleep with an untouched session and a
reason, which is NOT a failure: `keptAwake` means another client is attached to
that terminal, `neverActive` means nobody has prompted it yet. The sweep treats
either as "leave this row alone" and moves on.
*/
export function gxserverSleepWasDeclined(result: GxserverSleepSessionResult | undefined): boolean {
  return result?.declined !== undefined;
}

export function createGpuiAutoSleepAgentSessionIds({
  activeProjectId,
  commandPaneSessions = [],
  delayedSendSessionIds = [],
  displayedWorkspaceSessionIds = [],
  focusedSessionId,
  groups = [],
  nowMs,
  presentation,
  settings,
}: {
  activeProjectId?: string;
  commandPaneSessions?: readonly GpuiCommandPaneSessionSummary[];
  delayedSendSessionIds?: readonly string[];
  displayedWorkspaceSessionIds?: readonly string[];
  focusedSessionId?: string;
  groups?: readonly SidebarSessionGroup[];
  nowMs: number;
  presentation: GxserverPresentationSnapshot;
  settings: Pick<
    ghostexSettings,
    'autoSleepAgentIdleMinutes' | 'autoSleepFavoriteAgentSessions' | 'autoSleepRequireAgentResumeCommand'
  >;
}): string[] {
  /*
  CDXC:GPUISidebarAutoSleep 2026-06-27-01:24:
  GPUI Agent Auto Sleep must choose only local gxserver presentation agent terminals after protecting selected/visible sidebar owners, focused sessions, active command-pane owners, and popped-out rows. Return bounded project/session routing ids for the existing setSessionSleeping path; do not inspect Browser/project-editor surfaces, titles, paths, commands, terminal output, URLs, tokens, or remote-machine rows.
  */
  if (settings.autoSleepAgentIdleMinutes === 0) {
    return [];
  }
  const protectedProjectSessionKeys = collectGpuiAutoSleepProtectedProjectSessionKeys({
    activeProjectId,
    commandPaneSessions,
    delayedSendSessionIds,
    displayedWorkspaceSessionIds,
    focusedSessionId,
    groups,
    presentation,
  });
  return presentation.sessions.flatMap((session) =>
    shouldAutoSleepGpuiPresentationAgentSession({
      nowMs,
      protectedProjectSessionKeys,
      session,
      settings,
    })
      ? [createGxserverPresentationProjectSessionId(session.projectId, session.sessionId)]
      : []
  );
}

export function collectGpuiAutoSleepProtectedProjectSessionKeys({
  activeProjectId,
  commandPaneSessions = [],
  delayedSendSessionIds = [],
  displayedWorkspaceSessionIds = [],
  focusedSessionId,
  groups = [],
  presentation,
}: {
  activeProjectId?: string;
  commandPaneSessions?: readonly GpuiCommandPaneSessionSummary[];
  delayedSendSessionIds?: readonly string[];
  displayedWorkspaceSessionIds?: readonly string[];
  focusedSessionId?: string;
  groups?: readonly SidebarSessionGroup[];
  presentation: GxserverPresentationSnapshot;
}): Set<string> {
  const protectedProjectSessionKeys = new Set<string>();
  /*
  CDXC:AutoSleepDisplayedSessions 2026-08-20:
  The shell's rendered sessions come first, because they are the only input here
  that reports what the user is actually looking at. Sidebar rows carry
  `isVisible`/`isFocused` from this runtime's own focus bookkeeping, which does
  not survive a gxserver reconnect and never learns that a session switched to
  Chat view, where the terminal is parked behind the chat surface but the
  session is very much on screen.
  */
  for (const displayedSessionId of displayedWorkspaceSessionIds) {
    addGpuiAutoSleepProtectedSessionId(protectedProjectSessionKeys, presentation, displayedSessionId);
  }
  /*
  CDXC:AutoSleepDelayedSend 2026-08-20:
  A session with Delayed Send armed has work waiting on its own timer. Sleeping
  it kills the provider the send would have been typed into, so the prompt the
  user scheduled simply never happens. Shell-owned timers (the gpui countdown
  and the send-when-stopped watchers) live only in the Rust bridge, so they are
  protected here; a daemon-owned send is caught by the eligibility check.
  */
  for (const delayedSendSessionId of delayedSendSessionIds) {
    addGpuiAutoSleepProtectedSessionId(protectedProjectSessionKeys, presentation, delayedSendSessionId);
  }
  for (const group of groups) {
    if (group.remoteMachineContext) {
      continue;
    }
    let hasProjectedOwner = false;
    for (const session of group.sessions) {
      if (session.isFocused === true || session.isVisible === true) {
        addGpuiAutoSleepProtectedSessionId(
          protectedProjectSessionKeys,
          presentation,
          session.sessionId,
          group.projectContext?.editor.projectId
        );
        hasProjectedOwner = true;
      }
      if (session.isPoppedOut === true) {
        addGpuiAutoSleepProtectedSessionId(
          protectedProjectSessionKeys,
          presentation,
          session.sessionId,
          group.projectContext?.editor.projectId
        );
      }
    }
    if (!hasProjectedOwner && group.sessions[0]) {
      addGpuiAutoSleepProtectedSessionId(
        protectedProjectSessionKeys,
        presentation,
        group.sessions[0].sessionId,
        group.projectContext?.editor.projectId
      );
    }
  }
  addGpuiAutoSleepProtectedSessionId(protectedProjectSessionKeys, presentation, focusedSessionId);
  /*
  CDXC:GPUISidebarAutoSleep 2026-06-27-06:54:
  Native Auto Sleep protects the active owner of every visible command-panel split leaf from the command-pane layout, not the HUD-focused tab. GPUI Rust sends that split ownership as sanitized `isPaneOwner:true` on external native-shaped `G...` ids; TypeScript protects only that field after the same local id and valid-status filtering used by command indicators, so internal numeric Rust ids, stale legacy rows, collapsed HUD focus, and malformed statuses cannot keep sessions awake.

  CDXC:GPUISidebarAutoSleep 2026-06-27-07:28:
  Native command-panel layout is scoped to the active project, so a GPUI command-pane owner summary must protect only the active project's matching external `G...` session. Do not treat a bare command-pane id as globally owned across projects because that can keep unrelated same-id agent sessions awake.
  */
  const localCommandPaneSessions = filterGpuiGxserverLocalCommandPaneSessions(commandPaneSessions);
  for (const commandPaneSession of localCommandPaneSessions) {
    if (commandPaneSession.isPaneOwner === true) {
      addGpuiAutoSleepProtectedSessionId(
        protectedProjectSessionKeys,
        presentation,
        commandPaneSession.sessionId,
        activeProjectId
      );
    }
  }
  return protectedProjectSessionKeys;
}

export function shouldAutoSleepGpuiPresentationAgentSession({
  nowMs,
  protectedProjectSessionKeys,
  session,
  settings,
}: {
  nowMs: number;
  protectedProjectSessionKeys: ReadonlySet<string>;
  session: GxserverPresentationSession;
  settings: Pick<
    ghostexSettings,
    'autoSleepAgentIdleMinutes' | 'autoSleepFavoriteAgentSessions' | 'autoSleepRequireAgentResumeCommand'
  >;
}): boolean {
  if (session.lifecycleState !== 'running' || session.activity !== 'idle') {
    return false;
  }
  if (session.actions.sleep !== true || !isGpuiAutoSleepAgentTerminalSession(session)) {
    return false;
  }
  /*
  CDXC:AutoSleepNeverActive 2026-08-22:
  A session nobody has prompted yet is not "idle for 15 minutes", it has no idle
  clock at all: `lastActiveAt` below is the daemon's `createdAt` fallback, so an
  untouched terminal looks stale the moment it ages past the threshold. Sleeping
  one is destructive — the agent published its session id at startup but wrote no
  transcript, so the stored resume reference points at a conversation that never
  existed and the woken terminal cannot get back to that agent. gxserver declines
  the same sweep with `declined: "neverActive"`; this keeps the sweep from asking.
  */
  if (session.hasEverBeenActive !== true) {
    return false;
  }
  if (protectedProjectSessionKeys.has(gpuiAutoSleepProjectSessionKey(session.projectId, session.sessionId))) {
    return false;
  }
  if (gpuiAutoSleepSessionHasArmedDelayedSend(session)) {
    return false;
  }
  if (gpuiAutoSleepSessionHasQueuedChatPrompts(session)) {
    return false;
  }
  if (session.isFavorite === true && settings.autoSleepFavoriteAgentSessions !== true) {
    return false;
  }
  if (settings.autoSleepRequireAgentResumeCommand && !gpuiAutoSleepSessionHasAgentResumeReference(session)) {
    return false;
  }
  const lastActivityMs = gpuiAutoSleepLastActivityMs(session);
  if (lastActivityMs === undefined) {
    return false;
  }
  return nowMs - lastActivityMs >= settings.autoSleepAgentIdleMinutes * GPUI_AUTO_SLEEP_MINUTE_MS;
}

export function gpuiAutoSleepSessionHasArmedDelayedSend(session: GxserverPresentationSession): boolean {
  /*
  CDXC:AutoSleepDelayedSend 2026-08-20:
  Daemon-owned Delayed Send state: a deadline/countdown for a timed send, or a
  send-when-stopped watcher waiting on this agent or the whole project. Any of
  them means a prompt is queued for this terminal, so it is not idle in the
  sense Auto Sleep means.
  */
  return (
    Boolean(session.delayedSendDeadlineAt) ||
    session.delayedSendRemainingMs !== undefined ||
    session.sendWhenAgentStopsActive === true ||
    session.sendWhenAllProjectSessionsStopActive === true
  );
}

export function gpuiAutoSleepSessionHasQueuedChatPrompts(session: GxserverPresentationSession): boolean {
  /*
  CDXC:SessionChatPromptQueue 2026-08-21:
  A session with Ghostex-owned chat prompts still waiting is not idle in the
  sense Auto Sleep means: the daemon's queue scheduler is about to hand the
  agent more work. Automatic sleeps decline here for the same reason they
  decline for an armed Delayed Send. An explicit user Sleep never reaches this
  path and stays untouched.

  CDXC:SessionChatPromptQueue 2026-08-21-b:
  `queuedPromptCount` now includes `failed` rows so the badge can show a stalled
  queue, but a failed row is waiting on the USER, not on the agent — nothing is
  about to be delivered because of it. Subtract them so this stays byte-for-byte
  the same rule gxserver's own decline uses
  (`session_has_pending_session_chat_queue`, `state <> 'failed'`); otherwise a
  single failed row would keep a session awake forever.
  */
  const total = typeof session.queuedPromptCount === 'number' ? session.queuedPromptCount : 0;
  const failed = typeof session.queuedPromptFailedCount === 'number' ? session.queuedPromptFailedCount : 0;
  return total - failed > 0;
}

export function gpuiAutoSleepSessionHasAgentResumeReference(session: GxserverPresentationSession): boolean {
  /*
  gxserver sleep kills the zmx provider and wake relaunches from the daemon's
  stored agent resume state, so a session without any published resume
  reference wakes degraded. macOS validates per-agent reference formats against
  its local agents catalog (canRestoreNativeTerminalSession); GPUI evaluates
  the same restorability contract against the daemon-published resume fields,
  which gxserver already normalizes.
  */
  return Boolean(
    normalizeNonEmptyString(session.agentSessionId) ||
    normalizeNonEmptyString(session.agentSessionPath) ||
    normalizeNonEmptyString(session.trustedResumeTitle)
  );
}

export function isGpuiAutoSleepAgentTerminalSession(session: GxserverPresentationSession): boolean {
  if (session.surface !== 'workspace' && session.surface !== 'commands') {
    return false;
  }
  if (session.kind === 'agent') {
    return true;
  }
  return Boolean(
    normalizeNonEmptyString(session.agentId) ||
    normalizeNonEmptyString(session.agentName) ||
    normalizeNonEmptyString(session.agentSessionId) ||
    normalizeNonEmptyString(session.agentSessionPath)
  );
}

export function gpuiAutoSleepLastActivityMs(session: GxserverPresentationSession): number | undefined {
  const timestamp = session.lastActiveAt ?? session.updatedAt;
  const timestampMs = Date.parse(timestamp);
  return Number.isFinite(timestampMs) ? timestampMs : undefined;
}

export function addGpuiAutoSleepProtectedSessionId(
  protectedProjectSessionKeys: Set<string>,
  presentation: GxserverPresentationSnapshot,
  sessionId: string | undefined,
  projectIdHint?: string
): void {
  const normalizedSessionId = normalizeNonEmptyString(sessionId)?.trim();
  if (!normalizedSessionId || parseGpuiRemotePresentationSessionId(normalizedSessionId)) {
    return;
  }
  const scopedReference = parseGxserverPresentationProjectSessionId(normalizedSessionId);
  if (scopedReference) {
    protectedProjectSessionKeys.add(
      gpuiAutoSleepProjectSessionKey(scopedReference.projectId, scopedReference.sessionId)
    );
    return;
  }
  const matchingSessions = presentation.sessions.filter(
    (session) => session.sessionId === normalizedSessionId && (!projectIdHint || session.projectId === projectIdHint)
  );
  for (const session of matchingSessions) {
    protectedProjectSessionKeys.add(gpuiAutoSleepProjectSessionKey(session.projectId, session.sessionId));
  }
}

export function gpuiAutoSleepProjectSessionKey(projectId: string, sessionId: string): string {
  return `${projectId}\u0000${sessionId}`;
}
