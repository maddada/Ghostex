import type { ComponentProps } from 'react';
import { Button } from '@/packages/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/packages/components/ui/dialog';
import { cn } from '@/packages/components/utils';

/**
 * CDXC:UnifiedAppModal 2026-08-26:
 * One shell for every Ghostex app modal, so the Codex-style modal language
 * (established by Session Automations, CDXC:CodexModalRestyle 2026-08-24) is
 * owned in exactly one place: this component plus the `.gx-app-modal` rules in
 * styles/modals.css. Modals compose their body from the shared shadcn
 * primitives (Card, Field, Input, Select, Switch, Textarea) inside this shell;
 * the shell's class skins those slots. Do not hand-roll modal chrome, overlay
 * divs, or footer button rows in individual modals.
 */

export type AppModalShellProps = Omit<ComponentProps<typeof DialogContent>, 'showCloseButton'> & {
  isOpen: boolean;
  /** Called when the dialog asks to close (backdrop click, Escape). */
  onClose: () => void;
  showCloseButton?: boolean;
  /** Overrides the shell's default 460px width, e.g. for wide review modals. */
  width?: number;
};

export function AppModalShell({
  children,
  className,
  isOpen,
  onClose,
  showCloseButton = false,
  style,
  width,
  ...props
}: AppModalShellProps) {
  return (
    <Dialog
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          onClose();
        }
      }}
      open={isOpen}
    >
      <DialogContent
        className={cn('gx-app-modal font-sans', className)}
        showCloseButton={showCloseButton}
        style={width === undefined ? style : { ...style, ['--gx-modal-width' as string]: `${width}px` }}
        {...props}
      >
        {children}
      </DialogContent>
    </Dialog>
  );
}

/** The modal's top-level column (20px rhythm between header, body, footer). */
export function AppModalColumn({ className, ...props }: ComponentProps<'div'>) {
  return <div className={cn('gx-app-modal-form', className)} {...props} />;
}

/** Same column rhythm as AppModalColumn, as a form element. */
export function AppModalForm({ className, ...props }: ComponentProps<'form'>) {
  return <form className={cn('gx-app-modal-form', className)} {...props} />;
}

/** A stack of section cards inside the body (12px apart). */
export function AppModalStack({ className, ...props }: ComponentProps<'div'>) {
  return <div className={cn('gx-app-modal-stack', className)} {...props} />;
}

/** Footer of equal-width pill buttons filling the modal width. */
export function AppModalFooter({ className, ...props }: ComponentProps<typeof DialogFooter>) {
  return <DialogFooter className={cn('gx-app-modal-footer', className)} {...props} />;
}

export type AppModalButtonProps = ComponentProps<typeof Button> & {
  tone?: 'neutral' | 'danger' | 'primary';
};

/** Full-width outline pill footer button; `tone='danger'` tints destructive confirms. */
export function AppModalButton({ className, tone = 'neutral', ...props }: AppModalButtonProps) {
  return (
    <Button
      className={cn(
        'gx-app-modal-action-button',
        tone === 'danger' && 'gx-app-modal-action-danger',
        tone === 'primary' && 'gx-app-modal-action-primary',
        className
      )}
      variant='outline'
      {...props}
    />
  );
}

export { DialogDescription as AppModalDescription, DialogHeader as AppModalHeader, DialogTitle as AppModalTitle };

/**
 * Class for SelectContent popovers spawned from inside an app modal: they
 * portal to <body>, so they cannot inherit the shell's tokens and must carry
 * this class explicitly.
 */
export const APP_MODAL_SELECT_CONTENT_CLASS = 'gx-app-modal-select-content';
