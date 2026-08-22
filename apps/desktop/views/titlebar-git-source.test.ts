import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const titlebarHostSource = readFileSync(new URL("./titlebar-host.tsx", import.meta.url), "utf8");

describe("native titlebar Git source", () => {
  test("hydrates transient refresh state from cached Git metadata", () => {
    /*
     * CDXC:TitlebarGit 2026-06-16-19:19:
     * The titlebar Git dropdown should reuse the last cached Git snapshot for
     * the active project while refresh is still publishing its busy/default
     * state, so Branch does not flash as detached before the branch probe
     * finishes.
     */
    expect(titlebarHostSource).toContain(
      'const TITLEBAR_GIT_STATE_CACHE_STORAGE_PREFIX = "ghostex.titlebar.gitState."',
    );
    expect(titlebarHostSource).toContain("function resolveTitlebarGitStateForMerge(");
    expect(titlebarHostSource).toContain("shouldHydrateMissingTitlebarGitStateFromCache(current, cached)");
    expect(titlebarHostSource).toMatch(
      /incoming\.isBusy &&\s*incoming\.branch === null &&\s*\(cached\.branch !== null \|\| cached\.isRepo\)/,
    );
    expect(titlebarHostSource).toContain("readCachedTitlebarGitState(projectIdentity)");
    expect(titlebarHostSource).toContain("cacheTitlebarGitState(next);");
    expect(titlebarHostSource).toContain("cacheTitlebarGitState(mergedState);");
    expect(titlebarHostSource).toContain("localStorage.setItem(cacheKey, JSON.stringify(state.git));");
  });
});
