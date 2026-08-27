import { IconMessageCircle } from '@tabler/icons-react';
import { type ComponentProps } from 'react';
import { cn } from '@/packages/components/utils';
import { resolveSessionChatTranscriptAgent } from '../shared/session-chat';
import type { SidebarAgentButton } from '../shared/sidebar-agents';
import { AppTooltip } from './app-tooltip';

export function agentSupportsChatView(agent: Pick<SidebarAgentButton, 'agentId' | 'icon'>): boolean {
  return resolveSessionChatTranscriptAgent(agent.agentId, agent.icon) !== null;
}

export function AgentMenuChatIndicator({ agent }: { agent: Pick<SidebarAgentButton, 'agentId' | 'icon'> }) {
  if (!agentSupportsChatView(agent)) {
    return null;
  }

  return (
    <AppTooltip content='Supports chat' side='left'>
      <span aria-label='Supports chat' className='group-agent-menu-chat-support' role='img'>
        <IconMessageCircle aria-hidden='true' size={14} stroke={1.8} />
      </span>
    </AppTooltip>
  );
}

/**
 * The same "this agent can run in Chat View" badge as the agent launcher menus,
 * for surfaces that lay their own rows out (Settings > Agents) instead of the
 * menu chrome that `AgentMenuChatIndicator` is glued into. Terminal-only agents
 * render nothing: absence is the negative state, so lists stay quiet.
 */
export function AgentChatViewSupportBadge({
  agent,
  className,
  side = 'top',
}: {
  agent: Pick<SidebarAgentButton, 'agentId' | 'icon'>;
  className?: string;
  side?: ComponentProps<typeof AppTooltip>['side'];
}) {
  if (!agentSupportsChatView(agent)) {
    return null;
  }

  return (
    <AppTooltip content='Supports Chat View' side={side}>
      <span
        aria-label='Supports Chat View'
        className={cn('inline-flex shrink-0 items-center justify-center text-muted-foreground/70', className)}
        role='img'
      >
        <IconMessageCircle aria-hidden='true' size={14} stroke={1.8} />
      </span>
    </AppTooltip>
  );
}
