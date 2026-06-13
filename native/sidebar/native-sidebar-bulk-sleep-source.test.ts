import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const nativeSidebarSource = readFileSync(new URL("./native-sidebar.tsx", import.meta.url), "utf8");

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

function countOccurrences(source: string, pattern: string): number {
  return source.split(pattern).length - 1;
}

describe("native sidebar bulk sleep cascade source", () => {
  test("keeps bulk sleep fan-out on a paced queue", () => {
    /*
     * CDXC:NativeSidebarBulkActions 2026-06-13-02:09:
     * Multi-session sleep commands must sleep one target at a time with a short
     * interval between provider shutdowns so Sleep Inactive, Sleep below, and
     * tab-scope sleep do not lag the app or spike the laptop.
     */
    const schedulerSource = sourceBetween(
      nativeSidebarSource,
      "type NativeSidebarBulkActionOptions",
      "type NativeBootstrap",
    );

    expect(schedulerSource).toContain("const NATIVE_SIDEBAR_BULK_SLEEP_INTERVAL_MS = 350;");
    expect(schedulerSource).toContain("delayBetweenOperationsMs?: number");
    expect(schedulerSource).toContain("window.setTimeout(runNext, delayBetweenOperationsMs)");
    expect(schedulerSource).toContain("function runNativeSidebarBulkSleepActionInBackground");
    expect(schedulerSource).toContain(
      "delayBetweenOperationsMs: NATIVE_SIDEBAR_BULK_SLEEP_INTERVAL_MS",
    );
  });

  test("routes multi-session sleep actions through the sleep cascade", () => {
    const setSessionsSleepingSource = sourceBetween(
      nativeSidebarSource,
      "function setNativeSessionsSleepingInBackground",
      "function closeNativeSessionsInBackground",
    );
    expect(setSessionsSleepingSource).toContain(
      "sleeping\n    ? runNativeSidebarBulkSleepActionInBackground",
    );

    const titlebarSleepSource = sourceBetween(
      nativeSidebarSource,
      "function sleepInactiveSessionsFromTitlebar",
      "function focusResourceSessionFromTitlebar",
    );
    expect(titlebarSleepSource).toContain("runNativeSidebarBulkSleepActionInBackground");

    const projectSleepSource = sourceBetween(
      nativeSidebarSource,
      "function sleepInactiveProjectSessions",
      "function hasProjectedDelayedSend",
    );
    expect(countOccurrences(projectSleepSource, "runNativeSidebarBulkSleepActionInBackground")).toBe(2);

    const autoSleepSource = sourceBetween(
      nativeSidebarSource,
      'function runNativeAutoSleepMonitor(source: "interval" | "settings-change" | "startup")',
      "function shouldAutoSleepAgentSession",
    );
    expect(countOccurrences(autoSleepSource, "runNativeSidebarBulkSleepActionInBackground")).toBe(2);

    const titlebarQuitSource = sourceBetween(
      nativeSidebarSource,
      "function quitResourcesFromTitlebar",
      "function setNativeSessionPoppedOut",
    );
    expect(titlebarQuitSource).toContain("let includesSleepOperation = false");
    expect(titlebarQuitSource).toContain(
      "includesSleepOperation\n    ? runNativeSidebarBulkSleepActionInBackground",
    );

    const closeAllSource = sourceBetween(
      nativeSidebarSource,
      "function closeAllNativeSessions",
      "function workspaceHasOpenSessions",
    );
    expect(closeAllSource).toContain("runNativeSidebarBulkSleepActionInBackground");

    const remoteSleepSource = sourceBetween(
      nativeSidebarSource,
      "function sleepInactiveRemoteProjectSessions",
      "function closeInactiveRemoteProjectSessions",
    );
    expect(remoteSleepSource).toContain("runNativeSidebarBulkSleepActionInBackground");

    const sidebarMessageSource = sourceBetween(
      nativeSidebarSource,
      '    case "setSessionsSleeping": {',
      '    case "setSessionFavorite":',
    );
    expect(sidebarMessageSource).toContain(
      "message.sleeping\n        ? runNativeSidebarBulkSleepActionInBackground",
    );

    const remoteGroupSleepSource = sourceBetween(
      nativeSidebarSource,
      '    case "setGroupSleeping": {',
      '    case "sleepInactiveProjectSessions":',
    );
    expect(remoteGroupSleepSource).toContain(
      "message.sleeping\n          ? runNativeSidebarBulkSleepActionInBackground",
    );

    const paneTabSleepSource = sourceBetween(
      nativeSidebarSource,
      "function handleNativePaneTabSleepRequested",
      "function handleNativePaneTabReorderRequested",
    );
    expect(paneTabSleepSource).toContain("runNativeSidebarBulkSleepActionInBackground");
  });
});
