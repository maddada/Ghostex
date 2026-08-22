import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fireEvent, waitFor, within } from "storybook/test";
import { findRequiredElement } from "../sidebar-app.interactions.helpers";
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
import { findSidebarV2Row, waitForSidebarV2 } from "./sidebar-v2.story-helpers";

/*
 * CDXC:SidebarV2Worktree 2026-07-29:
 * Interaction coverage for V2's worktree flow. Like the rest of the V2 stories
 * these assert MESSAGES, not moved rows: nothing here is optimistic, the host
 * owns the outcome, and the only local state is the pending/dirty bookkeeping
 * that the mocked host answers drive.
 *
 * The host is mocked the way every request/response surface in this harness
 * mocks it: read the request the sidebar posted (for its generated requestId),
 * then push the answer back through the same `window` message source SidebarApp
 * listens on.
 */

const WORKTREE_SESSION_ID = "v2-ghostex-working";
const WORKTREE_PATH = "/Users/story/dev/worktrees/sidebar-v2-inbox";
const PROJECT_GROUP_ID = "v2-project-ghostex";
/** A probed session whose cwd IS the project's own checkout, not a worktree. */
const PROJECT_ROOT_SESSION_ID = "v2-ghostex-pinned";

const meta = {
  title: "Sidebar/V2 Worktrees",
  args: {
    ...DEFAULT_SIDEBAR_STORY_ARGS,
    fixture: "sidebar-v2-inbox",
    sidebarLifecycleCapabilities: "settleSnoozeGitAndWorktree",
    sidebarV2Layout: "flat",
    sidebarVersion: "v2",
  },
  argTypes: SIDEBAR_STORY_ARG_TYPES,
  decorators: SIDEBAR_STORY_DECORATORS,
  render: renderSidebarStory,
} satisfies Meta<SidebarStoryArgs>;

export default meta;

type Story = StoryObj<typeof meta>;

type StoryMessage = Record<string, unknown> & { type: string };

async function findPostedMessage(type: string): Promise<StoryMessage> {
  let match: StoryMessage | undefined;
  await waitFor(() => {
    match = (getSidebarStoryMessages() as unknown as StoryMessage[]).find(
      (message) => message.type === type,
    );
    return expect(match).toBeTruthy();
  });
  if (!match) {
    throw new Error(`No ${type} message was posted`);
  }
  return match;
}

/** Answer the popover's branch/worktree probe as the host would. */
async function answerWorktreeListRequest(): Promise<void> {
  const request = await findPostedMessage("requestProjectWorktrees");
  window.postMessage(
    {
      branches: [
        { current: true, name: "main" },
        { current: false, name: "release/6.9" },
      ],
      ok: true,
      requestId: request.requestId,
      type: "projectWorktreesResult",
      worktrees: [{ branch: "ghostex/1a2b3c4d", path: WORKTREE_PATH }],
    },
    "*",
  );
}

async function openWorktreePopover(storyRoot: ParentNode): Promise<HTMLElement> {
  const chevron = await findRequiredElement(
    storyRoot,
    '.sidebar-v2-toolbar [aria-label="New session options"]',
    "create split chevron",
  );
  fireEvent.click(chevron);
  const body = within(storyRoot as HTMLElement);
  fireEvent.click(await body.findByRole("menuitem", { name: /New worktree session/ }));
  return findRequiredElement(storyRoot, ".sidebar-v2-worktree-popover", "worktree popover");
}

