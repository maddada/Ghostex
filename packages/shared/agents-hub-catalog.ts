import type { AgentsHubGroup, AgentsHubTab } from "./session-grid-contract-sidebar";

const agentsHubTabs: AgentsHubTab[] = ["configs", "hooks", "mds", "skills"];

export function applySavedAgentsHubContents(
  groupsByTab: Record<AgentsHubTab, AgentsHubGroup[]>,
  savedContentsByPath: Record<string, string>,
): Record<AgentsHubTab, AgentsHubGroup[]> {
  const savedPaths = new Set(Object.keys(savedContentsByPath));
  if (savedPaths.size === 0) {
    return groupsByTab;
  }

  return agentsHubTabs.reduce(
    (nextGroupsByTab, tab) => ({
      ...nextGroupsByTab,
      [tab]: groupsByTab[tab].map((group) => ({
        ...group,
        files: group.files.map((file) =>
          savedPaths.has(file.path) ? { ...file, content: savedContentsByPath[file.path]! } : file,
        ),
      })),
    }),
    {} as Record<AgentsHubTab, AgentsHubGroup[]>,
  );
}
