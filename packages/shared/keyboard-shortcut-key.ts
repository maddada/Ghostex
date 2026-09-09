/**
 * CDXC:Hotkeys 2026-09-10 DECISION:
 * User: Command-V and the other shortcuts must keep working when the keyboard is set to Arabic or any other language, on macOS, Linux and Windows, as the best long-term solution rather than a per-language patch.
 *
 * CDXC:Hotkeys 2026-09-10 WHY:
 * `KeyboardEvent.key` is the character the key types, so with an Arabic layout Cmd+V arrives as "ر" and a check against "v" fails; a Shift chord can even arrive as ASCII punctuation ("{" for Shift+V on Arabic PC).
 * For letter keys, `KeyboardEvent.keyCode` is the Latin letter the operating system assigns to the key: the printed letter on Latin layouts (AZERTY, Dvorak, QWERTZ) and the QWERTY letter on non-Latin layouts, identically in Chromium on macOS, Windows and Linux. It is what native menu shortcuts match and how VS Code dispatches keybindings.
 * `KeyboardEvent.code` is the physical position and is only a last resort: it is wrong for every layout that moves letters, so a code-only rule would break AZERTY and Dvorak users.
 * Plain typing never goes through this helper; callers use it only after checking a modifier, so it never changes what a key types.
 * SEE-ALSO: apps/desktop/native/macos/GpuiKeyboardShortcuts.m applies the same definition to AppKit events for the desktop app's native routing; packages/shared/ghostex-hotkeys.ts builds configured hotkey text from it.
 */

export type ShortcutKeyboardEvent = Pick<KeyboardEvent, 'key'> & Partial<Pick<KeyboardEvent, 'code' | 'keyCode'>>;

const SHORTCUT_SYMBOLS_BY_CODE: Readonly<Record<string, string>> = {
  Backquote: '`',
  Backslash: '\\',
  BracketLeft: '[',
  BracketRight: ']',
  Comma: ',',
  Equal: '=',
  Minus: '-',
  Period: '.',
  Quote: "'",
  Semicolon: ';',
  Slash: '/',
};

/**
 * The Latin letter (lowercase) or digit the operating system assigns to the
 * pressed key, independent of the active keyboard language, or undefined when
 * the key is neither.
 */
export function shortcutLetterOrDigitFromKeyboardEvent(event: ShortcutKeyboardEvent): string | undefined {
  const keyCode = event.keyCode ?? 0;
  if (keyCode >= 65 && keyCode <= 90) {
    return String.fromCharCode(keyCode + 32);
  }
  const key = event.key;
  if (key.length === 1) {
    const charCode = key.charCodeAt(0);
    if ((charCode >= 65 && charCode <= 90) || (charCode >= 97 && charCode <= 122)) {
      return key.toLowerCase();
    }
    if (charCode >= 48 && charCode <= 57) {
      return key;
    }
  }
  const code = event.code ?? '';
  const digitMatch = /^(?:Digit|Numpad)([0-9])$/u.exec(code);
  if (digitMatch) {
    return digitMatch[1];
  }
  // Only reached without a usable keyCode (synthetic events): the physical
  // position is the last remaining signal.
  const letterMatch = /^Key([A-Z])$/u.exec(code);
  if (letterMatch) {
    return letterMatch[1]?.toLowerCase();
  }
  return undefined;
}

/**
 * The lowercase key to compare a modifier chord against: the Latin letter or
 * digit identity of the key when it has one, else the printed ASCII character,
 * else the US symbol of the physical key (an Arabic layout types letters on the
 * bracket keys), else `event.key` lowercased.
 */
export function shortcutKeyFromKeyboardEvent(event: ShortcutKeyboardEvent): string {
  const letterOrDigit = shortcutLetterOrDigitFromKeyboardEvent(event);
  if (letterOrDigit) {
    return letterOrDigit;
  }
  const key = event.key;
  if (key.length === 1 && key.charCodeAt(0) > 0x20 && key.charCodeAt(0) < 0x7f) {
    return key.toLowerCase();
  }
  return SHORTCUT_SYMBOLS_BY_CODE[event.code ?? ''] ?? key.toLowerCase();
}