export const CreatesWorktreeSessionFromPopover: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    await waitForSidebarV2(storyRoot);
    resetSidebarStoryMessages();

    const popover = await openWorktreePopover(storyRoot);

    await step("the popover fills its pickers from the host's worktree probe", async () => {
      await answerWorktreeListRequest();
      await waitFor(async () => {
        const branchField = await findRequiredElement(
          popover,
          '[data-worktree-field="baseBranch"]',
          "base branch field",
        );
        return expect(branchField.tagName).toBe("SELECT");
      });
    });

    await step("submitting posts createWorktreeSession with the whole draft", async () => {
      resetSidebarStoryMessages();
      const branchField = await findRequiredElement(
        popover,
        '[data-worktree-field="baseBranch"]',
        "base branch field",
      );
      fireEvent.change(branchField, { target: { value: "release/6.9" } });
      fireEvent.click(
        await findRequiredElement(
          popover,
          '[data-worktree-field="startFromOrigin"]',
          "start from origin toggle",
        ),
      );
      fireEvent.change(
        await findRequiredElement(popover, '[data-worktree-field="firstPrompt"]', "first prompt"),
        { target: { value: "Port the worktree flow" } },
      );
      fireEvent.click(
        await findRequiredElement(popover, ".sidebar-v2-worktree-submit", "submit"),
      );

      const posted = await findPostedMessage("createWorktreeSession");
      expect(posted.projectId).toBe(PROJECT_GROUP_ID);
      expect(posted.baseBranch).toBe("release/6.9");
      expect(posted.startFromOrigin).toBe(true);
      expect(posted.firstPrompt).toBe("Port the worktree flow");
      expect(typeof posted.requestId).toBe("string");
      expect(posted.existingWorktreePath).toBeUndefined();
    });

    await step("the form stays pending until the host answers, then closes", async () => {
      const submit = await findRequiredElement(
        storyRoot,
        ".sidebar-v2-worktree-submit",
        "submit",
      );
      expect(submit).toHaveAttribute("data-pending", "true");

      const posted = await findPostedMessage("createWorktreeSession");
      window.postMessage(
        {
          branch: "ghostex/9f8e7d6c",
          ok: true,
          requestId: posted.requestId,
          sessionId: "v2-ghostex-worktree-new",
          type: "worktreeSessionResult",
          worktreePath: "/Users/story/dev/worktrees/ghostex-9f8e7d6c",
        },
        "*",
      );
      await waitFor(() => {
        return expect(storyRoot.querySelector(".sidebar-v2-worktree-popover")).toBeNull();
      });
    });
  },
};

export const OpensExistingWorktree: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    await waitForSidebarV2(storyRoot);
    resetSidebarStoryMessages();

    const popover = await openWorktreePopover(storyRoot);
    await answerWorktreeListRequest();

    await step("an existing checkout submits its path instead of a base branch", async () => {
      const existing = await findRequiredElement(
        popover,
        `.sidebar-v2-worktree-existing-item[data-worktree-path="${WORKTREE_PATH}"]`,
        "existing worktree entry",
      );
      resetSidebarStoryMessages();
      fireEvent.click(existing);

      const posted = await findPostedMessage("createWorktreeSession");
      expect(posted.existingWorktreePath).toBe(WORKTREE_PATH);
      expect(posted.projectId).toBe(PROJECT_GROUP_ID);
      expect(posted.baseBranch).toBeUndefined();
      expect(posted.startFromOrigin).toBeUndefined();
    });
  },
};

export const StartsSessionOnExistingBranch: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    await waitForSidebarV2(storyRoot);
    const body = within(storyRoot);
    resetSidebarStoryMessages();

    /*
     * The main checkout is not a worktree — `git worktree list` never offers it,
     * so gxserver refuses to spawn "into" it. The item must therefore be ABSENT
     * on a row whose cwd is the project root, however well-probed its branch is;
     * "another session in this project" is what the plain "+" already does.
     */
    await step("a session in the project's own checkout is never offered", async () => {
      const row = await findSidebarV2Row(storyRoot, PROJECT_ROOT_SESSION_ID);
      fireEvent.contextMenu(row);
      await body.findByRole("menuitem", { name: "Close" });
      expect(body.queryByRole("menuitem", { name: /New session on/ })).toBeNull();
      fireEvent.keyDown(storyRoot, { key: "Escape" });
      await waitFor(() => {
        return expect(body.queryByRole("menuitem", { name: "Close" })).toBeNull();
      });
    });

    await step("the row's branch is offered as a new-session target", async () => {
      const row = await findSidebarV2Row(storyRoot, WORKTREE_SESSION_ID);
      fireEvent.contextMenu(row);
      fireEvent.click(
        await body.findByRole("menuitem", { name: "New session on ghostex/sidebar-v2-inbox" }),
      );

      const posted = await findPostedMessage("createWorktreeSession");
      expect(posted.existingWorktreePath).toBe(WORKTREE_PATH);
      expect(posted.projectId).toBe(PROJECT_GROUP_ID);
    });
  },
};

