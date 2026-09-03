// The agent glyph beside a submenu row that names an agent configuration (the
// "Switch Account" rows in the chat composer's dots menu and the terminal
// surface's bottom bar). Brand-coloured, like the draft agent switcher's rows,
// so two Claude accounts read as Claude at a glance; an icon the sidebar does
// not know renders the generic launcher glyph instead of nothing.

import { getDefaultSidebarAgentByIcon, isSidebarAgentIcon } from '../../shared/sidebar-agents';
import { ProjectAgentLauncherIcon } from '../project-agent-launcher-icon';

export function SessionChatHostActionAgentIcon({ icon }: { icon: string | undefined }) {
  const agent = getDefaultSidebarAgentByIcon(isSidebarAgentIcon(icon) ? icon : undefined);
  return (
    <span className='ghostex-chat-host-action-agent-icon inline-flex size-4 shrink-0 items-center justify-center [&_svg]:size-4'>
      <ProjectAgentLauncherIcon agent={agent ? { ...agent, isDefault: true } : undefined} colorMode='brand' />
    </span>
  );
}
