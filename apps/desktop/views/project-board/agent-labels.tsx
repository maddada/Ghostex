import { type ProjectAutomationAgentOption } from "@/packages/shared/automations";
import {
  AGENT_LOGO_COLORS,
  AGENT_LOGOS,
} from "@/packages/core-ui/agent-logos";
import {
  getSidebarAgentIconById,
  type SidebarAgentIcon,
} from "@/packages/shared/sidebar-agents";

export function automationAgentLabel(agents: ProjectAutomationAgentOption[], agentId: string): string {
  return agents.find((agent) => agent.agentId === agentId)?.label ?? agentId;
}

export function resolveAutomationAgentIcon(
  agent: Pick<ProjectAutomationAgentOption, "agentId" | "icon">,
): SidebarAgentIcon | undefined {
  return agent.icon ?? getSidebarAgentIconById(agent.agentId);
}

export function AutomationAgentOptionLabel({ agent }: { agent: ProjectAutomationAgentOption }) {
  const icon = resolveAutomationAgentIcon(agent);
  return (
    <span className="project-automation-agent-option">
      {icon ? <AutomationAgentIcon icon={icon} /> : null}
      <span>{agent.label}</span>
    </span>
  );
}

export function AutomationAgentIcon({ icon }: { icon: SidebarAgentIcon }) {
  return (
    <span
      aria-hidden="true"
      className="project-automation-agent-icon"
      data-agent-icon={icon}
      style={{
        backgroundColor: AGENT_LOGO_COLORS[icon],
        display: "block",
        flex: "0 0 auto",
        height: 14,
        maskImage: `url("${AGENT_LOGOS[icon]}")`,
        maskPosition: "center",
        maskRepeat: "no-repeat",
        maskSize: "contain",
        width: 14,
        WebkitMaskImage: `url("${AGENT_LOGOS[icon]}")`,
        WebkitMaskPosition: "center",
        WebkitMaskRepeat: "no-repeat",
        WebkitMaskSize: "contain",
      }}
    />
  );
}