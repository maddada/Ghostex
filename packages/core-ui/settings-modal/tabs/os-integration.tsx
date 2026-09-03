import { type ReactNode } from 'react';
import {
  IconAlertTriangle,
  IconCircleCheckFilled,
  IconCodeDots,
  IconPlayerPlay,
  IconRefresh,
  IconTerminal2,
} from '@tabler/icons-react';
import {
  type SidebarOSIntegrationStatusMessage,
  type SidebarOSIntegrationStatusItem,
} from '../../../shared/session-grid-contract';
import { SettingButton, SettingsNativeScrollArea, SettingsSection } from '../fields';
import { SettingsTabSearch, hasVisibleSettingsSearchResult, shouldShowSettingsSection } from '../search';

export function OSIntegrationSettingsTab({
  loading,
  onRequestStatus,
  onSetDefaults,
  search,
  searchEmptyState,
  status,
}: {
  loading?: boolean;
  onRequestStatus?: () => void;
  onSetDefaults?: (target: 'editor' | 'terminalLinks' | 'scriptRunner' | 'all') => void;
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
  status?: SidebarOSIntegrationStatusMessage;
}) {
  const ghostexBundleId = status?.bundleIdentifier;
  const editorDefaultCount =
    status && ghostexBundleId
      ? Object.values(status.editorDefaults).filter((bundleId) => bundleId === ghostexBundleId).length
      : 0;
  const scriptDefaultCount =
    status && ghostexBundleId
      ? Object.values(status.scriptDefaults).filter((bundleId) => bundleId === ghostexBundleId).length
      : 0;
  const terminalDefault = Boolean(
    status?.terminalLinkDefaultBundleId && status.terminalLinkDefaultBundleId === ghostexBundleId
  );
  const statusItems = status?.statusItems ?? [];
  const visibleStatusItems = statusItems.slice(0, 6);
  const remainingStatusItemCount = Math.max(0, statusItems.length - visibleStatusItems.length);
  return (
    <SettingsNativeScrollArea className='h-full min-h-0'>
      <div className='settings-page-width flex flex-col gap-6 px-5 pb-5'>
        {search.tab.isSearching && !hasVisibleSettingsSearchResult(search.tab) ? searchEmptyState : null}
        {shouldShowSettingsSection(search.sections.defaults) ? (
          <SettingsSection title='Defaults'>
            {/*
             * CDXC:OsIntegration 2026-05-27-18:06:
             * Ghostex registers as an available macOS editor and script handler
             * at install/build time, but Settings is the only place that changes
             * default editor, terminal-link, or script-runner ownership.
             */}
            <div className='grid grid-cols-1 gap-3 sm:grid-cols-2'>
              <SettingButton
                className='h-8 w-full justify-start px-3'
                disabled={!onSetDefaults}
                disabledReason='macOS default-app changes aren’t available here.'
                disabledTooltipClassName='w-full'
                onClick={() => onSetDefaults?.('editor')}
                type='button'
                variant='outline'
              >
                <IconCodeDots aria-hidden='true' data-icon='inline-start' />
                Set as Default Editor
              </SettingButton>
              <SettingButton
                className='h-8 w-full justify-start px-3'
                disabled={!onSetDefaults}
                disabledReason='macOS default-app changes aren’t available here.'
                disabledTooltipClassName='w-full'
                onClick={() => onSetDefaults?.('terminalLinks')}
                type='button'
                variant='outline'
              >
                <IconTerminal2 aria-hidden='true' data-icon='inline-start' />
                Set Terminal Links
              </SettingButton>
              <SettingButton
                className='h-8 w-full justify-start px-3'
                disabled={!onSetDefaults}
                disabledReason='macOS default-app changes aren’t available here.'
                disabledTooltipClassName='w-full'
                onClick={() => onSetDefaults?.('scriptRunner')}
                type='button'
                variant='outline'
              >
                <IconPlayerPlay aria-hidden='true' data-icon='inline-start' />
                Set Script Runner
              </SettingButton>
              <SettingButton
                className='h-8 w-full justify-start px-3'
                disabled={!onSetDefaults}
                disabledReason='macOS default-app changes aren’t available here.'
                disabledTooltipClassName='w-full'
                onClick={() => onSetDefaults?.('all')}
                type='button'
              >
                <IconCircleCheckFilled aria-hidden='true' data-icon='inline-start' />
                Set All
              </SettingButton>
            </div>
          </SettingsSection>
        ) : null}

        {shouldShowSettingsSection(search.sections.cli) ? (
          <SettingsSection title='CLI'>
            <div className='grid gap-2 rounded-none border border-border bg-muted/20 p-3 font-mono text-xs text-muted-foreground'>
              <div>ghostex open ./folder</div>
              <div>ghostex edit --wait file.ts:12:3</div>
              <div>ghostex terminal --cwd /tmp --title Scratch -- echo hi</div>
              <div>ghostex ./file.txt</div>
            </div>
          </SettingsSection>
        ) : null}

        {shouldShowSettingsSection(search.sections.diagnostics) ? (
          <SettingsSection title='Diagnostics'>
            <div className='flex flex-col gap-3 rounded-none border border-border bg-muted/20 p-3 text-sm text-muted-foreground'>
              <div className='flex items-center justify-between gap-3'>
                <span>{loading && !status ? 'Checking macOS handlers...' : 'macOS handler status'}</span>
                <SettingButton
                  className='h-8 px-3'
                  disabled={loading || !onRequestStatus}
                  disabledReason={
                    loading ? 'macOS handler status is being checked.' : 'Status checks aren’t available here.'
                  }
                  onClick={onRequestStatus}
                  type='button'
                  variant='outline'
                >
                  <IconRefresh aria-hidden='true' data-icon='inline-start' />
                  Refresh
                </SettingButton>
              </div>
              {status ? (
                <div className='grid gap-2'>
                  {statusItems.length > 0 ? (
                    <div className='grid gap-2 rounded-none border border-destructive/30 bg-destructive/5 p-3 text-xs text-muted-foreground'>
                      {/*
                       * CDXC:OsIntegration 2026-06-24-15:10:
                       * Settings must account for shared Launch Services status items without exposing raw OSStatus values or native paths. Show generic repair guidance and sanitized target/extension labels so the same UI works for Swift and GPUI senders.
                       */}
                      <div className='flex items-start gap-2'>
                        <IconAlertTriangle aria-hidden='true' className='mt-0.5 shrink-0 text-destructive' size={16} />
                        <div className='grid gap-1'>
                          <div className='font-medium text-foreground'>
                            {getOSIntegrationStatusNoticeTitle(statusItems)}
                          </div>
                          <div>{getOSIntegrationStatusNoticeDescription(statusItems)}</div>
                        </div>
                      </div>
                      <div className='grid gap-1'>
                        {visibleStatusItems.map((item, index) => (
                          <div className='flex items-center justify-between gap-3' key={index}>
                            <span>{formatOSIntegrationStatusItemSubject(item)}</span>
                            <span className='text-right font-medium text-foreground'>
                              {formatOSIntegrationStatusItemReason(item)}
                            </span>
                          </div>
                        ))}
                        {remainingStatusItemCount > 0 ? (
                          <div className='text-muted-foreground'>
                            {remainingStatusItemCount} more handler updates need attention.
                          </div>
                        ) : null}
                      </div>
                    </div>
                  ) : null}
                  <OSIntegrationDiagnosticRow
                    label='Available editor'
                    value={status.registeredEditableFiles ? 'Registered' : 'Missing'}
                  />
                  <OSIntegrationDiagnosticRow
                    label='Available script runner'
                    value={status.registeredScriptRunner ? 'Registered' : 'Missing'}
                  />
                  <OSIntegrationDiagnosticRow
                    label='ghostex:// links'
                    value={
                      status.registeredGhostexURLScheme
                        ? terminalDefault
                          ? 'Default'
                          : `Default: ${status.terminalLinkDefaultBundleId ?? 'None'}`
                        : 'Missing'
                    }
                  />
                  <OSIntegrationDiagnosticRow
                    label='Editor defaults'
                    value={`${editorDefaultCount}/${Object.keys(status.editorDefaults).length} sampled`}
                  />
                  <OSIntegrationDiagnosticRow
                    label='Script defaults'
                    value={`${scriptDefaultCount}/${Object.keys(status.scriptDefaults).length} sampled`}
                  />
                </div>
              ) : (
                <div>Ghostex has not checked Launch Services yet.</div>
              )}
            </div>
          </SettingsSection>
        ) : null}
      </div>
    </SettingsNativeScrollArea>
  );
}

