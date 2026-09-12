import { useEffect, useRef, useState, type FormEvent } from 'react';
import { Button } from '@/packages/components/ui/button';
import { Input } from '@/packages/components/ui/input';
import { cn } from '@/packages/components/utils';
import {
  DEFAULT_CUSTOM_SESSION_TAG_ICON,
  MAX_CUSTOM_SESSION_TAG_NAME_LENGTH,
  SESSION_TAG_COLOR_PRESETS,
  type CustomSessionTagsState,
} from '../shared/session-tags';
import { isSidebarCommandIcon, type SidebarCommandIcon } from '../shared/sidebar-command-icons';
import { AppTooltip } from './app-tooltip';
import { CommandIconPicker } from './command-icon-picker';

export type CustomSessionTagEditorSubmit = {
  color: string;
  icon: SidebarCommandIcon;
  name: string;
};

/**
 * CDXC:Sessions 2026-09-12 DECISION:
 * User: the New tag form lives only in Settings > Sidebar Tags, and it has to be built from the same rounded controls as the rest of Settings.
 * So it is the Settings swatch button, Input, and Button primitives rather than hand-rolled CSS, and it is laid out as one more row of the tag list: the icon trigger doubles as the live preview by drawing the chosen glyph in the chosen color next to the name field.
 * SEE-ALSO: packages/core-ui/settings-modal/fields.tsx (SidebarTagListSettingsField owns the catalog write).
 */
export function CustomSessionTagEditorForm({
  autoFocus = true,
  className,
  onCancel,
  onSubmit,
  suggestedColorIndex = 0,
}: {
  autoFocus?: boolean;
  className?: string;
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
  const trimmedName = name.trim();

  useEffect(() => {
    if (autoFocus) {
      inputRef.current?.focus({ preventScroll: true });
    }
  }, [autoFocus]);

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!trimmedName) {
      return;
    }
    onSubmit({ color, icon, name: trimmedName });
  };

  return (
    <form
      className={cn('flex w-full flex-col gap-2.5', className)}
      onKeyDown={(event) => {
        if (event.key === 'Escape') {
          event.preventDefault();
          event.stopPropagation();
          onCancel();
        }
      }}
      onSubmit={submit}
    >
      <div className='flex items-center gap-2'>
        <CommandIconPicker color={color} compact icon={icon} onIconChange={setIcon} triggerLabel='Tag icon' />
        <Input
          aria-label='Tag name'
          autoComplete='off'
          className='h-8 min-w-0 flex-1'
          maxLength={MAX_CUSTOM_SESSION_TAG_NAME_LENGTH}
          onChange={(event) => setName(event.currentTarget.value)}
          placeholder='Tag name'
          ref={inputRef}
          spellCheck={false}
          value={name}
        />
      </div>
      <div className='flex flex-wrap items-center gap-2'>
        <div aria-label='Tag color' className='flex flex-wrap items-center gap-1.5' role='radiogroup'>
          {SESSION_TAG_COLOR_PRESETS.map((preset) => {
            const isSelected = preset.value === color;
            return (
              <AppTooltip content={preset.label} key={preset.value}>
                <Button
                  aria-checked={isSelected}
                  aria-label={`Use ${preset.label}`}
                  className={cn(
                    'size-7 min-w-0 shrink-0 border p-0',
                    isSelected ? 'border-ring ring-2 ring-ring/45' : 'border-border/80'
                  )}
                  onClick={() => setColor(preset.value)}
                  role='radio'
                  style={{ backgroundColor: preset.value }}
                  type='button'
                  variant='ghost'
                />
              </AppTooltip>
            );
          })}
        </div>
        <div className='ml-auto flex items-center gap-2'>
          <Button onClick={onCancel} type='button' variant='ghost'>
            Cancel
          </Button>
          <Button disabled={!trimmedName} type='submit' variant='outline'>
            Create tag
          </Button>
        </div>
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
