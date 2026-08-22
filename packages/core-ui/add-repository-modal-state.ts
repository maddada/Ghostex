import type { GxserverRepositoryClonePreviewResult } from "../shared/gxserver-protocol";
import { isRepositoryCloneBranchNameInputValid, parseRepositoryCloneInput } from "../shared/repository-clone";

export type AddRepositoryCloneSubmitState = {
  branchName: string;
  clonePreview?: Pick<GxserverRepositoryClonePreviewResult, "destinationExists">;
  folderPath: string;
  isCloning: boolean;
  newFolderName: string;
  previewErrorMessage?: string;
  repositoryInput: string;
};

/*
CDXC:AddRepository 2026-06-06-06:38:
Clone & Add must enable as soon as the typed repository, parent folder, and new folder are locally valid. The gxserver preview remains the authority for existing-destination warnings, but waiting for an async preview before enabling the button makes correct input look broken.

CDXC:AddRepository 2026-06-07-16:01:
The new-folder field is optional. Empty input means gxserver should use the parsed repository name as the destination folder, while custom text still overrides that default.

CDXC:AddRepository 2026-06-07-16:06:
The branch field is optional. Empty branch input leaves Git on the repository default branch, while valid typed branch names are passed through as the checkout branch.
*/
export function canSubmitAddRepositoryClone(state: AddRepositoryCloneSubmitState): boolean {
  if (state.isCloning || state.previewErrorMessage || state.clonePreview?.destinationExists) {
    return false;
  }
  return (
    parseRepositoryCloneInput(state.repositoryInput.trim()) !== undefined &&
    isRepositoryCloneBranchNameInputValid(state.branchName) &&
    state.folderPath.trim().length > 0
  );
}
