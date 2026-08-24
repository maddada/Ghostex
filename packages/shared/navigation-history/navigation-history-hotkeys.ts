/**
 * CDXC:NavigationHistory 2026-08-19:
 * Keyboard access to the Back/Forward trail for hosts that resolve hotkeys in
 * the page — the web app today.
 *
 * The gpui desktop app deliberately does NOT use this. Its sidebar surface is
 * mouse- and focus-passive, so key events belong to the focused terminal and
 * never reach the sidebar document; gpui matches the same shared chord natively
 * (`GPUI_DEFAULT_GHOSTEX_HOTKEYS` in apps/desktop/src/app/hotkeys.rs) and routes it to the same
 * controller through its titlebar bridge. Both apps therefore honour the user's
 * configured chord from one table, without the page ever competing with a
 * terminal for the keystroke.
 */

import {
  getghostexHotkeyActionIdForKey,
  normalizeghostexHotkeySettings,
  type ghostexHotkeySettings,
} from '../ghostex-hotkeys';
import type { NavigationHistoryDirection } from './navigation-history-contract';

const NAVIGATION_HISTORY_HOTKEY_DIRECTIONS: Readonly<Record<string, NavigationHistoryDirection>> = {
  navigateHistoryBack: 'back',
  navigateHistoryForward: 'forward',
};

/** The trail direction a shared hotkey action id asks for, if any. */
export function navigationHistoryHotkeyDirection(actionId: string | undefined): NavigationHistoryDirection | undefined {
  return actionId ? NAVIGATION_HISTORY_HOTKEY_DIRECTIONS[actionId] : undefined;
}

function chordTextForEvent(event: KeyboardEvent): string | undefined {
  // `event.key` carries the layout-shifted character (Alt+[ is "“" on macOS),
  // so bracket-style chords are read from the physical code, matching how the
  // native gpui path reads charactersIgnoringModifiers.
  const key =
    event.code === 'BracketLeft'
      ? '['
      : event.code === 'BracketRight'
        ? ']'
        : event.key.length === 1
          ? event.key.toLowerCase()
          : undefined;
  if (!key) {
    return undefined;
  }
  const parts: string[] = [];
  if (event.metaKey) parts.push('cmd');
  if (event.ctrlKey) parts.push('ctrl');
  if (event.altKey) parts.push('alt');
  if (event.shiftKey) parts.push('shift');
  if (parts.length === 0) {
    // Never claim an unmodified key; the terminal and every text field own those.
    return undefined;
  }
  parts.push(key);
  return parts.join('+');
}

export type NavigationHistoryHotkeyOptions = {
  /** Current app settings, so a user's rebinding is honoured immediately. */
  readHotkeys(): ghostexHotkeySettings | undefined;
  navigate(direction: NavigationHistoryDirection): void;
};

/**
 * Listen for the configured Back/Forward chords on `window`. Returns the
 * unsubscribe function.
 *
 * Capture phase, on purpose: the focused surface is usually a terminal, and an
 * xterm that consumes the keystroke would otherwise decide whether the shortcut
 * exists. The claim is narrow — the handler resolves the chord first and only
 * takes the event when it maps to a navigation-history action, so every other
 * key reaches the terminal untouched. This mirrors what the gpui app does
 * natively with its own pre-dispatch capture.
 */
export function installNavigationHistoryHotkeys(options: NavigationHistoryHotkeyOptions): () => void {
  const handleKeyDown = (event: KeyboardEvent): void => {
    if (event.defaultPrevented || event.repeat) {
      return;
    }
    const chord = chordTextForEvent(event);
    if (!chord) {
      return;
    }
    const hotkeys = normalizeghostexHotkeySettings(options.readHotkeys());
    const direction = navigationHistoryHotkeyDirection(getghostexHotkeyActionIdForKey(hotkeys, chord));
    if (!direction) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    options.navigate(direction);
  };
  window.addEventListener('keydown', handleKeyDown, true);
  return () => {
    window.removeEventListener('keydown', handleKeyDown, true);
  };
}
