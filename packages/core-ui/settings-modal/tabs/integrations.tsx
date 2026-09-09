import { useId, useState, type ReactNode } from 'react';
import { Button } from '@/packages/components/ui/button';
import { SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/packages/components/ui/select';
import { Switch } from '@/packages/components/ui/switch';
import { AppTooltip } from '../../app-tooltip';
import { IconDeviceDesktop, IconDownload, IconInfoCircle, IconRefresh, IconTerminal2 } from '@tabler/icons-react';
import { type SidebarGhostexCliStatusMessage } from '../../../shared/session-grid-contract';
import { APP_SHOTS_HOTKEY_OPTIONS, type AppShotsHotkey } from '../../../shared/ghostex-settings';
import { type BundledGhostexAgentSkillId } from '../../../shared/ghostex-agent-skills';
import { AgentSkillsSection, DesktopControlSection, IntegrationRowTitle } from './integration-skills';
import {
  SettingButton,
  SettingRow,
  SettingsListItem,
  SettingsNativeScrollArea,
  SettingsSection,
  SettingsSelect,
  SettingsSelectContent,
  SettingSwitch,
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
  onInstallHelpSkill,
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
  onInstallHelpSkill?: () => void;
  onUninstallBundledAgentSkill?: (skillId: BundledGhostexAgentSkillId) => void;
  onUninstallBundledAgentSkills?: () => void;
  onOpenAccessibilityPreferences?: () => void;
  onOpenScreenRecordingPreferences?: () => void;
  onRequestGhostexCliStatus?: () => void;
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
}) {
  const showIntegrationRow = (settingKey: string) => shouldShowSetting(search.sections.integrations, settingKey);
  const appShotsHotkeyId = useId();
  const appShotsMetadataId = useId();
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
          <SettingsSection title='Ghostex CLI'>
            {showIntegrationRow('ghostexCli') ? (
              <IntegrationSettingsRow
                description='Ghostex keeps the app-bundled ghostex command linked automatically for mobile apps and CLI-backed integration setup. gx is linked when that alias is available and not taken by another command.'
                icon={IconTerminal2}
                status={ghostexCliStatusChecking ? 'Checking' : cliReady ? 'Installed' : 'Not installed'}
                tone={ghostexCliStatusChecking ? 'neutral' : cliReady ? 'success' : 'warning'}
                title='Command line tool'
              >
                <SettingButton
                  disabled={ghostexCliStatusChecking || !onInstallGhostexCli}
                  disabledReason={
                    ghostexCliStatusChecking ? 'CLI status is being checked.' : 'CLI repair isn’t available here.'
                  }
                  onClick={onInstallGhostexCli}
                  type='button'
                  variant='outline'
                >
                  <IconDownload aria-hidden='true' data-icon='inline-start' />
                  Repair
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

            {/*
            CDXC:Settings 2026-06-19-14:51:
            macOS Settings > Integrations should not include a Setup Flow launcher row.
            Keep setup access owned by first-launch and other explicit entry points instead of listing it as an integration setting.
          */}
          </SettingsSection>
        ) : null}
        {shouldShowSettingsSection(search.sections.integrations) ? (
          <DesktopControlSection
            ghostexCliStatus={ghostexCliStatus}
            ghostexCliStatusLoading={ghostexCliStatusChecking}
            onInstallCuaDriver={onInstallCuaDriver}
            onOpenAccessibilityPreferences={onOpenAccessibilityPreferences}
            onOpenScreenRecordingPreferences={onOpenScreenRecordingPreferences}
            permissionStatus={cuaPermissionStatus}
            showPermissions={showIntegrationRow('cuaPermissions')}
            showTrycua={showIntegrationRow('bundledAgentSkills')}
          />
        ) : null}
        {shouldShowSettingsSection(search.sections.integrations) && showIntegrationRow('bundledAgentSkills') ? (
          <AgentSkillsSection
            ghostexCliStatus={ghostexCliStatus}
            ghostexCliStatusLoading={ghostexCliStatusChecking}
            onInstallSkill={{
              cli: onInstallCliSkill,
              browserUse: onInstallBrowserUseSkill,
              computerUse: onInstallComputerUseSkill,
              embeddedBrowserUse: onInstallBrowserControl,
              fable56Orchestration: onInstallFable56OrchestrationSkill,
              manageBeads: onInstallManageBeadsSkill,
              generateTitle: onInstallGenerateTitleSkill,
              moveCodexSession: onInstallMoveCodexSessionSkill,
              help: onInstallHelpSkill,
            }}
            onRefreshStatus={onRequestGhostexCliStatus}
            onUninstallAllSkills={onUninstallBundledAgentSkills}
            onUninstallSkill={onUninstallBundledAgentSkill}
          />
        ) : null}
        {/* CDXC:Settings 2026-09-09 DECISION: User: App Shots is its own section on the Integrations page, separate from the CLI, skills, and Trycua card. */}
        {shouldShowSettingsSection(search.sections.integrations) && showIntegrationRow('appShots') ? (
          <SettingsSection title='App Shots'>
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
            <IntegrationSettingsRow
              badge='Beta'
              description='Capture the frontmost app window, then stage it in the focused or recent agent session as local image context.'
              icon={IconDeviceDesktop}
              status={appShotsEnabled ? 'Enabled' : 'Disabled'}
              tone={appShotsEnabled ? 'success' : 'neutral'}
              title='App Shots'
            >
              <Switch
                aria-label='Enable App Shots'
                checked={appShotsEnabled}
                onCheckedChange={onAppShotsEnabledChange}
              />
            </IntegrationSettingsRow>
            <SettingRow
              description='Which Command key press captures the frontmost app window.'
              htmlFor={appShotsHotkeyId}
              label='App Shots hotkey'
            >
              <SettingsSelect
                disabled={!appShotsEnabled}
                disabledReason='Turn on App Shots first.'
                onValueChange={(value) => onAppShotsHotkeyChange(value as AppShotsHotkey)}
                value={appShotsHotkey}
              >
                <SelectTrigger aria-label='App Shots hotkey' id={appShotsHotkeyId}>
                  <SelectValue />
                </SelectTrigger>
                <SettingsSelectContent className='settings-list-select-content'>
                  <SelectGroup>
                    {APP_SHOTS_HOTKEY_OPTIONS.map((option) => (
                      <SelectItem key={option.value} value={option.value}>
                        {option.label}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SettingsSelectContent>
              </SettingsSelect>
            </SettingRow>
            <SettingRow
              description='Paste the window title and app name together with the image link.'
              htmlFor={appShotsMetadataId}
              label='App Shots metadata'
            >
              <SettingSwitch
                aria-label='Include App Shots metadata'
                checked={appShotsMetadataEnabled}
                disabled={!appShotsEnabled}
                disabledReason='Turn on App Shots first.'
                id={appShotsMetadataId}
                onCheckedChange={onAppShotsMetadataEnabledChange}
              />
            </SettingRow>
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
    <SettingsListItem
      icon={<Icon aria-hidden='true' size={17} />}
      status={tone}
      title={
        <span className='flex flex-wrap items-center gap-2'>
          {/*
           * CDXC:AppShots 2026-06-13-19:51:
           * Settings must visibly mark App Shots as Beta while keeping
           * the separate Enabled/Disabled status badge for its toggle
           * state.
           */}
          <IntegrationRowTitle badge={badge} description={`${status}. ${description}`} label={title} />
          {version ? <VersionInfoButton label={title} version={version} /> : null}
        </span>
      }
    >
      <div className='flex shrink-0 flex-wrap justify-end gap-2'>{children}</div>
    </SettingsListItem>
  );
}
