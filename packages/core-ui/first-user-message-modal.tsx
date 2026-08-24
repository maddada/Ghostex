import { useEffect, useRef } from 'react';

export type FirstUserMessageModalProps = {
  isOpen: boolean;
  message: string;
  onClose: () => void;
  title?: string;
};

export function FirstUserMessageModal({ isOpen, message, onClose, title }: FirstUserMessageModalProps) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onClose();
      }
    };

    document.addEventListener('keydown', handleKeyDown, true);
    return () => {
      document.removeEventListener('keydown', handleKeyDown, true);
    };
  }, [isOpen, onClose]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    const timeout = window.setTimeout(() => {
      textareaRef.current?.focus();
    }, 0);
    return () => {
      window.clearTimeout(timeout);
    };
  }, [isOpen]);

  if (!isOpen) {
    return null;
  }

  return (
    <div className='confirm-modal-root scroll-mask-y' role='presentation'>
      <button className='confirm-modal-backdrop' onClick={onClose} type='button' />
      <div
        aria-labelledby='first-user-message-modal-title'
        aria-modal='true'
        className='confirm-modal first-user-message-modal scroll-mask-y'
        role='dialog'
      >
        <div className='confirm-modal-header'>
          <div className='confirm-modal-title' id='first-user-message-modal-title'>
            View 1st Message
          </div>
          {title ? <div className='confirm-modal-description'>{title}</div> : null}
        </div>
        {/*
         * CDXC:FirstMessage 2026-04-28-05:48
         * The first-message viewer must use a textarea, not a styled paragraph,
         * so users can read the saved prompt and select/copy the exact text
         * from both active sessions and previous-session modal cards.
         */}
        <textarea
          className='scratch-pad-textarea first-user-message-textarea'
          readOnly
          ref={textareaRef}
          value={message}
        />
      </div>
    </div>
  );
}
