import { describe, expect, test } from "vite-plus/test";
import type { SidebarStoryArgs } from "./sidebar-story-fixtures";
import { createSidebarStoryMessage as createFixtureMessage } from "./sidebar-story-fixtures";
import {
  createSidebarStoryMessage as createWorkspaceMessage,
  createSidebarStoryWorkspace,
} from "./sidebar-story-workspace";

const DEFAULT_STORY_ARGS: SidebarStoryArgs = {
  createSessionOnSidebarDoubleClick: false,
  debuggingMode: false,
  fixture: "default",
  highlightedVisibleCount: 1,
  isFocusModeActive: false,
  renameSessionOnDoubleClick: false,
  showCloseButtonOnSessionCards: true,
  showSessionCloseContextMenuAction: false,
  showSessionCommandCopyActions: false,
  showSessionDetailsCopyAction: false,
  theme: "dark-blue",
  viewMode: "grid",
  visibleCount: 1,
};

describe("sidebar story messages", () => {
  test("fixture hydrate messages include previous sessions and revision fields", () => {
    const message = createFixtureMessage(DEFAULT_STORY_ARGS);

    expect(message.previousSessions).toEqual([]);
    expect(message.revision).toBe(1);
    expect(message.type).toBe("hydrate");
  });

  test("workspace session state messages include previous sessions and revision fields", () => {
    const workspace = createSidebarStoryWorkspace(createFixtureMessage(DEFAULT_STORY_ARGS));
    const message = createWorkspaceMessage(workspace);

    expect(message.previousSessions).toEqual([]);
    expect(message.revision).toBe(1);
    expect(message.type).toBe("sessionState");
  });

  test("fixture messages can opt into session details copy actions", () => {
    const message = createFixtureMessage({
      ...DEFAULT_STORY_ARGS,
      showSessionDetailsCopyAction: true,
    });

    expect(message.hud.settings?.showSessionDetailsCopyAction).toBe(true);
  });
});