export function getOSIntegrationStatusNoticeTitle(items: readonly SidebarOSIntegrationStatusItem[]): string {
  if (items.some((item) => item.reason === 'unsupportedPlatform')) {
    return 'macOS Launch Services is unavailable in this build.';
  }
  return 'Some macOS handler updates need attention.';
}

export function getOSIntegrationStatusNoticeDescription(items: readonly SidebarOSIntegrationStatusItem[]): string {
  if (items.some((item) => item.reason === 'unsupportedPlatform')) {
    return 'This platform cannot inspect or change macOS app defaults.';
  }
  return 'Refresh after macOS finishes updating Launch Services, or choose Ghostex manually in macOS Open With/System Settings.';
}

export function formatOSIntegrationStatusItemSubject(item: SidebarOSIntegrationStatusItem): string {
  const fileExtension = formatOSIntegrationStatusExtension(item.extension);
  if (item.target === 'editor') {
    return fileExtension ? `Editor default .${fileExtension}` : 'Editor defaults';
  }
  if (item.target === 'scriptRunner') {
    return fileExtension ? `Script runner .${fileExtension}` : 'Script runner';
  }
  if (item.target === 'terminalLinks') {
    return item.scheme === 'ghostex' ? 'Terminal links ghostex://' : 'Terminal links';
  }
  if (item.target === 'bundleRegistration') {
    return item.operation === 'registerBundle' ? 'App registration' : 'App identity';
  }
  return 'Platform support';
}

export function formatOSIntegrationStatusExtension(extension: string | undefined): string | undefined {
  if (!extension || !/^[A-Za-z0-9][A-Za-z0-9_-]{0,24}$/u.test(extension)) {
    return undefined;
  }
  return extension;
}

export function formatOSIntegrationStatusItemReason(item: SidebarOSIntegrationStatusItem): string {
  switch (item.reason) {
    case 'bundleIdentifierMissing':
      return 'App identity missing';
    case 'bundleRegistrationFailed':
      return 'Registration failed';
    case 'contentTypeUnavailable':
      return 'File type unavailable';
    case 'invalidTarget':
      return 'Unsupported action';
    case 'launchServicesRejected':
      return 'Default change rejected';
    case 'unsupportedPlatform':
      return 'Unavailable';
  }
}

export function OSIntegrationDiagnosticRow({ label, value }: { label: string; value: string }) {
  return (
    <div className='flex items-center justify-between gap-3'>
      <span>{label}</span>
      <span className='text-right font-medium text-foreground'>{value}</span>
    </div>
  );
}
