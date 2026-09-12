import { IconArrowRight, IconCheck, IconRefresh } from '@tabler/icons-react';
import { Button } from '@/packages/components/ui/button';
import type { AccountSwitchProgress, AccountUsageWindow, AgentAccount } from '@/packages/shared/agent-accounts';
import { fableWindow, isFiveHourWindow, isWeeklyWindow } from '@/packages/shared/account-usage-windows';
import { formatResetCountdown } from '@/packages/shared/reset-countdown';
import { getBrandAgentLogoStyle } from '../agent-logos';
import { useAccountText } from './account-text';
import './account-switch-card.css';

function UsageCard({ label, usage, now }: { label: string; usage?: AccountUsageWindow; now: number }) {
  const used = usage && Number.isFinite(usage.usedPercent) ? Math.max(0, Math.min(100, usage.usedPercent)) : undefined;
  const level =
    used === undefined ? 'unknown' : used >= 100 ? 'exhausted' : used >= 80 ? 'high' : used >= 50 ? 'moderate' : 'low';
  const remaining = usage?.resetsAt ? Date.parse(usage.resetsAt) - now : NaN;
  const reset = Number.isFinite(remaining) ? (remaining > 0 ? formatResetCountdown(remaining) : 'Due') : null;
  return (
    <div
      className='gx-account-switch-usage-card'
      data-level={level}
      role={used === undefined ? undefined : 'meter'}
      aria-label={label}
      aria-valuenow={used}
      aria-valuemin={used === undefined ? undefined : 0}
      aria-valuemax={used === undefined ? undefined : 100}
      aria-valuetext={used === undefined ? undefined : `${Math.round(used)}%${reset ? `. Resets in ${reset}` : ''}`}
    >
      <span className='gx-account-switch-usage-label'>{label}</span>
      <strong className='gx-account-switch-usage-value'>
        {used === undefined ? '–' : Math.round(used)}
        {used !== undefined && <small>%</small>}
      </strong>
      <span className='gx-account-switch-usage-reset' title={reset ? `Resets in ${reset}` : 'Reset time unavailable'}>
        {reset && <IconRefresh size={10} aria-hidden='true' />}
        {reset ?? '–'}
      </span>
    </div>
  );
}

function Account({
  account,
  role,
  provider,
  target,
  verified,
  now,
}: {
  account?: AgentAccount;
  role: string;
  provider: AccountSwitchProgress['provider'];
  target?: boolean;
  verified: boolean;
  now: number;
}) {
  const text = useAccountText();
  const main = account?.usage.filter((window) => !window.model) ?? [];
  const label = account?.email || (target ? 'Selected account' : 'Current CLI login');
  return (
    <div
      className={`gx-account-switch-account gx-account-switch-account-${target ? 'to' : 'from'}`}
      role='group'
      aria-label={`${text(label)} usage`}
    >
      <span className='gx-account-switch-account-role'>{role}</span>
      <div className='gx-account-switch-account-identity'>
        <span className='gx-account-switch-agent-mark' aria-hidden='true'>
          <span style={getBrandAgentLogoStyle(provider)} />
        </span>
        <div>
          <strong title={text(label)}>{text(label)}</strong>
        </div>
        {target && verified && <IconCheck size={15} className='gx-account-switch-verified' />}
      </div>
      <div className='gx-account-switch-usage-cards'>
        <UsageCard label='5h limit' usage={main.find(isFiveHourWindow)} now={now} />
        <UsageCard label='7d limit' usage={main.find(isWeeklyWindow)} now={now} />
        {provider === 'claude' && <UsageCard label='Fable' usage={fableWindow(account?.usage ?? [])} now={now} />}
      </div>
    </div>
  );
}

/**
 * CDXC:AgentProviders 2026-09-12 DECISION:
 * User: center the account-switch card in chat until the switch completes; only add this card and leave the Switch Account menu unchanged.
 * Show both accounts with three percentage cards side by side, including Fable, colored by proximity to the limit and red at 100%. Omit "used" and "limit reached" captions.
 * Keep numbered steps with one animated line beneath the active step, replacing the rejected spinner around the number. The automatic third step is "Continue Session"; manual switches wait for the user's next message.
 * No heading spinner, repeated status above the composer, View terminal button, draft reassurance, or bottom bar.
 * Show plain provider logos in this card, without the account's two-character indicator inside them.
 * Identify each account by its real email on one line, respecting Hide emails, rather than account names or the preview's former invented aliases.
 */
