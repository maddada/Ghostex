import {
  IconBrowser,
  IconCircleCheckFilled,
  IconDeviceDesktop,
  IconDownload,
  IconExternalLink,
  IconGitPullRequest,
  IconHistory,
  IconPencil,
  IconRefresh,
  IconSitemap,
  IconTrash,
} from "@tabler/icons-react";
import { Button } from "@/components/ui/button";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldTitle,
} from "@/components/ui/field";
import { cn } from "@/lib/utils";
import { AppTooltip } from "./app-tooltip";
import { DisabledSettingControlTooltip } from "./disabled-setting-control-tooltip";
import {
  BUNDLED_GHOSTEX_AGENT_SKILLS,
  GHOSTEX_CUA_PROJECT_URL,
  type BundledGhostexAgentSkill,
  type BundledGhostexAgentSkillId,
  type BundledGhostexAgentSkillTier,
} from "../shared/ghostex-agent-skills";
import type { SidebarGhostexCliStatusMessage } from "../shared/session-grid-contract";

export type BundledAgentSkillInstallHandlers = Partial<
  Record<BundledGhostexAgentSkillId, () => void>
>;

export type BundledAgentSkillUninstallHandler = (
  skillId: BundledGhostexAgentSkillId,
) => void;

type BundledAgentSkillsPanelProps = {
  className?: string;
  ghostexCliStatus?: SidebarGhostexCliStatusMessage;
  ghostexCliStatusLoading?: boolean;
  onInstallCuaDriver?: () => void;
  onInstallSkill?: BundledAgentSkillInstallHandlers;
  onOpenExternalUrl?: (url: string) => void;
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
    description: "What most people want on day one — agents that can drive your Mac and your browser.",
    tier: "recommended",
    title: "Recommended",
  },
  {
    description: "Handy extras. Install them whenever you need them.",
    tier: "optional",
    title: "Optional",
  },
];

const BUNDLED_AGENT_SKILL_ICONS: Record<
  BundledGhostexAgentSkillId,
  typeof IconBrowser
