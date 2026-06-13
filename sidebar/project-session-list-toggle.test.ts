import { describe, expect, test } from "vitest";
import {
  PROJECT_SESSION_LIST_COLLAPSED_COUNT,
  getProjectSessionListCollapsedHeight,
  getVisibleProjectSessionIds,
  normalizeStoredProjectSessionListCollapsedState,
} from "./project-session-list-toggle";

describe("normalizeStoredProjectSessionListCollapsedState", () => {
  test("keeps only explicitly collapsed project ids", () => {
    expect(
      normalizeStoredProjectSessionListCollapsedState({
        "project-1": true,
        "project-2": false,
        "project-3": "true",
        "": true,
      }),
    ).toEqual({
      "project-1": true,
    });
  });
});

describe("getVisibleProjectSessionIds", () => {
  const sessionIds = Array.from(
    { length: PROJECT_SESSION_LIST_COLLAPSED_COUNT + 2 },
    (_, index) => `session-${index + 1}`,
  );

  test("shows all project sessions by default", () => {
    expect(
      getVisibleProjectSessionIds({
        isCollapsed: false,
        isProjectGroup: true,
        isToggleEnabled: true,
        sessionIds,
      }),
    ).toEqual(sessionIds);
  });

  test("shows the default project session count after Show less is selected", () => {
    expect(
      getVisibleProjectSessionIds({
        isCollapsed: true,
        isProjectGroup: true,
        isToggleEnabled: true,
        sessionIds,
      }),
    ).toEqual(sessionIds.slice(0, PROJECT_SESSION_LIST_COLLAPSED_COUNT));
  });

  test("uses the configured Show less session count", () => {
    const configuredSessionIds = Array.from({ length: 12 }, (_, index) => `session-${index + 1}`);
    expect(
      getVisibleProjectSessionIds({
        collapsedCount: 10,
        isCollapsed: true,
        isProjectGroup: true,
        isToggleEnabled: true,
        sessionIds: configuredSessionIds,
      }),
    ).toEqual(configuredSessionIds.slice(0, 10));
  });

  test("does not trim non-project or temporarily disabled lists", () => {
    expect(
      getVisibleProjectSessionIds({
        isCollapsed: true,
        isProjectGroup: false,
        isToggleEnabled: true,
        sessionIds,
      }),
    ).toEqual(sessionIds);

    expect(
      getVisibleProjectSessionIds({
        isCollapsed: true,
        isProjectGroup: true,
        isToggleEnabled: false,
        sessionIds,
      }),
    ).toEqual(sessionIds);
  });
});

describe("getProjectSessionListCollapsedHeight", () => {
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

  function createSessionElement(sessionId: string, top: number, bottom: number): HTMLElement {
    const frame = {
      getBoundingClientRect: () => createRect(top, bottom),
    } as HTMLElement;
    return {
      closest: (selector: string) => (selector === ".session-frame" ? frame : null),
      dataset: {
        sidebarSessionId: sessionId,
      },
    } as unknown as HTMLElement;
  }

  function createSessionListElement({
    bottom,
    sessions,
    top,
  }: {
    bottom: number;
    sessions: HTMLElement[];
    top: number;
  }): HTMLElement {
    return {
      getBoundingClientRect: () => createRect(top, bottom),
      querySelectorAll: () => sessions,
    } as unknown as HTMLElement;
  }

  test("measures through the bottom of the last visible session frame", () => {
    const sessionListElement = createSessionListElement({
      bottom: 140,
      sessions: [
        createSessionElement("session-1", 10, 38),
        createSessionElement("session-2", 39, 67),
        createSessionElement("session-3", 68, 96),
      ],
      top: 10,
    });

    expect(
      getProjectSessionListCollapsedHeight({
        lastVisibleSessionId: "session-2",
        sessionListElement,
      }),
    ).toBe(57);
  });

  test("uses zero height for an empty collapsed list", () => {
    const sessionListElement = createSessionListElement({
      bottom: 10,
      sessions: [],
      top: 10,
    });

    expect(
      getProjectSessionListCollapsedHeight({
        lastVisibleSessionId: undefined,
        sessionListElement,
      }),
    ).toBe(0);
  });
});
