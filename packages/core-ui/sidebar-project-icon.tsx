import { IconFolder, IconFolderOpen, IconGitBranch } from '@tabler/icons-react';
import {
  normalizeDiscoveredProjectIconDataUrl,
  resolveWorkspaceProjectIconDataUrl,
  type WorkspaceProjectIcon,
} from '../shared/workspace-project-appearance';
import { AppTooltip } from './app-tooltip';
import { SidebarCommandIconGlyph } from './sidebar-command-icon';

export type SidebarProjectIconProps = {
  discoveredIconDataUrl?: string;
  fallback?: 'folder' | 'folder-open' | 'worktree';
  icon?: WorkspaceProjectIcon;
  iconDataUrl?: string;
  title: string;
  tooltipDelay?: number;
};

export function SidebarProjectIcon({
  discoveredIconDataUrl,
  fallback = 'folder',
  icon,
  iconDataUrl,
  title,
  tooltipDelay,
}: SidebarProjectIconProps) {
  const imageDataUrl = resolveWorkspaceProjectIconDataUrl({ icon, iconDataUrl });
  if (imageDataUrl) {
    return (
      <AppTooltip content={title} delay={tooltipDelay}>
        <img alt='' aria-hidden='true' className='sidebar-project-icon' data-icon-variant='image' src={imageDataUrl} />
      </AppTooltip>
    );
  }

  const discovered = normalizeDiscoveredProjectIconDataUrl(discoveredIconDataUrl);
  if (discovered) {
    return (
      <AppTooltip content={title} delay={tooltipDelay}>
        <img
          alt=''
          aria-hidden='true'
          className='sidebar-project-icon'
          data-icon-variant='discovered'
          src={discovered}
        />
      </AppTooltip>
    );
  }

  if (icon?.kind === 'tabler') {
    return (
      <AppTooltip content={title} delay={tooltipDelay}>
        <span aria-hidden='true' className='sidebar-project-icon' data-icon-variant='tabler'>
          <SidebarCommandIconGlyph color={icon.color} icon={icon.icon} size={16} stroke={1.8} />
        </span>
      </AppTooltip>
    );
  }

  const FallbackIcon =
    fallback === 'worktree' ? IconGitBranch : fallback === 'folder-open' ? IconFolderOpen : IconFolder;
  return (
    <FallbackIcon
      aria-hidden='true'
      className='sidebar-project-icon'
      data-fallback-kind={fallback}
      data-icon-variant='glyph'
      size={16}
      stroke={1.8}
    />
  );
}
