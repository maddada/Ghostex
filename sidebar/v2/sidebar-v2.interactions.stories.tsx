import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fireEvent, waitFor, within } from "storybook/test";
import {
  dragAndDrop,
  dragToHover,
  expectMessage,
  findRequiredElement,
  releaseDrag,
} from "../sidebar-app.interactions.helpers";
import type { SidebarStoryArgs } from "../sidebar-story-fixtures";
import {
  getSidebarStoryMessages,
  resetSidebarStoryMessages,
} from "../sidebar-story-harness";
import {
  DEFAULT_SIDEBAR_STORY_ARGS,
  SIDEBAR_STORY_ARG_TYPES,
  SIDEBAR_STORY_DECORATORS,
  renderSidebarStory,
} from "../sidebar-story-meta";
import {
  expectSettingsPatch,
  findSidebarV2Row,
  waitForSidebarV2,
} from "./sidebar-v2.story-helpers";

/*
 * CDXC:SidebarV2 2026-07-29:
 * Interaction coverage for the Inbox sidebar. The assertions are deliberately
 * about MESSAGES, not about local UI state: V2's core promise is that it drives
 * the host through exactly the same commands the classic sidebar sends, so the
 * host can never tell which sidebar the user is running.
 */

const meta = {
  title: "Sidebar/V2 Interactions",
  args: {
    ...DEFAULT_SIDEBAR_STORY_ARGS,
    fixture: "sidebar-v2-inbox",
    sidebarLifecycleCapabilities: "settleAndSnooze",
    sidebarV2Layout: "flat",
    sidebarVersion: "v2",
  },
  argTypes: SIDEBAR_STORY_ARG_TYPES,
  decorators: SIDEBAR_STORY_DECORATORS,
  render: renderSidebarStory,
} satisfies Meta<SidebarStoryArgs>;

export default meta;

type Story = StoryObj<typeof meta>;

export const ActivatesSessionOnClick: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    await waitForSidebarV2(storyRoot);
    resetSidebarStoryMessages();

    await step("focus an agent session from its inbox card", async () => {
      const row = await findSidebarV2Row(storyRoot, "v2-quick-idle");
      fireEvent.click(row, { detail: 1 });
      await expectMessage({ sessionId: "v2-quick-idle", type: "focusSession" });
    });

    await step("activate a browser session with the same command", async () => {
      resetSidebarStoryMessages();
      const row = await findSidebarV2Row(storyRoot, "v2-ghostex-browser");
      fireEvent.click(row, { detail: 1 });
      await expectMessage({ sessionId: "v2-ghostex-browser", type: "focusSession" });
    });
  },
};

export const FiltersByProjectScope: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);
    const body = within(storyRoot);

    await step("open the scope menu with every project plus Quick", async () => {
      fireEvent.click(
        await findRequiredElement(root, ".sidebar-v2-scope-trigger", "scope trigger"),
      );
      await body.findByRole("menuitemradio", { name: /All projects/ });
      await body.findByRole("menuitemradio", { name: /Quick/ });
      await body.findByRole("menuitemradio", { name: /zmx/ });
    });

    /*
     * CDXC:SidebarV2ProjectIcons 2026-07-29:
     * The menu names projects, so it must show the icons the user gave them —
     * an image where there is one, the Tabler glyph where there is one, and the
     * folder ONLY for entries with no project behind them ("All projects").
     */
    await step("carry each project's own icon into the scope menu", async () => {
      const zmxItem = await body.findByRole("menuitemradio", { name: /zmx/ });
      expect(zmxItem.querySelector("img.sidebar-v2-project-icon")).toBeTruthy();
      const ghostexItem = await body.findByRole("menuitemradio", { name: /^ghostex/ });
      expect(
        ghostexItem.querySelector('.sidebar-v2-project-icon[data-icon-variant="tabler"]'),
      ).toBeTruthy();
      const allItem = await body.findByRole("menuitemradio", { name: /All projects/ });
      expect(allItem.querySelector("img.sidebar-v2-project-icon")).toBeNull();
    });

    await step("scope the inbox to a single project", async () => {
      fireEvent.click(await body.findByRole("menuitemradio", { name: /zmx/ }));
      await waitFor(() => {
        expect(root.querySelector('[data-session-id="v2-quick-idle"]')).toBeNull();
      });
      await findSidebarV2Row(storyRoot, "v2-zmx-done");
    });

    await step("show the scoped empty state when a project has no matches", async () => {
      fireEvent.click(
        await findRequiredElement(root, ".sidebar-v2-scope-trigger", "scope trigger"),
      );
      fireEvent.click(await body.findByRole("menuitemradio", { name: /All projects/ }));
      await findSidebarV2Row(storyRoot, "v2-quick-idle");
    });
  },
};

export const TogglesGroupByProject: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    await waitForSidebarV2(storyRoot);
    const body = within(storyRoot);
    resetSidebarStoryMessages();

    await step("reach Sort & Filter from the V2 header", async () => {
      const sortAction = await findRequiredElement(
        storyRoot,
        '[data-reference-section="projects"] .reference-sidebar-section-sort-action',
        "Sort & Filter trigger",
      );
      fireEvent.click(sortAction);
      await body.findByRole("menuitemradio", { name: "Inbox sidebar (New)" });
    });

    await step("persist Group by Project through the settings pipeline", async () => {
      fireEvent.click(await body.findByRole("menuitemcheckbox", { name: "Group by Project" }));
      await expectSettingsPatch("sidebarV2Layout", "byProject");
    });
  },
};

/** Sorting is a V1-only concept, so the whole sort radio group leaves the menu
    while the Inbox is active — no lone, no-op radio, and no sort mode in the
    trigger's accessible name. */
export const HidesSortModesWhileTheInboxIsActive: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    await waitForSidebarV2(storyRoot);
    const body = within(storyRoot);

    await step("name the trigger after the sidebar, not after a sort order", async () => {
      const sortAction = await findRequiredElement(
        storyRoot,
        '[data-reference-section="projects"] .reference-sidebar-section-sort-action',
        "Sort & Filter trigger",
      );
      expect(sortAction.getAttribute("aria-label")).toBe("Filter sessions: Inbox sidebar");
      fireEvent.click(sortAction);
      await body.findByRole("menuitemradio", { name: "Inbox sidebar (New)" });
    });

    await step("drop both sort radios, not just Manual Sorting", async () => {
      expect(body.queryByRole("menuitemradio", { name: "Manual Sorting" })).toBeNull();
      expect(body.queryByRole("menuitemradio", { name: "Last Active Sorting" })).toBeNull();
    });

    await step("keep the tag filters the inbox does honor", async () => {
      await body.findByRole("menuitemcheckbox", { name: "Favorite" });
    });
  },
};

/** Search filters the inbox exactly as it filters V1 — including the closed
    sessions the user can reopen. */
export const SearchIncludesPreviousSessions: Story = {
  play: async ({ canvasElement, step, userEvent }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);
    const body = within(storyRoot);

    await step("filter the inbox from the shared search row", async () => {
      await userEvent.click(await body.findByRole("button", { name: "Search" }));
      const input = await body.findByRole("textbox", {
        name: "Search current sessions and sessions to reopen",
      });
      await userEvent.click(input);
      await userEvent.keyboard("release");
      await waitFor(() => {
        expect(root.querySelector('[data-session-id="v2-zmx-done"]')).toBeNull();
      });
      await findSidebarV2Row(storyRoot, "v2-quick-idle");
    });

    await step("offer the matching closed sessions below the inbox", async () => {
      const previousGroup = await findRequiredElement(
        storyRoot,
        ".session-search-previous-group",
        "previous sessions result group",
      );
      expect(previousGroup.textContent).toContain("release retro notes");
      expect(previousGroup.getBoundingClientRect().top).toBeGreaterThanOrEqual(
        root.getBoundingClientRect().top,
      );
    });
  },
};

