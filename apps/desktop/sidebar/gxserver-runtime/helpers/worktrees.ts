/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import type {
  GpuiProjectWorktreeParentCandidate,
  GpuiWorktreeDeleteBranchMetadata,
  GpuiWorktreeMetadata,
  GpuiWorktreeModalCommand,
} from '../types-and-protocol';
import { normalizeGpuiPathForProjectComparison } from './presentation-projection';
import { booleanFromRecord, optionalStringField, stringFromRecord } from './records';
import type {
  GxserverPresentationSnapshot,
  GxserverProjectDomainState,
  GxserverProjectWorktreeListResult,
  GxserverTypedOperationResult,
} from '@/packages/shared/gxserver-protocol';
import type { SidebarProjectWorktreeMetadata } from '@/packages/shared/session-grid-contract';

export function normalizeGpuiWorktreeDeleteBranchName(
  currentBranch: string | null | undefined,
  fallbackBranch: string | null | undefined
): string | undefined {
  for (const candidate of [currentBranch, fallbackBranch]) {
    const branch = candidate?.trim();
    if (branch && branch !== 'HEAD' && branch !== 'detached') {
      return branch;
    }
  }
  return undefined;
}

export async function resolveGpuiWorktreeDeleteBranchMetadata(
  branchName: string | undefined,
  checkRemoteBranch: (remoteName: string, remoteBranchName: string) => Promise<GxserverTypedOperationResult>
): Promise<GpuiWorktreeDeleteBranchMetadata> {
  const remoteName = 'origin';
  if (!branchName) {
    return {
      branch: null,
      canDeleteLocalBranch: false,
      remoteBranchDisabledReason: 'No local branch is checked out for this worktree.',
      remoteBranchExists: false,
      remoteName,
    };
  }
  const remoteBranch = await checkRemoteBranch(remoteName, branchName);
  const remoteBranchExists = remoteBranch.exitCode === 0;
  return {
    branch: branchName,
    canDeleteLocalBranch: true,
    localBranchName: branchName,
    remoteBranchDisabledReason: remoteBranchExists ? undefined : `No ${remoteName}/${branchName} remote branch exists.`,
    remoteBranchExists,
    remoteBranchName: branchName,
    remoteName,
  };
}

/**
 * The field's prefill: the folder's own name with the `<ParentFolder>-` prefix
 * stripped, because that prefix is re-applied on submit and showing it would
 * invite the user to type it twice.
 */
export function gpuiWorktreeFolderSuffix(folderName: string, parentFolderName: string): string {
  const prefix = `${parentFolderName}-`;
  return parentFolderName && folderName.startsWith(prefix) ? folderName.slice(prefix.length) : folderName;
}

/*
CDXC:WorktreeRename 2026-08-09-18:40:
The branch checkbox defaults on only for a branch gxserver minted or manages —
`ghostex/<8hex>` or `ghostex/<slug>`, mirroring `is_worktree_temp_branch` and
`is_managed_worktree_branch` in `server/src/worktree_sessions.rs`. A branch
the user named is theirs and stays put unless they say otherwise; a branch
Ghostex named is Ghostex's to keep in step with the folder. Getting this backwards
would silently rename branches people had pushed.
*/
export function isGpuiManagedWorktreeBranch(branch: string | undefined): boolean {
  const slug = branch?.startsWith('ghostex/') ? branch.slice('ghostex/'.length) : undefined;
  return Boolean(slug && slug !== 'automation' && !slug.startsWith('automation/') && /^[a-z0-9-]+$/.test(slug));
}

