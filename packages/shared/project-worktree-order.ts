export type ProjectWorktreeOrderItem = {
  isChat?: boolean;
  isQuick?: boolean;
  orderId?: string;
  projectId: string;
  worktree?: {
    parentProjectId?: string;
  };
};

export type ProjectWorktreeDropTarget = {
  orderId: string;
  position: "after" | "before";
};

/*
 * CDXC:WorktreeProjectOrder 2026-05-25-12:38:
 * Worktree projects are children of their main project in the sidebar. A main
 * project drag carries its worktrees underneath it in their existing order, and
 * a worktree can only be reordered inside that same main-project family.
 */
export function orderProjectsWithWorktrees<T extends ProjectWorktreeOrderItem>(
  projects: readonly T[],
): T[] {
  const chatProjects = projects.filter((project) => project.isChat === true || project.isQuick === true);
  const codeProjects = projects.filter((project) => project.isChat !== true && project.isQuick !== true);
  return [...chatProjects, ...orderCodeProjectsWithWorktrees(codeProjects)];
}

export function moveProjectsWithWorktrees<T extends ProjectWorktreeOrderItem>(
  projects: readonly T[],
  sourceOrderId: string,
  target: ProjectWorktreeDropTarget,
): T[] {
  if (!canDropProjectWithWorktrees(projects, sourceOrderId, target)) {
    return [...projects];
  }

  const sourceIndex = projects.findIndex((project) => getProjectOrderId(project) === sourceOrderId);
  const targetIndex = projects.findIndex((project) => getProjectOrderId(project) === target.orderId);
  if (sourceIndex < 0 || targetIndex < 0 || sourceIndex === targetIndex) {
    return [...projects];
  }

  const insertIndex = targetIndex + (target.position === "after" ? 1 : 0);
  const adjustedInsertIndex = insertIndex > sourceIndex ? insertIndex - 1 : insertIndex;
  const nextProjects = projects.filter((project) => getProjectOrderId(project) !== sourceOrderId);
  nextProjects.splice(
    clampProjectIndex(adjustedInsertIndex, nextProjects.length),
    0,
    projects[sourceIndex]!,
  );
  return orderProjectsWithWorktrees(nextProjects);
}

export function canDropProjectWithWorktrees<T extends ProjectWorktreeOrderItem>(
  projects: readonly T[],
  sourceOrderId: string,
  target: ProjectWorktreeDropTarget,
): boolean {
  const projectsByOrderId = createProjectOrderIdMap(projects);
  const sourceProject = projectsByOrderId.get(sourceOrderId);
  const targetProject = projectsByOrderId.get(target.orderId);
  if (!sourceProject || !targetProject || sourceOrderId === target.orderId) {
    return false;
  }

  const projectsByProjectId = createProjectIdMap(projects);
  const sourceFamilyParentId = resolveProjectWorktreeFamilyParentId(
    sourceProject.projectId,
    projectsByProjectId,
  );
  const targetFamilyParentId = resolveProjectWorktreeFamilyParentId(
    targetProject.projectId,
    projectsByProjectId,
  );

  if (!sourceFamilyParentId) {
    return targetFamilyParentId !== sourceProject.projectId;
  }

  if (targetProject.projectId === sourceFamilyParentId) {
    return target.position === "after";
  }

  return targetFamilyParentId === sourceFamilyParentId;
}

export function getProjectOrderId(project: ProjectWorktreeOrderItem): string {
  return project.orderId ?? project.projectId;
}

function orderCodeProjectsWithWorktrees<T extends ProjectWorktreeOrderItem>(
  projects: readonly T[],
): T[] {
  const projectsByProjectId = createProjectIdMap(projects);
  const worktreeProjectIds = new Set<string>();
  const worktreesByParentProjectId = new Map<string, T[]>();

  for (const project of projects) {
    const familyParentId = resolveProjectWorktreeFamilyParentId(
      project.projectId,
      projectsByProjectId,
    );
    if (!familyParentId || !projectsByProjectId.has(familyParentId)) {
      continue;
    }

    worktreeProjectIds.add(project.projectId);
    const worktrees = worktreesByParentProjectId.get(familyParentId) ?? [];
    worktrees.push(project);
    worktreesByParentProjectId.set(familyParentId, worktrees);
  }

  const orderedProjects: T[] = [];
  for (const project of projects) {
    if (worktreeProjectIds.has(project.projectId)) {
      continue;
    }

    orderedProjects.push(project);
    if (!project.worktree?.parentProjectId?.trim()) {
      orderedProjects.push(...(worktreesByParentProjectId.get(project.projectId) ?? []));
    }
  }

  return orderedProjects;
}

function resolveProjectWorktreeFamilyParentId<T extends ProjectWorktreeOrderItem>(
  projectId: string,
  projectsByProjectId: ReadonlyMap<string, T>,
): string | undefined {
  const directParentId = projectsByProjectId.get(projectId)?.worktree?.parentProjectId?.trim();
  if (!directParentId) {
    return undefined;
  }

  let familyParentId = directParentId;
  const seenProjectIds = new Set([projectId]);
  while (!seenProjectIds.has(familyParentId)) {
    seenProjectIds.add(familyParentId);
    const parentProject = projectsByProjectId.get(familyParentId);
    const nextParentId = parentProject?.worktree?.parentProjectId?.trim();
    if (!nextParentId) {
      return familyParentId;
    }
    familyParentId = nextParentId;
  }

  return directParentId;
}

function createProjectIdMap<T extends ProjectWorktreeOrderItem>(
  projects: readonly T[],
): Map<string, T> {
  return new Map(projects.map((project) => [project.projectId, project]));
}

function createProjectOrderIdMap<T extends ProjectWorktreeOrderItem>(
  projects: readonly T[],
): Map<string, T> {
  return new Map(projects.map((project) => [getProjectOrderId(project), project]));
}

function clampProjectIndex(index: number, max: number): number {
  return Math.max(0, Math.min(index, max));
}
