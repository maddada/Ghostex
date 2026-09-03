import { useCallback, useEffect, useId, useRef, useState, type FormEvent } from 'react';
import { Field, FieldLabel, FieldTitle } from '@/packages/components/ui/field';
import { Input } from '@/packages/components/ui/input';
import {
  AppModalButton,
  AppModalDescription,
  AppModalFooter,
  AppModalForm,
  AppModalHeader,
  AppModalShell,
  AppModalTitle,
} from './app-modal-shell';
import { CommandIconPicker } from './command-icon-picker';
import { SIDEBAR_PROJECT_COLLECTION_COLORS, SIDEBAR_PROJECT_COLLECTION_COLOR_LABELS } from './project-collections';
import { DEFAULT_SIDEBAR_SPACE_ICON } from './spaces';
import { isSidebarCommandIcon, type SidebarCommandIcon } from '../shared/sidebar-command-icons';

/*
CDXC:Spaces 2026-08-27:
The whole New Space / Edit Space surface, per the Spaces decision that editing is
one small popup with name, icon, and color — and no central management screen.

This dialog owns NO Space state. It reports the three field values (and, in edit
mode, a Delete) and the host applies them to the Space document the sidebar holds
at that moment. That split is deliberate: the dialog renders in its own native
child window on the desktop app, so any document it carried would already be one
edit behind by the time the user pressed Save.
*/

export type SpaceEditorModalSubmit = {
  color: string;
  icon: string;
  name: string;
};

export type SpaceEditorModalProps = {
  initialColor?: string;
  initialIcon?: string;
  initialName?: string;
  isOpen: boolean;
  mode: 'create' | 'edit';
  onCancel: () => void;
  /** Edit mode only. Deleting removes the Space and its memberships, nothing else. */
  onDelete: () => void;
  onSubmit: (space: SpaceEditorModalSubmit) => void;
};

export function SpaceEditorModal({
  initialColor,
  initialIcon,
  initialName,
  isOpen,
  mode,
  onCancel,
  onDelete,
  onSubmit,
}: SpaceEditorModalProps) {
  const resolvedInitialColor = initialColor ?? SIDEBAR_PROJECT_COLLECTION_COLORS[0];
  const resolvedInitialIcon = resolveSpaceEditorIcon(initialIcon);
  const [name, setName] = useState(initialName ?? '');
  const [icon, setIcon] = useState<SidebarCommandIcon>(resolvedInitialIcon);
  const [color, setColor] = useState(resolvedInitialColor);
  const nameInputId = useId();
  const colorLabelId = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  const userInteractedAfterOpenRef = useRef(false);

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    userInteractedAfterOpenRef.current = false;
    setName(initialName ?? '');
    setIcon(resolvedInitialIcon);
    setColor(resolvedInitialColor);
  }, [initialName, isOpen, resolvedInitialColor, resolvedInitialIcon]);

  /**
   * CDXC:Spaces 2026-08-27:
   * Same initial-focus contract as Rename Session (see CDXC:Sessions in
   * session-rename-modal.tsx): the dialog opens in a hidden native child window
   * that becomes key only after React has already reported `presented`, so the
   * focus request has to survive that boundary — and has to stop as soon as the
   * user touches anything, or delayed host focus steals their caret.
   */
  const focusAndSelectInput = useCallback(() => {
    const input = inputRef.current;
    if (input) {
      input.focus({ preventScroll: true });
      input.setSelectionRange(0, input.value.length);
    }
    return false as const;
  }, []);

  const markUserInteractedAfterOpen = useCallback(() => {
    userInteractedAfterOpenRef.current = true;
  }, []);

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    const focusUnlessUserInteracted = () => {
      if (userInteractedAfterOpenRef.current) {
        return;
      }
      focusAndSelectInput();
    };
    const retryDelaysMs = [0, 16, 50, 100, 250, 500, 1000, 1600, 2400];
    const timeoutIds = retryDelaysMs.map((delayMs) => window.setTimeout(focusUnlessUserInteracted, delayMs));
    const animationFrame = window.requestAnimationFrame(focusUnlessUserInteracted);
    const windowFocusTimeoutIds: number[] = [];
    const handleWindowFocus = () => {
      windowFocusTimeoutIds.push(window.setTimeout(focusUnlessUserInteracted, 0));
    };

    window.addEventListener('focus', handleWindowFocus);
    return () => {
      window.cancelAnimationFrame(animationFrame);
      timeoutIds.forEach((timeoutId) => window.clearTimeout(timeoutId));
      windowFocusTimeoutIds.forEach((timeoutId) => window.clearTimeout(timeoutId));
      window.removeEventListener('focus', handleWindowFocus);
    };
  }, [focusAndSelectInput, isOpen]);

  if (!isOpen) {
    return null;
  }

  const trimmedName = name.trim();
  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!trimmedName) {
      return;
    }
    onSubmit({ color, icon, name: trimmedName });
  };

  return (
    <AppModalShell className='space-editor-modal-shadcn' initialFocus={focusAndSelectInput} isOpen onClose={onCancel}>
      <AppModalForm
        className='space-editor-form'
        onKeyDownCapture={markUserInteractedAfterOpen}
        onPointerDownCapture={markUserInteractedAfterOpen}
        onSubmit={submit}
      >
        <AppModalHeader>
          <AppModalTitle>{mode === 'edit' ? 'Edit Space' : 'New Space'}</AppModalTitle>
          <AppModalDescription>
            A Space is a saved sidebar filter. Add groups and ungrouped projects to it from their own right-click menus.
          </AppModalDescription>
        </AppModalHeader>
        <Field>
          <FieldLabel htmlFor={nameInputId}>Name</FieldLabel>
          <Input
            autoComplete='off'
            className='space-editor-name-input'
            id={nameInputId}
            onChange={(event) => setName(event.currentTarget.value)}
            ref={inputRef}
            spellCheck={false}
            value={name}
          />
        </Field>
        <CommandIconPicker icon={icon} onIconChange={setIcon} />
        <Field>
          <FieldTitle id={colorLabelId}>Color</FieldTitle>
          <div aria-labelledby={colorLabelId} className='space-editor-color-strip' role='radiogroup'>
            {SIDEBAR_PROJECT_COLLECTION_COLORS.map((swatchColor) => (
              <button
                aria-checked={swatchColor === color}
                aria-label={SIDEBAR_PROJECT_COLLECTION_COLOR_LABELS[swatchColor]}
                className='space-editor-color-swatch'
                data-selected={String(swatchColor === color)}
                key={swatchColor}
                onClick={() => setColor(swatchColor)}
                role='radio'
                style={{ background: swatchColor }}
                title={SIDEBAR_PROJECT_COLLECTION_COLOR_LABELS[swatchColor]}
                type='button'
              />
            ))}
          </div>
        </Field>
        <AppModalFooter>
          <AppModalButton onClick={onCancel} type='button'>
            Cancel
          </AppModalButton>
          {mode === 'edit' ? (
            <AppModalButton onClick={onDelete} tone='danger' type='button'>
              Delete
            </AppModalButton>
          ) : null}
          <AppModalButton disabled={!trimmedName} type='submit'>
            {mode === 'edit' ? 'Save' : 'Create'}
          </AppModalButton>
        </AppModalFooter>
      </AppModalForm>
    </AppModalShell>
  );
}

function resolveSpaceEditorIcon(icon: string | undefined): SidebarCommandIcon {
  return icon && isSidebarCommandIcon(icon) ? icon : DEFAULT_SIDEBAR_SPACE_ICON;
}