/*
CDXC:WorktreeRename 2026-08-09-18:40:
`gpuiWorktreeUserVisibleErrorMessage` drops any message containing `/`, which
would swallow every rename refusal that names a branch — `Branch "feat/x" already
exists.` is exactly the sentence the user needs. gxserver's rename errors are
bounded strings by contract (never git stderr), so this keeps the slash and
guards the shape instead: single line, no backslashes, bounded length.
*/
export function gpuiWorktreeRenameUserVisibleErrorMessage(error: unknown): string {
  const message = error instanceof Error ? error.message.trim() : '';
  /*
  CDXC:WorktreeRename 2026-08-09-18:40:
  A daemon older than this feature cannot route the rename endpoint at all, and
  says so by naming the path back at the user — verified live as
  `notFound: "No gxserver endpoint for POST /api/renameWorktreeProject."`, though
  gxserver has more than one phrasing for it. Match on the endpoint path instead
  of on any one sentence: an error that names this route is always the daemon
  being older than the app, never anything the user did to their worktree.

  This is not hypothetical. A freshly built app attaches to whatever gxserver is
  already listening on 127.0.0.1:58744, which is normally the daemon the
  installed app started — so the very first run of a new build hits it.
  */
  if (message.includes('/api/renameWorktreeProject')) {
    return "This Ghostex build's background service is out of date. Quit Ghostex fully, reopen it, and try again.";
  }
  if (message && !message.includes('\\') && !message.includes('\n') && message.length <= 200) {
    return message;
  }
  return 'The gxserver worktree rename failed.';
}

export function parseGpuiWorktreeModalCommand(payload: unknown): GpuiWorktreeModalCommand | undefined {
  // Worktree modal commands arrive from the native app-modal host bridge.
  // Rebuild them field-by-field with bounded strings so only the shared modal
  // contract enters the runtime's worktree/git handlers, which then
  // revalidate all project and worktree identity against gxserver state.
  if (!payload || typeof payload !== 'object') {
    return undefined;
  }
  const record = payload as Record<string, unknown>;
  const stringField = (field: string, maxChars: number): string | undefined => {
    const value = record[field];
    return typeof value === 'string' && value.length > 0 && value.length <= maxChars ? value : undefined;
  };
  switch (record.type) {
    case 'requestProjectWorktrees': {
      const requestId = stringField('requestId', 120);
      if (!requestId) {
        return undefined;
      }
      return {
        projectId: stringField('projectId', 300),
        projectPath: stringField('projectPath', 1024),
        remoteMachineId: stringField('remoteMachineId', 300),
        requestId,
        type: 'requestProjectWorktrees',
      };
    }
    case 'createProjectWorktree':
      return {
        agentId: stringField('agentId', 300),
        baseBranch: stringField('baseBranch', 300),
        existingWorktreeKey: stringField('existingWorktreeKey', 600),
        existingWorktreePath: stringField('existingWorktreePath', 1024),
        mode: record.mode === 'openExisting' || record.mode === 'create' ? record.mode : undefined,
        projectId: stringField('projectId', 300),
        projectPath: stringField('projectPath', 1024),
        prompt: stringField('prompt', 20_000),
        remoteMachineId: stringField('remoteMachineId', 300),
        type: 'createProjectWorktree',
      };
    case 'confirmDeleteWorktree': {
      const projectId = stringField('projectId', 300);
      if (!projectId) {
        return undefined;
      }
      return {
        deleteLocalBranch: record.deleteLocalBranch === true,
        deleteRemoteBranch: record.deleteRemoteBranch === true,
        projectId,
        type: 'confirmDeleteWorktree',
      };
    }
    case 'confirmRenameWorktree': {
      /*
      CDXC:WorktreeRename 2026-08-09-18:40:
      `name` crosses this boundary as bounded text, never as a path: gxserver
      derives the destination folder from it and re-validates it against the
      daemon's own ref policy, so nothing here can name a directory. The 200-char
      cap matches the shared validator's.
      */
      const projectId = stringField('projectId', 300);
      const name = stringField('name', 200);
      if (!projectId || !name) {
        return undefined;
      }
      return {
        name,
        projectId,
        renameBranch: record.renameBranch === true,
        type: 'confirmRenameWorktree',
      };
    }
    case 'commitWorktreeBeforeDelete': {
      const groupId = stringField('groupId', 300);
      if (!groupId) {
        return undefined;
      }
      return { groupId, type: 'commitWorktreeBeforeDelete' };
    }
    default:
      return undefined;
  }
}

