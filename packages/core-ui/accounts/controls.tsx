import { useId } from 'react';
import { accountUsageLabel } from '@/packages/shared/account-usage-label';
import { formatResetCountdown } from '@/packages/shared/reset-countdown';
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/packages/components/ui/select';
import { SettingRow, SettingsSelect, SettingsSelectContent } from '../settings-modal/fields';
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
/**
 * CDXC:AgentProviders 2026-09-08 DECISION:
 * User: Codex account badges show the five-hour percentage on the second line when that limit exists; otherwise show available resets as "2rs" or "0rs".
 * Use the main account windows so Spark's separate five-hour limit does not stand in for an absent account limit.
 */
export function AccountIdentity({ account }: { account: AgentAccount }) {
  const mainWindows = account.usage.filter((window) => !window.model);
  const first =
    account.provider === 'codex'
      ? mainWindows.find((window) => (window.limitWindowSeconds ?? 0) >= 604800)
      : account.usage[0];
  const second =
    account.provider === 'codex' ? mainWindows.find((window) => window.limitWindowSeconds === 18000) : account.usage[1];
  const figures = [
    { label: first?.label, value: first ? `${Math.round(first.usedPercent)}%` : '·' },
    second
      ? { label: second.label, value: `${Math.round(second.usedPercent)}%` }
      : account.provider === 'codex' && account.resetCredits != null
        ? { label: 'Available usage resets', value: `${account.resetCredits}rs` }
        : { label: undefined, value: '·' },
  ];
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
  if (!value) return 'Reset time unavailable';
  const time = new Date(value);
  if (!Number.isFinite(time.getTime())) return 'Reset time unavailable';
  const remainingMs = time.getTime() - Date.now();
  return remainingMs > 0 ? `Resets ${formatResetCountdown(remainingMs)}` : 'Reset due';
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
        <p>
          {policy.atLimit === 'wait'
            ? 'Pick up on this account when its usage resets.'
            : 'Use another eligible login for this model. Wait when every account is at its limit.'}
        </p>
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
              <SelectContent>
                <SelectItem value='leastUsed'>Lowest usage first</SelectItem>
                <SelectItem value='mostUsed'>Highest usage first</SelectItem>
                <SelectItem value='soonestReset'>Earliest reset first</SelectItem>
                <SelectItem value='latestReset'>Latest reset first</SelectItem>
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
        <p>
          Retry after 5, 10, 20, 40, then every 60 minutes. Login and permission requests need your attention. Stop
          cancels recovery for the current task.
        </p>
      </fieldset>
    </div>
  );
}

/**
 * CDXC:Settings 2026-09-10 DECISION:
 * User: on the Accounts page the auto-continue policy renders as ordinary setting rows (switch, segmented control, dropdown), matching the General page, instead of the stacked form the session popover keeps in PolicyControls.
 */
export function PolicySettingRows({
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
  const inactive = disabled || !policy.enabled;
  const inactiveReason = disabled ? 'Accounts are being updated.' : 'Turn on Continue automatically first.';
  return (
    <>
      <SettingRow
        description='Keep working when the account for a new session reaches its usage limit.'
        htmlFor={id}
        label='Continue automatically'
      >
        <Switch
          id={id}
          checked={policy.enabled}
          disabled={disabled}
          onCheckedChange={(enabled) => onChange({ ...policy, enabled })}
        />
      </SettingRow>
      <SettingRow
        description={
          policy.atLimit === 'wait'
            ? 'Pick up on this account when its usage resets.'
            : 'Use another eligible login for this model. Wait when every account is at its limit.'
        }
        htmlFor={`${id}-limit`}
        label='When the account runs out'
      >
        <SegmentedControl
          disabled={inactive}
          id={`${id}-limit`}
          value={policy.atLimit}
          onValueChange={(value) => {
            if (value === 'wait' || value === 'switch') onChange({ ...policy, atLimit: value });
          }}
        >
          <SegmentedControlItem disabled={inactive} value='wait'>
            Wait for reset
          </SegmentedControlItem>
          <SegmentedControlItem disabled={inactive} value='switch'>
            Use another account
          </SegmentedControlItem>
        </SegmentedControl>
      </SettingRow>
      {policy.atLimit === 'switch' ? (
        <SettingRow
          description='Which eligible account Ghostex tries first.'
          htmlFor={`${id}-priority`}
          label='Account preference'
        >
          <SettingsSelect
            disabled={inactive}
            disabledReason={inactiveReason}
            items={POLICY_PRIORITY_OPTIONS}
            onValueChange={(value) => {
              if (value) onChange({ ...policy, priority: value as AccountPolicy['priority'] });
            }}
            value={policy.priority}
          >
            <SelectTrigger aria-label={`${scope} account preference`} className='h-8 px-3' id={`${id}-priority`}>
              <SelectValue />
            </SelectTrigger>
            <SettingsSelectContent className='settings-list-select-content'>
              <SelectGroup>
                {POLICY_PRIORITY_OPTIONS.map((option) => (
                  <SelectItem key={option.value} value={option.value}>
                    {option.label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SettingsSelectContent>
          </SettingsSelect>
        </SettingRow>
      ) : null}
      <SettingRow
        description='Retry after 5, 10, 20, 40, then every 60 minutes. Login and permission requests need your attention. Stop cancels recovery for the current task.'
        htmlFor={`${id}-errors`}
        label='Recover from temporary errors'
      >
        <Switch
          id={`${id}-errors`}
          checked={policy.retryErrors}
          disabled={inactive}
          onCheckedChange={(retryErrors) => onChange({ ...policy, retryErrors })}
        />
      </SettingRow>
    </>
  );
}

const POLICY_PRIORITY_OPTIONS = [
  { label: 'Lowest usage first', value: 'leastUsed' },
  { label: 'Highest usage first', value: 'mostUsed' },
  { label: 'Earliest reset first', value: 'soonestReset' },
  { label: 'Latest reset first', value: 'latestReset' },
] as const;