export const RenamesFromTheRow: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    await waitForSidebarV2(storyRoot);
    resetSidebarStoryMessages();

    await step("open the inline editor on double click", async () => {
      const row = await findSidebarV2Row(storyRoot, "v2-quick-idle");
      fireEvent.doubleClick(row);
      await findRequiredElement(row, ".sidebar-v2-row-rename-input", "rename input");
    });

    await step("commit the rename with the same message the modal posts", async () => {
      const row = await findSidebarV2Row(storyRoot, "v2-quick-idle");
      const input = await findRequiredElement(
        row,
        ".sidebar-v2-row-rename-input",
        "rename input",
      );
      fireEvent.change(input, { target: { value: "Renamed from the inbox" } });
      fireEvent.keyDown(input, { key: "Enter" });
      await expectMessage({
        sessionId: "v2-quick-idle",
        title: "Renamed from the inbox",
        type: "renameSession",
      });
    });
  },
};

/*
 * CDXC:SidebarV2Lifecycle 2026-07-29:
 * The lifecycle affordances are asserted through the MESSAGES they post, never
 * through local state, because the UI is deliberately not optimistic: gxserver
 * owns the transition and answers with a presentation delta. A test that
 * asserted "the row moved" would be testing a behavior the client must not have.
 */
export const SettlesAndSnoozesFromHoverActions: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);
    const body = within(storyRoot);
    resetSidebarStoryMessages();

    await step("settle an inbox card from its hover check", async () => {
      const row = await findSidebarV2Row(storyRoot, "v2-quick-idle");
      fireEvent.click(
        await findRequiredElement(row, '[aria-label="Settle session"]', "settle action"),
      );
      await expectMessage({ sessionId: "v2-quick-idle", type: "settleSession" });
    });

    await step("refuse a second click while the write is unanswered", async () => {
      const row = await findSidebarV2Row(storyRoot, "v2-quick-idle");
      const settle = await findRequiredElement(
        row,
        '[aria-label="Settle session"]',
        "settle action",
      );
      expect(settle).toBeDisabled();
    });

    await step("open the snooze presets from the clock button", async () => {
      resetSidebarStoryMessages();
      const row = await findSidebarV2Row(storyRoot, "v2-ghostex-pinned");
      fireEvent.click(
        await findRequiredElement(row, '[aria-label="Snooze session"]', "snooze action"),
      );
      await body.findByRole("menuitem", { name: /In 1 hour/ });
      await body.findByRole("menuitem", { name: /Tomorrow/ });
    });

    await step("snooze with a wake time strictly in the future", async () => {
      const preset = await body.findByRole("menuitem", { name: /Tomorrow/ });
      fireEvent.click(preset);
      await waitFor(() => {
        const message = getSidebarStoryMessages().find(
          (entry) => entry.type === "snoozeSession",
        );
        expect(message).toBeTruthy();
        expect(message).toMatchObject({ sessionId: "v2-ghostex-pinned" });
        const snoozedUntil = (message as { snoozedUntil?: string } | undefined)?.snoozedUntil;
        expect(typeof snoozedUntil).toBe("string");
        expect(Date.parse(snoozedUntil!)).toBeGreaterThan(Date.now());
      });
    });

    await step("un-settle from the settled shelf", async () => {
      resetSidebarStoryMessages();
      const row = await findSidebarV2Row(storyRoot, "v2-ghostex-settled");
      fireEvent.click(
        await findRequiredElement(row, '[aria-label="Un-settle session"]', "un-settle action"),
      );
      await expectMessage({ sessionId: "v2-ghostex-settled", type: "unsettleSession" });
    });

    await step("wake early from the snoozed shelf", async () => {
      resetSidebarStoryMessages();
      fireEvent.click(
        await findRequiredElement(
          root,
          '.sidebar-v2-shelf-header[data-tone="snoozed"]',
          "snoozed shelf header",
        ),
      );
      const row = await findSidebarV2Row(storyRoot, "v2-ghostex-snoozed");
      fireEvent.click(
        await findRequiredElement(row, '[aria-label="Wake session now"]', "wake action"),
      );
      await expectMessage({ sessionId: "v2-ghostex-snoozed", type: "unsnoozeSession" });
    });
  },
};

export const RunsLifecycleContextMenuActions: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    await waitForSidebarV2(storyRoot);
    const body = within(storyRoot);
    resetSidebarStoryMessages();

    await step("offer Settle and Snooze on an inbox row", async () => {
      const row = await findSidebarV2Row(storyRoot, "v2-quick-idle");
      const bounds = row.getBoundingClientRect();
      fireEvent.contextMenu(row, {
        bubbles: true,
        clientX: bounds.left + 20,
        clientY: bounds.top + 10,
      });
      await body.findByRole("menuitem", { name: "Settle" });
      await body.findByRole("menuitem", { name: "Snooze" });
    });

    await step("expand the Snooze submenu instead of guessing a preset", async () => {
      fireEvent.click(await body.findByRole("menuitem", { name: "Snooze" }));
      const preset = await body.findByRole("menuitem", { name: /Next week/ });
      fireEvent.click(preset);
      await waitFor(() => {
        expect(
          getSidebarStoryMessages().some((entry) => entry.type === "snoozeSession"),
        ).toBe(true);
      });
    });

    await step("offer Un-settle on a settled row", async () => {
      resetSidebarStoryMessages();
      const row = await findSidebarV2Row(storyRoot, "v2-ghostex-settled-manual");
      const bounds = row.getBoundingClientRect();
      fireEvent.contextMenu(row, {
        bubbles: true,
        clientX: bounds.left + 20,
        clientY: bounds.top + 10,
      });
      fireEvent.click(await body.findByRole("menuitem", { name: "Un-settle" }));
      await expectMessage({
        sessionId: "v2-ghostex-settled-manual",
        type: "unsettleSession",
      });
    });
  },
};

/** An older gxserver publishes no capability block: the affordances are absent,
    not disabled, so a click can never reach an endpoint that does not exist. */
export const HidesLifecycleActionsWithoutCapabilities: Story = {
  args: { sidebarLifecycleCapabilities: "absent" },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);
    const body = within(storyRoot);

    await step("render no settle or snooze hover action", async () => {
      await findSidebarV2Row(storyRoot, "v2-quick-idle");
      expect(root.querySelectorAll('[aria-label="Settle session"]')).toHaveLength(0);
      expect(root.querySelectorAll('[aria-label="Snooze session"]')).toHaveLength(0);
      expect(root.querySelectorAll('[aria-label="Un-settle session"]')).toHaveLength(0);
      expect(root.querySelectorAll('[aria-label="Wake session now"]')).toHaveLength(0);
    });

    await step("leave the lifecycle items out of the context menu", async () => {
      const row = await findSidebarV2Row(storyRoot, "v2-quick-idle");
      const bounds = row.getBoundingClientRect();
      fireEvent.contextMenu(row, {
        bubbles: true,
        clientX: bounds.left + 20,
        clientY: bounds.top + 10,
      });
      await body.findByRole("menuitem", { name: "Rename" });
      expect(body.queryByRole("menuitem", { name: "Settle" })).toBeNull();
      expect(body.queryByRole("menuitem", { name: "Snooze" })).toBeNull();
    });
  },
};

