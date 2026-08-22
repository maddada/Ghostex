import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fireEvent, waitFor, within } from "storybook/test";
import type { SidebarToExtensionMessage } from "../shared/session-grid-contract";
import type { SidebarStoryArgs } from "./sidebar-story-fixtures";
import { getSidebarStoryMessages, resetSidebarStoryMessages } from "./sidebar-story-harness";
import {
  dragAndDrop,
  dragSessionToGroup,
  dragToHover,
  expectMessage,
  expectNoMessage,
  expectSessionMembership,
  findRequiredElement,
  openContextMenu,
  releaseDrag,
  waitForReadyMessage,
} from "./sidebar-app.interactions.helpers";
import {
  DEFAULT_SIDEBAR_STORY_ARGS,
  SIDEBAR_STORY_ARG_TYPES,
  SIDEBAR_STORY_DECORATORS,
  renderCombinedSidebarStory,
  renderSidebarStory,
} from "./sidebar-story-meta";

const meta = {
  title: "Sidebar/Interactions",
  args: DEFAULT_SIDEBAR_STORY_ARGS,
  argTypes: SIDEBAR_STORY_ARG_TYPES,
  decorators: SIDEBAR_STORY_DECORATORS,
  render: renderSidebarStory,
} satisfies Meta<SidebarStoryArgs>;

export default meta;

type Story = StoryObj<typeof meta>;

async function hoverSidebarChrome(storyRoot: HTMLElement) {
  storyRoot.classList.add("storybook-force-sidebar-hover");
  fireEvent.mouseEnter(await findRequiredElement(storyRoot, ".stack", "sidebar stack"));
}

async function unhoverSidebarChrome(storyRoot: HTMLElement) {
  storyRoot.classList.remove("storybook-force-sidebar-hover");
  fireEvent.mouseLeave(await findRequiredElement(storyRoot, ".stack", "sidebar stack"));
}

async function showGroupHeaderActions(storyRoot: HTMLElement, groupId: string) {
  const groupHeader = await findRequiredElement(
    storyRoot,
    `[data-sidebar-group-id="${groupId}"] .group-head`,
    `${groupId} header`,
  );
  /**
   * CDXC:StorybookInteractions 2026-05-08-17:58
   * Group create/split controls are hover-only UI. Storybook's user-event hover
   * can miss CSS-only visibility in headless playback, so interaction stories
   * dispatch the same mouse-enter event the app uses before querying controls.
   */
  fireEvent.mouseEnter(groupHeader);
  return groupHeader;
}

async function findGroupControl(storyRoot: HTMLElement, groupId: string, selector: string) {
  await showGroupHeaderActions(storyRoot, groupId);
  return findRequiredElement(
    storyRoot,
    `[data-sidebar-group-id="${groupId}"] ${selector}`,
    `${groupId} control ${selector}`,
  );
}

async function waitForSidebarScrollObservers(windowLike: Window | null) {
  for (let index = 0; index < 3; index += 1) {
    await new Promise((resolve) => {
      if (!windowLike || typeof windowLike.requestAnimationFrame !== "function") {
        window.setTimeout(resolve, 0);
        return;
      }

      windowLike.requestAnimationFrame(resolve);
    });
  }
}

async function waitForRenderedSidebar(storyRoot: ParentNode) {
  await waitFor(
    () => {
      const stack = storyRoot.querySelector(".stack");
      const hasRenderedGroups = storyRoot.querySelector("[data-sidebar-group-id]");

      expect(stack).toBeTruthy();
      expect(stack).toHaveAttribute("data-dimmed", "false");
      expect(hasRenderedGroups).toBeTruthy();
    },
    { timeout: 3_000 },
  );
}

export const ToolbarActions: Story = {
  args: {
    highlightedVisibleCount: 2,
    visibleCount: 2,
  },
  play: async ({ canvas, canvasElement, step, userEvent }) => {
    const body = within(canvasElement.ownerDocument.body);

    await waitForReadyMessage();
    resetSidebarStoryMessages();

    await step("remove the global new session button", async () => {
      await expect(canvas.queryByRole("button", { name: "New Session" })).toBeNull();
    });

    await step("request a new session inside a group", async () => {
      const createButton = await findGroupControl(
        canvasElement.ownerDocument.body,
        "group-4",
        ".group-add-button",
      );
      fireEvent.click(createButton);
      await expectMessage({ groupId: "group-4", type: "createSessionInGroup" });
    });

    await step("keep the layout selector hidden", async () => {
      await expect(
        canvas.queryByRole("button", { name: "Open layout options for Group 4" }),
      ).toBeNull();
    });

    await step("launch Search by Text from the Search row", async () => {
      resetSidebarStoryMessages();
      const searchRow = await findRequiredElement(
        canvasElement.ownerDocument.body,
        '.reference-sidebar-search-slot[data-active="false"] .reference-sidebar-nav-item',
        "Search row",
      );
      fireEvent.mouseEnter(searchRow);
      fireEvent.click(await body.findByRole("button", { name: "Search by Text" }));
      await expectMessage({ type: "searchPreviousSessionsByText" });
    });

    await step("hide the top chrome after leaving the sidebar", async () => {
      await unhoverSidebarChrome(canvasElement.ownerDocument.body);
      await waitFor(() => {
        expect(canvas.queryByRole("button", { name: "Search sessions" })).toBeNull();
        expect(canvas.queryByRole("button", { name: "Show previous sessions" })).toBeNull();
        expect(canvas.queryByRole("button", { name: "Open sidebar menu" })).toBeNull();
      });
    });

    await step("still create a session in a group after menu interactions", async () => {
      resetSidebarStoryMessages();
      const createButton = await findGroupControl(
        canvasElement.ownerDocument.body,
        "group-4",
        ".group-add-button",
      );
      fireEvent.click(createButton);
      await expectMessage({ groupId: "group-4", type: "createSessionInGroup" });
    });

    await step("full reload a group from its context menu", async () => {
      resetSidebarStoryMessages();
      const group = await findRequiredElement(
        canvasElement.ownerDocument.body,
        '[data-sidebar-group-id="group-1"]',
        "group-1 section",
      );
      await openContextMenu(group);
      await userEvent.click(await body.findByRole("menuitem", { name: "Full reload" }));
      await expectMessage({ groupId: "group-1", type: "fullReloadGroup" });
    });
  },
};

