import { describe, expect, test } from "vitest";
import {
  createDefaultSidebarProjectDiffStats,
  mergeSidebarProjectDiffStats,
  parseGitNumstatDiffStats,
  parseGitZeroDelimitedPaths,
  resolveSidebarProjectDiffStats,
} from "./project-diff-stats";

describe("parseGitNumstatDiffStats", () => {
  test("counts changed files and tracked line stats", () => {
    expect(parseGitNumstatDiffStats("9\t11\tsrc/app.ts\n-\t-\timage.png\n")).toEqual({
      additions: 9,
      deletions: 11,
      files: 2,
      isLoading: false,
      isRepo: true,
    });
  });
});

describe("parseGitZeroDelimitedPaths", () => {
  test("keeps spaces in untracked file paths", () => {
    expect(parseGitZeroDelimitedPaths("new file.ts\0nested/other.ts\0")).toEqual([
      "new file.ts",
      "nested/other.ts",
    ]);
  });
});

describe("mergeSidebarProjectDiffStats", () => {
  test("combines tracked and untracked stats", () => {
    expect(
      mergeSidebarProjectDiffStats(parseGitNumstatDiffStats("1\t2\ttracked.ts\n"), {
        ...createDefaultSidebarProjectDiffStats(),
        additions: 5,
        files: 2,
        isRepo: true,
      }),
    ).toEqual({
      additions: 6,
      deletions: 2,
      files: 3,
      isLoading: false,
      isRepo: true,
    });
  });
});

describe("resolveSidebarProjectDiffStats", () => {
  const trackedStats = parseGitNumstatDiffStats("9\t11\tsrc/app.ts\n");
  const untrackedStats = {
    ...createDefaultSidebarProjectDiffStats(),
    additions: 40,
    files: 2,
    isRepo: true,
  };

  test("returns tracked-only stats by default", () => {
    expect(
      resolveSidebarProjectDiffStats({
        showUntrackedWhenNoTrackedChanges: false,
        trackedStats,
        untrackedStats,
      }),
    ).toEqual(trackedStats);
  });

  test("keeps tracked-only stats when tracked line changes exist", () => {
    expect(
      resolveSidebarProjectDiffStats({
        showUntrackedWhenNoTrackedChanges: true,
        trackedStats,
        untrackedStats,
      }),
    ).toEqual(trackedStats);
  });

  test("merges untracked stats only when tracked diff is +0 -0 and opt-in is enabled", () => {
    const emptyTrackedStats = parseGitNumstatDiffStats("");
    expect(
      resolveSidebarProjectDiffStats({
        showUntrackedWhenNoTrackedChanges: true,
        trackedStats: emptyTrackedStats,
        untrackedStats,
      }),
    ).toEqual(mergeSidebarProjectDiffStats(emptyTrackedStats, untrackedStats));
  });
});
