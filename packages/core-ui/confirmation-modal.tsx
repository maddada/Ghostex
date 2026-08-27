import {
  AppModalButton,
  AppModalColumn,
  AppModalDescription,
  AppModalFooter,
  AppModalHeader,
  AppModalShell,
  AppModalTitle,
} from './app-modal-shell';

export type ConfirmationModalProps = {
  cancelLabel?: string;
  confirmLabel: string;
  confirmTone?: 'neutral' | 'danger' | 'primary';
  description: string;
  isOpen: boolean;
  onCancel: () => void;
  onClose?: () => void;
  onConfirm: () => void;
  title: string;
};

/**
 * CDXC:UnifiedAppModal 2026-08-26:
 * The shared yes/no prompt. It used to hand-roll its own portal, backdrop, and
 * primary/secondary button row; it now composes AppModalShell so it inherits
 * the one app-modal design language (and Radix's Escape/backdrop handling)
 * instead of maintaining a parallel `.confirm-modal-*` chrome.
 */
export function ConfirmationModal({
  cancelLabel = 'Cancel',
  confirmLabel,
  confirmTone = 'neutral',
  description,
  isOpen,
  onCancel,
  onClose = onCancel,
  onConfirm,
  title,
}: ConfirmationModalProps) {
  return (
    <AppModalShell className='confirmation-modal' isOpen={isOpen} onClose={onClose}>
      <AppModalColumn>
        <AppModalHeader>
          <AppModalTitle>{title}</AppModalTitle>
          <AppModalDescription>{description}</AppModalDescription>
        </AppModalHeader>
        <AppModalFooter>
          <AppModalButton onClick={onCancel} type='button'>
            {cancelLabel}
          </AppModalButton>
          <AppModalButton onClick={onConfirm} tone={confirmTone} type='button'>
            {confirmLabel}
          </AppModalButton>
        </AppModalFooter>
      </AppModalColumn>
    </AppModalShell>
  );
}
