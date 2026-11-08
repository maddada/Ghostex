import { IconLink, IconX } from '@tabler/icons-react';
import { useEffect } from 'react';
import { cx } from './components';

export interface WorkToastState {
  /** A new id restarts the timer for a toast with the same text. */
  id: number;
  text: string;
  tone?: 'error' | 'warning';
  /** The bold first line of a notice with a button (`action`). */
  title?: string;
  action?: { label: string; run: () => void };
}

/** How long a toast without a button stays. */
const TOAST_MS = 6_000;

/**
 * The page's one toast, at the bottom of the view: a cloud start, an error. One with a button
 * (a link's Undo) is a notice in the middle of the view instead, and stays until it is answered.
 *
 * CDXC:AppModal 2026-11-08 DECISION: User: "please 1 thing we must fix is to never show toasts with
 * buttons inside them 1- they look ugly 2- they can be missed. show a nice modal middle of screen
 * for all such cases". The app's own notices follow the same rule
 * (apps/desktop/src/app/window/notice_modal.rs). Escape and Close close it, Enter never runs its
 * button.
 */
export function WorkToast({ toast, onClose }: { toast: WorkToastState | null; onClose: () => void }) {
  useEffect(() => {
    if (!toast || toast.action) return;
    const timer = window.setTimeout(onClose, TOAST_MS);
    return () => window.clearTimeout(timer);
  }, [toast, onClose]);
  useEffect(() => {
    if (!toast?.action) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [toast, onClose]);
  if (!toast) return null;
  if (toast.action) {
    const action = toast.action;
    return (
      <div className='w-ask-layer'>
        <div className='w-ask' role='alertdialog' aria-modal='true' aria-labelledby='w-ask-title'>
          <div className='w-ask-head'>
            <span className='w-ask-icon'>
              <IconLink size={18} />
            </span>
            <div className='w-ask-words'>
              <div className='w-ask-title' id='w-ask-title'>
                {toast.title ?? toast.text}
              </div>
              {toast.title ? <div className='w-ask-text'>{toast.text}</div> : null}
            </div>
          </div>
          <div className='w-ask-buttons'>
            <button type='button' className='w-btn w-btn--md' onClick={onClose} autoFocus>
              Close
            </button>
            <button type='button' className='w-btn w-btn--md w-btn--primary' onClick={action.run}>
              {action.label}
            </button>
          </div>
        </div>
      </div>
    );
  }
  return (
    <div className={cx('w-toast', toast.tone && `is-${toast.tone}`)} role='status'>
      <span className='w-toast-text'>{toast.text}</span>
      <button type='button' className='w-toast-close' title='Close' onClick={onClose}>
        <IconX size={13} />
      </button>
    </div>
  );
}
