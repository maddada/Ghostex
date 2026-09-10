import { IconChevronLeft, IconTerminal2, IconUser, IconWorld } from '@tabler/icons-react';
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
} from '@/packages/components/ui/command';
import { quickLaunchAccountId, type AccountsTransport } from '@/packages/shared/agent-accounts';
import type { SidebarAgentButton } from '@/packages/shared/sidebar-agents';
import { AccountText, useAccountText } from './accounts/account-text';
import { AccountLauncherUsage, launcherProvider } from './accounts/agent-launcher-menu';
import { AccountLogo } from './accounts/controls';
import { useAccounts } from './accounts/use-accounts';
import { AgentMenuChatIndicator } from './agent-menu-chat-indicator';
import { openAppModal } from './app-modal-host-bridge';
import { readPrimaryAgentLauncherId, writePrimaryAgentLauncherId } from './primary-agent-launcher';
import { ProjectAgentLauncherIcon } from './project-agent-launcher-icon';
import type { WebviewApi } from './webview-api';

const AGENT_VALUE_PREFIX = 'agent:';
const ACCOUNT_VALUE_PREFIX = 'account:';
const BROWSER_VALUE = 'browser';
const TERMINAL_VALUE = 'terminal';
const CLI_LOGIN_VALUE = 'cli-login';
const ADD_ACCOUNT_VALUE = 'add-account';
const RETRY_VALUE = 'retry';

/**
 * CDXC:AgentLauncher 2026-09-09 DECISION:
 * User: the New Thread hotkey opens a borderless picker for the active project that mirrors the project-header agent dropdown: every agent with its account count and chat badge, the last-used agent preselected at the top, Browser and Terminal at the end so they can be typed to, arrow keys to move, typing to filter, and Tab on Claude or Codex to open that provider's account list with the same keyboard filtering.
 * Launches go through the same `runSidebarAgent` selector the dropdown posts, so hook checks and account policy stay in the sidebar runtime. The last-used agent is the dropdown's own localStorage key, which the modal host shares with the sidebar page.
 * SEE-ALSO: packages/core-ui/accounts/agent-launcher-menu.tsx, packages/shared/ghostex-hotkeys.ts (openNewThreadPalette), apps/desktop/src/app/delayed_send.rs (runSidebarAgent forwarding).
 */
