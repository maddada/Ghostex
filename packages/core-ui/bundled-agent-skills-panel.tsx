import { useState } from 'react';
import {
  IconBrowser,
  IconCircleCheckFilled,
  IconCopy,
  IconDeviceDesktop,
  IconDownload,
  IconGitPullRequest,
  IconLayoutKanban,
  IconPencil,
  IconRefresh,
  IconSitemap,
  IconTerminal2,
  IconTrash,
} from '@tabler/icons-react';
import { Button } from '@/packages/components/ui/button';
import { Field, FieldContent, FieldDescription, FieldTitle } from '@/packages/components/ui/field';
import { cn } from '@/packages/components/utils';
import { AppTooltip } from './app-tooltip';
import { DisabledSettingControlTooltip } from './disabled-setting-control-tooltip';
import {
  BUNDLED_GHOSTEX_AGENT_SKILLS,
  VISIBLE_BUNDLED_GHOSTEX_AGENT_SKILLS,
  GHOSTEX_TRYCUA_PRODUCT_NAME,
  type BundledGhostexAgentSkill,
  type BundledGhostexAgentSkillId,
  type BundledGhostexAgentSkillTier,
} from '../shared/ghostex-agent-skills';
import type { SidebarGhostexCliStatusMessage } from '../shared/session-grid-contract';

export type BundledAgentSkillInstallHandlers = Partial<Record<BundledGhostexAgentSkillId, () => void>>;

export type BundledAgentSkillUninstallHandler = (skillId: BundledGhostexAgentSkillId) => void;

type BundledAgentSkillsPanelProps = {
  className?: string;
  ghostexCliStatus?: SidebarGhostexCliStatusMessage;
  ghostexCliStatusLoading?: boolean;
  onInstallCuaDriver?: () => void;
  onInstallSkill?: BundledAgentSkillInstallHandlers;
  onRefreshStatus?: () => void;
  onUninstallAllSkills?: () => void;
  onUninstallSkill?: BundledAgentSkillUninstallHandler;
  showHeader?: boolean;
};

/*
 * CDXC:AgentSkills 2026-08-19:
 * The bundled skills list is ordered by how much a new user needs it, so the
 * install surfaces split it into Recommended and Optional instead of showing
 * eight equally-weighted rows. Tiers live in the shared catalog so Settings and
 * first launch cannot disagree about what is recommended.
 */
const BUNDLED_AGENT_SKILL_TIER_SECTIONS: readonly {
  description: string;
  tier: BundledGhostexAgentSkillTier;
  title: string;
}[] = [
  {
    description:
      'What most people want on day one: agents that can drive your machine, your browser, and Ghostex itself.',
    tier: 'recommended',
    title: 'Recommended',
  },
  {
    description: 'A handy extra for power users. Install it whenever you need it.',
    tier: 'optional',
    title: 'Optional',
  },
];

const BUNDLED_AGENT_SKILL_ICONS: Record<BundledGhostexAgentSkillId, typeof IconBrowser> = {
  browserUse: IconBrowser,
  cli: IconTerminal2,
  computerUse: IconDeviceDesktop,
  embeddedBrowserUse: IconBrowser,
  fable56Orchestration: IconSitemap,
  generateTitle: IconPencil,
  manageBeads: IconLayoutKanban,
  moveCodexSession: IconGitPullRequest,
};

/**
 * CDXC:AgentSkills 2026-05-31-09:18:
 * Users must explicitly install each bundled Ghostex skill instead of learning
 * after the fact that CLI setup copied agent instructions into ~/.agents/skills.
 * This panel is shared by Settings and first launch so each bundled skill has
 * the same explanation, status, command, and individual install button.
 */
