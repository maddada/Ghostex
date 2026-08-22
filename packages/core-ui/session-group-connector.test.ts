import { describe, expect, test } from "vitest";
import { shouldShowSessionGroupConnector } from "./session-group-connector";

describe("shouldShowSessionGroupConnector", () => {
  test("should show the connector for any non-empty group", () => {
    expect(
      shouldShowSessionGroupConnector({
        sessions: [
          {
            sessionId: "session-1",
          },
          {
            sessionId: "session-2",
          },
        ],
      }),
    ).toBe(true);
  });

  test("should not show the connector for empty project groups without sidebar editor rows", () => {
    /**
     * CDXC:ProjectGroups 2026-05-15-14:33:
     * Empty project groups no longer have a sidebar Code editor row, so they
     * should not keep a connector rail only for hidden editor content.
     */
    expect(
      shouldShowSessionGroupConnector({
        sessions: [],
      }),
    ).toBe(false);
  });

  test("should not show the connector when a non-project group is empty", () => {
    expect(
      shouldShowSessionGroupConnector({
        sessions: [],
      }),
    ).toBe(false);
  });
});
