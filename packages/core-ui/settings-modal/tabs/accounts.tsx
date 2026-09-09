import type { ReactNode } from 'react';
import { AccountsSettingsSection } from '../../accounts/manager';
import { SettingsNativeScrollArea } from '../fields';
import { type SettingsTabSearch, shouldShowSettingsSection } from '../search';

/** CDXC:Settings 2026-09-09 DECISION: User: account management has its own Accounts page in Settings, replacing the Accounts section under Agents. */
export function AccountsSettingsTab({
  isActive,
  hideAccountEmails,
  onHideAccountEmailsChange,
  search,
  searchEmptyState,
}: {
  isActive: boolean;
  hideAccountEmails: boolean;
  onHideAccountEmailsChange: (hidden: boolean) => void;
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
}) {
  return (
    <SettingsNativeScrollArea className='h-full min-h-0'>
      <div className='settings-page-width flex flex-col gap-6 px-5 pb-5'>
        {shouldShowSettingsSection(search.sections.accounts) ? (
          <AccountsSettingsSection
            active={isActive}
            hideEmails={hideAccountEmails}
            onHideEmailsChange={onHideAccountEmailsChange}
          />
        ) : searchEmptyState}
      </div>
    </SettingsNativeScrollArea>
  );
}
