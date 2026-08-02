import { describe, expect, test } from "vite-plus/test";
import {
  createDefaultProjectSidebarCommandsState,
  getProjectSidebarCommandsState,
  normalizeProjectSidebarCommandsStore,
  resolveProjectCommandsOwnerId,
} from "./project-sidebar-commands";

describe("normalizeProjectSidebarCommandsStore", () => {
  test("should ignore invalid entries and normalize per-project command state", () => {
    expect(
      normalizeProjectSidebarCommandsStore({
        "project-a": {
          commands: [
            {
              actionType: "terminal",
              closeTerminalOnExit: false,
              command: "pnpm dev",
              commandId: "custom-dev",
              isDefault: false,
              name: "Dev Server",
              playCompletionSound: true,
            },
          ],
          deletedDefaultCommandIds: ["setup", "setup"],
          order: ["custom-dev", "custom-dev"],
        },
        "": { commands: [] },
        invalid: null,
      }),
    ).toEqual({
      "project-a": {
        commands: [
          {
            actionType: "terminal",
            closeTerminalOnExit: false,
            command: "pnpm dev",
            commandId: "custom-dev",
            isDefault: false,
            name: "Dev Server",
            playCompletionSound: true,
            showOnProjectRow: false,
          },
        ],
        deletedDefaultCommandIds: ["setup"],
        order: ["custom-dev"],
      },
    });
  });

  test("should return an empty store for non-object candidates", () => {
    expect(normalizeProjectSidebarCommandsStore(null)).toEqual({});
    expect(normalizeProjectSidebarCommandsStore([])).toEqual({});
  });
});

describe("resolveProjectCommandsOwnerId", () => {
  test("should use the parent project id for worktree projects", () => {
    expect(resolveProjectCommandsOwnerId("worktree-a", "parent-a")).toBe("parent-a");
  });

  test("should keep the project id when no parent is provided", () => {
    expect(resolveProjectCommandsOwnerId("project-a")).toBe("project-a");
    expect(resolveProjectCommandsOwnerId("project-a", "   ")).toBe("project-a");
  });
});

describe("getProjectSidebarCommandsState", () => {
  test("should return defaults when a project has no stored actions", () => {
    expect(getProjectSidebarCommandsState({}, "missing-project")).toEqual(
      createDefaultProjectSidebarCommandsState(),
    );
  });

  test("should return the stored slice for a known project", () => {
    const projectState = {
      commands: [],
      deletedDefaultCommandIds: ["test"],
      order: ["dev"],
    };
    expect(
      getProjectSidebarCommandsState({ "project-b": projectState }, "project-b"),
    ).toEqual(projectState);
  });
});
