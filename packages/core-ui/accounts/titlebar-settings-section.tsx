import { useMemo, useSyncExternalStore } from 'react';
import { Button } from '@/packages/components/ui/button';
import { SettingsListItem, SettingsSection } from '../settings-modal/fields';
import { AccountPrivacyContext, AccountText } from './account-text';
import { AccountIdentity } from './controls';
import { AccountTitlebarStar } from './titlebar-star';
import {
  getAccountsConnections,
  getAccountsConnectionRevision,
  subscribeAccountsConnections,
  type AccountsConnection,
} from './transport';
import { useAccounts } from './use-accounts';

/** CDXC:Extensions 2026-09-10 DECISION: User: the Extensions page also controls which accounts show usage in the GPUI titlebar, keeping the existing controls on the Accounts page. */
export function TitlebarAccountUsageSection({ active, hideEmails }: { active: boolean; hideEmails: boolean }) {
  const revision = useSyncExternalStore(subscribeAccountsConnections, getAccountsConnectionRevision);
  const connections = useMemo(() => (active ? getAccountsConnections() : []), [active, revision]);
  return (
    <AccountPrivacyContext value={hideEmails}>
      <SettingsSection
        title='Titlebar account usage'
        description='Star the accounts whose usage you want to see in the desktop titlebar. These are the same stars as in Settings > Accounts.'
      >
        {connections.length === 0 ? (
          <SettingsListItem title='No computer connected' detail='Connect to a computer to choose its accounts.' />
        ) : (
          connections.map((connection) => (
            <TitlebarAccountUsageList
              key={connection.id}
              connection={connection}
              showComputer={connections.length > 1}
            />
          ))
        )}
      </SettingsSection>
    </AccountPrivacyContext>
  );
}

function TitlebarAccountUsageList({
  connection,
  showComputer,
}: {
  connection: AccountsConnection;
  showComputer: boolean;
}) {
  const { data, error, busy, request } = useAccounts(connection.request);
  const accounts = data?.accounts
    .filter((account) => account.registered)
    .sort((a, b) => a.provider.localeCompare(b.provider) || Number(a.selector) - Number(b.selector));
  return (
    <>
      {showComputer ? <div className='settings-list-group-label'>{connection.label}</div> : null}
      {error ? (
        <SettingsListItem title='Accounts could not be read' detail={<AccountText text={error} />} status='warning'>
          <Button
            disabled={busy}
            onClick={() => void request({ operation: 'list' })}
            size='sm'
            type='button'
            variant='outline'
          >
            Try again
          </Button>
        </SettingsListItem>
      ) : null}
      {!data && !error ? <SettingsListItem aria-live='polite' title='Reading saved accounts…' /> : null}
      {accounts?.length === 0 ? (
        <SettingsListItem
          title='No saved accounts'
          detail='Add a Claude or Codex account in Settings > Accounts to show its usage here.'
        />
      ) : null}
      {accounts?.length ? (
        <div className='settings-list-rows'>
          {accounts.map((account) => (
            <div key={account.id} className='extensions-row flex min-h-14 items-center gap-3 py-2'>
              <AccountIdentity account={account} />
              <div className='min-w-0 flex-1'>
                <span className='block truncate text-sm font-normal text-foreground'>
                  <AccountText text={account.name} />
                </span>
                <span className='block truncate text-[13px] font-normal text-muted-foreground'>
                  {account.provider === 'claude' ? 'Claude' : 'Codex'}
                  {account.email && account.email !== account.name ? (
                    <>
                      {' '}
                      · <AccountText text={account.email} />
                    </>
                  ) : null}
                </span>
              </div>
              <AccountTitlebarStar account={account} busy={busy} request={request} machineId={connection.id} />
            </div>
          ))}
        </div>
      ) : null}
    </>
  );
}