export function BundledAgentSkillsPanel({
  className,
  ghostexCliStatus,
  ghostexCliStatusLoading = false,
  onInstallCuaDriver,
  onInstallSkill,
  onRefreshStatus,
  onUninstallAllSkills,
  onUninstallSkill,
  showHeader = true,
}: BundledAgentSkillsPanelProps) {
  const cliReady = ghostexCliStatus?.installed === true;
  const cuaDriverInstalled = ghostexCliStatus?.cuaDriverInstalled === true;
  const anySkillInstalled = BUNDLED_GHOSTEX_AGENT_SKILLS.some((skill) =>
    isBundledGhostexAgentSkillInstalled(skill.id, ghostexCliStatus)
  );

  return (
    <div className={cn('flex flex-col gap-3', className)}>
      {showHeader ? (
        <div className='flex flex-col gap-1'>
          <h3 className='text-sm font-semibold'>Bundled Agent Skills</h3>
          <p className='text-xs text-muted-foreground'>
            Install the Ghostex skills you want agents to discover. Each skill is copied to ~/.agents/skills and can be
            updated independently.
          </p>
        </div>
      ) : null}
      {BUNDLED_AGENT_SKILL_TIER_SECTIONS.map((section) => {
        const tierSkills = VISIBLE_BUNDLED_GHOSTEX_AGENT_SKILLS.filter((skill) => skill.tier === section.tier);
        /*
         * CDXC:TrycuaPrerequisite 2026-08-24:
         * Trycua is one install shared by every skill that drives the machine,
         * so it gets one card per tier and the skills that depend on it are
         * nested under that card. Repeating the install control inside each
         * dependent skill row made it look like two separate installs.
         */
        const trycuaSkills = tierSkills.filter((skill) => skill.requiresCuaDriver === true);
        const standaloneSkills = tierSkills.filter((skill) => skill.requiresCuaDriver !== true);
        const renderSkill = (skill: BundledGhostexAgentSkill) => (
          <BundledAgentSkillRow
            cliReady={cliReady}
            cuaDriverInstalled={cuaDriverInstalled}
            ghostexCliStatus={ghostexCliStatus}
            ghostexCliStatusLoading={ghostexCliStatusLoading}
            key={skill.id}
            onInstall={onInstallSkill?.[skill.id]}
            onUninstall={onUninstallSkill ? () => onUninstallSkill(skill.id) : undefined}
            skill={skill}
          />
        );

        return (
          <div className='flex flex-col gap-3' key={section.tier}>
            <div className='flex flex-col gap-0.5'>
              <h4 className='text-xs font-semibold uppercase tracking-wide text-muted-foreground'>{section.title}</h4>
              <p className='text-xs text-muted-foreground'>{section.description}</p>
            </div>
            {trycuaSkills.length > 0 ? (
              <div className='flex flex-col gap-2'>
                <TrycuaPrerequisiteCard
                  cuaDriverInstalled={cuaDriverInstalled}
                  dependentSkillNames={trycuaSkills.map((skill) => skill.name)}
                  ghostexCliStatus={ghostexCliStatus}
                  ghostexCliStatusLoading={ghostexCliStatusLoading}
                  onInstallCuaDriver={onInstallCuaDriver}
                />
                <div className='ml-3 flex flex-col gap-3 border-l-2 border-muted-foreground/25 pl-3'>
                  <p className='text-[11px] font-semibold uppercase tracking-wide text-muted-foreground'>
                    Step 2: install the skills that use {GHOSTEX_TRYCUA_PRODUCT_NAME}
                  </p>
                  {trycuaSkills.map(renderSkill)}
                </div>
              </div>
            ) : null}
            {standaloneSkills.map(renderSkill)}
          </div>
        );
      })}
      {onRefreshStatus || onUninstallAllSkills ? (
        <div className='flex flex-wrap justify-end gap-1.5'>
          {/*
           * CDXC:AgentSkills 2026-08-19-11:20:
           * Skill removal lives beside the install controls it undoes: an icon-only
           * remove on each installed row, plus one Uninstall All for the whole set.
           * Uninstall All stays disabled when no bundled skill is present so the
           * footer cannot fire a no-op removal.
           */}
          {onUninstallAllSkills ? (
            <DisabledSettingControlTooltip
              disabled={ghostexCliStatusLoading || !anySkillInstalled}
              reason={
                ghostexCliStatusLoading ? 'Skill status is being checked.' : 'No bundled Ghostex skills are installed.'
              }
            >
              <Button
                disabled={ghostexCliStatusLoading || !anySkillInstalled}
                onClick={onUninstallAllSkills}
                type='button'
                variant='outline'
              >
                <IconTrash aria-hidden='true' data-icon='inline-start' />
                Uninstall All
              </Button>
            </DisabledSettingControlTooltip>
          ) : null}
          {onRefreshStatus ? (
            <DisabledSettingControlTooltip disabled={ghostexCliStatusLoading} reason='Skill status is being checked.'>
              <Button disabled={ghostexCliStatusLoading} onClick={onRefreshStatus} type='button' variant='ghost'>
                <IconRefresh aria-hidden='true' data-icon='inline-start' />
                Refresh Skill Status
              </Button>
            </DisabledSettingControlTooltip>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

function BundledAgentSkillRow({
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
  const Icon = BUNDLED_AGENT_SKILL_ICONS[skill.id];
  const installDisabled = ghostexCliStatusLoading || !cliReady || !onInstall;
  const installDisabledReason = ghostexCliStatusLoading
    ? 'Skill status is being checked.'
    : !cliReady
      ? 'Install or repair the Ghostex CLI first.'
      : 'Skill installation isn’t available here.';
  const uninstallDisabled = ghostexCliStatusLoading || !onUninstall;
  const uninstallDisabledReason = ghostexCliStatusLoading
    ? 'Skill status is being checked.'
    : 'Skill removal isn’t available here.';

  return (
    <Field className='rounded-none border border-border bg-muted/20 px-4 py-3'>
      <div className='flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between'>
        <div className='flex min-w-0 gap-3'>
          <span className='mt-0.5 inline-flex size-8 shrink-0 items-center justify-center rounded-none bg-muted text-muted-foreground'>
            <Icon aria-hidden='true' size={17} />
          </span>
          <FieldContent>
            <div className='mb-1.5 flex flex-wrap items-center gap-2'>
              <FieldTitle className='text-sm'>{skill.name}</FieldTitle>
              {skill.requiresCuaDriver ? (
                /*
                 * CDXC:TrycuaPrerequisite 2026-08-24:
                 * The dependency stays legible on each skill row even after the
                 * install control moved up to the shared Trycua card, and turns
                 * into a warning while Trycua is still missing.
                 */
                <span
                  className={cn(
                    'inline-flex rounded-none border px-2 py-0.5 text-[11px] font-semibold',
                    cuaDriverInstalled || ghostexCliStatusLoading
                      ? 'border-border bg-card text-muted-foreground'
                      : 'border-amber-500/40 bg-amber-500/10 text-amber-200'
                  )}
                >
                  {cuaDriverInstalled || ghostexCliStatusLoading
                    ? `Uses ${GHOSTEX_TRYCUA_PRODUCT_NAME}`
                    : `Needs ${GHOSTEX_TRYCUA_PRODUCT_NAME}`}
                </span>
              ) : null}
            </div>
            <FieldDescription className='text-xs text-muted-foreground'>{skill.description}</FieldDescription>
            <code className='mt-2 block select-text rounded-none border border-border bg-muted/40 px-2.5 py-1.5 text-xs text-muted-foreground'>
              {skill.command}
            </code>
          </FieldContent>
        </div>
        <div className='flex w-[110px] shrink-0 flex-wrap gap-1 sm:justify-end'>
          <DisabledSettingControlTooltip disabled={installDisabled} reason={installDisabledReason}>
            <Button
              className={cn('w-[110px]', installed && 'w-[74px] px-2.5')}
              disabled={installDisabled}
              onClick={onInstall}
              type='button'
              variant='default'
            >
              {installed ? (
                'Reinstall'
              ) : (
                <>
                  <IconDownload aria-hidden='true' data-icon='inline-start' />
                  Install Skill
                </>
              )}
            </Button>
          </DisabledSettingControlTooltip>
          {installed ? (
            <DisabledSettingControlTooltip disabled={uninstallDisabled} reason={uninstallDisabledReason}>
              <AppTooltip content={`Uninstall ${skill.name}`}>
                <Button
                  aria-label={`Uninstall ${skill.name}`}
                  disabled={uninstallDisabled}
                  onClick={onUninstall}
                  size='icon'
                  type='button'
                  variant='destructive'
                >
                  <IconTrash aria-hidden='true' />
                </Button>
              </AppTooltip>
            </DisabledSettingControlTooltip>
          ) : null}
        </div>
      </div>
    </Field>
  );
}

/*
 * CDXC:AgentSkills 2026-08-19:
 * Ghostex Computer Use and Ghostex Browser Use both drive the real machine
 * through Trycua, so the install surfaces ask about that one-time install right
 * here instead of letting the user find out when an agent first tries to click
 * something.
 *
 * CDXC:TrycuaPrerequisite 2026-08-24:
 * One card for the one Trycua install, shown above the skills that depend on
 * it. It shows the exact command the host will run so the install is never a
 * black box, and the button runs that same command in a command-pane terminal.
 */
function TrycuaPrerequisiteCard({
  cuaDriverInstalled,
  dependentSkillNames,
  ghostexCliStatus,
  ghostexCliStatusLoading,
  onInstallCuaDriver,
}: {
  cuaDriverInstalled: boolean;
  dependentSkillNames: readonly string[];
  ghostexCliStatus?: SidebarGhostexCliStatusMessage;
  ghostexCliStatusLoading: boolean;
  onInstallCuaDriver?: () => void;
}) {
  const installCommand = ghostexCliStatus?.cuaDriverInstallCommand;
  const status =
    ghostexCliStatusLoading && !ghostexCliStatus ? 'Checking' : cuaDriverInstalled ? 'Installed' : 'Not installed';

  return (
    <Field className='rounded-none border border-border bg-muted/20 px-4 py-3'>
      <div className='flex flex-col gap-3'>
        <div className='flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between'>
          <div className='flex min-w-0 gap-3'>
            <span className='mt-0.5 inline-flex size-8 shrink-0 items-center justify-center rounded-none bg-muted text-muted-foreground'>
              <IconDeviceDesktop aria-hidden='true' size={17} />
            </span>
            <FieldContent>
              <div className='mb-1.5 flex flex-wrap items-center gap-2'>
                <span className='text-[11px] font-semibold uppercase tracking-wide text-muted-foreground'>Step 1</span>
                <FieldTitle className='text-sm'>{GHOSTEX_TRYCUA_PRODUCT_NAME}</FieldTitle>
                <span
                  className={cn(
                    'inline-flex rounded-none border px-2 py-0.5 text-[11px] font-semibold',
                    cuaDriverInstalled
                      ? 'border-emerald-500/40 bg-emerald-500/10 text-emerald-300'
                      : ghostexCliStatusLoading && !ghostexCliStatus
                        ? 'border-border bg-card text-muted-foreground'
                        : 'border-amber-500/40 bg-amber-500/10 text-amber-200'
                  )}
                >
                  {status}
                </span>
              </div>
              <FieldDescription className='text-xs text-muted-foreground'>
                {GHOSTEX_TRYCUA_PRODUCT_NAME} is a utility that lets any agent control your machine: clicking, typing,
                and seeing what is on screen. {formatDependentSkillNames(dependentSkillNames)} run through it, so
                install {GHOSTEX_TRYCUA_PRODUCT_NAME} once and then install those skills below.
              </FieldDescription>
            </FieldContent>
          </div>
          <div className='flex shrink-0 flex-wrap gap-2 sm:justify-end'>
            <DisabledSettingControlTooltip
              disabled={ghostexCliStatusLoading || cuaDriverInstalled || !onInstallCuaDriver}
              reason={
                ghostexCliStatusLoading
                  ? `${GHOSTEX_TRYCUA_PRODUCT_NAME} status is being checked.`
                  : cuaDriverInstalled
                    ? `${GHOSTEX_TRYCUA_PRODUCT_NAME} is already installed.`
                    : `${GHOSTEX_TRYCUA_PRODUCT_NAME} installation isn’t available here.`
              }
            >
              <Button
                disabled={ghostexCliStatusLoading || cuaDriverInstalled || !onInstallCuaDriver}
                onClick={onInstallCuaDriver}
                type='button'
                variant={cuaDriverInstalled ? 'outline' : 'default'}
              >
                {cuaDriverInstalled ? (
                  <IconCircleCheckFilled aria-hidden='true' data-icon='inline-start' />
                ) : (
                  <IconDownload aria-hidden='true' data-icon='inline-start' />
                )}
                {cuaDriverInstalled
                  ? `${GHOSTEX_TRYCUA_PRODUCT_NAME} Installed`
                  : `Install ${GHOSTEX_TRYCUA_PRODUCT_NAME}`}
              </Button>
            </DisabledSettingControlTooltip>
          </div>
        </div>
        {!cuaDriverInstalled && installCommand ? (
          <div className='flex flex-col gap-1.5'>
            <p className='text-[11px] text-muted-foreground'>
              Install {GHOSTEX_TRYCUA_PRODUCT_NAME} runs this command in a command pane terminal so you can watch it
              finish. You can also run it yourself:
            </p>
            <div className='flex items-start gap-1.5'>
              <code className='block min-w-0 flex-1 overflow-x-auto whitespace-pre rounded-none border border-border bg-muted/40 px-2.5 py-1.5 text-xs text-muted-foreground'>
                {installCommand}
              </code>
              <CopyCommandButton command={installCommand} />
            </div>
          </div>
        ) : null}
      </div>
    </Field>
  );
}

function CopyCommandButton({ command }: { command: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <AppTooltip content={copied ? 'Copied' : 'Copy command'}>
      <Button
        aria-label={`Copy the ${GHOSTEX_TRYCUA_PRODUCT_NAME} install command`}
        onClick={() => {
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
        variant='outline'
      >
        {copied ? <IconCircleCheckFilled aria-hidden='true' /> : <IconCopy aria-hidden='true' />}
      </Button>
    </AppTooltip>
  );
}

function formatDependentSkillNames(names: readonly string[]): string {
  if (names.length === 0) {
    return 'The skills below';
  }
  if (names.length === 1) {
    return names[0] as string;
  }
  return `${names.slice(0, -1).join(', ')} and ${names[names.length - 1] as string}`;
}

function isBundledGhostexAgentSkillInstalled(
  skillId: BundledGhostexAgentSkillId,
  status?: SidebarGhostexCliStatusMessage
): boolean {
  switch (skillId) {
    case 'browserUse':
      return status?.browserSkillInstalled === true;
    case 'embeddedBrowserUse':
      return status?.embeddedBrowserSkillInstalled === true;
    case 'computerUse':
      return status?.computerUseSkillInstalled === true;
    case 'cli':
      return status?.cliSkillInstalled === true;
    case 'fable56Orchestration':
      return status?.fable56OrchestrationSkillInstalled === true;
    case 'manageBeads':
      return status?.manageBeadsSkillInstalled === true;
    case 'generateTitle':
      return status?.generateTitleSkillInstalled === true;
    case 'manageBeads':
      return status?.manageBeadsSkillInstalled === true;
    case 'moveCodexSession':
      return status?.moveCodexSessionSkillInstalled === true;
  }
}