export const RunsSessionContextMenuActions: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    await waitForSidebarV2(storyRoot);
    const body = within(storyRoot);
    resetSidebarStoryMessages();

    await step("open the session context menu from a right click", async () => {
      const row = await findSidebarV2Row(storyRoot, "v2-quick-idle");
      const bounds = row.getBoundingClientRect();
      fireEvent.contextMenu(row, {
        bubbles: true,
        clientX: bounds.left + 20,
        clientY: bounds.top + 10,
      });
      await body.findByRole("menuitem", { name: "Rename" });
    });

    await step("sleep the session through the shared host command", async () => {
      fireEvent.click(await body.findByRole("menuitem", { name: "Sleep" }));
      await expectMessage({
        sessionId: "v2-quick-idle",
        sleeping: true,
        type: "setSessionSleeping",
      });
    });

    /*
     * 2026-07-30 (UX batch):
     * Pinning moved OUT of the hover bar and into the menu: the bar carries the
     * triage verbs plus unpin, and unpin only on rows that are pinned. So the
     * pin path under test is the menu item, and the row's bar must not offer a
     * pin control at all.
     */
    await step("pin the session from its context menu", async () => {
      resetSidebarStoryMessages();
      const row = await findSidebarV2Row(storyRoot, "v2-quick-idle");
      expect(row.querySelector('[aria-label="Pin session"]')).toBeNull();
      const bounds = row.getBoundingClientRect();
      fireEvent.contextMenu(row, {
        bubbles: true,
        clientX: bounds.left + 20,
        clientY: bounds.top + 10,
      });
      fireEvent.click(await body.findByRole("menuitem", { name: "Pin" }));
      await expectMessage({
        pinned: true,
        sessionId: "v2-quick-idle",
        type: "setSessionPinned",
      });
    });

    await step("unpin a pinned session from its hover chip", async () => {
      resetSidebarStoryMessages();
      const row = await findSidebarV2Row(storyRoot, "v2-ghostex-pinned");
      const unpin = await findRequiredElement(
        row,
        '[aria-label="Unpin session"]',
        "unpin action",
      );
      /* Leftmost in the bar: the first control the pointer meets. */
      expect(unpin.parentElement?.firstElementChild).toBe(unpin);
      fireEvent.click(unpin);
      await expectMessage({
        pinned: false,
        sessionId: "v2-ghostex-pinned",
        type: "setSessionPinned",
      });
    });
  },
};

/*
 * CDXC:SidebarV2Git 2026-07-29:
 * The git line is pure presentation — it posts no messages — so this asserts
 * the two things that CAN regress silently: the badge's state (the class hook
 * every color rule keys off) and the absence of a third line for rows with no
 * git data. The latter is the promise that P3 did not make every card taller.
 */
export const RendersPullRequestStateOnCards: Story = {
  args: { sidebarLifecycleCapabilities: "settleSnoozeAndGit" },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);

    await step("mark each badge with the review state that colors it", async () => {
      const openRow = await findSidebarV2Row(storyRoot, "v2-ghostex-working");
      const openBadge = await findRequiredElement(openRow, ".sidebar-v2-row-pr", "open PR badge");
      expect(openBadge.getAttribute("data-pr-state")).toBe("open");
      expect(openBadge.textContent).toBe("#128");
      expect(openBadge.getAttribute("title")).toBe("#128 · Open");

      const draftRow = await findSidebarV2Row(storyRoot, "v2-zmx-failed");
      const draftBadge = await findRequiredElement(
        draftRow,
        ".sidebar-v2-row-pr",
        "draft PR badge",
      );
      expect(draftBadge.getAttribute("data-pr-state")).toBe("draft");

      const closedRow = await findSidebarV2Row(storyRoot, "v2-zmx-done");
      const closedBadge = await findRequiredElement(
        closedRow,
        ".sidebar-v2-row-pr",
        "closed PR badge",
      );
      expect(closedBadge.getAttribute("data-pr-state")).toBe("closed");

      const mergedRow = await findSidebarV2Row(storyRoot, "v2-ghostex-settled-manual");
      const mergedBadge = await findRequiredElement(
        mergedRow,
        ".sidebar-v2-row-pr",
        "merged PR badge",
      );
      expect(mergedBadge.getAttribute("data-pr-state")).toBe("merged");
    });

    await step("hover text names the branch and the review state", async () => {
      const row = await findSidebarV2Row(storyRoot, "v2-ghostex-working");
      const git = await findRequiredElement(row, "[data-sidebar-v2-git]", "git meta line");
      expect(git.getAttribute("title")).toBe(
        "ghostex/sidebar-v2-inbox · #128 · Open · +412 −87",
      );
    });

    await step("give rows without git data no third line at all", async () => {
      for (const sessionId of ["v2-quick-idle", "v2-quick-approval", "v2-ghostex-browser"]) {
        const row = await findSidebarV2Row(storyRoot, sessionId);
        expect(row.querySelector("[data-sidebar-v2-git]")).toBeNull();
        expect(row.querySelector(".sidebar-v2-row-pr")).toBeNull();
      }
      /*
       * 2026-07-30: no git means no meta line at all. The old `detail` tenant
       * was gxserver's cwd/project path, so the line it kept alive was a folder
       * path — the exact thing the card must never show instead of a branch.
       */
      const idleRow = await findSidebarV2Row(storyRoot, "v2-quick-idle");
      expect(idleRow.querySelector('[data-line="meta"]')).toBeNull();
      expect(
        idleRow.closest(".sidebar-v2-row-item")?.getAttribute("data-card-lines"),
      ).toBe("2");
    });

    await step("never let the git line steal a row click", async () => {
      resetSidebarStoryMessages();
      const row = await findSidebarV2Row(storyRoot, "v2-ghostex-working");
      const git = await findRequiredElement(row, "[data-sidebar-v2-git]", "git meta line");
      fireEvent.click(git, { bubbles: true, detail: 1 });
      await expectMessage({ sessionId: "v2-ghostex-working", type: "focusSession" });
      expect(root.querySelector(".sidebar-v2-row-rename-input")).toBeNull();
    });
  },
};

/*
 * CDXC:SidebarV2SingleCreateControl 2026-07-30:
 * The whole point of this story is what is NOT in the header. V2 used to borrow
 * the classic header's create trio (Quick Browser Tab, Quick Terminal, agent
 * split button) on top of its own split "+", which put four creation controls in
 * one strip and made THREE of them create in Quick regardless of which project
 * the user was looking at. Now there is exactly one control, and its plain half
 * targets a real project.
 */
export const HeaderHasOneCreateControl: Story = {
  args: { sidebarLifecycleCapabilities: "settleSnoozeGitAndWorktree" },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);
    const body = within(storyRoot);
    /* The fixture's ACTIVE project, which is what the "+" must resolve to. */
    const projectGroupId = "v2-project-ghostex";

    const header = await findRequiredElement(
      storyRoot,
      '[data-reference-section="projects"]',
      "sessions section header",
    );

    await step("the shared header keeps Sort & Filter and nothing that creates", async () => {
      await findRequiredElement(
        header,
        ".reference-sidebar-section-sort-action",
        "Sort & Filter trigger",
      );
      expect(header.querySelector('[aria-label="Quick Browser Tab"]')).toBeNull();
      expect(header.querySelector('[aria-label="Quick Terminal"]')).toBeNull();
      expect(header.querySelector(".reference-sidebar-section-agent-picker")).toBeNull();
    });

    await step("V2's toolbar owns the only create control", async () => {
      await findRequiredElement(
        root,
        ".sidebar-v2-toolbar .sidebar-v2-create-button",
        "plain create button",
      );
      await findRequiredElement(
        root,
        ".sidebar-v2-toolbar .sidebar-v2-create-chevron",
        "create chevron",
      );
    });

    await step("the plain + creates in the resolved project, never in Quick", async () => {
      resetSidebarStoryMessages();
      fireEvent.click(
        await findRequiredElement(
          root,
          ".sidebar-v2-toolbar .sidebar-v2-create-button",
          "plain create button",
        ),
      );
      await expectMessage({ groupId: projectGroupId, type: "runSidebarAgent" });
      expect(
        getSidebarStoryMessages().some(
          (message) =>
            message.type === "runSidebarAgent" &&
            (message as { groupId?: string }).groupId !== projectGroupId,
        ),
      ).toBe(false);
    });

    await step("the chevron menu carries every create path in one list", async () => {
      fireEvent.click(
        await findRequiredElement(
          root,
          ".sidebar-v2-toolbar .sidebar-v2-create-chevron",
          "create chevron",
        ),
      );
      /* The agent picker: the very list the classic split chevron offered. */
      await body.findByRole("menuitem", { name: "Claude" });
      await body.findByRole("menuitem", { name: "Codex" });
      await body.findByRole("menuitem", { name: /New worktree session/ });
      await body.findByRole("menuitem", { name: "Quick Terminal" });
      await body.findByRole("menuitem", { name: "Quick Browser Tab" });
      await body.findByRole("menuitemcheckbox", {
        name: /Default new sessions to worktree/,
      });
    });

    await step("picking an agent launches THAT agent in the same project", async () => {
      resetSidebarStoryMessages();
      const claudeItem = await findRequiredElement(
        storyRoot,
        '.sidebar-v2-create-menu [data-agent-id="claude"]',
        "Claude picker item",
      );
      fireEvent.click(claudeItem);
      await expectMessage({
        agentId: "claude",
        groupId: projectGroupId,
        type: "runSidebarAgent",
      });
    });

    await step("the Quick entries are the only path into Quick", async () => {
      resetSidebarStoryMessages();
      fireEvent.click(
        await findRequiredElement(
          root,
          ".sidebar-v2-toolbar .sidebar-v2-create-chevron",
          "create chevron",
        ),
      );
      fireEvent.click(await body.findByRole("menuitem", { name: "Quick Terminal" }));
      await expectMessage({ type: "createChat" });

      resetSidebarStoryMessages();
      fireEvent.click(
        await findRequiredElement(
          root,
          ".sidebar-v2-toolbar .sidebar-v2-create-chevron",
          "create chevron",
        ),
      );
      fireEvent.click(await body.findByRole("menuitem", { name: "Quick Browser Tab" }));
      await expectMessage({ type: "openBrowserChat" });
    });
  },
};

