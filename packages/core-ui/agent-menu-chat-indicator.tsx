import { IconMessageCircle } from "@tabler/icons-react";
import { resolveSessionChatTranscriptAgent } from "../shared/session-chat";
import type { SidebarAgentButton } from "../shared/sidebar-agents";
import { AppTooltip } from "./app-tooltip";

export function AgentMenuChatIndicator({
  agent,
}: {
  agent: Pick<SidebarAgentButton, "agentId" | "icon">;
}) {
  if (resolveSessionChatTranscriptAgent(agent.agentId, agent.icon) === null) {
    return null;
  }

  return (
    <AppTooltip content="Supports chat" side="left">
      <span
        aria-label="Supports chat"
        className="group-agent-menu-chat-support"
        role="img"
      >
        <IconMessageCircle aria-hidden="true" size={14} stroke={1.8} />
      </span>
    </AppTooltip>
  );
}
