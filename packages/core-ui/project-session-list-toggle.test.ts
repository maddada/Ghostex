import { describe, expect, test } from 'vitest';
import {
  PROJECT_SESSION_LIST_COLLAPSED_COUNT,
  getExpandedProjectSessionListScrollHeight,
  getProjectSessionListCollapsedHeight,
  getVisibleProjectSessionIds,
  normalizeStoredProjectSessionListCollapsedState,
} from './project-session-list-toggle';

describe('normalizeStoredProjectSessionListCollapsedState', () => {
  test('keeps only explicitly collapsed project ids', () => {
    expect(
      normalizeStoredProjectSessionListCollapsedState({
        'project-1': true,
        'project-2': false,
        'project-3': 'true',
        '': true,
      })
    ).toEqual({
      'project-1': true,
    });
  });
});

describe('getVisibleProjectSessionIds', () => {
  const sessionIds = Array.from(
    { length: PROJECT_SESSION_LIST_COLLAPSED_COUNT + 2 },
    (_, index) => `session-${index + 1}`
  );

  test('shows all project sessions by default', () => {
    expect(
      getVisibleProjectSessionIds({
        isCollapsed: false,
        isProjectGroup: true,
        isToggleEnabled: true,
        sessionIds,
      })
    ).toEqual(sessionIds);
  });

  test('shows the default project session count after Show less is selected', () => {
    expect(
      getVisibleProjectSessionIds({
        isCollapsed: true,
        isProjectGroup: true,
        isToggleEnabled: true,
        sessionIds,
      })
    ).toEqual(sessionIds.slice(0, PROJECT_SESSION_LIST_COLLAPSED_COUNT));
  });

  test('uses the configured Show less session count', () => {
    const configuredSessionIds = Array.from({ length: 12 }, (_, index) => `session-${index + 1}`);
    expect(
      getVisibleProjectSessionIds({
        collapsedCount: 10,
        isCollapsed: true,
        isProjectGroup: true,
        isToggleEnabled: true,
        sessionIds: configuredSessionIds,
      })
    ).toEqual(configuredSessionIds.slice(0, 10));
  });

  test('does not trim non-project or temporarily disabled lists', () => {
    expect(
      getVisibleProjectSessionIds({
        isCollapsed: true,
        isProjectGroup: false,
        isToggleEnabled: true,
        sessionIds,
      })
    ).toEqual(sessionIds);

    expect(
      getVisibleProjectSessionIds({
        isCollapsed: true,
        isProjectGroup: true,
        isToggleEnabled: false,
        sessionIds,
      })
    ).toEqual(sessionIds);
  });
});

describe('getProjectSessionListCollapsedHeight', () => {
  function createRect(top: number, bottom: number): DOMRect {
    return {
      bottom,
      height: bottom - top,
      left: 0,
      right: 0,
      toJSON: () => ({}),
      top,
      width: 0,
      x: 0,
      y: top,
    } as DOMRect;
  }

  function createMeasuredElement(top: number, bottom: number): HTMLElement {
    return {
      getBoundingClientRect: () => createRect(top, bottom),
    } as HTMLElement;
  }

  function createSessionElement(sessionId: string, top: number, bottom: number): HTMLElement {
    const frame = createMeasuredElement(top, bottom);
    return {
      closest: (selector: string) => (selector === '.session-frame' ? frame : null),
      dataset: {
        sidebarSessionId: sessionId,
      },
    } as unknown as HTMLElement;
  }

  function createSessionListElement({
    bottom,
    moreToggleElement,
    sessions,
    top,
  }: {
    bottom: number;
    moreToggleElement?: HTMLElement;
    sessions: HTMLElement[];
    top: number;
  }): HTMLElement {
    return {
      getBoundingClientRect: () => createRect(top, bottom),
      querySelector: () => moreToggleElement ?? null,
      querySelectorAll: () => sessions,
    } as unknown as HTMLElement;
  }

  test('measures through the bottom of the last visible session frame', () => {
    const sessionListElement = createSessionListElement({
      bottom: 140,
      sessions: [
        createSessionElement('session-1', 10, 38),
        createSessionElement('session-2', 39, 67),
        createSessionElement('session-3', 68, 96),
      ],
      top: 10,
    });

    expect(
      getProjectSessionListCollapsedHeight({
        lastVisibleSessionId: 'session-2',
        sessionListElement,
      })
    ).toBe(57);
  });

  test('measures through the bottom collapsed-list more row', () => {
    const sessionListElement = createSessionListElement({
      bottom: 140,
      moreToggleElement: createMeasuredElement(68, 90),
      sessions: [
        createSessionElement('session-1', 10, 38),
        createSessionElement('session-2', 39, 67),
        createSessionElement('session-3', 91, 119),
      ],
      top: 10,
    });

    expect(
      getProjectSessionListCollapsedHeight({
        lastVisibleSessionId: 'session-2',
        sessionListElement,
      })
    ).toBe(80);
  });

  test('uses zero height for an empty collapsed list', () => {
    const sessionListElement = createSessionListElement({
      bottom: 10,
      sessions: [],
      top: 10,
    });

    expect(
      getProjectSessionListCollapsedHeight({
        lastVisibleSessionId: undefined,
        sessionListElement,
      })
    ).toBe(0);
  });
});

describe('getExpandedProjectSessionListScrollHeight', () => {
  test('calculates expanded scroll bounds from fixed reference row-stack geometry', () => {
    /*
     * CDXC:ProjectSessionLists 2026-06-30-12:55:
     * Expanded Show more lists use fixed row-stack math instead of DOM
     * measurement, and the setting value must fit that many complete session
     * rows. A value of 10 should not clip the tenth row at the top or bottom
     * of the inner project scroller.
     */
    expect(getExpandedProjectSessionListScrollHeight({ rowCount: 2 })).toBe(70);
    expect(getExpandedProjectSessionListScrollHeight({ rowCount: 10 })).toBe(350);
    expect(getExpandedProjectSessionListScrollHeight({ rowCount: -1 })).toBe(0);
  });
});
