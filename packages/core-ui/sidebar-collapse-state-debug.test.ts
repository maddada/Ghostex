import { describe, expect, test } from "vitest";
import {
  hashSidebarCollapseDebugId,
  summarizeSidebarCollapseDebugGroupIds,
} from "./sidebar-collapse-state-debug";

describe("sidebar collapse-state debug helpers", () => {
  test("hashes raw group identifiers before persistent logging", () => {
    const rawGroupId =
      "project:/Users/example/private-repo?token=secret https://example.test/path?q=secret command";

    const hashed = hashSidebarCollapseDebugId(rawGroupId);
    const summary = summarizeSidebarCollapseDebugGroupIds([rawGroupId]);

    expect(hashed).toMatch(/^[0-9a-f]{8}$/);
    expect(summary).toEqual([hashed]);
    expect(JSON.stringify(summary)).not.toContain("private-repo");
    expect(JSON.stringify(summary)).not.toContain("/Users/example");
    expect(JSON.stringify(summary)).not.toContain("https://example.test");
    expect(JSON.stringify(summary)).not.toContain("token=secret");
    expect(JSON.stringify(summary)).not.toContain("command");
  });
});