/*
 * CDXC:SidebarV2SingleCreateControl 2026-07-30:
 * The other half of the same change: the classic sidebar keeps its create trio.
 * V2 drops those props at ITS mount only, and this story is what would fail if
 * that removal ever leaked into the shared component or the V1 mounts.
 */
export const ClassicSidebarKeepsItsCreateTrio: Story = {
  args: { sidebarVersion: "v1" },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;

    await step("the Quick header still creates browsers, terminals, and agents", async () => {
      const header = await findRequiredElement(
        storyRoot,
        '[data-reference-section="quick"]',
        "Quick section header",
      );
      await findRequiredElement(header, '[aria-label="Quick Browser Tab"]', "browser action");
      await findRequiredElement(header, '[aria-label="Quick Terminal"]', "terminal action");
      await findRequiredElement(
        header,
        ".reference-sidebar-section-agent-picker",
        "agent split button",
      );
      await findRequiredElement(
        header,
        ".reference-sidebar-section-sort-action",
        "Sort & Filter trigger",
      );
    });

    await step("V2's own toolbar is not mounted at all", async () => {
      expect(storyRoot.querySelector(".sidebar-v2-toolbar")).toBeNull();
    });
  },
};

/*
 * CDXC:SidebarV2BrowserShelfFirst 2026-07-30:
 * Browser tabs are a tab strip, and a tab strip belongs at the top. This asserts
 * DOM ORDER rather than a screenshot, because the shelf's own markup is
 * position-agnostic and the only thing that can regress is where the list puts
 * it.
 */
export const BrowserShelfLeadsTheFlatList: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);

    await step("the Browser shelf header is the flat list's first row", async () => {
      const list = await findRequiredElement(root, ".sidebar-v2-list", "flat list");
      /*
       * Shelves are flat: the header is an `<li>` and its rows are its SIBLINGS,
       * so "first" is literally the first child of the list.
       */
      await findRequiredElement(
        list,
        ':scope > li:first-child .sidebar-v2-shelf-header[data-tone="browser"]',
        "browser shelf header",
      );
      const items = [...list.children];
      const browserRowIndex = items.findIndex((item) =>
        item.querySelector('[data-session-id="v2-ghostex-browser"]'),
      );
      const inboxCardIndex = items.findIndex((item) =>
        item.querySelector('[data-session-id="v2-ghostex-pinned"]'),
      );
      expect(browserRowIndex).toBeGreaterThan(0);
      expect(browserRowIndex).toBeLessThan(inboxCardIndex);
    });

    await step("the shelves keep browser, snoozed, settled order", async () => {
      const tones = [
        ...root.querySelectorAll<HTMLElement>(".sidebar-v2-list .sidebar-v2-shelf-header"),
      ].map((shelf) => shelf.getAttribute("data-tone"));
      expect(tones).toEqual(["browser", "snoozed", "settled"]);
    });
  },
};

/*
 * CDXC:SidebarV2ContextMenuParity 2026-07-30:
 * Coverage for the V1 session-menu items the V2 row menu adopted. The right-click
 * is the ONLY menu trigger now (the ⋯ button is gone), so every step below opens
 * the menu the way a user does.
 */
async function openSidebarV2RowMenu(storyRoot: ParentNode, sessionId: string): Promise<void> {
  const row = await findSidebarV2Row(storyRoot, sessionId);
  const bounds = row.getBoundingClientRect();
  fireEvent.contextMenu(row, {
    bubbles: true,
    clientX: bounds.left + 20,
    clientY: bounds.top + 10,
  });
  await waitFor(() => {
    return expect(
      storyRoot.querySelector('.sidebar-v2-session-context-menu [role="menuitem"]'),
    ).toBeTruthy();
  });
}

/** Every top-level item currently in the open menu, in DOM order. Order is the
    assertion that matters most: it is what proves the parity items landed in
    their own section between the primary and lifecycle groups. */
function readSidebarV2MenuLabels(storyRoot: ParentNode): string[] {
  return [
    ...storyRoot.querySelectorAll<HTMLElement>(
      '.sidebar-v2-session-context-menu [role="menuitem"]',
    ),
  ].map((item) => item.textContent?.trim() ?? "");
}

/**
 * Delayed Send and View 1st message are the two items whose effect is a native
 * full-window modal, delivered through `window.webkit`. That bridge THROWS when
 * the host is absent (by design — a missing modal host must be loud), so a story
 * that exercises those items has to stand in for the host and capture what it
 * was asked to open.
 */
function installAppModalHostCapture(): { opened: unknown[]; restore: () => void } {
  const opened: unknown[] = [];
  const previous = window.webkit;
  window.webkit = {
    ...previous,
    messageHandlers: {
      ...previous?.messageHandlers,
      ghostexAppModalHost: {
        postMessage: (message: unknown) => {
          opened.push(message);
        },
      },
    },
  };
  return {
    opened,
    restore: () => {
      window.webkit = previous;
    },
  };
}

export const ShowsV1ParityContextMenuItems: Story = {
  args: { showSessionCommandCopyActions: true, showSessionDetailsCopyAction: true },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    await waitForSidebarV2(storyRoot);
    const body = within(storyRoot);

    await step("a fully equipped local agent row offers the whole menu, in order", async () => {
      await openSidebarV2RowMenu(storyRoot, "v2-quick-idle");
      /*
       * The exact sequence, not a set: primary verbs, then the borrowed
       * per-session section, then the lifecycle verdicts (with Close After Done
       * beside Snooze), then the state toggles (with Tag as beside Pin), then
       * the destructive one.
       */
      expect(readSidebarV2MenuLabels(storyRoot)).toEqual([
        "Rename",
        "Focus",
        "View 1st message",
        "Copy resume",
        "Copy attach command",
        "Copy details",
        "Delayed Send",
        "Fork",
        "Generate Title",
        "Full reload",
        "Settle",
        "Snooze",
        "Close After Done",
        "Pin",
        "Tag as",
        "Sleep",
        "Close",
      ]);
    });

    await step("the items V2 deliberately did not adopt stay absent", async () => {
      /* The other menu items name V1 structures V2 does not
         render, or a host command gpui leaves unhandled. */
      for (const label of [
        "Remote Access",
        "Move to New Group",
        "Sleep below",
        "Close below",
        "Pop Out Pane",
      ]) {
        expect(body.queryByRole("menuitem", { name: label })).toBeNull();
      }
    });

    await step("a browser tab keeps only the three verbs a tab can answer", async () => {
      await openSidebarV2RowMenu(storyRoot, "v2-ghostex-browser");
      /*
       * A browser row has no agent, so V1's eligibility answers no to every
       * terminal/agent action — and its project group cannot zoom, so Focus is
       * out too. Copy details is the ONE parity item that survives, because V1
       * gates it on "is a concrete row" rather than on an agent: a tab's project,
       * machine and URL are still worth copying.
       */
      expect(readSidebarV2MenuLabels(storyRoot)).toEqual([
        "Copy details",
        "Pin",
        "Sleep",
        "Close",
      ]);
    });

    await step("Focus is absent in a group with no split panes to zoom", async () => {
      await openSidebarV2RowMenu(storyRoot, "v2-ghostex-pinned");
      const labels = readSidebarV2MenuLabels(storyRoot);
      expect(labels).toContain("Rename");
      expect(labels).not.toContain("Focus");
      /* The pinned row proves the state toggle reads its own state. */
      expect(labels).toContain("Unpin");
    });
  },
};

