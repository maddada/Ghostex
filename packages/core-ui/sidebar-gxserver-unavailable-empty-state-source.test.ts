import { readFileSync } from 'node:fs';
import { describe, expect, test } from 'vitest';

const sidebarAppSource = readFileSync(new URL('./sidebar-app.tsx', import.meta.url), 'utf8');

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe('gxserver unavailable sidebar empty state source', () => {
  test('keeps the startup placeholder silent until the delayed restart guidance', () => {
    /*
     * CDXC:StateSync 2026-06-16-09:35:
     * The synthetic gxserver-unavailable group exists for startup state
     * bookkeeping, not as visible copy. Source coverage keeps the raw placeholder
     * out of the Projects list and protects the 20-second delayed, No-projects
     * styled restart guidance requested for gxserver-off startup.
     */
    const constantsSource = sourceBetween(
      sidebarAppSource,
      'const SIDEBAR_GXSERVER_UNAVAILABLE_GROUP_ID',
      'const MIN_SESSION_SEARCH_QUERY_LENGTH'
    );
    const timerSource = sourceBetween(
      sidebarAppSource,
      'const hasGxserverUnavailablePlaceholder = Boolean(',
      'const effectiveSettings = settings ?? DEFAULT_ghostex_SETTINGS;'
    );
    const projectGroupFilterSource = sourceBetween(
      sidebarAppSource,
      'const displayedReferenceProjectGroupIds = useMemo(',
      'const remoteProjectGroupIdsByMachineId = useMemo('
    );
    const emptyStateSource = sourceBetween(
      sidebarAppSource,
      'const referenceProjectsEmptyState = showGxserverUnavailableEmptyState',
      'const {'
    );

    expect(constantsSource).toContain('const SIDEBAR_GXSERVER_UNAVAILABLE_EMPTY_STATE_DELAY_MS = 20_000;');
    expect(timerSource).toContain('setShowGxserverUnavailableEmptyState(false);');
    expect(timerSource).toContain('setShowGxserverUnavailableEmptyState(true);');
    expect(timerSource).toContain('SIDEBAR_GXSERVER_UNAVAILABLE_EMPTY_STATE_DELAY_MS');
    expect(projectGroupFilterSource).toContain('groupId !== SIDEBAR_GXSERVER_UNAVAILABLE_GROUP_ID');
    expect(emptyStateSource).toContain("className='reference-sidebar-empty-state'");
    expect(emptyStateSource).toContain('Unable to load sessions.');
    expect(emptyStateSource).toContain('<br />');
    expect(emptyStateSource).toContain('Restart Ghostex to try again.');
    expect(emptyStateSource).toContain('No Projects Added.');
    expect(emptyStateSource).toContain(
      'Open the More menu at the top of the sidebar and choose Add Project to get started!'
    );
  });
});
