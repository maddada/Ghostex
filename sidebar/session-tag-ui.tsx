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
  IconTagOff,
  IconTestPipe,
  type TablerIcon,
} from "@tabler/icons-react";
import {
  getEffectiveSidebarSessionTag,
  getSidebarSessionTagLabel,
  SIDEBAR_SESSION_TAG_FILTER_UNTAGGED,
  SIDEBAR_SESSION_TAG_OPTIONS,
  SIDEBAR_SESSION_TAG_SECTIONS,
  type SidebarSessionTag,
  type SidebarSessionTagFilter,
} from "../shared/session-tags";

const SIDEBAR_SESSION_TAG_ICONS = {
  blocked: IconBarrierBlock,
  bug: IconBug,
  design: IconPalette,
  done: IconCircleCheck,
  favorite: IconStar,
  feature: IconPuzzle,
  "high-priority": IconAlertTriangle,
  "in-progress": IconPlayerPlay,
  "low-priority": IconArrowDown,
  "on-hold": IconPlayerPause,
  research: IconMicroscope,
  testing: IconTestPipe,
  todo: IconCheckbox,
} satisfies Record<SidebarSessionTag, TablerIcon>;

export {
  SIDEBAR_SESSION_TAG_FILTER_UNTAGGED,
  SIDEBAR_SESSION_TAG_OPTIONS,
  SIDEBAR_SESSION_TAG_SECTIONS,
  getSidebarSessionTagLabel,
};
export type { SidebarSessionTag, SidebarSessionTagFilter };

export function getSessionTagIcon(tag: SidebarSessionTag): TablerIcon {
  return SIDEBAR_SESSION_TAG_ICONS[tag];
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
  if (tag === SIDEBAR_SESSION_TAG_FILTER_UNTAGGED) {
    return (
      <IconTagOff
        aria-hidden="true"
        className={className}
        data-session-tag={tag}
        fill="none"
        size={size}
        stroke={stroke}
      />
    );
  }
  const Icon = getSessionTagIcon(tag);
  return (
    <Icon
      aria-hidden="true"
      className={className}
      data-session-tag={tag}
      fill={fillFavorite && tag === "favorite" ? "currentColor" : "none"}
      size={size}
      stroke={stroke}
    />
  );
}
