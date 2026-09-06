import { useMemo, useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import { Switch } from '@/packages/components/ui/switch';
import { SettingsSection } from '../settings-modal/fields';
import type {
  AccountIconColor,
  AccountProvider,
  AgentAccount,
  AgentAccountsRequest,
  AgentAccountsState,
} from '@/packages/shared/agent-accounts';
import { getAccountsConnections } from './transport';
import { useAccounts } from './use-accounts';
import { AccountColorSelect, AccountIdentity, AccountLogo, PolicyControls } from './controls';
type Mutation = (request: AgentAccountsRequest) => Promise<boolean>;
export function AccountsSettingsSection({ active }: { active: boolean }) {
  const connections = useMemo(() => (active ? getAccountsConnections() : []), [active]);
  const [selected, setSelected] = useState('');
  const connection = connections.find((c) => c.id === selected) ?? connections[0];
  const { data, error, busy, request } = useAccounts(connection?.request, false, active);
  return (
    <SettingsSection title='Accounts'>
      <div className='gx-accounts'>
        {connections.length > 1 && (
          <label className='gx-account-field'>
            Computer
            <select value={connection?.id} onChange={(e) => setSelected(e.target.value)}>
              {connections.map((c) => (
                <option value={c.id} key={c.id}>
                  {c.label}
                </option>
              ))}
            </select>
          </label>
        )}
        {!connection ? (
          <p>Connect to a computer to manage its accounts.</p>
        ) : (
          <>
            {error && (
              <div className='gx-account-error' role='alert'>
                {error}
                <Button variant='ghost' onClick={() => void request({ operation: 'list', refresh: true })}>
                  Try again
                </Button>
              </div>
            )}
            {!data ? (
              <p aria-live='polite'>
                {busy ? 'Reading saved accounts and usage…' : 'Account information is unavailable.'}
              </p>
            ) : (
              <AccountManager key={connection.id} data={data} busy={busy} request={request} />
            )}
          </>
        )}
      </div>
    </SettingsSection>
  );
}
/** CDXC:Settings 2026-09-05 DECISION: Accounts live under Settings > Agents; Claude uses cswap and Codex uses xswap. */
export function AccountManager({
  data,
  busy,
  request,
}: {
  data: AgentAccountsState;
  busy: boolean;
  request: Mutation;
}) {
  const [adding, setAdding] = useState<AccountProvider>();
  const [editing, setEditing] = useState<string>();
  return (
    <div className='gx-accounts'>
      <div className='gx-account-heading'>
        <p>Manage the logins available on this computer.</p>
        <Button
          variant='outline'
          size='sm'
          disabled={busy}
          onClick={() => void request({ operation: 'list', refresh: true })}
        >
          Refresh
        </Button>
      </div>
      {(['claude', 'codex'] as const).map((provider) => (
        <section key={provider} className='gx-account-provider'>
          <div className='gx-account-heading'>
            <h3>{provider === 'claude' ? 'Claude' : 'Codex'}</h3>
            <Button variant='outline' size='sm' onClick={() => setAdding(adding === provider ? undefined : provider)}>
              Add account
            </Button>
          </div>
          <label className='gx-account-field'>
            Account for new sessions
            <select
              disabled={busy}
              value={data.defaultAccounts[provider] ?? ''}
              onChange={(e) =>
                void request({ operation: 'defaultAccount', provider, accountId: e.target.value || null })
              }
            >
              <option value=''>Default CLI login</option>
              {data.accounts
                .filter((a) => a.provider === provider && a.registered)
                .map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.name}
                  </option>
                ))}
            </select>
            <small>Default uses the CLI's ordinary login. Existing sessions keep their account.</small>
          </label>
          {data.accounts
            .filter((a) => a.provider === provider && a.registered)
            .map((account) => (
              <div key={account.id}>
                <div className='gx-account-row'>
                  <AccountIdentity account={account} />
                  <div className='gx-account-row-copy'>
                    <strong>{account.name}</strong>
                    <small>{account.email || 'Saved login unavailable'}</small>
                  </div>
                  <div className='gx-account-row-actions'>
                    <span className='gx-account-status'>
                      {account.status === 'ready' ? (account.eligible ? 'Available' : 'Manual only') : 'Reconnect'}
                    </span>
                    <Button
                      variant='ghost'
                      size='sm'
                      onClick={() => setEditing(editing === account.id ? undefined : account.id)}
                    >
                      Edit
                    </Button>
                  </div>
                </div>
                {editing === account.id && (
                  <AccountEditor account={account} busy={busy} request={request} close={() => setEditing(undefined)} />
                )}
              </div>
            ))}
          {!data.accounts.some((a) => a.provider === provider && a.registered) && (
            <p>Add a saved login to switch accounts while keeping the same conversation.</p>
          )}
          {adding === provider && (
            <AccountSetup
              key={provider}
              provider={provider}
              data={data}
              busy={busy}
              request={request}
              close={() => setAdding(undefined)}
            />
          )}
          <details className='gx-account-local-policy'>
            <summary>Keep going at a limit · New sessions</summary>
            <p>
              Choose how new {provider === 'claude' ? 'Claude' : 'Codex'} sessions recover. Existing sessions keep their
              saved defaults or overrides.
            </p>
            <PolicyControls
              scope={`${provider} defaults`}
              policy={data.defaults[provider]}
              disabled={busy}
              onChange={(policy) => void request({ operation: 'defaults', provider, policy })}
            />
          </details>
        </section>
      ))}
    </div>
  );
}
function AccountEditor({
  account,
  busy,
  request,
  close,
}: {
  account: AgentAccount;
  busy: boolean;
  request: Mutation;
  close: () => void;
}) {
  const [name, setName] = useState(account.name);
  const [color, setColor] = useState<AccountIconColor>(account.color);
  const [eligible, setEligible] = useState(account.eligible);
  const [remove, setRemove] = useState(false);
  const [reconnect, setReconnect] = useState(false);
  const command =
    account.provider === 'codex'
      ? `xswap login ${account.selector}`
      : `cswap run ${account.selector} --share-history --`;
  return (
    <div className='gx-account-editor'>
      <label className='gx-account-field'>
        Account name
        <input type='text' maxLength={80} value={name} onChange={(e) => setName(e.target.value)} />
      </label>
      <AccountColorSelect value={color} onChange={setColor} />
      <div className='gx-account-preview'>
        <span className='gx-account-color-preview-label'>Chat bar</span>
        <div>
          <AccountLogo provider={account.provider} color={color} />
          <span>
            {account.provider === 'codex' ? 'Codex' : 'Claude'} · {name}
          </span>
        </div>
        <span className='gx-account-color-preview-label'>Sidebar</span>
        <div>
          <AccountLogo provider={account.provider} color={color} />
          <span>Current task</span>
          <small>{name}</small>
        </div>
      </div>
      <div className='gx-account-field-row'>
        <label htmlFor={`eligible-${account.id}`}>Available for automatic switching</label>
        <Switch id={`eligible-${account.id}`} checked={eligible} onCheckedChange={setEligible} />
      </div>
      <p>
        {account.sessionCount} session{account.sessionCount === 1 ? '' : 's'} use this account. Conversations are shared
        within this provider.
      </p>
      <div className='gx-account-row-actions'>
        <Button
          disabled={busy || !name.trim()}
          onClick={async () => {
            if (await request({ operation: 'update', id: account.id, name, color, eligible })) close();
          }}
        >
          Save changes
        </Button>
        <Button variant='ghost' onClick={close}>
          Cancel
        </Button>
        <Button variant='ghost' onClick={() => setReconnect(!reconnect)}>
          Sign in again
        </Button>
        <Button variant='ghost' onClick={() => setRemove(!remove)}>
          Remove
        </Button>
      </div>
      {reconnect && (
        <div className='gx-account-setup'>
          <p>
            {account.provider === 'claude'
              ? 'Open this account in a terminal, then use /login to reconnect the intended login. Do not log out first.'
              : 'Finish active launches for this account, then sign in again using xswap.'}
          </p>
          <CopyCommand command={command} />
          <Button
            variant='outline'
            disabled={busy}
            onClick={() =>
              void request({
                operation: 'register',
                id: account.id,
                provider: account.provider,
                selector: account.selector,
                shareHistory: true,
              })
            }
          >
            Check connection
          </Button>
        </div>
      )}
      {remove && (
        <div className='gx-account-error'>
          <strong>Remove {account.name} from Ghostex?</strong>
          <p>
            The saved helper login and shared conversations remain. Sessions using this account need a different account
            before their next resume.
          </p>
          <Button
            variant='outline'
            disabled={busy}
            onClick={async () => {
              if (await request({ operation: 'remove', id: account.id })) close();
            }}
          >
            Remove from Ghostex
          </Button>
        </div>
      )}
    </div>
  );
}
function CopyCommand({ command }: { command: string }) {
  const [message, setMessage] = useState('');
  return (
    <>
      <code>{command}</code>
      <Button
        variant='outline'
        size='sm'
        onClick={() => {
          void navigator.clipboard.writeText(command).then(
            () => setMessage('Copied'),
            () => setMessage('Select the command and copy it.')
          );
        }}
      >
        Copy command
      </Button>
      <small role='status'> {message}</small>
    </>
  );
}
function AccountSetup({
  provider,
  data,
  busy,
  request,
  close,
}: {
  provider: AccountProvider;
  data: AgentAccountsState;
  busy: boolean;
  request: Mutation;
  close: () => void;
}) {
  const [consent, setConsent] = useState(false);
  const helper = data.helpers.find((h) => h.provider === provider);
  const available = data.accounts.filter((a) => a.provider === provider && !a.registered);
  return (
    <div className='gx-account-setup'>
      <div className='gx-account-heading'>
        <h3>Add a {provider === 'claude' ? 'Claude' : 'Codex'} account</h3>
        <Button variant='ghost' size='sm' onClick={close}>
          Close
        </Button>
      </div>
      {!helper?.installed && (
        <>
          <p>Install {provider === 'claude' ? 'claude-swap with uv' : 'xswap with Cargo'} on this computer first.</p>
          {helper && <CopyCommand command={helper.installCommand} />}
        </>
      )}
      {helper?.error && <p>{helper.error}</p>}
      {helper?.installed && (
        <>
          <label className='gx-account-consent'>
            <input type='checkbox' checked={consent} onChange={(e) => setConsent(e.target.checked)} />
            <span>
              Share conversations between my {provider === 'claude' ? 'Claude' : 'Codex'} accounts so sessions can
              resume with another login.
            </span>
          </label>
          {available.map((a) => (
            <div className='gx-account-row' key={a.id}>
              <AccountLogo provider={provider} />
              <div className='gx-account-row-copy'>
                <strong>{a.name || `Account ${a.selector}`}</strong>
                <small>{a.email}</small>
              </div>
              <Button
                variant='outline'
                size='sm'
                disabled={busy || !consent || a.status !== 'ready'}
                onClick={() =>
                  void request({ operation: 'register', provider, selector: a.selector, shareHistory: true })
                }
              >
                Add saved login
              </Button>
            </div>
          ))}
          <p>
            To add another login, run this command in a terminal on this computer, complete sign-in, then refresh the
            saved logins.
          </p>
          {provider === 'claude' && (
            <p>Sign in without logging out first, so the previous saved login remains usable.</p>
          )}
          <CopyCommand command={helper.loginCommand} />
        </>
      )}
      <Button
        variant='outline'
        size='sm'
        disabled={busy}
        onClick={() => void request({ operation: 'list', refresh: true })}
      >
        {busy ? 'Checking…' : 'Refresh saved logins'}
      </Button>
    </div>
  );
}
