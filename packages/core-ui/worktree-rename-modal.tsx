import { useEffect, useId, useMemo, useRef, useState, type FormEvent } from "react";
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
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/packages/components/ui/field";
import { Input } from "@/packages/components/ui/input";
import {
  normalizeWorktreeRenameName,
  worktreeRenameFolderSlug,
  worktreeRenameNameError,
} from "../shared/worktree-rename-name";
import type { SidebarTheme } from "../shared/session-grid-contract";

export type WorktreeRenameModalDraft = {
  /**
   * A reason the rename cannot run at all, resolved before the modal opened
   * (populated submodules, a locked checkout). Rename stays disabled while it
   * is present — the field is still editable so the user can read the preview.
   */
  blockingReason?: string;
  branch?: string;
  /** The current folder's suffix, i.e. the field's prefill. */
  currentName: string;
  currentPath: string;
  parentFolderName: string;
  parentProjectPath: string;
  projectId: string;
  /**
   * Absolute paths of every OTHER registered project, so a destination that is
   * already someone else's project row is refused while the user types instead
   * of after they submit.
   */
  registeredProjectPaths?: readonly string[];
  renameBranchDefault: boolean;
  warnings?: readonly string[];
  worktreeName: string;
};

export type WorktreeRenameModalProps = {
  draft: WorktreeRenameModalDraft;
  isOpen: boolean;
  onCancel: () => void;
  onRename: (projectId: string, options: { name: string; renameBranch: boolean }) => void;
  theme?: SidebarTheme;
};

/*
 * CDXC:WorktreeRename 2026-08-09-18:40:
 * One field, three effects: the folder becomes `<ParentFolder>-<slug>`, the
 * project label follows it, and the branch takes the typed name verbatim when
 * the checkbox is on. The live preview exists because those three are not the
 * same string — the branch keeps `feat/kanban-assignee` while the folder gets
 * `feat-kanban-assignee` — and a user who cannot see that mapping cannot predict
 * what they are about to do to their filesystem.
 *
 * CDXC:WorktreeRename 2026-08-09-18:40:
 * The branch checkbox is opt-in rather than automatic because renaming a branch
 * that is already pushed silently breaks the user's next `git push` with an
 * error that never mentions Ghostex. It defaults on only for branches gxserver
 * minted itself: a branch the user named is theirs, a branch Ghostex named is
 * Ghostex's to keep in step with the folder.
 */
export function WorktreeRenameModal({
  draft,
  isOpen,
  onCancel,
  onRename,
  theme = "dark-1",
}: WorktreeRenameModalProps) {
  const initialName = resolveWorktreeRenameInitialName(draft);
  const [name, setName] = useState(initialName);
  const [renameBranch, setRenameBranch] = useState(draft.renameBranchDefault);
  const nameInputId = useId();
  const branchCheckboxId = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  const isDarkTheme = getSidebarThemeVariant(theme) === "dark";

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    setName(initialName);
    setRenameBranch(draft.renameBranchDefault);
  }, [draft.projectId, draft.renameBranchDefault, initialName, isOpen]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    const focusInput = () => {
      const input = inputRef.current;
      if (!input) {
        return;
      }
      input.focus({ preventScroll: true });
      input.setSelectionRange(0, input.value.length);
    };
    const animationFrame = window.requestAnimationFrame(focusInput);
    return () => window.cancelAnimationFrame(animationFrame);
  }, [isOpen]);

  const trimmedName = normalizeWorktreeRenameName(name);
  const folderSlug = worktreeRenameFolderSlug(name);
  const nextFolderName = folderSlug ? `${draft.parentFolderName}-${folderSlug}` : "";
  const nextFolderPath = useMemo(
    () => (nextFolderName ? joinRenameParentDirectory(draft.parentProjectPath, nextFolderName) : ""),
    [draft.parentProjectPath, nextFolderName],
  );

  const validationError = worktreeRenameNameError(name);
  const unchanged = trimmedName === initialName;
  const collisionError = resolveWorktreeRenameCollisionError({
    draft,
    nextFolderName,
    nextFolderPath,
    unchanged,
  });
  const submitError =
    validationError ??
    (unchanged && !renameBranch ? "Nothing to rename." : undefined) ??
    collisionError;
  const canSubmit = !submitError && !draft.blockingReason;
  const warnings = draft.warnings ?? [];

  const submitRename = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!canSubmit) {
      return;
    }
    onRename(draft.projectId, { name: trimmedName, renameBranch });
  };

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
          "ghostex-settings-shadcn command-config-modal-shadcn worktree-rename-modal-shadcn flex flex-col gap-0 overflow-hidden p-0 font-sans",
          isDarkTheme && "dark",
        )}
        data-sidebar-theme={theme}
      >
        <form className="worktree-rename-modal-form" onSubmit={submitRename}>
          <DialogHeader className="worktree-rename-modal-header">
            <DialogTitle className="text-xl">Rename Worktree</DialogTitle>
            <DialogDescription className="worktree-rename-modal-subject">
              {draft.worktreeName}
              {draft.branch ? ` · ${draft.branch}` : ""}
            </DialogDescription>
          </DialogHeader>
          <div className="worktree-rename-modal-body">
            <p className="worktree-rename-modal-path">{draft.currentPath}</p>
            <FieldGroup className="worktree-rename-field-group">
              <Field>
                <FieldLabel htmlFor={nameInputId}>Name</FieldLabel>
                <Input
                  autoComplete="off"
                  className="worktree-rename-name-input"
                  id={nameInputId}
                  onChange={(event) => setName(event.currentTarget.value)}
                  ref={inputRef}
                  spellCheck={false}
                  value={name}
                />
                <FieldDescription className="worktree-rename-preview">
                  <span className="worktree-rename-preview-line">
                    Folder: <code>{nextFolderName || "—"}</code>
                  </span>
                  {renameBranch ? (
                    <span className="worktree-rename-preview-line">
                      Branch: <code>{trimmedName || "—"}</code>
                    </span>
                  ) : null}
                </FieldDescription>
              </Field>
            </FieldGroup>
            <label className="worktree-rename-branch-option" htmlFor={branchCheckboxId}>
              <input
                checked={renameBranch}
                className="worktree-rename-branch-checkbox"
                id={branchCheckboxId}
                onChange={(event) => setRenameBranch(event.currentTarget.checked)}
                type="checkbox"
              />
              <span className="worktree-rename-branch-option-copy">
                <span className="worktree-rename-branch-option-label">
                  Also rename the git branch
                </span>
                <span className="worktree-rename-branch-option-help">
                  The branch takes the typed name exactly, without the folder&apos;s slug.
                </span>
              </span>
            </label>
            {draft.blockingReason ? (
              <p className="worktree-rename-message worktree-rename-message-blocking">
                {draft.blockingReason}
              </p>
            ) : null}
            {submitError && !draft.blockingReason ? (
              <p className="worktree-rename-message worktree-rename-message-blocking">
                {submitError}
              </p>
            ) : null}
            {warnings.map((warning) => (
              <p className="worktree-rename-message" key={warning}>
                {warning}
              </p>
            ))}
          </div>
          <DialogFooter className="worktree-rename-modal-actions">
            <Button
              className="worktree-rename-modal-button"
              onClick={onCancel}
              type="button"
              variant="outline"
            >
              Cancel
            </Button>
            <Button className="worktree-rename-modal-button" disabled={!canSubmit} type="submit">
              Rename
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

