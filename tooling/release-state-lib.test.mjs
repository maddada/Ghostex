import { describe, expect, test } from "vitest";
import { createInitialState, planAdvanceAfterStaging } from "./release-state-lib.mjs";

const version = "6.3.0";
const sourceSha = "a".repeat(40);

function releaseResult(completed, packages = null) {
  const updateSparkle = packages === null || packages.includes("macos-arm64");
  const state = createInitialState({ packages, sourceSha, updateSparkle, version });
  return {
    completed,
    release: { draft: true },
    state,
  };
}

describe("resumable release automatic advancement", () => {
  test("waits until every dependency of macOS is staged", () => {
    const result = releaseResult({
      android: { run_id: 10 },
      "gxserver-linux-x64": { run_id: 11 },
    });

    expect(planAdvanceAfterStaging(version, result, "gxserver-linux-x64")).toEqual({
      assemblyNeeded: false,
      decisions: [],
    });
  });

  test("dispatches macOS exactly when the second gxserver dependency is staged", () => {
    const result = releaseResult({
      android: { run_id: 10 },
      "gxserver-linux-arm64": { run_id: 12 },
      "gxserver-linux-x64": { run_id: 11 },
    });

    const plan = planAdvanceAfterStaging(version, result, "gxserver-linux-arm64");
    expect(plan.assemblyNeeded).toBe(false);
    expect(plan.decisions).toHaveLength(1);
    expect(plan.decisions[0]).toMatchObject({
      fields: {
        gxserver_arm64_run_id: 12,
        gxserver_x64_run_id: 11,
        version,
      },
      workflow: "release-build-macos.yml",
    });
  });

  test("dispatches assembly when staging completes the selected package scope", () => {
    const result = releaseResult(
      { android: { run_id: 10 } },
      ["android"],
    );

    expect(planAdvanceAfterStaging(version, result, "android")).toEqual({
      assemblyNeeded: true,
      decisions: [],
    });
  });

  test("refuses to advance from a package without validated staged assets", () => {
    const result = releaseResult({});
    expect(() => planAdvanceAfterStaging(version, result, "android")).toThrow(
      "android is not validly staged",
    );
  });
});
