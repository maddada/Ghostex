import { openAppModal } from '../app-modal-host-bridge';
import { AccountText, useAccountText } from './account-text';
import { useLayoutEffect, useRef, useState } from 'react';
import { IconChevronLeft, IconSettings, IconUser } from '@tabler/icons-react';
import { accountUsageLabel } from '@/packages/shared/account-usage-label';
import {
  quickLaunchAccountId,
  type AgentAccount,
  type AccountsTransport,
} from '@/packages/shared/agent-accounts';
import type { SidebarAgentButton } from '@/packages/shared/sidebar-agents';
import { ProjectAgentLauncherIcon } from '../project-agent-launcher-icon';
import { AgentMenuChatIndicator } from '../agent-menu-chat-indicator';
import { AccountLogo } from './controls';
import { useAccounts } from './use-accounts';

/** CDXC:AgentLauncher 2026-09-09 DECISION: Claude and Codex, including custom agents, offer an account submenu only from the profile/count sub-button; the main row launches immediately. Mark the actual saved account with Default instead of a separate Default account row. An account choice applies to this launch only; quick launch uses the saved default and its provider policy. User: with no accounts added for that provider, use Current CLI login; hide that choice once an account is added, replacing the earlier Add accounts gate. Account management lives in Settings > Accounts. */
export function AgentLauncherMenuItems({
  agents,
  primaryAgentId,
  transport,
  onRun,
  onConfigure,
}: {
  agents: readonly SidebarAgentButton[];
  primaryAgentId?: string;
  transport?: AccountsTransport;
  onRun: (agent: SidebarAgentButton, accountId?: string) => void;
  onConfigure: () => void;
}) {
  const accountText = useAccountText();
  const [selected, setSelected] = useState<SidebarAgentButton>();
  const { data, error, busy, load, request } = useAccounts(transport);
  const root = useRef<HTMLDivElement>(null);
  const provider = selected ? launcherProvider(selected) : null;
  const accounts = data?.accounts.filter((account) => account.registered && account.provider === provider) ?? [];
  useLayoutEffect(() => {
    root.current?.querySelector<HTMLButtonElement>('[role="menuitem"]:not(:disabled)')?.focus();
  }, [selected]);
  const rowClass = 'session-context-menu-item group-control-menu-item group-agent-menu-item';
  return (
    <div
      ref={root}
      onKeyDown={(event) => {
        if (selected && (event.key === 'ArrowLeft' || event.key === 'Escape')) {
          event.preventDefault();
          event.stopPropagation();
          setSelected(undefined);
          return;
        }
        if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
        const rows = Array.from(
          root.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)') ?? []
        );
        const index = rows.indexOf(document.activeElement as HTMLButtonElement);
        const next =
          event.key === 'Home'
            ? 0
            : event.key === 'End'
              ? rows.length - 1
              : (index + (event.key === 'ArrowDown' ? 1 : -1) + rows.length) % rows.length;
        event.preventDefault();
        rows[next]?.focus();
      }}
    >
      {selected && (provider === 'claude' || provider === 'codex') ? (
        <>
          <button className={rowClass} role='menuitem' onClick={() => setSelected(undefined)}>
            <IconChevronLeft size={14} aria-hidden='true' />
            <span className='group-agent-menu-label'>{selected.name}</span>
          </button>
          <div role='group' aria-label={`${selected.name} accounts`}>
            {accounts.map((account) => (
              <button
                key={account.id}
                className={rowClass}
                role='menuitem'
                disabled={account.status !== 'ready'}
                title={account.status !== 'ready' ? 'Reconnect this account in Settings first.' : accountText(account.name)}
                onClick={() => onRun(selected, account.id)}
              >
                <AccountLogo provider={provider} slot={account.selector} />
                <span className='gx-account-launcher-copy'>
                  <span className='gx-account-launcher-heading'>
                    <span className='group-agent-menu-label'><AccountText text={account.name} /></span>
                    {account.id === data?.defaultAccounts[provider] && (
                      <span className='gx-account-launcher-default'>· Default</span>
                    )}
                  </span>
                  <AccountLauncherUsage account={account} />
                </span>
              </button>
            ))}
            {busy && !data && (
              <p className='gx-account-launcher-hint' role='status'>
                Reading accounts…
              </p>
            )}
            {error && (
              <>
                <p className='gx-account-launcher-hint' role='alert'>
                  <AccountText text={error} />
                </p>
                <button
                  className={rowClass}
                  role='menuitem'
                  onClick={() => void request({ operation: 'list', refresh: true })}
                >
                  Try again
                </button>
              </>
            )}
            {!transport && <p className='gx-account-launcher-hint'>Account connection unavailable.</p>}
            {data && !accounts.length && (
              <>
                <button className={rowClass} role='menuitem' onClick={() => onRun(selected)}>
                  <ProjectAgentLauncherIcon agent={selected} colorMode='brand' />
                  <span className='group-agent-menu-label'>Current CLI login</span>
                </button>
                <p className='gx-account-launcher-hint'>Uses your existing CLI sign-in. No account switcher needed.</p>
                <div className='session-context-menu-divider' role='separator' />
                <p className='gx-account-launcher-hint'>Add your account to see usage and reset times in Ghostex.</p>
                <button className={rowClass} role='menuitem' onClick={() => openAppModal({ type: 'open', modal: 'settings', initialTab: 'accounts' })}>
                  Add account
                </button>
              </>
            )}
          </div>
        </>
      ) : (
        <>
          {agents.map((agent) => {
            const family = launcherProvider(agent);
            const hasAccounts = family === 'claude' || family === 'codex';
            return (
              <div
                key={agent.agentId}
                className='group-agent-menu-row'
                data-selected={String(primaryAgentId === agent.agentId)}
              >
                <button
                  aria-label={`Start ${agent.name}`}
                  className={`${rowClass} group-agent-menu-launch`}
                  role='menuitem'
                  onKeyDown={(event) => {
                    if (hasAccounts && event.key === 'ArrowRight') {
                      event.preventDefault();
                      setSelected(agent);
                    }
                  }}
                  onClick={async () => {
                    if (!hasAccounts || !transport) {
                      onRun(agent);
                      return;
                    }
                    const accountState = data ?? (await load({ operation: 'list' }));
                    if (!accountState) {
                      setSelected(agent);
                      return;
                    }
                    onRun(agent, quickLaunchAccountId(accountState, family));
                  }}
                >
                  <ProjectAgentLauncherIcon agent={agent} colorMode='brand' />
                  <span className='group-agent-menu-label'>{agent.name}</span>
                </button>
                {hasAccounts ? (
                  <AgentMenuAccountsHint
                    count={
                      data?.accounts.filter((account) => account.registered && account.provider === family).length ?? 0
                    }
                    ready={Boolean(data)}
                    onOpen={() => setSelected(agent)}
                  />
                ) : null}
                <AgentMenuChatIndicator agent={agent} />
              </div>
            );
          })}
          {agents.length > 0 && <div className='session-context-menu-divider' role='separator' />}
          <button className={rowClass} role='menuitem' onClick={onConfigure}>
            <IconSettings aria-hidden='true' className='session-context-menu-icon' size={14} />
            <span className='group-agent-menu-label'>Configure</span>
          </button>
        </>
      )}
    </div>
  );
}