/** The copy items are settings-gated in V1 and stay settings-gated in V2: an
    untouched install (both flags default OFF) sees none of them, while the
    agent-capability items are unaffected. */
export const HidesCopyActionsWithoutTheirSettings: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    await waitForSidebarV2(storyRoot);
    const body = within(storyRoot);

    await step("no copy item appears without its setting", async () => {
      await openSidebarV2RowMenu(storyRoot, "v2-quick-idle");
      const labels = readSidebarV2MenuLabels(storyRoot);
      expect(labels).not.toContain("Copy resume");
      expect(labels).not.toContain("Copy attach command");
      expect(labels).not.toContain("Copy details");
      expect(labels).toEqual([
        "Rename",
        "Focus",
        "View 1st message",
        "Delayed Send",
        "Fork",
        "Generate Title",
        "Full reload",
        "Settle",
        "Snooze",
        "Close After Done",
        "Pin",
        "Tag as",
        "Sleep",
        "Close",
      ]);
      expect(body.queryByRole("menuitem", { name: "Fork" })).toBeTruthy();
    });
  },
};

export const RunsV1ParityContextMenuCommands: Story = {
  args: { showSessionCommandCopyActions: true, showSessionDetailsCopyAction: true },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    await waitForSidebarV2(storyRoot);
    const body = within(storyRoot);

    const clickMenuItem = async (label: string) => {
      resetSidebarStoryMessages();
      await openSidebarV2RowMenu(storyRoot, "v2-quick-idle");
      fireEvent.click(await body.findByRole("menuitem", { name: label }));
    };

    await step("Fork posts the shared fork command", async () => {
      await clickMenuItem("Fork");
      await expectMessage({ sessionId: "v2-quick-idle", type: "forkSession" });
    });

    await step("Full reload posts the shared reload command", async () => {
      await clickMenuItem("Full reload");
      await expectMessage({ sessionId: "v2-quick-idle", type: "fullReloadSession" });
    });

    await step("Close After Done toggles through the host, with no local guess", async () => {
      await clickMenuItem("Close After Done");
      await expectMessage({ sessionId: "v2-quick-idle", type: "toggleCloseAfterDone" });
    });

    await step("Generate Title retitles through the ordinary rename path", async () => {
      await clickMenuItem("Generate Title");
      /* The captured 1st user message IS the rename input; the host summarizes it. */
      await expectMessage({
        sessionId: "v2-quick-idle",
        shouldGenerateTitle: true,
        title: "Write the 6.9.0 release notes from the merged PRs",
        type: "renameSession",
      });
    });

    await step("both command copies post their own host command", async () => {
      await clickMenuItem("Copy resume");
      await expectMessage({ sessionId: "v2-quick-idle", type: "copyResumeCommand" });
      await clickMenuItem("Copy attach command");
      await expectMessage({ sessionId: "v2-quick-idle", type: "copyAttachCommand" });
    });

    await step("Copy details sends the text the sidebar built, not a request", async () => {
      await clickMenuItem("Copy details");
      await waitFor(() => {
        const matched = getSidebarStoryMessages().some(
          (message) =>
            message.type === "copySessionDetails" &&
            message.sessionId === "v2-quick-idle" &&
            message.detailsText.includes("Draft the release notes"),
        );
        return expect(matched).toBe(true);
      });
    });

    await step("Tag as lists the enabled tags and marks the current one", async () => {
      resetSidebarStoryMessages();
      await openSidebarV2RowMenu(storyRoot, "v2-quick-idle");
      fireEvent.click(await body.findByRole("menuitem", { name: "Tag as" }));
      const favorite = await body.findByRole("menuitemradio", { name: "Favorite" });
      /* The row already carries Favorite, so it is the checked option. */
      expect(favorite).toHaveAttribute("data-checked", "true");
      const done = await body.findByRole("menuitemradio", { name: "Done" });
      expect(done).toHaveAttribute("data-checked", "false");
      /* Default-disabled tags never reach the assignment menu. */
      expect(body.queryByRole("menuitemradio", { name: "Bug" })).toBeNull();
      fireEvent.click(done);
      await expectMessage({
        sessionId: "v2-quick-idle",
        sessionTag: "done",
        type: "setSessionTag",
      });
    });

    await step("re-picking the current marker clears it", async () => {
      resetSidebarStoryMessages();
      await openSidebarV2RowMenu(storyRoot, "v2-quick-idle");
      fireEvent.click(await body.findByRole("menuitem", { name: "Tag as" }));
      fireEvent.click(await body.findByRole("menuitemradio", { name: "Favorite" }));
      await expectMessage({
        sessionId: "v2-quick-idle",
        sessionTag: null,
        type: "setSessionTag",
      });
    });

    await step("the two modal items open the native full-window host", async () => {
      const modalHost = installAppModalHostCapture();
      try {
        await clickMenuItem("View 1st message");
        await waitFor(() => {
          return expect(
            modalHost.opened.some(
              (message) =>
                (message as { modal?: string }).modal === "firstUserMessage" &&
                (message as { message?: string }).message ===
                  "Write the 6.9.0 release notes from the merged PRs",
            ),
          ).toBe(true);
        });

        await clickMenuItem("Delayed Send");
        await waitFor(() => {
          return expect(
            modalHost.opened.some(
              (message) =>
                (message as { modal?: string }).modal === "delayedSend" &&
                (message as { sessionId?: string }).sessionId === "v2-quick-idle",
            ),
          ).toBe(true);
        });
      } finally {
        modalHost.restore();
      }
    });
  },
};

/*
 * CDXC:SidebarV2ContextMenuParity 2026-07-30:
 * Delayed Send, Close After Done and Full reload are local AppKit/host-timer
 * actions. V1's resolver makes a REMOTE row opt into each one through published
 * capabilities, and V2 inherits that rule by importing the resolver rather than
 * re-deriving it — this story is what proves the inheritance is real.
 */
export const HidesLocalOnlyActionsOnRemoteRows: Story = {
  args: {
    fixture: "sidebar-v2-multi-machine",
    showSessionCommandCopyActions: true,
    showSessionDetailsCopyAction: true,
  },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    await waitForSidebarV2(storyRoot);

    await step("a local agent row keeps the host-timer actions", async () => {
      await openSidebarV2RowMenu(storyRoot, "v2-mm-local-working");
      const labels = readSidebarV2MenuLabels(storyRoot);
      expect(labels).toContain("Delayed Send");
      expect(labels).toContain("Close After Done");
    });

    await step("the host-timer actions are absent on a row from another machine", async () => {
      await openSidebarV2RowMenu(storyRoot, "v2-mm-remote-active");
      const labels = readSidebarV2MenuLabels(storyRoot);
      /* Both need a published capability the fixture's remote rows do not claim. */
      expect(labels).not.toContain("Delayed Send");
      expect(labels).not.toContain("Close After Done");
      /*
       * What a remote row CAN still do goes through gxserver, so it stays —
       * including Full reload, which V1 allows for a remote terminal-kind row.
       * Asserting its PRESENCE is what proves the remote branch of the resolver
       * ran at all rather than the whole section having been dropped.
       */
      expect(labels).toContain("Full reload");
      expect(labels).toContain("Copy resume");
      expect(labels).toContain("Rename");
      expect(labels).toContain("Tag as");
    });
  },
};