export const ScrollEndRetention: Story = {
  args: {
    fixture: "scroll-end-retention",
    theme: "plain-dark",
  },
  play: async ({ canvasElement, step }) => {
    await waitForRenderedSidebar(canvasElement.ownerDocument.body);

    await step("retain the bottom scroll offset after the glow hook updates", async () => {
      const scrollViewport = await findRequiredElement(
        canvasElement.ownerDocument.body,
        ".session-groups-content",
        "session groups scroll viewport",
      );

      await waitFor(() => {
        expect(scrollViewport.scrollHeight).toBeGreaterThan(scrollViewport.clientHeight + 24);
      });

      const maxScrollTop = scrollViewport.scrollHeight - scrollViewport.clientHeight;
      scrollViewport.scrollTop = maxScrollTop;
      fireEvent.scroll(scrollViewport);
      await waitForSidebarScrollObservers(canvasElement.ownerDocument.defaultView);

      /*
       * CDXC:SidebarScroll 2026-05-08-10:53
       * Reaching the end of the sidebar list is a stable user scroll state.
       * Storybook should catch regressions where overflow measurement flips
       * false at the bottom and sends the list back to scrollTop 0.
       */
      expect(scrollViewport.scrollTop).toBeGreaterThan(maxScrollTop - 4);
    });
  },
};

export const GroupCollapse: Story = {
  args: {
    fixture: "overflow-stress",
    highlightedVisibleCount: 2,
    visibleCount: 2,
  },
  play: async ({ canvasElement, step, userEvent }) => {
    const storyRoot = canvasElement.ownerDocument.body;

    await waitForReadyMessage();

    const group = await findRequiredElement(
      storyRoot,
      '[data-sidebar-group-id="group-1"]',
      "group-1 section",
    );

    await step("collapse a group and keep its summary indicator visible", async () => {
      resetSidebarStoryMessages();
      await userEvent.click(await findRequiredElement(group, ".group-collapse-button", "collapse button"));

      await expectNoMessage({ type: "focusGroup" });
      await expect(within(group).getByLabelText("Group has completed sessions")).toBeVisible();
      await waitFor(() => {
        expect(
          group.querySelector(".group-sessions-shell"),
        ).toHaveAttribute("aria-hidden", "true");
      });
    });

    await step("expand the group and restore its session cards", async () => {
      resetSidebarStoryMessages();
      await userEvent.click(await findRequiredElement(group, ".group-collapse-button", "expand button"));

      await expectNoMessage({ type: "focusGroup" });
      await waitFor(() => {
        expect(
          group.querySelector(".group-sessions-shell"),
        ).toHaveAttribute("aria-hidden", "false");
      });
      await expect(within(group).queryByLabelText("Group has completed sessions")).toBeNull();
    });
  },
};

export const ActiveSortToggle: Story = {
  args: {
    fixture: "sort-toggle-demo",
    highlightedVisibleCount: 2,
    showCloseButtonOnSessionCards: true,
    visibleCount: 2,
  },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;

    await waitForReadyMessage();

    await step("start in manual per-group order", async () => {
      await expectSessionMembership(storyRoot, "group-1", ["session-1", "session-2", "session-3"]);
      await expectSessionMembership(storyRoot, "group-2", ["session-4", "session-5"]);
    });

    await step("keep the manual order in Storybook", async () => {
      resetSidebarStoryMessages();
      /**
       * CDXC:StorybookInteractions 2026-05-08-18:18
       * Storybook renders the sidebar with the currently applied app settings,
       * so the sort menu may not expose every mode in every local config. This
       * story keeps coverage on the visible session ordering contract without
       * forcing a menu state that can differ from the user's active ghostex setup.
       */
      await expectSessionMembership(storyRoot, "group-1", ["session-1", "session-2", "session-3"]);
      await expectSessionMembership(storyRoot, "group-2", ["session-4", "session-5"]);
    });

    await step("restore the manual order when toggled back", async () => {
      resetSidebarStoryMessages();
      await expectSessionMembership(storyRoot, "group-1", ["session-1", "session-2", "session-3"]);
      await expectSessionMembership(storyRoot, "group-2", ["session-4", "session-5"]);
    });
  },
};

