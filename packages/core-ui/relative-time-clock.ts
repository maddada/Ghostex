type ClockSubscriber = {
  notify: (nowMs: number) => void;
  nextTick: (nowMs: number) => number;
  dueAt: number;
};

const subscribers = new Set<ClockSubscriber>();
let timeout: ReturnType<typeof setTimeout> | undefined;

function clearClockTimeout() {
  if (timeout !== undefined) {
    clearTimeout(timeout);
    timeout = undefined;
  }
}

function scheduleClock() {
  clearClockTimeout();
  if (document.hidden || subscribers.size === 0) return;
  let dueAt = Infinity;
  for (const subscriber of subscribers) dueAt = Math.min(dueAt, subscriber.dueAt);
  if (!Number.isFinite(dueAt)) return;
  // Coalesce fractional timestamp boundaries into one wakeup per second, the display's finest unit.
  dueAt = Math.ceil(dueAt / 1_000) * 1_000;
  timeout = setTimeout(tickClock, Math.max(1, Math.min(2_147_483_647, dueAt - Date.now())));
}

function tickClock(refreshAll = false) {
  const nowMs = Date.now();
  for (const subscriber of subscribers) {
    if (refreshAll || subscriber.dueAt <= nowMs) {
      subscriber.dueAt = subscriber.nextTick(nowMs);
      subscriber.notify(nowMs);
    }
  }
  scheduleClock();
}

function onVisibilityChange() {
  if (document.hidden) clearClockTimeout();
  else tickClock(true);
}

/**
 * CDXC:SessionStatus 2026-09-11 WHY:
 * Sidebar cards share one display clock so each row does not own a perpetual timer.
 * Hidden pages stop ticking and refresh immediately on return; countdown actions remain owned by gxserver.
 */
export function subscribeRelativeTimeClock(
  notify: (nowMs: number) => void,
  nextTick: (nowMs: number) => number
): () => void {
  const nowMs = Date.now();
  const subscriber = { notify, nextTick, dueAt: nextTick(nowMs) };
  if (subscribers.size === 0) document.addEventListener('visibilitychange', onVisibilityChange);
  subscribers.add(subscriber);
  notify(nowMs);
  scheduleClock();
  return () => {
    subscribers.delete(subscriber);
    if (subscribers.size === 0) document.removeEventListener('visibilitychange', onVisibilityChange);
    scheduleClock();
  };
}

/** Matches formatRelativeTime with allowJustNow: false, including future timestamps. */
export function nextRelativeTimeLabelTick(timestampMs: number, nowMs: number): number {
  if (!Number.isFinite(timestampMs)) return Infinity;
  const ageMs = Math.max(0, nowMs - timestampMs);
  const unitMs = ageMs < 60_000 ? 1_000 : ageMs < 3_600_000 ? 60_000 : ageMs < 86_400_000 ? 3_600_000 : 86_400_000;
  return timestampMs + (Math.floor(ageMs / unitMs) + 1) * unitMs;
}
