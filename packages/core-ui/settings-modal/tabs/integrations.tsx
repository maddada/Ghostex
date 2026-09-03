import { useState, type ReactNode } from 'react';
import { cn } from '@/packages/components/utils';
import { Button } from '@/packages/components/ui/button';
import { Field, FieldContent, FieldDescription, FieldTitle } from '@/packages/components/ui/field';
import { SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/packages/components/ui/select';
import { Switch } from '@/packages/components/ui/switch';
import { AppTooltip } from '../../app-tooltip';
import {
  IconDeviceDesktop,
  IconDownload,
  IconInfoCircle,
  IconRefresh,
  IconSettings,
  IconTerminal2,
} from '@tabler/icons-react';
import { type SidebarGhostexCliStatusMessage } from '../../../shared/session-grid-contract';
import { APP_SHOTS_HOTKEY_OPTIONS, type AppShotsHotkey } from '../../../shared/ghostex-settings';
import { type BundledGhostexAgentSkillId } from '../../../shared/ghostex-agent-skills';
import { BundledAgentSkillsPanel } from '../../bundled-agent-skills-panel';
import {
  SettingButton,
  SettingSwitch,
  SettingsNativeScrollArea,
  SettingsSection,
  SettingsSelect,
  SettingsSelectContent,
} from '../fields';
import {
  SettingsTabSearch,
  hasVisibleSettingsSearchResult,
  shouldShowSetting,
  shouldShowSettingsSection,
} from '../search';

export function getCuaPermissionStatus(
  ghostexCliStatus: SidebarGhostexCliStatusMessage | undefined,
  ghostexCliStatusLoading: boolean
): { status: string; tone: 'success' | 'warning' | 'neutral' } {
  if (ghostexCliStatusLoading || !ghostexCliStatus) {
    return { status: 'Checking', tone: 'neutral' };
  }
  if (ghostexCliStatus?.cuaDriverInstalled !== true) {
    return { status: 'Trycua Not Installed', tone: 'warning' };
  }

  const accessibilityGranted = ghostexCliStatus.cuaDriverAccessibilityPermissionGranted;
  const screenRecordingGranted = ghostexCliStatus.cuaDriverScreenRecordingPermissionGranted;
  if (accessibilityGranted === true && screenRecordingGranted === true) {
    return { status: 'Permissions Allowed', tone: 'success' };
  }
  if (accessibilityGranted === false && screenRecordingGranted === false) {
    return { status: 'Permissions Off - Open Settings', tone: 'warning' };
  }
  if (accessibilityGranted === false) {
    return { status: 'Accessibility Off - Open Settings', tone: 'warning' };
  }
  if (screenRecordingGranted === false) {
    return { status: 'Screen Recording Off - Open Settings', tone: 'warning' };
  }
  if (accessibilityGranted === true) {
    return { status: 'Screen Recording Unknown', tone: 'warning' };
  }
  if (screenRecordingGranted === true) {
    return { status: 'Accessibility Unknown', tone: 'warning' };
  }
  return { status: 'Permission Status Unknown', tone: 'warning' };
}

export function VersionInfoButton({ label, version }: { label: string; version: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <AppTooltip content={copied ? `Copied ${version}` : version}>
      <Button
        aria-label={`Copy ${label} version ${version}`}
        onClick={() => {
          void navigator.clipboard.writeText(version).then(
            () => {
              setCopied(true);
              window.setTimeout(() => setCopied(false), 1200);
            },
            () => undefined
          );
        }}
        size='icon-xs'
        type='button'
        variant='ghost'
      >
        <IconInfoCircle aria-hidden='true' />
      </Button>
    </AppTooltip>
  );
}

export function IntegrationsSettingsTab({
  appShotsEnabled,
  appShotsHotkey,
  appShotsMetadataEnabled,
  ghostexCliStatus,
  ghostexCliStatusLoading,
  onAppShotsEnabledChange,
  onAppShotsHotkeyChange,
  onAppShotsMetadataEnabledChange,
  onInstallCliSkill,
  onInstallBrowserControl,
  onInstallBrowserUseSkill,
  onInstallComputerUseSkill,
  onInstallCuaDriver,
  onInstallFable56OrchestrationSkill,
  onInstallManageBeadsSkill,
  onInstallGenerateTitleSkill,
  onInstallGhostexCli,
  onInstallMoveCodexSessionSkill,
  onUninstallBundledAgentSkill,
  onUninstallBundledAgentSkills,
  onOpenAccessibilityPreferences,
  onOpenScreenRecordingPreferences,
  onRequestGhostexCliStatus,
  search,
  searchEmptyState,
}: {
  appShotsEnabled: boolean;
  appShotsHotkey: AppShotsHotkey;
  appShotsMetadataEnabled: boolean;
  ghostexCliStatus?: SidebarGhostexCliStatusMessage;
  ghostexCliStatusLoading: boolean;
  onAppShotsEnabledChange: (checked: boolean) => void;
  onAppShotsHotkeyChange: (hotkey: AppShotsHotkey) => void;
  onAppShotsMetadataEnabledChange: (checked: boolean) => void;
  onInstallCliSkill?: () => void;
  onInstallBrowserControl?: () => void;
  onInstallBrowserUseSkill?: () => void;
  onInstallComputerUseSkill?: () => void;
  onInstallCuaDriver?: () => void;
  onInstallFable56OrchestrationSkill?: () => void;
  onInstallManageBeadsSkill?: () => void;
  onInstallGenerateTitleSkill?: () => void;
  onInstallGhostexCli?: () => void;
  onInstallMoveCodexSessionSkill?: () => void;
  onUninstallBundledAgentSkill?: (skillId: BundledGhostexAgentSkillId) => void;
  onUninstallBundledAgentSkills?: () => void;
  onOpenAccessibilityPreferences?: () => void;
  onOpenScreenRecordingPreferences?: () => void;
  onRequestGhostexCliStatus?: () => void;
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
}) {
  const showIntegrationRow = (settingKey: string) => shouldShowSetting(search.sections.integrations, settingKey);
  const ghostexCliStatusChecking = ghostexCliStatusLoading || !ghostexCliStatus;
  const cliReady = ghostexCliStatus?.installed === true;
  /**
   * CDXC:OsIntegration 2026-05-29-06:00:
   * Trycua Permissions status must be based on Trycua's own permission check,
   * because granting Trycua in macOS can still leave Ghostex's separate
   * Accessibility trust bit false. The row represents desktop automation
   * readiness for agents, not Ghostex's ability to synthesize input.
   */
  const cuaPermissionStatus = getCuaPermissionStatus(ghostexCliStatus, ghostexCliStatusChecking);

  return (
    <SettingsNativeScrollArea className='h-full min-h-0'>
      <div className='settings-page-width flex flex-col gap-6 px-5 pb-5'>
        {/*
         * CDXC:RemotePairing 2026-05-27-04:17:
         * Settings owns one Integrations tab for post-onboarding CLI, bundled
         * Ghostex skills, Trycua runtime lifecycle, and macOS privacy
         * permissions. Keeping Trycua here avoids duplicating it in Extensions.
         *
         * CDXC:AgentHooks 2026-06-29-01:26:
         * Agent hook install/status UI lives in Settings -> Agents, where the detailed per-agent hook list already exists. Integrations should not duplicate that setup row.
         *
         * CDXC:AgentHooks 2026-08-19-11:20:
         * Hook and bundled-skill removal moved next to the hook setup panel in Settings -> Agents, so Integrations no longer carries a Hooks & Skills recovery card.
         *
         * CDXC:AgentSkills 2026-05-31-09:18:
         * Bundled Ghostex skills are explicit per-skill installs in Settings,
         * not hidden side effects of CLI setup. Each row explains what the skill
         * teaches agents and remains disabled until the Ghostex CLI is present.
         *
         * CDXC:Cli 2026-06-07-13:53:
         * Ghostex installs and repairs the app-bundled CLI automatically for
         * DMG and Homebrew installs. Settings should expose a manual Repair CLI
         * action for unusual PATH states, not a cask reinstall flow.
         */}
        {search.tab.isSearching && !hasVisibleSettingsSearchResult(search.tab) ? searchEmptyState : null}
        {shouldShowSettingsSection(search.sections.integrations) ? (
          <SettingsSection title='Integrations'>
            {showIntegrationRow('ghostexCli') ? (
              <IntegrationSettingsRow
                description='Ghostex keeps the app-bundled ghostex command linked automatically for mobile apps and CLI-backed integration setup. gx is linked when that alias is available and not taken by another command.'
                icon={IconTerminal2}
                status={ghostexCliStatusChecking ? 'Checking' : cliReady ? 'Installed' : 'Not installed'}
                tone={ghostexCliStatusChecking ? 'neutral' : cliReady ? 'success' : 'warning'}
                title='Ghostex CLI'
              >
                <SettingButton
                  disabled={ghostexCliStatusChecking || !onInstallGhostexCli}
                  disabledReason={
                    ghostexCliStatusChecking ? 'CLI status is being checked.' : 'CLI repair isn’t available here.'
                  }
                  onClick={onInstallGhostexCli}
                  type='button'
                  variant={cliReady ? 'outline' : 'default'}
                >
                  <IconDownload aria-hidden='true' data-icon='inline-start' />
                  Repair CLI
                </SettingButton>
                <SettingButton
                  disabled={ghostexCliStatusChecking || !onRequestGhostexCliStatus}
                  disabledReason={
                    ghostexCliStatusChecking
                      ? 'CLI status is being checked.'
                      : 'CLI status refresh isn’t available here.'
                  }
                  onClick={onRequestGhostexCliStatus}
                  type='button'
                  variant='ghost'
                >
                  <IconRefresh aria-hidden='true' data-icon='inline-start' />
                  Refresh
                </SettingButton>
              </IntegrationSettingsRow>
            ) : null}

            {showIntegrationRow('bundledAgentSkills') ? (
              <BundledAgentSkillsPanel
                ghostexCliStatus={ghostexCliStatus}
                ghostexCliStatusLoading={ghostexCliStatusChecking}
                onInstallCuaDriver={onInstallCuaDriver}
                onInstallSkill={{
                  cli: onInstallCliSkill,
                  browserUse: onInstallBrowserUseSkill,
                  computerUse: onInstallComputerUseSkill,
                  embeddedBrowserUse: onInstallBrowserControl,
                  fable56Orchestration: onInstallFable56OrchestrationSkill,
                  manageBeads: onInstallManageBeadsSkill,
                  generateTitle: onInstallGenerateTitleSkill,
                  moveCodexSession: onInstallMoveCodexSessionSkill,
                }}
                onRefreshStatus={onRequestGhostexCliStatus}
                onUninstallAllSkills={onUninstallBundledAgentSkills}
                onUninstallSkill={onUninstallBundledAgentSkill}
              />
            ) : null}

            {/*
             * CDXC:AppShots 2026-06-12-11:12:
             * Settings copy must describe App Shots as an agent-session feature because captured context now targets the focused or recent agent instead of Codex only.
             *
             * CDXC:AppShots 2026-06-15-02:01:
             * App Shots should be instant screenshot capture. Settings copy must not promise OCR, Accessibility text extraction, or other app-content scraping.
             *
             * CDXC:AppShots 2026-06-29-02:59:
             * App Shot prompt metadata is disabled by default and must be a visible opt-in under the App Shots row, because routine captures should paste only the image link unless the user asks for window metadata.
             */}
            {showIntegrationRow('appShots') ? (
              <IntegrationSettingsRow
                badge='Beta'
                description='Capture the frontmost app window, then stage it in the focused or recent agent session as local image context.'
                icon={IconDeviceDesktop}
                status={appShotsEnabled ? 'Enabled' : 'Disabled'}
                tone={appShotsEnabled ? 'success' : 'neutral'}
                title='App Shots'
              >
                <div className='flex min-w-[190px] flex-col gap-2 sm:items-end'>
                  <div className='flex items-center gap-2'>
                    <span className='text-xs text-muted-foreground'>Enabled</span>
                    <Switch
                      aria-label='Enable App Shots'
                      checked={appShotsEnabled}
                      onCheckedChange={onAppShotsEnabledChange}
                    />
                  </div>
                  <SettingsSelect
                    disabled={!appShotsEnabled}
                    disabledReason='Turn on App Shots first.'
                    onValueChange={(value) => onAppShotsHotkeyChange(value as AppShotsHotkey)}
                    value={appShotsHotkey}
                  >
                    <SelectTrigger aria-label='App Shots hotkey' className='w-[190px]'>
                      <SelectValue />
                    </SelectTrigger>
                    <SettingsSelectContent>
                      <SelectGroup>
                        {APP_SHOTS_HOTKEY_OPTIONS.map((option) => (
                          <SelectItem key={option.value} value={option.value}>
                            {option.label}
                          </SelectItem>
                        ))}
                      </SelectGroup>
                    </SettingsSelectContent>
                  </SettingsSelect>
                  <div className='flex items-center gap-2'>
                    <span className='text-xs text-muted-foreground'>Metadata</span>
                    <SettingSwitch
                      aria-label='Include App Shots metadata'
                      checked={appShotsMetadataEnabled}
                      disabled={!appShotsEnabled}
                      disabledReason='Turn on App Shots first.'
                      onCheckedChange={onAppShotsMetadataEnabledChange}
                    />
                  </div>
                </div>
              </IntegrationSettingsRow>
            ) : null}

            {showIntegrationRow('cuaPermissions') ? (
              <IntegrationSettingsRow
                description='Trycua needs Accessibility to click and type in apps, and Screen Recording to understand what is visible on the desktop.'
                icon={IconSettings}
                status={cuaPermissionStatus.status}
                tone={cuaPermissionStatus.tone}
                title='Trycua Permissions'
              >
                <SettingButton
                  disabled={!onOpenAccessibilityPreferences}
                  disabledReason='Accessibility settings aren’t available here.'
                  onClick={onOpenAccessibilityPreferences}
                  type='button'
                  variant='outline'
                >
                  Accessibility
                </SettingButton>
                <SettingButton
                  disabled={!onOpenScreenRecordingPreferences}
                  disabledReason='Screen Recording settings aren’t available here.'
                  onClick={onOpenScreenRecordingPreferences}
                  type='button'
                  variant='outline'
                >
                  Screen Recording
                </SettingButton>
              </IntegrationSettingsRow>
            ) : null}
            {/*
            CDXC:Settings 2026-06-19-14:51:
            macOS Settings > Integrations should not include a Setup Flow launcher row.
            Keep setup access owned by first-launch and other explicit entry points instead of listing it as an integration setting.
          */}
          </SettingsSection>
        ) : null}
      </div>
    </SettingsNativeScrollArea>
  );
}

export function IntegrationSettingsRow({
  badge,
  children,
  description,
  icon: Icon,
  status,
  title,
  tone,
  version,
}: {
  badge?: string;
  children: ReactNode;
  description: string;
  icon: typeof IconInfoCircle;
  status: string;
  title: string;
  tone: 'success' | 'warning' | 'neutral';
  version?: string;
}) {
  return (
    <Field className='rounded-none border border-border bg-muted/20 px-4 py-3'>
      <div className='flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between'>
        <div className='flex min-w-0 gap-3'>
          <span className='mt-0.5 inline-flex size-8 shrink-0 items-center justify-center rounded-none bg-muted text-muted-foreground'>
            <Icon aria-hidden='true' size={17} />
          </span>
          <FieldContent>
            <div className='flex flex-wrap items-center gap-2'>
              <FieldTitle className='text-sm'>{title}</FieldTitle>
              {badge ? (
                /*
                 * CDXC:AppShots 2026-06-13-19:51:
                 * Settings must visibly mark App Shots as Beta while keeping
                 * the separate Enabled/Disabled status badge for its toggle
                 * state.
                 */
                <span className='inline-flex rounded-none border border-sky-500/40 bg-sky-500/10 px-2 py-0.5 text-[11px] font-semibold text-sky-200'>
                  {badge}
                </span>
              ) : null}
              <span
                className={cn(
                  'inline-flex rounded-none border px-2 py-0.5 text-[11px] font-semibold',
                  tone === 'success' && 'border-emerald-500/40 bg-emerald-500/10 text-emerald-300',
                  tone === 'warning' && 'border-amber-500/40 bg-amber-500/10 text-amber-200',
                  tone === 'neutral' && 'border-border bg-card text-muted-foreground'
                )}
              >
                {status}
              </span>
              {version ? <VersionInfoButton label={title} version={version} /> : null}
            </div>
            <FieldDescription className='text-xs text-muted-foreground'>{description}</FieldDescription>
          </FieldContent>
        </div>
        <div className='flex shrink-0 flex-wrap gap-2 sm:justify-end'>{children}</div>
      </div>
    </Field>
  );
}
