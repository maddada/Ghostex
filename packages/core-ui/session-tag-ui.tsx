import {
  IconAlertTriangle,
  IconArrowDown,
  IconBarrierBlock,
  IconBug,
  IconCheckbox,
  IconCircleCheck,
  IconMicroscope,
  IconPalette,
  IconPlayerPause,
  IconPlayerPlay,
  IconPuzzle,
  IconStar,
  IconTag,
  IconTagOff,
  IconTestPipe,
  type TablerIcon,
} from '@tabler/icons-react';
import { useMemo } from 'react';
import {
  findCustomSessionTag,
  getEffectiveSidebarSessionTag,
  getSidebarSessionTagLabel,
  isCustomSessionTagId,
  SIDEBAR_SESSION_TAG_FILTER_UNTAGGED,
  SIDEBAR_SESSION_TAG_OPTIONS,
  SIDEBAR_SESSION_TAG_SECTIONS,
  type BuiltinSidebarSessionTag,
  type CustomSessionTag,
  type CustomSessionTagCatalogs,
  type SidebarSessionTag,
  type SidebarSessionTagFilter,
} from '../shared/session-tags';
import { isSidebarCommandIcon } from '../shared/sidebar-command-icons';
import { SidebarCommandIconGlyph } from './sidebar-command-icon';
import { useSidebarStore } from './sidebar-store';

const SIDEBAR_SESSION_TAG_ICONS = {
  blocked: IconBarrierBlock,
  bug: IconBug,
  design: IconPalette,
  done: IconCircleCheck,
  favorite: IconStar,
  feature: IconPuzzle,
  'high-priority': IconAlertTriangle,
  'in-progress': IconPlayerPlay,
  'low-priority': IconArrowDown,
  'on-hold': IconPlayerPause,
  research: IconMicroscope,
  testing: IconTestPipe,
  todo: IconCheckbox,
} satisfies Record<BuiltinSidebarSessionTag, TablerIcon>;

export {
  SIDEBAR_SESSION_TAG_FILTER_UNTAGGED,
  SIDEBAR_SESSION_TAG_OPTIONS,
  SIDEBAR_SESSION_TAG_SECTIONS,
  getSidebarSessionTagLabel,
};
export type { SidebarSessionTag, SidebarSessionTagFilter };

export function getSessionTagIcon(tag: SidebarSessionTag): TablerIcon {
  return isCustomSessionTagId(tag) ? IconTag : SIDEBAR_SESSION_TAG_ICONS[tag];
}

/**
 * CDXC:Sessions 2026-09-11 WHY:
 * Custom tags resolve against every catalog the sidebar store holds (the local daemon's plus each remote daemon's), so a remote session's tag renders without the caller knowing which machine owns it. Ids are random tokens, so cross-catalog lookup cannot collide in practice.
 */
export function useSessionTagCatalogs(): CustomSessionTagCatalogs {
  const local = useSidebarStore((state) => state.customSessionTags);
  const remote = useSidebarStore((state) => state.remoteCustomSessionTagsByMachineId);
  return useMemo(() => [local, ...Object.values(remote)], [local, remote]);
}

export function useCustomSessionTag(tag: string | undefined): CustomSessionTag | undefined {
  return useSidebarStore((state) =>
    isCustomSessionTagId(tag)
      ? findCustomSessionTag(tag, [state.customSessionTags, ...Object.values(state.remoteCustomSessionTagsByMachineId)])
      : undefined
  );
}

/** Non-hook variant for helpers that run outside render (tooltips, search keywords). */
export function getSessionTagCatalogs(): CustomSessionTagCatalogs {
  return selectSessionTagCatalogs(useSidebarStore.getState());
}

function selectSessionTagCatalogs(state: ReturnType<typeof useSidebarStore.getState>): CustomSessionTagCatalogs {
  return [state.customSessionTags, ...Object.values(state.remoteCustomSessionTagsByMachineId)];
}

/** Draws a custom tag's own icon in its own color. Unknown icon ids fall back to the generic tag glyph. */
export function CustomSessionTagGlyph({
  className,
  color,
  icon,
  size = 14,
  stroke = 1.8,
  tagId,
}: {
  className?: string;
  color?: string;
  icon?: string;
  size?: number;
  stroke?: number;
  tagId: string;
}) {
  if (icon && isSidebarCommandIcon(icon)) {
    return (
      <SidebarCommandIconGlyph
        className={className}
        color={color}
        data-session-tag={tagId}
        icon={icon}
        size={size}
        stroke={stroke}
      />
    );
  }
  return (
    <IconTag
      aria-hidden='true'
      className={className}
      color={color}
      data-session-tag={tagId}
      fill='none'
      size={size}
      stroke={stroke}
    />
  );
}

export function getEffectiveSessionTag(input: {
  isFavorite?: boolean;
  sessionTag?: SidebarSessionTag;
}): SidebarSessionTag | undefined {
  return getEffectiveSidebarSessionTag(input);
}

export function SessionTagIcon({
  className,
  fillFavorite = false,
  size = 14,
  stroke = 1.8,
  tag,
}: {
  className?: string;
  fillFavorite?: boolean;
  size?: number;
  stroke?: number;
  tag: SidebarSessionTagFilter;
}) {
  const customTag = useCustomSessionTag(tag);
  if (isCustomSessionTagId(tag)) {
    return (
      <CustomSessionTagGlyph
        className={className}
        color={customTag?.color}
        icon={customTag?.icon}
        size={size}
        stroke={stroke}
        tagId={tag}
      />
    );
  }
  if (tag === SIDEBAR_SESSION_TAG_FILTER_UNTAGGED) {
    return (
      <IconTagOff
        aria-hidden='true'
        className={className}
        data-session-tag={tag}
        fill='none'
        size={size}
        stroke={stroke}
      />
    );
  }
  const Icon = getSessionTagIcon(tag);
  return (
    <Icon
      aria-hidden='true'
      className={className}
      data-session-tag={tag}
      fill={fillFavorite && tag === 'favorite' ? 'currentColor' : 'none'}
      size={size}
      stroke={stroke}
    />
  );
}