/*
 * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
 * ── grouped mode adopts V1's project UX ───────────────────────────────────────
 * Grouped V2 renders V1's project header with V2 cards underneath. The whole look
 * is inherited from `groups.css`'s reference-layout block, which keys off V1's
 * classnames — so the DOM CONTRACT is the thing worth testing. A CSS assertion
 * would only re-measure V1's own stylesheet; an assertion that these exact
 * classnames and this exact nesting are emitted is what proves the inheritance
 * can happen at all, and it is also the contract the host's drag pipeline reads
 * (`data-sidebar-group-id` on the section, a `.group-head` child to measure).
 */
function readSidebarV2GroupSections(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>("[data-sidebar-v2-group-id]")];
}

async function openSidebarV2GroupMenu(
  storyRoot: ParentNode,
  groupId: string,
): Promise<void> {
  const head = await findRequiredElement(
    storyRoot,
    `[data-sidebar-v2-group-id="${groupId}"] .group-head`,
    `group head for ${groupId}`,
  );
  fireEvent.contextMenu(head, { bubbles: true, clientX: 40, clientY: 40 });
  await waitFor(() => {
    return expect(
      storyRoot.querySelector('.sidebar-v2-session-context-menu [role="menuitem"]'),
    ).toBeTruthy();
  });
}

export const GroupedHeadersUseTheClassicProjectChrome: Story = {
  args: { sidebarV2Layout: "byProject" },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);

    await step("mount the grouped list under V1's project-list classnames", async () => {
      const list = await findRequiredElement(root, ".sidebar-v2-groups", "grouped list");
      /* `reference-project-group-list` is the selector the reference-layout block
         keys its project-list spacing off; without it the list falls back to base
         group spacing that V1 deliberately removed. */
      expect(list.classList.contains("group-list")).toBe(true);
      expect(list.classList.contains("workspace-group-list")).toBe(true);
      expect(list.classList.contains("reference-project-group-list")).toBe(true);
    });

    await step("emit V1's project-header DOM for every grouped row", async () => {
      const sections = readSidebarV2GroupSections(root);
      expect(sections.length).toBeGreaterThan(1);
      for (const section of sections) {
        expect(section.tagName).toBe("SECTION");
        expect(section.classList.contains("group")).toBe(true);
        expect(section.getAttribute("data-project-group")).toBe("true");
        /* The drop-bounds contract: `getSidebarGroupBoundaryTargetAtY` finds the
           section by this attribute and measures the `.group-head` inside it. */
        expect(section.getAttribute("data-sidebar-group-id")).toBe(
          section.getAttribute("data-sidebar-v2-group-id"),
        );
        const head = section.querySelector<HTMLElement>(".group-head");
        expect(head).toBeTruthy();
        expect(head!.parentElement).toBe(section);
        expect(head!.querySelector(".group-title-wrap > .group-title-row")).toBeTruthy();
        expect(
          head!.querySelector(".group-collapse-button.section-titlebar-toggle"),
        ).toBeTruthy();
        expect(
          head!.querySelector(".group-title-handle > .group-title-button > .group-title"),
        ).toBeTruthy();
        expect(head!.querySelector(".group-title-spacer")).toBeTruthy();
        /* V2 keeps its own project icon and session count inside V1's row. */
        expect(head!.querySelector(".sidebar-v2-project-icon")).toBeTruthy();
        expect(head!.querySelector(".sidebar-v2-group-count")).toBeTruthy();
      }
    });

    await step("retire the bespoke V2 header entirely", async () => {
      expect(root.querySelector(".sidebar-v2-group-header")).toBeNull();
      expect(root.querySelector(".sidebar-v2-group-header-row")).toBeNull();
      expect(root.querySelector(".sidebar-v2-group-title")).toBeNull();
    });

    await step("put the per-project create control in V1's action cluster", async () => {
      const section = await findRequiredElement(
        root,
        '[data-sidebar-v2-group-id="v2-project-ghostex"]',
        "ghostex group",
      );
      /* `.group-header-actions` is what V1's CSS reveals on hover and what
         `shouldPreventGroupDragActivation` reads to keep these clicks from
         starting a project drag. */
      const actions = section.querySelector<HTMLElement>(".group-head .group-header-actions");
      expect(actions).toBeTruthy();
      expect(actions!.querySelector(".sidebar-v2-create-button")).toBeTruthy();
    });

    await step("mark the user's active project the way V1 does", async () => {
      const active = readSidebarV2GroupSections(root).filter(
        (section) => section.getAttribute("data-active") === "true",
      );
      expect(active.length).toBe(1);
      expect(active[0]?.getAttribute("data-sidebar-v2-group-id")).toBe("v2-project-ghostex");
    });

    await step("keep the sessions inside as V2 cards", async () => {
      const section = await findRequiredElement(
        root,
        '[data-sidebar-v2-group-id="v2-project-ghostex"]',
        "ghostex group",
      );
      expect(section.querySelectorAll(".sidebar-v2-row").length).toBeGreaterThan(0);
      /* No V1 session card leaked in with the V1 header. */
      expect(section.querySelector(".session-frame")).toBeNull();
    });
  },
};

export const CollapsesGroupedProjectsThroughTheSharedState: Story = {
  args: { sidebarV2Layout: "byProject" },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);

    const section = async () =>
      findRequiredElement(
        root,
        '[data-sidebar-v2-group-id="v2-project-zmx"]',
        "zmx group",
      );

    await step("collapse a project from V1's title button", async () => {
      expect((await section()).querySelectorAll(".sidebar-v2-row").length).toBeGreaterThan(0);
      fireEvent.click(
        await findRequiredElement(
          root,
          '[data-sidebar-v2-group-id="v2-project-zmx"] .group-title-button',
          "zmx title button",
        ),
      );
      await waitFor(async () => {
        const current = await section();
        expect(current.getAttribute("data-collapsed")).toBe("true");
        return expect(current.querySelectorAll(".sidebar-v2-row").length).toBe(0);
      });
    });

    /*
     * Collapse is the ONE piece of grouped state V1 and V2 already share: the
     * same `collapsedGroupsById`, keyed by the same representative group id, in
     * the same `ghostex-sidebar-ui-collapse-state` localStorage entry. Asserting
     * the persisted entry is what proves V2 wrote through the shared pipeline
     * rather than into a V2-local piece of state.
     */
    await step("persist it through the sidebar's shared collapse state", async () => {
      await waitFor(() => {
        const stored = window.localStorage.getItem("ghostex-sidebar-ui-collapse-state");
        expect(stored).toBeTruthy();
        const parsed = JSON.parse(stored!) as {
          collapsedGroupsById?: Record<string, boolean>;
        };
        return expect(parsed.collapsedGroupsById?.["v2-project-zmx"]).toBe(true);
      });
    });

    await step("expand it again from the collapse control", async () => {
      fireEvent.click(
        await findRequiredElement(
          root,
          '[data-sidebar-v2-group-id="v2-project-zmx"] .group-collapse-button',
          "zmx collapse button",
        ),
      );
      await waitFor(async () => {
        const current = await section();
        expect(current.getAttribute("data-collapsed")).toBe("false");
        return expect(current.querySelectorAll(".sidebar-v2-row").length).toBeGreaterThan(0);
      });
    });
  },
};

/*
 * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
 * The reported bug, as a test. One repository open on this Mac AND on Build Box
 * AND in a second local clone merges into ONE grouped row, so closing the
 * representative alone leaves the row on screen, now backed only by the members
 * the user was not thinking about. Close Project has to fan out over every
 * member checkout.
 */
