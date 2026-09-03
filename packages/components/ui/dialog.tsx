import * as React from 'react';
import { Dialog as DialogPrimitive } from '@base-ui/react/dialog';

import { cn } from '../utils';
import { Button } from './button';
import { IconX } from '@tabler/icons-react';

function Dialog({ ...props }: DialogPrimitive.Root.Props) {
  return <DialogPrimitive.Root data-slot='dialog' {...props} />;
}

function DialogTrigger({ ...props }: DialogPrimitive.Trigger.Props) {
  return <DialogPrimitive.Trigger data-slot='dialog-trigger' {...props} />;
}

function DialogPortal({ ...props }: DialogPrimitive.Portal.Props) {
  return <DialogPrimitive.Portal data-slot='dialog-portal' {...props} />;
}

function DialogClose({ ...props }: DialogPrimitive.Close.Props) {
  return <DialogPrimitive.Close data-slot='dialog-close' {...props} />;
}

function DialogOverlay({ className, ...props }: DialogPrimitive.Backdrop.Props) {
  return (
    <DialogPrimitive.Backdrop
      data-slot='dialog-overlay'
      className={cn(
        'fixed inset-0 isolate z-50 bg-black/30 duration-100 supports-backdrop-filter:backdrop-blur-sm data-open:animate-in data-open:fade-in-0 data-closed:animate-out data-closed:fade-out-0',
        className
      )}
      {...props}
    />
  );
}

/**
 * CDXC:AppModal 2026-09-03 WHY:
 * Base UI drops the Backdrop of a dialog that opens inside another open dialog (`DialogBackdrop` renders only when `forceRender || !nested`), so a dialog nested in Settings got no overlay at all and its outside-click target was the bare Settings page.
 * `nested` opts back in: it force-renders the backdrop and stamps it `data-nested` so the stylesheet can tell it apart from the window-root dialog (the Popup already carries Base UI's own `data-nested`).
 * SEE-ALSO: `.app-modal-host-native-window-body` rules in packages/core-ui/styles/modals.css, which paint only the window-root dialog as the window itself.
 */
function DialogContent({
  className,
  children,
  nested = false,
  onEscapeKeyDown,
  onKeyDownCapture,
  onOpenAutoFocus,
  showCloseButton = false,
  ...props
}: DialogPrimitive.Popup.Props & {
  /** Set when this dialog opens on top of another open dialog and needs its own backdrop. */
  nested?: boolean;
  onEscapeKeyDown?: (event: React.KeyboardEvent<HTMLElement>) => void;
  onOpenAutoFocus?: (event: { preventDefault: () => void }) => void;
  showCloseButton?: boolean;
}) {
  React.useEffect(() => {
    onOpenAutoFocus?.({ preventDefault: () => undefined });
  }, [onOpenAutoFocus]);

  return (
    <DialogPortal>
      {nested ? <DialogOverlay data-nested='' forceRender /> : <DialogOverlay />}
      <DialogPrimitive.Popup
        data-slot='dialog-content'
        className={cn(
          'fixed top-1/2 left-1/2 z-50 grid w-full max-w-[calc(100%-2rem)] -translate-x-1/2 -translate-y-1/2 gap-6 rounded-none bg-popover p-6 text-sm text-popover-foreground shadow-xl ring-1 ring-foreground/5 duration-100 outline-none sm:max-w-md dark:ring-foreground/10 data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95 data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95',
          className
        )}
        onKeyDownCapture={(event) => {
          if (event.key === 'Escape') {
            onEscapeKeyDown?.(event);
          }
          onKeyDownCapture?.(event);
        }}
        {...props}
      >
        {children}
        {showCloseButton && (
          <DialogPrimitive.Close
            data-slot='dialog-close'
            render={<Button variant='ghost' className='absolute top-4 right-4 bg-secondary' size='icon-sm' />}
          >
            <IconX />
            <span className='sr-only'>Close</span>
          </DialogPrimitive.Close>
        )}
      </DialogPrimitive.Popup>
    </DialogPortal>
  );
}

function DialogHeader({ className, ...props }: React.ComponentProps<'div'>) {
  return <div data-slot='dialog-header' className={cn('flex flex-col gap-1.5', className)} {...props} />;
}

function DialogFooter({
  className,
  showCloseButton = false,
  children,
  ...props
}: React.ComponentProps<'div'> & {
  showCloseButton?: boolean;
}) {
  return (
    <div
      data-slot='dialog-footer'
      className={cn('flex flex-col-reverse gap-2 sm:flex-row sm:justify-end', className)}
      {...props}
    >
      {children}
      {showCloseButton && <DialogPrimitive.Close render={<Button variant='outline' />}>Close</DialogPrimitive.Close>}
    </div>
  );
}

function DialogTitle({ className, ...props }: DialogPrimitive.Title.Props) {
  return (
    <DialogPrimitive.Title
      data-slot='dialog-title'
      className={cn('font-heading text-base leading-none font-medium', className)}
      {...props}
    />
  );
}

function DialogDescription({ className, ...props }: DialogPrimitive.Description.Props) {
  return (
    <DialogPrimitive.Description
      data-slot='dialog-description'
      className={cn(
        'text-sm text-muted-foreground *:[a]:underline *:[a]:underline-offset-3 *:[a]:hover:text-foreground',
        className
      )}
      {...props}
    />
  );
}

export {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogOverlay,
  DialogPortal,
  DialogTitle,
  DialogTrigger,
};
