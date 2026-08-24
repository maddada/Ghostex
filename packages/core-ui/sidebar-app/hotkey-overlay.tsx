import { useEffect, useRef, useState } from 'react';
import {
  GHOSTEX_HOTKEY_DEFINITIONS,
  normalizeHotkeyText,
  normalizeghostexHotkeySettings,
  type ghostexHotkeySettings,
} from '../../shared/ghostex-hotkeys';
import { formatSidebarHotkeyLabel } from '../hotkey-label';

export type NativeModifierStateHostEvent = {
  isCommandPressed: boolean;
  type: 'nativeModifierState';
};

export const SIDEBAR_HOTKEY_OVERLAY_ENABLED = false;
/*
 * CDXC:Hotkeys 2026-06-15-02:33:
 * Temporarily disable the Cmd-hold sidebar hotkey overlay while keeping the
 * hook, renderer, styles, and native modifier bridge in source for near-term
 * re-enable. Holding Cmd must not show the overlay from sidebar DOM focus or
 * native terminal/browser/titlebar focus while this flag is false.
 */

export function useCommandHotkeyOverlay(): boolean {
  const [isVisible, setIsVisible] = useState(false);
  const isCommandPressedRef = useRef(false);
  const showTimerRef = useRef<number | undefined>(undefined);

  useEffect(() => {
    if (!SIDEBAR_HOTKEY_OVERLAY_ENABLED) {
      return;
    }

    const clearOverlayTimer = () => {
      if (showTimerRef.current !== undefined) {
        window.clearTimeout(showTimerRef.current);
        showTimerRef.current = undefined;
      }
    };
    const hideOverlay = () => {
      isCommandPressedRef.current = false;
      clearOverlayTimer();
      setIsVisible(false);
    };
    const showOverlayAfterDelay = () => {
      if (isCommandPressedRef.current || showTimerRef.current !== undefined) {
        return;
      }
      isCommandPressedRef.current = true;
      /**
       * CDXC:Hotkeys 2026-05-11-09:26
       * Holding Cmd for one second should reveal an in-sidebar cheat sheet of
       * the current effective hotkeys. Delay the overlay so normal Cmd chords
       * do not flash UI while still making discovery available from the key the
       * simplified keymap now centers on.
       *
       * CDXC:Hotkeys 2026-06-14-19:40:
       * Native terminal, browser, and titlebar focus can hold Cmd without
       * delivering a WebKit keydown to the sidebar. Keep this dormant path wired
       * to native modifier host events so the cheat sheet can be restored by
       * flipping SIDEBAR_HOTKEY_OVERLAY_ENABLED.
       *
       * CDXC:Hotkeys 2026-06-15-02:33:
       * SIDEBAR_HOTKEY_OVERLAY_ENABLED intentionally short-circuits this effect
       * before listeners attach, so holding Cmd must not show this overlay until
       * the temporary disable is removed.
       */
      showTimerRef.current = window.setTimeout(() => {
        showTimerRef.current = undefined;
        if (isCommandPressedRef.current) {
          setIsVisible(true);
        }
      }, 1_000);
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Meta') {
        return;
      }
      showOverlayAfterDelay();
    };
    const handleKeyUp = (event: KeyboardEvent) => {
      if (event.key === 'Meta' || !event.metaKey) {
        hideOverlay();
      }
    };
    const handleNativeHostEvent = (event: Event) => {
      if (!(event instanceof CustomEvent) || !isNativeModifierStateHostEvent(event.detail)) {
        return;
      }
      if (event.detail.isCommandPressed) {
        showOverlayAfterDelay();
      } else {
        hideOverlay();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('keyup', handleKeyUp);
    window.addEventListener('ghostex-native-host-event', handleNativeHostEvent);
    window.addEventListener('blur', hideOverlay);
    return () => {
      clearOverlayTimer();
      window.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('keyup', handleKeyUp);
      window.removeEventListener('ghostex-native-host-event', handleNativeHostEvent);
      window.removeEventListener('blur', hideOverlay);
    };
  }, []);

  return SIDEBAR_HOTKEY_OVERLAY_ENABLED && isVisible;
}

export function isNativeModifierStateHostEvent(value: unknown): value is NativeModifierStateHostEvent {
  return (
    Boolean(value) &&
    typeof value === 'object' &&
    (value as NativeModifierStateHostEvent).type === 'nativeModifierState' &&
    typeof (value as NativeModifierStateHostEvent).isCommandPressed === 'boolean'
  );
}

export function SidebarHotkeyOverlay({ hotkeys }: { hotkeys?: ghostexHotkeySettings }) {
  const normalizedHotkeys = normalizeghostexHotkeySettings(hotkeys);
  const rows = getSidebarHotkeyOverlayRows(normalizedHotkeys);

  return (
    <>
      <div aria-hidden='true' className='sidebar-hotkey-overlay-backdrop' />
      <aside aria-label='Keyboard shortcuts' className='sidebar-hotkey-overlay'>
        <div className='sidebar-hotkey-overlay-title'>Hotkeys</div>
        <div className='sidebar-hotkey-overlay-grid'>
          {rows.map((row) => (
            <div className='sidebar-hotkey-overlay-row' key={`${row.title}-${row.hotkey}`}>
              <span className='sidebar-hotkey-overlay-action'>{row.title}</span>
              <kbd className='sidebar-hotkey-overlay-key'>{formatSidebarHotkeyLabel(row.hotkey)}</kbd>
            </div>
          ))}
        </div>
      </aside>
    </>
  );
}
export function getSidebarHotkeyOverlayRows(hotkeys: ghostexHotkeySettings) {
  const rows: Array<{ hotkey: string; title: string }> = [];
  for (const definition of GHOSTEX_HOTKEY_DEFINITIONS) {
    if (definition.id === 'jumpToProject1') {
      const hotkey = normalizeHotkeyText(hotkeys.jumpToProject1 ?? '');
      if (hotkey) {
        rows.push({
          hotkey: formatNumberedHotkeyExample(hotkey),
          title: 'Jump to Project N',
        });
      }
      continue;
    }
    if (definition.id === 'focusSessionSlot1') {
      const hotkey = normalizeHotkeyText(hotkeys.focusSessionSlot1 ?? '');
      if (hotkey) {
        rows.push({
          hotkey: formatNumberedHotkeyExample(hotkey),
          title: 'Focus Session N',
        });
      }
      continue;
    }
    if (/^jumpToProject[2-9]$/u.test(definition.id) || /^focusSessionSlot[2-9]$/u.test(definition.id)) {
      continue;
    }
    const hotkey = normalizeHotkeyText(hotkeys[definition.id] ?? '');
    if (hotkey) {
      rows.push({ hotkey, title: definition.title });
    }
  }
  return rows;
}

export function formatNumberedHotkeyExample(hotkey: string): string {
  /**
   * CDXC:Hotkeys 2026-05-11-09:36
   * The Cmd-hold overlay should not list every numbered session or group slot.
   * Show one N-based example derived from slot 1 so user rebinds still explain
   * the whole numbered family without crowding the cheat sheet.
   */
  return hotkey.replace(/(^|[+ ])1(?=$| )/u, '$1n');
}