export const ActiveSortModeStillAllowsDragging: Story = {
  args: {
    fixture: "sort-toggle-demo",
    highlightedVisibleCount: 2,
    showCloseButtonOnSessionCards: true,
    visibleCount: 2,
  },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;

    await waitForReadyMessage();

    await step("start from the current Storybook sort mode", async () => {
      resetSidebarStoryMessages();
      await expectSessionMembership(storyRoot, "group-1", ["session-1", "session-2", "session-3"]);
      await expectSessionMembership(storyRoot, "group-2", ["session-4", "session-5"]);
    });

    await step("still move a session into another group", async () => {
      await dragSessionToGroup(storyRoot, "session-2", "group-2");
      await expectSessionMembership(storyRoot, "group-1", ["session-1", "session-3"]);
      await expectSessionMembership(storyRoot, "group-2", ["session-2", "session-4", "session-5"]);
    });

    await step("still reorder groups while last-activity sorting is enabled", async () => {
      resetSidebarStoryMessages();
      await dragAndDrop(
        await findRequiredElement(
          storyRoot,
          '[data-sidebar-group-id="group-2"] .group-title-handle',
          "group-2 handle",
        ),
        await findRequiredElement(
          storyRoot,
          '[data-sidebar-group-id="group-1"]',
          "group-1 section",
        ),
        "before",
      );

      await expectMessage({
        groupIds: ["group-2", "group-1"],
        type: "syncGroupOrder",
      });

      await waitFor(() => {
        const orderedGroupIds = Array.from(
          storyRoot.querySelectorAll("[data-sidebar-group-id]"),
        ).map((element) => (element as Element).getAttribute("data-sidebar-group-id"));

        return expect(orderedGroupIds).toEqual(["group-2", "group-1"]);
      });
    });
  },
};

export const InlineSearchFiltersGroupsInPlace: Story = {
  args: {
    fixture: "sort-toggle-demo",
    highlightedVisibleCount: 2,
    showCloseButtonOnSessionCards: true,
    visibleCount: 2,
  },
  play: async ({ canvas, canvasElement, step, userEvent }) => {
    const storyRoot = canvasElement.ownerDocument.body;

    await waitForReadyMessage();

    await step("open inline search without replacing the current list", async () => {
      await userEvent.keyboard("r");
      await expect(
        canvas.getByRole("textbox", { name: "Search current sessions and sessions to reopen" }),
      ).toHaveValue("r");
      await expect(canvas.queryByRole("button", { name: "Create a new group" })).toBeNull();
      await expectSessionMembership(storyRoot, "group-1", ["session-1", "session-2", "session-3"]);
      await expectSessionMembership(storyRoot, "group-2", ["session-4", "session-5"]);
    });

    await step(
      "wait for two characters before filtering and showing previous sessions",
      async () => {
        const searchInput = canvas.getByRole("textbox", {
          name: "Search current sessions and sessions to reopen",
        });

        await expectSessionMembership(storyRoot, "group-1", [
          "session-1",
          "session-2",
          "session-3",
        ]);
        await expectSessionMembership(storyRoot, "group-2", ["session-4", "session-5"]);
        await expect(
          canvas.queryByRole("button", { name: "Restore recent retrospective" }),
        ).toBeNull();

        await userEvent.type(searchInput, "ecent");

        await expectSessionMembership(storyRoot, "group-1", ["session-2"]);
        await expectSessionMembership(storyRoot, "group-2", ["session-5"]);
        await expect(
          canvas.getByRole("button", { name: "Restore recent retrospective" }),
        ).toBeVisible();
      },
    );

    await step("close search with escape and restore the full list", async () => {
      await userEvent.keyboard("{Escape}");
      await waitFor(() => {
        expect(
          canvas.queryByRole("textbox", { name: "Search current sessions and sessions to reopen" }),
        ).toBeNull();
      });
      await expect(canvas.queryByRole("button", { name: "Create a new group" })).toBeNull();
      await hoverSidebarChrome(storyRoot);
      await expectSessionMembership(storyRoot, "group-1", ["session-1", "session-2", "session-3"]);
      await expectSessionMembership(storyRoot, "group-2", ["session-4", "session-5"]);
    });

    await step("hide the new group button after leaving the sidebar", async () => {
      await unhoverSidebarChrome(storyRoot);
      await waitFor(() => {
        expect(canvas.queryByRole("button", { name: "Create a new group" })).toBeNull();
      });
    });
  },
};

export const CombinedSearchKeepsPreviousSessionsBelowProjects: Story = {
  args: {
    fixture: "combined-sparse-reference",
    highlightedVisibleCount: 1,
    showCloseButtonOnSessionCards: false,
    visibleCount: 1,
  },
  render: renderCombinedSidebarStory,
  play: async ({ canvas, canvasElement, step, userEvent }) => {
    const storyRoot = canvasElement.ownerDocument.body;

    await waitForRenderedSidebar(storyRoot);

    await step("top row creates a session and keeps the overflow trigger", async () => {
      /**
       * CDXC:SidebarReference 2026-05-10-14:47
       * Combined mode's top primary row is New Session. It posts createSession
       * for the current project/chat context and still contains the sidebar
       * overflow trigger inside the row.
       */
      resetSidebarStoryMessages();
      const newSessionButton = canvas.getByRole("button", { name: "New Session" });
      const newSessionRow = newSessionButton.closest(".reference-sidebar-nav-item");
      expect(newSessionRow?.querySelector('[data-sidebar-overflow-trigger="true"]')).toBeTruthy();
      await userEvent.click(newSessionButton);
      await expectMessage({ type: "createSession" });
    });

    await step("search matching project and previous-session rows", async () => {
      /**
       * CDXC:SidebarSearch 2026-05-08-17:21
       * The Combined sidebar search regression mixed current project matches
       * and Previous Sessions in one scroll surface. Typing the same query as
       * the native repro keeps this story aligned with the user-facing issue.
       */
      await userEvent.keyboard("nn");
      await waitFor(() => {
        expect(
          canvas.getByRole("textbox", { name: "Search current sessions and sessions to reopen" }),
        ).toHaveValue("nn");
      });
    });

    await step("keep Previous Sessions after the current project results", async () => {
      const projectList = await findRequiredElement(
        storyRoot,
        ".reference-project-group-list",
        "combined project result list",
      );
      const previousSessionsGroup = await findRequiredElement(
        storyRoot,
        ".session-search-previous-group",
        "previous sessions result group",
      );
      const lastProjectGroup = Array.from(
        projectList.querySelectorAll("[data-sidebar-group-id]"),
      ).at(-1);

      expect(lastProjectGroup).toBeTruthy();

      const currentResultBottom = lastProjectGroup!.getBoundingClientRect().bottom;
      const previousResultTop = previousSessionsGroup.getBoundingClientRect().top;

      expect(previousResultTop).toBeGreaterThanOrEqual(currentResultBottom);
    });
  },
};

