import { useEffect, useId, useRef, useState, type FormEvent, type KeyboardEvent } from 'react';
import { cn } from '@/packages/components/utils';
import {
  DEFAULT_CUSTOM_SESSION_TAG_ICON,
  MAX_CUSTOM_SESSION_TAG_NAME_LENGTH,
  SESSION_TAG_COLOR_PRESETS,
  type CustomSessionTagsState,
} from '../shared/session-tags';
import { isSidebarCommandIcon, type SidebarCommandIcon } from '../shared/sidebar-command-icons';
import { CommandIconPicker } from './command-icon-picker';
import { CustomSessionTagGlyph } from './session-tag-ui';

export type CustomSessionTagEditorSubmit = {
  color: string;
  icon: SidebarCommandIcon;
  name: string;
};

/**
 * CDXC:Sessions 2026-09-11 DECISION:
 * User: a custom tag is created from both Settings (Sidebar Tags) and the Tag as menu, so the same small form (name, an icon from the shared icon allowlist, a color from the preset list) is rendered inline in both places instead of opening another window.
 * The form owns no catalog state: it reports the three field values and the host applies them to the catalog it holds at that moment, mirroring the Space editor.
 */
export function CustomSessionTagEditorForm({
  autoFocus = true,
  className,
  compact = false,
  onCancel,
  onSubmit,
  suggestedColorIndex = 0,
}: {
  autoFocus?: boolean;
  className?: string;
  /** Tighter spacing for the context-menu variant. */
  compact?: boolean;
  onCancel: () => void;
  onSubmit: (tag: CustomSessionTagEditorSubmit) => void;
  /** Rotates the preselected swatch so consecutive new tags do not all start on the same color. */
  suggestedColorIndex?: number;
}) {
  const [name, setName] = useState('');
  const [icon, setIcon] = useState<SidebarCommandIcon>(resolveEditorIcon(DEFAULT_CUSTOM_SESSION_TAG_ICON));
  const [color, setColor] = useState<string>(
    SESSION_TAG_COLOR_PRESETS[Math.abs(suggestedColorIndex) % SESSION_TAG_COLOR_PRESETS.length]!.value
  );
  const inputRef = useRef<HTMLInputElement>(null);
  const colorLabelId = useId();
  const trimmedName = name.trim();

  useEffect(() => {
    if (autoFocus) {
      inputRef.current?.focus({ preventScroll: true });
    }
  }, [autoFocus]);

  const submit = (event?: FormEvent<HTMLFormElement>) => {
    event?.preventDefault();
    if (!trimmedName) {
      return;
    }
    onSubmit({ color, icon, name: trimmedName });
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    /*
     * The form can sit inside a context menu that treats Escape, arrows, and Enter as menu navigation, so it owns those keys while it is open.
     */
    event.stopPropagation();
    if (event.key === 'Escape') {
      event.preventDefault();
      onCancel();
    }
  };

  return (
    <form
      className={cn('custom-session-tag-editor', compact && 'custom-session-tag-editor-compact', className)}
      data-empty-space-blocking='true'
      onClick={(event) => event.stopPropagation()}
      onKeyDown={handleKeyDown}
      onSubmit={submit}
    >
      <div className='custom-session-tag-editor-preview' aria-hidden='true'>
        <CustomSessionTagGlyph color={color} icon={icon} size={16} stroke={1.8} tagId='custom-preview' />
        <span className='custom-session-tag-editor-preview-name'>{trimmedName || 'New tag'}</span>
      </div>
      <input
        aria-label='Tag name'
        autoComplete='off'
        className='custom-session-tag-editor-input'
        maxLength={MAX_CUSTOM_SESSION_TAG_NAME_LENGTH}
        onChange={(event) => setName(event.currentTarget.value)}
        placeholder='Tag name'
        ref={inputRef}
        spellCheck={false}
        value={name}
      />
      <CommandIconPicker icon={icon} onIconChange={setIcon} />
      <div aria-labelledby={colorLabelId} className='custom-session-tag-editor-swatches' role='radiogroup'>
        <span className='sr-only' id={colorLabelId}>
          Color
        </span>
        {SESSION_TAG_COLOR_PRESETS.map((preset) => (
          <button
            aria-checked={preset.value === color}
            aria-label={preset.label}
            className='custom-session-tag-editor-swatch'
            data-selected={String(preset.value === color)}
            key={preset.value}
            onClick={() => setColor(preset.value)}
            role='radio'
            style={{ background: preset.value }}
            title={preset.label}
            type='button'
          />
        ))}
      </div>
      <div className='custom-session-tag-editor-actions'>
        <button className='custom-session-tag-editor-button' onClick={onCancel} type='button'>
          Cancel
        </button>
        <button
          className='custom-session-tag-editor-button custom-session-tag-editor-button-primary'
          disabled={!trimmedName}
          type='submit'
        >
          Create tag
        </button>
      </div>
    </form>
  );
}

export function nextCustomSessionTagColorIndex(state: CustomSessionTagsState | undefined): number {
  return state?.order.length ?? 0;
}

function resolveEditorIcon(icon: string): SidebarCommandIcon {
  return isSidebarCommandIcon(icon) ? icon : 'sparkles';
}
