import { useState } from 'react';
import {
  IconCircleArrowUp,
  IconCircleCheck,
  IconCircleCheckFilled,
  IconCloudSearch,
  IconCopy,
  IconDownload,
  IconRefresh,
  IconSettings,
  IconTrash,
} from '@tabler/icons-react';
import { cn } from '@/packages/components/utils';
import { AppTooltip } from '../../app-tooltip';
import {
  BUNDLED_AGENT_SKILL_ICONS,
  isBundledGhostexAgentSkillInstalled,
  type BundledAgentSkillInstallHandlers,
  type BundledAgentSkillUninstallHandler,
} from '../../bundled-agent-skills-panel';
import {
  GHOSTEX_TRYCUA_PRODUCT_NAME,
  VISIBLE_BUNDLED_GHOSTEX_AGENT_SKILLS,
  type BundledGhostexAgentSkill,
  type BundledGhostexAgentSkillTier,
} from '../../../shared/ghostex-agent-skills';
import { type SidebarGhostexCliStatusMessage } from '../../../shared/session-grid-contract';
import { SettingButton, SettingDescriptionTooltip, SettingsListItem, SettingsSection } from '../fields';
import { playCopySound } from '../../copy-sound';

export type IntegrationStatusTone = 'success' | 'warning' | 'neutral';

/**
 * CDXC:Settings 2026-09-09 DECISION:
 * User picked the "quiet rows" mockup (docs/2026-09-09/integrations-settings) for the Integrations page: Trycua is its own Desktop control section of plain rows, skills are one row each with the description and install command behind the hover info icon, Recommended and Optional are caption rows inside one card, and Refresh plus Uninstall all live in the section header. The nested skills panel with step numbers stays only on first launch.
 */
export function IntegrationStatusPill({ children, tone }: { children: string; tone: IntegrationStatusTone }) {
  return (
    <span
      className={cn(
        'inline-flex rounded-none border px-2 py-0.5 text-[11px]',
        tone === 'success' && 'border-emerald-500/40 bg-emerald-500/10 text-emerald-300',
        tone === 'warning' && 'border-amber-500/40 bg-amber-500/10 text-amber-200',
        tone === 'neutral' && 'border-border bg-card text-muted-foreground'
      )}
    >
      {children}
    </span>
  );
}

export function IntegrationRowTitle({
  badge,
  description,
  label,
  pill,
}: {
  /** A short product-state badge such as Beta. */
  badge?: string;
  description: string;
  label: string;
  /** A dependency note such as Needs Trycua. Install state itself is the row's status dot, never a pill. */
  pill?: { text: string; tone: IntegrationStatusTone };
}) {
  return (
    <span className='settings-row-label-line flex flex-wrap items-center gap-2'>
      <span>{label}</span>
      {badge ? (
        <span className='ml-1.5 inline-flex rounded-none border border-sky-500/40 bg-sky-500/10 px-2 py-0.5 text-[11px] text-sky-200'>
          {badge}
        </span>
      ) : null}
      {pill ? (
        <span className='ml-1.5 inline-flex'>
          <IntegrationStatusPill tone={pill.tone}>{pill.text}</IntegrationStatusPill>
        </span>
      ) : null}
      <SettingDescriptionTooltip description={description} label={label} />
    </span>
  );
}