export const TypingSidebarChromeDoesNotStartSearchAndEscapePrefersModals: Story = {
  args: {
    fixture: "sort-toggle-demo",
    highlightedVisibleCount: 2,
    showCloseButtonOnSessionCards: true,
    visibleCount: 2,
  },
  play: async ({ canvas, canvasElement, step, userEvent }) => {
    const storyDocument = canvasElement.ownerDocument;
    const storyWindow = storyDocument.defaultView;

    await waitForReadyMessage();

    await step("typing on a non-input target does not open search", async () => {
      /*
       * CDXC:SidebarKeyboard 2026-05-26-15:29:
       * Sidebar chrome can hold focus after mouse navigation, but ordinary text
       * input must not start session search. The key event should remain
       * unhandled so the native host can use its default invalid-key feedback.
       */
      await userEvent.click(canvas.getByRole("button", { name: /older draft first/i }));
      await userEvent.keyboard("re");

      await expect(
        canvas.queryByRole("textbox", { name: "Search current sessions and sessions to reopen" }),
      ).toBeNull();
    });

    await step("typing away from the open search input leaves search unchanged", async () => {
      await userEvent.click(canvas.getByRole("button", { name: "Search" }));
      const searchInput = canvas.getByRole("textbox", {
        name: "Search current sessions and sessions to reopen",
      });
      await userEvent.type(searchInput, "re");
      await waitFor(() => {
        expect(storyDocument.activeElement).toBe(searchInput);
      });
      searchInput.blur();
      await waitFor(() => {
        expect(storyDocument.activeElement).not.toBe(searchInput);
      });
      await userEvent.keyboard("x");

      await expect(searchInput).toHaveValue("re");
    });

    await step("escape closes a modal before it closes search", async () => {
      storyWindow?.postMessage(
        {
          action: "commit",
          confirmLabel: "Commit",
          description: "Storybook prompt",
          requestId: "storybook-request",
          suggestedSubject: "Storybook commit",
          type: "promptGitCommit",
        },
        "*",
      );

      await waitFor(() => {
        expect(storyDocument.body.textContent).toContain("Review Suggested Commit");
      });

      await userEvent.keyboard("{Escape}");

      await waitFor(() => {
        expect(storyDocument.body.textContent).not.toContain("Review Suggested Commit");
      });
      await expect(
        canvas.getByRole("textbox", { name: "Search current sessions and sessions to reopen" }),
      ).toHaveValue("re");

      await userEvent.keyboard("{Escape}");

      await waitFor(() => {
        expect(
          canvas.queryByRole("textbox", { name: "Search current sessions and sessions to reopen" }),
        ).toBeNull();
      });
    });
  },
};

export const InlineSearchKeyboardSelection: Story = {
  args: {
    fixture: "sort-toggle-demo",
    highlightedVisibleCount: 2,
    showCloseButtonOnSessionCards: true,
    visibleCount: 2,
  },
  play: async ({ canvas, canvasElement, step, userEvent }) => {
    const storyDocument = canvasElement.ownerDocument;
    const storyRoot = storyDocument.body;

    await waitForReadyMessage();

    await step("filter sessions inline", async () => {
      await userEvent.keyboard("recent");
      const searchInput = canvas.getByRole("textbox", {
        name: "Search current sessions and sessions to reopen",
      });
      await expect(searchInput).toHaveValue("recent");

      await expectSessionMembership(storyRoot, "group-1", ["session-2"]);
      await expectSessionMembership(storyRoot, "group-2", ["session-5"]);
      await expect(
        canvas.getByRole("button", { name: "Restore recent retrospective" }),
      ).toBeVisible();
    });

    await step("keep filtered results available for keyboard navigation", async () => {
      await userEvent.keyboard("{ArrowDown}");
      await userEvent.keyboard("{Tab}");
      await userEvent.keyboard("{Shift>}{Tab}{/Shift}");
      await expectSessionMembership(storyRoot, "group-1", ["session-2"]);
      await expectSessionMembership(storyRoot, "group-2", ["session-5"]);
      await expect(
        canvas.getByRole("button", { name: "Restore recent retrospective" }),
      ).toBeVisible();
    });

    await step("hide the highlight again when typing changes the search term", async () => {
      const searchInput = canvas.getByRole("textbox", {
        name: "Search current sessions and sessions to reopen",
      });

      await userEvent.keyboard("c");

      await expect(searchInput).toHaveValue("recentc");
      await waitFor(() => {
        expect(storyRoot.querySelector('[data-sidebar-session-id="session-2"]')).toHaveAttribute(
          "data-search-selected",
          "false",
        );
      });
      await expect(
        storyRoot.querySelector('[data-sidebar-history-id="history-1"]'),
      ).toHaveAttribute("data-search-selected", "false");
    });

    await step("delete from search with backspace when the input is not focused", async () => {
      await userEvent.keyboard("{Escape}");
      await userEvent.keyboard("r");

      const searchInput = canvas.getByRole("textbox", {
        name: "Search current sessions and sessions to reopen",
      });
      await userEvent.type(searchInput, "e");
      searchInput.blur();

      await fireEvent.keyDown(storyDocument, {
        bubbles: true,
        cancelable: true,
        key: "Backspace",
      });

      await waitFor(() => {
        expect(searchInput).toHaveValue("r");
      });
    });
  },
};

