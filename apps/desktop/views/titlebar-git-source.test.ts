import { readFileSync } from 'node:fs';
import { describe, expect, test } from 'vitest';

const appSource = readFileSync(new URL('./titlebar/app.tsx', import.meta.url), 'utf8');
const constantsSource = readFileSync(new URL('./titlebar/constants.ts', import.meta.url), 'utf8');
const projectStateSource = readFileSync(new URL('./titlebar/project-state.ts', import.meta.url), 'utf8');

describe('native titlebar Git source', () => {
  test('hydrates transient refresh state from cached Git metadata', () => {
    /*
     * CDXC:Git 2026-06-16-19:19:
     * The titlebar Git dropdown should reuse the last cached Git snapshot for
     * the active project while refresh is still publishing its busy/default
     * state, so Branch does not flash as detached before the branch probe
     * finishes.
     */
    expect(constantsSource).toContain("const TITLEBAR_GIT_STATE_CACHE_STORAGE_PREFIX = 'ghostex.titlebar.gitState.'");
    expect(projectStateSource).toContain('function resolveTitlebarGitStateForMerge(');
    expect(projectStateSource).toContain('shouldHydrateMissingTitlebarGitStateFromCache(current, cached)');
    expect(projectStateSource).toMatch(
      /incoming\.isBusy &&\s*incoming\.branch === null &&\s*\(cached\.branch !== null \|\| cached\.isRepo\)/
    );
    expect(projectStateSource).toContain('readCachedTitlebarGitState(projectIdentity)');
    expect(appSource).toContain('cacheTitlebarGitState(next);');
    expect(projectStateSource).toContain('cacheTitlebarGitState(mergedState);');
    expect(projectStateSource).toContain('localStorage.setItem(cacheKey, JSON.stringify(state.git));');
  });
});
