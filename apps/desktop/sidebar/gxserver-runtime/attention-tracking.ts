/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import {
  GPUI_ATTENTION_COMPLETION_SOUND_EVENT_CACHE_LIMIT,
  GPUI_ESCAPE_DONE_SUPPRESSION_MS,
  GPUI_LOCALLY_ACKNOWLEDGED_ATTENTION_EVENT_CACHE_LIMIT,
  GPUI_MIN_ATTENTION_VISIBLE_MS,
  GPUI_SIDEBAR_SESSION_COMPLETION_SOUND_MESSAGE_TYPE,
  GPUI_SIDEBAR_SESSION_COMPLETION_SOUND_MESSAGE_VERSION,
} from './constants';
import type { GpuiSidebarRuntime } from './core';
import {
  getGpuiPresentationAttentionEventId,
  getGpuiSessionAttentionEventKey,
  gpuiSessionAttentionTargetKey,
} from './helpers/attention';
import { createGpuiSidebarSettings } from './helpers/bootstrap';
import { normalizeNonEmptyString } from './helpers/records';
import {
  createGpuiRemotePresentationSessionId,
  parseGpuiRemotePresentationSessionId,
} from './helpers/remote-presentation';
import {
  normalizeGpuiWorkspaceSessionAttentionAcknowledge,
  normalizeGpuiWorkspaceTerminalEscapePressed,
} from './helpers/terminal-lifecycle';
import type { GpuiSessionAttentionAcknowledgeReason, GpuiSessionAttentionTarget } from './types-and-protocol';
import type { CompletionSoundSetting } from '@/packages/shared/completion-sound';
import {
  createGxserverPresentationProjectSessionId,
  parseGxserverPresentationProjectSessionId,
} from '@/packages/shared/gxserver-presentation-sidebar-projection';
import type { GxserverPresentationSession, GxserverPresentationSnapshot } from '@/packages/shared/gxserver-protocol';