export function AccountSwitchCard({
  progress,
  accounts,
  onRetry,
  retrying = false,
  now = Date.now(),
}: {
  progress: AccountSwitchProgress;
  accounts: readonly AgentAccount[];
  onRetry?: () => void;
  retrying?: boolean;
  now?: number;
}) {
  const text = useAccountText();
  const { phase, source, provider } = progress;
  const settled = phase === 'success';
  const verified = progress.accountReady === true || settled || phase === 'continuing';
  const providerName = provider === 'claude' ? 'Claude' : 'Codex';
  if (phase === 'cancelled') return null;
  const labels = ['Switch account', 'Resume conversation', ...(source === 'automatic' ? ['Continue Session'] : [])];
  return (
    <section className='gx-account-switch-card' data-phase={phase} aria-label='Account switch status'>
      <div className='gx-account-switch-card-heading'>
        <div role='status' aria-live='polite'>
          <h2>
            {phase === 'failed'
              ? verified
                ? 'Couldn’t continue the session'
                : 'Couldn’t complete the switch'
              : settled
                ? 'Account switched'
                : `Switching ${providerName} account`}
          </h2>
          <p>
            {phase === 'failed'
              ? verified
                ? 'The account is ready, but continuation needs attention.'
                : 'The new account isn’t ready yet.'
              : settled
                ? source === 'automatic'
                  ? 'Your task is continuing on the new account.'
                  : 'Ready whenever you are. Send your next message.'
                : source === 'automatic'
                  ? 'Usage limit reached. Continuing on an available account.'
                  : 'Your conversation will be ready for your next message.'}
          </p>
        </div>
      </div>
      <div className='gx-account-switch-account-route'>
        <Account
          account={accounts.find((account) => account.id === progress.fromAccountId)}
          provider={provider}
          role={verified ? 'Previous account' : 'Current account'}
          verified={verified}
          now={now}
        />
        <IconArrowRight size={17} className='gx-account-switch-route-arrow' aria-hidden='true' />
        <Account
          account={accounts.find((account) => account.id === progress.toAccountId)}
          provider={provider}
          role={verified ? 'Active account' : 'Switching to'}
          target
          verified={verified}
          now={now}
        />
      </div>
      {phase === 'failed' ? (
        <div className='gx-account-switch-failure-detail' role='alert'>
          <p>
            {text(
              progress.reason ||
                'We couldn’t confirm the new login. Retry this switch or use Switch Account to choose another account.'
            )}
          </p>
          {onRetry && (
            <div className='gx-account-switch-failure-actions'>
              <Button size='sm' variant='outline' disabled={retrying} onClick={onRetry}>
                <IconRefresh />
                Retry switch
              </Button>
            </div>
          )}
        </div>
      ) : (
        <ol className='gx-account-switch-progress' aria-label='Switch progress'>
          {labels.map((label, index) => {
            const current = phase === 'switching' ? 0 : phase === 'resuming' ? 1 : 2;
            const done = settled || index < current;
            const active = !done && index === current;
            return (
              <li
                key={label}
                className='gx-account-switch-step'
                data-state={done ? 'done' : active ? 'active' : 'pending'}
                aria-current={active ? 'step' : undefined}
                aria-label={`Step ${index + 1}: ${label}, ${done ? 'complete' : active ? 'in progress' : 'pending'}`}
              >
                <span className='gx-account-switch-step-marker' aria-hidden='true'>
                  <span className='gx-account-switch-step-number'>{index + 1}</span>
                </span>
                <span className='gx-account-switch-step-copy'>
                  <span className='gx-account-switch-step-label'>{label}</span>
                  <span className='gx-account-switch-step-track' aria-hidden='true'>
                    {active && <span className='gx-account-switch-step-motion' />}
                  </span>
                </span>
              </li>
            );
          })}
        </ol>
      )}
    </section>
  );
}