export function NewThreadPalette({
  agents,
  isOpen,
  onOpenChange,
  openRequestSequence,
  showBrowser = true,
  transport,
  vscode,
}: {
  agents: readonly SidebarAgentButton[];
  isOpen: boolean;
  /** The web app has no Browser pane, so it hides that row. */
  showBrowser?: boolean;
  onOpenChange: (isOpen: boolean) => void;
  openRequestSequence: number;
  transport?: AccountsTransport;
  vscode: WebviewApi;
}) {
  const accountText = useAccountText();
  const [query, setQuery] = useState('');
  const [value, setValue] = useState('');
  const [accountsAgent, setAccountsAgent] = useState<SidebarAgentButton>();
  const scopeChipRef = useRef<HTMLButtonElement>(null);
  const [scopeChipWidth, setScopeChipWidth] = useState(0);
  useLayoutEffect(() => {
    setScopeChipWidth(scopeChipRef.current?.offsetWidth ?? 0);
  }, [accountsAgent]);
  const { data, error, busy, load, request } = useAccounts(transport, false, isOpen);

  const orderedAgents = useMemo(() => {
    const primaryId = readPrimaryAgentLauncherId();
    const primary = agents.find((agent) => agent.agentId === primaryId);
    return primary ? [primary, ...agents.filter((agent) => agent !== primary)] : [...agents];
    // The primary agent is re-read on every open so a launch from the sidebar
    // in between reorders the next palette.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [agents, openRequestSequence]);

  const normalizedQuery = query.trim().toLowerCase();
  const visibleAgents = useMemo(
    () => orderedAgents.filter((agent) => matchesQuery(agent.name, normalizedQuery)),
    [orderedAgents, normalizedQuery]
  );
  const showBrowserRow = showBrowser && matchesQuery('Browser', normalizedQuery);
  const showTerminal = matchesQuery('Terminal', normalizedQuery);

  const provider = accountsAgent ? launcherProvider(accountsAgent) : null;
  const providerAccounts = useMemo(
    () => data?.accounts.filter((account) => account.registered && account.provider === provider) ?? [],
    [data, provider]
  );
  const visibleAccounts = useMemo(
    () => providerAccounts.filter((account) => matchesQuery(account.name, normalizedQuery)),
    [providerAccounts, normalizedQuery]
  );

  const agentValues = useMemo(() => {
    const values = visibleAgents.map((agent) => `${AGENT_VALUE_PREFIX}${agent.agentId}`);
    if (showBrowserRow) values.push(BROWSER_VALUE);
    if (showTerminal) values.push(TERMINAL_VALUE);
    return values;
  }, [visibleAgents, showBrowserRow, showTerminal]);
  const accountValues = useMemo(() => {
    if (!accountsAgent) return [];
    if (data && !providerAccounts.length) return [CLI_LOGIN_VALUE, ADD_ACCOUNT_VALUE];
    const values = visibleAccounts
      .filter((account) => account.status === 'ready')
      .map((account) => `${ACCOUNT_VALUE_PREFIX}${account.id}`);
    if (error) values.push(RETRY_VALUE);
    return values;
  }, [accountsAgent, data, providerAccounts.length, visibleAccounts, error]);
  const values = accountsAgent ? accountValues : agentValues;

  const orderedAgentsRef = useRef(orderedAgents);
  orderedAgentsRef.current = orderedAgents;
  useEffect(() => {
    if (!isOpen) return;
    const first = orderedAgentsRef.current[0];
    setQuery('');
    setAccountsAgent(undefined);
    setValue(first ? `${AGENT_VALUE_PREFIX}${first.agentId}` : TERMINAL_VALUE);
  }, [isOpen, openRequestSequence]);

  useEffect(() => {
    if (!values.includes(value)) {
      setValue(values[0] ?? '');
    }
  }, [values, value]);

  const close = () => onOpenChange(false);

  const run = (agent: SidebarAgentButton, accountId?: string) => {
    writePrimaryAgentLauncherId(agent.agentId);
    close();
    vscode.postMessage({ agentId: agent.agentId, accountId, type: 'runSidebarAgent' });
  };

  const enterAccounts = (agent: SidebarAgentButton) => {
    setAccountsAgent(agent);
    setQuery('');
  };

  const leaveAccounts = () => {
    const agent = accountsAgent;
    setAccountsAgent(undefined);
    setQuery('');
    if (agent) setValue(`${AGENT_VALUE_PREFIX}${agent.agentId}`);
  };

  const launchAgent = async (agent: SidebarAgentButton) => {
    const family = launcherProvider(agent);
    if (!family || !transport) {
      run(agent);
      return;
    }
    const accountState = data ?? (await load({ operation: 'list' }));
    if (!accountState) {
      enterAccounts(agent);
      return;
    }
    run(agent, quickLaunchAccountId(accountState, family));
  };

  const selectedAgent = value.startsWith(AGENT_VALUE_PREFIX)
    ? visibleAgents.find((agent) => `${AGENT_VALUE_PREFIX}${agent.agentId}` === value)
    : undefined;

  return (
    <CommandDialog
      className='ghostex-settings-shadcn new-thread-palette-dialog'
      description='Start a new agent, browser, or terminal thread in the active project.'
      open={isOpen}
      showCloseButton={false}
      title='Ghostex New Thread'
      onOpenChange={onOpenChange}
    >
      <Command
        className='quick-access-surface new-thread-palette'
        loop
        shouldFilter={false}
        value={value}
        onValueChange={setValue}
      >
        <div
          className='new-thread-palette-search'
          data-scope={accountsAgent ? 'accounts' : 'agents'}
          style={{ '--new-thread-scope-width': `${scopeChipWidth}px` } as CSSProperties}
        >
          {accountsAgent ? (
            <button
              aria-label={`Back to agents`}
              className='new-thread-palette-scope'
              ref={scopeChipRef}
              tabIndex={-1}
              type='button'
              onClick={leaveAccounts}
            >
              <IconChevronLeft aria-hidden='true' size={12} />
              <ProjectAgentLauncherIcon agent={accountsAgent} colorMode='brand' />
              <span>{accountsAgent.name}</span>
            </button>
          ) : null}
          <CommandInput
            autoFocus
            className='pl-3'
            clearLabel='Clear search'
            clearOnEscape={false}
            placeholder={accountsAgent ? 'Search accounts...' : 'Search agents, browser, terminal...'}
            value={query}
            onKeyDown={(event) => {
              if (event.key === 'Escape') {
                event.preventDefault();
                event.stopPropagation();
                if (accountsAgent) leaveAccounts();
                else close();
                return;
              }
              if (accountsAgent) {
                if (event.key === 'ArrowLeft' || (event.key === 'Backspace' && query === '')) {
                  event.preventDefault();
                  leaveAccounts();
                }
                return;
              }
              if ((event.key === 'Tab' && !event.shiftKey) || event.key === 'ArrowRight') {
                if (selectedAgent && launcherProvider(selectedAgent)) {
                  event.preventDefault();
                  enterAccounts(selectedAgent);
                } else if (event.key === 'Tab') {
                  event.preventDefault();
                }
              }
            }}
            onValueChange={setQuery}
          />
        </div>
        <div aria-hidden='true' className='new-thread-palette-hints'>
          <span>
            <kbd>↑</kbd>
            <kbd>↓</kbd> Move
          </span>
          <span>
            <kbd>↵</kbd> Start
          </span>
          {accountsAgent ? (
            <span>
              <kbd>←</kbd> Back
            </span>
          ) : (
            <span>
              <kbd>⇥</kbd> Accounts
            </span>
          )}
          <span>
            <kbd>esc</kbd> {accountsAgent ? 'Back' : 'Close'}
          </span>
        </div>
        <CommandList className='new-thread-palette-list'>
          {accountsAgent && provider ? (
            <>
              {visibleAccounts.map((account) => (
                <CommandItem
                  key={account.id}
                  className='new-thread-palette-row new-thread-palette-account-row'
                  disabled={account.status !== 'ready'}
                  title={
                    account.status !== 'ready' ? 'Reconnect this account in Settings first.' : accountText(account.name)
                  }
                  value={`${ACCOUNT_VALUE_PREFIX}${account.id}`}
                  onSelect={() => run(accountsAgent, account.id)}
                >
                  <AccountLogo provider={provider} slot={account.selector} />
                  <span className='gx-account-launcher-copy'>
                    <span className='gx-account-launcher-heading'>
                      <span className='group-agent-menu-label'>
                        <AccountText text={account.name} />
                      </span>
                      {account.id === data?.defaultAccounts[provider] && (
                        <span className='gx-account-launcher-default'>· Default</span>
                      )}
                    </span>
                    <AccountLauncherUsage account={account} />
                  </span>
                </CommandItem>
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
                  <CommandItem
                    className='new-thread-palette-row'
                    value={RETRY_VALUE}
                    onSelect={() => void request({ operation: 'list', refresh: true })}
                  >
                    <span className='group-agent-menu-label'>Try again</span>
                  </CommandItem>
                </>
              )}
              {!transport && <p className='gx-account-launcher-hint'>Account connection unavailable.</p>}
              {data && !providerAccounts.length && (
                <>
                  <CommandItem
                    className='new-thread-palette-row'
                    value={CLI_LOGIN_VALUE}
                    onSelect={() => run(accountsAgent)}
                  >
                    <ProjectAgentLauncherIcon agent={accountsAgent} colorMode='brand' />
                    <span className='group-agent-menu-label'>Current CLI login</span>
                  </CommandItem>
                  <p className='gx-account-launcher-hint'>
                    Uses your existing CLI sign-in. No account switcher needed.
                  </p>
                  <CommandSeparator />
                  <p className='gx-account-launcher-hint'>Add your account to see usage and reset times in Ghostex.</p>
                  <CommandItem
                    className='new-thread-palette-row'
                    value={ADD_ACCOUNT_VALUE}
                    onSelect={() => {
                      close();
                      openAppModal({ initialTab: 'accounts', modal: 'settings', type: 'open' });
                    }}
                  >
                    <span className='group-agent-menu-label'>Add account</span>
                  </CommandItem>
                </>
              )}
              {data && providerAccounts.length > 0 && !visibleAccounts.length && (
                <CommandEmpty>No accounts found.</CommandEmpty>
              )}
            </>
          ) : (
            <>
              {visibleAgents.map((agent) => {
                const family = launcherProvider(agent);
                return (
                  <CommandItem
                    key={agent.agentId}
                    className='new-thread-palette-row'
                    value={`${AGENT_VALUE_PREFIX}${agent.agentId}`}
                    onSelect={() => void launchAgent(agent)}
                  >
                    <ProjectAgentLauncherIcon agent={agent} colorMode='brand' />
                    <span className='group-agent-menu-label'>{agent.name}</span>
                    {family ? (
                      <button
                        aria-label='Accounts'
                        className='group-agent-menu-accounts group-agent-menu-account-button new-thread-palette-accounts'
                        tabIndex={-1}
                        title='Tab: choose account'
                        type='button'
                        onClick={(event) => {
                          event.stopPropagation();
                          enterAccounts(agent);
                        }}
                      >
                        <IconUser aria-hidden='true' size={14} stroke={1.8} />
                        {data ? (
                          <span className='group-agent-menu-accounts-count'>
                            {
                              data.accounts.filter((account) => account.registered && account.provider === family)
                                .length
                            }
                          </span>
                        ) : null}
                      </button>
                    ) : null}
                    <AgentMenuChatIndicator agent={agent} />
                  </CommandItem>
                );
              })}
              {visibleAgents.length > 0 && (showBrowserRow || showTerminal) && <CommandSeparator />}
              {showBrowserRow && (
                <CommandItem
                  className='new-thread-palette-row'
                  value={BROWSER_VALUE}
                  onSelect={() => {
                    close();
                    vscode.postMessage({ type: 'openBrowserPaneInGroup' });
                  }}
                >
                  <IconWorld aria-hidden='true' className='new-thread-palette-glyph' size={14} stroke={1.8} />
                  <span className='group-agent-menu-label'>Browser</span>
                </CommandItem>
              )}
              {showTerminal && (
                <CommandItem
                  className='new-thread-palette-row'
                  value={TERMINAL_VALUE}
                  onSelect={() => {
                    close();
                    vscode.postMessage({ type: 'createSession' });
                  }}
                >
                  <IconTerminal2 aria-hidden='true' className='new-thread-palette-glyph' size={14} stroke={1.8} />
                  <span className='group-agent-menu-label'>Terminal</span>
                </CommandItem>
              )}
              {!agentValues.length && <CommandEmpty>Nothing matches.</CommandEmpty>}
            </>
          )}
        </CommandList>
      </Command>
    </CommandDialog>
  );
}

function matchesQuery(text: string, normalizedQuery: string): boolean {
  if (!normalizedQuery) return true;
  const haystack = text.toLowerCase();
  if (haystack.includes(normalizedQuery)) return true;
  let index = 0;
  for (const char of normalizedQuery) {
    index = haystack.indexOf(char, index);
    if (index === -1) return false;
    index += 1;
  }
  return true;
}