export function launcherProvider(agent: SidebarAgentButton) {
  const family = agent.icon ?? agent.agentId;
  return family === 'claude' || family === 'codex' ? family : null;
}

/** CDXC:AgentLauncher 2026-09-09 DECISION: User: account-picker rows show Claude's weekly and five-hour usage, and Codex's weekly usage and available resets, using the same account data as starred titlebar buttons. */
export function AccountLauncherUsage({ account }: { account: AgentAccount }) {
  const mainWindows = account.usage.filter((window) => !window.model);
  const weekly = mainWindows.find(
    (window) => window.id === 'sevenDay' || (window.limitWindowSeconds ?? 0) >= 604800
  );
  const fiveHour = mainWindows.find(
    (window) => window.id === 'fiveHour' || window.limitWindowSeconds === 18000
  );
  const values = account.provider === 'claude'
    ? [weekly, fiveHour].map((window) => window ? `${accountUsageLabel(window)}: ${Math.round(window.usedPercent)}%` : null)
    : [
        weekly ? `${accountUsageLabel(weekly)}: ${Math.round(weekly.usedPercent)}%` : null,
        account.resetCredits != null ? `${account.resetCredits}rs` : null,
      ];
  const label = values.filter((value): value is string => value !== null).join(' · ');
  return label ? <span className='gx-account-launcher-usage'>{label}</span> : null;
}

function AgentMenuAccountsHint({ count, ready, onOpen }: { count: number; ready: boolean; onOpen: () => void }) {
  return (
    <button
      aria-label={ready ? `${count} ${count === 1 ? 'account' : 'accounts'}` : 'Accounts'}
      aria-haspopup='menu'
      className='group-agent-menu-accounts group-agent-menu-account-button'
      role='menuitem'
      onKeyDown={(event) => {
        if (event.key === 'ArrowRight') {
          event.preventDefault();
          onOpen();
        }
      }}
      onClick={onOpen}
    >
      <IconUser aria-hidden='true' size={14} stroke={1.8} />
      {ready ? <span className='group-agent-menu-accounts-count'>{count}</span> : null}
    </button>
  );
}
