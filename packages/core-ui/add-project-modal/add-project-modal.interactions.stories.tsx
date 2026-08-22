import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fireEvent, waitFor } from "storybook/test";
import { findRequiredElement } from "../sidebar-app.interactions.helpers";
import {
  ADD_PROJECT_STORY_LOCAL_MACHINE,
  ADD_PROJECT_STORY_REMOTE_MACHINE,
} from "./add-project-modal-mocks";
import {
  AddProjectStoryHarness,
  findAddProjectStoryCall,
  getAddProjectStoryMocks,
} from "./add-project-modal.story-harness";

/*
 * CDXC:AddProject 2026-07-30:
 * Interaction coverage for the shared add-project dialog. These assert the
 * CALLBACK PAYLOADS the dialog would send to gxserver plus the exact
 * keyboard model, because that model is the whole point of the port:
 * nothing is auto-highlighted in path modes, so plain Enter always submits the
 * typed path and only an explicit highlight (arrow keys / hover) turns Enter
 * into "descend" with mod+Enter as the override.
 */

const meta = {
  title: "Modals/Add Project Interactions",
  component: AddProjectStoryHarness,
} satisfies Meta<typeof AddProjectStoryHarness>;

export default meta;

type Story = StoryObj<typeof meta>;

async function findDialog(storyRoot: ParentNode): Promise<HTMLElement> {
  return findRequiredElement(storyRoot, "[data-add-project-modal]", "add project dialog");
}

async function findPathInput(dialog: ParentNode): Promise<HTMLInputElement> {
  const input = await findRequiredElement(
    dialog,
    '[data-add-project-field="pathInput"]',
    "path input",
  );
  return input as HTMLInputElement;
}

function typeQuery(input: HTMLInputElement, value: string): void {
  fireEvent.change(input, { target: { value } });
}

async function chooseSource(dialog: ParentNode, source: string): Promise<void> {
  const row = await findRequiredElement(
    dialog,
    `[data-add-project-field="sourceOption"][data-add-project-source="${source}"]`,
    `${source} source row`,
  );
  fireEvent.click(row);
}

function listedSources(dialog: ParentNode): string[] {
  return Array.from(dialog.querySelectorAll('[data-add-project-field="sourceOption"]')).map(
    (row) => row.getAttribute("data-add-project-source") ?? "",
  );
}

function listedDirectories(dialog: ParentNode): string[] {
  return Array.from(dialog.querySelectorAll('[data-add-project-field="directoryEntry"]')).map(
    (row) => row.getAttribute("data-add-project-path") ?? "",
  );
}

/** 1. Browse into a directory and back out through the `..` row. */
export const BrowsesDescendsAndGoesUp: Story = {
  args: {},
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const dialog = await findDialog(storyRoot);

    await step("Local folder opens the browser at the machine's base directory", async () => {
      await chooseSource(dialog, "local");
      const input = await findPathInput(dialog);
      await expect(input.value).toBe("~/");
      await waitFor(async () =>
        expect(listedDirectories(dialog)).toContain("/Users/story/dev"),
      );
      await expect(listedDirectories(dialog)).not.toContain("/Users/story/.config");
    });

    await step("clicking a directory descends into it", async () => {
      fireEvent.click(
        await findRequiredElement(dialog, '[data-add-project-path="/Users/story/dev"]', "dev row"),
      );
      const input = await findPathInput(dialog);
      await expect(input.value).toBe("~/dev/");
      await waitFor(async () =>
        expect(listedDirectories(dialog)).toContain("/Users/story/dev/ghostex"),
      );
    });

    await step("the .. row walks back to the parent directory", async () => {
      fireEvent.click(
        await findRequiredElement(dialog, '[data-add-project-field="directoryUp"]', "up row"),
      );
      const input = await findPathInput(dialog);
      await expect(input.value).toBe("~/");
    });
  },
};

/** 2. With nothing highlighted, Enter submits the typed path. */
export const EnterSubmitsTypedPath: Story = {
  args: {},
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const dialog = await findDialog(storyRoot);
    await chooseSource(dialog, "local");
    const input = await findPathInput(dialog);

    await step("typing a directory path never preselects a suggestion", async () => {
      typeQuery(input, "~/dev/ghostex/");
      await waitFor(async () =>
        expect(listedDirectories(dialog)).toContain("/Users/story/dev/ghostex/sidebar"),
      );
      await expect(dialog.querySelector('[aria-selected="true"]')).toBeNull();
    });

    await step("Enter adds the server-resolved directory", async () => {
      fireEvent.keyDown(input, { key: "Enter" });
      await waitFor(async () =>
        expect(findAddProjectStoryCall("addProject")).toEqual({
          createIfMissing: false,
          machineId: "local",
          path: "/Users/story/dev/ghostex",
        }),
      );
      await waitFor(async () =>
        expect(storyRoot.querySelector("[data-add-project-modal]")).toBeNull(),
      );
    });
  },
};

