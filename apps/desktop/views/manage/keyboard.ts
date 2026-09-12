import { detectghostexHotkeyPlatform } from '@/packages/shared/ghostex-hotkeys';

/**
 * CDXC:Docs 2026-09-12 DECISION:
 * User: Cmd+F, and Ctrl+F on Windows, show the Docs search and put the caret in it.
 * The platform split is deliberate: macOS binds Ctrl+F to move the caret forward inside text fields, so claiming it there would break typing in the Markdown editor.
 * Cmd+Shift+F and Cmd+Alt+F already belong to app hotkeys, so any modifier beyond the platform's primary one means this is not the Docs find shortcut.
 * SEE-ALSO: packages/shared/ghostex-hotkeys.ts.
 */
export function isManageFindShortcut(event: KeyboardEvent): boolean {
  if (event.key.toLowerCase() !== 'f' || event.shiftKey || event.altKey) {
    return false;
  }
  return detectghostexHotkeyPlatform() === 'mac' ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
}
