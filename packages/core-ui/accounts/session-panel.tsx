import { AccountText } from './account-text';
import { IconAdjustmentsHorizontal, IconArrowBackUp, IconRefresh, IconSettings } from '@tabler/icons-react';
import { AppTooltip } from '../app-tooltip';
import type { SessionChatContextUsage } from '@/packages/shared/session-chat';
import {
  resolveSessionChatContextMeterUsage,
  formatSessionChatContextTokens,
} from '../chat/session-chat-context-meter';
import { useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import type { AgentAccount, AgentAccountsRequest, AgentAccountsState } from '@/packages/shared/agent-accounts';
import { openAppModal } from '../app-modal-host-bridge';
import { AccountIdentity, PolicyControls, UsageBars, accountFigureWindows, resetsLine } from './controls';
/** CDXC:Settings 2026-09-09 DECISION: Account management and provider defaults belong in Settings > Accounts, replacing the section under Agents. The chat panel keeps session switching and recovery controls; its settings icon opens Accounts. Use menu dismissal instead of a Close button, omit the heading subtitle, make Refresh an unframed icon, and keep session controls flat instead of inside a card. */
export function SessionAccountsPanel({
  data,
  error,
  busy,
  request,
  close,
  contextUsage,
}: {
  contextUsage?: SessionChatContextUsage;
  data?: AgentAccountsState;
  error: string;
  busy: boolean;
  request: (p: AgentAccountsRequest) => Promise<boolean>;
  close: () => void;
}) {
  const [customize, setCustomize] = useState(false);
  const context = resolveSessionChatContextMeterUsage(contextUsage);
  const session = data?.session;
  const current = data?.accounts.find((a) => a.id === session?.accountId);
  const manageAccounts = () => {
    openAppModal({
      type: 'open',
      modal: 'settings',
      initialTab: 'accounts',
    });
    close();
  };
  // CDXC:AgentProviders 2026-09-09 DECISION: Current CLI login remains available until an account is added for this provider, replacing the earlier saved-account gate.
  if (data && session && !data.accounts.some((a) => a.registered && a.provider === session.provider)) {
    return (
      <div className='gx-accounts gx-account-panel'>
        <strong>Current CLI login</strong>
        <p>Add your account to see usage and reset times in Ghostex, even if you only use one account.</p>
        <Button variant='outline' onClick={manageAccounts}>
          Add account
        </Button>
      </div>
    );
  }
  return (
    <div className='gx-accounts gx-account-panel'>
      <div className='gx-account-heading'>
        <h3>Accounts &amp; limits</h3>
        <div className='gx-account-row-actions'>
          <AppTooltip content='Refresh accounts and usage'>
            <Button
              aria-label='Refresh accounts and usage'
              variant='ghost'
              size='icon-sm'
              disabled={busy}
              onClick={() => void request({ operation: 'session', refresh: true })}
            >
              <IconRefresh aria-hidden='true' />
            </Button>
          </AppTooltip>
          <AppTooltip content='Manage accounts in Settings'>
            <Button aria-label='Manage accounts in Settings' variant='ghost' size='icon-sm' onClick={manageAccounts}>
              <IconSettings aria-hidden='true' />
            </Button>
          </AppTooltip>
        </div>
      </div>
      {error && (
        <div className='gx-account-error' role='alert'>
          <AccountText text={error} />
        </div>
      )}
      {!data ? (
        <p aria-live='polite'>{busy ? 'Reading accounts and usage…' : 'Could not load accounts.'}</p>
      ) : !session ? (
        <p>Account management supports Claude and Codex sessions.</p>
      ) : (
        <>
          {session.recovery && (
            <div className='gx-account-recovery' role='status'>
              <strong>
                <AccountText text={session.recovery.reason} />
              </strong>
              {session.recovery.nextAttemptAt && (
                <p>
                  Next attempt: {new Date(session.recovery.nextAttemptAt).toLocaleString()} · Attempt{' '}
                  {session.recovery.attempt + 1}
                </p>
              )}
              <Button
                variant='outline'
                size='sm'
                disabled={busy}
                onClick={() => void request({ operation: 'stopRecovery' })}
              >
                Stop automatic recovery
              </Button>
            </div>
          )}
          <div className='gx-account-panel-columns'>
            <section>
              <div className='gx-account-current'>
                {current && <AccountIdentity account={current} />}
                <div>
                  <strong>
                    <AccountText text={current?.name ?? 'Choose an account'} />
                  </strong>
                  {current?.email && current.email !== current.name ? (
                    <p>
                      <AccountText text={current.email} />
                    </p>
                  ) : null}
                </div>
              </div>
              {current?.usageError && (
                <p>
                  <AccountText text={current.usageError} />
                </p>
              )}
              {current?.usage.length ? (
                <UsageBars windows={current.usage} />
              ) : (
                <p>Usage is unavailable for this login.</p>
              )}
              {context && (
                <div className='gx-account-usage'>
                  <div className='gx-account-usage-window'>
                    <strong>Conversation context</strong>
                    <div
                      className='gx-account-meter'
                      role='meter'
                      aria-label='Conversation context'
                      aria-valuenow={context.usedPercentage ?? 0}
                      aria-valuemin={0}
                      aria-valuemax={100}
                    >
                      <span style={{ width: `${context.usedPercentage ?? 0}%` }} />
                    </div>
                    <div className='gx-account-usage-caption'>
                      <span>
                        {context.usedPercentage === null
                          ? 'Usage unavailable'
                          : `${Math.round(context.usedPercentage)}%`}
                      </span>
                      <span>
                        {formatSessionChatContextTokens(context.usedTokens)} /{' '}
                        {formatSessionChatContextTokens(context.windowSize)}
                      </span>
                    </div>
                  </div>
                </div>
              )}
              <h3>{current ? 'Switch account' : 'Accounts'}</h3>
              {data.accounts
                .filter((a) => a.registered && a.provider === session.provider && a.id !== session.accountId)
                .map((a) => (
                  <button
                    key={a.id}
                    className='gx-account-switch'
                    disabled={busy}
                    title={a.status !== 'ready' ? 'Reconnect this account in Settings' : undefined}
                    onClick={() => {
                      if (a.status === 'ready') {
                        void request({ operation: 'select', accountId: a.id });
                      } else {
                        manageAccounts();
                      }
                    }}
                  >
                    <AccountIdentity account={a} />
                    <span className='gx-account-row-copy'>
                      <strong>
                        <AccountText text={a.name} />
                      </strong>
                      <SwitchAccountDetail account={a} />
                    </span>
                    <span>{a.status === 'ready' ? 'Use account →' : 'Reconnect'}</span>
                  </button>
                ))}
              <p>Switching resumes the same conversation. Stop an active turn before switching.</p>
            </section>
            <section>
              <div className='gx-account-local-policy'>
                <div className='gx-account-session-policy-heading'>
                  <h3>Keep going at a limit</h3>
                  {!customize && !session.override && (
                    <Button
                      variant='ghost'
                      size='sm'
                      aria-label='Customize this session'
                      onClick={() => setCustomize(true)}
                    >
                      <IconAdjustmentsHorizontal aria-hidden='true' />
                      Customize
                    </Button>
                  )}
                </div>
                <p>
                  {session.override
                    ? 'Custom settings · This session only'
                    : `Session defaults · ${session.policy.enabled ? (session.policy.atLimit === 'wait' ? 'Wait for reset' : 'Switch when eligible') : 'Automatic continuation off'}`}
                </p>
                {customize || session.override ? (
                  <>
                    <PolicyControls
                      policy={session.override ?? session.policy}
                      scope='This session'
                      disabled={busy}
                      onChange={(policy) => void request({ operation: 'sessionPolicy', policy })}
                    />
                    <Button
                      variant='ghost'
                      size='sm'
                      className='gx-account-policy-reset'
                      disabled={busy}
                      onClick={async () => {
                        if (await request({ operation: 'sessionPolicy', policy: null })) setCustomize(false);
                      }}
                    >
                      <IconArrowBackUp aria-hidden='true' />
                      Use session defaults
                    </Button>
                  </>
                ) : null}
              </div>
            </section>
          </div>
        </>
      )}
    </div>
  );
}

/**
 * CDXC:AgentProviders 2026-09-11 DECISION:
 * User: the chat panel must not print an account's email twice. The account name is the email whenever the login helper has no alias, so the second line of a Switch account row is the email only when the account has a name of its own; otherwise it shows the reset countdowns of the two limits in the badge, in the same order. A usage error takes the line instead, as in Settings. The current-account block above drops the line because the usage bars beneath it already show each reset.
 */
function SwitchAccountDetail({ account }: { account: AgentAccount }) {
  if (account.usageError) {
    return (
      <small>
        <AccountText text={account.usageError} />
      </small>
    );
  }
  if (account.email && account.email !== account.name) {
    return (
      <small>
        <AccountText text={account.email} />
      </small>
    );
  }
  const windows = accountFigureWindows(account).filter((window) => window !== undefined);
  return windows.length ? <small>{resetsLine(windows)}</small> : null;
}
