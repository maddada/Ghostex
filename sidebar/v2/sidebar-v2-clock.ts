import { useEffect, useState } from "react";

/*
 * CDXC:SidebarV2 2026-07-29:
 * Every V2 label that ages — "Working 7m", "3d", the settled/snoozed shelf
 * labels, and the auto-settle window itself — reads one quantized clock owned
 * by the root. The alternative (a timer per row) re-renders the whole inbox N
 * times a minute for lists that can hold a hundred sessions.
 *
 * Resolution is deliberately 30 seconds, not one second. Ghostex's V2 status
 * label lives inside the row, so a per-second whole-inbox
 * re-render is exactly the scroll-linked paint work this sidebar has spent a lot
 * of effort removing. Half a minute is the coarsest tick that still lands a
 * minute-granular label ("Working 7m", "3d") within half a minute of the truth.
 */

export const SIDEBAR_V2_CLOCK_INTERVAL_MS = 30_000;

/**
 * Shared now-in-ms for the V2 tree. Returns a fresh value on mount and then at
 * most one update per `intervalMs`, so time-based labels stay honest without
 * making the sidebar animate.
 */
export function useSidebarV2Clock(intervalMs: number = SIDEBAR_V2_CLOCK_INTERVAL_MS): number {
  const [nowMs, setNowMs] = useState(() => Date.now());

  useEffect(() => {
    const intervalId = window.setInterval(() => {
      setNowMs(Date.now());
    }, intervalMs);
    return () => {
      window.clearInterval(intervalId);
    };
  }, [intervalMs]);

  return nowMs;
}
