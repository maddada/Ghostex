import { useEffect, useMemo } from 'react';
import {
  getghostexHotkeyActionIdForKey,
  ghostexHotkeyTextFromKeyboardEvent,
  normalizeghostexHotkeySettings,
  type ghostexHotkeyActionId,
} from '../shared/ghostex-hotkeys';
import { openQuickAccess, type QuickAccessPage } from './app-modal-host-bridge';
import { formatSidebarHotkeyLabel } from './hotkey-label';
import { useSidebarStore } from './sidebar-store';

export type QuickAccessTab = QuickAccessPage;

const QUICK_ACCESS_TABS = [
  { hotkey: 'cmd+1', id: 'commands', label: 'Command Pane' },
  { hotkey: 'cmd+2', id: 'recentProjects', label: 'Projects' },
  { hotkey: 'cmd+3', id: 'recentSessions', label: 'Sessions' },
  { hotkey: 'cmd+4', id: 'savedPrompts', label: 'Saved Prompts' },
] as const satisfies ReadonlyArray<{
  hotkey: string;
  id: QuickAccessTab;
  label: string;
}>;

const QUICK_ACCESS_HOTKEY_ACTION_TABS: Partial<Record<ghostexHotkeyActionId, QuickAccessTab>> = {
  openCommandPalette: 'commands',
  openSessionSearchPalette: 'recentSessions',
  stashedPrompts: 'savedPrompts',
};

export function QuickAccessHeader({ activeTab }: { activeTab: QuickAccessTab }) {
  const hotkeys = useSidebarStore((state) => state.hud.settings?.hotkeys);
  const normalizedHotkeys = useMemo(() => normalizeghostexHotkeySettings(hotkeys), [hotkeys]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const hotkey = ghostexHotkeyTextFromKeyboardEvent(event);
      if (!hotkey) {
        return;
      }
      const actionId = getghostexHotkeyActionIdForKey(normalizedHotkeys, hotkey);
      const tab =
        QUICK_ACCESS_TABS.find((candidate) => candidate.hotkey === hotkey)?.id ??
        (actionId ? QUICK_ACCESS_HOTKEY_ACTION_TABS[actionId] : undefined);
      if (!tab) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      openQuickAccess(tab);
    };
    document.addEventListener('keydown', handleKeyDown, true);
    return () => document.removeEventListener('keydown', handleKeyDown, true);
  }, [normalizedHotkeys]);

  return (
    <nav aria-label='Ghostex Quick Access sections' className='quick-access-tabs'>
      {QUICK_ACCESS_TABS.map((tab) => (
        <button
          aria-current={tab.id === activeTab ? 'page' : undefined}
          className='quick-access-tab'
          data-active={String(tab.id === activeTab)}
          key={tab.id}
          onClick={() => openQuickAccess(tab.id)}
          type='button'
        >
          <span>{tab.label}</span>
          <kbd>{formatSidebarHotkeyLabel(tab.hotkey)}</kbd>
        </button>
      ))}
    </nav>
  );
}
