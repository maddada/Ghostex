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
CDXC:GPUIBulkSleep 2026-06-27-02:05:
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