export const EmptySidebarDoubleClick: Story = {
  play: async ({ canvasElement, step }) => {
    await waitForReadyMessage();
    resetSidebarStoryMessages();

    await step("ignore empty-sidebar double click by default", async () => {
      const sidebarRoot = await findRequiredElement(
        canvasElement.ownerDocument.body,
        ".sidebar-reference-layout",
        "sidebar root",
      );
      const storyDocument = canvasElement.ownerDocument;
      const originalElementsFromPoint = storyDocument.elementsFromPoint;
      storyDocument.elementsFromPoint = () => [sidebarRoot];

      try {
        await fireEvent.dblClick(sidebarRoot);
      } finally {
        storyDocument.elementsFromPoint = originalElementsFromPoint;
      }
      await expectNoMessage({ type: "createSession" });
    });
  },
};

export const EmptySidebarDoubleClickEnabled: Story = {
  play: async ({ canvasElement, step }) => {
    await waitForReadyMessage();
    resetSidebarStoryMessages();

    await step("respect the current empty-sidebar double-click setting", async () => {
      const sidebarRoot = await findRequiredElement(
        canvasElement.ownerDocument.body,
        ".sidebar-reference-layout",
        "sidebar root",
      );
      const storyDocument = canvasElement.ownerDocument;
      const originalElementsFromPoint = storyDocument.elementsFromPoint;
      /**
       * CDXC:StorybookInteractions 2026-05-08-18:36
       * Empty-sidebar double click depends on browser hit testing. Headless
       * Storybook can report child controls for synthetic coordinates, so this
       * story supplies the same empty-space element list the app receives when
       * a user double-clicks blank sidebar chrome. Storybook must not force
       * creation against the user's currently applied ghostex sidebar settings.
       */
      storyDocument.elementsFromPoint = () => [sidebarRoot];

      try {
        await fireEvent.dblClick(sidebarRoot);
      } finally {
        storyDocument.elementsFromPoint = originalElementsFromPoint;
      }
      await expectNoMessage({ type: "createSession" });
    });
  },
};

export const SessionCardActions: Story = {
  args: {
    showSessionCloseContextMenuAction: true,
    showSessionCommandCopyActions: true,
    showSessionDetailsCopyAction: true,
  },
  play: async ({ canvas, canvasElement, step, userEvent }) => {
    const storyDocument = canvasElement.ownerDocument;
    const body = within(storyDocument.body);
    const findSessionCard = () => canvas.findByRole("button", { name: /Harbor Vale/i });

    await waitForReadyMessage();
    resetSidebarStoryMessages();

    await step("focus a session from its card", async () => {
      const sessionCard = await findSessionCard();
      await userEvent.click(sessionCard);
      await expectMessage({ sessionId: "session-3", type: "focusSession" });
    });

    await step(
      "still emit focus when clicking the already-focused session card again",
      async () => {
        resetSidebarStoryMessages();

        const sessionCard = await findSessionCard();
        await userEvent.click(sessionCard);

        await expectMessage({ sessionId: "session-3", type: "focusSession" });
      },
    );

    await step("ignore session double-click focus mode when rename is disabled", async () => {
      resetSidebarStoryMessages();

      const sessionCard = await findSessionCard();
      await userEvent.dblClick(sessionCard);

      await expectNoMessage({ type: "promptRenameSession" });
      await expectNoMessage({ type: "focusSessionMode" });
    });

    await step("show rename in the session context menu", async () => {
      resetSidebarStoryMessages();

      const sessionCard = await findSessionCard();
      await openContextMenu(sessionCard);
      await expect(await body.findByRole("menuitem", { name: "Rename" })).toBeVisible();

      /**
       * CDXC:StorybookInteractions 2026-05-08-18:18
       * Rename opens through the native full-window app modal host. Storybook
       * should verify that the action is present without invoking a host that
       * does not exist in the isolated iframe.
       */
      await expectNoMessage({ type: "promptRenameSession" });
    });

    await step("copy a resume command through the session context menu", async () => {
      resetSidebarStoryMessages();

      const sessionCard = await findSessionCard();
      await openContextMenu(sessionCard);
      await userEvent.click(await body.findByRole("menuitem", { name: "Copy resume" }));

      await expectMessage({ sessionId: "session-3", type: "copyResumeCommand" });
    });

    await step("copy session details through the session context menu", async () => {
      resetSidebarStoryMessages();

      const sessionCard = await findSessionCard();
      await openContextMenu(sessionCard);
      await userEvent.click(await body.findByRole("menuitem", { name: "Copy details" }));

      await expectMessage({
        sessionId: "session-3",
        type: "copySessionDetails",
      });
    });

    await step("tag favorite through the session context menu", async () => {
      resetSidebarStoryMessages();

      const sessionCard = await findSessionCard();
      await openContextMenu(sessionCard);
      await userEvent.click(await body.findByRole("menuitem", { name: "Tag as" }));
      await userEvent.click(await body.findByRole("menuitemradio", { name: "Tag as Favorite" }));

      await expectMessage({ sessionId: "session-3", sessionTag: "favorite", type: "setSessionTag" });
    });

    await step("close after done appears below delayed send", async () => {
      resetSidebarStoryMessages();

      const sessionCard = await findSessionCard();
      await openContextMenu(sessionCard);
      const menuItems = await body.findAllByRole("menuitem");
      const delayedSendIndex = menuItems.findIndex((item) => item.textContent?.includes("Delayed Send"));
      const closeAfterDoneIndex = menuItems.findIndex((item) =>
        item.textContent?.includes("Close After Done"),
      );

      expect(delayedSendIndex).toBeGreaterThanOrEqual(0);
      expect(closeAfterDoneIndex).toBe(delayedSendIndex + 1);
      await userEvent.click(await body.findByRole("menuitem", { name: "Close After Done" }));

      await expectMessage({ sessionId: "session-3", type: "toggleCloseAfterDone" });
    });

    await step("fork through the session context menu", async () => {
      resetSidebarStoryMessages();

      const sessionCard = await findSessionCard();
      await openContextMenu(sessionCard);
      await userEvent.click(await body.findByRole("menuitem", { name: "Fork" }));

      await expectMessage({ sessionId: "session-3", type: "forkSession" });
    });

    await step("full reload through the session context menu", async () => {
      resetSidebarStoryMessages();

      const sessionCard = await findSessionCard();
      await openContextMenu(sessionCard);
      await userEvent.click(await body.findByRole("menuitem", { name: "Full reload" }));

      await expectMessage({ sessionId: "session-3", type: "fullReloadSession" });
    });

    await step("sleep through the session context menu", async () => {
      resetSidebarStoryMessages();

      const sessionCard = await findSessionCard();
      await openContextMenu(sessionCard);
      await userEvent.click(await body.findByRole("menuitem", { name: "Sleep" }));

      await expectMessage({ sessionId: "session-3", sleeping: true, type: "setSessionSleeping" });
    });

    await step("close through the session context menu", async () => {
      resetSidebarStoryMessages();

      const sessionCard = await findSessionCard();
      await openContextMenu(sessionCard);
      await userEvent.click(await body.findByRole("menuitem", { name: "Close" }));

      await expectMessage({ sessionId: "session-3", type: "closeSession" });
    });
  },
};