> = {
  agentOrchestration: IconGitPullRequest,
  browserUse: IconBrowser,
  computerUse: IconDeviceDesktop,
  embeddedBrowserUse: IconBrowser,
  fable56Orchestration: IconSitemap,
  findPrevSession: IconHistory,
  generateTitle: IconPencil,
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
  onOpenExternalUrl,
  onRefreshStatus,
  onUninstallAllSkills,
  onUninstallSkill,
  showHeader = true,
}: BundledAgentSkillsPanelProps) {
  const cliReady = ghostexCliStatus?.installed === true;
  const cuaDriverInstalled = ghostexCliStatus?.cuaDriverInstalled === true;
  const anySkillInstalled = BUNDLED_GHOSTEX_AGENT_SKILLS.some((skill) =>
    isBundledGhostexAgentSkillInstalled(skill.id, ghostexCliStatus),
  );

  return (
    <div className={cn("flex flex-col gap-3", className)}>
      {showHeader ? (
        <div className="flex flex-col gap-1">
          <h3 className="text-sm font-semibold">Bundled Agent Skills</h3>
          <p className="text-xs text-muted-foreground">
            Install the Ghostex skills you want agents to discover. Each skill is copied to
            ~/.agents/skills and can be updated independently.
          </p>
        </div>
      ) : null}
      {BUNDLED_AGENT_SKILL_TIER_SECTIONS.map((section) => (
        <div className="flex flex-col gap-3" key={section.tier}>
          <div className="flex flex-col gap-0.5">
            <h4 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
              {section.title}
            </h4>
            <p className="text-xs text-muted-foreground">{section.description}</p>
          </div>
          {BUNDLED_GHOSTEX_AGENT_SKILLS.filter((skill) => skill.tier === section.tier).map(
            (skill) => (
              <BundledAgentSkillRow
                cliReady={cliReady}
                cuaDriverInstalled={cuaDriverInstalled}
                ghostexCliStatus={ghostexCliStatus}
                ghostexCliStatusLoading={ghostexCliStatusLoading}
                key={skill.id}
                onInstall={onInstallSkill?.[skill.id]}
                onInstallCuaDriver={onInstallCuaDriver}
                onOpenExternalUrl={onOpenExternalUrl}
                onUninstall={
                  onUninstallSkill ? () => onUninstallSkill(skill.id) : undefined
                }
                skill={skill}
              />
            ),
          )}
        </div>
      ))}
      {onRefreshStatus || onUninstallAllSkills ? (
        <div className="flex flex-wrap justify-end gap-1.5">
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
                ghostexCliStatusLoading
                  ? "Skill status is being checked."
                  : "No bundled Ghostex skills are installed."
              }
            >
              <Button
                disabled={ghostexCliStatusLoading || !anySkillInstalled}
                onClick={onUninstallAllSkills}
                type="button"
                variant="outline"
              >
                <IconTrash aria-hidden="true" data-icon="inline-start" />
                Uninstall All
              </Button>
            </DisabledSettingControlTooltip>
          ) : null}
          {onRefreshStatus ? (
            <DisabledSettingControlTooltip
              disabled={ghostexCliStatusLoading}
              reason="Skill status is being checked."
            >
              <Button
                disabled={ghostexCliStatusLoading}
                onClick={onRefreshStatus}
                type="button"
                variant="ghost"
              >
                <IconRefresh aria-hidden="true" data-icon="inline-start" />
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
  onInstallCuaDriver,
  onOpenExternalUrl,
  onUninstall,
  skill,
}: {
  cliReady: boolean;
  cuaDriverInstalled: boolean;
  ghostexCliStatus?: SidebarGhostexCliStatusMessage;
  ghostexCliStatusLoading: boolean;
  onInstall?: () => void;
  onInstallCuaDriver?: () => void;
  onOpenExternalUrl?: (url: string) => void;
  onUninstall?: () => void;
  skill: BundledGhostexAgentSkill;
}) {
  const installed = isBundledGhostexAgentSkillInstalled(skill.id, ghostexCliStatus);
  const Icon = BUNDLED_AGENT_SKILL_ICONS[skill.id];
  const installDisabled =
    ghostexCliStatusLoading || !cliReady || !onInstall;
  const installDisabledReason = ghostexCliStatusLoading
    ? "Skill status is being checked."
    : !cliReady
      ? "Install or repair the Ghostex CLI first."
      : "Skill installation isn’t available here.";
  const uninstallDisabled = ghostexCliStatusLoading || !onUninstall;
  const uninstallDisabledReason = ghostexCliStatusLoading
    ? "Skill status is being checked."
    : "Skill removal isn’t available here.";

  return (
    <Field className="rounded-none border border-border bg-muted/20 px-4 py-3">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div className="flex min-w-0 gap-3">
          <span className="mt-0.5 inline-flex size-8 shrink-0 items-center justify-center rounded-none bg-muted text-muted-foreground">
            <Icon aria-hidden="true" size={17} />
          </span>
          <FieldContent>
            <div className="mb-1.5 flex flex-wrap items-center gap-2">
              <FieldTitle className="text-sm">{skill.name}</FieldTitle>
            </div>
            <FieldDescription className="text-xs text-muted-foreground">
              {skill.description}
            </FieldDescription>
            <code className="mt-2 block select-text rounded-none border border-border bg-muted/40 px-2.5 py-1.5 text-xs text-muted-foreground">
              {skill.command}
            </code>
            {skill.requiresCuaDriver ? (
              <CuaDriverNote
                cuaDriverInstalled={cuaDriverInstalled}
                ghostexCliStatusLoading={ghostexCliStatusLoading}
                onInstallCuaDriver={onInstallCuaDriver}
                onOpenExternalUrl={onOpenExternalUrl}
              />
            ) : null}
          </FieldContent>
        </div>
        <div className="flex w-[110px] shrink-0 flex-wrap gap-1 sm:justify-end">
          <DisabledSettingControlTooltip
            disabled={installDisabled}
            reason={installDisabledReason}
          >
            <Button
              className={cn("w-[110px]", installed && "w-[74px] px-2.5")}
              disabled={installDisabled}
              onClick={onInstall}
              type="button"
              variant="default"
            >
              {installed ? (
                "Reinstall"
              ) : (
                <>
                  <IconDownload aria-hidden="true" data-icon="inline-start" />
                  Install Skill
                </>
              )}
            </Button>
          </DisabledSettingControlTooltip>
          {installed ? (
            <DisabledSettingControlTooltip
              disabled={uninstallDisabled}
              reason={uninstallDisabledReason}
            >
              <AppTooltip content={`Uninstall ${skill.name}`}>
                <Button
                  aria-label={`Uninstall ${skill.name}`}
                  disabled={uninstallDisabled}
                  onClick={onUninstall}
                  size="icon"
                  type="button"
                  variant="destructive"
                >
                  <IconTrash aria-hidden="true" />
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
 * through Cua Driver, so the skill row asks about that one-time install right
 * here instead of letting the user find out when an agent first tries to click
 * something.
 */
function CuaDriverNote({
  cuaDriverInstalled,
  ghostexCliStatusLoading,
  onInstallCuaDriver,
  onOpenExternalUrl,
}: {
  cuaDriverInstalled: boolean;
  ghostexCliStatusLoading: boolean;
  onInstallCuaDriver?: () => void;
  onOpenExternalUrl?: (url: string) => void;
}) {
  return (
    <div className="mt-2 flex flex-col gap-2 border border-border bg-muted/30 px-2.5 py-2">
      <p className="text-xs text-muted-foreground">
        {cuaDriverInstalled ? (
          <>
            <strong className="font-semibold text-foreground">Cua Driver is ready.</strong> This
            skill uses it to click, type, and see what is on screen.
          </>
        ) : (
          <>
            <strong className="font-semibold text-foreground">
              Want agents to really control your Mac and browser?
            </strong>{" "}
            This skill needs Cua Driver, the open-source helper from the Cua project. It is a
            one-time setup — install it once and every Ghostex agent can use it.
          </>
        )}
      </p>
      <div className="flex flex-wrap items-center gap-1.5">
        <Button
          disabled={ghostexCliStatusLoading || cuaDriverInstalled || !onInstallCuaDriver}
          onClick={onInstallCuaDriver}
          type="button"
          variant={cuaDriverInstalled ? "outline" : "default"}
        >
          {cuaDriverInstalled ? (
            <IconCircleCheckFilled aria-hidden="true" data-icon="inline-start" />
          ) : (
            <IconDownload aria-hidden="true" data-icon="inline-start" />
          )}
          {cuaDriverInstalled ? "Cua Driver Installed" : "Install Cua Driver"}
        </Button>
        <Button
          disabled={!onOpenExternalUrl}
          onClick={() => onOpenExternalUrl?.(GHOSTEX_CUA_PROJECT_URL)}
          type="button"
          variant="ghost"
        >
          <IconExternalLink aria-hidden="true" data-icon="inline-start" />
          trycua/cua
        </Button>
      </div>
    </div>
  );
}

function isBundledGhostexAgentSkillInstalled(
  skillId: BundledGhostexAgentSkillId,
  status?: SidebarGhostexCliStatusMessage,
): boolean {
  switch (skillId) {
    case "browserUse":
      return status?.browserSkillInstalled === true;
    case "embeddedBrowserUse":
      return status?.embeddedBrowserSkillInstalled === true;
    case "computerUse":
      return status?.computerUseSkillInstalled === true;
    case "agentOrchestration":
      return status?.agentOrchestrationSkillInstalled === true;
    case "fable56Orchestration":
      return status?.fable56OrchestrationSkillInstalled === true;
    case "findPrevSession":
      return status?.findPrevSessionSkillInstalled === true;
    case "generateTitle":
      return status?.generateTitleSkillInstalled === true;
    case "moveCodexSession":
      return status?.moveCodexSessionSkillInstalled === true;
  }
}
