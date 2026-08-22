import type { SidebarProjectReference } from "./sidebar-ids";

const listeners = new Set<() => void>();
let activeProject: SidebarProjectReference | undefined;

export function getActiveSidebarProject(): SidebarProjectReference | undefined {
  return activeProject;
}

export function subscribeActiveSidebarProject(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function setActiveSidebarProject(project: SidebarProjectReference | undefined): void {
  if (
    activeProject?.machineId === project?.machineId
    && activeProject?.projectId === project?.projectId
  ) {
    return;
  }
  activeProject = project;
  for (const listener of listeners) {
    listener();
  }
}
