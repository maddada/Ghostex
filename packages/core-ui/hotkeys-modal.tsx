import { useEffect, useMemo, useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/packages/components/ui/dialog';
import {
  DEFAULT_ghostex_HOTKEYS,
  GHOSTEX_HOTKEY_DEFINITIONS,
  normalizeHotkeyText,
  normalizeghostexHotkeySettings,
  type ghostexHotkeyActionId,
  type ghostexHotkeySettings,
} from '../shared/ghostex-hotkeys';
import { HotkeyRecorderField } from './hotkey-recorder-field';

export type HotkeysModalProps = {
  hotkeys?: ghostexHotkeySettings;
  isOpen: boolean;
  onChange: (hotkeys: ghostexHotkeySettings) => void;
  onClose: () => void;
};

export function HotkeysModal({ hotkeys, isOpen, onChange, onClose }: HotkeysModalProps) {
  const [draft, setDraft] = useState<ghostexHotkeySettings>(() => normalizeghostexHotkeySettings(hotkeys));
  const duplicateIds = useMemo(() => getDuplicateHotkeyIds(draft), [draft]);
  const defaultHotkeys = normalizeghostexHotkeySettings(DEFAULT_ghostex_HOTKEYS);

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    setDraft(normalizeghostexHotkeySettings(hotkeys));
  }, [hotkeys, isOpen]);

  const updateHotkey = (id: ghostexHotkeyActionId, value: string) => {
    const nextHotkeys = normalizeghostexHotkeySettings({
      ...draft,
      [id]: normalizeHotkeyText(value),
    });
    setDraft(nextHotkeys);
    onChange(nextHotkeys);
  };

  const resetHotkeys = () => {
    const nextHotkeys = defaultHotkeys;
    setDraft(nextHotkeys);
    onChange(nextHotkeys);
  };

  return (
    <Dialog onOpenChange={(nextOpen) => (!nextOpen ? onClose() : undefined)} open={isOpen}>
      <DialogContent
        className='ghostex-settings-shadcn hotkeys-modal max-h-[min(760px,calc(100vh-2rem))] gap-0 overflow-hidden p-0 font-sans sm:max-w-2xl'
        onEscapeKeyDown={(event) => {
          if (hasActiveHotkeyRecorder()) {
            event.preventDefault();
          }
        }}
      >
        <DialogHeader className='px-5 pt-5 pb-3'>
          <DialogTitle className='text-xl'>Hotkeys</DialogTitle>
        </DialogHeader>
        {/* CDXC:Hotkeys 2026-05-10-12:06
            Hotkeys are rebound through a recorder control so macOS Command
            chords are captured as shortcuts instead of being typed into a text
            field or stolen by the global hotkey listener.
            CDXC:Hotkeys 2026-04-28-05:31
            The hotkey list must use a constrained native overflow container
            so long shortcut sets scroll inside the modal instead of expanding
            beyond the app window and hiding footer controls. */}
        <div className='hotkeys-modal-scroll scroll-mask-y'>
          <div className='hotkeys-modal-body px-5 pb-5'>
            {GHOSTEX_HOTKEY_DEFINITIONS.map((definition) => {
              const value = draft[definition.id] ?? definition.defaultKey;
              const isDuplicate = duplicateIds.has(definition.id);
              return (
                <label className='hotkeys-modal-row' key={definition.id}>
                  <span className='hotkeys-modal-copy'>
                    <span className='hotkeys-modal-title'>{definition.title}</span>
                    <span className='hotkeys-modal-description'>{definition.description}</span>
                  </span>
                  <HotkeyRecorderField
                    ariaInvalid={isDuplicate}
                    className='hotkeys-modal-input'
                    hotkey={value}
                    onChange={(nextHotkey) => updateHotkey(definition.id, nextHotkey)}
                    originalHotkey={defaultHotkeys[definition.id] ?? ''}
                  />
                </label>
              );
            })}
          </div>
        </div>
        <div className='confirm-modal-actions px-5 pb-5'>
          <Button onClick={resetHotkeys} type='button' variant='outline'>
            Reset
          </Button>
          <Button onClick={onClose} type='button'>
            Done
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function hasActiveHotkeyRecorder(): boolean {
  return Boolean(document.querySelector("[data-hotkey-recorder='true'][data-recording='true']"));
}

function getDuplicateHotkeyIds(hotkeys: ghostexHotkeySettings): Set<ghostexHotkeyActionId> {
  const idsByHotkey = new Map<string, ghostexHotkeyActionId[]>();
  for (const definition of GHOSTEX_HOTKEY_DEFINITIONS) {
    const hotkey = normalizeHotkeyText(hotkeys[definition.id] ?? definition.defaultKey);
    if (!hotkey) {
      continue;
    }
    idsByHotkey.set(hotkey, [...(idsByHotkey.get(hotkey) ?? []), definition.id]);
  }

  return new Set(
    Array.from(idsByHotkey.values())
      .filter((ids) => ids.length > 1)
      .flat()
  );
}
