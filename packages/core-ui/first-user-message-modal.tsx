import { useEffect, useRef } from 'react';
import { Textarea } from '@/packages/components/ui/textarea';
import {
  AppModalButton,
  AppModalColumn,
  AppModalDescription,
  AppModalFooter,
  AppModalHeader,
  AppModalShell,
  AppModalTitle,
} from './app-modal-shell';

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
    <AppModalShell className='first-user-message-modal' isOpen={isOpen} onClose={onClose} width={560}>
      <AppModalColumn>
        <AppModalHeader>
          <AppModalTitle>View 1st Message</AppModalTitle>
          {title ? <AppModalDescription>{title}</AppModalDescription> : null}
        </AppModalHeader>
        {/*
         * CDXC:Sessions 2026-04-28-05:48
         * The first-message viewer must use a textarea, not a styled paragraph,
         * so users can read the saved prompt and select/copy the exact text
         * from both active sessions and previous-session modal cards.
         */}
        <Textarea
          aria-label='First message'
          className='first-user-message-textarea'
          readOnly
          ref={textareaRef}
          value={message}
        />
        <AppModalFooter>
          <AppModalButton onClick={onClose} type='button'>
            Close
          </AppModalButton>
        </AppModalFooter>
      </AppModalColumn>
    </AppModalShell>
  );
}
