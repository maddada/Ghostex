import { AccountIndicator } from './accounts/indicator';
import { IconCode } from '@tabler/icons-react';
import type { SidebarAgentButton } from '../shared/sidebar-agents';
import { AGENT_LOGOS, getBrandAgentLogoStyle } from './agent-logos';

export function ProjectAgentLauncherIcon({
  agent,
  accountIndicator,
  colorMode = 'monochrome',
}: {
  agent?: SidebarAgentButton;
  colorMode?: 'brand' | 'monochrome';
  accountIndicator?: string;
}) {
  if (!agent) {
    return (
      <IconCode
        aria-hidden='true'
        className='group-agent-launcher-icon group-agent-launcher-tabler-icon'
        size={14}
        stroke={1.9}
      />
    );
  }

  if (agent.icon) {
    /**
     * CDXC:AgentLauncher 2026-05-16-18:21:
     * The sidebar project agent dropdown should show colored provider icons for
     * scanability, while compact split launchers stay monochrome unless the
     * caller opts into brand color.
     */
    const iconStyle =
      colorMode === 'brand' || (accountIndicator !== undefined && (agent.icon === 'claude' || agent.icon === 'codex'))
        ? getBrandAgentLogoStyle(agent.icon)
        : {
            backgroundColor: 'currentColor',
            maskImage: `url("${AGENT_LOGOS[agent.icon]}")`,
            WebkitMaskImage: `url("${AGENT_LOGOS[agent.icon]}")`,
          };

    return (
      <span className='gx-account-mark' data-provider={agent.icon}>
        <span
          aria-hidden='true'
          className='group-agent-launcher-icon group-agent-launcher-agent-icon'
          data-agent-icon={agent.icon}
          style={iconStyle}
        />
        <AccountIndicator value={accountIndicator} />
      </span>
    );
  }

  return (
    <IconCode
      aria-hidden='true'
      className='group-agent-launcher-icon group-agent-launcher-tabler-icon'
      size={14}
      stroke={1.9}
    />
  );
}
