import { shortcutKeyFromKeyboardEvent } from '@/packages/shared/keyboard-shortcut-key';
import type { SidebarGroupRecord } from './sidebar-store';

export function getQuickAccessSessionProjectId(group: SidebarGroupRecord | undefined): string | undefined {
  const remote = group?.remoteMachineContext;
  return remote?.projectId
    ? `remote:${remote.machineId}:project:${remote.projectId}`
    : group?.projectContext?.editor.projectId;
}

export const SESSIONS_SCOPE_TOGGLE_HOTKEY = 'alt+c';

export function isQuickAccessSessionScopeHotkey(event: KeyboardEvent): boolean {
  return (
    event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey && shortcutKeyFromKeyboardEvent(event) === 'c'
  );
}

export function isReservedQuickAccessSessionScopeHotkey(event: KeyboardEvent): boolean {
  return isQuickAccessSessionScopeHotkey(event) && document.querySelector('.previous-sessions-modal') !== null;
}
