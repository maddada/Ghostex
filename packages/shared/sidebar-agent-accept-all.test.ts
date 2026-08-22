import { describe, expect, test } from "vitest";
import {
  AGENT_ACCEPT_ALL_MODE_SELECT_ITEMS,
  normalizeAgentAcceptAllMode,
  resolveAgentAcceptAllFlagSpec,
  resolveAgentAcceptAllSpec,
  supportsAgentAcceptAll,
} from "./sidebar-agent-accept-all";

describe("sidebar Accept All support metadata", () => {
  test("should normalize per-agent Accept All modes for Settings UI", () => {
    expect(normalizeAgentAcceptAllMode("inherit")).toBe("inherit");
    expect(normalizeAgentAcceptAllMode("enabled")).toBe("enabled");
    expect(normalizeAgentAcceptAllMode("disabled")).toBe("disabled");
    expect(normalizeAgentAcceptAllMode("invalid")).toBeUndefined();
  });

  test("should expose user-facing labels for collapsed Accept All selects", () => {
    expect(AGENT_ACCEPT_ALL_MODE_SELECT_ITEMS).toEqual([
      { label: "Inherit global setting", value: "inherit" },
      { label: "Accept All", value: "enabled" },
      { label: "Ask for permission", value: "disabled" },
    ]);
  });

  test("should expose support detection without shaping commands in the sidebar", () => {
    expect(supportsAgentAcceptAll("codex")).toBe(true);
    expect(supportsAgentAcceptAll("pi")).toBe(false);
    expect(supportsAgentAcceptAll("custom-codex", "codex")).toBe(true);
  });

  test("should keep display-only flag metadata for supported CLIs", () => {
    expect(resolveAgentAcceptAllFlagSpec("codex")?.canonicalFlag).toBe("--yolo");
    expect(resolveAgentAcceptAllFlagSpec("claude")?.canonicalFlag).toBe(
      "--dangerously-skip-permissions",
    );
    expect(resolveAgentAcceptAllFlagSpec("opencode")).toBeUndefined();
    expect(resolveAgentAcceptAllSpec("opencode")?.kind).toBe("runtimeConfig");
  });
});
