import { readFileSync } from 'node:fs';
import { describe, expect, test } from 'vitest';
import { hasKnownSidebarProjectInventory } from './sidebar-project-empty-state';

const sidebarAppSource = readFileSync(new URL('./sidebar-app.tsx', import.meta.url), 'utf8');

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe('hasKnownSidebarProjectInventory', () => {
  test('treats no non-chat groups, settings projects, or recent projects as first-run empty', () => {
    expect(
      hasKnownSidebarProjectInventory({
        groupsById: {
          quick: { isChatCollection: true },
          'gxserver-unavailable': {},
        },
        projectSettingsProjectCount: 0,
        recentProjectCount: 0,
        unavailableProjectGroupId: 'gxserver-unavailable',
        workspaceGroupIds: ['quick', 'gxserver-unavailable'],
      })
    ).toBe(false);
  });

  test('counts authoritative project groups even when search has no visible rows', () => {
    expect(
      hasKnownSidebarProjectInventory({
        groupsById: {
          project: {},
        },
        projectSettingsProjectCount: 0,
        recentProjectCount: 0,
        unavailableProjectGroupId: 'gxserver-unavailable',
        workspaceGroupIds: ['project'],
      })
    ).toBe(true);
  });

  test('counts settings and recent project inventory without rendered project groups', () => {
    /*
     * CDXC:Projects 2026-06-30-03:25:
     * Once a user has added or parked projects, the Projects section should use
     * the compact "No projects" empty copy during search/display transitions
     * instead of flashing the first-project onboarding block.
     */
    expect(
      hasKnownSidebarProjectInventory({
        groupsById: {},
        projectSettingsProjectCount: 1,
        recentProjectCount: 0,
        unavailableProjectGroupId: 'gxserver-unavailable',
        workspaceGroupIds: [],
      })
    ).toBe(true);
    expect(
      hasKnownSidebarProjectInventory({
        groupsById: {},
        projectSettingsProjectCount: 0,
        recentProjectCount: 1,
        unavailableProjectGroupId: 'gxserver-unavailable',
        workspaceGroupIds: [],
      })
    ).toBe(true);
  });

  test('keeps first-project onboarding out of active sidebar search', () => {
    const emptyStateSource = sourceBetween(
      sidebarAppSource,
      'const hasKnownProjectInventoryForEmptyState = hasKnownSidebarProjectInventory({',
      'const referenceProjectsEmptyState = showGxserverUnavailableEmptyState'
    );

    expect(emptyStateSource).toContain('projectSettingsProjectCount: projectSettingsProjects?.length ?? 0');
    expect(emptyStateSource).toContain('recentProjectCount: recentProjects.length');
    expect(emptyStateSource).toContain(
      'const shouldShowFirstProjectEmptyState = !isSessionSearchOpen && !hasKnownProjectInventoryForEmptyState;'
    );
  });
});