/*
CDXC:GxserverRuntimeSplit 2026-08-22:
The method signatures below are copied verbatim from the original class body.
They exist as a standalone interface — rather than being derived from
`typeof gpuiSidebarRuntimeAttentionMethods` — because deriving them would make
`GpuiSidebarRuntime` depend on the bodies that depend on it, which TypeScript
reports as a circular base type. `gpuiSidebarRuntimeAttentionMethodsShapeCheck`
at the bottom of this file is what keeps the two in step.
*/
export interface GpuiSidebarRuntimeAttentionMethods {
  handleGpuiWorkspaceSessionAttentionAcknowledge(payload: unknown): void;
  handleGpuiWorkspaceTerminalEscapePressed(payload: unknown): void;
  acknowledgeSessionAttention(sessionId: string, reason: GpuiSessionAttentionAcknowledgeReason): boolean;
  acknowledgePresentationSessionAttention(
    target: GpuiSessionAttentionTarget,
    reason: GpuiSessionAttentionAcknowledgeReason
  ): boolean;
  completePresentationSessionAttentionAcknowledgement(
    target: GpuiSessionAttentionTarget,
    reason: GpuiSessionAttentionAcknowledgeReason,
    attentionEnteredAt?: number
  ): boolean;
  clearPresentationSessionAttentionLocally(
    target: GpuiSessionAttentionTarget,
    reason: GpuiSessionAttentionAcknowledgeReason
  ): boolean;
  currentPresentationSessionForAttentionTarget(
    target: GpuiSessionAttentionTarget
  ): GxserverPresentationSession | undefined;
  setLocalPresentationSessionActivityLocally(
    projectId: string,
    sessionId: string,
    activity: GxserverPresentationSession['activity'],
    _reason: string
  ): boolean;
  setRemotePresentationSessionActivityLocally(
    machineId: string,
    projectId: string,
    sessionId: string,
    activity: GxserverPresentationSession['activity'],
    _reason: string
  ): boolean;
  syncLocalSessionAttentionAcknowledgementWithGxserver(
    projectId: string,
    sessionId: string,
    agentName: string | undefined
  ): Promise<void>;
  syncLocalSessionTerminalEscapeWithGxserver(
    projectId: string,
    sessionId: string,
    agentName: string | undefined
  ): Promise<void>;
  syncRemoteSessionAttentionAcknowledgementWithGxserver(
    machineId: string,
    projectId: string,
    sessionId: string,
    agentName: string | undefined
  ): Promise<void>;
  projectLocalPresentationAttentionAcknowledgementGuards(
    presentation: GxserverPresentationSnapshot
  ): GxserverPresentationSnapshot;
  projectRemotePresentationAttentionAcknowledgementGuards(
    machineId: string,
    presentation: GxserverPresentationSnapshot
  ): GxserverPresentationSnapshot;
  projectPresentationAttentionAcknowledgementGuards(
    presentation: GxserverPresentationSnapshot,
    sessionKeyForPresentationSession: (session: GxserverPresentationSession) => string
  ): GxserverPresentationSnapshot;
  syncLocalPresentationAttentionTracking(
    previousSessions: readonly GxserverPresentationSession[],
    nextSessions: readonly GxserverPresentationSession[]
  ): void;
  syncRemotePresentationAttentionTracking(
    machineId: string,
    previousSessions: readonly GxserverPresentationSession[],
    nextSessions: readonly GxserverPresentationSession[]
  ): void;
  syncPresentationAttentionTracking(
    previousSessions: readonly GxserverPresentationSession[],
    nextSessions: readonly GxserverPresentationSession[],
    sessionKeyForPresentationSession: (session: GxserverPresentationSession) => string
  ): void;
  clearSessionAttentionTracking(sessionKey: string): void;
  clearSessionAttentionAcknowledgementTimer(sessionKey: string): void;
  markSessionAttentionEventLocallyAcknowledged(sessionKey: string, attentionEventId: string | undefined): void;
  isSessionAttentionEventLocallyAcknowledged(sessionKey: string, attentionEventId: string | undefined): boolean;
  clearLocallyAcknowledgedAttentionEventsForSession(sessionKey: string): void;
  detectSessionAttentionCompletionSounds(
    previousSessions: readonly GxserverPresentationSession[],
    nextSessions: readonly GxserverPresentationSession[]
  ): void;
  suppressAttentionCompletionSoundAfterTerminalEscape(sessionKey: string): void;
  getAttentionCompletionSoundSuppressedUntil(sessionKey: string): number | undefined;
  postNativeSessionCompletionSound(sound: CompletionSoundSetting): void;
  rememberAttentionCompletionSoundEventKey(eventKey: string): boolean;
}