/** 3. Highlighted row: Enter descends, mod+Enter submits the typed path anyway. */
export const ModifierEnterOverridesHighlight: Story = {
  args: {},
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const dialog = await findDialog(storyRoot);
    await chooseSource(dialog, "local");
    let input = await findPathInput(dialog);

    await step("Enter on a highlighted directory descends into it", async () => {
      typeQuery(input, "~/dev/");
      await waitFor(async () =>
        expect(listedDirectories(dialog)).toContain("/Users/story/dev/ghostex"),
      );
      fireEvent.keyDown(input, { key: "ArrowDown" });
      fireEvent.keyDown(input, { key: "ArrowDown" });
      const submit = await findRequiredElement(
        dialog,
        '[data-add-project-field="submit"]',
        "submit button",
      );
      await waitFor(async () =>
        expect(submit.getAttribute("aria-label")).toBe("Add (⌘ Enter)"),
      );
      fireEvent.keyDown(input, { key: "Enter" });
      input = await findPathInput(dialog);
      await expect(input.value).toBe("~/dev/ghostex/");
    });

    await step("mod+Enter submits the typed path even with a highlighted row", async () => {
      typeQuery(input, "~/dev/");
      await waitFor(async () =>
        expect(listedDirectories(dialog)).toContain("/Users/story/dev/playground"),
      );
      fireEvent.keyDown(input, { key: "ArrowDown" });
      fireEvent.keyDown(input, { key: "ArrowDown" });
      fireEvent.keyDown(input, { key: "Enter", metaKey: true });
      await waitFor(async () =>
        expect(findAddProjectStoryCall("addProject")).toEqual({
          createIfMissing: false,
          machineId: "local",
          path: "/Users/story/dev",
        }),
      );
    });
  },
};

/** 4. The submit label flips to "Create & Add" for a directory that does not exist yet. */
export const SubmitLabelFlipsToCreateAndAdd: Story = {
  args: {},
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const dialog = await findDialog(storyRoot);
    await chooseSource(dialog, "local");
    const input = await findPathInput(dialog);
    const submit = await findRequiredElement(
      dialog,
      '[data-add-project-field="submit"]',
      "submit button",
    );

    await step("an exact directory name keeps the plain Add label", async () => {
      typeQuery(input, "~/dev/ghostex");
      await waitFor(async () => expect(submit.textContent).toBe("Add"));
    });

    await step("an unknown leaf segment flips the label to Create & Add", async () => {
      typeQuery(input, "~/dev/brand-new");
      await waitFor(async () => expect(submit.textContent).toBe("Create & Add"));
      /* The `..` row still lists: a group exists, so no empty state. */
      await expect(dialog.querySelector('[data-add-project-field="directoryUp"]')).not.toBeNull();
    });

    await step("with no rows at all the hint explains the create-on-Enter behavior", async () => {
      typeQuery(input, "~/brand-new");
      const emptyState = await findRequiredElement(
        dialog,
        '[data-add-project-field="emptyState"]',
        "empty state",
      );
      await waitFor(async () =>
        expect(emptyState.textContent).toBe(
          "Press Enter to create this folder and add it as a project.",
        ),
      );
    });

    await step("submitting sends createIfMissing", async () => {
      fireEvent.keyDown(input, { key: "Enter" });
      await waitFor(async () =>
        expect(findAddProjectStoryCall("addProject")).toEqual({
          createIfMissing: true,
          machineId: "local",
          path: "~/brand-new",
        }),
      );
    });
  },
};

/** 5. More than one machine: the machine step comes first and scopes every later call. */
export const MachineStepPicksRemoteMachine: Story = {
  args: {
    mockOptions: {
      machines: [ADD_PROJECT_STORY_LOCAL_MACHINE, ADD_PROJECT_STORY_REMOTE_MACHINE],
    },
  },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const dialog = await findDialog(storyRoot);

    await step("both machines are offered", async () => {
      await waitFor(async () =>
        expect(
          Array.from(dialog.querySelectorAll('[data-add-project-field="machineOption"]')).map(
            (row) => row.getAttribute("data-add-project-machine-id"),
          ),
        ).toEqual(["local", "machine-bigbox"]),
      );
    });

    await step("choosing the remote machine scopes the sources step to it", async () => {
      fireEvent.click(
        await findRequiredElement(
          dialog,
          '[data-add-project-machine-id="machine-bigbox"]',
          "remote machine row",
        ),
      );
      const machineLabel = await findRequiredElement(
        dialog,
        '[data-add-project-field="machineLabel"]',
        "machine label",
      );
      await expect(machineLabel.textContent).toContain("Bigbox");
      await waitFor(async () =>
        expect(findAddProjectStoryCall("discoverSourceControl")).toEqual({
          machineId: "machine-bigbox",
        }),
      );
    });

    await step("the remote machine browses from its own base directory", async () => {
      await chooseSource(dialog, "local");
      const input = await findPathInput(dialog);
      await expect(input.value).toBe("~/projects/");
      await waitFor(async () => expect(listedDirectories(dialog)).toContain("/srv/projects/api"));
    });
  },
};

