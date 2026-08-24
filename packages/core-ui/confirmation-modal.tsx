import { createPortal } from 'react-dom';
import { useEffect } from 'react';

export type ConfirmationModalProps = {
  confirmLabel: string;
  description: string;
  isOpen: boolean;
  onCancel: () => void;
  onConfirm: () => void;
  title: string;
};

export function ConfirmationModal({
  confirmLabel,
  description,
  isOpen,
  onCancel,
  onConfirm,
  title,
}: ConfirmationModalProps) {
  useEffect(() => {
    if (!isOpen) {
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onCancel();
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [isOpen, onCancel]);

  if (!isOpen) {
    return null;
  }

  return createPortal(
    <div className='confirm-modal-root scroll-mask-y' role='presentation'>
      <button className='confirm-modal-backdrop' onClick={onCancel} type='button' />
      <div
        aria-describedby='confirm-modal-description'
        aria-labelledby='confirm-modal-title'
        aria-modal='true'
        className='confirm-modal scroll-mask-y'
        role='dialog'
      >
        <div className='confirm-modal-header'>
          <div className='confirm-modal-title' id='confirm-modal-title'>
            {title}
          </div>
          <div className='confirm-modal-description' id='confirm-modal-description'>
            {description}
          </div>
        </div>
        <div className='confirm-modal-actions'>
          <button className='secondary confirm-modal-button' onClick={onCancel} type='button'>
            Cancel
          </button>
          <button className='primary confirm-modal-button' onClick={onConfirm} type='button'>
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>,
    document.body
  );
}
