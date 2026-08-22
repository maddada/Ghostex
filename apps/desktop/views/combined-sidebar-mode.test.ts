import { describe, expect, test } from "vitest";
import {
  createCombinedProjectGroupId,
  createCombinedProjectSessionId,
  parseCombinedProjectGroupId,
  parseCombinedProjectSessionId,
} from "./combined-sidebar-mode";

describe("combined sidebar mode ids", () => {
  test("round-trips project group ids", () => {
    /**
     * CDXC:SidebarMode 2026-05-03-10:42
     * Combined-mode project groups need stable presentation IDs that can be
     * converted back into project IDs when drag reorder syncs to storage.
     */
    const groupId = createCombinedProjectGroupId("project-alpha:one");

    expect(parseCombinedProjectGroupId(groupId)).toBe("project-alpha:one");
    expect(parseCombinedProjectGroupId("group-1")).toBeUndefined();
  });

  test("round-trips project-scoped session ids", () => {
    const sessionId = createCombinedProjectSessionId("project-alpha:one", "session-00");

    expect(parseCombinedProjectSessionId(sessionId)).toEqual({
      projectId: "project-alpha:one",
      sessionId: "session-00",
    });
    expect(parseCombinedProjectSessionId("session-00")).toBeUndefined();
  });
});