export const ClosesEveryMemberCheckoutOfAGroupedProject: Story = {
  args: { fixture: "sidebar-v2-multi-machine", sidebarV2Layout: "byProject" },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);

    await step("close a merged project through every one of its members", async () => {
      resetSidebarStoryMessages();
      await openSidebarV2GroupMenu(storyRoot, "v2-mm-local");
      expect(readSidebarV2MenuLabels(storyRoot)).toEqual([
        "Group across machines",
        "Close Project",
      ]);
      fireEvent.click(
        [
          ...storyRoot.querySelectorAll<HTMLElement>(
            '.sidebar-v2-session-context-menu [role="menuitem"]',
          ),
        ].find((item) => item.textContent?.trim() === "Close Project")!,
      );
      for (const groupId of ["v2-mm-local"]) {
        await expectMessage({ groupId, type: "closeWorkspaceProjectForGroup" });
      }
      /* Nothing else: the row's members are exactly those three. */
      expect(
        getSidebarStoryMessages().filter(
          (message) => message.type === "closeWorkspaceProjectForGroup",
        ).length,
      ).toBe(3);
    });

    await step("offer Close Project on a project with no git origin", async () => {
      /*
       * The grouping submenu cannot apply without a remote to merge on, and the
       * group menu used to be suppressed entirely for exactly that case — which
       * is why a non-git project had no way out of the grouped list at all.
       */
      await openSidebarV2GroupMenu(storyRoot, "v2-mm-notes");
      expect(readSidebarV2MenuLabels(storyRoot)).toEqual(["Close Project"]);
    });
  },
};

export const HidesCloseProjectOnRowsThatAreNotProjects: Story = {
  args: { sidebarV2Layout: "byProject" },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);

    await step("open no group menu at all on the Quick collection", async () => {
      /*
       * Quick has no `projectContext`, so it is neither closable nor mergeable
       * and the builder produces nothing. A menu with no items must not open —
       * an empty popover over the rows would be worse than no response.
       */
      const head = await findRequiredElement(
        root,
        '[data-sidebar-v2-group-id="v2-quick"] .group-head',
        "quick group head",
      );
      fireEvent.contextMenu(head, { bubbles: true, clientX: 40, clientY: 40 });
      await new Promise((resolve) => globalThis.setTimeout(resolve, 60));
      expect(storyRoot.querySelector(".sidebar-v2-session-context-menu")).toBeNull();
    });
  },
};

/*
 * CDXC:SidebarV2ContextMenuLook 2026-07-30:
 * The V2 menu must not merely resemble the classic one, it must BE it: the same
 * portal, the same classnames, and therefore the same chrome rules, with a
 * submenu that flies out into its own stacked panel instead of expanding inline.
 * Every assertion below reads the SHIPPED result of that reuse, so restyling a
 * copy of the chrome would fail here even if it looked right in a screenshot.
 */
export const MatchesTheClassicContextMenuChrome: Story = {
  args: { showSessionCommandCopyActions: true, showSessionDetailsCopyAction: true },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    await waitForSidebarV2(storyRoot);
    const body = within(storyRoot);

    const findMenu = (): HTMLElement => {
      const menu = storyRoot.querySelector<HTMLElement>(".sidebar-v2-session-context-menu");
      expect(menu).toBeTruthy();
      return menu!;
    };

    await step("the row menu is V1's portal, under V1's classnames", async () => {
      await openSidebarV2RowMenu(storyRoot, "v2-quick-idle");
      const menu = findMenu();
      /* `sidebar-session-context-menu` is the classic SESSION menu's own class:
         carrying it is what makes the width and the max-width V1's, rather than
         letting each row's longest label decide how wide its menu is. */
      expect(menu.classList.contains("session-context-menu")).toBe(true);
      expect(menu.classList.contains("sidebar-session-context-menu")).toBe(true);
      expect(menu.getAttribute("role")).toBe("menu");
      /* A portal into the document body, with the shared click-away backdrop
         under it — not a popover parented into the sidebar tree. */
      expect(menu.parentElement).toBe(storyRoot);
      expect(storyRoot.querySelectorAll(".sidebar-context-menu-backdrop")).toHaveLength(1);
    });

    await step("the chrome measures what the shared rules say", async () => {
      const menu = findMenu();
      const menuStyle = getComputedStyle(menu);
      /* 178px is `min(178px, calc(100vw - 24px))`; assert the viewport is wide
         enough that the clamp is not what is being measured. */
      expect(window.innerWidth).toBeGreaterThan(202);
      expect(menuStyle.width).toBe("178px");
      expect(menuStyle.padding).toBe("6px");
      expect(menuStyle.display).toBe("grid");
      const item = await body.findByRole("menuitem", { name: "Rename" });
      expect(getComputedStyle(item).padding).toBe("8px 10px");
    });

    await step("sections and dividers are the menu grid's own children", async () => {
      /* V1 renders each section in a Fragment, so the divider and the section are
         siblings in the menu's grid and the 2px gap applies to both. A wrapper
         element per section would silently change every gap between sections. */
      const menu = findMenu();
      const childClasses = [...menu.children].map((child) => child.className);
      expect(childClasses.length).toBeGreaterThan(1);
      for (const childClass of childClasses) {
        expect(
          childClass.includes("session-context-menu-section") ||
            childClass.includes("session-context-menu-divider"),
        ).toBe(true);
      }
    });

    await step("a submenu parent advertises itself with V1's trailing chevron", async () => {
      const tagAs = await body.findByRole("menuitem", { name: "Tag as" });
      expect(tagAs.getAttribute("aria-haspopup")).toBe("menu");
      expect(tagAs.getAttribute("aria-expanded")).toBe("false");
      expect(tagAs.querySelector(".session-context-menu-trailing-icon")).toBeTruthy();
      /* Snooze carries the same affordance: it is the same mechanism. */
      const snooze = await body.findByRole("menuitem", { name: "Snooze" });
      expect(snooze.querySelector(".session-context-menu-trailing-icon")).toBeTruthy();
    });

    await step("Tag as flies out into its own stacked panel", async () => {
      const menu = findMenu();
      const tagAs = await body.findByRole("menuitem", { name: "Tag as" });
      const tagAsBounds = tagAs.getBoundingClientRect();
      fireEvent.click(tagAs);

      const submenu = await waitFor(() => {
        const panel = storyRoot.querySelector<HTMLElement>(".sidebar-v2-context-submenu");
        expect(panel).toBeTruthy();
        return panel!;
      });
      /* A separate portal: not nested inside the menu it came from, and stacked
         above it, exactly like the classic Tag as submenu. */
      expect(menu.contains(submenu)).toBe(false);
      expect(submenu.parentElement).toBe(storyRoot);
      expect(submenu.classList.contains("session-context-menu")).toBe(true);
      const submenuStyle = getComputedStyle(submenu);
      expect(Number(submenuStyle.zIndex)).toBeGreaterThan(Number(getComputedStyle(menu).zIndex));
      /*
       * Anchored to its parent ROW — V1's geometry: left edge aligned with the
       * row, 4px under it, then clamped inside the viewport by the SAME 12px
       * margin the parent portal uses. The clamp is part of the contract, not an
       * exception to it: this panel is 8 tag markers tall and a row near the
       * bottom of a real sidebar cannot open one downwards, so the assertion
       * states the anchor AND the clamp rather than a canvas-height-dependent
       * "below the row".
       */
      const margin = 12;
      const submenuBounds = submenu.getBoundingClientRect();
      expect(Math.round(submenuBounds.left)).toBe(
        Math.round(
          Math.max(
            margin,
            Math.min(tagAsBounds.left, window.innerWidth - submenuBounds.width - margin),
          ),
        ),
      );
      expect(Math.round(submenuBounds.top)).toBe(
        Math.round(
          Math.max(
            margin,
            Math.min(tagAsBounds.bottom + 4, window.innerHeight - submenuBounds.height - margin),
          ),
        ),
      );
      expect(submenuBounds.bottom).toBeLessThanOrEqual(window.innerHeight - margin + 0.5);
      /* The parent menu stays open underneath it. */
      expect(storyRoot.querySelector(".sidebar-v2-session-context-menu")).toBeTruthy();
      expect(tagAs.getAttribute("aria-expanded")).toBe("true");
      /* The tag markers keep V1's grouped blocks, so the eight options read as
         Favorite / progress / type instead of one flat run. */
      expect(submenu.querySelectorAll(".session-tag-menu-section").length).toBeGreaterThan(1);
      expect(submenu.querySelectorAll(".sidebar-v2-context-submenu-item").length).toBe(8);
      /* No item is left indented as if it were still an inline child row. */
      const submenuItem = submenu.querySelector<HTMLElement>(".sidebar-v2-context-submenu-item");
      expect(getComputedStyle(submenuItem!).padding).toBe("8px 10px");
    });

    await step("Escape dismisses the panel and the menu together", async () => {
      fireEvent.keyDown(document, { key: "Escape" });
      await waitFor(() => {
        expect(storyRoot.querySelector(".sidebar-v2-context-submenu")).toBeNull();
        return expect(storyRoot.querySelector(".sidebar-v2-session-context-menu")).toBeNull();
      });
    });
  },
};

