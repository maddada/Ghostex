import {
  IconBrowser,
  IconCircleCheckFilled,
  IconDeviceDesktop,
  IconDownload,
  IconGitPullRequest,
  IconPencil,
  IconRefresh,
  IconSitemap,
} from "@tabler/icons-react";
import { Button } from "@/components/ui/button";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldTitle,
} from "@/components/ui/field";
import { cn } from "@/lib/utils";
import {
  BUNDLED_GHOSTEX_AGENT_SKILLS,
  type BundledGhostexAgentSkill,
  type BundledGhostexAgentSkillId,
} from "../shared/ghostex-agent-skills";
import type { SidebarGhostexCliStatusMessage } from "../shared/session-grid-contract";

export type BundledAgentSkillInstallHandlers = Partial<
  Record<BundledGhostexAgentSkillId, () => void>
>;

type BundledAgentSkillsPanelProps = {
  className?: string;
  ghostexCliStatus?: SidebarGhostexCliStatusMessage;
  ghostexCliStatusLoading?: boolean;
  onInstallSkill?: BundledAgentSkillInstallHandlers;
  onRefreshStatus?: () => void;
  showHeader?: boolean;
};

const BUNDLED_AGENT_SKILL_ICONS: Record<
  BundledGhostexAgentSkillId,
  typeof IconBrowser
> = {
  agentOrchestration: IconGitPullRequest,
  browserUse: IconBrowser,
  computerUse: IconDeviceDesktop,
  fable56Orchestration: IconSitemap,
  generateTitle: IconPencil,
  moveCodexSession: IconGitPullRequest,
};

/**
 * CDXC:AgentSkills 2026-05-31-09:18:
 * Users must explicitly install each bundled Ghostex skill instead of learning
 * after the fact that CLI setup copied agent instructions into ~/agents/skills.
 * This panel is shared by Settings and first launch so each bundled skill has
 * the same explanation, status, command, and individual install button.
 */
export function BundledAgentSkillsPanel({
  className,
  ghostexCliStatus,
  ghostexCliStatusLoading = false,
  onInstallSkill,
  onRefreshStatus,
  showHeader = true,
}: BundledAgentSkillsPanelProps) {
  const cliReady = ghostexCliStatus?.installed === true;

  return (
    <div className={cn("flex flex-col gap-3", className)}>
      {showHeader ? (
        <div className="flex flex-col gap-1">
          <h3 className="text-sm font-semibold">Bundled Agent Skills</h3>
          <p className="text-xs text-muted-foreground">
            Install the Ghostex skills you want agents to discover. Each skill is copied to
            ~/agents/skills and can be updated independently.
          </p>
        </div>
      ) : null}
      <div className="flex flex-col gap-3">
        {BUNDLED_GHOSTEX_AGENT_SKILLS.map((skill) => (
          <BundledAgentSkillRow
            cliReady={cliReady}
            ghostexCliStatus={ghostexCliStatus}
            ghostexCliStatusLoading={ghostexCliStatusLoading}
            key={skill.id}
            onInstall={onInstallSkill?.[skill.id]}
            skill={skill}
          />
        ))}
      </div>
      {onRefreshStatus ? (
        <div className="flex justify-end">
          <Button
            disabled={ghostexCliStatusLoading}
            onClick={onRefreshStatus}
            type="button"
            variant="ghost"
          >
            <IconRefresh aria-hidden="true" data-icon="inline-start" />
            Refresh Skill Status
          </Button>
        </div>
      ) : null}
    </div>
  );
}

function BundledAgentSkillRow({
  cliReady,
  ghostexCliStatus,
  ghostexCliStatusLoading,
  onInstall,
  skill,
}: {
  cliReady: boolean;
  ghostexCliStatus?: SidebarGhostexCliStatusMessage;
  ghostexCliStatusLoading: boolean;
  onInstall?: () => void;
  skill: BundledGhostexAgentSkill;
}) {
  const installed = isBundledGhostexAgentSkillInstalled(skill.id, ghostexCliStatus);
  const Icon = BUNDLED_AGENT_SKILL_ICONS[skill.id];

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
          </FieldContent>
        </div>
        <div className="flex shrink-0 flex-wrap gap-2 sm:justify-end">
          <Button
            className={cn(
              installed &&
                "border-emerald-500/40 bg-emerald-500/10 text-emerald-300 hover:bg-emerald-500/15 hover:text-emerald-200",
            )}
            disabled={ghostexCliStatusLoading || installed || !cliReady || !onInstall}
            onClick={onInstall}
            type="button"
            variant={installed ? "outline" : "default"}
          >
            {installed ? (
              <IconCircleCheckFilled aria-hidden="true" data-icon="inline-start" />
            ) : (
              <IconDownload aria-hidden="true" data-icon="inline-start" />
            )}
            {installed ? "Installed" : "Install Skill"}
          </Button>
        </div>
      </div>
    </Field>
  );
}

function isBundledGhostexAgentSkillInstalled(
  skillId: BundledGhostexAgentSkillId,
  status?: SidebarGhostexCliStatusMessage,
): boolean {
  switch (skillId) {
    case "browserUse":
      return status?.browserSkillInstalled === true;
    case "computerUse":
      return status?.computerUseSkillInstalled === true;
    case "agentOrchestration":
      return status?.agentOrchestrationSkillInstalled === true;
    case "fable56Orchestration":
      return status?.fable56OrchestrationSkillInstalled === true;
    case "generateTitle":
      return status?.generateTitleSkillInstalled === true;
    case "moveCodexSession":
      return status?.moveCodexSessionSkillInstalled === true;
  }
}
