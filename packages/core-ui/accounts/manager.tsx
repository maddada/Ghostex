import { normalizeAccountIndicatorInput } from '@/packages/shared/agent-accounts';
import { AccountIndicator } from './indicator';
import { AccountTitlebarStar } from './titlebar-star';
import { AccountConnectFlow } from './connect-flow';
import { accountSetupOwner } from './setup-monitor';
import { AccountPrivacyContext, AccountText, useAccountText, useHideAccountEmails } from './account-text';
import {
  useEffect,
  useId,
  useRef,
  useMemo,
  useState,
  useSyncExternalStore,
  type ReactNode,
  type RefObject,
} from 'react';
import { IconBook, IconChevronDown, IconInfoCircle, IconPlus, IconRefresh, IconX } from '@tabler/icons-react';
import { cn } from '@/packages/components/utils';
import { AppTooltip } from '../app-tooltip';
import { Button } from '@/packages/components/ui/button';
import { Switch } from '@/packages/components/ui/switch';
import { SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/packages/components/ui/select';
import {
  SettingButton,
  SettingRow,
  SettingsInput,
  SettingsListItem,
  SettingsSection,
  SettingsSelect,
  SettingsSelectContent,
} from '../settings-modal/fields';
import type {
  AccountProvider,
  AgentAccount,
  AgentAccountsRequest,
  AgentAccountsState,
} from '@/packages/shared/agent-accounts';
import {
  getAccountsConnections,
  getAccountsConnectionRevision,
  subscribeAccountsConnections,
  showAccountFlowToast,
} from './transport';
import { CopyCommand } from './copy-command';
import { AccountConnectionGuide } from './connection-guide';
import { useAccounts } from './use-accounts';
import { AccountIdentity, AccountLogo } from './controls';
import { PolicySettingRows } from './policy-setting-rows';
type Mutation = (request: AgentAccountsRequest) => Promise<boolean>;
const providerLabel = (provider: AccountProvider) => (provider === 'claude' ? 'Claude' : 'Codex');
const helperLabel = (provider: AccountProvider) => (provider === 'claude' ? 'Claude Swap' : 'Codex Swap');
/**
 * CDXC:Settings 2026-09-10 DECISION:
 * User: the Accounts page is organized like the General page. One Accounts section holds the page-wide rows, and each provider is its own section whose card lists the saved accounts as expandable management rows, with Connection guide and Add account as quiet header actions.
 * Refresh accounts is a quiet header action too, superseding the 2026-09-07 decision that made it a brighter standalone button above the list.
 */
export function AccountsSettingsSection({
  active,
  sectionRef,
  hideEmails,
  onHideEmailsChange,
}: {
  active: boolean;
  hideEmails: boolean;
  onHideEmailsChange: (hidden: boolean) => void;
  sectionRef?: RefObject<HTMLDivElement | null>;
}) {
  const connectionRevision = useSyncExternalStore(subscribeAccountsConnections, getAccountsConnectionRevision);
  const connections = useMemo(() => (active ? getAccountsConnections() : []), [active, connectionRevision]);
  const [selected, setSelected] = useState('');
  const computerId = useId();
  useEffect(() => {
    if (!active) return;
    let stopped = false;
    let lastCompleted = '';
    const poll = async () => {
      const results = await Promise.allSettled(
        connections.map(async (connection) => ({
          connection,
          jobs: (await connection.request({ operation: 'setupStatus', owner: accountSetupOwner() })).setupJobs ?? [],
        }))
      );
      if (stopped) return;
      const completed = results
        .flatMap((result) =>
          result.status === 'fulfilled'
            ? result.value.jobs
                .filter((job) => job.status === 'complete')
                .map((job) => ({ connection: result.value.connection, job }))
            : []
        )
        .sort((a, b) => a.job.createdAt - b.job.createdAt)
        .at(-1);
      if (completed && completed.job.id !== lastCompleted) {
        lastCompleted = completed.job.id;
        setSelected(completed.connection.id);
      }
    };
    void poll();
    const timer = setInterval(() => void poll(), 2500);
    return () => {
      stopped = true;
      clearInterval(timer);
    };
  }, [active, connections]);
  const connection = connections.find((c) => c.id === selected) ?? connections[0];
  // CDXC:Settings 2026-09-08 DECISION: Refresh accounts every time the Accounts page opens and show loading on the Refresh accounts button itself.
  const { data, error, busy, refreshing, request } = useAccounts(connection?.request, false, active, true);
  return (
    <AccountPrivacyContext value={hideEmails}>
      <SettingsSection
        actions={
          connection ? (
            <SettingButton
              aria-busy={refreshing}
              aria-live='polite'
              className='gx-account-refresh'
              disabled={busy}
              disabledReason='Accounts are being read.'
              onClick={async () => {
                if (await request({ operation: 'list', refresh: true })) {
                  showAccountFlowToast('Accounts refreshed', 'Saved accounts and usage are up to date.');
                }
              }}
              type='button'
              variant='ghost'
            >
              <IconRefresh aria-hidden='true' data-icon='inline-start' />
              {refreshing ? 'Refreshing…' : 'Refresh accounts'}
            </SettingButton>
          ) : null
        }
        /* CDXC:Settings 2026-09-09 DECISION: User: recommend adding even a single account so usage stats are easy to find in the titlebar and status lines. */
        description='Add your account to see usage and reset times in Ghostex, even if you only use one account. Star an account to show its stats in the titlebar; in chat context details, star Account limits to show usage in the status line.'
        sectionRef={sectionRef}
        title='Accounts'
      >
        <SettingRow
          description='Show only the first and last address characters and obscure the domain.'
          htmlFor='hide-account-emails'
          label='Hide emails'
        >
          <Switch id='hide-account-emails' checked={hideEmails} onCheckedChange={onHideEmailsChange} />
        </SettingRow>
        {connections.length > 1 ? (
          <SettingRow
            description='Accounts are managed on the computer where sessions run.'
            htmlFor={computerId}
            label='Computer'
          >
            <SettingsSelect
              items={connections.map((c) => ({ label: c.label, value: c.id }))}
              onValueChange={(value) => {
                if (value) setSelected(value);
              }}
              value={connection?.id}
            >
              <SelectTrigger aria-label='Computer' className='h-8 px-3' id={computerId}>
                <SelectValue />
              </SelectTrigger>
              <SettingsSelectContent className='settings-list-select-content'>
                <SelectGroup>
                  {connections.map((c) => (
                    <SelectItem value={c.id} key={c.id}>
                      {c.label}
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SettingsSelectContent>
            </SettingsSelect>
          </SettingRow>
        ) : null}
        {!connection ? (
          <SettingsListItem detail='Connect to a computer to manage its accounts.' title='No computer connected' />
        ) : null}
        {connection && error ? (
          <SettingsListItem detail={<AccountText text={error} />} status='warning' title='Accounts could not be read'>
            <Button
              onClick={() => void request({ operation: 'list', refresh: true })}
              size='sm'
              type='button'
              variant='outline'
            >
              Try again
            </Button>
          </SettingsListItem>
        ) : null}
        {connection && !data && !error ? (
          <SettingsListItem
            aria-live='polite'
            title={busy ? 'Reading saved accounts and usage…' : 'Account information is unavailable.'}
          />
        ) : null}
      </SettingsSection>
      {connection && data ? (
        <AccountManager key={connection.id} machineId={connection.id} data={data} busy={busy} request={request} />
      ) : null}
    </AccountPrivacyContext>
  );
}
/** CDXC:Settings 2026-09-09 DECISION: Accounts live on their own Settings > Accounts page, replacing the section under Agents; Claude uses cswap and Codex uses xswap. */
function AccountManager({
  data,
  busy,
  request,
  machineId,
}: {
  data: AgentAccountsState;
  busy: boolean;
  request: Mutation;
  machineId: string;
}) {
  const accountText = useAccountText();
  const [guide, setGuide] = useState<AccountProvider>();
  const [adding, setAdding] = useState<AccountProvider>();
  const [editing, setEditing] = useState<string>();
  const [defaultsOpen, setDefaultsOpen] = useState<AccountProvider>();
  const [highlighted, setHighlighted] = useState<string>();
  const [pendingJob, setPendingJob] = useState<import('@/packages/shared/agent-accounts').AccountSetupJob>();
  const completedJob = useRef('');
  useEffect(() => {
    let closed = false;
    const poll = async () => {
      try {
        const connection = getAccountsConnections().find((c) => c.id === machineId);
        const jobs =
          (await connection?.request({ operation: 'setupStatus', owner: accountSetupOwner() }))?.setupJobs ?? [];
        if (closed) return;
        setPendingJob(jobs.filter((job) => !['complete', 'failed'].includes(job.status)).at(-1));
        const complete = jobs.filter((job) => job.status === 'complete').at(-1);
        if (complete?.accountId && complete.id !== completedJob.current) {
          completedJob.current = complete.id;
          setHighlighted(complete.accountId);
          setAdding(undefined);
          setGuide(undefined);
          setEditing(complete.accountId);
          await request({ operation: 'list', refresh: true });
        }
      } catch {
        /* Keep the account error and retry controls owned by useAccounts. */
      }
    };
    void poll();
    const timer = setInterval(() => void poll(), 2000);
    return () => {
      closed = true;
      clearInterval(timer);
    };
  }, [machineId, request]);
  useEffect(() => {
    if (highlighted)
      document.getElementById(`account-${highlighted}`)?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }, [highlighted, data]);
  return (
    <>
      <AccountConnectionGuide
        provider={guide}
        helpers={data.helpers}
        machineId={machineId}
        busy={busy}
        onClose={() => setGuide(undefined)}
      />
      {(['claude', 'codex'] as const).map((provider) => {
        const label = providerLabel(provider);
        const accounts = data.accounts
          .filter((a) => a.provider === provider && a.registered)
          .sort((a, b) => Number(a.selector) - Number(b.selector));
        const defaultAccount = accounts.find((a) => a.id === data.defaultAccounts[provider]);
        const policy = data.defaults[provider];
        const defaultsPanelId = `account-defaults-${provider}`;
        const defaultsExpanded = defaultsOpen === provider;
        return (
          <SettingsSection
            actions={
              <>
                <Button
                  aria-label={`How to connect ${label}`}
                  onClick={() => setGuide(provider)}
                  type='button'
                  variant='ghost'
                >
                  <IconBook aria-hidden='true' data-icon='inline-start' />
                  Connection guide
                </Button>
                <Button
                  aria-expanded={adding === provider}
                  onClick={() => setAdding(adding === provider ? undefined : provider)}
                  type='button'
                  variant='ghost'
                >
                  <IconPlus aria-hidden='true' data-icon='inline-start' />
                  Add account
                </Button>
              </>
            }
            description={
              accounts.length === 0
                ? `${label} uses your current CLI login. No account switcher is needed to start sessions.`
                : `${accounts.length} saved ${accounts.length === 1 ? 'account' : 'accounts'}. Uses ${helperLabel(provider)}.`
            }
            key={provider}
            title={label}
          >
            {pendingJob && pendingJob.provider === provider && !adding && !editing ? (
              <div className='gx-account-inset'>
                <AccountConnectFlow machineId={machineId} provider={provider} initialJob={pendingJob} />
              </div>
            ) : null}
            {adding === provider ? (
              <AccountSetup
                key={provider}
                provider={provider}
                machineId={machineId}
                data={data}
                busy={busy}
                request={request}
                close={() => setAdding(undefined)}
              />
            ) : null}
            {accounts.length > 0 ? (
              <div className='settings-list-rows'>
                {accounts.map((account) => {
                  const isEditing = editing === account.id;
                  const panelId = `account-panel-${account.id}`;
                  const name = accountText(account.name);
                  return (
                    <div
                      key={account.id}
                      id={`account-${account.id}`}
                      data-highlighted={highlighted === account.id}
                      className='gx-account-saved'
                      data-editing={isEditing}
                    >
                      <div className='settings-management-row flex min-h-14 items-center gap-2 py-1.5'>
                        <Button
                          aria-controls={panelId}
                          aria-expanded={isEditing}
                          className='settings-management-edit-button h-auto min-w-0 flex-1 justify-start gap-3 px-2 py-2 text-left'
                          onClick={() => setEditing(isEditing ? undefined : account.id)}
                          type='button'
                          variant='ghost'
                        >
                          <AccountIdentity account={account} />
                          <span className='min-w-0 flex-1'>
                            <span className='block truncate text-sm font-medium text-foreground'>
                              <AccountText text={account.name} />
                            </span>
                            {account.email !== account.name || account.usageError ? (
                              <span className='block truncate text-[13px] font-normal text-muted-foreground'>
                                <AccountText text={account.usageError ?? (account.email || 'Saved login unavailable')} />
                              </span>
                            ) : null}
                          </span>
                        </Button>
                        {/* CDXC:AgentProviders 2026-09-09 DECISION: User: put the account-switching status before the titlebar star, label it Automatic or Manual, and explain the meaning through an adjacent info tooltip. */}
                        {account.status === 'ready' ? (
                          <span className='gx-account-status'>
                            {account.eligible ? 'Automatic' : 'Manual'}
                            <AppTooltip
                              content={
                                account.eligible
                                  ? 'When automatic account switching is enabled, Ghostex can switch to this account when another reaches its usage limit.'
                                  : 'Ghostex uses this account only when you select it. Automatic account switching skips it.'
                              }
                            >
                              <span
                                aria-label={`About ${account.eligible ? 'automatic' : 'manual'} account switching`}
                                className='gx-account-status-info'
                                role='img'
                              >
                                <IconInfoCircle aria-hidden='true' />
                              </span>
                            </AppTooltip>
                          </span>
                        ) : (
                          <Button onClick={() => setEditing(account.id)} size='sm' type='button' variant='outline'>
                            Reconnect
                          </Button>
                        )}
                        <AccountTitlebarStar account={account} busy={busy} request={request} machineId={machineId} />
                        <Button
                          aria-controls={panelId}
                          aria-expanded={isEditing}
                          aria-label={isEditing ? `Collapse ${name} details` : `Expand ${name} details`}
                          onClick={() => setEditing(isEditing ? undefined : account.id)}
                          size='icon-sm'
                          type='button'
                          variant='ghost'
                        >
                          <IconChevronDown
                            aria-hidden='true'
                            className={cn('transition-transform duration-150', isEditing && 'rotate-180')}
                          />
                        </Button>
                      </div>
                      {isEditing ? (
                        <div className='settings-list-panel border-t border-border/70' id={panelId}>
                          <AccountEditor
                            accounts={accounts}
                            account={account}
                            machineId={machineId}
                            busy={busy}
                            request={request}
                            close={() => setEditing(undefined)}
                          />
                        </div>
                      ) : null}
                    </div>
                  );
                })}
                <div>
                  <div className='settings-management-row flex min-h-14 items-center gap-2 py-1.5'>
                    <Button
                      aria-controls={defaultsPanelId}
                      aria-expanded={defaultsExpanded}
                      className='settings-management-edit-button h-auto min-w-0 flex-1 justify-start gap-3 px-2 py-2 text-left'
                      onClick={() => setDefaultsOpen(defaultsExpanded ? undefined : provider)}
                      type='button'
                      variant='ghost'
                    >
                      <span className='min-w-0 flex-1'>
                        <span className='block truncate text-sm font-medium text-foreground'>New session defaults</span>
                        <span className='gx-account-default-summary block truncate text-[13px] font-normal text-muted-foreground'>
                          {defaultAccount ? (
                            <AccountLogo provider={defaultAccount.provider} slot={defaultAccount.selector} />
                          ) : null}
                          <AccountText text={defaultAccount?.name ?? 'Choose an account'} /> ·{' '}
                          {policy.enabled
                            ? policy.atLimit === 'wait'
                              ? 'Wait for reset'
                              : 'Switch at a limit'
                            : 'Auto-continue off'}
                        </span>
                      </span>
                    </Button>
                    <Button
                      aria-controls={defaultsPanelId}
                      aria-expanded={defaultsExpanded}
                      aria-label={
                        defaultsExpanded
                          ? `Collapse ${label} new session defaults`
                          : `Expand ${label} new session defaults`
                      }
                      onClick={() => setDefaultsOpen(defaultsExpanded ? undefined : provider)}
                      size='icon-sm'
                      type='button'
                      variant='ghost'
                    >
                      <IconChevronDown
                        aria-hidden='true'
                        className={cn('transition-transform duration-150', defaultsExpanded && 'rotate-180')}
                      />
                    </Button>
                  </div>
                  {defaultsExpanded ? (
                    <div className='settings-list-panel border-t border-border/70' id={defaultsPanelId}>
                      <DefaultAccountRow
                        accounts={accounts}
                        busy={busy}
                        label={label}
                        onChange={(accountId) => void request({ operation: 'defaultAccount', provider, accountId })}
                        value={data.defaultAccounts[provider] ?? ''}
                      />
                      <PolicySettingRows
                        scope={`${provider} defaults`}
                        policy={policy}
                        disabled={busy}
                        onChange={(nextPolicy) => void request({ operation: 'defaults', provider, policy: nextPolicy })}
                      />
                    </div>
                  ) : null}
                </div>
              </div>
            ) : (
              <SettingsListItem
                detail='Optionally add your account to see usage and reset times in the titlebar and chat status lines.'
                title='Current CLI login'
              />
            )}
          </SettingsSection>
        );
      })}
    </>
  );
}

function DefaultAccountRow({
  accounts,
  busy,
  label,
  onChange,
  value,
}: {
  accounts: AgentAccount[];
  busy: boolean;
  label: string;
  onChange: (accountId: string | null) => void;
  value: string;
}) {
  const formatAccountText = useAccountText();
  const id = useId();
  return (
    <SettingRow
      description={`Quick launch starts new ${label} sessions with this saved account. Existing sessions keep their saved settings.`}
      htmlFor={id}
      label='Account for new sessions'
    >
      <SettingsSelect
        disabled={busy}
        disabledReason='Accounts are being updated.'
        items={accounts.map((account) => ({ label: <AccountText text={account.name} />, value: account.id }))}
        onValueChange={(next) => onChange(next || null)}
        value={value}
      >
        <SelectTrigger aria-label={`${label} account for new sessions`} className='h-8 px-3' id={id}>
          <SelectValue />
        </SelectTrigger>
        <SettingsSelectContent className='settings-list-select-content'>
          <SelectGroup>
            {accounts.map((account) => (
              <SelectItem key={account.id} value={account.id} label={formatAccountText(account.name)}>
                <AccountLogo provider={account.provider} slot={account.selector} />
                <AccountText text={account.name} />
              </SelectItem>
            ))}
          </SelectGroup>
        </SettingsSelectContent>
      </SettingsSelect>
    </SettingRow>
  );
}

function AccountEditor({
  accounts,
  account,
  machineId,
  busy,
  request,
  close,
}: {
  accounts: AgentAccount[];
  account: AgentAccount;
  machineId: string;
  busy: boolean;
  request: Mutation;
  close: () => void;
}) {
  const hideEmails = useHideAccountEmails();
  const formatAccountText = useAccountText();
  const id = useId();
  const [name, setName] = useState(account.name);
  const [indicator, setIndicator] = useState(account.indicator ?? '');
  const [eligible, setEligible] = useState(account.eligible);
  const [remove, setRemove] = useState(false);
  const [reconnect, setReconnect] = useState(account.status !== 'ready');
  const [swapTarget, setSwapTarget] = useState('');
  const sessionsNote = `${account.sessionCount} session${account.sessionCount === 1 ? '' : 's'} use this account.`;
  return (
    <>
      <SettingRow htmlFor={`${id}-name`} label='Account name'>
        <SettingsInput
          autoComplete='off'
          className='w-56'
          id={`${id}-name`}
          maxLength={80}
          onChange={(e) => setName(e.target.value)}
          type={hideEmails && name.includes('@') ? 'password' : 'text'}
          value={name}
        />
      </SettingRow>
      <SettingRow
        description={`Up to two letters or numbers, such as cw for Claude work. Enter - to hide the indicator, or leave blank to use slot ${account.selector}.`}
        htmlFor={`${id}-indicator`}
        label='Account indicator'
      >
        <SettingsInput
          autoComplete='off'
          className='w-24'
          id={`${id}-indicator`}
          maxLength={4}
          onChange={(event) => setIndicator(normalizeAccountIndicatorInput(event.target.value))}
          placeholder={account.selector}
          value={indicator}
        />
      </SettingRow>
      <SettingRow
        description='How sessions using this account appear in the sidebar.'
        htmlFor={`${id}-preview`}
        label='Session icon preview'
      >
        <span className='gx-account-preview-row' id={`${id}-preview`}>
          <span className='gx-account-mark' data-provider={account.provider}>
            <AccountLogo provider={account.provider} />
            <AccountIndicator value={indicator || account.selector} />
          </span>
          <span className='text-[13px] text-muted-foreground'>
            {providerLabel(account.provider)} · <AccountText text={name} />
          </span>
        </span>
      </SettingRow>
      <SettingRow
        description='When automatic account switching is enabled, Ghostex may switch to this account when another reaches its usage limit.'
        htmlFor={`${id}-eligible`}
        label='Available for automatic switching'
      >
        <Switch id={`${id}-eligible`} checked={eligible} onCheckedChange={setEligible} />
      </SettingRow>
      {accounts.length > 1 ? (
        <SettingRow
          description={
            account.provider === 'claude'
              ? 'Stop sessions using either account before swapping Claude slots.'
              : 'Exchange the slot numbers of two saved accounts.'
          }
          htmlFor={`${id}-swap`}
          label={`Swap slot ${account.selector} with`}
        >
          <SettingsSelect
            items={accounts
              .filter((other) => other.id !== account.id)
              .map((other) => ({ label: <AccountText text={`Slot ${other.selector} ${other.name}`} />, value: other.id }))}
            onValueChange={(value) => setSwapTarget(value ?? '')}
            value={swapTarget}
          >
            <SelectTrigger aria-label='Account to swap slots with' className='h-8 px-3' id={`${id}-swap`}>
              <SelectValue placeholder='Choose an account' />
            </SelectTrigger>
            <SettingsSelectContent className='settings-list-select-content'>
              <SelectGroup>
                {accounts
                  .filter((other) => other.id !== account.id)
                  .map((other) => (
                    <SelectItem key={other.id} value={other.id} label={formatAccountText(`Slot ${other.selector} ${other.name}`)}>
                      Slot {other.selector} · <AccountText text={other.name} />
                    </SelectItem>
                  ))}
              </SelectGroup>
            </SettingsSelectContent>
          </SettingsSelect>
          <Button
            disabled={busy || !swapTarget}
            onClick={() => void request({ operation: 'swapSlots', firstId: account.id, secondId: swapTarget })}
            size='sm'
            type='button'
            variant='outline'
          >
            Swap slots
          </Button>
        </SettingRow>
      ) : null}
      <SettingsListItem detail={sessionsNote} title='Account actions'>
        <div className='flex flex-wrap justify-end gap-2'>
          <Button onClick={() => setRemove(!remove)} size='sm' type='button' variant='ghost'>
            Remove
          </Button>
          <Button onClick={() => setReconnect(!reconnect)} size='sm' type='button' variant='ghost'>
            Sign in again
          </Button>
          <Button onClick={close} size='sm' type='button' variant='ghost'>
            Cancel
          </Button>
          <Button
            disabled={busy || !name.trim()}
            onClick={async () => {
              if (
                await request({ operation: 'update', id: account.id, name, color: account.color, eligible, indicator })
              )
                close();
            }}
            size='sm'
            type='button'
          >
            Save changes
          </Button>
        </div>
      </SettingsListItem>
      {reconnect ? (
        <div className='gx-account-inset'>
          <AccountConnectFlow machineId={machineId} provider={account.provider} account={account} />
        </div>
      ) : null}
      {remove ? (
        <SettingsListItem
          detail='The saved helper login and shared conversations remain. Sessions using this account need a different account before their next resume.'
          status='warning'
          title={
            <>
              Remove <AccountText text={account.name} /> from Ghostex?
            </>
          }
        >
          <Button
            disabled={busy}
            onClick={async () => {
              if (await request({ operation: 'remove', id: account.id })) close();
            }}
            size='sm'
            type='button'
            variant='destructive'
          >
            Remove from Ghostex
          </Button>
        </SettingsListItem>
      ) : null}
    </>
  );
}
/** CDXC:Settings 2026-09-07 DECISION: A saved account with missing credentials shows the reason and a Click to run login button directly in the account row. Repairing its login is available before consenting to shared conversations; adding it still requires that consent. */
function AccountSetup({
  provider,
  machineId,
  data,
  busy,
  request,
  close,
}: {
  provider: AccountProvider;
  machineId: string;
  data: AgentAccountsState;
  busy: boolean;
  request: Mutation;
  close: () => void;
}) {
  const formatAccountText = useAccountText();
  const id = useId();
  const [selected, setSelected] = useState(
    () =>
      data.accounts.find(
        (account) => account.provider === provider && !account.registered && account.status === 'ready'
      )?.id ?? 'new'
  );
  const [consent, setConsent] = useState(false);
  const helper = data.helpers.find((h) => h.provider === provider);
  const available = data.accounts.filter((a) => a.provider === provider && !a.registered);
  const account = available.find((a) => a.id === selected);
  const label = providerLabel(provider);
  let body: ReactNode;
  if (!helper?.installed) {
    body = helper ? (
      <>
        <SettingsListItem
          detail={`To connect your account for usage stats, install ${helperLabel(provider)} on this computer, then refresh accounts. This setup is optional; you can keep using your current CLI login.`}
          title={`Install ${helperLabel(provider)}`}
        />
        <div className='gx-account-inset'>
          <CopyCommand command={helper.installCommand} />
        </div>
      </>
    ) : null;
  } else {
    body = (
      <>
        {available.length > 0 ? (
          <SettingRow htmlFor={`${id}-login`} label='Login to add'>
            <SettingsSelect
              items={[
                { label: 'Sign in to a new account', value: 'new' },
                ...available.map((a) => ({ label: <AccountText text={a.name || a.email} />, value: a.id })),
              ]}
              onValueChange={(value) => setSelected(value ?? 'new')}
              value={selected}
            >
              <SelectTrigger aria-label='Login to add' className='h-8 px-3' id={`${id}-login`}>
                <SelectValue />
              </SelectTrigger>
              <SettingsSelectContent className='settings-list-select-content'>
                <SelectGroup>
                  <SelectItem value='new'>Sign in to a new account</SelectItem>
                  {available.map((a) => (
                    <SelectItem key={a.id} value={a.id} label={formatAccountText(a.name || a.email)}>
                      <AccountLogo provider={provider} slot={a.selector} />
                      <AccountText text={a.name || a.email} />
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SettingsSelectContent>
            </SettingsSelect>
          </SettingRow>
        ) : null}
        {account?.status === 'ready' ? (
          <>
            <SettingRow
              description='Required before Ghostex adds the account.'
              htmlFor={`${id}-consent`}
              label={`Share conversations between my ${label} accounts`}
            >
              <Switch checked={consent} id={`${id}-consent`} onCheckedChange={setConsent} />
            </SettingRow>
            <SettingsListItem title={<>Add <AccountText text={account.name || account.email} /></>}>
              <Button
                disabled={busy || !consent}
                onClick={async () => {
                  if (
                    await request({ operation: 'register', provider, selector: account.selector, shareHistory: true })
                  )
                    close();
                }}
                size='sm'
                type='button'
              >
                Add account
              </Button>
            </SettingsListItem>
          </>
        ) : (
          <div className='gx-account-inset'>
            <AccountConnectFlow key={selected} machineId={machineId} provider={provider} account={account} />
          </div>
        )}
      </>
    );
  }
  return (
    <div className='settings-list-group'>
      <SettingsListItem title={`Add a ${label} account`}>
        <Button aria-label='Close account setup' onClick={close} size='icon-sm' type='button' variant='ghost'>
          <IconX aria-hidden='true' />
        </Button>
      </SettingsListItem>
      {body}
    </div>
  );
}