export function DesktopControlSection({
  ghostexCliStatus,
  ghostexCliStatusLoading,
  onCheckCuaDriverUpdate,
  onInstallCuaDriver,
  onOpenAccessibilityPreferences,
  onOpenScreenRecordingPreferences,
  onReinstallCuaDriver,
  onUninstallCuaDriver,
  permissionStatus,
  showPermissions,
  showTrycua,
}: {
  ghostexCliStatus?: SidebarGhostexCliStatusMessage;
  ghostexCliStatusLoading: boolean;
  onCheckCuaDriverUpdate?: () => void;
  /** Installs Trycua when missing; on hosts with managed updates it updates an installed Trycua. */
  onInstallCuaDriver?: () => void;
  onOpenAccessibilityPreferences?: () => void;
  onOpenScreenRecordingPreferences?: () => void;
  onReinstallCuaDriver?: () => void;
  onUninstallCuaDriver?: () => void;
  permissionStatus: { status: string; tone: IntegrationStatusTone };
  showPermissions: boolean;
  showTrycua: boolean;
}) {
  const cuaDriverInstalled = ghostexCliStatus?.cuaDriverInstalled === true;
  const cuaDriverVersion = ghostexCliStatus?.cuaDriverVersion;
  const installCommand = ghostexCliStatus?.cuaDriverInstallCommand;
  const installDisabled = ghostexCliStatusLoading || cuaDriverInstalled || !onInstallCuaDriver;
  if (!showTrycua && !showPermissions) {
    return null;
  }
  return (
    <SettingsSection title='Desktop control'>
      {showTrycua ? (
        <SettingsListItem
          icon={<BUNDLED_AGENT_SKILL_ICONS.computerUse aria-hidden='true' size={17} />}
          status={ghostexCliStatusLoading ? 'neutral' : cuaDriverInstalled ? 'success' : 'warning'}
          title={
            <IntegrationRowTitle
              description={`${cuaDriverInstalled ? (cuaDriverVersion ? `Version ${cuaDriverVersion} installed. ` : 'Installed. ') : ''}${GHOSTEX_TRYCUA_PRODUCT_NAME} is a utility that lets any agent control your machine: clicking, typing, and seeing what is on screen. Ghostex Computer Use and Ghostex Browser Use run through it, so install it once and then install those skills below.`}
              label={GHOSTEX_TRYCUA_PRODUCT_NAME}
            />
          }
        >
          {cuaDriverInstalled ? (
            <TrycuaInstalledActions
              ghostexCliStatus={ghostexCliStatus}
              ghostexCliStatusLoading={ghostexCliStatusLoading}
              onCheckUpdate={onCheckCuaDriverUpdate}
              onReinstall={onReinstallCuaDriver}
              onUninstall={onUninstallCuaDriver}
              onUpdate={onInstallCuaDriver}
            />
          ) : (
            <SettingButton
              disabled={installDisabled}
              disabledReason={
                ghostexCliStatusLoading
                  ? `${GHOSTEX_TRYCUA_PRODUCT_NAME} status is being checked.`
                  : `${GHOSTEX_TRYCUA_PRODUCT_NAME} installation isn’t available here.`
              }
              onClick={onInstallCuaDriver}
              type='button'
              variant='outline'
            >
              <IconDownload aria-hidden='true' data-icon='inline-start' />
              Install {GHOSTEX_TRYCUA_PRODUCT_NAME}
            </SettingButton>
          )}
        </SettingsListItem>
      ) : null}
      {showTrycua && !cuaDriverInstalled && installCommand ? (
        <SettingsListItem
          title={
            <IntegrationRowTitle
              description={`Install ${GHOSTEX_TRYCUA_PRODUCT_NAME} runs this command in a command pane terminal so you can watch it finish. You can also run it yourself.`}
              label='Install command'
            />
          }
        >
          <code className='settings-inline-code max-w-[26rem] truncate'>{installCommand}</code>
          <CopyCommandButton command={installCommand} />
        </SettingsListItem>
      ) : null}
      {showPermissions ? (
        <SettingsListItem
          icon={<IconSettings aria-hidden='true' size={17} />}
          status={permissionStatus.tone}
          title={
            <IntegrationRowTitle
              description={`${permissionStatus.status}. ${GHOSTEX_TRYCUA_PRODUCT_NAME} needs Accessibility to click and type in apps, and Screen Recording to understand what is visible on the desktop.`}
              label='System permissions'
            />
          }
        >
          <SettingButton
            disabled={!onOpenAccessibilityPreferences}
            disabledReason='Accessibility settings aren’t available here.'
            onClick={onOpenAccessibilityPreferences}
            type='button'
            variant='ghost'
          >
            Accessibility
          </SettingButton>
          <SettingButton
            disabled={!onOpenScreenRecordingPreferences}
            disabledReason='Screen Recording settings aren’t available here.'
            onClick={onOpenScreenRecordingPreferences}
            type='button'
            variant='ghost'
          >
            Screen Recording
          </SettingButton>
        </SettingsListItem>
      ) : null}
    </SettingsSection>
  );
}

/**
 * CDXC:Settings 2026-09-26 DECISION:
 * An installed Trycua row gets one-click Update, an "up to date" state, Reinstall and Uninstall (which keeps the system permissions), as icon-only buttons so the Desktop control area stays quiet; versions live in the tooltips, not in row text.
 */
