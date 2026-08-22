import { describe, expect, test } from "vitest";
import type { AgentsHubCatalogMessage } from "../shared/session-grid-contract";
import { applySavedAgentsHubContents } from "../shared/agents-hub-catalog";

const catalog: AgentsHubCatalogMessage = {
  generatedAt: "2026-05-16T03:19:00.000Z",
  groupsByTab: {
    configs: [],
    hooks: [],
    mds: [
      {
        description: "Shared agent instructions.",
        files: [
          {
            content: "old instructions",
            id: "shared-main",
            language: "markdown",
            name: "main.md",
            path: "/Users/madda/.agents/main.md",
          },
        ],
        id: "shared-agents",
        name: "Shared agents",
        path: "/Users/madda/.agents",
        profiles: [],
      },
    ],
    skills: [
      {
        description: "Skill instructions.",
        files: [
          {
            content: "old skill",
            id: "skill-main",
            language: "markdown",
            name: "SKILL.md",
            path: "/Users/madda/agents/skills/example/SKILL.md",
          },
          {
            content: "old instructions",
            id: "linked-shared-main",
            language: "markdown",
            name: "main.md",
            path: "/Users/madda/.agents/main.md",
          },
        ],
        id: "example-skill",
        name: "example",
        path: "/Users/madda/agents/skills/example",
        profiles: [],
      },
    ],
  },
  type: "agentsHubCatalog",
};

describe("applySavedAgentsHubContents", () => {
  test("keeps the native catalog object when no saved overlay exists", () => {
    expect(applySavedAgentsHubContents(catalog.groupsByTab, {})).toBe(catalog.groupsByTab);
  });

  test("applies saved editor contents to every matching file path", () => {
    /**
     * CDXC:AgentsHub 2026-05-16-07:19:
     * Returning to a saved Hub file should show the just-persisted editor buffer even when multiple catalog entries point at the same path.
     * The overlay helper updates every matching file path so profile-linked files cannot display the stale native scan content after a Save and file reselect.
     */
    const updated = applySavedAgentsHubContents(catalog.groupsByTab, {
      "/Users/madda/.agents/main.md": "new instructions",
    });

    expect(updated.mds[0]!.files[0]!.content).toBe("new instructions");
    expect(updated.skills[0]!.files[1]!.content).toBe("new instructions");
    expect(updated.skills[0]!.files[0]!.content).toBe("old skill");
    expect(catalog.groupsByTab.mds[0]!.files[0]!.content).toBe("old instructions");
  });

  test("adds saved editor contents to metadata-only catalog rows", () => {
    /*
     * CDXC:AgentsHub 2026-06-12-02:53:
     * Native catalog rows no longer include full file buffers on open. A saved
     * in-modal edit should still hydrate matching metadata-only rows so
     * reselecting the file shows the persisted editor text immediately.
     */
    const metadataOnlyCatalog: AgentsHubCatalogMessage = {
      ...catalog,
      groupsByTab: {
        configs: [],
        hooks: [],
        mds: [
          {
            ...catalog.groupsByTab.mds[0]!,
            files: [
              {
                id: "shared-main",
                language: "markdown",
                name: "main.md",
                path: "/Users/madda/.agents/main.md",
              },
            ],
          },
        ],
        skills: [],
      },
    };

    const updated = applySavedAgentsHubContents(metadataOnlyCatalog.groupsByTab, {
      "/Users/madda/.agents/main.md": "new instructions",
    });

    expect(updated.mds[0]!.files[0]!.content).toBe("new instructions");
    expect(metadataOnlyCatalog.groupsByTab.mds[0]!.files[0]!.content).toBeUndefined();
  });
});