export function createGpuiProjectWorktreeParentCandidates({
  domainProjects,
  presentation,
}: {
  domainProjects: readonly GxserverProjectDomainState[];
  presentation: GxserverPresentationSnapshot;
}): GpuiProjectWorktreeParentCandidate[] {
  return [
    ...presentation.projects.map((project) => ({
      name: project.title,
      path: project.path,
      projectId: project.projectId,
      worktree: project.worktree,
    })),
    ...domainProjects.map((project) => ({
      name: project.name,
      path: project.path,
      projectId: project.projectId,
      worktree: project.worktree,
    })),
  ];
}

export function resolveGpuiProjectWorktreeParentMetadata(
  worktree: SidebarProjectWorktreeMetadata | undefined,
  candidates: readonly GpuiProjectWorktreeParentCandidate[]
): SidebarProjectWorktreeMetadata | undefined {
  if (!worktree) {
    return undefined;
  }
  const parentPath = normalizeGpuiPathForProjectComparison(worktree.parentProjectPath);
  const canonicalParent = candidates.find((candidate) => {
    if (candidate.projectId === worktree.parentProjectId || !candidate.path) {
      return false;
    }
    if (normalizeGpuiPathForProjectComparison(candidate.path) !== parentPath) {
      return false;
    }
    return !normalizeGpuiWorktreeParentProjectId(candidate.worktree);
  });
  if (!canonicalParent) {
    return worktree;
  }
  const canonicalParentPath = canonicalParent.path?.trim();
  return {
    ...worktree,
    parentProjectId: canonicalParent.projectId,
    parentProjectName: canonicalParent.name?.trim() || worktree.parentProjectName,
    parentProjectPath: canonicalParentPath || worktree.parentProjectPath,
  };
}

export function normalizeGpuiWorktreeParentProjectId(
  worktree: Record<string, unknown> | undefined
): string | undefined {
  return stringFromRecord(worktree, 'parentProjectId');
}

export function normalizeGpuiSidebarWorktreeMetadata(
  worktree: Record<string, unknown> | undefined
): SidebarProjectWorktreeMetadata | undefined {
  const branch = stringFromRecord(worktree, 'branch');
  const name = stringFromRecord(worktree, 'name');
  const parentProjectId = normalizeGpuiWorktreeParentProjectId(worktree);
  const parentProjectName = stringFromRecord(worktree, 'parentProjectName');
  const parentProjectPath = stringFromRecord(worktree, 'parentProjectPath');
  if (!branch || !name || !parentProjectId || !parentProjectName || !parentProjectPath) {
    return undefined;
  }
  const createdAt = stringFromRecord(worktree, 'createdAt');
  return {
    branch,
    ...(createdAt && !Number.isNaN(Date.parse(createdAt)) ? { createdAt } : {}),
    name,
    parentProjectId,
    parentProjectName,
    parentProjectPath,
  };
}

export function normalizeGpuiWorktreeMetadata(
  worktree: Record<string, unknown> | undefined
): GpuiWorktreeMetadata | undefined {
  const parentProjectId = normalizeGpuiWorktreeParentProjectId(worktree);
  if (!parentProjectId) {
    return undefined;
  }
  return {
    ...optionalStringField('branch', stringFromRecord(worktree, 'branch')),
    ...optionalStringField('name', stringFromRecord(worktree, 'name')),
    ...optionalStringField('parentProjectName', stringFromRecord(worktree, 'parentProjectName')),
    parentProjectId,
  };
}

export function normalizeGpuiProjectPath(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim().length > 0 ? value.trim().replace(/\/+$/u, '') : undefined;
}

export function normalizeGpuiWorktreeBaseBranches(
  branches: GxserverTypedOperationResult['branches']
): Array<{ current: boolean; name: string; remote: boolean }> {
  const seenBranches = new Set<string>();
  return (branches ?? []).flatMap((branch) => {
    const name = branch.name?.trim();
    if (!name || seenBranches.has(name)) {
      return [];
    }
    seenBranches.add(name);
    return [
      {
        current: branch.current === true,
        name,
        remote: branch.remote === true,
      },
    ];
  });
}

