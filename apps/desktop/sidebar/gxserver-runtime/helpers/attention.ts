/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import type { GpuiSessionAttentionTarget } from '../types-and-protocol';
import { normalizeNonEmptyString } from './records';
import { createGpuiRemotePresentationSessionId } from './remote-presentation';
import { createGxserverPresentationProjectSessionId } from '@/packages/shared/gxserver-presentation-sidebar-projection';
import type { GxserverPresentationSession } from '@/packages/shared/gxserver-protocol';

export function gpuiSessionAttentionTargetKey(target: GpuiSessionAttentionTarget): string {
  return target.kind === 'remote'
    ? createGpuiRemotePresentationSessionId(target.machineId, target.projectId, target.sessionId)
    : createGxserverPresentationProjectSessionId(target.projectId, target.sessionId);
}

export function getGpuiSessionAttentionEventKey(
  sessionKey: string,
  attentionEventId: string | undefined
): string | undefined {
  const normalizedSessionKey = normalizeNonEmptyString(sessionKey)?.trim();
  const normalizedAttentionEventId = normalizeNonEmptyString(attentionEventId)?.trim();
  if (!normalizedSessionKey || !normalizedAttentionEventId) {
    return undefined;
  }
  return `${normalizedSessionKey}\u001f${normalizedAttentionEventId}`;
}

export function getGpuiPresentationAttentionEventId(
  session: Pick<GxserverPresentationSession, 'activity' | 'attention'>
): string | undefined {
  /*
  Presentation attention rows carry eventId for sound dedupe; enteredAt stays
  a compatibility key for older daemon payloads, matching macOS.
  */
  if (session.activity !== 'attention') {
    return undefined;
  }
  const eventId = session.attention?.eventId?.trim();
  if (eventId) {
    return eventId;
  }
  const enteredAt = session.attention?.enteredAt?.trim();
  return enteredAt ? enteredAt : undefined;
}
