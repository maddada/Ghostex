import { createPortal } from 'react-dom';
import { useEffect } from 'react';
import { formatWorktreePathForDisplay } from '../../shared/sidebar-v2-worktree-cleanup';

/*
 * CDXC:SidebarV2Worktree 2026-07-29:
 * The prompt shown when closing the LAST session that lives in a worktree this
 * flow created. It is a sidebar-document dialog (the same `.confirm-modal-*`
 * chrome `ConfirmationModal` uses) rather than the native full-window
 * `deleteWorktree` modal, for three reasons:
 * - The V1 delete modal is keyed to a worktree PROJECT registration; V2
 *   worktrees are session attributes and have no project row to delete.
 * - It renders in a native child window, so it cannot be driven from the
 *   sidebar's own story harness.
 * - The V1 flow must stay exactly as it is; this prompt never touches it.
 *
 * It cannot reuse `ConfirmationModal` verbatim because the choice is not
 * yes/no: closing the session always happens, and the question is only whether
 * the checkout goes with it.
 *
 * The dirty state is a RE-ASK, not an error: gxserver refuses to delete a
 * checkout with uncommitted work, and the second pass carries `force`.
 */

export type SidebarV2WorktreeCleanupPromptProps = {
  /** gxserver refused: the checkout has uncommitted work. */
  isDirty: boolean;
  /** A removal request is in flight. */
  isPending: boolean;
  /** Sanitized failure text from the host. */
  errorMessage?: string;
  onCancel: () => void;
  /** Close the session and leave the checkout on disk. */
  onKeepWorktree: () => void;
  /** Close the session and remove the checkout (with force after a refusal). */
  onRemoveWorktree: () => void;
  warnings?: readonly string[];
  worktreePath: string;
};

export function SidebarV2WorktreeCleanupPrompt({
  errorMessage,
  isDirty,
  isPending,
  onCancel,
  onKeepWorktree,
  onRemoveWorktree,
  warnings,
  worktreePath,
}: SidebarV2WorktreeCleanupPromptProps) {
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onCancel();
      }
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [onCancel]);

  const worktreeName = formatWorktreePathForDisplay(worktreePath);

  return createPortal(
    <div className='confirm-modal-root sidebar-v2-worktree-cleanup' role='presentation'>
      <button className='confirm-modal-backdrop' onClick={onCancel} type='button' />
      <div
        aria-labelledby='sidebar-v2-worktree-cleanup-title'
        aria-modal='true'
        className='confirm-modal'
        role='dialog'
      >
        <div className='confirm-modal-header'>
          <div className='confirm-modal-title' id='sidebar-v2-worktree-cleanup-title'>
            {isDirty ? 'Worktree has uncommitted changes' : 'Remove this worktree?'}
          </div>
          <div className='confirm-modal-description'>
            {isDirty
              ? `${worktreeName} still has uncommitted work. Removing it discards those changes.`
              : `This was the last session in ${worktreeName}.`}
          </div>
        </div>
        {warnings && warnings.length > 0 ? (
          <ul className='sidebar-v2-worktree-cleanup-warnings'>
            {warnings.map((warning) => (
              <li key={warning}>{warning}</li>
            ))}
          </ul>
        ) : null}
        {errorMessage ? (
          <p className='sidebar-v2-worktree-error' role='alert'>
            {errorMessage}
          </p>
        ) : null}
        <div className='confirm-modal-actions'>
          <button
            className='secondary confirm-modal-button'
            data-worktree-cleanup-action='keep'
            disabled={isPending}
            onClick={onKeepWorktree}
            type='button'
          >
            Close, keep worktree
          </button>
          <button
            className='primary confirm-modal-button'
            data-worktree-cleanup-action={isDirty ? 'force' : 'remove'}
            disabled={isPending}
            onClick={onRemoveWorktree}
            type='button'
          >
            {isPending ? 'Removing…' : isDirty ? 'Remove anyway' : 'Close and remove worktree'}
          </button>
        </div>
      </div>
    </div>,
    document.body
  );
}
