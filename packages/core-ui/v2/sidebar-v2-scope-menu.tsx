import { IconChevronDown, IconFolder } from '@tabler/icons-react';
import { useState, type MouseEvent as ReactMouseEvent } from 'react';
import { SidebarContextMenuPortal } from '../sidebar-context-menu-portal';
import type { WebviewApi } from '../webview-api';
import { SidebarV2ProjectIcon } from './sidebar-v2-icons';
import type { SidebarV2ScopeOption } from './sidebar-v2-view-model';

/*
 * CDXC:SidebarV2 2026-07-29:
 * Scoping is a menu, not a row of chips:
 * filtering the inbox must not make the sidebar header's width depend on how
 * many projects exist or how long their names are. The trigger states the
 * current scope, the menu owns the list.
 *
 * The menu reuses the shared sidebar context-menu portal so V2 inherits the
 * same dismissal contract as every other sidebar menu: Escape, click-away,
 * window blur, and the native outside-click bridge.
 */

export type SidebarV2ScopeMenuProps = {
  onSelectScope: (scopeId: string) => void;
  options: readonly SidebarV2ScopeOption[];
  scopeId: string;
  showProjectIcons: boolean;
  vscode: WebviewApi;
};

export function SidebarV2ScopeMenu({
  onSelectScope,
  options,
  scopeId,
  showProjectIcons,
  vscode,
}: SidebarV2ScopeMenuProps) {
  const [menuPosition, setMenuPosition] = useState<{ left: number; top: number; width: number }>();
  const activeOption = options.find((option) => option.scopeId === scopeId) ?? options[0];

  const openMenu = (event: ReactMouseEvent<HTMLButtonElement>) => {
    const bounds = event.currentTarget.getBoundingClientRect();
    setMenuPosition({ left: bounds.left, top: bounds.bottom + 4, width: bounds.width });
  };

  return (
    <>
      <button
        aria-expanded={menuPosition !== undefined}
        aria-haspopup='menu'
        aria-label='Filter sessions by project'
        className='sidebar-v2-scope-trigger'
        onClick={openMenu}
        type='button'
      >
        {showProjectIcons && (activeOption?.groupId === null || activeOption === undefined) ? (
          <IconFolder aria-hidden='true' className='sidebar-v2-project-icon' size={16} stroke={1.8} />
        ) : showProjectIcons && activeOption ? (
          <SidebarV2ProjectIcon
            discoveredIconDataUrl={activeOption.discoveredIconDataUrl}
            fallback={activeOption.isWorktree ? 'worktree' : 'folder'}
            icon={activeOption.icon}
            iconDataUrl={activeOption.iconDataUrl}
            title={activeOption.label}
          />
        ) : null}
        <span className='sidebar-v2-scope-trigger-label'>{activeOption?.label ?? 'All projects'}</span>
        <IconChevronDown aria-hidden='true' size={16} stroke={1.8} />
      </button>
      {menuPosition ? (
        <SidebarContextMenuPortal
          menuClassName='session-context-menu sidebar-v2-scope-menu'
          menuStyle={{
            left: `${menuPosition.left}px`,
            minWidth: `${menuPosition.width}px`,
            top: `${menuPosition.top}px`,
          }}
          onDismiss={() => setMenuPosition(undefined)}
          vscode={vscode}
        >
          <div className='session-context-menu-section' role='none'>
            {options.map((option) => (
              <button
                aria-checked={option.scopeId === scopeId}
                className='session-context-menu-item sidebar-v2-scope-menu-item'
                key={option.scopeId}
                onClick={() => {
                  setMenuPosition(undefined);
                  onSelectScope(option.scopeId);
                }}
                role='menuitemradio'
                type='button'
              >
                {showProjectIcons && option.groupId === null ? (
                  <IconFolder aria-hidden='true' className='sidebar-v2-project-icon' size={16} stroke={1.8} />
                ) : showProjectIcons ? (
                  <SidebarV2ProjectIcon
                    discoveredIconDataUrl={option.discoveredIconDataUrl}
                    fallback={option.isWorktree ? 'worktree' : 'folder'}
                    icon={option.icon}
                    iconDataUrl={option.iconDataUrl}
                    title={option.label}
                  />
                ) : null}
                <span className='sidebar-v2-scope-menu-label'>{option.label}</span>
                <span className='sidebar-v2-scope-menu-count'>{option.count}</span>
              </button>
            ))}
          </div>
        </SidebarContextMenuPortal>
      ) : null}
    </>
  );
}