function TrycuaInstalledActions({
  ghostexCliStatus,
  ghostexCliStatusLoading,
  onCheckUpdate,
  onReinstall,
  onUninstall,
  onUpdate,
}: {
  ghostexCliStatus?: SidebarGhostexCliStatusMessage;
  ghostexCliStatusLoading: boolean;
  onCheckUpdate?: () => void;
  onReinstall?: () => void;
  onUninstall?: () => void;
  onUpdate?: () => void;
}) {
  const name = GHOSTEX_TRYCUA_PRODUCT_NAME;
  const current = ghostexCliStatus?.cuaDriverVersion;
  const latest = ghostexCliStatus?.cuaDriverLatestVersion;
  const updateAvailable = ghostexCliStatus?.cuaDriverUpdateAvailable;
  const checkingReason = `${name} status is being checked.`;
  const installedSuffix = current ? ` (installed v${current})` : '';
  const upToDateVersion = current ?? latest;
  const update =
    updateAvailable === true
      ? {
          Icon: IconCircleArrowUp,
          className: 'text-sky-400',
          label: `Update ${name}`,
          onClick: onUpdate,
          tooltip: latest ? `Update ${name} to v${latest}${installedSuffix}` : `Update ${name}${installedSuffix}`,
        }
      : updateAvailable === false
        ? {
            Icon: IconCircleCheck,
            className: 'text-muted-foreground',
            label: `Check for ${name} updates`,
            onClick: onCheckUpdate,
            tooltip: `${name}${upToDateVersion ? ` v${upToDateVersion}` : ''} is up to date. Click to check again.`,
          }
        : {
            Icon: IconCloudSearch,
            className: undefined,
            label: `Check for ${name} updates`,
            onClick: onCheckUpdate,
            tooltip: `Check for ${name} updates${installedSuffix}`,
          };
  return (
    <>
      {ghostexCliStatus?.cuaDriverManagedUpdatesSupported === true ? (
        <AppTooltip content={update.tooltip}>
          <SettingButton
            aria-label={update.label}
            className={update.className}
            disabled={ghostexCliStatusLoading || !update.onClick}
            disabledReason={ghostexCliStatusLoading ? checkingReason : `${name} updates aren’t available here.`}
            onClick={update.onClick}
            size='icon'
            type='button'
            variant='ghost'
          >
            <update.Icon aria-hidden='true' />
          </SettingButton>
        </AppTooltip>
      ) : null}
      <AppTooltip content={`Reinstall the latest ${name} with the official installer${installedSuffix}`}>
        <SettingButton
          aria-label={`Reinstall ${name}`}
          disabled={ghostexCliStatusLoading || !onReinstall}
          disabledReason={ghostexCliStatusLoading ? checkingReason : `${name} reinstall isn’t available here.`}
          onClick={onReinstall}
          size='icon'
          type='button'
          variant='ghost'
        >
          <IconRefresh aria-hidden='true' />
        </SettingButton>
      </AppTooltip>
      <AppTooltip content={`Uninstall ${name} (keeps Accessibility and Screen Recording permissions)`}>
        <SettingButton
          aria-label={`Uninstall ${name}`}
          disabled={ghostexCliStatusLoading || !onUninstall}
          disabledReason={ghostexCliStatusLoading ? checkingReason : `${name} removal isn’t available here.`}
          onClick={onUninstall}
          size='icon'
          type='button'
          variant='ghost'
        >
          <IconTrash aria-hidden='true' />
        </SettingButton>
      </AppTooltip>
    </>
  );
}

const SKILL_TIERS: readonly { label: string; tier: BundledGhostexAgentSkillTier }[] = [
  { label: 'Recommended', tier: 'recommended' },
  { label: 'Optional', tier: 'optional' },
];

export function AgentSkillsSection({
  ghostexCliStatus,
  ghostexCliStatusLoading,
  onInstallSkill,
  onRefreshStatus,
  onUninstallAllSkills,
  onUninstallSkill,
}: {
  ghostexCliStatus?: SidebarGhostexCliStatusMessage;
  ghostexCliStatusLoading: boolean;
  onInstallSkill?: BundledAgentSkillInstallHandlers;
  onRefreshStatus?: () => void;
  onUninstallAllSkills?: () => void;
  onUninstallSkill?: BundledAgentSkillUninstallHandler;
}) {
  const cliReady = ghostexCliStatus?.installed === true;
  const cuaDriverInstalled = ghostexCliStatus?.cuaDriverInstalled === true;
  const anySkillInstalled = VISIBLE_BUNDLED_GHOSTEX_AGENT_SKILLS.some((skill) =>
    isBundledGhostexAgentSkillInstalled(skill.id, ghostexCliStatus)
  );
  return (
    <SettingsSection
      actions={
        <>
          <SettingButton
            disabled={ghostexCliStatusLoading || !onRefreshStatus}
            disabledReason='Skill status is being checked.'
            onClick={onRefreshStatus}
            type='button'
            variant='ghost'
          >
            <IconRefresh aria-hidden='true' data-icon='inline-start' />
            Refresh
          </SettingButton>
          <SettingButton
            disabled={ghostexCliStatusLoading || !anySkillInstalled || !onUninstallAllSkills}
            disabledReason={
              ghostexCliStatusLoading ? 'Skill status is being checked.' : 'No bundled Ghostex skills are installed.'
            }
            onClick={onUninstallAllSkills}
            type='button'
            variant='ghost'
          >
            <IconTrash aria-hidden='true' data-icon='inline-start' />
            Uninstall all
          </SettingButton>
        </>
      }
      title='Agent skills'
    >
      {SKILL_TIERS.map((tier) => {
        const skills = VISIBLE_BUNDLED_GHOSTEX_AGENT_SKILLS.filter((skill) => skill.tier === tier.tier);
        if (skills.length === 0) {
          return null;
        }
        return (
          <div className='settings-list-group' key={tier.tier}>
            <div className='settings-list-group-label'>{tier.label}</div>
            {skills.map((skill) => (
              <AgentSkillRow
                cliReady={cliReady}
                cuaDriverInstalled={cuaDriverInstalled}
                ghostexCliStatus={ghostexCliStatus}
                ghostexCliStatusLoading={ghostexCliStatusLoading}
                key={skill.id}
                onInstall={onInstallSkill?.[skill.id]}
                onUninstall={onUninstallSkill ? () => onUninstallSkill(skill.id) : undefined}
                skill={skill}
              />
            ))}
          </div>
        );
      })}
    </SettingsSection>
  );
}

