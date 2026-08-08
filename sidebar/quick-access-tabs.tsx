import { useEffect } from 'react';
import { openQuickAccess, type QuickAccessPage } from './app-modal-host-bridge';

export type QuickAccessTab = QuickAccessPage;

const QUICK_ACCESS_TABS = [
  { hotkey: '⌘1', id: 'commands', label: 'Command Pane' },
  { hotkey: '⌘2', id: 'recentProjects', label: 'Recent Projects' },
  { hotkey: '⌘3', id: 'recentSessions', label: 'Sessions' },
  { hotkey: '⌘4', id: 'savedPrompts', label: 'Saved Prompts' },
] as const satisfies ReadonlyArray<{
  hotkey: string;
  id: QuickAccessTab;
  label: string;
}>;

export function QuickAccessHeader({ activeTab }: { activeTab: QuickAccessTab }) {
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (!event.metaKey || event.altKey || event.ctrlKey || event.shiftKey) {
        return;
      }
      const tab = QUICK_ACCESS_TABS[Number(event.key) - 1]?.id;
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
          <kbd>{tab.hotkey}</kbd>
        </button>
      ))}
    </nav>
  );
}