/**
 * The group menu is the same renderer, so it inherits the same chrome by
 * construction — including the grouping submenu, which flies out on the same
 * mechanism the tag markers use.
 */
export const GroupMenuMatchesTheClassicContextMenuChrome: Story = {
  args: { fixture: "sidebar-v2-multi-machine", sidebarV2Layout: "byProject" },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    await waitForSidebarV2(storyRoot);
    const body = within(storyRoot);

    await step("open a merged project's menu at V1's project-menu width", async () => {
      await openSidebarV2GroupMenu(storyRoot, "v2-mm-local");
      const menu = storyRoot.querySelector<HTMLElement>(".sidebar-v2-session-context-menu");
      expect(menu).toBeTruthy();
      expect(menu!.classList.contains("session-context-menu")).toBe(true);
      /*
       * A PROJECT row's menu, so 196px — the classic sidebar's project-menu
       * width, not its 178px session-menu width. The session class must be
       * absent for the same reason: it is the thing that would impose 178px.
       */
      expect(getComputedStyle(menu!).width).toBe("196px");
      expect(menu!.classList.contains("sidebar-session-context-menu")).toBe(false);
      expect(menu!.parentElement).toBe(storyRoot);
      expect(storyRoot.querySelectorAll(".sidebar-context-menu-backdrop")).toHaveLength(1);
    });

    await step("the grouping choice flies out as its own panel", async () => {
      const menu = storyRoot.querySelector<HTMLElement>(".sidebar-v2-session-context-menu")!;
      const grouping = await body.findByRole("menuitem", { name: "Group across machines" });
      const chevron = grouping.querySelector<HTMLElement>(".session-context-menu-trailing-icon");
      expect(chevron).toBeTruthy();
      /*
       * The affordance has to be INSIDE the menu box, not pushed past its edge by
       * a long label. This is the item that would lose it: at V1's fixed menu
       * width, "Group across machines" is wide enough to clip a trailing glyph.
       */
      expect(chevron!.getBoundingClientRect().right).toBeLessThanOrEqual(
        menu.getBoundingClientRect().right,
      );
      fireEvent.click(grouping);
      const submenu = await waitFor(() => {
        const panel = storyRoot.querySelector<HTMLElement>(".sidebar-v2-context-submenu");
        expect(panel).toBeTruthy();
        return panel!;
      });
      /* Radio options, so exactly one block and no divider inside it. */
      expect(submenu.querySelectorAll(".session-tag-menu-section")).toHaveLength(1);
      expect(await body.findAllByRole("menuitemradio")).toHaveLength(3);
      /* Close Project is a top-level item and must not have moved into the
         flyout: the destructive verb stays one click from the pointer. */
      const closeProject = await body.findByRole("menuitem", { name: "Close Project" });
      expect(submenu.contains(closeProject)).toBe(false);
    });
  },
};

/*
 * CDXC:SidebarV2GroupedProjectUX 2026-07-30:
 * Grouped V2 project REORDER, end to end through the real dnd-kit pipeline: the
 * one DragDropProvider that now wraps both sidebar bodies, V1's group sensors
 * (8px distance OR a 250ms hold), pointer-resolved drop targets because project
 * drags use `feedback: "none"`, and the per-machine projection on release.
 *
 * The multi-machine fixture is the interesting shape: its first row MERGES three
 * checkouts (this Mac's clone, a second local clone, and Build Box's), so the
 * committed order is not "move one id" — it is the merged block moving inside the
 * local machine's own list while Build Box, which owns no member of the row being
 * dragged, is left alone. Cross-machine fan-out of the projection itself is unit
 * tested in `shared/sidebar-v2-group-order.test.ts`, where several machines can be
 * given several projects each.
 */
export const ReordersGroupedProjectsByDrag: Story = {
  args: { fixture: "sidebar-v2-multi-machine", sidebarV2Layout: "byProject" },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);

    const groupHead = async (groupId: string) =>
      findRequiredElement(
        root,
        `[data-sidebar-v2-group-id="${groupId}"] .group-head`,
        `group head for ${groupId}`,
      );

    await step("start from the merged repository above the non-git project", async () => {
      await waitFor(() => {
        return expect(
          readSidebarV2GroupSections(root).map((section) =>
            section.getAttribute("data-sidebar-v2-group-id"),
          ),
        ).toEqual(["v2-mm-local", "v2-mm-notes"]);
      });
    });

    await step("drag the non-git project above the merged repository", async () => {
      resetSidebarStoryMessages();
      await dragAndDrop(await groupHead("v2-mm-notes"), await groupHead("v2-mm-local"), "before");
      /*
       * ONE message, for the local daemon only, carrying that machine's WHOLE
       * project list — `syncGroupOrder` rejects a list that mixes machines, and
       * Build Box owns no member of the row that moved, so it must not be told
       * anything at all.
       */
      await expectMessage({
        groupIds: ["v2-mm-notes", "v2-mm-local", "v2-mm-local-copy"],
        type: "syncGroupOrder",
      });
      expect(
        getSidebarStoryMessages().filter((message) => message.type === "syncGroupOrder").length,
      ).toBe(1);
    });

    /*
     * The order the user SEES comes back from the host, not from V2: this sidebar
     * is not optimistic about project order any more than it is about session
     * lifecycle. Storybook's stand-in host cannot echo it, because
     * `syncGroupOrderInWorkspace` only accepts a list covering every group in the
     * snapshot, while a per-machine order deliberately covers one machine's
     * groups. That is a limitation of the stand-in, not of the product — gxserver's
     * `syncWorkspaceGroupOrder` is built for exactly the partial per-machine shape
     * (local ids normalize into the workspace project order, remote ids into that
     * machine's order overlay). So the assertions here stay on the MESSAGES, which
     * is what the rest of the V2 suite is built around, plus the drag CHROME below.
     */
    await step("wear V1's drag chrome while a project row is in flight", async () => {
      const source = await groupHead("v2-mm-local");
      const dragState = await dragToHover(source, await groupHead("v2-mm-notes"), "after");
      const sourceSection = source.closest<HTMLElement>("[data-sidebar-v2-group-id]");
      /* V1 paints the grabbed row as a faint placeholder and marks the insertion
         boundary on the TARGET row, both driven off these attributes. */
      expect(sourceSection?.getAttribute("data-dragging")).toBe("true");
      await waitFor(() => {
        return expect(
          [...root.querySelectorAll<HTMLElement>("[data-group-drop-position]")].map(
            (section) =>
              `${section.getAttribute("data-sidebar-v2-group-id")}:${section.getAttribute(
                "data-group-drop-position",
              )}`,
          ),
        ).toEqual(["v2-mm-notes:after"]);
      });

      resetSidebarStoryMessages();
      await releaseDrag(await groupHead("v2-mm-notes"), dragState);
      /*
       * The merged row's two LOCAL checkouts travel together, in the order the
       * machine already had them — a merged row is one project to the user, so it
       * cannot leave half of itself behind. Build Box still hears nothing: moving
       * this row past a project that does not exist there changes nothing there.
       */
      await expectMessage({
        groupIds: ["v2-mm-notes", "v2-mm-local", "v2-mm-local-copy"],
        type: "syncGroupOrder",
      });
      expect(
        getSidebarStoryMessages().filter((message) => message.type === "syncGroupOrder").length,
      ).toBe(1);
      await waitFor(() => {
        return expect(
          root.querySelector('[data-sidebar-v2-group-id][data-dragging="true"]'),
        ).toBeNull();
      });
    });
  },
};
