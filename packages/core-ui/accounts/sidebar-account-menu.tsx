import { openAppModal } from '../app-modal-host-bridge';
import { AccountLogo } from './controls';
import { AccountText, useAccountText } from './account-text';
import { IconCheck, IconSettings } from '@tabler/icons-react';
import { useLayoutEffect, useMemo, useRef, useState } from 'react';
import { getClampedSidebarContextMenuCoordinate } from '../sidebar-context-menu-portal';
import type { WebviewApi } from '../webview-api';
import { useAccounts } from './use-accounts';

/** CDXC:AgentProviders 2026-09-06 DECISION: Sidebar Switch Account lists only saved Claude or Codex account names for the session's provider. Selecting one uses the account service; agent configurations and the full chat account panel do not belong in this submenu. */
export function SidebarAccountMenu({
  sessionId,
  requestAccounts,
  position,
  working,
  close,
}: {
  sessionId: string;
  requestAccounts: NonNullable<WebviewApi['requestSessionAccounts']>;
  position: { x: number; y: number };
  working: boolean;
  close: () => void;
}) {
  const accountText = useAccountText();
  const transport = useMemo(() => requestAccounts.bind(null, sessionId), [requestAccounts, sessionId]);
  const { data, error, busy, request } = useAccounts(transport, true);
  const menuRef = useRef<HTMLDivElement>(null);
  const [top, setTop] = useState(position.y);
  const [left, setLeft] = useState(position.x);
  const accounts = data?.accounts.filter((account) => account.registered && account.provider === data.session?.provider);
  useLayoutEffect(() => {
    const menu = menuRef.current;
    if (menu) {
      const bounds = menu.getBoundingClientRect();
      setTop(getClampedSidebarContextMenuCoordinate(position.y, bounds.height, window.innerHeight));
      setLeft(getClampedSidebarContextMenuCoordinate(position.x, bounds.width, window.innerWidth));
    }
  }, [position.x, position.y, data, error]);
  return (
    <div
      ref={menuRef}
      aria-label='Switch Account'
      className='session-context-menu session-tag-submenu session-saved-account-submenu'
      data-empty-space-blocking='true'
      onClick={(event) => event.stopPropagation()}
      role='menu'
      style={{ left, top, zIndex: 'var(--sidebar-context-menu-submenu-z-index, 301)' }}
      onKeyDown={(event) => {
        if (event.key === 'Escape') {
          event.stopPropagation();
          close();
        }
        if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
        event.preventDefault();
        event.stopPropagation();
        const buttons = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('button:not(:disabled)'));
        if (buttons.length === 0) return;
        const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
        const next = event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1 :
          (index + (event.key === 'ArrowDown' ? 1 : -1) + buttons.length) % buttons.length;
        buttons[next]?.focus();
      }}
    >
      <div className='session-context-menu-section'>
        {error && (
          <>
            <p className='session-saved-account-message' role='alert'><AccountText text={error} /></p>
            <button className='session-context-menu-item' role='menuitem' disabled={busy}
              onClick={() => void request({ operation: 'session', refresh: true })}>Try again</button>
          </>
        )}
        {!data && !error && <p className='session-saved-account-message' role='status'>Loading accounts…</p>}
        {data && accounts?.length === 0 && <p className='session-saved-account-message'>No saved accounts.</p>}
        {accounts?.map((account) => {
          const current = account.id === data?.session?.accountId;
          return (
            <button
              key={account.id}
              className='session-context-menu-item'
              role='menuitemradio'
              aria-checked={current}
              disabled={busy || working || current || account.status !== 'ready'}
              title={working ? 'Stop the active turn before switching accounts.' : account.status !== 'ready' ? 'Reconnect this account in Settings.' : accountText(account.name)}
              onClick={async () => {
                if (await request({ operation: 'select', accountId: account.id })) close();
              }}
              type='button'
            >
              <AccountLogo provider={account.provider} slot={account.selector} />
              <span className='session-tag-menu-item-label'><AccountText text={account.name} /></span>
              {current && <IconCheck aria-hidden='true' className='session-context-menu-trailing-icon' size={16} />}
            </button>
          );
        })}
      </div>
      {/* CDXC:AgentProviders 2026-09-07 DECISION: Put Manage accounts beneath a separator in the sidebar account submenu; it opens the Accounts section in Settings. */}
      <div role='separator' className='session-context-menu-divider' />
      <button type='button' className='session-context-menu-item' role='menuitem' onClick={() => {
        openAppModal({ type: 'open', modal: 'settings', initialTab: 'accounts' });
        close();
      }}><IconSettings aria-hidden='true' size={16} /><span>Manage accounts</span></button>
    </div>
  );
}