export const CleansUpTheLastSessionsWorktree: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    await waitForSidebarV2(storyRoot);
    const body = within(storyRoot);
    resetSidebarStoryMessages();

    await step("closing the last session in a worktree asks about the checkout", async () => {
      const row = await findSidebarV2Row(storyRoot, WORKTREE_SESSION_ID);
      fireEvent.contextMenu(row);
      fireEvent.click(await body.findByRole("menuitem", { name: "Close" }));
      await findRequiredElement(
        storyRoot,
        ".sidebar-v2-worktree-cleanup",
        "worktree cleanup prompt",
      );
    });

    await step("removing closes the session and asks the host to delete", async () => {
      fireEvent.click(
        await findRequiredElement(
          storyRoot,
          '[data-worktree-cleanup-action="remove"]',
          "remove worktree",
        ),
      );
      const closed = await findPostedMessage("closeSession");
      expect(closed.sessionId).toBe(WORKTREE_SESSION_ID);
      const removal = await findPostedMessage("removeSessionWorktree");
      expect(removal.worktreePath).toBe(WORKTREE_PATH);
      expect(removal.projectId).toBe(PROJECT_GROUP_ID);
      expect(removal.force).toBeUndefined();
    });

    await step("a dirty refusal re-asks with force instead of failing", async () => {
      const removal = await findPostedMessage("removeSessionWorktree");
      window.postMessage(
        {
          dirty: true,
          ok: true,
          removed: false,
          requestId: removal.requestId,
          type: "sessionWorktreeRemovalResult",
          warnings: ["2 uncommitted files"],
          worktreePath: WORKTREE_PATH,
        },
        "*",
      );
      const force = await findRequiredElement(
        storyRoot,
        '[data-worktree-cleanup-action="force"]',
        "force remove",
      );
      resetSidebarStoryMessages();
      fireEvent.click(force);

      const forced = await findPostedMessage("removeSessionWorktree");
      expect(forced.force).toBe(true);
      /* The session was already closed on the first pass; do not close twice. */
      expect(
        (getSidebarStoryMessages() as unknown as StoryMessage[]).filter(
          (message) => message.type === "closeSession",
        ),
      ).toHaveLength(0);

      window.postMessage(
        {
          ok: true,
          removed: true,
          requestId: forced.requestId,
          type: "sessionWorktreeRemovalResult",
          worktreePath: WORKTREE_PATH,
        },
        "*",
      );
      await waitFor(() => {
        return expect(storyRoot.querySelector(".sidebar-v2-worktree-cleanup")).toBeNull();
      });
    });
  },
};

export const KeepsTheWorktreeWhenAsked: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    await waitForSidebarV2(storyRoot);
    const body = within(storyRoot);
    resetSidebarStoryMessages();

    await step("keeping the checkout closes the session and nothing else", async () => {
      const row = await findSidebarV2Row(storyRoot, WORKTREE_SESSION_ID);
      fireEvent.contextMenu(row);
      fireEvent.click(await body.findByRole("menuitem", { name: "Close" }));
      fireEvent.click(
        await findRequiredElement(
          storyRoot,
          '[data-worktree-cleanup-action="keep"]',
          "keep worktree",
        ),
      );

      const closed = await findPostedMessage("closeSession");
      expect(closed.sessionId).toBe(WORKTREE_SESSION_ID);
      expect(
        (getSidebarStoryMessages() as unknown as StoryMessage[]).filter(
          (message) => message.type === "removeSessionWorktree",
        ),
      ).toHaveLength(0);
      await waitFor(() => {
        return expect(storyRoot.querySelector(".sidebar-v2-worktree-cleanup")).toBeNull();
      });
    });
  },
};

/*
 * The capability-absent case is the important one for the rollout: a daemon
 * that predates the worktree flow must leave V2 exactly as it was — no worktree
 * items anywhere, no worktree context item, and a close that never asks about
 * checkouts.
 *
 * CDXC:SidebarV2SingleCreateControl 2026-07-30:
 * The chevron itself is no longer part of that promise: it now holds the agent
 * picker and the Quick entries, which every daemon can serve, so it stays. What
 * must disappear is every WORKTREE item inside it — the entry point and the
 * default-to-worktree preference, which is meaningless without the capability.
 */