/** 6. Provider readiness: ready providers first, unready rows carry Setup Required. */
export const SourceReadinessOrdersAndDisablesProviders: Story = {
  args: {},
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const dialog = await findDialog(storyRoot);

    await step("ready providers sort ahead of unready ones", async () => {
      await waitFor(async () =>
        expect(listedSources(dialog)).toEqual([
          "local",
          "url",
          "github",
          "azure-devops",
          "bitbucket",
          "gitlab",
        ]),
      );
    });

    await step("the ready GitHub row has no Setup Required affordance", async () => {
      const github = await findRequiredElement(
        dialog,
        '[data-add-project-source="github"]',
        "github row",
      );
      await expect(github.getAttribute("aria-disabled")).toBeNull();
      await expect(
        github.querySelector('[data-add-project-field="setupRequired"]'),
      ).toBeNull();
    });

    await step("the unready Bitbucket row is disabled and explains itself", async () => {
      const bitbucket = await findRequiredElement(
        dialog,
        '[data-add-project-field="sourceOption"][data-add-project-source="bitbucket"]',
        "bitbucket row",
      );
      await expect(bitbucket.getAttribute("aria-disabled")).toBe("true");
      await expect(bitbucket.textContent).toContain(
        "Bitbucket support needs a CLI Ghostex does not ship yet.",
      );
      fireEvent.click(bitbucket);
      await expect(dialog.querySelector('[data-add-project-field="repositoryCard"]')).toBeNull();
      await expect(listedSources(dialog).length).toBe(6);
    });

    await step("Setup Required routes to source-control settings", async () => {
      const setupRequired = await findRequiredElement(
        dialog,
        '[data-add-project-field="setupRequired"][data-add-project-source="bitbucket"]',
        "setup required button",
      );
      fireEvent.click(setupRequired);
      await waitFor(async () =>
        expect(
          storyRoot
            .querySelector("[data-add-project-story-settings-provider]")
            ?.getAttribute("data-add-project-story-settings-provider"),
        ).toBe("bitbucket"),
      );
    });
  },
};

/** 7. Git URL -> destination -> clone job -> add, the whole happy path. */
export const ClonesFromGitUrlAndAddsProject: Story = {
  args: { mockOptions: { cloneRunningPolls: 2 } },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const dialog = await findDialog(storyRoot);

    await step("the Git URL source asks for a clone URL", async () => {
      await chooseSource(dialog, "url");
      const input = await findPathInput(dialog);
      await expect(input.getAttribute("placeholder")).toBe(
        "Enter repository, URL, or clone command",
      );
      const emptyState = await findRequiredElement(
        dialog,
        '[data-add-project-field="emptyState"]',
        "empty state",
      );
      await expect(emptyState.textContent).toBe(
        "Enter a repository, URL, or clone command and press Enter to continue.",
      );
    });

    await step("Enter moves straight to the destination step", async () => {
      const input = await findPathInput(dialog);
      typeQuery(input, "https://github.com/acme/widgets.git");
      fireEvent.keyDown(input, { key: "Enter" });
      const card = await findRequiredElement(
        dialog,
        '[data-add-project-field="repositoryCard"]',
        "repository card",
      );
      await expect(card.textContent).toContain("https://github.com/acme/widgets.git");
      await waitFor(async () => expect((await findPathInput(dialog)).value).toBe("~/"));
    });

    await step("a new destination folder flips the label to Create & Clone", async () => {
      const input = await findPathInput(dialog);
      typeQuery(input, "~/dev/widgets");
      const submit = await findRequiredElement(
        dialog,
        '[data-add-project-field="submit"]',
        "submit button",
      );
      await waitFor(async () => expect(submit.textContent).toBe("Create & Clone"));
    });

    await step("Enter starts the clone job, polls it, then registers the project", async () => {
      const input = await findPathInput(dialog);
      fireEvent.keyDown(input, { key: "Enter" });
      await waitFor(async () =>
        expect(findAddProjectStoryCall("startClone")).toEqual({
          destinationPath: "~/dev/widgets",
          machineId: "local",
          remoteUrl: "https://github.com/acme/widgets.git",
        }),
      );
      await waitFor(async () =>
        expect(
          getAddProjectStoryMocks().calls.filter((call) => call.name === "readCloneJob").length,
        ).toBeGreaterThanOrEqual(3),
      );
      await waitFor(async () =>
        expect(findAddProjectStoryCall("addProject")).toEqual({
          createIfMissing: false,
          machineId: "local",
          path: "~/dev/widgets",
        }),
      );
      await waitFor(async () =>
        expect(storyRoot.querySelector("[data-add-project-modal]")).toBeNull(),
      );
    });
  },
};

