import { describe, expect, test, vi } from "vite-plus/test";
import { logSidebarDebug } from "./sidebar-debug";

describe("logSidebarDebug", () => {
  test("should not write sidebar debug events to the browser console", () => {
    const consoleDebugSpy = vi.spyOn(console, "debug").mockImplementation(() => {});

    logSidebarDebug(true, "session.dragIndicatorChanged", {
      indicator: {
        groupId: "group-1",
        kind: "session",
        position: "after",
        sessionId: "session-09",
      },
    });

    expect(consoleDebugSpy).not.toHaveBeenCalled();
  });
});
