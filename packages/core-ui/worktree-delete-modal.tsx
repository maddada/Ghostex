import { useEffect, useId, useState } from "react";
import { IconCheck } from "@tabler/icons-react";
import { Button } from "@/packages/components/ui/button";
import { cn } from "@/packages/components/utils";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/packages/components/ui/dialog";
import type { SidebarTheme } from "../shared/session-grid-contract";

export type WorktreeDeleteModalDraft = {
  branch: string | null;
  canDeleteLocalBranch: boolean;
  groupId: string;
  hasChanges: boolean;
  localBranchName?: string;
  projectId: string;
  remoteBranchDisabledReason?: string;
  remoteBranchExists: boolean;
  remoteBranchName?: string;
  remoteName?: string;
  statusSummary: string;
  worktreeName: string;
};

export type WorktreeDeleteModalProps = {
  draft: WorktreeDeleteModalDraft;
  isOpen: boolean;
  onCancel: () => void;
  onCommit: (groupId: string) => void;
  onDelete: (
    projectId: string,
    options: { deleteLocalBranch: boolean; deleteRemoteBranch: boolean },
  ) => void;
  theme?: SidebarTheme;
};

export function WorktreeDeleteModal({
  draft,
  isOpen,
  onCancel,
  onCommit,
  onDelete,
  theme = "dark-1",
}: WorktreeDeleteModalProps) {
  const [deleteLocalBranch, setDeleteLocalBranch] = useState(false);
  const [deleteRemoteBranch, setDeleteRemoteBranch] = useState(false);
  const localBranchCheckboxId = useId();
  const remoteBranchCheckboxId = useId();
  const isDarkTheme = getSidebarThemeVariant(theme) === "dark";
  const localBranchName = draft.localBranchName ?? draft.branch ?? undefined;
  const remoteName = draft.remoteName ?? "origin";
  const remoteBranchLabel = draft.remoteBranchName
    ? `${remoteName}/${draft.remoteBranchName}`
    : `${remoteName} branch`;
  const selectedBranchDeletes = [
    deleteLocalBranch && draft.canDeleteLocalBranch && localBranchName
      ? `local branch ${localBranchName}`
      : "",
    deleteRemoteBranch && draft.remoteBranchExists && draft.remoteBranchName
      ? `remote branch ${remoteBranchLabel}`
      : "",
  ].filter(Boolean);

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    setDeleteLocalBranch(false);
    setDeleteRemoteBranch(false);
  }, [draft.projectId, isOpen]);

  useEffect(() => {
    if (!draft.canDeleteLocalBranch) {
      setDeleteLocalBranch(false);
    }
    if (!draft.remoteBranchExists) {
      setDeleteRemoteBranch(false);
    }
  }, [draft.canDeleteLocalBranch, draft.remoteBranchExists]);

  /*
   * CDXC:WorktreeDelete 2026-06-02-13:41:
   * Delete Worktree must be a full-window confirmation modal. Dirty worktrees
   * show the gxserver-provided Git status summary and offer Commit, which
   * switches to the existing commit review modal; clean worktrees show a green
   * checkmark instead of an empty status block.
   *
   * CDXC:WorktreeDelete 2026-06-10-22:56:
   * Branch deletion is an explicit opt-in after checkout removal. Keep local
   * and origin-branch checkboxes unchecked by default, disabled when native did
   * not verify the target branch, and include selected branch cleanup in the
   * confirmation note before the destructive action runs.
   */
  return (
    <Dialog
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          onCancel();
        }
      }}
      open={isOpen}
    >
      <DialogContent
        className={cn(
          "ghostex-settings-shadcn command-config-modal-shadcn worktree-delete-modal-shadcn flex flex-col gap-0 overflow-hidden p-0 font-sans",
          isDarkTheme && "dark",
        )}
        data-sidebar-theme={theme}
      >
        <DialogHeader className="worktree-delete-modal-header">
          <DialogTitle className="text-xl">Delete worktree</DialogTitle>
          <DialogDescription className="sr-only">
            Confirm whether to delete the selected worktree checkout.
          </DialogDescription>
        </DialogHeader>
        <div className="worktree-delete-modal-body">
          <p className="worktree-delete-modal-question">
            Delete worktree &quot;{draft.worktreeName}&quot;?
          </p>
          {draft.hasChanges ? (
            <div className="worktree-delete-status-block">
              <p className="worktree-delete-status-heading">
                This worktree has uncommitted changes:
              </p>
              <pre className="worktree-delete-status-summary">{draft.statusSummary}</pre>
            </div>
          ) : (
            <div className="worktree-delete-clean-row">
              <span className="worktree-delete-clean-icon" aria-hidden="true">
                <IconCheck size={15} stroke={2.4} />
              </span>
              <span>The worktree has no local changes</span>
            </div>
          )}
          <div className="worktree-delete-branch-options" aria-label="Branch deletion options">
            <label
              className={`worktree-delete-branch-option${
                draft.canDeleteLocalBranch ? "" : " worktree-delete-branch-option-disabled"
              }`}
              htmlFor={localBranchCheckboxId}
            >
              <input
                checked={deleteLocalBranch && draft.canDeleteLocalBranch}
                className="worktree-delete-branch-checkbox"
                disabled={!draft.canDeleteLocalBranch}
                id={localBranchCheckboxId}
                onChange={(event) => setDeleteLocalBranch(event.currentTarget.checked)}
                type="checkbox"
              />
              <span className="worktree-delete-branch-option-copy">
                <span className="worktree-delete-branch-option-label">
                  Delete local branch {localBranchName ? <code>{localBranchName}</code> : ""}
                </span>
                {!draft.canDeleteLocalBranch ? (
                  <span className="worktree-delete-branch-option-help">
                    No local branch is checked out for this worktree.
                  </span>
                ) : null}
              </span>
            </label>
            <label
              className={`worktree-delete-branch-option${
                draft.remoteBranchExists ? "" : " worktree-delete-branch-option-disabled"
              }`}
              htmlFor={remoteBranchCheckboxId}
            >
              <input
                checked={deleteRemoteBranch && draft.remoteBranchExists}
                className="worktree-delete-branch-checkbox"
                disabled={!draft.remoteBranchExists}
                id={remoteBranchCheckboxId}
                onChange={(event) => setDeleteRemoteBranch(event.currentTarget.checked)}
                type="checkbox"
              />
              <span className="worktree-delete-branch-option-copy">
                <span className="worktree-delete-branch-option-label">
                  Delete remote branch {draft.remoteBranchName ? <code>{remoteBranchLabel}</code> : ""}
                </span>
                {!draft.remoteBranchExists ? (
                  <span className="worktree-delete-branch-option-help">
                    {draft.remoteBranchDisabledReason ?? "No matching remote branch exists."}
                  </span>
                ) : null}
              </span>
            </label>
          </div>
          <p className="worktree-delete-modal-note">
            This action will remove the worktree directory
            {selectedBranchDeletes.length > 0
              ? ` and delete ${selectedBranchDeletes.join(" and ")}.`
              : "."}
          </p>
        </div>
        <DialogFooter className="worktree-delete-modal-actions">
          <Button
            className="worktree-delete-modal-button"
            onClick={onCancel}
            type="button"
            variant="outline"
          >
            Cancel
          </Button>
          {draft.hasChanges ? (
            <Button
              className="worktree-delete-modal-button"
              onClick={() => onCommit(draft.groupId)}
              type="button"
              variant="outline"
            >
              Commit
            </Button>
          ) : null}
          <Button
            className="worktree-delete-modal-button"
            onClick={() =>
              onDelete(draft.projectId, {
                deleteLocalBranch: deleteLocalBranch && draft.canDeleteLocalBranch,
                deleteRemoteBranch: deleteRemoteBranch && draft.remoteBranchExists,
              })
            }
            type="button"
            variant="destructive"
          >
            Delete Worktree
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function getSidebarThemeVariant(theme: SidebarTheme): "dark" | "light" {
  /**
   * CDXC:SidebarTheme 2026-06-15-01:43:
   * Worktree delete confirmation is part of the app-modal family, so Light
   * removes the dark class while Dark 1 and Dark 2 keep dark shadcn mode.
   */
  return theme.startsWith("light-") || theme === "plain-light" ? "light" : "dark";
}