/*
 * CDXC:WorktreeRename 2026-08-10:
 * Reopening the dialog must be lossless. The folder can only ever hold the
 * slugged name, so a worktree on `feat/kanban-assignee` lives in
 * `<Parent>-feat-kanban-assignee` — and prefilling from the FOLDER handed back
 * `feat-kanban-assignee`. Pressing Rename without touching anything then
 * "renamed" the branch from `feat/kanban-assignee` to `feat-kanban-assignee`
 * and silently ate the slash. Caught in manual testing, reflog and all.
 *
 * So when the current branch slugs down to exactly the current folder suffix,
 * the branch IS the name the user typed last time, with the detail the folder
 * had to drop. Prefill that instead, and reopening becomes a no-op.
 */
function resolveWorktreeRenameInitialName(draft: WorktreeRenameModalDraft): string {
  const branch = draft.branch?.trim();
  return branch && worktreeRenameFolderSlug(branch) === draft.currentName
    ? branch
    : draft.currentName;
}

/*
 * CDXC:WorktreeRename 2026-08-09-18:40:
 * These two refusals are pure computations over data the draft already carries,
 * so they answer while the user types instead of after they submit. Everything
 * that needs the filesystem or git — the destination already existing on disk, a
 * branch name already taken, a ref-namespace collision — is enforced by gxserver
 * at submit, because this modal has no channel to ask the daemon anything.
 */
function resolveWorktreeRenameCollisionError({
  draft,
  nextFolderName,
  nextFolderPath,
  unchanged,
}: {
  draft: WorktreeRenameModalDraft;
  nextFolderName: string;
  nextFolderPath: string;
  unchanged: boolean;
}): string | undefined {
  if (!nextFolderName || unchanged) {
    return undefined;
  }
  if (nextFolderPath === draft.parentProjectPath) {
    return "That name would collide with the main checkout.";
  }
  if ((draft.registeredProjectPaths ?? []).includes(nextFolderPath)) {
    return "Another project is already registered at that folder.";
  }
  return undefined;
}

/*
 * CDXC:WorktreeRename 2026-08-10:
 * The draft carries the project path exactly as it is registered, and on Windows
 * that is `C:\Users\me\repo`. Splitting on "/" alone found no separator there and
 * produced `/feat-name`, which matches no main checkout and no registered
 * project, so both live collision checks went quiet until the daemon refused the
 * submit. Take whichever separator the path actually uses and keep it.
 */
function joinRenameParentDirectory(parentProjectPath: string, folderName: string): string {
  const trimmed = parentProjectPath.replace(/[/\\]+$/, "");
  const separatorIndex = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
  const separator = separatorIndex >= 0 ? trimmed[separatorIndex] : "/";
  const familyRoot = separatorIndex > 0 ? trimmed.slice(0, separatorIndex) : "";
  return `${familyRoot}${separator}${folderName}`;
}

function getSidebarThemeVariant(theme: SidebarTheme): "dark" | "light" {
  /**
   * CDXC:SidebarTheme 2026-06-15-01:43:
   * Worktree rename is part of the app-modal family, so Light removes the dark
   * class while Dark 1 and Dark 2 keep dark shadcn mode.
   */
  return theme.startsWith("light-") || theme === "plain-light" ? "light" : "dark";
}
