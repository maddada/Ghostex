import { useId } from 'react';
import { accountUsageLabel } from '@/packages/shared/account-usage-label';
import {
  accountFigures,
  accountPolicyAtLimitDescription,
  accountResetLabel,
  accountResetsLine,
  ACCOUNT_POLICY_PRIORITY_OPTIONS,
  ACCOUNT_POLICY_RETRY_DESCRIPTION,
} from './presentation';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/packages/components/ui/select';
import { Switch } from '@/packages/components/ui/switch';
import { SegmentedControl, SegmentedControlItem } from '@/packages/components/ui/segmented-control';
import {
  type AccountPolicy,
  type AccountProvider,
  type AccountUsageWindow,
  type AgentAccount,
} from '@/packages/shared/agent-accounts';
import { AGENT_LOGOS, getBrandAgentLogoStyle } from '../agent-logos';
import './accounts.css';
export function AccountLogo({
  provider,
  slot,
  className = '',
}: {
  provider: AccountProvider;
  slot?: string;
  className?: string;
}) {
  return (
    <span
      role='img'
      aria-label={`${provider === 'codex' ? 'Codex' : 'Claude'}${slot ? ` account ${slot}` : ''}`}
      className={`gx-account-logo ${className}`}
    >
      <span className='gx-account-logo-image' style={getBrandAgentLogoStyle(provider)} />
    </span>
  );
}
export { accountFigureWindows } from './presentation';
export function AccountIdentity({ account }: { account: AgentAccount }) {
  const figures = accountFigures(account);
  return (
    <span className='gx-account-identity'>
      <AccountLogo provider={account.provider} slot={account.selector} />
      <span className='gx-account-figures'>
        {figures.map((figure, i) => (
          <span key={i} title={figure.label}>
            {figure.value}
          </span>
        ))}
      </span>
    </span>
  );
}
export function resetLabel(value?: string) {
  return accountResetLabel(value);
}
/** One "Resets 2h 14m · 3d 6h" line for several limits, in the order given, skipping limits without a reset time. */
export function resetsLine(windows: AccountUsageWindow[]): string {
  return accountResetsLine(windows);
}
export function UsageBars({ windows }: { windows: AccountUsageWindow[] }) {
  return (
    <div className='gx-account-usage'>
      {windows.map((w) => (
        <div className='gx-account-usage-window' key={w.id}>
          <strong>
            {accountUsageLabel(w)}: {Math.round(w.usedPercent)}%
          </strong>
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
            <span>{resetLabel(w.resetsAt)}</span>
          </div>
        </div>
      ))}
    </div>
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
        <legend>When the session's account runs out</legend>
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
        <p>{accountPolicyAtLimitDescription(policy)}</p>
        {policy.atLimit === 'switch' && (
          <label className='gx-account-field'>
            Account preference
            <Select
              value={policy.priority}
              onValueChange={(value) => {
                if (value) onChange({ ...policy, priority: value as AccountPolicy['priority'] });
              }}
            >
              <SelectTrigger aria-label={`${scope} account preference`} className='w-full'>
                <SelectValue />
              </SelectTrigger>
              <SelectContent className='ghostex-session-chat-popup'>
                {ACCOUNT_POLICY_PRIORITY_OPTIONS.map((option) => (
                  <SelectItem key={option.value} value={option.value}>
                    {option.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
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
        <p>{ACCOUNT_POLICY_RETRY_DESCRIPTION}</p>
      </fieldset>
    </div>
  );
}
