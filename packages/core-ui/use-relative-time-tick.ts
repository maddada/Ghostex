import { useEffect, useState } from "react";

export function useRelativeTimeTick(enabled: boolean, intervalMs = 1_000): number {
  const [tick, setTick] = useState(() => Date.now());

  useEffect(() => {
    if (!enabled) {
      return;
    }

    const id = window.setInterval(() => {
      setTick(Date.now());
    }, intervalMs);

    return () => {
      window.clearInterval(id);
    };
  }, [enabled, intervalMs]);

  return tick;
}
