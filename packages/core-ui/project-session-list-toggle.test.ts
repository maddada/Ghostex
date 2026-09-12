import { describe, expect, test } from 'vitest';
import { PROJECT_SESSION_LIST_COMPACT_COUNT, getVisibleProjectSessionIds } from './project-session-list-toggle';

describe('getVisibleProjectSessionIds', () => {
  const sessionIds = Array.from(
    { length: PROJECT_SESSION_LIST_COMPACT_COUNT + 2 },
    (_, index) => `session-${index + 1}`
  );

  test('shows the compact row count by default', () => {
    expect(
      getVisibleProjectSessionIds({
        isExpanded: false,
        isProjectGroup: true,
        isToggleEnabled: true,
        sessionIds,
      })
    ).toEqual(sessionIds.slice(0, PROJECT_SESSION_LIST_COMPACT_COUNT));
  });

  test('shows every project session once the project is switched to Full', () => {
    expect(
      getVisibleProjectSessionIds({
        isExpanded: true,
        isProjectGroup: true,
        isToggleEnabled: true,
        sessionIds,
      })
    ).toEqual(sessionIds);
  });

  test('uses the configured compact row count', () => {
    const configuredSessionIds = Array.from({ length: 12 }, (_, index) => `session-${index + 1}`);
    expect(
      getVisibleProjectSessionIds({
        compactCount: 10,
        isExpanded: false,
        isProjectGroup: true,
        isToggleEnabled: true,
        sessionIds: configuredSessionIds,
      })
    ).toEqual(configuredSessionIds.slice(0, 10));
  });

  test('does not count rows inside collapsed sections toward the compact cap', () => {
    const pinnedSessionIds = Array.from({ length: 4 }, (_, index) => `pinned-${index + 1}`);
    const plainSessionIds = Array.from({ length: 12 }, (_, index) => `session-${index + 1}`);
    expect(
      getVisibleProjectSessionIds({
        compactCount: 10,
        isExpanded: false,
        isProjectGroup: true,
        isSessionInCollapsedSection: (sessionId) => sessionId.startsWith('pinned-'),
        isToggleEnabled: true,
        sessionIds: [...pinnedSessionIds, ...plainSessionIds],
      })
    ).toEqual(plainSessionIds.slice(0, 10));
  });

  test('does not trim non-project or temporarily disabled lists', () => {
    expect(
      getVisibleProjectSessionIds({
        isExpanded: false,
        isProjectGroup: false,
        isToggleEnabled: true,
        sessionIds,
      })
    ).toEqual(sessionIds);

    expect(
      getVisibleProjectSessionIds({
        isExpanded: false,
        isProjectGroup: true,
        isToggleEnabled: false,
        sessionIds,
      })
    ).toEqual(sessionIds);
  });
});