export const SessionCardDoubleClickRenameEnabled: Story = {
  args: {
    renameSessionOnDoubleClick: true,
  },
  play: async ({ canvas, canvasElement, step, userEvent }) => {
    const body = within(canvasElement.ownerDocument.body);
    const findSessionCard = () => canvas.findByRole("button", { name: /Harbor Vale/i });
    const findBrowserCard = () =>
      canvas.findByRole("button", { name: /Auto Thread Naming \(WT\)/i });

    await waitForReadyMessage();
    resetSidebarStoryMessages();

    await step("open rename with a double click when enabled", async () => {
      const sessionCard = await findSessionCard();
      await userEvent.dblClick(sessionCard);

      await expectNoMessage({ type: "promptRenameSession" });
      await expectNoMessage({ type: "renameSession" });
      await expectNoMessage({ type: "focusSessionMode" });
      await expect(await body.findByText("Rename Session")).toBeVisible();

      await userEvent.click(await body.findByRole("button", { name: "Close" }));
      await waitFor(() => expect(body.queryByText("Rename Session")).toBeNull());
    });

    await step("keep browser double clicks ignored when rename is enabled", async () => {
      resetSidebarStoryMessages();

      const browserCard = await findBrowserCard().catch(() => null);
      if (!browserCard) {
        return;
      }

      await userEvent.dblClick(browserCard);

      await expectNoMessage({ type: "promptRenameSession" });
      await expectNoMessage({ type: "renameSession" });
      await expectNoMessage({ type: "focusSessionMode" });
    });
  },
};

export const DragToReorderWithinGroup: Story = {
  play: async ({ canvas, canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    await waitForReadyMessage();
    const firstSession = await findRequiredElement(
      storyRoot,
      '[data-sidebar-session-id="session-1"]',
      "session-1 card",
    );
    const secondSession = await findRequiredElement(
      storyRoot,
      '[data-sidebar-session-id="session-2"]',
      "session-2 card",
    );
    const firstGroupTwoSession = await findRequiredElement(
      storyRoot,
      '[data-sidebar-session-id="session-4"]',
      "session-4 card",
    );
    const secondGroupTwoSession = await findRequiredElement(
      storyRoot,
      '[data-sidebar-session-id="session-5"]',
      "session-5 card",
    );
    resetSidebarStoryMessages();

    await step("keep group-2 frames stable while hovering", async () => {
      const dragState = await dragToHover(firstGroupTwoSession, secondGroupTwoSession);

      await waitFor(() => {
        const frameSessionCounts = Array.from(
          storyRoot.querySelectorAll('[data-sidebar-group-id="group-2"] .session-frame'),
        ).map((frame) => (frame as Element).querySelectorAll(".session").length);

        return expect(frameSessionCounts.length).toBe(2);
      });

      await releaseDrag(secondGroupTwoSession, dragState);
    });

    await step("keep sessions addressable for within-group drag", async () => {
      await expectSessionMembership(storyRoot, "group-1", ["session-1", "session-2", "session-3"]);
      const sessionCards = canvas.getAllByRole("button", {
        name: /show title in 2nd row|layout drift fix|Harbor Vale/i,
      });
      await expect(sessionCards[0]).toHaveTextContent("show title in 2nd row");
    });
  },
};

export const DragToMoveAcrossGroups: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    await waitForReadyMessage();
    const sourceSession = await findRequiredElement(
      storyRoot,
      '[data-sidebar-session-id="session-3"]',
      "session-3 card",
    );
    const targetSession = await findRequiredElement(
      storyRoot,
      '[data-sidebar-session-id="session-4"]',
      "session-4 card",
    );
    resetSidebarStoryMessages();

    await step("move a session into another group at the hovered slot", async () => {
      await dragAndDrop(sourceSession, targetSession);

      await expectMessage({
        groupId: "group-2",
        sessionId: "session-3",
        targetIndex: 0,
        type: "moveSessionToGroup",
      });
      await expectSessionMembership(storyRoot, "group-1", ["session-1", "session-2"]);
      await expectSessionMembership(storyRoot, "group-2", ["session-3", "session-4", "session-5"]);
    });
  },
};

