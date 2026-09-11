import type { SessionChatComposerKeyEvent } from './session-chat-composer';

export interface SessionChatCaretMovement {
  direction: 'left' | 'right' | 'up' | 'down';
  select: boolean;
  unit: 'character' | 'word' | 'line' | 'lineBoundary' | 'paragraph' | 'document';
}

/**
 * CDXC:SessionChat 2026-09-06 DECISION:
 * User: background Shift+Enter inserts a newline at the saved caret; arrows, including Option/Cmd arrows, restore input focus and move the caret unless a picker owns the key.
 * CDXC:SessionChat 2026-09-11 DECISION: User: the configurable Scroll Chat to Bottom shortcut (Ctrl+Shift+Down by default) takes priority over caret selection in chat.
 */
export function sessionChatCaretMovement(event: SessionChatComposerKeyEvent): SessionChatCaretMovement | null {
  if (event.isComposing || Number(event.altKey) + Number(event.ctrlKey) + Number(event.metaKey) > 1) {
    return null;
  }
  const direction = {
    ArrowLeft: 'left',
    ArrowRight: 'right',
    ArrowUp: 'up',
    ArrowDown: 'down',
  }[event.key] as SessionChatCaretMovement['direction'] | undefined;
  if (!direction) return null;
  const horizontal = direction === 'left' || direction === 'right';
  const unit = event.metaKey
    ? horizontal
      ? 'lineBoundary'
      : 'document'
    : event.altKey || event.ctrlKey
      ? horizontal
        ? 'word'
        : 'paragraph'
      : horizontal
        ? 'character'
        : 'line';
  return { direction, select: event.shiftKey, unit };
}

/** A popup keeps its keyboard ownership even if it has not moved DOM focus yet. */
export function sessionChatKeyboardPopupOpen(root: HTMLElement): boolean {
  return [
    ...root.ownerDocument.querySelectorAll<HTMLElement>(
      '[role="dialog"], [role="menu"], [role="listbox"], .quick-input-widget, .suggest-widget.visible'
    ),
  ].some(
    (popup) =>
      !popup.contains(root) &&
      !popup.closest('.ghostex-chat-composer-picker') &&
      popup.getClientRects().length > 0 &&
      getComputedStyle(popup).visibility !== 'hidden'
  );
}