export const gpuiSidebarRuntimeAttentionMethods = {
  handleGpuiWorkspaceSessionAttentionAcknowledge(this: GpuiSidebarRuntime, payload: unknown): void {
    const acknowledgement = normalizeGpuiWorkspaceSessionAttentionAcknowledge(payload);
    if (!acknowledgement) {
      return;
    }
    this.acknowledgeSessionAttention(
      createGxserverPresentationProjectSessionId(acknowledgement.projectId, acknowledgement.sessionId),
      'native-focus'
    );
  },

  handleGpuiWorkspaceTerminalEscapePressed(this: GpuiSidebarRuntime, payload: unknown): void {
    const escape = normalizeGpuiWorkspaceTerminalEscapePressed(payload);
    if (!escape) {
      return;
    }
    const target: GpuiSessionAttentionTarget = {
      kind: 'local',
      projectId: escape.projectId,
      sessionId: escape.sessionId,
    };
    const sessionKey = gpuiSessionAttentionTargetKey(target);
    this.suppressAttentionCompletionSoundAfterTerminalEscape(sessionKey);
    const session = this.currentPresentationSessionForAttentionTarget(target);
    if (session?.activity === 'attention') {
      const didChange = this.clearPresentationSessionAttentionLocally(target, 'terminal-escape');
      if (didChange) {
        this.publishPresentation('patch');
      }
    }
    void this.syncLocalSessionTerminalEscapeWithGxserver(
      escape.projectId,
      escape.sessionId,
      normalizeNonEmptyString(session?.agentName)
    );
  },

  acknowledgeSessionAttention(
    this: GpuiSidebarRuntime,
    sessionId: string,
    reason: GpuiSessionAttentionAcknowledgeReason
  ): boolean {
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    if (remoteSession) {
      return this.acknowledgePresentationSessionAttention(
        {
          kind: 'remote',
          machineId: remoteSession.machineId,
          projectId: remoteSession.projectId,
          sessionId: remoteSession.sessionId,
        },
        reason
      );
    }
    const reference = parseGxserverPresentationProjectSessionId(sessionId);
    if (!reference) {
      return false;
    }
    return this.acknowledgePresentationSessionAttention(
      {
        kind: 'local',
        projectId: reference.projectId,
        sessionId: reference.sessionId,
      },
      reason
    );
  },

  acknowledgePresentationSessionAttention(
    this: GpuiSidebarRuntime,
    target: GpuiSessionAttentionTarget,
    reason: GpuiSessionAttentionAcknowledgeReason
  ): boolean {
    const session = this.currentPresentationSessionForAttentionTarget(target);
    if (session?.activity !== 'attention') {
      return false;
    }
    const sessionKey = gpuiSessionAttentionTargetKey(target);
    const attentionEnteredAt = this.attentionEnteredAtBySessionKey.get(sessionKey);
    const remainingVisibleMs =
      attentionEnteredAt === undefined
        ? 0
        : GPUI_MIN_ATTENTION_VISIBLE_MS - Math.max(0, Date.now() - attentionEnteredAt);
    if (attentionEnteredAt !== undefined && remainingVisibleMs > 0) {
      if (!this.attentionAcknowledgementTimeoutsBySessionKey.has(sessionKey)) {
        const timeout = window.setTimeout(() => {
          this.attentionAcknowledgementTimeoutsBySessionKey.delete(sessionKey);
          const latestAttentionEnteredAt = this.attentionEnteredAtBySessionKey.get(sessionKey);
          if (
            latestAttentionEnteredAt !== attentionEnteredAt ||
            !this.completePresentationSessionAttentionAcknowledgement(target, reason, attentionEnteredAt)
          ) {
            return;
          }
        }, remainingVisibleMs);
        this.attentionAcknowledgementTimeoutsBySessionKey.set(sessionKey, timeout);
      }
      return true;
    }
    return this.completePresentationSessionAttentionAcknowledgement(target, reason, attentionEnteredAt);
  },

  completePresentationSessionAttentionAcknowledgement(
    this: GpuiSidebarRuntime,
    target: GpuiSessionAttentionTarget,
    reason: GpuiSessionAttentionAcknowledgeReason,
    attentionEnteredAt?: number
  ): boolean {
    const session = this.currentPresentationSessionForAttentionTarget(target);
    if (session?.activity !== 'attention') {
      return false;
    }
    const sessionKey = gpuiSessionAttentionTargetKey(target);
    const latestAttentionEnteredAt = this.attentionEnteredAtBySessionKey.get(sessionKey);
    if (
      attentionEnteredAt !== undefined &&
      latestAttentionEnteredAt !== undefined &&
      latestAttentionEnteredAt !== attentionEnteredAt
    ) {
      return false;
    }

    const didChange = this.clearPresentationSessionAttentionLocally(target, reason);
    if (target.kind === 'remote') {
      if (didChange) {
        this.publishRemotePresentationPatch();
      }
      void this.syncRemoteSessionAttentionAcknowledgementWithGxserver(
        target.machineId,
        target.projectId,
        target.sessionId,
        normalizeNonEmptyString(session.agentName)
      );
      return true;
    }

    if (didChange) {
      this.publishPresentation('patch');
    }
    void this.syncLocalSessionAttentionAcknowledgementWithGxserver(
      target.projectId,
      target.sessionId,
      normalizeNonEmptyString(session.agentName)
    );
    return true;
  },

  clearPresentationSessionAttentionLocally(
    this: GpuiSidebarRuntime,
    target: GpuiSessionAttentionTarget,
    reason: GpuiSessionAttentionAcknowledgeReason
  ): boolean {
    const session = this.currentPresentationSessionForAttentionTarget(target);
    if (session?.activity !== 'attention') {
      return false;
    }
    const sessionKey = gpuiSessionAttentionTargetKey(target);
    const attentionEventId =
      getGpuiPresentationAttentionEventId(session) ?? this.attentionEventIdBySessionKey.get(sessionKey);
    this.markSessionAttentionEventLocallyAcknowledged(sessionKey, attentionEventId);
    const didChange =
      target.kind === 'remote'
        ? this.setRemotePresentationSessionActivityLocally(
            target.machineId,
            target.projectId,
            target.sessionId,
            'idle',
            reason
          )
        : this.setLocalPresentationSessionActivityLocally(target.projectId, target.sessionId, 'idle', reason);
    this.clearSessionAttentionTracking(sessionKey);
    return didChange;
  },

  currentPresentationSessionForAttentionTarget(
    this: GpuiSidebarRuntime,
    target: GpuiSessionAttentionTarget
  ): GxserverPresentationSession | undefined {
    if (target.kind === 'remote') {
      return this.findRemotePresentationSession(target);
    }
    return this.findLocalPresentationSession(target.projectId, target.sessionId);
  },

  setLocalPresentationSessionActivityLocally(
    this: GpuiSidebarRuntime,
    projectId: string,
    sessionId: string,
    activity: GxserverPresentationSession['activity'],
    _reason: string
  ): boolean {
    const presentation = this.presentation;
    if (!presentation) {
      return false;
    }
    let didChange = false;
    const sessions = presentation.sessions.map((session) => {
      if (session.projectId !== projectId || session.sessionId !== sessionId) {
        return session;
      }
      if (session.activity === activity && (activity === 'attention' || session.attention === undefined)) {
        return session;
      }
      didChange = true;
      if (activity !== 'attention') {
        const { attention: _attention, ...withoutAttention } = session;
        return {
          ...withoutAttention,
          activity,
        };
      }
      return {
        ...session,
        activity,
      };
    });
    if (!didChange) {
      return false;
    }
    this.presentation = {
      ...presentation,
      sessions,
    };
    return true;
  },

  setRemotePresentationSessionActivityLocally(
    this: GpuiSidebarRuntime,
    machineId: string,
    projectId: string,
    sessionId: string,
    activity: GxserverPresentationSession['activity'],
    _reason: string
  ): boolean {
    const presentation = this.remotePresentations.get(machineId);
    if (!presentation) {
      return false;
    }
    let didChange = false;
    const sessions = presentation.sessions.map((session) => {
      if (session.projectId !== projectId || session.sessionId !== sessionId) {
        return session;
      }
      if (session.activity === activity && (activity === 'attention' || session.attention === undefined)) {
        return session;
      }
      didChange = true;
      if (activity !== 'attention') {
        const { attention: _attention, ...withoutAttention } = session;
        return {
          ...withoutAttention,
          activity,
        };
      }
      return {
        ...session,
        activity,
      };
    });
    if (!didChange) {
      return false;
    }
    this.remotePresentations.set(machineId, {
      ...presentation,
      sessions,
    });
    return true;
  },

  async syncLocalSessionAttentionAcknowledgementWithGxserver(
    this: GpuiSidebarRuntime,
    projectId: string,
    sessionId: string,
    agentName: string | undefined
  ): Promise<void> {
    const client = this.client;
    if (!client) {
      return;
    }
    try {
      await client.rpc('/api/updateAgentActivity', {
        ...(agentName ? { agentName } : {}),
        event: 'acknowledge',
        projectId,
        sessionId,
      });
    } catch {
      // gxserver acknowledgement sync is best-effort, matching macOS's log-only failure path.
    }
  },

  async syncLocalSessionTerminalEscapeWithGxserver(
    this: GpuiSidebarRuntime,
    projectId: string,
    sessionId: string,
    agentName: string | undefined
  ): Promise<void> {
    const client = this.client;
    if (!client) {
      return;
    }
    try {
      await client.rpc('/api/updateAgentActivity', {
        ...(agentName ? { agentName } : {}),
        event: 'escape',
        projectId,
        sessionId,
      });
    } catch {
      // Terminal escape suppression is best-effort locally until gxserver confirms it.
    }
  },

  async syncRemoteSessionAttentionAcknowledgementWithGxserver(
    this: GpuiSidebarRuntime,
    machineId: string,
    projectId: string,
    sessionId: string,
    agentName: string | undefined
  ): Promise<void> {
    try {
      await this.requestRemoteGxserver(machineId, '/api/updateAgentActivity', {
        ...(agentName ? { agentName } : {}),
        event: 'acknowledge',
        projectId,
        sessionId,
      });
    } catch {
      // Remote acknowledgement uses the same optimistic presentation clear as local.
    }
  },

  projectLocalPresentationAttentionAcknowledgementGuards(
    this: GpuiSidebarRuntime,
    presentation: GxserverPresentationSnapshot
  ): GxserverPresentationSnapshot {
    return this.projectPresentationAttentionAcknowledgementGuards(presentation, (session) =>
      createGxserverPresentationProjectSessionId(session.projectId, session.sessionId)
    );
  },

  projectRemotePresentationAttentionAcknowledgementGuards(
    this: GpuiSidebarRuntime,
    machineId: string,
    presentation: GxserverPresentationSnapshot
  ): GxserverPresentationSnapshot {
    return this.projectPresentationAttentionAcknowledgementGuards(presentation, (session) =>
      createGpuiRemotePresentationSessionId(machineId, session.projectId, session.sessionId)
    );
  },

  projectPresentationAttentionAcknowledgementGuards(
    this: GpuiSidebarRuntime,
    presentation: GxserverPresentationSnapshot,
    sessionKeyForPresentationSession: (session: GxserverPresentationSession) => string
  ): GxserverPresentationSnapshot {
    let didChange = false;
    const sessions = presentation.sessions.map((session) => {
      if (session.activity !== 'attention') {
        return session;
      }
      const sessionKey = sessionKeyForPresentationSession(session);
      const attentionEventId = getGpuiPresentationAttentionEventId(session);
      if (
        attentionEventId !== undefined &&
        this.isSessionAttentionEventLocallyAcknowledged(sessionKey, attentionEventId)
      ) {
        didChange = true;
        const { attention: _attention, ...withoutAttention } = session;
        return {
          ...withoutAttention,
          activity: 'idle' as const,
        };
      }
      if (attentionEventId !== undefined) {
        this.clearLocallyAcknowledgedAttentionEventsForSession(sessionKey);
      }
      return session;
    });
    return didChange
      ? {
          ...presentation,
          sessions,
        }
      : presentation;
  },

  syncLocalPresentationAttentionTracking(
    this: GpuiSidebarRuntime,
    previousSessions: readonly GxserverPresentationSession[],
    nextSessions: readonly GxserverPresentationSession[]
  ): void {
    this.syncPresentationAttentionTracking(previousSessions, nextSessions, (session) =>
      createGxserverPresentationProjectSessionId(session.projectId, session.sessionId)
    );
  },

  syncRemotePresentationAttentionTracking(
    this: GpuiSidebarRuntime,
    machineId: string,
    previousSessions: readonly GxserverPresentationSession[],
    nextSessions: readonly GxserverPresentationSession[]
  ): void {
    this.syncPresentationAttentionTracking(previousSessions, nextSessions, (session) =>
      createGpuiRemotePresentationSessionId(machineId, session.projectId, session.sessionId)
    );
  },

  syncPresentationAttentionTracking(
    this: GpuiSidebarRuntime,
    previousSessions: readonly GxserverPresentationSession[],
    nextSessions: readonly GxserverPresentationSession[],
    sessionKeyForPresentationSession: (session: GxserverPresentationSession) => string
  ): void {
    const previousKeys = new Set(previousSessions.map(sessionKeyForPresentationSession));
    const nextAttentionKeys = new Set<string>();
    for (const session of nextSessions) {
      const sessionKey = sessionKeyForPresentationSession(session);
      if (session.activity !== 'attention') {
        if (previousKeys.has(sessionKey)) {
          this.clearSessionAttentionTracking(sessionKey);
        }
        continue;
      }
      nextAttentionKeys.add(sessionKey);
      const nextAttentionEventId = getGpuiPresentationAttentionEventId(session);
      const hadPreviousEventId = this.attentionEventIdBySessionKey.has(sessionKey);
      const previousAttentionEventId = this.attentionEventIdBySessionKey.get(sessionKey);
      const eventIdChanged =
        nextAttentionEventId !== (hadPreviousEventId ? previousAttentionEventId : undefined) &&
        (nextAttentionEventId !== undefined || hadPreviousEventId);
      if (!this.attentionEnteredAtBySessionKey.has(sessionKey) || eventIdChanged) {
        this.clearSessionAttentionAcknowledgementTimer(sessionKey);
        this.attentionEnteredAtBySessionKey.set(sessionKey, Date.now());
      }
      if (nextAttentionEventId === undefined) {
        this.attentionEventIdBySessionKey.delete(sessionKey);
      } else {
        this.attentionEventIdBySessionKey.set(sessionKey, nextAttentionEventId);
      }
    }
    for (const session of previousSessions) {
      const sessionKey = sessionKeyForPresentationSession(session);
      if (!nextAttentionKeys.has(sessionKey)) {
        this.clearSessionAttentionTracking(sessionKey);
      }
    }
  },

  clearSessionAttentionTracking(this: GpuiSidebarRuntime, sessionKey: string): void {
    this.clearSessionAttentionAcknowledgementTimer(sessionKey);
    this.attentionEnteredAtBySessionKey.delete(sessionKey);
    this.attentionEventIdBySessionKey.delete(sessionKey);
  },

  clearSessionAttentionAcknowledgementTimer(this: GpuiSidebarRuntime, sessionKey: string): void {
    const timeout = this.attentionAcknowledgementTimeoutsBySessionKey.get(sessionKey);
    if (timeout === undefined) {
      return;
    }
    window.clearTimeout(timeout);
    this.attentionAcknowledgementTimeoutsBySessionKey.delete(sessionKey);
  },

  markSessionAttentionEventLocallyAcknowledged(
    this: GpuiSidebarRuntime,
    sessionKey: string,
    attentionEventId: string | undefined
  ): void {
    const eventKey = getGpuiSessionAttentionEventKey(sessionKey, attentionEventId);
    if (eventKey === undefined || this.locallyAcknowledgedAttentionEventKeys.has(eventKey)) {
      return;
    }
    this.locallyAcknowledgedAttentionEventKeys.add(eventKey);
    this.locallyAcknowledgedAttentionEventKeyOrder.push(eventKey);
    while (
      this.locallyAcknowledgedAttentionEventKeyOrder.length > GPUI_LOCALLY_ACKNOWLEDGED_ATTENTION_EVENT_CACHE_LIMIT
    ) {
      const staleKey = this.locallyAcknowledgedAttentionEventKeyOrder.shift();
      if (staleKey !== undefined) {
        this.locallyAcknowledgedAttentionEventKeys.delete(staleKey);
      }
    }
  },

  isSessionAttentionEventLocallyAcknowledged(
    this: GpuiSidebarRuntime,
    sessionKey: string,
    attentionEventId: string | undefined
  ): boolean {
    const eventKey = getGpuiSessionAttentionEventKey(sessionKey, attentionEventId);
    return eventKey !== undefined && this.locallyAcknowledgedAttentionEventKeys.has(eventKey);
  },

  clearLocallyAcknowledgedAttentionEventsForSession(this: GpuiSidebarRuntime, sessionKey: string): void {
    const keyPrefix = `${sessionKey}\u001f`;
    let didClear = false;
    for (const eventKey of this.locallyAcknowledgedAttentionEventKeys) {
      if (eventKey.startsWith(keyPrefix)) {
        this.locallyAcknowledgedAttentionEventKeys.delete(eventKey);
        didClear = true;
      }
    }
    if (didClear) {
      this.locallyAcknowledgedAttentionEventKeyOrder = this.locallyAcknowledgedAttentionEventKeyOrder.filter(
        (eventKey) => !eventKey.startsWith(keyPrefix)
      );
    }
  },

  /*
  Completion sound + card flash (macOS parity): only live presentation deltas
  represent the edge where a session newly enters attention. Startup and
  stream-recovery snapshots can carry sessions that were already in attention
  before this client observed them, so applyPresentationSnapshot must not run
  this detection, and re-published attention states dedupe by attention event
  id so a replayed event updates UI state without replaying the sound. Focus
  acknowledgement round-trips through gxserver and only ever transitions a
  session OUT of attention, so it cannot re-trigger this edge.
  */
  detectSessionAttentionCompletionSounds(
    this: GpuiSidebarRuntime,
    previousSessions: readonly GxserverPresentationSession[],
    nextSessions: readonly GxserverPresentationSession[]
  ): void {
    const settings = createGpuiSidebarSettings(this.runtimeSettings);
    if (!settings.completionBellEnabled) {
      return;
    }
    let previousActivityBySessionKey: Map<string, GxserverPresentationSession['activity']> | undefined;
    for (const session of nextSessions) {
      if (session.activity !== 'attention' || session.attention?.acknowledged === true) {
        continue;
      }
      const sessionKey = createGxserverPresentationProjectSessionId(session.projectId, session.sessionId);
      if (this.getAttentionCompletionSoundSuppressedUntil(sessionKey) !== undefined) {
        continue;
      }
      previousActivityBySessionKey ??= new Map(
        previousSessions.map((previousSession) => [
          createGxserverPresentationProjectSessionId(previousSession.projectId, previousSession.sessionId),
          previousSession.activity,
        ])
      );
      const previousActivity = previousActivityBySessionKey.get(sessionKey);
      if (previousActivity === undefined || previousActivity === 'attention') {
        continue;
      }
      const attentionEventId = getGpuiPresentationAttentionEventId(session);
      if (
        attentionEventId !== undefined &&
        !this.rememberAttentionCompletionSoundEventKey(`${sessionKey}\u001f${attentionEventId}`)
      ) {
        continue;
      }
      this.messageSource.postMessage({
        sessionId: sessionKey,
        sound: settings.completionSound,
        type: 'playCompletionSound',
      });
      this.postNativeSessionCompletionSound(settings.completionSound);
    }
  },

  suppressAttentionCompletionSoundAfterTerminalEscape(this: GpuiSidebarRuntime, sessionKey: string): void {
    this.attentionCompletionSoundSuppressedUntilBySessionKey.set(
      sessionKey,
      Date.now() + GPUI_ESCAPE_DONE_SUPPRESSION_MS
    );
  },

  getAttentionCompletionSoundSuppressedUntil(this: GpuiSidebarRuntime, sessionKey: string): number | undefined {
    const suppressedUntil = this.attentionCompletionSoundSuppressedUntilBySessionKey.get(sessionKey);
    if (suppressedUntil === undefined) {
      return undefined;
    }
    if (!Number.isFinite(suppressedUntil) || suppressedUntil <= Date.now()) {
      this.attentionCompletionSoundSuppressedUntilBySessionKey.delete(sessionKey);
      return undefined;
    }
    return suppressedUntil;
  },

  /*
  GPUI has no webview sound assets (the SidebarApp player's sound-URL global
  is never populated), so audible playback is Rust-owned from the bundled
  sound files — the same native-playback ownership macOS uses via its
  playSound host message. The SidebarApp message above still drives the card
  flash.
  */
  postNativeSessionCompletionSound(this: GpuiSidebarRuntime, sound: CompletionSoundSetting): void {
    const postCompletionSound = window.ghostexGpui?.postSessionCompletionSound;
    if (typeof postCompletionSound !== 'function') {
      return;
    }
    postCompletionSound(
      JSON.stringify({
        version: GPUI_SIDEBAR_SESSION_COMPLETION_SOUND_MESSAGE_VERSION,
        type: GPUI_SIDEBAR_SESSION_COMPLETION_SOUND_MESSAGE_TYPE,
        sound,
      })
    );
  },

  rememberAttentionCompletionSoundEventKey(this: GpuiSidebarRuntime, eventKey: string): boolean {
    if (this.attentionCompletionSoundEventKeys.has(eventKey)) {
      return false;
    }
    this.attentionCompletionSoundEventKeys.add(eventKey);
    this.attentionCompletionSoundEventKeyOrder.push(eventKey);
    while (this.attentionCompletionSoundEventKeyOrder.length > GPUI_ATTENTION_COMPLETION_SOUND_EVENT_CACHE_LIMIT) {
      const staleKey = this.attentionCompletionSoundEventKeyOrder.shift();
      if (staleKey !== undefined) {
        this.attentionCompletionSoundEventKeys.delete(staleKey);
      }
    }
    return true;
  },
};

const gpuiSidebarRuntimeAttentionMethodsShapeCheck: GpuiSidebarRuntimeAttentionMethods =
  gpuiSidebarRuntimeAttentionMethods;
void gpuiSidebarRuntimeAttentionMethodsShapeCheck;
