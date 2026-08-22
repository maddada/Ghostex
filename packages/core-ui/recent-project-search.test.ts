import { describe, expect, test } from "vitest";
import { groupRecentProjectsByMachine } from "./recent-project-search";
import type { SidebarRecentProject } from "../shared/session-grid-contract";

const PROJECTS: SidebarRecentProject[] = [
  {
    path: "/Users/story/dev/agent-manager-x",
    projectId: "agent-manager-x",
    sessionCount: 2,
    title: "agent-manager-x",
  },
  {
    path: "/Users/story/dev/open-design",
    projectId: "open-design",
    sessionCount: 0,
    title: "open-design",
  },
  {
    path: "/home/story/dev/remote-control",
    projectId: "remote:main-machine:project:remote-control",
    remoteMachineId: "main-machine",
    remoteMachineName: "Raspberry Pi",
    sessionCount: 1,
    title: "remote-control",
  },
];

describe("groupRecentProjectsByMachine", () => {
  test("keeps local and remote recency order within their machine sections", () => {
    const grouped = groupRecentProjectsByMachine(PROJECTS);
    expect(grouped.local.map((project) => project.projectId)).toEqual([
      "agent-manager-x",
      "open-design",
    ]);
    expect(
      grouped.remoteByMachineId.get("main-machine")?.map((project) => project.projectId),
    ).toEqual([
      "remote:main-machine:project:remote-control",
    ]);
  });
});
