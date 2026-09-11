import { useEffect, useState } from 'react';
import { nextRelativeTimeLabelTick, subscribeRelativeTimeClock } from './relative-time-clock';

export function useRelativeTimeTick(enabled: boolean, intervalMs = 1_000, relativeTimestamp?: string): number {
  const [tick, setTick] = useState(() => Date.now());

  useEffect(() => {
    if (!enabled) {
      return;
    }

    const timestampMs = relativeTimestamp === undefined ? undefined : Date.parse(relativeTimestamp);
    return subscribeRelativeTimeClock(setTick, (nowMs) =>
      timestampMs === undefined ? nowMs + intervalMs : nextRelativeTimeLabelTick(timestampMs, nowMs)
    );
  }, [enabled, intervalMs, relativeTimestamp]);

  return tick;
}
