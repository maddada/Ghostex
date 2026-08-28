import { IconCodeDots } from '@tabler/icons-react';
import type { SidebarAgentIcon } from '../shared/sidebar-agents';
import { getBrandAgentLogoStyle } from './agent-logos';

export function AgentTypeSelectOption({ icon, name }: { icon: SidebarAgentIcon | 'custom'; name: string }) {
  return (
    <span className='flex min-w-0 items-center gap-2'>
      {icon === 'custom' ? (
        <IconCodeDots aria-hidden='true' className='size-4 shrink-0' />
      ) : (
        <span
          aria-hidden='true'
          className='configure-agents-list-agent-icon shrink-0'
          style={getBrandAgentLogoStyle(icon)}
        />
      )}
      <span className='truncate'>{name}</span>
    </span>
  );
}