export const HidesWorktreeAffordancesWithoutCapability: Story = {
  args: { sidebarLifecycleCapabilities: "settleSnoozeAndGit" },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);
    const body = within(storyRoot);
    resetSidebarStoryMessages();

    await step("the create menu keeps its agent and Quick items only", async () => {
      await findRequiredElement(
        root,
        ".sidebar-v2-toolbar .sidebar-v2-create-button",
        "plain create button",
      );
      fireEvent.click(
        await findRequiredElement(
          root,
          '.sidebar-v2-toolbar [aria-label="New session options"]',
          "create split chevron",
        ),
      );
      await body.findByRole("menuitem", { name: "Quick Terminal" });
      expect(body.queryByRole("menuitem", { name: /New worktree session/ })).toBeNull();
      expect(
        body.queryByRole("menuitemcheckbox", { name: /Default new sessions to worktree/ }),
      ).toBeNull();
      fireEvent.keyDown(storyRoot, { key: "Escape" });
    });

    await step("the branch context item is absent", async () => {
      const row = await findSidebarV2Row(storyRoot, WORKTREE_SESSION_ID);
      fireEvent.contextMenu(row);
      await body.findByRole("menuitem", { name: "Close" });
      expect(body.queryByRole("menuitem", { name: /New session on/ })).toBeNull();
    });

    await step("closing posts the plain close command with no prompt", async () => {
      fireEvent.click(await body.findByRole("menuitem", { name: "Close" }));
      const closed = await findPostedMessage("closeSession");
      expect(closed.sessionId).toBe(WORKTREE_SESSION_ID);
      expect(storyRoot.querySelector(".sidebar-v2-worktree-cleanup")).toBeNull();
    });
  },
};

export const PlainCreateButtonStartsAnInstantSession: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);
    resetSidebarStoryMessages();

    await step("the plain half posts the unchanged agent launch", async () => {
      fireEvent.click(
        await findRequiredElement(
          root,
          ".sidebar-v2-toolbar .sidebar-v2-create-button",
          "plain create button",
        ),
      );
      const posted = await findPostedMessage("runSidebarAgent");
      expect(typeof posted.agentId).toBe("string");
      /*
       * CDXC:SidebarV2SingleCreateControl 2026-07-30:
       * The header "+" resolves a real project — here the fixture's ACTIVE one —
       * instead of substituting the Quick collection because the click did not
       * come from a project row.
       */
      expect(posted.groupId).toBe(PROJECT_GROUP_ID);
      expect(
        (getSidebarStoryMessages() as unknown as StoryMessage[]).filter(
          (message) => message.type === "createWorktreeSession",
        ),
      ).toHaveLength(0);
    });
  },
};

export const GroupHeaderCreatesInItsOwnProject: Story = {
  args: { sidebarV2Layout: "byProject" },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);
    resetSidebarStoryMessages();

    await step("a project group's + launches into that group", async () => {
      const group = await findRequiredElement(
        root,
        `[data-sidebar-v2-group-id="${PROJECT_GROUP_ID}"]`,
        "project group",
      );
      fireEvent.click(
        await findRequiredElement(group, ".sidebar-v2-create-button", "group create button"),
      );
      const posted = await findPostedMessage("runSidebarAgent");
      expect(posted.groupId).toBe(PROJECT_GROUP_ID);
    });

    await step("its chevron opens the worktree popover for the same project", async () => {
      const group = await findRequiredElement(
        root,
        `[data-sidebar-v2-group-id="${PROJECT_GROUP_ID}"]`,
        "project group",
      );
      fireEvent.click(
        await findRequiredElement(group, ".sidebar-v2-create-chevron", "group create chevron"),
      );
      const body = within(storyRoot);
      fireEvent.click(await body.findByRole("menuitem", { name: /New worktree session/ }));
      await findRequiredElement(storyRoot, ".sidebar-v2-worktree-popover", "worktree popover");
      resetSidebarStoryMessages();
      fireEvent.click(
        await findRequiredElement(storyRoot, ".sidebar-v2-worktree-submit", "submit"),
      );
      const posted = await findPostedMessage("createWorktreeSession");
      expect(posted.projectId).toBe(PROJECT_GROUP_ID);
    });
  },
};

export const TogglesTheWorktreeDefault: Story = {
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const root = await waitForSidebarV2(storyRoot);
    const body = within(storyRoot);
    resetSidebarStoryMessages();

    await step("the + menu writes the global default through settings", async () => {
      fireEvent.click(
        await findRequiredElement(
          root,
          '.sidebar-v2-toolbar [aria-label="New session options"]',
          "create split chevron",
        ),
      );
      fireEvent.click(
        await body.findByRole("menuitemcheckbox", { name: /Default new sessions to worktree/ }),
      );
      await waitFor(() => {
        const matched = (getSidebarStoryMessages() as unknown as StoryMessage[]).some(
          (message) =>
            message.type === "updateSettingsPatch" &&
            (message.patch as Record<string, unknown> | undefined)?.[
              "newSessionsDefaultEnvMode"
            ] === "worktree",
        );
        return expect(matched).toBe(true);
      });
    });
  },
};
