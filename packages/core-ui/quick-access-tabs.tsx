import { useEffect } from 'react';
import { ghostexHotkeyTextFromKeyboardEvent } from '../shared/ghostex-hotkeys';
import { openQuickAccess, type QuickAccessPage } from './app-modal-host-bridge';
import { formatSidebarHotkeyLabel } from './hotkey-label';

export type QuickAccessTab = QuickAccessPage;

const QUICK_ACCESS_TABS = [
  { hotkey: 'cmd+1', id: 'commands', label: 'Command Pane' },
  { hotkey: 'cmd+2', id: 'recentProjects', label: 'Recent Projects' },
  { hotkey: 'cmd+3', id: 'recentSessions', label: 'Sessions' },
  { hotkey: 'cmd+4', id: 'savedPrompts', label: 'Saved Prompts' },
] as const satisfies ReadonlyArray<{
  hotkey: string;
  id: QuickAccessTab;
  label: string;
}>;

export function QuickAccessHeader({ activeTab }: { activeTab: QuickAccessTab }) {
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const hotkey = ghostexHotkeyTextFromKeyboardEvent(event);
      const tab = QUICK_ACCESS_TABS.find((candidate) => candidate.hotkey === hotkey)?.id;
      if (!tab) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      openQuickAccess(tab);
    };
    document.addEventListener('keydown', handleKeyDown, true);
    return () => document.removeEventListener('keydown', handleKeyDown, true);
  }, []);

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