/** 8. A failed provider lookup keeps the repository step and its typed value. */
export const LookupFailureStaysOnRepositoryStep: Story = {
  args: {
    mockOptions: { lookupError: "GitHub repository not found: acme/missing" },
  },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const dialog = await findDialog(storyRoot);

    await step("the GitHub source asks for owner/repo", async () => {
      /* Wait for discovery: before it answers every provider still reads as unready. */
      await waitFor(async () =>
        expect(
          dialog.querySelector(
            '[data-add-project-field="setupRequired"][data-add-project-source="github"]',
          ),
        ).toBeNull(),
      );
      await chooseSource(dialog, "github");
      const input = await findPathInput(dialog);
      await expect(input.getAttribute("placeholder")).toBe(
        "Enter GitHub repository, URL, or clone command",
      );
    });

    await step("the failure renders inline and the step keeps the typed repository", async () => {
      const input = await findPathInput(dialog);
      typeQuery(input, "acme/missing");
      fireEvent.keyDown(input, { key: "Enter" });
      const error = await findRequiredElement(
        dialog,
        '[data-add-project-field="error"]',
        "error region",
      );
      await expect(error.textContent).toContain("GitHub repository not found: acme/missing");
      await expect((await findPathInput(dialog)).value).toBe("acme/missing");
      await expect(dialog.querySelector('[data-add-project-field="repositoryCard"]')).toBeNull();
    });
  },
};

/** 9. A failed add keeps the dialog open with a persistent inline error. */
export const AddFailureShowsPersistentError: Story = {
  args: {
    mockOptions: {
      addProjectError: "Workspace root is not a directory: /Users/story/dev/ghostex",
    },
  },
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const dialog = await findDialog(storyRoot);
    await chooseSource(dialog, "local");
    const input = await findPathInput(dialog);

    await step("the add failure is rendered as an inline region, not a list line", async () => {
      typeQuery(input, "~/dev/ghostex/");
      await waitFor(async () =>
        expect(listedDirectories(dialog)).toContain("/Users/story/dev/ghostex/gpui"),
      );
      fireEvent.keyDown(input, { key: "Enter" });
      const error = await findRequiredElement(
        dialog,
        '[data-add-project-field="error"]',
        "error region",
      );
      await expect(error.textContent).toContain(
        "Workspace root is not a directory: /Users/story/dev/ghostex",
      );
    });

    await step("the error survives further navigation instead of flashing away", async () => {
      fireEvent.keyDown(input, { key: "ArrowDown" });
      fireEvent.keyDown(input, { key: "ArrowUp" });
      await expect(
        dialog.querySelector('[data-add-project-field="error"]')?.textContent,
      ).toContain("Workspace root is not a directory: /Users/story/dev/ghostex");
      await expect(storyRoot.querySelector("[data-add-project-modal]")).not.toBeNull();
    });
  },
};

/** 10. Both back paths: clearing an initialQuery view, and Backspace on an empty input. */
export const BackNavigationPopsSteps: Story = {
  args: {},
  play: async ({ canvasElement, step }) => {
    const storyRoot = canvasElement.ownerDocument.body;
    const dialog = await findDialog(storyRoot);

    await step("clearing the local-browse query pops back to the sources step", async () => {
      await chooseSource(dialog, "local");
      const input = await findPathInput(dialog);
      await expect(input.value).toBe("~/");
      typeQuery(input, "");
      await waitFor(async () => expect(listedSources(dialog)).toContain("url"));
    });

    await step("Backspace on the empty repository step pops back too", async () => {
      await chooseSource(dialog, "url");
      const input = await findPathInput(dialog);
      await expect(input.value).toBe("");
      fireEvent.keyDown(input, { key: "Backspace" });
      await waitFor(async () => expect(listedSources(dialog)).toContain("local"));
      await expect(dialog.querySelector('[data-add-project-field="back"]')).toBeNull();
    });
  },
};
