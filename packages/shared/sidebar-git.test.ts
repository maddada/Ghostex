import { describe, expect, test } from "vite-plus/test";
import {
  buildSidebarGitMenuItems,
  createDefaultSidebarGitState,
  getSidebarGitActionCategory,
  getSidebarGitDisabledReason,
  hasSidebarGitRemoteCommitDelta,
  resolveSidebarGitPrimaryActionState,
} from "./sidebar-git";

describe("resolveSidebarGitPrimaryActionState", () => {
  test("should show commit and push copy for push actions with local changes", () => {
    const state = {
      ...createDefaultSidebarGitState("push"),
      branch: "feature/test",
      hasOriginRemote: true,
      hasWorkingTreeChanges: true,
      isRepo: true,
    };

    expect(resolveSidebarGitPrimaryActionState(state)).toEqual({
      action: "push",
      disabled: false,
      disabledReason: undefined,
      label: "Commit & Push",
    });
  });

  test("should show commit push and pr copy for pr actions with local changes", () => {
    const state = {
      ...createDefaultSidebarGitState("pr"),
      branch: "feature/test",
      hasGitHubCli: true,
      hasOriginRemote: true,
      hasWorkingTreeChanges: true,
      isRepo: true,
    };

    expect(resolveSidebarGitPrimaryActionState(state).label).toBe("Commit, Push & PR");
  });

  test("should show view pr copy when an open pr already exists", () => {
    const state = {
      ...createDefaultSidebarGitState("pr"),
      branch: "feature/test",
      hasGitHubCli: true,
      hasOriginRemote: true,
      hasUpstream: true,
      isRepo: true,
      pr: {
        state: "open" as const,
        title: "Add test feature",
        url: "https://example.com/pr/1",
      },
    };

    expect(resolveSidebarGitPrimaryActionState(state).label).toBe("View PR");
  });
});

describe("buildSidebarGitMenuItems", () => {
  test("should rename the pr menu item to view pr when a pr already exists", () => {
    const state = {
      ...createDefaultSidebarGitState("commit"),
      branch: "feature/test",
      hasGitHubCli: true,
      hasOriginRemote: true,
      hasUpstream: true,
      isRepo: true,
      pr: {
        state: "open" as const,
        title: "Add test feature",
        url: "https://example.com/pr/1",
      },
    };

    expect(buildSidebarGitMenuItems(state).map((item) => item.label)).toEqual([
      "Commit",
      "Push",
      "View PR",
      "Multicommit & Release",
      "Release",
    ]);
  });

  test("should show sync with main only for worktree projects", () => {
    const baseState = {
      ...createDefaultSidebarGitState("commit"),
      branch: "feature/test",
      hasGitHubCli: true,
      hasOriginRemote: true,
      isRepo: true,
    };

    expect(buildSidebarGitMenuItems(baseState).map((item) => item.label)).not.toContain(
      "Sync with Main",
    );
    expect(
      buildSidebarGitMenuItems({ ...baseState, isWorktree: true }).map((item) => item.label),
    ).toEqual([
      "Commit",
      "Push",
      "Create PR",
      "Sync with Main",
      "Multicommit & Release",
      "Release",
    ]);
  });
});

describe("hasSidebarGitRemoteCommitDelta", () => {
  test("should treat zero and negative remote counts as synced", () => {
    expect(hasSidebarGitRemoteCommitDelta({ aheadCount: 0, behindCount: 0 })).toBe(false);
    expect(hasSidebarGitRemoteCommitDelta({ aheadCount: -1, behindCount: 0 })).toBe(false);
    expect(hasSidebarGitRemoteCommitDelta({ aheadCount: 0, behindCount: -1 })).toBe(false);
  });

  test("should detect either ahead or behind remote counts", () => {
    expect(hasSidebarGitRemoteCommitDelta({ aheadCount: 1, behindCount: 0 })).toBe(true);
    expect(hasSidebarGitRemoteCommitDelta({ aheadCount: 0, behindCount: 1 })).toBe(true);
    expect(hasSidebarGitRemoteCommitDelta({ aheadCount: 2.7, behindCount: 0 })).toBe(true);
  });
});

describe("getSidebarGitActionCategory", () => {
  test("should classify create pr and sync with main as agent workflows", () => {
    const state = {
      ...createDefaultSidebarGitState("commit"),
      branch: "feature/test",
      isRepo: true,
      isWorktree: true,
    };

    expect(getSidebarGitActionCategory(state, "commit")).toBe("direct");
    expect(getSidebarGitActionCategory(state, "push")).toBe("direct");
    expect(getSidebarGitActionCategory(state, "syncRemote")).toBe("direct");
    expect(getSidebarGitActionCategory(state, "pr")).toBe("agent");
    expect(getSidebarGitActionCategory(state, "syncMain")).toBe("agent");
    expect(getSidebarGitActionCategory(state, "multiRelease")).toBe("agent");
    expect(getSidebarGitActionCategory(state, "release")).toBe("agent");
  });

  test("should classify an open pr as a direct view action", () => {
    const state = {
      ...createDefaultSidebarGitState("commit"),
      branch: "feature/test",
      isRepo: true,
      pr: {
        state: "open" as const,
        title: "Add test feature",
        url: "https://example.com/pr/1",
      },
    };

    expect(getSidebarGitActionCategory(state, "pr")).toBe("direct");
  });
});

describe("getSidebarGitDisabledReason", () => {
  test("should block push actions on detached head", () => {
    expect(
      getSidebarGitDisabledReason(
        {
          ...createDefaultSidebarGitState("push"),
          hasOriginRemote: true,
          isRepo: true,
        },
        "push",
      ),
    ).toBe("Create and checkout a branch before pushing or creating a PR.");
  });

  test("should require gh for pr actions", () => {
    expect(
      getSidebarGitDisabledReason(
        {
          ...createDefaultSidebarGitState("pr"),
          branch: "feature/test",
          hasOriginRemote: true,
          isRepo: true,
        },
        "pr",
      ),
    ).toBe("Install GitHub CLI to create or view pull requests.");
  });

  test("should allow sync with main in worktree projects without requiring gh", () => {
    expect(
      getSidebarGitDisabledReason(
        {
          ...createDefaultSidebarGitState("commit"),
          branch: "feature/test",
          isRepo: true,
          isWorktree: true,
        },
        "syncMain",
      ),
    ).toBeUndefined();
  });

  test("should allow remote sync for normal branches with an upstream", () => {
    expect(
      getSidebarGitDisabledReason(
        {
          ...createDefaultSidebarGitState("commit"),
          branch: "main",
          hasUpstream: true,
          isRepo: true,
        },
        "syncRemote",
      ),
    ).toBeUndefined();
  });
});
