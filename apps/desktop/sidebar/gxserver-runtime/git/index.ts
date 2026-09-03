/*
CDXC:RepoStructure 2026-08-23:
Directory split of gxserver-runtime/git.ts (~3,251 lines), pure move, no
logic changes. Each sibling module contributes part of the
`gpuiSidebarRuntimeGitMethods` object; this barrel combines them with the
same object-literal shape the original file had, so `./git`'s public
surface (the `GpuiSidebarRuntimeGitMethods` type and the
`gpuiSidebarRuntimeGitMethods` value) is unchanged for `core.ts` and every
other importer. `gpuiSidebarRuntimeGitMethodsShapeCheck` below is the same
check the original file ran, moved here since this is now where the object
literal is assembled.
*/
import { gpuiSidebarRuntimeGitActionsAndConfirmMethods } from './actions-and-confirm';
import { gpuiSidebarRuntimeGitBranchOperationsMethods } from './branch-operations';
import { gpuiSidebarRuntimeGitDiffStatsMethods } from './diff-stats';
import { gpuiSidebarRuntimeGitStateAndGithubMethods } from './state-and-github';
import { gpuiSidebarRuntimeGitTypedOperationsMethods } from './typed-operations';
import type { GpuiSidebarRuntimeGitMethods } from './types';
import { gpuiSidebarRuntimeGitWorkflowAndPreferencesMethods } from './workflow-and-preferences';
import { gpuiSidebarRuntimeGitWorktreeMergeAndReviewMethods } from './worktree-merge-and-review';

export type { GpuiSidebarRuntimeGitMethods } from './types';

export const gpuiSidebarRuntimeGitMethods = {
  ...gpuiSidebarRuntimeGitDiffStatsMethods,
  ...gpuiSidebarRuntimeGitStateAndGithubMethods,
  ...gpuiSidebarRuntimeGitActionsAndConfirmMethods,
  ...gpuiSidebarRuntimeGitWorktreeMergeAndReviewMethods,
  ...gpuiSidebarRuntimeGitBranchOperationsMethods,
  ...gpuiSidebarRuntimeGitWorkflowAndPreferencesMethods,
  ...gpuiSidebarRuntimeGitTypedOperationsMethods,
};

const gpuiSidebarRuntimeGitMethodsShapeCheck: GpuiSidebarRuntimeGitMethods = gpuiSidebarRuntimeGitMethods;
void gpuiSidebarRuntimeGitMethodsShapeCheck;
