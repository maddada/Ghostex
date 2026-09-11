import { useEffect, useState, type RefObject } from 'react';
import {
  ghostexHotkeyTextFromKeyboardEvent,
  normalizeghostexHotkeySettings,
  type ghostexHotkeySettings,
} from '@/packages/shared/ghostex-hotkeys';
import { formatSidebarHotkeyLabel } from '../hotkey-label';

/** CDXC:SessionChat 2026-09-11 WHY:
 * Capture before the composer so the configured chat command also works while editing. Each pane owns only its focused chat; a shared browser window must not scroll every conversation.
 * SEE-ALSO: ghostex-hotkeys.ts owns the default; the desktop host supplies live settings and leaves this command to the chat page.
 */
export function useSessionChatScrollToBottom(
  rootRef: RefObject<HTMLDivElement | null>,
  hotkeys?: ghostexHotkeySettings
) {
  const [request, setRequest] = useState(0);
  const shortcut = normalizeghostexHotkeySettings(hotkeys).scrollChatToBottom ?? '';
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent): void => {
      if (event.isComposing || !shortcut || ghostexHotkeyTextFromKeyboardEvent(event) !== shortcut) return;
      const root = rootRef.current;
      if (!root?.getClientRects().length || root.closest('[aria-hidden="true"]')) return;
      if (document.querySelector('[data-hotkey-recorder][data-recording="true"]')) return;
      const target = event.target;
      if (!(target instanceof Node)) return;
      if (!root.contains(target)) {
        if (target !== document.body) return;
        const focusedPane = document.querySelector('.workspace-pane--focused');
        if (
          focusedPane
            ? !focusedPane.contains(root)
            : document.querySelectorAll('.ghostex-session-chat-scope').length !== 1
        )
          return;
      }
      event.preventDefault();
      event.stopImmediatePropagation();
      if (!event.repeat) setRequest((current) => current + 1);
    };
    window.addEventListener('keydown', handleKeyDown, true);
    return () => window.removeEventListener('keydown', handleKeyDown, true);
  }, [rootRef, shortcut]);
  return { request, label: formatSidebarHotkeyLabel(shortcut) };
}
