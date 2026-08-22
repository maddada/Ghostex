import { describe, expect, test } from "vitest";
import {
  BUILT_IN_WORKSPACE_OPEN_TARGETS,
  normalizeCustomWorkspaceOpenTargets,
  normalizeWorkspaceOpenTargetAvailability,
  normalizeWorkspaceOpenTargetHiddenIds,
} from "./workspace-open-targets";

describe("workspace open targets", () => {
  test("keeps embedded editor out of the built-in open target catalog", () => {
    /**
     * CDXC:TitlebarOpenIn 2026-05-16-23:02
     * Embedded Editor is opened through Code mode, so the Open In target
     * catalog should contain only external project-open destinations.
     */
    expect(BUILT_IN_WORKSPACE_OPEN_TARGETS.map((target) => target.id)).not.toContain(
      "embedded-editor",
    );
    expect(BUILT_IN_WORKSPACE_OPEN_TARGETS[0]?.id).toBe("cursor");
  });

  test("normalizes hidden built-in ids and drops unknown entries", () => {
    expect(normalizeWorkspaceOpenTargetHiddenIds(["cursor", "cursor", "unknown"])).toEqual([
      "cursor",
    ]);
  });

  test("normalizes custom command targets", () => {
    expect(
      normalizeCustomWorkspaceOpenTargets([
        { args: ["--reuse-window"], command: "fleet", id: "custom:fleet", label: "Fleet" },
        { command: "", label: "Broken" },
      ]),
    ).toEqual([
      {
        args: ["--reuse-window"],
        command: "fleet",
        id: "custom:fleet",
        label: "Fleet",
      },
    ]);
  });

  test("normalizes installed target availability separately from hidden ids", () => {
    expect(
      normalizeWorkspaceOpenTargetAvailability({
        availableTargetIds: ["cursor", "unknown"],
        checkedAtMs: 123,
        resolvedAppNames: { cursor: "Cursor", unknown: "Nope" },
        resolvedCommands: { cursor: "cursor", finder: "open", unknown: "nope" },
      }),
    ).toEqual({
      availableTargetIds: ["cursor", "finder"],
      checkedAtMs: 123,
      resolvedAppNames: { cursor: "Cursor" },
      resolvedCommands: { cursor: "cursor", finder: "open" },
    });
  });
});
