import { detectghostexHotkeyPlatform } from '@/packages/shared/ghostex-hotkeys';
import { shortcutKeyFromKeyboardEvent } from '@/packages/shared/keyboard-shortcut-key';
import type { SessionChatComposerKeyEvent } from './session-chat-composer';

export type SessionChatTextEditCommand =
  | 'undo'
  | 'redo'
  | 'deleteAllLeft'
  | 'deleteAllRight'
  | 'deleteWordLeft'
  | 'deleteWordRight'
  | 'killLineLeft'
  | 'killLineRight'
  | 'yank'
  | 'lineStart'
  | 'lineEnd';

/**
 * CDXC:SessionChat 2026-09-08 DECISION:
 * User: Ctrl+U, Ctrl+K, Ctrl+Y, Ctrl+E, and Ctrl+A behave like the terminal in the chat input.
 * Use logical line boundaries, retain killed text for yank, and keep Cmd+A as select-all.
 */
export function sessionChatTerminalShortcut(event: SessionChatComposerKeyEvent): SessionChatTextEditCommand | null {
  if (event.isComposing || !event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return null;
  switch (shortcutKeyFromKeyboardEvent(event)) {
    case 'u':
      return 'killLineLeft';
    case 'k':
      return 'killLineRight';
    case 'y':
      return 'yank';
    case 'a':
      return 'lineStart';
    case 'e':
      return 'lineEnd';
    default:
      return null;
  }
}

export function sessionChatBreaksKillSequence(event: SessionChatComposerKeyEvent): boolean {
  return !['Control', 'Shift', 'Alt', 'Meta', 'CapsLock'].includes(event.key) && !sessionChatTerminalShortcut(event);
}

type SessionChatEditingShortcut = SessionChatTextEditCommand | 'selectAll' | 'copy' | 'cut' | 'paste';

/**
 * CDXC:SessionChat 2026-09-07 DECISION:
 * User: Cmd+A and other text-editing shortcuts, including undo/redo, act on the composer even when only the chat background is focused.
 * Match editing chords explicitly so app shortcuts and other controls keep their own keyboard ownership.
 */
export function sessionChatEditingShortcut(event: SessionChatComposerKeyEvent): SessionChatEditingShortcut | null {
  if (event.isComposing) return null;
  const terminal = sessionChatTerminalShortcut(event);
  if (terminal) return terminal;
  const mac = detectghostexHotkeyPlatform() === 'mac';
  const primary = mac ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
  if ((event.key === 'Backspace' || event.key === 'Delete') && !event.shiftKey) {
    const backward = event.key === 'Backspace';
    if (mac && primary && !event.altKey) return backward ? 'deleteAllLeft' : 'deleteAllRight';
    const word = mac ? event.altKey && !event.metaKey && !event.ctrlKey : primary && !event.altKey;
    if (word) return backward ? 'deleteWordLeft' : 'deleteWordRight';
  }
  if (!primary || event.altKey) return null;
  const key = shortcutKeyFromKeyboardEvent(event);
  if (key === 'z') return event.shiftKey ? 'redo' : 'undo';
  if (key === 'v') return 'paste';
  if (event.shiftKey) return null;
  switch (key) {
    case 'a':
      return 'selectAll';
    case 'c':
      return 'copy';
    case 'x':
      return 'cut';
    case 'y':
      return 'redo';
    default:
      return null;
  }
}

export function sessionChatHasTranscriptSelection(root: HTMLElement): boolean {
  const selection = root.ownerDocument.getSelection();
  if (!selection || selection.isCollapsed || selection.rangeCount === 0) return false;
  const node = selection.getRangeAt(0).commonAncestorContainer;
  const element = node instanceof Element ? node : node.parentElement;
  return root.contains(node) && !element?.closest('.ghostex-chat-composer');
}
