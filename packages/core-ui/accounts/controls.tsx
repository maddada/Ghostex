import { useId } from 'react';
import { Switch } from '@/packages/components/ui/switch';
import { SegmentedControl, SegmentedControlItem } from '@/packages/components/ui/segmented-control';
import {
  ACCOUNT_ICON_COLORS,
  accountIconColor,
  type AccountIconColor,
  type AccountPolicy,
  type AccountProvider,
  type AccountUsageWindow,
  type AgentAccount,
} from '@/packages/shared/agent-accounts';
import { AGENT_LOGOS } from '../agent-logos';
import './accounts.css';
export function AccountLogo({
  provider,
  color,
  className = '',
}: {
  provider: AccountProvider;
  color?: AccountIconColor;
  className?: string;
}) {
  return (
    <span
      role='img'
      aria-label={provider === 'codex' ? 'Codex account' : 'Claude account'}
      className={`gx-account-logo ${className}`}
      style={{
        backgroundColor: accountIconColor(color),
        maskImage: `url("${AGENT_LOGOS[provider]}")`,
        WebkitMaskImage: `url("${AGENT_LOGOS[provider]}")`,
      }}
    />
  );
}
export function AccountIdentity({ account }: { account: AgentAccount }) {
  return (
    <span className='gx-account-identity'>
      <AccountLogo provider={account.provider} color={account.color} />
      <span className='gx-account-figures'>
        {[0, 1].map((i) => (
          <span key={i} title={account.usage[i]?.label}>
            {account.usage[i] ? `${Math.round(account.usage[i].usedPercent)}%` : '·'}
          </span>
        ))}
      </span>
    </span>
  );
}
export function resetLabel(value?: string) {
  if (!value) return 'Reset time unavailable';
  const time = new Date(value);
  if (!Number.isFinite(time.getTime())) return 'Reset time unavailable';
  return `Resets ${time.toLocaleString(undefined, { weekday: 'short', hour: 'numeric', minute: '2-digit' })}`;
}
export function UsageBars({ windows }: { windows: AccountUsageWindow[] }) {
  return (
    <div className='gx-account-usage'>
      {windows.map((w) => (
        <div className='gx-account-usage-window' key={w.id}>
          <strong>{w.label}</strong>
          <div
            role='meter'
            aria-label={w.label}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={w.usedPercent}
            className='gx-account-meter'
          >
            <span style={{ width: `${w.usedPercent}%` }} />
          </div>
          <div className='gx-account-usage-caption'>
            <span>{Math.round(w.usedPercent)}% used</span>
            <span>{resetLabel(w.resetsAt)}</span>
          </div>
        </div>
      ))}
    </div>
  );
}
export function AccountColorSelect({
  value,
  onChange,
}: {
  value: AccountIconColor;
  onChange: (value: AccountIconColor) => void;
}) {
  const id = useId();
  return (
    <label className='gx-account-field' htmlFor={id}>
      Account icon color
      <select id={id} value={value} onChange={(e) => onChange(e.target.value as AccountIconColor)}>
        {ACCOUNT_ICON_COLORS.map((c) => (
          <option key={c.id} value={c.id}>
            {c.label}
          </option>
        ))}
      </select>
      <small>Identifies this login in the chat bar and sidebar.</small>
    </label>
  );
}
export function PolicyControls({
  policy,
  onChange,
  disabled = false,
  scope,
}: {
  policy: AccountPolicy;
  onChange: (policy: AccountPolicy) => void;
  disabled?: boolean;
  scope: string;
}) {
  const id = useId();
  return (
    <div className='gx-account-policy'>
      <div className='gx-account-field-row'>
        <label htmlFor={id}>Continue automatically</label>
        <Switch
          id={id}
          checked={policy.enabled}
          disabled={disabled}
          onCheckedChange={(enabled) => onChange({ ...policy, enabled })}
        />
      </div>
      <fieldset disabled={disabled || !policy.enabled}>
        <legend>When this account runs out</legend>
        <SegmentedControl
          value={policy.atLimit}
          onValueChange={(value) => {
            if (value === 'wait' || value === 'switch') onChange({ ...policy, atLimit: value });
          }}
          stretch
        >
          <SegmentedControlItem value='wait'>Wait for reset</SegmentedControlItem>
          <SegmentedControlItem value='switch'>Use another account</SegmentedControlItem>
        </SegmentedControl>
        <p>
          {policy.atLimit === 'wait'
            ? 'Pick up on this account when its usage resets.'
            : 'Use another eligible login for this model. Wait when every account is at its limit.'}
        </p>
        {policy.atLimit === 'switch' && (
          <label className='gx-account-field'>
            Account preference
            <select
              aria-label={`${scope} account preference`}
              value={policy.priority}
              onChange={(e) => onChange({ ...policy, priority: e.target.value as AccountPolicy['priority'] })}
            >
              <option value='leastUsed'>Lowest usage first</option>
              <option value='mostUsed'>Highest usage first</option>
              <option value='soonestReset'>Earliest reset first</option>
              <option value='latestReset'>Latest reset first</option>
            </select>
          </label>
        )}
        <div className='gx-account-field-row'>
          <label htmlFor={`${id}-errors`}>Recover from temporary errors</label>
          <Switch
            id={`${id}-errors`}
            checked={policy.retryErrors}
            onCheckedChange={(retryErrors) => onChange({ ...policy, retryErrors })}
          />
        </div>
        <p>
          Retry after 5, 10, 20, 40, then every 60 minutes. Login and permission requests need your attention. Stop
          cancels recovery for the current task.
        </p>
      </fieldset>
    </div>
  );
}
