import type { SessionChatCaretMovement } from './session-chat-caret-navigation';
import type { SessionChatTextEditCommand } from './session-chat-edit-shortcuts';

/**
 * Backend-neutral key event: the Lexical input adapts the native key event
 * before the editor handles it.
 */
export interface SessionChatComposerKeyEvent {
  repeat?: boolean;
  altKey: boolean;
  code?: string;
  ctrlKey: boolean;
  isComposing: boolean;
  key: string;
  keyCode?: number;
  metaKey: boolean;
  shiftKey: boolean;
  preventDefault: () => void;
}

/**
 * Imperative surface of the prompt input. The host's draft stays the source of
 * truth; applyValue only synchronizes the visual input (and caret) after the
 * host has already updated the draft itself.
 */
export interface SessionChatComposerInputApi {
  applyValue: (next: string, caret: number) => void;
  setSelection?: (start: number, end: number) => void;
  focus: () => void;
  getSelection: () => { end: number; start: number };
  getValue: () => string;
  insertSavedPrompt: (text: string) => boolean;
  insertText: (text: string) => boolean;
  navigateCaret: (movement: SessionChatCaretMovement) => void;
  editText: (command: SessionChatTextEditCommand) => void;
  selectAll: () => void;
}
