/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import { GPUI_CLOSE_AFTER_DONE_STORAGE_KEY, GPUI_DELAYED_SEND_MIN_DELAY_MS } from '../constants';
import type { GxserverPresentationSession } from '@/packages/shared/gxserver-protocol';

export function isGpuiInactiveProjectPresentationSession(session: GxserverPresentationSession): boolean {
  return session.lifecycleState !== 'sleeping' && session.activity !== 'working' && session.activity !== 'attention';
}

export function isGpuiCloseAfterDonePresentationSessionDone(session: GxserverPresentationSession): boolean {
  if (session.activity === 'attention') {
    return true;
  }
  return session.activity !== 'working' && hasGpuiCloseAfterDoneAgentIdentity(session);
}

export function hasGpuiCloseAfterDoneAgentIdentity(session: GxserverPresentationSession): boolean {
  return Boolean(
    session.agentSessionId?.trim() ||
    session.agentSessionPath?.trim() ||
    session.agentName?.trim() ||
    session.agentId?.trim() ||
    session.agentIcon?.trim()
  );
}

export function formatGpuiCloseAfterDoneCountdown(remainingMs: number): string {
  const totalSeconds = Math.max(0, Math.ceil(remainingMs / 1_000));
  const hours = Math.floor(totalSeconds / 3_600);
  const minutes = Math.floor((totalSeconds % 3_600) / 60);
  const seconds = totalSeconds % 60;
  const paddedMinutes = String(minutes).padStart(2, '0');
  const paddedSeconds = String(seconds).padStart(2, '0');
  if (hours > 0) {
    return `${String(hours).padStart(2, '0')}:${paddedMinutes}:${paddedSeconds}`;
  }
  return `${paddedMinutes}:${paddedSeconds}`;
}

export function formatGpuiDelayedSendDelay(delayMs: number): string {
  const totalMinutes = Math.max(1, Math.round(delayMs / GPUI_DELAYED_SEND_MIN_DELAY_MS));
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  return [hours > 0 ? `${hours}h` : undefined, minutes > 0 ? `${minutes}m` : undefined]
    .filter((part): part is string => part !== undefined)
    .join(' ');
}

export function readStoredGpuiCloseAfterDoneSessionIds(): string[] {
  try {
    const raw = window.localStorage.getItem(GPUI_CLOSE_AFTER_DONE_STORAGE_KEY);
    if (!raw) {
      return [];
    }
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) {
      return [];
    }
    return parsed.filter((value): value is string => typeof value === 'string' && value.trim().length > 0);
  } catch {
    return [];
  }
}

export function writeStoredGpuiCloseAfterDoneSessionIds(sessionIds: readonly string[]): void {
  try {
    if (sessionIds.length === 0) {
      window.localStorage.removeItem(GPUI_CLOSE_AFTER_DONE_STORAGE_KEY);
      return;
    }
    window.localStorage.setItem(GPUI_CLOSE_AFTER_DONE_STORAGE_KEY, JSON.stringify([...sessionIds]));
  } catch {
    // Storage availability must never gate close-after-done behavior.
  }
}
