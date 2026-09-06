import type { SessionChatContextUsage } from '@/packages/shared/session-chat';
import {
  resolveSessionChatContextMeterUsage,
  formatSessionChatContextTokens,
} from '../chat/session-chat-context-meter';
import { useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import type { AgentAccountsRequest, AgentAccountsState } from '@/packages/shared/agent-accounts';
import { AccountIdentity, PolicyControls, UsageBars } from './controls';
import { AccountManager } from './manager';
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
  const [manage, setManage] = useState(false);
  const [customize, setCustomize] = useState(false);
  const context = resolveSessionChatContextMeterUsage(contextUsage);
  const session = data?.session;
  const current = data?.accounts.find((a) => a.id === session?.accountId);
  return (
    <div className='gx-accounts gx-account-panel'>
      <div className='gx-account-heading'>
        <div>
          <h3>Accounts &amp; limits</h3>
          <p>The login behind this conversation.</p>
        </div>
        <div className='gx-account-row-actions'>
          <Button
            variant='outline'
            size='sm'
            disabled={busy}
            onClick={() => void request({ operation: 'session', refresh: true })}
          >
            Refresh
          </Button>
          <Button variant='ghost' size='sm' onClick={() => setManage(!manage)}>
            {manage ? 'Back to session' : 'Manage accounts'}
          </Button>
          <Button variant='ghost' size='sm' onClick={close}>
            Close
          </Button>
        </div>
      </div>
      {error && (
        <div className='gx-account-error' role='alert'>
          {error}
        </div>
      )}
      {!data ? (
        <p aria-live='polite'>{busy ? 'Reading accounts and usage…' : 'Could not load accounts.'}</p>
      ) : manage ? (
        <AccountManager data={data} busy={busy} request={request} />
      ) : !session ? (
        <p>Account management supports Claude and Codex sessions.</p>
      ) : (
        <>
          {session.recovery && (
            <div className='gx-account-recovery' role='status'>
              <strong>{session.recovery.reason}</strong>
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
                  <strong>{current?.name ?? 'Default CLI login'}</strong>
                  <p>{current?.email ?? 'Uses the CLI’s ordinary login on this computer.'}</p>
                </div>
              </div>
              {current?.usageError && <p>{current.usageError}</p>}
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
                          : `${Math.round(context.usedPercentage)}% used`}
                      </span>
                      <span>
                        {formatSessionChatContextTokens(context.usedTokens)} /{' '}
                        {formatSessionChatContextTokens(context.windowSize)}
                      </span>
                    </div>
                  </div>
                </div>
              )}
              <h3>Other {session.provider === 'claude' ? 'Claude' : 'Codex'} accounts</h3>
              {data.accounts
                .filter((a) => a.registered && a.provider === session.provider && a.id !== session.accountId)
                .map((a) => (
                  <button
                    key={a.id}
                    className='gx-account-switch'
                    disabled={busy || a.status !== 'ready'}
                    onClick={() => void request({ operation: 'select', accountId: a.id })}
                  >
                    <AccountIdentity account={a} />
                    <span className='gx-account-row-copy'>
                      <strong>{a.name}</strong>
                      <small>{a.email}</small>
                    </span>
                    <span>{a.status === 'ready' ? 'Use account →' : 'Reconnect'}</span>
                  </button>
                ))}
              {session.accountId && (
                <Button
                  variant='ghost'
                  size='sm'
                  disabled={busy}
                  onClick={() => void request({ operation: 'select', accountId: null })}
                >
                  Use default CLI login
                </Button>
              )}
              <p>Switching resumes the same conversation. Stop an active turn before switching.</p>
            </section>
            <section>
              <h3>Keep going at a limit</h3>
              <p>New {session.provider === 'claude' ? 'Claude' : 'Codex'} sessions</p>
              <PolicyControls
                policy={data.defaults[session.provider]}
                scope='New sessions'
                disabled={busy}
                onChange={(policy) => void request({ operation: 'defaults', provider: session.provider, policy })}
              />
              <div className='gx-account-local-policy'>
                <h3>This session</h3>
                <p>
                  {session.override
                    ? 'Custom continuation settings'
                    : `Saved defaults: ${session.policy.enabled ? (session.policy.atLimit === 'wait' ? 'wait for reset' : 'switch when eligible') : 'automatic continuation off'}`}
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
                      disabled={busy}
                      onClick={async () => {
                        if (await request({ operation: 'sessionPolicy', policy: null })) setCustomize(false);
                      }}
                    >
                      Use session defaults
                    </Button>
                  </>
                ) : (
                  <Button variant='outline' size='sm' onClick={() => setCustomize(true)}>
                    Customize this session
                  </Button>
                )}
              </div>
            </section>
          </div>
        </>
      )}
    </div>
  );
}