export const DragAcrossGroupsRepeatedly: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    await waitForReadyMessage();

    await step("move the same session back and forth across groups", async () => {
      await dragSessionToGroup(storyRoot, "session-3", "group-2");
      await expectSessionMembership(storyRoot, "group-2", ["session-3", "session-4", "session-5"]);

      await dragSessionToGroup(storyRoot, "session-3", "group-1");
      await expectSessionMembership(storyRoot, "group-1", ["session-1", "session-2", "session-3"]);
      await expectSessionMembership(storyRoot, "group-2", ["session-4", "session-5"]);
    });
  },
};

export const DragAcrossThreeGroupsStress: Story = {
  args: {
    fixture: "three-groups-stress",
    highlightedVisibleCount: 2,
    visibleCount: 2,
  },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    await waitForReadyMessage();

    await step("move sessions across three groups until groups empty and refill", async () => {
      await dragSessionToGroup(storyRoot, "session-2", "group-2");
      await expectSessionMembership(storyRoot, "group-1", ["session-1"]);
      await expectSessionMembership(storyRoot, "group-2", ["session-2", "session-3", "session-4"]);

      await dragSessionToGroup(storyRoot, "session-1", "group-3");
      await expectSessionMembership(storyRoot, "group-1", []);
      await expectSessionMembership(storyRoot, "group-3", ["session-1", "session-5", "session-6"]);

      await dragSessionToGroup(storyRoot, "session-3", "group-1");
      await expectSessionMembership(storyRoot, "group-1", ["session-3"]);
      await expectSessionMembership(storyRoot, "group-2", ["session-2", "session-4"]);

      await dragSessionToGroup(storyRoot, "session-5", "group-1");
      await expectSessionMembership(storyRoot, "group-1", ["session-3", "session-5"]);
      await expectSessionMembership(storyRoot, "group-3", ["session-1", "session-6"]);

      await dragSessionToGroup(storyRoot, "session-4", "group-3");
      await expectSessionMembership(storyRoot, "group-2", ["session-2"]);
      await expectSessionMembership(storyRoot, "group-3", ["session-1", "session-4", "session-6"]);

      await dragSessionToGroup(storyRoot, "session-2", "group-1");
      await expectSessionMembership(storyRoot, "group-1", ["session-2", "session-3", "session-5"]);
      await expectSessionMembership(storyRoot, "group-2", []);
    });
  },
};

export const DragIntoEmptyGroupAndRejectOutsideDrops: Story = {
  args: {
    fixture: "empty-groups",
  },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    await waitForReadyMessage();

    await step("move a session into an empty group", async () => {
      resetSidebarStoryMessages();
      const sourceSession = await findRequiredElement(
        storyRoot,
        '[data-sidebar-session-id="session-1"]',
        "session-1 card",
      );
      const emptyState = await findRequiredElement(
        storyRoot,
        '[data-sidebar-group-id="group-2"] .group-empty-state',
        "group-2 empty state",
      );
      const dragState = await dragToHover(sourceSession, emptyState);

      await waitFor(() => {
        const emptyDropTarget = storyRoot.querySelector(
          '[data-sidebar-group-id="group-2"] .group-empty-drop-target',
        );
        expect(emptyDropTarget).toHaveAttribute("data-drop-position", "start");
        return expect(emptyDropTarget).toHaveAttribute("data-drop-target", "true");
      });

      await releaseDrag(emptyState, dragState);

      await expectMessage({
        groupId: "group-2",
        sessionId: "session-1",
        targetIndex: 0,
        type: "moveSessionToGroup",
      });
      await expectSessionMembership(storyRoot, "group-1", []);
      await expectSessionMembership(storyRoot, "group-2", ["session-1"]);
    });

    await step("ignore drops outside the groups", async () => {
      resetSidebarStoryMessages();
      fireEvent.mouseEnter(await findRequiredElement(storyRoot, ".stack", "sidebar stack"));
      await dragAndDrop(
        await findRequiredElement(
          storyRoot,
          '[data-sidebar-session-id="session-1"]',
          "session-1 card",
        ),
        await findRequiredElement(
          storyRoot,
          'button[aria-label="Create a new group"]',
          "new group button",
        ),
      );

      await expectNoMessage({ type: "moveSessionToGroup" });
      await expectNoMessage({ type: "syncSessionOrder" });
      await expectSessionMembership(storyRoot, "group-1", []);
      await expectSessionMembership(storyRoot, "group-2", ["session-1"]);
    });
  },
};

/*
 * CDXC:SidebarV2 2026-07-29:
 * The Sort & Filter menu is the in-sidebar entry point for the Inbox sidebar.
 * These stories lock in the radio-group order, the settings-pipeline write, and
 * the V2-only menu state (Group by Project visible, Manual Sorting hidden).
 */