export function normalizeGpuiExistingWorktreeOptions(
  worktrees: GxserverProjectWorktreeListResult['worktrees'] | unknown
): Array<{
  branch: string;
  isCurrentProject: boolean;
  isRegistered: boolean;
  name: string;
  path: string;
  worktreeKey: string;
}> {
  if (!Array.isArray(worktrees)) {
    return [];
  }
  return worktrees.flatMap((entry) => {
    if (!entry || typeof entry !== 'object') {
      return [];
    }
    const worktree = entry as Record<string, unknown>;
    const path = normalizeGpuiProjectPath(worktree.path);
    const name = stringFromRecord(worktree, 'name') ?? (path ? gpuiProjectNameFromPath(path) : undefined);
    const worktreeKey = stringFromRecord(worktree, 'worktreeKey');
    if (!path || !name || !worktreeKey) {
      return [];
    }
    return [
      {
        branch: stringFromRecord(worktree, 'branch') ?? '',
        isCurrentProject: booleanFromRecord(worktree, 'isCurrentProject') === true,
        isRegistered: booleanFromRecord(worktree, 'isRegistered') === true,
        name,
        path,
        worktreeKey,
      },
    ];
  });
}

export function createGpuiExistingWorktreeOptions(
  worktrees: GxserverTypedOperationResult['worktrees'],
  parentProject: GxserverProjectDomainState,
  sourceProject: GxserverProjectDomainState,
  domainProjects: readonly GxserverProjectDomainState[]
): Array<{
  branch: string;
  isCurrentProject: boolean;
  isRegistered: boolean;
  name: string;
  path: string;
}> {
  const entries = worktrees ?? [];
  const mainEntry = entries.find((entry) => entry.bare !== true);
  const mainPath = normalizeGpuiProjectPath(mainEntry?.path) ?? normalizeGpuiProjectPath(parentProject.path);
  const sourcePath = normalizeGpuiProjectPath(sourceProject.path);
  const registeredPaths = new Set(
    domainProjects
      .map((project) => normalizeGpuiProjectPath(project.path))
      .filter((path): path is string => Boolean(path))
  );
  return entries.flatMap((entry) => {
    if (entry.bare === true) {
      return [];
    }
    const path = normalizeGpuiProjectPath(entry.path);
    if (!path || path === mainPath) {
      return [];
    }
    return [
      {
        branch: entry.branch?.trim() ?? '',
        isCurrentProject: path === sourcePath,
        isRegistered: registeredPaths.has(path),
        name: gpuiProjectNameFromPath(path),
        path,
      },
    ];
  });
}

export function gpuiProjectNameFromPath(path: string): string {
  return path.split('/').filter(Boolean).at(-1) ?? 'Project';
}

export function gpuiDirname(path: string): string {
  const parts = path.replace(/\/+$/u, '').split('/').filter(Boolean);
  if (parts.length <= 1) {
    return '/';
  }
  return `/${parts.slice(0, -1).join('/')}`;
}

export function gpuiWorktreeSlugFromPrompt(prompt: string): string {
  const firstWords = prompt
    .trim()
    .toLowerCase()
    .replace(/[`'"]/gu, '')
    .replace(/[^a-z0-9]+/gu, '-')
    .replace(/^-+|-+$/gu, '')
    .split('-')
    .filter(Boolean)
    .slice(0, 6)
    .join('-');
  return (firstWords || 'worktree').slice(0, 48).replace(/-+$/u, '') || 'worktree';
}

export function createGpuiWorktreeToastId(): string {
  return `toast-gpui-worktree-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

export function gpuiWorktreeUserVisibleErrorMessage(error: unknown): string {
  const message = error instanceof Error ? error.message.trim() : '';
  if (
    message &&
    !message.includes('/') &&
    !message.includes('\\') &&
    !message.includes('\n') &&
    message.length <= 160
  ) {
    return message;
  }
  return 'The gxserver worktree operation failed.';
}