function AgentSkillRow({
  cliReady,
  cuaDriverInstalled,
  ghostexCliStatus,
  ghostexCliStatusLoading,
  onInstall,
  onUninstall,
  skill,
}: {
  cliReady: boolean;
  cuaDriverInstalled: boolean;
  ghostexCliStatus?: SidebarGhostexCliStatusMessage;
  ghostexCliStatusLoading: boolean;
  onInstall?: () => void;
  onUninstall?: () => void;
  skill: BundledGhostexAgentSkill;
}) {
  const installed = isBundledGhostexAgentSkillInstalled(skill.id, ghostexCliStatus);
  const needsTrycua = skill.requiresCuaDriver === true && !cuaDriverInstalled && !ghostexCliStatusLoading;
  const Icon = BUNDLED_AGENT_SKILL_ICONS[skill.id];
  const installDisabled = ghostexCliStatusLoading || !cliReady || !onInstall;
  const installDisabledReason = ghostexCliStatusLoading
    ? 'Skill status is being checked.'
    : !cliReady
      ? 'Install or repair the Ghostex CLI first.'
      : 'Skill installation isn’t available here.';
  const uninstallDisabled = ghostexCliStatusLoading || !onUninstall;
  const pill = needsTrycua ? { text: `Needs ${GHOSTEX_TRYCUA_PRODUCT_NAME}`, tone: 'warning' as const } : undefined;
  return (
    <SettingsListItem
      icon={<Icon aria-hidden='true' size={17} />}
      status={installed ? 'success' : 'neutral'}
      title={
        <IntegrationRowTitle description={`${skill.description}\n\n${skill.command}`} label={skill.name} pill={pill} />
      }
    >
      <SettingButton
        className={cn(needsTrycua && !installed && 'opacity-60')}
        disabled={installDisabled}
        disabledReason={installDisabledReason}
        onClick={onInstall}
        type='button'
        variant={installed ? 'ghost' : 'outline'}
      >
        {installed ? (
          <IconRefresh aria-hidden='true' data-icon='inline-start' />
        ) : (
          <IconDownload aria-hidden='true' data-icon='inline-start' />
        )}
        {installed ? 'Reinstall' : 'Install'}
      </SettingButton>
      {installed ? (
        <AppTooltip content={`Uninstall ${skill.name}`}>
          <SettingButton
            aria-label={`Uninstall ${skill.name}`}
            disabled={uninstallDisabled}
            disabledReason={
              ghostexCliStatusLoading ? 'Skill status is being checked.' : 'Skill removal isn’t available here.'
            }
            onClick={onUninstall}
            size='icon'
            type='button'
            variant='ghost'
          >
            <IconTrash aria-hidden='true' />
          </SettingButton>
        </AppTooltip>
      ) : null}
    </SettingsListItem>
  );
}

function CopyCommandButton({ command }: { command: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <AppTooltip content={copied ? 'Copied' : 'Copy command'}>
      <SettingButton
        aria-label='Copy the install command'
        disabledReason='Copy isn’t available here.'
        onClick={() => {
          playCopySound();
          void navigator.clipboard.writeText(command).then(
            () => {
              setCopied(true);
              window.setTimeout(() => setCopied(false), 1200);
            },
            () => undefined
          );
        }}
        size='icon'
        type='button'
        variant='ghost'
      >
        {copied ? <IconCircleCheckFilled aria-hidden='true' /> : <IconCopy aria-hidden='true' />}
      </SettingButton>
    </AppTooltip>
  );
}