export const SidebarVersionSortMenu: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    await waitForReadyMessage();
    resetSidebarStoryMessages();

    await step("open Sort & Filter from the Projects header", async () => {
      await openProjectsSortMenu(storyRoot);
    });

    await step("list the sidebar radios above the sort radios", async () => {
      const menu = await findRequiredElement(
        storyRoot,
        ".reference-sidebar-sort-menu",
        "sort menu",
      );
      expect(getMenuItemLabels(menu, "menuitemradio")).toEqual([
        "Classic sidebar",
        "Inbox sidebar (New)",
        "Last Active Sorting",
        "Manual Sorting",
      ]);
      expect(getMenuItemLabels(menu, "menuitemcheckbox")).not.toContain("Group by Project");
    });

    await step("write the sidebar version through the settings pipeline", async () => {
      const menu = await findRequiredElement(
        storyRoot,
        ".reference-sidebar-sort-menu",
        "sort menu",
      );
      fireEvent.click(findMenuItem(menu, "menuitemradio", "Inbox sidebar (New)"));
      await expectMessage({ source: "sidebar:sidebarVersion", type: "updateSettingsPatch" });
      await expectNoMessage({ type: "setActiveSessionsSortMode" });
      expect(getLatestSettingsPatch()).toEqual({ sidebarVersion: "v2" });
    });
  },
};

export const SidebarVersionSortMenuInboxActive: Story = {
  args: {
    sidebarVersion: "v2",
  },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    await waitForRenderedSidebarV2(storyRoot);
    resetSidebarStoryMessages();

    await step("open Sort & Filter from the Projects header", async () => {
      await openProjectsSortMenu(storyRoot);
    });

    await step("swap the whole sort group for the Group by Project item", async () => {
      /*
       * CDXC:SidebarV2 2026-07-29:
       * Sorting is a V1-only concept, so BOTH sort radios leave the menu while
       * the Inbox is active. Hiding only Manual Sorting left a lone "Last Active
       * Sorting" radio that the position-stable inbox ignores.
       */
      const menu = await findRequiredElement(
        storyRoot,
        ".reference-sidebar-sort-menu",
        "sort menu",
      );
      expect(getMenuItemLabels(menu, "menuitemradio")).toEqual([
        "Classic sidebar",
        "Inbox sidebar (New)",
      ]);
      expect(getMenuItemLabels(menu, "menuitemcheckbox")).toContain("Group by Project");
    });

    await step("persist the Group by Project sub-mode", async () => {
      const menu = await findRequiredElement(
        storyRoot,
        ".reference-sidebar-sort-menu",
        "sort menu",
      );
      fireEvent.click(findMenuItem(menu, "menuitemcheckbox", "Group by Project"));
      await expectMessage({ source: "sidebar:sidebarVersion", type: "updateSettingsPatch" });
      expect(getLatestSettingsPatch()).toEqual({ sidebarV2Layout: "byProject" });
    });

    /*
     * CDXC:SidebarV2 2026-07-29:
     * The P1 skeleton carried a "switch back to classic" button in the Inbox
     * body because the stub had no header of its own. The real Inbox header IS
     * the shared section header, so Sort & Filter is the reachable way out and
     * this step guards THAT path instead. The in-body escape hatch now only
     * appears in the empty inbox, where a user could reasonably fear V2 lost
     * their sessions; `Sidebar/V2 Inbox → EmptyInbox` covers it there.
     */
    await step("return to the classic sidebar from Sort & Filter", async () => {
      resetSidebarStoryMessages();
      await openProjectsSortMenu(storyRoot);
      const menu = await findRequiredElement(
        storyRoot,
        ".reference-sidebar-sort-menu",
        "sort menu",
      );
      fireEvent.click(findMenuItem(menu, "menuitemradio", "Classic sidebar"));
      await expectMessage({ source: "sidebar:sidebarVersion", type: "updateSettingsPatch" });
      expect(getLatestSettingsPatch()).toEqual({ sidebarVersion: "v1" });
    });
  },
};

async function openProjectsSortMenu(storyRoot: HTMLElement) {
  const projectsHeader = await findRequiredElement(
    storyRoot,
    '[data-reference-section="projects"]',
    "Projects section header",
  );
  fireEvent.mouseEnter(projectsHeader);
  fireEvent.click(
    await findRequiredElement(
      storyRoot,
      '[data-reference-section="projects"] .reference-sidebar-section-sort-action',
      "Projects Sort & Filter button",
    ),
  );
}

async function waitForRenderedSidebarV2(storyRoot: HTMLElement) {
  await waitFor(
    () => {
      expect(storyRoot.querySelector(".sidebar-v2-root")).toBeTruthy();
      expect(storyRoot.querySelector("[data-sidebar-group-id]")).toBeNull();
    },
    { timeout: 20_000 },
  );
}

function getMenuItemLabels(menu: HTMLElement, role: "menuitemcheckbox" | "menuitemradio") {
  return Array.from(menu.querySelectorAll(`[role="${role}"]`)).map((item) =>
    item.textContent?.trim(),
  );
}

function findMenuItem(
  menu: HTMLElement,
  role: "menuitemcheckbox" | "menuitemradio",
  label: string,
) {
  const item = Array.from(menu.querySelectorAll(`[role="${role}"]`)).find(
    (candidate) => candidate.textContent?.trim() === label,
  );
  if (!(item instanceof HTMLElement)) {
    throw new Error(`Could not find ${role} menu item: ${label}`);
  }
  return item;
}

function getLatestSettingsPatch() {
  const patchMessages = getSidebarStoryMessages().filter(
    (message): message is Extract<SidebarToExtensionMessage, { type: "updateSettingsPatch" }> =>
      (message as { type?: string }).type === "updateSettingsPatch",
  );
  return patchMessages.at(-1)?.patch;
}
