import { describe, expect, test } from "vitest";
import {
  canDropProjectWithWorktrees,
  moveProjectsWithWorktrees,
  orderProjectsWithWorktrees,
  type ProjectWorktreeOrderItem,
} from "./project-worktree-order";

function project(projectId: string): ProjectWorktreeOrderItem {
  return { projectId };
}

function worktree(projectId: string, parentProjectId = "main"): ProjectWorktreeOrderItem {
  return {
    projectId,
    worktree: { parentProjectId },
  };
}

describe("orderProjectsWithWorktrees", () => {
  test("keeps chats first and groups worktrees directly below their main project", () => {
    expect(
      orderProjectsWithWorktrees([
        { isChat: true, projectId: "chat" },
        worktree("wt-2"),
        project("other"),
        project("main"),
        worktree("wt-1"),
      ]).map((item) => item.projectId),
    ).toEqual(["chat", "other", "main", "wt-2", "wt-1"]);
  });

  test("moving a main project carries its worktrees in the same relative order", () => {
    expect(
      orderProjectsWithWorktrees([
        worktree("wt-1"),
        worktree("wt-2"),
        project("other"),
        project("main"),
      ]).map((item) => item.projectId),
    ).toEqual(["other", "main", "wt-1", "wt-2"]);
  });
});

describe("moveProjectsWithWorktrees", () => {
  test("allows worktree reordering inside the same main-project family", () => {
    expect(
      moveProjectsWithWorktrees(
        [project("main"), worktree("wt-1"), worktree("wt-2"), project("other")],
        "wt-2",
        { orderId: "wt-1", position: "before" },
      ).map((item) => item.projectId),
    ).toEqual(["main", "wt-2", "wt-1", "other"]);
  });

  test("blocks worktree drops outside the main-project family", () => {
    const projects = [project("main"), worktree("wt-1"), worktree("wt-2"), project("other")];

    expect(
      canDropProjectWithWorktrees(projects, "wt-1", {
        orderId: "other",
        position: "after",
      }),
    ).toBe(false);
    expect(
      moveProjectsWithWorktrees(projects, "wt-1", { orderId: "other", position: "after" }).map(
        (item) => item.projectId,
      ),
    ).toEqual(["main", "wt-1", "wt-2", "other"]);
  });

  test("allows a worktree drop immediately below its main project", () => {
    expect(
      moveProjectsWithWorktrees(
        [project("main"), worktree("wt-1"), worktree("wt-2"), project("other")],
        "wt-2",
        { orderId: "main", position: "after" },
      ).map((item) => item.projectId),
    ).toEqual(["main", "wt-2", "wt-1", "other"]);
  });
});
