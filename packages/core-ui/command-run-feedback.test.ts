import { describe, expect, test } from "vite-plus/test";
import {
  applySidebarCommandRunStateChangedMessage,
  getSidebarCommandRunFeedbackDuration,
  getSidebarCommandRunModeForClick,
  reconcileSidebarCommandRunFeedbackStates,
  SIDEBAR_COMMAND_ERROR_FEEDBACK_DURATION_MS,
  SIDEBAR_COMMAND_SUCCESS_FEEDBACK_DURATION_MS,
} from "./command-run-feedback";

describe("applySidebarCommandRunStateChangedMessage", () => {
  test("should keep a command in the running state until the last overlapping run settles", () => {
    const runningA = applySidebarCommandRunStateChangedMessage(undefined, {
      commandId: "build",
      runId: "run-a",
      state: "running",
      type: "sidebarCommandRunStateChanged",
    });
    const runningB = applySidebarCommandRunStateChangedMessage(runningA, {
      commandId: "build",
      runId: "run-b",
      state: "running",
      type: "sidebarCommandRunStateChanged",
    });
    const afterFirstExit = applySidebarCommandRunStateChangedMessage(runningB, {
      commandId: "build",
      runId: "run-a",
      state: "success",
      type: "sidebarCommandRunStateChanged",
    });
    const afterSecondExit = applySidebarCommandRunStateChangedMessage(afterFirstExit, {
      commandId: "build",
      runId: "run-b",
      state: "error",
      type: "sidebarCommandRunStateChanged",
    });

    expect(runningB).toEqual({
      activeRunIds: ["run-a", "run-b"],
      status: "running",
    });
    expect(afterFirstExit).toEqual({
      activeRunIds: ["run-b"],
      status: "running",
    });
    expect(afterSecondExit).toEqual({
      activeRunIds: [],
      status: "error",
    });
  });
});

describe("getSidebarCommandRunFeedbackDuration", () => {
  test("should use the requested success and error display durations", () => {
    expect(getSidebarCommandRunFeedbackDuration("running")).toBeUndefined();
    expect(getSidebarCommandRunFeedbackDuration("success")).toBe(
      SIDEBAR_COMMAND_SUCCESS_FEEDBACK_DURATION_MS,
    );
    expect(getSidebarCommandRunFeedbackDuration("error")).toBe(
      SIDEBAR_COMMAND_ERROR_FEEDBACK_DURATION_MS,
    );
  });
});

describe("getSidebarCommandRunModeForClick", () => {
  test("should rerun close-on-exit terminal actions in debug mode while error feedback is visible", () => {
    expect(
      getSidebarCommandRunModeForClick(
        {
          actionType: "terminal",
          closeTerminalOnExit: true,
        },
        {
          activeRunIds: [],
          status: "error",
        },
      ),
    ).toBe("debug");
  });

  test("should keep normal clicks in default mode for non-error states", () => {
    expect(
      getSidebarCommandRunModeForClick(
        {
          actionType: "terminal",
          closeTerminalOnExit: true,
        },
        {
          activeRunIds: [],
          status: "success",
        },
      ),
    ).toBe("default");
    expect(
      getSidebarCommandRunModeForClick(
        {
          actionType: "terminal",
          closeTerminalOnExit: true,
        },
        {
          activeRunIds: ["run-a"],
          status: "running",
        },
      ),
    ).toBe("default");
    expect(
      getSidebarCommandRunModeForClick(
        {
          actionType: "terminal",
          closeTerminalOnExit: false,
        },
        {
          activeRunIds: [],
          status: "error",
        },
      ),
    ).toBe("default");
    expect(
      getSidebarCommandRunModeForClick(
        {
          actionType: "browser",
          closeTerminalOnExit: false,
        },
        {
          activeRunIds: [],
          status: "error",
        },
      ),
    ).toBe("default");
  });
});

describe("reconcileSidebarCommandRunFeedbackStates", () => {
  test("should drop stale command feedback for commands that no longer exist", () => {
    expect(
      reconcileSidebarCommandRunFeedbackStates(
        {
          build: {
            activeRunIds: [],
            status: "success",
          },
          test: {
            activeRunIds: ["run-test"],
            status: "running",
          },
        },
        ["test"],
      ),
    ).toEqual({
      test: {
        activeRunIds: ["run-test"],
        status: "running",
      },
    });
  });
});
