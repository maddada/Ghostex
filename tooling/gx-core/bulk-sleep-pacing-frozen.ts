/**
 * The app runtime's bulk sleep pacing (`apps/desktop/sidebar/bulk-sleep-pacing.ts`), moved here
 * unchanged on 2026-09-25: its callers left the runtime with F3 (c30db3086) and only the gates in
 * this folder still import it. The Rust pacing is packages/gx-core/src/sidebar_actions/bulk.rs.
 */
export const GPUI_SIDEBAR_BULK_SLEEP_INTERVAL_MS = 350;

export type GpuiSidebarBulkSleepCounts = {
  attempted: number;
  completed: number;
  failed: number;
};

export type GpuiSidebarBulkSleepPacingOptions = {
  intervalMs?: number;
  wait?: (intervalMs: number) => Promise<void>;
};

type GpuiSidebarBulkSleepOperation<Target> = (target: Target, index: number) => Promise<void> | void;

/*
CDXC:SessionSleep 2026-06-27-02:05:
GPUI bulk sleep must mirror native sidebar pacing by sleeping one target at a time and waiting 350ms between attempts. Return only aggregate counts so failed operations cannot leak session ids, titles, paths, commands, URLs, or user text through helper results.
*/
export async function runGpuiSidebarBulkSleepPaced<Target>(
  targets: readonly Target[],
  sleepTarget: GpuiSidebarBulkSleepOperation<Target>,
  options: GpuiSidebarBulkSleepPacingOptions = {}
): Promise<GpuiSidebarBulkSleepCounts> {
  const counts: GpuiSidebarBulkSleepCounts = {
    attempted: 0,
    completed: 0,
    failed: 0,
  };
  const wait = options.wait ?? waitForGpuiSidebarBulkSleepInterval;
  const intervalMs = options.intervalMs ?? GPUI_SIDEBAR_BULK_SLEEP_INTERVAL_MS;

  for (const [index, target] of targets.entries()) {
    counts.attempted += 1;

    try {
      await sleepTarget(target, index);
      counts.completed += 1;
    } catch {
      counts.failed += 1;
    }

    if (index < targets.length - 1) {
      await wait(intervalMs);
    }
  }

  return counts;
}

function waitForGpuiSidebarBulkSleepInterval(intervalMs: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, intervalMs);
  });
}
