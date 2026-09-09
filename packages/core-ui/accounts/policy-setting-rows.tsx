import { useId } from 'react';
import { SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/packages/components/ui/select';
import { SegmentedControl, SegmentedControlItem } from '@/packages/components/ui/segmented-control';
import { Switch } from '@/packages/components/ui/switch';
import { type AccountPolicy } from '@/packages/shared/agent-accounts';
import { SettingRow, SettingsSelect, SettingsSelectContent } from '../settings-modal/fields';

/*
 * CDXC:Settings 2026-09-10 WHY:
 * These rows are the only part of the accounts controls that reads the Settings
 * field primitives, and the chat account panel imports controls.tsx. Keeping
 * them here stops the mobile chat bundle from pulling the whole Settings module
 * (and its pet-avatar artwork) in through that one import.
 */
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
