import type { SidebarRecentProject } from "../shared/session-grid-contract";

export type SidebarRecentProjectsByMachine = {
  local: SidebarRecentProject[];
  remoteByMachineId: ReadonlyMap<string, SidebarRecentProject[]>;
};

export function groupRecentProjectsByMachine(
  projects: readonly SidebarRecentProject[],
): SidebarRecentProjectsByMachine {
  const local: SidebarRecentProject[] = [];
  const remoteByMachineId = new Map<string, SidebarRecentProject[]>();
  for (const project of projects) {
    if (!project.remoteMachineId) {
      local.push(project);
      continue;
    }
    const machineProjects = remoteByMachineId.get(project.remoteMachineId) ?? [];
    machineProjects.push(project);
    remoteByMachineId.set(project.remoteMachineId, machineProjects);
  }
  return { local, remoteByMachineId };
}
